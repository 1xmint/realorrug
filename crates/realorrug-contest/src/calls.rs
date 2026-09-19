// SPDX-License-Identifier: Apache-2.0
//! Scoring the daily five: settled calls in, points, the luck line and a
//! ranking out. Design [0028](../../../docs/design/0028-the-daily-five.md)
//! sections 3, 4 and 6.
//!
//! # Why it is pure
//!
//! Same shape as [`crate::score`] and `radar-risk`: no clock, no network, no
//! key. The caller supplies the time a call was made, the coin's outcome and
//! what it knows about the caller's account; this module returns the same
//! ranking on any machine given the same input, in any order the calls
//! arrive in (test `same_calls_shuffled_same_ranking` is the property that
//! matters: replaying a week twice must agree).
//!
//! # No floating point decides a rank
//!
//! A call's points are whole basis points, and a player's total is an `i64`
//! sum of them: an integer, exactly reproducible. The luck line (design
//! 0028 §4) is defined with a `ln`, which genuinely needs a float, but the
//! float is used **once per ranking** to build a threshold, never per call
//! and never to accumulate a total. [`clears_luck_line`] explains the bound
//! that keeps that one float from being able to flip a real decision.
//!
//! # Unknown is not eligible
//!
//! Same rule as [`crate::score`]: an account whose age could not be
//! established does not rank, and says why rather than being admitted on the
//! assumption that it is old enough (score.rs's "Unknown is not eligible",
//! itself rule 9). A stated exclusion, not a dropped row.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// An odds value in basis points, `0..=10_000`. Refuses anything above that:
/// a caller that passes a percentage times 100 by mistake, or a stray
/// fraction, is refused rather than scored as a coin certain to do something
/// impossible.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct Odds(u16);

/// The top of the basis-point scale. 10 000 basis points is 100%.
pub const MAX_ODDS_BP: u16 = 10_000;

impl Odds {
    /// Basis points, refusing anything above [`MAX_ODDS_BP`].
    pub fn new(basis_points: u16) -> Result<Self, OddsRefused> {
        if basis_points > MAX_ODDS_BP {
            return Err(OddsRefused { basis_points });
        }
        Ok(Self(basis_points))
    }

    /// The value, in basis points.
    #[must_use]
    pub const fn basis_points(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for Odds {
    type Error = OddsRefused;

    fn try_from(basis_points: u16) -> Result<Self, Self::Error> {
        Self::new(basis_points)
    }
}

impl From<Odds> for u16 {
    fn from(odds: Odds) -> Self {
        odds.0
    }
}

/// An odds value above 100%. Refused, not clamped: a clamp would silently
/// score a bad input as a certainty, and the bug that produced it would never
/// surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OddsRefused {
    /// The value that was refused.
    pub basis_points: u16,
}

impl std::fmt::Display for OddsRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} basis points is above {MAX_ODDS_BP}, the top of the odds scale",
            self.basis_points
        )
    }
}

impl std::error::Error for OddsRefused {}

/// A call: which way the player bet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    /// The player called it a rug.
    Rug,
    /// The player called it real.
    Real,
}

/// What the chain settled, at the window's close (design 0028 §2.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Outcome {
    /// The code-computed level reached `Rugged` inside the window.
    Rugged,
    /// The window closed without that.
    Stood,
}

