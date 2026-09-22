// SPDX-License-Identifier: Apache-2.0
//! The four-part report published beside every reply (plan 0002 phase 2,
//! design 0029, ADR 0037-0039): the owner's tool for reading a capture and
//! deciding whether the reply that came out of it is one they would post.
//!
//! # Beside the reply, never inside it
//!
//! A 280-character reply cannot carry all four parts and stay legible, so
//! this is a second, longer document built from the same [`FactSheet`] and
//! never folded into the reply's own text. Both are downstream of the sheet;
//! neither is downstream of the other.
//!
//! # What this module does not do
//!
//! - **It never re-scores.** [`crate::verdict`] and [`crate::assessment`]
//!   stay the only source of the level and the risk index; this module reads
//!   both and states neither.
//! - **Missing data is never turned into "safe".** [`Report::missing`] lists
//!   every unknown or skipped fact as missing, plain, with no reassurance
//!   attached -- the failure this module exists to keep from recurring is a
//!   sheet with unread holders being read as "fine" because nothing on the
//!   page said otherwise.
//! - **The risk index is a number, never a probability.** Nothing in this
//!   module calls [`crate::assessment::Assessment::risk_index`] a chance or a
//!   likelihood; it is passed through as the 0-100 index it is.

use crate::assessment::Assessment;
use crate::clause::Kind;
use crate::sheet::{FactSheet, Signal};
use crate::verdict::Level;
use realorrug_types::ReadAt;

/// The strongest concern the sheet supports, chosen by [`crate::salience`]
/// alone -- never by a fact's `label`, for the same reason `salience.rs`
/// itself gives (AGENTS.md §4): a relabelled fact must not change what a
/// report opens with any more than it may change what a reply opens with.
///
/// `None` when the sheet supports no candidate at all (`salience::lead`
/// returns `None` for a sheet with nothing to rank).
#[derive(Clone, Debug, PartialEq)]
pub struct Concern {
    /// The fact kinds the chosen candidate bundles, exactly as
    /// [`crate::salience::CandidateId`] carries them.
    pub kinds: Vec<Kind>,
    /// The evidence sentence [`crate::salience`] built for this candidate,
    /// already honest about role and denominator -- reused rather than
    /// restated, so the report can never disagree with the ranking it was
    /// drawn from.
    pub evidence: String,
}

/// One alternative reading of a fired signal, keyed by the fact kind
/// [`signal_kind`] ties it to.
///
/// **Never the signal's name.** `explanation` is [`crate::sheet::twin_for`]'s own sentence
/// -- phrased as what was read, with no digit in it -- because a reply or a
/// report that named the `Signal` variant would teach a reader (or a model
/// prompted from this report) a vocabulary the sheet itself never uses.
#[derive(Clone, Debug, PartialEq)]
pub struct AlternativeRow {
    /// The fact kind this alternative reading is about.
    pub kind: Kind,
    /// The signal it is the innocent twin of, kept for traceability back to
    /// [`FactSheet::signals`] -- never printed by name in front of a reader.
    pub signal: Signal,
    /// The innocent, on-chain-identical reading.
    pub explanation: String,
}

/// Every unknown or skipped fact, plus the moment the sheet was read.
///
/// A gap that reaches this struct is stated as a gap. There is no field here
/// that turns an absence into a level, a word like "clean", or a reassurance
/// -- that translation is exactly the bug this module exists to make
/// impossible to reintroduce (see `tests::unknown_holders_are_listed_as_missing_not_as_fine`).
#[derive(Clone, Debug, PartialEq)]
pub struct Missing {
    /// [`FactSheet::unknown`], verbatim: required facts that force
    /// [`Level::CantTell`].
    pub unknown: Vec<String>,
    /// [`FactSheet::skipped`], verbatim: optional facts that do not.
    pub skipped: Vec<String>,
    /// [`FactSheet::read_at`]: when the sheet's numbers were true. `None`
    /// only when the sheet itself carries no read point (Robinhood Chain
    /// before that reader is wired up, per `read_at`'s own doc comment) --
    /// never omitted because it looked redundant.
    pub read_at: Option<ReadAt>,
}

