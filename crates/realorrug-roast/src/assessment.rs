// SPDX-License-Identifier: Apache-2.0
//! The shared assessment packet -- ADR 0032, design 0027's "Judgement".
//!
//! One type, built once per sheet from [`FactSheet`] alone, shared by the
//! analyst reply, the site's `/v1/check` cache and the voice layer's shadow
//! band request. Pure and deterministic, the same way [`crate::verdict::level`]
//! is: given the same sheet this returns the same packet on any machine at
//! any time.
//!
//! # Correlated flags count once
//!
//! Two signals that both read off the same underlying event -- a launch
//! block's recipient band and the creator's own buy inside that block -- are
//! not two independent pieces of evidence. [`episode`] names the event each
//! signal is evidence *of*, and [`Assessment::risk_index`] sums a weight per
//! **distinct episode**, not per signal: a token that fires both halves of
//! one launch-block read moves the index by one launch-block's worth of risk,
//! the same as a token that fires only one half of it. This is the one
//! published behaviour change in slice 7 -- [`crate::verdict::level`]'s
//! `RugMechanicsLive` threshold now counts the same way, so two live signals
//! that are really one observation no longer double-count toward it either.
//!
//! # Coverage travels beside the score, never inside it
//!
//! [`Assessment::coverage`] is a fraction of facts read over facts read plus
//! gaps -- both the required gaps ([`FactSheet::unknown`]) and the
//! non-degrading ones [`FactSheet::build`] chose not to let sink the level
//! ([`FactSheet::skipped`]). It never adjusts [`Assessment::risk_index`]
//! (ADR 0032 decision 2): a fact that was not read lowers coverage and
//! nothing else. [`Assessment::critical_gaps`] is `sheet.unknown` restated
//! beside it, not folded into the fraction -- "critical gaps survive
//! coverage" is the property a reader would lose if a high coverage number
//! could hide the one gap that forces [`crate::verdict::Level::CantTell`].

use crate::sheet::{FactSheet, Signal};
use crate::verdict::Level;

/// The finding groups design 0027's "Judgement" section names: launch
/// structure, ownership, creator activity, exit mechanics and trading
/// behaviour. Carried on [`Finding`] so a reader groups by what kind of
/// evidence a signal is, not by guessing from its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Group {
    /// The launch's own shape: the recipient band a launch-block read fell
    /// into.
    LaunchStructure,
    /// Who holds or controls the token: concentration and live authority.
    Ownership,
    /// What the creator has done, on this launch or across launches.
    CreatorActivity,
    /// How the token's exit was observed: liquidity, a creator sale, buyers'
    /// ability to sell.
    ExitMechanics,
    /// Post-launch trading behaviour. No shipped signal reads this group yet
    /// -- see [`group`]'s doc comment.
    TradingBehaviour,
}

/// Which underlying causal event a signal is evidence of.
///
/// Two signals that map to the same variant are evidence of the *same*
/// event, and [`Assessment::risk_index`] counts that event once. See the
/// module doc.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Episode {
    /// One launch-block read: the recipient band and a creator buy inside
    /// that same block are the same observation seen through two signals.
    LaunchBlock,
    /// The creator's measured launch record, across launches.
    CreatorHistory,
    /// The token's exit mechanics: liquidity, a creator's own sale, and
    /// whether buyers can sell, are three readings of one exit.
    Exit,
    /// The largest non-curve holder's share.
    Holders,
    /// A live owner-only authority (mint, pause, blacklist).
    Authority,
}

/// The episode a signal is evidence of.
///
/// **Total, no wildcard arm.** `Signal` grows a variant, this fails to
/// compile -- the same enforcement `sheet::twin_for` uses for the same
/// reason: a new signal cannot ship without also saying which event it is
/// evidence of, and a `_ =>` arm would let it silently join whichever
/// episode happened to be last.
#[must_use]
pub const fn episode(signal: Signal) -> Episode {
    match signal {
        Signal::LaunchBlockInStrongestBand | Signal::CreatorBoughtOwnLaunch => Episode::LaunchBlock,
        Signal::RepeatLauncher | Signal::CreatorNeverGraduatedOrganically => {
            Episode::CreatorHistory
        }
        Signal::LiquidityGone | Signal::CreatorSoldOut | Signal::BuyersCannotSell => Episode::Exit,
        Signal::HolderConcentration => Episode::Holders,
        Signal::OwnerCanStillMintOrPause => Episode::Authority,
    }
}

