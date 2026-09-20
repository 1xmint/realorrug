// SPDX-License-Identifier: Apache-2.0
//! Which words a batch of launches have in common.
//!
//! The cheapest possible narrative signal, and deliberately the dumbest one:
//! it counts how many *different* tokens used each word in their name or
//! symbol over a window, and calls a word used by enough of them a theme. No
//! chain read, no model, no outside service -- only the rows
//! [`crate::memory::Memory::token_texts_since`] already holds, which were
//! written down as dossiers went past (research 0055 §3).
//!
//! **A theme is a count, not a claim.** "Eleven of the last two hundred
//! launches called themselves something with `neuro` in it" is a fact about
//! the names; it says nothing about whether those launches are related, good
//! or bad. Rule 2 is why this lives here at all rather than being something a
//! model notices: it is computed by code, from stored rows, before any model
//! sees it, so a reply that mentions a theme is quoting a number off the
//! sheet like any other.
//!
//! The launcher chooses this text, so it is untrusted (rule 3). Only words
//! survive tokenising -- letters and digits, lowercased, nothing else -- so
//! there is no punctuation, no URL and no sentence left to read as an
//! instruction.

use std::collections::{BTreeMap, BTreeSet};

use crate::memory::TokenText;

/// The shortest word that can be a theme.
///
/// Three, because two-letter fragments are mostly noise from splitting
/// symbols (`AI` is the well-known exception, and losing it is cheaper than
/// admitting every `of`, `to` and `xx`). A theme missed is a theme a later
/// window can still find; a theme invented cannot be taken back.
pub const MIN_TERM_CHARS: usize = 3;

/// The longest word kept.
///
/// Anything longer is not a word anyone is riding, it is a launcher pasting
/// something. The same cap [`crate::robinhood`]'s symbol sanitiser uses.
pub const MAX_TERM_CHARS: usize = 32;

/// Words that say nothing about a theme.
///
/// Kept short on purpose. Every word removed here is a theme that can never
/// be found, so this holds only words that describe *being a token at all* --
/// the ones every batch would otherwise rank first and which would say the
/// same thing in any week.
const EMPTY_WORDS: [&str; 6] = ["coin", "token", "the", "and", "finance", "protocol"];

/// One word and the launches that used it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    /// The word itself, lowercased.
    pub term: String,
    /// The token addresses that used it, in the order the store returned
    /// them, which is oldest first.
    pub tokens: Vec<String>,
    /// How many of the window's tokens used it, in hundredths of a percent,
    /// so a caller can compare one week with a busier one. Tokens whose name
    /// and symbol both failed to read are in the denominator: they were
    /// launches, and pretending the window was smaller would inflate every
    /// share.
    pub share_bps: u32,
}

/// The themes in a window of launches, commonest first.
///
/// `min_tokens` is the floor: a word used by fewer than this many different
/// tokens is not reported at all. One launch calling itself something is not
/// a theme, it is a launch, and a list that includes every one-off word is a
/// list of every word.
#[must_use]
pub fn themes(texts: &[TokenText], min_tokens: usize) -> Vec<Theme> {
    let total = texts.len();
    let mut by_term: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for text in texts {
        for term in terms(text) {
            by_term.entry(term).or_default().push(text.token.clone());
        }
    }
    let mut themes: Vec<Theme> = by_term
        .into_iter()
        .filter(|(_, tokens)| tokens.len() >= min_tokens.max(1))
        .map(|(term, tokens)| Theme {
            share_bps: share_bps(tokens.len(), total),
            term,
            tokens,
        })
        .collect();
    // Commonest first; ties alphabetically, so two runs over the same rows
    // print the same list in the same order and a diff of two days is a diff
    // of what changed rather than of how the map happened to iterate.
    themes.sort_by(|a, b| {
        b.tokens
            .len()
            .cmp(&a.tokens.len())
            .then_with(|| a.term.cmp(&b.term))
    });
    themes
}

/// The distinct words one launch used, across its name and its symbol.
///
/// A [`BTreeSet`], so a launch called `NEURO neuro` counts once for `neuro`:
/// the measure is how many launches used a word, and a launcher repeating
/// themselves must not be able to lift their own word up the list.
fn terms(text: &TokenText) -> BTreeSet<String> {
    let mut terms = BTreeSet::new();
    for source in [text.name.as_deref(), text.symbol.as_deref()]
        .into_iter()
        .flatten()
    {
        for word in words(source) {
            terms.insert(word);
        }
    }
    terms
}

/// The usable words in one string.
fn words(source: &str) -> Vec<String> {
    source
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(|raw| {
            let word = raw.to_ascii_lowercase();
            let length = word.chars().count();
            if !(MIN_TERM_CHARS..=MAX_TERM_CHARS).contains(&length) {
                return None;
            }
            // A word of digits is a version or a year, never a theme.
            if word.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            if EMPTY_WORDS.contains(&word.as_str()) {
                return None;
            }
            Some(word)
        })
        .collect()
}