/// What evidence, if read, would let the record replace one of the fired
/// signals' alternative readings -- keyed by the fact kind and the level the
/// sheet was published at.
///
/// **This describes what a reading would resolve, never what it would
/// find.** A row here never states a new level, a new score or a direction
/// ("this would clear it" / "this would confirm it") -- doing either would
/// be this module moving the verdict, which `verdict.rs` and `assessment.rs`
/// stay the only source of.
#[derive(Clone, Debug, PartialEq)]
pub struct ChangeRow {
    /// The fact kind further evidence would be read about.
    pub kind: Kind,
    /// The level the sheet carried when this row was built, so a reviewer
    /// comparing two captures of the same mint can see which row belonged to
    /// which read.
    pub level: Level,
    /// What kind of evidence would let the alternative reading above be
    /// resolved, or stay unresolved, one way or the other -- never a
    /// prediction of which.
    pub would_resolve: String,
}

/// The four-part report.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// Part 1: the strongest concern and its evidence. `None` when no signal
    /// fired -- the salience-ranked fact still exists on a signal-less sheet,
    /// but it is not a concern, so it travels in [`Report::context`] instead
    /// (replay-2026-09: the fix for a 0.08% top-holder share leading this
    /// section while `alternatives` said "no signal fired" one line below).
    pub strongest_concern: Option<Concern>,
    /// Part 2: alternative explanations, one row per fired signal.
    pub alternatives: Vec<AlternativeRow>,
    /// Part 3: missing checks.
    pub missing: Missing,
    /// Part 4: what evidence would change the assessment.
    pub would_change: Vec<ChangeRow>,
    /// The salience-ranked fact, demoted out of [`Report::strongest_concern`]
    /// because no signal fired. `Some` only when `alternatives` is empty and
    /// the sheet still had a candidate to rank -- the same fact a reader
    /// would otherwise have seen mislabelled as the concern.
    pub context: Option<Concern>,
}

/// The fact kind a fired [`Signal`] is read from.
///
/// One exhaustive `match`, no `_ =>` arm, the same discipline
/// [`crate::sheet::twin_for`] and [`Signal::plain`] already hold themselves
/// to: a new `Signal` variant that is not given a kind here fails to
/// compile, rather than silently being left out of the alternatives table.
fn signal_kind(signal: Signal) -> Kind {
    match signal {
        Signal::LaunchBlockInStrongestBand => Kind::LaunchRecipients,
        Signal::CreatorNeverGraduatedOrganically => Kind::CreatorOrganic,
        Signal::CreatorBoughtOwnLaunch => Kind::DevBuy,
        // No dossier constructs `BuyersCannotSell` yet (`sheet.rs`'s own
        // doc comment); the nearest measured kind for both is the curve's
        // own liquidity, which is what a simulated sell reads against and
        // what a drain of reserves is a read of.
        Signal::LiquidityGone | Signal::BuyersCannotSell => Kind::CurveLiquidity,
        // The creator's balance going to zero is read from the same
        // observed cash flow `Kind::CreatorCashFlow` already names -- there
        // is no separate "creator balance" kind on the sheet.
        Signal::CreatorSoldOut => Kind::CreatorCashFlow,
        Signal::RepeatLauncher => Kind::CreatorLaunches,
        Signal::HolderConcentration => Kind::LargestHolderShare,
        Signal::OwnerCanStillMintOrPause => Kind::CreatorTaxBps,
        Signal::CorrelatedSelling => Kind::CorrelatedSellWallets,
    }
}

