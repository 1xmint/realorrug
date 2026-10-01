// SPDX-License-Identifier: Apache-2.0
//! Free public case browsing and authenticated, bounded investigation intake.
use crate::auth::{AuthState, refuse};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use realorrug_onchain::{
    Memory,
    cases::{Capacity, CaseError, CaseKey, Investigation, Network, TimeWindow, request_id},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

/// Shared route state; no SQLite lock lives on an async worker.
pub(crate) struct LibraryState {
    auth: Arc<AuthState>,
    path: Option<PathBuf>,
    capacity: Option<Capacity>,
    clients: Mutex<HashMap<String, (u64, u32)>>,
    trust_cloudflare: bool,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl LibraryState {
    fn from_env(auth: Arc<AuthState>) -> Self {
        let get = |key: &str| std::env::var(key).ok();
        let path =
            realorrug_types::env::env_or_legacy("REALORRUG_ANALYST_DIR", "RADAR_ANALYST_DIR", get)
                .map(|dir| PathBuf::from(dir).join("memory.sqlite3"));
        Self {
            auth,
            path,
            capacity: realorrug_analyst::investigator::capacity_from(&get),
            clients: Mutex::new(HashMap::new()),
            trust_cloudflare: get("REALORRUG_TRUST_CLOUDFLARE").as_deref() == Some("1"),
            clock: Arc::new(now),
        }
    }
    fn allow(&self, headers: &HeaderMap, request: &Request) -> bool {
        // Proxy headers do not identify a caller unless the operator configures
        // the trusted proxy. Direct deployments use the TCP peer address.
        let peer = request
            .extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map_or_else(|| "unknown".into(), |p| p.0.ip().to_string());
        let key = if self.trust_cloudflare {
            headers
                .get("cf-connecting-ip")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<std::net::IpAddr>().ok())
                .map_or(peer, |ip| ip.to_string())
        } else {
            peer
        };
        let at = (self.clock)();
        let Ok(mut clients) = self.clients.lock() else {
            return false;
        };
        clients.retain(|_, (minute, _)| *minute == at / 60);
        if clients.len() >= 1024 && !clients.contains_key(&key) {
            return false;
        }
        let entry = clients.entry(key).or_insert((at / 60, 0));
        entry.1 += 1;
        entry.1 <= 30
    }
    #[allow(
        clippy::result_large_err,
        reason = "HTTP handlers return the existing axum response type on failure"
    )]
    async fn memory<T: Send + 'static>(
        self: &Arc<Self>,
        write: bool,
        f: impl FnOnce(&Memory) -> Result<T, CaseError> + Send + 'static,
    ) -> Result<T, Response> {
        let path = self
            .path
            .clone()
            .filter(|p| p.is_file())
            .ok_or_else(unavailable)?;
        tokio::task::spawn_blocking(move || {
            let memory = if write {
                Memory::open(&path)
            } else {
                Memory::read_only(&path)
            }
            .map_err(|_| unavailable())?;
            f(&memory).map_err(case_error)
        })
        .await
        .unwrap_or_else(|_| Err(unavailable()))
    }
}