/// The finding group a signal belongs to.
///
/// Total for the same reason [`episode`] is: a signal ships, this compiles,
/// which forces its group to be named alongside it. Design 0027's
/// "trading behaviour" group has no member yet -- no shipped signal reads
/// post-launch trading -- so it is unreachable through this function today,
/// same as `Rugged` is unreachable through production signals per ADR 0032's
/// context section; the variant stays on [`Group`] because the packet is the
/// contract this crate's other readers (voice, analyst, site) serialise, and
/// removing it would be a wire-format change for no reason tied to this
/// slice.
#[must_use]
pub const fn group(signal: Signal) -> Group {
    match signal {
        Signal::LaunchBlockInStrongestBand => Group::LaunchStructure,
        Signal::CreatorNeverGraduatedOrganically
        | Signal::CreatorBoughtOwnLaunch
        | Signal::RepeatLauncher => Group::CreatorActivity,
        Signal::HolderConcentration | Signal::OwnerCanStillMintOrPause => Group::Ownership,
        Signal::LiquidityGone | Signal::CreatorSoldOut | Signal::BuyersCannotSell => {
            Group::ExitMechanics
        }
    }
}

/// One fired signal, with the group and episode it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    /// The signal that fired.
    pub signal: Signal,
    /// Which finding group the signal belongs to.
    pub group: Group,
    /// Which causal episode the signal is evidence of.
    pub episode: Episode,
}

/// How much of the evidence that would move [`Assessment::risk_index`] was
/// actually read, as a fraction -- **rendered as a fraction, not a
/// percent** (ADR 0032 decision 2), so a reader is never handed a single
/// number that already did the rounding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Coverage {
    /// Facts the sheet actually holds.
    pub read: usize,
    /// `read` plus every gap: [`FactSheet::unknown`] (required, forces
    /// [`Level::CantTell`]) and [`FactSheet::skipped`] (optional, does not).
    pub applicable: usize,
}

/// The shared assessment packet. See the module doc.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Assessment {
    /// One entry per fired signal, in `sheet.signals` order.
    pub findings: Vec<Finding>,
    /// 0-100, **not a probability** (ADR 0032 decision 1): the sum, over
    /// distinct episodes among `findings`, of that episode's hand-set
    /// weight, saturating at 100. Two signals in one episode add once.
    pub risk_index: u8,
    /// Facts read over facts read plus gaps. Never moves `risk_index`.
    pub coverage: Coverage,
    /// `sheet.unknown`, verbatim -- the gaps that force `CantTell` today,
    /// reported beside `coverage` rather than folded into it.
    pub critical_gaps: Vec<String>,
    /// The published level, code-computed, unchanged by anything below.
    pub level: Level,
    /// The band set a model may choose from, shadow-only in this slice --
    /// nothing reads this to publish anything (design 0027's "Judgement":
    /// "run it in shadow, with code alone supplying the published band").
    pub admissible: Vec<Level>,
}

/// One launch-block read: 25. Design 0020 §3's own signals for it
/// (`LaunchBlockInStrongestBand`, `CreatorBoughtOwnLaunch`) are ADR 0032
/// decision 6's fitted-weight scope (launch-shape and creator-history
/// factors), but the fit itself is future work (decision 7's ordering);
/// this is the hand-set starting weight decision 5 asks for, with its
/// reason stated beside it.
const WEIGHT_LAUNCH_BLOCK: u32 = 25;
/// The creator's launch record across launches: 20, hand-set (ADR 0032
/// decision 6 scope, decision 7 order -- the fit is future work).
const WEIGHT_CREATOR_HISTORY: u32 = 20;
/// Liquidity gone, a creator's own sale, buyers unable to sell: 40, the
/// heaviest weight -- these are observed mechanics, not a read of intent,
/// and ADR 0032 decision 6 keeps this hand-set (no outcome-labelled
/// population reaches an in-progress exit).
const WEIGHT_EXIT: u32 = 40;
/// The largest non-curve holder's share: 25, hand-set (ADR 0032 decision 6:
/// no outcome-labelled population for concentration yet).
const WEIGHT_HOLDERS: u32 = 25;
/// A live owner-only mint/pause/blacklist authority: 20, hand-set (ADR 0032
/// decision 6: no outcome-labelled population for authority yet).
const WEIGHT_AUTHORITY: u32 = 20;

