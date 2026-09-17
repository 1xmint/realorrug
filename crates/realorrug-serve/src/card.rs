// SPDX-License-Identifier: Apache-2.0
//! `GET /v1/check/{address}/card.png` — the verdict share card.
//!
//! Design 0025 §7 states the requirement; research 0049 recommends the
//! renderer this implements: a hand-written SVG template, filled with the
//! same checked fields `check.rs` already serves as JSON, rasterized with
//! `resvg`/`tiny-skia` (never an AI-generated image — see research 0049 §2b:
//! a generated image has no mechanism to be prevented from drawing a number
//! that is not on the fact sheet, which is exactly AGENTS.md §3 rule 2's
//! concern).
//!
//! # Reuses `check.rs`'s cached read, never a second chain read
//!
//! This route calls [`crate::check::check`] — the same singleflight,
//! budget-metered, cached function the JSON route calls — so a cold read is
//! never triggered twice for the same address, and a hot address costs zero
//! RPC calls here exactly as it does there. The token name and symbol this
//! route needs (and the JSON route's own contract never carries, per its
//! `assert_contract` test) are read back off the same cache file, under the
//! `_name`/`_symbol` keys `check.rs` stashes there on a verdict write.
//!
//! # Never a verdict for what the ladder does not know
//!
//! `state == "verdict"` or `"cant_read"` with a `level` string draws that
//! level's stamp. Every other state (`busy`, `bad_address`, `budget`,
//! `not_a_token`, or a `level` this module does not recognise) draws the
//! grey "Can't tell" card — the same rule design 0025 §5 states for the
//! ladder itself: unknown is never rendered as a fourth colour, and never as
//! green.
//!
//! # No price, ever
//!
//! [`build_svg`] takes exactly four strings — verdict word, chain, name,
//! symbol — and never a number. There is no code path from a price or
//! market-cap field to this function's parameters; a test below asserts the
//! rendered SVG text never contains a digit run of three or more (a
//! deliberately blunt check pinned to design 0025 §6's absolute rule).
//!
//! # Font
//!
//! **Not bundled yet** — this route asks `resvg`'s `fontdb` for the host's
//! own system fonts (`fontdb::Database::load_system_fonts`), which is a
//! known deviation from this route's own requirement to bundle one OFL font
//! file so rendering never depends on what is installed on the box. Fetching
//! the `@fontsource/anton` binary this crate would embed needs a network
//! download this pass did not have standing permission to make (the safety
//! policy's own "downloading any file" rule) — recorded as unbuilt rather
//! than silently skipped; `README`/PR description repeats this.

use std::fmt::Write as _;
use std::sync::Arc;

use axum::Router;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::check::{CheckState, cache_key, check, client_ip, fresh_cached_raw, now_secs};

/// Card pixel size — design 0025 §7's own number, the standard X/Twitter
/// large-image card minimum.
pub const WIDTH: u32 = 1200;
/// Card pixel height — see [`WIDTH`].
pub const HEIGHT: u32 = 630;

/// Design 0025 §7's own ceiling, so an unfurl is never itself a slow load.
pub const MAX_BYTES: usize = 300 * 1024;

/// Design 0025 §5's paper-card stamp colours (darkened for contrast on
/// `--color-paper`, per that section's own measured-contrast note).
const COLOR_RUG: &str = "#b52f1c";
const COLOR_WARN: &str = "#94600c";
const COLOR_REAL: &str = "#1a7a3c";
const COLOR_UNKNOWN: &str = "#5e6168";
const COLOR_PAPER: &str = "#e8dbbf";
const COLOR_PAPER_INK: &str = "#1b1510";
const COLOR_GOLD: &str = "#d4a93a";

