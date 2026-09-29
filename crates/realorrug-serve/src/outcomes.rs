// SPDX-License-Identifier: Apache-2.0
//! Ingest of the settlement job's published outcomes file (design 0032 §12).
//!
//! The settlement job (`realorrug settle`) never opens the research store:
//! this server is its one writer (ADR 0041 decision 6; design 0032 §3). The job
//! publishes a file of outcome rows; this module reads it and writes each row
//! that checks out through `Store::record_outcome_once`, the same single write
//! path every other row takes.
//!
//! # When it runs
//!
//! Once at start and then every [`INGEST_EVERY`], on the blocking pool. A poll
//! rather than a file watch: the file changes at most twice a day, a poll has
//! no platform-specific parts to get wrong, and a missed tick costs ten
//! minutes, not a row. Ingest is idempotent, so running it again is free.
//!
//! # Deny by default
//!
//! `REALORRUG_OUTCOMES_FILE` unset or empty means no task and no ingest. A file
//! that cannot be read or is not the published shape ingests nothing. The
//! rounds file (`REALORRUG_ROUNDS_FILE`) unreadable ingests nothing, since the
//! close a row is checked against comes from it.
//!
//! # What a row must be
//!
//! The file is trusted for its reads: they are the settlement job's own dated
//! chain reads, and this server cannot take them again. It is trusted for
//! nothing else. A row is written only when
//!
//! * its round is in the rounds file and its coin is one that round allows,
//! * the close its own evidence states is the close the rounds file states
//!   (a row settled against another close is another round's row),
//! * it was made under the published rule and re-derives to its own outcome
//!   from its own evidence (`OutcomeRow::rederives`, which also refuses a row
//!   settled before the close or before a read it cites),
//! * it was not settled in this server's future: `settled_at` no later than
//!   this server's clock plus [`CLOCK_SKEW_SECS`], so a row cannot cite reads
//!   that have not happened yet, and
//! * a `Stood` or `Unresolved` row is not accepted before the round's horizon
//!   has passed by this server's clock, whatever the row says.
//!
//! A row that fails is counted and skipped; the rest of the file is still
//! ingested, since rows are independent and one bad row must not hold back a
//! rug the board is waiting on.
//!
//! A row that names a different outcome from the one the store already holds
//! for the coin is not written over it (an outcome is permanent) and is not
//! counted as already held: it is counted as conflicting and logged with both
//! values, so a rule change or a hand edit is seen by a person and not
//! absorbed.

use std::sync::Arc;
use std::time::Duration;

use realorrug_contest::calls::Outcome;
use realorrug_contest::settle::{self, OutcomeRow};
use realorrug_store::Store;

use crate::auth::AuthState;
use crate::forecast::{Round, load_rounds};

/// How often the published file is read again.
pub const INGEST_EVERY: Duration = Duration::from_mins(10);

/// How far ahead of this server's clock a row's `settled_at` may be, in
/// seconds: enough for two machines' clocks to differ, not enough to settle a
/// window that has not ended.
pub const CLOCK_SKEW_SECS: i64 = 300;

/// What one ingest did.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Report {
    /// Rows written now.
    pub(crate) written: usize,
    /// Valid rows the store already held.
    pub(crate) already: usize,
    /// Rows that failed a check and were skipped.
    pub(crate) refused: usize,
    /// Valid rows that name a different outcome from the one the store holds
    /// for the coin. The held one stays; each is logged with both values.
    pub(crate) conflicting: usize,
}

/// The log line for one ingest, or nothing when it did nothing (an idle poll
/// every ten minutes would drown the rest of the log).
fn describe(result: &Result<Report, String>) -> Option<String> {
    match result {
        Ok(report) if *report == Report::default() => None,
        Ok(report) => Some(format!(
            "realorrug-serve: outcomes ingested: {} written, {} already held, {} refused, {} conflicting",
            report.written, report.already, report.refused, report.conflicting
        )),
        Err(why) => Some(format!("realorrug-serve: outcomes not ingested: {why}")),
    }
}

