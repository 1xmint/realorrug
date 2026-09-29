// SPDX-License-Identifier: Apache-2.0
//! `realorrug settle` — the settlement job (design 0032 §12).
//!
//! The board (`hit_miss`, in `realorrug-contest`) counts settled calls. This
//! job produces the outcomes it counts, in three steps, and never opens the
//! research store: `realorrug-serve` is the store's one writer (ADR 0041
//! decision 6; design 0032 §3), so this job's product is a file.
//!
//! * `settle observe` reads the chain. For each coin of a closed round whose
//!   14-day window is still open, it builds the coin's fact sheet with the
//!   readers `roast` and `capture` use, under the same per-read budget, takes
//!   the code-computed level, and appends one dated line to the observations
//!   file. **No model is called**; the level is the code's own
//!   (`realorrug_roast::level`).
//! * `settle publish` derives outcomes from those dated lines with the
//!   published rule (`realorrug_contest::settle`) and writes them to the
//!   outcomes file, atomically. Rows already in the file are kept as they are.
//! * `settle verify` re-derives every row of the outcomes file from the row
//!   alone.
//!
//! `realorrug-serve` ingests the outcomes file (design 0032 §12), checks each
//! row again, and writes it through `Store::record_outcome_once`.
//!
//! # Why the evidence is this job's own reads
//!
//! The analyst memory keeps one overwritten, untimed label per token. It can
//! say a token rugged at some point; it cannot say whether that was inside the
//! window, and it can never say a token stood on a given day. A dated read
//! history can, so the observations file is the only evidence the rule sees.
//!
//! # What a read is worth
//!
//! A `Rugged` level is a rug read (the rule then wants a second one, design
//! 0032 §12). Any other level counts as "no rug seen" only when the read was
//! complete (not `CantTell`, the ladder saying a fact it needed could not be
//! read) **and** the instrument could have shown a rug at all
//! (`realorrug_roast::rug_detectable`, recorded on the line). Today it cannot:
//! nothing builds the signals `Rugged` needs, so every calm read is recorded
//! and none is evidence, and every coin ends `Unresolved` (AGENTS.md section 3
//! rule 8: unknown is not safe; section 1: zero is a measurement about your
//! instrument). An incomplete read is recorded for the audit and never covers
//! a 24-hour span.
//!
//! Each line is written in one write with its newline, under a lock, so two
//! runs cannot interleave and a kill leaves at worst an unterminated tail,
//! which is cut off before the next run appends and is never evidence.
//!
//! # Cost, and why the timer may fire twice a day
//!
//! [`realorrug_contest::settle::due`] owes a coin a read only while its current
//! 24-hour span has none, so the reads stay at one per coin per day however
//! often the timer fires. Firing twice a day is for resilience: a late or
//! missed run cannot leave a span empty, which would turn a `Stood` into an
//! `Unresolved`. A per-run ceiling ([`MAX_READS_PER_RUN`]) bounds a run
//! whatever the rounds file says.
//!
//! # Deny by default
//!
//! No RPC configured (`--rpc` or `REALORRUG_RPC`) means `observe` reads
//! nothing and says so; the public endpoint is never a fallback here. The RPC
//! address is never printed, and neither is a reader's error text, which could
//! carry it.

use std::collections::HashSet;
use std::io::Write;
use std::time::SystemTime;

use realorrug_contest::settle::{self, OutcomeRow, OutcomesFile, Read, Reading};
use realorrug_onchain::{RpcClient, dispatch};
use realorrug_roast::{BaseRates, FactSheet, Level};
use serde::{Deserialize, Serialize};

use crate::flag;

/// The longest round id `realorrug-serve` accepts; a longer one could never
/// have had a forecast, so a rounds file naming one is not this program's.
const MAX_ID: usize = 128;

/// The most chain reads one `observe` run makes. A run that reaches it stops
/// and says so; the coins it did not reach are still owed and are read at the
/// next run. Five coins a day for fourteen days is seventy coins in flight, so
/// this leaves room for a retry of every one.
const MAX_READS_PER_RUN: usize = 150;

const USAGE: &str = "usage: realorrug settle observe|publish|verify [--rounds PATH] \
[--observations PATH] [--outcomes PATH] [--rpc URL] [--robinhood-rpc URL] [--dry-run]";

/// One coin a round allows. Unknown fields (the odds, for one) are not this
/// command's business and are ignored rather than refused.
#[derive(Clone, Debug, Deserialize)]
struct Coin {
    chain: String,
    token: String,
}

/// One round, with its single close (seconds since the epoch): the end of the
/// entry window, from which the 14-day settlement window runs.
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

/// One dated chain read of one coin: a line of the observations file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    round: String,
    chain: String,
    token: String,
    /// When the read was taken (seconds since the epoch).
    at: i64,
    /// The code-computed level of the fact sheet this read built.
    level: Level,
    /// Whether every fact the ladder needed was read.
    complete: bool,
    /// Whether a sheet built by the code that took this read could have shown
    /// a rug at all (`realorrug_roast::rug_detectable`, measured when the read
    /// was taken). A calm level from an instrument that cannot fire says
    /// nothing about whether a rug happened. A line written before this field
    /// existed reads as `false`: it was taken when nothing could be told.
    #[serde(default)]
    rug_detectable: bool,
    /// The rule the reading is meant for ([`settle::RULE_VERSION`]).
    rule_version: String,
    /// A digest of the fact sheet the level was computed from, or `unreadable`
    /// when no sheet could be built.
    evidence_reference: String,
    /// RPC calls the read cost.
    calls: u32,
}

impl Observation {
    /// What this read tells the rule, if it tells it anything.
    ///
    /// * A rug when the level is `Rugged`: the sheet showed the pair itself.
    /// * Nothing when the level is `CantTell`.
    /// * "No rug seen" only from a read that was complete **and** taken with
    ///   an instrument that could have seen one. This is the one seam that
    ///   keeps a calm read from an instrument that cannot fire from being
    ///   published as a coin that stood (design 0032 §12); it changes in this
    ///   one place when a rug can be detected.
    fn reading(&self) -> Option<Reading> {
        match self.level {
            Level::Rugged => Some(Reading::Rug),
            Level::CantTell => None,
            _ => (self.complete && self.rug_detectable).then_some(Reading::NoRug),
        }
    }
}

/// Reads the observations file's text: one JSON object a line, blank lines
/// ignored. A line that does not parse is an error naming the line, and
/// nothing is settled from a file that cannot be read whole, since dropping a
/// line could drop a rug.
///
/// The one exception is a final line with no newline after it. The writer
/// puts each line and its newline down in a single write, so an unterminated
/// tail is a write that was cut short (a kill, a full disk), never a finished
/// read; it is not evidence, is reported by the second value, and the coin it
/// was for is still owed a read. It is never read as a partial line.
fn parse_observations(text: &str) -> Result<(Vec<Observation>, bool), String> {
    let (whole, torn) = match text.rfind('\n') {
        Some(end) => (&text[..=end], text.len() > end + 1),
        None => ("", !text.is_empty()),
    };
    let mut out = Vec::new();
    for (i, line) in whole.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let obs = serde_json::from_str(line)
            .map_err(|e| format!("the observations file is not valid at line {}: {e}", i + 1))?;
        out.push(obs);
    }
    Ok((out, torn))
}

/// If the observations file ends in a line that was cut short, cuts it off, so
/// the next line is appended after a newline and not glued to a fragment.
/// Returns whether anything was cut. Called with the run lock held: no other
/// writer is appending.
fn drop_torn_tail(path: &str) -> Result<bool, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("cannot read {path}: {e}")),
    };
    if bytes.last().is_none_or(|last| *last == b'\n') {
        return Ok(false);
    }
    let keep = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.set_len(keep as u64))
        .map_err(|e| format!("cannot repair {path}: {e}"))?;
    Ok(true)
}

