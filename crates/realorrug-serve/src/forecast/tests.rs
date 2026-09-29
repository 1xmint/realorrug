// SPDX-License-Identifier: Apache-2.0
//! The forecast routes, driven through the same router the server runs.
//!
//! Sessions are seeded straight into the store (an identity row holding the
//! session's hash), because sign-in has its own tests in `auth.rs`; what is
//! under test here is what a signed-in or anonymous caller can and cannot
//! read or write.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

use axum::body::Body;
use axum::http::header::{CONTENT_TYPE, COOKIE, ORIGIN};
use axum::http::{Method, Request};
use http_body_util::BodyExt;
use realorrug_contest::calls::Outcome;
use realorrug_store::{Identity, PlayerKey};
use tower::ServiceExt;

use super::*;
use crate::auth::{CSRF_HEADER, SESSION_COOKIE, csrf_for};

const T0: i64 = 1_800_000_000;
const CLOSE: i64 = T0 + 1_000;
const ROUNDS: &str = r#"{"rounds":[{"id":"r1","close":1800001000,"coins":[
    {"chain":"solana","token":"coinA","q_basis_points":6000},
    {"chain":"solana","token":"coinB","q_basis_points":4000}]}]}"#;
/// The public routes, all of them: what an anonymous reader can fetch.
const PUBLIC: [&str; 5] = [
    "/v1/rounds/r1",
    "/v1/rounds/r1/forecasts",
    "/v1/rounds/r1/outcomes",
    "/v1/board",
    "/v1/privacy",
];

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|_| panic!("not json: {}", self.body))
    }
}

struct H {
    dir: tempfile::TempDir,
    now: Arc<AtomicI64>,
    state: Arc<AuthState>,
}

fn token(n: u8) -> String {
    format!("{n:0>64x}")
}

impl H {
    fn new(origins: Option<&str>, rounds: Option<&str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let now = Arc::new(AtomicI64::new(T0));
        let clock: crate::auth::Clock = {
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
        ]);
        if let Some(o) = origins {
            vars.insert("REALORRUG_APP_ORIGINS", o.to_owned());
        }
        if let Some(text) = rounds {
            let path = dir.path().join("rounds.json");
            std::fs::write(&path, text).unwrap();
            vars.insert("REALORRUG_ROUNDS_FILE", path.to_string_lossy().into_owned());
        }
        let state = AuthState::from_vars(&|k| vars.get(k).cloned(), None, clock);
        Self {
            dir,
            now,
            state: Arc::new(state),
        }
    }

    fn open() -> Self {
        Self::new(None, Some(ROUNDS))
    }

    fn set_time(&self, t: i64) {
        self.now.store(t, Ordering::SeqCst);
    }

    /// Signs a player in without X: an identity row whose session hash is the
    /// hash of `token(n)`. Returns the raw token and the player's key.
    fn player(&self, n: u8) -> (String, PlayerKey) {
        let t = token(n);
        let key = self
            .state
            .with_store(|s| {
                let key = s.new_player_key()?;
                s.upsert_identity(&Identity {
                    player_key: key.clone(),
                    x_id: format!("x-{n}"),
                    handle: format!("handle{n}"),
                    account_created_at: Some(0),
                    session_hash: Some(sha256_hex(t.as_bytes())),
                    signed_in_at: T0,
                })?;
                Ok(key)
            })
            .unwrap_or_else(|_| panic!("seeding a player"));
        (t, key)
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        session: Option<&str>,
        csrf: bool,
        origin: Option<&str>,
        body: Option<&str>,
    ) -> Reply {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(t) = session {
            req = req.header(COOKIE, format!("{SESSION_COOKIE}={t}"));
            if csrf {
                req = req.header(CSRF_HEADER, csrf_for(t));
            }
        }
        if let Some(o) = origin {
            req = req.header(ORIGIN, o);
        }
        if body.is_some() {
            req = req.header(CONTENT_TYPE, "application/json");
        }
        let req = req
            .body(body.map_or_else(Body::empty, |b| Body::from(b.to_owned())))
            .unwrap();
        let response = crate::session_routes(Arc::clone(&self.state))
            .oneshot(req)
            .await
            .unwrap();
        let (parts, body) = response.into_parts();
        Reply {
            status: parts.status,
            headers: parts.headers,
            body: String::from_utf8_lossy(&body.collect().await.unwrap().to_bytes()).into_owned(),
        }
    }

    async fn get(&self, path: &str, session: Option<&str>) -> Reply {
        self.send(Method::GET, path, session, false, None, None)
            .await
    }

    async fn call(&self, session: &str, coin: &str, side: &str) -> Reply {
        let body = format!(r#"{{"round":"r1","chain":"solana","token":"{coin}","side":"{side}"}}"#);
        self.send(
            Method::POST,
            "/forecast",
            Some(session),
            true,
            None,
            Some(&body),
        )
        .await
    }

    fn settle(&self, coin: &str, outcome: Outcome, at: i64) {
        self.state
            .with_store(|s| s.record_outcome("r1", "solana", coin, outcome, "v1", None, at))
            .unwrap_or_else(|_| panic!("settling"));
    }

    /// Every public route's body, in order.
    async fn public_bodies(&self) -> Vec<String> {
        let mut out = Vec::new();
        for path in PUBLIC {
            out.push(self.get(path, None).await.body);
        }
        out
    }
}

