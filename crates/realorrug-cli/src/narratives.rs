// SPDX-License-Identifier: Apache-2.0
//! `realorrug narratives` — what recent launches are calling themselves.
//!
//! Reads only the rows the serving routes already wrote (`token_texts`),
//! counts the words they share, and prints them. **No chain read, no outside
//! service, no model**: running this costs nothing and can be run as often as
//! an operator likes.
//!
//! `--by-volume` orders the list by what the theme's launches traded rather
//! than by how many launchers picked the word, because a word ten launchers
//! happened to share and nobody trades is a coincidence.
//!
//! It is an operator's command and nothing publishes its output. A theme is a
//! count of names, and what a paid customer should be told about one is an
//! open question (research 0055 §3); printing it here first is how we find
//! out whether the counts are worth anything before anyone is charged for
//! them.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{Duration, SystemTime};

use realorrug_onchain::memory::{MarketRead, Memory};
use realorrug_onchain::narrative::{self, Theme, Trading};

use crate::flag;

/// The only chain whose launches carry names worth counting today.
const CHAIN: &str = "robinhood";

/// How far back to look when `--days` is not given.
///
/// Seven, matching the window the creator index joins on: long enough that a
/// theme forming over a few days shows up, short enough that last month's
/// theme does not drown this week's.
const DEFAULT_DAYS: u64 = 7;

/// How many launches must share a word before it is called a theme.
///
/// Three, the smallest number that is not a coincidence between two
/// launchers. A caller who wants the long tail passes `--min 2`.
const DEFAULT_MIN: usize = 3;

/// How many themes to print.
const DEFAULT_TOP: usize = 20;

/// Prints the themes in the window.
///
/// # Errors
///
/// A message naming the file when the memory cannot be opened or read.
pub fn run(args: &[String]) -> Result<(), String> {
    let path = flag(args, "--memory").unwrap_or_else(|| {
        let dir = std::env::var("REALORRUG_ANALYST_DIR")
            .or_else(|_| std::env::var("RADAR_ANALYST_DIR"))
            .unwrap_or_else(|_| "data/analyst".to_owned());
        format!("{dir}/memory.sqlite3")
    });
    let days = number(args, "--days").unwrap_or(DEFAULT_DAYS);
    let min =
        usize::try_from(number(args, "--min").unwrap_or(DEFAULT_MIN as u64)).unwrap_or(DEFAULT_MIN);
    let top =
        usize::try_from(number(args, "--top").unwrap_or(DEFAULT_TOP as u64)).unwrap_or(DEFAULT_TOP);

    let memory = Memory::open(std::path::Path::new(&path))
        .map_err(|e| format!("cannot open {path}: {e}"))?;
    let since = SystemTime::now()
        .checked_sub(Duration::from_secs(days.saturating_mul(86_400)))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let texts = memory
        .token_texts_since(CHAIN, since)
        .map_err(|e| format!("cannot read the names: {e}"))?;
    let latest = memory
        .latest_market_since(CHAIN, since)
        .map_err(|e| format!("cannot read the market rows: {e}"))?;
    let asked = memory
        .mention_terms_since(since)
        .map_err(|e| format!("cannot read the mention words: {e}"))?;
    let mut themes = narrative::themes(&texts, min);
    if has(args, "--by-volume") {
        by_volume(&mut themes, &latest);
    }
    print!(
        "{}",
        report(&themes, &latest, &asked, texts.len(), days, min, top)
    );
    Ok(())
}