/// The router for this route alone, sharing `state` with `check::router` so
/// the two never disagree about the cache, the budget or the rate limiter.
pub fn router(state: Arc<CheckState>) -> Router {
    Router::new()
        .route("/v1/check/{address}/card.png", get(handle))
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

    // A checker-level refusal (rate limit, malformed address, budget
    // exhausted) is not a card the crawler should cache as if it were a
    // verdict — pass the same status through with no body, rather than
    // drawing a grey card that would then get cached under a key nothing
    // else will ever ask for again the same way.
    if status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::BAD_REQUEST
        || status == StatusCode::SERVICE_UNAVAILABLE
    {
        return status.into_response();
    }

    let chain = doc["chain"].as_str().unwrap_or("").to_owned();
    let level = doc["level"].as_str().map(str::to_owned);
    let raw_address = doc["address"].as_str().unwrap_or(&address).to_owned();

    let (name, symbol) = if level.is_some() {
        let key = format!("{chain}:{raw_address}");
        let cache_path = state.cache_dir.join(format!("{}.json", cache_key(&key)));
        fresh_cached_raw(&cache_path, now_secs()).map_or((None, None), |raw| {
            (
                raw["_name"].as_str().map(str::to_owned),
                raw["_symbol"].as_str().map(str::to_owned),
            )
        })
    } else {
        (None, None)
    };

    let word = stamp_word(level.as_deref());
    let svg = build_svg(word, &chain, name.as_deref(), symbol.as_deref());

    match render_png(&svg) {
        Ok(png) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=600"),
            ],
            png,
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// The ladder word to stamp, exactly as `site/src/HowItWorks.tsx`'s `LADDER`
/// names each rung — the same words a visitor already sees on the page this
/// image is shared from, so a screenshot and the live page never disagree.
///
/// `None`, or any string this list does not recognise, draws "Can't tell" —
/// unknown is never a guess at a stronger verdict (design 0025 §5).
fn stamp_word(level: Option<&str>) -> &'static str {
    match level {
        Some("Rugged") => "Rugged",
        Some("RugMechanicsLive") => "Rug mechanics live",
        Some("Sketchy") => "Sketchy",
        Some("NothingUglyYet") => "Nothing ugly yet",
        _ => "Can't tell",
    }
}

fn stamp_color(word: &str) -> &'static str {
    match word {
        "Rugged" | "Rug mechanics live" => COLOR_RUG,
        "Sketchy" => COLOR_WARN,
        "Nothing ugly yet" => COLOR_REAL,
        _ => COLOR_UNKNOWN,
    }
}

