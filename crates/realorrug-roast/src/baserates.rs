// SPDX-License-Identifier: Apache-2.0
//! Reading the published base-rate snapshot.
//!
//! The store's job in this product is **base rates, not lookup**. A per-mint
//! question is answered from the chain (see `realorrug-onchain`); the population it
//! is placed against is measured once, published as
//! `docs/research/data/0024-base-rates.json`, and read from disk in
//! microseconds.
//!
//! # Every figure carries the date it was measured
//!
//! This is the whole lesson of `0024`. `0008` measured these same quantities and
//! its headline was wrong by 2.7× nine days later, because the recipient count
//! is a **configuration** of whatever tool the launchers are running rather than
//! a law. A consumer that hard-codes any of these numbers is repeating that.
//!
//! So [`BaseRates::measured_on`] is not decoration, and
//! [`BaseRates::is_stale_at`] exists so a caller can refuse rather than quote a
//! month-old distribution as though it were current.
//!
//! # Rule 8: absent is a refusal to claim
//!
//! No snapshot means no population context — the reply says less, and says why.
//! It does **not** mean falling back on remembered numbers, which is how a
//! superseded figure gets published long after the note correcting it.

use serde::Deserialize;

use crate::firstparty::Chain;

/// Where the snapshot lives, relative to the repository root.
pub const DEFAULT_PATH: &str = "docs/research/data/0024-base-rates.json";

/// How old a snapshot may be before it is dropped rather than quoted.
///
/// Sixty days. This used to be fourteen -- `0008`'s figures were wrong by
/// 2.7× after nine days, so a two-week backstop looked generous -- but a
/// snapshot's own date already carries the caveat: every fact line built from
/// it says "as of `measured_on`" (`sheet.rs`'s `push_band` and
/// `push_base_rates`), so a reader is never told a stale number is fresh.
/// What a hard fourteen-day drop bought instead was silence: population
/// context vanished from every reply for however long a rebuild job was
/// broken, with nothing louder than a startup log line to say why. Sixty days
/// is long enough that only a genuinely broken rebuild trips it, which is what
/// this constant is for -- not a substitute for the scheduled re-run `0024`
/// asks for, a backstop against a job nobody noticed had stopped running.
pub const STALE_AFTER_DAYS: i64 = 60;

/// Why the snapshot could not be used.
#[derive(Debug, thiserror::Error)]
pub enum NotLoaded {
    /// The file was not there or could not be read.
    #[error("base rates not readable at {path}: {why}")]
    Unreadable {
        /// Where it was looked for.
        path: String,
        /// The underlying reason.
        why: String,
    },
    /// The file was there but is not the shape this expects.
    #[error("base rates malformed: {0}")]
    Malformed(String),
}

/// One band of the recipient distribution.
#[derive(Clone, Debug)]
pub struct Band {
    /// Its name, as the snapshot spells it.
    pub name: String,
    /// Lowest recipient count in the band.
    pub lo: u32,
    /// Highest recipient count in the band.
    pub hi: u32,
    /// Share of **all** launches whose block falls in this band.
    ///
    /// The number the public site prints as "70.5% of launches". It was
    /// deserialised and dropped until 2026-09-05, when the stats endpoint
    /// needed it and would otherwise have carried a copy of it.
    pub fires_on: f64,
    /// Share of never-graduated launches in this band.
    pub never_graduated: f64,
    /// Share of organic graduations in this band.
    pub organic: f64,
    /// Share of instant graduations in this band.
    pub instant: f64,
    /// Probability a launch in this band graduates instantly.
    pub p_instant: f64,
    /// How many times the population rate that is.
    pub x_base_instant: f64,
}

/// One notional band of the measured round trip.
#[derive(Clone, Debug)]
pub struct CostBand {
    /// The notional range, as text.
    pub band: String,
    /// Round-trip cost in basis points.
    pub round_trip: f64,
}

