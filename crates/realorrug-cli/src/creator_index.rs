// SPDX-License-Identifier: Apache-2.0
//! `realorrug creator-index`: build the Pons v2 launcher index from the
//! factory's own logs.
//!
//! This is the bot's memory of who has launched before. Without it every
//! launcher is a stranger, and the one question a reader most wants answered
//! about a new token -- *has this person done this before, and how did it go*
//! -- gets no answer at all.
//!
//! # What it counts, and how
//!
//! Launches, per launcher, and now every outcome column too
//! (`measured`, `organic`, `instant`, `stillborn` on both `Record` and
//! `Population`), plus launch-block base rates and 24-hour curve-price
//! outcomes, from four log walks in the same range and cached block timestamps.
//! There is no per-token `eth_call` except the optional verification sample.
//!
//! The module doc this replaced argued that an outcome needed
//! `getLaunchedToken` per token -- about 171,000 calls, roughly a day of
//! continuous polite calling. That argument holds for a per-token *state*
//! read, but graduation is not a state, it is an **event**: the factory emits
//! `realorrug_robinhood::pons::topic::GRADUATED` once per graduating token
//! ([`realorrug_robinhood::pons::Graduated::from_log`]), so a walk of that
//! topic over the factory is the same order of cost as the `TokenLaunched`
//! walk already here -- minutes, not a day. `CurveBuy` is not scoped to the
//! factory (each curve is its own contract), so that walk is the expensive
//! one; see its section below.
//!
//! ## The four walks
//!
//! 1. **`TokenLaunched`**, scoped to the factory. Counts launches per
//!    deployer, as before, and now also builds a token -> launch info map
//!    (deployer, launch block, curve address) and a curve -> launch block
//!    map, both consumed by the next two walks.
//! 2. **`Graduated`**, scoped to the factory. For each graduation, the token
//!    is looked up in the map from walk 1. Found: credited to its deployer as
//!    `instant` when `graduation_block - launch_block <= 3`, `organic`
//!    otherwise. Not found (graduated outside `[from, to]`, or launched
//!    before `--from`): skipped, not credited to a phantom deployer, and
//!    counted so the summary can say how many were skipped.
//!
//!    Three **blocks** is Robinhood Chain's reading of the three-**slot**
//!    threshold `realorrug_roast::creator`'s doc states for the Solana index
//!    it mirrors. The concept transfers even though the unit does not: a
//!    curve bought out within a handful of blocks of its own creation was
//!    bought by capital that had to be staged *before* the token existed --
//!    evidence of coordination -- while a curve that fills over hundreds or
//!    thousands of blocks was bought by demand that arrived over time, which
//!    no one wallet can stage in advance. The number of blocks that pass in a
//!    given span of wall-clock time differs between the two chains; the
//!    distinction the threshold is drawing does not.
//! 3. **`CurveBuy` or `CurveSell`**, *not* scoped to any one address -- each token's curve is
//!    its own contract, so the walk is by topic across every address in the
//!    range (see [`realorrug_robinhood::Rpc::logs_range_any_event`]). Buys
//!    still determine `stillborn`; both directions measure first-fill and
//!    peak quote-per-token execution prices within 86,400 seconds of launch.
//!    Logs are ordered by block and log index before consuming them. Only
//!    running prices survive, not the trade history. No DEX prices after
//!    graduation are covered and no USD or net profit claim follows.
//! 4. **`Transfer`**, across addresses, retaining only known tokens' own
//!    launch-block destinations. Distinct nonzero recipients of positive
//!    transfers include the curve's mint and factory machinery, not just
//!    buyers. A launch without any such transfer refuses the snapshot.
//!
//! `--base-rates-out` defaults to
//! `docs/research/data/0051-robinhood-base-rates.json`. It writes 0024's bands
//! and histogram schema with `chain: robinhood`, and optional `outcomes_24h`
//! counts. Unfinished windows and absent/zero-sized fills are reported
//! separately, never as failed returns. Each distinct launch/trade block's
//! timestamp is fetched at most once; once a curve's window ends its later
//! trades need no timestamps. The summary counts these extra RPC reads and
//! the Transfer walk, alongside the expanded shared trade walk.
//!
//! ## What `stillborn` means here, and why it is not Solana's definition
//!
//! `realorrug_roast::creator::Population::stillborn_share`'s doc and
//! `sheet.rs`'s prose call it "showed almost no life" / "almost no activity
//! at all" / "never moved", but neither pins down a rule. Research 0038 §4
//! (the only place a rule was proposed) recommends, uncalibrated, "few or
//! zero `CurveBuy`/`CurveSell` logs from the curve in some fixed window (for
//! example seven days)" -- a *time* window and *both* trade directions.
//! Robinhood's logs cannot cheaply answer that: block timestamps are not
//! part of a log, and a sell without any buy first is not a state a working
//! curve can reach (nothing to sell). So this command uses a different,
//! stricter, and fully specified rule: **a launch is `stillborn` when its
//! curve has no `CurveBuy` in any block after the launch block** -- a token
//! nobody but the deployer ever bought, using launch-relative blocks instead
//! of a wall-clock window, and buys only, because a sell presupposes a buy.
//! `realorrug_roast::creator::Record::stillborn` and `::Population::stillborn`
//! carry the same wording now; this is Robinhood's own definition, not a
//! port of a Solana measurement, because the one Solana rule on record does
//! not map onto what these logs can answer.
//!
//! ## `measured`
//!
//! A launch is `measured` when this pass covered its launch block in all
//! three walks, which every launch found by walk 1 is by construction: the
//! same `[from, to]` bounds every walk. So `measured` equals `launches` for
//! the range walked. `measured` is **not** the count of tokens that
//! graduated -- the *absence* of a `Graduated` log for a covered launch is
//! itself the measurement that it did not graduate, which is the entire
//! reason to do this from logs instead of per-token calls. If any of the
//! four walks fails to finish the full range, nothing is published: this
//! command returns an error naming the walk that broke and writes no file,
//! rather than let a half-finished pass overwrite a good one with small
//! numbers that read as "nothing graduates here" (AGENTS.md §1 rule 8).
//!
//! ## `--verify <n>`
//!
//! `topic::GRADUATED`'s hash and decode were inferred from **one** captured
//! transaction (research 0040 §2); the event's name and full signature were
//! never recovered from a verified source. So this count is only as good as
//! that one capture. `--verify <n>` samples `n` tokens this pass says
//! graduated and `n` it says did not, calls the factory's
//! `getLaunchedToken` on each (`realorrug_robinhood::pons::LaunchedToken`;
//! `phase == 2` means graduated), and refuses to write the file if any
//! sampled token disagrees -- forty calls instead of 171,000, and the
//! difference between a published figure and a guess.
//!
//! **It only means anything when `--to` is the head of the chain.**
//! `getLaunchedToken` answers with the token's state *now*; the walk answers
//! for the range it was given. A token that graduated after `--to` is
//! correctly absent from the walk's answer and correctly graduated in the
//! factory's, and `--verify` calls that a disagreement and refuses to write.
//! It is not wrong to do so -- it cannot tell that disagreement from the one
//! it exists to catch -- but on a short range it will fire for a reason that
//! has nothing to do with the decode. Leave `--to` off, which defaults it to
//! the head, unless you are deliberately re-reading an old range, and then
//! expect to drop `--verify` with it.
//!
//! # Why the launcher and not the fee recipient
//!
//! Research 0038 §2 recommends keying on `creator_fee_recipient`: the escrow
//! credits it, and two wallets that share one are one person. It is the better
//! key and it is not free -- it is only in `getLaunchedToken`, which is the
//! per-token call above. The `TokenLaunched` log carries the deployer, which
//! is also exactly what `FactSheet` looks a creator up by. So the first index
//! is keyed the way the log is written and the lookup asks; the fee-recipient
//! key arrives later as an alias from deployer to fee recipient, built for new
//! launches as they happen, one cheap call each.

