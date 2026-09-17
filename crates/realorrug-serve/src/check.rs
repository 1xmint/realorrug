// SPDX-License-Identifier: Apache-2.0
//! `GET /v1/check/{address}` — the public checker page's route.
//!
//! Design 0023 owns the product decisions this implements; the doc comments
//! here record the choices 0023 §8 left open, not restated arguments.
//!
//! # What this is not
//!
//! Not `public.rs`. That file's own doc comment says its handlers "read a
//! published file and never the store" — this handler's cache-miss path does
//! neither of those things: it makes a live, budget-metered RPC read and
//! writes the answer to disk itself. Kept in its own module for that reason.
//!
//! # No model call
//!
//! The verdict is [`realorrug_roast::verdict::Verdict::from`] over the same
//! [`realorrug_roast::sheet::FactSheet`] `realorrug-roast::roast` builds,
//! stopped before the voice pass. `FactSheet::build` already drops every
//! [`realorrug_roast::sheet::About::Price`] fact before anything downstream
//! can state it (AGENTS.md §3 rule 5) — this route inherits that guarantee
//! rather than re-implementing it, and a test below pins it.
//!
//! # The cache, and the TTL stand-in
//!
//! Design 0021 (per-fact shelf life) is not yet on `main` as of this route's
//! writing. Per design 0023 §3, this stands in with a single fixed TTL,
//! [`CACHE_TTL_SECS`], until 0021 lands and this can ask "is every fact in
//! this still inside its shelf life" instead.
//!
//! # Singleflight
//!
//! One `tokio::sync::Mutex` per `(chain, address)` key, held for the whole
//! cold read. A second request for the same cold key blocks on the same
//! mutex and, once it can proceed, finds the first request's answer already
//! on disk — so it never issues a second RPC read (design 0023 §3).
//!
//! # Budgets, both deny-by-default (AGENTS.md §3 rule 7)
//!
//! - **No RPC endpoint configured for the resolved chain** answers `budget`
//!   for every cold request on that chain — design 0023 §4's own text: "a box
//!   with no RPC credential set answers every cold request with the
//!   budget-exhausted message". Robinhood Chain reuses
//!   `REALORRUG_ROBINHOOD_RPC` (falling back to `RADAR_ROBINHOOD_RPC`), the
//!   same variable `realorrug-analyst`'s daemon and `realorrug roast
//!   --robinhood-rpc` already read. Solana keeps `realorrug_onchain::RpcClient`'s
//!   own public-endpoint default (`RpcClient::from_vars`), so a Solana cold
//!   read needs no extra configuration to be included here — design 0023 §2's
//!   "if Solana is equally cheap to wire through the same code, include it".
//! - **The daily cold-read count**, [`REALORRUG_CHECK_DAILY_BUDGET`](Paths)
//!   (falling back to `RADAR_CHECK_DAILY_BUDGET`), unset
//!   or zero refuses every cold read (rule 7 — no budget refuses spending).
//!   Reserved before the read, released back on a failed read so a network
//!   error does not permanently shrink the day's budget for nothing learned.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use realorrug_onchain::{RpcClient, dispatch};
use realorrug_roast::verdict::{Level, Verdict};
use realorrug_roast::{BaseRates, FactSheet};
use realorrug_types::ChainAddress;
use realorrug_types::env::env_or_legacy;
use serde_json::{Value, json};
use tokio::sync::Mutex as AsyncMutex;

/// A cached or fresh verdict is good for this long before a request for it
/// counts as cold again.
///
/// **Stands in for design 0021's per-fact shelf life**, which is not yet
/// merged (design 0023 §3's own dependency note). Ten minutes: long enough
/// that a viral link does not each re-spend a cold read, short enough that a
/// liquidity pull a visitor is actively watching for shows up inside one
/// coffee break.
pub const CACHE_TTL_SECS: u64 = 600;

/// How many per-IP checker requests are allowed inside a rolling minute.
///
/// Design 0023 §4's own number: "generous enough for a person pasting a few
/// addresses in a sitting... tight enough that one IP cannot drive a
/// meaningful fraction of a cold-read budget alone".
pub const PER_IP_LIMIT: usize = 10;

/// The window [`PER_IP_LIMIT`] is counted over.
const PER_IP_WINDOW: Duration = Duration::from_mins(1);