/// Held for the length of an `observe` run so two runs cannot interleave their
/// lines. The lock is the operating system's, on a file beside the
/// observations, so it goes when the process does: a killed run leaves no
/// stale lock to clear by hand.
struct RunLock {
    _file: std::fs::File,
}

impl RunLock {
    fn acquire(observations_path: &str) -> Result<Self, String> {
        let path = format!("{observations_path}.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("cannot open {path}: {e}"))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(format!(
                "another settle run holds {path}: nothing was read, and it will be tried at the next run"
            )),
            Err(std::fs::TryLockError::Error(e)) => Err(format!("cannot lock {path}: {e}")),
        }
    }
}

/// Writes one line, and its newline, in one write. A kill lands before it or
/// after it, or leaves a tail [`parse_observations`] and [`drop_torn_tail`]
/// know how to drop; it can never leave half of one line glued to the next.
fn append_line(file: &mut std::fs::File, line: &str) -> std::io::Result<()> {
    let mut bytes = String::with_capacity(line.len() + 1);
    bytes.push_str(line);
    bytes.push('\n');
    file.write_all(bytes.as_bytes())?;
    file.flush()
}

/// The reads on record for one coin, taken no later than `now`, that the
/// current rule can use. An observation stamped after `now` is a clock fault,
/// not evidence, and one made for another rule is not this rule's evidence.
fn reads_for(
    observations: &[Observation],
    round: &str,
    chain: &str,
    token: &str,
    now: i64,
) -> Vec<Read> {
    observations
        .iter()
        .filter(|o| {
            o.round == round
                && o.chain == chain
                && o.token == token
                && o.at <= now
                && o.rule_version == settle::RULE_VERSION
        })
        .filter_map(|o| o.reading().map(|reading| Read { at: o.at, reading }))
        .collect()
}

/// A coin's identity in the outcomes file.
type Key = (String, String, String);

fn key_of(row: &OutcomeRow) -> Key {
    (row.round.clone(), row.chain.clone(), row.token.clone())
}

/// What one read of one coin gave.
struct Sample {
    level: Level,
    /// Whether the code that built the sheet could have shown a rug at all.
    rug_detectable: bool,
    evidence: String,
    calls: u32,
}

/// What an `observe` run did.
#[derive(Debug, Default, PartialEq, Eq)]
struct ObserveReport {
    /// Coins read, complete or not.
    read: usize,
    /// Of those, reads that were incomplete or failed.
    incomplete: usize,
    /// Coins not owed a read now (settled, covered today, or outside the
    /// window).
    skipped: usize,
    /// The run stopped at its ceiling with coins still owed.
    capped: bool,
}

/// Reads every coin that is owed a read and hands each dated observation to
/// `sink`. `read` is the chain; `limit` is the run's ceiling. What is owed is
/// decided at the clock's reading when the run starts; each read is dated by
/// `clock` when it finishes, so a late read in a long run is not dated before
/// it happened.
fn observe_all(
    rounds: &[Round],
    observations: &[Observation],
    published: &[OutcomeRow],
    clock: &dyn Fn() -> i64,
    limit: usize,
    read: &mut dyn FnMut(&str, &str) -> Result<Sample, String>,
    sink: &mut dyn FnMut(&Observation) -> Result<(), String>,
) -> Result<ObserveReport, String> {
    let now = clock();
    let settled: HashSet<Key> = published.iter().map(key_of).collect();
    let mut report = ObserveReport::default();
    for round in rounds {
        for coin in &round.coins {
            let key = (round.id.clone(), coin.chain.clone(), coin.token.clone());
            let reads = reads_for(observations, &round.id, &coin.chain, &coin.token, now);
            if settled.contains(&key) || !settle::due(round.close, &reads, now) {
                report.skipped += 1;
                continue;
            }
            if report.read >= limit {
                report.capped = true;
                return Ok(report);
            }
            report.read += 1;
            let observation = match read(&coin.chain, &coin.token) {
                Ok(sample) => Observation {
                    round: round.id.clone(),
                    chain: coin.chain.clone(),
                    token: coin.token.clone(),
                    at: clock(),
                    level: sample.level,
                    complete: sample.level != Level::CantTell,
                    rug_detectable: sample.rug_detectable,
                    rule_version: settle::RULE_VERSION.to_owned(),
                    evidence_reference: sample.evidence,
                    calls: sample.calls,
                },
                // The reader's reason is not kept: it can carry an endpoint.
                Err(_) => Observation {
                    round: round.id.clone(),
                    chain: coin.chain.clone(),
                    token: coin.token.clone(),
                    at: clock(),
                    level: Level::CantTell,
                    complete: false,
                    rug_detectable: false,
                    rule_version: settle::RULE_VERSION.to_owned(),
                    evidence_reference: "unreadable".to_owned(),
                    calls: 0,
                },
            };
            if !observation.complete {
                report.incomplete += 1;
            }
            sink(&observation)?;
        }
    }
    Ok(report)
}

/// What a `publish` run decided.
#[derive(Debug, Default, PartialEq, Eq)]
struct Decision {
    /// Outcomes newly decided.
    added: Vec<OutcomeRow>,
    /// Coins that already had a row in the file, kept as they were.
    carried: usize,
    /// Coins the rule cannot decide yet: nothing is written for them.
    pending: usize,
}

/// Decides every coin that has no row yet, at `now`.
fn decide(
    rounds: &[Round],
    observations: &[Observation],
    published: &[OutcomeRow],
    now: i64,
) -> Decision {
    let have: HashSet<Key> = published.iter().map(key_of).collect();
    let mut decision = Decision::default();
    for round in rounds {
        for coin in &round.coins {
            let key = (round.id.clone(), coin.chain.clone(), coin.token.clone());
            if have.contains(&key) {
                decision.carried += 1;
                continue;
            }
            let reads = reads_for(observations, &round.id, &coin.chain, &coin.token, now);
            let Some(outcome) = settle::derive(round.close, &reads, now) else {
                decision.pending += 1;
                continue;
            };
            decision.added.push(OutcomeRow {
                round: round.id.clone(),
                chain: coin.chain.clone(),
                token: coin.token.clone(),
                outcome,
                rule_version: settle::RULE_VERSION.to_owned(),
                evidence_reference: settle::encode_evidence(round.close, &reads),
                settled_at: now,
            });
        }
    }
    decision
}

/// The seconds-since-the-epoch of a moment, or `None` before it.
fn secs(t: SystemTime) -> Option<i64> {
    let d = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    i64::try_from(d.as_secs()).ok()
}

/// An environment value, or nothing when it is unset or empty: an empty path
/// is a config that was not given, not a path (AGENTS.md section 3 rule 7).
fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

/// The RPC address, or nothing. No default: `RpcClient` alone would fall back
/// to the public endpoint, and a settlement job that quietly reads through it
/// spends a budget nobody set (design 0032 §9).
fn rpc_url(flag_value: Option<String>, get: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    non_empty(flag_value).or_else(|| {
        non_empty(realorrug_types::env::env_or_legacy(
            "REALORRUG_RPC",
            "RADAR_RPC",
            get,
        ))
    })
}

/// A path from its flag or its variable.
fn path_arg(
    args: &[String],
    flag_name: &str,
    var: &str,
    get: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    non_empty(flag(args, flag_name)).or_else(|| non_empty(get(var)))
}

/// A path that must be given.
fn required(value: Option<String>, what: &str) -> Result<String, String> {
    value.ok_or_else(|| format!("{what} is required: there is no default"))
}

/// A file's text, or `None` when it is not there. Any other failure is an
/// error: an unreadable file is not an empty one.
fn read_optional(path: &str) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {path}: {e}")),
    }
}

/// The rows of a published outcomes file, or none when it is not there yet.
fn read_outcomes(path: &str) -> Result<Option<OutcomesFile>, String> {
    read_optional(path)?
        .map(|text| settle::parse_outcomes(&text))
        .transpose()
}