/// Whether the rounds file, the row's own evidence and this server's clock
/// (`now`) vouch for a row.
fn vetted(row: &OutcomeRow, rounds: &[Round], now: i64) -> bool {
    let Some(round) = rounds.iter().find(|r| r.id == row.round) else {
        return false;
    };
    // `Rugged` can be decided inside the window; the other two only once the
    // window is over. A row that says otherwise is checked against this
    // server's clock, not against a clock the row wrote.
    let window_over = now >= settle::horizon(round.close);
    round
        .coins
        .iter()
        .any(|c| c.chain == row.chain && c.token == row.token)
        && row.close() == Some(round.close)
        && row.settled_at <= now.saturating_add(CLOCK_SKEW_SECS)
        && (row.outcome == Outcome::Rugged || window_over)
        && row.rederives()
}

/// Ingests the file's text; `now` is this server's clock.
///
/// # Errors
///
/// A message when the text is not the published shape, or when the store
/// refuses a write (the rows written before it stay written).
pub(crate) fn ingest_text(
    text: &str,
    rounds: &[Round],
    store: &Store,
    now: i64,
) -> Result<Report, String> {
    let file = settle::parse_outcomes(text)?;
    let mut report = Report::default();
    for row in &file.outcomes {
        if !vetted(row, rounds, now) {
            report.refused += 1;
            continue;
        }
        let held = store
            .outcome(&row.round, &row.chain, &row.token)
            .map_err(|_| "the store refused an outcome read".to_owned())?;
        if let Some(held) = held
            && held.outcome != row.outcome
        {
            report.conflicting += 1;
            // The coin keeps the first outcome. Naming both values is what
            // lets a person see a rule change or a hand-edited file.
            eprintln!(
                "realorrug-serve: warning: outcomes file disagrees with the store for round {} coin {}: the store holds {:?}, the file says {:?}; the store's is kept",
                row.round, row.token, held.outcome, row.outcome
            );
            continue;
        }
        let wrote = store
            .record_outcome_once(
                &row.round,
                &row.chain,
                &row.token,
                row.outcome,
                &row.rule_version,
                Some(&row.evidence_reference),
                row.settled_at,
            )
            .map_err(|_| "the store refused an outcome write".to_owned())?;
        if wrote {
            report.written += 1;
        } else {
            report.already += 1;
        }
    }
    Ok(report)
}

impl AuthState {
    /// Reads the configured outcomes file and writes what it holds. `None`
    /// when nothing is configured or nothing could be read: not a failure,
    /// since an unconfigured server ingests nothing by design.
    pub(crate) fn ingest_outcomes(&self) -> Option<Result<Report, String>> {
        let path = self.outcomes_path.as_ref()?;
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            // Not published yet is the normal state before the first run.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(_) => return Some(Err("the outcomes file cannot be read".to_owned())),
        };
        let Some(rounds) = load_rounds(self) else {
            return Some(Err("the rounds file cannot be read".to_owned()));
        };
        Some(
            self.with_store(|store| Ok(ingest_text(&text, &rounds, store, (self.clock)())))
                .unwrap_or_else(|_| Err("the store is unavailable".to_owned())),
        )
    }
}

/// Runs the periodic ingest. Built by [`crate::app_with_ingest`].
pub struct Ingest(pub(crate) Arc<AuthState>);

