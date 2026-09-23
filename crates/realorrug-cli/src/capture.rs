// SPDX-License-Identifier: Apache-2.0
//! `realorrug capture <mint> --out <dir>` — a fact sheet read from the chain
//! and frozen to disk.
//!
//! `roast` answers "what would the bot say right now"; `capture` answers "what
//! did the bot measure just now" and writes the measurement down, with nothing
//! about how it would be phrased. That split is what makes `replay` possible:
//! a capture holds no model output and no verdict wording, only the facts and
//! the deterministic level and score a later run can recompute and compare
//! against, offline, with no chain and no key.
//!
//! Read-only, the same as `dossier` and `roast`. It never posts, and it never
//! calls a model — a capture is evidence, not a reply, and evidence that
//! already carried an opinion would not be worth keeping.

use std::path::Path;

use realorrug_onchain::{RpcClient, dispatch, dossier::Dossier};
use realorrug_roast::{Assessment, BaseRates, Capture, FactSheet};

use crate::dossier::safe;
use crate::flag;

/// Runs the command.
///
/// # Errors
///
/// A message when the mint or `--out` is missing or unparseable, when the
/// token's history cannot be read at all, or when the capture cannot be
/// written to `--out`.
pub fn run(args: &[String]) -> Result<(), String> {
    let mint_arg = mint_arg_from(args).ok_or_else(|| {
        "usage: realorrug capture <mint> --out <dir> [--rpc URL] [--label NAME]".to_owned()
    })?;
    let out_dir = flag(args, "--out").ok_or_else(|| {
        "usage: realorrug capture <mint> --out <dir> [--rpc URL] [--label NAME]".to_owned()
    })?;
    let label = flag(args, "--label").unwrap_or_default();

    // Same dispatcher `roast` and `dossier` go through: Solana or Robinhood,
    // decided purely from the address's own shape (`realorrug-onchain::dispatch`).
    // Plan 0002 phase 2 scopes captures to pump.fun/PumpSwap (Solana), but the
    // dispatcher is the one place that decision is made — restating it here
    // would be a second copy that could disagree with the first.
    let client = flag(args, "--rpc").map_or_else(
        || RpcClient::from_vars(&|k| std::env::var(k).ok()),
        RpcClient::new,
    );
    let market = realorrug_onchain::market::Http::default();
    let clients = dispatch::Clients {
        solana: &client,
        robinhood: None,
        market: Some(&market),
    };
    let dossier = match dispatch::read(&mint_arg, &clients) {
        Ok(d) => d,
        Err(dispatch::Error::NotAnAddress) => {
            return Err(format!("not a valid address: {}", safe(&mint_arg, 64)));
        }
        Err(dispatch::Error::Unreadable(why)) => return Err(why),
    };
    print_gaps(&dossier);

    let rates_path = flag(args, "--rates")
        .unwrap_or_else(|| realorrug_roast::baserates::DEFAULT_PATH.to_owned());
    let rates = BaseRates::load(&rates_path).ok();
    let creators = realorrug_roast::CreatorIndex::read(realorrug_roast::creator::DEFAULT_PATH).ok();
    let self_mint = realorrug_analyst::daemon::self_mint_from(&|k| std::env::var(k).ok())?;

    // No provider argument, and no reply built here at all (unlike `roast`):
    // a capture is the fact sheet the model would see, never what a model or
    // the template said about it. Freezing a reply here would let a capture
    // outlive today's voice pass and be replayed as if it were still current,
    // which is exactly the confusion `replay` recomputing the reply is meant
    // to prevent.
    let sheet = FactSheet::build(
        &dossier,
        rates.as_ref(),
        creators.as_ref(),
        self_mint.as_ref(),
        None,
    );
    let level = realorrug_roast::level(&sheet);
    let assessment = Assessment::from(&sheet);

    let capture = Capture {
        mint: mint_arg.clone(),
        label,
        captured_at: now(),
        rules_version: realorrug_roast::RULES_VERSION.to_owned(),
        level,
        assessment,
        sheet,
    };

    let dir = Path::new(&out_dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {out_dir}: {e}"))?;
    let path = dir.join(format!("{mint_arg}.sheet.json"));
    let json = serde_json::to_string_pretty(&capture)
        .map_err(|e| format!("cannot encode capture: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))?;

    println!("wrote {}", path.display());
    Ok(())
}

/// Prints every raw read gap this dossier hit, one `gap: ` line per gap, to
/// stderr -- for the operator running `capture` by hand, never for the sheet
/// or a model.
///
/// # Why stderr, and why here
///
/// `sheet.rs`'s own doc on `Dossier::unavailable` (around `phrase_for`)
/// explains why the raw reason never reaches `FactSheet` or a reply: it is
/// diagnostic text an attacker's own mint could shape (an injection surface,
/// AGENTS.md rule 3), and it is bad copy even when it is not hostile. That
/// reasoning does not apply to an operator's own terminal -- nothing printed
/// here is stored, sent to a model, or reachable by anyone but whoever ran
/// the command -- so `capture` can afford to say exactly what stopped a
/// read, which is what let this file's investigation
/// (`docs/research/0056-the-solana-replay-set.md`'s 2026-09-23 addendum)
/// confirm the call ceiling, not the page one, was starving Solana funding
/// reads.
///
/// Reads `dossier` straight from the one `dispatch::read` call `run` already
/// made -- the same chain read `roast` builds its own reply from
/// (`crates/realorrug-cli/src/roast.rs` calls `dispatch::read` too) -- so
/// this never triggers a second one.
fn print_gaps(dossier: &Dossier) {
    for miss in &dossier.unavailable {
        eprintln!("gap: {}: {}", miss.fact, miss.why);
    }
    if let Some(funding) = &dossier.funding {
        for gap in &funding.gaps {
            eprintln!("gap: {gap}");
        }
    }
}

/// The mint argument, exactly as `roast`'s own `mint_arg_from` reads it: the
/// first non-flag argument, or `--mint`.
fn mint_arg_from(args: &[String]) -> Option<String> {
    args.get(1)
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .or_else(|| flag(args, "--mint"))
}

/// Now, as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// The clock is the only thing this does; the formatting is
/// [`realorrug_types::civil::timestamp_from_seconds`], which is pure and
/// tested at its own boundaries.
fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    realorrug_types::civil::timestamp_from_seconds(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_mint_is_the_first_bare_argument() {
        assert_eq!(
            mint_arg_from(&args(&["capture", "SoMeMiNt", "--out", "dir"])),
            Some("SoMeMiNt".to_owned())
        );
    }

    #[test]
    fn the_mint_can_be_named_with_a_flag() {
        assert_eq!(
            mint_arg_from(&args(&["capture", "--mint", "SoMeMiNt", "--out", "dir"])),
            Some("SoMeMiNt".to_owned())
        );
    }

    #[test]
    fn no_bare_argument_and_no_flag_is_no_mint() {
        assert_eq!(mint_arg_from(&args(&["capture", "--out", "dir"])), None);
    }

    #[test]
    fn now_reads_as_a_utc_timestamp() {
        let text = now();
        assert!(text.ends_with('Z'), "{text}");
        assert!(text.contains('T'), "{text}");
    }

    // `Capture` itself, and its JSON round trip, are `realorrug-roast`'s own
    // type now (`realorrug-roast/src/capture.rs`), tested there --
    // `realorrug-roast/tests/accepted_replies_still_pass.rs` (plan 0002
    // phase 2, unit 5) needs to build one without depending on this crate,
    // which is why it moved. This file keeps only what is specific to the
    // command: the argument parsing and the clock above.
}