/// Writes `text` to `path` so a reader sees the old file or the new one and
/// never half of it.
fn write_atomically(path: &str, text: &str) -> Result<(), String> {
    let temporary = format!("{path}.tmp");
    std::fs::write(&temporary, text).map_err(|e| format!("cannot write {temporary}: {e}"))?;
    std::fs::rename(&temporary, path).map_err(|e| format!("cannot replace {path}: {e}"))
}

/// A short digest naming the fact sheet a level was computed from.
fn sheet_reference(sheet: &FactSheet) -> String {
    let hex = blake3::hash(sheet.render().as_bytes()).to_hex();
    format!("sheet-blake3:{}", &hex.as_str()[..32])
}

/// What one read of one coin gave, from its dossier: the real fact sheet, the
/// code's own level, and whether that instrument could have shown a rug. The
/// one place a level is taken for settlement, so a test can drive the same
/// path the command does.
fn sample_of(
    dossier: &realorrug_onchain::Dossier,
    rates: Option<&BaseRates>,
    creators: Option<&realorrug_roast::CreatorIndex>,
    self_mint: Option<&realorrug_types::Address>,
) -> Sample {
    let sheet = FactSheet::build(dossier, rates, creators, self_mint, None);
    Sample {
        level: realorrug_roast::level(&sheet),
        rug_detectable: realorrug_roast::rug_detectable(),
        evidence: sheet_reference(&sheet),
        calls: dossier.calls,
    }
}

/// The rounds and observations paths, both required: there is no default.
fn input_paths(
    args: &[String],
    get: &dyn Fn(&str) -> Option<String>,
) -> Result<(String, String), String> {
    let rounds = required(
        path_arg(args, "--rounds", "REALORRUG_ROUNDS_FILE", get),
        "--rounds PATH (or REALORRUG_ROUNDS_FILE)",
    )?;
    let observations = required(
        path_arg(args, "--observations", "REALORRUG_SETTLE_OBSERVATIONS", get),
        "--observations PATH (or REALORRUG_SETTLE_OBSERVATIONS)",
    )?;
    Ok((rounds, observations))
}

/// The rounds, and the observations on record so far (none when the file is
/// not there yet).
fn load_inputs(
    rounds_path: &str,
    observations_path: &str,
) -> Result<(Vec<Round>, Vec<Observation>), String> {
    let text = std::fs::read_to_string(rounds_path)
        .map_err(|e| format!("cannot read {rounds_path}: {e}"))?;
    let rounds = parse_rounds(&text)?;
    let (observations, torn) = read_optional(observations_path)?
        .map(|text| parse_observations(&text))
        .transpose()?
        .unwrap_or_default();
    if torn {
        eprintln!(
            "warning: {observations_path} ends in a line that was cut short; it is not evidence and its coin is read again"
        );
    }
    Ok((rounds, observations))
}

/// `settle observe`.
fn observe_command(args: &[String], get: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
    let (rounds_path, observations_path) = input_paths(args, get)?;
    let Some(rpc) = rpc_url(flag(args, "--rpc"), get) else {
        println!("no RPC is configured (--rpc or REALORRUG_RPC): nothing was read");
        return Ok(());
    };
    // A read that cannot be evidence is spend for nothing: the fact sheet this
    // build makes cannot show a rug, so a calm level from it settles nothing
    // (design 0032 §12). Deny by default; this line goes when the detector does.
    if !realorrug_roast::rug_detectable() {
        println!(
            "this build cannot detect a rug, so a read of a coin could not count as evidence: nothing was read"
        );
        return Ok(());
    }
    let _lock = RunLock::acquire(&observations_path)?;
    if drop_torn_tail(&observations_path)? {
        eprintln!("warning: cut a line that was cut short off the end of {observations_path}");
    }
    let (rounds, observations) = load_inputs(&rounds_path, &observations_path)?;
    let published = match path_arg(args, "--outcomes", "REALORRUG_OUTCOMES_FILE", get) {
        Some(path) => read_outcomes(&path)?.unwrap_or_default().outcomes,
        None => Vec::new(),
    };
    let now = secs(SystemTime::now()).ok_or("the system clock is before 1970")?;

    let client = RpcClient::new(rpc);
    let robinhood = non_empty(flag(args, "--robinhood-rpc"))
        .or_else(|| {
            non_empty(realorrug_types::env::env_or_legacy(
                "REALORRUG_ROBINHOOD_RPC",
                "RADAR_ROBINHOOD_RPC",
                get,
            ))
        })
        .map(|url| realorrug_robinhood::Rpc::new(&url));
    let market = realorrug_onchain::market::Http::default();
    let clients = dispatch::Clients {
        solana: &client,
        robinhood: robinhood.as_ref(),
        market: Some(&market),
    };
    let rates = BaseRates::load(
        &flag(args, "--rates")
            .unwrap_or_else(|| realorrug_roast::baserates::DEFAULT_PATH.to_owned()),
    )
    .ok();
    let creators = realorrug_roast::CreatorIndex::read(realorrug_roast::creator::DEFAULT_PATH).ok();
    let self_mint = realorrug_analyst::daemon::self_mint_from(&|k| get(k))?;

    let mut read = |_chain: &str, token: &str| -> Result<Sample, String> {
        let dossier = dispatch::read(token, &clients).map_err(|_| "unreadable".to_owned())?;
        Ok(sample_of(
            &dossier,
            rates.as_ref(),
            creators.as_ref(),
            self_mint.as_ref(),
        ))
    };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&observations_path)
        .map_err(|e| format!("cannot open {observations_path}: {e}"))?;
    let mut sink = |o: &Observation| -> Result<(), String> {
        let line = serde_json::to_string(o).map_err(|e| format!("cannot write a line: {e}"))?;
        append_line(&mut file, &line)
            .map_err(|e| format!("cannot write {observations_path}: {e}"))?;
        println!(
            "observed {} {}: {:?}{} ({} calls)",
            o.round,
            o.token,
            o.level,
            if o.complete { "" } else { ", incomplete" },
            o.calls
        );
        Ok(())
    };
    let report = observe_all(
        &rounds,
        &observations,
        &published,
        &|| secs(SystemTime::now()).unwrap_or(now),
        MAX_READS_PER_RUN,
        &mut read,
        &mut sink,
    )?;
    println!(
        "read {} coins ({} incomplete), {} not owed a read now{}",
        report.read,
        report.incomplete,
        report.skipped,
        if report.capped {
            "; stopped at the per-run ceiling with coins still owed"
        } else {
            ""
        }
    );
    Ok(())
}

/// `settle publish`.
fn publish_command(args: &[String], get: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
    let (rounds_path, observations_path) = input_paths(args, get)?;
    let outcomes_path = required(
        path_arg(args, "--outcomes", "REALORRUG_OUTCOMES_FILE", get),
        "--outcomes PATH (or REALORRUG_OUTCOMES_FILE)",
    )?;
    let (rounds, observations) = load_inputs(&rounds_path, &observations_path)?;
    let existing = read_outcomes(&outcomes_path)?;
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let now = secs(SystemTime::now()).ok_or("the system clock is before 1970")?;
    let file_exists = existing.is_some();
    let mut file = existing.unwrap_or_default();
    let decision = decide(&rounds, &observations, &file.outcomes, now);
    let verb = if dry_run {
        "would publish"
    } else {
        "published"
    };
    for row in &decision.added {
        println!(
            "{verb} {} {} {}: {:?}",
            row.round, row.chain, row.token, row.outcome
        );
    }
    println!(
        "{verb} {}, {} already published, {} not decidable yet",
        decision.added.len(),
        decision.carried,
        decision.pending
    );
    // Rewrite only when there is something new to say (or no file at all, so
    // serve finds a file that says "nothing yet" rather than none).
    if !dry_run && (!decision.added.is_empty() || !file_exists) {
        file.outcomes.extend(decision.added);
        write_atomically(&outcomes_path, &settle::render_outcomes(&file)?)?;
    }
    Ok(())
}

/// `settle verify`'s verdict: clean only when every row re-derives.
fn audit_result(disagree: usize) -> Result<(), String> {
    if disagree == 0 {
        Ok(())
    } else {
        Err("a published outcome does not re-derive from its own evidence".to_owned())
    }
}

