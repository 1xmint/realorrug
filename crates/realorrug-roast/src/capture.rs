// SPDX-License-Identifier: Apache-2.0
//! [`Capture`]: a fact sheet read from the chain and frozen to disk, with the
//! reading it was built from and the rule version that scored it.
//!
//! Lives here rather than in `realorrug-cli` (where `realorrug capture`
//! writes one and `realorrug replay` reads one back) because
//! `realorrug-roast/tests/accepted_replies_still_pass.rs` (plan 0002 phase 2,
//! unit 5) needs the type too, and a roast-crate test depending on the CLI
//! crate would be the dependency running backwards -- the CLI is a caller of
//! this crate, never the other way round.
//!
//! `roast` answers "what would the bot say right now"; a capture answers
//! "what did the bot measure just now" and writes the measurement down, with
//! nothing about how it would be phrased. That split is what makes
//! `realorrug replay` possible: a capture holds no model output and no
//! verdict wording, only the facts and the deterministic level and score a
//! later run can recompute and compare against, offline, with no chain and
//! no key.

use serde::{Deserialize, Serialize};

use crate::assessment::Assessment;
use crate::sheet::FactSheet;
use crate::verdict::Level;

/// One capture: a fact sheet, frozen with the reading it was built from and
/// the rule version that scored it.
///
/// `replay` reads this back and recomputes `level`/`assessment` from `sheet`
/// to see whether the rules have moved since — so every field here is either
/// the raw sheet or context about the reading, never a cached opinion `replay`
/// would otherwise have no way to tell from a fresh one.
#[derive(Debug, Serialize, Deserialize)]
pub struct Capture {
    /// The mint this capture is of, as given on the command line.
    pub mint: String,
    /// An operator-chosen name for the case, blank when none was given.
    ///
    /// A `String` rather than `Option<String>`: an empty label and an absent
    /// one both display as nothing in `review.md`, and a second field carrying
    /// only that distinction is not a case a reviewer needs to see.
    #[serde(default)]
    pub label: String,
    /// Wall-clock time this capture was taken, `YYYY-MM-DDTHH:MM:SSZ`.
    ///
    /// Distinct from `sheet.read_at` (the chain's own clock, a slot or a
    /// block): this is when the *operator's machine* asked, which is what a
    /// reviewer months later needs to judge how stale a capture is against
    /// today, without knowing how to convert a Solana slot to a date.
    pub captured_at: String,
    /// [`crate::RULES_VERSION`] at capture time, so a replay months later can
    /// tell a rule change from a data change.
    pub rules_version: String,
    /// The verdict level, computed the same way `roast` computes it.
    pub level: Level,
    /// The full risk assessment `roast` builds under that level.
    pub assessment: Assessment,
    /// The fact sheet itself — everything a reply or a report may state.
    pub sheet: FactSheet,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_dossier() -> realorrug_onchain::Dossier {
        realorrug_onchain::Dossier {
            mint: "11111111111111111111111111111112"
                .parse()
                .expect("an address"),
            read_at: None,
            launch: None,
            curve: None,
            creator_transactions: None,
            chain_launch: None,
            holders: None,
            funding: None,
            market: None,
            token_ownership: None,
            creator_cash_flow: None,
            powers: None,
            unavailable: Vec::new(),
            calls: 0,
            elapsed_ms: 0,

            retries: 0,
            paused_ms: 0,
        }
    }

    /// A capture round-trips through JSON: `replay` reads back exactly what
    /// `capture` wrote, with no field lost to a `#[serde(skip)]` or a type
    /// that only serializes.
    #[test]
    fn a_capture_round_trips_through_json() {
        let sheet = FactSheet::build(&empty_dossier(), None, None, None, None);
        let level = crate::verdict::level(&sheet);
        let assessment = Assessment::from(&sheet);
        let capture = Capture {
            mint: "SoMeMiNt".to_owned(),
            label: "ordinary launch".to_owned(),
            captured_at: "2026-09-21T00:00:00Z".to_owned(),
            rules_version: crate::RULES_VERSION.to_owned(),
            level,
            assessment,
            sheet,
        };
        let json = serde_json::to_string(&capture).expect("encode");
        let back: Capture = serde_json::from_str(&json).expect("decode");
        assert_eq!(back.mint, capture.mint);
        assert_eq!(back.label, capture.label);
        assert_eq!(back.rules_version, capture.rules_version);
    }
}