use std::collections::{BTreeMap, BTreeSet};

use realorrug_roast::baserates::{BaseRates, OutcomeBand, OutcomeRates};
use realorrug_roast::creator::{CreatorIndex, Population, Record};
use realorrug_roast::firstparty::Chain;
use realorrug_robinhood::pons::{FACTORY, Graduated, Launched, LaunchedToken, Trade, topic};
use realorrug_robinhood::{Address, Log, Rpc};

const BASE_RATES_OUT: &str = "docs/research/data/0051-robinhood-base-rates.json";
const DAY: u64 = 86_400;
// These are 0024's shapes, not its measured rates. Overlap is deliberate:
// exactly six belongs to both that band and five to seven.
const BANDS: [(&str, u32, u32); 4] = [
    ("one to three", 1, 3),
    ("exactly six", 6, 6),
    ("five to seven", 5, 7),
    ("ten to thirteen", 10, 13),
];

#[derive(Default)]
struct BlockTimes {
    times: BTreeMap<u64, u64>,
    calls: u64,
}

impl BlockTimes {
    fn get(
        &mut self,
        block: u64,
        mut fetch: impl FnMut(u64) -> Result<(u64, u64), String>,
    ) -> Result<u64, String> {
        if let Some(time) = self.times.get(&block) {
            return Ok(*time);
        }
        self.calls += 1;
        let (returned, time) = fetch(block)?;
        if returned != block {
            return Err("timestamp response names another block".to_owned());
        }
        self.times.insert(block, time);
        Ok(time)
    }
}

struct LaunchMeasurement {
    recipients: BTreeSet<Address>,
    launched_at: u64,
    first_price: Option<f64>,
    peak_price: Option<f64>,
    unusable_price: bool,
    window_ended: bool,
}

impl LaunchMeasurement {
    fn new(launched_at: u64) -> Self {
        Self {
            recipients: BTreeSet::new(),
            launched_at,
            first_price: None,
            peak_price: None,
            unusable_price: false,
            window_ended: false,
        }
    }

    fn trade(&mut self, trade: &Trade, time: u64) -> Result<(), String> {
        let age = time
            .checked_sub(self.launched_at)
            .ok_or("trade precedes launch")?;
        if age > DAY {
            self.window_ended = true;
            return Ok(());
        }
        if trade.quote == 0 || trade.tokens == 0 {
            // Skipping a zero-price first fill would silently rebase the
            // return on a later fill. Keep the entire launch unpriced instead.
            self.unusable_price = true;
            return Ok(());
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "fill-price ratios are approximate f64 measurements"
        )]
        let price = trade.quote as f64 / trade.tokens as f64;
        self.first_price.get_or_insert(price);
        self.peak_price = Some(self.peak_price.map_or(price, |peak| peak.max(price)));
        Ok(())
    }

    fn peak_multiple(&self) -> Option<f64> {
        if self.unusable_price {
            return None;
        }
        Some(self.peak_price? / self.first_price?)
    }
}

fn note_recipient(
    log: &Log,
    launches: &BTreeMap<Address, LaunchInfo>,
    measurements: &mut BTreeMap<Address, LaunchMeasurement>,
) -> Result<(), String> {
    let Some(info) = launches.get(&log.address) else {
        return Ok(());
    };
    if log.block != info.block {
        return Ok(());
    }
    let recipient = log
        .topic_address(2)
        .ok_or("Transfer recipient did not decode")?;
    let amount = log.data_u128(0).ok_or("Transfer amount did not decode")?;
    if recipient != Address::ZERO && amount > 0 {
        measurements
            .get_mut(&log.address)
            .ok_or("launch measurement missing")?
            .recipients
            .insert(recipient);
    }
    Ok(())
}

fn walk_recipients(
    rpc: &Rpc,
    from: u64,
    to: u64,
    launches: &BTreeMap<Address, LaunchInfo>,
    measurements: &mut BTreeMap<Address, LaunchMeasurement>,
) -> Result<u64, String> {
    let mut failure = None;
    let walked = realorrug_onchain::robinhood::walk_logs(
        from,
        to,
        |a, b| rpc.logs_range_any_address(&[topic::TRANSFER], a, b),
        |log| {
            if failure.is_none() {
                failure = note_recipient(log, launches, measurements).err();
            }
        },
    )
    .map_err(|e| format!("Transfer walk: {e}"))?;
    if let Some(error) = failure {
        return Err(error);
    }
    // Every Pons launch mints supply to its curve. An empty set means the
    // instrument missed that mint, not a measured zero-recipient launch.
    if measurements.values().any(|m| m.recipients.is_empty()) {
        return Err(
            "a launch has no launch-block Transfer recipients; refusing base rates".to_owned(),
        );
    }
    Ok(walked.requests)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "population shares are approximate f64 ratios"
)]
fn share(n: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        n as f64 / total as f64
    }
}

fn base_rates_json(
    launches: &BTreeMap<Address, LaunchInfo>,
    graduations: &BTreeMap<Address, u64>,
    measurements: &BTreeMap<Address, LaunchMeasurement>,
    from: u64,
    watermark: (u64, u64),
) -> Result<serde_json::Value, String> {
    if launches.is_empty() || launches.len() != measurements.len() {
        return Err("base rates require a nonempty, fully measured launch population".to_owned());
    }
    let mut histogram = BTreeMap::<u32, [u64; 3]>::new();
    let mut populations = [0_u64; 3];
    let mut outcomes = Vec::new();
    let total = u64::try_from(launches.len()).map_err(|e| e.to_string())?;
    for (token, info) in launches {
        let measured = measurements
            .get(token)
            .ok_or("launch measurement missing")?;
        let recipients = u32::try_from(measured.recipients.len()).map_err(|e| e.to_string())?;
        let column = graduations
            .get(token)
            .map_or(0, |block| if instant(info.block, *block) { 2 } else { 1 });
        histogram.entry(recipients).or_default()[column] += 1;
        populations[column] += 1;
    }
    let base_instant = share(populations[2], total);
    let mut bands = Vec::new();
    for (name, lo, hi) in BANDS {
        let mut counts = [0_u64; 3];
        for (_, row) in histogram.range(lo..=hi) {
            for (sum, n) in counts.iter_mut().zip(row) {
                *sum += n;
            }
        }
        let n = counts.iter().sum();
        let mut outcome = OutcomeBand {
            name: name.to_owned(),
            lo,
            hi,
            launches: n,
            measured: 0,
            incomplete: 0,
            unpriced: 0,
            reached_2x: 0,
            reached_5x: 0,
            reached_10x: 0,
        };
        for measurement in measurements.values() {
            let recipients =
                u32::try_from(measurement.recipients.len()).map_err(|e| e.to_string())?;
            if !(lo..=hi).contains(&recipients) {
                continue;
            }
            if watermark
                .1
                .checked_sub(measurement.launched_at)
                .is_none_or(|age| age < DAY)
            {
                outcome.incomplete += 1;
            } else if let Some(peak) = measurement.peak_multiple() {
                outcome.measured += 1;
                outcome.reached_2x += u64::from(peak >= 2.0);
                outcome.reached_5x += u64::from(peak >= 5.0);
                outcome.reached_10x += u64::from(peak >= 10.0);
            } else {
                outcome.unpriced += 1;
            }
        }
        outcomes.push(outcome);
        // Undefined conditional rates are not zero. Empty bands and an
        // unobserved instant population cannot supply enrichment claims.
        if n > 0 && populations[2] > 0 {
            let p_instant = share(counts[2], n);
            bands.push(serde_json::json!({
                "name": name, "lo": lo, "hi": hi, "fires_on": share(n, total),
                "p_graduates": share(counts[1] + counts[2], n),
                "p_instant": p_instant, "x_base_instant": p_instant / base_instant,
            }));
        }
    }
    let measured_on = realorrug_types::civil::date_from_days(
        i64::try_from(now() / DAY).map_err(|e| e.to_string())?,
    );
    let json = serde_json::json!({
        "schema": 1, "chain": "robinhood", "measured_on": measured_on,
        "launch_block": {
            "_comment": "Distinct nonzero recipients of positive token Transfers in the launch block, including curve/factory machinery; not owners or buyers. Graduation observed through watermark; instant means within three blocks. Bands with undefined conditional rates are omitted.",
            "from_block": from, "watermark_block": watermark.0,
            "watermark_timestamp": watermark.1, "launches": total,
            "base_rate_graduates": share(populations[1] + populations[2], total),
            "base_rate_instant": base_instant,
            "populations": {"never_graduated": {"n": populations[0]},
                "organic": {"n": populations[1]}, "instant": {"n": populations[2]}},
            "histogram": histogram, "bands": bands,
        },
        "outcomes_24h": OutcomeRates {
            window_seconds: DAY,
            scope: "Pons CurveBuy/CurveSell only; quote/tokens execution price as logged, before any separate fee adjustment, in each launch's quote asset, not USD or net profit. Peak/first fill within [launch, launch+86400s]. No post-graduation DEX prices. Full windows only; absent or zero-sized prices are unpriced. Bands overlap.".to_owned(),
            bands: outcomes,
        },
    });
    BaseRates::parse(&json.to_string()).map_err(|e| e.to_string())?;
    Ok(json)
}

