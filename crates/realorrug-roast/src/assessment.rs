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
    /// Post-launch trading behaviour: `Signal::CorrelatedSelling` (S7).
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
        // S7 is an act, not a shape: unlike `LaunchBlockInStrongestBand`
        // (how the launch block looked) it is something wallets *did* after
        // the fact, the same kind of event `LiquidityGone`,
        // `CreatorSoldOut` and `BuyersCannotSell` are -- the exit itself,
        // not evidence about who set it up.
        Signal::LiquidityGone
        | Signal::CreatorSoldOut
        | Signal::BuyersCannotSell
        | Signal::CorrelatedSelling => Episode::Exit,
        Signal::HolderConcentration => Episode::Holders,
        Signal::OwnerCanStillMintOrPause => Episode::Authority,
    }
}

/// The finding group a signal belongs to.
///
/// Total for the same reason [`episode`] is: a signal ships, this compiles,
/// which forces its group to be named alongside it. Design 0027's
/// "trading behaviour" group reached its first member with S7
/// (`Signal::CorrelatedSelling`, M-D-0008): post-launch selling, not launch
/// structure, creator history, ownership or curve exit.
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
        // The first shipped signal to read this group: unlike the exit
        // mechanics above (the curve, the creator's own wallet), S7 is
        // wallets *trading* after launch, which is what `TradingBehaviour`
        // was named for -- see the group's own doc comment.
        Signal::CorrelatedSelling => Group::TradingBehaviour,
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
    /// The same per-episode weights as `risk_index`, scaled to basis points
    /// and combined by [`noisy_or`] instead of summed (research 0052 §4,
    /// ADR 0032's noisy-OR paragraph). Kept alongside `risk_index` rather
    /// than replacing it -- `risk_index` is a pinned JSON key (module
    /// `tests::packet_json_field_names_are_pinned`) and this slice does not
    /// change its value.
    pub score_bps: Weight,
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
    /// Research 0052 §4.2, M-D-0005: the level `score_bps` alone would
    /// publish, from [`crate::verdict::level_from_score`]. **Shadow only** --
    /// nothing reads this to publish anything either; `level` above stays
    /// the one [`crate::verdict::level`] computed until a later step flips a
    /// flag research 0052 §9 says must wait for the owner to confirm.
    pub score_level: Level,
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

/// A weight in basis points (bps; 10,000 bps = 100%) -- research 0052's unit
/// for a signal's weight and for the combined score alike. `u32` only: this
/// file stays whole-number throughout, no floating-point type anywhere in it
/// (`AGENTS.md` §3 rule 2, design 0028's whole-number rule).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct Weight(u32);

impl Weight {
    /// 10,000 bps: the ceiling both a single weight and [`noisy_or`]'s output
    /// saturate at.
    pub const MAX: Self = Self(10_000);
    /// 0 bps: [`noisy_or`] on an empty slice, and the floor no weight goes
    /// under.
    pub const ZERO: Self = Self(0);

    /// Builds a weight from a basis-point count, clamped to `0..=10_000` --
    /// a weight above the ceiling is a caller bug, not a value this type
    /// exists to reject at a distance; clamping keeps the type total the way
    /// [`episode`] and [`weight`] are total.
    #[must_use]
    pub fn from_bps(bps: u32) -> Self {
        // `min`, not `if bps > MAX`: at exactly 10,000 both branches of the
        // `if` give the same value, so the comparison could not be tested.
        Self(bps.min(Self::MAX.0))
    }

    /// The weight as a raw basis-point count.
    #[must_use]
    pub const fn bps(self) -> u32 {
        self.0
    }
}

/// Research 0052 §3.1's base weight for one signal, before any factor.
///
/// **Per signal, not per episode.** [`weight`] above is the coarser
/// per-episode number `risk_index` sums (unchanged by this packet); this is
/// the finer, per-signal number the catalogue actually names, because a
/// [`crate::sheet::Factor`] is anchored to one signal and has to add to
/// something of the same shape. Two signals sharing an episode (`S1`, `S2`
/// in the paper) still fold to that episode's *maximum* before
/// [`noisy_or`], the same as before -- only the number being maxed changed.
#[must_use]
const fn signal_base_bps(signal: Signal) -> u32 {
    match signal {
        // These base weights are independently drawn from research 0052
        // §3.1's catalogue; a few signals happen to share a value today,
        // and clippy's match_same_arms requires the shared ones written on
        // one line below. That is a syntactic merge only: a later
        // re-measurement of one must not be assumed to move the other.
        Signal::LaunchBlockInStrongestBand => 1_500,
        Signal::RepeatLauncher | Signal::CorrelatedSelling => 1_000,
        Signal::CreatorNeverGraduatedOrganically => 600,
        Signal::CreatorBoughtOwnLaunch | Signal::HolderConcentration => 1_200,
        Signal::LiquidityGone | Signal::BuyersCannotSell => 4_000,
        Signal::CreatorSoldOut => 2_500,
        Signal::OwnerCanStillMintOrPause => 800,
    }
}

