// SPDX-License-Identifier: Apache-2.0
//! `realorrug creator-index`: build the Pons v2 launcher index from the
//! factory's own logs.
//!
//! This is the bot's memory of who has launched before. Without it every
//! launcher is a stranger, and the one question a reader most wants answered
//! about a new token -- *has this person done this before, and how did it go*
//! -- gets no answer at all.
//!
//! # What it counts, and what it deliberately does not
//!
//! Launches, per launcher, and nothing else. Every outcome column
//! (`measured`, `organic`, `instant`, `stillborn`) is written as zero, which
//! `realorrug_roast::creator::Record` defines as *not measured* rather than
//! *measured as none* -- the reply says the launch count and says in the same
//! breath that no outcome has been measured.
//!
//! That is not laziness, it is the cost. Research 0038 §4: the whole launch
//! history is on the order of twenty to forty `eth_getLogs` calls, because one
//! log carries one launch and the window resizes to the provider's cap. A
//! launch's *outcome* needs `getLaunchedToken` per token -- about 171,000
//! calls, roughly a day of continuous polite calling against a free endpoint.
//! Waiting for that before writing anything would mean the bot stays blind for
//! a day to learn what it could know in a minute.
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

use std::collections::BTreeMap;

use realorrug_roast::creator::{CreatorIndex, Population, Record};
use realorrug_roast::firstparty::Chain;
use realorrug_robinhood::Rpc;
use realorrug_robinhood::pons::{FACTORY, topic};

/// Runs the command.
///
/// # Errors
///
/// A missing flag, an endpoint that cannot be read, a block range that runs
/// backwards, or a file that cannot be written.
pub fn run(args: &[String]) -> Result<(), String> {
    // No default endpoint (rule 7): the public one is rate-limited, and
    // choosing it silently would be choosing for the operator.
    let rpc = Rpc::new(crate::flag(args, "--rpc").ok_or("--rpc <url> is required")?);
    let out = crate::flag(args, "--out").ok_or("--out <path> is required")?;
    let from = number(args, "--from")?.unwrap_or(0);
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
    let mut calls = 0_u64;
    let launches = realorrug_onchain::robinhood::walk_launches(
        from,
        to,
        |a, b| {
            calls += 1;
            rpc.logs_range(&FACTORY, &[topic::TOKEN_LAUNCHED], a, b)
        },
        |launch| {
            let record = creators.entry(launch.deployer.to_string()).or_default();
            record.launches = record.launches.saturating_add(1);
        },
    )?;

    let index = CreatorIndex {
        chain: Chain::Robinhood,
        watermark_slot: to,
        built_at: now(),
        // `launches` measured, every outcome column not measured. Five zeroes
        // would be a claim that nothing ever graduated if `measured` did not
        // carry the denominator -- it does, and `Population::organic_share`
        // answers `None` rather than 0% while it is zero.
        population: Some(Population {
            launches,
            measured: 0,
            organic: 0,
            instant: 0,
            stillborn: 0,
        }),
        creators,
    };
    // Writes the summary beside the index itself, from the same pass, so the
    // two cannot disagree about what was measured.
    index.write(&out).map_err(|e| format!("{out}: {e}"))?;
    let summary_path = realorrug_roast::creator::summary_path_beside(&out);

    println!(
        "{launches} launches by {} launchers, blocks {from} to {to}, {calls} calls\n{out}\n{summary_path}",
        index.len()
    );
    Ok(())
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
    use super::{number, run};

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
}