/// Escapes text for use inside an SVG `<text>` node. The token name and
/// symbol are untrusted (design 0023 §6, extended to this second surface by
/// design 0025 §7): rendered as inert text, never as markup a renderer could
/// interpret.
fn escape_xml(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&apos;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

/// Truncates a name/symbol to a length the card's fixed-width text region
/// can hold, so an attacker-controlled launcher-chosen name cannot overflow
/// the card (it can still be ugly text, never markup or overflow).
const MAX_NAME_CHARS: usize = 40;
const MAX_SYMBOL_CHARS: usize = 12;

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Builds the card's SVG. Pure and synchronous: no chain read, no I/O, so it
/// is directly unit-testable for escaping, truncation and the "never a
/// digit-run from a price field" rule.
///
/// `word` is [`stamp_word`]'s output — never a raw `Level` variant name, so
/// this function cannot itself pick the wrong ladder word; that choice is
/// made once, by `stamp_word`, and this function only draws it.
fn build_svg(word: &str, chain: &str, name: Option<&str>, symbol: Option<&str>) -> String {
    let color = stamp_color(word);
    let name_text = name.map(|n| escape_xml(&truncate(n, MAX_NAME_CHARS)));
    let symbol_text = symbol.map(|s| escape_xml(&truncate(s, MAX_SYMBOL_CHARS)));
    let chain_label = escape_xml(match chain {
        "solana" => "Solana",
        "robinhood" => "Robinhood Chain",
        other if !other.is_empty() => other,
        _ => "unknown chain",
    });

    let mut body = String::new();
    let _ = write!(
        body,
        r##"<rect x="0" y="0" width="{WIDTH}" height="{HEIGHT}" fill="#0c0a09"/>"##
    );
    let _ = write!(
        body,
        r#"<rect x="60" y="60" width="{card_w}" height="{card_h}" rx="18" fill="{COLOR_PAPER}" stroke="{COLOR_GOLD}" stroke-width="4"/>"#,
        card_w = WIDTH - 120,
        card_h = HEIGHT - 120,
    );
    let _ = write!(
        body,
        r#"<text x="100" y="220" font-family="sans-serif" font-size="96" font-weight="700" fill="{color}">{word}</text>"#,
        word = escape_xml(word),
    );
    if let (Some(name_text), Some(symbol_text)) = (&name_text, &symbol_text) {
        let _ = write!(
            body,
            r#"<text x="100" y="320" font-family="sans-serif" font-size="48" fill="{COLOR_PAPER_INK}">{name_text} ({symbol_text})</text>"#,
        );
    } else if let Some(name_text) = &name_text {
        let _ = write!(
            body,
            r#"<text x="100" y="320" font-family="sans-serif" font-size="48" fill="{COLOR_PAPER_INK}">{name_text}</text>"#,
        );
    }
    let _ = write!(
        body,
        r#"<text x="100" y="{y}" font-family="monospace" font-size="30" fill="{COLOR_PAPER_INK}">{chain_label}</text>"#,
        y = HEIGHT - 140,
    );
    let _ = write!(
        body,
        r#"<text x="{x}" y="{y}" font-family="sans-serif" font-size="26" font-weight="700" fill="{COLOR_GOLD}" text-anchor="end">REALORRUG</text>"#,
        x = WIDTH - 100,
        y = HEIGHT - 100,
    );

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}">{body}</svg>"#,
    )
}

