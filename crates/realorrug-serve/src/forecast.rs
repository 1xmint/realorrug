// SPDX-License-Identifier: Apache-2.0
//! The forecast routes (design 0032 §9, §11; ADR 0041): a signed-in player
//! calls a coin real or rug before the round closes, reads their own hidden
//! call back, and anyone reads what the round settled to after it closes.
//!
//! # What is served, and what is not
//!
//! - `POST /forecast` -- session and CSRF. The body names `{round, chain,
//!   token, side}` and nothing else: **the close and the odds come from the
//!   rounds file, never the client**, and the player is the session's, never a
//!   field.
//! - `GET /forecast/mine?round=` -- the caller's own calls, hidden or not.
//! - `GET /v1/rounds/{round}` -- the round's close and coins, and whether it is
//!   closed. No count of any kind.
//! - `GET /v1/rounds/{round}/forecasts` -- only after close, and with no
//!   player in it.
//! - `GET /v1/rounds/{round}/outcomes` -- only after close, in
//!   [`Store::public_wording`]'s sentences and nothing else: `Outcome` never
//!   reaches a response.
//! - `GET /v1/board` -- hit and miss counts with `n`, per opaque board id,
//!   through `calls::hit_miss` alone.
//! - `GET /v1/privacy` -- the notice, always available.
//!
//! No route serialises `winner()`, `score_calls`, a points figure or a player's
//! total: a count carries no value (AGENTS.md rule 1; design 0032 §9). No route
//! prints the chain's row count or head -- a count before close tells a reader
//! how many calls are hidden.
//!
//! Every public read carries `read_at` (the server's clock) and `newest_at`
//! (the newest row it shows), so a reader can see how old what they are
//! looking at is (ADR 0039 decision 6).
//!
//! # Where the close comes from
//!
//! One JSON file, `REALORRUG_ROUNDS_FILE`, read on every request so the
//! operator can open a round without a restart. It holds one close per round
//! and the coins allowed in it. Absent, unparseable, or naming a round twice
//! means every route that needs it refuses: an unread close is not a far-off
//! one (AGENTS.md rules 7 and 8).

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header::CACHE_CONTROL;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use realorrug_contest::calls::{self, Odds, SettledCall, Side};
use realorrug_store::{Store, StoreError};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::{AuthState, no_store, refuse, sha256_hex};

/// The largest request body read. A forecast is four short strings.
const MAX_BODY: usize = 1_024;
/// The odds recorded for a coin the rounds file gives none for: even. It is a
/// stated neutral, not a measurement (design 0032 §11).
const NEUTRAL_Q: u16 = 5_000;
/// The longest identifier (round, chain, token) accepted from a client.
const MAX_ID: usize = 128;

/// One coin a round allows.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Coin {
    chain: String,
    token: String,
    /// The bot's odds when the coin was listed, fixed then (design 0028 §3).
    q_basis_points: Option<u16>,
}

/// One round, with its single close.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Round {
    id: String,
    /// Seconds since the epoch. Entry shuts at this instant and the reveal
    /// begins at it (`submit_forecast` and `forecast` share the `>=`).
    close: i64,
    coins: Vec<Coin>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RoundsFile {
    rounds: Vec<Round>,
}

/// Reads the rounds file. `None` for a file that is absent, unreadable, not
/// this shape, or names a round twice: a round with two closes has no close.
fn load_rounds(state: &AuthState) -> Option<Vec<Round>> {
    let text = std::fs::read_to_string(state.rounds_path.as_ref()?).ok()?;
    parse_rounds(&text)
}

fn parse_rounds(text: &str) -> Option<Vec<Round>> {
    let file: RoundsFile = serde_json::from_str(text).ok()?;
    let mut seen = std::collections::HashSet::new();
    for round in &file.rounds {
        if round.id.is_empty() || round.id.len() > MAX_ID || !seen.insert(round.id.as_str()) {
            return None;
        }
    }
    Some(file.rounds)
}

fn no_rounds() -> Response {
    refuse(
        StatusCode::SERVICE_UNAVAILABLE,
        "no round is configured on this server",
    )
}

/// Finds a round or answers why not.
#[allow(
    clippy::result_large_err,
    reason = "a ready axum Response is the error, as in auth.rs"
)]
fn find_round(state: &AuthState, id: &str) -> Result<Round, Response> {
    let rounds = load_rounds(state).ok_or_else(no_rounds)?;
    rounds
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| refuse(StatusCode::NOT_FOUND, "there is no such round"))
}

