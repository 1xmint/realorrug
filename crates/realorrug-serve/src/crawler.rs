// SPDX-License-Identifier: Apache-2.0
//! `GET /check/{address}` — the crawler-facing unfurl shell.
//!
//! Design 0025 §7 wants every shared `/check/:address` link to unfurl on X
//! as the verdict card. The live site is a static SPA (`site/`, built and
//! served elsewhere — see the module doc's "What this route does not solve"
//! below); X's crawler does not run JS, so it never sees the SPA's
//! client-side title or `og:image`. This route answers the same path with a
//! tiny, fully server-rendered HTML document carrying `og:image` /
//! `twitter:card=summary_large_image` pointing at
//! [`crate::card`]'s PNG, plus an immediate redirect for an actual browser
//! that lands here — so a human who opens the shared link still ends up on
//! the real, interactive SPA, never stuck on this shell.
//!
//! # Reuses the same cached read, never a second chain read
//!
//! Exactly like `card.rs`, this calls [`crate::check::check`] — the shared
//! singleflight/cache/budget `CheckState` — rather than reading the chain
//! itself. A cold address here costs the same one RPC read the JSON and PNG
//! routes already share, never a second one.
//!
//! # What this route does not solve
//!
//! `deploy/README.md` (as of this commit) says the live hostname's traffic
//! is split at the Cloudflare Tunnel by *path*: `/v1/public/*` (and, since
//! the previous commit, the rest of `/v1/check/*`) goes to `realorrug-serve`
//! on `127.0.0.1:8090`; everything else — including plain `/check/:address`,
//! which the SPA's client router owns — still goes to `radar-serve`, a
//! different process this repository does not build. A path-based tunnel
//! rule cannot tell X's crawler apart from a human browser on the *same*
//! path, so simply adding `/check/*` to that tunnel rule would take the SPA
//! away from every human visitor, not just bots.
//!
//! This route exists so that swap is possible, but the swap itself is a
//! hostname-level decision outside this repository:
//!
//! - **What is needed**: a rule in front of the tunnel that inspects
//!   `User-Agent` (`Twitterbot`, `facebookexternalhit`, `Slackbot`,
//!   `Discordbot`, `LinkedInBot`, …) on the `/check/*` path specifically,
//!   and sends only those requests to `realorrug-serve`'s `/check/{address}`
//!   here, leaving every other `/check/*` request routed to `radar-serve` as
//!   today. `cloudflared`'s own ingress rules (`/etc/cloudflared/config.yml`)
//!   match by hostname and path only, not by header, so this needs a
//!   Cloudflare-side Transform Rule / Worker on the zone in front of the
//!   tunnel, not a `config.yml` edit.
//! - **Until that rule exists**: this route answers nothing in production —
//!   it is merged and tested, but no request reaches it, because
//!   `/check/*` is not yet pointed at `realorrug-serve` for any
//!   User-Agent. A human who follows this route directly (e.g. by hitting
//!   `https://radar.heyvera.org:8090/check/...` on the box itself) still
//!   gets the meta tags and the redirect below, so the route is not merely
//!   theoretical, but it is not live-reachable through the public hostname
//!   yet.

use std::fmt::Write as _;
use std::sync::Arc;

use axum::Router;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;

use crate::card::{MAX_NAME_CHARS, MAX_SYMBOL_CHARS, escape_xml, stamp_word, truncate};
use crate::check::{CheckState, check, client_ip};

/// The public hostname `deploy/README.md` names as live since 2026-09-14.
/// `site/index.html`'s own static `og:*` tags still point at
/// `cabalhunter.org`, a domain design 0025 notes is not purchased yet — this
/// route uses the hostname that is actually serving traffic today instead,
/// so a card URL an unfurl actually fetches is never a dead link.
const HOSTNAME: &str = "https://radar.heyvera.org";

/// The router for this route alone, sharing `state` with `check::router` and
/// `card::router` so the three never disagree about the cache, the budget or
/// the rate limiter.
pub fn router(state: Arc<CheckState>) -> Router {
    Router::new()
        .route("/check/{address}", get(handle))
        .with_state(state)
}

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

    // A checker-level refusal is not a page worth an unfurl or a cache entry
    // under this address — same reasoning as `card.rs`'s early return.
    if status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::BAD_REQUEST
        || status == StatusCode::SERVICE_UNAVAILABLE
    {
        return status.into_response();
    }

    let level = doc["level"].as_str().map(str::to_owned);
    let raw_address = doc["address"].as_str().unwrap_or(&address).to_owned();
    let word = stamp_word(level.as_deref());

    let html = build_html(word, &raw_address);
    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "public, max-age=600")],
        Html(html),
    )
        .into_response()
}

