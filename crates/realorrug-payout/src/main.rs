// SPDX-License-Identifier: Apache-2.0
//! The payout process.
//!
//! Runs as its own systemd unit under its own user, with the Turnkey API key
//! readable by nobody else, on a timer. Everything that decides anything is in
//! the library or in the small functions below, which a test can call; `main`
//! reads the configuration and prints what happened.
//!
//! # Configuration
//!
//! Every one of these is required, and any of them unset means nothing is paid
//! (AGENTS.md rule 7):
//!
//! - `TURNKEY_API_KEY` -- path to the Turnkey API private key: the `key.private`
//!   Turnkey's CLI writes (64 hex digits, `:p256`), mode 0400.
//! - `TURNKEY_API_PUBLIC_KEY` -- its compressed public key, as the Turnkey
//!   dashboard shows it. The file must derive it.
//! - `TURNKEY_ORGANIZATION_ID` -- the Turnkey organisation.
//! - `RADAR_PAYOUT_ADDRESS` -- the wallet Turnkey signs for: the token's creator
//!   fee recipient.
//! - `REALORRUG_TOKEN` -- the token. Unset before launch, so nothing is paid
//!   before there is anything to pay.
//! - `REALORRUG_ROBINHOOD_RPC` -- the Robinhood Chain endpoint. Its own name, so
//!   the analyst's Solana `RADAR_RPC_URL` can never be picked up by mistake.
//!
//! Optional: `RADAR_PAYOUT_FLOOR_WEI` (unset is no floor) and
//! `RADAR_CONTEST_DIR` (`data/contest`).
//!
//! `--week N` names the week; `--due` pays every claimed, unpaid week, which
//! is what the timer runs; `--dry-run` plans and signs nothing;
//! `--setup-proof` runs ADR 0025's three Turnkey requests and sends nothing to
//! any chain.

use std::process::ExitCode;

use realorrug_contest::{Record, Week};
use realorrug_payout::turnkey::{API, Turnkey, load_api_key};
use realorrug_payout::{
    Config, Lock, PayError, floor_from, floor_notice, pay, pending_weeks, plan, preflight,
    setup_proof,
};
use realorrug_robinhood::{Address, Rpc};

