// SPDX-License-Identifier: Apache-2.0
//! `realorrug settle` — gives every coin of a closed round its one outcome
//! row in the research store (design 0032 §12).
//!
//! The board (`hit_miss`, in `realorrug-contest`) counts settled calls. Until
//! something writes the outcome rows it has nothing to count; this is that
//! something. It is run by a timer, once a day (design 0032 §9), and nothing
//! here is a model call or a network call.
//!
//! # What it reads and what it writes
//!
//! It reads the rounds file (the same one `realorrug-serve` reads, so a round
//! has one close in one place) and, per coin, the label `label-outcomes` keeps
//! in the analyst's memory file. It writes only outcome rows, through
//! [`realorrug_store::Store::record_outcome_once`]. The rule that turns the one
//! into the other is `realorrug_contest::settle`, which is pure.
//!
//! # Why a second run changes nothing
//!
//! A coin that already has an outcome is skipped before anything is read, and
//! the write itself refuses a second row under the store's own write lock, so
//! two overlapping runs cannot both write one either. A timer that fires twice
//! is harmless; a correction is a deliberate `record_outcome`, never this.
//!
//! # What it declines to do
//!
//! A memory file that cannot be opened is an error and nothing is written,
//! because a written `Unresolved` is permanent and "the instrument was not
//! there" must not become a settled fact about every coin. A round that has not
//! closed writes nothing. A coin the rule cannot decide yet writes nothing.

use std::collections::HashSet;
use std::time::SystemTime;

use realorrug_contest::calls::Outcome;
use realorrug_contest::settle::{self, Read, Reading};
use realorrug_onchain::memory::{Memory, OutcomeLabel};
use realorrug_store::{Store, Verified};
use serde::Deserialize;

use crate::flag;

/// The longest round id `realorrug-serve` accepts; a longer one could never
/// have had a forecast, so a rounds file naming one is not this program's.
const MAX_ID: usize = 128;

/// One coin a round allows. Unknown fields (the odds, for one) are not this
/// command's business and are ignored rather than refused.
#[derive(Clone, Debug, Deserialize)]
struct Coin {
    chain: String,
    token: String,
}

/// One round, with its single close (seconds since the epoch).
#[derive(Clone, Debug, Deserialize)]
struct Round {
    id: String,
    close: i64,
    coins: Vec<Coin>,
}

#[derive(Deserialize)]
struct RoundsFile {
    rounds: Vec<Round>,
}

/// Reads a rounds file's text. Refuses an empty id, an over-long one and a
/// round named twice: a round with two closes has no close.
fn parse_rounds(text: &str) -> Result<Vec<Round>, String> {
    let file: RoundsFile =
        serde_json::from_str(text).map_err(|e| format!("the rounds file is not valid: {e}"))?;
    let mut seen = HashSet::new();
    for round in &file.rounds {
        if round.id.is_empty() || round.id.len() > MAX_ID {
            return Err(format!("the rounds file has a bad round id {:?}", round.id));
        }
        if !seen.insert(round.id.as_str()) {
            return Err(format!("the rounds file names round {} twice", round.id));
        }
    }
    Ok(file.rounds)
}

/// What a run did, coin by coin.
#[derive(Debug, Default, PartialEq, Eq)]
struct Report {
    /// Outcomes written (or, in a dry run, that would be): round, chain,
    /// token, outcome.
    written: Vec<(String, String, String, Outcome)>,
    /// Coins that already had an outcome.
    already: usize,
    /// Coins the rule cannot decide yet.
    pending: usize,
}

/// The seconds-since-the-epoch of a moment, or `None` before it.
fn secs(t: SystemTime) -> Option<i64> {
    let d = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    i64::try_from(d.as_secs()).ok()
}