/// One settled call, as the caller read it back from the log.
///
/// Pure crates do not read the platform or the chain: the caller settles the
/// coin, reads the player's account age, and passes both in. There is no
/// clock here, so `called_at` is the caller's evidence, the same way
/// `score::rank` trusts the week it is given rather than re-deriving it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettledCall {
    /// The player: the X user id.
    pub player: String,
    /// The coin the call was about.
    pub coin_id: String,
    /// Who launched it. Used for the "many creators" guard (design 0028 §6).
    pub creator_id: String,
    /// Which way the player called it.
    pub side: Side,
    /// The odds at the moment of listing -- fixed then, and stored beside the
    /// call, never recomputed later (design 0028 §3).
    pub q: Odds,
    /// What the chain settled.
    pub outcome: Outcome,
    /// When the call was made, seconds since the epoch. Supplied by the
    /// caller; this crate has no clock.
    pub called_at: u64,
    /// The player's account age in days, if it could be established.
    ///
    /// `None` means the caller could not read it, and such a player never
    /// ranks: see [`Excluded::AccountAgeUnknown`] and `score::Rules`'s "Unknown
    /// is not eligible". This mirrors `Rules::min_account_age_days`
    /// semantics from [`crate::score`] rather than reusing that type
    /// directly, because the daily five's eligibility gate (settled calls,
    /// distinct creators, age) is its own rule with its own thresholds, not
    /// the weekly contest's operator list and cooldown.
    pub account_age_days: Option<u32>,
}

/// Points for one call, in whole basis points (design 0028 §3).
///
/// `rug`+`Rugged` and `real`+`Stood` are the sides that were right; a call
/// that guessed the odds exactly right earns nothing on average, only being
/// right about `p` against the bot's stated `q` does (the table in design
/// 0028 §3). `i64` because the widest possible run (many thousands of
/// calls at up to 10 000 each) is nowhere near overflowing it.
#[must_use]
pub fn points(side: Side, q: Odds, outcome: Outcome) -> i64 {
    let q = i64::from(q.basis_points());
    let against_q = i64::from(MAX_ODDS_BP) - q;
    match (side, outcome) {
        (Side::Rug, Outcome::Rugged) => against_q,
        (Side::Rug, Outcome::Stood) => -q,
        (Side::Real, Outcome::Rugged) => -against_q,
        (Side::Real, Outcome::Stood) => q,
    }
}

/// The per-call contribution to a zero-edge player's variance:
/// `q(10 000 − q)`, an integer, summed exactly over a player's calls.
///
/// This is the binomial variance of one call's points under the odds the bot
/// itself published (design 0028 §4): if `p = q`, the call is a fair bet with
/// this spread. Fits comfortably in `u64` (`q(10 000 - q) <= 2.5 * 10^7`).
#[must_use]
fn call_variance(q: Odds) -> u64 {
    let q = u64::from(q.basis_points());
    q * (u64::from(MAX_ODDS_BP) - q)
}

/// Why a player does not rank this run. Stated beside the player, never
/// silently dropped (score.rs's "exclusions are stated, not silent").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Excluded {
    /// The account's age could not be established. Unknown is not eligible.
    AccountAgeUnknown,
    /// The account was younger than the rule allows.
    AccountTooNew {
        /// Its age, in days.
        days: u32,
    },
    /// Fewer than 20 settled calls (design 0028 §4).
    TooFewCalls {
        /// How many it had.
        settled: u32,
    },
    /// Fewer than 10 distinct creators called on (design 0028 §4 and §6).
    TooFewCreators {
        /// How many distinct creators it had.
        creators: u32,
    },
}

/// The eligibility gate: at least 20 settled calls, at least 10 distinct
/// creators, and the age rule met. Design 0028 §4 and §6.
pub const MIN_SETTLED_CALLS: u32 = 20;
/// See [`MIN_SETTLED_CALLS`].
pub const MIN_DISTINCT_CREATORS: u32 = 10;

/// One player's record over their settled calls.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerRecord {
    /// The X user id.
    pub player: String,
    /// Sum of points over every settled call, in basis points.
    pub total: i64,
    /// How many settled calls this player has.
    pub settled: u32,
    /// How many distinct creators those calls covered.
    pub distinct_creators: u32,
    /// `Σ q(10 000 − q)` over the player's calls: the spread a zero-edge
    /// player's total would have. Exact, integer, order-independent.
    pub variance: u128,
    /// `total / sqrt(variance)`: how many spreads above zero this player
    /// sits. `None` when `variance` is zero -- every one of the player's
    /// calls was made at `q = 0` or `q = 10 000`, a "certainty" the bot
    /// itself gave no spread to, so no luck-adjusted record can be computed
    /// from it. Used only to order players who already cleared the line by
    /// the exact integer test in [`clears_luck_line`]; never used to decide
    /// whether they cleared it.
    pub z: Option<f64>,
    /// The earliest `called_at` among this player's settled calls. A tie
    /// breaker only (design 0028 §4: "ties ... then the earlier first
    /// call").
    pub first_call_at: u64,
}

