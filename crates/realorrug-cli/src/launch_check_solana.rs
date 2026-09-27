// SPDX-License-Identifier: Apache-2.0
//! `realorrug launch-check solana`: the pump.fun arm of the launch check.
//!
//! `launch-check solana --signature <sig> --treasury <addr> --dev-wallet
//! <addr> --dev-buy-lamports <n> --rpc URL [--seconds N]` (or `REALORRUG_RPC`
//! in place of `--rpc`; one of the two is required) reads the launch
//! transaction and the accounts it created, and prints either the six checks
//! that passed or every reason it refuses. Read-only: no key, nothing
//! signed. This is the instrument ADR 0037 decision 6 and deploy/LAUNCH.md
//! steps 6 and 9 name -- run once, by hand, right after Josh signs the
//! launch on pump.fun.
//!
//! The RPC endpoint is never printed (it carries a key): only the values the
//! checks read.

use std::time::Duration;

use realorrug_onchain::budget::{DEFAULT_MAX_CALLS, DEFAULT_MAX_PAGES};
use realorrug_onchain::pumpfun_launch_check::AccountState;
use realorrug_onchain::{
    AccountRead, Budget, CheckOutcome, LaunchCheck, RpcClient, StatedTransfer, candidate_mint,
    check_launch,
};
use realorrug_types::Address;

/// The value following every occurrence of `name`, in order.
///
/// Mirrors `bio.rs`'s own copy: [`crate::flag`] reads only the first, and
/// `--allow-transfer` is repeatable (a launch can state more than one
/// transfer), so this is the same walk done without stopping at the first
/// match.
fn values(args: &[String], name: &str) -> Vec<String> {
    args.iter()
        .zip(args.iter().skip(1))
        .filter(|(a, _)| a.as_str() == name)
        .map(|(_, v)| v.clone())
        .collect()
}

/// Parses one `--allow-transfer <address>:<lamports>` value.
///
/// Strict, not best-effort (rule 8: unknown is not safe): a missing colon,
/// an address that does not parse, a lamports field that is not a plain
/// number, or 0 lamports (not a transfer at all) is an error the operator
/// sees before the check runs -- never a value silently dropped, which
/// would let a launch tool's tip through unchecked exactly because the
/// operator mistyped the flag that was supposed to state it.
fn parse_stated_transfer(value: &str) -> Result<StatedTransfer, String> {
    let (addr, lamports) = value
        .split_once(':')
        .ok_or_else(|| format!("--allow-transfer {value}: expected <address>:<lamports>"))?;
    let to: Address = addr
        .parse()
        .map_err(|e| format!("--allow-transfer {value}: {e}"))?;
    let lamports: u64 = lamports
        .parse()
        .map_err(|_| format!("--allow-transfer {value}: {lamports} is not a lamport amount"))?;
    if lamports == 0 {
        return Err(format!(
            "--allow-transfer {value}: 0 lamports is not a transfer"
        ));
    }
    Ok(StatedTransfer { to, lamports })
}

/// Every `--allow-transfer` value, parsed -- the first one that does not
/// parse stops the command rather than running the check on a partial list.
fn stated_transfers(args: &[String]) -> Result<Vec<StatedTransfer>, String> {
    values(args, "--allow-transfer")
        .iter()
        .map(|v| parse_stated_transfer(v))
        .collect()
}

/// Runs the command.
///
/// # Errors
///
/// A message when a required flag is missing or unparseable (rule 7: deny by
/// default), or a string naming every check that refused (also the six
/// checks' own text, joined) when the transaction does not read clean.
pub fn run(args: &[String]) -> Result<(), String> {
    let signature = crate::flag(args, "--signature").ok_or("--signature <sig> is required")?;
    let treasury: Address = crate::flag(args, "--treasury")
        .ok_or("--treasury <address> is required")?
        .parse()
        .map_err(|e| format!("--treasury: {e}"))?;
    let dev_wallet: Address = crate::flag(args, "--dev-wallet")
        .ok_or("--dev-wallet <address> is required")?
        .parse()
        .map_err(|e| format!("--dev-wallet: {e}"))?;
    let dev_buy_lamports: u64 = crate::flag(args, "--dev-buy-lamports")
        .ok_or("--dev-buy-lamports <n> is required")?
        .parse()
        .map_err(|e| format!("--dev-buy-lamports: {e}"))?;

    run_with(
        args,
        &signature,
        &treasury,
        &dev_wallet,
        dev_buy_lamports,
        &|k| std::env::var(k).ok(),
    )
}