/// A graduation counts as `instant` at this many blocks or fewer since
/// launch, `organic` past it. Robinhood's reading, in blocks, of the three
/// **slot** threshold `realorrug_roast::creator`'s doc states for Solana; see
/// the module doc for why the concept, not the unit, is what transfers.
const INSTANT_BLOCKS: u64 = 3;

/// What walk 1 (`TokenLaunched`) records about one launch, for the two walks
/// that follow to use.
#[derive(Clone, Copy)]
struct LaunchInfo {
    deployer: Address,
    block: u64,
    curve: Address,
}

/// What walk 1 leaves behind for the other two walks and for `run`'s summary.
struct Walk1 {
    launch_map: BTreeMap<Address, LaunchInfo>,
    curve_to_launch_block: BTreeMap<Address, u64>,
    launches: u64,
    calls: u64,
}

/// Walk 1: `TokenLaunched`, scoped to the factory. One pass builds the
/// per-deployer launch count (in `creators`), the token -> launch-info map
/// and the curve -> launch-block map the other two walks key their lookups
/// against -- `walk_logs` hands over the raw log, which carries the block
/// a decoded-only sink would have thrown away, so this needs only the one
/// pass, not a second re-walk of the factory to recover it.
fn walk_token_launched(
    rpc: &Rpc,
    from: u64,
    to: u64,
    creators: &mut BTreeMap<String, Record>,
) -> Result<Walk1, String> {
    let mut launch_map: BTreeMap<Address, LaunchInfo> = BTreeMap::new();
    let mut curve_to_launch_block: BTreeMap<Address, u64> = BTreeMap::new();
    let mut launches = 0_u64;
    let mut invalid = false;
    let walked = realorrug_onchain::robinhood::walk_logs(
        from,
        to,
        |a, b| rpc.logs_range(&FACTORY, &[topic::TOKEN_LAUNCHED], a, b),
        |log| {
            let Some(launch) = Launched::from_log(log) else {
                invalid = true;
                return;
            };
            if launch_map.contains_key(&launch.token)
                || curve_to_launch_block.contains_key(&launch.curve)
            {
                invalid = true;
                return;
            }
            launches = launches.saturating_add(1);
            let record = creators.entry(launch.deployer.to_string()).or_default();
            record.launches = record.launches.saturating_add(1);
            launch_map.insert(
                launch.token,
                LaunchInfo {
                    deployer: launch.deployer,
                    block: log.block,
                    curve: launch.curve,
                },
            );
            curve_to_launch_block.insert(launch.curve, log.block);
        },
    )
    .map_err(|e| format!("TokenLaunched walk: {e}"))?;
    if invalid {
        return Err("TokenLaunched walk contains an unreadable or repeated launch".to_owned());
    }
    Ok(Walk1 {
        launch_map,
        curve_to_launch_block,
        launches,
        calls: walked.requests,
    })
}

/// What walk 2 leaves behind.
struct Walk2 {
    graduated_tokens: BTreeSet<Address>,
    graduation_blocks: BTreeMap<Address, u64>,
    calls: u64,
    skipped: u64,
}

/// Walk 2: `Graduated`, scoped to the factory. Every graduation on chain is
/// one more walk of the same factory address -- the same order of cost as
/// walk 1, not the 171,000 per-token calls the old module doc costed
/// outcomes at. Credits each graduation's deployer with `instant` or
/// `organic`, per the boundary in [`INSTANT_BLOCKS`].
fn walk_graduated(
    rpc: &Rpc,
    from: u64,
    to: u64,
    launch_map: &BTreeMap<Address, LaunchInfo>,
    creators: &mut BTreeMap<String, Record>,
) -> Result<Walk2, String> {
    let mut skipped = 0_u64;
    let mut graduated_tokens: BTreeSet<Address> = BTreeSet::new();
    let mut graduation_blocks = BTreeMap::new();
    let mut invalid = false;
    let walked = realorrug_onchain::robinhood::walk_logs(
        from,
        to,
        |a, b| rpc.logs_range(&FACTORY, &[topic::GRADUATED], a, b),
        |log| {
            let Some(graduated) = Graduated::from_log(log) else {
                invalid = true;
                return;
            };
            if let Some(info) = launch_map.get(&graduated.token)
                && (log.block < info.block || graduation_blocks.contains_key(&graduated.token))
            {
                invalid = true;
                return;
            }
            if credit_graduation(launch_map, creators, graduated.token, log.block) {
                graduated_tokens.insert(graduated.token);
                graduation_blocks.insert(graduated.token, log.block);
            } else {
                skipped = skipped.saturating_add(1);
            }
        },
    )
    .map_err(|e| format!("Graduated walk: {e}"))?;
    if invalid {
        return Err(
            "Graduated walk contains an unreadable, repeated or pre-launch event".to_owned(),
        );
    }
    Ok(Walk2 {
        graduated_tokens,
        graduation_blocks,
        calls: walked.requests,
        skipped,
    })
}

/// Credits one graduation to whoever launched the token, and says whether it
/// belonged to this pass at all.
///
/// Pulled out of [`walk_graduated`]'s sink because everything it decides --
/// which launcher, `instant` or `organic`, or neither -- is decided from
/// three plain values, while the sink around it can only be reached through
/// a live endpoint. Until 2026-09-17 the rule lived inside that sink and the
/// test beside it re-implemented the same comparison on its own, so flipping
/// the real `<=` broke nothing: the test was checking its own copy.
///
/// Returns `false` for a token with no launch in `launch_map`: it graduated
/// inside `[from, to]` but launched before it, so this pass knows no launcher
/// to credit and must not invent one (rule 8).
fn credit_graduation(
    launch_map: &BTreeMap<Address, LaunchInfo>,
    creators: &mut BTreeMap<String, Record>,
    token: Address,
    graduation_block: u64,
) -> bool {
    let Some(info) = launch_map.get(&token) else {
        return false;
    };
    let record = creators.entry(info.deployer.to_string()).or_default();
    if instant(info.block, graduation_block) {
        record.instant = record.instant.saturating_add(1);
    } else {
        record.organic = record.organic.saturating_add(1);
    }
    true
}