/// The measured round-trip cost, when the snapshot's chain has one.
///
/// `Option` on [`BaseRates::round_trip`], not a zero-filled struct, because
/// Robinhood Chain has no round-trip measurement yet (research 0024 measured
/// Solana/pump.fun fills). AGENTS.md rule 8: absent is not zero, and a
/// Robinhood snapshot that filled this with zeros would print "0 bps round
/// trip" as a fact rather than saying nothing.
#[derive(Clone, Debug)]
pub struct RoundTrip {
    /// The measured all-in round trip the kernel assumes.
    pub kernel: f64,
    /// The bar a strategy must clear.
    pub bar: f64,
    /// Round trip by notional.
    pub cost_bands: Vec<CostBand>,
}

/// What happens after graduation, from research 0011.
///
/// Carried in the snapshot with its own date rather than remembered in code,
/// so the public site states it from a file. `Option` on the snapshot because
/// an older snapshot without it is still a valid snapshot; a consumer that
/// needs the figure refuses when it is absent rather than filling it in.
#[derive(Clone, Debug)]
pub struct Aftermath {
    /// When it was measured, as `YYYY-MM-DD`.
    pub measured_on: String,
    /// Where an organic graduation ends, held from first fill to last price,
    /// in basis points. Negative.
    pub organic_median_bps: f64,
}

/// The chain a snapshot written before [`BaseRates::chain`] existed describes.
///
/// Every snapshot in the repository before 2026-09-17 is `0024`, a
/// Solana/pump.fun measurement -- so a file with no `chain` field is a Solana
/// one, not an unknown one. The alternative default (refuse to parse) would
/// break every existing consumer's fixture over a field none of them wrote
/// wrong; the alternative default `Robinhood` would misattribute the one
/// snapshot in the tree that predates the field.
const fn chain_before_the_field_existed() -> Chain {
    Chain::Solana
}

/// The published snapshot.
#[derive(Clone, Debug)]
pub struct BaseRates {
    /// Which chain this snapshot was measured on.
    ///
    /// Radar's Solana/pump.fun measurement and a Robinhood Chain one are two
    /// different populations that happen to share a schema. A consumer that
    /// read either snapshot without checking this field would eventually
    /// print the wrong chain's numbers as fact -- exactly what happened when
    /// `push_population` and `Signal::LaunchBlockInStrongestBand` ran for any
    /// chain whenever a snapshot was loaded at all. `sheet.rs`'s `build` reads
    /// this the same way it already reads `CreatorIndex::chain`: filtered
    /// against the token's own chain before anything downstream sees it.
    pub chain: Chain,
    /// When it was measured, as `YYYY-MM-DD`.
    pub measured_on: String,
    /// Research 0011's aftermath figure, when the snapshot carries it.
    pub aftermath: Option<Aftermath>,
    /// Launches the distribution was measured over.
    pub launches: u64,
    /// Share of all launches that graduate at all.
    pub base_rate_graduates: f64,
    /// Share of all launches that graduate instantly.
    pub base_rate_instant: f64,
    /// The recipient bands.
    pub bands: Vec<Band>,
    /// The measured round-trip cost, absent when this chain has none measured
    /// yet (Robinhood Chain, as of this snapshot's writing).
    pub round_trip: Option<RoundTrip>,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default = "chain_before_the_field_existed")]
    chain: Chain,
    measured_on: String,
    launch_block: RawLaunchBlock,
    /// Optional: a chain with no round-trip measurement yet (Robinhood Chain)
    /// omits this block entirely rather than filling it with zeros. Absent is
    /// not zero (AGENTS.md rule 8).
    #[serde(default)]
    round_trip_bps: Option<RawCost>,
    /// Optional, because a snapshot written before 2026-09-05 has none, and
    /// that snapshot is still valid for everything else.
    #[serde(default)]
    aftermath: Option<RawAftermath>,
}

#[derive(Deserialize)]
struct RawAftermath {
    measured_on: String,
    organic_median_bps: f64,
}

