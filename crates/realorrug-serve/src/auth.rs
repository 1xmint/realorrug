// SPDX-License-Identifier: Apache-2.0
//! Sign-in with X, sessions and the CSRF check (design 0032 §4, ADR 0041).
//!
//! # What this holds, and what it refuses to
//!
//! The site's forecast routes need to know which player is calling and nothing
//! else about them. This module does OAuth 2 with PKCE against X, reads
//! `/2/users/me` **once**, drops the access token, and keeps a random session
//! whose SHA-256 is the only thing written down. It never holds a spending key
//! and never posts anything: the only outbound calls are the token exchange and
//! that one read, and the read is paid for out of a serve-owned
//! [`Meter`](realorrug_provider::Meter) (rule 1, rule 7).
//!
//! # Deny by default (AGENTS.md rule 7)
//!
//! - No store path, X client id or redirect URI: every route here answers 503
//!   and nothing is opened.
//! - No `REALORRUG_SERVE_MONTHLY_USD`, or one that does not parse to a positive
//!   dollar figure: `/auth/x/start` and the callback answer 503 and make no
//!   call to X. There is no default allowance, because a default is a
//!   spending decision made by whoever wrote the code.
//! - No configured origin: no CORS header (`public.rs`'s existing rule).
//!
//! # One session per player
//!
//! The identity row holds one `session_hash`, so a new sign-in replaces the old
//! session. That is the cheapest thing that satisfies "the server keeps only a
//! hash", and the privacy notice says so. The session's issue time is the
//! identity row's `signed_in_at`, so it also dies with `delete_identity` and
//! there is no second table to outlive a deletion.

#![allow(
    clippy::result_large_err,
    reason = "a ready axum Response is the error: the caller returns it as is, and boxing it would only add a deref at every site"
)]

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{ConnectInfo, Query, Request, State};
use axum::http::header::{
    ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_MAX_AGE, CACHE_CONTROL, CONTENT_TYPE, COOKIE,
    LOCATION, ORIGIN, SET_COOKIE, VARY,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use realorrug_provider::{Budget, Commitment, Ledger, Meter};
use realorrug_store::{Identity, PlayerKey, Store, StoreError};
use realorrug_types::MicroUsd;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// A session lives thirty days from the sign-in that issued it (design 0032
/// §4).
pub(crate) const SESSION_TTL_SECS: i64 = 30 * 86_400;
/// How long a sign-in attempt may take between `/auth/x/start` and the
/// callback.
const OAUTH_TTL_SECS: i64 = 600;
/// Sign-in attempts one IP may start in [`START_WINDOW_SECS`].
const START_LIMIT: usize = 10;
/// The rate limit's window: an hour.
const START_WINDOW_SECS: i64 = 3_600;
/// Most sign-ins in flight at once. A full table refuses new starts rather
/// than growing without bound, which is the direction a flood should fail in.
const PENDING_CAP: usize = 4_096;
/// Most distinct IPs the limiter remembers, for the same reason.
const LIMITER_CAP: usize = 10_000;
/// One `/2/users/me` read: $0.010 (design 0032 §4, research 0051 §1).
const READ_COST: MicroUsd = MicroUsd(10_000);

/// The session cookie's name.
pub(crate) const SESSION_COOKIE: &str = "rr_session";
/// The short-lived cookie tying a callback to the browser that started it.
const OAUTH_COOKIE: &str = "rr_oauth";
/// The header a POST carries its CSRF token in.
pub(crate) const CSRF_HEADER: &str = "x-csrf-token";

const AUTHORIZE_URL: &str = "https://x.com/i/oauth2/authorize";
const TOKEN_URL: &str = "https://api.x.com/2/oauth2/token";
const ME_URL: &str = "https://api.x.com/2/users/me?user.fields=created_at";
/// `users.read` is what `/2/users/me` needs; it also asks for `tweet.read`,
/// which X requires alongside it. Nothing here can write.
const SCOPE: &str = "users.read tweet.read";

/// Unix seconds. Injectable so a test decides what time it is.
pub(crate) type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// The wall clock.
pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// The X seam
// ---------------------------------------------------------------------------

/// An X access token. Its `Debug` says nothing, so a stray `{:?}` cannot put it
/// in a log, and it is not `Clone`: it is handed to one read and dropped.
pub(crate) struct AccessToken(String);

impl std::fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccessToken(<redacted>)")
    }
}

/// What `/2/users/me` said about the caller, and nothing more.
pub(crate) struct XUser {
    pub(crate) id: String,
    pub(crate) handle: String,
    /// Unix seconds, when X returned `created_at` in a form this could read.
    pub(crate) created_at: Option<i64>,
}

/// Why an X call failed. A short label, never a response body or a token.
#[derive(Debug)]
pub(crate) struct XError(pub(crate) String);

/// The two calls sign-in makes to X, behind a trait so a test injects a fake
/// and makes none.
pub(crate) trait XClient: Send + Sync {
    /// Trades the authorisation code (with the PKCE verifier) for an access
    /// token.
    fn exchange_code(&self, code: &str, verifier: &str) -> Result<AccessToken, XError>;
    /// `GET /2/users/me?user.fields=created_at`. The paid read.
    fn me(&self, token: &AccessToken) -> Result<XUser, XError>;
}

/// The real client. No `Debug`: it holds the client secret.
struct HttpX {
    agent: ureq::Agent,
    client_id: String,
    redirect_uri: String,
    client_secret: Option<String>,
}

impl HttpX {
    fn new(client_id: &str, redirect_uri: &str, client_secret: Option<String>) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        Self {
            agent,
            client_id: client_id.to_owned(),
            redirect_uri: redirect_uri.to_owned(),
            client_secret,
        }
    }
}

impl XClient for HttpX {
    fn exchange_code(&self, code: &str, verifier: &str) -> Result<AccessToken, XError> {
        let mut request = self.agent.post(TOKEN_URL);
        // A confidential X app authenticates with Basic client_id:secret; a
        // public one sends its id in the form and no header.
        if let Some(secret) = &self.client_secret {
            let pair = format!("{}:{secret}", self.client_id);
            let basic = base64::engine::general_purpose::STANDARD.encode(pair);
            request = request.header("Authorization", format!("Basic {basic}"));
        }
        let mut response = request
            .send_form([
                ("code", code),
                ("grant_type", "authorization_code"),
                ("client_id", self.client_id.as_str()),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("code_verifier", verifier),
            ])
            .map_err(|e| XError(format!("the token exchange failed: {}", short(&e))))?;
        let doc: Value = response
            .body_mut()
            .read_json()
            .map_err(|_| XError("the token exchange answered something unreadable".into()))?;
        doc.get("access_token")
            .and_then(Value::as_str)
            .map(|t| AccessToken(t.to_owned()))
            .ok_or_else(|| XError("the token exchange returned no access token".into()))
    }

    fn me(&self, token: &AccessToken) -> Result<XUser, XError> {
        let mut response = self
            .agent
            .get(ME_URL)
            .header("Authorization", format!("Bearer {}", token.0))
            .call()
            .map_err(|e| XError(format!("the account read failed: {}", short(&e))))?;
        let doc: Value = response
            .body_mut()
            .read_json()
            .map_err(|_| XError("the account read answered something unreadable".into()))?;
        let data = doc
            .get("data")
            .ok_or_else(|| XError("the account read returned no data".into()))?;
        let text = |k: &str| data.get(k).and_then(Value::as_str).map(str::to_owned);
        let (Some(id), Some(handle)) = (text("id"), text("username")) else {
            return Err(XError("the account read returned no id or handle".into()));
        };
        Ok(XUser {
            id,
            handle,
            created_at: text("created_at").as_deref().and_then(parse_utc),
        })
    }
}

/// `ureq`'s error as a label that cannot carry a header or a body.
fn short(e: &ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(code) => format!("X answered {code}"),
        ureq::Error::Timeout(_) => "timed out".to_owned(),
        _ => "could not reach X".to_owned(),
    }
}

