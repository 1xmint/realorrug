// SPDX-License-Identifier: Apache-2.0
//! Meters the commands an operator runs by hand against their own ledger,
//! beside the daemon's rather than inside it.
//!
//! `realorrug-analyst`'s daemon polls on its own, but `realorrug analyst`,
//! `realorrug roast` and `realorrug replay --model` call the same paid model
//! provider directly, from a terminal. Without this module each of those
//! commands would not meter itself at all, and a hand-run call would spend
//! money the shared $90/month stop (ADR 0039 decision 5) never saw.
//!
//! This module used to open the daemon's own ledger file
//! (`realorrug_analyst::daemon::Paths::under`'s `ledger`). That let a hand-run
//! charge and the daemon's own next save erase each other -- both processes
//! wrote the same path, and whichever saved last won, with the daemon never
//! even reserving against what the CLI had already spent. Per the pattern
//! design 0032 set for `realorrug-serve`'s own monthly figure (9-27-0026b
//! defect 2, orchestrator decision 2026-09-28): **each process meters its own
//! ledger**, in a file beside the daemon's rather than the daemon's own, and
//! the monthly ceiling is split so the sum of every slice can never exceed it
//! ([`realorrug_provider::daemon_monthly_allowance_from`],
//! [`realorrug_provider::cli_monthly_allowance_from`]). This module's own
//! ledger is metered by `REALORRUG_CLI_MONTHLY_USD` alone; unset or invalid
//! closes it (rule 7) rather than borrowing room from the daemon's or
//! serve's share.
//!
//! `realorrug replay` with no `--model` and `realorrug capture`, `dossier`,
//! `launch-check` and the rest make no paid model call at all -- they need no
//! budget and do not call anything in this module.

use realorrug_analyst::daemon::{cli_budget_from, day_of, now, unfunded_notice};
use realorrug_analyst::{Prices, Spend};

/// Opens this process's own ledger, priced and budgeted from the environment.
///
/// `None` when prices are not fully configured (rule 8: an unpriced call
/// cannot be metered, so nothing may be answered by a paid provider). A
/// closed budget still opens a `Spend` -- `unfunded_notice` has already been
/// printed by then -- because
/// [`gate_model_call`](realorrug_analyst::daemon::gate_model_call) refusing
/// every reservation against it is the same "no paid call" outcome reached
/// the same way the daemon reaches it, rather than a second, CLI-only code
/// path for it.
#[must_use]
pub fn open() -> Option<Spend> {
    let env = |k: &str| std::env::var(k).ok();
    let prices = Prices::from_vars(&env)?;
    let budget = cli_budget_from(&env);
    if let Some(notice) = unfunded_notice(budget) {
        eprintln!("{notice}");
    }
    let dir = std::env::var("REALORRUG_ANALYST_DIR")
        .or_else(|_| std::env::var("RADAR_ANALYST_DIR"))
        .unwrap_or_else(|_| "data/analyst".to_owned());
    // Beside the daemon's ledger (`Paths::under(&dir).ledger` names
    // `<dir>/ledger.json`), never the daemon's own file -- see the module doc.
    let ledger = format!("{dir}/cli-ledger.json");
    Some(Spend::open(budget, prices, ledger, day_of(now())))
}

/// Today's day number, on the ledger's own clock -- exported so a caller
/// reserving a model call (`realorrug_analyst::daemon::gate_model_call`) and
/// this module's `open` agree on what day it is without each reading the
/// clock separately.
#[must_use]
pub fn today() -> u64 {
    day_of(now())
}

#[cfg(test)]
mod tests {
    use realorrug_analyst::daemon::{Paths, cli_budget_from};
    use realorrug_analyst::{Cost, Prices, Spend};
    use realorrug_types::MicroUsd;

    fn prices() -> Prices {
        Prices {
            mention_read: MicroUsd(1_000),
            post_read: MicroUsd(5_000),
            reply: MicroUsd(10_000),
            post: MicroUsd(15_000),
            model_call: MicroUsd(2_000),
            user_read: MicroUsd(20_000),
        }
    }

    #[test]
    fn a_cli_charge_never_lands_in_the_daemon_s_ledger_file() {
        // (d) Before 9-27-0026b, this module opened `Paths::under(dir).ledger`
        // -- the daemon's own file -- so a hand-run charge and the daemon's
        // next save erased each other. Re-apply that bug by writing the CLI's
        // spend to `Paths::under(&dir).ledger` here instead of a sibling
        // path, and this fails: the daemon's ledger would carry the CLI's
        // reservation instead of staying exactly as it started, untouched.
        let dir = std::env::temp_dir().join(format!("radar-cli-ledger-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let dir = dir.to_string_lossy().into_owned();

        let daemon_ledger = Paths::under(&dir).ledger;
        let _ = std::fs::remove_file(&daemon_ledger);
        std::fs::write(&daemon_ledger, "untouched").expect("seed the daemon's own file");

        let cli_ledger = format!("{dir}/cli-ledger.json");
        let _ = std::fs::remove_file(&cli_ledger);
        assert_ne!(
            cli_ledger, daemon_ledger,
            "the CLI's ledger path must never be the daemon's own file"
        );

        // A budget built the same way `open()` builds one -- through
        // `cli_budget_from`, never a `Budget` literal named directly, which
        // would need a dependency this crate does not otherwise have.
        let mut env = std::collections::HashMap::new();
        env.insert("REALORRUG_ANALYST_DAILY_USD".to_owned(), "1.0".to_owned());
        env.insert(
            "REALORRUG_ANALYST_PER_CALL_USD".to_owned(),
            "1.0".to_owned(),
        );
        env.insert("REALORRUG_CLI_MONTHLY_USD".to_owned(), "1.0".to_owned());
        let get = |k: &str| env.get(k).cloned();
        let budget = cli_budget_from(&get);

        let mut cli_spend = Spend::open(budget, prices(), cli_ledger, 1);
        let c = cli_spend.authorize(Cost::ModelCall, 1).expect("authorised");
        cli_spend.settle(c, MicroUsd::from_dollars(0.5));

        assert_eq!(
            std::fs::read_to_string(&daemon_ledger).expect("still there"),
            "untouched",
            "a CLI reservation must never write to the daemon's own ledger file"
        );
    }

    #[test]
    fn today_reports_the_real_day_number_not_a_stub() {
        // CI's mutation report flagged "replace today -> u64 with 0" and
        // "with 1" as MISSED: nothing here asserted `today()` returned
        // anything but *some* `u64`. 19,000 days since the epoch is
        // 2022-01-15 -- comfortably behind any date this repository is
        // worked on -- so a stubbed 0 or 1 fails this bound while the real
        // day-since-epoch count clears it.
        let day = crate::spend::today();
        assert!(day > 19_000, "today() looks stubbed: got {day}");
    }
}