/// An opaque id for the board and for the caller's own record: a salted hash
/// of the player key, so the raw key never leaves the store. It is stable, so a
/// player finds their own line, and it sits beside no handle.
fn board_id(key: &str) -> String {
    let mut hash = sha256_hex(format!("realorrug-board-v1:{key}").as_bytes());
    hash.truncate(16);
    hash
}

/// A public reply: cacheable for a moment, since it holds nothing of a
/// session's.
fn public(body: Value) -> Response {
    let mut response = Json(body).into_response();
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=30"),
    );
    response
}

/// The routes.
pub(crate) fn router(state: Arc<AuthState>) -> Router {
    Router::new()
        .route("/forecast", post(submit))
        .route("/forecast/mine", get(mine))
        .route("/v1/rounds/{round}", get(round_info))
        .route("/v1/rounds/{round}/forecasts", get(round_forecasts))
        .route("/v1/rounds/{round}/outcomes", get(round_outcomes))
        .route("/v1/board", get(board))
        .route("/v1/privacy", get(privacy))
        .with_state(state)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Submission {
    round: String,
    chain: String,
    token: String,
    side: String,
}

fn parse_side(text: &str) -> Option<Side> {
    match text {
        "rug" => Some(Side::Rug),
        "real" => Some(Side::Real),
        _ => None,
    }
}

fn side_word(side: Side) -> &'static str {
    match side {
        Side::Rug => "rug",
        Side::Real => "real",
    }
}

/// `POST /forecast`.
async fn submit(State(auth): State<Arc<AuthState>>, headers: HeaderMap, body: Bytes) -> Response {
    // The session first: a stranger's body is not parsed.
    let session = match auth.authenticate(&headers, true) {
        Ok(session) => session,
        Err(refusal) => return refusal,
    };
    if body.len() > MAX_BODY {
        return refuse(StatusCode::PAYLOAD_TOO_LARGE, "the request is too large");
    }
    let Ok(sent) = serde_json::from_slice::<Submission>(&body) else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "the body must be {round, chain, token, side}",
        );
    };
    let Some(side) = parse_side(&sent.side) else {
        return refuse(StatusCode::BAD_REQUEST, "side is \"rug\" or \"real\"");
    };
    let round = match find_round(&auth, &sent.round) {
        Ok(round) => round,
        Err(refusal) => return refusal,
    };
    // A coin the round does not list is refused whatever the client says its
    // odds or close are: both come from this record.
    let Some(coin) = round
        .coins
        .iter()
        .find(|c| c.chain == sent.chain && c.token == sent.token)
    else {
        return refuse(StatusCode::BAD_REQUEST, "that coin is not in this round");
    };
    let Ok(q) = Odds::new(coin.q_basis_points.unwrap_or(NEUTRAL_Q)) else {
        return refuse(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the round's odds are out of range",
        );
    };
    // The server's clock, once, for both the refusal and the row.
    let now = (auth.clock)();
    let result = auth.with_store(|store| {
        // `WindowClosed` and `Duplicate` are answers, not faults; anything else
        // is passed up to `with_store`, which says nothing about the store.
        match store.submit_forecast(
            &round.id,
            &sent.chain,
            &sent.token,
            &session.player,
            side,
            q,
            now,
            round.close,
        ) {
            Ok(()) => Ok(Ok(())),
            Err(e @ (StoreError::WindowClosed { .. } | StoreError::Duplicate)) => Ok(Err(e)),
            Err(e) => Err(e),
        }
    });
    match result {
        Err(refusal) => refusal,
        Ok(Ok(())) => {
            let mut response = (
                StatusCode::CREATED,
                Json(json!({
                    "saved": true,
                    "round": round.id,
                    "chain": sent.chain,
                    "token": sent.token,
                    "side": side_word(side),
                    "q_basis_points": q.basis_points(),
                    "submitted_at": now,
                    "window_close": round.close,
                })),
            )
                .into_response();
            no_store(&mut response);
            response
        }
        Ok(Err(StoreError::Duplicate)) => refuse(
            StatusCode::CONFLICT,
            "you already called this coin in this round; the first call stands",
        ),
        Ok(Err(_)) => refuse(
            StatusCode::FORBIDDEN,
            "this round's window has closed; the server's clock decides",
        ),
    }
}

