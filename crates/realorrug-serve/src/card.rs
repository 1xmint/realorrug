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
//! `_name`/`_symbol` keys `check.rs` stashes there on a verdict write. The
//! same cache file carries `_signals`: the fired [`realorrug_roast::sheet::Signal`]s'
//! plain-English phrases, stashed by `check.rs` at write time and read back
//! here to draw under the verdict word.
//!
//! # Why signals draw the "why" lines, never `Verdict::reasons`
//!
//! `realorrug_roast::verdict::level` picks the ladder level from
//! `sheet.signals` alone — nothing else. That makes the fired signals *the
//! reason* the word above them says what it says, so a card built from them
//! can never draw a line that disagrees with its own headline. `reasons` is
//! ordered by "worth reading", not by "what moved the level" (its own doc
//! comment says so), and can lead with a fact — how long ago a token
//! launched, on the token this route was built against — that never fired a
//! signal at all. Numbers stay excluded the same way the rest of this module
//! excludes them: [`realorrug_roast::sheet::Signal::plain`] returns a fixed,
//! digit-free phrase per variant, so the "No price, ever" section below and
//! its pinning test are untouched by this feature.
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
//! [`build_svg`] takes exactly four strings and one string slice — verdict
//! word, chain, name, symbol, and up to three signal-phrase flags — and
//! never a number. There is no code path from a price or market-cap field to
//! this function's parameters. The token's own name and symbol are drawn as
//! written, digits included: "Pepe2024" is a name, and scrubbing digits from
//! it would misquote the token. The flags are [`realorrug_roast::sheet::Signal::plain`]'s
//! own fixed, digit-free phrases, never a fact with a number in it — that is
//! why this route draws *signals*, not the fact sheet's measured values. A
//! test below pins that with every text input free of digits, the only
//! digits in the SVG are the template's own layout numbers, so nothing
//! numeric enters by any other road.
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
    if refused(status) {
        return status.into_response();
    }

    let chain = doc["chain"].as_str().unwrap_or("").to_owned();
    let level = doc["level"].as_str().map(str::to_owned);
    let raw_address = doc["address"].as_str().unwrap_or(&address).to_owned();

    let (name, symbol, signals) = if level.is_some() {
        let key = format!("{chain}:{raw_address}");
        let cache_path = state.cache_dir.join(format!("{}.json", cache_key(&key)));
        fresh_cached_raw(&cache_path, now_secs()).map_or((None, None, Vec::new()), |raw| {
            let signals = raw["_signals"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            (
                raw["_name"].as_str().map(str::to_owned),
                raw["_symbol"].as_str().map(str::to_owned),
                signals,
            )
        })
    } else {
        (None, None, Vec::new())
    };

    let word = stamp_word(level.as_deref());
    let svg = build_svg(word, &chain, name.as_deref(), symbol.as_deref(), &signals);

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
pub(crate) fn stamp_word(level: Option<&str>) -> &'static str {
    match level {
        Some("Rugged") => "Rugged",
        Some("RugMechanicsLive") => "Rug mechanics live",
        Some("Sketchy") => "Sketchy",
        Some("NothingUglyYet") => "Nothing ugly yet",
        _ => "Can't tell",
    }
}

