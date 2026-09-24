// SPDX-License-Identifier: Apache-2.0
//! `realorrug launch-check solana`: the pump.fun arm of the launch check.
//!
//! `launch-check solana --signature <sig> --treasury <addr> --dev-wallet
//! <addr> --dev-buy-lamports <n> [--rpc URL] [--seconds N]` reads the launch
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
use realorrug_onchain::{Budget, CheckOutcome, LaunchCheck, RpcClient, candidate_mint, check_launch};
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

    // No default endpoint (rule 7): the public one is rate-limited, and
    // picking it silently would be picking for the operator.
    let client = crate::flag(args, "--rpc").map_or_else(
        || RpcClient::from_vars(&|k| std::env::var(k).ok()),
        RpcClient::new,
    );
    let seconds = crate::flag(args, "--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let mut budget = Budget::new(
        DEFAULT_MAX_CALLS,
        DEFAULT_MAX_PAGES,
        Duration::from_secs(seconds),
    );

    let tx = client
        .transaction(&mut budget, &signature)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{signature} is not in a block yet"))?;

    // The mint is read from the transaction itself before any account fetch
    // -- there is nowhere else to derive the curve or mint address from.
    // `check_launch` below still runs its own full scan and refuses on more
    // than one launch instruction, so a second one here is never silently
    // dropped for being second.
    let mint = candidate_mint(&tx);
    let curve_account = mint
        .and_then(|m| realorrug_pumpfun::pda::bonding_curve(&m))
        .and_then(|curve| client.account(&mut budget, &curve).ok().flatten())
        .map(|a| a.data);
    let mint_account = mint
        .and_then(|m| client.account(&mut budget, &m).ok().flatten())
        .map(|a| a.data);

    let result = check_launch(
        &tx,
        curve_account.as_deref(),
        mint_account.as_deref(),
        &treasury,
        &dev_wallet,
        dev_buy_lamports,
    );

    let text = report(&signature, &result);
    if result.clean() {
        print!("{text}");
        Ok(())
    } else {
        Err(text)
    }
}

/// What the operator reads: one line per check, PASS or REFUSE with the
/// value read.
fn report(signature: &str, result: &LaunchCheck) -> String {
    let mut lines = vec![format!(
        "{} launch {signature} (slot {})",
        if result.clean() { "CLEAN" } else { "NOT CLEAN:" },
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(text.starts_with("NOT CLEAN: launch sig123 (slot 5)\n"), "{text}");
        assert!(text.contains("REFUSE  fee recipient: not the treasury"), "{text}");
        assert!(text.contains("PASS    single launch: mint abc"), "{text}");
    }
}
