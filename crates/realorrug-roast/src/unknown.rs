// SPDX-License-Identifier: Apache-2.0
//! The unknown-data check: a gap is never reassurance, and the risk index is
//! never a probability (plan 0002 phase 2, unit 4).
//!
//! [`report.rs`] already refuses to compute a level from an absence -- an
//! unread fact stays [`crate::verdict::Level::CantTell`] or a plain line in
//! `Report::missing`, never a score. This module refuses the same bug in
//! *words*: a sentence that names an unknown, missing or skipped fact and
//! then calls it safe, clean, fine, free of risk or nothing to worry about,
//! and a sentence that turns the 0-100 risk index into a probability, a
//! chance or odds -- the exact inversion `report.rs`'s own doc comment
//! warns about ("a sheet with unread holders being read as 'fine' because
//! nothing on the page said otherwise").
//!
//! Unlike [`crate::forbidden::words_refused_at`], this runs at every level:
//! a gap is not a stronger claim at `CantTell` and a weaker one at
//! `NothingUglyYet` -- it is the same absence, and reassuring about it is
//! the same bug, regardless of what else the sheet shows.

/// A phrase found where it may not be, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Violation {
    /// The reassuring or probability-shaped phrase found.
    pub phrase: &'static str,
    /// Why it is refused.
    pub because: &'static str,
}

/// Words that name a gap: an unread, unavailable or intentionally-unread
/// fact. Matched case-insensitively, the same as [`REASSURANCE`] and
/// [`PROBABILITY`] below.
const GAP_WORDS: &[&str] = &[
    "unknown",
    "not known",
    "missing",
    "skipped",
    "not read",
    "not available",
    "no data",
    "unread",
    "unavailable",
    "could not be read",
    "couldn't be read",
];

/// Reassurance a gap must never earn -- the sentence this exists to catch is
/// "holder data is unknown, but the launch looks safe", where the absence
/// itself is doing the reassuring.
const REASSURANCE: &[(&str, &str)] = &[
    ("safe", "an unread fact is a gap, never a reassurance"),
    ("clean", "an unread fact is a gap, never a reassurance"),
    ("fine", "an unread fact is a gap, never a reassurance"),
    ("no risk", "an unread fact is a gap, never a reassurance"),
    ("low risk", "an unread fact is a gap, never a reassurance"),
    (
        "nothing to worry about",
        "an unread fact is a gap, never a reassurance",
    ),
    (
        "nothing to worry",
        "an unread fact is a gap, never a reassurance",
    ),
    ("all clear", "an unread fact is a gap, never a reassurance"),
];

/// Words that turn the risk index into a forecast rather than the 0-100
/// count it is -- the same distinction `report.rs::render`'s trailing line
/// states in prose; this is the check that keeps it true.
const PROBABILITY: &[(&str, &str)] = &[
    (
        "probability",
        "the risk index is a count of what fired, never a probability",
    ),
    (
        "chance",
        "the risk index is a count of what fired, never a chance of anything",
    ),
    (
        "odds",
        "the risk index is a count of what fired, never odds of anything",
    ),
    (
        "likelihood",
        "the risk index is a count of what fired, never a likelihood",
    ),
];

/// Splits text into roughly-sentence windows, the same discipline
/// [`crate::forbidden::sentences`] uses and for the same reason: a
/// reassurance about a gap named two sentences away is not this bug, and a
/// real parser would be a second thing to get wrong beside the check.
fn sentences(text: &str) -> impl Iterator<Item = &str> {
    text.split(['.', '!', '?', '\n'])
}

/// Every place `text` names a gap and reassures about it in the same
/// sentence, or turns the risk index into a probability, a chance or odds,
/// anywhere in `text`.
///
/// Run on both the reply and the report (`realorrug replay`'s `review.md`):
/// neither is scored by this module, only read for the words above.
#[must_use]
pub fn check(text: &str) -> Vec<Violation> {
    let lower = text.to_lowercase();
    let mut violations = Vec::new();

    for sentence in sentences(&lower) {
        if GAP_WORDS.iter().any(|g| sentence.contains(g)) {
            for &(phrase, because) in REASSURANCE {
                if sentence.contains(phrase) {
                    violations.push(Violation { phrase, because });
                }
            }
        }
    }

    for &(phrase, because) in PROBABILITY {
        if lower.contains(phrase) {
            violations.push(Violation { phrase, because });
        }
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gap_reassured_about_in_the_same_sentence_is_a_violation() {
        let v = check("Holder data is unknown, but the launch looks safe.");
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].phrase, "safe");
    }

    /// The bug this module exists to keep from recurring, restated exactly:
    /// `report.rs`'s own doc comment names it as "a sheet with unread holders
    /// being read as 'fine' because nothing on the page said otherwise".
    /// This re-applies that bug and checks it is caught.
    #[test]
    fn the_unread_holders_read_as_fine_bug_re_applied() {
        let v = check(
            "Creator cash flow could not be read, so nothing on this sheet says otherwise -- \
             it's fine.",
        );
        assert!(v.iter().any(|x| x.phrase == "fine"), "{v:?}");
    }

    #[test]
    fn a_gap_named_with_no_reassurance_is_not_a_violation() {
        let v = check("Holder data is unknown. The largest wallet holds 40% of supply.");
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn reassurance_with_no_gap_named_is_not_this_checks_job() {
        // Calling a *measured* fact safe is `forbidden.rs`'s level-scoped
        // ceiling, not this check's: this module only fires when a gap and a
        // reassurance share a sentence.
        let v = check("The largest holder owns 2% of supply, which is fine.");
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn the_risk_index_called_a_probability_is_a_violation() {
        let v = check("risk index 40/100 -- a 40% probability of a rug");
        assert!(v.iter().any(|x| x.phrase == "probability"), "{v:?}");
    }

    #[test]
    fn the_risk_index_stated_plainly_is_not_a_violation() {
        // `report.rs::render`'s own trailing line, verbatim: it states the
        // count and disclaims it as not the published level without ever
        // writing "probability", "chance", "odds" or "likelihood" -- because
        // this checker, like `forbidden::check`, is deliberately blunt about
        // substrings rather than trying to read a negation, so the safest
        // sentence is the one that never spells the banned word at all.
        let v = check(
            "(risk index 40/100, coverage 8/10 facts read -- a count of what fired, not the \
             published level and not a forecast)",
        );
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn nothing_but_gap_words_and_ordinary_prose_is_clean() {
        assert!(check("").is_empty());
        assert!(check("This launch graduated three days ago.").is_empty());
    }
}
