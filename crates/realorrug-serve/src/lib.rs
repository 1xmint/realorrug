// SPDX-License-Identifier: Apache-2.0
//! The public site's server.
//!
//! Five documents and a health check, each read from a published file, plus
//! the sign-in and account routes of `auth.rs`, which exist only when a store
//! path and an X app are configured. No operator routes: the reply log's full
//! fact sheets are an operator's working material and stay on the box, so
//! nothing here serves them.

mod auth;
pub mod card;
pub mod check;
pub mod facts;
mod forecast;
pub mod outcomes;
pub mod public;
mod record;

use std::sync::Arc;

use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

/// Builds the router.
///
/// Every route but one is public, read-only and stateless, so there is no
/// audience table to keep in step with it. `/v1/check/{address}` is the
/// exception -- design 0023's checker route, in `check.rs`, which needs its
/// own shared state (the cache, the rate limiter, the daily budget) and is
/// merged in rather than added to this router's own state-free routes.
/// Anything unrouted is a 404, including other methods.
pub fn app() -> Router {
    app_with_ingest().0
}

/// The router, and the task that ingests the settlement job's published
/// outcomes (`outcomes.rs`). The two share one `AuthState`, so the store has
/// one handle and one writer. The caller runs the task only when
/// [`outcomes::Ingest::enabled`]; the server's `main` does.
pub fn app_with_ingest() -> (Router, outcomes::Ingest) {
    let auth = Arc::new(auth::AuthState::from_env());
    (build(Arc::clone(&auth)), outcomes::Ingest(auth))
}

fn build(auth: Arc<auth::AuthState>) -> Router {
    // One `CheckState` shared by `/v1/check/{address}` and
    // `/v1/check/{address}/card.png` -- they must agree on the same cache,
    // rate limiter and daily budget, not each hold their own (card.rs's own
    // doc comment: "never a second chain read").
    let check_state = check::CheckState::shared();
    // One `AuthState` for the sign-in and the forecast routes: two would open
    // the store twice and split the session table's one writer in two.
    let session_routes = session_routes(auth);
    let router = Router::new()
        .route("/health", get(health))
        .route("/v1/public/stats", get(public::stats))
        .route("/v1/public/leaderboard", get(public::leaderboard))
        .route("/v1/public/pool", get(public::pool))
        .route("/v1/public/weeks", get(public::weeks))
        .route("/v1/public/hunters", get(public::hunters))
        .route("/v1/public/recent", get(public::recent))
        .merge(check::router(check_state.clone()))
        .merge(card::router(check_state))
        .merge(session_routes);
    // ADR 0036 decision 4: no `REALORRUG_X402_PAY_TO`, no route at all. A
    // request under `/v1/facts` on an unconfigured box then 404s the
    // ordinary axum way, the same as any other unrouted path.
    match facts::FactsState::from_vars(&|k| std::env::var(k).ok()) {
        Some(state) => router.merge(facts::router(Arc::new(state))),
        None => router,
    }
}

/// The sign-in, account and forecast routes with credentialed CORS over all of
/// them. One function so the tests drive the same layering the server runs.
fn session_routes(auth: Arc<auth::AuthState>) -> Router {
    auth::router(Arc::clone(&auth))
        .merge(forecast::router(Arc::clone(&auth)))
        .layer(axum::middleware::from_fn_with_state(auth, auth::cors))
}

/// `GET /health`: the version and the commit, so "is the running process the
/// one I built" is answerable from the box.
async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "build": realorrug_types::build_sha_or_unknown(),
    }))
}

#[cfg(test)]
mod tests {
    use super::app;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn get(path: &str) -> (StatusCode, String) {
        let response = app()
            .oneshot(Request::get(path).body(Body::empty()).expect("a request"))
            .await
            .expect("a response");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    #[tokio::test]
    async fn health_names_the_version() {
        let (status, body) = get("/health").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains(env!("CARGO_PKG_VERSION")), "{body}");
    }

    #[tokio::test]
    async fn every_public_document_is_routed() {
        // A route missing from `app` answers 404 with an empty body; a routed
        // document answers with JSON even when its file is absent.
        for path in [
            "/v1/public/stats",
            "/v1/public/leaderboard",
            "/v1/public/pool",
            "/v1/public/weeks",
            "/v1/public/hunters",
            "/v1/public/recent",
        ] {
            let (_, body) = get(path).await;
            assert!(body.starts_with('{'), "{path} is not routed: {body:?}");
        }
    }

    #[tokio::test]
    async fn the_checker_route_is_routed() {
        // With no RPC configured and no daily budget, this is a cold miss
        // answered `budget` -- still routed JSON, not a 404, which is what
        // this test actually pins (`check.rs`'s own tests cover the states).
        let (_, body) = get("/v1/check/So11111111111111111111111111111111111111112").await;
        assert!(body.starts_with('{'), "/v1/check is not routed: {body:?}");
    }

    #[tokio::test]
    async fn an_operator_route_does_not_exist_here() {
        let (status, _) = get("/v1/analyst/replies").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