/// A self-reported factor may lower a signal by at most this many basis
/// points in total, and never raise it (research 0052 §3.3).
const SELF_REPORTED_FLOOR_BPS: i64 = -200;

/// No factor set may take a signal below this share of its base (research
/// 0052 §3.3): a measured fact always leaves a mark, but factors alone
/// cannot erase a signal that fired.
const MIN_SHARE_OF_BASE_PERCENT: i64 = 25;

/// One signal's weight: its research 0052 §3.1 base, adjusted by every
/// [`crate::sheet::Factor`] on `factors` whose [`crate::sheet::Factor::signal`]
/// matches, with two rules enforced here rather than trusted from the
/// caller (both from research 0052 §3.3):
///
/// 1. A [`crate::sheet::Grade::SelfReported`] factor's `delta_bps` is
///    clamped to at most 0 before it is summed -- a positive self-reported
///    delta is a caller bug, not a value this function exists to reject at
///    a distance ([`Weight::from_bps`]'s own pattern) -- and the *summed*
///    self-reported delta for this signal is then clamped to
///    [`SELF_REPORTED_FLOOR_BPS`].
/// 2. The result never falls below 25% of the signal's base, computed
///    before any factor is applied.
///
/// Measured and inferred deltas carry no cap of their own: research 0052's
/// catalogue already sizes them (a `LiquidityGone` twin's `-1,500` on a
/// 4,000 base does not need a second ceiling here).
#[must_use]
pub fn adjusted_weight(signal: Signal, factors: &[crate::sheet::Factor]) -> Weight {
    let base = i64::from(signal_base_bps(signal));
    let mut other_delta: i64 = 0;
    let mut self_reported_delta: i64 = 0;
    for factor in factors.iter().filter(|factor| factor.signal == signal) {
        match factor.grade {
            crate::sheet::Grade::SelfReported => {
                self_reported_delta += i64::from(factor.delta_bps).min(0);
            }
            crate::sheet::Grade::Measured | crate::sheet::Grade::Inferred => {
                other_delta += i64::from(factor.delta_bps);
            }
        }
    }
    let self_reported_delta = self_reported_delta.max(SELF_REPORTED_FLOOR_BPS);
    let floor = base * MIN_SHARE_OF_BASE_PERCENT / 100;
    let total = (base + other_delta + self_reported_delta).max(floor);
    Weight::from_bps(u32::try_from(total).unwrap_or(0))
}

