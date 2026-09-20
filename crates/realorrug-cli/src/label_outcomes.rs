// SPDX-License-Identifier: Apache-2.0
//! `realorrug label-outcomes` — reads the launches this analyst already judged
//! and writes down what each one turned out to be.
//!
//! The other half of the record. `realorrug-serve` writes a row every time it
//! publishes a verdict; that row is half a calibration sample, and it stays
//! half until somebody goes back and looks at the launch. This command is that
//! somebody. Without it the verdict table fills up forever and research 0052
//! §5's replay (task M-D-0006) never has a single pair to work with.
//!
//! **It judges nothing.** The label comes from
//! [`realorrug_onchain::outcome::label`], which reports which of three things
//! happened from facts on the chain; nothing here scores, weighs or ranks.
//!
//! # Why it is capped and why it waits
//!
//! Each label costs a whole dossier read, so `--max` bounds what one run
//! spends and the queue is served oldest first — a run that stopped at the cap
//! resumes where it left off instead of re-reading the same recent tokens.
//!
//! `--days` is the waiting period: a launch looked at an hour after it was
//! judged has not had time to become anything, and labelling it early would
//! fill the sample with `alive` rows that only mean "not yet".
//!
//! # What it declines to say
//!
//! Most runs will leave most tokens unlabelled, and that is the design
//! (AGENTS.md §3 rule 8). A launch this reader cannot settle — a graduated one,
//! or one whose curve could not be read — is skipped and stays in the queue,
//! because a guessed `alive` enters the fit as a *control* and teaches it that
//! the signals which fired meant nothing.

use std::time::{Duration, SystemTime};

use realorrug_onchain::memory::{Memory, Outcome};
use realorrug_onchain::{RpcClient, dispatch, outcome};

use crate::flag;

/// The chain the record is kept under. Only Robinhood Chain launches have a
/// bonding curve this labeller can read, so there is nothing to pass here yet.
const CHAIN: &str = "robinhood";

/// How long a launch is left alone after it was judged, in days.
const DEFAULT_WAIT_DAYS: u64 = 7;

/// How many launches one run will read. Each is a full dossier read.
const DEFAULT_MAX: u32 = 25;

/// Runs the command.
///
/// # Errors
///
/// A message when the memory file cannot be opened, when its queue cannot be
/// read, or when no chain endpoint was named. A single launch that fails to
/// read is reported and skipped, not fatal: one unreachable token must not
/// stop the rest of the batch.
pub fn run(args: &[String]) -> Result<(), String> {
    let path = flag(args, "--memory").unwrap_or_else(|| {
        let dir = std::env::var("REALORRUG_ANALYST_DIR")
            .or_else(|_| std::env::var("RADAR_ANALYST_DIR"))
            .unwrap_or_else(|_| "data/analyst".to_owned());
        format!("{dir}/memory.sqlite3")
    });
    let days = number(args, "--days").unwrap_or(DEFAULT_WAIT_DAYS);
    let max = u32::try_from(number(args, "--max").unwrap_or(u64::from(DEFAULT_MAX)))
        .unwrap_or(DEFAULT_MAX);
    let dry_run = has(args, "--dry-run");

    let memory = Memory::open(std::path::Path::new(&path))
        .map_err(|e| format!("cannot open {path}: {e}"))?;
    let settled_by = SystemTime::now()
        .checked_sub(Duration::from_secs(days.saturating_mul(86_400)))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let tokens = memory
        .tokens_awaiting_outcome(CHAIN, settled_by, max)
        .map_err(|e| format!("cannot read the queue: {e}"))?;
    if tokens.is_empty() {
        println!("nothing waiting: no judged launch is older than {days} days and unlabelled");
        return Ok(());
    }

    // No default endpoint (rule 7), same as `roast --robinhood-rpc`: an
    // operator who did not pass one has chosen "this chain cannot be read",
    // not "read it against an endpoint this command invented".
    let Some(url) = flag(args, "--robinhood-rpc") else {
        return Err(
            "--robinhood-rpc URL is required: labelling reads the chain, and there is \
                    no default endpoint to fall back on"
                .to_owned(),
        );
    };
    let robinhood = realorrug_robinhood::Rpc::new(&url);
    let solana = RpcClient::from_vars(&|k| std::env::var(k).ok());
    let market = realorrug_onchain::market::Http::default();
    let clients = dispatch::Clients {
        solana: &solana,
        robinhood: Some(&robinhood),
        market: Some(&market),
    };

    let mut tally = Tally::default();
    for token in &tokens {
        let read = dispatch::read(token, &clients);
        tally.absorb(&memory, token, read, dry_run)?;
    }

    println!("{}", tally.report(dry_run));
    Ok(())
}