#[derive(Deserialize)]
struct RawLaunchBlock {
    launches: u64,
    base_rate_graduates: f64,
    base_rate_instant: f64,
    bands: Vec<RawBand>,
    populations: RawPopulations,
    histogram: serde_json::Value,
}

#[derive(Deserialize)]
struct RawPopulations {
    never_graduated: RawPopulation,
    organic: RawPopulation,
    instant: RawPopulation,
}

#[derive(Deserialize)]
struct RawPopulation {
    n: f64,
}

/// One band as the snapshot spells it.
///
/// `p_graduates` is deserialised but not published. It is kept because its
/// presence is what distinguishes this schema from an older one: a snapshot
/// missing it fails to parse rather than loading with a field quietly absent.
#[derive(Deserialize)]
struct RawBand {
    name: String,
    lo: u32,
    hi: u32,
    fires_on: f64,
    #[expect(
        dead_code,
        reason = "deserialised to validate the schema, not published"
    )]
    p_graduates: f64,
    p_instant: f64,
    x_base_instant: f64,
}

#[derive(Deserialize)]
struct RawCost {
    bar: f64,
    kernel_assumed: f64,
    by_notional: Vec<RawCostBand>,
}

#[derive(Deserialize)]
struct RawCostBand {
    band: String,
    round_trip: f64,
}

impl BaseRates {
    /// Reads the snapshot from a path.
    ///
    /// # Errors
    ///
    /// [`NotLoaded`] when the file cannot be read or is not the expected shape.
    pub fn load(path: &str) -> Result<Self, NotLoaded> {
        let text = std::fs::read_to_string(path).map_err(|e| NotLoaded::Unreadable {
            path: path.to_owned(),
            why: e.to_string(),
        })?;
        Self::parse(&text)
    }

    /// Parses the snapshot.
    ///
    /// # Errors
    ///
    /// [`NotLoaded::Malformed`] when the JSON is not the expected shape.
    pub fn parse(text: &str) -> Result<Self, NotLoaded> {
        let raw: Raw =
            serde_json::from_str(text).map_err(|e| NotLoaded::Malformed(e.to_string()))?;
        let lb = &raw.launch_block;

        let bands = lb
            .bands
            .iter()
            .map(|b| Band {
                name: b.name.clone(),
                lo: b.lo,
                hi: b.hi,
                fires_on: b.fires_on,
                never_graduated: share_in(
                    &lb.histogram,
                    b.lo,
                    b.hi,
                    0,
                    lb.populations.never_graduated.n,
                ),
                organic: share_in(&lb.histogram, b.lo, b.hi, 1, lb.populations.organic.n),
                instant: share_in(&lb.histogram, b.lo, b.hi, 2, lb.populations.instant.n),
                p_instant: b.p_instant,
                x_base_instant: b.x_base_instant,
            })
            .collect();

        Ok(Self {
            chain: raw.chain,
            measured_on: raw.measured_on,
            aftermath: raw.aftermath.map(|a| Aftermath {
                measured_on: a.measured_on,
                organic_median_bps: a.organic_median_bps,
            }),
            launches: lb.launches,
            base_rate_graduates: lb.base_rate_graduates,
            base_rate_instant: lb.base_rate_instant,
            bands,
            round_trip: raw.round_trip_bps.map(|rt| RoundTrip {
                kernel: rt.kernel_assumed,
                bar: rt.bar,
                cost_bands: rt
                    .by_notional
                    .iter()
                    .map(|c| CostBand {
                        band: c.band.clone(),
                        round_trip: c.round_trip,
                    })
                    .collect(),
            }),
        })
    }

    /// The band a recipient count falls in, if any.
    ///
    /// The bands in the snapshot overlap deliberately — "exactly six" sits
    /// inside "five to seven" — so this returns the **narrowest** match, which
    /// is the most specific true statement available about that count.
    #[must_use]
    pub fn band_for(&self, recipients: u32) -> Option<&Band> {
        self.bands
            .iter()
            .filter(|b| recipients >= b.lo && recipients <= b.hi)
            .min_by_key(|b| b.hi - b.lo)
    }