/// Whether a player's record clears the luck line, without computing `z`.
///
/// Design 0028 §4 defines the line as `z >= z* = sqrt(2 ln(N / 0.05))`.
/// Squaring both sides of `total / sqrt(variance) >= z*` and multiplying
/// through by `variance` (positive whenever this function is called) gives
/// the equivalent, division- and sqrt-free test `total^2 >= z*^2 * variance`,
/// which only needs one `f64` value (`z*^2`) rather than a `sqrt` per player.
///
/// `z*^2 = 2 ln(N / 0.05)` still needs a transcendental function, so this is
/// not free of floating point, but it is computed **once per ranking**
/// rather than once per player, and the two sides of the comparison
/// (`total^2` as an exact `i128`, cast to `f64`, against `z*^2 * variance`)
/// stay far enough below `f64`'s 2^53-bit exact-integer range for any
/// realistic call volume (`variance <= 2.5*10^7` per call; thousands of calls
/// would need to run for years) that a last-bit difference in `ln` between
/// platforms cannot change which side of `>=` the comparison lands on for
/// any record that is not itself within a few parts in 10^15 of the line --
/// and a record that close is, by construction, not distinguishable from
/// lucky noise anyway.
#[must_use]
fn clears_luck_line(total: i64, variance: u128, luck_line_sq: f64) -> bool {
    if total <= 0 || variance == 0 {
        return false;
    }
    let total_sq = i128::from(total) * i128::from(total);
    (total_sq as f64) >= luck_line_sq * (variance as f64)
}

/// `z*^2 = 2 ln(N / 0.05)`, the squared luck line for `n` ranking-eligible
/// players (design 0028 §4). `n` is always at least 1 where this is called
/// (an empty ranking has no line to clear), so `n / 0.05` is always `> 1`
/// and the log is always positive.
fn luck_line_sq(n: u32) -> f64 {
    2.0 * (f64::from(n) / 0.05).ln()
}

/// The result of scoring one run of settled calls (design 0028 §4).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DailyFiveRanking {
    /// Players who cleared the luck line, `z` highest first. Ties go to more
    /// settled calls, then the earlier first call, then the player id, so
    /// the order is total and replaying a week twice agrees.
    pub above_line: Vec<PlayerRecord>,
    /// Ranking-eligible players who did not clear the line, "within luck",
    /// ordered by total points, with the same tie breakers.
    pub within_luck: Vec<PlayerRecord>,
    /// Every player who did not reach the eligibility gate, with the reason.
    pub excluded: Vec<(String, Excluded)>,
}

impl DailyFiveRanking {
    /// The winner: the top player above the line, or `None` meaning the pot
    /// rolls over (design 0028 §4, "the pot").
    #[must_use]
    pub fn winner(&self) -> Option<&PlayerRecord> {
        self.above_line.first()
    }
}

/// A total order over player records, used for every place in this module
/// that must give the same order twice: `player` breaks every remaining tie,
/// so two runs over the same calls in any order agree exactly.
fn tie_break(a: &PlayerRecord, b: &PlayerRecord) -> std::cmp::Ordering {
    b.settled
        .cmp(&a.settled)
        .then_with(|| a.first_call_at.cmp(&b.first_call_at))
        .then_with(|| a.player.cmp(&b.player))
}