/// The printed page.
///
/// Split from [`run`] so the wording is testable without a database: the
/// reading is on one side of this boundary and the counting on the other.
fn report(
    themes: &[Theme],
    latest: &BTreeMap<String, MarketRead>,
    asked: &BTreeMap<String, usize>,
    launches: usize,
    days: u64,
    min: usize,
    top: usize,
) -> String {
    let mut out = format!("{launches} launch(es) named in the last {days} day(s)\n");
    if themes.is_empty() {
        // Said plainly, because "no themes" is a real answer and not a
        // failure: a week where every launcher chose a different word is a
        // week with no theme in it (rule 8 -- absent is not zero, so the
        // number of launches read is printed above either way).
        let _ = writeln!(out, "no word is shared by {min} or more of them");
        return out;
    }
    for theme in themes.iter().take(top) {
        let _ = writeln!(
            out,
            "{:>4}  {:<24} {}.{:02}%  {:<28}  {}",
            theme.tokens.len(),
            theme.term,
            theme.share_bps / 100,
            theme.share_bps % 100,
            traded(&narrative::trading(theme, latest), theme.tokens.len()),
            asked_about(asked.get(&theme.term).copied())
        );
    }
    if themes.len() > top {
        let _ = writeln!(out, "... and {} more", themes.len() - top);
    }
    out
}

/// Reorders themes by what their launches traded, biggest first.
///
/// A different question from the default order, not a better one: the count
/// asks how many launchers picked a word, and this asks whether anybody is
/// buying what they launched. A theme nobody priced sorts last, because it
/// is the one we know least about, and ties break alphabetically so two runs
/// over the same rows print the same list.
fn by_volume(themes: &mut [Theme], latest: &BTreeMap<String, MarketRead>) {
    themes.sort_by(|a, b| {
        narrative::trading(b, latest)
            .volume_24h_usd
            .total_cmp(&narrative::trading(a, latest).volume_24h_usd)
            .then_with(|| a.term.cmp(&b.term))
    });
}

/// The dollar column, which always says how much of the theme it covers.
///
/// "$8,200 (2 of 11 priced)" and "$8,200" are different claims, and only the
/// first is true: the aggregator answers for a token with a pool, so a brand
/// new theme is mostly launches nobody has priced yet. Printing the bare
/// total would quietly turn "two of these traded" into "these traded".
fn traded(trading: &Trading, tokens: usize) -> String {
    if trading.tokens_priced == 0 {
        return "no reading".to_owned();
    }
    format!(
        "${:.0} ({} of {} priced)",
        trading.volume_24h_usd, trading.tokens_priced, tokens
    )
}

/// Whether a bare flag was passed.
///
/// Split from [`run`] for the same reason as [`report`]: the comparison here
/// is the whole of `--by-volume`, and inside `run` no test could reach it
/// without a database.
fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

/// The asking column: how many different people used the word in a question.
///
/// A word no one has asked about prints "nobody asked", never "0 people":
/// the bot only hears from people who mention it, so an empty count is a
/// thing we did not hear, not a thing that did not happen (rule 8). The two
/// numbers on a row answer different questions -- the launches column is
/// what launchers did, and this is what everybody else noticed -- and a
/// theme that is high in one and low in the other is the interesting row.
fn asked_about(people: Option<usize>) -> String {
    match people {
        None | Some(0) => "nobody asked".to_owned(),
        Some(1) => "1 person asked".to_owned(),
        Some(n) => format!("{n} people asked"),
    }
}