    /// The band most enriched for instant graduation, or `None` when the
    /// snapshot has no bands.
    ///
    /// This is what design 0009's hunter rule means by "the 10–13 band or
    /// above", said without the number: research 0024 is the record of the
    /// strongest band moving from six to ten-to-thirteen, and a rule that named
    /// the band would have fired on the wrong launches from the day it moved.
    /// Two bands tied on enrichment resolve to the one with the lower floor, so
    /// a tie widens the signal rather than narrowing it.
    #[must_use]
    pub fn strongest_band(&self) -> Option<&Band> {
        self.bands.iter().max_by(|a, b| {
            a.x_base_instant
                .total_cmp(&b.x_base_instant)
                .then_with(|| b.lo.cmp(&a.lo))
        })
    }

    /// Whether the snapshot is too old to quote, given today's date.
    ///
    /// Dates are compared as `YYYY-MM-DD` text converted to a day count. A date
    /// that will not parse is treated as **stale**, not as fresh: an unreadable
    /// measurement date is exactly the case where quoting the numbers anyway is
    /// how a superseded figure survives.
    #[must_use]
    pub fn is_stale_at(&self, today: &str) -> bool {
        match (days(&self.measured_on), days(today)) {
            (Some(then), Some(now)) => now - then > STALE_AFTER_DAYS,
            _ => true,
        }
    }
}

/// The share of a population sitting in a recipient band.
fn share_in(histogram: &serde_json::Value, lo: u32, hi: u32, column: usize, total: f64) -> f64 {
    if total <= 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    for r in lo..=hi {
        if let Some(row) = histogram.get(r.to_string()).and_then(|v| v.as_array())
            && let Some(n) = row.get(column).and_then(serde_json::Value::as_f64)
        {
            sum += n;
        }
    }
    sum / total
}

