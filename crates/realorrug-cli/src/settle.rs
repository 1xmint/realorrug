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
//! `Rugged` is decisive whatever else was unread. Any other level counts as a
//! complete read only when it is not `CantTell` — `CantTell` is the ladder
//! saying a fact it needed could not be read — so an incomplete read is
//! recorded (for the audit) and never covers a 24-hour span (AGENTS.md section
//! 3 rule 8: unknown is not safe).
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
    /// The rule the reading is meant for ([`settle::RULE_VERSION`]).
    rule_version: String,
    /// A digest of the fact sheet the level was computed from, or `unreadable`
    /// when no sheet could be built.
    evidence_reference: String,
    /// RPC calls the read cost.
    calls: u32,
}

impl Observation {
    /// What this read tells the rule, if it tells it anything: a rug when the
    /// level is `Rugged`, no rug only when the read was complete, and nothing
    /// otherwise.
    fn reading(&self) -> Option<Reading> {
        match self.level {
            Level::Rugged => Some(Reading::Rug),
            Level::CantTell => None,
            _ => self.complete.then_some(Reading::NoRug),
        }
    }
}

/// Reads the observations file's text: one JSON object a line, blank lines
/// ignored. A line that does not parse is an error naming the line, and
/// nothing is settled from a file that cannot be read whole, since dropping a
/// line could drop a rug.
fn parse_observations(text: &str) -> Result<Vec<Observation>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let obs = serde_json::from_str(line)
            .map_err(|e| format!("the observations file is not valid at line {}: {e}", i + 1))?;
        out.push(obs);
    }
    Ok(out)
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

/// Reads every coin that is owed a read at `now` and hands each dated
/// observation to `sink`. `read` is the chain; `limit` is the run's ceiling.
fn observe_all(
    rounds: &[Round],
    observations: &[Observation],
    published: &[OutcomeRow],
    now: i64,
    limit: usize,
    read: &mut dyn FnMut(&str, &str) -> Result<Sample, String>,
    sink: &mut dyn FnMut(&Observation) -> Result<(), String>,
) -> Result<ObserveReport, String> {
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
                    at: now,
                    level: sample.level,
                    complete: sample.level != Level::CantTell,
                    rule_version: settle::RULE_VERSION.to_owned(),
                    evidence_reference: sample.evidence,
                    calls: sample.calls,
                },
                // The reader's reason is not kept: it can carry an endpoint.
                Err(_) => Observation {
                    round: round.id.clone(),
                    chain: coin.chain.clone(),
                    token: coin.token.clone(),
                    at: now,
                    level: Level::CantTell,
                    complete: false,
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
    let observations = read_optional(observations_path)?
        .map(|text| parse_observations(&text))
        .transpose()?
        .unwrap_or_default();
    Ok((rounds, observations))
}