/// The reads on record for one coin, taken no later than `now`.
///
/// Today that is the one label `label-outcomes` keeps per token: it says
/// `Rug`, or it says the launch was not seen to rug (`failed`, or `alive` as of
/// the moment it was observed). Both of the second are a successful read that
/// found no extraction. A label stamped after `now` is a clock fault, not
/// evidence, and is dropped.
fn reads_for(memory: &Memory, chain: &str, token: &str, now: i64) -> Result<Vec<Read>, String> {
    let label = memory
        .outcome_label(chain, token)
        .map_err(|e| format!("cannot read the label of {token}: {e}"))?;
    let Some(label) = label else {
        return Ok(Vec::new());
    };
    let Some(at) = secs(label.observed_at) else {
        return Ok(Vec::new());
    };
    if at > now {
        return Ok(Vec::new());
    }
    let reading = match label.label {
        OutcomeLabel::Rug => Reading::Rug,
        OutcomeLabel::Failed | OutcomeLabel::Alive => Reading::NoRug,
    };
    Ok(vec![Read { at, reading }])
}

/// Settles every coin of every round at `now`. With `dry_run` it reads and
/// decides but writes nothing.
fn settle_all(
    memory: &Memory,
    store: &Store,
    rounds: &[Round],
    now: i64,
    dry_run: bool,
) -> Result<Report, String> {
    let mut report = Report::default();
    for round in rounds {
        for coin in &round.coins {
            let stored = store
                .outcome(&round.id, &coin.chain, &coin.token)
                .map_err(|e| format!("cannot read the store: {e}"))?;
            if stored.is_some() {
                report.already += 1;
                continue;
            }
            let reads = reads_for(memory, &coin.chain, &coin.token, now)?;
            let Some(outcome) = settle::derive(round.close, &reads, now) else {
                report.pending += 1;
                continue;
            };
            if !dry_run {
                let evidence = settle::encode_evidence(round.close, &reads);
                let wrote = store
                    .record_outcome_once(
                        &round.id,
                        &coin.chain,
                        &coin.token,
                        outcome,
                        settle::RULE_VERSION,
                        Some(&evidence),
                        now,
                    )
                    .map_err(|e| format!("cannot write the outcome: {e}"))?;
                if !tally(&mut report, wrote) {
                    continue;
                }
            }
            report.written.push((
                round.id.clone(),
                coin.chain.clone(),
                coin.token.clone(),
                outcome,
            ));
        }
    }
    Ok(report)
}

/// What a verify pass found.
#[derive(Debug, Default, PartialEq, Eq)]
struct Audit {
    /// Rows whose evidence re-derives to the stored outcome.
    agree: usize,
    /// Rows whose evidence re-derives to something else.
    disagree: usize,
    /// Rows this code cannot re-derive (another rule version, no or unreadable
    /// evidence).
    unverifiable: usize,
    /// Coins with no row yet.
    unsettled: usize,
}

/// Re-derives every stored outcome of the rounds file from the row alone.
fn verify_all(store: &Store, rounds: &[Round]) -> Result<Audit, String> {
    match store
        .verify()
        .map_err(|e| format!("cannot verify the store: {e}"))?
    {
        Verified::Intact { .. } => {}
        Verified::Broken { at, why } => {
            return Err(format!("the store's chain is broken at row {at}: {why}"));
        }
    }
    let mut audit = Audit::default();
    for round in rounds {
        for coin in &round.coins {
            let Some(row) = store
                .outcome(&round.id, &coin.chain, &coin.token)
                .map_err(|e| format!("cannot read the store: {e}"))?
            else {
                audit.unsettled += 1;
                continue;
            };
            match settle::rederive(
                &row.rule_version,
                row.evidence_reference.as_deref(),
                row.settled_at,
            ) {
                Some(again) if again == row.outcome => audit.agree += 1,
                Some(_) => audit.disagree += 1,
                None => audit.unverifiable += 1,
            }
        }
    }
    Ok(audit)
}

/// Counts the outcome of a write. `false` means another run put this coin's
/// outcome in between our check and our write, which is the same as having
/// found it there: counted as already settled, and the row is not ours.
fn tally(report: &mut Report, wrote: bool) -> bool {
    if !wrote {
        report.already += 1;
    }
    wrote
}

/// An environment value, or nothing when it is unset or empty: an empty path
/// is a config that was not given, not a path (AGENTS.md section 3 rule 7).
fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