/// Everything the route needs, built once from the environment and shared
/// across requests behind an [`Arc`].
pub struct CheckState {
    /// Where cached verdict documents live, one file per `(chain, address)`.
    pub cache_dir: std::path::PathBuf,
    /// Solana's endpoint. Always set (`RpcClient::from_vars`'s own
    /// public-endpoint default), so a Solana cold read is never refused for
    /// lack of an endpoint the way a Robinhood one can be. A fresh
    /// [`RpcClient`] is built from this string per request — `RpcClient`
    /// holds a `Box<dyn Transport>` and is not `Clone`, and constructing one
    /// is cheap (it wraps an HTTP agent, not a connection).
    pub solana_endpoint: String,
    /// Robinhood Chain's endpoint, or `None` when `REALORRUG_ROBINHOOD_RPC`
    /// (or the legacy `RADAR_ROBINHOOD_RPC`) is unset — every Robinhood cold
    /// request then answers `budget`.
    pub robinhood_endpoint: Option<String>,
    /// The base rates a fact sheet is built against, or `None` when the
    /// snapshot could not be loaded — the sheet then simply carries no
    /// population context (`FactSheet::build`'s own rule).
    pub rates: Option<BaseRates>,
    /// How many cold reads remain today, and which UTC day that count is
    /// for. `None` means the budget was never configured — rule 7, refuse.
    daily: Mutex<Option<DailyBudget>>,
    /// The daily ceiling itself, so [`DailyBudget`] can be rebuilt at UTC
    /// midnight without re-reading the environment.
    daily_max: i64,
    /// One rolling window of request instants per IP.
    ip_limits: Mutex<HashMap<String, VecDeque<Instant>>>,
    /// One lock per in-flight or recently-flighted `(chain, address)` key.
    inflight: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    /// Whether `CF-Connecting-IP` may be trusted for the per-IP limit.
    /// `false` unless `REALORRUG_TRUST_CLOUDFLARE` (or the legacy
    /// `RADAR_TRUST_CLOUDFLARE`) is set — AGENTS.md §3 rule 7:
    /// a header is never trusted by default, only when an operator has said
    /// the box sits behind the proxy that sets it honestly.
    trust_cloudflare: bool,
}

/// Today's remaining cold-read count.
struct DailyBudget {
    /// The UTC day this count is for, days since the epoch.
    day: u64,
    /// How many cold reads are left today.
    remaining: i64,
}

impl CheckState {
    /// Builds the state from the environment, with the repository's default
    /// cache location.
    ///
    /// Takes a getter, the same shape `public::Paths::from_vars` uses, so the
    /// rule is testable without setting process-wide variables.
    #[must_use]
    pub fn from_vars(get: &impl Fn(&str) -> Option<String>) -> Self {
        let cache_dir = env_or_legacy("REALORRUG_CHECK_CACHE_DIR", "RADAR_CHECK_CACHE_DIR", get)
            .unwrap_or_else(|| "data/check".to_owned())
            .into();
        let solana_endpoint = RpcClient::from_vars(get).endpoint().to_owned();
        let robinhood_endpoint =
            env_or_legacy("REALORRUG_ROBINHOOD_RPC", "RADAR_ROBINHOOD_RPC", get);
        let rates = BaseRates::load(
            &env_or_legacy("REALORRUG_BASE_RATES", "RADAR_BASE_RATES", get)
                .unwrap_or_else(|| realorrug_roast::baserates::DEFAULT_PATH.to_owned()),
        )
        .ok();
        // Deny by default: an unset or unparseable budget is zero, not
        // "unlimited" — rule 7, a missing config never fails open.
        let daily_max = env_or_legacy(
            "REALORRUG_CHECK_DAILY_BUDGET",
            "RADAR_CHECK_DAILY_BUDGET",
            get,
        )
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
        let trust_cloudflare =
            env_or_legacy("REALORRUG_TRUST_CLOUDFLARE", "RADAR_TRUST_CLOUDFLARE", get)
                .is_some_and(|v| v == "1");
        Self {
            cache_dir,
            solana_endpoint,
            robinhood_endpoint,
            rates,
            daily: Mutex::new(None),
            daily_max,
            ip_limits: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            trust_cloudflare,
        }
    }

    fn from_env() -> Self {
        Self::from_vars(&|k| std::env::var(k).ok())
    }

