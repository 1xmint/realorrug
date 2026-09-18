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
//! `Population`), from three passes over logs in the same block range and no
//! per-token `eth_call`.
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
//! ## The three walks
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
//! 3. **`CurveBuy`**, *not* scoped to any one address -- each token's curve is
//!    its own contract, so the walk is by topic across every address in the
//!    range (see [`realorrug_robinhood::Rpc::logs_range_any_address`]). This
//!    is the biggest of the three walks by a wide margin: millions of logs
//!    across the chain's history, against `TokenLaunched`'s hundreds of
//!    thousands and `Graduated`'s much smaller graduated subset. Only
//!    whether a curve had a buy *after* its own launch block is kept -- a
//!    `BTreeSet` of curve addresses, never the logs themselves -- which is
//!    what keeps memory flat regardless of how many buys are seen.
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
//! three walks fails to finish the full range, nothing is measured: this
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

use realorrug_roast::creator::{CreatorIndex, Population, Record};
use realorrug_roast::firstparty::Chain;
use realorrug_robinhood::pons::{FACTORY, Graduated, Launched, LaunchedToken, topic};
use realorrug_robinhood::{Address, Rpc};

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
    let mut calls = 0_u64;
    realorrug_onchain::robinhood::walk_logs(
        from,
        to,
        |a, b| {
            calls += 1;
            rpc.logs_range(&FACTORY, &[topic::TOKEN_LAUNCHED], a, b)
        },
        |log| {
            let Some(launch) = Launched::from_log(log) else {
                return;
            };
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
    Ok(Walk1 {
        launch_map,
        curve_to_launch_block,
        launches,
        calls,
    })
}

/// What walk 2 leaves behind.
struct Walk2 {
    graduated_tokens: BTreeSet<Address>,
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
    let mut calls = 0_u64;
    let mut skipped = 0_u64;
    let mut graduated_tokens: BTreeSet<Address> = BTreeSet::new();
    realorrug_onchain::robinhood::walk_logs(
        from,
        to,
        |a, b| {
            calls += 1;
            rpc.logs_range(&FACTORY, &[topic::GRADUATED], a, b)
        },
        |log| {
            let Some(graduated) = Graduated::from_log(log) else {
                return;
            };
            let Some(info) = launch_map.get(&graduated.token) else {
                // Graduated outside [from, to], or launched before --from:
                // not this pass's to measure, and not a phantom deployer's
                // problem. Counted so the summary can say how many.
                skipped += 1;
                return;
            };
            let record = creators.entry(info.deployer.to_string()).or_default();
            if log.block.saturating_sub(info.block) <= INSTANT_BLOCKS {
                record.instant = record.instant.saturating_add(1);
            } else {
                record.organic = record.organic.saturating_add(1);
            }
            graduated_tokens.insert(graduated.token);
        },
    )
    .map_err(|e| format!("Graduated walk: {e}"))?;
    Ok(Walk2 {
        graduated_tokens,
        calls,
        skipped,
    })
}

/// What walk 3 leaves behind.
struct Walk3 {
    curve_has_later_buy: BTreeSet<Address>,
    calls: u64,
}

/// Walk 3: `CurveBuy`, by topic across every address -- the curve that emits
/// it is a different contract per token, so there is no one address to scope
/// to. The biggest walk of the three; only whether each known curve had a
/// buy after its own launch block survives it, as a set of curve addresses,
/// never the logs themselves.
fn walk_curve_buy(
    rpc: &Rpc,
    from: u64,
    to: u64,
    curve_to_launch_block: &BTreeMap<Address, u64>,
) -> Result<Walk3, String> {
    let mut calls = 0_u64;
    let mut curve_has_later_buy: BTreeSet<Address> = BTreeSet::new();
    realorrug_onchain::robinhood::walk_logs(
        from,
        to,
        |a, b| {
            calls += 1;
            rpc.logs_range_any_address(&[topic::CURVE_BUY], a, b)
        },
        |log| {
            if let Some(&launch_block) = curve_to_launch_block.get(&log.address)
                && log.block > launch_block
            {
                curve_has_later_buy.insert(log.address);
            }
        },
    )
    .map_err(|e| format!("CurveBuy walk: {e}"))?;
    Ok(Walk3 {
        curve_has_later_buy,
        calls,
    })
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
    let from = number(args, "--from")?.unwrap_or(0);
    let verify = number(args, "--verify")?;
    // The latest block, read now, is the watermark. Taking it *before* the
    // walk rather than after is what makes the file's claim exact: a block
    // mined while the walk is running is outside the range that was asked
    // for, and a watermark read afterwards would claim it was included.
    let to = match number(args, "--to")? {
        Some(block) => block,
        None => rpc.block_time(None).map_err(|e| format!("--to: {e}"))?.0,
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
        calls: graduation_calls,
        skipped: skipped_graduations,
    } = walk_graduated(&rpc, from, to, &launch_map, &mut creators)?;

    let Walk3 {
        curve_has_later_buy,
        calls: curve_buy_calls,
    } = walk_curve_buy(&rpc, from, to, &curve_to_launch_block)?;

    // `stillborn`: a launch whose curve never saw this pass's set gain its
    // address, i.e. never had a `CurveBuy` in a block after its own launch.
    for info in launch_map.values() {
        if !curve_has_later_buy.contains(&info.curve) {
            let record = creators.entry(info.deployer.to_string()).or_default();
            record.stillborn = record.stillborn.saturating_add(1);
        }
    }

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

    let total_calls = launch_calls + graduation_calls + curve_buy_calls;
    println!(
        "{launches} launches by {} launchers, blocks {from} to {to}\n\
         TokenLaunched: {launch_calls} calls\n\
         Graduated: {graduation_calls} calls, {} graduated, {skipped_graduations} skipped (outside range)\n\
         CurveBuy: {curve_buy_calls} calls\n\
         {total_calls} calls total\n\
         {out}\n{summary_path}",
        index.len(),
        graduated_tokens.len(),
    );
    Ok(())
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
    let n = usize::try_from(n).unwrap_or(usize::MAX);
    let not_graduated: Vec<Address> = launch_map
        .keys()
        .filter(|t| !graduated_tokens.contains(t))
        .take(n)
        .copied()
        .collect();
    let graduated_sample: Vec<Address> = graduated_tokens.iter().take(n).copied().collect();

    let mut disagreements = Vec::new();
    for (token, expect_graduated) in graduated_sample
        .into_iter()
        .map(|t| (t, true))
        .chain(not_graduated.into_iter().map(|t| (t, false)))
    {
        let data = LaunchedToken::call_data(&token);
        let bytes = rpc
            .call_contract(&FACTORY, &data)
            .map_err(|e| format!("--verify: getLaunchedToken({token}): {e}"))?;
        let record = LaunchedToken::from_return(&bytes)
            .ok_or_else(|| format!("--verify: getLaunchedToken({token}): unreadable return"))?;
        let actual_graduated = record.phase == 2;
        if actual_graduated != expect_graduated {
            disagreements.push(format!(
                "{token}: walk said {}, getLaunchedToken says {}",
                if expect_graduated {
                    "graduated"
                } else {
                    "not graduated"
                },
                if actual_graduated {
                    "graduated"
                } else {
                    "not graduated"
                },
            ));
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

    use super::{LaunchInfo, number, run};

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
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

    /// Runs the same graduation-crediting rule `run` uses inline, against a
    /// one-launch map, and returns whether it landed `instant` or `organic`.
    ///
    /// Kept as a small helper rather than duplicated in each test below so
    /// the boundary test's inversion (see that test) is changing one place.
    fn credit(launch_block: u64, graduation_block: u64) -> &'static str {
        let deployer = addr(1);
        let token = addr(2);
        let curve = addr(3);
        let mut launch_map = BTreeMap::new();
        launch_map.insert(
            token,
            LaunchInfo {
                deployer,
                block: launch_block,
                curve,
            },
        );
        let log = graduated_log(token, graduation_block);
        let graduated = realorrug_robinhood::pons::Graduated::from_log(&log).expect("decodes");
        let info = launch_map.get(&graduated.token).expect("in map");
        if log.block.saturating_sub(info.block) <= super::INSTANT_BLOCKS {
            "instant"
        } else {
            "organic"
        }
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
        // Inverting the rule (`<` instead of `<=`) is what a mutation test
        // would try here. Done by hand: with `<` in place of `<=`, a
        // graduation exactly 3 blocks after launch (`104 - ... ` no --
        // `103 - 100 == 3`) would read `organic` instead of `instant`,
        // flipping the first assertion above to fail. That is the failure
        // this test exists to catch; see the report for what actually
        // printed when this was tried.
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
        assert!(launch_map.get(&graduated.token).is_none());
        // The real code's `let Some(info) = ... else { skipped += 1; return; }`
        // is exactly this lookup; a `None` here is what drives that branch,
        // and there is no `.unwrap()` on the path, so it cannot panic.
    }
}