#[tokio::test]
async fn a_late_forecast_is_refused_by_the_servers_clock_and_the_client_cannot_name_a_close() {
    let h = H::open();
    let (a, _) = h.player(1);
    // A client-supplied close is not a field.
    let with_close = format!(
        r#"{{"round":"r1","chain":"solana","token":"coinA","side":"rug","window_close":{}}}"#,
        CLOSE + 9_999
    );
    let r = h
        .send(
            Method::POST,
            "/forecast",
            Some(&a),
            true,
            None,
            Some(&with_close),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.body);
    // The instant of the close is already too late.
    h.set_time(CLOSE);
    let r = h.call(&a, "coinA", "rug").await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.body);
    h.set_time(CLOSE - 1);
    let r = h.call(&a, "coinA", "rug").await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
    // The stored close is the round's, whatever the client sent.
    assert_eq!(r.json()["window_close"], CLOSE);
}

#[tokio::test]
async fn a_repeat_is_a_409_and_the_first_call_stands() {
    let h = H::open();
    let (a, _) = h.player(1);
    assert_eq!(h.call(&a, "coinA", "rug").await.status, StatusCode::CREATED);
    let again = h.call(&a, "coinA", "real").await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.body);
    let mine = h.get("/forecast/mine?round=r1", Some(&a)).await.json();
    let list = mine["forecasts"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["side"], "rug", "the second call replaced the first");
    // Odds come from the rounds file: 6000 for coinA.
    assert_eq!(list[0]["q_basis_points"], 6_000);
}