/// The hand-set weight for one episode. A `const fn`, matched exhaustively
/// for the same reason [`episode`] is: [`Episode`] grows a variant, this
/// fails to compile rather than silently scoring the new episode at zero.
#[must_use]
const fn weight(episode: Episode) -> u32 {
    match episode {
        Episode::LaunchBlock => WEIGHT_LAUNCH_BLOCK,
        Episode::CreatorHistory => WEIGHT_CREATOR_HISTORY,
        Episode::Exit => WEIGHT_EXIT,
        Episode::Holders => WEIGHT_HOLDERS,
        Episode::Authority => WEIGHT_AUTHORITY,
    }
}

/// The milder band a model may propose beside the published one, or `None`
/// when the level admits nothing else.
///
/// Shadow only (see [`Assessment::admissible`]'s doc comment): `RugMechanicsLive`
/// and `Sketchy` each open one step down the ladder toward "less alarming",
/// never up -- a model proposing a *harsher* band than code computed is not
/// the failure mode this exists to bound, ADR 0032 decision 4 ("the model
/// still cannot move ... the level") already forbids it outright regardless
/// of what this function returns. `Rugged`, `CantTell` and `NothingUglyYet`
/// admit nothing else: `Rugged` because a milder proposal beside an observed
/// rug is the "green shield" ADR 0032 is answering, `CantTell` because it is
/// not on the badness axis at all ([`Level`]'s own doc comment) so no
/// "milder" direction exists for it, and `NothingUglyYet` because it is
/// already the floor.
const fn milder(level: Level) -> Option<Level> {
    match level {
        Level::RugMechanicsLive => Some(Level::Sketchy),
        Level::Sketchy => Some(Level::NothingUglyYet),
        Level::Rugged | Level::NothingUglyYet | Level::CantTell => None,
    }
}