/// What kind of read would let a fired signal's alternative reading be
/// resolved, phrased with no direction and no digit -- the same discipline
/// [`crate::sheet::twin_for`] holds its own sentences to, for the same reason: this text
/// can reach a model's prompt, and a number here would be a claim
/// `fidelity::check` could not source.
fn would_resolve_text(signal: Signal) -> &'static str {
    match signal {
        Signal::LaunchBlockInStrongestBand => {
            "whether the launch was promoted somewhere off-chain, which no chain read settles"
        }
        Signal::CreatorNeverGraduatedOrganically => {
            "more measured launches from the same creator, which only time and further reads \
             can add"
        }
        Signal::CreatorBoughtOwnLaunch => {
            "a public statement of intent from the creator, which is off-chain and unverifiable"
        }
        Signal::LiquidityGone => {
            "a trace of where the withdrawn reserves went, which a further chain read could \
             still recover"
        }
        Signal::CreatorSoldOut => {
            "whether the creator's tokens moved to a wallet still under the same control, which \
             a further chain read could still recover"
        }
        Signal::BuyersCannotSell => {
            "a second simulated sell at a smaller size, which a further chain read could still \
             perform"
        }
        Signal::RepeatLauncher => {
            "whether one person or an automated relayer is behind the repeated launches, which \
             no chain read settles"
        }
        Signal::HolderConcentration => {
            "a label for the large address -- a vesting contract, a bridge or an exchange -- \
             which no chain read settles by itself"
        }
        Signal::OwnerCanStillMintOrPause => {
            "whether the power was ever exercised, which a further chain read over time could \
             still show"
        }
        Signal::CorrelatedSelling => {
            "whether the linked wallets share a controller or only share a launch window, which \
             no chain read settles"
        }
    }
}

/// Builds the report from a sheet, deterministically.
///
/// Pure, like [`crate::verdict::Verdict::from`]: the same sheet builds the
/// same report on any machine at any time, which is what lets a replay
/// (`realorrug replay`) reproduce it from a saved capture with no network.
#[must_use]
pub fn build(sheet: &FactSheet) -> Report {
    let lead = crate::salience::lead(sheet).map(|candidate| Concern {
        kinds: candidate.id.0.clone(),
        evidence: candidate.sentence,
    });
    // Nothing fired: the ranked fact is real but is not a concern -- putting
    // it under "Strongest concern" is the exact defect replay-2026-09 found
    // (a tiny holder share leading the section while `alternatives` says "no
    // signal fired" one line below). It still reaches the reader, as
    // context instead of as the concern.
    let (strongest_concern, context) = if sheet.signals.is_empty() {
        (None, lead)
    } else {
        (lead, None)
    };

    let level = crate::verdict::level(sheet);

    let mut alternatives = Vec::with_capacity(sheet.signals.len());
    let mut would_change = Vec::with_capacity(sheet.signals.len());
    // `signals` and `twins` are the same length in the same order --
    // `FactSheet`'s own invariant (see its `twins` field doc comment) -- so
    // this single zip is the one place both tables are built from, and the
    // two can never drift apart into different lengths.
    for (&signal, twin) in sheet.signals.iter().zip(sheet.twins.iter()) {
        let kind = signal_kind(signal);
        alternatives.push(AlternativeRow {
            kind,
            signal,
            explanation: twin.clone(),
        });
        would_change.push(ChangeRow {
            kind,
            level,
            would_resolve: would_resolve_text(signal).to_owned(),
        });
    }

    Report {
        strongest_concern,
        alternatives,
        missing: Missing {
            unknown: sheet.unknown.clone(),
            skipped: sheet.skipped.clone(),
            read_at: sheet.read_at,
        },
        would_change,
        context,
    }
}

impl Report {
    /// Renders the report as Markdown, for `realorrug replay`'s
    /// `review.md`.
    ///
    /// Never used to decide anything -- this is prose for the owner, not a
    /// value anything downstream parses back.
    #[must_use]
    pub fn render(&self, assessment: &Assessment) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();