    /// The shared, `Arc`-wrapped state both this route and `card.rs`'s
    /// route are built on — one process-wide cache/budget/rate-limiter, not
    /// two independent copies that would each think they own the daily
    /// budget.
    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::from_env())
    }

    /// Reserves one cold read against today's budget, rolling the day over
    /// first. `Err(())` when nothing is left — the caller answers `budget`.
    fn reserve_cold_read(&self, today: u64) -> Result<(), ()> {
        let mut guard = self.daily.lock().expect("daily budget lock");
        let needs_reset = match guard.as_ref() {
            Some(b) => b.day != today,
            None => true,
        };
        if needs_reset {
            *guard = Some(DailyBudget {
                day: today,
                remaining: self.daily_max,
            });
        }
        let budget = guard.as_mut().expect("just set");
        if budget.remaining <= 0 {
            return Err(());
        }
        budget.remaining -= 1;
        Ok(())
    }

    /// Gives one reservation back — a read that was reserved but did not
    /// resolve (an RPC failure) must not permanently shrink the day's count
    /// for nothing learned.
    fn release_cold_read(&self, today: u64) {
        let mut guard = self.daily.lock().expect("daily budget lock");
        if let Some(budget) = guard.as_mut()
            && budget.day == today
        {
            budget.remaining += 1;
        }
    }

    /// Whether `ip` may make another checker request right now, recording
    /// this one if so.
    fn allow(&self, ip: &str, now: Instant) -> bool {
        let mut guard = self.ip_limits.lock().expect("ip limit lock");
        let window = guard.entry(ip.to_owned()).or_default();
        while let Some(&oldest) = window.front() {
            if now.duration_since(oldest) > PER_IP_WINDOW {
                window.pop_front();
            } else {
                break;
            }
        }
        if window.len() >= PER_IP_LIMIT {
            return false;
        }
        window.push_back(now);
        true
    }

    /// The lock for one `(chain, address)` key, created if this is the first
    /// request to ever ask for it.
    fn lock_for(&self, key: &str) -> Arc<AsyncMutex<()>> {
        let mut guard = self.inflight.lock().expect("inflight lock");
        guard
            .entry(key.to_owned())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }
}

/// The router for this route alone, merged into [`crate::app`].
///
/// A separate function, the same shape `public.rs`'s handlers use for their
/// own paths, so the state this route needs is built once and never leaks
/// into the five read-only handlers beside it.
pub fn router(state: Arc<CheckState>) -> Router {
    Router::new()
        .route("/v1/check/{address}", get(handle))
        .with_state(state)
}

/// The route, wired to a real [`CheckState`] built from the environment.
///
/// `req` is the extractor of last resort, not the whole `Request` because a
/// body is wanted, but because `Option<ConnectInfo<SocketAddr>>` does not
/// implement `FromRequestParts` on its own (axum only special-cases that for
/// types that opt in via `OptionalFromRequestParts`, and `ConnectInfo` does
/// not) — reading the extension straight off `req` is the direct way to get
/// "the peer address, or none when the router was never given one" without
/// a request that lacks it rejecting the whole handler with a 500.
async fn handle(
    State(state): State<Arc<CheckState>>,
    Path(address): Path<String>,
    req: Request,
) -> Response {
    let headers = req.headers().clone();
    let connect_info = req
        .extensions()
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .copied();
    let ip = client_ip(&state, &headers, connect_info);
    let (status, doc) = check(&state, &address, &ip).await;
    (status, Json(doc)).into_response()
}

/// The IP a request is billed against for the per-minute limit.
///
/// `CF-Connecting-IP` is read only when `REALORRUG_TRUST_CLOUDFLARE` says the box
/// sits behind Cloudflare — otherwise the socket peer address, and never the
/// header, because an untrusted header is exactly how a visitor would spoof
/// past their own limit (AGENTS.md §3 rule 7).
pub(crate) fn client_ip(
    state: &CheckState,
    headers: &HeaderMap,
    connect_info: Option<ConnectInfo<std::net::SocketAddr>>,
) -> String {
    if state.trust_cloudflare
        && let Some(v) = headers.get("CF-Connecting-IP")
        && let Ok(v) = v.to_str()
        && !v.trim().is_empty()
    {
        return v.trim().to_owned();
    }
    connect_info.map_or_else(
        || "unknown".to_owned(),
        |ConnectInfo(addr)| addr.ip().to_string(),
    )
}

