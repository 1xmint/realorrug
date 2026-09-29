// SPDX-License-Identifier: Apache-2.0
//! Shares the daemon's monthly ledger with the commands an operator runs by
//! hand.
//!
//! `realorrug-analyst`'s daemon polls on its own, but `realorrug analyst`,
//! `realorrug roast` and `realorrug replay --model` call the same paid model
//! provider directly, from a terminal. Without this module each of those
//! commands would meter itself against a budget of its own -- or, before
//! this file existed, not meter itself at all -- and a hand-run call would
//! spend money the shared $90/month stop (ADR 0039 decision 5) never saw.
//! Opening the same [`realorrug_analyst::daemon::Paths::under`] ledger the
//! daemon does, from the same `REALORRUG_ANALYST_DIR`, is what makes the
//! stop a single number rather than one per caller.
//!
//! `realorrug replay` with no `--model` and `realorrug capture`, `dossier`,
//! `launch-check` and the rest make no paid model call at all -- they need no
//! budget and do not call anything in this module.

use realorrug_analyst::daemon::{Paths, budget_from, day_of, now, unfunded_notice};
use realorrug_analyst::{Prices, Spend};

/// Opens the shared ledger, priced and budgeted from the environment.
///
/// `None` when prices are not fully configured (rule 8: an unpriced call
/// cannot be metered, so nothing may be answered by a paid provider). A
/// closed budget still opens a `Spend` -- `unfunded_notice` has already been
/// printed by then -- because [`gate`] refusing every reservation against it
/// is the same "no paid call" outcome reached the same way the daemon
/// reaches it, rather than a second, CLI-only code path for it.
#[must_use]
pub fn open() -> Option<Spend> {
    let env = |k: &str| std::env::var(k).ok();
    let prices = Prices::from_vars(&env)?;
    let budget = budget_from(&env);
    if let Some(notice) = unfunded_notice(budget) {
        eprintln!("{notice}");
    }
    let dir = std::env::var("REALORRUG_ANALYST_DIR")
        .or_else(|_| std::env::var("RADAR_ANALYST_DIR"))
        .unwrap_or_else(|_| "data/analyst".to_owned());
    Some(Spend::open(
        budget,
        prices,
        Paths::under(&dir).ledger,
        day_of(now()),
    ))
}

/// Today's day number, on the ledger's own clock -- exported so a caller
/// reserving a model call (`realorrug_analyst::daemon::gate_model_call`) and
/// this module's `open` agree on what day it is without each reading the
/// clock separately.
#[must_use]
pub fn today() -> u64 {
    day_of(now())
}