        let _ = writeln!(out, "**Strongest concern**");
        match &self.strongest_concern {
            Some(c) => {
                let _ = writeln!(out, "- {}", c.evidence);
                let kinds: Vec<String> = c.kinds.iter().map(|k| format!("{k:?}")).collect();
                let _ = writeln!(out, "  (evidence: {})", kinds.join(", "));
            }
            // `alternatives` is built 1:1 from `sheet.signals` (`build`'s own
            // zip), so an empty one here means no signal fired -- the exact
            // case `build` also empties `strongest_concern` for, so the two
            // sections never contradict each other the way replay-2026-09
            // found them doing.
            None if self.alternatives.is_empty() => {
                let _ = writeln!(out, "- no signal fired");
            }
            None => {
                let _ = writeln!(
                    out,
                    "- nothing on this sheet ranked -- no candidate to lead with"
                );
            }
        }

        let _ = writeln!(out, "\n**Alternative explanations**");
        if self.alternatives.is_empty() {
            let _ = writeln!(
                out,
                "- no signal fired, so there is no alternative reading to state"
            );
        } else {
            let _ = writeln!(out, "| fact kind | alternative reading |");
            let _ = writeln!(out, "|---|---|");
            for row in &self.alternatives {
                let _ = writeln!(out, "| {:?} | {} |", row.kind, row.explanation);
            }
        }

        if let Some(c) = &self.context {
            let _ = writeln!(out, "\n**Context**");
            let _ = writeln!(out, "- {}", c.evidence);
            let kinds: Vec<String> = c.kinds.iter().map(|k| format!("{k:?}")).collect();
            let _ = writeln!(out, "  (evidence: {})", kinds.join(", "));
        }

        let _ = writeln!(out, "\n**Missing checks**");
        let _ = writeln!(
            out,
            "- read at: {}",
            self.missing
                .read_at
                .map_or_else(|| "not known".to_owned(), |r| r.to_string())
        );
        if self.missing.unknown.is_empty() && self.missing.skipped.is_empty() {
            let _ = writeln!(out, "- nothing unread on this sheet");
        } else {
            for miss in &self.missing.unknown {
                let _ = writeln!(out, "- not known -- {miss}");
            }
            for miss in &self.missing.skipped {
                let _ = writeln!(out, "- not read (optional) -- {miss}");
            }
        }

        let _ = writeln!(out, "\n**What would change it**");
        if self.would_change.is_empty() {
            let _ = writeln!(out, "- no fired signal to resolve either way");
        } else {
            let _ = writeln!(out, "| fact kind | level at read | what would resolve it |");
            let _ = writeln!(out, "|---|---|---|");
            for row in &self.would_change {
                let _ = writeln!(
                    out,
                    "| {:?} | {:?} | {} |",
                    row.kind, row.level, row.would_resolve
                );
            }
        }

        // `crate::unknown::check` refuses "probability"/"chance"/"odds" near
        // the risk index on sight, deliberately as bluntly as
        // `forbidden::check` refuses "not a scam" (both files' own doc
        // comments give the same reason: a checker that tried to read a
        // negation would be a checker arguing about meaning). So this line
        // states the count and disclaims it as "not the published level"
        // without naming any of the words the check bans, rather than
        // writing the disclaimer those words would make and asking the
        // check to read past it.
        let _ = writeln!(
            out,
            "\n(risk index {}/100, coverage {}/{} facts read -- a count of what fired, not the \
             published level and not a forecast)",
            assessment.risk_index, assessment.coverage.read, assessment.coverage.applicable
        );

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::twin_for;

    fn sheet_with(signals: Vec<Signal>, unknown: Vec<String>, skipped: Vec<String>) -> FactSheet {
        let twins = signals
            .iter()
            .map(|&signal| twin_for(signal).to_owned())
            .collect();
        FactSheet {
            mint: "MintReport".to_owned(),
            read_at: None,
            facts: Vec::new(),
            untrusted: Vec::new(),
            unknown,
            signals,
            twins,
            skipped,
        }
    }