/// The full route logic, taking the pieces `handle` extracts so it is
/// testable without an HTTP request.
pub(crate) async fn check(
    state: &Arc<CheckState>,
    raw_address: &str,
    ip: &str,
) -> (StatusCode, Value) {
    if !state.allow(ip, Instant::now()) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            json!({
                "state": "busy",
                "chain": Value::Null,
                "address": raw_address,
                "level": Value::Null,
                "reasons": Vec::<String>::new(),
                "twins": Vec::<String>::new(),
                "measured_at": Value::Null,
                "message": "You're checking addresses faster than we can read them. \
                             Wait a minute and try again.",
            }),
        );
    }

    let parsed: Result<ChainAddress, _> = raw_address.parse();
    let Ok(address) = parsed else {
        return (
            StatusCode::BAD_REQUEST,
            json!({
                "state": "bad_address",
                "chain": Value::Null,
                "address": raw_address,
                "level": Value::Null,
                "reasons": Vec::<String>::new(),
                "twins": Vec::<String>::new(),
                "measured_at": Value::Null,
                "message": "That's not shaped like a Robinhood Chain or a Solana \
                             address. Robinhood Chain: 0x + 40 hex characters. \
                             Solana: a base58 string, 32-44 characters, no 0, O, I \
                             or l. Paste the address exactly as the chain gave it.",
            }),
        );
    };

    let chain = chain_name(&address);
    let key = format!("{chain}:{address}");
    let cache_path = state.cache_dir.join(format!("{}.json", cache_key(&key)));

    if let Some(doc) = fresh_cached(&cache_path, now_secs()) {
        return (StatusCode::OK, doc);
    }

    // Singleflight: only one cold read per key runs at a time. A second
    // request for the same key waits here, then re-checks the cache below —
    // the first request's answer is on disk by the time this one can
    // proceed, so it never issues a second RPC read.
    let lock = state.lock_for(&key);
    let _guard = lock.lock().await;

    if let Some(doc) = fresh_cached(&cache_path, now_secs()) {
        return (StatusCode::OK, doc);
    }

    let today = day_of(now_secs());
    if state.reserve_cold_read(today).is_err() {
        return (StatusCode::SERVICE_UNAVAILABLE, budget_doc(raw_address));
    }

    let solana_endpoint = state.solana_endpoint.clone();
    let robinhood_endpoint = state.robinhood_endpoint.clone();
    let mint_text = address.to_string();
    let outcome = tokio::task::spawn_blocking(move || {
        let solana = RpcClient::new(solana_endpoint);
        let robinhood = robinhood_endpoint
            .as_deref()
            .map(realorrug_robinhood::Rpc::new);
        let clients = dispatch::Clients {
            solana: &solana,
            robinhood: robinhood.as_ref(),
        };
        dispatch::read(&mint_text, &clients)
    })
    .await;

    let dossier = match outcome {
        Ok(Ok(dossier)) => dossier,
        Ok(Err(dispatch::Error::NotAnAddress)) => {
            // Already validated above; this arm exists only because
            // `dispatch::read` reparses the text itself.
            state.release_cold_read(today);
            return (StatusCode::BAD_REQUEST, bad_address_doc(raw_address));
        }
        Ok(Err(dispatch::Error::Unreadable(why))) => {
            state.release_cold_read(today);
            return unreadable_doc(raw_address, &chain, &why);
        }
        Err(_join_error) => {
            state.release_cold_read(today);
            return (
                StatusCode::OK,
                cant_read_doc(raw_address, &chain, "the read did not finish"),
            );
        }
    };

    let sheet = FactSheet::build(&dossier, state.rates.as_ref(), None, None, None);
    let verdict = Verdict::from(&sheet);
    let measured_at = realorrug_types::civil::timestamp_from_seconds(now_secs());
    let doc = verdict_doc(raw_address, &chain, &verdict, &measured_at);

    // The card route (§7's requirement) needs the token name/symbol to draw
    // on the image, but the JSON contract below (`assert_contract`) never
    // carries them. Stash them in the cache file only, under `_`-prefixed
    // keys `fresh_cached` already knows to strip before a client sees them —
    // the same trick `_written_at` uses. `sheet.untrusted` is
    // `FactSheet::build`'s own labeled pair list; "token name"/"token
    // symbol" are only pushed there when a launch block was read (`sheet.rs`),
    // so a partial dossier simply leaves these absent, never guessed.
    let mut stored = doc.clone();
    if let Some(name) = untrusted_field(&sheet.untrusted, "token name") {
        stored["_name"] = json!(name);
    }
    if let Some(symbol) = untrusted_field(&sheet.untrusted, "token symbol") {
        stored["_symbol"] = json!(symbol);
    }
    let _ = write_cache(&cache_path, &stored, now_secs());
    (StatusCode::OK, doc)
}

/// Looks up one label in a `FactSheet::untrusted` pair list.
fn untrusted_field<'a>(untrusted: &'a [(String, String)], label: &str) -> Option<&'a str> {
    untrusted
        .iter()
        .find(|(l, _)| l == label)
        .map(|(_, v)| v.as_str())
}