/// `settle observe`.
fn observe_command(args: &[String], get: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
    let (rounds_path, observations_path) = input_paths(args, get)?;
    let Some(rpc) = rpc_url(flag(args, "--rpc"), get) else {
        println!("no RPC is configured (--rpc or REALORRUG_RPC): nothing was read");
        return Ok(());
    };
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
        let sheet = FactSheet::build(
            &dossier,
            rates.as_ref(),
            creators.as_ref(),
            self_mint.as_ref(),
            None,
        );
        Ok(Sample {
            level: realorrug_roast::level(&sheet),
            evidence: sheet_reference(&sheet),
            calls: dossier.calls,
        })
    };

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&observations_path)
        .map_err(|e| format!("cannot open {observations_path}: {e}"))?;
    let mut sink = |o: &Observation| -> Result<(), String> {
        let line = serde_json::to_string(o).map_err(|e| format!("cannot write a line: {e}"))?;
        writeln!(file, "{line}")
            .and_then(|()| file.flush())
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
        now,
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
            rule_version: settle::RULE_VERSION.to_owned(),
            evidence_reference: "sheet-blake3:x".to_owned(),
            calls: 7,
        }
    }

    fn calm(token: &str, at: i64) -> Observation {
        obs(token, at, Level::NothingUglyYet, true)
    }

    /// One complete calm read in the middle of each of the fourteen spans.
    fn daily(token: &str) -> Vec<Observation> {
        (0..14)
            .map(|d| calm(token, CLOSE + d * DAY + 3_600))
            .collect()
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

    /// A rug seen inside the window settles the coin `Rugged` at once, and the
    /// row it publishes re-derives to that from itself.
    #[test]
    fn a_rug_observed_inside_the_window_settles_rugged() {
        let observations = vec![
            calm(TOKEN, CLOSE + 3_600),
            obs(TOKEN, CLOSE + 2 * DAY, Level::Rugged, true),
        ];
        let decision = decide(&one_round(), &observations, &[], CLOSE + 3 * DAY);
        assert_eq!(outcome_of(&decision), Some(Outcome::Rugged));
        assert!(decision.added[0].rederives());
        assert_eq!(decision.pending, 0);
    }

    /// A rug read is decisive even when the read was flagged incomplete.
    #[test]
    fn a_rugged_level_counts_however_the_read_is_flagged() {
        let observations = vec![obs(TOKEN, CLOSE + DAY, Level::Rugged, false)];
        let decision = decide(&one_round(), &observations, &[], CLOSE + 2 * DAY);
        assert_eq!(outcome_of(&decision), Some(Outcome::Rugged));
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
        let early = vec![obs(TOKEN, CLOSE - 5, Level::Rugged, true)];
        assert!(
            decide(&one_round(), &early, &[], CLOSE - 1)
                .added
                .is_empty()
        );
        // The same coin, rugged: written at once.
        let mut rugged = calm_only;
        rugged.push(obs(TOKEN, CLOSE + 5 * DAY, Level::Rugged, true));
        let decision = decide(&one_round(), &rugged, &[], end() - 1);
        assert_eq!(outcome_of(&decision), Some(Outcome::Rugged));
    }

    /// Fourteen complete daily reads, and the horizon, make `Stood`.
    #[test]
    fn a_full_history_stands_at_the_horizon() {
        let decision = decide(&one_round(), &daily(TOKEN), &[], end());
        assert_eq!(outcome_of(&decision), Some(Outcome::Stood));
        assert!(decision.added[0].rederives());
        assert_eq!(decision.added[0].settled_at, end());
    }

    /// A missing 24-hour span is a gap in the read history, which is no read:
    /// `Unresolved`, never `Stood`.
    #[test]
    fn a_missing_span_is_unresolved_not_stood() {
        for missing in 0..14usize {
            let mut observations = daily(TOKEN);
            observations.remove(missing);
            let decision = decide(&one_round(), &observations, &[], end());
            assert_eq!(
                outcome_of(&decision),
                Some(Outcome::Unresolved),
                "day {missing} missing"
            );
        }
        // No history at all, after the horizon: unresolved, not absent.
        let decision = decide(&one_round(), &[], &[], end() + DAY);
        assert_eq!(outcome_of(&decision), Some(Outcome::Unresolved));
    }

    /// A read that could not see everything it needed does not cover a span.
    #[test]
    fn an_incomplete_read_does_not_cover_its_span() {
        let mut observations = daily(TOKEN);
        observations[6] = obs(TOKEN, CLOSE + 6 * DAY + 3_600, Level::CantTell, false);
        let decision = decide(&one_round(), &observations, &[], end());
        assert_eq!(outcome_of(&decision), Some(Outcome::Unresolved));
        // A level other than CantTell that the read itself flagged incomplete
        // is also not a complete read.
        let mut flagged = daily(TOKEN);
        flagged[3].complete = false;
        let decision = decide(&one_round(), &flagged, &[], end());
        assert_eq!(outcome_of(&decision), Some(Outcome::Unresolved));
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
        let observations = vec![obs(TOKEN, CLOSE + DAY, Level::Rugged, true)];
        let first = decide(&one_round(), &observations, &[], CLOSE + 2 * DAY);
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
                evidence: "sheet-blake3:e".to_owned(),
                calls: 9,
            })
        }
    }

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
            now,
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
        assert_eq!(o.rule_version, settle::RULE_VERSION);
        assert_eq!(o.evidence_reference, "sheet-blake3:e");
        assert_eq!((o.round.as_str(), o.token.as_str()), ("r1", TOKEN));
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
                evidence: "e".to_owned(),
                calls: 1,
            })
        };
        // Before the close and from the horizon on: no window.
        for now in [CLOSE - 1, end(), end() + DAY] {
            let (report, written) = run_observe(&one_round(), &[], &[], now, 10, &mut counting);
            assert_eq!((report.read, report.skipped), (0, 1), "now {now}");
            assert!(written.is_empty());
        }
        // Covered today.
        let covered = vec![calm(TOKEN, CLOSE + 100)];
        let (report, _) = run_observe(&one_round(), &covered, &[], CLOSE + 200, 10, &mut counting);
        assert_eq!((report.read, report.skipped), (0, 1));
        // Rugged (observed, not yet published).
        let rugged = vec![obs(TOKEN, CLOSE + 100, Level::Rugged, true)];
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
        assert_eq!(calls, 1);
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
            CLOSE + 5,
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
        assert_eq!(parse_observations(&text).expect("parse").len(), 2);
        assert!(parse_observations("").expect("empty").is_empty());
        let err = parse_observations(&format!("{good}\nnot json\n")).expect_err("bad");
        assert!(err.contains("line 2"), "{err}");
        let extra = good.replace('}', r#","extra":1}"#);
        assert!(parse_observations(&extra).is_err());
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
        let line =
            serde_json::to_string(&obs(TOKEN, close + DAY, Level::Rugged, true)).expect("json");
        std::fs::write(&observations, format!("{line}\n")).expect("observations");
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

    #[test]
    fn a_missing_optional_file_is_none_and_a_directory_is_an_error() {
        let dir = temp_dir("optional");
        let missing = dir.join("nope");
        assert_eq!(read_optional(missing.to_str().expect("p")), Ok(None));
        assert!(read_optional(dir.to_str().expect("p")).is_err());
    }
}