/// [`run`] after its flags are read, with the environment passed in so a test
/// can hold what an unset one does.
fn run_with(
    args: &[String],
    signature: &str,
    treasury: &Address,
    dev_wallet: &Address,
    dev_buy_lamports: u64,
    env: &impl Fn(&str) -> Option<String>,
) -> Result<(), String> {
    // No default endpoint (rule 7): `from_vars` would fall back to the public
    // one, which is rate-limited, and a refusal caused by that would read as a
    // finding about the launch.
    // Parsed before the first chain read, so a mistyped flag costs nothing.
    let transfers = stated_transfers(args)?;
    let endpoint = crate::rpc_arg::required_endpoint(args, env)?;
    let client = RpcClient::new(endpoint.clone());
    let hide = |e: &dyn std::fmt::Display| crate::rpc_arg::redact(&e.to_string(), &endpoint);
    let seconds = crate::flag(args, "--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let mut budget = Budget::new(
        DEFAULT_MAX_CALLS,
        DEFAULT_MAX_PAGES,
        Duration::from_secs(seconds),
    );

    let tx = client
        .transaction(&mut budget, signature)
        .map_err(|e| hide(&e))?
        .ok_or_else(|| format!("{signature} is not in a block yet"))?;

    // The mint is read from the transaction itself before any account fetch
    // -- there is nowhere else to derive the curve or mint address from.
    // `check_launch` below still runs its own full scan and refuses on more
    // than one launch instruction, so a second one here is never silently
    // dropped for being second.
    // A failed read keeps its (redacted) reason and a good one its slot, so a
    // refusal says whether the node failed or the address was wrong.
    let mint = candidate_mint(&tx);
    let mut read = |addr: Option<Address>, what: &str| match addr {
        None => Err(format!(
            "no {what} address: no launch instruction named a mint"
        )),
        Some(a) => client.account(&mut budget, &a).map_err(|e| hide(&e)),
    };
    let curve_read = read(
        mint.and_then(|m| realorrug_pumpfun::pda::bonding_curve(&m)),
        "bonding curve",
    );
    let mint_read = read(mint, "mint");

    let result = check_launch(
        &tx,
        state(&curve_read),
        state(&mint_read),
        treasury,
        dev_wallet,
        dev_buy_lamports,
        &transfers,
    );

    let text = report(signature, &result);
    if result.clean() {
        print!("{text}");
        Ok(())
    } else {
        Err(text)
    }
}

/// One account read, as the check takes it.
fn state(read: &Result<Option<AccountRead>, String>) -> AccountState<'_> {
    match read {
        Ok(Some(a)) => AccountState::Present {
            data: &a.data,
            slot: a.slot,
        },
        Ok(None) => AccountState::Absent,
        Err(why) => AccountState::Failed(why.clone()),
    }
}