/// Whether `dispatch::Error::Unreadable`'s message names the one case design
/// 0023 §1 calls "wrong chain" — the address is real but the factory (or,
/// eventually, Solana's own launch record) has no record of it — versus every
/// other unreadable cause, which is treated as `cant_read` (a fact the ladder
/// needed could not be read).
///
/// Matched on the message text rather than a typed variant because
/// `dispatch::Error::Unreadable` is a `String` today (`realorrug-onchain`'s
/// own boundary, not this route's to widen) — this is a known fragility,
/// named rather than hidden; see the crate's `NOT VERIFIED` note.
fn unreadable_doc(raw_address: &str, chain: &str, why: &str) -> (StatusCode, Value) {
    if why.contains("no Robinhood endpoint is configured") {
        return (StatusCode::SERVICE_UNAVAILABLE, budget_doc(raw_address));
    }
    if why.contains("no record of this token") {
        return (
            StatusCode::OK,
            json!({
                "state": "not_a_token",
                "chain": chain,
                "address": raw_address,
                "level": Value::Null,
                "reasons": Vec::<String>::new(),
                "twins": Vec::<String>::new(),
                "measured_at": Value::Null,
                "message": "We looked for this address on Robinhood Chain and found \
                             nothing there. If this is a token on a chain we don't \
                             read yet, we can't tell you anything about it -- not \
                             \"clean,\" not \"sketchy.\" We just haven't looked at the \
                             right place.",
            }),
        );
    }
    (StatusCode::OK, cant_read_doc(raw_address, chain, why))
}

fn cant_read_doc(raw_address: &str, chain: &str, why: &str) -> Value {
    json!({
        "state": "cant_read",
        "chain": chain,
        "address": raw_address,
        "level": "CantTell",
        "reasons": vec![format!("not known -- {why}")],
        "twins": Vec::<String>::new(),
        "measured_at": Value::Null,
        "message": "We couldn't read something we needed to give this a real \
                     verdict. That is not the same as clean -- it means we don't \
                     know, and a token we don't know about is not a token we're \
                     calling safe.",
    })
}

fn bad_address_doc(raw_address: &str) -> Value {
    json!({
        "state": "bad_address",
        "chain": Value::Null,
        "address": raw_address,
        "level": Value::Null,
        "reasons": Vec::<String>::new(),
        "twins": Vec::<String>::new(),
        "measured_at": Value::Null,
        "message": "That's not shaped like a Robinhood Chain or a Solana address. \
                     Robinhood Chain: 0x + 40 hex characters. Solana: a base58 \
                     string, 32-44 characters, no 0, O, I or l. Paste the address \
                     exactly as the chain gave it.",
    })
}

fn budget_doc(raw_address: &str) -> Value {
    json!({
        "state": "budget",
        "chain": Value::Null,
        "address": raw_address,
        "level": Value::Null,
        "reasons": Vec::<String>::new(),
        "twins": Vec::<String>::new(),
        "measured_at": Value::Null,
        "message": "New checks are switched off right now. Cached verdicts still \
                     work.",
    })
}

fn verdict_doc(raw_address: &str, chain: &str, verdict: &Verdict, measured_at: &str) -> Value {
    let level = level_name(verdict.level);
    let state = if matches!(verdict.level, Level::CantTell) {
        "cant_read"
    } else {
        "verdict"
    };
    json!({
        "state": state,
        "chain": chain,
        "address": raw_address,
        "level": level,
        "reasons": verdict.reasons,
        "twins": verdict.twins,
        "measured_at": measured_at,
        "message": Value::Null,
    })
}

fn level_name(level: Level) -> &'static str {
    match level {
        Level::Rugged => "Rugged",
        Level::RugMechanicsLive => "RugMechanicsLive",
        Level::Sketchy => "Sketchy",
        Level::NothingUglyYet => "NothingUglyYet",
        Level::CantTell => "CantTell",
    }
}

pub(crate) fn chain_name(address: &ChainAddress) -> String {
    match address {
        ChainAddress::Solana(_) => "solana".to_owned(),
        ChainAddress::Robinhood(_) => "robinhood".to_owned(),
    }
}

/// A stable, filesystem-safe name for a cache file — the key itself may
/// contain characters a path segment should not carry verbatim.
pub(crate) fn cache_key(key: &str) -> String {
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The UTC day number a Unix time falls in: the key the daily cold-read budget
/// resets on.
fn day_of(secs: u64) -> u64 {
    secs / 86_400
}

/// Reads the cached document at `path`, if one exists and every fact in it is
/// still inside [`CACHE_TTL_SECS`] — a stale file is a cache miss, not a
/// silent reuse of a number that may no longer be true (design 0023 §3).
fn fresh_cached(path: &std::path::Path, now: u64) -> Option<Value> {
    let mut doc = fresh_cached_raw(path, now)?;
    // Strip every internal `_`-prefixed field (`_written_at`, and the card
    // route's `_name`/`_symbol` stash) — the client-facing contract is
    // exactly the eight fields `assert_contract` pins, never these.
    doc.as_object_mut()?.retain(|k, _| !k.starts_with('_'));
    Some(doc)
}

/// Like [`fresh_cached`], but keeps every internal `_`-prefixed field —
/// the card route (`card.rs`) reads `_name`/`_symbol` off this, never off
/// the client-facing, stripped document.
pub(crate) fn fresh_cached_raw(path: &std::path::Path, now: u64) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    let stored: Value = serde_json::from_str(&text).ok()?;
    let written_at = stored.get("_written_at")?.as_u64()?;
    if now.saturating_sub(written_at) > CACHE_TTL_SECS {
        return None;
    }
    Some(stored)
}