/// A day number from a `YYYY-MM-DD` date.
///
/// Rough — it treats every month as 31 days — which is fine for "is this more
/// than a fortnight old" and would not be fine for anything else. Written out
/// rather than pulling a date crate into a tree that has none.
fn days(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(y * 372 + m * 31 + d)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAPSHOT: &str = include_str!("../../../docs/research/data/0024-base-rates.json");

    #[test]
    fn the_published_snapshot_carries_the_band_shares_and_the_aftermath() {
        // Both were added for the public stats document on 2026-09-05, and
        // both are what the site prints: "70.5% of launches" and "organic
        // graduations end at a median of -3,228 bps". A snapshot that dropped
        // either would have the site fall back to its fixture and say so --
        // which is the right failure, and this is what makes it a loud one.
        let rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        let one_to_three = rates
            .bands
            .iter()
            .find(|b| b.name == "one to three")
            .expect("the refusal band");
        assert!((one_to_three.fires_on - 0.705).abs() < 1e-9);
        let aftermath = rates
            .aftermath
            .expect("research 0011's figure travels with the snapshot");
        assert!((aftermath.organic_median_bps - -3228.0).abs() < 1e-9);
        assert_eq!(aftermath.measured_on, "2026-08-26");

        // An older snapshot without the block still parses: the figure is
        // absent, not zero, and a consumer that needs it refuses.
        let mut older: serde_json::Value = serde_json::from_str(SNAPSHOT).expect("json");
        older.as_object_mut().expect("object").remove("aftermath");
        let older = BaseRates::parse(&older.to_string()).expect("still a snapshot");
        assert!(older.aftermath.is_none());
    }

    #[test]
    fn the_published_snapshot_parses() {
        // The file in the repository, not a fixture. A snapshot the code cannot
        // read is the LEARNINGS 1 shape: a document that outlived the thing it
        // describes.
        let rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        assert_eq!(rates.measured_on, "2026-09-03");
        assert_eq!(rates.launches, 17_497);
        assert!((rates.base_rate_instant - 0.011_830).abs() < 1e-6);
        // The three reconciled round-trip numbers, as docs/STATE.md carries
        // them. If the snapshot and that table ever disagree, the analyst and
        // the research notes publish different costs for the same trade.
        let round_trip = rates
            .round_trip
            .as_ref()
            .expect("Solana snapshot measures round trip");
        assert!((round_trip.kernel - 850.0).abs() < 1e-9);
        assert!((round_trip.bar - 456.0).abs() < 1e-9);
        assert_eq!(rates.chain, Chain::Solana);
        assert!(!rates.bands.is_empty());
    }

    #[test]
    fn a_recipient_count_lands_in_the_narrowest_band_that_holds_it() {
        let rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        // Six is inside both "exactly six" and "five to seven"; the specific
        // one is the more informative true statement.
        assert_eq!(rates.band_for(6).expect("a band").name, "exactly six");
        assert_eq!(rates.band_for(5).expect("a band").name, "five to seven");
        assert_eq!(rates.band_for(2).expect("a band").name, "one to three");
        assert_eq!(rates.band_for(11).expect("a band").name, "ten to thirteen");
        // A count in no band gets no claim rather than the nearest one.
        assert!(rates.band_for(40).is_none());
    }

    #[test]
    fn the_strongest_band_is_the_most_enriched_and_a_tie_goes_to_the_lower_floor() {
        // The published snapshot's strongest band is ten to thirteen at 10.1x,
        // not six at 4.4x -- which is research 0024's finding, and the reason
        // the hunter rule reads this rather than naming a band.
        let rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        assert_eq!(
            rates.strongest_band().expect("a band").name,
            "ten to thirteen"
        );

        // Listed weakest-first so that "first" is the wrong answer; and a tie
        // resolves to the lower floor, so it fires on more launches, not fewer.
        // The tied pair is listed lower-floor FIRST because `max_by` keeps the
        // last of equals: the first draft listed it last, dropped the tie-break
        // by hand, and the test still passed -- order was doing the rule's job.
        // Re-applied again with this order: the higher floor wins and the
        // assertion fails.
        let band = |name: &str, lo, hi, x| Band {
            name: name.to_owned(),
            lo,
            hi,
            fires_on: 0.0,
            never_graduated: 0.0,
            organic: 0.0,
            instant: 0.0,
            p_instant: 0.0,
            x_base_instant: x,
        };
        let mut rates = overlapping(1, 3, 6, 6);
        rates.bands = vec![
            band("weak", 1, 3, 0.0),
            band("strong-low", 5, 7, 7.0),
            band("strong-high", 10, 13, 7.0),
        ];
        assert_eq!(rates.strongest_band().expect("a band").name, "strong-low");

        rates.bands.clear();
        assert!(rates.strongest_band().is_none(), "no bands, no strongest");
    }

    /// Two bands holding the same count, where the wide one is listed first.
    ///
    /// The published snapshot happens to list its narrow bands early, so
    /// `min_by_key` returned the right answer even when the key was wrong --
    /// which is why every mutation of `b.hi - b.lo` survived. Order is not the
    /// rule; width is.
    fn overlapping(lo_wide: u32, hi_wide: u32, lo_narrow: u32, hi_narrow: u32) -> BaseRates {
        let band = |name: &str, lo, hi| Band {
            name: name.to_owned(),
            lo,
            hi,
            fires_on: 0.0,
            never_graduated: 0.0,
            organic: 0.0,
            instant: 0.0,
            p_instant: 0.0,
            x_base_instant: 0.0,
        };
        BaseRates {
            chain: Chain::Solana,
            measured_on: "2026-09-03".to_owned(),
            aftermath: None,
            launches: 1,
            base_rate_graduates: 0.0,
            base_rate_instant: 0.0,
            bands: vec![
                band("wide", lo_wide, hi_wide),
                band("narrow", lo_narrow, hi_narrow),
            ],
            round_trip: None,
        }
    }

    #[test]
    fn the_narrowest_band_wins_even_when_the_wide_one_is_listed_first() {
        // 4..=9 is width 5; 6..=6 is width 0. Listed wide-first, so returning
        // the first match rather than the narrowest gives the wrong answer.
        let rates = overlapping(4, 9, 6, 6);
        assert_eq!(rates.band_for(6).expect("a band").name, "narrow");

        // And the width has to be `hi - lo`, not `hi + lo`: here the wide band
        // 1..=9 sums to 10 and the narrow 5..=5 sums to 10 as well, so a sum
        // ties and yields the first. The difference does not tie.
        let rates = overlapping(1, 9, 5, 5);
        assert_eq!(rates.band_for(5).expect("a band").name, "narrow");

        // Nor `hi / lo`: 2..=8 divides to 4 and 6..=6 divides to 1, which is
        // the right order by accident. 1..=3 divides to 3 and 2..=2 to 1 --
        // also right. This pair is the one that inverts: 3..=4 divides to 1,
        // and 4..=4 divides to 1, a tie the wide band wins on order.
        let rates = overlapping(3, 4, 4, 4);
        assert_eq!(rates.band_for(4).expect("a band").name, "narrow");
    }

    #[test]
    fn the_bands_carry_the_shares_0024_measured() {
        let rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        let six = rates.band_for(6).expect("a band");
        // 0024's corrected table: 52/207 instant, 930/16,972 never-graduated.
        assert!((six.instant - 52.0 / 207.0).abs() < 1e-6, "{}", six.instant);
        assert!((six.never_graduated - 930.0 / 16_972.0).abs() < 1e-6);
        // And the figure 0008 got wrong is nowhere near what this now says.
        assert!(six.instant < 0.30, "0008's 68% must not reappear");
    }

    #[test]
    fn a_snapshot_whose_date_will_not_parse_is_stale_rather_than_fresh() {
        // The direction matters: treating an unreadable date as fresh is how a
        // superseded figure keeps getting published.
        let mut rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        rates.measured_on = "not a date".to_owned();
        assert!(rates.is_stale_at("2026-09-03"));
        rates.measured_on = "2026-09-03".to_owned();
        assert!(!rates.is_stale_at("2026-09-10"));
        assert!(rates.is_stale_at("2026-10-03"));
    }

    #[test]
    fn the_staleness_boundary_is_where_the_constant_says_it_is() {
        // `> STALE_AFTER_DAYS` rather than `>=`, and the day either side of it.
        // Without both, the comparison can move by one and nothing notices --
        // which is a sixty-day rule that is quietly sixty-one.
        let mut rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        rates.measured_on = "2026-07-01".to_owned();
        assert_eq!(STALE_AFTER_DAYS, 60, "the dates below are chosen for this");

        // Exactly sixty days on: still usable.
        assert!(!rates.is_stale_at("2026-08-30"));
        // Sixty-one: past the backstop.
        assert!(rates.is_stale_at("2026-08-31"));
    }

    #[test]
    fn a_snapshot_older_than_fourteen_days_is_still_usable_with_its_own_date() {
        // The old behaviour (`STALE_AFTER_DAYS == 14`) dropped a snapshot this
        // old outright. The new rule keeps using it -- a caller states its
        // own measurement date rather than a live figure, so an older-but-
        // still-under-backstop snapshot is not stale, and its date still
        // reaches the reply (`sheet.rs`'s "as of" lines read `measured_on`
        // straight off this struct, not off `is_stale_at`).
        let mut rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        rates.measured_on = "2026-08-01".to_owned();
        assert!(
            !rates.is_stale_at("2026-08-20"),
            "nineteen days old must not be stale under the sixty-day backstop"
        );
        assert_eq!(rates.measured_on, "2026-08-01");
    }

    #[test]
    fn a_date_that_is_not_a_date_is_rejected_on_both_fields() {
        // The guard is `month out of range OR day out of range`. With `AND`, a
        // date has to be wrong in *both* to be refused -- so "2026-13-05" would
        // parse, and a snapshot dated in a thirteenth month would read as fresh.
        let mut rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        rates.measured_on = "2026-09-01".to_owned();

        // Bad month, good day.
        assert!(rates.is_stale_at("2026-13-05"));
        assert!(rates.is_stale_at("2026-00-05"));
        // Good month, bad day.
        assert!(rates.is_stale_at("2026-09-32"));
        assert!(rates.is_stale_at("2026-09-00"));
        // And a real date is still readable, so the guard is not simply always
        // refusing.
        assert!(!rates.is_stale_at("2026-09-02"));
    }

    #[test]
    fn the_day_count_orders_dates_across_years_months_and_days() {
        // `y * 372 + m * 31 + d`. Every coefficient here has to be big enough
        // that the field below it cannot overtake it: a year must outrank any
        // month, and a month must outrank any day. The mutations that survived
        // were `*` to `+` and `+` to `*` on exactly these, which is the same
        // rule stated as arithmetic.
        let mut rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        rates.measured_on = "2026-01-01".to_owned();

        // A year later is stale, however early in the year it falls -- so the
        // year term dominates twelve months plus thirty-one days.
        assert!(rates.is_stale_at("2027-01-01"));
        // A month later is stale, however early in the month -- so the month
        // term dominates thirty-one days.
        assert!(rates.is_stale_at("2026-02-01"));
        // And the December-to-January step is one month, not a jump backwards.
        rates.measured_on = "2026-12-20".to_owned();
        assert!(!rates.is_stale_at("2026-12-30"));
        assert!(rates.is_stale_at("2027-01-20"));
    }

    #[test]
    fn a_malformed_snapshot_is_refused_rather_than_defaulted() {
        assert!(BaseRates::parse("{}").is_err());
        assert!(BaseRates::parse("not json").is_err());
    }

    #[test]
    fn a_snapshot_with_no_chain_field_is_read_as_the_solana_measurement_it_is() {
        // The published snapshot now states its chain explicitly ("solana"),
        // but every snapshot written before 2026-09-17 predates the field, and
        // this repository's own history is one of them. A missing field must
        // default to `Solana` -- the chain every pre-existing snapshot is --
        // not fail to parse and not default to `Robinhood`, which would
        // misattribute the one kind of file that predates this default.
        let mut value: serde_json::Value = serde_json::from_str(SNAPSHOT).expect("json");
        assert_eq!(
            value["chain"], "solana",
            "the published snapshot should say its own chain"
        );
        value.as_object_mut().expect("object").remove("chain");
        let rates = BaseRates::parse(&value.to_string()).expect("still a snapshot");
        assert_eq!(rates.chain, Chain::Solana);

        // And an explicit field is read, not ignored.
        value
            .as_object_mut()
            .expect("object")
            .insert("chain".to_owned(), serde_json::json!("robinhood"));
        let rates = BaseRates::parse(&value.to_string()).expect("still a snapshot");
        assert_eq!(rates.chain, Chain::Robinhood);
    }

    #[test]
    fn a_snapshot_with_no_round_trip_block_carries_no_cost_figures() {
        // Robinhood Chain has no round-trip measurement yet. A snapshot for it
        // omits `round_trip_bps` entirely rather than filling it with zeros --
        // absent is not zero (AGENTS.md rule 8), and `sheet.rs`'s `push_cost`
        // is only reachable when this is `Some`.
        let mut value: serde_json::Value = serde_json::from_str(SNAPSHOT).expect("json");
        value
            .as_object_mut()
            .expect("object")
            .remove("round_trip_bps");
        let rates = BaseRates::parse(&value.to_string()).expect("still a snapshot");
        assert!(rates.round_trip.is_none());
        // Everything else the snapshot carries is unaffected.
        assert!(!rates.bands.is_empty());
    }
}
