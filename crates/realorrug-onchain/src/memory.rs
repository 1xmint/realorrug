// SPDX-License-Identifier: Apache-2.0
//! The read memory: nothing is bought twice.
//!
//! Design [0021](../../../../docs/design/0021-the-read-memory.md) is the
//! specification this module builds; §1, §2 and §3 are why the shapes below
//! look the way they do, and are referenced by section rather than repeated.
//!
//! # The key
//!
//! `(what, subject, block)`, never `(what, subject)`. §1: two reads of the
//! same kind about the same subject at two different blocks are two
//! observations, not one fact updated — collapsing them would let a second
//! read silently overwrite the first, hiding exactly the bug a *forever*
//! fact (a launch record) coming back different is supposed to surface.
//! [`Memory::record`] refuses that instead of overwriting (see its doc).
//!
//! # Shelf life
//!
//! Three kinds (§2), each carrying its own shelf life: [`Kind::Forever`]
//! (never expires — a past block cannot become wrong later, AGENTS.md §1),
//! [`Kind::TenMinutes`] and [`Kind::Daily`] (both the owner's chosen
//! numbers, not forced ones). A row past its shelf life is **stale, not
//! deleted** (§3) — [`Memory::latest`] still returns it, with its own age.
//!
//! # One freshness predicate
//!
//! §3: a stale *required* fact is the same `CantTell` trigger as a missing
//! one, so design 0020 needs one predicate whose negative case folds
//! "never read" and "read but aged out" together, rather than a caller
//! combining "is there a row" and "is it still fresh" itself.
//! [`Memory::is_fresh`] is that predicate.
//!
//! # Chain-blind (ADR 0028)
//!
//! A subject is an address and a token, stored as its rendered string —
//! Solana's base58 and Robinhood's hex addresses do not collide as
//! strings, so no venue column is added. [`Memory::has_prior_balance`]'s
//! subject is the one place a fact needs *two* addresses at once (a
//! creator and a token); that is handled with a composite subject string
//! (see its doc), not a second column, so the key shape stays
//! `(what, subject, block)` everywhere else in the store.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

/// How long a fact of a given kind stays fresh (design 0021 §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Launch record, creator address, launch block, graduation event and
    /// the quote it raised. Never expires: forced by "a capture disposes"
    /// (AGENTS.md §1), not chosen — a block cannot become wrong later.
    Forever,
    /// Holders, balances, curve reserves, trade counts. **The owner's
    /// chosen number**, not a forced one (design 0021 §2) — say so here so
    /// a later editor knows this ten minutes is arguable.
    TenMinutes,
    /// The creator's track record (prior launches, prior graduations).
    /// **The owner's chosen number**, not a forced one (design 0021 §2) —
    /// this day is arguable too.
    Daily,
}

impl Kind {
    /// `None` means never expires.
    fn shelf_life(self) -> Option<Duration> {
        match self {
            Kind::Forever => None,
            Kind::TenMinutes => Some(Duration::from_secs(10 * 60)),
            Kind::Daily => Some(Duration::from_secs(24 * 60 * 60)),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Kind::Forever => "forever",
            Kind::TenMinutes => "ten_minutes",
            Kind::Daily => "daily",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "forever" => Some(Kind::Forever),
            "ten_minutes" => Some(Kind::TenMinutes),
            "daily" => Some(Kind::Daily),
            _ => None,
        }
    }
}

/// A fact as stored: its key, when it was read, its kind, and its value.
///
/// The value is stored and returned as text; a specific fact (a balance, a
/// holder count) is a caller's number formatted into a string, not a typed
/// column here — the store does not need to know what a `curve reserves`
/// fact means to keep it keyed, timed and fresh-checked correctly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    /// The named kind of fact — `"holder count"`, `"curve reserves"`, and so
    /// on. Free text, not an enum: the set of named facts is design 0020's
    /// to grow, not this crate's to enumerate up front.
    pub what: String,
    /// The token or address the fact is about.
    pub subject: String,
    /// The block the fact was read at.
    pub block: u64,
    /// The wall-clock time it was read.
    pub read_at: SystemTime,
    /// Which shelf-life table row it belongs to.
    pub kind: Kind,
    /// The fact's value, as text.
    pub value: String,
}

