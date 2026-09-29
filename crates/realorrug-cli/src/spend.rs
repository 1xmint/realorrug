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
//!
//! # One hand-run paid command at a time
//!
//! `cli-ledger.json` is a plain file, read once at [`open`] and written after
//! every reservation -- there is no in-memory total shared between processes
//! the way the daemon's is shared between its own two lanes. Two of these
//! commands started at once would each load the same starting figure, spend
//! independently, and each overwrite the other's save on exit, so `open`
//! also takes an exclusive OS lock on `cli-ledger.lock` for the life of the
//! run (9-27-0026c finding 3). The lock releases itself on exit or on a
//! crash, so nothing here can leave a stale one behind; a second `open()`
//! while the first is still running is refused, naming the lock path,
//! rather than racing it for the ledger file.

use std::fs::File;
use std::io::ErrorKind;

use realorrug_analyst::daemon::{cli_budget_from, day_of, now, unfunded_notice};
use realorrug_analyst::{Prices, Spend};

/// The open ledger for one hand-run paid command, plus the lock that keeps a
/// second one from racing it (9-27-0026c finding 3).
///
/// `Deref`/`DerefMut` to [`Spend`] so every existing caller of `open()` --
/// `authorize`, `settle`, `release`, and passing `&mut Handle` anywhere a
/// `&mut Spend` is expected (`gate_model_call`) -- needed no change of its
/// own. The lock file is held for exactly as long as this value lives: it is
/// never read again after `open()` confirms the lock, its only job is to
/// keep existing and keep the OS lock alive until `Drop`.
pub struct Handle {
    spend: Spend,
    _lock: File,
}

impl std::ops::Deref for Handle {
    type Target = Spend;
    fn deref(&self) -> &Spend {
        &self.spend
    }
}

impl std::ops::DerefMut for Handle {
    fn deref_mut(&mut self) -> &mut Spend {
        &mut self.spend
    }
}

/// Opens this process's own ledger, priced and budgeted from the
/// environment, and locked for this run alone.
///
/// `None` when prices are not fully configured (rule 8: an unpriced call
/// cannot be metered, so nothing may be answered by a paid provider), when
/// the ledger's directory cannot be created or locked, or when the ledger
/// cannot be saved -- each case prints why and says "no paid call" before
/// returning, per 9-27-0026c finding 2: a `Spend` that opened but could
/// never be written back would restore as a fresh, empty ledger on every
/// single run, making a monthly cap that in practice resets every process.
///
/// A closed *budget* still opens a `Handle` -- `unfunded_notice` has already
/// been printed by then -- because
/// [`gate_model_call`](realorrug_analyst::daemon::gate_model_call) refusing
/// every reservation against it is the same "no paid call" outcome reached
/// the same way the daemon reaches it, rather than a second, CLI-only code
/// path for it. That is a different case from the ones above: there, no
/// ledger could be trusted at all.
#[must_use]
pub fn open() -> Option<Handle> {
    let env = |k: &str| std::env::var(k).ok();
    let dir = std::env::var("REALORRUG_ANALYST_DIR")
        .or_else(|_| std::env::var("RADAR_ANALYST_DIR"))
        .unwrap_or_else(|_| "data/analyst".to_owned());
    open_at(&dir, &env)
}