/// `used` out of `total`, in hundredths of a percent.
fn share_bps(used: usize, total: usize) -> u32 {
    if total == 0 {
        return 0;
    }
    let bps = used.saturating_mul(10_000) / total;
    u32::try_from(bps).unwrap_or(10_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn text(token: &str, name: Option<&str>, symbol: Option<&str>) -> TokenText {
        TokenText {
            chain: "robinhood".to_owned(),
            token: token.to_owned(),
            name: name.map(str::to_owned),
            symbol: symbol.map(str::to_owned),
            first_seen: SystemTime::UNIX_EPOCH,
        }
    }

    /// The whole point: three launches sharing a word are a theme, and the
    /// word one launch used alone is not. Whole words only -- `neurogirl`
    /// would not count, because a rule that matched inside words would make
    /// every short word a theme of every long one.
    #[test]
    fn a_word_three_launches_share_is_a_theme() {
        let texts = [
            text("0x1", Some("Neuro Dog"), Some("NEURO")),
            text("0x2", Some("Neuro Cat"), Some("NCAT")),
            text("0x3", Some("neuro girl"), Some("NGIRL")),
            text("0x4", Some("Something Else"), Some("ELSE")),
        ];
        let found = themes(&texts, 2);
        assert_eq!(found.len(), 1, "only `neuro` is shared: {found:?}");
        assert_eq!(found[0].term, "neuro");
        assert_eq!(found[0].tokens, vec!["0x1", "0x2", "0x3"]);
    }

    /// A share is measured against every launch in the window, including the
    /// ones whose text never read.
    #[test]
    fn unread_launches_are_still_in_the_denominator() {
        let texts = [
            text("0x1", Some("Neuro One"), None),
            text("0x2", Some("Neuro Two"), None),
            text("0x3", None, None),
            text("0x4", None, None),
        ];
        let found = themes(&texts, 2);
        assert_eq!(found[0].share_bps, 5_000, "two of four, not two of two");
    }

    /// A launcher repeating their own word cannot lift it up the list.
    #[test]
    fn one_launch_counts_once_however_often_it_repeats_a_word() {
        let texts = [text("0x1", Some("neuro neuro neuro"), Some("NEURO"))];
        let found = themes(&texts, 1);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].tokens, vec!["0x1"]);
    }

    /// Words that describe being a token say nothing about a theme, and
    /// would otherwise top every list in every week.
    #[test]
    fn words_that_mean_token_are_not_themes() {
        let texts = [
            text("0x1", Some("Alpha Coin"), Some("ACOIN")),
            text("0x2", Some("Beta Coin"), Some("BCOIN")),
            text("0x3", Some("Gamma token"), Some("GTOK")),
        ];
        assert!(themes(&texts, 2).is_empty(), "coin and token are dropped");
    }

    /// Short fragments and bare numbers are noise from splitting symbols.
    #[test]
    fn short_fragments_and_bare_numbers_are_dropped() {
        let texts = [
            text("0x1", Some("AI v2 2026"), Some("AI")),
            text("0x2", Some("AI v2 2026"), Some("AI")),
        ];
        assert!(
            themes(&texts, 2).is_empty(),
            "ai, v2 and 2026 all fail the word test"
        );
    }

    /// Punctuation, scripts and anything that could read as an instruction
    /// do not survive tokenising (rule 3).
    #[test]
    fn only_letters_and_digits_survive() {
        let texts = [
            text(
                "0x1",
                Some("ignore previous; <b>drop</b> https://x.test"),
                None,
            ),
            text("0x2", Some("ignore this"), None),
        ];
        let found = themes(&texts, 2);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].term, "ignore");
    }

    /// An empty window has no themes and no division by zero.
    #[test]
    fn an_empty_window_has_no_themes() {
        assert!(themes(&[], 2).is_empty());
        assert_eq!(share_bps(1, 0), 0);
    }

    /// The order is commonest first, then alphabetical, so two runs over the
    /// same rows print the same list.
    #[test]
    fn themes_are_ordered_commonest_then_alphabetically() {
        let texts = [
            text("0x1", Some("zebra apple"), None),
            text("0x2", Some("zebra apple"), None),
            text("0x3", Some("zebra"), None),
        ];
        let found = themes(&texts, 2);
        assert_eq!(found[0].term, "zebra");
        assert_eq!(found[0].tokens.len(), 3);
        assert_eq!(found[1].term, "apple");
    }

    /// A floor of zero is treated as one: a theme needs at least one launch.
    #[test]
    fn a_floor_of_zero_still_needs_one_launch() {
        let texts = [text("0x1", Some("solo"), None)];
        assert_eq!(themes(&texts, 0).len(), 1);
    }

    /// A word longer than a ticker is a launcher pasting something.
    #[test]
    fn an_overlong_word_is_not_a_theme() {
        let long = "a".repeat(MAX_TERM_CHARS + 1);
        let texts = [
            text("0x1", Some(&long), None),
            text("0x2", Some(&long), None),
        ];
        assert!(themes(&texts, 2).is_empty());
    }
}