impl Ingest {
    /// Whether an outcomes file is configured. The caller starts no task when
    /// it is not.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.0.outcomes_path.is_some()
    }

    /// Ingests now and every [`INGEST_EVERY`], for ever.
    pub async fn run(self) {
        loop {
            let state = Arc::clone(&self.0);
            // On the blocking pool: the store's mutex and the file read must
            // not sit on an async worker.
            if let Ok(Some(result)) =
                tokio::task::spawn_blocking(move || state.ingest_outcomes()).await
                && let Some(line) = describe(&result)
            {
                if result.is_err() {
                    eprintln!("{line}");
                } else {
                    println!("{line}");
                }
            }
            tokio::time::sleep(INGEST_EVERY).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use realorrug_contest::calls::Outcome;
    use realorrug_contest::settle::{OutcomesFile, Read, Reading, encode_evidence, horizon};

    use super::*;

    const CLOSE: i64 = 1_800_001_000;
    const DAY: i64 = 86_400;
    const ROUNDS: &str = r#"{"rounds":[{"id":"r1","close":1800001000,"coins":[
        {"chain":"solana","token":"coinA","q_basis_points":6000},
        {"chain":"solana","token":"coinB","q_basis_points":4000}]}]}"#;

    /// Long after every window here has ended: the clock the server has unless
    /// a test sets another.
    const LATER: i64 = CLOSE + 30 * DAY;

    /// A rug that began after the close: a calm read, then two rug reads, the
    /// last at `CLOSE + DAY + 100`, settled at once.
    fn rugged(token: &str) -> OutcomeRow {
        let reads = [
            Read {
                at: CLOSE + 60,
                reading: Reading::NoRug,
            },
            Read {
                at: CLOSE + DAY,
                reading: Reading::Rug,
            },
            Read {
                at: CLOSE + DAY + 100,
                reading: Reading::Rug,
            },
        ];
        OutcomeRow {
            round: "r1".to_owned(),
            chain: "solana".to_owned(),
            token: token.to_owned(),
            outcome: Outcome::Rugged,
            rule_version: settle::RULE_VERSION.to_owned(),
            evidence_reference: encode_evidence(CLOSE, &reads),
            settled_at: CLOSE + DAY + 105,
        }
    }

    /// Fourteen calm daily reads and one at the horizon, settled the moment
    /// after.
    fn stood(token: &str) -> OutcomeRow {
        stood_at(token, horizon(CLOSE) + 3_600)
    }

    /// The same, with the horizon read (and the settling) at `at`.
    fn stood_at(token: &str, at: i64) -> OutcomeRow {
        let mut reads: Vec<Read> = (0..14)
            .map(|d| Read {
                at: CLOSE + d * DAY + 60,
                reading: Reading::NoRug,
            })
            .collect();
        reads.push(Read {
            at,
            reading: Reading::NoRug,
        });
        OutcomeRow {
            outcome: Outcome::Stood,
            evidence_reference: encode_evidence(CLOSE, &reads),
            settled_at: at,
            ..rugged(token)
        }
    }

    struct H {
        dir: tempfile::TempDir,
        state: AuthState,
    }

    impl H {
        fn new(rounds: Option<&str>, outcomes: Option<&OutcomesFile>) -> Self {
            Self::at(rounds, outcomes, LATER)
        }

        /// A harness whose server clock reads `now`.
        fn at(rounds: Option<&str>, outcomes: Option<&OutcomesFile>, now: i64) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut vars: HashMap<&str, String> = HashMap::from([
                (
                    "REALORRUG_STORE_PATH",
                    dir.path().join("store.db").to_string_lossy().into_owned(),
                ),
                ("REALORRUG_X_CLIENT_ID", "test-client".to_owned()),
                (
                    "REALORRUG_X_REDIRECT_URI",
                    "https://api.test/auth/x/callback".to_owned(),
                ),
            ]);
            if let Some(text) = rounds {
                let path = dir.path().join("rounds.json");
                std::fs::write(&path, text).unwrap();
                vars.insert("REALORRUG_ROUNDS_FILE", path.to_string_lossy().into_owned());
            }
            if let Some(file) = outcomes {
                let path = dir.path().join("outcomes.json");
                std::fs::write(&path, settle::render_outcomes(file).unwrap()).unwrap();
                vars.insert(
                    "REALORRUG_OUTCOMES_FILE",
                    path.to_string_lossy().into_owned(),
                );
            }
            let state =
                AuthState::from_vars(&|k| vars.get(k).cloned(), None, Arc::new(move || now));
            Self { dir, state }
        }

        fn outcome(&self, token: &str) -> Option<Outcome> {
            self.state
                .with_store(|s| s.outcome("r1", "solana", token))
                .ok()
                .flatten()
                .map(|v| v.outcome)
        }

        fn rows(&self) -> usize {
            match self.state.with_store(realorrug_store::Store::verify) {
                Ok(realorrug_store::Verified::Intact { rows, .. }) => rows,
                _ => panic!("the chain is not intact"),
            }
        }

        fn outcomes_path(&self) -> std::path::PathBuf {
            self.dir.path().join("outcomes.json")
        }
    }

    fn file(rows: Vec<OutcomeRow>) -> OutcomesFile {
        OutcomesFile { outcomes: rows }
    }

    /// An idle poll logs nothing; work and failure are each said once.
    #[test]
    fn only_a_poll_that_did_something_is_logged() {
        assert_eq!(describe(&Ok(Report::default())), None);
        let some = describe(&Ok(Report {
            written: 2,
            already: 1,
            refused: 3,
            conflicting: 4,
        }))
        .expect("a line");
        assert!(
            some.contains("2 written, 1 already held, 3 refused, 4 conflicting"),
            "{some}"
        );
        let idle_but_held = describe(&Ok(Report {
            written: 0,
            already: 1,
            refused: 0,
            conflicting: 0,
        }));
        assert!(idle_but_held.is_some());
        let failed = describe(&Err("no such file".to_owned())).expect("a line");
        assert!(failed.contains("not ingested: no such file"), "{failed}");
    }

    /// The same file ingested twice writes its rows once.
    #[test]
    fn ingesting_the_same_file_twice_writes_once() {
        let h = H::new(Some(ROUNDS), Some(&file(vec![rugged("coinA")])));
        let first = h.state.ingest_outcomes().unwrap().unwrap();
        assert_eq!(
            first,
            Report {
                written: 1,
                already: 0,
                refused: 0,
                conflicting: 0
            }
        );
        assert_eq!(h.outcome("coinA"), Some(Outcome::Rugged));
        let rows = h.rows();
        let second = h.state.ingest_outcomes().unwrap().unwrap();
        assert_eq!(
            second,
            Report {
                written: 0,
                already: 1,
                refused: 0,
                conflicting: 0
            }
        );
        assert_eq!(h.rows(), rows, "the chain did not grow");
    }

    /// A row already in the file is not re-decided when the file later grows.
    #[test]
    fn a_grown_file_writes_only_the_new_rows() {
        let h = H::new(Some(ROUNDS), Some(&file(vec![rugged("coinA")])));
        h.state.ingest_outcomes().unwrap().unwrap();
        let grown = file(vec![rugged("coinA"), stood("coinB")]);
        std::fs::write(h.outcomes_path(), settle::render_outcomes(&grown).unwrap()).unwrap();
        let report = h.state.ingest_outcomes().unwrap().unwrap();
        assert_eq!((report.written, report.already), (1, 1));
        assert_eq!(h.outcome("coinB"), Some(Outcome::Stood));
    }

    /// No file configured ingests nothing, and the task is not started.
    #[test]
    fn no_outcomes_file_configured_ingests_nothing() {
        let h = H::new(Some(ROUNDS), None);
        assert!(h.state.ingest_outcomes().is_none());
        assert_eq!(h.outcome("coinA"), None);
        assert!(!Ingest(Arc::new(h.state)).enabled());
    }

    /// A configured file that is not there yet is not an error and not a row.
    #[test]
    fn a_file_not_published_yet_ingests_nothing() {
        let h = H::new(Some(ROUNDS), Some(&file(vec![])));
        std::fs::remove_file(h.outcomes_path()).unwrap();
        assert!(h.state.ingest_outcomes().is_none());
    }

    #[test]
    fn a_configured_file_enables_the_task() {
        let h = H::new(Some(ROUNDS), Some(&file(vec![])));
        assert!(Ingest(Arc::new(h.state)).enabled());
    }

    /// Without the rounds file there is no close to check a row against, so
    /// nothing is written.
    #[test]
    fn no_rounds_file_ingests_nothing() {
        let h = H::new(None, Some(&file(vec![rugged("coinA")])));
        let result = h.state.ingest_outcomes().unwrap();
        assert!(result.is_err());
        assert_eq!(h.outcome("coinA"), None);
    }

    /// A file that is not the published shape writes nothing.
    #[test]
    fn a_file_of_another_shape_ingests_nothing() {
        let h = H::new(Some(ROUNDS), Some(&file(vec![])));
        std::fs::write(h.outcomes_path(), r#"{"outcomes":[{"round":"r1"}]}"#).unwrap();
        assert!(h.state.ingest_outcomes().unwrap().is_err());
        std::fs::write(h.outcomes_path(), "not json").unwrap();
        assert!(h.state.ingest_outcomes().unwrap().is_err());
        assert_eq!(h.rows(), 0);
    }

    fn refused(row: &OutcomeRow) -> bool {
        let h = H::new(Some(ROUNDS), Some(&file(vec![row.clone()])));
        let report = h.state.ingest_outcomes().unwrap().unwrap();
        let none_written = h.outcome(&row.token).is_none();
        report.refused == 1 && report.written == 0 && none_written
    }

    /// Each check a row must pass, failed one at a time. The last case is the
    /// control: the same row unchanged is accepted.
    #[test]
    fn a_row_the_rounds_file_does_not_vouch_for_is_refused() {
        let mut other_round = rugged("coinA");
        other_round.round = "r2".to_owned();
        assert!(refused(&other_round));

        assert!(refused(&rugged("coinZ")), "a coin the round does not allow");

        let mut other_chain = rugged("coinA");
        other_chain.chain = "robinhood".to_owned();
        assert!(refused(&other_chain));

        // Evidence built against another close.
        let reads = [Read {
            at: CLOSE + 2 * DAY,
            reading: Reading::Rug,
        }];
        let mut wrong_close = rugged("coinA");
        wrong_close.evidence_reference = encode_evidence(CLOSE + DAY, &reads);
        wrong_close.settled_at = CLOSE + 3 * DAY;
        assert!(refused(&wrong_close));

        // Settled before the close.
        let mut early = rugged("coinA");
        early.settled_at = CLOSE - 1;
        assert!(refused(&early));

        // Another rule's row.
        let mut other_rule = rugged("coinA");
        other_rule.rule_version = "settle-0".to_owned();
        assert!(refused(&other_rule));

        // Says Stood, on evidence of a rug.
        let mut forged = rugged("coinA");
        forged.outcome = Outcome::Stood;
        assert!(refused(&forged));

        // Says Stood before the horizon ended.
        let mut hasty = stood("coinA");
        hasty.settled_at = horizon(CLOSE) - 1;
        assert!(refused(&hasty));

        // The controls: the unchanged rows are accepted.
        let h = H::new(
            Some(ROUNDS),
            Some(&file(vec![rugged("coinA"), stood("coinB")])),
        );
        let report = h.state.ingest_outcomes().unwrap().unwrap();
        assert_eq!(report.written, 2);
        assert_eq!(report.refused, 0);
    }

    /// One bad row does not hold back the others.
    #[test]
    fn a_bad_row_does_not_hold_back_a_good_one() {
        let mut bad = rugged("coinA");
        bad.outcome = Outcome::Stood;
        let h = H::new(Some(ROUNDS), Some(&file(vec![bad, rugged("coinB")])));
        let report = h.state.ingest_outcomes().unwrap().unwrap();
        assert_eq!((report.written, report.refused), (1, 1));
        assert_eq!(h.outcome("coinA"), None);
        assert_eq!(h.outcome("coinB"), Some(Outcome::Rugged));
    }

    /// The store has one outcome per coin: a second, different row for a coin
    /// already settled is not written over it.
    #[test]
    fn a_coin_already_settled_keeps_its_first_outcome() {
        let mut later = rugged("coinA");
        later.settled_at += 10;
        let h = H::new(Some(ROUNDS), Some(&file(vec![rugged("coinA"), later])));
        let report = h.state.ingest_outcomes().unwrap().unwrap();
        assert_eq!((report.written, report.already), (1, 1));
    }

    /// A row cannot be settled in this server's future: the clock the row
    /// wrote is not the clock that decides. The edge is `CLOCK_SKEW_SECS`.
    #[test]
    fn a_row_settled_in_the_servers_future_is_refused() {
        let row = rugged("coinA");
        let at = |now: i64| {
            let h = H::at(Some(ROUNDS), Some(&file(vec![row.clone()])), now);
            let report = h.state.ingest_outcomes().unwrap().unwrap();
            (report.written, report.refused)
        };
        assert_eq!(at(row.settled_at - CLOCK_SKEW_SECS), (1, 0));
        assert_eq!(at(row.settled_at - CLOCK_SKEW_SECS - 1), (0, 1));
        assert_eq!(at(CLOSE), (0, 1));
    }

    /// `Stood` and `Unresolved` wait for the horizon by this server's clock,
    /// whatever the row says. The row here is self-consistent (its horizon read
    /// is at the horizon itself), so only the server's clock refuses it.
    #[test]
    fn a_stood_row_before_the_servers_horizon_is_refused() {
        let end = horizon(CLOSE);
        let row = stood_at("coinA", end);
        let at = |now: i64| {
            let h = H::at(Some(ROUNDS), Some(&file(vec![row.clone()])), now);
            let report = h.state.ingest_outcomes().unwrap().unwrap();
            (report.written, report.refused)
        };
        assert_eq!(at(end), (1, 0), "the horizon itself is the window's end");
        assert_eq!(at(end - 1), (0, 1), "one second early");
        // A rug is not held to the horizon: it is decided when it is seen.
        let rug = rugged("coinB");
        let h = H::at(Some(ROUNDS), Some(&file(vec![rug.clone()])), rug.settled_at);
        assert_eq!(h.state.ingest_outcomes().unwrap().unwrap().written, 1);
    }

    /// A row that names another outcome than the one the store holds is not
    /// written, not counted as already held, and counted as conflicting; the
    /// first outcome stands.
    #[test]
    fn a_conflicting_second_outcome_is_counted_and_the_first_is_kept() {
        let h = H::new(Some(ROUNDS), Some(&file(vec![rugged("coinA")])));
        assert_eq!(h.state.ingest_outcomes().unwrap().unwrap().written, 1);
        let rows = h.rows();
        let mut stood_instead = stood("coinA");
        // The same coin, on evidence that is valid by itself.
        stood_instead.round = "r1".to_owned();
        std::fs::write(
            h.outcomes_path(),
            settle::render_outcomes(&file(vec![stood_instead])).unwrap(),
        )
        .unwrap();
        let report = h.state.ingest_outcomes().unwrap().unwrap();
        assert_eq!(
            report,
            Report {
                written: 0,
                already: 0,
                refused: 0,
                conflicting: 1
            }
        );
        assert_eq!(h.outcome("coinA"), Some(Outcome::Rugged));
        assert_eq!(h.rows(), rows, "the chain did not grow");
        let line = describe(&Ok(report)).expect("a conflict is logged");
        assert!(line.contains("1 conflicting"), "{line}");
    }

    /// The published file's directory need not exist for an unconfigured
    /// server; a configured path that is a directory is an error, not a row.
    #[test]
    fn an_unreadable_path_is_an_error_not_a_row() {
        let h = H::new(Some(ROUNDS), Some(&file(vec![])));
        let path = h.outcomes_path();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(Path::new(&path)).unwrap();
        assert!(h.state.ingest_outcomes().unwrap().is_err());
    }
}