/// Scores a run of settled calls into the ranking design 0028 §4 describes.
///
/// `min_account_age_days` mirrors `score::Rules::min_account_age_days`: the
/// same floor the weekly contest already uses, passed in rather than
/// hard-coded so the two rules do not have to change together by accident.
#[must_use]
pub fn score_calls(calls: &[SettledCall], min_account_age_days: u32) -> DailyFiveRanking {
    let mut by_player: BTreeMap<&str, Vec<&SettledCall>> = BTreeMap::new();
    for call in calls {
        by_player
            .entry(call.player.as_str())
            .or_default()
            .push(call);
    }

    let mut excluded = Vec::new();
    let mut eligible = Vec::new();
    for (player, player_calls) in &by_player {
        match eligibility(player_calls, min_account_age_days) {
            Some(why) => excluded.push(((*player).to_string(), why)),
            None => eligible.push(record_for(player, player_calls)),
        }
    }
    // `String` ordering on the `BTreeMap` keys already made both loops above
    // deterministic; nothing here depends on the order calls arrived in.

    let n = u32::try_from(eligible.len()).unwrap_or(u32::MAX);
    let mut above_line = Vec::new();
    let mut within_luck = Vec::new();
    if n > 0 {
        let line_sq = luck_line_sq(n);
        for mut record in eligible {
            record.z = if record.variance > 0 {
                Some(record.total as f64 / (record.variance as f64).sqrt())
            } else {
                None
            };
            if clears_luck_line(record.total, record.variance, line_sq) {
                above_line.push(record);
            } else {
                within_luck.push(record);
            }
        }
    }

    above_line.sort_by(|a, b| {
        // `z` decides first; a `NaN` cannot occur (both operands are finite
        // whenever `z` is `Some`, which every record in `above_line` has,
        // since `clears_luck_line` already required `variance > 0`), but
        // `total_cmp` is used anyway so a future change to this function
        // cannot introduce a panic-on-uncomparable here.
        let by_z =
            b.z.unwrap_or(f64::NEG_INFINITY)
                .total_cmp(&a.z.unwrap_or(f64::NEG_INFINITY));
        by_z.then_with(|| tie_break(a, b))
    });
    within_luck.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| tie_break(a, b)));
    excluded.sort();

    DailyFiveRanking {
        above_line,
        within_luck,
        excluded,
    }
}

/// Why this player does not rank, or `None` when the gate is cleared.
///
/// Checks run in a fixed order -- age, then call count, then creator count --
/// so a player failing more than one is reported with the same reason on
/// every run, the same discipline `score::exclusion` uses.
fn eligibility(calls: &[&SettledCall], min_account_age_days: u32) -> Option<Excluded> {
    match calls.first().and_then(|c| c.account_age_days) {
        None => return Some(Excluded::AccountAgeUnknown),
        Some(days) if days < min_account_age_days => {
            return Some(Excluded::AccountTooNew { days });
        }
        Some(_) => {}
    }
    let settled = u32::try_from(calls.len()).unwrap_or(u32::MAX);
    if settled < MIN_SETTLED_CALLS {
        return Some(Excluded::TooFewCalls { settled });
    }
    let creators: BTreeSet<&str> = calls.iter().map(|c| c.creator_id.as_str()).collect();
    let creators_count = u32::try_from(creators.len()).unwrap_or(u32::MAX);
    if creators_count < MIN_DISTINCT_CREATORS {
        return Some(Excluded::TooFewCreators {
            creators: creators_count,
        });
    }
    None
}

/// Builds one player's record from their settled calls. `z` is left `None`
/// here; `score_calls` fills it in once it knows `variance`.
fn record_for(player: &str, calls: &[&SettledCall]) -> PlayerRecord {
    let mut total: i64 = 0;
    let mut variance: u128 = 0;
    let mut first_call_at = u64::MAX;
    let mut creators: BTreeSet<&str> = BTreeSet::new();
    for call in calls {
        total += points(call.side, call.q, call.outcome);
        variance += u128::from(call_variance(call.q));
        first_call_at = first_call_at.min(call.called_at);
        creators.insert(call.creator_id.as_str());
    }
    PlayerRecord {
        player: player.to_string(),
        total,
        settled: u32::try_from(calls.len()).unwrap_or(u32::MAX),
        distinct_creators: u32::try_from(creators.len()).unwrap_or(u32::MAX),
        variance,
        z: None,
        first_call_at,
    }
}

