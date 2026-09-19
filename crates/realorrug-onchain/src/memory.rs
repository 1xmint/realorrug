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
    /// A transfer ledger that does not add up: a debit below zero, or a
    /// stored amount that is not a number. Refused rather than applied,
    /// because a balance that went negative means the events were
    /// incomplete or out of order, and a count published from that ledger
    /// would misreport a real balance (AGENTS.md §3 rule 8).
    #[error("transfer ledger for {token:?}: {why}")]
    Ledger {
        /// The token whose ledger failed.
        token: String,
        /// What did not add up.
        why: String,
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
    pub(crate) fn open_in_memory() -> Result<Self, Error> {
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
        Self::init_events(&conn)?;
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

// ---------------------------------------------------------------------------
// Event memory: a token's Transfer ledger, its balances and its checkpoint
// (design 0021 §9). The `facts` table above remembers what a read *said*;
// these tables remember the events themselves, so the next summon reads only
// the blocks the last one did not cover.
// ---------------------------------------------------------------------------

/// How far behind a checkpoint a block is still treated as reorganisable.
///
/// A checkpoint whose hash no longer matches the chain is rolled back this
/// many blocks and re-read from there; transfers older than this behind the
/// checkpoint are treated as final and may be pruned. **The owner's chosen
/// number, not a measured one**: Robinhood Chain is an Arbitrum Orbit chain
/// whose sequencer reorganises rarely and shallowly, and at its ~0.1s blocks
/// this is under a minute of chain. A deeper reorg than this leaves the
/// memory wrong until the token is forgotten; design 0021 §9 says so.
pub const REORG_DEPTH: u64 = 256;

/// The most transfers kept per token before the oldest final ones are
/// pruned. Sized from the same measurement as the reader's page cap: the
/// busiest live token walked on 2026-09-17 held 28,652 transfers, so this
/// keeps a token like it whole and bounds a busier one. Balances survive
/// pruning (they were applied when each row was inserted), so what is lost
/// is only the receipt behind an old balance, which the chain can re-supply.
pub const MAX_TRANSFERS_PER_TOKEN: u64 = 50_000;

/// The most check runs kept per `(chain, token, what)`; older ones go.
const MAX_CHECK_RUNS: u64 = 32;

/// The block a token's transfer memory is complete through, with the hash
/// that block had when it was read. The hash is what makes "complete
/// through N" checkable later: if the chain's block N now has another hash,
/// the suffix was built on a block that is no longer canonical.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    /// The block number.
    pub block: u64,
    /// The block's hash, rendered.
    pub hash: String,
}

/// One token `Transfer` event as the memory stores it.
///
/// Identity is `(chain, token, block, transaction_index, log_index)` -- the
/// event's position, never its transaction hash alone, because one
/// transaction can emit the same event twice. `block_hash` is kept per row
/// so a reorg can be told apart from a retry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferEvent {
    /// The block number.
    pub block: u64,
    /// The block's hash, rendered.
    pub block_hash: String,
    /// The transaction's index within the block.
    pub transaction_index: u64,
    /// The log's index within the block.
    pub log_index: u64,
    /// The transaction hash, rendered.
    pub transaction: String,
    /// Who sent, or `None` for a mint -- nothing is debited then. The
    /// memory does not know which address a chain mints from; the reader
    /// that decoded the event does, and says so here.
    pub from: Option<String>,
    /// Who received.
    pub to: String,
    /// How much, in the token's smallest unit.
    pub amount: u128,
}

/// One native-currency transfer into a wallet that later bought the token:
/// an observed edge, never an ownership claim (design 0027 §2.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FundingEdge {
    /// The buyer that received the funds, in the chain's own canonical text
    /// form (0x-lowercase hex for Robinhood, base58 for Solana).
    pub recipient: String,
    /// The address that sent them, on the same terms as `recipient`.
    pub funder: String,
    /// The block (or slot) the transfer landed in.
    pub block: u64,
    /// The transaction that carried it.
    pub transaction: String,
    /// The provider's identity for this transfer, the dedupe key.
    pub unique_id: String,
    /// The amount transferred, in the chain's native smallest unit (wei for
    /// Robinhood, lamports for Solana).
    pub amount: u128,
    /// Whether the amount was material against the buyer's purchase; dust
    /// is kept as an observation but never counts for attribution.
    pub material: bool,
}

/// One decoded creator buy or sell, design 0027 slice 5. An event, on the
/// same terms [`FundingEdge`] is: identity is the log's own
/// `unique_id`, never `(recipient, side, amount)`, so two distinct trades
/// that happen to match on amount are never collapsed into one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatorTradeEvent {
    /// The token this trade was against, in the chain's own canonical text
    /// form.
    pub token: String,
    /// Which paid account was the beneficiary: `"deployer"` or
    /// `"fee_recipient"`. A plain string, not an enum, so this table stays
    /// chain-agnostic the way [`FundingEdge`] is -- a future chain's own
    /// role names do not need a new column.
    pub role: String,
    /// `"buy"` or `"sell"`.
    pub side: String,
    /// Quote paid in (a buy) or received (a sell), in the chain's native
    /// smallest unit.
    pub quote: u128,
    /// Tokens received (a buy) or given up (a sell).
    pub tokens: u128,
    /// The block (or slot) it landed in.
    pub block: u64,
    /// The transaction that carried it.
    pub transaction: String,
    /// The log's own identity, the dedupe key.
    pub unique_id: String,
}