/// What one run did, counted as it goes.
///
/// A struct rather than three loose counters so the counting can be tested
/// without a chain behind it: [`Tally::absorb`] takes a read that already
/// happened, which is the only part of the loop that needs a network.
#[derive(Default, Debug, PartialEq, Eq)]
struct Tally {
    /// Launches this run gave a label to.
    settled: u32,
    /// Launches it could not settle, which stay in the queue.
    skipped: u32,
    /// Launches it looked at, settled or not.
    read: usize,
}

impl Tally {
    /// Takes one launch's read and records what became of it.
    ///
    /// # Errors
    ///
    /// Only a failed *write*. A failed read is counted and moved past: one
    /// unreachable token must not end a batch that has others to do.
    fn absorb(
        &mut self,
        memory: &Memory,
        token: &str,
        read: Result<realorrug_onchain::Dossier, dispatch::Error>,
        dry_run: bool,
    ) -> Result<(), String> {
        self.read += 1;
        match read {
            Ok(dossier) => {
                if label_one(memory, token, &dossier, dry_run)? {
                    self.settled += 1;
                } else {
                    self.skipped += 1;
                }
            }
            Err(dispatch::Error::NotAnAddress) => {
                eprintln!("{token}: not a valid address, so it can never be labelled");
                self.skipped += 1;
            }
            Err(dispatch::Error::Unreadable(why)) => {
                eprintln!("{token}: unreadable ({why}); left in the queue");
                self.skipped += 1;
            }
        }
        Ok(())
    }

    /// The closing line. `--dry-run` says "would write" rather than "wrote",
    /// because an operator reading the two lines side by side has to be able
    /// to tell which run actually settled anything.
    fn report(&self, dry_run: bool) -> String {
        let written = if dry_run { "would write" } else { "wrote" };
        format!(
            "{written} {} outcome(s), left {} unsettled, out of {} read",
            self.settled, self.skipped, self.read
        )
    }
}

/// Labels one launch if this reading can settle it, returning whether it did.
///
/// A launch that cannot be settled is left in the queue on purpose: a
/// graduated one may become readable once research 0044 finds the pool's
/// address, and a failed read may succeed tomorrow.
fn label_one(
    memory: &Memory,
    token: &str,
    dossier: &realorrug_onchain::Dossier,
    dry_run: bool,
) -> Result<bool, String> {
    let Some(labelled) = outcome::label(dossier) else {
        println!("{token} unsettled: this reading cannot say what it became");
        return Ok(false);
    };
    println!("{token} {}: {}", labelled.label.as_str(), labelled.evidence);
    if dry_run {
        return Ok(true);
    }
    let at_block = dossier.read_at.and_then(|r| match r {
        realorrug_types::ReadAt::Robinhood(block) => Some(block),
        realorrug_types::ReadAt::Solana(_) => None,
    });
    memory
        .record_outcome(&Outcome {
            chain: CHAIN.to_owned(),
            token: token.to_owned(),
            label: labelled.label,
            at_block,
            evidence: labelled.evidence,
            observed_at: SystemTime::now(),
        })
        .map_err(|e| format!("cannot write the outcome for {token}: {e}"))?;
    Ok(true)
}

/// Whether a bare flag was passed.
fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

