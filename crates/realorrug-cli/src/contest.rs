// SPDX-License-Identifier: Apache-2.0
//! `realorrug contest`: the payout's manual fallback, through the same checks.
//!
//! Design 0007 C5, ADR 0025. Three subcommands, and none holds a key:
//!
//! - `pay --dry-run --week N --wallet <address> --token <address> --rpc <url>
//!   [--contest-dir <dir>]` runs the payout's own preflight and plan -- the
//!   chain, the wallet as the token's creator fee recipient, the escrow, the
//!   policy, the recipient, the gas -- and prints the claim and the transfer it
//!   would sign. Nothing is signed here.
//! - `record-payout --week N --wallet <address> --rpc <url> --claim-tx <hash>
//!   --transfer-tx <hash>` reads a hand-made claim and transfer back through
//!   the same checks the automated run uses -- the escrow's figure from the
//!   claim, and a transfer of exactly that from the wallet to the claim -- and
//!   only then writes the payout into the week's record.
//! - `void --week N --reason <words>` voids a week, and touches no chain.
//!
//! The fallback is therefore exercised by the automated path's own tests,
//! which is the condition design 0007 set for having one at all.

use realorrug_contest::Week;
use realorrug_payout::{Config, plan, preflight, record_payout};
use realorrug_robinhood::{Address, Hash32, Rpc};

fn address(args: &[String], name: &str, what: &str) -> Result<Address, String> {
    crate::flag(args, name)
        .ok_or_else(|| format!("{name} <address> is required: {what}"))?
        .parse()
        .map_err(|e| format!("{name}: {e}"))
}

fn hash(args: &[String], name: &str) -> Result<Hash32, String> {
    crate::flag(args, name)
        .ok_or_else(|| format!("{name} <hash> is required"))?
        .parse()
        .map_err(|e| format!("{name}: {e}"))
}

