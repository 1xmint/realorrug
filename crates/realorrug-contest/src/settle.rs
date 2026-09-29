// SPDX-License-Identifier: Apache-2.0
//! The settlement rule, pure (design 0032 §12).
//!
//! # What this is
//!
//! [`derive`] turns a round's close and the observation reads recorded for one
//! coin into an [`Outcome`], or into nothing yet. The `realorrug settle`
//! command feeds it with the settlement job's own dated chain reads and
//! publishes what it returns as an [`OutcomesFile`]; `realorrug-serve` ingests
//! that file through the store's one write path. Nothing here reads a chain, a
//! file or a clock, so the same close, reads and `now` give the same outcome
//! on any machine, which is what lets [`rederive`] check a stored row from the
//! row alone.
//!
//! # The rule
//!
//! Design 0032 §2 and ADR 0041 decision 5, as code. The window is the
//! [`WINDOW_SECS`] after the round's close; a read is evidence about the window
//! only when it was taken inside it.
//!
//! * a `Rug` read inside the window settles [`Outcome::Rugged`], at once;
//! * with none, nothing is written until the window has closed (a coin that has
//!   not rugged *yet* has not stood);
//! * once it has closed, [`Outcome::Stood`] needs a `NoRug` read in every
//!   24-hour span of the window, and anything less is [`Outcome::Unresolved`]:
//!   a gap in the read history is not "no rug seen", it is no read (AGENTS.md
//!   §3 rule 8);
//! * before the round's close nothing settles, whatever was read.
//!
//! No model is consulted and none could move the result.

use serde::{Deserialize, Serialize};

use crate::calls::Outcome;

/// The rule's name, written on every outcome row it produces. A changed rule
/// gets a new name, so an old row is never read as the new rule's answer.
pub const RULE_VERSION: &str = "settle-1";

/// The settlement window after a round's close, in seconds: fourteen days
/// (design 0028 §2.4; it becomes three only if the replay set says so, and
/// that would be a new [`RULE_VERSION`]).
pub const WINDOW_SECS: i64 = 14 * SPAN_SECS;

/// One span of the window that `Stood` needs a read in, in seconds.
pub const SPAN_SECS: i64 = 24 * 60 * 60;

/// What one observation read found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    /// The read observed an extraction: the level reached `Rugged`.
    Rug,
    /// The read succeeded and observed none.
    NoRug,
}

/// One recorded observation: when it was taken (seconds since the epoch) and
/// what it found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Read {
    /// When the read was taken.
    pub at: i64,
    /// What it found.
    pub reading: Reading,
}

/// The instant the window closes.
#[must_use]
pub fn horizon(close: i64) -> i64 {
    close.saturating_add(WINDOW_SECS)
}

/// The reads that are evidence about the window, oldest first: those taken at
/// or after the close and at or before the horizon, once each.
fn in_window(close: i64, reads: &[Read]) -> Vec<Read> {
    let end = horizon(close);
    let mut kept: Vec<Read> = reads
        .iter()
        .copied()
        .filter(|r| r.at >= close && r.at <= end)
        .collect();
    kept.sort_by_key(|r| (r.at, r.reading == Reading::NoRug));
    kept.dedup();
    kept
}

/// Whether every 24-hour span of the window holds a `NoRug` read.
fn covered(close: i64, window: &[Read]) -> bool {
    (0..WINDOW_SECS / SPAN_SECS).all(|span| {
        let from = close.saturating_add(span * SPAN_SECS);
        let to = from.saturating_add(SPAN_SECS);
        window
            .iter()
            .any(|r| r.reading == Reading::NoRug && r.at >= from && r.at < to)
    })
}