/// One launch [`Memory::launches_bought_by`] reports for a wallet: it
/// bought this token's launch, first seen at this block, for this amount.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuyerLaunch {
    /// The token whose launch was bought, in the chain's own canonical text
    /// form.
    pub token: String,
    /// The block of the buy [`Memory::record_buy`] was given -- the launch
    /// window's already-read first purchase, not a re-derived one.
    pub block: u64,
    /// The amount recorded with the buy, in the chain's smallest unit for
    /// whatever was paid (wei for Robinhood's launch-window quote).
    pub amount: u128,
}

/// What [`Memory::extend_transfers`] did with the events it was handed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extended {
    /// Events not seen before, whose amounts moved balances.
    pub inserted: u64,
    /// Events already on file -- a retried or overlapping range. Balances
    /// were not moved for these.
    pub duplicates: u64,
}

/// How a check run ended. An empty complete range and a failed query are
/// different results (design packet §2.3): the first says "nothing
/// happened in these blocks", the second says nothing at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completeness {
    /// Every block in the interval was read and every event kept.
    Complete,
    /// The query did not finish; the interval is not covered.
    Failed,
    /// The read stopped early by a cap or budget; the interval is covered
    /// only up to where it stopped.
    Truncated,
}

impl Completeness {
    fn as_str(self) -> &'static str {
        match self {
            Completeness::Complete => "complete",
            Completeness::Failed => "failed",
            Completeness::Truncated => "truncated",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "complete" => Some(Completeness::Complete),
            "failed" => Some(Completeness::Failed),
            "truncated" => Some(Completeness::Truncated),
            _ => None,
        }
    }
}

/// One recorded check run: what was asked, over which blocks, how it ended
/// and what it cost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckRun {
    /// The chain, e.g. `"robinhood"`.
    pub chain: String,
    /// The token the check was about.
    pub token: String,
    /// The check's name, e.g. `"transfers"`.
    pub what: String,
    /// The check's versioned parameters, rendered by the caller.
    pub parameters: String,
    /// The first block asked for.
    pub from_block: u64,
    /// The last block asked for.
    pub to_block: u64,
    /// How it ended.
    pub completeness: Completeness,
    /// RPC calls spent, including retries the provider still billed.
    pub calls: u32,
    /// The error text on failure, or the reason for truncation; empty when
    /// complete.
    pub note: String,
    /// When it ran.
    pub ran_at: SystemTime,
}

