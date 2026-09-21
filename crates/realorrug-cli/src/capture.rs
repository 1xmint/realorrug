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

use realorrug_onchain::{RpcClient, dispatch};
use realorrug_roast::{Assessment, BaseRates, FactSheet, Level};
use serde::{Deserialize, Serialize};

use crate::dossier::safe;
use crate::flag;

/// One capture: a fact sheet, frozen with the reading it was built from and
/// the rule version that scored it.
///
/// `replay` reads this back and recomputes `level`/`assessment` from `sheet`
/// to see whether the rules have moved since — so every field here is either
/// the raw sheet or context about the reading, never a cached opinion `replay`
/// would otherwise have no way to tell from a fresh one.
#[derive(Debug, Serialize, Deserialize)]
pub struct Capture {
    /// The mint this capture is of, as given on the command line.
    pub mint: String,
    /// An operator-chosen name for the case, blank when none was given.
    ///
    /// A `String` rather than `Option<String>`: an empty label and an absent
    /// one both display as nothing in `review.md`, and a second field carrying
    /// only that distinction is not a case a reviewer needs to see.
    #[serde(default)]
    pub label: String,
    /// Wall-clock time this capture was taken, `YYYY-MM-DDTHH:MM:SSZ`.
    ///
    /// Distinct from `sheet.read_at` (the chain's own clock, a slot or a
    /// block): this is when the *operator's machine* asked, which is what a
    /// reviewer months later needs to judge how stale a capture is against
    /// today, without knowing how to convert a Solana slot to a date.
    pub captured_at: String,
    /// [`realorrug_roast::RULES_VERSION`] at capture time, so a replay months
    /// later can tell a rule change from a data change.
    pub rules_version: String,
    /// The verdict level, computed the same way `roast` computes it.
    pub level: Level,
    /// The full risk assessment `roast` builds under that level.
    pub assessment: Assessment,
    /// The fact sheet itself — everything a reply or a report may state.
    pub sheet: FactSheet,
}

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

    /// A capture round-trips through JSON: `replay` reads back exactly what
    /// `capture` wrote, with no field lost to a `#[serde(skip)]` or a type
    /// that only serializes.
    fn empty_dossier() -> realorrug_onchain::Dossier {
        realorrug_onchain::Dossier {
            mint: "11111111111111111111111111111112"
                .parse()
                .expect("an address"),
            read_at: None,
            launch: None,
            curve: None,
            creator_transactions: None,
            chain_launch: None,
            holders: None,
            funding: None,
            market: None,
            token_ownership: None,
            creator_cash_flow: None,
            powers: None,
            unavailable: Vec::new(),
            calls: 0,
            elapsed_ms: 0,
        }
    }

    #[test]
    fn a_capture_round_trips_through_json() {
        let sheet = FactSheet::build(&empty_dossier(), None, None, None, None);
        let level = realorrug_roast::level(&sheet);
        let assessment = Assessment::from(&sheet);
        let capture = Capture {
            mint: "SoMeMiNt".to_owned(),
            label: "ordinary launch".to_owned(),
            captured_at: "2026-09-21T00:00:00Z".to_owned(),
            rules_version: realorrug_roast::RULES_VERSION.to_owned(),
            level,
            assessment,
            sheet,
        };
        let json = serde_json::to_string(&capture).expect("encode");
        let back: Capture = serde_json::from_str(&json).expect("decode");
        assert_eq!(back.mint, capture.mint);
        assert_eq!(back.label, capture.label);
        assert_eq!(back.rules_version, capture.rules_version);
    }
}