/// The outcome the rule gives at `now`, or `None` when nothing may be written
/// yet.
///
/// Once it returns `Some`, the same reads give the same answer at any later
/// `now`: `Rugged` is decided by a read already taken, and the other two only
/// by reads inside a window that has already closed.
#[must_use]
pub fn derive(close: i64, reads: &[Read], now: i64) -> Option<Outcome> {
    if now < close {
        return None;
    }
    let window = in_window(close, reads);
    if window.iter().any(|r| r.reading == Reading::Rug) {
        return Some(Outcome::Rugged);
    }
    if now < horizon(close) {
        return None;
    }
    if covered(close, &window) {
        Some(Outcome::Stood)
    } else {
        Some(Outcome::Unresolved)
    }
}

/// The evidence reference an outcome row carries: the close and the reads it
/// rests on, in one line. The reads are pinned into the row itself because the
/// place they came from can be overwritten (a later label replaces an earlier
/// one), and a reference that can change under a settled row is not a
/// reference. Same inputs, same string.
#[must_use]
pub fn encode_evidence(close: i64, reads: &[Read]) -> String {
    let list: Vec<String> = in_window(close, reads)
        .iter()
        .map(|r| {
            let tag = match r.reading {
                Reading::Rug => 'R',
                Reading::NoRug => 'N',
            };
            format!("{}:{tag}", r.at)
        })
        .collect();
    format!("close={close};reads={}", list.join(","))
}

/// Reads an evidence reference back. `None` for anything [`encode_evidence`]
/// would not have written.
#[must_use]
pub fn decode_evidence(text: &str) -> Option<(i64, Vec<Read>)> {
    let (close, reads) = text.split_once(';')?;
    let close: i64 = close.strip_prefix("close=")?.parse().ok()?;
    let list = reads.strip_prefix("reads=")?;
    let mut decoded = Vec::new();
    if !list.is_empty() {
        for item in list.split(',') {
            let (at, tag) = item.split_once(':')?;
            let reading = match tag {
                "R" => Reading::Rug,
                "N" => Reading::NoRug,
                _ => return None,
            };
            decoded.push(Read {
                at: at.parse().ok()?,
                reading,
            });
        }
    }
    (encode_evidence(close, &decoded) == text).then_some((close, decoded))
}

/// Re-derives the outcome a stored row should hold, from the row alone: its
/// rule version, its evidence reference and the time it was written.
///
/// `None` when the row cannot be re-derived: a rule this code does not know,
/// no evidence reference, one that does not parse, or one that names a read
/// taken after the row was written. A caller treats `None` as a row it cannot
/// vouch for, never as agreement.
#[must_use]
pub fn rederive(
    rule_version: &str,
    evidence_reference: Option<&str>,
    settled_at: i64,
) -> Option<Outcome> {
    if rule_version != RULE_VERSION {
        return None;
    }
    let (close, reads) = decode_evidence(evidence_reference?)?;
    if reads.iter().any(|r| r.at > settled_at) {
        return None;
    }
    derive(close, &reads, settled_at)
}

/// Whether a coin is owed a chain read at `now`: its window is open, no read
/// has seen it rug, and the current 24-hour span holds no `NoRug` read yet.
///
/// This is what keeps the cost at one complete read per coin per day however
/// often the timer fires: the timer may run twice a day so that one missed or
/// late run cannot lose a span, and the second run of a day finds the span
/// already covered and reads nothing. An incomplete read is not a `Read` (the
/// caller drops it), so it leaves the span uncovered and the next run tries
/// again.
#[must_use]
pub fn due(close: i64, reads: &[Read], now: i64) -> bool {
    if now < close || now >= horizon(close) {
        return false;
    }
    let window = in_window(close, reads);
    if window.iter().any(|r| r.reading == Reading::Rug) {
        return false;
    }
    let from = close.saturating_add((now - close) / SPAN_SECS * SPAN_SECS);
    let to = from.saturating_add(SPAN_SECS);
    !window
        .iter()
        .any(|r| r.reading == Reading::NoRug && r.at >= from && r.at < to)
}