/// A whole-number flag, ignoring one that will not parse rather than inventing
/// a value for it.
fn number(args: &[String], name: &str) -> Option<u64> {
    flag(args, name).and_then(|v| v.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing flag falls back to the default; a present one is read. The
    /// parse failure returning `None` is what keeps `--max banana` from
    /// silently meaning zero, which would make a run that read nothing look
    /// like a run with nothing left to do.
    #[test]
    fn a_number_flag_is_read_or_left_to_its_default() {
        let args = vec![
            "label-outcomes".to_owned(),
            "--max".to_owned(),
            "3".to_owned(),
            "--days".to_owned(),
            "banana".to_owned(),
        ];
        assert_eq!(number(&args, "--max"), Some(3));
        assert_eq!(number(&args, "--days"), None);
        assert_eq!(number(&args, "--missing"), None);
    }

    /// Reading the chain costs money and time, so a run with work to do and no
    /// named endpoint refuses rather than picking one (rule 7).
    #[test]
    fn labelling_without_an_endpoint_refuses() {
        let dir = std::env::temp_dir().join("realorrug-label-refuses");
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("memory.sqlite3");
        let _ = std::fs::remove_file(&path);
        let memory = Memory::open(&path).expect("open");
        memory
            .record_verdict(&a_verdict("0x00000000000000000000000000000000000000aa"))
            .expect("record");
        drop(memory);

        let args = vec![
            "label-outcomes".to_owned(),
            "--memory".to_owned(),
            path.display().to_string(),
        ];
        let error = run(&args).expect_err("no endpoint");
        assert!(error.contains("--robinhood-rpc"), "{error}");
        let _ = std::fs::remove_file(&path);
    }

    /// An empty queue is an ordinary, successful run: nothing is old enough
    /// yet, and that is not a failure to report. It also never asks for an
    /// endpoint, because it has nothing to spend one on.
    #[test]
    fn an_empty_queue_is_not_an_error() {
        let dir = std::env::temp_dir().join("realorrug-label-empty");
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("memory.sqlite3");
        let _ = std::fs::remove_file(&path);
        drop(Memory::open(&path).expect("open"));
        let args = vec![
            "label-outcomes".to_owned(),
            "--memory".to_owned(),
            path.display().to_string(),
        ];
        run(&args).expect("an empty queue is fine");
        let _ = std::fs::remove_file(&path);
    }

    fn a_dossier(quote_reserves: u128) -> realorrug_onchain::Dossier {
        let mint: realorrug_types::ChainAddress = "0x00000000000000000000000000000000000000aa"
            .parse()
            .expect("an address");
        realorrug_onchain::Dossier {
            mint,
            read_at: Some(realorrug_types::ReadAt::Robinhood(4_242)),
            launch: None,
            curve: Some(realorrug_onchain::dossier::CurveFacts {
                complete: false,
                quote_reserves,
                quote_capacity: None,
                quote_asset: None,
                creator: mint,
                fees: None,
            }),
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

    fn memory_at(name: &str) -> (std::path::PathBuf, Memory) {
        let dir = std::env::temp_dir().join("realorrug-label-outcomes");
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join(format!("{name}.sqlite3"));
        let _ = std::fs::remove_file(&path);
        let memory = Memory::open(&path).expect("open");
        (path, memory)
    }

    fn a_verdict(token: &str) -> realorrug_onchain::memory::VerdictRecord {
        realorrug_onchain::memory::VerdictRecord {
            chain: CHAIN.to_owned(),
            token: token.to_owned(),
            read_at_block: Some(42),
            level: "Sketchy".to_owned(),
            score_bps: Some(2_520),
            fired: Vec::new(),
            source: "check".to_owned(),
            decided_at: SystemTime::UNIX_EPOCH,
        }
    }

    /// A settled launch is written down with the block it was observed at, so
    /// a label somebody disputes can be re-checked against that block rather
    /// than against whatever the chain looks like when they ask. Writing it
    /// also takes the token off the queue, which is what stops the next run
    /// spending another dossier read on it.
    #[test]
    fn a_settled_launch_is_recorded_with_its_evidence() {
        let (path, memory) = memory_at("settled");
        memory
            .record_verdict(&a_verdict("0xtoken"))
            .expect("record");
        assert_eq!(
            memory
                .tokens_awaiting_outcome(CHAIN, SystemTime::now(), 10)
                .expect("read"),
            vec!["0xtoken".to_owned()]
        );

        assert!(
            label_one(&memory, "0xtoken", &a_dossier(4_000_000), false).expect("label"),
            "a curve still holding money settles"
        );

        let pairs = memory.labelled_verdicts(CHAIN).expect("pairs");
        assert_eq!(pairs.len(), 1);
        assert_eq!(
            pairs[0].1.label,
            realorrug_onchain::memory::OutcomeLabel::Alive
        );
        assert_eq!(
            pairs[0].1.at_block,
            Some(4_242),
            "the block the outcome was read at, not the block the verdict was"
        );
        assert!(
            pairs[0].1.evidence.contains("4000000"),
            "the evidence names the reserves"
        );
        assert!(
            memory
                .tokens_awaiting_outcome(CHAIN, SystemTime::now(), 10)
                .expect("read")
                .is_empty(),
            "a labelled token is off the queue"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// `--dry-run` prints the label and writes nothing. The distinction is the
    /// whole reason the flag exists: an operator checking what a batch would
    /// say must not thereby settle it.
    #[test]
    fn a_dry_run_settles_nothing() {
        let (path, memory) = memory_at("dry-run");
        memory
            .record_verdict(&a_verdict("0xtoken"))
            .expect("record");
        assert!(
            label_one(&memory, "0xtoken", &a_dossier(4_000_000), true).expect("label"),
            "the label was reached"
        );
        assert!(
            memory.labelled_verdicts(CHAIN).expect("pairs").is_empty(),
            "a dry run wrote an outcome"
        );

        // The same call without the flag does write, and the pair appears.
        label_one(&memory, "0xtoken", &a_dossier(4_000_000), false).expect("label");
        assert_eq!(memory.labelled_verdicts(CHAIN).expect("pairs").len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// A bare flag is matched whole, not by prefix: `--dry-runner` is not
    /// `--dry-run`, and a run that wrote rows while the operator believed it
    /// was rehearsing is the worst version of this bug.
    #[test]
    fn a_bare_flag_is_matched_whole() {
        let args = vec!["label-outcomes".to_owned(), "--dry-run".to_owned()];
        assert!(has(&args, "--dry-run"));
        assert!(!has(&args, "--dry-runner"));
        assert!(!has(&["label-outcomes".to_owned()], "--dry-run"));
    }

    /// The counting, with the chain read already done: one settled, one it
    /// could not settle, one bad address and one unreadable token. Each lands
    /// in its own column, and every token read is counted once.
    #[test]
    fn every_read_lands_in_exactly_one_column() {
        let (path, memory) = memory_at("tally");
        let mut tally = Tally::default();

        tally
            .absorb(&memory, "0xalive", Ok(a_dossier(4_000_000)), false)
            .expect("absorb");
        let mut unsettleable = a_dossier(0);
        unsettleable.curve = None;
        tally
            .absorb(&memory, "0xunknown", Ok(unsettleable), false)
            .expect("absorb");
        tally
            .absorb(
                &memory,
                "nonsense",
                Err(dispatch::Error::NotAnAddress),
                false,
            )
            .expect("absorb");
        tally
            .absorb(
                &memory,
                "0xoffline",
                Err(dispatch::Error::Unreadable("no endpoint".to_owned())),
                false,
            )
            .expect("absorb");

        assert_eq!(
            tally,
            Tally {
                settled: 1,
                skipped: 3,
                read: 4,
            }
        );
        let _ = std::fs::remove_file(&path);
    }

    /// The closing line says what happened, and says something different when
    /// nothing was written.
    #[test]
    fn the_closing_line_distinguishes_a_rehearsal_from_a_run() {
        let tally = Tally {
            settled: 2,
            skipped: 1,
            read: 3,
        };
        assert_eq!(
            tally.report(false),
            "wrote 2 outcome(s), left 1 unsettled, out of 3 read"
        );
        assert_eq!(
            tally.report(true),
            "would write 2 outcome(s), left 1 unsettled, out of 3 read"
        );
    }
}