#[tokio::test]
async fn a_coin_outside_the_round_is_refused_and_nothing_is_saved() {
    let h = H::open();
    let (a, _) = h.player(1);
    let r = h.call(&a, "notInRound", "rug").await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.body);
    let unknown = r#"{"round":"r9","chain":"solana","token":"coinA","side":"rug"}"#;
    let r = h
        .send(
            Method::POST,
            "/forecast",
            Some(&a),
            true,
            None,
            Some(unknown),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.body);
    let bad_side = h.call(&a, "coinA", "moon").await;
    assert_eq!(bad_side.status, StatusCode::BAD_REQUEST);
    let mine = h.get("/forecast/mine?round=r1", Some(&a)).await.json();
    assert!(mine["forecasts"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_forecast_needs_a_session_and_the_csrf_token() {
    let h = H::open();
    let (a, _) = h.player(1);
    let body = r#"{"round":"r1","chain":"solana","token":"coinA","side":"rug"}"#;
    let none = h
        .send(Method::POST, "/forecast", None, false, None, Some(body))
        .await;
    assert_eq!(none.status, StatusCode::UNAUTHORIZED);
    let no_csrf = h
        .send(Method::POST, "/forecast", Some(&a), false, None, Some(body))
        .await;
    assert_eq!(no_csrf.status, StatusCode::FORBIDDEN);
    let mine = h.get("/forecast/mine?round=r1", Some(&a)).await.json();
    assert!(mine["forecasts"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn a_hidden_call_changes_nothing_a_stranger_can_read_before_close() {
    let h = H::open();
    let (a, akey) = h.player(1);
    let (b, _) = h.player(2);
    let before = h.public_bodies().await;
    assert_eq!(h.call(&a, "coinA", "rug").await.status, StatusCode::CREATED);
    // Even an outcome recorded early must not surface a hidden call.
    h.settle("coinA", Outcome::Rugged, T0 + 10);
    let after = h.public_bodies().await;
    // Byte for byte the same: no count, no list, no board line, no row count.
    // The one route an outcome may move is the outcomes route, and that is
    // closed until the round is.
    assert_eq!(before, after, "a public route changed with a hidden call");
    // The author reads it; another player, asking for the author by every means
    // the URL offers, does not.
    let mine = h.get("/forecast/mine?round=r1", Some(&a)).await;
    assert_eq!(mine.json()["forecasts"].as_array().unwrap().len(), 1);
    let path = format!("/forecast/mine?round=r1&player={}", akey.as_str());
    let theirs = h.get(&path, Some(&b)).await;
    assert!(theirs.json()["forecasts"].as_array().unwrap().is_empty());
    assert!(!theirs.body.contains(akey.as_str()));
    assert_eq!(
        h.get("/forecast/mine?round=r1", None).await.status,
        StatusCode::UNAUTHORIZED
    );
    // At the close the call is public, and still names no one.
    h.set_time(CLOSE);
    let shown = h.get("/v1/rounds/r1/forecasts", None).await;
    let list = shown.json()["forecasts"].as_array().unwrap().clone();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["side"], "rug");
    assert!(!shown.body.contains(akey.as_str()));
    assert!(!shown.body.contains("handle1"));
    assert!(!shown.body.contains(&board_id(akey.as_str())));
}

#[tokio::test]
async fn unresolved_outcomes_are_absent_from_the_boards_sample_size() {
    let h = H::open();
    let (a, _) = h.player(1);
    assert_eq!(h.call(&a, "coinA", "rug").await.status, StatusCode::CREATED);
    assert_eq!(
        h.call(&a, "coinB", "real").await.status,
        StatusCode::CREATED
    );
    h.set_time(CLOSE + 100);
    h.settle("coinA", Outcome::Rugged, CLOSE + 50);
    h.settle("coinB", Outcome::Unresolved, CLOSE + 60);
    let board = h.get("/v1/board", None).await.json();
    let lines = board["board"].as_array().unwrap();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["hits"], 1);
    assert_eq!(lines[0]["misses"], 0);
    assert_eq!(lines[0]["n"], 1, "an unresolved call grew the sample size");
}

#[tokio::test]
async fn every_public_read_states_its_age_and_speaks_only_in_the_public_wording() {
    let h = H::open();
    let (a, _) = h.player(1);
    h.call(&a, "coinA", "rug").await;
    h.set_time(CLOSE + 100);
    h.settle("coinA", Outcome::Rugged, CLOSE + 50);
    h.settle("coinB", Outcome::Stood, CLOSE + 60);
    for path in PUBLIC {
        if path == "/v1/privacy" {
            continue;
        }
        let body = h.get(path, None).await.json();
        assert_eq!(body["read_at"], CLOSE + 100, "{path} does not say when");
        assert!(
            body.get("newest_at").is_some(),
            "{path} does not say how old its newest row is"
        );
    }
    let outcomes = h.get("/v1/rounds/r1/outcomes", None).await;
    assert_eq!(outcomes.json()["newest_at"], CLOSE + 60);
    assert!(outcomes.body.contains("rug observed within the window"));
    assert!(outcomes.body.contains("no qualifying rug observed"));
    // No public response carries an internal name, a value, a winner or the
    // chain's row count or head.
    for (path, body) in PUBLIC.iter().zip(h.public_bodies().await) {
        // The notice is prose about rows and heads; the others are data.
        if *path == "/v1/privacy" {
            continue;
        }
        for banned in [
            "Rugged",
            "Stood",
            "winner",
            // Keys, quoted: `q_basis_points` is not a points figure.
            "\"points\"",
            "\"score",
            "\"total",
            "\"rows\"",
            "\"head\"",
        ] {
            assert!(
                !body.contains(banned),
                "a response holds {banned:?}: {body}"
            );
        }
    }
}

#[tokio::test]
async fn no_origin_configured_sends_no_cors_header_and_a_configured_one_is_matched_exactly() {
    // Nothing configured: no header, even for a browser that sends an Origin.
    let closed = H::new(None, Some(ROUNDS));
    let r = closed
        .send(
            Method::GET,
            "/v1/rounds/r1",
            None,
            false,
            Some("https://site.test"),
            None,
        )
        .await;
    assert!(r.headers.get("access-control-allow-origin").is_none());
    assert!(r.headers.get("access-control-allow-credentials").is_none());
    let pre = closed
        .send(
            Method::OPTIONS,
            "/forecast",
            None,
            false,
            Some("https://site.test"),
            None,
        )
        .await;
    assert!(pre.headers.get("access-control-allow-origin").is_none());

    let open = H::new(Some("https://site.test"), Some(ROUNDS));
    let r = open
        .send(
            Method::GET,
            "/v1/rounds/r1",
            None,
            false,
            Some("https://site.test"),
            None,
        )
        .await;
    assert_eq!(
        r.headers["access-control-allow-origin"],
        "https://site.test"
    );
    assert_eq!(r.headers["access-control-allow-credentials"], "true");
    assert_eq!(r.headers["vary"], "Origin");
    let pre = open
        .send(
            Method::OPTIONS,
            "/forecast",
            None,
            false,
            Some("https://site.test"),
            None,
        )
        .await;
    assert_eq!(pre.status, StatusCode::NO_CONTENT);
    let allowed = pre.headers["access-control-allow-headers"]
        .to_str()
        .unwrap();
    assert!(allowed.contains("x-csrf-token"), "{allowed}");
    assert_eq!(
        pre.headers["access-control-allow-origin"],
        "https://site.test"
    );
    // Near misses are not the origin.
    for other in [
        "https://site.test.evil.example",
        "http://site.test",
        "https://evil.example",
        "null",
    ] {
        let r = open
            .send(Method::GET, "/v1/rounds/r1", None, false, Some(other), None)
            .await;
        assert!(
            r.headers.get("access-control-allow-origin").is_none(),
            "{other} was allowed"
        );
    }
}

#[tokio::test]
async fn without_a_usable_rounds_file_every_round_route_refuses() {
    let none = H::new(None, None);
    let (a, _) = none.player(1);
    assert_eq!(
        none.call(&a, "coinA", "rug").await.status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        none.get("/v1/rounds/r1", None).await.status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    // Two closes for one round is no close.
    let twice = r#"{"rounds":[{"id":"r1","close":5,"coins":[]},{"id":"r1","close":9,"coins":[]}]}"#;
    assert!(parse_rounds(twice).is_none());
    let twice = H::new(None, Some(twice));
    assert_eq!(
        twice.get("/v1/rounds/r1", None).await.status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let garbage = H::new(None, Some("not json"));
    assert_eq!(
        garbage.get("/v1/rounds/r1", None).await.status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    // An unknown round is a 404 on a good file.
    assert_eq!(
        H::open().get("/v1/rounds/nope", None).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn the_privacy_notice_lists_every_kept_field_and_needs_no_configuration() {
    let h = H::new(None, None);
    let r = h.get("/v1/privacy", None).await;
    assert_eq!(r.status, StatusCode::OK);
    for field in [
        "X handle",
        "X account id",
        "account creation date",
        "session hash",
        "signed_in_at",
        "player key",
    ] {
        assert!(r.body.contains(field), "the notice omits {field}");
    }
    assert!(r.body.contains("replaces the old one"));
    assert!(r.body.contains("thrown away"));
}

#[test]
fn a_round_id_is_kept_only_when_present_unique_and_at_most_the_limit() {
    let one = |id: &str| format!(r#"{{"rounds":[{{"id":"{id}","close":5,"coins":[]}}]}}"#);
    // Empty alone, and a too-long id alone: each is refused by its own clause.
    assert!(parse_rounds(&one("")).is_none());
    assert!(parse_rounds(&one(&"a".repeat(MAX_ID + 1))).is_none());
    // The last good length and the shortest good one are kept.
    assert!(parse_rounds(&one(&"a".repeat(MAX_ID))).is_some());
    assert!(parse_rounds(&one("a")).is_some());
    // Distinct ids are kept, so "already seen" is not just "any second id".
    let two = r#"{"rounds":[{"id":"r1","close":5,"coins":[]},{"id":"r2","close":9,"coins":[]}]}"#;
    assert_eq!(parse_rounds(two).map(|r| r.len()), Some(2));
}

#[tokio::test]
async fn a_body_of_exactly_the_limit_is_read_and_one_byte_more_is_a_413() {
    let h = H::open();
    let (a, _) = h.player(1);
    let base = r#"{"round":"r1","chain":"solana","token":"coinA","side":"rug"}"#;
    let pad = |n: usize| format!("{base}{}", " ".repeat(n - base.len()));
    let post = |body: String| {
        let (h, a) = (&h, a.clone());
        async move {
            h.send(Method::POST, "/forecast", Some(&a), true, None, Some(&body))
                .await
        }
    };
    let over = post(pad(MAX_BODY + 1)).await;
    assert_eq!(over.status, StatusCode::PAYLOAD_TOO_LARGE);
    let at_limit = post(pad(MAX_BODY)).await;
    assert_eq!(at_limit.status, StatusCode::CREATED, "{}", at_limit.body);
}

#[tokio::test]
async fn a_coin_without_odds_makes_the_rounds_file_invalid_and_writes_no_row() {
    // The file is the only source of odds. A coin that lists none must not be
    // given a stand-in: the row is append-only, so an invented figure would be
    // permanent (AGENTS.md rule 8).
    let no_odds = r#"{"rounds":[{"id":"r1","close":1800001000,"coins":[
        {"chain":"solana","token":"coinA"}]}]}"#;
    assert!(parse_rounds(no_odds).is_none());
    let h = H::new(None, Some(no_odds));
    let (a, _) = h.player(1);
    let r = h.call(&a, "coinA", "rug").await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE, "{}", r.body);
    // Nothing reached the chain: the round shows no forecast row at all.
    let rows = h.state.with_store(|s| {
        let all = s.closed_forecasts("r1", CLOSE + 1)?;
        Ok(all.len())
    });
    assert_eq!(rows.unwrap_or(usize::MAX), 0, "a row was written");
}

#[tokio::test]
async fn a_close_moved_after_calls_were_saved_is_a_503_and_saves_nothing() {
    let h = H::open();
    let (a, _) = h.player(1);
    let (b, _) = h.player(2);
    assert_eq!(h.call(&a, "coinA", "rug").await.status, StatusCode::CREATED);
    // The operator pushes the close later by editing the file.
    let moved = ROUNDS.replace("1800001000", "1800009000");
    std::fs::write(h.dir.path().join("rounds.json"), moved).unwrap();
    let r = h.call(&b, "coinA", "real").await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE, "{}", r.body);
    let mine = h.get("/forecast/mine?round=r1", Some(&b)).await.json();
    assert!(mine["forecasts"].as_array().unwrap().is_empty());
}

/// A GET that arrives from `peer`, the way the server's `ConnectInfo` layer
/// would stamp it.
async fn get_from(h: &H, path: &str, peer: &str) -> StatusCode {
    let addr = std::net::SocketAddr::new(peer.parse().unwrap(), 4000);
    let mut req = Request::builder().method(Method::GET).uri(path);
    req.extensions_mut()
        .unwrap()
        .insert(axum::extract::ConnectInfo(addr));
    let response = crate::session_routes(Arc::clone(&h.state))
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    response.status()
}

#[tokio::test]
async fn the_reads_that_scan_the_store_are_limited_per_client_and_per_ipv6_slash_64() {
    let h = H::open();
    // Closed, so the per-round reads reach the store instead of stopping at "open".
    h.set_time(CLOSE + 100);
    // The three scanning reads share one allowance for a client.
    let scans = [
        "/v1/board",
        "/v1/rounds/r1/forecasts",
        "/v1/rounds/r1/outcomes",
    ];
    for i in 0..READ_LIMIT_FOR_TEST {
        let status = get_from(&h, scans[i % 3], "203.0.113.7").await;
        assert_eq!(status, StatusCode::OK, "read {i} was refused early");
    }
    for path in scans {
        assert_eq!(
            get_from(&h, path, "203.0.113.7").await,
            StatusCode::TOO_MANY_REQUESTS,
            "{path} was not limited"
        );
    }
    // Another client is not affected.
    assert_eq!(
        get_from(&h, "/v1/board", "203.0.113.8").await,
        StatusCode::OK
    );
    // Reads that do not walk the store are not counted.
    assert_eq!(
        get_from(&h, "/v1/privacy", "203.0.113.7").await,
        StatusCode::OK
    );
    assert_eq!(
        get_from(&h, "/v1/rounds/r1", "203.0.113.7").await,
        StatusCode::OK
    );
    // The window passes and the client may read again.
    h.set_time(CLOSE + 160);
    assert_eq!(
        get_from(&h, "/v1/board", "203.0.113.7").await,
        StatusCode::OK
    );

    // An IPv6 client hopping addresses inside its /64 stays in one bucket.
    for i in 0..READ_LIMIT_FOR_TEST {
        let peer = format!("2001:db8:9:9::{:x}", i + 1);
        assert_eq!(get_from(&h, "/v1/board", &peer).await, StatusCode::OK);
    }
    assert_eq!(
        get_from(&h, "/v1/board", "2001:db8:9:9:ffff::1").await,
        StatusCode::TOO_MANY_REQUESTS,
        "a fresh address in the same /64 got a fresh allowance"
    );
    assert_eq!(
        get_from(&h, "/v1/board", "2001:db8:9:a::1").await,
        StatusCode::OK,
        "a different /64 was counted with the first"
    );
}

/// `READ_LIMIT`, restated here so a change to it is a change to this test.
const READ_LIMIT_FOR_TEST: usize = 30;