impl Memory {
    fn init_events(conn: &Connection) -> Result<(), Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS transfers (
                chain      TEXT    NOT NULL,
                token      TEXT    NOT NULL,
                block      INTEGER NOT NULL,
                tx_index   INTEGER NOT NULL,
                log_index  INTEGER NOT NULL,
                block_hash TEXT    NOT NULL,
                tx_hash    TEXT    NOT NULL,
                sender     TEXT,
                recipient  TEXT    NOT NULL,
                amount     TEXT    NOT NULL,
                PRIMARY KEY (chain, token, block, tx_index, log_index)
             );
             CREATE TABLE IF NOT EXISTS token_balances (
                chain   TEXT NOT NULL,
                token   TEXT NOT NULL,
                holder  TEXT NOT NULL,
                balance TEXT NOT NULL,
                PRIMARY KEY (chain, token, holder)
             );
             CREATE TABLE IF NOT EXISTS token_state (
                chain            TEXT    NOT NULL,
                token            TEXT    NOT NULL,
                complete_through INTEGER NOT NULL,
                complete_hash    TEXT    NOT NULL,
                PRIMARY KEY (chain, token)
             );
             CREATE TABLE IF NOT EXISTS check_runs (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                chain        TEXT    NOT NULL,
                token        TEXT    NOT NULL,
                what         TEXT    NOT NULL,
                parameters   TEXT    NOT NULL,
                from_block   INTEGER NOT NULL,
                to_block     INTEGER NOT NULL,
                completeness TEXT    NOT NULL,
                calls        INTEGER NOT NULL,
                note         TEXT    NOT NULL,
                ran_at       INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS check_runs_by_subject
                ON check_runs (chain, token, what, id);
             CREATE TABLE IF NOT EXISTS funding_edges (
                chain     TEXT    NOT NULL,
                token     TEXT    NOT NULL,
                recipient TEXT    NOT NULL,
                funder    TEXT    NOT NULL,
                block     INTEGER NOT NULL,
                tx_hash   TEXT    NOT NULL,
                unique_id TEXT    NOT NULL,
                amount    TEXT    NOT NULL,
                material  INTEGER NOT NULL,
                PRIMARY KEY (chain, unique_id)
             );
             CREATE INDEX IF NOT EXISTS funding_edges_by_token
                ON funding_edges (chain, token, recipient);
             CREATE INDEX IF NOT EXISTS funding_edges_by_funder
                ON funding_edges (chain, funder);
             CREATE TABLE IF NOT EXISTS creator_trades (
                chain     TEXT    NOT NULL,
                token     TEXT    NOT NULL,
                role      TEXT    NOT NULL,
                side      TEXT    NOT NULL,
                quote     TEXT    NOT NULL,
                tokens    TEXT    NOT NULL,
                block     INTEGER NOT NULL,
                tx_hash   TEXT    NOT NULL,
                unique_id TEXT    NOT NULL,
                PRIMARY KEY (chain, unique_id)
             );
             CREATE INDEX IF NOT EXISTS creator_trades_by_token
                ON creator_trades (chain, token);
             CREATE TABLE IF NOT EXISTS buyer_index (
                chain  TEXT    NOT NULL,
                buyer  TEXT    NOT NULL,
                token  TEXT    NOT NULL,
                block  INTEGER NOT NULL,
                amount TEXT    NOT NULL,
                PRIMARY KEY (chain, buyer, token)
             );
             CREATE INDEX IF NOT EXISTS buyer_index_by_buyer
                ON buyer_index (chain, buyer);",
        )?;
        Ok(())
    }

    /// Remembers that `buyer` bought `token`'s launch, once per
    /// `(chain, buyer, token)` (design 0021, buyer index; research 0052
    /// §7.2 and §8, task M-D-0009).
    ///
    /// The primary key is `(chain, buyer, token)`, not an event id the way
    /// [`Memory::record_funding_edges`] and [`Memory::record_creator_trades`]
    /// key theirs -- S8's cross-token recurrence counter (research 0052
    /// §3, row S8) only ever asks "did this wallet buy this launch", so a
    /// second buy by the same wallet in the same launch window is the same
    /// fact, not a second one. `INSERT OR IGNORE` makes a re-record of the
    /// same key a no-op: the first-seen block and amount stand, and a
    /// retried write (the sheet runs again over the same launch) never
    /// fails or overwrites.
    ///
    /// Returns `true` when this call inserted a new row, `false` when the
    /// key was already present.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the write fails.
    pub fn record_buy(
        &self,
        chain: &str,
        buyer: &str,
        token: &str,
        block: u64,
        amount: u128,
    ) -> Result<bool, Error> {
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO buyer_index (chain, buyer, token, block, amount)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![chain, buyer, token, to_i64(block), amount.to_string()],
        )?;
        Ok(changed > 0)
    }

    /// Every launch `buyer` is on record as having bought, oldest first --
    /// the query S8's pre-aged-wallet / cross-token-recurrence factor needs:
    /// given a wallet address, which launches has it bought, and how many.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the read fails, or a stored amount no longer
    /// parses.
    pub fn launches_bought_by(&self, chain: &str, buyer: &str) -> Result<Vec<BuyerLaunch>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT token, block, amount FROM buyer_index
             WHERE chain = ?1 AND buyer = ?2
             ORDER BY block, token",
        )?;
        let rows = stmt.query_map(params![chain, buyer], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut launches = Vec::new();
        for row in rows {
            let (token, block, amount) = row?;
            launches.push(BuyerLaunch {
                token: token.clone(),
                block: u64::try_from(block).unwrap_or(0),
                amount: amount.parse().map_err(|_| Error::Ledger {
                    token,
                    why: format!("buyer index amount {amount:?} does not parse"),
                })?,
            });
        }
        Ok(launches)
    }

    /// Remembers funding edges as events, once each.
    ///
    /// The identity is the provider's own `uniqueId` for the transfer
    /// (`hash:external:index` on Alchemy), not `(recipient, funder, amount)`:
    /// two identical top-ups an hour apart are two events, and collapsing
    /// them would understate how often one wallet fed another (design 0027
    /// §2.3, "as events, not a collapsing key"). A retry that re-reads the
    /// same page inserts nothing the second time.
    ///
    /// Indexed both ways -- by `(token, recipient)` for "who funded this
    /// buyer" and by `funder` for "who else did this wallet fund" -- so a
    /// later slice can walk paths without a cluster that cannot be undone.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if a write fails.
    pub fn record_funding_edges(
        &self,
        chain: &str,
        token: &str,
        edges: &[FundingEdge],
    ) -> Result<u64, Error> {
        let tx = self.conn.unchecked_transaction()?;
        let mut inserted = 0u64;
        for edge in edges {
            let changed = tx.execute(
                "INSERT OR IGNORE INTO funding_edges
                 (chain, token, recipient, funder, block, tx_hash, unique_id, amount, material)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    chain,
                    token,
                    edge.recipient,
                    edge.funder,
                    to_i64(edge.block),
                    edge.transaction,
                    edge.unique_id,
                    edge.amount.to_string(),
                    i32::from(edge.material),
                ],
            )?;
            inserted += u64::try_from(changed).unwrap_or(0);
        }
        tx.commit()?;
        Ok(inserted)
    }

    /// Every funding edge remembered for `(chain, token)`, oldest first.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the read fails, or a stored amount no longer
    /// parses.
    pub fn funding_edges(&self, chain: &str, token: &str) -> Result<Vec<FundingEdge>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT recipient, funder, block, tx_hash, unique_id, amount, material
             FROM funding_edges WHERE chain = ?1 AND token = ?2
             ORDER BY block, unique_id",
        )?;
        let rows = stmt.query_map(params![chain, token], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i32>(6)?,
            ))
        })?;
        let mut edges = Vec::new();
        for row in rows {
            let (recipient, funder, block, transaction, unique_id, amount, material) = row?;
            edges.push(FundingEdge {
                recipient,
                funder,
                block: u64::try_from(block).unwrap_or(0),
                transaction,
                unique_id,
                amount: amount.parse().map_err(|_| Error::Ledger {
                    token: token.to_owned(),
                    why: format!("funding amount {amount:?} does not parse"),
                })?,
                material: material != 0,
            });
        }
        Ok(edges)
    }

    /// Remembers creator trades as events, once each, the same
    /// `INSERT OR IGNORE` idempotency [`Memory::record_funding_edges`] uses:
    /// a retry that re-reads the same log range inserts nothing the second
    /// time.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if a write fails.
    pub fn record_creator_trades(
        &self,
        chain: &str,
        trades: &[CreatorTradeEvent],
    ) -> Result<u64, Error> {
        let tx = self.conn.unchecked_transaction()?;
        let mut inserted = 0u64;
        for trade in trades {
            let changed = tx.execute(
                "INSERT OR IGNORE INTO creator_trades
                 (chain, token, role, side, quote, tokens, block, tx_hash, unique_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    chain,
                    trade.token,
                    trade.role,
                    trade.side,
                    trade.quote.to_string(),
                    trade.tokens.to_string(),
                    to_i64(trade.block),
                    trade.transaction,
                    trade.unique_id,
                ],
            )?;
            inserted += u64::try_from(changed).unwrap_or(0);
        }
        tx.commit()?;
        Ok(inserted)
    }

    /// Every creator trade remembered for `(chain, token)`, oldest first.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the read fails, or a stored amount no longer
    /// parses.
    pub fn creator_trades(
        &self,
        chain: &str,
        token: &str,
    ) -> Result<Vec<CreatorTradeEvent>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT role, side, quote, tokens, block, tx_hash, unique_id
             FROM creator_trades WHERE chain = ?1 AND token = ?2
             ORDER BY block, unique_id",
        )?;
        let rows = stmt.query_map(params![chain, token], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;
        let mut trades = Vec::new();
        for row in rows {
            let (role, side, quote, tokens, block, transaction, unique_id) = row?;
            trades.push(CreatorTradeEvent {
                token: token.to_owned(),
                role,
                side,
                quote: quote.parse().map_err(|_| Error::Ledger {
                    token: token.to_owned(),
                    why: format!("creator trade quote {quote:?} does not parse"),
                })?,
                tokens: tokens.parse().map_err(|_| Error::Ledger {
                    token: token.to_owned(),
                    why: format!("creator trade tokens {tokens:?} does not parse"),
                })?,
                block: u64::try_from(block).unwrap_or(0),
                transaction,
                unique_id,
            });
        }
        Ok(trades)
    }

    /// The block `token`'s transfer memory is complete through, or `None`
    /// when nothing complete is on file (never read, or rolled back and not
    /// yet re-read).
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the read fails.
    pub fn token_checkpoint(&self, chain: &str, token: &str) -> Result<Option<Checkpoint>, Error> {
        Ok(self
            .conn
            .query_row(
                "SELECT complete_through, complete_hash FROM token_state
                 WHERE chain = ?1 AND token = ?2",
                params![chain, token],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .map(|(block, hash)| Checkpoint {
                block: u64::try_from(block).unwrap_or(0),
                hash,
            }))
    }

    /// Stores `events` and moves balances for the ones not already on file,
    /// then marks the memory complete through `through` -- all in one
    /// transaction, so a crash leaves either the old checkpoint with the old
    /// balances or the new with the new, never a checkpoint ahead of the
    /// ledger behind it.
    ///
    /// An event already on file (same position) is counted a duplicate and
    /// moves nothing, so a retried or overlapping range is harmless. Rows
    /// older than [`REORG_DEPTH`] behind `through` are pruned, oldest first,
    /// once the token holds more than [`MAX_TRANSFERS_PER_TOKEN`].
    ///
    /// # Errors
    ///
    /// [`Error::Ledger`] if a debit would take a balance below zero -- the
    /// events are out of order or incomplete, and nothing is committed;
    /// [`Error::Sqlite`] if a write fails.
    pub fn extend_transfers(
        &self,
        chain: &str,
        token: &str,
        events: &[TransferEvent],
        through: &Checkpoint,
    ) -> Result<Extended, Error> {
        let tx = self.conn.unchecked_transaction()?;
        let mut extended = Extended {
            inserted: 0,
            duplicates: 0,
        };
        for event in events {
            let changed = tx.execute(
                "INSERT OR IGNORE INTO transfers
                 (chain, token, block, tx_index, log_index, block_hash, tx_hash,
                  sender, recipient, amount)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    chain,
                    token,
                    to_i64(event.block),
                    to_i64(event.transaction_index),
                    to_i64(event.log_index),
                    event.block_hash,
                    event.transaction,
                    event.from,
                    event.to,
                    event.amount.to_string(),
                ],
            )?;
            if changed == 0 {
                extended.duplicates += 1;
                continue;
            }
            extended.inserted += 1;
            if let Some(from) = &event.from {
                adjust_balance(&tx, chain, token, from, event.amount, Direction::Debit)?;
            }
            adjust_balance(
                &tx,
                chain,
                token,
                &event.to,
                event.amount,
                Direction::Credit,
            )?;
        }
        tx.execute(
            "INSERT INTO token_state (chain, token, complete_through, complete_hash)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (chain, token) DO UPDATE
             SET complete_through = excluded.complete_through,
                 complete_hash = excluded.complete_hash",
            params![chain, token, to_i64(through.block), through.hash],
        )?;
        // Bound the receipts, never the balances: the rows dropped here are
        // final (older than a reorg can reach) and already applied.
        let held: i64 = tx.query_row(
            "SELECT COUNT(*) FROM transfers WHERE chain = ?1 AND token = ?2",
            params![chain, token],
            |row| row.get(0),
        )?;
        let excess = u64::try_from(held)
            .unwrap_or(0)
            .saturating_sub(MAX_TRANSFERS_PER_TOKEN);
        if excess > 0 {
            let final_before = through.block.saturating_sub(REORG_DEPTH);
            tx.execute(
                "DELETE FROM transfers WHERE rowid IN (
                    SELECT rowid FROM transfers
                    WHERE chain = ?1 AND token = ?2 AND block < ?3
                    ORDER BY block, tx_index, log_index LIMIT ?4)",
                params![chain, token, to_i64(final_before), to_i64(excess)],
            )?;
        }
        tx.commit()?;
        Ok(extended)
    }

    /// Forgets every transfer of `token` after `block`, reversing its effect
    /// on balances, and drops the checkpoint -- the memory is then complete
    /// through nothing until the next [`Memory::extend_transfers`]. Called
    /// when the checkpoint's hash no longer matches the chain (a reorg).
    /// Returns how many events were forgotten.
    ///
    /// Dropping the checkpoint rather than moving it to `block` is
    /// deliberate: the hash block `block` *had* is not on file, so a
    /// checkpoint there would claim a match nothing verified. A reader that
    /// dies between this and the re-read leaves rows with no checkpoint,
    /// and the next summon walks from the launch block again -- duplicates
    /// are ignored, so that costs calls, not correctness.
    ///
    /// # Errors
    ///
    /// [`Error::Ledger`] if reversing a credit would take a balance below
    /// zero (the ledger was already inconsistent); [`Error::Sqlite`] if a
    /// write fails. Nothing is committed on error.
    pub fn roll_back_transfers_after(
        &self,
        chain: &str,
        token: &str,
        block: u64,
    ) -> Result<u64, Error> {
        let tx = self.conn.unchecked_transaction()?;
        let rows: Vec<(Option<String>, String, String)> = {
            let mut stmt = tx.prepare(
                "SELECT sender, recipient, amount FROM transfers
                 WHERE chain = ?1 AND token = ?2 AND block > ?3
                 ORDER BY block DESC, tx_index DESC, log_index DESC",
            )?;
            let rows = stmt.query_map(params![chain, token, to_i64(block)], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
            rows.collect::<Result<_, _>>()?
        };
        for (sender, recipient, amount) in &rows {
            let amount = parse_amount(amount)?;
            adjust_balance(&tx, chain, token, recipient, amount, Direction::Debit)?;
            if let Some(sender) = sender {
                adjust_balance(&tx, chain, token, sender, amount, Direction::Credit)?;
            }
        }
        tx.execute(
            "DELETE FROM transfers WHERE chain = ?1 AND token = ?2 AND block > ?3",
            params![chain, token, to_i64(block)],
        )?;
        tx.execute(
            "DELETE FROM token_state WHERE chain = ?1 AND token = ?2",
            params![chain, token],
        )?;
        tx.commit()?;
        Ok(u64::try_from(rows.len()).unwrap_or(u64::MAX))
    }

    /// Every non-zero balance of `token` on file, as `(holder, balance)`.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the read fails; [`Error::Ledger`] if a stored
    /// balance does not parse.
    pub fn token_balances(&self, chain: &str, token: &str) -> Result<Vec<(String, u128)>, Error> {
        let mut stmt = self.conn.prepare(
            "SELECT holder, balance FROM token_balances
             WHERE chain = ?1 AND token = ?2 AND balance != '0'
             ORDER BY holder",
        )?;
        let rows = stmt.query_map(params![chain, token], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (holder, balance) = row?;
            Ok((holder, parse_amount(&balance)?))
        })
        .collect()
    }

    /// Records one check run and prunes the oldest beyond [`MAX_CHECK_RUNS`]
    /// for the same `(chain, token, what)`.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the write fails.
    pub fn record_check_run(&self, run: &CheckRun) -> Result<(), Error> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO check_runs
             (chain, token, what, parameters, from_block, to_block, completeness,
              calls, note, ran_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                run.chain,
                run.token,
                run.what,
                run.parameters,
                to_i64(run.from_block),
                to_i64(run.to_block),
                run.completeness.as_str(),
                run.calls,
                run.note,
                to_unix(run.ran_at),
            ],
        )?;
        tx.execute(
            "DELETE FROM check_runs WHERE chain = ?1 AND token = ?2 AND what = ?3
             AND id NOT IN (
                SELECT id FROM check_runs WHERE chain = ?1 AND token = ?2 AND what = ?3
                ORDER BY id DESC LIMIT ?4)",
            params![run.chain, run.token, run.what, to_i64(MAX_CHECK_RUNS)],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The most recent check run for `(chain, token, what)`, if any.
    ///
    /// # Errors
    ///
    /// [`Error::Sqlite`] if the read fails.
    pub fn latest_check_run(
        &self,
        chain: &str,
        token: &str,
        what: &str,
    ) -> Result<Option<CheckRun>, Error> {
        self.conn
            .query_row(
                "SELECT parameters, from_block, to_block, completeness, calls, note, ran_at
                 FROM check_runs WHERE chain = ?1 AND token = ?2 AND what = ?3
                 ORDER BY id DESC LIMIT 1",
                params![chain, token, what],
                |row| {
                    Ok(CheckRun {
                        chain: chain.to_owned(),
                        token: token.to_owned(),
                        what: what.to_owned(),
                        parameters: row.get(0)?,
                        from_block: u64::try_from(row.get::<_, i64>(1)?).unwrap_or(0),
                        to_block: u64::try_from(row.get::<_, i64>(2)?).unwrap_or(0),
                        completeness: Completeness::parse(&row.get::<_, String>(3)?)
                            .unwrap_or(Completeness::Failed),
                        calls: row.get(4)?,
                        note: row.get(5)?,
                        ran_at: from_unix(row.get(6)?),
                    })
                },
            )
            .optional()
            .map_err(Error::from)
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Credit,
    Debit,
}

/// Moves `holder`'s balance of `token` by `amount`. Stored as decimal text
/// because SQLite's integer is 64 bits and a token amount is 128.
fn adjust_balance(
    conn: &Connection,
    chain: &str,
    token: &str,
    holder: &str,
    amount: u128,
    direction: Direction,
) -> Result<(), Error> {
    let current: Option<String> = conn
        .query_row(
            "SELECT balance FROM token_balances WHERE chain = ?1 AND token = ?2 AND holder = ?3",
            params![chain, token, holder],
            |row| row.get(0),
        )
        .optional()?;
    let current = current.as_deref().map_or(Ok(0), parse_amount)?;
    let next = match direction {
        Direction::Credit => current.saturating_add(amount),
        Direction::Debit => current.checked_sub(amount).ok_or_else(|| Error::Ledger {
            token: token.to_owned(),
            why: format!("{holder} would spend {amount} holding {current}"),
        })?,
    };
    conn.execute(
        "INSERT INTO token_balances (chain, token, holder, balance) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (chain, token, holder) DO UPDATE SET balance = excluded.balance",
        params![chain, token, holder, next.to_string()],
    )?;
    Ok(())
}

fn parse_amount(text: &str) -> Result<u128, Error> {
    text.parse::<u128>().map_err(|_| Error::Ledger {
        token: String::new(),
        why: format!("stored amount {text:?} is not a number"),
    })
}

fn to_i64(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
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

    // -- event memory (design 0021 §9) --

    const CHAIN: &str = "robinhood";
    const TOKEN: &str = "0xtoken";

    fn transfer(
        block: u64,
        log_index: u64,
        from: Option<&str>,
        to: &str,
        amount: u128,
    ) -> TransferEvent {
        TransferEvent {
            block,
            block_hash: format!("0xhash{block}"),
            transaction_index: 0,
            log_index,
            transaction: format!("0xtx{block}_{log_index}"),
            from: from.map(str::to_owned),
            to: to.to_owned(),
            amount,
        }
    }

    fn checkpoint(block: u64) -> Checkpoint {
        Checkpoint {
            block,
            hash: format!("0xhash{block}"),
        }
    }

    #[test]
    fn a_retried_range_moves_no_balance_twice() {
        let mem = Memory::open_in_memory().expect("open");
        let events = [
            transfer(10, 0, None, "curve", 1_000),
            transfer(11, 0, Some("curve"), "alice", 300),
            transfer(11, 1, Some("alice"), "bob", 100),
        ];
        let first = mem
            .extend_transfers(CHAIN, TOKEN, &events, &checkpoint(20))
            .expect("first extend");
        assert_eq!(
            first,
            Extended {
                inserted: 3,
                duplicates: 0
            }
        );
        let before = mem.token_balances(CHAIN, TOKEN).expect("balances");

        // The same range again, as a retry after a lost answer would send it.
        let again = mem
            .extend_transfers(CHAIN, TOKEN, &events, &checkpoint(20))
            .expect("retried extend");
        assert_eq!(
            again,
            Extended {
                inserted: 0,
                duplicates: 3
            }
        );
        assert_eq!(mem.token_balances(CHAIN, TOKEN).expect("balances"), before);
        assert_eq!(
            before,
            vec![
                ("alice".to_owned(), 200),
                ("bob".to_owned(), 100),
                ("curve".to_owned(), 700),
            ]
        );
        assert_eq!(
            mem.token_checkpoint(CHAIN, TOKEN).expect("checkpoint"),
            Some(checkpoint(20))
        );
    }

    #[test]
    fn a_reorged_suffix_is_rolled_back_and_the_re_read_lands_right() {
        let mem = Memory::open_in_memory().expect("open");
        mem.extend_transfers(
            CHAIN,
            TOKEN,
            &[
                transfer(10, 0, None, "curve", 1_000),
                transfer(11, 0, Some("curve"), "alice", 300),
                // Block 30 is the part that will turn out to be reorged away.
                transfer(30, 0, Some("alice"), "bob", 250),
            ],
            &checkpoint(30),
        )
        .expect("extend");

        // The chain's block 30 now has another hash: everything after the
        // last block still trusted (20) is forgotten and its effect undone.
        let forgotten = mem
            .roll_back_transfers_after(CHAIN, TOKEN, 20)
            .expect("roll back");
        assert_eq!(forgotten, 1);
        assert_eq!(
            mem.token_checkpoint(CHAIN, TOKEN).expect("checkpoint"),
            None,
            "a rolled-back memory is complete through nothing until re-read"
        );
        assert_eq!(
            mem.token_balances(CHAIN, TOKEN).expect("balances"),
            vec![("alice".to_owned(), 300), ("curve".to_owned(), 700)]
        );

        // The re-read: in the canonical block 30 Alice sent Bob 50, not 250.
        let reread = TransferEvent {
            block_hash: "0xcanonical30".to_owned(),
            ..transfer(30, 0, Some("alice"), "bob", 50)
        };
        mem.extend_transfers(
            CHAIN,
            TOKEN,
            &[reread],
            &Checkpoint {
                block: 30,
                hash: "0xcanonical30".to_owned(),
            },
        )
        .expect("re-read");
        assert_eq!(
            mem.token_balances(CHAIN, TOKEN).expect("balances"),
            vec![
                ("alice".to_owned(), 250),
                ("bob".to_owned(), 50),
                ("curve".to_owned(), 700),
            ]
        );
    }

    #[test]
    fn a_debit_below_zero_commits_nothing() {
        let mem = Memory::open_in_memory().expect("open");
        let err = mem
            .extend_transfers(
                CHAIN,
                TOKEN,
                &[
                    transfer(10, 0, None, "curve", 100),
                    transfer(10, 1, Some("alice"), "bob", 1),
                ],
                &checkpoint(10),
            )
            .expect_err("alice never received anything");
        assert!(matches!(err, Error::Ledger { .. }), "{err}");
        // Atomic: the mint that preceded the bad debit was not kept either,
        // and no checkpoint claims the range is covered.
        assert!(
            mem.token_balances(CHAIN, TOKEN)
                .expect("balances")
                .is_empty()
        );
        assert_eq!(
            mem.token_checkpoint(CHAIN, TOKEN).expect("checkpoint"),
            None
        );
    }

    #[test]
    fn an_empty_complete_range_and_a_failed_query_are_different_records() {
        let mem = Memory::open_in_memory().expect("open");
        let run = |completeness, note: &str| CheckRun {
            chain: CHAIN.to_owned(),
            token: TOKEN.to_owned(),
            what: "transfers".to_owned(),
            parameters: "v1".to_owned(),
            from_block: 100,
            to_block: 200,
            completeness,
            calls: 1,
            note: note.to_owned(),
            ran_at: secs(1_000),
        };
        mem.record_check_run(&run(Completeness::Complete, ""))
            .expect("record");
        let latest = mem
            .latest_check_run(CHAIN, TOKEN, "transfers")
            .expect("read")
            .expect("present");
        assert_eq!(latest.completeness, Completeness::Complete);
        assert_eq!((latest.from_block, latest.to_block), (100, 200));

        mem.record_check_run(&run(Completeness::Failed, "eth_getLogs: timeout"))
            .expect("record");
        let latest = mem
            .latest_check_run(CHAIN, TOKEN, "transfers")
            .expect("read")
            .expect("present");
        assert_eq!(latest.completeness, Completeness::Failed);
        assert_eq!(latest.note, "eth_getLogs: timeout");
        for c in [
            Completeness::Complete,
            Completeness::Failed,
            Completeness::Truncated,
        ] {
            assert_eq!(Completeness::parse(c.as_str()), Some(c));
        }
    }

    #[test]
    fn transfers_past_the_cap_are_pruned_oldest_first_and_balances_survive() {
        let mem = Memory::open_in_memory().expect("open");
        let count = MAX_TRANSFERS_PER_TOKEN + 10;
        let events: Vec<TransferEvent> = (0..count)
            .map(|i| transfer(i, 0, None, "alice", 1))
            .collect();
        // The checkpoint is far past every event, so all of them are final.
        let through = checkpoint(count + REORG_DEPTH + 1);
        mem.extend_transfers(CHAIN, TOKEN, &events, &through)
            .expect("extend");
        let held: i64 = mem
            .conn
            .query_row("SELECT COUNT(*) FROM transfers", [], |row| row.get(0))
            .expect("count");
        assert_eq!(u64::try_from(held).expect("fits"), MAX_TRANSFERS_PER_TOKEN);
        let oldest: i64 = mem
            .conn
            .query_row("SELECT MIN(block) FROM transfers", [], |row| row.get(0))
            .expect("min");
        assert_eq!(oldest, 10, "the ten oldest rows went, not the newest");
        assert_eq!(
            mem.token_balances(CHAIN, TOKEN).expect("balances"),
            vec![("alice".to_owned(), u128::from(count))]
        );
    }

    #[test]
    fn a_funding_edge_is_remembered_once_by_its_provider_identity() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mem = Memory::open(&dir.path().join("m.sqlite3")).expect("open");
        let edge = |unique_id: &str, amount: u128, material: bool| FundingEdge {
            recipient: "buyer".to_owned(),
            funder: "funder".to_owned(),
            block: 7,
            transaction: "0xaa".to_owned(),
            unique_id: unique_id.to_owned(),
            amount,
            material,
        };
        // Two identical top-ups with different identities are two events;
        // the same identity read twice (a retried page) is one.
        let first = [
            edge("0xaa:external:0", 5, true),
            edge("0xaa:external:1", 5, false),
        ];
        assert_eq!(
            mem.record_funding_edges(CHAIN, TOKEN, &first)
                .expect("record"),
            2
        );
        assert_eq!(
            mem.record_funding_edges(CHAIN, TOKEN, &first)
                .expect("again"),
            0
        );
        let back = mem.funding_edges(CHAIN, TOKEN).expect("read");
        assert_eq!(back, first.to_vec());
        assert!(back[0].material && !back[1].material);
        // Scoped to the token asked about.
        assert!(mem.funding_edges(CHAIN, "other").expect("read").is_empty());
    }

    #[test]
    fn a_creator_trade_is_remembered_once_by_its_log_identity() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mem = Memory::open(&dir.path().join("m.sqlite3")).expect("open");
        let trade = |unique_id: &str, role: &str| CreatorTradeEvent {
            token: TOKEN.to_owned(),
            role: role.to_owned(),
            side: "sell".to_owned(),
            quote: 100,
            tokens: 40,
            block: 7,
            transaction: "0xaa".to_owned(),
            unique_id: unique_id.to_owned(),
        };
        // Two distinct log positions in the same transaction are two events;
        // the same log identity read twice (a retried page) is one.
        let first = [
            trade("0xaa-0-0", "deployer"),
            trade("0xaa-0-1", "fee_recipient"),
        ];
        assert_eq!(mem.record_creator_trades(CHAIN, &first).expect("record"), 2);
        assert_eq!(mem.record_creator_trades(CHAIN, &first).expect("again"), 0);
        let back = mem.creator_trades(CHAIN, TOKEN).expect("read");
        assert_eq!(back, first.to_vec());
        // Scoped to the token asked about.
        assert!(mem.creator_trades(CHAIN, "other").expect("read").is_empty());
    }

    #[test]
    fn a_wallet_seen_buying_two_launches_is_retrievable_by_address() {
        let mem = Memory::open_in_memory().expect("open");
        mem.record_buy(CHAIN, "wallet", "TOKEN_A", 10, 111)
            .expect("record A");
        mem.record_buy(CHAIN, "wallet", "TOKEN_B", 20, 222)
            .expect("record B");
        // A different wallet buying the same launch must not show up here --
        // the index is scoped by buyer, not by launch.
        mem.record_buy(CHAIN, "someone else", "TOKEN_A", 10, 999)
            .expect("record other buyer");

        let launches = mem.launches_bought_by(CHAIN, "wallet").expect("read");
        assert_eq!(
            launches,
            vec![
                BuyerLaunch {
                    token: "TOKEN_A".to_owned(),
                    block: 10,
                    amount: 111,
                },
                BuyerLaunch {
                    token: "TOKEN_B".to_owned(),
                    block: 20,
                    amount: 222,
                },
            ]
        );

        // A wallet never recorded gets an empty list, not an error --
        // "never bought" and "unread" both settle to nothing here, and the
        // list length is the count S8's recurrence factor needs.
        assert!(
            mem.launches_bought_by(CHAIN, "nobody")
                .expect("read")
                .is_empty()
        );
    }

    #[test]
    fn re_recording_the_same_buy_is_idempotent() {
        let mem = Memory::open_in_memory().expect("open");
        assert!(
            mem.record_buy(CHAIN, "wallet", TOKEN, 10, 111)
                .expect("first record"),
            "the first record of a key must insert"
        );
        // Same (chain, buyer, token) key, even with a different block or
        // amount than the first call: the sheet re-running over the same
        // launch window must not fail or overwrite the first-seen buy.
        assert!(
            !mem.record_buy(CHAIN, "wallet", TOKEN, 999, 1)
                .expect("re-record"),
            "a re-record of the same key must be a no-op, not a second insert"
        );

        let launches = mem.launches_bought_by(CHAIN, "wallet").expect("read");
        assert_eq!(
            launches,
            vec![BuyerLaunch {
                token: TOKEN.to_owned(),
                block: 10,
                amount: 111,
            }],
            "the first-seen block and amount must stand"
        );
    }

    #[test]
    fn opening_a_pre_buyer_index_database_gains_the_table_without_losing_facts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("old.sqlite3");

        // A database created with only the schema that predates the buyer
        // index (the `facts` table this module always had) -- standing in
        // for a memory file on disk before this migration shipped.
        {
            let conn = Connection::open(&path).expect("create old db");
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
            )
            .expect("old schema");
            conn.execute(
                "INSERT INTO facts (what, subject, block, read_at, kind, value)
                 VALUES ('launch record', 'TOKEN', 100, 1000, 'forever', 'creator=ABC')",
                [],
            )
            .expect("seed old row");
        }

        // Opening it through today's `Memory::open` must add the buyer
        // index table alongside the pre-existing one, and must not touch
        // the row that was already there.
        let mem = Memory::open(&path).expect("open old db");
        let fact = mem
            .latest("launch record", "TOKEN")
            .expect("read")
            .expect("the pre-existing row must survive the migration");
        assert_eq!(fact.value, "creator=ABC");
        assert_eq!(fact.block, 100);

        mem.record_buy(CHAIN, "wallet", "TOKEN", 100, 5)
            .expect("the buyer index table must now exist");
        assert_eq!(
            mem.launches_bought_by(CHAIN, "wallet").expect("read").len(),
            1
        );
    }
}