/// A numeric flag, ignoring one that will not parse.
fn number(args: &[String], name: &str) -> Option<u64> {
    flag(args, name).and_then(|v| v.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use realorrug_onchain::memory::TokenText;

    fn nobody() -> BTreeMap<String, usize> {
        BTreeMap::new()
    }

    fn no_market() -> BTreeMap<String, MarketRead> {
        BTreeMap::new()
    }

    fn priced(tokens: &[(&str, f64)]) -> BTreeMap<String, MarketRead> {
        tokens
            .iter()
            .map(|(token, volume)| {
                (
                    (*token).to_owned(),
                    MarketRead {
                        chain: CHAIN.to_owned(),
                        token: (*token).to_owned(),
                        volume_24h_usd: Some(*volume),
                        liquidity_usd: None,
                        price_usd: None,
                        source: "DexScreener".to_owned(),
                        observed_at: SystemTime::UNIX_EPOCH,
                    },
                )
            })
            .collect()
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn theme(term: &str, count: usize, share_bps: u32) -> Theme {
        Theme {
            term: term.to_owned(),
            tokens: (0..count).map(|i| format!("0x{i}")).collect(),
            share_bps,
        }
    }

    /// The page leads with how many launches it looked at, so a small number
    /// of themes read off a tiny window cannot look like a finding.
    #[test]
    fn the_page_says_how_many_launches_it_counted() {
        let page = report(
            &[theme("neuro", 3, 1_500)],
            &no_market(),
            &nobody(),
            20,
            7,
            3,
            20,
        );
        assert!(
            page.starts_with("20 launch(es) named in the last 7 day(s)\n"),
            "{page}"
        );
        assert!(page.contains("neuro"), "{page}");
        assert!(page.contains("15.00%"), "{page}");
    }

    /// A week with no shared word says so, rather than printing nothing.
    #[test]
    fn a_week_with_no_theme_says_so() {
        let page = report(&[], &no_market(), &nobody(), 40, 7, 3, 20);
        assert!(page.contains("no word is shared by 3 or more"), "{page}");
    }

    /// The list is capped, and the cap is visible rather than silent.
    #[test]
    fn the_list_is_capped_and_says_what_it_left_out() {
        let themes: Vec<Theme> = (0..5).map(|i| theme(&format!("t{i}"), 3, 100)).collect();
        let page = report(&themes, &no_market(), &nobody(), 50, 7, 3, 2);
        assert!(page.contains("... and 3 more"), "{page}");
        assert!(!page.contains("t4"), "{page}");
    }

    /// A cap no smaller than the list prints no footer.
    ///
    /// The exactly-full case is here too, because a list of three under a cap
    /// of three leaves nothing out, and a footer saying "and 0 more" would be
    /// a lie in the one place a reader is most likely to look.
    #[test]
    fn nothing_is_left_out_when_the_list_fits() {
        let page = report(
            &[theme("neuro", 3, 100)],
            &no_market(),
            &nobody(),
            50,
            7,
            3,
            20,
        );
        assert!(!page.contains("more"), "{page}");

        let themes: Vec<Theme> = (0..3).map(|i| theme(&format!("t{i}"), 3, 100)).collect();
        let exactly_full = report(&themes, &no_market(), &nobody(), 50, 7, 3, 3);
        assert!(!exactly_full.contains("more"), "{exactly_full}");
        assert!(exactly_full.contains("t2"), "{exactly_full}");
    }

    /// The dollar column says how much of the theme it covers, so a total
    /// resting on two of eleven launches cannot read as all eleven.
    #[test]
    fn the_dollar_column_says_how_many_launches_it_covers() {
        let page = report(
            &[theme("neuro", 3, 1_500)],
            &priced(&[("0x0", 8_000.0), ("0x1", 200.0)]),
            &nobody(),
            20,
            7,
            3,
            20,
        );
        assert!(page.contains("$8200 (2 of 3 priced)"), "{page}");
    }

    /// A theme nobody priced says so rather than printing a zero.
    #[test]
    fn an_unpriced_theme_says_no_reading() {
        let page = report(
            &[theme("neuro", 3, 1_500)],
            &no_market(),
            &nobody(),
            20,
            7,
            3,
            20,
        );
        assert!(page.contains("no reading"), "{page}");
        assert!(!page.contains("$0"), "{page}");
    }

    /// Ordering by dollars puts the traded theme above the merely popular
    /// one, and the theme nobody priced last.
    #[test]
    fn by_volume_puts_the_traded_theme_first() {
        let mut themes = vec![
            Theme {
                term: "popular".to_owned(),
                tokens: vec!["0xa".to_owned(), "0xb".to_owned(), "0xc".to_owned()],
                share_bps: 3_000,
            },
            Theme {
                term: "traded".to_owned(),
                tokens: vec!["0xd".to_owned()],
                share_bps: 1_000,
            },
            Theme {
                term: "unpriced".to_owned(),
                tokens: vec!["0xz".to_owned()],
                share_bps: 1_000,
            },
        ];
        by_volume(
            &mut themes,
            &priced(&[("0xa", 10.0), ("0xb", 10.0), ("0xd", 900.0)]),
        );
        let order: Vec<&str> = themes.iter().map(|t| t.term.as_str()).collect();
        assert_eq!(order, vec!["traded", "popular", "unpriced"]);
    }

    /// A flag that will not parse is ignored rather than failing the run.
    #[test]
    fn an_unparsable_number_is_ignored() {
        assert_eq!(number(&args(&["--days", "seven"]), "--days"), None);
        assert_eq!(number(&args(&["--days", "3"]), "--days"), Some(3));
    }

    /// End to end over a real file: names written by the serving routes come
    /// back as a theme.
    #[test]
    fn names_written_to_a_memory_come_back_as_a_theme() {
        let dir = std::env::temp_dir().join("realorrug-narratives");
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("memory.sqlite3");
        let _ = std::fs::remove_file(&path);
        let memory = Memory::open(&path).expect("open");
        for (i, name) in ["Neuro Dog", "Neuro Cat", "Neuro Girl", "Plain"]
            .iter()
            .enumerate()
        {
            memory
                .record_token_text(&TokenText {
                    chain: CHAIN.to_owned(),
                    token: format!("0x{i}"),
                    name: Some((*name).to_owned()),
                    symbol: None,
                    first_seen: SystemTime::now(),
                })
                .expect("record");
        }
        let args = args(&["--memory", path.to_str().expect("path")]);
        run(&args).expect("run");

        let texts = memory
            .token_texts_since(CHAIN, SystemTime::UNIX_EPOCH)
            .expect("read");
        let found = narrative::themes(&texts, 3);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].term, "neuro");
    }

    /// A memory file that is not there names the file it tried.
    #[test]
    fn a_missing_memory_file_names_itself() {
        let path = std::env::temp_dir()
            .join("realorrug-narratives-missing")
            .join("memory.sqlite3");
        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
        let err = run(&args(&["--memory", path.to_str().expect("path")])).expect_err("missing");
        assert!(err.starts_with("cannot open "), "{err}");
    }

    /// A flag counts only when it is the flag asked for. The wrong-name case
    /// is the point: a comparison that answered "yes" to every other argument
    /// would sort by dollars whenever anything at all was passed.
    #[test]
    fn a_bare_flag_is_seen_only_when_it_is_there() {
        assert!(has(&args(&["--by-volume"]), "--by-volume"));
        assert!(!has(&args(&["--days", "7"]), "--by-volume"));
        assert!(!has(&args(&[]), "--by-volume"));
    }

    /// The asking column counts people, and says so in words rather than
    /// printing a bare number that could be read as launches.
    #[test]
    fn the_asking_column_counts_people() {
        let asked = [("neuro".to_owned(), 4)].into_iter().collect();
        let page = report(
            &[theme("neuro", 3, 1_500)],
            &no_market(),
            &asked,
            20,
            7,
            3,
            20,
        );
        assert!(page.contains("4 people asked"), "{page}");
    }

    /// One person is one person, not "1 people", and a word nobody used says
    /// nobody asked rather than zero -- the bot only hears from people who
    /// mention it, so an empty count is silence, not absence.
    #[test]
    fn one_person_is_one_person_and_none_is_silence() {
        assert_eq!(asked_about(Some(1)), "1 person asked");
        assert_eq!(asked_about(Some(2)), "2 people asked");
        assert_eq!(asked_about(None), "nobody asked");
        assert_eq!(asked_about(Some(0)), "nobody asked");
    }
}