/// Combines weights the way independent chances combine: the chance none of
/// them holds is the product of each one's absence, so the combined weight is
/// `10_000 - product_i(10_000 - w_i)`, scaled back to bps at every step
/// (research 0052 §4). Picked over a capped sum (lets several mild,
/// correlated signals outrun one hard fact) and over "strongest wins" (every
/// corroborating signal after the first is free) because this is the one
/// rule where the strongest signal sets the floor, each further signal adds
/// with diminishing returns, the result never exceeds [`Weight::MAX`], and
/// adding a weight never lowers the result.
///
/// **Caller dedups by episode first.** This fold has no notion of which
/// weights are evidence of the same underlying event -- [`Assessment::from`]
/// folds `sheet.signals` down to one weight per distinct [`Episode`] (the
/// existing `max`-per-episode read, module doc "Correlated flags count
/// once") before calling this, so a bundle seen four ways is one input here,
/// not four.
///
/// **Sorted descending, then folded in that fixed order.** The floor
/// division at each step is not associative: the same multiset folded in a
/// different order can round differently by a few bps. Sorting first makes
/// the order a function of the *values* alone, so every permutation of the
/// same input produces the same output, and the whole computation stays in
/// `u32` (`10_000 * 10_000` fits well under `u32::MAX` before the divide).
#[must_use]
pub fn noisy_or(weights: &[Weight]) -> Weight {
    // `min` because a `Weight` read back through serde skips `from_bps`'s
    // clamp; an over-cap value must saturate, not underflow `10_000 - w`.
    let mut bps: Vec<u32> = weights.iter().map(|w| w.0.min(10_000)).collect();
    bps.sort_unstable_by(|a, b| b.cmp(a));
    let rest = bps
        .iter()
        .fold(10_000u32, |rest, &w| rest * (10_000 - w) / 10_000);
    Weight::from_bps(10_000 - rest)
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

        // Dedup by episode happens right here, once, before either
        // `risk_index` or `score_bps` reads a weight: a bundle that fires
        // several signals of the same causal episode is one entry in
        // `episodes`, so neither the summed nor the noisy-OR combination
        // below counts it more than once (module doc "Correlated flags
        // count once").
        let mut episodes: Vec<Episode> = findings.iter().map(|f| f.episode).collect();
        episodes.sort_by_key(|e| *e as u8);
        episodes.dedup();
        let risk_index = episodes
            .iter()
            .fold(0u32, |total, &ep| total + weight(ep))
            .min(100) as u8;
        // M-D-0002: `score_bps` now folds research 0052 §3.1's per-signal
        // base weight, adjusted by whatever factors the sheet's own facts
        // support (`crate::sheet::factors`), rather than `risk_index`'s
        // coarser per-episode placeholder above -- `risk_index` itself is
        // untouched (a pinned JSON value this packet was told not to move).
        // Two signals sharing an episode still fold to that episode's
        // *maximum* adjusted weight before `noisy_or`, same as before
        // (module doc "Correlated flags count once"); a `BTreeMap` keyed on
        // the episode's own ordinal keeps the fold deterministic the same
        // way `episodes.sort_by_key` above does for `risk_index`.
        let sheet_factors = crate::sheet::factors(sheet);
        let mut episode_bps: std::collections::BTreeMap<u8, u32> =
            std::collections::BTreeMap::new();
        for finding in &findings {
            let adjusted = adjusted_weight(finding.signal, &sheet_factors).bps();
            let entry = episode_bps.entry(finding.episode as u8).or_insert(0);
            *entry = (*entry).max(adjusted);
        }
        let score_bps = noisy_or(
            &episode_bps
                .into_values()
                .map(Weight::from_bps)
                .collect::<Vec<_>>(),
        );

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
        // M-D-0005, shadow only: computed beside `level`, published nowhere.
        let score_level = crate::verdict::level_from_score(sheet, score_bps, coverage);

        Self {
            findings,
            risk_index,
            score_bps,
            coverage,
            critical_gaps: sheet.unknown.clone(),
            level,
            admissible,
            score_level,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::{Factor, Grade, Signal};

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
            Signal::CorrelatedSelling,
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
        // `score_bps` folds research 0052 §3.1's per-signal base weights (no
        // factors on this bare sheet), combined by noisy-OR -- a different
        // number from `risk_index`'s coarser per-episode placeholder above,
        // by design (M-D-0002).
        let (a, b) = (
            signal_base_bps(Signal::LaunchBlockInStrongestBand),
            signal_base_bps(Signal::RepeatLauncher),
        );
        let expected = 10_000 - (10_000 - a) * (10_000 - b) / 10_000;
        assert_eq!(assessment.score_bps.bps(), expected);
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
                Signal::CorrelatedSelling,
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

    /// A skipped optional read is still a read that did not happen: it
    /// counts against coverage alongside every unknown, never offsetting one.
    #[test]
    fn a_skipped_read_and_an_unknown_both_lower_coverage() {
        let mut sheet = sheet_with(&[Signal::RepeatLauncher], &["holders"]);
        sheet.skipped = vec!["creator trade history".to_owned()];
        let assessment = Assessment::from(&sheet);
        assert_eq!(assessment.coverage.applicable, assessment.coverage.read + 2);
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
                "score_bps",
                "score_level",
            ]
        );
    }

    #[test]
    fn weight_bps_reads_back_what_went_in() {
        assert_eq!(Weight::from_bps(1_234).bps(), 1_234);
        assert_eq!(Weight::from_bps(10_001).bps(), 10_000);
    }

    #[test]
    fn noisy_or_saturates_a_deserialized_over_cap_weight() {
        let over: Weight = serde_json::from_str("20000").expect("a bare number");
        assert_eq!(noisy_or(&[over]), Weight::MAX);
    }

    #[test]
    fn noisy_or_dedup_case_from_research_0052() {
        // §4.1, case C after episode dedup: S2, S1, S5 -> 3,800; 2,700;
        // 1,800 bps.
        let weights = [
            Weight::from_bps(3_800),
            Weight::from_bps(2_700),
            Weight::from_bps(1_800),
        ];
        assert_eq!(noisy_or(&weights), Weight::from_bps(6_289));
    }

    #[test]
    fn noisy_or_raw_case_from_research_0052() {
        // §4.1, case C before episode dedup: adds S8 at 1,400 bps.
        let weights = [
            Weight::from_bps(3_800),
            Weight::from_bps(2_700),
            Weight::from_bps(1_800),
            Weight::from_bps(1_400),
        ];
        assert_eq!(noisy_or(&weights), Weight::from_bps(6_809));
    }

    #[test]
    fn noisy_or_is_order_independent() {
        let ascending = [
            Weight::from_bps(1_400),
            Weight::from_bps(1_800),
            Weight::from_bps(2_700),
            Weight::from_bps(3_800),
        ];
        let shuffled = [
            Weight::from_bps(2_700),
            Weight::from_bps(1_400),
            Weight::from_bps(3_800),
            Weight::from_bps(1_800),
        ];
        let descending = [
            Weight::from_bps(3_800),
            Weight::from_bps(2_700),
            Weight::from_bps(1_800),
            Weight::from_bps(1_400),
        ];
        let expected = Weight::from_bps(6_809);
        assert_eq!(noisy_or(&ascending), expected);
        assert_eq!(noisy_or(&shuffled), expected);
        assert_eq!(noisy_or(&descending), expected);
    }

    #[test]
    fn noisy_or_never_lowers_when_a_weight_is_added() {
        let before = [Weight::from_bps(3_800), Weight::from_bps(2_700)];
        let after = [
            Weight::from_bps(3_800),
            Weight::from_bps(2_700),
            Weight::from_bps(1_800),
        ];
        assert!(noisy_or(&after) >= noisy_or(&before));

        // Also true for a weight far weaker than every existing one, and for
        // one far stronger -- the property holds regardless of where the
        // new weight lands once sorted.
        let with_weak_addition = [
            Weight::from_bps(3_800),
            Weight::from_bps(2_700),
            Weight::from_bps(10),
        ];
        assert!(noisy_or(&with_weak_addition) >= noisy_or(&before));

        let with_strong_addition = [
            Weight::from_bps(3_800),
            Weight::from_bps(2_700),
            Weight::from_bps(9_000),
        ];
        assert!(noisy_or(&with_strong_addition) >= noisy_or(&before));
    }

    #[test]
    fn noisy_or_empty_is_zero() {
        assert_eq!(noisy_or(&[]), Weight::ZERO);
    }

    #[test]
    fn noisy_or_single_weight_is_itself() {
        for bps in [1, 2_500, 5_000, 9_999] {
            let weight = Weight::from_bps(bps);
            assert_eq!(noisy_or(&[weight]), weight);
        }
    }

    #[test]
    fn noisy_or_saturates_at_ten_thousand() {
        let weights = [Weight::MAX, Weight::from_bps(2_500)];
        assert_eq!(noisy_or(&weights), Weight::MAX);
    }

    #[test]
    fn weight_clamps_above_ten_thousand() {
        assert_eq!(Weight::from_bps(10_001), Weight::MAX);
        assert_eq!(Weight::from_bps(u32::MAX), Weight::MAX);
    }

    fn self_reported(delta_bps: i32) -> Factor {
        Factor {
            signal: Signal::RepeatLauncher,
            name: "self reported".to_owned(),
            delta_bps,
            grade: Grade::SelfReported,
            evidence: "the creator's own claim".to_owned(),
        }
    }

    fn measured(delta_bps: i32) -> Factor {
        Factor {
            signal: Signal::RepeatLauncher,
            name: "measured".to_owned(),
            delta_bps,
            grade: Grade::Measured,
            evidence: "counted from the record".to_owned(),
        }
    }

    // Research 0052 §3.3: "Self-reported factors ... may never raise" a
    // signal above its base. A positive self-reported delta must be dropped
    // entirely, not merely capped below the raise it claims.
    #[test]
    fn factors_never_raise_from_self_reported() {
        let base = signal_base_bps(Signal::RepeatLauncher);
        let raising = [self_reported(500)];
        assert_eq!(
            adjusted_weight(Signal::RepeatLauncher, &raising).bps(),
            base
        );

        let lowering = [self_reported(-150)];
        assert_eq!(
            adjusted_weight(Signal::RepeatLauncher, &lowering).bps(),
            base - 150
        );
    }

    // Research 0052 §3.3: self-reported factors together may lower a signal
    // by at most 200 bps. -200 total is the boundary case that must still
    // apply in full; -201 must clamp to exactly -200, not -201.
    #[test]
    fn self_reported_total_holds_at_the_two_hundred_cap() {
        let base = i64::from(signal_base_bps(Signal::RepeatLauncher));

        let at_cap = [self_reported(-200)];
        assert_eq!(
            adjusted_weight(Signal::RepeatLauncher, &at_cap).bps(),
            u32::try_from(base - 200).expect("base exceeds 200")
        );

        let past_cap = [self_reported(-201)];
        assert_eq!(
            adjusted_weight(Signal::RepeatLauncher, &past_cap).bps(),
            u32::try_from(base - 200).expect("base exceeds 200"),
            "a -201 bps self-reported claim must clamp to the -200 cap, not apply in full"
        );
    }

    // Research 0052 §3.3: "No factor set may take a signal below 25% of its
    // base." Landing exactly on the floor from measured/inferred deltas is
    // allowed unclamped; one bps past it must clamp back up to the floor.
    #[test]
    fn measured_factors_hold_at_the_twenty_five_percent_floor() {
        let base = i64::from(signal_base_bps(Signal::RepeatLauncher));
        let floor = base * 25 / 100;

        let at_floor = [measured(-750)];
        assert_eq!(
            adjusted_weight(Signal::RepeatLauncher, &at_floor).bps(),
            u32::try_from(floor).expect("floor is non-negative")
        );

        let past_floor = [measured(-751)];
        assert_eq!(
            adjusted_weight(Signal::RepeatLauncher, &past_floor).bps(),
            u32::try_from(floor).expect("floor is non-negative"),
            "one bps past the floor must clamp back up to 25% of base, not sit below it"
        );
    }

    // Research 0052 §6 case A: a 50 bps dev-buy share lowers
    // `CreatorBoughtOwnLaunch` by 400 (the `<100` bps band, see
    // `crate::sheet::factors`), read from the launch receipt's `CurveBuy`
    // `tokensOut` and mint `Transfer` -- no `eth_call` added. Case A's
    // published 600 also folds in a -200 self-reported factor this task does
    // not build, so only the Measured part (1,200 - 400 = 800) is asserted
    // here, through `adjusted_weight` the same way the score is computed.
    #[test]
    fn research_0052_case_a_fifty_bps_share_lowers_creator_bought_own_launch_to_800() {
        let base = signal_base_bps(Signal::CreatorBoughtOwnLaunch);
        assert_eq!(base, 1_200, "base weight is unchanged by this task");
        let factors = [Factor {
            signal: Signal::CreatorBoughtOwnLaunch,
            name: "dev buy share".to_owned(),
            delta_bps: -400,
            grade: Grade::Measured,
            evidence: "the launch receipt's mint and CurveBuy tokensOut".to_owned(),
        }];
        assert_eq!(
            adjusted_weight(Signal::CreatorBoughtOwnLaunch, &factors).bps(),
            800,
            "case A: 1,200 base - 400 (50 bps share, the <100 bps band) = 800"
        );
    }

    // Research 0052 §6 case B: an 800 bps dev-buy share raises
    // `CreatorBoughtOwnLaunch` by 800 (the 500..=999 bps band), landing
    // exactly on the case's published 2,000.
    #[test]
    fn research_0052_case_b_eight_hundred_bps_share_raises_creator_bought_own_launch_to_2000() {
        let factors = [Factor {
            signal: Signal::CreatorBoughtOwnLaunch,
            name: "dev buy share".to_owned(),
            delta_bps: 800,
            grade: Grade::Measured,
            evidence: "the launch receipt's mint and CurveBuy tokensOut".to_owned(),
        }];
        assert_eq!(
            adjusted_weight(Signal::CreatorBoughtOwnLaunch, &factors).bps(),
            2_000,
            "case B: 1,200 base + 800 (800 bps share, the 500..=999 bps band) = 2,000"
        );
    }
}
