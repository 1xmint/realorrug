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
    AccountRead, Budget, CheckOutcome, LaunchCheck, RpcClient, candidate_mint, check_launch,
};
use realorrug_types::Address;

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
