// SPDX-License-Identifier: Apache-2.0
//! `realorrug narratives` — what recent launches are calling themselves.
//!
//! Reads only the rows the serving routes already wrote (`token_texts`),
//! counts the words they share, and prints them. **No chain read, no outside
//! service, no model**: running this costs nothing and can be run as often as
//! an operator likes.
//!
//! It is an operator's command and nothing publishes its output. A theme is a
//! count of names, and what a paid customer should be told about one is an
//! open question (research 0055 §3); printing it here first is how we find
//! out whether the counts are worth anything before anyone is charged for
//! them.

use std::fmt::Write as _;
use std::time::{Duration, SystemTime};

use realorrug_onchain::memory::Memory;
use realorrug_onchain::narrative::{self, Theme};

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
    print!(
        "{}",
        report(&narrative::themes(&texts, min), texts.len(), days, min, top)
    );
    Ok(())
}

/// The printed page.
///
/// Split from [`run`] so the wording is testable without a database: the
/// reading is on one side of this boundary and the counting on the other.
fn report(themes: &[Theme], launches: usize, days: u64, min: usize, top: usize) -> String {
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
            "{:>4}  {:<24} {}.{:02}%",
            theme.tokens.len(),
            theme.term,
            theme.share_bps / 100,
            theme.share_bps % 100
        );
    }
    if themes.len() > top {
        let _ = writeln!(out, "... and {} more", themes.len() - top);
    }
    out
}

/// A numeric flag, ignoring one that will not parse.
fn number(args: &[String], name: &str) -> Option<u64> {
    flag(args, name).and_then(|v| v.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use realorrug_onchain::memory::TokenText;

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
        let page = report(&[theme("neuro", 3, 1_500)], 20, 7, 3, 20);
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
        let page = report(&[], 40, 7, 3, 20);
        assert!(page.contains("no word is shared by 3 or more"), "{page}");
    }

    /// The list is capped, and the cap is visible rather than silent.
    #[test]
    fn the_list_is_capped_and_says_what_it_left_out() {
        let themes: Vec<Theme> = (0..5).map(|i| theme(&format!("t{i}"), 3, 100)).collect();
        let page = report(&themes, 50, 7, 3, 2);
        assert!(page.contains("... and 3 more"), "{page}");
        assert!(!page.contains("t4"), "{page}");
    }

    /// A cap no smaller than the list prints no footer.
    #[test]
    fn nothing_is_left_out_when_the_list_fits() {
        let page = report(&[theme("neuro", 3, 100)], 50, 7, 3, 20);
        assert!(!page.contains("more"), "{page}");
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
}
