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
//! Design 0032 §2 and ADR 0041 decision 5, as code, with the two tightenings
//! design 0032 §12 records. The window is the [`WINDOW_SECS`] after the
//! round's close; a read is evidence about the window only when it was taken
//! inside it. What counts as a rug is not decided here: a read is a
//! [`Reading::Rug`] only when the caller says so (the sheet's level reached
//! `Rugged`), which is the one seam that changes when the owner decides.
//!
//! * [`Outcome::Rugged`] needs a first in-window read that was *not* a rug
//!   (so the rug began after the entry window closed, not before it, where
//!   players could see it) and then two rug reads in a row, taken at different
//!   times (a level is a state, and one read of it can be a launch-block
//!   glitch);
//! * with no such pair, nothing is written until the window has closed (a coin
//!   that has not rugged *yet* has not stood);
//! * [`Outcome::Stood`] needs a `NoRug` read in every 24-hour span of the
//!   window, no rug read in it at all, and a `NoRug` read at or just after the
//!   horizon (design 0032 §9: once a day, plus once at the horizon), so the
//!   last day is read too;
//! * [`Outcome::Unresolved`] is what is left once the horizon read's grace
//!   ([`HORIZON_GRACE_SECS`]) has passed without `Stood`: a gap in the read
//!   history is not "no rug seen", it is no read (AGENTS.md §3 rule 8);
//! * before the round's close nothing settles, whatever was read.
//!
//! No model is consulted and none could move the result.

use serde::{Deserialize, Serialize};

use crate::calls::Outcome;

/// The rule's name, written on every outcome row it produces. A changed rule
/// gets a new name, so an old row is never read as the new rule's answer.
/// `settle-2` added the horizon read and the two-read, after-the-close
/// conditions on `Rugged`; nothing was ever settled under `settle-1`.
pub const RULE_VERSION: &str = "settle-2";

/// The settlement window after a round's close, in seconds: fourteen days
/// (design 0028 §2.4; it becomes three only if the replay set says so, and
/// that would be a new [`RULE_VERSION`]).
pub const WINDOW_SECS: i64 = 14 * SPAN_SECS;

/// One span of the window that `Stood` needs a read in, in seconds.
pub const SPAN_SECS: i64 = 24 * 60 * 60;

/// How long after the horizon the read that closes the last day may still be
/// taken. Twice the timer's interval, so a missed run does not cost it.
pub const HORIZON_GRACE_SECS: i64 = SPAN_SECS;

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

/// The last instant a horizon read may be taken at.
#[must_use]
pub fn final_read_by(close: i64) -> i64 {
    horizon(close).saturating_add(HORIZON_GRACE_SECS)
}

/// The reads that are evidence, oldest first: those taken at or after the
/// close and no later than the end of the horizon's grace, once each.
fn evidence_reads(close: i64, reads: &[Read]) -> Vec<Read> {
    let end = final_read_by(close);
    let mut kept: Vec<Read> = reads
        .iter()
        .copied()
        .filter(|r| r.at >= close && r.at <= end)
        .collect();
    kept.sort_by_key(|r| (r.at, r.reading == Reading::NoRug));
    kept.dedup();
    kept
}

/// The evidence reads taken inside the window itself (before the horizon).
fn inside(close: i64, evidence: &[Read]) -> Vec<Read> {
    let end = horizon(close);
    evidence.iter().copied().filter(|r| r.at < end).collect()
}

/// Whether the window's reads show a rug that began after the entry window
/// closed and was seen twice in a row.
///
/// The first read must be calm: a coin first seen rugged might have rugged
/// before the close, where players could see it, and a level is a state, not
/// an event time. Two consecutive rug reads at different times, not one: the
/// sheet's own twin (`synthetic_ladder.rs`) warns that a zero-reserve read can
/// be a graduation block.
fn rugged(window: &[Read]) -> bool {
    window
        .first()
        .is_some_and(|first| first.reading == Reading::NoRug)
        && window
            .windows(2)
            .any(|pair| pair[0].reading == Reading::Rug && pair[1].reading == Reading::Rug)
}