/// `2019-08-24T14:15:22.000Z` as unix seconds. UTC only; X returns nothing else.
fn parse_utc(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    let num = |from: usize, to: usize| text.get(from..to)?.parse::<i64>().ok();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    // Hinnant's days_from_civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// The serve-owned monthly allowance for `/2/users/me` reads, in dollars.
///
/// Read with `monthly_allowance_from`'s discipline (unset, unparseable,
/// non-finite or not above zero is `None`, which closes sign-in), from
/// `REALORRUG_SERVE_MONTHLY_USD`. It is one small function so it can be
/// swapped for the provider crate's own `serve_monthly_allowance_from` when
/// that lands (PR #205); nothing else here parses the variable.
pub(crate) fn serve_monthly_allowance_from(
    get: &impl Fn(&str) -> Option<String>,
) -> Option<MicroUsd> {
    // `from_dollars` already answers zero for a non-finite or non-positive
    // figure, and the filter refuses zero (which is also what a fraction of a
    // micro-dollar rounds to): a second check before it would only repeat it.
    get("REALORRUG_SERVE_MONTHLY_USD")?
        .trim()
        .parse::<f64>()
        .ok()
        .map(MicroUsd::from_dollars)
        .filter(|m| m.get() > 0)
}

/// The meter's day number for a unix time. One function, so the start-up
/// ledger and every booking agree on what a day is.
fn day_of(unix_seconds: i64) -> u64 {
    u64::try_from(unix_seconds / 86_400).unwrap_or(0)
}

/// The origins allowed to call the API from a browser with credentials, from
/// `REALORRUG_APP_ORIGINS` (comma separated, exact). A `*`, a path or a scheme
/// that is not http(s) is dropped rather than repaired: a wildcard with
/// credentials is the one thing this list exists to prevent.
pub(crate) fn app_origins_from(get: &impl Fn(&str) -> Option<String>) -> Vec<String> {
    let Some(raw) = get("REALORRUG_APP_ORIGINS") else {
        return Vec::new();
    };
    raw.split(',')
        .map(|o| o.trim().trim_end_matches('/'))
        .filter(|o| {
            let Some(rest) = o
                .strip_prefix("https://")
                .or_else(|| o.strip_prefix("http://"))
            else {
                return false;
            };
            !rest.is_empty()
                && !rest.contains(['*', '/', '?', '#', ' '])
                && HeaderValue::from_str(o).is_ok()
        })
        .map(str::to_owned)
        .collect()
}

fn nonempty(get: &impl Fn(&str) -> Option<String>, name: &str) -> Option<String> {
    get(name)
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// A serve-owned spend meter and where its ledger lives. Never the analyst's
/// or the CLI's ledger: a sign-in flood must not eat their allowance, and
/// theirs must not open this one.
struct Spend {
    meter: Meter,
    path: PathBuf,
}

impl Spend {
    /// Restores today's ledger if there is one. A file that exists but cannot
    /// be read is an error, not a fresh allowance: a corrupt ledger that reset
    /// to zero would hand back everything already spent.
    fn open(path: PathBuf, budget: Budget, day: u64) -> Result<Self, String> {
        let meter = match std::fs::read_to_string(&path) {
            Ok(text) => {
                let ledger: Ledger = serde_json::from_str(&text)
                    .map_err(|_| "the spend ledger is unreadable".to_owned())?;
                Meter::restore(budget, &ledger, day)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Meter::new(budget, day),
            Err(_) => return Err("the spend ledger cannot be read".to_owned()),
        };
        Ok(Self { meter, path })
    }

    /// Writes the ledger through a temp file and a rename, so a crash leaves
    /// the old ledger or the new one, never half of one.
    fn persist(&self) -> std::io::Result<()> {
        let json = serde_json::to_string(&self.meter.ledger()).map_err(std::io::Error::other)?;
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)
    }
}

/// A sign-in that has started and not yet come back.
struct Pending {
    verifier: String,
    expires: i64,
}

/// Everything that exists only when sign-in is configured.
struct Live {
    store: Mutex<Store>,
    x: Arc<dyn XClient>,
    client_id: String,
    redirect_uri: String,
    return_to: String,
    trust_cloudflare: bool,
    /// `None` when no valid allowance is configured: sign-in is closed.
    spend: Option<Mutex<Spend>>,
    pending: Mutex<HashMap<String, Pending>>,
    starts: Mutex<HashMap<String, Vec<i64>>>,
}

/// The auth module's shared state. Always constructible: an unconfigured box
/// gets one whose routes answer 503.
pub(crate) struct AuthState {
    live: Option<Live>,
    pub(crate) clock: Clock,
    /// The exact origins allowed to call with credentials. Empty sends no CORS
    /// header at all (AGENTS.md rule 7).
    pub(crate) app_origins: Vec<String>,
    /// The rounds file (`REALORRUG_ROUNDS_FILE`). `None` refuses every forecast.
    pub(crate) rounds_path: Option<PathBuf>,
}

/// A signed-in caller: whose forecasts these are, and the CSRF token their
/// session expects on a POST.
pub(crate) struct Session {
    pub(crate) player: PlayerKey,
    pub(crate) csrf: String,
    pub(crate) expires_at: i64,
}

impl AuthState {
    /// From the process environment, with the real X client.
    pub(crate) fn from_env() -> Self {
        Self::from_vars(&|k| std::env::var(k).ok(), None, Arc::new(now_secs))
    }

    /// From any variable source. `x` replaces the real X client (a test's
    /// fake); `None` builds the real one from the client id, redirect URI and
    /// optional secret.
    pub(crate) fn from_vars(
        get: &impl Fn(&str) -> Option<String>,
        x: Option<Arc<dyn XClient>>,
        clock: Clock,
    ) -> Self {
        let app_origins = app_origins_from(get);
        let live = Self::live_from(get, x, &clock, &app_origins);
        let rounds_path = nonempty(get, "REALORRUG_ROUNDS_FILE").map(PathBuf::from);
        Self {
            live,
            clock,
            app_origins,
            rounds_path,
        }
    }

    fn live_from(
        get: &impl Fn(&str) -> Option<String>,
        x: Option<Arc<dyn XClient>>,
        clock: &Clock,
        app_origins: &[String],
    ) -> Option<Live> {
        let store_path = nonempty(get, "REALORRUG_STORE_PATH")?;
        let client_id = nonempty(get, "REALORRUG_X_CLIENT_ID")?;
        let redirect_uri = nonempty(get, "REALORRUG_X_REDIRECT_URI")?;
        let store = match Store::open(Path::new(&store_path)) {
            Ok(store) => store,
            Err(e) => {
                eprintln!("realorrug-serve: sign-in is off, the store did not open: {e}");
                return None;
            }
        };
        record_head(&store);
        let x = x.unwrap_or_else(|| {
            Arc::new(HttpX::new(
                &client_id,
                &redirect_uri,
                nonempty(get, "REALORRUG_X_CLIENT_SECRET"),
            ))
        });
        let spend = serve_monthly_allowance_from(get).and_then(|monthly| {
            let budget = Budget {
                per_call_max: READ_COST,
                // The month is the stop that was asked for; a day cap of the
                // whole month adds no second, invented number.
                daily_max: monthly,
                monthly_max: monthly,
            };
            let mut ledger = store_path.clone().into_bytes();
            ledger.extend_from_slice(b".spend.json");
            let ledger = PathBuf::from(String::from_utf8(ledger).ok()?);
            let day = day_of(clock());
            match Spend::open(ledger, budget, day) {
                Ok(spend) => Some(Mutex::new(spend)),
                Err(why) => {
                    eprintln!("realorrug-serve: sign-in is closed, {why}");
                    None
                }
            }
        });
        let trust_cloudflare =
            nonempty(get, "REALORRUG_TRUST_CLOUDFLARE").is_some_and(|v| v == "1");
        Some(Live {
            store: Mutex::new(store),
            x,
            client_id,
            redirect_uri,
            return_to: app_origins
                .first()
                .cloned()
                .unwrap_or_else(|| "/".to_owned()),
            trust_cloudflare,
            spend,
            pending: Mutex::new(HashMap::new()),
            starts: Mutex::new(HashMap::new()),
        })
    }

    fn live(&self) -> Result<&Live, Response> {
        self.live.as_ref().ok_or_else(|| {
            refuse(
                StatusCode::SERVICE_UNAVAILABLE,
                "sign-in is not configured on this server",
            )
        })
    }

    /// Runs `f` on the store, mapping a store failure to a response that says
    /// nothing about the store's internals. `StoreError::Sqlite` (a busy
    /// database) means the write did not happen, and the reply says so.
    pub(crate) fn with_store<T>(
        &self,
        f: impl FnOnce(&Store) -> Result<T, StoreError>,
    ) -> Result<T, Response> {
        let live = self.live()?;
        let store = live.store.lock().map_err(|_| {
            refuse(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the store is unavailable",
            )
        })?;
        f(&store).map_err(|e| match e {
            StoreError::Sqlite(_) => refuse(
                StatusCode::SERVICE_UNAVAILABLE,
                "the store is busy; nothing was saved, try again",
            ),
            _ => refuse(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the store refused the request",
            ),
        })
    }

    /// The caller behind a request's session cookie.
    ///
    /// With `csrf_required`, also the CSRF token on the request, checked
    /// against the one this session derives. A POST that changes anything
    /// asks for it; a GET that only reads does not.
    ///
    /// # Errors
    ///
    /// A ready response: 503 when sign-in is off, 401 for no session, an
    /// unknown one or an expired one, 403 for a missing or wrong CSRF token.
    pub(crate) fn authenticate(
        &self,
        headers: &HeaderMap,
        csrf_required: bool,
    ) -> Result<Session, Response> {
        self.live()?;
        let token = cookie(headers, SESSION_COOKIE)
            .filter(|t| is_token(t))
            .ok_or_else(signed_out)?;
        let hash = sha256_hex(token.as_bytes());
        let found = self.with_store(|s| s.player_for_session_hash(&hash))?;
        let (player, issued) = found.ok_or_else(signed_out)?;
        let now = (self.clock)();
        let expires_at = issued.saturating_add(SESSION_TTL_SECS);
        // `>=`: the session is good for thirty days, not thirty days and a
        // second.
        if now >= expires_at {
            return Err(signed_out());
        }
        let csrf = csrf_for(token);
        if csrf_required {
            let sent = headers
                .get(CSRF_HEADER)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if !ct_eq(sent, &csrf) {
                return Err(refuse(
                    StatusCode::FORBIDDEN,
                    "missing or wrong CSRF token; read it from /auth/me",
                ));
            }
        }
        Ok(Session {
            player,
            csrf,
            expires_at,
        })
    }
}

impl Live {
    /// Books one `/2/users/me` read against the meter, or answers why not.
    fn authorize(&self, now: i64) -> Result<Commitment, Response> {
        let Some(spend) = &self.spend else {
            return Err(closed());
        };
        let mut spend = spend.lock().map_err(|_| {
            refuse(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the spend record is unavailable",
            )
        })?;
        let day = day_of(now);
        let Ok(commitment) = spend.meter.authorize(READ_COST, day) else {
            let _ = spend.persist();
            return Err(refuse(
                StatusCode::SERVICE_UNAVAILABLE,
                "sign-in is closed until the allowance for reading X accounts renews",
            ));
        };
        // The reservation is written before the call is made: a crash in
        // flight counts as spent, which is the safe direction. If it cannot
        // be written the call is not made.
        if spend.persist().is_err() {
            spend.meter.release(commitment);
            return Err(closed());
        }
        Ok(commitment)
    }

    /// Records what the read cost (`settle`), or gives the reservation back
    /// because the call never reached the paid read (`release`).
    fn conclude(&self, commitment: Commitment, read_happened: bool) {
        let Some(spend) = &self.spend else { return };
        let Ok(mut spend) = spend.lock() else { return };
        if read_happened {
            spend.meter.settle(commitment, READ_COST);
        } else {
            spend.meter.release(commitment);
        }
        if spend.persist().is_err() {
            eprintln!("realorrug-serve: the spend ledger could not be written");
        }
    }

    /// Counts a sign-in start against `ip`. False once the IP has made
    /// [`START_LIMIT`] in the last hour, or when the table is full.
    fn allow_start(&self, ip: &str, now: i64) -> bool {
        let Ok(mut starts) = self.starts.lock() else {
            return false;
        };
        if starts.len() >= LIMITER_CAP && !starts.contains_key(ip) {
            starts.retain(|_, stamps| stamps.iter().any(|t| now - t < START_WINDOW_SECS));
            if starts.len() >= LIMITER_CAP {
                return false;
            }
        }
        let stamps = starts.entry(ip.to_owned()).or_default();
        stamps.retain(|t| now - t < START_WINDOW_SECS);
        if stamps.len() >= START_LIMIT {
            return false;
        }
        stamps.push(now);
        true
    }

    /// Stores a started sign-in. False when too many are in flight.
    fn remember(&self, state: &str, verifier: &str, now: i64) -> bool {
        let Ok(mut pending) = self.pending.lock() else {
            return false;
        };
        if pending.len() >= PENDING_CAP {
            pending.retain(|_, p| p.expires > now);
            if pending.len() >= PENDING_CAP {
                return false;
            }
        }
        pending.insert(
            state.to_owned(),
            Pending {
                verifier: verifier.to_owned(),
                expires: now + OAUTH_TTL_SECS,
            },
        );
        true
    }

    /// The verifier for a state, consuming it: a callback works once.
    fn take(&self, state: &str, now: i64) -> Option<String> {
        let mut pending = self.pending.lock().ok()?;
        let entry = pending.remove(state)?;
        (entry.expires > now).then_some(entry.verifier)
    }
}

// ---------------------------------------------------------------------------
// Small pure helpers
// ---------------------------------------------------------------------------

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest.as_slice() {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// The CSRF token for a session: a hash of the raw session token under a
/// domain label, so it is unguessable without the cookie, stable for the
/// session's life, and needs no second thing stored.
pub(crate) fn csrf_for(session_token: &str) -> String {
    sha256_hex(format!("realorrug-csrf-v1:{session_token}").as_bytes())
}

/// `bytes` random bytes from the operating system, as lower-case hex.
fn random_hex(bytes: usize) -> Option<String> {
    let mut buf = vec![0_u8; bytes];
    getrandom::fill(&mut buf).ok()?;
    let mut out = String::with_capacity(bytes * 2);
    for b in buf {
        let _ = write!(out, "{b:02x}");
    }
    Some(out)
}

/// A token this module issued: 64 lower-case hex characters. Anything else in
/// the cookie is not ours and never reaches the store.
fn is_token(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Equal-length comparison that does not stop at the first difference.
fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0_u8, |d, (x, y)| d | (x ^ y)) == 0
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (k, v) = pair.trim().split_once('=')?;
            (k == name).then_some(v)
        })
}