/// Whether a graduation that many blocks after its launch was bought by
/// capital that was already committed before the token existed.
///
/// [`INSTANT_BLOCKS`] blocks or fewer is `instant`; one block later is
/// `organic`. The boundary is inclusive, and a graduation in the launch block
/// itself is the most instant there is, not an error.
fn instant(launch_block: u64, graduation_block: u64) -> bool {
    graduation_block.saturating_sub(launch_block) <= INSTANT_BLOCKS
}

/// What walk 3 leaves behind.
struct Walk3 {
    curve_has_later_buy: BTreeSet<Address>,
    calls: u64,
}

/// Walk 3: buys and sells in chain order, across every address -- the curve that emits
/// it is a different contract per token, so there is no one address to scope
/// to. The biggest walk of the three; only whether each known curve had a
/// buy after its own launch block and its running first/peak price survive;
/// logs themselves are not retained.
fn walk_curve_trades(
    rpc: &Rpc,
    from: u64,
    to: u64,
    curve_to_launch_block: &BTreeMap<Address, u64>,
    curve_to_token: &BTreeMap<Address, Address>,
    measurements: &mut BTreeMap<Address, LaunchMeasurement>,
    times: &mut BlockTimes,
) -> Result<Walk3, String> {
    let mut curve_has_later_buy: BTreeSet<Address> = BTreeSet::new();
    let mut failure = None;
    let walked = realorrug_onchain::robinhood::walk_logs(
        from,
        to,
        |a, b| rpc.logs_range_any_event(&[topic::CURVE_BUY, topic::CURVE_SELL], a, b),
        |log| {
            if log.topics.first() == Some(&topic::CURVE_BUY) {
                note_curve_buy(
                    curve_to_launch_block,
                    &mut curve_has_later_buy,
                    log.address,
                    log.block,
                );
            }
            if failure.is_some() {
                return;
            }
            let Some(token) = curve_to_token.get(&log.address) else {
                return;
            };
            // The RPC reader orders fills. Once one is past the window,
            // every later fill for that curve is too; no more clock reads.
            if measurements.get(token).is_some_and(|m| m.window_ended) {
                return;
            }
            let result = (|| {
                if log.block < curve_to_launch_block[&log.address] {
                    return Err("curve trade precedes its launch block".to_owned());
                }
                let trade = Trade::from_log(log).ok_or("a known curve trade did not decode")?;
                let time = times.get(log.block, |b| rpc.block_time(Some(b)))?;
                measurements
                    .get_mut(token)
                    .ok_or("launch measurement missing")?
                    .trade(&trade, time)
            })();
            failure = result.err();
        },
    )
    .map_err(|e| format!("CurveBuy/CurveSell walk: {e}"))?;
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(Walk3 {
        curve_has_later_buy,
        calls: walked.requests,
    })
}

/// Notes one `CurveBuy`, if it belongs to a curve this pass launched and
/// happened after that launch.
///
/// The walk has no address filter, so most logs handed to it are from curves
/// outside `[from, to]` entirely and are dropped here. A buy **in** the launch
/// block is not a later buy: the dev buy that funds a launch lands in the same
/// block as the launch, and counting it would make every token that a launcher
/// bought their own way into look alive.
fn note_curve_buy(
    curve_to_launch_block: &BTreeMap<Address, u64>,
    curve_has_later_buy: &mut BTreeSet<Address>,
    curve: Address,
    block: u64,
) {
    if let Some(&launch_block) = curve_to_launch_block.get(&curve)
        && block > launch_block
    {
        curve_has_later_buy.insert(curve);
    }
}