/// One published outcome: the row `realorrug-serve` will write, and everything
/// it needs to check the row before it does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomeRow {
    /// The round.
    pub round: String,
    /// The coin's chain, as the rounds file names it.
    pub chain: String,
    /// The coin's address.
    pub token: String,
    /// What the rule decided.
    pub outcome: Outcome,
    /// The rule that decided it ([`RULE_VERSION`]).
    pub rule_version: String,
    /// The close and the reads it rests on ([`encode_evidence`]).
    pub evidence_reference: String,
    /// When the settlement job decided it (seconds since the epoch).
    pub settled_at: i64,
}

impl OutcomeRow {
    /// Whether the row re-derives to its own outcome from its own rule version,
    /// evidence and `settled_at`. A row that does not is one nobody can vouch
    /// for: serve refuses it.
    #[must_use]
    pub fn rederives(&self) -> bool {
        rederive(
            &self.rule_version,
            Some(&self.evidence_reference),
            self.settled_at,
        ) == Some(self.outcome)
    }

    /// The round's close, as the row's own evidence states it.
    #[must_use]
    pub fn close(&self) -> Option<i64> {
        decode_evidence(&self.evidence_reference).map(|(close, _)| close)
    }
}

/// The published file: every outcome the settlement job has decided.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutcomesFile {
    /// The rows, in the order they were decided.
    pub outcomes: Vec<OutcomeRow>,
}

/// Reads a published outcomes file's text.
///
/// # Errors
///
/// A message when the text is not this shape. Nothing is guessed from a file
/// that does not parse: the caller ingests nothing from it.
pub fn parse_outcomes(text: &str) -> Result<OutcomesFile, String> {
    serde_json::from_str(text).map_err(|e| format!("the outcomes file is not valid: {e}"))
}