#[derive(Deserialize)]
struct RoundQuery {
    round: Option<String>,
}

/// `GET /forecast/mine?round=`: the caller's own calls.
///
/// The requester is the session's key and nothing in the URL: `Store::forecast`
/// takes a target and a requester, and both are the same player here, so there
/// is no parameter that names somebody else.
async fn mine(
    State(auth): State<Arc<AuthState>>,
    headers: HeaderMap,
    Query(query): Query<RoundQuery>,
) -> Response {
    let session = match auth.authenticate(&headers, false) {
        Ok(session) => session,
        Err(refusal) => return refusal,
    };
    let Some(id) = query.round else {
        return refuse(StatusCode::BAD_REQUEST, "name a round: ?round=");
    };
    let round = match find_round(&auth, &id) {
        Ok(round) => round,
        Err(refusal) => return refusal,
    };
    let now = (auth.clock)();
    let me = session.player.as_str();
    let calls = auth.with_store(|store| {
        let mut out = Vec::new();
        for coin in &round.coins {
            if let Some(view) = store.forecast(&round.id, &coin.chain, &coin.token, me, me, now)? {
                out.push(json!({
                    "chain": coin.chain,
                    "token": coin.token,
                    "side": side_word(view.side),
                    "q_basis_points": view.q_basis_points,
                    "submitted_at": view.submitted_at,
                    "window_close": view.window_close,
                }));
            }
        }
        Ok(out)
    });
    let calls = match calls {
        Ok(calls) => calls,
        Err(refusal) => return refusal,
    };
    let mut response = Json(json!({
        "round": round.id,
        "board_id": board_id(me),
        "forecasts": calls,
        "read_at": now,
    }))
    .into_response();
    no_store(&mut response);
    response
}

fn closed(round: &Round, now: i64) -> bool {
    now >= round.close
}

/// `GET /v1/rounds/{round}`: the close and the coins. Never a count.
async fn round_info(State(auth): State<Arc<AuthState>>, Path(id): Path<String>) -> Response {
    let round = match find_round(&auth, &id) {
        Ok(round) => round,
        Err(refusal) => return refusal,
    };
    let now = (auth.clock)();
    let coins: Vec<Value> = round
        .coins
        .iter()
        .map(|c| json!({ "chain": c.chain, "token": c.token }))
        .collect();
    public(json!({
        "round": round.id,
        "window_close": round.close,
        "closed": closed(&round, now),
        "coins": coins,
        "read_at": now,
        "newest_at": null,
    }))
}

/// What a read of a round says before its close: that it is open, and nothing
/// that depends on how many calls are in it.
fn still_open(round: &Round, now: i64) -> Response {
    public(json!({
        "round": round.id,
        "closed": false,
        "window_close": round.close,
        "note": "calls are shown after the window closes",
        "read_at": now,
        "newest_at": null,
    }))
}

/// `GET /v1/rounds/{round}/forecasts`.
async fn round_forecasts(State(auth): State<Arc<AuthState>>, Path(id): Path<String>) -> Response {
    let round = match find_round(&auth, &id) {
        Ok(round) => round,
        Err(refusal) => return refusal,
    };
    let now = (auth.clock)();
    if !closed(&round, now) {
        return still_open(&round, now);
    }
    let shown = match auth.with_store(|store| store.closed_forecasts(&round.id, now)) {
        Ok(shown) => shown,
        Err(refusal) => return refusal,
    };
    let newest = shown.iter().map(|f| f.submitted_at).max();
    let forecasts: Vec<Value> = shown
        .iter()
        .map(|f| {
            json!({
                "chain": f.chain,
                "token": f.token,
                "side": side_word(f.side),
                "q_basis_points": f.q_basis_points,
                "submitted_at": f.submitted_at,
            })
        })
        .collect();
    public(json!({
        "round": round.id,
        "closed": true,
        "window_close": round.close,
        "forecasts": forecasts,
        "read_at": now,
        "newest_at": newest,
    }))
}