/// A dummy player's fixed strategy (design 0028 §5): the board's four live
/// tests of the odds, scored the same way a real player is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DummyStrategy {
    /// Calls rug on every coin.
    AlwaysRug,
    /// Calls real on every coin.
    AlwaysReal,
    /// Calls rug or real by a coin flip that depends only on the coin id, not
    /// on any RNG: `splitmix64` isn't pulled in as a dependency for this, a
    /// stable hash of the id's bytes is deterministic and cheaper.
    CoinFlip,
    /// Agrees with the bot: calls rug when `q >= 5 000`.
    AgreeWithBot,
}

/// One coin the dummy players are scored over: settled, with the odds the bot
/// gave it at listing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettledCoin {
    /// The coin id, used by [`DummyStrategy::CoinFlip`]'s hash.
    pub coin_id: String,
    /// The odds at listing.
    pub q: Odds,
    /// What the chain settled.
    pub outcome: Outcome,
}

impl DummyStrategy {
    /// The side this strategy calls on one coin.
    ///
    /// A pure function of the coin, never of position in a slice or of any
    /// external randomness: `CoinFlip` must give the same call for the same
    /// coin id on every run, or the dummy player's score would not replay.
    #[must_use]
    pub fn call(self, coin: &SettledCoin) -> Side {
        match self {
            Self::AlwaysRug => Side::Rug,
            Self::AlwaysReal => Side::Real,
            Self::CoinFlip => {
                if fnv1a64(coin.coin_id.as_bytes()).is_multiple_of(2) {
                    Side::Rug
                } else {
                    Side::Real
                }
            }
            Self::AgreeWithBot => {
                if coin.q.basis_points() >= MAX_ODDS_BP / 2 {
                    Side::Rug
                } else {
                    Side::Real
                }
            }
        }
    }

    /// Total points this strategy earns over a slice of settled coins.
    #[must_use]
    pub fn score(self, coins: &[SettledCoin]) -> i64 {
        coins
            .iter()
            .map(|coin| points(self.call(coin), coin.q, coin.outcome))
            .sum()
    }
}

/// A small, fixed, non-cryptographic hash (FNV-1a) used only to turn a coin
/// id into a deterministic coin flip. Not a dependency: the whole point of
/// `CoinFlip` is that it needs no RNG and no crate, just a stable function of
/// the id.
const fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(PRIME);
        i += 1;
    }
    hash
}