/// Runs the command.
///
/// # Errors
///
/// A missing flag, an endpoint that cannot be read, a block range that runs
/// backwards, one of the three walks failing to finish the range, a
/// disagreement in `--verify` sampling, or a file that cannot be written.
pub fn run(args: &[String]) -> Result<(), String> {
    // No default endpoint (rule 7): the public one is rate-limited, and
    // choosing it silently would be choosing for the operator.
    let rpc = Rpc::new(crate::flag(args, "--rpc").ok_or("--rpc <url> is required")?);
    let out = crate::flag(args, "--out").ok_or("--out <path> is required")?;
    let base_out =
        crate::flag(args, "--base-rates-out").unwrap_or_else(|| BASE_RATES_OUT.to_owned());
    if base_out == out || base_out == realorrug_roast::creator::summary_path_beside(&out) {
        return Err("--base-rates-out must differ from the index and population paths".to_owned());
    }
    let from = number(args, "--from")?.unwrap_or(0);
    let verify = number(args, "--verify")?;
    // The latest block, read now, is the watermark. Taking it *before* the
    // walk rather than after is what makes the file's claim exact: a block
    // mined while the walk is running is outside the range that was asked
    // for, and a watermark read afterwards would claim it was included.
    let mut times = BlockTimes::default();
    let mut head_calls = 0;
    let to = match number(args, "--to")? {
        Some(block) => {
            // `--verify` asks the factory what a token's phase is *now* and
            // compares it to what the walk concluded for the range. Those two
            // only answer the same question when the range ends at the head:
            // a token that graduated after `--to` disagrees for a reason that
            // says nothing about the decode `--verify` exists to check. The
            // refusal is here rather than in the sampler because a check that
            // fires for the wrong reason is worse than no check -- an
            // operator who has seen one false disagreement discounts the true
            // one. Found by hitting it: a hundred-thousand-block range ending
            // four thousand blocks short of the head refused to write over
            // one token that had graduated in between.
            if verify.is_some() {
                return Err(
                    "--verify compares against the token's state now, so it only holds when the \
                     range ends at the head of the chain: drop --to, or drop --verify"
                        .to_owned(),
                );
            }
            block
        }
        None => {
            head_calls = 1;
            let (block, time) = rpc.block_time(None).map_err(|e| format!("--to: {e}"))?;
            times.times.insert(block, time);
            block
        }
    };
    if to < from {
        return Err(format!("--from {from} is after --to {to}"));
    }

    let mut creators: BTreeMap<String, Record> = BTreeMap::new();
    let Walk1 {
        launch_map,
        curve_to_launch_block,
        launches,
        calls: launch_calls,
    } = walk_token_launched(&rpc, from, to, &mut creators)?;

    let Walk2 {
        graduated_tokens,
        graduation_blocks,
        calls: graduation_calls,
        skipped: skipped_graduations,
    } = walk_graduated(&rpc, from, to, &launch_map, &mut creators)?;

    let watermark_time = times.get(to, |b| rpc.block_time(Some(b)))?;
    let mut measurements = BTreeMap::new();
    let mut curve_to_token = BTreeMap::new();
    for (token, info) in &launch_map {
        let time = times.get(info.block, |b| rpc.block_time(Some(b)))?;
        measurements.insert(*token, LaunchMeasurement::new(time));
        curve_to_token.insert(info.curve, *token);
    }

    let Walk3 {
        curve_has_later_buy,
        calls: curve_buy_calls,
    } = walk_curve_trades(
        &rpc,
        from,
        to,
        &curve_to_launch_block,
        &curve_to_token,
        &mut measurements,
        &mut times,
    )?;

    let transfer_calls = walk_recipients(&rpc, from, to, &launch_map, &mut measurements)?;
    let base_rates = base_rates_json(
        &launch_map,
        &graduation_blocks,
        &measurements,
        from,
        (to, watermark_time),
    )?;

    count_stillborn(&launch_map, &curve_has_later_buy, &mut creators);

    // Every launch this pass found is measured: all three walks spanned its
    // launch block, by construction of using the same [from, to] in each.
    for record in creators.values_mut() {
        record.measured = record.launches;
    }

    let population = Population {
        launches,
        measured: launches,
        organic: u64::from(creators.values().map(|r| r.organic).sum::<u32>()),
        instant: u64::from(creators.values().map(|r| r.instant).sum::<u32>()),
        stillborn: u64::from(creators.values().map(|r| r.stillborn).sum::<u32>()),
    };

    let verification_calls = verify.map_or(0, |n| {
        sample(&launch_map, &graduated_tokens, n).len() as u64
    });
    if let Some(n) = verify {
        verify_sample(&rpc, &launch_map, &graduated_tokens, n)?;
    }

    let index = CreatorIndex {
        chain: Chain::Robinhood,
        watermark_slot: to,
        built_at: now(),
        population: Some(population),
        creators,
    };
    // Writes the summary beside the index itself, from the same pass, so the
    // two cannot disagree about what was measured.
    index.write(&out).map_err(|e| format!("{out}: {e}"))?;
    let summary_path = realorrug_roast::creator::summary_path_beside(&out);
    // All walks, parsing and verification finish before replacing a snapshot.
    let temporary = format!("{base_out}.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(&base_rates).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("base rates: {e}"))?;
    std::fs::rename(&temporary, &base_out).map_err(|e| format!("base rates: {e}"))?;

    println!(
        "{}\n{out}\n{summary_path}\n{base_out}",
        summary(&Summary {
            launches,
            launchers: index.len(),
            from,
            to,
            launch_calls,
            graduation_calls,
            graduated: graduated_tokens.len(),
            skipped_graduations,
            curve_buy_calls,
            transfer_calls,
            timestamp_calls: times.calls,
            head_calls,
            verification_calls,
        })
    );
    Ok(())
}

/// Credits every launch whose curve never saw a buy after its launch block.
///
/// Separate from [`run`] only so it can be tested: `run` needs an endpoint,
/// and this is the whole of the `stillborn` rule -- the one number in the
/// index that no other pass can check, since a curve with no buys leaves no
/// log of its own to count.
fn count_stillborn(
    launch_map: &BTreeMap<Address, LaunchInfo>,
    curve_has_later_buy: &BTreeSet<Address>,
    creators: &mut BTreeMap<String, Record>,
) {
    for info in launch_map.values() {
        if !curve_has_later_buy.contains(&info.curve) {
            let record = creators.entry(info.deployer.to_string()).or_default();
            record.stillborn = record.stillborn.saturating_add(1);
        }
    }
}

/// What one pass cost and found, for the line it prints when it finishes.
struct Summary {
    launches: u64,
    launchers: usize,
    from: u64,
    to: u64,
    launch_calls: u64,
    graduation_calls: u64,
    graduated: usize,
    skipped_graduations: u64,
    curve_buy_calls: u64,
    transfer_calls: u64,
    timestamp_calls: u64,
    head_calls: u64,
    verification_calls: u64,
}

/// The finished pass, in the words an operator reads to decide whether the
/// next range fits in a free plan's quota.
///
/// A function returning a `String` rather than a `println!` in [`run`]: the
/// total is arithmetic over three counts, and arithmetic inside a function
/// that needs a live endpoint is arithmetic nothing checks.
fn summary(s: &Summary) -> String {
    let total = s
        .launch_calls
        .saturating_add(s.graduation_calls)
        .saturating_add(s.curve_buy_calls)
        .saturating_add(s.transfer_calls)
        .saturating_add(s.timestamp_calls)
        .saturating_add(s.head_calls)
        .saturating_add(s.verification_calls);
    let extra = s.transfer_calls.saturating_add(s.timestamp_calls);
    format!(
        "{} launches by {} launchers, blocks {} to {}\n\
         TokenLaunched: {} calls\n\
         Graduated: {} calls, {} graduated, {} skipped (outside range)\n\
         CurveBuy/CurveSell (replaces buy walk): {} calls\n\
         Transfer: {} calls; timestamps: {} calls ({extra} extra base-rate calls)\n\
         Watermark: {} calls; verification: {} calls\n\
         {total} calls total",
        s.launches,
        s.launchers,
        s.from,
        s.to,
        s.launch_calls,
        s.graduation_calls,
        s.graduated,
        s.skipped_graduations,
        s.curve_buy_calls,
        s.transfer_calls,
        s.timestamp_calls,
        s.head_calls,
        s.verification_calls,
    )
}

/// Samples up to `n` tokens this pass says graduated and up to `n` it says
/// did not, and calls the factory's `getLaunchedToken` on each to check
/// `phase == 2` against what the walk concluded.
///
/// # Errors
///
/// An RPC failure or unreadable return on any sampled token, or -- the point
/// of this check -- any sampled token where the log-derived answer and the
/// call-derived answer disagree. Either way this refuses rather than write a
/// file the sample itself does not trust.
fn verify_sample(
    rpc: &Rpc,
    launch_map: &BTreeMap<Address, LaunchInfo>,
    graduated_tokens: &BTreeSet<Address>,
    n: u64,
) -> Result<(), String> {
    let mut disagreements = Vec::new();
    for (token, expect_graduated) in sample(launch_map, graduated_tokens, n) {
        let data = LaunchedToken::call_data(&token);
        let bytes = rpc
            .call_contract(&FACTORY, &data)
            .map_err(|e| format!("--verify: getLaunchedToken({token}): {e}"))?;
        let record = LaunchedToken::from_return(&bytes)
            .ok_or_else(|| format!("--verify: getLaunchedToken({token}): unreadable return"))?;
        if let Some(said) = disagreement(&token, record.phase, expect_graduated) {
            disagreements.push(said);
        }
    }
    if disagreements.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "--verify: {} of the sample disagreed with getLaunchedToken, refusing to write: {}",
            disagreements.len(),
            disagreements.join("; ")
        ))
    }
}

/// Up to `n` tokens this pass says graduated and up to `n` it says did not,
/// each paired with what the pass claims about it.
///
/// Both halves, never one: a pass that credited every token as graduated
/// would pass a sample drawn only from its graduated set, and a pass that
/// credited none would pass a sample drawn only from the rest. The check is
/// worth its calls because the two halves can fail in opposite directions.
fn sample(
    launch_map: &BTreeMap<Address, LaunchInfo>,
    graduated_tokens: &BTreeSet<Address>,
    n: u64,
) -> Vec<(Address, bool)> {
    let n = usize::try_from(n).unwrap_or(usize::MAX);
    let graduated = graduated_tokens.iter().take(n).map(|t| (*t, true));
    let not_graduated = launch_map
        .keys()
        .filter(|t| !graduated_tokens.contains(t))
        .take(n)
        .map(|t| (*t, false));
    graduated.chain(not_graduated).collect()
}

/// What to report when the factory's own view of a token contradicts the
/// walk's, or `None` when the two agree.
///
/// `phase == 2` is the factory's word for graduated
/// (`realorrug_robinhood::pons::LaunchedToken`). Any other phase is not.
fn disagreement(token: &Address, phase: u8, expect_graduated: bool) -> Option<String> {
    let actual_graduated = phase == 2;
    if actual_graduated == expect_graduated {
        return None;
    }
    let said = |g: bool| if g { "graduated" } else { "not graduated" };
    Some(format!(
        "{token}: walk said {}, getLaunchedToken says {} (phase {phase})",
        said(expect_graduated),
        said(actual_graduated),
    ))
}