/// Percent-encodes everything but RFC 3986's unreserved characters.
fn pct(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

fn session_cookie(token: &str) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Max-Age={SESSION_TTL_SECS}; Path=/; HttpOnly; Secure; SameSite=Lax"
    )
}

fn clear_cookie(name: &str, path: &str) -> String {
    format!("{name}=; Max-Age=0; Path={path}; HttpOnly; Secure; SameSite=Lax")
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

pub(crate) fn refuse(status: StatusCode, why: &str) -> Response {
    let mut response = (status, Json(json!({ "error": why }))).into_response();
    no_store(&mut response);
    response
}

fn signed_out() -> Response {
    refuse(
        StatusCode::UNAUTHORIZED,
        "not signed in, or the session ended",
    )
}

fn closed() -> Response {
    refuse(
        StatusCode::SERVICE_UNAVAILABLE,
        "sign-in is closed: no spending allowance is configured for reading X accounts",
    )
}

/// Nothing here is cacheable: a shared cache holding a `Set-Cookie` or a
/// CSRF token would hand one visitor's session to the next.
pub(crate) fn no_store(response: &mut Response) {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

fn with_cookies(mut response: Response, cookies: &[String]) -> Response {
    for c in cookies {
        if let Ok(v) = HeaderValue::from_str(c) {
            response.headers_mut().append(SET_COOKIE, v);
        }
    }
    no_store(&mut response);
    response
}

fn redirect(to: &str) -> Response {
    let mut response = StatusCode::FOUND.into_response();
    let target = HeaderValue::from_str(to).unwrap_or_else(|_| HeaderValue::from_static("/"));
    response.headers_mut().insert(LOCATION, target);
    response
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

/// Credentialed CORS for the sign-in and forecast routes, to one exact origin
/// at a time.
///
/// The header is set only when the request's `Origin` equals, byte for byte,
/// an origin in `REALORRUG_APP_ORIGINS`. It is never `*` -- a wildcard with
/// credentials is refused by browsers and is the thing this list exists to
/// prevent -- and with no origin configured nothing is sent (AGENTS.md rule
/// 7), so a browser elsewhere cannot read a signed-in reply. `Vary: Origin`
/// always, so a shared cache never serves one origin's answer to another.
pub(crate) async fn cors(
    State(auth): State<Arc<AuthState>>,
    request: Request,
    next: axum::middleware::Next,
) -> Response {
    let allowed = request
        .headers()
        .get(ORIGIN)
        .and_then(|o| o.to_str().ok())
        .filter(|o| auth.app_origins.iter().any(|a| a == o))
        .and_then(|o| HeaderValue::from_str(o).ok());
    let preflight = request.method() == axum::http::Method::OPTIONS;
    let mut response = if preflight {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    headers.append(VARY, HeaderValue::from_static("Origin"));
    if let Some(origin) = allowed {
        headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        headers.insert(
            ACCESS_CONTROL_ALLOW_CREDENTIALS,
            HeaderValue::from_static("true"),
        );
        if preflight {
            headers.insert(
                ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST, OPTIONS"),
            );
            headers.insert(
                ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("content-type, x-csrf-token"),
            );
            headers.insert(ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("600"));
        }
    }
    response
}

/// Writes the chain's head to the process log at start-up, where the service
/// user cannot rewrite it (journald keeps it under its own account).
///
/// `Verified::Intact` proves the rows there chain from genesis and cannot
/// prove a tail was not cut off; only a head recorded somewhere the writer of
/// the database cannot reach can (design 0032 §3). The head is a hash: it
/// carries no identity and, unlike the row count beside it, is not printed to
/// any response -- a count before close would tell a reader how many calls are
/// hidden.
fn record_head(store: &Store) {
    match store.verify() {
        Ok(realorrug_store::Verified::Intact { head, .. }) => {
            eprintln!("realorrug-serve: store head {head}");
        }
        Ok(realorrug_store::Verified::Broken { at, .. }) => {
            eprintln!("realorrug-serve: THE STORE CHAIN DOES NOT VERIFY (first fault at row {at})");
        }
        Err(_) => eprintln!("realorrug-serve: the store chain could not be read"),
    }
}

/// The sign-in, session and account routes.
pub(crate) fn router(state: Arc<AuthState>) -> Router {
    Router::new()
        .route("/auth/x/start", get(start))
        .route("/auth/x/callback", get(callback))
        .route("/auth/me", get(me))
        .route("/auth/logout", post(logout))
        .route("/account/delete", post(delete_account))
        .with_state(state)
}

/// `GET /auth/x/start`: sets the state cookie and sends the browser to X.
async fn start(State(auth): State<Arc<AuthState>>, req: Request) -> Response {
    let live = match auth.live() {
        Ok(live) => live,
        Err(refusal) => return refusal,
    };
    // Closed before it is counted: a box with no allowance should not spend
    // a visitor's ten attempts on a page that cannot work.
    if live.spend.is_none() {
        return closed();
    }
    let now = (auth.clock)();
    let connect_info = req
        .extensions()
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .copied();
    let ip = crate::check::client_ip_from(live.trust_cloudflare, req.headers(), connect_info);
    if !live.allow_start(&ip, now) {
        return refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "too many sign-in attempts from this address; try again in an hour",
        );
    }
    let (Some(state), Some(verifier)) = (random_hex(32), random_hex(32)) else {
        return refuse(StatusCode::INTERNAL_SERVER_ERROR, "no randomness available");
    };
    if !live.remember(&state, &verifier, now) {
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "too many sign-ins in flight; try again shortly",
        );
    }
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    let location = format!(
        "{AUTHORIZE_URL}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={state}\
         &code_challenge={challenge}&code_challenge_method=S256",
        pct(&live.client_id),
        pct(&live.redirect_uri),
        pct(SCOPE),
    );
    let cookie = format!(
        "{OAUTH_COOKIE}={state}; Max-Age={OAUTH_TTL_SECS}; Path=/auth/x; HttpOnly; Secure; SameSite=Lax"
    );
    with_cookies(redirect(&location), &[cookie])
}

/// What X sends the browser back with.
#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// How the two X calls ended.
enum XOutcome {
    /// The exchange failed, so the paid read was never made.
    NoRead(XError),
    /// The read was made, whatever it answered.
    Read(Result<XUser, XError>),
}

/// `GET /auth/x/callback`: checks `state`, spends one metered read on X's
/// `/2/users/me`, drops the access token, and starts the session.
async fn callback(
    State(auth): State<Arc<AuthState>>,
    headers: HeaderMap,
    Query(query): Query<CallbackQuery>,
) -> Response {
    let live = match auth.live() {
        Ok(live) => live,
        Err(refusal) => return refusal,
    };
    let now = (auth.clock)();
    let bad_state = || {
        refuse(
            StatusCode::BAD_REQUEST,
            "the sign-in did not match this browser; start again",
        )
    };
    // The state must be the one this browser was given AND one this server
    // issued and has not yet used. Both, so neither a forged callback link
    // (no cookie) nor a replayed one (state already consumed) gets as far as
    // the paid read. Nothing here has touched the meter or X.
    let (Some(from_url), Some(from_cookie)) =
        (query.state.as_deref(), cookie(&headers, OAUTH_COOKIE))
    else {
        return bad_state();
    };
    if !ct_eq(from_url, from_cookie) {
        return bad_state();
    }
    let Some(verifier) = live.take(from_url, now) else {
        return bad_state();
    };
    let cleared = clear_cookie(OAUTH_COOKIE, "/auth/x");
    let Some(code) = query
        .code
        .filter(|c| !c.is_empty() && query.error.is_none())
    else {
        return with_cookies(
            refuse(StatusCode::BAD_REQUEST, "X did not approve the sign-in"),
            &[cleared],
        );
    };

    // Books the read before any call to X is made (rule 7).
    let commitment = match live.authorize(now) {
        Ok(c) => c,
        Err(refusal) => return with_cookies(refusal, &[cleared]),
    };
    let x = Arc::clone(&live.x);
    let outcome = tokio::task::spawn_blocking(move || {
        // The access token lives inside this closure and nowhere else.
        match x.exchange_code(&code, &verifier) {
            Err(e) => XOutcome::NoRead(e),
            Ok(token) => XOutcome::Read(x.me(&token)),
        }
    })
    .await;
    let (read_happened, user) = match outcome {
        Ok(XOutcome::NoRead(why)) => {
            eprintln!(
                "realorrug-serve: sign-in stopped before the read: {}",
                why.0
            );
            (false, Err(()))
        }
        Ok(XOutcome::Read(user)) => (true, user.map_err(|_| ())),
        // A panic inside the client: unknown whether the read happened, so
        // count it.
        Err(_) => (true, Err(())),
    };
    live.conclude(commitment, read_happened);
    let Ok(user) = user else {
        return with_cookies(
            refuse(
                StatusCode::BAD_GATEWAY,
                "X did not complete the sign-in; try again",
            ),
            &[cleared],
        );
    };

    let Some(session_token) = random_hex(32) else {
        return refuse(StatusCode::INTERNAL_SERVER_ERROR, "no randomness available");
    };
    let signed = auth.with_store(|store| {
        sign_in(
            store,
            &user,
            &sha256_hex(session_token.as_bytes()),
            now,
            &|s, x_id| s.player_for_x_id(x_id),
        )
    });
    if let Err(refusal) = signed {
        return with_cookies(refusal, &[cleared]);
    }
    with_cookies(
        redirect(&live.return_to),
        &[session_cookie(&session_token), cleared],
    )
}

/// Finds the key an X id already has.
type Lookup<'a> = dyn Fn(&Store, &str) -> Result<Option<PlayerKey>, StoreError> + 'a;

/// Upserts the identity for an X account, keyed by a random player key, and
/// makes `session_hash` its one session.
///
/// A returning X id keeps the key it already has. Two sign-ins for one new X
/// id can both find no key and both mint one; the second `upsert` then gets
/// [`StoreError::XIdTaken`], and the right answer is to look the key up again
/// and use the winner's, not to fail a sign-in that did nothing wrong.
/// [`StoreError::KeyTaken`] is different: the store was offered a key already
/// held by another X account, which no lookup repairs, so it is refused.
///
/// `lookup` is a parameter only so a test can force the losing side of that
/// race.
fn sign_in(
    store: &Store,
    user: &XUser,
    session_hash: &str,
    now: i64,
    lookup: &Lookup<'_>,
) -> Result<(), StoreError> {
    let mut retries = 0;
    loop {
        let player_key = match lookup(store, &user.id)? {
            Some(key) => key,
            None => store.new_player_key()?,
        };
        let identity = Identity {
            player_key,
            x_id: user.id.clone(),
            handle: user.handle.clone(),
            account_created_at: user.created_at,
            session_hash: Some(session_hash.to_owned()),
            signed_in_at: now,
        };
        match store.upsert_identity(&identity) {
            Err(StoreError::XIdTaken) if retries < 3 => retries += 1,
            other => return other,
        }
    }
}

/// `GET /auth/me`: who the session belongs to, and the CSRF token for its
/// POSTs. The handle is the caller's own; the player key is never sent, so
/// there is no response with a key beside a handle.
async fn me(State(auth): State<Arc<AuthState>>, headers: HeaderMap) -> Response {
    let session = match auth.authenticate(&headers, false) {
        Ok(session) => session,
        Err(refusal) => return refusal,
    };
    let identity = match auth.with_store(|s| s.identity(&session.player)) {
        Ok(Some(identity)) => identity,
        Ok(None) => return signed_out(),
        Err(refusal) => return refusal,
    };
    let mut response = (
        StatusCode::OK,
        [(CONTENT_TYPE, "application/json")],
        Json(json!({
            "signed_in": true,
            "handle": identity.handle,
            "csrf_token": session.csrf,
            "session_expires_at": session.expires_at,
        })),
    )
        .into_response();
    no_store(&mut response);
    response
}

/// `POST /auth/logout`: ends the session. The identity stays; only its
/// session hash goes.
async fn logout(State(auth): State<Arc<AuthState>>, headers: HeaderMap) -> Response {
    let session = match auth.authenticate(&headers, true) {
        Ok(session) => session,
        Err(refusal) => return refusal,
    };
    let ended = auth.with_store(|s| {
        if let Some(mut identity) = s.identity(&session.player)? {
            identity.session_hash = None;
            s.upsert_identity(&identity)?;
        }
        Ok(())
    });
    if let Err(refusal) = ended {
        return refusal;
    }
    with_cookies(
        (StatusCode::OK, Json(json!({ "signed_out": true }))).into_response(),
        &[clear_cookie(SESSION_COOKIE, "/")],
    )
}

/// `POST /account/delete`: removes the identity row (handle, X id, account
/// age, session hash, and with them the link from this person to their player
/// key). The chain rows stay, with nothing left that names a person
/// (design 0032 §4).
async fn delete_account(State(auth): State<Arc<AuthState>>, headers: HeaderMap) -> Response {
    let session = match auth.authenticate(&headers, true) {
        Ok(session) => session,
        Err(refusal) => return refusal,
    };
    if let Err(refusal) = auth.with_store(|s| s.delete_identity(&session.player)) {
        return refusal;
    }
    with_cookies(
        (StatusCode::OK, Json(json!({ "deleted": true }))).into_response(),
        &[clear_cookie(SESSION_COOKIE, "/")],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use http_body_util::BodyExt;
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
    use tower::ServiceExt;

    /// The access token the fake hands out. Every test that asks "was it
    /// stored" searches for exactly this.
    const FAKE_TOKEN: &str = "FAKE-ACCESS-TOKEN-7f3a9c41";
    /// 2026-10-01T00:00:00Z.
    const T0: i64 = 1_790_812_800;

    struct FakeX {
        calls: AtomicUsize,
        verifiers: Mutex<Vec<String>>,
        x_id: Mutex<String>,
    }

    impl FakeX {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
                verifiers: Mutex::new(Vec::new()),
                x_id: Mutex::new("1001".to_owned()),
            })
        }
        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl XClient for FakeX {
        fn exchange_code(&self, _code: &str, verifier: &str) -> Result<AccessToken, XError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.verifiers.lock().unwrap().push(verifier.to_owned());
            Ok(AccessToken(FAKE_TOKEN.to_owned()))
        }
        fn me(&self, token: &AccessToken) -> Result<XUser, XError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(token.0, FAKE_TOKEN);
            Ok(XUser {
                id: self.x_id.lock().unwrap().clone(),
                handle: "testhandle".to_owned(),
                created_at: parse_utc("2019-08-24T14:15:22.000Z"),
            })
        }
    }

    struct Harness {
        dir: tempfile::TempDir,
        fake: Arc<FakeX>,
        now: Arc<AtomicI64>,
        state: Arc<AuthState>,
    }

    impl Harness {
        fn with(monthly: Option<&str>) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let fake = FakeX::new();
            let now = Arc::new(AtomicI64::new(T0));
            let clock: Clock = {
                let now = Arc::clone(&now);
                Arc::new(move || now.load(Ordering::SeqCst))
            };
            let mut vars: HashMap<&str, String> = HashMap::from([
                (
                    "REALORRUG_STORE_PATH",
                    dir.path().join("store.db").to_string_lossy().into_owned(),
                ),
                ("REALORRUG_X_CLIENT_ID", "test-client".to_owned()),
                (
                    "REALORRUG_X_REDIRECT_URI",
                    "https://api.test/auth/x/callback".to_owned(),
                ),
                ("REALORRUG_APP_ORIGINS", "https://site.test".to_owned()),
            ]);
            if let Some(m) = monthly {
                vars.insert("REALORRUG_SERVE_MONTHLY_USD", m.to_owned());
            }
            let state = AuthState::from_vars(
                &|k| vars.get(k).cloned(),
                Some(fake.clone() as Arc<dyn XClient>),
                clock,
            );
            Self {
                dir,
                fake,
                now,
                state: Arc::new(state),
            }
        }

        fn open() -> Self {
            Self::with(Some("5"))
        }

        fn advance(&self, secs: i64) {
            self.now.fetch_add(secs, Ordering::SeqCst);
        }

        async fn send(&self, req: HttpRequest<Body>) -> (StatusCode, HeaderMap, String) {
            let response = router(Arc::clone(&self.state)).oneshot(req).await.unwrap();
            let (parts, body) = response.into_parts();
            let bytes = body.collect().await.unwrap().to_bytes();
            (
                parts.status,
                parts.headers,
                String::from_utf8_lossy(&bytes).into_owned(),
            )
        }

        async fn get(&self, path: &str, cookie: Option<&str>) -> (StatusCode, HeaderMap, String) {
            let mut req = HttpRequest::get(path);
            if let Some(c) = cookie {
                req = req.header(COOKIE, c);
            }
            self.send(req.body(Body::empty()).unwrap()).await
        }

        async fn post(
            &self,
            path: &str,
            session: Option<&str>,
            csrf: Option<&str>,
        ) -> (StatusCode, HeaderMap, String) {
            let mut req = HttpRequest::post(path);
            if let Some(s) = session {
                req = req.header(COOKIE, format!("{SESSION_COOKIE}={s}"));
            }
            if let Some(c) = csrf {
                req = req.header(CSRF_HEADER, c);
            }
            self.send(req.body(Body::empty()).unwrap()).await
        }

        /// Runs `start`, returning the `state` X would echo and the cookie
        /// the browser would hold.
        async fn begin(&self) -> (String, String, String) {
            let (status, headers, _) = self.get("/auth/x/start", None).await;
            assert_eq!(status, StatusCode::FOUND);
            let location = headers[LOCATION].to_str().unwrap().to_owned();
            let state = query_param(&location, "state");
            let set = headers[SET_COOKIE].to_str().unwrap();
            let cookie = set.split(';').next().unwrap().to_owned();
            (state, cookie, location)
        }

        /// A whole sign-in. Returns the raw session token.
        async fn sign_in(&self) -> String {
            let (state, cookie, _) = self.begin().await;
            let (status, headers, _) = self
                .get(
                    &format!("/auth/x/callback?code=abc&state={state}"),
                    Some(&cookie),
                )
                .await;
            assert_eq!(status, StatusCode::FOUND, "sign-in did not complete");
            session_token_in(&headers)
        }

        fn store_bytes(&self) -> Vec<u8> {
            std::fs::read(self.dir.path().join("store.db")).unwrap()
        }

        fn player_of(&self, x_id: &str) -> Option<PlayerKey> {
            self.state
                .live
                .as_ref()
                .unwrap()
                .store
                .lock()
                .unwrap()
                .player_for_x_id(x_id)
                .unwrap()
        }
    }

    fn chain_rows(store: &Store) -> usize {
        match store.verify().unwrap() {
            realorrug_store::Verified::Intact { rows, .. } => rows,
            broken @ realorrug_store::Verified::Broken { .. } => panic!("chain broken: {broken:?}"),
        }
    }

    fn query_param(url: &str, name: &str) -> String {
        url.split_once('?')
            .unwrap()
            .1
            .split('&')
            .find_map(|p| p.strip_prefix(&format!("{name}=")))
            .unwrap()
            .to_owned()
    }

    fn session_token_in(headers: &HeaderMap) -> String {
        headers
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find_map(|c| c.strip_prefix(&format!("{SESSION_COOKIE}=")))
            .expect("a session cookie")
            .split(';')
            .next()
            .unwrap()
            .to_owned()
    }

    fn contains(haystack: &[u8], needle: &str) -> bool {
        haystack
            .windows(needle.len())
            .any(|w| w == needle.as_bytes())
    }

    #[tokio::test]
    async fn a_state_that_does_not_match_the_browsers_is_refused() {
        let h = Harness::open();
        let (state, cookie, _) = h.begin().await;

        // The cookie's state is not the URL's: a forged callback link.
        let other = "0".repeat(64);
        let (status, _, _) = h
            .get(
                &format!("/auth/x/callback?code=abc&state={other}"),
                Some(&cookie),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        // No cookie at all.
        let (status, _, _) = h
            .get(&format!("/auth/x/callback?code=abc&state={state}"), None)
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        // A genuinely issued state, but not the one this browser was given:
        // an attacker's own link opened in a victim's browser.
        let (attacker_state, _, _) = h.begin().await;
        let (status, _, _) = h
            .get(
                &format!("/auth/x/callback?code=abc&state={attacker_state}"),
                Some(&cookie),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        // A state the browser holds but this server never issued.
        let forged = format!("{OAUTH_COOKIE}={other}");
        let (status, _, _) = h
            .get(
                &format!("/auth/x/callback?code=abc&state={other}"),
                Some(&forged),
            )
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);

        assert_eq!(h.fake.calls(), 0, "a refused state must not reach X");
        assert!(h.player_of("1001").is_none());

        // The genuine pair still works once, and only once.
        let good = format!("/auth/x/callback?code=abc&state={state}");
        assert_eq!(h.get(&good, Some(&cookie)).await.0, StatusCode::FOUND);
        assert_eq!(h.get(&good, Some(&cookie)).await.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn no_budget_refuses_before_any_call_to_x() {
        // Unset, and the values that must read as unset.
        for monthly in [
            None,
            Some(""),
            Some("abc"),
            Some("-5"),
            Some("0"),
            Some("NaN"),
        ] {
            let h = Harness::with(monthly);
            let (status, _, body) = h.get("/auth/x/start", None).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{monthly:?}");
            assert!(body.contains("sign-in is closed"), "{body}");
            assert_eq!(h.fake.calls(), 0, "{monthly:?}");
        }

        // An allowance smaller than one read: start is open (there is an
        // allowance) but the meter refuses the read before the exchange.
        let h = Harness::with(Some("0.005"));
        let (state, cookie, _) = h.begin().await;
        let (status, _, _) = h
            .get(
                &format!("/auth/x/callback?code=abc&state={state}"),
                Some(&cookie),
            )
            .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(h.fake.calls(), 0, "a refused spend must not reach X");
        assert!(h.player_of("1001").is_none());

        // And the allowance is a real cap: $0.02 buys two reads, not three.
        let h = Harness::with(Some("0.02"));
        for expected in [
            StatusCode::FOUND,
            StatusCode::FOUND,
            StatusCode::SERVICE_UNAVAILABLE,
        ] {
            let (state, cookie, _) = h.begin().await;
            let (status, _, _) = h
                .get(
                    &format!("/auth/x/callback?code=abc&state={state}"),
                    Some(&cookie),
                )
                .await;
            assert_eq!(status, expected);
        }
        assert_eq!(h.fake.calls(), 4, "two sign-ins, two calls each");
    }

    #[tokio::test]
    async fn the_spend_survives_a_restart() {
        // $0.01 is exactly one read.
        let h = Harness::with(Some("0.01"));
        h.sign_in().await;
        assert_eq!(h.fake.calls(), 2);

        // A second process over the same store path and clock: same ledger
        // file, so the read already spent is still spent.
        let vars: HashMap<&str, String> = HashMap::from([
            (
                "REALORRUG_STORE_PATH",
                h.dir.path().join("store.db").to_string_lossy().into_owned(),
            ),
            ("REALORRUG_X_CLIENT_ID", "test-client".to_owned()),
            ("REALORRUG_X_REDIRECT_URI", "https://api.test/cb".to_owned()),
            ("REALORRUG_SERVE_MONTHLY_USD", "0.01".to_owned()),
        ]);
        let now = Arc::clone(&h.now);
        let restarted = Harness {
            dir: tempfile::tempdir().unwrap(),
            fake: Arc::clone(&h.fake),
            now: Arc::clone(&h.now),
            state: Arc::new(AuthState::from_vars(
                &|k| vars.get(k).cloned(),
                Some(h.fake.clone() as Arc<dyn XClient>),
                Arc::new(move || now.load(Ordering::SeqCst)),
            )),
        };
        let (state, cookie, _) = restarted.begin().await;
        let (status, _, _) = restarted
            .get(
                &format!("/auth/x/callback?code=abc&state={state}"),
                Some(&cookie),
            )
            .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(h.fake.calls(), 2, "the restarted server made no new call");
    }

    #[tokio::test]
    async fn the_access_token_is_not_kept() {
        let h = Harness::open();
        let session = h.sign_in().await;
        assert!(h.fake.calls() >= 2, "the fake was really called");

        // Not in the store's bytes, not in the spend ledger, not in what the
        // session cookie or the CSRF token are made of.
        assert!(!contains(&h.store_bytes(), FAKE_TOKEN));
        let ledger = std::fs::read(h.dir.path().join("store.db.spend.json")).unwrap();
        assert!(!ledger.is_empty());
        assert!(!contains(&ledger, FAKE_TOKEN));
        assert!(!session.contains(FAKE_TOKEN));
        let (_, _, me) = h
            .get("/auth/me", Some(&format!("{SESSION_COOKIE}={session}")))
            .await;
        assert!(!me.contains(FAKE_TOKEN), "{me}");
        // And its Debug says nothing.
        assert_eq!(
            format!("{:?}", AccessToken(FAKE_TOKEN.into())),
            "AccessToken(<redacted>)"
        );
    }

    #[tokio::test]
    async fn the_pkce_challenge_is_the_hash_of_the_verifier_x_later_receives() {
        let h = Harness::open();
        let (state, cookie, location) = h.begin().await;
        assert!(location.starts_with(AUTHORIZE_URL));
        assert_eq!(query_param(&location, "code_challenge_method"), "S256");
        assert_eq!(query_param(&location, "client_id"), "test-client");
        assert_eq!(
            query_param(&location, "redirect_uri"),
            "https%3A%2F%2Fapi.test%2Fauth%2Fx%2Fcallback"
        );
        h.get(
            &format!("/auth/x/callback?code=abc&state={state}"),
            Some(&cookie),
        )
        .await;
        let verifier = h.fake.verifiers.lock().unwrap()[0].clone();
        let expected = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(query_param(&location, "code_challenge"), expected);
    }

    #[tokio::test]
    async fn the_session_cookie_is_http_only_secure_and_only_its_hash_is_stored() {
        let h = Harness::open();
        let (state, cookie, _) = h.begin().await;
        assert!(headers_have(
            &cookie_attrs(&h, "/auth/x/start").await,
            &["HttpOnly", "Secure", "SameSite=Lax"]
        ));

        let (_, headers, _) = h
            .get(
                &format!("/auth/x/callback?code=abc&state={state}"),
                Some(&cookie),
            )
            .await;
        let set: Vec<&str> = headers
            .get_all(SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        let session = set
            .iter()
            .find(|c| c.starts_with(&format!("{SESSION_COOKIE}=")))
            .expect("a session cookie");
        for attr in [
            "HttpOnly",
            "Secure",
            "SameSite=Lax",
            "Path=/",
            "Max-Age=2592000",
        ] {
            assert!(session.contains(attr), "{attr} missing from {session}");
        }
        let token = session_token_in(&headers);
        assert_eq!(token.len(), 64, "32 random bytes, hex");

        // The store holds the hash, and never the token.
        let key = h.player_of("1001").expect("an identity");
        let identity = h
            .state
            .live
            .as_ref()
            .unwrap()
            .store
            .lock()
            .unwrap()
            .identity(&key)
            .unwrap()
            .unwrap();
        assert_eq!(identity.session_hash, Some(sha256_hex(token.as_bytes())));
        assert!(!contains(&h.store_bytes(), &token));
        assert!(contains(&h.store_bytes(), &sha256_hex(token.as_bytes())));
    }

    async fn cookie_attrs(h: &Harness, path: &str) -> String {
        let (_, headers, _) = h.get(path, None).await;
        headers[SET_COOKIE].to_str().unwrap().to_owned()
    }

    fn headers_have(cookie: &str, attrs: &[&str]) -> bool {
        attrs.iter().all(|a| cookie.contains(a))
    }

    #[tokio::test]
    async fn an_expired_session_is_refused() {
        let h = Harness::open();
        let session = h.sign_in().await;
        let cookie = format!("{SESSION_COOKIE}={session}");
        assert_eq!(h.get("/auth/me", Some(&cookie)).await.0, StatusCode::OK);

        // The last second it is good for, then the second it is not.
        h.advance(SESSION_TTL_SECS - 1);
        assert_eq!(h.get("/auth/me", Some(&cookie)).await.0, StatusCode::OK);
        h.advance(1);
        assert_eq!(
            h.get("/auth/me", Some(&cookie)).await.0,
            StatusCode::UNAUTHORIZED
        );
        // Refused for a POST too, CSRF or not.
        assert_eq!(
            h.post("/auth/logout", Some(&session), Some(&csrf_for(&session)))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn a_post_without_the_csrf_token_is_refused() {
        let h = Harness::open();
        let session = h.sign_in().await;
        for path in ["/auth/logout", "/account/delete"] {
            for csrf in [
                None,
                Some("wrong"),
                Some(csrf_for("another-session").as_str()),
            ] {
                let (status, _, _) = h.post(path, Some(&session), csrf).await;
                assert_eq!(status, StatusCode::FORBIDDEN, "{path} {csrf:?}");
            }
        }
        // Nothing happened: the session still works and the identity stands.
        let cookie = format!("{SESSION_COOKIE}={session}");
        assert_eq!(h.get("/auth/me", Some(&cookie)).await.0, StatusCode::OK);
        assert!(h.player_of("1001").is_some());

        // The token /auth/me hands out is the one that works.
        let (_, _, body) = h.get("/auth/me", Some(&cookie)).await;
        let doc: Value = serde_json::from_str(&body).unwrap();
        let csrf = doc["csrf_token"].as_str().unwrap();
        assert_eq!(
            h.post("/auth/logout", Some(&session), Some(csrf)).await.0,
            StatusCode::OK
        );
        // And logging out ended the session.
        assert_eq!(
            h.get("/auth/me", Some(&cookie)).await.0,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn deleting_an_account_removes_the_identity_and_leaves_a_verifying_chain() {
        let h = Harness::open();
        let session = h.sign_in().await;
        let key = h.player_of("1001").expect("an identity");
        {
            let live = h.state.live.as_ref().unwrap();
            let store = live.store.lock().unwrap();
            store
                .submit_forecast(
                    "r1",
                    "solana",
                    "MintA",
                    &key,
                    realorrug_contest::calls::Side::Rug,
                    realorrug_contest::calls::Odds::new(5_000).unwrap(),
                    T0,
                    T0 + 3_600,
                )
                .unwrap();
            assert_eq!(chain_rows(&store), 1);
        }

        let (status, headers, _) = h
            .post("/account/delete", Some(&session), Some(&csrf_for(&session)))
            .await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            headers[SET_COOKIE].to_str().unwrap().contains("Max-Age=0"),
            "the cookie is cleared"
        );

        let live = h.state.live.as_ref().unwrap();
        {
            let store = live.store.lock().unwrap();
            assert!(store.identity(&key).unwrap().is_none());
            assert!(store.player_for_x_id("1001").unwrap().is_none());
            assert_eq!(chain_rows(&store), 1, "chain rows stay");
        }
        // The old session is refused, and so is a POST with it.
        let cookie = format!("{SESSION_COOKIE}={session}");
        assert_eq!(
            h.get("/auth/me", Some(&cookie)).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            h.post("/account/delete", Some(&session), Some(&csrf_for(&session)))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn the_eleventh_start_in_an_hour_is_refused() {
        let h = Harness::open();
        for n in 1..=10 {
            let (status, _, _) = h.get("/auth/x/start", None).await;
            assert_eq!(status, StatusCode::FOUND, "start {n}");
        }
        let (status, _, body) = h.get("/auth/x/start", None).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{body}");

        // Another address is not held back by this one.
        let mut req = HttpRequest::get("/auth/x/start")
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(ConnectInfo(
            "203.0.113.9:4000".parse::<std::net::SocketAddr>().unwrap(),
        ));
        assert_eq!(h.send(req).await.0, StatusCode::FOUND);

        // And the window is an hour, not forever.
        h.advance(START_WINDOW_SECS);
        assert_eq!(h.get("/auth/x/start", None).await.0, StatusCode::FOUND);
        assert_eq!(h.fake.calls(), 0, "starting a sign-in never calls X");
    }

    #[tokio::test]
    async fn an_unconfigured_server_answers_503_on_every_route() {
        let state = Arc::new(AuthState::from_vars(&|_| None, None, Arc::new(|| T0)));
        for (method, path) in [
            ("GET", "/auth/x/start"),
            ("GET", "/auth/x/callback?code=a&state=b"),
            ("GET", "/auth/me"),
            ("POST", "/auth/logout"),
            ("POST", "/account/delete"),
        ] {
            let response = router(Arc::clone(&state))
                .oneshot(
                    HttpRequest::builder()
                        .method(method)
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE, "{path}");
        }
    }

    #[tokio::test]
    async fn a_returning_x_account_keeps_its_key_and_a_new_sign_in_replaces_the_old_session() {
        let h = Harness::open();
        let first = h.sign_in().await;
        let key = h.player_of("1001").unwrap();
        let second = h.sign_in().await;
        assert_eq!(h.player_of("1001").unwrap(), key, "one X id, one key");
        assert_ne!(first, second);
        assert_eq!(
            h.get("/auth/me", Some(&format!("{SESSION_COOKIE}={first}")))
                .await
                .0,
            StatusCode::UNAUTHORIZED,
            "the identity holds one session"
        );
        assert_eq!(
            h.get("/auth/me", Some(&format!("{SESSION_COOKIE}={second}")))
                .await
                .0,
            StatusCode::OK
        );
    }

    #[test]
    fn losing_the_sign_in_race_uses_the_winners_key_instead_of_failing() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("s.db")).unwrap();
        let user = XUser {
            id: "77".into(),
            handle: "h".into(),
            created_at: None,
        };
        // The winner got there first.
        sign_in(&store, &user, "hash-a", 1, &|s, x| s.player_for_x_id(x)).unwrap();
        let winner = store.player_for_x_id("77").unwrap().unwrap();

        // The loser's lookup ran before the winner's write, so it sees no key
        // the first time: it mints one, `upsert` says XIdTaken, and the retry
        // must look again and land on the winner's key.
        let looks = std::cell::Cell::new(0);
        sign_in(&store, &user, "hash-b", 2, &|s, x| {
            looks.set(looks.get() + 1);
            if looks.get() == 1 {
                Ok(None)
            } else {
                s.player_for_x_id(x)
            }
        })
        .unwrap();
        assert_eq!(looks.get(), 2);
        assert_eq!(store.player_for_x_id("77").unwrap().unwrap(), winner);
        assert_eq!(
            store.player_for_session_hash("hash-b").unwrap().unwrap().0,
            winner
        );
    }

    #[test]
    fn a_key_offered_with_another_x_account_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("s.db")).unwrap();
        let owner = XUser {
            id: "1".into(),
            handle: "a".into(),
            created_at: None,
        };
        sign_in(&store, &owner, "h1", 1, &|s, x| s.player_for_x_id(x)).unwrap();
        let key = store.player_for_x_id("1").unwrap().unwrap();
        let other = XUser {
            id: "2".into(),
            handle: "b".into(),
            created_at: None,
        };
        // A lookup that hands account 2 account 1's key.
        let refused = sign_in(&store, &other, "h2", 2, &|_, _| Ok(Some(key.clone())));
        assert!(matches!(refused, Err(StoreError::KeyTaken)), "{refused:?}");
        assert!(store.player_for_x_id("2").unwrap().is_none());
    }

    #[test]
    fn created_at_is_read_as_utc_seconds() {
        assert_eq!(parse_utc("2019-08-24T14:15:22.000Z"), Some(1_566_656_122));
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_utc("2000-03-01T00:00:00Z"), Some(951_868_800));
        assert_eq!(parse_utc("2019-13-24T14:15:22Z"), None);
        assert_eq!(parse_utc("not a date"), None);
        assert_eq!(parse_utc(""), None);
    }

    #[test]
    fn only_exact_origins_are_allowed() {
        let get =
            |v: &'static str| move |k: &str| (k == "REALORRUG_APP_ORIGINS").then(|| v.to_owned());
        assert_eq!(
            app_origins_from(&get(
                "https://a.test, https://b.test/ ,,http://localhost:3000"
            )),
            ["https://a.test", "https://b.test", "http://localhost:3000"]
        );
        for bad in [
            "*",
            "https://*.test",
            "a.test",
            "ftp://a.test",
            "https://a.test/path",
            "https://",
        ] {
            assert!(app_origins_from(&get(bad)).is_empty(), "{bad}");
        }
        assert!(app_origins_from(&|_| None).is_empty());
    }

    // ---- Boundaries the mutation run found unpinned (CI, PR #207). --------

    #[test]
    fn a_utc_time_is_read_only_within_its_calendar_and_clock_bounds() {
        // Exactly the shortest accepted form, and one character less.
        assert_eq!(parse_utc("1970-01-01T00:00:00"), Some(0));
        assert_eq!(parse_utc("1970-01-01T00:00:0"), None);
        // Each separator on its own.
        assert_eq!(parse_utc("1970/01-01T00:00:00Z"), None);
        assert_eq!(parse_utc("1970-01/01T00:00:00Z"), None);
        assert_eq!(parse_utc("1970-01-01 00:00:00Z"), None);
        // Each clock field at its last good value and its first bad one.
        assert_eq!(parse_utc("1970-01-01T23:00:00Z"), Some(82_800));
        assert_eq!(parse_utc("1970-01-01T24:00:00Z"), None);
        assert_eq!(parse_utc("1970-01-01T00:59:00Z"), Some(3_540));
        assert_eq!(parse_utc("1970-01-01T00:60:00Z"), None);
        assert_eq!(parse_utc("1970-01-01T00:00:60Z"), Some(60));
        assert_eq!(parse_utc("1970-01-01T00:00:61Z"), None);
        // And the calendar fields.
        assert_eq!(parse_utc("1970-00-01T00:00:00Z"), None);
        assert_eq!(parse_utc("1970-01-00T00:00:00Z"), None);
        assert_eq!(parse_utc("1970-01-32T00:00:00Z"), None);
    }

    #[test]
    fn an_x_error_is_labelled_without_its_headers_or_body() {
        assert_eq!(short(&ureq::Error::StatusCode(429)), "X answered 429");
        assert_eq!(
            short(&ureq::Error::Timeout(ureq::Timeout::Global)),
            "timed out"
        );
        assert_eq!(short(&ureq::Error::HostNotFound), "could not reach X");
    }

    #[test]
    fn the_wall_clock_is_the_present_and_a_day_is_86400_seconds() {
        assert!(now_secs() > 1_700_000_000, "the clock reads {}", now_secs());
        assert_eq!(day_of(0), 0);
        assert_eq!(day_of(86_399), 0);
        assert_eq!(day_of(86_400), 1);
        assert_eq!(day_of(T0), 20_727);
        assert_eq!(day_of(-1), 0);
    }

    #[test]
    fn the_monthly_allowance_is_open_only_for_a_positive_finite_figure() {
        let read = |v: &str| serve_monthly_allowance_from(&|_| Some(v.to_owned()));
        assert_eq!(
            read("5").map(realorrug_types::MicroUsd::get),
            Some(5_000_000)
        );
        assert_eq!(
            read(" 0.5 ").map(realorrug_types::MicroUsd::get),
            Some(500_000)
        );
        for closed in ["0", "-1", "inf", "NaN", "abc", "", "0.0000001"] {
            assert!(read(closed).is_none(), "{closed:?} opened the allowance");
        }
        assert!(serve_monthly_allowance_from(&|_| None).is_none());
    }

    #[test]
    fn only_our_own_hex_tokens_pass_and_a_swapped_pair_is_not_equal() {
        let good = "0123456789abcdef".repeat(4);
        assert!(is_token(&good));
        assert!(!is_token(&good[..63]));
        assert!(!is_token(&"g".repeat(64)));
        assert!(!is_token(&good.to_uppercase()));
        assert!(!is_token(&format!("{good}0")));
        // Same length, same bytes swapped: an XOR fold would cancel to zero.
        assert!(ct_eq("ab", "ab"));
        assert!(!ct_eq("ab", "ba"));
        assert!(!ct_eq("ab", "abc"));
    }

    #[test]
    fn cloudflare_is_trusted_only_when_the_variable_is_exactly_one() {
        let with = |v: Option<&str>| {
            let dir = tempfile::tempdir().unwrap();
            let vars: HashMap<&str, String> = HashMap::from([
                (
                    "REALORRUG_STORE_PATH",
                    dir.path().join("s.db").to_string_lossy().into_owned(),
                ),
                ("REALORRUG_X_CLIENT_ID", "c".to_owned()),
                ("REALORRUG_X_REDIRECT_URI", "https://api.test/cb".to_owned()),
                ("REALORRUG_SERVE_MONTHLY_USD", "5".to_owned()),
                ("REALORRUG_TRUST_CLOUDFLARE", v.unwrap_or("").to_owned()),
            ]);
            let state = AuthState::from_vars(
                &|k| vars.get(k).cloned(),
                Some(FakeX::new() as Arc<dyn XClient>),
                Arc::new(|| T0),
            );
            state.live().unwrap().trust_cloudflare
        };
        assert!(with(Some("1")));
        assert!(!with(Some("0")));
        assert!(!with(Some("true")));
        assert!(!with(None));
    }

    #[test]
    fn a_busy_store_is_a_503_and_any_other_store_failure_is_a_500() {
        let h = Harness::open();
        let busy = h
            .state
            .with_store(|_| -> Result<(), StoreError> {
                Err(StoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows))
            })
            .unwrap_err();
        assert_eq!(busy.status(), StatusCode::SERVICE_UNAVAILABLE);
        let other = h
            .state
            .with_store(|_| -> Result<(), StoreError> { Err(StoreError::Duplicate) })
            .unwrap_err();
        assert_eq!(other.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn a_callback_with_an_error_or_an_empty_code_never_reaches_x() {
        let h = Harness::open();
        let (state, cookie, _) = h.begin().await;
        for query in [
            format!("code=abc&error=access_denied&state={state}"),
            format!("code=&state={state}"),
        ] {
            let (status, _, _) = h
                .get(&format!("/auth/x/callback?{query}"), Some(&cookie))
                .await;
            assert_ne!(status, StatusCode::FOUND, "{query}");
        }
        assert_eq!(h.fake.calls(), 0, "a refused callback must not reach X");
        assert!(h.player_of("1001").is_none());
    }

    #[test]
    fn a_sign_in_that_keeps_losing_the_race_stops_after_three_retries() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("s.db")).unwrap();
        let user = XUser {
            id: "5".into(),
            handle: "h".into(),
            created_at: None,
        };
        sign_in(&store, &user, "a", 1, &|s, x| s.player_for_x_id(x)).unwrap();
        // A lookup that never sees the winner, so every write says XIdTaken.
        let looks = std::cell::Cell::new(0);
        let result = sign_in(&store, &user, "b", 2, &|_, _| {
            looks.set(looks.get() + 1);
            Ok(None)
        });
        assert!(matches!(result, Err(StoreError::XIdTaken)), "{result:?}");
        assert_eq!(looks.get(), 4, "one try and three retries, no more");
    }

    #[test]
    fn the_start_table_is_capped_and_forgets_only_what_is_an_hour_old() {
        let h = Harness::open();
        let live = h.state.live().unwrap();
        let now = T0;
        let fill = |stamp: i64| {
            let mut starts = live.starts.lock().unwrap();
            starts.clear();
            for i in 0..LIMITER_CAP {
                starts.insert(format!("ip{i}"), vec![stamp]);
            }
        };
        let len = || live.starts.lock().unwrap().len();

        // Full of attempts still inside the hour: a stranger is turned away,
        // an address already in the table is still counted, not shut out.
        fill(now - START_WINDOW_SECS + 1);
        assert!(!live.allow_start("stranger", now));
        assert!(live.allow_start("ip0", now));
        assert_eq!(len(), LIMITER_CAP, "a known address never purges the table");
        // One short of full leaves room.
        live.starts.lock().unwrap().remove("ip1");
        assert!(live.allow_start("stranger", now));

        // Full of attempts exactly an hour old: all forgotten, then counted.
        fill(now - START_WINDOW_SECS);
        assert!(live.allow_start("stranger", now));
        assert_eq!(len(), 1);

        // Below the cap nothing is purged, old or not.
        {
            let mut starts = live.starts.lock().unwrap();
            starts.clear();
            starts.insert("old".into(), vec![now - START_WINDOW_SECS]);
        }
        assert!(live.allow_start("stranger", now));
        assert!(live.starts.lock().unwrap().contains_key("old"));
    }

    #[test]
    fn a_started_sign_in_is_capped_in_flight_and_dies_after_its_ttl() {
        let h = Harness::open();
        let live = h.state.live().unwrap();
        let now = T0;
        let fill = |expires: i64| {
            let mut pending = live.pending.lock().unwrap();
            pending.clear();
            for i in 0..PENDING_CAP {
                pending.insert(
                    format!("s{i}"),
                    Pending {
                        verifier: "v".into(),
                        expires,
                    },
                );
            }
        };
        let len = || live.pending.lock().unwrap().len();

        fill(now + 1);
        assert!(!live.remember("new", "v", now), "a full table took another");
        live.pending.lock().unwrap().remove("s0");
        assert!(live.remember("new", "v", now), "one short of full has room");

        // Full of states that have just expired: purged, then stored.
        fill(now);
        assert!(live.remember("new", "v", now));
        assert_eq!(len(), 1);

        // A state lives for exactly OAUTH_TTL_SECS and works once.
        live.pending.lock().unwrap().clear();
        assert!(live.remember("a", "ver", now));
        assert_eq!(
            live.take("a", now + OAUTH_TTL_SECS - 1).as_deref(),
            Some("ver")
        );
        assert!(live.take("a", now).is_none(), "a callback works once");
        assert!(live.remember("b", "ver", now));
        assert!(live.take("b", now + OAUTH_TTL_SECS).is_none());
    }
}