/// The one line to draw when no signal fired: a true statement derived from
/// the verdict word itself, so the card never sits with an empty middle.
///
/// This is the single place the word-to-line rule lives — [`build_svg`]
/// calls this only when `flags` is empty, and nowhere else picks a
/// no-signal line of its own.
fn no_signal_line(word: &str) -> &'static str {
    match word {
        "Rugged" | "Rug mechanics live" => "the mechanics of this token did the harm",
        "Sketchy" => "something about this token did not read clean",
        "Nothing ugly yet" => "nothing ugly in what was read",
        _ => "not enough of it could be read",
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
pub(crate) fn escape_xml(s: &str) -> String {
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
pub(crate) const MAX_NAME_CHARS: usize = 40;
pub(crate) const MAX_SYMBOL_CHARS: usize = 12;
/// The signal/no-signal lines sit at 34px, narrower per character than the
/// 48px name line, so they tolerate more characters across the same card
/// width before truncation is needed.
pub(crate) const MAX_FLAG_CHARS: usize = 60;

/// The flag lines' vertical layout, named because it has to fit between two
/// lines that were already there: the name line's baseline at 320 above, and
/// the chain label's at [`CHAIN_LABEL_Y`] below. Three lines at 56px apart
/// starting at 400 put the last baseline at 512, which is *below* the chain
/// label's cap height -- two strings drawn over each other at x=100 on the
/// one image that gets shared. The test below pins the arithmetic rather
/// than the picture, because nothing renders an SVG in CI and a human
/// reading these four numbers will not do this subtraction.
const FLAG_FIRST_Y: usize = 360;
const FLAG_STEP_Y: usize = 46;
const FLAG_FONT: usize = 34;
const MAX_FLAG_LINES: usize = 3;
/// Baseline of the chain label, the first thing under the flag lines.
const CHAIN_LABEL_Y: usize = HEIGHT as usize - 140;
/// Its font size, used with [`CHAIN_LABEL_Y`] to find where its tallest
/// glyph starts. Cap height is about 0.72 em in the fonts this draws with.
const CHAIN_LABEL_FONT: usize = 30;

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Whether the checker refused rather than answered: rate limit, malformed
/// address, or budget spent. Those pass through as a bare status.
fn refused(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::BAD_REQUEST
        || status == StatusCode::SERVICE_UNAVAILABLE
}

/// The chain's name as a reader knows it; an empty field is said as unknown
/// rather than drawn as a blank line.
fn chain_label(chain: &str) -> &str {
    match chain {
        "solana" => "Solana",
        "robinhood" => "Robinhood Chain",
        other if !other.is_empty() => other,
        _ => "unknown chain",
    }
}

/// Font families named in the card, first match wins.
///
/// Named rather than the bare generic `sans-serif`: `usvg` maps that generic
/// to Arial by default, and neither the server nor CI has Arial, so a card
/// asking only for `sans-serif` rendered with no words on it at all. DejaVu is
/// what the server has; Liberation and Noto cover other Linux hosts.
const FONT_SANS: &str = "'DejaVu Sans', 'Liberation Sans', 'Noto Sans', Arial, sans-serif";
/// The same rule for the monospace chain label.
const FONT_MONO: &str =
    "'DejaVu Sans Mono', 'Liberation Mono', 'Noto Sans Mono', 'Courier New', monospace";

/// Builds the card's SVG. Pure and synchronous: no chain read, no I/O, so it
/// is directly unit-testable for escaping, truncation and the "never a
/// digit-run from a price field" rule.
///
/// `word` is [`stamp_word`]'s output — never a raw `Level` variant name, so
/// this function cannot itself pick the wrong ladder word; that choice is
/// made once, by `stamp_word`, and this function only draws it.
///
/// `flags` is up to three of [`realorrug_roast::sheet::Signal::plain`]'s
/// phrases for the signals that fired on this token — the same signals
/// `verdict::level` used to choose `word`, so these lines can never
/// contradict it (see this module's doc comment). An empty slice draws
/// [`no_signal_line`]'s single true statement instead of nothing.
fn build_svg(
    word: &str,
    chain: &str,
    name: Option<&str>,
    symbol: Option<&str>,
    flags: &[String],
) -> String {
    let color = stamp_color(word);
    let name_text = name.map(|n| escape_xml(&truncate(n, MAX_NAME_CHARS)));
    let symbol_text = symbol.map(|s| escape_xml(&truncate(s, MAX_SYMBOL_CHARS)));
    let chain_label = escape_xml(chain_label(chain));

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
        r#"<text x="100" y="220" font-family="{FONT_SANS}" font-size="96" font-weight="700" fill="{color}">{word}</text>"#,
        word = escape_xml(word),
    );
    if let (Some(name_text), Some(symbol_text)) = (&name_text, &symbol_text) {
        let _ = write!(
            body,
            r#"<text x="100" y="320" font-family="{FONT_SANS}" font-size="48" fill="{COLOR_PAPER_INK}">{name_text} ({symbol_text})</text>"#,
        );
    } else if let Some(name_text) = &name_text {
        let _ = write!(
            body,
            r#"<text x="100" y="320" font-family="{FONT_SANS}" font-size="48" fill="{COLOR_PAPER_INK}">{name_text}</text>"#,
        );
    }
    let lines: Vec<String> = if flags.is_empty() {
        vec![no_signal_line(word).to_owned()]
    } else {
        flags.iter().take(MAX_FLAG_LINES).cloned().collect()
    };
    for (i, line) in lines.iter().enumerate() {
        let text = escape_xml(&truncate(line, MAX_FLAG_CHARS));
        let y = FLAG_FIRST_Y + i * FLAG_STEP_Y;
        let _ = write!(
            body,
            r#"<text x="100" y="{y}" font-family="{FONT_SANS}" font-size="{FLAG_FONT}" fill="{COLOR_PAPER_INK}">{text}</text>"#,
        );
    }
    let _ = write!(
        body,
        r#"<text x="100" y="{CHAIN_LABEL_Y}" font-family="{FONT_MONO}" font-size="{CHAIN_LABEL_FONT}" fill="{COLOR_PAPER_INK}">{chain_label}</text>"#,
    );
    let _ = write!(
        body,
        r#"<text x="{x}" y="{y}" font-family="{FONT_SANS}" font-size="26" font-weight="700" fill="{COLOR_GOLD}" text-anchor="end">REALORRUG</text>"#,
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
            &[],
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

        let svg = build_svg(word, "solana", None, None, &[]);
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
        let svg = build_svg("Sketchy", "solana", Some(&long), Some("SYM"), &[]);
        assert!(
            !svg.contains(&long),
            "the full 200-char name must not appear verbatim"
        );
        assert!(
            svg.matches('A').count() < 100,
            "the rendered name must be shorter than the input"
        );
    }

    /// The template adds no number of its own beyond layout. With name,
    /// symbol, word and chain all digit-free, every digit in the SVG sits in
    /// a fixed layout attribute, so a price could only arrive through a
    /// parameter, and `build_svg` has no numeric one. A `{price}` slipped
    /// into the template's visible text fails this.
    #[test]
    fn the_only_digits_on_a_digit_free_card_are_layout_numbers() {
        let flags = vec![
            "one address holds most of the supply".to_owned(),
            "a test sell into this token failed".to_owned(),
        ];
        let svg = build_svg(
            "Sketchy",
            "robinhood",
            Some("Pepe Coin"),
            Some("PEPE"),
            &flags,
        );
        let visible: String = svg
            .split('>')
            .filter_map(|chunk| chunk.split('<').next())
            .collect();
        assert!(
            !visible.chars().any(|c| c.is_ascii_digit()),
            "visible card text carries a digit the inputs did not: {visible}"
        );
    }

    /// Two fired signals both land on the card as their `plain()` text.
    #[test]
    fn two_flags_both_appear_on_the_card() {
        let flags = vec![
            "one address holds most of the supply".to_owned(),
            "a test sell into this token failed".to_owned(),
        ];
        let svg = build_svg("Sketchy", "solana", None, None, &flags);
        assert!(
            svg.contains("one address holds most of the supply"),
            "{svg}"
        );
        assert!(svg.contains("a test sell into this token failed"), "{svg}");
    }

    /// Four fired signals draw only the first three — the card has room for
    /// three lines, never a fourth spilling past the card's edge.
    #[test]
    fn four_flags_draw_only_three() {
        let flags = vec![
            "flag one is drawn".to_owned(),
            "flag two is drawn".to_owned(),
            "flag three is drawn".to_owned(),
            "flag four is dropped".to_owned(),
        ];
        let svg = build_svg("Sketchy", "solana", None, None, &flags);
        assert!(svg.contains("flag one is drawn"));
        assert!(svg.contains("flag two is drawn"));
        assert!(svg.contains("flag three is drawn"));
        assert!(
            !svg.contains("flag four is dropped"),
            "a fourth flag must never reach the card: {svg}"
        );
    }

    /// With no signal fired, the card still says something true rather than
    /// leaving the middle empty — one line derived from the verdict word via
    /// [`no_signal_line`], the single place that mapping lives.
    #[test]
    fn no_flags_draws_the_levels_own_line() {
        let svg = build_svg("Nothing ugly yet", "solana", None, None, &[]);
        assert!(
            svg.contains("nothing ugly in what was read"),
            "the no-signal fallback line for this word must be drawn: {svg}"
        );

        let svg = build_svg("Can't tell", "solana", None, None, &[]);
        assert!(
            svg.contains("not enough of it could be read"),
            "the no-signal fallback line for this word must be drawn: {svg}"
        );
    }

    #[test]
    fn a_full_three_flag_card_leaves_the_chain_label_its_own_space() {
        // The first version of this feature drew flags at 400, 456 and 512
        // while the chain label sat at 490, so a three-flag card -- the loud
        // one, the one worth sharing -- printed two strings over each other
        // at x=100. Nothing in CI renders an SVG, so this is the only place
        // that catches it. Re-apply the bug by setting FLAG_FIRST_Y to 400
        // and FLAG_STEP_Y to 56.
        //
        // A baseline is where the letters sit; descenders (the tail of a
        // "g") hang about 0.22 em below it, and capitals rise about 0.72 em
        // above. So the lowest ink of the last flag line must stay above the
        // highest ink of the chain label.
        let last_flag_baseline = FLAG_FIRST_Y + (MAX_FLAG_LINES - 1) * FLAG_STEP_Y;
        let flag_ink_bottom = last_flag_baseline + FLAG_FONT * 22 / 100;
        let label_ink_top = CHAIN_LABEL_Y - CHAIN_LABEL_FONT * 72 / 100;
        assert!(
            flag_ink_bottom < label_ink_top,
            "flag line {MAX_FLAG_LINES} ends at y={flag_ink_bottom} but the chain \
             label starts at y={label_ink_top}: they would overlap on the card"
        );

        // And the first flag line must clear the name line's own descenders.
        let name_ink_bottom = 320 + 48 * 22 / 100;
        let first_flag_ink_top = FLAG_FIRST_Y - FLAG_FONT * 72 / 100;
        assert!(
            first_flag_ink_top > name_ink_bottom,
            "the first flag line starts at y={first_flag_ink_top}, into the name \
             line's descenders at y={name_ink_bottom}"
        );
    }

    #[test]
    fn only_the_three_refusals_skip_the_card() {
        assert!(refused(StatusCode::TOO_MANY_REQUESTS));
        assert!(refused(StatusCode::BAD_REQUEST));
        assert!(refused(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!refused(StatusCode::OK));
        assert!(!refused(StatusCode::NOT_FOUND));
    }

    #[test]
    fn each_chain_is_named_as_a_reader_knows_it() {
        assert_eq!(chain_label("solana"), "Solana");
        assert_eq!(chain_label("robinhood"), "Robinhood Chain");
        assert_eq!(chain_label("base"), "base");
        assert_eq!(chain_label(""), "unknown chain");
    }

    /// The card's words are drawn from the host's fonts. Without them the
    /// PNG is the same whatever the verdict says, so two different words
    /// must give two different images. Skipped on a host with no fonts,
    /// where there is nothing to draw with.
    #[test]
    fn the_verdict_word_is_actually_drawn() {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        if db.is_empty() {
            return;
        }
        let a = render_png(&build_svg("Sketchy", "robinhood", None, None, &[])).expect("render");
        let b = render_png(&build_svg("Rugged", "robinhood", None, None, &[])).expect("render");
        assert_ne!(a, b, "the stamp word left no mark on the image");
    }

    #[test]
    fn the_rendered_card_is_under_the_size_budget_and_the_right_dimensions() {
        let svg = build_svg("Sketchy", "solana", Some("Example Token"), Some("EX"), &[]);
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