/// What the operator reads: one line per check, PASS or REFUSE with the
/// value read.
fn report(signature: &str, result: &LaunchCheck) -> String {
    let mut lines = vec![format!(
        "{} launch {signature} (slot {})",
        if result.clean() {
            "CLEAN"
        } else {
            "NOT CLEAN:"
        },
        result.slot.0
    )];
    for (name, outcome) in [
        ("transaction", &result.transaction),
        ("single launch", &result.single_launch),
        ("fee recipient", &result.fee_recipient),
        ("authorities", &result.authorities),
        ("dev buy", &result.dev_buy),
        ("allowlist", &result.allowlist),
    ] {
        lines.push(match outcome {
            CheckOutcome::Pass(v) => format!("  PASS    {name}: {v}"),
            CheckOutcome::Refuse(why) => format!("  REFUSE  {name}: {why}"),
        });
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// Whether `launch-check`'s arguments ask for this arm rather than the
/// Robinhood one. A named function, not a guard inline in `main`, so a test
/// holds which arm "solana" reaches: a guard in `main` has no test to fail.
#[must_use]
pub fn selected(args: &[String]) -> bool {
    args.get(1).map(String::as_str) == Some("solana")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_solana_in_the_second_place_selects_this_arm() {
        let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(selected(&args(&[
            "launch-check",
            "solana",
            "--signature",
            "x"
        ])));
        assert!(!selected(&args(&["launch-check", "--signature", "x"])));
        assert!(!selected(&args(&["launch-check", "robinhood"])));
        assert!(!selected(&args(&["launch-check"])));
    }

    #[test]
    fn required_flags_are_enforced_in_order() {
        assert_eq!(
            run(&["launch-check".to_owned(), "solana".to_owned()]),
            Err("--signature <sig> is required".to_owned())
        );
        let base = |extra: &[&str]| {
            let mut v = vec!["launch-check".to_owned(), "solana".to_owned()];
            v.extend(extra.iter().map(|s| (*s).to_owned()));
            v
        };
        assert_eq!(
            run(&base(&["--signature", "sig"])),
            Err("--treasury <address> is required".to_owned())
        );
        assert_eq!(
            run(&base(&[
                "--signature",
                "sig",
                "--treasury",
                &Address::new([1; 32]).to_string(),
            ])),
            Err("--dev-wallet <address> is required".to_owned())
        );
        assert_eq!(
            run(&base(&[
                "--signature",
                "sig",
                "--treasury",
                &Address::new([1; 32]).to_string(),
                "--dev-wallet",
                &Address::new([2; 32]).to_string(),
            ])),
            Err("--dev-buy-lamports <n> is required".to_owned())
        );
    }

    fn flags(rpc: Option<&str>) -> (Vec<String>, Address, Address) {
        let mut v = vec!["launch-check".to_owned(), "solana".to_owned()];
        if let Some(r) = rpc {
            v.extend(["--rpc".to_owned(), r.to_owned()]);
        }
        (v, Address::new([1; 32]), Address::new([2; 32]))
    }

    #[test]
    fn no_endpoint_configured_is_refused_not_defaulted() {
        let (args, treasury, dev) = flags(None);
        assert_eq!(
            run_with(&args, "sig", &treasury, &dev, 1, &|_| None),
            Err("--rpc or REALORRUG_RPC is required".to_owned())
        );
    }

    /// ureq's bad-URI error quotes the endpoint it was given; a scheme-less
    /// one fails before any network call, so this never leaves the machine.
    #[test]
    fn the_endpoint_key_never_reaches_the_error() {
        let (args, treasury, dev) = flags(Some("rpc.invalid/?api-key=SENTINEL-4412"));
        let err = run_with(&args, "sig", &treasury, &dev, 1, &|_| None)
            .expect_err("a scheme-less endpoint cannot be read");
        assert!(!err.contains("SENTINEL-4412"), "{err}");
    }

    /// Each failure mode is its own assertion so a mutant that weakens any
    /// one check (e.g. `lamports == 0` to `lamports < 0`, which is always
    /// false for a `u64` and would let a stray `--allow-transfer x:0` pass
    /// silently) fails a specific case rather than the whole thing reading
    /// green by accident.
    #[test]
    fn allow_transfer_parsing_is_strict() {
        let addr = Address::new([7; 32]).to_string();
        let good = format!("{addr}:1000");

        // Success: the one shape that must still work.
        assert_eq!(
            parse_stated_transfer(&good),
            Ok(StatedTransfer {
                to: Address::new([7; 32]),
                lamports: 1000
            })
        );

        // Missing colon.
        assert_eq!(
            parse_stated_transfer(&addr),
            Err(format!(
                "--allow-transfer {addr}: expected <address>:<lamports>"
            ))
        );

        // Address does not parse (not valid base58 -- '0', 'O', 'I', 'l' are
        // excluded from the alphabet).
        assert_eq!(
            parse_stated_transfer("not-0-a-valid-address:1000"),
            Err(
                "--allow-transfer not-0-a-valid-address:1000: not valid base58: \
                 not-0-a-valid-address"
                    .to_owned()
            )
        );

        // Lamports is not a plain number.
        assert_eq!(
            parse_stated_transfer(&format!("{addr}:abc")),
            Err(format!(
                "--allow-transfer {addr}:abc: abc is not a lamport amount"
            ))
        );

        // 0 lamports is not a transfer (a mutant that drops this arm, or
        // that flips `== 0` to `< 0` which a u64 never satisfies, lets this
        // through).
        assert_eq!(
            parse_stated_transfer(&format!("{addr}:0")),
            Err(format!(
                "--allow-transfer {addr}:0: 0 lamports is not a transfer"
            ))
        );
    }

    /// Every `--allow-transfer` is read, in order, and no other flag's value
    /// is taken for one: a transfer the operator never stated would otherwise
    /// be allowed through.
    #[test]
    fn values_reads_every_occurrence_of_one_flag_only() {
        let args: Vec<String> = [
            "--mint",
            "m",
            "--allow-transfer",
            "a:1",
            "--signature",
            "s",
            "--allow-transfer",
            "b:2",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(values(&args, "--allow-transfer"), ["a:1", "b:2"]);
    }

    #[test]
    fn a_report_names_every_check_pass_or_refuse() {
        let result = LaunchCheck {
            slot: realorrug_types::Slot(5),
            transaction: CheckOutcome::Pass("slot 5".to_owned()),
            single_launch: CheckOutcome::Pass("mint abc".to_owned()),
            fee_recipient: CheckOutcome::Refuse("not the treasury".to_owned()),
            authorities: CheckOutcome::Pass("both revoked".to_owned()),
            dev_buy: CheckOutcome::Pass("0 lamports".to_owned()),
            allowlist: CheckOutcome::Pass("only allowed programs, no other buy".to_owned()),
        };
        let text = report("sig123", &result);
        assert!(
            text.starts_with("NOT CLEAN: launch sig123 (slot 5)\n"),
            "{text}"
        );
        assert!(
            text.contains("REFUSE  fee recipient: not the treasury"),
            "{text}"
        );
        assert!(text.contains("PASS    single launch: mint abc"), "{text}");
    }
}