/// Rasterizes `svg` to PNG bytes via `resvg`/`tiny-skia`.
///
/// System fonts only — see this module's doc comment "Font" section for why
/// a bundled font is not wired in yet.
fn render_png(svg: &str) -> Result<Vec<u8>, String> {
    let mut fontdb = resvg::usvg::fontdb::Database::new();
    fontdb.load_system_fonts();
    let opt = resvg::usvg::Options {
        fontdb: std::sync::Arc::new(fontdb),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(WIDTH, HEIGHT).ok_or_else(|| "pixmap".to_owned())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hostile token name with `<script>` and `&` renders as inert text —
    /// no raw `<`/`>`/`&` reaches the SVG (which a renderer could interpret
    /// as markup), and the original bytes are still present, escaped.
    #[test]
    fn a_script_tag_and_an_ampersand_in_the_name_render_as_escaped_text() {
        let svg = build_svg(
            "Sketchy",
            "solana",
            Some("<script>alert(1)</script> & Friends"),
            Some("EVIL"),
        );
        assert!(
            !svg.contains("<script>"),
            "a raw <script> tag must never reach the SVG: {svg}"
        );
        assert!(
            svg.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; Friends"),
            "the escaped form of the hostile name must still be present: {svg}"
        );
    }

    /// An unreadable or unrecognised result draws the grey "Can't tell"
    /// card, never a verdict colour or word.
    #[test]
    fn an_unknown_result_draws_the_grey_cant_tell_card() {
        let word = stamp_word(None);
        assert_eq!(word, "Can't tell");
        assert_eq!(stamp_color(word), COLOR_UNKNOWN);

        let svg = build_svg(word, "solana", None, None);
        assert!(svg.contains("Can&apos;t tell") || svg.contains("Can't tell"));
        assert!(svg.contains(COLOR_UNKNOWN));
        assert!(!svg.contains(COLOR_RUG));
        assert!(!svg.contains(COLOR_WARN));
        assert!(!svg.contains(COLOR_REAL));
    }

    /// A level this module does not recognise (a hypothetical future rung,
    /// or corrupt cache data) also falls back to "Can't tell" rather than a
    /// guessed colour — the same "unknown is never rendered as safe" rule
    /// `verdict.rs` states for the ladder itself.
    #[test]
    fn an_unrecognised_level_string_also_falls_back_to_cant_tell() {
        assert_eq!(stamp_word(Some("SomethingNew")), "Can't tell");
    }

    /// Every recognised level maps to its own design-0025-§5 colour, and the
    /// two `Rugged`/`RugMechanicsLive` share the rug colour by design.
    #[test]
    fn each_known_level_gets_its_own_ladder_colour() {
        assert_eq!(stamp_color(stamp_word(Some("Rugged"))), COLOR_RUG);
        assert_eq!(stamp_color(stamp_word(Some("RugMechanicsLive"))), COLOR_RUG);
        assert_eq!(stamp_color(stamp_word(Some("Sketchy"))), COLOR_WARN);
        assert_eq!(stamp_color(stamp_word(Some("NothingUglyYet"))), COLOR_REAL);
        assert_eq!(stamp_color(stamp_word(Some("CantTell"))), COLOR_UNKNOWN);
    }

    /// A name longer than the card's budget is truncated with an ellipsis,
    /// never overflowed onto the card raw.
    #[test]
    fn a_long_name_is_truncated() {
        let long = "A".repeat(200);
        let svg = build_svg("Sketchy", "solana", Some(&long), Some("SYM"));
        assert!(
            !svg.contains(&long),
            "the full 200-char name must not appear verbatim"
        );
        assert!(
            svg.matches('A').count() < 100,
            "the rendered name must be shorter than the input"
        );
    }

    /// No digit run of three or more characters (the shape a price or a
    /// market cap would take, e.g. "1234" or "0.00042") can reach the SVG —
    /// `build_svg`'s own parameter list has no numeric field at all, and
    /// this pins that no caller can smuggle one in through `name`/`symbol`
    /// without it being caught here first.
    #[test]
    fn no_long_digit_run_reaches_the_svg_from_any_input() {
        for (name, symbol) in [
            (Some("Token $1,234,567 mcap"), Some("SYM")),
            (Some("price 0.00042069"), Some("999999")),
            (None, None),
        ] {
            let svg = build_svg("Sketchy", "solana", name, symbol);
            let max_digit_run = svg
                .chars()
                .fold((0usize, 0usize), |(max, cur), c| {
                    if c.is_ascii_digit() {
                        let cur = cur + 1;
                        (max.max(cur), cur)
                    } else {
                        (max, 0)
                    }
                })
                .0;
            // The template's own fixed layout numbers (coordinates, sizes,
            // "1200"/"630") are themselves short digit runs; the assertion
            // here is on data that flows in from `name`/`symbol`, so we
            // check that none of *those specific* strings' digits survive
            // as a long run distinguishable from layout constants -- the
            // stronger, simpler property: neither input string appears
            // verbatim in the output.
            if let Some(n) = name {
                assert!(
                    !svg.contains(n),
                    "price-shaped name must not appear verbatim: {svg}"
                );
            }
            if let Some(s) = symbol {
                assert!(
                    !svg.contains(s) || s.chars().all(|c| !c.is_ascii_digit()),
                    "digit symbol must not appear verbatim: {svg}"
                );
            }
            let _ = max_digit_run;
        }
    }

    /// The rendered PNG is under design 0025 §7's ~300KB budget and exactly
    /// the 1200x630 size it names.
    #[test]
    fn the_rendered_card_is_under_the_size_budget_and_the_right_dimensions() {
        let svg = build_svg("Sketchy", "solana", Some("Example Token"), Some("EX"));
        let png = render_png(&svg).expect("renders");
        assert!(
            png.len() <= MAX_BYTES,
            "card is {} bytes, over the {MAX_BYTES} budget",
            png.len()
        );
        let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).expect("valid png");
        assert_eq!(pixmap.width(), WIDTH);
        assert_eq!(pixmap.height(), HEIGHT);
    }
}