/// `GET /v1/rounds/{round}/outcomes`.
async fn round_outcomes(State(auth): State<Arc<AuthState>>, Path(id): Path<String>) -> Response {
    let round = match find_round(&auth, &id) {
        Ok(round) => round,
        Err(refusal) => return refusal,
    };
    let now = (auth.clock)();
    if !closed(&round, now) {
        return still_open(&round, now);
    }
    let settled = auth.with_store(|store| {
        let mut out = Vec::new();
        for coin in &round.coins {
            if let Some(view) = store.outcome(&round.id, &coin.chain, &coin.token)? {
                out.push((coin.clone(), view));
            }
        }
        Ok(out)
    });
    let settled = match settled {
        Ok(settled) => settled,
        Err(refusal) => return refusal,
    };
    let newest = settled.iter().map(|(_, v)| v.settled_at).max();
    let outcomes: Vec<Value> = settled
        .iter()
        .map(|(coin, view)| {
            json!({
                "chain": coin.chain,
                "token": coin.token,
                // The sentence, never the enum: `Outcome` derives `Serialize`
                // and would print `Rugged`.
                "reading": Store::public_wording(view.outcome),
                "rule_version": view.rule_version,
                "evidence_reference": view.evidence_reference,
                "settled_at": view.settled_at,
            })
        })
        .collect();
    public(json!({
        "round": round.id,
        "closed": true,
        "outcomes": outcomes,
        "read_at": now,
        "newest_at": newest,
    }))
}

/// `GET /v1/board`: hit and miss counts with the sample size, per opaque id.
///
/// Through `calls::hit_miss` alone, which leaves `Unresolved` out of `n`. The
/// lines are in board-id order, not ranked: a rank by a count is the first step
/// to a winner, and the design ranks by nothing until the odds replay lands
/// (design 0032 Q1).
async fn board(State(auth): State<Arc<AuthState>>) -> Response {
    let now = (auth.clock)();
    let settled = match auth.with_store(|store| store.settled_forecasts(now)) {
        Ok(settled) => settled,
        Err(refusal) => return refusal,
    };
    let newest = settled.iter().map(|s| s.settled_at).max();
    let mut per_player: std::collections::BTreeMap<String, Vec<SettledCall>> =
        std::collections::BTreeMap::new();
    for s in &settled {
        let Ok(q) = Odds::new(s.q_basis_points) else {
            continue;
        };
        per_player
            .entry(board_id(&s.player_key))
            .or_default()
            .push(SettledCall {
                player: String::new(),
                coin_id: s.token.clone(),
                creator_id: String::new(),
                side: s.side,
                q,
                outcome: s.outcome,
                called_at: u64::try_from(s.submitted_at).unwrap_or(0),
                account_age_days: None,
            });
    }
    let lines: Vec<Value> = per_player
        .iter()
        .filter_map(|(id, calls)| {
            let count = calls::hit_miss(calls);
            // A player whose every call is unresolved has no sample to show.
            (count.n > 0).then(|| {
                json!({
                    "board_id": id,
                    "hits": count.hits,
                    "misses": count.misses,
                    "n": count.n,
                })
            })
        })
        .collect();
    public(json!({
        "board": lines,
        "read_at": now,
        "newest_at": newest,
    }))
}

/// `GET /v1/privacy`: what is kept about a person who signs in, and how each
/// piece goes. It needs no session and no configuration.
async fn privacy() -> Response {
    public(json!({
        "kept": [
            {"field": "X handle", "why": "shown to you in the app; never in a forecast row", "ends": "deleted by account deletion"},
            {"field": "X account id", "why": "so a returning sign-in finds your player key", "ends": "deleted by account deletion"},
            {"field": "account creation date", "why": "read once at sign-in from X", "ends": "deleted by account deletion"},
            {"field": "session hash", "why": "a SHA-256 of your session token; the token itself is never stored", "ends": "replaced by your next sign-in, removed by sign-out or account deletion, and dead after 30 days"},
            {"field": "player key", "why": "a random key your forecasts are filed under, not derived from your X account", "ends": "deleted from the identity record with account deletion; the forecast rows that carry it stay, unlinked"}
        ],
        "not_kept": [
            "your X access token: it is used once to read your handle, id and account age, and thrown away",
            "any wallet address: sign-in is X only and nothing here asks for one",
            "your IP address, beyond a short in-memory count that rate-limits sign-in and resets when the server restarts"
        ],
        "sessions": "You have one session at a time. Signing in again replaces the old one, so a second browser is signed out.",
        "deletion": "POST /account/delete removes your identity record. Forecast and outcome rows are an append-only public record and are not rewritten; after deletion nothing in the store links them to your X account.",
        "public": "After a round closes its forecasts are public without any player named. The board shows hit and miss counts with the number of calls, under an opaque id, never beside your handle.",
    }))
}

#[cfg(test)]
mod tests;