impl Fact {
    /// How long ago this fact was read, relative to `now`. Zero if `now`
    /// predates `read_at` — a clock running backwards is not this type's
    /// job to correct.
    #[must_use]
    pub fn age(&self, now: SystemTime) -> Duration {
        now.duration_since(self.read_at).unwrap_or(Duration::ZERO)
    }

    /// Whether this fact is still fresh at `now`, per its kind's shelf life
    /// (§2). A stale fact answers `false` here but is not deleted (§3).
    ///
    /// The boundary is strict: `age == shelf_life` exactly is **stale**,
    /// not fresh (`<`, not `<=`) — a fact's shelf life is how long it may
    /// be trusted, so the instant it turns that old it is no longer inside
    /// that window.
    #[must_use]
    pub fn is_fresh(&self, now: SystemTime) -> bool {
        match self.kind.shelf_life() {
            None => true,
            Some(life) => self.age(now) < life,
        }
    }
}

/// What went wrong recording or reading a fact.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The underlying SQLite call failed.
    #[error("read memory: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The same `(what, subject, block)` key was read twice with two
    /// different values — §1's "a launch record that came back different"
    /// bug. Refused rather than silently overwritten.
    #[error(
        "refusing to overwrite {what:?} for {subject:?} at block {block}: \
         stored {stored:?}, new read says {new:?}"
    )]
    Conflict {
        /// The fact's kind name.
        what: String,
        /// The fact's subject.
        subject: String,
        /// The fact's block.
        block: u64,
        /// The value already on record.
        stored: String,
        /// The value the new read produced.
        new: String,
    },
}

/// Whether [`Memory::record`] added a new row or found the same fact
/// already there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded {
    /// A new `(what, subject, block)` row was inserted.
    Inserted,
    /// The same key was already stored with the same value — a duplicate
    /// read of a settled fact, not a new observation. No row was written.
    AlreadyPresent,
}

/// The read memory: one SQLite file, one writer, on a two-core box (design
/// 0021 §4). No connection pool, no async runtime — a single [`Connection`]
/// behind `&self`, because SQLite serialises writes on one file anyway.
pub struct Memory {
    conn: Connection,
}