/// The published file's text for `file`. Same rows, same text.
///
/// # Errors
///
/// A message if the rows cannot be serialised.
pub fn render_outcomes(file: &OutcomesFile) -> Result<String, String> {
    serde_json::to_string_pretty(file).map_err(|e| format!("cannot write the outcomes file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOSE: i64 = 1_000_000;

    fn rug(at: i64) -> Read {
        Read {
            at,
            reading: Reading::Rug,
        }
    }

    fn calm(at: i64) -> Read {
        Read {
            at,
            reading: Reading::NoRug,
        }
    }

    /// One calm read in the middle of each of the fourteen spans.
    fn daily() -> Vec<Read> {
        (0..14)
            .map(|d| calm(CLOSE + d * SPAN_SECS + 3_600))
            .collect()
    }

    /// Nothing is settled before the close, whatever was read: a rug read
    /// stamped after the close does not settle a round that has not closed.
    #[test]
    fn nothing_settles_before_the_close() {
        assert_eq!(derive(CLOSE, &[rug(CLOSE + 5)], CLOSE - 1), None);
        assert_eq!(derive(CLOSE, &daily(), CLOSE - 1), None);
        assert_eq!(derive(CLOSE, &[], CLOSE - 1), None);
    }

    /// The close itself is the first instant a rug read can settle.
    #[test]
    fn a_rug_read_at_the_close_settles_rugged() {
        assert_eq!(derive(CLOSE, &[rug(CLOSE)], CLOSE), Some(Outcome::Rugged));
    }

    /// A rug is a rug on the day it is seen; the round does not wait for the
    /// horizon to say so.
    #[test]
    fn a_rug_settles_at_once_inside_the_window() {
        assert_eq!(
            derive(CLOSE, &[rug(CLOSE + 100)], CLOSE + 200),
            Some(Outcome::Rugged)
        );
        // A calm read beside it does not outvote it.
        assert_eq!(
            derive(CLOSE, &[calm(CLOSE + 50), rug(CLOSE + 100)], CLOSE + 200),
            Some(Outcome::Rugged)
        );
    }

    /// A read before the close, or after the horizon, is not about the window.
    #[test]
    fn a_rug_outside_the_window_is_not_a_rug_in_it() {
        let end = horizon(CLOSE);
        assert_eq!(end, CLOSE + WINDOW_SECS);
        assert_eq!(
            derive(CLOSE, &[rug(CLOSE - 1)], end),
            Some(Outcome::Unresolved)
        );
        assert_eq!(
            derive(CLOSE, &[rug(end + 1)], end + 2),
            Some(Outcome::Unresolved)
        );
        // The horizon instant itself is inside.
        assert_eq!(derive(CLOSE, &[rug(end)], end), Some(Outcome::Rugged));
    }

    /// No rug yet is not yet a stand: nothing is written before the horizon.
    #[test]
    fn no_rug_yet_writes_nothing_until_the_horizon() {
        let end = horizon(CLOSE);
        assert_eq!(derive(CLOSE, &daily(), end - 1), None);
        assert_eq!(derive(CLOSE, &[], end - 1), None);
        assert_eq!(derive(CLOSE, &daily(), end), Some(Outcome::Stood));
    }

    /// No evidence at all is `Unresolved`, never `Stood`.
    #[test]
    fn no_evidence_settles_unresolved() {
        assert_eq!(
            derive(CLOSE, &[], horizon(CLOSE)),
            Some(Outcome::Unresolved)
        );
    }

    /// `Stood` needs every span; one missing day is a gap, not a calm day.
    #[test]
    fn a_gap_in_the_read_history_is_unresolved_not_stood() {
        for missing in 0..14usize {
            let mut reads = daily();
            reads.remove(missing);
            assert_eq!(
                derive(CLOSE, &reads, horizon(CLOSE) + 1),
                Some(Outcome::Unresolved),
                "day {missing} missing"
            );
        }
    }

    /// Span edges: a read on the first instant of a span counts for that span,
    /// and a read on the last instant of one counts for it and not the next.
    #[test]
    fn span_edges_belong_to_the_span_they_open() {
        let mut reads: Vec<Read> = (0..14).map(|d| calm(CLOSE + d * SPAN_SECS)).collect();
        assert_eq!(derive(CLOSE, &reads, horizon(CLOSE)), Some(Outcome::Stood));
        reads = (0..14)
            .map(|d| calm(CLOSE + (d + 1) * SPAN_SECS - 1))
            .collect();
        assert_eq!(derive(CLOSE, &reads, horizon(CLOSE)), Some(Outcome::Stood));
        // All fourteen reads in the first span leave the other thirteen empty.
        let crowded: Vec<Read> = (0..14).map(|s| calm(CLOSE + s)).collect();
        assert_eq!(
            derive(CLOSE, &crowded, horizon(CLOSE)),
            Some(Outcome::Unresolved)
        );
        // A calm read on the horizon instant belongs to no span.
        let mut shifted = daily();
        shifted.remove(13);
        shifted.push(calm(horizon(CLOSE)));
        assert_eq!(
            derive(CLOSE, &shifted, horizon(CLOSE)),
            Some(Outcome::Unresolved)
        );
    }

    /// The order the reads arrive in, and a read repeated, change nothing.
    #[test]
    fn order_and_repeats_do_not_change_the_answer() {
        let mut shuffled = daily();
        shuffled.reverse();
        shuffled.push(calm(CLOSE + 3_600));
        assert_eq!(
            derive(CLOSE, &shuffled, horizon(CLOSE)),
            derive(CLOSE, &daily(), horizon(CLOSE))
        );
        assert_eq!(
            encode_evidence(CLOSE, &shuffled),
            encode_evidence(CLOSE, &daily())
        );
    }

    /// Two reads taken at the same instant that disagree are written in one
    /// fixed order, a rug first, whichever way they arrive, so the reference is
    /// one string and it decodes back.
    #[test]
    fn reads_at_the_same_instant_are_written_rug_first() {
        let at = CLOSE + 50;
        let expected = format!("close={CLOSE};reads={at}:R,{at}:N");
        assert_eq!(encode_evidence(CLOSE, &[rug(at), calm(at)]), expected);
        assert_eq!(encode_evidence(CLOSE, &[calm(at), rug(at)]), expected);
        assert_eq!(
            decode_evidence(&expected),
            Some((CLOSE, vec![rug(at), calm(at)]))
        );
    }

    /// Once decided the answer does not move as `now` advances.
    #[test]
    fn a_decided_outcome_is_stable_as_time_passes() {
        let cases: [Vec<Read>; 3] = [vec![rug(CLOSE + 10)], daily(), vec![]];
        for reads in cases {
            let first = derive(CLOSE, &reads, horizon(CLOSE));
            assert!(first.is_some());
            for later in [1, 60, SPAN_SECS, 400 * SPAN_SECS] {
                assert_eq!(derive(CLOSE, &reads, horizon(CLOSE) + later), first);
            }
        }
    }

    /// The evidence string is exact and round-trips.
    #[test]
    fn evidence_encodes_the_window_reads_and_reads_back() {
        let reads = [
            calm(CLOSE - 5),
            calm(CLOSE + 7),
            rug(CLOSE + 9),
            calm(horizon(CLOSE) + 1),
        ];
        let text = encode_evidence(CLOSE, &reads);
        assert_eq!(text, "close=1000000;reads=1000007:N,1000009:R");
        assert_eq!(
            decode_evidence(&text),
            Some((CLOSE, vec![calm(CLOSE + 7), rug(CLOSE + 9)]))
        );
        assert_eq!(encode_evidence(CLOSE, &[]), "close=1000000;reads=");
        assert_eq!(
            decode_evidence("close=1000000;reads="),
            Some((CLOSE, vec![]))
        );
    }

    /// A string the encoder would not have written is refused, including one
    /// that parses but is not in the canonical order.
    #[test]
    fn evidence_that_was_not_written_by_the_encoder_is_refused() {
        for bad in [
            "",
            "close=1",
            "close=x;reads=",
            "reads=;close=1",
            "close=1000000;reads=1000007:X",
            "close=1000000;reads=1000007",
            "close=1000000;reads=abc:N",
            "close=1000000;reads=1000009:R,1000007:N",
            "close=1000000;reads=1000007:N,1000007:N",
            "close=1000000;reads=5:N",
            "close=1000000;reads=,",
            "close=1000000;reads=1000007:N;extra",
        ] {
            assert_eq!(decode_evidence(bad), None, "{bad:?}");
        }
    }

    /// A row re-derives to the value it was written with, from the row alone.
    #[test]
    fn rederive_reproduces_the_derived_outcome() {
        let end = horizon(CLOSE);
        let cases: [(Vec<Read>, i64); 3] = [
            (vec![rug(CLOSE + 10)], CLOSE + 20),
            (daily(), end),
            (vec![], end + 999),
        ];
        for (reads, now) in cases {
            let derived = derive(CLOSE, &reads, now).expect("decided");
            let evidence = encode_evidence(CLOSE, &reads);
            assert_eq!(rederive(RULE_VERSION, Some(&evidence), now), Some(derived));
        }
    }

    /// A row it cannot vouch for is `None`, not agreement.
    #[test]
    fn rederive_refuses_what_it_cannot_reproduce() {
        let evidence = encode_evidence(CLOSE, &[rug(CLOSE + 10)]);
        assert_eq!(
            rederive(RULE_VERSION, Some(&evidence), CLOSE + 20),
            Some(Outcome::Rugged)
        );
        assert_eq!(rederive("settle-0", Some(&evidence), CLOSE + 20), None);
        assert_eq!(rederive(RULE_VERSION, None, CLOSE + 20), None);
        assert_eq!(rederive(RULE_VERSION, Some("nonsense"), CLOSE + 20), None);
        // A read newer than the row that claims to rest on it.
        assert_eq!(rederive(RULE_VERSION, Some(&evidence), CLOSE + 9), None);
        // A read exactly as old as the row is fine.
        assert_eq!(
            rederive(RULE_VERSION, Some(&evidence), CLOSE + 10),
            Some(Outcome::Rugged)
        );
        // Written before the close: nothing to say.
        assert_eq!(
            rederive(RULE_VERSION, Some(&encode_evidence(CLOSE, &[])), CLOSE - 1),
            None
        );
    }

    /// The published constants are what the docs say.
    #[test]
    fn the_window_is_fourteen_days_of_twenty_four_hours() {
        assert_eq!(SPAN_SECS, 86_400);
        assert_eq!(WINDOW_SECS, 1_209_600);
        assert_eq!(RULE_VERSION, "settle-1");
        assert_eq!(horizon(i64::MAX), i64::MAX, "no overflow near the top");
    }

    /// A coin is read only while its window is open: not before the close and
    /// not from the horizon on.
    #[test]
    fn a_coin_is_due_only_inside_its_window() {
        assert!(!due(CLOSE, &[], CLOSE - 1));
        assert!(due(CLOSE, &[], CLOSE));
        assert!(due(CLOSE, &[], horizon(CLOSE) - 1));
        assert!(!due(CLOSE, &[], horizon(CLOSE)));
    }

    /// Once the day's span holds a complete read, the day's second run reads
    /// nothing; the next span is owed one again.
    #[test]
    fn a_covered_span_is_not_read_twice() {
        let reads = [calm(CLOSE + 3_600)];
        assert!(!due(CLOSE, &reads, CLOSE + 7_200));
        assert!(!due(CLOSE, &reads, CLOSE + SPAN_SECS - 1));
        assert!(due(CLOSE, &reads, CLOSE + SPAN_SECS));
        // A read in another span does not cover this one.
        assert!(due(
            CLOSE,
            &[calm(CLOSE + 3_600)],
            CLOSE + 2 * SPAN_SECS + 5
        ));
    }

    /// A coin that has been seen to rug needs no more reads.
    #[test]
    fn a_rugged_coin_is_not_read_again() {
        assert!(!due(CLOSE, &[rug(CLOSE + 10)], CLOSE + 5 * SPAN_SECS));
        // A rug read from before the close is not a rug in the window.
        assert!(due(CLOSE, &[rug(CLOSE - 10)], CLOSE + 5));
    }

    /// The last span is read too: the horizon-end pass is the daily one.
    #[test]
    fn the_last_span_is_owed_a_read() {
        let mut reads = daily();
        reads.pop();
        assert!(due(CLOSE, &reads, CLOSE + 13 * SPAN_SECS + 100));
    }

    fn row(outcome: Outcome, evidence: &str, at: i64) -> OutcomeRow {
        OutcomeRow {
            round: "r".to_owned(),
            chain: "solana".to_owned(),
            token: "t".to_owned(),
            outcome,
            rule_version: RULE_VERSION.to_owned(),
            evidence_reference: evidence.to_owned(),
            settled_at: at,
        }
    }

    /// A published row vouches for itself only when its own evidence gives its
    /// own outcome.
    #[test]
    fn a_row_rederives_only_to_its_own_outcome() {
        let evidence = encode_evidence(CLOSE, &[rug(CLOSE + 5)]);
        let good = row(Outcome::Rugged, &evidence, CLOSE + 10);
        assert!(good.rederives());
        assert_eq!(good.close(), Some(CLOSE));
        assert!(!row(Outcome::Stood, &evidence, CLOSE + 10).rederives());
        assert!(
            !OutcomeRow {
                rule_version: "settle-0".to_owned(),
                ..good.clone()
            }
            .rederives()
        );
        let bad = row(Outcome::Rugged, "not evidence", CLOSE + 10);
        assert!(!bad.rederives());
        assert_eq!(bad.close(), None);
    }

    /// The file round-trips, and a file of any other shape reads as nothing.
    #[test]
    fn the_outcomes_file_round_trips_and_refuses_other_shapes() {
        let file = OutcomesFile {
            outcomes: vec![row(Outcome::Unresolved, &encode_evidence(CLOSE, &[]), 1)],
        };
        let text = render_outcomes(&file).expect("render");
        assert_eq!(parse_outcomes(&text).expect("parse"), file);
        assert!(parse_outcomes("[]").is_err());
        assert!(parse_outcomes(r#"{"outcomes":[],"extra":1}"#).is_err());
        assert!(parse_outcomes("").is_err());
    }
}