/// Scores all four dummy players (design 0028 §5) over the same coins.
#[must_use]
pub fn score_dummy_players(coins: &[SettledCoin]) -> BTreeMap<DummyStrategy, i64> {
    [
        DummyStrategy::AlwaysRug,
        DummyStrategy::AlwaysReal,
        DummyStrategy::CoinFlip,
        DummyStrategy::AgreeWithBot,
    ]
    .into_iter()
    .map(|strategy| (strategy, strategy.score(coins)))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-rolled, deterministic PRNG (splitmix64) -- not a dependency,
    /// just enough to draw reproducible test coins and outcomes. Any seed
    /// gives the same sequence on every run and every platform, which is the
    /// whole point of a property test that must not flake.
    struct SplitMix64(u64);

    impl SplitMix64 {
        fn new(seed: u64) -> Self {
            Self(seed)
        }

        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        /// A uniform value in `[0, 1)`.
        fn next_f64(&mut self) -> f64 {
            // Top 53 bits: exactly the mantissa `f64` can hold, so this is a
            // uniform draw with no bias from the conversion.
            (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
        }

        /// A basis-point value in `[low, high]`.
        fn next_bp(&mut self, low: u16, high: u16) -> Odds {
            let span = u64::from(high - low) + 1;
            let bp = u64::from(low) + self.next_u64() % span;
            Odds::new(bp as u16).expect("bp within range")
        }

        fn outcome_with_prob(&mut self, q: Odds) -> Outcome {
            if self.next_f64() < f64::from(q.basis_points()) / f64::from(MAX_ODDS_BP) {
                Outcome::Rugged
            } else {
                Outcome::Stood
            }
        }
    }

    // --- (a) calibrated average, and the exact zero-expectation identity ---

    #[test]
    fn expected_points_are_exactly_zero_when_calibrated() {
        // p = q: multiplying the two-outcome expectation through by 10 000
        // (the shared denominator) keeps everything an exact integer, so this
        // checks the identity design 0028 §3 states, not an approximation of
        // it.
        for bp in 0..=MAX_ODDS_BP {
            let q = Odds::new(bp).unwrap();
            let rug_numerator = i64::from(bp) * points(Side::Rug, q, Outcome::Rugged)
                + i64::from(MAX_ODDS_BP - bp) * points(Side::Rug, q, Outcome::Stood);
            assert_eq!(
                rug_numerator, 0,
                "rug side not calibrated to zero at q={bp}"
            );

            let real_numerator = i64::from(bp) * points(Side::Real, q, Outcome::Rugged)
                + i64::from(MAX_ODDS_BP - bp) * points(Side::Real, q, Outcome::Stood);
            assert_eq!(
                real_numerator, 0,
                "real side not calibrated to zero at q={bp}"
            );
        }
    }

    #[test]
    fn dummy_strategies_average_near_zero_when_calibrated() {
        let mut rng = SplitMix64::new(0xDA11_F15E_5EED_5EED);
        let n: u32 = 20_000;
        let coins: Vec<SettledCoin> = (0..n)
            .map(|i| {
                let q = rng.next_bp(1_000, 9_000);
                let outcome = rng.outcome_with_prob(q);
                SettledCoin {
                    coin_id: format!("coin-{i}"),
                    q,
                    outcome,
                }
            })
            .collect();

        let scores = score_dummy_players(&coins);
        // Per-call variance is at most q(10000-q) <= 2.5e7 (q in [1000,9000]
        // keeps it well above the tiny-variance extremes); over n iid calls
        // the mean's standard deviation is at most sqrt(2.5e7 / n) ~= 35.4
        // for n = 20 000. Six standard deviations is ~212; 300 leaves margin
        // without being so loose it would pass a broken sign or a dropped
        // factor.
        let bound = 300.0;
        for (strategy, total) in scores {
            let mean = total as f64 / f64::from(n);
            assert!(
                mean.abs() < bound,
                "{strategy:?} mean {mean} exceeds bound {bound}"
            );
        }
    }

    // --- (b) noise never clears the line; real edge clears it and wins ---

    fn creator_pool(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("creator-{i}")).collect()
    }

    fn zero_edge_player(rng: &mut SplitMix64, id: &str, creators: &[String]) -> Vec<SettledCall> {
        (0..25)
            .map(|i| {
                let q = rng.next_bp(2_000, 8_000);
                let outcome = rng.outcome_with_prob(q);
                let side = if rng.next_u64().is_multiple_of(2) {
                    Side::Rug
                } else {
                    Side::Real
                };
                SettledCall {
                    player: id.to_string(),
                    coin_id: format!("{id}-coin-{i}"),
                    creator_id: creators[i % creators.len()].clone(),
                    side,
                    q,
                    outcome,
                    called_at: i as u64,
                    account_age_days: Some(365),
                }
            })
            .collect()
    }

    #[test]
    fn noise_fails_the_line_and_real_edge_clears_it_first() {
        let creators = creator_pool(12);
        let mut rng = SplitMix64::new(7);
        let mut calls = Vec::new();
        for p in 0..1_000 {
            calls.extend(zero_edge_player(&mut rng, &format!("noise-{p}"), &creators));
        }

        // A caller who knows the outcome 80% of the time, over 60 calls on
        // 30 creators, all at even odds -- a clean, large edge that swamps
        // any noise total.
        let edge_creators = creator_pool(30);
        let q = Odds::new(5_000).unwrap();
        let mut edge_calls = Vec::new();
        for i in 0..60u64 {
            let side = if i % 2 == 0 { Side::Rug } else { Side::Real };
            let correct = rng.next_f64() < 0.8;
            let outcome = match (side, correct) {
                (Side::Rug, true) | (Side::Real, false) => Outcome::Rugged,
                (Side::Rug, false) | (Side::Real, true) => Outcome::Stood,
            };
            edge_calls.push(SettledCall {
                player: "edge".to_string(),
                coin_id: format!("edge-coin-{i}"),
                creator_id: edge_creators[i as usize % edge_creators.len()].clone(),
                side,
                q,
                outcome,
                called_at: i,
                account_age_days: Some(365),
            });
        }
        calls.extend(edge_calls);

        let ranking = score_calls(&calls, 30);
        assert!(
            ranking.above_line.len() <= 1,
            "more than the edge player cleared the line: {:?}",
            ranking
                .above_line
                .iter()
                .map(|r| &r.player)
                .collect::<Vec<_>>()
        );
        let winner = ranking.winner().expect("edge player should clear the line");
        assert_eq!(winner.player, "edge");
    }

    // --- (c) order independence ---

    #[test]
    fn same_calls_shuffled_same_ranking() {
        let creators = creator_pool(12);
        let mut rng = SplitMix64::new(99);
        let mut calls = Vec::new();
        for p in 0..20 {
            calls.extend(zero_edge_player(&mut rng, &format!("p-{p}"), &creators));
        }

        let original = score_calls(&calls, 30);

        // A fixed, non-random permutation: reverse then interleave, so the
        // shuffle itself is reproducible and the test does not depend on the
        // RNG draw order matching the one above.
        let mut shuffled = calls.clone();
        shuffled.reverse();
        let (a, b) = shuffled.split_at(shuffled.len() / 2);
        let interleaved: Vec<SettledCall> = b
            .iter()
            .zip(a.iter())
            .flat_map(|(x, y)| [x.clone(), y.clone()])
            .collect();

        let replayed = score_calls(&interleaved, 30);
        assert_eq!(original, replayed);
    }

    // --- (d) odds refuse anything above 100% ---

    #[test]
    fn odds_above_max_are_refused() {
        assert!(Odds::new(10_000).is_ok());
        assert_eq!(
            Odds::new(10_001),
            Err(OddsRefused {
                basis_points: 10_001
            })
        );
    }

    // --- (e) unknown age never ranks ---

    #[test]
    fn unknown_account_age_never_ranks() {
        let creators = creator_pool(12);
        let mut rng = SplitMix64::new(11);
        let mut calls = zero_edge_player(&mut rng, "mystery", &creators);
        for call in &mut calls {
            call.account_age_days = None;
        }

        let ranking = score_calls(&calls, 30);
        assert!(ranking.above_line.is_empty());
        assert!(ranking.within_luck.is_empty());
        assert_eq!(
            ranking.excluded,
            vec![("mystery".to_string(), Excluded::AccountAgeUnknown)]
        );
    }

    // --- (f) too few creators excludes even with enough calls ---

    #[test]
    fn too_few_creators_excludes_with_enough_calls() {
        let creators = creator_pool(3);
        let mut rng = SplitMix64::new(21);
        let calls = zero_edge_player(&mut rng, "narrow", &creators);
        assert_eq!(calls.len(), 25);

        let ranking = score_calls(&calls, 30);
        assert_eq!(
            ranking.excluded,
            vec![(
                "narrow".to_string(),
                Excluded::TooFewCreators { creators: 3 }
            )]
        );
    }
}