/// The dir-parameterised core of [`open`], split out so tests can drive it
/// against a temp directory of their own choosing without touching process
/// environment variables -- this workspace forbids `unsafe_code`, and
/// `std::env::set_var` is `unsafe` as of edition 2024, so a test cannot set
/// `REALORRUG_ANALYST_DIR` (or any other var `open()` reads) and then call
/// `open()` itself. `get` is threaded through exactly like every other
/// env-reading function in this workspace (`cli_budget_from`,
/// `Prices::from_vars`) for the same reason: a test builds a `HashMap`
/// closure instead.
fn open_at(dir: &str, get: &impl Fn(&str) -> Option<String>) -> Option<Handle> {
    let prices = Prices::from_vars(get)?;
    let budget = cli_budget_from(get);
    if let Some(notice) = unfunded_notice(budget) {
        eprintln!("{notice}");
    }

    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("cannot create {dir}: {e}; no paid call");
        return None;
    }

    let lock = lock_ledger(dir)?;

    let spend = Spend::open(budget, prices, ledger_path(dir), day_of(now()));
    // Written immediately, not left for the first reservation's own save: an
    // unwritable ledger must close the run before anything is spent, not
    // after the first call finds out the hard way (finding 2). `persist`
    // (used by `authorize`/`settle`/`release` themselves) only logs a save
    // failure and carries on, which is right for a process already
    // committed to a reservation but wrong for deciding whether to open at
    // all.
    if let Err(e) = spend.save() {
        eprintln!(
            "cannot write the ledger at {}: {e}; no paid call",
            ledger_path(dir)
        );
        return None;
    }

    Some(Handle { spend, _lock: lock })
}

/// This process's own ledger file, beside the daemon's
/// (`Paths::under(dir).ledger` names `<dir>/ledger.json`), never the
/// daemon's own file -- see the module doc. A `fn` rather than an inline
/// `format!` in `open()` so the test proving a CLI charge never lands in the
/// daemon's file calls the same path-building code `open()` does, rather
/// than a second copy that could drift from it silently (9-27-0026c finding
/// 6).
fn ledger_path(dir: &str) -> String {
    format!("{dir}/cli-ledger.json")
}

/// The lock file naming one hand-run paid command's exclusive claim on
/// `ledger_path`, beside it rather than the ledger file itself -- an
/// exclusive lock on the ledger file would need it opened for writing before
/// `Spend::open` has even read it, and this way the lock's own lifetime has
/// nothing to do with how the ledger is read or written.
fn lock_path(dir: &str) -> String {
    format!("{dir}/cli-ledger.lock")
}

/// Takes the exclusive lock naming this run's claim on the CLI ledger, or
/// prints why not and returns `None`.
///
/// `dir` must already exist (`open` creates it first): a lock file cannot be
/// opened under a directory that is not there.
fn lock_ledger(dir: &str) -> Option<File> {
    let path = lock_path(dir);
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cannot open the lock file at {path}: {e}; no paid call");
            return None;
        }
    };
    match file.try_lock() {
        Ok(()) => Some(file),
        Err(std::fs::TryLockError::WouldBlock) => {
            eprintln!(
                "another hand-run paid command is running and holds {path}; wait for it or \
                 stop it. The lock releases when that process exits; deleting the file does \
                 not release it. No paid call."
            );
            None
        }
        Err(std::fs::TryLockError::Error(e)) => {
            eprintln!(
                "cannot lock {path}: {}; no paid call",
                lock_error_reason(&e)
            );
            None
        }
    }
}