impl Memory {
    /// Opens (creating if absent) the memory file at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Sqlite`] if the file cannot be opened or the schema
    /// cannot be created.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// An in-memory store, for tests that do not need [`Memory::open`]'s
    /// close-and-reopen durability.
    #[cfg(test)]
    fn open_in_memory() -> Result<Self, Error> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS facts (
                what     TEXT    NOT NULL,
                subject  TEXT    NOT NULL,
                block    INTEGER NOT NULL,
                read_at  INTEGER NOT NULL,
                kind     TEXT    NOT NULL,
                value    TEXT    NOT NULL,
                PRIMARY KEY (what, subject, block)
             );",
        )?;
        Ok(Self { conn })
    }

    /// Records a fact read at `block`.
    ///
    /// The key is `(what, subject, block)` (§1). A key never seen before is
    /// [`Recorded::Inserted`]. A key seen before with the identical value is
    /// [`Recorded::AlreadyPresent`] — a duplicate read of a settled fact is
    /// not an error, it is expected for the *forever* kind once indexing
    /// has already run. A key seen before with a **different** value is
    /// [`Error::Conflict`]: never a silent overwrite, because for a
    /// *forever* fact that difference is a bug (the chain does not change
    /// what block N said), and for the shorter-lived kinds it means the
    /// caller reused a block number it should not have.
    ///
    /// # Errors
    ///
    /// [`Error::Conflict`] on a same-key, different-value re-read;
    /// [`Error::Sqlite`] if the write itself fails.
    pub fn record(
        &self,
        what: &str,
        subject: &str,
        block: u64,
        kind: Kind,
        value: &str,
        read_at: SystemTime,
    ) -> Result<Recorded, Error> {
        if let Some(existing) = self.get(what, subject, block)? {
            return if existing.value == value {
                Ok(Recorded::AlreadyPresent)
            } else {
                Err(Error::Conflict {
                    what: what.to_owned(),
                    subject: subject.to_owned(),
                    block,
                    stored: existing.value,
                    new: value.to_owned(),
                })
            };
        }
        self.conn.execute(
            "INSERT INTO facts (what, subject, block, read_at, kind, value)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                what,
                subject,
                i64::try_from(block).unwrap_or(i64::MAX),
                to_unix(read_at),
                kind.as_str(),
                value,
            ],
        )?;
        Ok(Recorded::Inserted)
    }

    fn get(&self, what: &str, subject: &str, block: u64) -> Result<Option<Fact>, Error> {
        self.conn
            .query_row(
                "SELECT read_at, kind, value FROM facts
                 WHERE what = ?1 AND subject = ?2 AND block = ?3",
                params![what, subject, i64::try_from(block).unwrap_or(i64::MAX)],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .map(|(read_at, kind, value)| row_to_fact(what, subject, block, read_at, &kind, value))
            .transpose()
    }

    /// The newest-block row on file for `(what, subject)`, fresh or stale
    /// alike (§3 — stale is kept, never deleted).
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the read fails.
    pub fn latest(&self, what: &str, subject: &str) -> Result<Option<Fact>, Error> {
        self.conn
            .query_row(
                "SELECT block, read_at, kind, value FROM facts
                 WHERE what = ?1 AND subject = ?2
                 ORDER BY block DESC LIMIT 1",
                params![what, subject],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?
            .map(|(block, read_at, kind, value)| {
                let block = u64::try_from(block).unwrap_or(0);
                row_to_fact(what, subject, block, read_at, &kind, value)
            })
            .transpose()
    }

    /// The one freshness predicate design 0021 §3 calls for: does the
    /// memory hold a **fresh** answer for `(what, subject)` right now? A
    /// fact never read and a fact read but aged out both answer `false` —
    /// design 0020 gates a required fact on this one function, not on a
    /// caller remembering to check "is there a row" and "is it still
    /// fresh" separately.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the read fails.
    pub fn is_fresh(&self, what: &str, subject: &str, now: SystemTime) -> Result<bool, Error> {
        Ok(self
            .latest(what, subject)?
            .is_some_and(|fact| fact.is_fresh(now)))
    }

    /// Design 0020 §7 / design 0022 §144's `has_prior_balance`: the newest
    /// balance this creator has ever been read holding of this token, fresh
    /// or stale. "Did they ever hold it" is a *has this been observed at
    /// all* question, unlike "how much, right now" — so this deliberately
    /// does not gate on [`Fact::is_fresh`]; a caller that needs a current
    /// number should read the fact directly and check freshness itself.
    ///
    /// The subject is `"{creator}:{token}"`, a composite string, because a
    /// balance fact is about a pair of addresses and the key shape
    /// (`what, subject, block`) has room for only one subject. This is the
    /// "neutral column... say why in a comment" case ADR 0028 allows for a
    /// subject that needs more than an address to stay unique — it is not a
    /// venue column, and no chain tag is added.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the read fails.
    pub fn has_prior_balance(&self, creator: &str, token: &str) -> Result<Option<u128>, Error> {
        let subject = balance_subject(creator, token);
        Ok(self
            .latest(BALANCE, &subject)?
            .and_then(|fact| fact.value.parse::<u128>().ok()))
    }

    /// Records a balance observation for [`Memory::has_prior_balance`] to
    /// read back. Kept alongside it rather than requiring a caller to know
    /// [`Memory::record`]'s `what`/subject convention for balances.
    ///
    /// # Errors
    ///
    /// Same as [`Memory::record`].
    pub fn record_balance(
        &self,
        creator: &str,
        token: &str,
        block: u64,
        amount: u128,
        read_at: SystemTime,
    ) -> Result<Recorded, Error> {
        let subject = balance_subject(creator, token);
        self.record(
            BALANCE,
            &subject,
            block,
            Kind::TenMinutes,
            &amount.to_string(),
            read_at,
        )
    }

    /// Design 0020 §7 / design 0022 §144's `launches_in_window`: how many
    /// distinct blocks this address launched a token at, counting only
    /// launches the memory **learned of** within the last `minutes`
    /// minutes — keyed by `read_at`, not by the launch's own block, so
    /// re-indexing an old launch never counts as a new one.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the read fails.
    pub fn launches_in_window(
        &self,
        address: &str,
        minutes: u32,
        now: SystemTime,
    ) -> Result<u32, Error> {
        let window_start = to_unix(now) - i64::from(minutes) * 60;
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM facts
             WHERE what = ?1 AND subject = ?2 AND read_at >= ?3",
            params![LAUNCH, address, window_start],
            |row| row.get(0),
        )?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// Records a launch observation for [`Memory::launches_in_window`] to
    /// count. `subject` is the creator's address; `block` is the block the
    /// launch happened at.
    ///
    /// # Errors
    ///
    /// Same as [`Memory::record`].
    pub fn record_launch(
        &self,
        creator: &str,
        block: u64,
        read_at: SystemTime,
    ) -> Result<Recorded, Error> {
        self.record(LAUNCH, creator, block, Kind::Forever, "launched", read_at)
    }
}

/// The `what` [`Memory::has_prior_balance`]/[`Memory::record_balance`] use.
const BALANCE: &str = "balance";
/// The `what` [`Memory::launches_in_window`]/[`Memory::record_launch`] use.
const LAUNCH: &str = "launch";

fn balance_subject(creator: &str, token: &str) -> String {
    format!("{creator}:{token}")
}

fn row_to_fact(
    what: &str,
    subject: &str,
    block: u64,
    read_at: i64,
    kind: &str,
    value: String,
) -> Result<Fact, Error> {
    Ok(Fact {
        what: what.to_owned(),
        subject: subject.to_owned(),
        block,
        read_at: from_unix(read_at),
        // A kind string that fails to parse can only come from a hand-edited
        // database file, since `Kind::as_str` is the only writer; there is no
        // recovery better than surfacing it as a SQLite-shaped error.
        kind: Kind::parse(kind).ok_or_else(|| {
            Error::Sqlite(rusqlite::Error::InvalidColumnType(
                4,
                "kind".to_owned(),
                rusqlite::types::Type::Text,
            ))
        })?,
        value,
    })
}

fn to_unix(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(_) => 0,
    }
}

