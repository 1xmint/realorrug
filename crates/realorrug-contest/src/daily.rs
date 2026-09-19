// SPDX-License-Identifier: Apache-2.0
//! Picking the daily five: the day's launches in, up to five picks out.
//! Design [0028](../../../docs/design/0028-the-daily-five.md) §2.1 and §10
//! (step G3).
//!
//! # Why it is pure
//!
//! Same shape as [`crate::calls`]: no clock, no network, no chain read. The
//! caller has already read the day's launches -- from whichever chains the
//! bot reads -- and passes them in with the day's boundary; this module only
//! sorts and filters. Given the same launches and the same day, on any
//! machine, it returns the same five, in the same order, regardless of what
//! order the launches arrived in.
//!
//! # A pick needs a number the caller already agreed on
//!
//! Launches happen in a chain's own smallest unit -- lamports, wei, whatever
//! -- and those are not comparable across chains. Ranking "took in the most
//! money" across every chain the bot reads therefore needs one common-unit
//! figure, not each chain's raw intake. `Launch::intake_usd_cents` is that
//! figure: the caller converts at a stated moment (its own price read, with
//! its own timestamp) before calling in here, and this module trusts the
//! number it is given the same way [`crate::score::rank`] trusts the week it
//! is given rather than re-deriving it. A launch the caller could not price
//! is `None`, not zero -- an unpriced launch is unknown money, not a launch
//! that took in nothing (score.rs's "unknown is not eligible", applied to
//! money rather than an account).
//!
//! # Unknown is never picked
//!
//! A launch with unknown intake or unknown odds is excluded with the reason,
//! never picked with a stand-in value: a guess here would rank an unpriced or
//! unscored launch on data invented for it rather than money it is known to
//! have taken in.

use serde::{Deserialize, Serialize};

use crate::calls::Odds;

/// The bot's level for a coin at the moment it was listed. Mirrors
/// `realorrug_roast::verdict::Level`'s five variants; this crate cannot
/// depend on that one (it lives in the model-facing side of the tree, this
/// one is pure and has no path to it) so the names are kept in step by hand,
/// the same way `SettledCall::account_age_days`'s doc comment already keeps
/// `score::Rules::min_account_age_days`'s semantics in step without sharing
/// the type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Level {
    /// The code-computed level had already reached `Rugged`.
    Rugged,
    /// Two or more live-risk signals fired.
    RugMechanicsLive,
    /// Some signal fired, short of `RugMechanicsLive`.
    Sketchy,
    /// Every required fact was read; nothing fired.
    NothingUglyYet,
    /// A required fact could not be read.
    CantTell,
}

/// One launch, as the caller already read it.
///
/// Pure crates do not read the chain: the caller reads the day's launches
/// across every chain it watches, converts each one's intake to a common
/// unit at a stated moment, and passes the result in. There is no clock
/// here; `launched_at` and the day boundary passed to [`pick`] are the
/// caller's evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launch {
    /// The chain it launched on.
    pub chain: String,
    /// The token's address on that chain.
    pub token_address: String,
    /// Who launched it.
    pub creator_id: String,
    /// When it launched, seconds since the epoch.
    pub launched_at: u64,
    /// Money taken in over the day, in US cents, at whatever moment the
    /// caller priced it. `None` when the caller could not price it -- unknown
    /// money, never zero.
    pub intake_usd_cents: Option<u64>,
    /// The bot's level at listing.
    pub level: Level,
    /// The bot's odds at listing. `None` when the bot could not score it.
    pub q: Option<Odds>,
}

/// Why a launch was not picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotPicked {
    /// It launched outside the day the caller asked for.
    OutsideDay,
    /// It was already `Rugged` at the moment it was listed. It is not
    /// playable: a call on it would be settled before it could be made,
    /// since `Outcome::Rugged` (design 0028 §2.5) is exactly the state this
    /// launch was already in at listing.
    AlreadyRugged,
    /// The caller could not price its intake.
    UnknownIntake,
    /// The bot could not score it.
    UnknownOdds,
    /// It qualified but the day already has five picks that took in more.
    OutsideTopFive,
}