/// Chooses the wording for a lock failure that is not the ordinary
/// contention case (`TryLockError::WouldBlock`, handled above and needing no
/// wording of its own).
///
/// Split out from [`lock_ledger`] so the `==`/`!=` choice at
/// `ErrorKind::Unsupported` is a two-line unit test rather than something
/// only provable by getting the real OS or filesystem into an unsupported
/// state -- which `lock_ledger` cannot be driven to in a test, but the
/// wording it picks can be, on a plain `std::io::Error` built from a kind.
fn lock_error_reason(e: &std::io::Error) -> String {
    if e.kind() == ErrorKind::Unsupported {
        // Not the ordinary contention case -- an OS or filesystem that
        // cannot lock at all. Named plainly rather than reading like the
        // same "someone else has it" case `WouldBlock` already covers.
        "file locking is not supported here".to_owned()
    } else {
        e.to_string()
    }
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
        // next save erased each other. This test writes through `ledger_path`,
        // the same production function `open()` calls (9-27-0026c finding 6:
        // the previous version rebuilt the path inline instead, so a change
        // that made `ledger_path` itself return the daemon's path would have
        // passed this test unnoticed). Re-apply that bug locally by making
        // `ledger_path` return `Paths::under(dir).ledger` and this fails: the
        // daemon's ledger would carry the CLI's reservation instead of
        // staying exactly as it started, untouched.
        let dir = std::env::temp_dir().join(format!("radar-cli-ledger-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let dir = dir.to_string_lossy().into_owned();

        let daemon_ledger = Paths::under(&dir).ledger;
        let _ = std::fs::remove_file(&daemon_ledger);
        std::fs::write(&daemon_ledger, "untouched").expect("seed the daemon's own file");

        let cli_ledger = super::ledger_path(&dir);
        let _ = std::fs::remove_file(&cli_ledger);
        assert_ne!(
            cli_ledger, daemon_ledger,
            "the CLI's ledger path must never be the daemon's own file"
        );

        // A budget built the same way `open()` builds one -- through
        // `cli_budget_from`, never a `Budget` literal named directly, which
        // would need a dependency this crate does not otherwise have.
        //
        // MONTHLY and FIXED are set here too (9-27-0026c finding 1): once
        // `cli_monthly_allowance_from` bounds the CLI's slice by what
        // `REALORRUG_MONTHLY_USD` leaves, a CLI slice configured with no
        // monthly ceiling at all correctly closes the budget -- this test's
        // budget must actually be funded, or it would exercise that closed
        // path instead of the ledger-isolation property it means to prove.
        let mut env = std::collections::HashMap::new();
        env.insert("REALORRUG_ANALYST_DAILY_USD".to_owned(), "1.0".to_owned());
        env.insert(
            "REALORRUG_ANALYST_PER_CALL_USD".to_owned(),
            "1.0".to_owned(),
        );
        env.insert("REALORRUG_MONTHLY_USD".to_owned(), "10.0".to_owned());
        env.insert("REALORRUG_FIXED_MONTHLY_USD".to_owned(), "0.0".to_owned());
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

    /// Every env var `open_at`'s two gates (`Prices::from_vars`,
    /// `cli_budget_from`) need to hand back a funded, open budget, so a test
    /// of what happens *after* those gates (findings 2 and 3) is not
    /// accidentally exercising a closed-budget or no-prices path instead.
    fn full_env() -> std::collections::HashMap<String, String> {
        let mut env = std::collections::HashMap::new();
        env.insert(
            "REALORRUG_X_PRICE_MENTION_READ".to_owned(),
            "1000".to_owned(),
        );
        env.insert("REALORRUG_X_PRICE_POST_READ".to_owned(), "5000".to_owned());
        env.insert("REALORRUG_X_PRICE_REPLY".to_owned(), "10000".to_owned());
        env.insert("REALORRUG_X_PRICE_POST".to_owned(), "15000".to_owned());
        env.insert(
            "REALORRUG_MODEL_PER_CALL_USD_MICRO".to_owned(),
            "2000".to_owned(),
        );
        env.insert("REALORRUG_X_PRICE_USER_READ".to_owned(), "20000".to_owned());
        env.insert("REALORRUG_ANALYST_DAILY_USD".to_owned(), "1.0".to_owned());
        env.insert(
            "REALORRUG_ANALYST_PER_CALL_USD".to_owned(),
            "1.0".to_owned(),
        );
        env.insert("REALORRUG_MONTHLY_USD".to_owned(), "10.0".to_owned());
        env.insert("REALORRUG_FIXED_MONTHLY_USD".to_owned(), "0.0".to_owned());
        env.insert("REALORRUG_CLI_MONTHLY_USD".to_owned(), "1.0".to_owned());
        env
    }

    #[test]
    fn an_unwritable_ledger_path_yields_no_paid_call() {
        // 9-27-0026c finding 2: before this fix, `open()` never called
        // `spend.save()` after `Spend::open` -- an unwritable ledger still
        // handed back a working `Spend`, and every run would restore from
        // nothing (the write it never managed the run before), spending as
        // if the monthly cap had just reset each time. Re-apply that bug
        // locally by deleting the `if let Err(e) = spend.save() { ... }`
        // block in `open_at`, leaving only `Some(Handle { spend, _lock:
        // lock })` right after `Spend::open`, and this test fails: it gets a
        // `Handle` back instead of `None`.
        //
        // The directory itself is fine -- `create_dir_all` and the lock (a
        // different filename, `cli-ledger.lock`) both succeed -- only the
        // ledger path is unwritable: a real directory sits where the ledger
        // file needs to go, so `Spend::save`'s `rename(temp, path)` fails
        // renaming a file onto an existing directory, on every platform this
        // workspace targets. That is more surgical than an unwritable parent
        // directory (also tried), which never reaches `save()` at all: it is
        // refused earlier, by `create_dir_all`, so removing the `save()`
        // check alone would not have made that version of this test fail.
        let dir = std::env::temp_dir().join(format!(
            "radar-cli-ledger-unwritable-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a usable dir");
        let dir = dir.to_string_lossy().into_owned();
        std::fs::create_dir_all(super::ledger_path(&dir))
            .expect("a directory sitting where the ledger file needs to go");

        let env = full_env();
        let get = |k: &str| env.get(k).cloned();

        assert!(
            super::open_at(&dir, &get).is_none(),
            "a ledger path that cannot be written must refuse to open, not silently restart the \
             ledger every run"
        );
    }

    #[test]
    fn a_second_open_while_the_first_is_still_running_is_refused() {
        // 9-27-0026c finding 3: `cli-ledger.json` is a plain file shared by
        // every hand-run command; with no lock, two started at once would
        // each load the same starting total, spend independently, and each
        // overwrite the other's save on exit. Re-apply that bug locally by
        // making `lock_ledger` skip the `try_lock` call entirely (return
        // `Some(file)` right after opening it), and this test fails: the
        // second `open_at` succeeds right alongside the first instead of
        // being refused.
        let dir =
            std::env::temp_dir().join(format!("radar-cli-ledger-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dir = dir.to_string_lossy().into_owned();

        let env = full_env();
        let get = |k: &str| env.get(k).cloned();

        let first = super::open_at(&dir, &get);
        assert!(first.is_some(), "the first open must succeed");

        let second = super::open_at(&dir, &get);
        assert!(
            second.is_none(),
            "a second open() while the first is still running must be refused, not race it for \
             the same ledger file"
        );

        drop(first);
        let third = super::open_at(&dir, &get);
        assert!(
            third.is_some(),
            "once the first run's lock is released, a new run may open"
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

    #[test]
    fn an_unsupported_lock_error_gets_a_plain_reason() {
        // CI's mutation report flagged "replace == with != in lock_ledger"
        // as MISSED: nothing here asserted which wording an unsupported
        // lock error gets. `lock_ledger` itself cannot be driven into this
        // branch in a test (it needs a real OS or filesystem that refuses to
        // lock at all), so the wording it picks is tested directly instead.
        let e = std::io::Error::from(super::ErrorKind::Unsupported);
        assert_eq!(
            super::lock_error_reason(&e),
            "file locking is not supported here"
        );
    }

    #[test]
    fn any_other_lock_error_keeps_its_own_message() {
        // The other side of the same boundary: a kind that is not
        // `Unsupported` must fall through to the error's own message rather
        // than the plain wording above -- which is exactly what `!=` in
        // place of `==` would do to *every* kind, `Unsupported` included.
        let kind = super::ErrorKind::PermissionDenied;
        assert_eq!(
            super::lock_error_reason(&std::io::Error::from(kind)),
            std::io::Error::from(kind).to_string()
        );
    }
}