    /// The bug this module exists to keep from recurring: a sheet with
    /// unread holders must list them as missing on `Report::missing`, never
    /// be described anywhere as clean, safe or fine because nothing else on
    /// the sheet said otherwise.
    #[test]
    fn unknown_holders_are_listed_as_missing_not_as_fine() {
        let sheet = sheet_with(Vec::new(), vec!["holders".to_owned()], Vec::new());
        let report = build(&sheet);
        assert_eq!(report.missing.unknown, vec!["holders".to_owned()]);
        assert!(report.missing.skipped.is_empty());
        // The rendered text says the fact is not known -- and, just as
        // important, contains none of the words that would turn that
        // absence into a reassurance.
        let assessment = Assessment {
            findings: Vec::new(),
            risk_index: 0,
            score_bps: crate::assessment::Weight::from_bps(0),
            coverage: crate::assessment::Coverage {
                read: 0,
                applicable: 1,
            },
            critical_gaps: sheet.unknown.clone(),
            level: crate::verdict::level(&sheet),
            admissible: Vec::new(),
            score_level: Level::CantTell,
        };
        let rendered = report.render(&assessment);
        assert!(rendered.contains("not known -- holders"), "{rendered}");
        for word in ["safe", "clean", "fine", "no risk"] {
            assert!(
                !rendered.to_lowercase().contains(word),
                "missing data must never read as {word:?}: {rendered}"
            );
        }
    }

    /// Skipped (optional) facts are listed distinctly from unknown
    /// (required) ones -- `FactSheet`'s own separation, carried through
    /// rather than merged into one undifferentiated "missing" bucket.
    #[test]
    fn skipped_facts_are_missing_too_but_named_separately_from_unknown() {
        let sheet = sheet_with(Vec::new(), Vec::new(), vec!["market".to_owned()]);
        let report = build(&sheet);
        assert!(report.missing.unknown.is_empty());
        assert_eq!(report.missing.skipped, vec!["market".to_owned()]);
    }

    /// The strongest concern comes from `salience::lead`, not from a label:
    /// a sheet with no ranked candidate carries `None` rather than a report
    /// inventing something to lead with.
    #[test]
    fn a_sheet_with_no_ranked_candidate_has_no_strongest_concern() {
        let sheet = sheet_with(Vec::new(), Vec::new(), Vec::new());
        let report = build(&sheet);
        assert!(report.strongest_concern.is_none());
        assert!(report.context.is_none());
    }

    /// Fault 2 (replay-2026-09): a sheet that ranks a candidate but fired no
    /// signal must not present that candidate as the strongest concern --
    /// the report's own Alternative explanations already say no signal
    /// fired, and a tiny holder share leading the concern section
    /// contradicted that line one row down. The fact is not dropped: it
    /// moves to `Report::context`, and `render` says "no signal fired"
    /// rather than a non-concern.
    #[test]
    fn a_ranked_candidate_with_no_signal_becomes_context_not_a_concern() {
        use crate::sheet::Fact;
        let mut sheet = sheet_with(Vec::new(), Vec::new(), Vec::new());
        sheet.facts = vec![
            Fact::exact(
                Kind::Holders,
                "addresses holding the token now",
                529.0,
                "529",
            ),
            Fact::exact(
                Kind::LargestHolderShare,
                "held by the single largest address",
                0.0008,
                "0.08%",
            ),
        ];
        let report = build(&sheet);
        assert!(
            report.strongest_concern.is_none(),
            "a non-concern must not fill the strongest-concern slot: {:?}",
            report.strongest_concern
        );
        let context = report
            .context
            .clone()
            .expect("the ranked fact still surfaces");
        assert!(context.evidence.contains("0.08%"), "{}", context.evidence);

        let assessment = Assessment {
            findings: Vec::new(),
            risk_index: 0,
            score_bps: crate::assessment::Weight::from_bps(0),
            coverage: crate::assessment::Coverage {
                read: 1,
                applicable: 1,
            },
            critical_gaps: Vec::new(),
            level: crate::verdict::level(&sheet),
            admissible: Vec::new(),
            score_level: Level::NothingUglyYet,
        };
        let rendered = report.render(&assessment);
        // The exact line under the heading: "no signal fired" alone also
        // matches the Alternative explanations section below it.
        assert!(
            rendered.starts_with(
                "**Strongest concern**
- no signal fired
"
            ),
            "the strongest-concern section must say so, not present a non-concern: {rendered}"
        );
        assert!(
            rendered.contains("**Context**") && rendered.contains("0.08%"),
            "the ranked fact must still reach the reader, as context: {rendered}"
        );
    }