/// A variable's value, with blank counting as unset.
fn present(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

fn env(key: &str) -> Option<String> {
    present(std::env::var(key).ok())
}

/// The value following `name`, if it is there.
fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

/// Everything the Turnkey half needs.
#[derive(Debug, PartialEq, Eq)]
struct Signing {
    key_path: String,
    public_key: String,
    organization: String,
    wallet: Address,
}

/// Everything a paying run needs.
#[derive(Debug, PartialEq, Eq)]
struct Settings {
    signing: Signing,
    token: Address,
    rpc: String,
}

/// An address variable: `None` with a complaint pushed when it is unset or
/// does not parse.
fn address_var(
    get: &impl Fn(&str) -> Option<String>,
    name: &str,
    unset: &str,
    missing: &mut Vec<String>,
) -> Address {
    match present(get(name)) {
        None => {
            missing.push(format!("{name} is not set{unset}"));
            Address::ZERO
        }
        Some(text) => text.trim().parse().unwrap_or_else(|e| {
            missing.push(format!("{name} does not parse: {e}"));
            Address::ZERO
        }),
    }
}

/// Reads the Turnkey variables, naming every one that is missing or will not
/// parse rather than stopping at the first, so one edit fixes them all.
fn signing_from(get: &impl Fn(&str) -> Option<String>, missing: &mut Vec<String>) -> Signing {
    let mut need = |name: &str| {
        present(get(name)).unwrap_or_else(|| {
            missing.push(format!("{name} is not set"));
            String::new()
        })
    };
    let key_path = need("TURNKEY_API_KEY");
    let public_key = need("TURNKEY_API_PUBLIC_KEY");
    let organization = need("TURNKEY_ORGANIZATION_ID");
    let wallet = address_var(get, "RADAR_PAYOUT_ADDRESS", "", missing);
    Signing {
        key_path,
        public_key,
        organization,
        wallet,
    }
}

/// The Turnkey variables alone, for `--setup-proof`, which touches no chain.
fn proof_settings_from(get: &impl Fn(&str) -> Option<String>) -> Result<Signing, Vec<String>> {
    let mut missing = Vec::new();
    let signing = signing_from(get, &mut missing);
    if missing.is_empty() {
        Ok(signing)
    } else {
        Err(missing)
    }
}

fn settings_from(get: &impl Fn(&str) -> Option<String>) -> Result<Settings, Vec<String>> {
    let mut missing = Vec::new();
    let signing = signing_from(get, &mut missing);
    let token = address_var(get, "REALORRUG_TOKEN", " (no token yet)", &mut missing);
    let rpc = present(get("REALORRUG_ROBINHOOD_RPC")).unwrap_or_else(|| {
        missing.push("REALORRUG_ROBINHOOD_RPC is not set".to_owned());
        String::new()
    });
    if missing.is_empty() {
        Ok(Settings {
            signing,
            token,
            rpc,
        })
    } else {
        Err(missing)
    }
}

/// Which weeks a run pays.
///
/// A pending payout comes first and alone: a run that finds one resumes that
/// week and does nothing else, because another week's claim beside an
/// unsettled one would race it for the same escrow balance and the next nonce.
/// Otherwise every claimed, unpaid record under `--due`, or the one `--week`
/// names.
///
/// # Errors
///
/// A message when two payouts are pending, or neither flag is usable.
fn weeks_to_pay(
    args: &[String],
    records: &[Record],
    pending: &[Week],
) -> Result<Vec<Week>, String> {
    match pending {
        [] => {}
        [one] => return Ok(vec![*one]),
        many => {
            return Err(format!(
                "{} payouts are pending ({many:?}); one run finishes one, so check them by hand",
                many.len()
            ));
        }
    }
    if has(args, "--due") {
        let mut due: Vec<Week> = records
            .iter()
            .filter(|r| r.claim.is_some() && r.payout.is_none())
            .map(|r| r.week)
            .collect();
        // **Ascending, and this is not cosmetic** (finding S18). Each payment
        // is everything the escrow holds, so with two weeks due the first one
        // paid takes the lot. The earliest due week takes it, because it has
        // been waiting longest, and the order is the same on every machine.
        due.sort_unstable();
        return Ok(due);
    }
    flag(args, "--week")
        .and_then(|w| w.parse::<u64>().ok())
        .map(|n| vec![Week(n)])
        .ok_or_else(|| "--week <n> or --due is required".to_owned())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn refuse(lines: &[String]) -> ExitCode {
    for line in lines {
        eprintln!("realorrug-payout: {line}, so nothing is paid.");
    }
    ExitCode::FAILURE
}

fn turnkey(signing: &Signing) -> Result<Turnkey, String> {
    let key = load_api_key(std::path::Path::new(&signing.key_path), &signing.public_key)
        .map_err(|e| e.to_string())?;
    Ok(Turnkey::new(
        API,
        &signing.organization,
        signing.wallet,
        key,
    ))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let contest_dir = env("RADAR_CONTEST_DIR").unwrap_or_else(|| "data/contest".to_owned());
    let get = |k: &str| std::env::var(k).ok();

    if has(&args, "--setup-proof") {
        let signing = match proof_settings_from(&get) {
            Ok(s) => s,
            Err(missing) => return refuse(&missing),
        };
        let client = match turnkey(&signing) {
            Ok(c) => c,
            Err(e) => return refuse(&[e]),
        };
        let (lines, held) = match setup_proof(&client, &signing.wallet) {
            Ok(lines) => (lines, true),
            Err(lines) => (lines, false),
        };
        for line in lines {
            println!("{line}");
        }
        return if held {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    let settings = match settings_from(&get) {
        Ok(s) => s,
        Err(missing) => return refuse(&missing),
    };
    let signer = match turnkey(&settings.signing) {
        Ok(c) => c,
        Err(e) => return refuse(&[e]),
    };
    let chain = Rpc::new(&settings.rpc);
    let floor = floor_from(&get);
    eprintln!("{}", floor_notice(floor));
    let config = Config {
        wallet: settings.signing.wallet,
        token: settings.token,
        floor,
    };

    let _lock = match Lock::acquire(&contest_dir) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("realorrug-payout: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = preflight(&chain, &config) {
        eprintln!("realorrug-payout: {e}");
        return ExitCode::FAILURE;
    }
    let records = realorrug_contest::records_in(std::path::Path::new(&contest_dir));
    let weeks = match weeks_to_pay(&args, &records, &pending_weeks(&contest_dir)) {
        Ok(weeks) => weeks,
        Err(why) => {
            eprintln!("realorrug-payout: {why}");
            return ExitCode::FAILURE;
        }
    };
    if weeks.is_empty() {
        println!("realorrug-payout: nothing is claimed and unpaid.");
        return ExitCode::SUCCESS;
    }

    let dry_run = has(&args, "--dry-run");
    for week in weeks {
        let at = now();
        let outcome = if dry_run {
            plan(&chain, &contest_dir, week, &config, at).map(|p| p.describe())
        } else {
            pay(&chain, &signer, &contest_dir, week, &config, at).map(|p| {
                format!(
                    "week {}: paid {:?} to {}, transfer {}",
                    week.0,
                    p.paid,
                    p.recipient,
                    p.transaction()
                )
            })
        };
        match outcome {
            Ok(text) => println!("{text}"),
            // A refusal or an empty pool is the answer, not an error: the timer
            // runs again and nothing was wrong with the process.
            Err(PayError::Refused(why)) => println!("week {}: refused: {why:?}", week.0),
            Err(e @ PayError::NothingCollected(_)) => println!("week {}: {e}", week.0),
            Err(e) => {
                // Stop here. A week left half-paid has a pending file, and the
                // next run finishes it before anything else is started.
                eprintln!("realorrug-payout: week {}: {e}", week.0);
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use realorrug_contest::{Claim, Paid, Payout, Ranking};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn record(week: u64, claimed: bool, paid: bool) -> Record {
        let mut r = Record::close(
            Week(week),
            Ranking::default(),
            &realorrug_contest::Rules::published(["op"]),
        );
        if claimed {
            r.claim = Some(Claim {
                address: "A".to_owned(),
                reply_id: "c".to_owned(),
                at: 1,
            });
        }
        if paid {
            r.payout = Some(Payout {
                recipient: "A".to_owned(),
                paid: Paid::Sol {
                    lamports: 1,
                    signature: "S".to_owned(),
                },
                at: 2,
            });
        }
        r
    }

    #[test]
    fn a_blank_variable_is_unset_and_a_flag_takes_the_value_after_it() {
        assert_eq!(present(Some("  ".to_owned())), None);
        assert_eq!(present(Some("x".to_owned())), Some("x".to_owned()));
        assert_eq!(present(None), None);
        let a = args(&["--week", "7", "--dry-run"]);
        assert_eq!(flag(&a, "--week"), Some("7".to_owned()));
        assert_eq!(
            flag(&a, "--dry-run"),
            None,
            "a flag at the end has no value"
        );
        assert_eq!(flag(&a, "--nope"), None);
        assert!(has(&a, "--dry-run"));
        assert!(!has(&args(&["--week", "7"]), "--dry-run"));
    }

    #[test]
    fn every_missing_variable_is_named_and_any_one_missing_pays_nothing() {
        // Re-apply by defaulting the RPC to the public endpoint: the loop finds
        // no complaint about it, and a spending path nobody chose is configured.
        let wallet = format!("0x{}", "11".repeat(20));
        let token = format!("0x{}", "22".repeat(20));
        let all = |k: &str| match k {
            "TURNKEY_API_KEY" => Some("/etc/realorrug/turnkey.key".to_owned()),
            "TURNKEY_API_PUBLIC_KEY" => Some("02ab".to_owned()),
            "TURNKEY_ORGANIZATION_ID" => Some("org".to_owned()),
            "RADAR_PAYOUT_ADDRESS" => Some(wallet.clone()),
            "REALORRUG_TOKEN" => Some(token.clone()),
            "REALORRUG_ROBINHOOD_RPC" => Some("https://rpc".to_owned()),
            _ => None,
        };
        let got = settings_from(&all).expect("complete");
        assert_eq!(got.signing.wallet.to_string(), wallet);
        assert_eq!(got.token.to_string(), token);
        assert_eq!(got.signing.key_path, "/etc/realorrug/turnkey.key");
        assert_eq!(got.rpc, "https://rpc");

        let names = [
            "TURNKEY_API_KEY",
            "TURNKEY_API_PUBLIC_KEY",
            "TURNKEY_ORGANIZATION_ID",
            "RADAR_PAYOUT_ADDRESS",
            "REALORRUG_TOKEN",
            "REALORRUG_ROBINHOOD_RPC",
        ];
        for name in names {
            let without = |k: &str| {
                if k == name {
                    Some(" ".to_owned())
                } else {
                    all(k)
                }
            };
            let missing = settings_from(&without).expect_err(name);
            assert_eq!(missing.len(), 1, "{name}: {missing:?}");
            assert!(missing[0].starts_with(name), "{missing:?}");
        }
        let none = settings_from(&|_| None).expect_err("nothing set");
        assert_eq!(none.len(), names.len(), "{none:?}");

        // The analyst's Solana variable is not a substitute.
        let solana = |k: &str| match k {
            "REALORRUG_ROBINHOOD_RPC" => None,
            "RADAR_RPC_URL" => Some("https://solana".to_owned()),
            _ => all(k),
        };
        assert!(settings_from(&solana).is_err());

        let bad = |k: &str| {
            if k == "RADAR_PAYOUT_ADDRESS" {
                Some("So111".to_owned())
            } else {
                all(k)
            }
        };
        assert!(settings_from(&bad).expect_err("bad")[0].contains("does not parse"));
    }

    #[test]
    fn the_setup_proof_needs_the_turnkey_variables_and_nothing_else() {
        // It touches no chain, so it must run before a token or an endpoint
        // exists, which is when the operator sets Turnkey up. Re-apply by
        // inverting the emptiness check: a complete set is refused.
        let wallet = format!("0x{}", "11".repeat(20));
        let turnkey_only = |k: &str| match k {
            "TURNKEY_API_KEY" => Some("/etc/realorrug/turnkey.key".to_owned()),
            "TURNKEY_API_PUBLIC_KEY" => Some("02ab".to_owned()),
            "TURNKEY_ORGANIZATION_ID" => Some("org".to_owned()),
            "RADAR_PAYOUT_ADDRESS" => Some(wallet.clone()),
            _ => None,
        };
        let got = proof_settings_from(&turnkey_only).expect("enough for the proof");
        assert_eq!(got.organization, "org");
        assert!(settings_from(&turnkey_only).is_err(), "not enough to pay");
        let missing = proof_settings_from(&|k: &str| {
            if k == "TURNKEY_ORGANIZATION_ID" {
                None
            } else {
                turnkey_only(k)
            }
        })
        .expect_err("missing");
        assert_eq!(
            missing,
            vec!["TURNKEY_ORGANIZATION_ID is not set".to_owned()]
        );
    }

    #[test]
    fn due_pays_the_earliest_week_first_whatever_order_the_directory_is_in() {
        // Finding S18. Re-apply by deleting the `sort_unstable`: this fails,
        // because the records are handed over newest first.
        let records = [
            record(9, true, false),
            record(7, true, false),
            record(8, true, false),
        ];
        assert_eq!(
            weeks_to_pay(&args(&["--due"]), &records, &[]),
            Ok(vec![Week(7), Week(8), Week(9)])
        );
    }

    #[test]
    fn due_pays_the_claimed_and_unpaid_and_week_names_one() {
        let records = [
            record(1, false, false),
            record(2, true, false),
            record(3, true, true),
            record(4, true, false),
        ];
        assert_eq!(
            weeks_to_pay(&args(&["--due"]), &records, &[]),
            Ok(vec![Week(2), Week(4)])
        );
        assert_eq!(
            weeks_to_pay(&args(&["--week", "9"]), &records, &[]),
            Ok(vec![Week(9)])
        );
        assert!(weeks_to_pay(&args(&["--week", "nine"]), &records, &[]).is_err());
        assert!(weeks_to_pay(&args(&[]), &records, &[]).is_err());
    }

    #[test]
    fn a_pending_payout_is_the_only_week_a_run_touches() {
        // Re-apply by appending the due weeks after the pending one: week 2's
        // claim would race week 4's unsettled one for the same escrow balance.
        let records = [record(2, true, false), record(4, true, false)];
        assert_eq!(
            weeks_to_pay(&args(&["--due"]), &records, &[Week(4)]),
            Ok(vec![Week(4)])
        );
        assert_eq!(
            weeks_to_pay(&args(&["--week", "2"]), &records, &[Week(4)]),
            Ok(vec![Week(4)])
        );
        assert!(weeks_to_pay(&args(&["--due"]), &records, &[Week(2), Week(4)]).is_err());
    }
}