/// `settle verify`: re-derives every row of the outcomes file from the row
/// alone.
fn verify_command(args: &[String], get: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
    let outcomes_path = required(
        path_arg(args, "--outcomes", "REALORRUG_OUTCOMES_FILE", get),
        "--outcomes PATH (or REALORRUG_OUTCOMES_FILE)",
    )?;
    let file = read_outcomes(&outcomes_path)?
        .ok_or_else(|| format!("cannot read {outcomes_path}: no such file"))?;
    let agree = file.outcomes.iter().filter(|r| r.rederives()).count();
    let disagree = file.outcomes.len() - agree;
    println!("{agree} outcomes re-derive, {disagree} do not");
    audit_result(disagree)
}

/// Runs the command.
///
/// # Errors
///
/// A message when a required path is not given (there is no default: an
/// operator who named none has chosen not to settle), when a file cannot be
/// read or is not the shape it should be, or, for `verify`, when a row does
/// not re-derive.
pub fn run(args: &[String]) -> Result<(), String> {
    let get = |name: &str| non_empty(std::env::var(name).ok());
    match args.get(1).map(String::as_str) {
        Some("observe") => observe_command(args, &get),
        Some("publish") => publish_command(args, &get),
        Some("verify") => verify_command(args, &get),
        _ => Err(USAGE.to_owned()),
    }
}