/// Builds the shell document: meta tags a crawler reads without running any
/// JS, and a same-effect `<meta http-equiv="refresh">` plus a `<script>`
/// redirect for a browser that renders this page — belt and suspenders,
/// since a refresh tag alone is honoured by every browser but a script is
/// instant, and a script alone does nothing for the handful of crawlers that
/// still execute basic JS.
///
/// `word` and `raw_address` are the only two strings this function accepts;
/// `raw_address` is XML/HTML-escaped exactly like `card.rs` escapes
/// untrusted text, even though today it only ever holds a chain address this
/// route's own `check()` call already validated the shape of — never a
/// price, a market cap, or any other number this module was not handed.
fn build_html(word: &str, raw_address: &str) -> String {
    let safe_address = escape_xml(&truncate(raw_address, MAX_NAME_CHARS.max(MAX_SYMBOL_CHARS)));
    let card_url = format!("{HOSTNAME}/v1/check/{safe_address}/card.png");
    let spa_url = format!("{HOSTNAME}/check/{safe_address}");
    let title = format!("Real or Rug: {word}");
    let description = "Checked, not predicted.";

    let mut out = String::new();
    let _ = write!(
        out,
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">"#
    );
    let _ = write!(out, r"<title>{title}</title>");
    let _ = write!(out, r#"<meta name="description" content="{description}">"#);
    let _ = write!(out, r#"<meta property="og:type" content="website">"#);
    let _ = write!(
        out,
        r#"<meta property="og:site_name" content="Real or Rug">"#
    );
    let _ = write!(out, r#"<meta property="og:title" content="{title}">"#);
    let _ = write!(
        out,
        r#"<meta property="og:description" content="{description}">"#
    );
    let _ = write!(out, r#"<meta property="og:url" content="{spa_url}">"#);
    let _ = write!(out, r#"<meta property="og:image" content="{card_url}">"#);
    let _ = write!(out, r#"<meta property="og:image:width" content="1200">"#);
    let _ = write!(out, r#"<meta property="og:image:height" content="630">"#);
    let _ = write!(
        out,
        r#"<meta name="twitter:card" content="summary_large_image">"#
    );
    let _ = write!(out, r#"<meta name="twitter:title" content="{title}">"#);
    let _ = write!(
        out,
        r#"<meta name="twitter:description" content="{description}">"#
    );
    let _ = write!(out, r#"<meta name="twitter:image" content="{card_url}">"#);
    let _ = write!(
        out,
        r#"<meta http-equiv="refresh" content="0; url={spa_url}">"#
    );
    let _ = write!(out, r"</head><body>");
    let _ = write!(
        out,
        r#"<p>Real or Rug: {word}. <a href="{spa_url}">Continue to the check.</a></p>"#
    );
    let _ = write!(out, r"<script>location.replace({spa_url:?});</script>");
    let _ = write!(out, r"</body></html>");
    out
}

#[cfg(test)]
mod tests {
    use super::build_html;

    #[test]
    fn a_script_tag_in_the_address_renders_as_escaped_text() {
        let html = build_html("Sketchy", "<script>alert(1)</script>");
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn the_card_and_spa_urls_use_the_live_hostname() {
        let html = build_html("Rugged", "abc123");
        assert!(html.contains("https://radar.heyvera.org/v1/check/abc123/card.png"));
        assert!(html.contains("https://radar.heyvera.org/check/abc123"));
    }

    #[test]
    fn no_price_or_market_cap_field_can_reach_this_document() {
        // `build_html` takes exactly two strings, neither of which is ever a
        // price or a market cap in any caller (`handle` above only ever
        // passes a ladder word and a validated address) -- this test pins
        // that shape so a future edit adding a third parameter here is
        // forced to explain what it is.
        let html = build_html(
            "Nothing ugly yet",
            "So11111111111111111111111111111111111111112",
        );
        assert!(!html.contains("price"));
        assert!(!html.contains("market_cap"));
        assert!(!html.contains("mcap"));
    }

    #[test]
    fn every_known_word_appears_in_the_title() {
        for word in [
            "Rugged",
            "Rug mechanics live",
            "Sketchy",
            "Nothing ugly yet",
            "Can't tell",
        ] {
            let html = build_html(word, "abc");
            assert!(html.contains(word), "{word} missing from: {html}");
        }
    }
}