/// Runs the command.
///
/// # Errors
///
/// A message when a flag is missing, the chain cannot be read, the record is
/// missing, or the policy refuses.
pub fn run(args: &[String]) -> Result<(), String> {
    let sub = args.get(1).map(String::as_str);
    let week = crate::flag(args, "--week")
        .and_then(|w| w.parse::<u64>().ok())
        .map(Week)
        .ok_or("--week <n> is required")?;
    let contest_dir =
        crate::flag(args, "--contest-dir").unwrap_or_else(|| "data/contest".to_owned());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    // Voiding reads and writes one file. It touches no chain and no wallet, so
    // it must not require the flags that do: an operator voiding a bought week
    // has enough to think about without looking up an address to satisfy an
    // argument parser. Handled before those are read, which is also what makes
    // it testable without a chain.
    if sub == Some("void") {
        let reason = crate::flag(args, "--reason")
            .ok_or("--reason <words> is required, and it is published verbatim")?;
        let record = realorrug_analyst::contest::void_week(&contest_dir, week, &reason, now)?;
        let voided = record.voided.as_ref().ok_or("the week was not voided")?;
        println!(
            "week {}: voided, pays nobody, the pool rolls over.
reason, published verbatim: {}",
            week.0, voided.reason
        );
        return Ok(());
    }

    match sub {
        Some("pay") => {
            if !args.iter().any(|a| a == "--dry-run") {
                return Err("`realorrug contest pay` only plans; pass --dry-run to say so, and sign elsewhere".to_owned());
            }
            let wallet = address(
                args,
                "--wallet",
                "the payout wallet, the token's creator fee recipient",
            )?;
            let token = address(args, "--token", "the token whose creator fees are the prize")?;
            let rpc = crate::flag(args, "--rpc")
                .ok_or("--rpc <url> is required: a Robinhood Chain endpoint")?;
            let chain = Rpc::new(rpc);
            // The same floor the timer uses, read the same way. A hand
            // payment that ignored it would be a second policy.
            let config = Config {
                wallet,
                token,
                floor: realorrug_payout::floor_from(&|k| std::env::var(k).ok()),
            };
            preflight(&chain, &config).map_err(|e| e.to_string())?;
            let planned =
                plan(&chain, &contest_dir, week, &config, now).map_err(|e| e.to_string())?;
            println!(
                "{}\nsign the claim, then a transfer of exactly the wei its Claimed event reports; then `realorrug contest record-payout --claim-tx <hash> --transfer-tx <hash>`",
                planned.describe()
            );
            Ok(())
        }
        Some("record-payout") => {
            let wallet = address(args, "--wallet", "the payout wallet that claimed and paid")?;
            let rpc = crate::flag(args, "--rpc")
                .ok_or("--rpc <url> is required: a Robinhood Chain endpoint")?;
            let claim_tx = hash(args, "--claim-tx")?;
            let transfer_tx = hash(args, "--transfer-tx")?;
            let payout = record_payout(
                &Rpc::new(rpc),
                &contest_dir,
                week,
                &wallet,
                &claim_tx,
                &transfer_tx,
                now,
            )
            .map_err(|e| e.to_string())?;
            println!(
                "week {}: recorded {:?} to {}",
                week.0, payout.paid, payout.recipient
            );
            Ok(())
        }
        _ => Err(
            "realorrug contest <pay --dry-run --wallet <address> --token <address> | record-payout --wallet <address> --claim-tx <hash> --transfer-tx <hash> | void --reason <words>> --week <n> [--rpc <url>]"
                .to_owned(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn pay_refuses_to_do_anything_without_dry_run_before_it_touches_a_chain() {
        // The command only plans; saying so is the flag. CI's mutants inverted
        // the check and nothing failed, because nothing had run the command.
        // The endpoint here is a closed port: a run that got past the check
        // would fail on the network and not with this message.
        let wallet = Address([1u8; 20]).to_string();
        let out = run(&args(&[
            "contest",
            "pay",
            "--week",
            "1",
            "--wallet",
            &wallet,
            "--token",
            &wallet,
            "--rpc",
            "http://127.0.0.1:1",
        ]));
        let why = out.expect_err("refused");
        assert!(why.contains("--dry-run"), "{why}");
        // And the usage line for anything else.
        let other = run(&args(&["contest", "sing", "--week", "1"]));
        assert!(other.expect_err("usage").contains("record-payout"));
        assert!(
            run(&args(&["contest", "pay"]))
                .expect_err("flags")
                .contains("--week")
        );
    }

    #[test]
    fn the_fallback_names_the_flag_it_is_missing_before_it_touches_a_chain() {
        // A Solana address from before the move is not a wallet here either.
        let solana = "So11111111111111111111111111111111111111112";
        let pay = run(&args(&[
            "contest",
            "pay",
            "--dry-run",
            "--week",
            "1",
            "--wallet",
            solana,
        ]))
        .expect_err("refused");
        assert!(pay.starts_with("--wallet"), "{pay}");
        let wallet = Address([1u8; 20]).to_string();
        let record = run(&args(&[
            "contest",
            "record-payout",
            "--week",
            "1",
            "--wallet",
            &wallet,
            "--rpc",
            "http://127.0.0.1:1",
            "--claim-tx",
            "0x01",
        ]))
        .expect_err("refused");
        assert!(record.starts_with("--claim-tx"), "{record}");
    }

    /// The command name plus the words, the way `main` passes them.
    fn voidargs(words: &[&str]) -> Vec<String> {
        std::iter::once("contest")
            .chain(words.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn voiding_a_week_needs_no_chain_and_no_creator_wallet() {
        // It reads and writes one file. Requiring `--creator` and `--rpc` for
        // it -- which the dispatch did until 2026-09-06, because the arm was
        // bolted onto a command that pays -- means an operator voiding a bought
        // week has to look up a creator address first, at the moment they have
        // the least patience for it.
        //
        // CI's mutants deleted the whole `Some("void")` arm and nothing failed,
        // because the arm was unreachable from a test for exactly that reason.
        // Re-apply by deleting it now: this fails on the usage string.
        let dir = std::env::temp_dir().join(format!("realorrug-cli-void-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.to_string_lossy().into_owned();

        let rules = realorrug_contest::Rules::published(["op"]);
        let record = realorrug_contest::Record::close(
            Week(2957),
            realorrug_contest::Ranking::default(),
            &rules,
        );
        realorrug_analyst::contest::write_record(&path, &record).expect("write");

        run(&voidargs(&[
            "void",
            "--week",
            "2957",
            "--reason",
            "every point came from six accounts made that morning",
            "--contest-dir",
            &path,
        ]))
        .expect("voided with no chain flags");

        let back = realorrug_contest::records_in(std::path::Path::new(&path));
        assert_eq!(
            back[0].voided.as_ref().expect("voided").reason,
            "every point came from six accounts made that morning"
        );
    }

    #[test]
    fn voiding_still_needs_a_week_and_a_reason() {
        // The two things it cannot invent. A missing reason in particular:
        // the reason is the mechanism, and a void nobody can read is the
        // private correction design 0011 rejects.
        let dir = std::env::temp_dir().join(format!("realorrug-cli-void2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.to_string_lossy().into_owned();

        let no_reason = run(&voidargs(&[
            "void",
            "--week",
            "2957",
            "--contest-dir",
            &path,
        ]))
        .expect_err("no reason");
        assert!(no_reason.contains("--reason"), "{no_reason}");

        let no_week = run(&voidargs(&[
            "void",
            "--reason",
            "x",
            "--contest-dir",
            &path,
        ]))
        .expect_err("no week");
        assert!(no_week.contains("--week"), "{no_week}");
    }
}