/// Writes `doc` to the cache with the moment it was written, so the next
/// reader can judge its own freshness.
fn write_cache(path: &std::path::Path, doc: &Value, now: u64) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut stored = doc.clone();
    stored["_written_at"] = json!(now);
    std::fs::write(path, stored.to_string())
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    /// The per-IP limit is [`PER_IP_LIMIT`] requests inside [`PER_IP_WINDOW`].
    /// Re-applying the bug (dropping the `>= PER_IP_LIMIT` check, or growing
    /// the constant) would let this pass an eleventh request in the same
    /// window -- that is exactly what this test would then fail to catch, so
    /// it pins the boundary rather than just "some requests are allowed".
    #[test]
    fn the_eleventh_request_in_a_minute_is_refused() {
        let state = CheckState::from_vars(&no_env);
        let now = Instant::now();
        for n in 0..PER_IP_LIMIT {
            assert!(state.allow("1.2.3.4", now), "request {n} should be allowed");
        }
        assert!(
            !state.allow("1.2.3.4", now),
            "the {}th request in the window must be refused",
            PER_IP_LIMIT + 1
        );
        // A different IP has its own window, unaffected by the first's limit.
        assert!(state.allow("5.6.7.8", now), "a fresh IP has its own budget");
    }

    /// A request exactly one window old still counts: it leaves only once it
    /// is *older* than the window. `>=` here would free a slot a moment early.
    #[test]
    fn a_request_exactly_one_window_old_still_counts() {
        let state = CheckState::from_vars(&no_env);
        let start = Instant::now();
        for _ in 0..PER_IP_LIMIT {
            assert!(state.allow("7.7.7.7", start));
        }
        assert!(!state.allow("7.7.7.7", start + PER_IP_WINDOW));
    }

    /// The budget resets at UTC midnight, not at some multiple of a day.
    #[test]
    fn the_budget_day_is_the_utc_day() {
        assert_eq!(day_of(0), 0);
        assert_eq!(day_of(86_399), 0);
        assert_eq!(day_of(86_400), 1);
        assert_eq!(day_of(2 * 86_400 + 5), 2);
    }

    /// The window is rolling: once the oldest request falls outside
    /// [`PER_IP_WINDOW`], it stops counting against the limit. Without this,
    /// an IP that once hit the limit would be locked out forever.
    #[test]
    fn the_window_rolls_forward() {
        let state = CheckState::from_vars(&no_env);
        let start = Instant::now();
        for _ in 0..PER_IP_LIMIT {
            assert!(state.allow("9.9.9.9", start));
        }
        assert!(!state.allow("9.9.9.9", start));
        let later = start + PER_IP_WINDOW + Duration::from_secs(1);
        assert!(
            state.allow("9.9.9.9", later),
            "a request after the window has rolled forward must be allowed"
        );
    }

    /// An unset `REALORRUG_CHECK_DAILY_BUDGET` (rule 7: no budget refuses
    /// spending) must refuse every cold read, not silently allow one. If the
    /// `.max(0)` or the `unwrap_or(0)` in `from_vars` were removed or
    /// inverted, this is the test that would catch a cold read slipping
    /// through with no configured budget.
    #[test]
    fn an_unconfigured_daily_budget_refuses_every_cold_read() {
        let state = CheckState::from_vars(&no_env);
        assert_eq!(
            state.reserve_cold_read(1),
            Err(()),
            "an unconfigured budget must refuse, not fail open"
        );
    }

    /// A configured budget is spent down and refuses once exhausted; a
    /// released reservation (a failed read) is given back rather than lost.
    #[test]
    fn a_configured_daily_budget_is_spent_and_can_be_released() {
        let state = CheckState::from_vars(&|k| {
            (k == "REALORRUG_CHECK_DAILY_BUDGET").then(|| "2".to_owned())
        });
        assert_eq!(state.reserve_cold_read(1), Ok(()));
        assert_eq!(state.reserve_cold_read(1), Ok(()));
        assert_eq!(
            state.reserve_cold_read(1),
            Err(()),
            "a third read on the same day must be refused once the budget of 2 is spent"
        );
        // A failed read's reservation is released, so it does not permanently
        // shrink the day's count for nothing learned.
        state.release_cold_read(1);
        assert_eq!(
            state.reserve_cold_read(1),
            Ok(()),
            "a released reservation must be spendable again"
        );
    }

    /// A new UTC day resets the count even when the previous day was fully
    /// spent -- the budget is daily, not a one-time allowance.
    #[test]
    fn the_daily_budget_rolls_over_at_a_new_day() {
        let state = CheckState::from_vars(&|k| {
            (k == "REALORRUG_CHECK_DAILY_BUDGET").then(|| "1".to_owned())
        });
        assert_eq!(state.reserve_cold_read(1), Ok(()));
        assert_eq!(state.reserve_cold_read(1), Err(()));
        assert_eq!(
            state.reserve_cold_read(2),
            Ok(()),
            "day 2 must have its own budget, not inherit day 1's exhaustion"
        );
    }

    /// Singleflight: two lookups of the same key return the same lock, so a
    /// second concurrent cold request for that key blocks on the first
    /// rather than racing it into its own RPC read. If `lock_for` ever
    /// started returning a fresh lock per call, this is the test that would
    /// catch it (concurrent callers would each get their own mutex and both
    /// would proceed to a cold read).
    #[test]
    fn the_same_key_shares_one_inflight_lock() {
        let state = CheckState::from_vars(&no_env);
        let a = state.lock_for("solana:abc");
        let b = state.lock_for("solana:abc");
        assert!(
            Arc::ptr_eq(&a, &b),
            "the same key must share one lock, not get a fresh one per call"
        );
        let c = state.lock_for("robinhood:abc");
        assert!(
            !Arc::ptr_eq(&a, &c),
            "a different key must not share the first key's lock"
        );
    }

    /// A second request for a key already being read blocks until the first
    /// finishes holding the lock -- the actual behaviour singleflight is for,
    /// not just "the map returns the same `Arc`".
    #[tokio::test]
    async fn a_second_request_for_the_same_key_waits_for_the_first() {
        let state = CheckState::from_vars(&no_env);
        let lock = state.lock_for("solana:xyz");
        let guard = lock.lock().await;

        let waiter_lock = state.lock_for("solana:xyz");
        let waiter = tokio::spawn(async move {
            let _g = waiter_lock.lock().await;
        });

        // The waiter cannot have finished yet: the first guard is still held.
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!waiter.is_finished(), "the waiter must still be blocked");

        drop(guard);
        waiter.await.expect("the waiter finishes once released");
    }

    /// The full JSON contract this route promises: exactly these eight
    /// fields, on every shape of response, and never a `price` or
    /// `market_cap` key -- AGENTS.md §3 rule 5, and the site depends on the
    /// field names not moving.
    fn assert_contract(doc: &Value) {
        let obj = doc.as_object().expect("a JSON object");
        let expected: std::collections::BTreeSet<&str> = [
            "state",
            "chain",
            "address",
            "level",
            "reasons",
            "twins",
            "measured_at",
            "message",
        ]
        .into_iter()
        .collect();
        let actual: std::collections::BTreeSet<&str> =
            obj.keys().map(std::string::String::as_str).collect();
        assert_eq!(
            actual, expected,
            "the response must carry exactly the contracted fields"
        );
        let text = doc.to_string();
        assert!(
            !text.to_lowercase().contains("price") && !text.to_lowercase().contains("market_cap"),
            "a checker response must never carry price or market cap (AGENTS.md §3 rule 5): {text}"
        );
    }

    #[test]
    fn every_response_shape_matches_the_contract_and_carries_no_price() {
        assert_contract(&bad_address_doc("not-an-address"));
        assert_contract(&budget_doc("0x0000000000000000000000000000000000000000"));
        assert_contract(&cant_read_doc(
            "0x0000000000000000000000000000000000000000",
            "robinhood",
            "the read did not finish",
        ));
        let verdict = Verdict {
            level: Level::NothingUglyYet,
            reasons: vec!["nothing observed yet".to_owned()],
            twins: Vec::new(),
        };
        assert_contract(&verdict_doc(
            "0x0000000000000000000000000000000000000000",
            "robinhood",
            &verdict,
            "2026-01-01T00:00:00Z",
        ));
    }

    /// A Robinhood chain read with no `REALORRUG_ROBINHOOD_RPC` configured must
    /// answer `budget`, not `cant_read` -- the two are different messages to
    /// a visitor ("we're switched off" versus "we tried and failed"), and
    /// `dispatch`'s own message text is what `unreadable_doc` matches on.
    #[test]
    fn no_robinhood_endpoint_answers_budget_not_cant_read() {
        let (status, doc) = unreadable_doc(
            "0x0000000000000000000000000000000000000000",
            "robinhood",
            "no Robinhood endpoint is configured, so this token's chain cannot be read",
        );
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(doc["state"], "budget");
    }

    /// A real address the factory has no record of answers `not_a_token`, not
    /// `cant_read` -- distinct states the site shows differently.
    #[test]
    fn no_record_of_the_token_answers_not_a_token() {
        let (status, doc) = unreadable_doc(
            "0x0000000000000000000000000000000000000000",
            "robinhood",
            "the factory has no record of this token",
        );
        assert_eq!(status, StatusCode::OK);
        assert_eq!(doc["state"], "not_a_token");
    }

    #[tokio::test]
    async fn a_malformed_address_answers_bad_address() {
        let state = Arc::new(CheckState::from_vars(&no_env));
        let (status, doc) = check(&state, "not-shaped-like-an-address", "1.1.1.1").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(doc["state"], "bad_address");
        assert_contract(&doc);
    }

    #[tokio::test]
    async fn a_rate_limited_ip_gets_busy_before_anything_else_runs() {
        let state = Arc::new(CheckState::from_vars(&no_env));
        for _ in 0..PER_IP_LIMIT {
            assert!(state.allow("3.3.3.3", Instant::now()));
        }
        let (status, doc) = check(
            &state,
            "So11111111111111111111111111111111111111112",
            "3.3.3.3",
        )
        .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(doc["state"], "busy");
    }

    /// Only when `REALORRUG_TRUST_CLOUDFLARE` is set does the header count; by
    /// default a visitor cannot spoof `CF-Connecting-IP` to dodge the limit.
    #[test]
    fn the_cloudflare_header_is_ignored_unless_trusted() {
        let untrusting = CheckState::from_vars(&no_env);
        let mut headers = HeaderMap::new();
        headers.insert("CF-Connecting-IP", "9.9.9.9".parse().unwrap());
        assert_eq!(client_ip(&untrusting, &headers, None), "unknown");

        let trusting =
            CheckState::from_vars(&|k| (k == "REALORRUG_TRUST_CLOUDFLARE").then(|| "1".to_owned()));
        assert_eq!(client_ip(&trusting, &headers, None), "9.9.9.9");
    }

    #[test]
    fn the_cache_key_is_filesystem_safe() {
        assert_eq!(cache_key("solana:abc/../etc"), "solana_abc____etc");
    }

    #[test]
    fn an_untrusted_field_is_found_by_its_own_label() {
        let pairs = vec![
            ("token name".to_owned(), "Pepe".to_owned()),
            ("token symbol".to_owned(), "PEPE".to_owned()),
        ];
        assert_eq!(untrusted_field(&pairs, "token symbol"), Some("PEPE"));
        assert_eq!(untrusted_field(&pairs, "token name"), Some("Pepe"));
        assert_eq!(untrusted_field(&pairs, "website"), None);
    }

    #[test]
    fn the_client_document_drops_the_card_stash_and_keeps_the_rest() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("entry.json");
        let doc = json!({"state": "verdict", "_name": "Pepe", "_symbol": "PEPE"});
        write_cache(&path, &doc, 1_000).expect("write");
        let served = fresh_cached(&path, 1_001).expect("fresh");
        assert_eq!(served["state"], "verdict");
        assert!(served.get("_name").is_none() && served.get("_symbol").is_none());
        let raw = fresh_cached_raw(&path, 1_001).expect("fresh");
        assert_eq!(raw["_name"], "Pepe");
    }

    #[test]
    fn a_stale_cache_entry_is_a_miss() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("entry.json");
        write_cache(&path, &json!({"state": "verdict"}), 1_000).expect("write");
        assert!(
            fresh_cached(&path, 1_000 + CACHE_TTL_SECS + 1).is_none(),
            "an entry older than the TTL must be a miss"
        );
        assert!(
            fresh_cached(&path, 1_000 + CACHE_TTL_SECS - 1).is_some(),
            "a document one second short of its TTL is still fresh"
        );
        // Exactly at the TTL is still fresh: the rule is "older than", so a
        // `>=` here would throw away a document on its last valid second.
        assert!(
            fresh_cached(&path, 1_000 + CACHE_TTL_SECS).is_some(),
            "an entry inside the TTL must be a hit"
        );
    }
}