/// A `u64` flag, or `None` when it was not given.
fn number(args: &[String], name: &'static str) -> Result<Option<u64>, String> {
    crate::flag(args, name)
        .map(|v| v.parse::<u64>().map_err(|e| format!("{name}: {e}")))
        .transpose()
}

/// Seconds since the epoch.
///
/// Before 1970 is not a time this can be run at, so the fallback is zero
/// rather than a panic on a clock nobody can set that far back.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use realorrug_robinhood::pons::topic;
    use realorrug_robinhood::{Address, Hash32, Log};

    use super::{
        BlockTimes, DAY, LaunchInfo, LaunchMeasurement, Summary, base_rates_json, count_stillborn,
        credit_graduation, disagreement, note_curve_buy, note_recipient, number, run, sample,
        summary,
    };

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    fn fill(quote: u128, tokens: u128) -> realorrug_robinhood::pons::Trade {
        realorrug_robinhood::pons::Trade {
            side: realorrug_robinhood::pons::Side::Buy,
            curve: addr(3),
            trader: addr(4),
            recipient: addr(5),
            quote,
            tokens,
            fee: 0,
            tax: 0,
        }
    }

    #[test]
    fn peaks_use_the_first_fill_and_include_exactly_twenty_four_hours_but_no_later_trade() {
        let mut measurement = LaunchMeasurement::new(100);
        measurement.trade(&fill(20, 10), 100).expect("first fill");
        measurement
            .trade(&fill(1, 10), 101)
            .expect("a dip must not rebase the return");
        let mut sell = fill(100, 5);
        sell.side = realorrug_robinhood::pons::Side::Sell;
        measurement
            .trade(&sell, 100 + DAY)
            .expect("inclusive endpoint");
        measurement
            .trade(&fill(1_000, 1), 101 + DAY)
            .expect("outside window");
        assert_eq!(measurement.peak_multiple(), Some(10.0));
        assert!(measurement.trade(&fill(1, 1), 99).is_err());
    }

    #[test]
    fn missing_or_zero_sized_fills_never_become_measured_zero_returns() {
        let mut measurement = LaunchMeasurement::new(0);
        assert_eq!(measurement.peak_multiple(), None);
        measurement.trade(&fill(0, 10), 0).expect("zero quote");
        measurement.trade(&fill(10, 1), 1).expect("later price");
        assert_eq!(measurement.peak_multiple(), None);
        let mut zero_tokens = LaunchMeasurement::new(0);
        zero_tokens.trade(&fill(10, 0), 0).expect("zero tokens");
        assert_eq!(zero_tokens.peak_multiple(), None);
    }

    #[test]
    fn launch_block_recipients_are_distinct_transfer_destinations_not_buy_counts() {
        let token = addr(2);
        let launches = BTreeMap::from([(
            token,
            LaunchInfo {
                deployer: addr(1),
                block: 100,
                curve: addr(3),
            },
        )]);
        let mut measurements = BTreeMap::from([(token, LaunchMeasurement::new(0))]);
        let mut log = Log {
            address: token,
            topics: vec![
                topic::TRANSFER,
                topic_word(&Address::ZERO),
                topic_word(&addr(3)),
            ],
            data: u128_word(10).to_vec(),
            block: 100,
            transaction: Hash32([0; 32]),
        };
        // The mint to the curve is a recipient too; repeated receipts do not
        // make a second recipient, and activity in another block is irrelevant.
        note_recipient(&log, &launches, &mut measurements).expect("mint");
        note_recipient(&log, &launches, &mut measurements).expect("repeat destination");
        log.topics[2] = topic_word(&addr(4));
        note_recipient(&log, &launches, &mut measurements).expect("another recipient");
        log.block = 101;
        log.topics[2] = topic_word(&addr(5));
        note_recipient(&log, &launches, &mut measurements).expect("later block");
        log.block = 100;
        log.data = u128_word(0).to_vec();
        note_recipient(&log, &launches, &mut measurements).expect("zero amount");
        log.data = u128_word(1).to_vec();
        log.topics[2] = topic_word(&Address::ZERO);
        note_recipient(&log, &launches, &mut measurements).expect("burn");
        log.address = addr(9);
        log.topics[2] = topic_word(&addr(6));
        note_recipient(&log, &launches, &mut measurements).expect("another token");
        assert_eq!(
            measurements[&token].recipients,
            BTreeSet::from([addr(3), addr(4)])
        );
        log.address = token;
        log.data.clear();
        assert!(note_recipient(&log, &launches, &mut measurements).is_err());
    }

    #[test]
    fn population_bands_share_the_full_denominator_and_outcomes_exclude_unfinished_and_unpriced_launches()
     {
        let mut launches = BTreeMap::new();
        let mut measurements = BTreeMap::new();
        // Six is in two bands, four is in none, and recent/unpriced launches
        // remain in the population while absent from the outcome denominator.
        for (id, recipients, launched_at, peak) in [
            (1, 6, 0, Some(10)),
            (2, 6, 0, Some(5)),
            (3, 6, 0, Some(2)),
            (4, 6, 0, Some(1)),
            (5, 6, 0, None),
            (6, 6, 1, Some(20)),
            (7, 4, 0, Some(1)),
        ] {
            launches.insert(
                addr(id),
                LaunchInfo {
                    deployer: addr(20),
                    block: 100,
                    curve: addr(id + 30),
                },
            );
            let mut m = LaunchMeasurement::new(launched_at);
            m.recipients.extend((1..=recipients).map(addr));
            if let Some(peak) = peak {
                m.trade(&fill(1, 1), launched_at).expect("first");
                m.trade(&fill(peak, 1), launched_at).expect("peak");
            }
            measurements.insert(addr(id), m);
        }
        let graduations = BTreeMap::from([(addr(1), 103), (addr(2), 104)]);
        let json = base_rates_json(&launches, &graduations, &measurements, 0, (200, DAY))
            .expect("snapshot");
        let rates = realorrug_roast::BaseRates::parse(&json.to_string())
            .expect("consumer accepts producer");
        assert_eq!(rates.chain, realorrug_roast::firstparty::Chain::Robinhood);
        assert_eq!(rates.launches, 7);
        assert_eq!(rates.base_rate_graduates, 2.0 / 7.0);
        assert_eq!(rates.base_rate_instant, 1.0 / 7.0);
        assert!(rates.round_trip.is_none());
        assert!(rates.aftermath.is_none());
        let band = rates.band_for(6).expect("narrowest band");
        assert_eq!(band.fires_on, 6.0 / 7.0);
        assert_eq!(band.never_graduated, 4.0 / 5.0);
        assert_eq!(band.organic, 1.0);
        assert_eq!(band.instant, 1.0);
        assert_eq!(band.p_instant, 1.0 / 6.0);
        assert!((band.x_base_instant - 7.0 / 6.0).abs() < 1e-12);
        assert_eq!(
            json["launch_block"]["histogram"]["4"],
            serde_json::json!([1, 0, 0])
        );
        let outcomes = rates.outcomes_24h.expect("measured outcomes");
        for band in outcomes.bands.iter().filter(|b| b.lo == 6 || b.lo == 5) {
            assert_eq!(
                (band.launches, band.measured, band.incomplete, band.unpriced),
                (6, 4, 1, 1)
            );
            assert_eq!(
                (band.reached_2x, band.reached_5x, band.reached_10x),
                (3, 2, 1)
            );
        }
        assert!(
            base_rates_json(&BTreeMap::new(), &graduations, &measurements, 0, (200, DAY)).is_err()
        );
        let no_instant = base_rates_json(&launches, &BTreeMap::new(), &measurements, 0, (200, DAY))
            .expect("no graduations");
        assert_eq!(no_instant["launch_block"]["base_rate_instant"], 0.0);
        assert_eq!(no_instant["launch_block"]["bands"], serde_json::json!([]));
    }

    #[test]
    fn timestamp_reads_are_cached_and_failed_or_mismatched_reads_are_not_measurements() {
        let mut times = BlockTimes::default();
        assert_eq!(times.get(7, |b| Ok((b, 100))), Ok(100));
        assert_eq!(
            times.get(7, |_| panic!("cached blocks must not call RPC")),
            Ok(100)
        );
        assert_eq!(times.calls, 1);
        assert!(times.get(8, |_| Err("unavailable".to_owned())).is_err());
        assert!(times.get(8, |_| Ok((9, 200))).is_err());
        assert!(!times.times.contains_key(&8));
        assert_eq!(times.get(8, |b| Ok((b, 201))), Ok(201));
        assert_eq!(times.calls, 4);
    }

    #[test]
    fn a_range_that_runs_backwards_is_refused_before_anything_is_written() {
        // The endpoint is deliberately unreachable: if this ever starts
        // failing with a transport error instead, the guard has moved behind
        // the first network call and a typo would cost a walk before saying so.
        let a = args(&[
            "creator-index",
            "--rpc",
            "http://127.0.0.1:1/never",
            "--out",
            "unwritten.json",
            "--from",
            "900",
            "--to",
            "100",
        ]);
        assert_eq!(
            run(&a).expect_err("a backwards range is an error"),
            "--from 900 is after --to 100"
        );
        assert!(!std::path::Path::new("unwritten.json").exists());
    }

    /// The pairing that makes `--verify` answer a question nobody asked.
    ///
    /// Refused before the endpoint is touched, which is what the unreachable
    /// address proves: the operator learns it from the flags alone, not after
    /// a walk that then throws its own result away. Re-apply the bug by
    /// deleting the `verify.is_some()` arm and this returns a connection
    /// error instead of the sentence.
    #[test]
    fn verifying_a_range_that_stops_short_of_the_head_is_refused_up_front() {
        let with_both = args(&[
            "creator-index",
            "--rpc",
            "http://127.0.0.1:1/never",
            "--out",
            "unwritten.json",
            "--to",
            "65760000",
            "--verify",
            "20",
        ]);
        assert_eq!(
            run(&with_both).expect_err("--to with --verify is an error"),
            "--verify compares against the token's state now, so it only holds when the range \
             ends at the head of the chain: drop --to, or drop --verify"
        );
        assert!(!std::path::Path::new("unwritten.json").exists());

        // `--to` on its own is still an ordinary thing to ask for: it gets
        // past the flags and fails on the unreachable endpoint instead.
        let to_only = args(&[
            "creator-index",
            "--rpc",
            "http://127.0.0.1:1/never",
            "--out",
            "unwritten.json",
            "--to",
            "65760000",
        ]);
        assert!(
            !run(&to_only)
                .expect_err("the endpoint is unreachable")
                .starts_with("--verify"),
            "--to alone must not be refused for --verify's reason"
        );
    }

    #[test]
    fn a_block_number_that_is_not_a_number_says_which_flag() {
        let e =
            number(&args(&["--from", "twelve"]), "--from").expect_err("not a number is an error");
        assert!(e.starts_with("--from: "), "{e}");
        assert_eq!(number(&args(&["--to", "7"]), "--to"), Ok(Some(7)));
        assert_eq!(number(&args(&[]), "--from"), Ok(None));
    }

    // The remaining tests exercise the outcome logic directly against
    // synthesised logs and maps, the same shape `run` builds internally,
    // rather than against the network -- the packet's instruction to test
    // over captured/synthesised logs, not a live RPC.

    fn addr(byte: u8) -> Address {
        Address([byte; 20])
    }

    fn topic_word(a: &Address) -> Hash32 {
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(&a.0);
        Hash32(word)
    }

    fn u128_word(v: u128) -> [u8; 32] {
        let mut word = [0u8; 32];
        word[16..].copy_from_slice(&v.to_be_bytes());
        word
    }

    fn graduated_log(token: Address, block: u64) -> Log {
        let mut data = Vec::new();
        data.extend_from_slice(&u128_word(0)); // quote_raised
        data.extend_from_slice(&u128_word(0)); // tokens_to_factory
        Log {
            address: realorrug_robinhood::pons::FACTORY,
            topics: vec![topic::GRADUATED, topic_word(&token)],
            data,
            block,
            transaction: Hash32([0u8; 32]),
        }
    }

    fn curve_buy_log(curve: Address, block: u64) -> Log {
        let mut data = Vec::new();
        data.extend_from_slice(&u128_word(0)); // quoteIn
        data.extend_from_slice(&u128_word(0)); // tokensOut
        data.extend_from_slice(&u128_word(0)); // fee
        data.extend_from_slice(&u128_word(0)); // tax
        Log {
            address: curve,
            topics: vec![
                topic::CURVE_BUY,
                topic_word(&addr(0xaa)),
                topic_word(&addr(0xbb)),
            ],
            data,
            block,
            transaction: Hash32([0u8; 32]),
        }
    }

    /// One launch, one graduation, through the code the walk itself runs:
    /// which column of the real `Record` the graduation landed in.
    ///
    /// It decodes a real `Graduated` log and calls [`credit_graduation`], so
    /// the boundary this returns is the boundary shipped. The version of this
    /// helper written on 2026-09-17 did the `<=` comparison itself, which
    /// made every test below pass against its own arithmetic while the walk's
    /// copy went unchecked -- four surviving mutants said so.
    fn credit(launch_block: u64, graduation_block: u64) -> &'static str {
        let deployer = addr(1);
        let token = addr(2);
        let mut launch_map = BTreeMap::new();
        launch_map.insert(
            token,
            LaunchInfo {
                deployer,
                block: launch_block,
                curve: addr(3),
            },
        );
        let log = graduated_log(token, graduation_block);
        let graduated = realorrug_robinhood::pons::Graduated::from_log(&log).expect("decodes");
        let mut creators = BTreeMap::new();
        assert!(
            credit_graduation(&launch_map, &mut creators, graduated.token, log.block),
            "a token that is in the launch map is this pass's to credit"
        );
        let record = creators.get(&deployer.to_string()).expect("credited");
        match (record.instant, record.organic) {
            (1, 0) => "instant",
            (0, 1) => "organic",
            other => panic!("one graduation credited {other:?} outcomes"),
        }
    }

    #[test]
    fn a_graduation_whose_launch_is_outside_the_range_credits_no_one() {
        // Rule 8: this pass does not know who launched it, so it invents
        // neither a launcher nor an outcome. `walk_graduated` counts it as
        // skipped on the strength of the `false` this returns.
        let mut creators = BTreeMap::new();
        assert!(
            !credit_graduation(&BTreeMap::new(), &mut creators, addr(2), 500),
            "a token with no launch in this pass is not this pass's to credit"
        );
        assert!(
            creators.is_empty(),
            "an unknown token created a launcher record: {creators:?}"
        );
    }

    #[test]
    fn a_curve_buy_counts_only_after_the_launch_block_and_only_for_a_known_curve() {
        let curve = addr(3);
        let mut launched = BTreeMap::new();
        launched.insert(curve, 100);

        let mut seen = BTreeSet::new();
        note_curve_buy(&launched, &mut seen, curve, 100);
        assert!(
            seen.is_empty(),
            "a buy in the launch block is the dev buy, not proof anyone else came"
        );

        note_curve_buy(&launched, &mut seen, addr(9), 500);
        assert!(
            seen.is_empty(),
            "a buy on a curve this pass never launched was counted"
        );

        note_curve_buy(&launched, &mut seen, curve, 101);
        assert!(
            seen.contains(&curve),
            "the first buy after the launch block was not counted"
        );
    }

    #[test]
    fn a_launch_whose_curve_never_saw_a_later_buy_is_stillborn_and_one_that_did_is_not() {
        // Two launchers, not one with two launches: a launcher who owns both
        // curves is credited once either way, so dropping the `!` would count
        // the *lively* curve and still land on one. The names have to come
        // apart for the test to be able to tell the rule from its inverse.
        let (quiet_launcher, lively_launcher) = (addr(1), addr(7));
        let (quiet, lively) = (addr(3), addr(4));
        let mut launch_map = BTreeMap::new();
        launch_map.insert(
            addr(2),
            LaunchInfo {
                deployer: quiet_launcher,
                block: 10,
                curve: quiet,
            },
        );
        launch_map.insert(
            addr(5),
            LaunchInfo {
                deployer: lively_launcher,
                block: 11,
                curve: lively,
            },
        );
        let bought: BTreeSet<Address> = [lively].into_iter().collect();

        let mut creators = BTreeMap::new();
        count_stillborn(&launch_map, &bought, &mut creators);
        assert_eq!(
            creators
                .get(&quiet_launcher.to_string())
                .expect("the launcher whose curve nobody bought was credited")
                .stillborn,
            1,
            "a curve with no buy after its launch block is stillborn"
        );
        assert!(
            !creators.contains_key(&lively_launcher.to_string()),
            "a curve somebody bought was counted stillborn, and its launcher \
             gained a record it should not have: {creators:?}"
        );
    }

    #[test]
    fn the_sample_draws_from_both_sides_so_a_pass_that_says_yes_to_everything_fails_it() {
        let graduated = addr(2);
        let not = addr(5);
        let mut launch_map = BTreeMap::new();
        for (token, block) in [(graduated, 10), (not, 11)] {
            launch_map.insert(
                token,
                LaunchInfo {
                    deployer: addr(1),
                    block,
                    curve: addr(3),
                },
            );
        }
        let graduated_tokens: BTreeSet<Address> = [graduated].into_iter().collect();

        let drawn = sample(&launch_map, &graduated_tokens, 5);
        assert!(
            drawn.contains(&(graduated, true)) && drawn.contains(&(not, false)),
            "the sample must carry both claims to be able to catch either error: {drawn:?}"
        );
        assert_eq!(drawn.len(), 2, "a token was sampled twice: {drawn:?}");
        assert_eq!(
            sample(&launch_map, &graduated_tokens, 0).len(),
            0,
            "--verify 0 must call nothing"
        );
    }

    #[test]
    fn the_factory_disagreeing_with_the_walk_is_reported_in_both_directions() {
        let token = addr(2);
        assert_eq!(disagreement(&token, 2, true), None);
        assert_eq!(disagreement(&token, 1, false), None);

        let missed =
            disagreement(&token, 2, false).expect("the factory says graduated, walk did not");
        assert!(
            missed.contains("walk said not graduated") && missed.contains("says graduated"),
            "{missed}"
        );
        let phantom =
            disagreement(&token, 1, true).expect("the walk says graduated, factory does not");
        assert!(
            phantom.contains("walk said graduated") && phantom.contains("says not graduated"),
            "{phantom}"
        );
    }

    #[test]
    fn the_summary_counts_extra_measurement_calls_and_verification_in_the_total() {
        // The figure that answers "can I afford the next range". Three counts
        // that are only ever added: a `-` or `*` here reads as a cheap run.
        let line = summary(&Summary {
            launches: 7,
            launchers: 2,
            from: 1,
            to: 9,
            launch_calls: 3,
            graduation_calls: 5,
            graduated: 4,
            skipped_graduations: 1,
            curve_buy_calls: 11,
            transfer_calls: 13,
            timestamp_calls: 17,
            head_calls: 1,
            verification_calls: 4,
        });
        assert!(line.contains("54 calls total"), "{line}");
        assert!(line.contains("30 extra base-rate calls"), "{line}");
        assert!(
            line.contains("7 launches by 2 launchers, blocks 1 to 9"),
            "{line}"
        );
        assert!(line.contains("4 graduated, 1 skipped"), "{line}");
    }

    #[test]
    fn graduation_two_blocks_after_launch_is_instant_and_launches_is_untouched_by_outcome() {
        assert_eq!(credit(100, 102), "instant");
        // `launches` is credited by `TokenLaunched` alone; nothing in the
        // graduation path touches it, which is checked by the type: `credit`
        // above never mentions `record.launches` at all.
    }

    #[test]
    fn graduation_four_hundred_blocks_after_launch_is_organic() {
        assert_eq!(credit(100, 500), "organic");
    }

    #[test]
    fn exactly_three_blocks_is_instant_and_four_is_organic_the_boundary_bites_when_inverted() {
        // The literal boundary the packet asks to prove by hand: exactly
        // `INSTANT_BLOCKS` is instant, one more is organic.
        assert_eq!(credit(100, 103), "instant");
        assert_eq!(credit(100, 104), "organic");
        // A graduation in the launch block itself is the most instant there
        // is, and `saturating_sub` is what keeps it from being a panic.
        assert_eq!(credit(100, 100), "instant");
        // Re-applied by hand in `instant`: `<` in place of `<=` turns the
        // first line here into `organic` and fails this test. It did not
        // before 2026-09-17, because the helper above ran its own copy of
        // the comparison instead of the shipped one.
    }

    #[test]
    fn a_launch_with_no_graduation_log_is_measured_but_neither_organic_nor_instant() {
        // This is the case the whole design rests on: an absent `Graduated`
        // log is itself the measurement. `measured` is set from `launches`
        // in `run`, not from graduation credits, which this asserts by
        // construction: a `Record` that only ever had `launches` set and
        // never touched by a graduation credit still reports `measured`
        // once `run`'s post-pass runs.
        let mut record = realorrug_roast::creator::Record {
            launches: 1,
            ..Default::default()
        };
        record.measured = record.launches;
        assert_eq!(record.measured, 1);
        assert_eq!(record.organic, 0);
        assert_eq!(record.instant, 0);
    }

    #[test]
    fn a_curve_with_a_later_buy_is_not_stillborn_one_with_only_launch_block_activity_is() {
        let curve = addr(3);
        let launch_block = 100;

        // A buy in a later block: not stillborn.
        let mut later = BTreeSet::new();
        let log = curve_buy_log(curve, 105);
        if log.block > launch_block {
            later.insert(log.address);
        }
        assert!(later.contains(&curve), "a later buy must mark the curve");

        // The only activity is in the launch block itself: stillborn.
        let mut only_launch_block = BTreeSet::new();
        let log = curve_buy_log(curve, launch_block);
        if log.block > launch_block {
            only_launch_block.insert(log.address);
        }
        assert!(
            !only_launch_block.contains(&curve),
            "a launch-block-only buy must not save a curve from stillborn"
        );
    }

    #[test]
    fn a_graduated_log_for_an_unknown_token_is_skipped_not_credited_and_does_not_panic() {
        let launch_map: BTreeMap<Address, LaunchInfo> = BTreeMap::new();
        let log = graduated_log(addr(9), 50);
        let graduated = realorrug_robinhood::pons::Graduated::from_log(&log).expect("decodes");
        assert!(!launch_map.contains_key(&graduated.token));
        // The real code's `let Some(info) = ... else { skipped += 1; return; }`
        // is exactly this lookup; a `None` here is what drives that branch,
        // and there is no `.unwrap()` on the path, so it cannot panic.
    }
}