/// Whether a path exists as a file (for tests of the commands).
#[cfg(test)]
fn is_file(path: &str) -> bool {
    std::path::Path::new(path).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use realorrug_contest::calls::Outcome;

    const CLOSE: i64 = 2_000_000;
    const DAY: i64 = 86_400;
    const CHAIN: &str = "solana";
    const TOKEN: &str = "coinA";

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

    fn obs(token: &str, at: i64, level: Level, complete: bool) -> Observation {
        Observation {
            round: "r1".to_owned(),
            chain: CHAIN.to_owned(),
            token: token.to_owned(),
            at,
            level,
            complete,
            rug_detectable: true,
            rule_version: settle::RULE_VERSION.to_owned(),
            evidence_reference: "sheet-blake3:x".to_owned(),
            calls: 7,
        }
    }

    fn calm(token: &str, at: i64) -> Observation {
        obs(token, at, Level::NothingUglyYet, true)
    }

    /// A calm, complete read from an instrument that could not have shown a
    /// rug: what every build reads today.
    fn blind(token: &str, at: i64) -> Observation {
        Observation {
            rug_detectable: false,
            ..calm(token, at)
        }
    }

    /// A read that found the rug pair.
    fn rug(token: &str, at: i64) -> Observation {
        obs(token, at, Level::Rugged, true)
    }

    /// One complete calm read in the middle of each of the fourteen spans,
    /// and the read at the horizon that closes the last day.
    fn daily(token: &str) -> Vec<Observation> {
        let mut reads: Vec<Observation> = (0..14)
            .map(|d| calm(token, CLOSE + d * DAY + 3_600))
            .collect();
        reads.push(calm(token, settle::horizon(CLOSE) + 3_600));
        reads
    }

    /// When the horizon read has been taken and `Stood` may be written.
    fn after_horizon_read() -> i64 {
        settle::horizon(CLOSE) + 7_200
    }

    /// When the horizon read's grace is over.
    fn grace_over() -> i64 {
        settle::final_read_by(CLOSE)
    }

    fn end() -> i64 {
        settle::horizon(CLOSE)
    }

    fn one_round() -> Vec<Round> {
        vec![round("r1", CLOSE, &[TOKEN])]
    }

    fn outcome_of(decision: &Decision) -> Option<Outcome> {
        decision.added.first().map(|r| r.outcome)
    }

    /// A rug seen twice inside the window, after a calm first read, settles the
    /// coin `Rugged` at once, and the row it publishes re-derives to that from
    /// itself.
    #[test]
    fn a_confirmed_rug_inside_the_window_settles_rugged() {
        let observations = vec![
            calm(TOKEN, CLOSE + 3_600),
            rug(TOKEN, CLOSE + 2 * DAY),
            rug(TOKEN, CLOSE + 3 * DAY),
        ];
        let decision = decide(&one_round(), &observations, &[], CLOSE + 4 * DAY);
        assert_eq!(outcome_of(&decision), Some(Outcome::Rugged));
        assert!(decision.added[0].rederives());
        assert_eq!(decision.pending, 0);
    }

    /// One rug read is not a rug (a zero-reserve read can be a graduation
    /// block), and a coin whose first read is already a rug might have rugged
    /// before the close: neither settles `Rugged`, however the read is flagged.
    #[test]
    fn one_rug_read_and_a_rug_at_the_close_do_not_settle_rugged() {
        let one = vec![calm(TOKEN, CLOSE + 3_600), rug(TOKEN, CLOSE + 2 * DAY)];
        let decision = decide(&one_round(), &one, &[], CLOSE + 3 * DAY);
        assert!(decision.added.is_empty());
        assert_eq!(decision.pending, 1);
        let mut flagged = rug(TOKEN, CLOSE + DAY);
        flagged.complete = false;
        let early = vec![flagged, rug(TOKEN, CLOSE + 2 * DAY)];
        let decision = decide(&one_round(), &early, &[], grace_over());
        assert_eq!(outcome_of(&decision), Some(Outcome::Unresolved));
    }

    /// Nothing is published for an open window unless the coin has rugged.
    #[test]
    fn nothing_is_published_before_the_horizon_except_a_rug() {
        let calm_only = daily(TOKEN);
        // Thirteen days of complete calm reads, one instant before the end.
        let decision = decide(&one_round(), &calm_only, &[], end() - 1);
        assert!(decision.added.is_empty());
        assert_eq!(decision.pending, 1);
        // Before the close, even a rug read is not evidence.
        let early = vec![rug(TOKEN, CLOSE - 5), rug(TOKEN, CLOSE - 4)];
        assert!(
            decide(&one_round(), &early, &[], CLOSE - 1)
                .added
                .is_empty()
        );
        // The same coin, rugged: written at once.
        let mut rugged = calm_only;
        rugged.push(rug(TOKEN, CLOSE + 5 * DAY + 100));
        rugged.push(rug(TOKEN, CLOSE + 5 * DAY + 200));
        let decision = decide(&one_round(), &rugged, &[], end() - 1);
        assert_eq!(outcome_of(&decision), Some(Outcome::Rugged));
    }

    /// Fourteen complete daily reads and the read at the horizon make `Stood`,
    /// but not before that read: the last day is read too.
    #[test]
    fn a_full_history_stands_once_the_horizon_is_read() {
        let full = daily(TOKEN);
        let decision = decide(&one_round(), &full, &[], after_horizon_read());
        assert_eq!(outcome_of(&decision), Some(Outcome::Stood));
        assert!(decision.added[0].rederives());
        assert_eq!(decision.added[0].settled_at, after_horizon_read());
        let without_horizon = &full[..14];
        let decision = decide(&one_round(), without_horizon, &[], end() + 10);
        assert!(decision.added.is_empty());
        assert_eq!(decision.pending, 1);
        let decision = decide(&one_round(), without_horizon, &[], grace_over());
        assert_eq!(outcome_of(&decision), Some(Outcome::Unresolved));
    }

    /// A missing 24-hour span is a gap in the read history, which is no read:
    /// `Unresolved`, never `Stood`.
    #[test]
    fn a_missing_span_is_unresolved_not_stood() {
        for missing in 0..14usize {
            let mut observations = daily(TOKEN);
            observations.remove(missing);
            let decision = decide(&one_round(), &observations, &[], grace_over());
            assert_eq!(
                outcome_of(&decision),
                Some(Outcome::Unresolved),
                "day {missing} missing"
            );
        }
        // No history at all, after the grace: unresolved, not absent.
        let decision = decide(&one_round(), &[], &[], grace_over());
        assert_eq!(outcome_of(&decision), Some(Outcome::Unresolved));
    }

    /// A read that could not see everything it needed does not cover a span.
    #[test]
    fn an_incomplete_read_does_not_cover_its_span() {
        let mut observations = daily(TOKEN);
        observations[6] = obs(TOKEN, CLOSE + 6 * DAY + 3_600, Level::CantTell, false);
        let decision = decide(&one_round(), &observations, &[], grace_over());
        assert_eq!(outcome_of(&decision), Some(Outcome::Unresolved));
        // A level other than CantTell that the read itself flagged incomplete
        // is also not a complete read.
        let mut flagged = daily(TOKEN);
        flagged[3].complete = false;
        let decision = decide(&one_round(), &flagged, &[], grace_over());
        assert_eq!(outcome_of(&decision), Some(Outcome::Unresolved));
    }

    /// A calm read from an instrument that could not have shown a rug is no
    /// evidence that there was none: fourteen days of them settle `Unresolved`.
    #[test]
    fn calm_reads_from_a_blind_instrument_settle_unresolved() {
        let observations: Vec<Observation> =
            daily(TOKEN).iter().map(|o| blind(TOKEN, o.at)).collect();
        let decision = decide(&one_round(), &observations, &[], grace_over());
        assert_eq!(outcome_of(&decision), Some(Outcome::Unresolved));
        // And a line written before the field existed reads as blind.
        let old = r#"{"round":"r1","chain":"solana","token":"coinA","at":5,"level":"NothingUglyYet","complete":true,"rule_version":"settle-2","evidence_reference":"x","calls":1}"#;
        let (parsed, _) = parse_observations(&format!("{old}\n")).expect("parse");
        assert!(!parsed[0].rug_detectable);
        assert_eq!(parsed[0].reading(), None);
    }

    /// Only this coin's, this round's reads, under this rule and no later than
    /// now, are evidence.
    #[test]
    fn reads_belong_to_their_coin_round_rule_and_moment() {
        let mut other_rule = calm(TOKEN, CLOSE + 10);
        other_rule.rule_version = "settle-0".to_owned();
        let mut other_round = calm(TOKEN, CLOSE + 20);
        other_round.round = "r2".to_owned();
        let mut other_chain = calm(TOKEN, CLOSE + 30);
        other_chain.chain = "robinhood".to_owned();
        let all = vec![
            calm(TOKEN, CLOSE + 40),
            calm("coinB", CLOSE + 50),
            other_rule,
            other_round,
            other_chain,
            calm(TOKEN, CLOSE + 100),
            obs(TOKEN, CLOSE + 60, Level::CantTell, false),
            blind(TOKEN, CLOSE + 70),
        ];
        let reads = reads_for(&all, "r1", CHAIN, TOKEN, CLOSE + 80);
        assert_eq!(
            reads,
            vec![Read {
                at: CLOSE + 40,
                reading: Reading::NoRug
            }]
        );
    }

    /// A second publish over the same observations changes nothing: rows are
    /// carried as they are, and only new decisions are added.
    #[test]
    fn deciding_again_adds_nothing_and_keeps_the_rows() {
        let observations = vec![
            calm(TOKEN, CLOSE + 3_600),
            rug(TOKEN, CLOSE + DAY),
            rug(TOKEN, CLOSE + 2 * DAY),
        ];
        let first = decide(&one_round(), &observations, &[], CLOSE + 3 * DAY);
        assert_eq!(first.added.len(), 1);
        let again = decide(&one_round(), &observations, &first.added, CLOSE + 9 * DAY);
        assert!(again.added.is_empty());
        assert_eq!(again.carried, 1);
        assert_eq!(again.pending, 0);
    }

    fn reader(level: Level) -> impl FnMut(&str, &str) -> Result<Sample, String> {
        move |_, _| {
            Ok(Sample {
                level,
                rug_detectable: true,
                evidence: "sheet-blake3:e".to_owned(),
                calls: 9,
            })
        }
    }

    /// Runs `observe_all` with every read dated `now`.
    fn run_observe(
        rounds: &[Round],
        observations: &[Observation],
        published: &[OutcomeRow],
        now: i64,
        limit: usize,
        read: &mut dyn FnMut(&str, &str) -> Result<Sample, String>,
    ) -> (ObserveReport, Vec<Observation>) {
        let mut written = Vec::new();
        let report = observe_all(
            rounds,
            observations,
            published,
            &|| now,
            limit,
            read,
            &mut |o| {
                written.push(o.clone());
                Ok(())
            },
        )
        .expect("observe");
        (report, written)
    }

    /// One owed coin gets one dated observation carrying the code's level,
    /// the completeness of the read, the rule and the evidence reference.
    #[test]
    fn an_owed_coin_gets_one_dated_observation() {
        let now = CLOSE + DAY + 5;
        let (report, written) =
            run_observe(&one_round(), &[], &[], now, 10, &mut reader(Level::Sketchy));
        assert_eq!(
            report,
            ObserveReport {
                read: 1,
                incomplete: 0,
                skipped: 0,
                capped: false
            }
        );
        assert_eq!(written.len(), 1);
        let o = &written[0];
        assert_eq!(
            (o.at, o.level, o.complete, o.calls),
            (now, Level::Sketchy, true, 9)
        );
        assert!(o.rug_detectable);
        assert_eq!(o.rule_version, settle::RULE_VERSION);
        assert_eq!(o.evidence_reference, "sheet-blake3:e");
        assert_eq!((o.round.as_str(), o.token.as_str()), ("r1", TOKEN));
    }

    /// Each read is dated when it finishes, not when the run started: a late
    /// read in a long run is not stamped before it happened.
    #[test]
    fn each_read_is_dated_by_the_clock_when_it_finishes() {
        let rounds = vec![round("r1", CLOSE, &["a", "b", "c"])];
        let ticks = std::cell::Cell::new(CLOSE + 100);
        let mut written = Vec::new();
        observe_all(
            &rounds,
            &[],
            &[],
            &|| {
                ticks.set(ticks.get() + 20);
                ticks.get()
            },
            10,
            &mut reader(Level::NothingUglyYet),
            &mut |o| {
                written.push(o.at);
                Ok(())
            },
        )
        .expect("observe");
        assert_eq!(written, vec![CLOSE + 140, CLOSE + 160, CLOSE + 180]);
    }

    /// `CantTell` is an incomplete read; a failed read is recorded as one and
    /// its reason is not kept.
    #[test]
    fn an_unread_fact_or_a_failed_read_is_recorded_incomplete() {
        let now = CLOSE + DAY;
        let (report, written) = run_observe(
            &one_round(),
            &[],
            &[],
            now,
            10,
            &mut reader(Level::CantTell),
        );
        assert_eq!(report.incomplete, 1);
        assert!(!written[0].complete);
        let mut failing = |_: &str, _: &str| Err("https://secret.example/key failed".to_owned());
        let (report, written) = run_observe(&one_round(), &[], &[], now, 10, &mut failing);
        assert_eq!((report.read, report.incomplete), (1, 1));
        assert_eq!(written[0].level, Level::CantTell);
        assert!(!written[0].complete);
        assert!(!written[0].rug_detectable);
        assert_eq!(written[0].evidence_reference, "unreadable");
        assert_eq!(written[0].calls, 0);
        assert!(
            !serde_json::to_string(&written[0])
                .expect("json")
                .contains("secret")
        );
        // Neither covers the span: the next run is owed a read again.
        assert!(settle::due(
            CLOSE,
            &reads_for(&written, "r1", CHAIN, TOKEN, now + 10),
            now + 10
        ));
    }

    /// A coin is not read when its window is not open, when it is already
    /// settled, when it rugged, or when today's span is covered.
    #[test]
    fn a_coin_not_owed_a_read_is_not_read() {
        let mut calls = 0usize;
        let mut counting = |_: &str, _: &str| {
            calls += 1;
            Ok(Sample {
                level: Level::NothingUglyYet,
                rug_detectable: true,
                evidence: "e".to_owned(),
                calls: 1,
            })
        };
        // Before the close, and after the horizon with no history to stand on.
        for now in [CLOSE - 1, end(), grace_over()] {
            let (report, written) = run_observe(&one_round(), &[], &[], now, 10, &mut counting);
            assert_eq!((report.read, report.skipped), (0, 1), "now {now}");
            assert!(written.is_empty());
        }
        // Covered today.
        let covered = vec![calm(TOKEN, CLOSE + 100)];
        let (report, _) = run_observe(&one_round(), &covered, &[], CLOSE + 200, 10, &mut counting);
        assert_eq!((report.read, report.skipped), (0, 1));
        // Rugged (observed twice, not yet published).
        let rugged = vec![
            calm(TOKEN, CLOSE + 100),
            rug(TOKEN, CLOSE + DAY + 100),
            rug(TOKEN, CLOSE + DAY + 200),
        ];
        let (report, _) = run_observe(
            &one_round(),
            &rugged,
            &[],
            CLOSE + 2 * DAY,
            10,
            &mut counting,
        );
        assert_eq!((report.read, report.skipped), (0, 1));
        // Already published.
        let row = decide(&one_round(), &rugged, &[], CLOSE + 2 * DAY).added;
        assert_eq!(row.len(), 1);
        let (report, _) = run_observe(&one_round(), &[], &row, CLOSE + 3 * DAY, 10, &mut counting);
        assert_eq!((report.read, report.skipped), (0, 1));
        // The next day's span is owed a read again.
        let (report, _) = run_observe(
            &one_round(),
            &covered,
            &[],
            CLOSE + DAY + 5,
            10,
            &mut counting,
        );
        assert_eq!(report.read, 1);
        // And so is the horizon, once fourteen days are covered.
        let covered_all = daily(TOKEN)[..14].to_vec();
        let (report, _) = run_observe(
            &one_round(),
            &covered_all,
            &[],
            end() + 5,
            10,
            &mut counting,
        );
        assert_eq!(report.read, 1);
        assert_eq!(calls, 2);
    }

    /// The per-run ceiling stops the run and says so; the coins it did not
    /// reach stay owed.
    #[test]
    fn a_run_stops_at_its_ceiling() {
        let rounds = vec![round("r1", CLOSE, &["a", "b", "c"])];
        let (report, written) = run_observe(
            &rounds,
            &[],
            &[],
            CLOSE + 5,
            2,
            &mut reader(Level::NothingUglyYet),
        );
        assert_eq!(report.read, 2);
        assert!(report.capped);
        assert_eq!(written.len(), 2);
        let (report, _) = run_observe(
            &rounds,
            &[],
            &[],
            CLOSE + 5,
            3,
            &mut reader(Level::NothingUglyYet),
        );
        assert!(!report.capped);
        assert_eq!(report.read, 3);
    }

    /// A sink that cannot write stops the run with its error.
    #[test]
    fn a_failed_write_stops_the_run() {
        let err = observe_all(
            &one_round(),
            &[],
            &[],
            &|| CLOSE + 5,
            5,
            &mut reader(Level::NothingUglyYet),
            &mut |_| Err("disk full".to_owned()),
        )
        .expect_err("stopped");
        assert_eq!(err, "disk full");
    }

    /// Lines are read whole, blanks are skipped, and a bad line names its
    /// number and reads as nothing.
    #[test]
    fn the_observations_file_is_read_whole_or_not_at_all() {
        let good = serde_json::to_string(&calm(TOKEN, CLOSE)).expect("json");
        let text = format!("{good}\n\n  \n{good}\n");
        let (parsed, torn) = parse_observations(&text).expect("parse");
        assert_eq!((parsed.len(), torn), (2, false));
        let (empty, torn) = parse_observations("").expect("empty");
        assert_eq!((empty.len(), torn), (0, false));
        let err = parse_observations(&format!("{good}\nnot json\n")).expect_err("bad");
        assert!(err.contains("line 2"), "{err}");
        let extra = format!("{}\n", good.replace('}', r#","extra":1}"#));
        assert!(parse_observations(&extra).is_err());
    }

    /// A final line with no newline is a write that was cut short: it is
    /// skipped and reported, whether it is a fragment or a whole object that
    /// only lost its newline, and the lines before it are kept.
    #[test]
    fn a_torn_final_line_is_skipped_and_reported_not_fatal() {
        let good = serde_json::to_string(&calm(TOKEN, CLOSE)).expect("json");
        let fragment = &good[..good.len() / 2];
        let (parsed, torn) = parse_observations(&format!("{good}\n{fragment}")).expect("parse");
        assert_eq!((parsed.len(), torn), (1, true));
        // Even a complete-looking object without its newline is not evidence.
        let (parsed, torn) = parse_observations(&format!("{good}\n{good}")).expect("parse");
        assert_eq!((parsed.len(), torn), (1, true));
        let (parsed, torn) = parse_observations(fragment).expect("parse");
        assert_eq!((parsed.len(), torn), (0, true));
        // A bad line that is not the last is still fatal.
        assert!(parse_observations(&format!("{fragment}\n{good}\n")).is_err());
    }

    fn scratch(name: &str) -> String {
        let dir =
            std::env::temp_dir().join(format!("realorrug-settle-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        dir.join("observations.jsonl")
            .to_string_lossy()
            .into_owned()
    }

    /// The fragment a killed writer left is cut off before the next line is
    /// appended, so the next line is never glued to it.
    #[test]
    fn the_torn_tail_is_cut_off_before_the_next_append() {
        let path = scratch("tail");
        let good = serde_json::to_string(&calm(TOKEN, CLOSE)).expect("json");
        std::fs::write(&path, format!("{good}\n{}", &good[..20])).expect("write");
        assert!(drop_torn_tail(&path).expect("cut"));
        assert_eq!(
            std::fs::read_to_string(&path).expect("read"),
            format!("{good}\n")
        );
        assert!(!drop_torn_tail(&path).expect("nothing to cut"));
        // A file that is only a fragment is emptied; a missing one is fine.
        std::fs::write(&path, &good[..20]).expect("write");
        assert!(drop_torn_tail(&path).expect("cut"));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "");
        std::fs::remove_file(&path).expect("remove");
        assert!(!drop_torn_tail(&path).expect("missing"));
    }

    /// A line goes down with its newline, and appended lines read back whole.
    #[test]
    fn appended_lines_end_in_a_newline_and_read_back() {
        let path = scratch("append");
        let _ = std::fs::remove_file(&path);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .expect("open");
        for at in [CLOSE, CLOSE + 1] {
            let line = serde_json::to_string(&calm(TOKEN, at)).expect("json");
            append_line(&mut file, &line).expect("append");
        }
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.ends_with('\n'));
        let (parsed, torn) = parse_observations(&text).expect("parse");
        assert_eq!((parsed.len(), torn), (2, false));
    }

    /// Two runs cannot hold the observations file at once.
    #[test]
    fn a_second_run_is_refused_while_one_holds_the_lock() {
        let path = scratch("lock");
        let first = RunLock::acquire(&path).expect("first");
        let err = RunLock::acquire(&path).err().expect("refused");
        assert!(err.contains("another settle run"), "{err}");
        drop(first);
        assert!(RunLock::acquire(&path).is_ok());
    }

    /// The drained coin, through the real builder and the real level, is never
    /// published `Stood`. Today the sheet cannot show a rug, so no calm read is
    /// evidence and the coin ends `Unresolved` (design 0032 §12). This is the
    /// test that goes through `FactSheet::build` rather than a stubbed level:
    /// with `Observation::reading` counting any complete calm read as "no rug
    /// seen" it fails.
    #[test]
    fn a_drained_coin_is_never_stood_through_the_real_sheet() {
        let mint =
            realorrug_types::ChainAddress::Robinhood(realorrug_robinhood::Address([0x13u8; 20]));
        let dossier = realorrug_onchain::Dossier {
            mint,
            read_at: Some(realorrug_types::ReadAt::Robinhood(3_012_345)),
            launch: None,
            curve: Some(realorrug_onchain::dossier::CurveFacts {
                complete: false,
                quote_reserves: 0,
                quote_capacity: None,
                quote_asset: Some(realorrug_onchain::QuoteAsset::eth()),
                creator: realorrug_types::ChainAddress::Robinhood(realorrug_robinhood::Address(
                    [9u8; 20],
                )),
                fees: None,
            }),
            creator_transactions: None,
            chain_launch: Some(realorrug_onchain::ChainLaunch {
                block: 2_998_000,
                age_seconds: Some(86_400),
                dev_buy_wei: Some(50_000_000_000_000_000),
                dev_buy_tokens: None,
                supply: None,
                name: None,
                symbol: None,
                correlated_selling: None,
            }),
            holders: Some(realorrug_onchain::Holders {
                count: 3,
                largest_share_bps: Some(9_900),
            }),
            funding: None,
            market: None,
            token_ownership: None,
            creator_cash_flow: None,
            powers: None,
            unavailable: Vec::new(),
            calls: 10,
            elapsed_ms: 1,
            retries: 0,
            paused_ms: 0,
        };
        let sample = sample_of(&dossier, None, None, None);
        assert_ne!(
            sample.level,
            Level::CantTell,
            "the fixture must be a complete read, or the test proves nothing"
        );
        assert!(!sample.rug_detectable);

        // Fourteen daily runs and the horizon read, each through the real
        // sheet; the coin is decided after every one.
        let mut observations: Vec<Observation> = Vec::new();
        let mut read = |_: &str, _: &str| Ok(sample_of(&dossier, None, None, None));
        for day in 0..=14 {
            let now = CLOSE + day * DAY + 3_600;
            let (_, written) = run_observe(&one_round(), &observations, &[], now, 10, &mut read);
            observations.extend(written);
            let decision = decide(&one_round(), &observations, &[], now);
            assert!(
                decision.added.iter().all(|r| r.outcome != Outcome::Stood),
                "day {day}: {:?}",
                decision.added
            );
        }
        assert!(observations.iter().all(|o| o.reading().is_none()));
        let decision = decide(&one_round(), &observations, &[], grace_over());
        assert_eq!(outcome_of(&decision), Some(Outcome::Unresolved));
    }

    /// The round list is read leniently on unknown fields and strictly on ids.
    #[test]
    fn a_rounds_file_is_read_leniently_and_refused_strictly() {
        let ok = r#"{"rounds":[{"id":"r1","close":5,"coins":[{"chain":"solana","token":"a","odds":1}]}]}"#;
        assert_eq!(parse_rounds(ok).expect("ok").len(), 1);
        assert!(parse_rounds("not json").is_err());
        let empty_id = r#"{"rounds":[{"id":"","close":5,"coins":[]}]}"#;
        assert!(parse_rounds(empty_id).is_err());
        let long = format!(
            r#"{{"rounds":[{{"id":"{}","close":5,"coins":[]}}]}}"#,
            "x".repeat(129)
        );
        assert!(parse_rounds(&long).is_err());
        let fits = format!(
            r#"{{"rounds":[{{"id":"{}","close":5,"coins":[]}}]}}"#,
            "x".repeat(128)
        );
        assert!(parse_rounds(&fits).is_ok());
        let twice =
            r#"{"rounds":[{"id":"r","close":5,"coins":[]},{"id":"r","close":6,"coins":[]}]}"#;
        assert!(parse_rounds(twice).is_err());
    }

    #[test]
    fn an_empty_value_is_a_missing_one() {
        assert_eq!(non_empty(Some(String::new())), None);
        assert_eq!(non_empty(Some("x".to_owned())), Some("x".to_owned()));
        assert_eq!(non_empty(None), None);
    }

    /// No flag and no variable is no RPC: the public endpoint is not a
    /// fallback. The flag wins over the variable, and the legacy name is read.
    #[test]
    fn the_rpc_has_no_default() {
        let none = |_: &str| None;
        assert_eq!(rpc_url(None, &none), None);
        assert_eq!(rpc_url(Some(String::new()), &none), None);
        let env = |k: &str| (k == "REALORRUG_RPC").then(|| "https://env.example".to_owned());
        assert_eq!(rpc_url(None, &env), Some("https://env.example".to_owned()));
        assert_eq!(
            rpc_url(Some(String::new()), &env),
            Some("https://env.example".to_owned())
        );
        assert_eq!(
            rpc_url(Some("https://flag.example".to_owned()), &env),
            Some("https://flag.example".to_owned())
        );
        let empty = |k: &str| (k == "REALORRUG_RPC").then(String::new);
        assert_eq!(
            rpc_url(None, &empty),
            Some(String::new()).filter(|s| !s.is_empty())
        );
        let legacy = |k: &str| (k == "RADAR_RPC").then(|| "https://old.example".to_owned());
        assert_eq!(
            rpc_url(None, &legacy),
            Some("https://old.example".to_owned())
        );
    }

    #[test]
    fn a_path_comes_from_its_flag_or_its_variable() {
        let get = |k: &str| (k == "VAR").then(|| "from-env".to_owned());
        let args: Vec<String> = ["settle", "publish", "--x", "from-flag"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(
            path_arg(&args, "--x", "VAR", &get),
            Some("from-flag".to_owned())
        );
        assert_eq!(
            path_arg(&args, "--y", "VAR", &get),
            Some("from-env".to_owned())
        );
        assert_eq!(path_arg(&args, "--y", "NOPE", &get), None);
        let blank: Vec<String> = ["settle", "--x", ""]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(
            path_arg(&blank, "--x", "VAR", &get),
            Some("from-env".to_owned())
        );
        assert!(required(None, "--x").is_err());
        assert_eq!(required(Some("p".to_owned()), "--x"), Ok("p".to_owned()));
    }

    #[test]
    fn a_moment_before_1970_has_no_seconds() {
        assert_eq!(secs(SystemTime::UNIX_EPOCH), Some(0));
        let before = SystemTime::UNIX_EPOCH - std::time::Duration::from_secs(5);
        assert_eq!(secs(before), None);
        assert_eq!(
            secs(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(90)),
            Some(90)
        );
    }

    #[test]
    fn verify_passes_only_when_nothing_disagrees() {
        assert!(audit_result(0).is_ok());
        assert!(audit_result(1).is_err());
    }

    fn args_of(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("realorrug-cli-settle-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    /// With no RPC configured `observe` reads nothing, says so, and does not
    /// even create the observations file. The rounds file names a coin that is
    /// owed a read, so only the missing RPC is what stops it.
    #[test]
    fn no_rpc_config_means_observe_reads_nothing() {
        let dir = temp_dir("no-rpc");
        let now = secs(SystemTime::now()).expect("clock");
        let rounds = dir.join("rounds.json");
        std::fs::write(
            &rounds,
            format!(
                r#"{{"rounds":[{{"id":"r1","close":{},"coins":[{{"chain":"solana","token":"{}"}}]}}]}}"#,
                now - DAY,
                "So11111111111111111111111111111111111111112"
            ),
        )
        .expect("rounds");
        let observations = dir.join("observations.jsonl");
        let args = args_of(&[
            "settle",
            "observe",
            "--rounds",
            rounds.to_str().expect("p"),
            "--observations",
            observations.to_str().expect("p"),
        ]);
        observe_command(&args, &|_| None).expect("does nothing, and says so");
        assert!(!is_file(observations.to_str().expect("p")));
    }

    #[test]
    fn observe_needs_its_paths() {
        let none = args_of(&["settle", "observe"]);
        let err = observe_command(&none, &|_| None).expect_err("no rounds");
        assert!(err.contains("--rounds"), "{err}");
        let only_rounds = args_of(&["settle", "observe", "--rounds", "r.json"]);
        let err = observe_command(&only_rounds, &|_| None).expect_err("no observations");
        assert!(err.contains("--observations"), "{err}");
    }

    /// The lines of a rug that began after the close and was seen twice.
    fn confirmed_rug(close: i64) -> String {
        [
            calm(TOKEN, close + 3_600),
            rug(TOKEN, close + DAY),
            rug(TOKEN, close + DAY + 100),
        ]
        .iter()
        .map(|o| serde_json::to_string(o).expect("json") + "\n")
        .collect()
    }

    /// The command over real files: publishes a rug, publishes it once, keeps
    /// the row on a second run, and `verify` passes on the file.
    #[test]
    fn publish_writes_the_file_once_and_verify_reads_it() {
        let dir = temp_dir("publish");
        let now = secs(SystemTime::now()).expect("clock");
        let close = now - 3 * DAY;
        let rounds = dir.join("rounds.json");
        std::fs::write(
            &rounds,
            format!(
                r#"{{"rounds":[{{"id":"r1","close":{close},"coins":[{{"chain":"solana","token":"{TOKEN}"}},{{"chain":"solana","token":"coinB"}}]}}]}}"#
            ),
        )
        .expect("rounds");
        let observations = dir.join("observations.jsonl");
        std::fs::write(&observations, confirmed_rug(close)).expect("observations");
        let outcomes = dir.join("outcomes.json");
        let args = args_of(&[
            "settle",
            "publish",
            "--rounds",
            rounds.to_str().expect("p"),
            "--observations",
            observations.to_str().expect("p"),
            "--outcomes",
            outcomes.to_str().expect("p"),
        ]);
        let none = |_: &str| None;

        // A dry run writes nothing.
        let mut dry = args.clone();
        dry.push("--dry-run".to_owned());
        publish_command(&dry, &none).expect("dry");
        assert!(!outcomes.exists());

        publish_command(&args, &none).expect("publish");
        let first = std::fs::read_to_string(&outcomes).expect("file");
        let parsed = settle::parse_outcomes(&first).expect("parse");
        assert_eq!(parsed.outcomes.len(), 1, "coinB is not decidable yet");
        assert_eq!(parsed.outcomes[0].outcome, Outcome::Rugged);
        assert!(!dir.join("outcomes.json.tmp").exists(), "no temporary left");

        // A second run leaves the bytes as they were.
        publish_command(&args, &none).expect("again");
        assert_eq!(std::fs::read_to_string(&outcomes).expect("file"), first);

        let verify = args_of(&[
            "settle",
            "verify",
            "--outcomes",
            outcomes.to_str().expect("p"),
        ]);
        verify_command(&verify, &none).expect("verifies");

        // A row that says something its evidence does not is caught.
        let forged = first.replace("Rugged", "Stood");
        std::fs::write(&outcomes, forged).expect("forge");
        assert!(verify_command(&verify, &none).is_err());
    }

    /// With nothing to say yet, publish still leaves a file, so serve finds
    /// "nothing yet" rather than no file; and it refuses to overwrite a file
    /// it cannot read.
    #[test]
    fn publish_leaves_an_empty_file_and_never_overwrites_an_unreadable_one() {
        let dir = temp_dir("publish-empty");
        let rounds = dir.join("rounds.json");
        std::fs::write(&rounds, r#"{"rounds":[]}"#).expect("rounds");
        let outcomes = dir.join("outcomes.json");
        let args = args_of(&[
            "settle",
            "publish",
            "--rounds",
            rounds.to_str().expect("p"),
            "--observations",
            dir.join("none.jsonl").to_str().expect("p"),
            "--outcomes",
            outcomes.to_str().expect("p"),
        ]);
        let none = |_: &str| None;
        publish_command(&args, &none).expect("publish");
        let file = settle::parse_outcomes(&std::fs::read_to_string(&outcomes).expect("file"))
            .expect("parse");
        assert!(file.outcomes.is_empty());

        std::fs::write(&outcomes, "garbage").expect("garbage");
        assert!(publish_command(&args, &none).is_err());
        assert_eq!(std::fs::read_to_string(&outcomes).expect("file"), "garbage");
    }

    #[test]
    fn the_subcommand_is_required() {
        let err = run(&args_of(&["settle"])).expect_err("no subcommand");
        assert!(err.contains("observe"), "{err}");
        assert!(run(&args_of(&["settle", "--verify"])).is_err());
    }

    /// Each subcommand reaches its own command, not the usage line: with
    /// nothing given, each fails on the path it needs. (A dropped dispatch
    /// arm would answer with the usage for all three.)
    #[test]
    fn each_subcommand_reaches_its_own_command() {
        for sub in ["observe", "publish", "verify"] {
            let err = run(&args_of(&["settle", sub])).expect_err(sub);
            assert_ne!(err, USAGE, "{sub} fell through to the usage line");
            assert!(err.contains("PATH"), "{sub}: {err}");
        }
        assert_eq!(
            run(&args_of(&["settle", "nonsense"])),
            Err(USAGE.to_owned())
        );
    }

    /// `CantTell` tells the rule nothing even when the read is marked
    /// complete; a rug is a rug even when it is not; another level counts as
    /// no rug only when complete.
    #[test]
    fn a_read_tells_the_rule_only_what_it_established() {
        assert_eq!(obs(TOKEN, 1, Level::CantTell, true).reading(), None);
        assert_eq!(obs(TOKEN, 1, Level::CantTell, false).reading(), None);
        assert_eq!(
            obs(TOKEN, 1, Level::Rugged, false).reading(),
            Some(Reading::Rug)
        );
        assert_eq!(
            obs(TOKEN, 1, Level::NothingUglyYet, true).reading(),
            Some(Reading::NoRug)
        );
        assert_eq!(obs(TOKEN, 1, Level::NothingUglyYet, false).reading(), None);
    }

    /// Publish rewrites an existing file when a row is new, and leaves it
    /// byte for byte alone when nothing is.
    #[test]
    fn publish_adds_a_new_row_to_an_existing_file_and_leaves_it_alone_otherwise() {
        let dir = temp_dir("publish-grow");
        let now = secs(SystemTime::now()).expect("clock");
        let close = now - 3 * DAY;
        let rounds = dir.join("rounds.json");
        std::fs::write(
            &rounds,
            format!(
                r#"{{"rounds":[{{"id":"r1","close":{close},"coins":[{{"chain":"solana","token":"{TOKEN}"}}]}}]}}"#
            ),
        )
        .expect("rounds");
        let observations = dir.join("observations.jsonl");
        let outcomes = dir.join("outcomes.json");
        let args = args_of(&[
            "settle",
            "publish",
            "--rounds",
            rounds.to_str().expect("p"),
            "--observations",
            observations.to_str().expect("p"),
            "--outcomes",
            outcomes.to_str().expect("p"),
        ]);
        let none = |_: &str| None;
        let rows = |p: &std::path::Path| {
            settle::parse_outcomes(&std::fs::read_to_string(p).expect("file"))
                .expect("parse")
                .outcomes
                .len()
        };

        // Nothing to say yet: an empty file exists.
        publish_command(&args, &none).expect("empty");
        assert_eq!(rows(&outcomes), 0);

        // Nothing new: the file is not touched (a marker survives).
        let marked = format!("{}\n\n", std::fs::read_to_string(&outcomes).expect("file"));
        std::fs::write(&outcomes, &marked).expect("mark");
        publish_command(&args, &none).expect("nothing new");
        assert_eq!(std::fs::read_to_string(&outcomes).expect("file"), marked);

        // A rug arrives: the existing file gains the row.
        std::fs::write(&observations, confirmed_rug(close)).expect("observations");
        publish_command(&args, &none).expect("grown");
        assert_eq!(rows(&outcomes), 1);
    }

    #[test]
    fn a_missing_optional_file_is_none_and_a_directory_is_an_error() {
        let dir = temp_dir("optional");
        let missing = dir.join("nope");
        assert_eq!(read_optional(missing.to_str().expect("p")), Ok(None));
        assert!(read_optional(dir.to_str().expect("p")).is_err());
    }
}