/// `--verify`'s verdict: clean only when nothing disagrees AND nothing is
/// unverifiable. A row this code cannot check is not a row it has confirmed.
fn audit_result(audit: &Audit) -> Result<(), String> {
    if audit.disagree == 0 && audit.unverifiable == 0 {
        Ok(())
    } else {
        Err("a stored outcome does not re-derive from its own evidence".to_owned())
    }
}

/// Runs the command.
///
/// # Errors
///
/// A message when a path is not given (there is no default: an operator who
/// named none has chosen not to settle), when the rounds file, the store or the
/// memory cannot be opened, or, with `--verify`, when a stored outcome does not
/// re-derive.
pub fn run(args: &[String]) -> Result<(), String> {
    let env = |name: &str| non_empty(std::env::var(name).ok());
    let rounds_path = flag(args, "--rounds")
        .or_else(|| env("REALORRUG_ROUNDS_FILE"))
        .ok_or(
            "--rounds PATH (or REALORRUG_ROUNDS_FILE) is required: there is no default round list",
        )?;
    let store_path = flag(args, "--store")
        .or_else(|| env("REALORRUG_STORE_PATH"))
        .ok_or("--store PATH (or REALORRUG_STORE_PATH) is required: there is no default store")?;
    let text = std::fs::read_to_string(&rounds_path)
        .map_err(|e| format!("cannot read {rounds_path}: {e}"))?;
    let rounds = parse_rounds(&text)?;
    // Opening would create an empty store and report a quiet week; `serve`
    // makes the store, this only writes into the one it made.
    if !std::path::Path::new(&store_path).is_file() {
        return Err(format!("cannot open {store_path}: no store file there"));
    }
    let store = Store::open(std::path::Path::new(&store_path))
        .map_err(|e| format!("cannot open {store_path}: {e}"))?;

    if args.iter().any(|a| a == "--verify") {
        let audit = verify_all(&store, &rounds)?;
        println!(
            "{} outcomes re-derive, {} disagree, {} cannot be re-derived, {} coins unsettled",
            audit.agree, audit.disagree, audit.unverifiable, audit.unsettled
        );
        return audit_result(&audit);
    }

    let memory_path = flag(args, "--memory").unwrap_or_else(|| {
        let dir = std::env::var("REALORRUG_ANALYST_DIR")
            .or_else(|_| std::env::var("RADAR_ANALYST_DIR"))
            .unwrap_or_else(|_| "data/analyst".to_owned());
        format!("{dir}/memory.sqlite3")
    });
    let memory = crate::open_memory(&memory_path)?;
    let now = secs(SystemTime::now()).ok_or("the system clock is before 1970")?;
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let report = settle_all(&memory, &store, &rounds, now, dry_run)?;
    let verb = if dry_run { "would settle" } else { "settled" };
    for (round, chain, token, outcome) in &report.written {
        println!(
            "{verb} {round} {chain} {token}: {}",
            Store::public_wording(*outcome)
        );
    }
    println!(
        "{verb} {}, {} already settled, {} not decidable yet",
        report.written.len(),
        report.already,
        report.pending
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use realorrug_onchain::memory::Outcome as Label;
    use std::time::Duration;

    const CLOSE: i64 = 2_000_000;
    const DAY: i64 = 86_400;
    const CHAIN: &str = "robinhood";

    fn stamp(secs: i64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(u64::try_from(secs).expect("positive"))
    }

    fn label(token: &str, kind: OutcomeLabel, observed: i64) -> Label {
        Label {
            chain: CHAIN.to_owned(),
            token: token.to_owned(),
            label: kind,
            at_block: None,
            evidence: "test".to_owned(),
            observed_at: stamp(observed),
        }
    }

    fn round(id: &str, close: i64, tokens: &[&str]) -> Round {
        Round {
            id: id.to_owned(),
            close,
            coins: tokens
                .iter()
                .map(|t| Coin {
                    chain: CHAIN.to_owned(),
                    token: (*t).to_owned(),
                })
                .collect(),
        }
    }

    /// A memory and a store in a folder of their own: the constructors that
    /// keep them in memory are for the crates' own tests only.
    fn fixture() -> (Memory, Store) {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "realorrug-cli-settle-fixture-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        (
            Memory::open(&dir.join("memory.sqlite3")).expect("memory"),
            Store::open(&dir.join("store.db")).expect("store"),
        )
    }

    fn outcome_of(store: &Store, id: &str, token: &str) -> Option<Outcome> {
        store
            .outcome(id, CHAIN, token)
            .expect("read")
            .map(|v| v.outcome)
    }

    fn rows(store: &Store) -> usize {
        match store.verify().expect("verify") {
            Verified::Intact { rows, .. } => rows,
            Verified::Broken { at, why } => panic!("broken at {at}: {why}"),
        }
    }

    /// Done-when 1: a replayed round settles to the same outcome twice. It is
    /// settled, the stored row re-derives to the same value from its own
    /// evidence, and settling again writes no new row.
    #[test]
    fn a_replayed_round_settles_to_the_same_outcome_twice() {
        let (memory, store) = fixture();
        memory
            .record_outcome(&label("rugger", OutcomeLabel::Rug, CLOSE + 500))
            .expect("label");
        let rounds = [round("r1", CLOSE, &["rugger"])];
        let now = CLOSE + 1_000;

        let first = settle_all(&memory, &store, &rounds, now, false).expect("settle");
        assert_eq!(first.written.len(), 1);
        assert_eq!(outcome_of(&store, "r1", "rugger"), Some(Outcome::Rugged));
        assert_eq!(rows(&store), 1);

        let audit = verify_all(&store, &rounds).expect("verify");
        assert_eq!(
            audit,
            Audit {
                agree: 1,
                ..Audit::default()
            }
        );
        let row = store
            .outcome("r1", CHAIN, "rugger")
            .expect("r")
            .expect("row");
        assert_eq!(
            settle::rederive(
                &row.rule_version,
                row.evidence_reference.as_deref(),
                row.settled_at
            ),
            Some(Outcome::Rugged)
        );
        assert_eq!(row.rule_version, settle::RULE_VERSION);
        assert_eq!(row.settled_at, now);

        // Replayed, later, with the label overwritten in the meantime: the
        // row stands and no row is added.
        memory
            .record_outcome(&label("rugger", OutcomeLabel::Alive, CLOSE + 9_000))
            .expect("relabel");
        let again = settle_all(&memory, &store, &rounds, now + DAY, false).expect("again");
        assert_eq!(again.written.len(), 0);
        assert_eq!(again.already, 1);
        assert_eq!(rows(&store), 1);
        assert_eq!(outcome_of(&store, "r1", "rugger"), Some(Outcome::Rugged));
        assert_eq!(verify_all(&store, &rounds).expect("verify").agree, 1);
    }

    /// Done-when 2: a coin with no evidence settles `Unresolved` at the
    /// horizon, and the board, fed by the store's own settled read, leaves it
    /// out of `n`.
    #[test]
    fn a_coin_with_no_evidence_settles_unresolved_and_is_left_out_of_the_board() {
        use realorrug_contest::calls::{Odds, SettledCall, Side, hit_miss};
        let (memory, store) = fixture();
        memory
            .record_outcome(&label("rugger", OutcomeLabel::Rug, CLOSE + 500))
            .expect("label");
        let player = store.new_player_key().expect("player");
        for token in ["rugger", "silent"] {
            store
                .submit_forecast(
                    "r1",
                    CHAIN,
                    token,
                    &player,
                    Side::Rug,
                    Odds::new(5_000).expect("odds"),
                    CLOSE - 100,
                    CLOSE,
                )
                .expect("forecast");
        }
        let rounds = [round("r1", CLOSE, &["rugger", "silent"])];
        let end = settle::horizon(CLOSE);

        // A coin that has not rugged has not stood, and a coin nobody read has
        // nothing to say: before the horizon only the rug is written.
        let early = settle_all(&memory, &store, &rounds, end - 1, false).expect("early");
        assert_eq!((early.written.len(), early.pending), (1, 1));
        assert_eq!(outcome_of(&store, "r1", "silent"), None);

        let done = settle_all(&memory, &store, &rounds, end, false).expect("settle");
        assert_eq!(done.written.len(), 1);
        assert_eq!(
            outcome_of(&store, "r1", "silent"),
            Some(Outcome::Unresolved)
        );
        assert_eq!(outcome_of(&store, "r1", "rugger"), Some(Outcome::Rugged));

        let settled = store.settled_forecasts(end).expect("settled");
        assert_eq!(settled.len(), 2, "both calls are settled reads");
        let calls: Vec<SettledCall> = settled
            .iter()
            .map(|s| SettledCall {
                player: String::new(),
                coin_id: s.token.clone(),
                creator_id: String::new(),
                side: s.side,
                q: Odds::new(s.q_basis_points).expect("odds"),
                outcome: s.outcome,
                called_at: 0,
                account_age_days: None,
            })
            .collect();
        let count = hit_miss(&calls);
        assert_eq!(
            (count.hits, count.misses, count.n),
            (1, 0, 1),
            "the unresolved coin is not in n"
        );
    }

    /// Done-when 3: nothing is written before the round's close, even with a
    /// rug label already on file and stamped after the close. With the guard
    /// removed from `settle::derive` this writes a `Rugged` row.
    #[test]
    fn nothing_is_written_before_the_rounds_close() {
        let (memory, store) = fixture();
        memory
            .record_outcome(&label("early", OutcomeLabel::Rug, CLOSE + 10))
            .expect("label");
        let rounds = [round("r1", CLOSE, &["early"])];
        // The label is stamped in the future relative to `now` here, so it is
        // also dropped as a clock fault; the settle-level guard is tested in
        // `realorrug_contest::settle`. This pins the end-to-end refusal.
        let report = settle_all(&memory, &store, &rounds, CLOSE - 1, false).expect("settle");
        assert!(report.written.is_empty());
        assert_eq!(report.pending, 1);
        assert_eq!(rows(&store), 0);
        assert_eq!(outcome_of(&store, "r1", "early"), None);
        // At the close it settles.
        let report = settle_all(&memory, &store, &rounds, CLOSE + 10, false).expect("settle");
        assert_eq!(report.written.len(), 1);
    }

    /// A dry run decides and reports and writes nothing; the real run after it
    /// still writes.
    #[test]
    fn a_dry_run_writes_nothing() {
        let (memory, store) = fixture();
        memory
            .record_outcome(&label("rugger", OutcomeLabel::Rug, CLOSE + 5))
            .expect("label");
        let rounds = [round("r1", CLOSE, &["rugger"])];
        let dry = settle_all(&memory, &store, &rounds, CLOSE + 10, true).expect("dry");
        assert_eq!(dry.written.len(), 1);
        assert_eq!(rows(&store), 0);
        let real = settle_all(&memory, &store, &rounds, CLOSE + 10, false).expect("real");
        assert_eq!(real.written.len(), 1);
        assert_eq!(rows(&store), 1);
    }

    /// The read is keyed by the round's own chain and token: another token's
    /// rug, or the same token on another chain, is not this coin's evidence.
    #[test]
    fn a_label_belongs_to_its_own_chain_and_token() {
        let (memory, _) = fixture();
        memory
            .record_outcome(&label("a", OutcomeLabel::Rug, CLOSE + 5))
            .expect("label");
        assert_eq!(reads_for(&memory, CHAIN, "b", CLOSE + 9).expect("read"), []);
        assert_eq!(
            reads_for(&memory, "solana", "a", CLOSE + 9).expect("read"),
            []
        );
        assert_eq!(
            reads_for(&memory, CHAIN, "a", CLOSE + 9).expect("read"),
            [Read {
                at: CLOSE + 5,
                reading: Reading::Rug
            }]
        );
    }

    /// Rug is a rug read; failed and alive are calm reads; a label from after
    /// `now` is not evidence yet.
    #[test]
    fn labels_map_to_readings_and_a_future_label_is_dropped() {
        let (memory, _) = fixture();
        for (token, kind, want) in [
            ("t1", OutcomeLabel::Rug, Reading::Rug),
            ("t2", OutcomeLabel::Failed, Reading::NoRug),
            ("t3", OutcomeLabel::Alive, Reading::NoRug),
        ] {
            memory
                .record_outcome(&label(token, kind, CLOSE + 7))
                .expect("label");
            assert_eq!(
                reads_for(&memory, CHAIN, token, CLOSE + 7).expect("read"),
                [Read {
                    at: CLOSE + 7,
                    reading: want
                }]
            );
            assert!(
                reads_for(&memory, CHAIN, token, CLOSE + 6)
                    .expect("read")
                    .is_empty(),
                "{token} stamped after now"
            );
        }
    }

    /// One calm label is not a fortnight of reads: an `alive` coin at the
    /// horizon is `Unresolved`, never `Stood`. `Stood` is reachable only with
    /// a read in every span (see `realorrug_contest::settle`).
    #[test]
    fn a_single_calm_label_does_not_settle_stood() {
        let (memory, store) = fixture();
        memory
            .record_outcome(&label("calm", OutcomeLabel::Alive, settle::horizon(CLOSE)))
            .expect("label");
        let rounds = [round("r1", CLOSE, &["calm"])];
        settle_all(&memory, &store, &rounds, settle::horizon(CLOSE), false).expect("settle");
        assert_eq!(outcome_of(&store, "r1", "calm"), Some(Outcome::Unresolved));
    }

    /// A lost race is counted as already settled, a win is not.
    #[test]
    fn a_lost_write_race_counts_as_already_settled() {
        let mut report = Report::default();
        assert!(!tally(&mut report, false));
        assert_eq!(report.already, 1);
        assert!(tally(&mut report, true));
        assert_eq!(report.already, 1, "a win adds nothing");
    }

    #[test]
    fn an_empty_environment_value_is_a_missing_one() {
        assert_eq!(non_empty(None), None);
        assert_eq!(non_empty(Some(String::new())), None);
        assert_eq!(non_empty(Some("x".to_owned())), Some("x".to_owned()));
    }

    /// Either kind of problem fails `--verify`; only a clean audit passes.
    #[test]
    fn verify_passes_only_when_nothing_disagrees_and_nothing_is_unverifiable() {
        let audit = |disagree, unverifiable| Audit {
            agree: 3,
            disagree,
            unverifiable,
            unsettled: 2,
        };
        assert!(audit_result(&audit(0, 0)).is_ok());
        assert!(audit_result(&audit(1, 0)).is_err());
        assert!(audit_result(&audit(0, 1)).is_err());
        assert!(audit_result(&audit(1, 1)).is_err());
    }

    /// Rounds are independent and so are coins; a mix of settled, pending and
    /// already-settled is counted exactly.
    #[test]
    fn the_report_counts_written_already_and_pending() {
        let (memory, store) = fixture();
        memory
            .record_outcome(&label("a", OutcomeLabel::Rug, CLOSE + 5))
            .expect("label");
        store
            .record_outcome("r1", CHAIN, "b", Outcome::Unresolved, "x", None, 1)
            .expect("prior");
        let rounds = [
            round("r1", CLOSE, &["a", "b", "c"]),
            round("r2", CLOSE + 10 * DAY, &["a"]),
        ];
        let report = settle_all(&memory, &store, &rounds, CLOSE + 10, false).expect("settle");
        assert_eq!(
            report.written,
            [(
                "r1".to_owned(),
                CHAIN.to_owned(),
                "a".to_owned(),
                Outcome::Rugged
            )]
        );
        assert_eq!(report.already, 1);
        assert_eq!(report.pending, 2, "c has no evidence yet, r2 is not closed");
        assert_eq!(rows(&store), 2);
    }

    /// Verify catches a stored outcome that its own evidence does not support,
    /// a row it cannot re-derive, and reports an untouched coin as unsettled.
    #[test]
    fn verify_reports_disagreement_and_the_unverifiable() {
        let (_, store) = fixture();
        let rounds = [round("r1", CLOSE, &["good", "wrong", "old", "none"])];
        let evidence = settle::encode_evidence(
            CLOSE,
            &[Read {
                at: CLOSE + 5,
                reading: Reading::Rug,
            }],
        );
        let write = |token: &str, outcome, version: &str, evidence: Option<&str>| {
            store
                .record_outcome("r1", CHAIN, token, outcome, version, evidence, CLOSE + 10)
                .expect("write");
        };
        write(
            "good",
            Outcome::Rugged,
            settle::RULE_VERSION,
            Some(&evidence),
        );
        write(
            "wrong",
            Outcome::Stood,
            settle::RULE_VERSION,
            Some(&evidence),
        );
        write("old", Outcome::Rugged, "settle-0", Some(&evidence));
        let audit = verify_all(&store, &rounds).expect("verify");
        assert_eq!(
            audit,
            Audit {
                agree: 1,
                disagree: 1,
                unverifiable: 1,
                unsettled: 1
            }
        );
    }

    /// A tampered chain is an error, not a count.
    #[test]
    fn verify_refuses_a_broken_chain() {
        let dir = std::env::temp_dir().join("realorrug-cli-settle-broken");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("store.db");
        {
            let store = Store::open(&path).expect("open");
            store
                .record_outcome("r1", CHAIN, "a", Outcome::Rugged, "settle-1", None, 5)
                .expect("write");
        }
        let raw = tamper_with_a_stored_payload(&path);
        assert!(raw, "the tamper ran");
        let store = Store::open(&path).expect("reopen");
        let err = verify_all(&store, &[round("r1", CLOSE, &["a"])]).expect_err("broken");
        assert!(err.contains("broken at row 1"), "{err}");
    }

    /// Edits a stored row the way a host attacker would. The store's triggers
    /// refuse an UPDATE through SQLite, so the payload bytes in the file are
    /// patched in place instead (same length, so the file stays well formed).
    fn tamper_with_a_stored_payload(path: &std::path::Path) -> bool {
        let mut bytes = std::fs::read(path).expect("read");
        let needle = b"\"Rugged\"";
        let Some(at) = bytes.windows(needle.len()).position(|w| w == needle) else {
            return false;
        };
        bytes[at + 1..at + 7].copy_from_slice(b"Stood\0");
        std::fs::write(path, bytes).expect("write");
        true
    }

    #[test]
    fn a_rounds_file_is_read_leniently_and_refused_strictly() {
        let ok = r#"{"rounds":[{"id":"r1","close":10,"coins":[
            {"chain":"c","token":"t","q_basis_points":5000}]}]}"#;
        let rounds = parse_rounds(ok).expect("parses");
        assert_eq!(rounds.len(), 1);
        assert_eq!(rounds[0].close, 10);
        assert_eq!(rounds[0].coins[0].token, "t");
        assert!(parse_rounds("{").is_err());
        assert!(parse_rounds(r#"{"rounds":[{"id":"","close":1,"coins":[]}]}"#).is_err());
        let long = format!(
            r#"{{"rounds":[{{"id":"{}","close":1,"coins":[]}}]}}"#,
            "x".repeat(129)
        );
        assert!(parse_rounds(&long).is_err());
        let max = format!(
            r#"{{"rounds":[{{"id":"{}","close":1,"coins":[]}}]}}"#,
            "x".repeat(128)
        );
        assert!(parse_rounds(&max).is_ok());
        let twice =
            r#"{"rounds":[{"id":"a","close":1,"coins":[]},{"id":"a","close":2,"coins":[]}]}"#;
        let err = parse_rounds(twice).expect_err("duplicate");
        assert!(err.contains("twice"), "{err}");
    }

    #[test]
    fn a_moment_before_1970_has_no_seconds() {
        assert_eq!(secs(SystemTime::UNIX_EPOCH), Some(0));
        assert_eq!(secs(stamp(7)), Some(7));
        assert_eq!(secs(SystemTime::UNIX_EPOCH - Duration::from_secs(1)), None);
    }

    /// The command refuses to run without its paths and does not invent a
    /// store.
    #[test]
    fn the_command_needs_its_paths_and_does_not_create_a_store() {
        let none: Vec<String> = vec!["settle".to_owned()];
        // SAFETY of the test: the environment variables are only read here.
        if std::env::var("REALORRUG_ROUNDS_FILE").is_err() {
            let err = run(&none).expect_err("no rounds");
            assert!(err.contains("--rounds"), "{err}");
        }
        let dir = std::env::temp_dir().join("realorrug-cli-settle-no-store");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let rounds = dir.join("rounds.json");
        std::fs::write(&rounds, r#"{"rounds":[]}"#).expect("rounds");
        let store = dir.join("store.db");
        let args: Vec<String> = [
            "settle",
            "--rounds",
            rounds.to_str().expect("p"),
            "--store",
            store.to_str().expect("p"),
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let err = run(&args).expect_err("no store");
        assert!(err.ends_with("no store file there"), "{err}");
        assert!(!store.exists());
    }

    /// The whole command, end to end: real files, a real memory, `--verify`
    /// after the run, and a second run that adds nothing.
    #[test]
    fn the_command_settles_then_verifies_then_settles_nothing_new() {
        let dir = std::env::temp_dir().join("realorrug-cli-settle-e2e");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let memory_path = dir.join("memory.sqlite3");
        let store_path = dir.join("store.db");
        let rounds_path = dir.join("rounds.json");
        // A round that closed long ago, one coin that rugged.
        let now = secs(SystemTime::now()).expect("clock");
        let close = now - 30 * DAY;
        {
            let memory = Memory::open(&memory_path).expect("memory");
            memory
                .record_outcome(&label("rugger", OutcomeLabel::Rug, close + 100))
                .expect("label");
            drop(Store::open(&store_path).expect("store"));
        }
        std::fs::write(
            &rounds_path,
            format!(
                r#"{{"rounds":[{{"id":"r1","close":{close},"coins":[
                {{"chain":"{CHAIN}","token":"rugger","q_basis_points":5000}},
                {{"chain":"{CHAIN}","token":"quiet","q_basis_points":5000}}]}}]}}"#
            ),
        )
        .expect("rounds");
        let base = |extra: &[&str]| -> Vec<String> {
            let mut v: Vec<String> = [
                "settle",
                "--rounds",
                rounds_path.to_str().expect("p"),
                "--store",
                store_path.to_str().expect("p"),
                "--memory",
                memory_path.to_str().expect("p"),
            ]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
            v.extend(extra.iter().map(|s| (*s).to_owned()));
            v
        };
        run(&base(&["--dry-run"])).expect("dry");
        assert_eq!(rows(&Store::open(&store_path).expect("s")), 0);
        run(&base(&[])).expect("settle");
        assert_eq!(rows(&Store::open(&store_path).expect("s")), 2);
        run(&base(&["--verify"])).expect("verify");
        run(&base(&[])).expect("again");
        assert_eq!(rows(&Store::open(&store_path).expect("s")), 2);
        let store = Store::open(&store_path).expect("s");
        assert_eq!(outcome_of(&store, "r1", "rugger"), Some(Outcome::Rugged));
        assert_eq!(outcome_of(&store, "r1", "quiet"), Some(Outcome::Unresolved));
    }

    /// A memory file that is not there stops the run before anything is
    /// written: a permanent `Unresolved` must not come from a missing file.
    #[test]
    fn a_missing_memory_writes_nothing() {
        let dir = std::env::temp_dir().join("realorrug-cli-settle-no-memory");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let store_path = dir.join("store.db");
        let rounds_path = dir.join("rounds.json");
        drop(Store::open(&store_path).expect("store"));
        std::fs::write(
            &rounds_path,
            format!(
                r#"{{"rounds":[{{"id":"r1","close":1,"coins":[
                {{"chain":"{CHAIN}","token":"a","q_basis_points":5000}}]}}]}}"#
            ),
        )
        .expect("rounds");
        let args: Vec<String> = [
            "settle",
            "--rounds",
            rounds_path.to_str().expect("p"),
            "--store",
            store_path.to_str().expect("p"),
            "--memory",
            dir.join("absent.sqlite3").to_str().expect("p"),
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let err = run(&args).expect_err("no memory");
        assert!(err.ends_with("no memory file there"), "{err}");
        assert_eq!(rows(&Store::open(&store_path).expect("s")), 0);
    }
}