fn from_unix(secs: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(u64::try_from(secs).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(n)
    }

    #[test]
    fn a_forever_fact_stored_once_answers_every_later_ask() {
        let mem = Memory::open_in_memory().expect("open");
        mem.record(
            "launch record",
            "TOKEN",
            100,
            Kind::Forever,
            "creator=ABC",
            secs(1_000),
        )
        .expect("record");

        for _ in 0..3 {
            let fact = mem
                .latest("launch record", "TOKEN")
                .expect("read")
                .expect("present");
            assert_eq!(fact.value, "creator=ABC");
            assert_eq!(fact.block, 100);
        }
    }

    #[test]
    fn a_duplicate_read_of_the_same_key_is_not_a_silent_overwrite() {
        let mem = Memory::open_in_memory().expect("open");
        mem.record(
            "launch record",
            "TOKEN",
            100,
            Kind::Forever,
            "creator=ABC",
            secs(1_000),
        )
        .expect("first read");

        // Same key, same value: a duplicate read, not an error, and not a
        // second row either.
        let second = mem
            .record(
                "launch record",
                "TOKEN",
                100,
                Kind::Forever,
                "creator=ABC",
                secs(2_000),
            )
            .expect("re-read of an identical fact must not fail");
        assert_eq!(second, Recorded::AlreadyPresent);

        // Same key, a DIFFERENT value: this is the bug §1 warns about — a
        // launch record that came back different — and must be refused,
        // never silently applied.
        let conflict = mem.record(
            "launch record",
            "TOKEN",
            100,
            Kind::Forever,
            "creator=XYZ",
            secs(3_000),
        );
        assert!(
            matches!(conflict, Err(Error::Conflict { .. })),
            "a same-key different-value re-read must be refused, got {conflict:?}"
        );
        // And the store still holds only the original value — the refused
        // write did not partially apply.
        let fact = mem
            .latest("launch record", "TOKEN")
            .expect("read")
            .expect("present");
        assert_eq!(fact.value, "creator=ABC");
    }

    #[test]
    fn two_reads_at_two_blocks_both_survive_and_latest_says_which_is_newer() {
        let mem = Memory::open_in_memory().expect("open");
        mem.record(
            "holder count",
            "TOKEN",
            100,
            Kind::TenMinutes,
            "12",
            secs(1_000),
        )
        .expect("first block");
        mem.record(
            "holder count",
            "TOKEN",
            1_000,
            Kind::TenMinutes,
            "45",
            secs(2_000),
        )
        .expect("second block");

        // The older observation was not overwritten...
        let older = mem.get("holder count", "TOKEN", 100).expect("read");
        assert_eq!(older.expect("still there").value, "12");

        // ...and `latest` reports the newer one by block, not by read order.
        let newest = mem
            .latest("holder count", "TOKEN")
            .expect("read")
            .expect("present");
        assert_eq!(newest.block, 1_000);
        assert_eq!(newest.value, "45");
    }

    #[test]
    fn a_ten_minute_fact_read_eleven_minutes_ago_is_stale_but_still_present() {
        let mem = Memory::open_in_memory().expect("open");
        let read_at = secs(0);
        mem.record(
            "curve reserves",
            "TOKEN",
            50,
            Kind::TenMinutes,
            "999",
            read_at,
        )
        .expect("record");

        let eleven_minutes_later = secs(11 * 60);
        let fact = mem
            .latest("curve reserves", "TOKEN")
            .expect("read")
            .expect("stale is kept, not deleted");
        assert!(!fact.is_fresh(eleven_minutes_later), "must be stale");
        assert_eq!(fact.age(eleven_minutes_later), Duration::from_secs(11 * 60));
        assert_eq!(fact.value, "999", "the stale value is still readable");
    }

    #[test]
    fn is_fresh_answers_false_the_same_way_for_never_read_and_aged_out() {
        let mem = Memory::open_in_memory().expect("open");
        let now = secs(1_000_000);

        // Never read at all.
        let never_read = mem
            .is_fresh("curve reserves", "NEVER_SEEN", now)
            .expect("read");
        assert!(!never_read);

        // Read, but past its ten-minute shelf life.
        mem.record(
            "curve reserves",
            "AGED_OUT",
            50,
            Kind::TenMinutes,
            "1",
            now - Duration::from_secs(20 * 60),
        )
        .expect("record");
        let aged_out = mem
            .is_fresh("curve reserves", "AGED_OUT", now)
            .expect("read");
        assert!(!aged_out);

        assert_eq!(
            never_read, aged_out,
            "missing and stale-required must be the same predicate answer"
        );

        // And a fresh fact answers differently, so the predicate is not
        // just always false.
        mem.record(
            "curve reserves",
            "FRESH",
            50,
            Kind::TenMinutes,
            "1",
            now - Duration::from_secs(60),
        )
        .expect("record");
        assert!(mem.is_fresh("curve reserves", "FRESH", now).expect("read"));
    }

    #[test]
    fn the_store_survives_being_closed_and_reopened() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("memory.sqlite3");

        {
            let mem = Memory::open(&path).expect("open");
            mem.record(
                "launch record",
                "TOKEN",
                100,
                Kind::Forever,
                "creator=ABC",
                secs(1_000),
            )
            .expect("record");
            // `mem` drops here, closing the handle.
        }

        let reopened = Memory::open(&path).expect("reopen the same file");
        let fact = reopened
            .latest("launch record", "TOKEN")
            .expect("read")
            .expect("survives a close and reopen");
        assert_eq!(fact.value, "creator=ABC");
    }

    #[test]
    fn has_prior_balance_is_none_until_a_balance_is_recorded() {
        let mem = Memory::open_in_memory().expect("open");
        assert_eq!(
            mem.has_prior_balance("CREATOR", "TOKEN").expect("read"),
            None
        );

        mem.record_balance("CREATOR", "TOKEN", 100, 5_000, secs(1_000))
            .expect("record");
        assert_eq!(
            mem.has_prior_balance("CREATOR", "TOKEN").expect("read"),
            Some(5_000)
        );

        // A different token for the same creator is a different subject.
        assert_eq!(
            mem.has_prior_balance("CREATOR", "OTHER_TOKEN")
                .expect("read"),
            None
        );
    }

    #[test]
    fn launches_in_window_counts_only_launches_read_recently() {
        let mem = Memory::open_in_memory().expect("open");
        let now = secs(1_000_000);

        mem.record_launch("CREATOR", 10, now - Duration::from_secs(5 * 60))
            .expect("recent launch");
        mem.record_launch("CREATOR", 20, now - Duration::from_secs(2 * 60 * 60))
            .expect("old launch");

        let count = mem.launches_in_window("CREATOR", 30, now).expect("read");
        assert_eq!(count, 1, "only the launch read within the window counts");

        let wider = mem.launches_in_window("CREATOR", 180, now).expect("read");
        assert_eq!(wider, 2, "a wider window catches both");
    }

    #[test]
    fn every_kind_round_trips_through_as_str_and_parse() {
        // Deleting the "daily" arm in `Kind::parse` (or any other arm)
        // leaves `as_str` and `parse` disagreeing for that kind, but
        // nothing notices unless every kind is checked, not just the one
        // a test happened to use elsewhere.
        for kind in [Kind::Forever, Kind::TenMinutes, Kind::Daily] {
            let s = kind.as_str();
            assert_eq!(
                Kind::parse(s),
                Some(kind),
                "{s:?} must parse back to {kind:?}"
            );
        }
        assert_eq!(Kind::parse("bogus"), None);
    }

    #[test]
    fn shelf_life_is_exactly_the_designs_chosen_numbers() {
        // Design 0021 §2 chose 10 minutes and 24 hours; pin the exact
        // second counts so `10 + 60` or `10 / 60` cannot pass silently in
        // place of `10 * 60`.
        assert_eq!(
            Kind::TenMinutes.shelf_life(),
            Some(Duration::from_secs(600)),
            "ten minutes must be exactly 600 seconds"
        );
        assert_eq!(
            Kind::Daily.shelf_life(),
            Some(Duration::from_secs(86_400)),
            "a day must be exactly 86,400 seconds"
        );
        assert_eq!(Kind::Forever.shelf_life(), None);
    }

    #[test]
    fn is_fresh_boundary_is_strict_for_ten_minutes_and_daily() {
        // Exactly at the shelf life is stale, not fresh: `age < life`, not
        // `age <= life` (see the comment on `Fact::is_fresh`). Check one
        // tick on each side of the boundary for both finite kinds. A
        // nanosecond either side is the letter of the spec, but
        // `SystemTime` on this target (`+stable-x86_64-pc-windows-gnullvm`)
        // is a Windows `FILETIME`-backed clock with 100ns resolution, so a
        // 1ns offset can round away to the boundary itself; a microsecond
        // (1,000ns) clears that resolution floor while still being far
        // smaller than either shelf life.
        let read_at = UNIX_EPOCH;
        let tick = Duration::from_micros(1);

        let ten_minute_fact = Fact {
            what: "curve reserves".to_owned(),
            subject: "TOKEN".to_owned(),
            block: 1,
            read_at,
            kind: Kind::TenMinutes,
            value: "1".to_owned(),
        };
        let ten_minutes = Duration::from_secs(600);
        assert!(
            ten_minute_fact.is_fresh(read_at + ten_minutes - tick),
            "just before the ten-minute shelf life must still be fresh"
        );
        assert!(
            !ten_minute_fact.is_fresh(read_at + ten_minutes),
            "exactly at the ten-minute shelf life must be stale"
        );
        assert!(
            !ten_minute_fact.is_fresh(read_at + ten_minutes + tick),
            "just past the ten-minute shelf life must be stale"
        );

        let daily_fact = Fact {
            kind: Kind::Daily,
            ..ten_minute_fact
        };
        let one_day = Duration::from_secs(86_400);
        assert!(
            daily_fact.is_fresh(read_at + one_day - tick),
            "just before the daily shelf life must still be fresh"
        );
        assert!(
            !daily_fact.is_fresh(read_at + one_day),
            "exactly at the daily shelf life must be stale"
        );
        assert!(
            !daily_fact.is_fresh(read_at + one_day + tick),
            "just past the daily shelf life must be stale"
        );
    }
}