    /// A signal fired but nothing ranked: the concern section says there was
    /// no candidate to lead with, never "no signal fired", which would
    /// contradict the alternative row printed below it.
    #[test]
    fn a_fired_signal_with_nothing_ranked_does_not_say_no_signal_fired() {
        let sheet = sheet_with(vec![Signal::HolderConcentration], Vec::new(), Vec::new());
        let report = build(&sheet);
        assert!(report.strongest_concern.is_none());
        let assessment = Assessment {
            findings: Vec::new(),
            risk_index: 0,
            score_bps: crate::assessment::Weight::from_bps(0),
            coverage: crate::assessment::Coverage {
                read: 0,
                applicable: 1,
            },
            critical_gaps: Vec::new(),
            level: crate::verdict::level(&sheet),
            admissible: Vec::new(),
            score_level: Level::CantTell,
        };
        let rendered = report.render(&assessment);
        assert!(
            rendered.starts_with(
                "**Strongest concern**
- nothing on this sheet ranked -- no candidate to lead with
"
            ),
            "{rendered}"
        );
    }

    /// Every fired signal produces one alternative row and one would-change
    /// row, in the same order, and neither table states a new level or
    /// score -- only the level the sheet already carried at read time.
    #[test]
    fn every_fired_signal_gets_an_alternative_and_a_would_change_row() {
        let sheet = sheet_with(
            vec![Signal::HolderConcentration, Signal::CorrelatedSelling],
            Vec::new(),
            Vec::new(),
        );
        let report = build(&sheet);
        assert_eq!(report.alternatives.len(), 2);
        assert_eq!(report.would_change.len(), 2);
        assert_eq!(report.alternatives[0].kind, Kind::LargestHolderShare);
        assert_eq!(report.alternatives[0].signal, Signal::HolderConcentration);
        assert_eq!(report.alternatives[1].kind, Kind::CorrelatedSellWallets);
        // The would-change row never states a level other than the one the
        // sheet already carries -- it is not a prediction of a different
        // verdict.
        let level = crate::verdict::level(&sheet);
        for row in &report.would_change {
            assert_eq!(row.level, level);
            // Never a digit: this text can reach a model's prompt and
            // `fidelity::check` has nothing to source a number in it from.
            assert!(
                !row.would_resolve.chars().any(|c| c.is_ascii_digit()),
                "{}",
                row.would_resolve
            );
        }
    }

    /// `signal_kind` is exhaustive; this is not a test of behaviour so much
    /// as a compile-time guard made visible -- if a variant were ever
    /// missing the match would not compile, so there is nothing more to
    /// assert here except that every known variant maps to *some* kind.
    #[test]
    fn every_signal_has_a_kind_and_a_resolution_sentence() {
        let all = [
            Signal::LaunchBlockInStrongestBand,
            Signal::CreatorNeverGraduatedOrganically,
            Signal::CreatorBoughtOwnLaunch,
            Signal::LiquidityGone,
            Signal::CreatorSoldOut,
            Signal::BuyersCannotSell,
            Signal::RepeatLauncher,
            Signal::HolderConcentration,
            Signal::OwnerCanStillMintOrPause,
            Signal::CorrelatedSelling,
        ];
        for signal in all {
            let _ = signal_kind(signal);
            assert!(!would_resolve_text(signal).is_empty());
        }
    }
}