/// Whether the window holds any rug read at all (which bars `Stood`, even an
/// unconfirmed one).
fn rug_seen(window: &[Read]) -> bool {
    window.iter().any(|r| r.reading == Reading::Rug)
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

/// Whether a `NoRug` read was taken at or after the horizon, inside the grace.
/// A rug read there is no evidence either way (it cannot be dated into the
/// window), so it is not a horizon read.
fn horizon_read(close: i64, evidence: &[Read]) -> bool {
    let end = horizon(close);
    evidence
        .iter()
        .any(|r| r.reading == Reading::NoRug && r.at >= end)
}

/// The outcome the rule gives at `now`, or `None` when nothing may be written
/// yet.
///
/// Once it returns `Some`, the same reads give the same answer at any later
/// `now`: `Rugged` is decided by reads already taken, and the other two only
/// by reads inside a period that has already ended.
#[must_use]
pub fn derive(close: i64, reads: &[Read], now: i64) -> Option<Outcome> {
    if now < close {
        return None;
    }
    let evidence = evidence_reads(close, reads);
    let window = inside(close, &evidence);
    if rugged(&window) {
        return Some(Outcome::Rugged);
    }
    if now < horizon(close) {
        return None;
    }
    if !rug_seen(&window) && covered(close, &window) && horizon_read(close, &evidence) {
        return Some(Outcome::Stood);
    }
    (now >= final_read_by(close)).then_some(Outcome::Unresolved)
}

/// The evidence reference an outcome row carries: the close and the reads it
/// rests on, in one line. The reads are pinned into the row itself because the
/// place they came from can be overwritten (a later label replaces an earlier
/// one), and a reference that can change under a settled row is not a
/// reference. Same inputs, same string.
#[must_use]
pub fn encode_evidence(close: i64, reads: &[Read]) -> String {
    let list: Vec<String> = evidence_reads(close, reads)
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

/// Whether a coin is owed a chain read at `now`.
///
/// * Inside the window: unless it has been settled `Rugged`, when the current
///   24-hour span holds no `NoRug` read yet, or when its latest read is a rug
///   that a second read would confirm (or clear).
/// * From the horizon to the end of its grace: when `Stood` is still possible
///   (every span covered, no rug read) and no `NoRug` read has been taken at
///   or after the horizon.
/// * Never before the close, and never after the grace.
///
/// This is what keeps the cost at one complete read per coin per day however
/// often the timer fires: the timer may run twice a day so that one missed or
/// late run cannot lose a span, and the second run of a day finds the span
/// already covered and reads nothing. An incomplete read is not a `Read` (the
/// caller drops it), so it leaves the span uncovered and the next run tries
/// again.
#[must_use]
pub fn due(close: i64, reads: &[Read], now: i64) -> bool {
    if now < close || now >= final_read_by(close) {
        return false;
    }
    let evidence = evidence_reads(close, reads);
    let window = inside(close, &evidence);
    // Settled `Rugged`, or first seen already rugged: no later read could
    // settle it either way, so it is owed none.
    if rugged(&window) || window.first().is_some_and(|f| f.reading == Reading::Rug) {
        return false;
    }
    if now >= horizon(close) {
        return !rug_seen(&window) && covered(close, &window) && !horizon_read(close, &evidence);
    }
    let confirmation_owed = window
        .first()
        .zip(window.last())
        .is_some_and(|(first, last)| {
            first.reading == Reading::NoRug && last.reading == Reading::Rug
        });
    let from = close.saturating_add((now - close) / SPAN_SECS * SPAN_SECS);
    let to = from.saturating_add(SPAN_SECS);
    confirmation_owed
        || !window
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

    /// The fourteen daily reads and the read just after the horizon.
    fn full() -> Vec<Read> {
        let mut reads = daily();
        reads.push(calm(horizon(CLOSE) + 60));
        reads
    }

    /// A calm first read, then two rug reads in a row: the one shape of rug.
    fn confirmed_rug() -> Vec<Read> {
        vec![calm(CLOSE + 10), rug(CLOSE + 100), rug(CLOSE + 200)]
    }

    /// Nothing is settled before the close, whatever was read: a rug read
    /// stamped after the close does not settle a round that has not closed.
    #[test]
    fn nothing_settles_before_the_close() {
        assert_eq!(derive(CLOSE, &confirmed_rug(), CLOSE - 1), None);
        assert_eq!(derive(CLOSE, &full(), CLOSE - 1), None);
        assert_eq!(derive(CLOSE, &[], CLOSE - 1), None);
        // The close instant itself is the first at which the rule applies. (The
        // caller never hands it a read from after `now`; the rule is a function
        // of the reads it is given, so the boundary is tested with them.)
        assert_eq!(
            derive(CLOSE, &confirmed_rug(), CLOSE),
            Some(Outcome::Rugged)
        );
    }

    /// A rug is a rug on the day it is confirmed; the round does not wait for
    /// the horizon to say so.
    #[test]
    fn a_confirmed_rug_settles_at_once_inside_the_window() {
        assert_eq!(
            derive(CLOSE, &confirmed_rug(), CLOSE + 300),
            Some(Outcome::Rugged)
        );
        // The second rug read is the one that decides: before it, nothing.
        assert_eq!(derive(CLOSE, &confirmed_rug()[..2], CLOSE + 300), None);
    }

    /// One rug read may be a graduation block: it never settles by itself,
    /// not even at the end, when the coin is `Unresolved`, not `Stood`.
    #[test]
    fn a_single_rug_read_never_settles_rugged() {
        let mut reads = daily();
        reads.push(rug(CLOSE + 5 * SPAN_SECS + 100));
        reads.push(calm(horizon(CLOSE) + 60));
        assert_eq!(derive(CLOSE, &reads, horizon(CLOSE) + 61), None);
        assert_eq!(
            derive(CLOSE, &reads, final_read_by(CLOSE)),
            Some(Outcome::Unresolved)
        );
        // Two rug reads with a calm one between them are not "in a row".
        let apart = [
            calm(CLOSE + 10),
            rug(CLOSE + 100),
            calm(CLOSE + 150),
            rug(CLOSE + 200),
        ];
        assert_eq!(derive(CLOSE, &apart, CLOSE + 300), None);
    }

    /// A coin first seen already rugged may have rugged before the close,
    /// where the entry window could see it: it is never `Rugged`, and it is
    /// `Unresolved` once the grace has passed.
    #[test]
    fn a_coin_already_rugged_at_the_close_is_unresolved() {
        let reads = [rug(CLOSE + 10), rug(CLOSE + 100), rug(CLOSE + 200)];
        assert_eq!(derive(CLOSE, &reads, CLOSE + 300), None);
        assert_eq!(derive(CLOSE, &reads, horizon(CLOSE)), None);
        assert_eq!(
            derive(CLOSE, &reads, final_read_by(CLOSE)),
            Some(Outcome::Unresolved)
        );
        // It is owed no more reads: nothing they could show would settle it.
        assert!(!due(CLOSE, &reads[..1], CLOSE + SPAN_SECS));
    }

    /// Two rug reads at one instant are one read (a repeated line), not a
    /// confirmation from a later run.
    #[test]
    fn a_repeated_rug_read_is_not_a_confirmation() {
        let reads = [calm(CLOSE + 10), rug(CLOSE + 100), rug(CLOSE + 100)];
        assert_eq!(derive(CLOSE, &reads, CLOSE + 300), None);
    }

    /// A read before the close is not about the window, and a rug read after
    /// the horizon cannot be dated into it.
    #[test]
    fn a_rug_outside_the_window_is_not_a_rug_in_it() {
        let end = horizon(CLOSE);
        assert_eq!(end, CLOSE + WINDOW_SECS);
        let early = [rug(CLOSE - 2), rug(CLOSE - 1), calm(CLOSE + 5)];
        assert_eq!(derive(CLOSE, &early, CLOSE + 10), None);
        let mut late = daily();
        late.push(rug(end));
        late.push(rug(end + 1));
        assert_eq!(derive(CLOSE, &late, end + 2), None);
        assert_eq!(
            derive(CLOSE, &late, final_read_by(CLOSE)),
            Some(Outcome::Unresolved)
        );
    }

    /// A rug read at the horizon instant belongs to the read that closes the
    /// last day, not to the window: a rug seen once inside it and once at the
    /// horizon is one rug in the window, which is not confirmed.
    #[test]
    fn the_window_ends_before_the_read_at_the_horizon() {
        let end = horizon(CLOSE);
        let reads = [calm(CLOSE + 10), rug(end - 1), rug(end)];
        assert_eq!(derive(CLOSE, &reads, end), None);
        assert_eq!(
            derive(CLOSE, &reads, final_read_by(CLOSE)),
            Some(Outcome::Unresolved)
        );
    }

    /// No rug yet is not yet a stand: nothing is written before the horizon,
    /// and the daily reads alone do not make `Stood`; the horizon read does.
    #[test]
    fn stood_needs_the_read_at_the_horizon() {
        let end = horizon(CLOSE);
        assert_eq!(derive(CLOSE, &full(), end - 1), None);
        assert_eq!(derive(CLOSE, &[], end - 1), None);
        assert_eq!(derive(CLOSE, &daily(), end), None);
        assert_eq!(derive(CLOSE, &daily(), final_read_by(CLOSE) - 1), None);
        assert_eq!(
            derive(CLOSE, &daily(), final_read_by(CLOSE)),
            Some(Outcome::Unresolved)
        );
        assert_eq!(derive(CLOSE, &full(), end + 61), Some(Outcome::Stood));
        // The read exactly at the horizon and exactly at the grace's end count.
        for at in [end, final_read_by(CLOSE)] {
            let mut reads = daily();
            reads.push(calm(at));
            assert_eq!(derive(CLOSE, &reads, at), Some(Outcome::Stood), "at {at}");
        }
        // One second past the grace is too late.
        let mut late = daily();
        late.push(calm(final_read_by(CLOSE) + 1));
        assert_eq!(
            derive(CLOSE, &late, final_read_by(CLOSE) + 1),
            Some(Outcome::Unresolved)
        );
    }

    /// A rug read anywhere in the window bars `Stood`, confirmed or not.
    #[test]
    fn an_unconfirmed_rug_read_bars_stood() {
        let mut reads = full();
        reads.push(rug(CLOSE + 7 * SPAN_SECS + 9));
        assert_eq!(
            derive(CLOSE, &reads, final_read_by(CLOSE)),
            Some(Outcome::Unresolved)
        );
    }

    /// No evidence at all is `Unresolved` once the grace has passed, never
    /// `Stood`.
    #[test]
    fn no_evidence_settles_unresolved() {
        assert_eq!(derive(CLOSE, &[], horizon(CLOSE)), None);
        assert_eq!(
            derive(CLOSE, &[], final_read_by(CLOSE)),
            Some(Outcome::Unresolved)
        );
    }

    /// `Stood` needs every span; one missing day is a gap, not a calm day.
    #[test]
    fn a_gap_in_the_read_history_is_unresolved_not_stood() {
        for missing in 0..14usize {
            let mut reads = full();
            reads.remove(missing);
            assert_eq!(
                derive(CLOSE, &reads, final_read_by(CLOSE)),
                Some(Outcome::Unresolved),
                "day {missing} missing"
            );
        }
    }

    /// Span edges: a read on the first instant of a span counts for that span,
    /// and a read on the last instant of one counts for it and not the next.
    #[test]
    fn span_edges_belong_to_the_span_they_open() {
        let horizon_read = calm(horizon(CLOSE));
        let mut reads: Vec<Read> = (0..14).map(|d| calm(CLOSE + d * SPAN_SECS)).collect();
        reads.push(horizon_read);
        assert_eq!(derive(CLOSE, &reads, horizon(CLOSE)), Some(Outcome::Stood));
        reads = (0..14)
            .map(|d| calm(CLOSE + (d + 1) * SPAN_SECS - 1))
            .collect();
        reads.push(horizon_read);
        assert_eq!(derive(CLOSE, &reads, horizon(CLOSE)), Some(Outcome::Stood));
        // All fourteen reads in the first span leave the other thirteen empty.
        let mut crowded: Vec<Read> = (0..14).map(|s| calm(CLOSE + s)).collect();
        crowded.push(horizon_read);
        assert_eq!(
            derive(CLOSE, &crowded, final_read_by(CLOSE)),
            Some(Outcome::Unresolved)
        );
        // A read on the instant a span ends belongs to the next span, not to
        // both: day 3's read is missing and day 4's sits on the seam.
        let mut seam = daily();
        seam.remove(3);
        seam[3] = calm(CLOSE + 4 * SPAN_SECS);
        seam.push(horizon_read);
        assert_eq!(
            derive(CLOSE, &seam, final_read_by(CLOSE)),
            Some(Outcome::Unresolved)
        );
        // A calm read on the horizon instant belongs to no span: it does not
        // stand in for the last day.
        let mut shifted = daily();
        shifted.remove(13);
        shifted.push(horizon_read);
        assert_eq!(
            derive(CLOSE, &shifted, final_read_by(CLOSE)),
            Some(Outcome::Unresolved)
        );
    }

    /// The order the reads arrive in, and a read repeated, change nothing.
    #[test]
    fn order_and_repeats_do_not_change_the_answer() {
        let mut shuffled = full();
        shuffled.reverse();
        shuffled.push(calm(CLOSE + 3_600));
        assert_eq!(
            derive(CLOSE, &shuffled, final_read_by(CLOSE)),
            derive(CLOSE, &full(), final_read_by(CLOSE))
        );
        assert_eq!(
            encode_evidence(CLOSE, &shuffled),
            encode_evidence(CLOSE, &full())
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
        let cases: [Vec<Read>; 3] = [confirmed_rug(), full(), vec![]];
        for reads in cases {
            let first = derive(CLOSE, &reads, final_read_by(CLOSE));
            assert!(first.is_some());
            for later in [1, 60, SPAN_SECS, 400 * SPAN_SECS] {
                assert_eq!(derive(CLOSE, &reads, final_read_by(CLOSE) + later), first);
            }
        }
    }

    /// The evidence string is exact and round-trips; it holds the reads up to
    /// the end of the grace and no later ones.
    #[test]
    fn evidence_encodes_the_window_reads_and_reads_back() {
        let reads = [
            calm(CLOSE - 5),
            calm(CLOSE + 7),
            rug(CLOSE + 9),
            calm(horizon(CLOSE) + 1),
            calm(final_read_by(CLOSE) + 1),
        ];
        let text = encode_evidence(CLOSE, &reads);
        assert_eq!(text, "close=1000000;reads=1000007:N,1000009:R,2209601:N");
        assert_eq!(
            decode_evidence(&text),
            Some((
                CLOSE,
                vec![calm(CLOSE + 7), rug(CLOSE + 9), calm(horizon(CLOSE) + 1)]
            ))
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
        let end = final_read_by(CLOSE);
        let cases: [(Vec<Read>, i64); 3] = [
            (confirmed_rug(), CLOSE + 300),
            (full(), horizon(CLOSE) + 61),
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
        let evidence = encode_evidence(CLOSE, &confirmed_rug());
        assert_eq!(
            rederive(RULE_VERSION, Some(&evidence), CLOSE + 300),
            Some(Outcome::Rugged)
        );
        assert_eq!(rederive("settle-1", Some(&evidence), CLOSE + 300), None);
        assert_eq!(rederive(RULE_VERSION, None, CLOSE + 300), None);
        assert_eq!(rederive(RULE_VERSION, Some("nonsense"), CLOSE + 300), None);
        // A read newer than the row that claims to rest on it.
        assert_eq!(rederive(RULE_VERSION, Some(&evidence), CLOSE + 199), None);
        // A read exactly as old as the row is fine.
        assert_eq!(
            rederive(RULE_VERSION, Some(&evidence), CLOSE + 200),
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
        assert_eq!(HORIZON_GRACE_SECS, 86_400);
        assert_eq!(RULE_VERSION, "settle-2");
        assert_eq!(horizon(i64::MAX), i64::MAX, "no overflow near the top");
        assert_eq!(
            final_read_by(CLOSE),
            CLOSE + WINDOW_SECS + HORIZON_GRACE_SECS
        );
    }

    /// A coin is read only from its close to the end of the horizon's grace.
    #[test]
    fn a_coin_is_due_only_inside_its_window() {
        assert!(!due(CLOSE, &[], CLOSE - 1));
        assert!(due(CLOSE, &[], CLOSE));
        assert!(due(CLOSE, &[], horizon(CLOSE) - 1));
        assert!(!due(CLOSE, &[], final_read_by(CLOSE)));
        assert!(!due(CLOSE, &daily(), final_read_by(CLOSE)));
    }

    /// The horizon read is owed once every span is covered and no rug was seen,
    /// until one is taken; a coin that cannot be `Stood` is not read for it.
    #[test]
    fn the_horizon_read_is_owed_only_to_a_coin_that_can_still_stand() {
        let end = horizon(CLOSE);
        assert!(due(CLOSE, &daily(), end));
        assert!(due(CLOSE, &daily(), final_read_by(CLOSE) - 1));
        assert!(!due(CLOSE, &full(), end + 100));
        // A gap in the history: no read could make it `Stood`.
        let mut gap = daily();
        gap.remove(4);
        assert!(!due(CLOSE, &gap, end));
        // A rug read in the window bars `Stood` too.
        let mut rugged_once = daily();
        rugged_once.push(rug(CLOSE + 100));
        assert!(!due(CLOSE, &rugged_once, end));
        // A rug read after the horizon is no horizon read: still owed.
        let mut after = daily();
        after.push(rug(end + 5));
        assert!(due(CLOSE, &after, end + 10));
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

    /// A coin settled `Rugged` needs no more reads; a lone rug read after a
    /// calm one is owed its confirmation even inside a covered span.
    #[test]
    fn a_rugged_coin_is_not_read_again_and_a_lone_rug_is_confirmed() {
        assert!(!due(CLOSE, &confirmed_rug(), CLOSE + 5 * SPAN_SECS));
        let lone = [calm(CLOSE + 10), rug(CLOSE + 100)];
        assert!(due(CLOSE, &lone, CLOSE + 200));
        // A rug read from before the close is not a rug in the window.
        assert!(due(CLOSE, &[rug(CLOSE - 10)], CLOSE + 5));
        // Once cleared by a calm read, the span rule applies again.
        let cleared = [calm(CLOSE + 10), rug(CLOSE + 100), calm(CLOSE + 200)];
        assert!(!due(CLOSE, &cleared, CLOSE + 300));
    }

    /// A span is half open: a read exactly at the next span's start belongs to
    /// that span and does not cover this one.
    #[test]
    fn a_read_on_the_next_spans_first_second_does_not_cover_this_one() {
        assert!(due(CLOSE, &[calm(CLOSE + SPAN_SECS)], CLOSE + 100));
        assert!(!due(CLOSE, &[calm(CLOSE)], CLOSE + 100));
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
        let evidence = encode_evidence(CLOSE, &confirmed_rug());
        let good = row(Outcome::Rugged, &evidence, CLOSE + 300);
        assert!(good.rederives());
        assert_eq!(good.close(), Some(CLOSE));
        assert!(!row(Outcome::Stood, &evidence, CLOSE + 300).rederives());
        assert!(
            !OutcomeRow {
                rule_version: "settle-1".to_owned(),
                ..good.clone()
            }
            .rederives()
        );
        let bad = row(Outcome::Rugged, "not evidence", CLOSE + 300);
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
