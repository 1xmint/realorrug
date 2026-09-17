// SPDX-License-Identifier: Apache-2.0
//! The public site's server.
//!
//! Five documents and a health check, each read from a published file. No
//! store, no identity, no operator routes: the reply log's full fact sheets are
//! an operator's working material and stay on the box, so nothing here serves
//! them.

pub mod card;
pub mod check;
pub mod crawler;
pub mod public;

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
    // One `CheckState` shared by `/v1/check/{address}` and
    // `/v1/check/{address}/card.png` -- they must agree on the same cache,
    // rate limiter and daily budget, not each hold their own (card.rs's own
    // doc comment: "never a second chain read").
    let check_state = check::CheckState::shared();
    Router::new()
        .route("/health", get(health))
        .route("/v1/public/stats", get(public::stats))
        .route("/v1/public/leaderboard", get(public::leaderboard))
        .route("/v1/public/pool", get(public::pool))
        .route("/v1/public/weeks", get(public::weeks))
        .route("/v1/public/hunters", get(public::hunters))
        .merge(check::router(check_state.clone()))
        .merge(card::router(check_state.clone()))
        .merge(crawler::router(check_state))
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