/// One of the day's five picks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    /// The chain it launched on.
    pub chain: String,
    /// The token's address on that chain.
    pub token_address: String,
    /// Who launched it.
    pub creator_id: String,
    /// When it launched, seconds since the epoch.
    pub launched_at: u64,
    /// Money taken in over the day, in US cents -- the figure the picks were
    /// ranked by.
    pub intake_usd_cents: u64,
    /// The bot's level at listing.
    pub level: Level,
    /// The bot's odds at listing.
    pub q: Odds,
}

/// The result of picking one day's five (design 0028 §2.1).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyFive {
    /// Up to five picks, ranked by intake, highest first.
    pub picks: Vec<Pick>,
    /// Every other launch the caller passed in, with why it was not picked.
    pub not_picked: Vec<(Launch, NotPicked)>,
}

/// How many launches make up a day's five.
pub const PICKS_PER_DAY: usize = 5;

/// Picks the day's five from the launches the caller already read.
///
/// `day_start` and `day_end` bound the window, inclusive of `day_start` and
/// exclusive of `day_end`, mirroring `Week::opens_at`/`closes_at`'s
/// half-open convention in [`crate::week`].
///
/// # Panics
///
/// Never, in practice: the internal `.expect` reads `launch.q` back out only
/// for launches the loop above already filtered to `Some` via
/// `NotPicked::UnknownOdds`, so every launch reaching that line has a known
/// `q`.
#[must_use]
pub fn pick(launches: &[Launch], day_start: u64, day_end: u64) -> DailyFive {
    let mut eligible: Vec<(u64, &Launch)> = Vec::new();
    let mut not_picked: Vec<(Launch, NotPicked)> = Vec::new();

    for launch in launches {
        if launch.launched_at < day_start || launch.launched_at >= day_end {
            not_picked.push((launch.clone(), NotPicked::OutsideDay));
            continue;
        }
        if launch.level == Level::Rugged {
            not_picked.push((launch.clone(), NotPicked::AlreadyRugged));
            continue;
        }
        let Some(intake) = launch.intake_usd_cents else {
            not_picked.push((launch.clone(), NotPicked::UnknownIntake));
            continue;
        };
        if launch.q.is_none() {
            not_picked.push((launch.clone(), NotPicked::UnknownOdds));
            continue;
        }
        eligible.push((intake, launch));
    }

    // Highest intake first; ties broken by chain then address, so the order
    // is total and two runs over differently ordered input agree (the same
    // discipline `score::rank` uses for its own tie breaks).
    eligible.sort_by(|(intake_a, a), (intake_b, b)| {
        intake_b
            .cmp(intake_a)
            .then_with(|| a.chain.cmp(&b.chain))
            .then_with(|| a.token_address.cmp(&b.token_address))
    });

    let (picked, rest) = eligible.split_at(eligible.len().min(PICKS_PER_DAY));
    let picks = picked
        .iter()
        .map(|(intake, launch)| Pick {
            chain: launch.chain.clone(),
            token_address: launch.token_address.clone(),
            creator_id: launch.creator_id.clone(),
            launched_at: launch.launched_at,
            intake_usd_cents: *intake,
            level: launch.level,
            q: launch.q.expect("filtered to Some above"),
        })
        .collect();
    not_picked.extend(
        rest.iter()
            .map(|(_, launch)| ((*launch).clone(), NotPicked::OutsideTopFive)),
    );

    DailyFive { picks, not_picked }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch(chain: &str, address: &str, launched_at: u64, intake_cents: u64, bp: u16) -> Launch {
        Launch {
            chain: chain.to_string(),
            token_address: address.to_string(),
            creator_id: format!("creator-{chain}-{address}"),
            launched_at,
            intake_usd_cents: Some(intake_cents),
            level: Level::NothingUglyYet,
            q: Some(Odds::new(bp).unwrap()),
        }
    }

    #[test]
    fn same_launches_any_order_same_five() {
        let launches = vec![
            launch("sol", "a", 10, 500, 1_000),
            launch("sol", "b", 11, 900, 1_000),
            launch("base", "c", 12, 700, 1_000),
            launch("sol", "d", 13, 300, 1_000),
            launch("base", "e", 14, 800, 1_000),
            launch("sol", "f", 15, 100, 1_000),
        ];
        let forward = pick(&launches, 0, 100);

        let mut reversed = launches.clone();
        reversed.reverse();
        let backward = pick(&reversed, 0, 100);

        assert_eq!(forward, backward);
        assert_eq!(
            forward
                .picks
                .iter()
                .map(|p| p.token_address.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "e", "c", "a", "d"]
        );
    }

    #[test]
    fn day_start_is_inclusive_day_end_is_exclusive() {
        // `launched_at < day_start` (not `<=`): a launch exactly at
        // `day_start` is inside the day, matching `day_end`'s exclusive
        // bound below to make the window half-open, not empty or double
        // counted at the seam between two days.
        let at_start = launch("sol", "at-start", 10, 500, 1_000);
        let before_start = launch("sol", "before-start", 9, 500, 1_000);
        let at_end = launch("sol", "at-end", 100, 500, 1_000);
        let launches = vec![at_start.clone(), before_start.clone(), at_end.clone()];

        let five = pick(&launches, 10, 100);
        assert_eq!(five.picks.len(), 1);
        assert_eq!(five.picks[0].token_address, "at-start");
        assert_eq!(
            five.not_picked,
            vec![
                (before_start, NotPicked::OutsideDay),
                (at_end, NotPicked::OutsideDay),
            ]
        );
    }

    #[test]
    fn unknown_intake_never_picked() {
        let mut unpriced = launch("sol", "x", 10, 0, 1_000);
        unpriced.intake_usd_cents = None;
        let launches = vec![launch("sol", "y", 10, 1, 1_000), unpriced.clone()];

        let five = pick(&launches, 0, 100);
        assert_eq!(five.picks.len(), 1);
        assert_eq!(five.picks[0].token_address, "y");
        assert_eq!(five.not_picked, vec![(unpriced, NotPicked::UnknownIntake)]);
    }

    #[test]
    fn ties_are_deterministic() {
        let launches = vec![
            launch("sol", "b", 10, 500, 1_000),
            launch("sol", "a", 10, 500, 1_000),
            launch("base", "a", 10, 500, 1_000),
        ];
        let a = pick(&launches, 0, 100);
        let mut shuffled = launches.clone();
        shuffled.rotate_left(1);
        let b = pick(&shuffled, 0, 100);

        assert_eq!(a, b);
        assert_eq!(
            a.picks
                .iter()
                .map(|p| (p.chain.as_str(), p.token_address.as_str()))
                .collect::<Vec<_>>(),
            vec![("base", "a"), ("sol", "a"), ("sol", "b")]
        );
    }

    #[test]
    fn fewer_than_five_eligible_never_pads() {
        let launches = vec![
            launch("sol", "a", 10, 500, 1_000),
            launch("sol", "b", 11, 900, 1_000),
        ];
        let five = pick(&launches, 0, 100);
        assert_eq!(five.picks.len(), 2);
        assert!(five.not_picked.is_empty());
    }

    #[test]
    fn outside_day_never_picked() {
        let launches = vec![
            launch("sol", "a", 5, 900, 1_000),
            launch("sol", "b", 50, 900, 1_000),
        ];
        let five = pick(&launches, 10, 100);
        assert_eq!(five.picks.len(), 1);
        assert_eq!(five.picks[0].token_address, "b");
        assert_eq!(
            five.not_picked,
            vec![(launches[0].clone(), NotPicked::OutsideDay)]
        );
    }

    #[test]
    fn already_rugged_at_listing_is_not_playable() {
        let mut rugged = launch("sol", "a", 10, 900, 1_000);
        rugged.level = Level::Rugged;
        let launches = vec![rugged.clone(), launch("sol", "b", 10, 100, 1_000)];
        let five = pick(&launches, 0, 100);
        assert_eq!(five.picks.len(), 1);
        assert_eq!(five.picks[0].token_address, "b");
        assert_eq!(five.not_picked, vec![(rugged, NotPicked::AlreadyRugged)]);
    }

    #[test]
    fn sixth_place_is_reported_outside_top_five() {
        let launches: Vec<Launch> = (0..6)
            .map(|i| launch("sol", &format!("t{i}"), 10, 1_000 - i, 1_000))
            .collect();
        let five = pick(&launches, 0, 100);
        assert_eq!(five.picks.len(), 5);
        assert_eq!(five.not_picked.len(), 1);
        assert_eq!(five.not_picked[0].0.token_address, "t5");
        assert_eq!(five.not_picked[0].1, NotPicked::OutsideTopFive);
    }
}