impl Assessment {
    /// Builds the packet from the sheet. Pure: a function of `sheet` alone,
    /// same as [`crate::verdict::Verdict::from`].
    #[must_use]
    pub fn from(sheet: &FactSheet) -> Self {
        let findings: Vec<Finding> = sheet
            .signals
            .iter()
            .map(|&signal| Finding {
                signal,
                group: group(signal),
                episode: episode(signal),
            })
            .collect();

        let mut episodes: Vec<Episode> = findings.iter().map(|f| f.episode).collect();
        episodes.sort_by_key(|e| *e as u8);
        episodes.dedup();
        let risk_index = episodes
            .iter()
            .fold(0u32, |total, &ep| total + weight(ep))
            .min(100) as u8;

        let read = sheet.facts.len();
        let gaps = sheet.unknown.len() + sheet.skipped.len();
        let coverage = Coverage {
            read,
            applicable: read + gaps,
        };

        let level = crate::verdict::level(sheet);
        let admissible = match milder(level) {
            Some(other) => vec![level, other],
            None => vec![level],
        };

        Self {
            findings,
            risk_index,
            coverage,
            critical_gaps: sheet.unknown.clone(),
            level,
            admissible,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::Signal;

    /// A sheet carrying only signals and unknowns -- the facts and skipped
    /// gaps are what each test below sets, when it cares about them. Mirrors
    /// `verdict::tests::sheet_with`, kept separate rather than made `pub`
    /// there: this one also drives `facts`/`skipped`, which the level tests
    /// never need.
    fn sheet_with(signals: &[Signal], unknown: &[&str]) -> FactSheet {
        let signals: Vec<Signal> = signals.to_vec();
        let twins = signals
            .iter()
            .map(|&signal| crate::sheet::twin_for(signal).to_owned())
            .collect();
        FactSheet {
            mint: "MintAssessment".to_owned(),
            read_at: None,
            facts: Vec::new(),
            untrusted: Vec::new(),
            unknown: unknown.iter().map(|s| (*s).to_owned()).collect(),
            signals,
            twins,
            skipped: Vec::new(),
        }
    }

    #[test]
    fn episode_map_covers_every_signal() {
        // Total by construction (`episode` has no wildcard arm, so this
        // would fail to compile before it could fail to run) -- this test
        // is the record that every variant was actually exercised, not just
        // that the match compiles.
        for signal in [
            Signal::LaunchBlockInStrongestBand,
            Signal::CreatorNeverGraduatedOrganically,
            Signal::CreatorBoughtOwnLaunch,
            Signal::LiquidityGone,
            Signal::CreatorSoldOut,
            Signal::BuyersCannotSell,
            Signal::RepeatLauncher,
            Signal::HolderConcentration,
            Signal::OwnerCanStillMintOrPause,
        ] {
            let _ = episode(signal);
        }
    }

    #[test]
    fn two_signals_in_one_episode_score_once() {
        let sheet = sheet_with(
            &[
                Signal::LaunchBlockInStrongestBand,
                Signal::CreatorBoughtOwnLaunch,
            ],
            &[],
        );
        let assessment = Assessment::from(&sheet);
        assert_eq!(
            assessment.risk_index,
            u8::try_from(WEIGHT_LAUNCH_BLOCK).unwrap()
        );
        assert_eq!(assessment.findings.len(), 2);
        let distinct: std::collections::HashSet<Episode> =
            assessment.findings.iter().map(|f| f.episode).collect();
        assert_eq!(distinct.len(), 1);
    }

    #[test]
    fn two_episodes_add() {
        let sheet = sheet_with(
            &[Signal::LaunchBlockInStrongestBand, Signal::RepeatLauncher],
            &[],
        );
        let assessment = Assessment::from(&sheet);
        assert_eq!(
            assessment.risk_index,
            u8::try_from(WEIGHT_LAUNCH_BLOCK + WEIGHT_CREATOR_HISTORY).unwrap()
        );
    }

    #[test]
    fn index_saturates_at_100() {
        let sheet = sheet_with(
            &[
                Signal::LaunchBlockInStrongestBand,
                Signal::CreatorNeverGraduatedOrganically,
                Signal::CreatorBoughtOwnLaunch,
                Signal::LiquidityGone,
                Signal::CreatorSoldOut,
                Signal::BuyersCannotSell,
                Signal::RepeatLauncher,
                Signal::HolderConcentration,
                Signal::OwnerCanStillMintOrPause,
            ],
            &[],
        );
        assert_eq!(Assessment::from(&sheet).risk_index, 100);
    }

    #[test]
    fn unread_fact_lowers_coverage_not_index() {
        let with_gap = sheet_with(&[Signal::RepeatLauncher], &["holders"]);
        let without_gap = sheet_with(&[Signal::RepeatLauncher], &[]);
        let with_assessment = Assessment::from(&with_gap);
        let without_assessment = Assessment::from(&without_gap);
        assert_eq!(with_assessment.risk_index, without_assessment.risk_index);
        assert_eq!(
            with_assessment.coverage.applicable,
            with_assessment.coverage.read + 1
        );
    }

    #[test]
    fn critical_gap_survives_high_coverage() {
        let mut sheet = crate::verdict::tests::the_live_robinhood_sheet();
        sheet
            .unknown
            .push("the bonding curve could not be read".to_owned());
        let assessment = Assessment::from(&sheet);
        assert!(
            assessment
                .critical_gaps
                .iter()
                .any(|g| g.contains("bonding curve")),
            "{:?}",
            assessment.critical_gaps
        );
        assert_eq!(assessment.level, Level::CantTell);
        assert!(
            assessment.coverage.read > assessment.coverage.applicable / 2,
            "{:?}",
            assessment.coverage
        );
    }

    #[test]
    fn admissible_always_contains_level() {
        for level in [
            Level::Rugged,
            Level::RugMechanicsLive,
            Level::Sketchy,
            Level::NothingUglyYet,
            Level::CantTell,
        ] {
            let admissible = match milder(level) {
                Some(other) => vec![level, other],
                None => vec![level],
            };
            assert!(admissible.contains(&level), "{level:?}");
        }
    }

    #[test]
    fn rugged_and_cant_tell_admit_nothing_else() {
        assert_eq!(milder(Level::Rugged), None);
        assert_eq!(milder(Level::CantTell), None);
        assert_eq!(milder(Level::NothingUglyYet), None);
        assert_eq!(milder(Level::RugMechanicsLive), Some(Level::Sketchy));
        assert_eq!(milder(Level::Sketchy), Some(Level::NothingUglyYet));
    }

    #[test]
    fn packet_json_field_names_are_pinned() {
        let sheet = sheet_with(&[Signal::HolderConcentration], &[]);
        let assessment = Assessment::from(&sheet);
        let value = serde_json::to_value(&assessment).expect("serialises");
        let object = value.as_object().expect("a JSON object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "admissible",
                "coverage",
                "critical_gaps",
                "findings",
                "level",
                "risk_index",
            ]
        );
    }
}