fn unavailable() -> Response {
    refuse(
        StatusCode::SERVICE_UNAVAILABLE,
        "the public Library is not configured or its history is unavailable",
    )
}
#[allow(
    clippy::needless_pass_by_value,
    reason = "map_err consumes the case error at the HTTP boundary"
)]
fn case_error(error: CaseError) -> Response {
    let status = match error {
        CaseError::Capacity => StatusCode::TOO_MANY_REQUESTS,
        CaseError::Conflict => StatusCode::CONFLICT,
        CaseError::Invalid(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    refuse(status, &error.to_string())
}

pub(crate) fn router(auth: Arc<AuthState>) -> Router {
    let state = Arc::new(LibraryState::from_env(auth));
    Router::new()
        .route("/v1/library", get(index))
        .route("/v1/cases/{chain}/{address}", get(dossier))
        .route("/v1/cases/{chain}/{address}/history", get(history))
        .route(
            "/v1/cases/{chain}/{address}/contributions",
            post(contribution),
        )
        .route("/v1/investigations", post(investigation))
        .route("/v1/investigations/{id}", get(job))
        .layer(DefaultBodyLimit::max(16_384))
        .with_state(state)
}

#[derive(Deserialize, Default)]
struct Search {
    chain: Option<Network>,
    q: Option<String>,
    before: Option<u64>,
    limit: Option<u32>,
}

async fn index(
    State(state): State<Arc<LibraryState>>,
    Query(search): Query<Search>,
    request: Request,
) -> Response {
    if !state.allow(request.headers(), &request) {
        return refuse(StatusCode::TOO_MANY_REQUESTS, "too many Library reads");
    }
    match state
        .memory(false, move |m| {
            m.library_cases(
                search.chain,
                search.q.as_deref().unwrap_or(""),
                search.before.unwrap_or(u64::MAX),
                search.limit.unwrap_or(20),
            )
        })
        .await
    {
        Ok(cases) => Json(
            json!({"cases":cases,"read_at":now(),"next_before":cases.last().map(|c|c.revision)}),
        )
        .into_response(),
        Err(e) => e,
    }
}

async fn dossier(
    State(state): State<Arc<LibraryState>>,
    Path((chain, address)): Path<(String, String)>,
    request: Request,
) -> Response {
    if !state.allow(request.headers(), &request) {
        return refuse(StatusCode::TOO_MANY_REQUESTS, "too many Library reads");
    }
    let key = match chain.parse().and_then(|n| CaseKey::new(n, &address)) {
        Ok(k) => k,
        Err(e) => return case_error(e),
    };
    match state.memory(false, move |m| m.case_dossier(&key)).await {
        Ok(Some(case)) => Json(json!({"dossier":case,"read_at":now()})).into_response(),
        Ok(None) => refuse(StatusCode::NOT_FOUND, "this token has no public case yet"),
        Err(e) => e,
    }
}

async fn history(
    State(state): State<Arc<LibraryState>>,
    Path((chain, address)): Path<(String, String)>,
    Query(search): Query<Search>,
    request: Request,
) -> Response {
    if !state.allow(request.headers(), &request) {
        return refuse(StatusCode::TOO_MANY_REQUESTS, "too many Library reads");
    }
    let key = match chain.parse().and_then(|n| CaseKey::new(n, &address)) {
        Ok(k) => k,
        Err(e) => return case_error(e),
    };
    match state
        .memory(false, move |m| {
            m.case_history(
                &key,
                search.before.unwrap_or(u64::MAX),
                search.limit.unwrap_or(20),
            )
        })
        .await
    {
        Ok(events) => Json(
            json!({"events":events,"read_at":now(),"next_before":events.last().map(|e|e.revision)}),
        )
        .into_response(),
        Err(e) => e,
    }
}

async fn job(
    State(state): State<Arc<LibraryState>>,
    Path(id): Path<String>,
    request: Request,
) -> Response {
    if !state.allow(request.headers(), &request) {
        return refuse(StatusCode::TOO_MANY_REQUESTS, "too many Library reads");
    }
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return refuse(StatusCode::BAD_REQUEST, "invalid request id");
    }
    match state.memory(false, move |m| m.case_job(&id)).await {
        Ok(Some(job)) => Json(json!({"job":job,"read_at":now()})).into_response(),
        Ok(None) => refuse(StatusCode::NOT_FOUND, "no such investigation"),
        Err(e) => e,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Intake {
    chain: Network,
    address: String,
    question: String,
    #[serde(default)]
    wallets: Vec<String>,
    #[serde(default)]
    transactions: Vec<String>,
    window: Option<TimeWindow>,
    idempotency_key: String,
}

async fn investigation(
    State(state): State<Arc<LibraryState>>,
    headers: HeaderMap,
    Json(input): Json<Intake>,
) -> Response {
    admit(state, headers, input, None).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contribution {
    note: String,
    idempotency_key: String,
}

async fn contribution(
    State(state): State<Arc<LibraryState>>,
    Path((chain, address)): Path<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<Contribution>,
) -> Response {
    let chain = match chain.parse() {
        Ok(c) => c,
        Err(e) => return case_error(e),
    };
    let question = format!("Community lead or correction (unverified): {}", input.note);
    admit(
        state,
        headers,
        Intake {
            chain,
            address,
            question,
            wallets: vec![],
            transactions: vec![],
            window: None,
            idempotency_key: input.idempotency_key,
        },
        Some("contribution".into()),
    )
    .await
}

async fn admit(
    state: Arc<LibraryState>,
    headers: HeaderMap,
    input: Intake,
    source: Option<String>,
) -> Response {
    let session = match state.auth.authenticate(&headers, true) {
        Ok(s) => s,
        Err(e) => return e,
    };
    let Some(capacity) = state.capacity else {
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "investigation intake is disabled",
        );
    };
    if input.idempotency_key.is_empty() || input.idempotency_key.len() > 128 {
        return refuse(StatusCode::BAD_REQUEST, "invalid idempotency key");
    }
    let case = match CaseKey::new(input.chain, &input.address) {
        Ok(k) => k,
        Err(e) => return case_error(e),
    };
    if case.chain == Network::Robinhood {
        return refuse(
            StatusCode::BAD_REQUEST,
            "Robinhood uses the existing checker; this investigation intake supports Solana, Base and Ethereum",
        );
    }
    let identity = match state
        .auth
        .with_store(|store| store.identity(&session.player))
    {
        Ok(Some(identity)) => identity,
        Ok(None) => return refuse(StatusCode::UNAUTHORIZED, "sign in with X to contribute"),
        Err(e) => return e,
    };
    let actor = realorrug_onchain::cases::actor_id(&identity.x_id);
    let request = Investigation {
        id: request_id(&actor, &input.idempotency_key),
        case,
        question: input.question,
        wallets: input.wallets,
        transactions: input.transactions,
        window: input.window,
        source,
        thread: None,
    };
    if let Err(e) = request.validate() {
        return case_error(e);
    }
    let at = now();
    match state
        .memory(true, move |m| {
            if m.case_job(&request.id)?.is_none() && !m.case_worker_live(at)? {
                return Err(CaseError::Capacity);
            }
            m.enqueue_case(&request, &actor, at, capacity)
        })
        .await
    {
        Ok(job) => (StatusCode::ACCEPTED, Json(json!({"job":job,"public":true}))).into_response(),
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    #[test]
    fn public_read_limits_expire_and_ignore_untrusted_proxy_headers() {
        use std::sync::atomic::{AtomicU64, Ordering};
        let time = Arc::new(AtomicU64::new(60));
        let clock = Arc::clone(&time);
        let auth = Arc::new(AuthState::from_vars(&|_| None, None, Arc::new(|| 1)));
        let mut state = LibraryState {
            auth,
            path: None,
            capacity: None,
            clients: Mutex::new(HashMap::new()),
            trust_cloudflare: false,
            clock: Arc::new(move || clock.load(Ordering::SeqCst)),
        };
        let request = Request::get("/v1/library")
            .header("cf-connecting-ip", "192.0.2.1")
            .body(Body::empty())
            .unwrap();
        for _ in 0..30 {
            assert!(state.allow(request.headers(), &request));
        }
        assert!(!state.allow(request.headers(), &request));
        let other = Request::get("/v1/library")
            .header("cf-connecting-ip", "192.0.2.2")
            .body(Body::empty())
            .unwrap();
        assert!(!state.allow(other.headers(), &other));
        time.store(120, Ordering::SeqCst);
        assert!(state.allow(other.headers(), &other));
        assert_eq!(state.clients.lock().unwrap()["unknown"].1, 1);
        state.trust_cloudflare = true;
        assert!(state.allow(request.headers(), &request));
        assert_eq!(state.clients.lock().unwrap()["192.0.2.1"].1, 1);
        assert!(state.allow(other.headers(), &other));
        let mut clients = state.clients.lock().unwrap();
        for i in 0..1024 {
            clients.insert(format!("cardinality-{i}"), (2, 1));
        }
        drop(clients);
        let new = Request::get("/v1/library")
            .header("cf-connecting-ip", "192.0.2.3")
            .body(Body::empty())
            .unwrap();
        assert!(!state.allow(new.headers(), &new));
        assert!(state.allow(other.headers(), &other));
    }
    #[tokio::test]
    async fn every_public_case_route_refuses_exhausted_read_capacity() {
        let auth = Arc::new(AuthState::from_vars(&|_| None, None, Arc::new(|| 1)));
        let state = Arc::new(LibraryState {
            auth,
            path: None,
            capacity: None,
            clients: Mutex::new(HashMap::from([("unknown".into(), (0, 30))])),
            trust_cloudflare: false,
            clock: Arc::new(|| 1),
        });
        let app = Router::new()
            .route("/v1/library", get(index))
            .route("/v1/cases/{chain}/{address}", get(dossier))
            .route("/v1/cases/{chain}/{address}/history", get(history))
            .route("/v1/investigations/{id}", get(job))
            .with_state(state);
        for path in [
            "/v1/library".into(),
            "/v1/cases/base/0x1111111111111111111111111111111111111111".into(),
            "/v1/cases/base/0x1111111111111111111111111111111111111111/history".into(),
            format!("/v1/investigations/{}", "a".repeat(64)),
        ] {
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        }
        assert_eq!(
            case_error(CaseError::Capacity).status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            case_error(CaseError::Conflict).status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            case_error(CaseError::Invalid("invalid".into())).status(),
            StatusCode::BAD_REQUEST
        );
    }
    #[tokio::test]
    async fn library_browsing_needs_no_session_and_unconfigured_intake_cannot_accept_work() {
        let vars = |_: &str| None;
        let auth = Arc::new(AuthState::from_vars(&vars, None, Arc::new(|| 1)));
        let state = Arc::new(LibraryState {
            auth,
            path: None,
            capacity: None,
            clients: Mutex::new(HashMap::new()),
            trust_cloudflare: false,
            clock: Arc::new(now),
        });
        let app = Router::new()
            .route("/v1/library", get(index))
            .with_state(state);
        let response = app
            .oneshot(Request::get("/v1/library").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn authenticated_intake_requires_csrf_and_recovers_duplicates_without_a_live_worker() {
        use crate::auth::{SESSION_COOKIE, csrf_for, sha256_hex};
        use http_body_util::BodyExt as _;
        use realorrug_store::Identity;
        let dir = tempfile::tempdir().unwrap();
        let account_path = dir.path().join("accounts.sqlite3");
        let variables = HashMap::from([
            (
                "REALORRUG_STORE_PATH",
                account_path.to_string_lossy().into_owned(),
            ),
            ("REALORRUG_X_CLIENT_ID", "local-test".into()),
            (
                "REALORRUG_X_REDIRECT_URI",
                "https://test.invalid/auth/x/callback".into(),
            ),
        ]);
        let auth = Arc::new(AuthState::from_vars(
            &|key| variables.get(key).cloned(),
            None,
            Arc::new(|| 1),
        ));
        let token = "a".repeat(64);
        auth.with_store(|store| {
            let player = store.new_player_key()?;
            store.upsert_identity(&Identity {
                player_key: player,
                x_id: "fixture-x-id".into(),
                handle: "fixture".into(),
                account_created_at: None,
                session_hash: Some(sha256_hex(token.as_bytes())),
                signed_in_at: 1,
            })
        })
        .unwrap();
        let path = dir.path().join("cases.sqlite3");
        let memory = Memory::open(&path).unwrap();
        memory.case_heartbeat(now()).unwrap();
        let state = Arc::new(LibraryState {
            auth,
            path: Some(path),
            capacity: Some(Capacity {
                daily: 1,
                per_actor: 1,
                pending: 1,
            }),
            clients: Mutex::new(HashMap::new()),
            trust_cloudflare: false,
            clock: Arc::new(now),
        });
        let app = Router::new()
            .route("/v1/investigations", post(investigation))
            .with_state(state);
        let input = json!({"chain":"base","address":"0x1111111111111111111111111111111111111111","question":"Trace the fee route", "wallets":[],"transactions":[],"window":null,"idempotency_key":"same"});
        let request = |csrf: bool| {
            let mut request = Request::post("/v1/investigations")
                .header("content-type", "application/json")
                .header("cookie", format!("{SESSION_COOKIE}={token}"));
            if csrf {
                request = request.header("x-csrf-token", csrf_for(&token));
            }
            request.body(Body::from(input.to_string())).unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request(false)).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        let first = app.clone().oneshot(request(true)).await.unwrap();
        assert_eq!(first.status(), StatusCode::ACCEPTED);
        let first: serde_json::Value =
            serde_json::from_slice(&first.into_body().collect().await.unwrap().to_bytes()).unwrap();
        // A duplicate that already reached disk is recoverable even after worker loss.
        memory.case_heartbeat(0).unwrap();
        let retry = app.oneshot(request(true)).await.unwrap();
        assert_eq!(retry.status(), StatusCode::ACCEPTED);
        let retry: serde_json::Value =
            serde_json::from_slice(&retry.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(first["job"]["request"]["id"], retry["job"]["request"]["id"]);
        assert!(!retry.to_string().contains("fixture-x-id"));
        assert_eq!(
            memory.library_cases(None, "", u64::MAX, 10).unwrap().len(),
            1
        );
    }
}
