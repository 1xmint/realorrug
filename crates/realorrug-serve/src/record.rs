// SPDX-License-Identifier: Apache-2.0
//! Writing down every verdict this server publishes, so it can later be
//! paired with what the token turned out to be.
//!
//! **Nothing reads this yet, and that is the point.** Research 0052 §5's
//! calibration replay (task M-D-0006) needs labelled launches, and the
//! published level stays on the flag rules until it has them. A pair can only
//! be made from a verdict that was written down at the moment it was served;
//! a verdict recomputed later from today's chain state is a different verdict
//! about a different moment, and scoring the model against one would flatter
//! it with facts it did not have. So the record starts now, ahead of the
//! thing that will read it.
//!
//! The record carries the score as well as the level, even though no surface
//! publishes the score (ADR 0036 decision 1 holds it back). Calibration
//! compares score *bands*; a record with only the level could never test
//! whether the weights beat the ladder, which is the whole question M-D-0006
//! is there to settle.
//!
//! Deny by default (`AGENTS.md` §3 rule 7): with no memory path configured,
//! this writes nothing and the route serves exactly as before. A failed write
//! is never allowed to fail a request either -- a buyer who paid for facts
//! got their facts, and our bookkeeping is not their problem.

use std::path::Path;
use std::time::SystemTime;

use realorrug_onchain::memory::{Memory, VerdictRecord};
use realorrug_roast::assessment::Assessment;
use realorrug_roast::sheet::{FactSheet, Signal};
use realorrug_roast::verdict::Level;

/// Writes one published verdict to the memory file, if one is configured.
///
/// `source` names the surface that served it (`"check"`, `"facts"`), so a
/// later fit can hold one surface out -- the free page and the paid endpoint
/// see different tokens for different reasons, and a model fitted on the mix
/// without being able to separate them cannot tell which.
pub(crate) fn verdict(
    memory_path: Option<&Path>,
    chain: &str,
    token: &str,
    sheet: &FactSheet,
    level: &str,
    source: &str,
) {
    let Some(path) = memory_path else {
        return;
    };
    let Ok(memory) = Memory::open(path) else {
        return;
    };
    let assessment = Assessment::from(sheet);
    let _ = memory.record_verdict(&VerdictRecord {
        chain: chain.to_owned(),
        token: token.to_owned(),
        read_at_block: sheet.read_at.and_then(|r| match r {
            realorrug_types::ReadAt::Robinhood(block) => Some(block),
            realorrug_types::ReadAt::Solana(_) => None,
        }),
        level: level.to_owned(),
        score_bps: Some(assessment.score_bps.bps()),
        fired: sheet.signals.iter().copied().map(signal_name).collect(),
        source: source.to_owned(),
        decided_at: SystemTime::now(),
    });
}

/// The stable name of a ladder level.
///
/// This spelling is stored, not only shown: it is the key a later fit groups
/// records by, so renaming one of these strings would silently split one
/// level's history into two. It lives here, next to the record, for that
/// reason -- `check.rs` borrows it for its response so the two can never
/// disagree.
pub(crate) fn level_name(level: Level) -> &'static str {
    match level {
        Level::Rugged => "Rugged",
        Level::RugMechanicsLive => "RugMechanicsLive",
        Level::Sketchy => "Sketchy",
        Level::NothingUglyYet => "NothingUglyYet",
        Level::CantTell => "CantTell",
    }
}

/// The stable name of a signal.
///
/// The serialized spelling, not `Debug` and not [`Signal::plain`]: `plain` is
/// a sentence written for a reader and may be reworded any day without the
/// signal changing at all, which would silently split one signal's history
/// into two. The `serde` name is the same string the wire already uses.
fn signal_name(signal: Signal) -> String {
    serde_json::to_value(signal)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use realorrug_onchain::memory::{Outcome, OutcomeLabel};

    /// The signal's recorded name is its wire spelling, which is what makes a
    /// record from last month comparable with one from today.
    #[test]
    fn a_signal_is_recorded_under_its_wire_name() {
        assert_eq!(
            signal_name(Signal::CreatorBoughtOwnLaunch),
            "creator_bought_own_launch"
        );
    }

    /// The stored spelling of every level, pinned. A rename here is a break
    /// in the record, not a display change, so it has to fail a test.
    #[test]
    fn every_level_has_its_stored_spelling() {
        assert_eq!(level_name(Level::Rugged), "Rugged");
        assert_eq!(level_name(Level::RugMechanicsLive), "RugMechanicsLive");
        assert_eq!(level_name(Level::Sketchy), "Sketchy");
        assert_eq!(level_name(Level::NothingUglyYet), "NothingUglyYet");
        assert_eq!(level_name(Level::CantTell), "CantTell");
    }

    /// No memory path means no write and no panic -- rule 7, and the route
    /// still answers.
    #[test]
    fn no_memory_path_records_nothing() {
        verdict(None, "robinhood", "0xtoken", &sheet(), "Sketchy", "check");
    }

    fn sheet() -> FactSheet {
        FactSheet::build(&dossier(), None, None, None, None)
    }

    fn dossier() -> realorrug_onchain::Dossier {
        realorrug_onchain::Dossier {
            mint: "0x00000000000000000000000000000000000000aa"
                .parse()
                .expect("an address"),
            read_at: Some(realorrug_types::ReadAt::Robinhood(42)),
            launch: None,
            curve: None,
            creator_transactions: None,
            chain_launch: None,
            holders: None,
            funding: None,
            market: None,
            token_ownership: None,
            creator_cash_flow: None,
            powers: Some(realorrug_onchain::dossier::Powers {
                creator_tax_bps: 900,
                pending_creator_fee_recipient: None,
                exemptions: Vec::new(),
            }),
            unavailable: Vec::new(),
            calls: 0,
            elapsed_ms: 0,
        }
    }

    /// The whole point, end to end: a verdict served today and an outcome
    /// observed later become one pair, with the score the record kept even
    /// though no surface published it.
    #[test]
    fn a_served_verdict_pairs_with_an_outcome_observed_later() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("memory.sqlite3");
        verdict(
            Some(&path),
            "robinhood",
            "0xtoken",
            &sheet(),
            "Sketchy",
            "check",
        );

        let memory = Memory::open(&path).expect("open");
        assert_eq!(memory.verdict_count("robinhood", "0xtoken"), 1);
        assert!(
            memory
                .labelled_verdicts("robinhood")
                .expect("pairs")
                .is_empty(),
            "no outcome yet, so no pair yet"
        );

        memory
            .record_outcome(&Outcome {
                chain: "robinhood".to_owned(),
                token: "0xtoken".to_owned(),
                label: OutcomeLabel::Rug,
                at_block: Some(9_000),
                evidence: "creator sold 5,100 bps of their position".to_owned(),
                observed_at: SystemTime::now(),
            })
            .expect("record");

        let pairs = memory.labelled_verdicts("robinhood").expect("pairs");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0.level, "Sketchy");
        assert_eq!(pairs[0].0.source, "check");
        assert_eq!(pairs[0].0.read_at_block, Some(42));
        assert!(
            pairs[0].0.score_bps.is_some(),
            "the score is kept even though nothing publishes it"
        );
        assert_eq!(pairs[0].1.label, OutcomeLabel::Rug);
    }
}
