// SPDX-License-Identifier: Apache-2.0
//! The research store (design [0032](../../../docs/design/0032-the-research-store.md)):
//! one SQLite [`Connection`], no pool, no async runtime, no clock read and no
//! key held. Every timestamp a row carries is passed in by the caller --
//! this crate has no clock, the same discipline `realorrug-contest` holds
//! for its own purity (§6: "no clock, no network, no key").
//!
//! # What this crate is not
//!
//! It is not HTTP, sign-in, settlement or a site page (those are phase 4
//! steps 4-3 through 4-7). It holds records of what was said and read,
//! never judgement: it cannot compute a verdict, hold a spending key, or
//! attach value to a call or a reputation figure (AGENTS.md §3 rule 1;
//! design 0032 §1 "What still protects it"). Nothing here serialises
//! [`realorrug_contest::calls::points`] or a computed winner -- a caller may
//! read a hit/miss count or a rank, never a value the store attached itself.
//!
//! # Deny by default
//!
//! [`Store::open`] is the only way in; there is no ambient global connection.
//! A caller with no path configured simply never calls [`Store::open`] --
//! the same shape `realorrug-serve/src/record.rs` uses for its own verdict
//! write (`let Some(path) = memory_path else { return; };`).
//!
//! # Append-only, hash-chained (§3)
//!
//! Every row carries the previous row's digest ([`chain::digest`]), and the
//! hash function is the one `realorrug-journal` uses (blake3); the field
//! encoding is this crate's own. A row is never updated or deleted -- no path
//! here does it and triggers refuse it -- and a correction is a new row.
//! Every append reads the tail and inserts inside one `BEGIN IMMEDIATE`, and a
//! unique index on the previous hash refuses a fork. [`Store::verify`] walks
//! the chain and reports the first break, and the head to record.
//!
//! # Identity is separate (§4)
//!
//! Chain rows name a player only by a random player key, never an X id or
//! handle. Those live in a separate, mutable `identity` table
//! ([`Store::upsert_identity`], [`Store::delete_identity`]) outside the
//! append-only chain -- deleting an identity leaves the chain rows valid and
//! unlinked.

mod chain;
mod identity;
mod row;

pub use identity::{Identity, PlayerKey};
pub use row::{ClosedForecast, ForecastView, OutcomeView, RowPayload, SettledForecast, Verified};

use std::collections::HashMap;
use std::path::Path;

use realorrug_contest::calls::{Odds, Outcome, Side};
use rusqlite::{Connection, OptionalExtension, params};

/// What stopped a store operation.
///
/// Every variant is a refusal to proceed, the same discipline
/// `realorrug-journal`'s `JournalError` documents: a caller that receives one
/// must not act as though the write happened.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The underlying SQLite call failed.
    #[error("store: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A row could not be turned into or read back from its stored form.
    #[error("store: a row could not be serialised: {0}")]
    Serialise(#[from] serde_json::Error),
    /// A forecast arrived at or after its round's window close (design 0032
    /// §9: "refused by the server clock, not the client's" -- the caller
    /// passes `now`, this refuses by comparing it to the round's close it
    /// was also given). At the close itself is already too late: that is the
    /// instant reveals begin.
    #[error("the window closed at {window_close} and this forecast arrived at {now}")]
    WindowClosed {
        /// When the round's window closed.
        window_close: i64,
        /// When the caller says the forecast arrived.
        now: i64,
    },
    /// A second forecast against the same `(round, coin, player)` -- the
    /// first stands (design 0032 §2, §9). `realorrug-serve` maps this to
    /// `409 Conflict`.
    #[error("a forecast for this round, coin and player already stands")]
    Duplicate,
    /// A second identity for an X account that already has one. One X id has
    /// one player key (design 0032 §4); look it up with
    /// [`Store::player_for_x_id`] instead of minting another.
    #[error("an identity for this X account already exists")]
    XIdTaken,
    /// An identity write for an existing player key named a different X id.
    /// A key never moves to another X account: that would hand one person's
    /// chain history to someone else.
    #[error("this player key belongs to another X account")]
    KeyTaken,
    /// A database constraint other than the one-forecast rule refused the
    /// write. Not `Duplicate`: a caller that maps it to `409 Conflict` would
    /// tell a player their call was a repeat when the chain refused it for
    /// another reason.
    #[error("store: a constraint refused the write: {0}")]
    Constraint(String),
    /// A stored row is not what its own kind says it is. The chain check
    /// ([`Store::verify`]) is the tool that names where; this refuses to read
    /// on rather than guess.
    #[error("store: a stored row is corrupt: {0}")]
    Corrupt(String),
}

/// A constraint failure becomes [`StoreError::Constraint`]; anything else stays
/// [`StoreError::Sqlite`].
pub(crate) fn constraint_or_sqlite(e: rusqlite::Error) -> StoreError {
    match &e {
        rusqlite::Error::SqliteFailure(f, _)
            if f.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            StoreError::Constraint(e.to_string())
        }
        _ => StoreError::Sqlite(e),
    }
}

/// The research store: one SQLite [`Connection`] behind `&self`, because
/// SQLite serialises writes on one file regardless
/// (`crates/realorrug-onchain/src/memory.rs`'s `Memory`, same doc comment,
/// same reason).
pub struct Store {
    conn: Connection,
    /// Test-only: runs once between an append's tail read and its INSERT, so
    /// a test can put a second connection in exactly the window a race would.
    #[cfg(test)]
    between_tail_and_insert: std::cell::RefCell<Option<Box<dyn FnOnce()>>>,
}

impl Store {
    /// Opens (creating if absent) the store at `path`.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] if the file cannot be opened or the schema
    /// cannot be created.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    /// An in-memory store, for tests that do not need [`Store::open`]'s
    /// close-and-reopen durability.
    #[cfg(test)]
    pub(crate) fn open_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, StoreError> {
        conn.execute_batch(
            // Deleting an identity must remove its bytes from the file, not
            // only its row: without this SQLite leaves a deleted page's
            // content in the free list, and the X id would stay readable
            // in the file after "deletion" (design 0032 §4).
            "PRAGMA secure_delete = ON;
             CREATE TABLE IF NOT EXISTS rows (
                seq           INTEGER PRIMARY KEY AUTOINCREMENT,
                kind          TEXT    NOT NULL,
                round         TEXT    NOT NULL,
                chain         TEXT    NOT NULL,
                token         TEXT    NOT NULL,
                player_key    TEXT,
                at            INTEGER NOT NULL,
                payload       TEXT    NOT NULL,
                previous_hash TEXT    NOT NULL,
                hash          TEXT    NOT NULL
             );
             -- The first stands (design 0032 §2, §9): one forecast per
             -- (round, coin, player). Partial, because outcome/evidence/
             -- discussion rows share this table and have no such rule.
             CREATE UNIQUE INDEX IF NOT EXISTS ux_forecast_once
                ON rows(round, chain, token, player_key)
                WHERE kind = 'forecast';
             -- One row per predecessor: two rows cannot both extend the same
             -- tail, so a fork is refused by the schema even if a writer skips
             -- the lock. The first row chains from the fixed genesis constant,
             -- and only one row can hold it, which is right.
             CREATE UNIQUE INDEX IF NOT EXISTS ux_chain_link ON rows(previous_hash);
             -- Append-only is enforced here as well as by having no update
             -- path in this crate: a raw UPDATE or DELETE on a file this
             -- process has open is refused. The tamper tests DROP these first,
             -- the way a host attacker with file access would have to.
             CREATE TRIGGER IF NOT EXISTS rows_no_update BEFORE UPDATE ON rows
             BEGIN SELECT RAISE(ABORT, 'rows are append-only'); END;
             CREATE TRIGGER IF NOT EXISTS rows_no_delete BEFORE DELETE ON rows
             BEGIN SELECT RAISE(ABORT, 'rows are append-only'); END;
             CREATE TABLE IF NOT EXISTS identity (
                player_key         TEXT    PRIMARY KEY,
                x_id                TEXT    NOT NULL UNIQUE,
                handle              TEXT    NOT NULL,
                account_created_at  INTEGER,
                session_hash        TEXT,
                signed_in_at        INTEGER NOT NULL
             );",
        )?;
        Ok(Self {
            conn,
            #[cfg(test)]
            between_tail_and_insert: std::cell::RefCell::new(None),
        })
    }

    /// A fresh player key: 128 bits from SQLite's `randomblob`, lower-case
    /// hex, 32 characters. SQLite seeds its generator from the operating
    /// system, so the key is not derived from an X id, a counter or the
    /// clock (design 0032 §4). It is not stored until an identity row or a
    /// chain row uses it.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] if the query fails.
    pub fn new_player_key(&self) -> Result<PlayerKey, StoreError> {
        let key: String = self
            .conn
            .query_row("SELECT lower(hex(randomblob(16)))", [], |r| r.get(0))?;
        Ok(PlayerKey::from_stored(key))
    }

    /// The player key already on file for an X account, so a returning user
    /// keeps one key (and one record) instead of getting a new one each
    /// sign-in.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] on a read failure.
    pub fn player_for_x_id(&self, x_id: &str) -> Result<Option<PlayerKey>, StoreError> {
        identity::player_for_x_id(&self.conn, x_id)
    }

    /// The player a session belongs to, with the time the session was issued
    /// (the identity row's `signed_in_at`), or `None` for a hash no identity
    /// holds. The caller decides expiry from the issue time and its own clock.
    ///
    /// An identity holds one `session_hash`, so a new sign-in replaces the old
    /// session, and a deleted identity takes its session with it.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] on a read failure.
    pub fn player_for_session_hash(
        &self,
        session_hash: &str,
    ) -> Result<Option<(PlayerKey, i64)>, StoreError> {
        identity::player_for_session_hash(&self.conn, session_hash)
    }

    /// Runs `f` inside `BEGIN IMMEDIATE` ... `COMMIT`, rolling back on error.
    ///
    /// `IMMEDIATE` takes SQLite's write lock at the start, so the tail read in
    /// `f` and its INSERT are one unit: a second connection on the same file
    /// waits (or gets `SQLITE_BUSY`) instead of reading the same tail and
    /// chaining a second row to it. Two autocommit statements do not give
    /// that, whatever the connection count.
    fn with_write<T>(&self, f: impl FnOnce() -> Result<T, StoreError>) -> Result<T, StoreError> {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        match f() {
            Ok(value) => match self.conn.execute_batch("COMMIT") {
                Ok(()) => Ok(value),
                Err(e) => {
                    let _ = self.conn.execute_batch("ROLLBACK");
                    Err(e.into())
                }
            },
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// The digest of the last row written, or [`chain::GENESIS`] if the
    /// store is empty. Read fresh on every write rather than cached on
    /// `self`, and only ever inside [`Store::with_write`]: the read and the
    /// INSERT that extends it must be one locked unit, or a second connection
    /// could read the same tail in between. Reading fresh is necessary but
    /// not sufficient; the lock is what makes it correct.
    fn tail(&self) -> Result<String, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT hash FROM rows ORDER BY seq DESC LIMIT 1", [], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or_else(|| chain::GENESIS.to_owned()))
    }

    /// Appends one row to the chain in its own write transaction. Every
    /// public write method except `submit_forecast` funnels through here, and
    /// that one uses [`Store::append_in_write`] inside a transaction of its
    /// own, so the chain's shape cannot drift between them.
    #[expect(
        clippy::too_many_arguments,
        reason = "one column per argument, the row's own fields"
    )]
    fn append(
        &self,
        kind: &str,
        round: &str,
        chain_name: &str,
        token: &str,
        player_key: Option<&PlayerKey>,
        at: i64,
        payload: &RowPayload,
    ) -> Result<(), StoreError> {
        self.with_write(|| {
            self.append_in_write(kind, round, chain_name, token, player_key, at, payload)
        })
    }

    /// The tail read and the INSERT. Call only inside [`Store::with_write`].
    #[expect(
        clippy::too_many_arguments,
        reason = "one column per argument, the row's own fields"
    )]
    fn append_in_write(
        &self,
        kind: &str,
        round: &str,
        chain_name: &str,
        token: &str,
        player_key: Option<&PlayerKey>,
        at: i64,
        payload: &RowPayload,
    ) -> Result<(), StoreError> {
        let previous = self.tail()?;
        #[cfg(test)]
        if let Some(hook) = self.between_tail_and_insert.borrow_mut().take() {
            hook();
        }
        let payload_json = serde_json::to_string(payload)?;
        let player_key = player_key.map(PlayerKey::as_str);
        let digest = chain::digest(
            &previous,
            &[
                kind,
                round,
                chain_name,
                token,
                &chain::player_field(player_key),
                &at.to_string(),
                &payload_json,
            ],
        );
        self.conn
            .execute(
                "INSERT INTO rows (kind, round, chain, token, player_key, at, payload, \
                 previous_hash, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    kind,
                    round,
                    chain_name,
                    token,
                    player_key,
                    at,
                    payload_json,
                    previous,
                    digest,
                ],
            )
            .map_err(constraint_or_sqlite)?;
        Ok(())
    }

    /// Submits a forecast. Refused after the round's window closes
    /// ([`StoreError::WindowClosed`]) or if this player already has one for
    /// this round and coin ([`StoreError::Duplicate`], the first stands).
    ///
    /// `now` and `window_close` are both the caller's clock, never this
    /// crate's -- it has none.
    ///
    /// # Errors
    ///
    /// See [`StoreError::WindowClosed`] and [`StoreError::Duplicate`], and
    /// [`StoreError::Sqlite`] for any other failure.
    #[expect(clippy::too_many_arguments, reason = "the forecast row's own fields")]
    pub fn submit_forecast(
        &self,
        round: &str,
        chain_name: &str,
        token: &str,
        player_key: &PlayerKey,
        side: Side,
        q: Odds,
        now: i64,
        window_close: i64,
    ) -> Result<(), StoreError> {
        // `>=`, not `>`: `forecast` reveals a call to everyone at
        // `now >= window_close`, so a call submitted at exactly the close would
        // let a second player read the first one's and still enter (design
        // 0028 §2.4: at close, entry shuts and the reveal begins).
        if now >= window_close {
            return Err(StoreError::WindowClosed { window_close, now });
        }
        let payload = RowPayload::Forecast {
            side,
            q_basis_points: q.basis_points(),
            window_close,
        };
        self.with_write(|| {
            // The one-forecast rule is checked here, under the write lock, and
            // not by catching the unique index's error: that error is
            // indistinguishable from any other constraint failure without
            // parsing its message, and a caller that maps every constraint
            // failure to `Duplicate` tells a player "you already called it"
            // when the chain refused for another reason. The index stays as
            // the backstop.
            let exists: bool = self.conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM rows WHERE kind = 'forecast' AND round = ?1 \
                 AND chain = ?2 AND token = ?3 AND player_key = ?4)",
                params![round, chain_name, token, player_key.as_str()],
                |r| r.get(0),
            )?;
            if exists {
                return Err(StoreError::Duplicate);
            }
            self.append_in_write(
                "forecast",
                round,
                chain_name,
                token,
                Some(player_key),
                now,
                &payload,
            )
        })
    }

    /// Reads one player's forecast for a round and coin, if the requester is
    /// allowed to see it (design 0032 §9, "Authors see their own call"):
    /// the row's own author, at any time, or anyone once `now` has reached
    /// the row's `window_close`. Answers `Ok(None)` for anyone else before
    /// close -- it leaks nothing, not even that a call exists.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] or [`StoreError::Serialise`] on a read
    /// failure.
    pub fn forecast(
        &self,
        round: &str,
        chain_name: &str,
        token: &str,
        target_player: &str,
        requester_player: &str,
        now: i64,
    ) -> Result<Option<ForecastView>, StoreError> {
        let Some((payload_json, at)) = self
            .conn
            .query_row(
                "SELECT payload, at FROM rows WHERE kind = 'forecast' AND round = ?1 \
                 AND chain = ?2 AND token = ?3 AND player_key = ?4",
                params![round, chain_name, token, target_player],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?
        else {
            return Ok(None);
        };
        let RowPayload::Forecast {
            side,
            q_basis_points,
            window_close,
        } = serde_json::from_str(&payload_json)?
        else {
            return Err(StoreError::Corrupt(format!(
                "a row of kind 'forecast' holds another kind's payload (round {round}, coin {token})"
            )));
        };
        let visible = target_player == requester_player || now >= window_close;
        if !visible {
            return Ok(None);
        }
        Ok(Some(ForecastView {
            player_key: target_player.to_owned(),
            side,
            q_basis_points,
            submitted_at: at,
            window_close,
        }))
    }

    /// Records an outcome (design 0032 §2, §9): what the settlement job
    /// found at the round's horizon, or [`Outcome::Unresolved`] when it
    /// could not produce a definitive reading.
    ///
    /// `outcome` is [`realorrug_contest::calls::Outcome`], reused rather
    /// than duplicated (design 0032 §6). `evidence_reference` names what the
    /// outcome rests on, never a copy of it (§2's "evidence references").
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] or [`StoreError::Serialise`] on a write
    /// failure. Nothing here refuses a second outcome for the same round and
    /// coin -- a correction is a new row (§3), and picking which of several
    /// outcome rows is authoritative is a caller question this pure store
    /// does not answer for itself.
    #[expect(clippy::too_many_arguments, reason = "the outcome row's own fields")]
    pub fn record_outcome(
        &self,
        round: &str,
        chain_name: &str,
        token: &str,
        outcome: Outcome,
        rule_version: &str,
        evidence_reference: Option<&str>,
        settled_at: i64,
    ) -> Result<(), StoreError> {
        let payload = RowPayload::Outcome {
            outcome,
            rule_version: rule_version.to_owned(),
            evidence_reference: evidence_reference.map(str::to_owned),
        };
        self.append(
            "outcome", round, chain_name, token, None, settled_at, &payload,
        )
    }

    /// The most recently recorded outcome for a round and coin, if any.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] or [`StoreError::Serialise`] on a read
    /// failure.
    pub fn outcome(
        &self,
        round: &str,
        chain_name: &str,
        token: &str,
    ) -> Result<Option<OutcomeView>, StoreError> {
        let Some((payload_json, at)) = self
            .conn
            .query_row(
                "SELECT payload, at FROM rows WHERE kind = 'outcome' AND round = ?1 \
                 AND chain = ?2 AND token = ?3 ORDER BY seq DESC LIMIT 1",
                params![round, chain_name, token],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?
        else {
            return Ok(None);
        };
        let RowPayload::Outcome {
            outcome,
            rule_version,
            evidence_reference,
        } = serde_json::from_str(&payload_json)?
        else {
            return Err(StoreError::Corrupt(format!(
                "a row of kind 'outcome' holds another kind's payload (round {round}, coin {token})"
            )));
        };
        Ok(Some(OutcomeView {
            outcome,
            rule_version,
            evidence_reference,
            settled_at: at,
        }))
    }

    /// Every forecast of a round that anyone may read at `now`: those whose
    /// own recorded close has passed, with no player in them. A forecast still
    /// open is not in the list, so the list's length before close is zero for a
    /// round with a hundred hidden calls and for one with none.
    ///
    /// The close is the one each row carries (the server's, set at submit), not
    /// a figure the caller supplies, so a caller asking with a wrong round
    /// record cannot open a hidden call.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`], [`StoreError::Serialise`] or
    /// [`StoreError::Corrupt`] on a read failure.
    pub fn closed_forecasts(
        &self,
        round: &str,
        now: i64,
    ) -> Result<Vec<ClosedForecast>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT chain, token, payload, at FROM rows WHERE kind = 'forecast' \
             AND round = ?1 ORDER BY seq ASC",
        )?;
        let mut rows = statement.query(params![round])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let payload_json: String = row.get(2)?;
            let RowPayload::Forecast {
                side,
                q_basis_points,
                window_close,
            } = serde_json::from_str(&payload_json)?
            else {
                return Err(StoreError::Corrupt(
                    "a row of kind 'forecast' holds another kind's payload".to_owned(),
                ));
            };
            if now >= window_close {
                out.push(ClosedForecast {
                    chain: row.get(0)?,
                    token: row.get(1)?,
                    side,
                    q_basis_points,
                    submitted_at: row.get(3)?,
                });
            }
        }
        Ok(out)
    }

    /// Every closed forecast that has an outcome, joined to the latest outcome
    /// recorded for its coin: the input to the board's hit/miss count. A
    /// forecast whose close has not passed, or whose coin has no outcome yet,
    /// is not in the result.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`], [`StoreError::Serialise`] or
    /// [`StoreError::Corrupt`] on a read failure.
    pub fn settled_forecasts(&self, now: i64) -> Result<Vec<SettledForecast>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT kind, round, chain, token, player_key, at, payload FROM rows \
             WHERE kind IN ('forecast', 'outcome') ORDER BY seq ASC",
        )?;
        let mut rows = statement.query([])?;
        // The latest outcome per coin wins, as `outcome` says: rows are read
        // in `seq` order, so a later one overwrites an earlier one.
        let mut latest: HashMap<(String, String, String), (Outcome, i64)> = HashMap::new();
        let mut forecasts = Vec::new();
        while let Some(row) = rows.next()? {
            let kind: String = row.get(0)?;
            let coin = (row.get(1)?, row.get(2)?, row.get(3)?);
            let at: i64 = row.get(5)?;
            let payload_json: String = row.get(6)?;
            match serde_json::from_str(&payload_json)? {
                RowPayload::Outcome { outcome, .. } if kind == "outcome" => {
                    latest.insert(coin, (outcome, at));
                }
                RowPayload::Forecast {
                    side,
                    q_basis_points,
                    window_close,
                } if kind == "forecast" => {
                    let player: Option<String> = row.get(4)?;
                    if let (true, Some(player_key)) = (now >= window_close, player) {
                        forecasts.push((coin, player_key, side, q_basis_points, at));
                    }
                }
                _ => {
                    return Err(StoreError::Corrupt(format!(
                        "a row of kind '{kind}' holds another kind's payload"
                    )));
                }
            }
        }
        Ok(forecasts
            .into_iter()
            .filter_map(|(coin, player_key, side, q_basis_points, submitted_at)| {
                let (outcome, settled_at) = *latest.get(&coin)?;
                let (round, chain, token) = coin;
                Some(SettledForecast {
                    player_key,
                    round,
                    chain,
                    token,
                    side,
                    q_basis_points,
                    outcome,
                    submitted_at,
                    settled_at,
                })
            })
            .collect())
    }

    /// The public wording for an outcome (design 0032 §2, A4). `Stood` and
    /// `Rugged` are internal names only -- this is the sentence a site page
    /// or reply is allowed to show, never the enum's own spelling.
    #[must_use]
    pub fn public_wording(outcome: Outcome) -> &'static str {
        match outcome {
            Outcome::Rugged => "rug observed within the window",
            Outcome::Stood => "no qualifying rug observed",
            Outcome::Unresolved => "unresolved",
        }
    }

    /// Submits an evidence reference: a link or a sheet reference plus the
    /// submitter's note (design 0032 §2, "Evidence submission"). Never a
    /// fact the bot itself used (AGENTS.md §3 rule 2) -- this store holds it
    /// as the submitter's claim, nothing more.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] or [`StoreError::Serialise`] on a write
    /// failure.
    #[expect(clippy::too_many_arguments, reason = "the evidence row's own fields")]
    pub fn submit_evidence(
        &self,
        round: &str,
        chain_name: &str,
        token: &str,
        player_key: &PlayerKey,
        reference: &str,
        note: &str,
        at: i64,
    ) -> Result<(), StoreError> {
        let payload = RowPayload::Evidence {
            reference: reference.to_owned(),
            note: note.to_owned(),
        };
        self.append(
            "evidence",
            round,
            chain_name,
            token,
            Some(player_key),
            at,
            &payload,
        )
    }

    /// Submits a discussion comment (design 0032 §2, "Discussion / vote").
    /// Never settles a chain fact, an outcome, or the bot's verdict, and is
    /// never weighted into one -- this store only holds it.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] or [`StoreError::Serialise`] on a write
    /// failure.
    pub fn submit_discussion(
        &self,
        round: &str,
        chain_name: &str,
        token: &str,
        player_key: &PlayerKey,
        comment: &str,
        at: i64,
    ) -> Result<(), StoreError> {
        let payload = RowPayload::Discussion {
            comment: comment.to_owned(),
        };
        self.append(
            "discussion",
            round,
            chain_name,
            token,
            Some(player_key),
            at,
            &payload,
        )
    }

    /// Records or replaces a player's identity (design 0032 §4). The X id,
    /// handle, account age and session hash live only here, never in a
    /// chain row.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] on a write failure.
    pub fn upsert_identity(&self, identity: &Identity) -> Result<(), StoreError> {
        identity::upsert(&self.conn, identity)
    }

    /// Deletes a player's identity row. The chain rows naming this player's
    /// key are untouched and stay valid -- they were never linked to the X
    /// id or handle in the first place (design 0032 §4).
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] on a write failure.
    pub fn delete_identity(&self, player_key: &PlayerKey) -> Result<(), StoreError> {
        identity::delete(&self.conn, player_key)
    }

    /// Reads a player's identity, if one is on file.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] on a read failure.
    pub fn identity(&self, player_key: &PlayerKey) -> Result<Option<Identity>, StoreError> {
        identity::read(&self.conn, player_key)
    }

    /// Walks the whole chain and reports the first break, the same shape as
    /// `realorrug_journal::Journal::verify` (design 0032 §3).
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] on a read failure.
    pub fn verify(&self) -> Result<Verified, StoreError> {
        row::verify(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use realorrug_contest::calls::Side;
    use std::cell::Cell;
    use std::rc::Rc;

    fn odds(bp: u16) -> Odds {
        Odds::new(bp).expect("valid odds")
    }

    fn store() -> Store {
        Store::open_in_memory().expect("open")
    }

    fn key(store: &Store) -> PlayerKey {
        store.new_player_key().expect("a player key")
    }

    /// A forecast by `player` on `coin` in round `r1`, closing at `close`.
    fn forecast_on(
        store: &Store,
        player: &PlayerKey,
        coin: &str,
        now: i64,
        close: i64,
    ) -> Result<(), StoreError> {
        store.submit_forecast(
            "r1",
            "solana",
            coin,
            player,
            Side::Rug,
            odds(6_000),
            now,
            close,
        )
    }

    /// What a host attacker with file access has to do before editing a row:
    /// remove the triggers that refuse it.
    fn drop_guards(store: &Store) {
        store
            .conn
            .execute_batch("DROP TRIGGER rows_no_update; DROP TRIGGER rows_no_delete;")
            .expect("drop triggers");
    }

    fn intact(verified: Verified) -> (usize, String) {
        match verified {
            Verified::Intact { rows, head } => (rows, head),
            broken @ Verified::Broken { .. } => panic!("expected Intact, got {broken:?}"),
        }
    }

    fn broken_at(verified: Verified) -> i64 {
        match verified {
            Verified::Broken { at, .. } => at,
            intact @ Verified::Intact { .. } => panic!("expected Broken, got {intact:?}"),
        }
    }

    /// Three forecast rows, then the guards dropped, ready to be tampered
    /// with.
    fn three_rows_unguarded() -> Store {
        let store = store();
        let p = key(&store);
        forecast_on(&store, &p, "coin-a", 10, 100).expect("row 1");
        forecast_on(&store, &p, "coin-b", 11, 100).expect("row 2");
        forecast_on(&store, &p, "coin-c", 12, 100).expect("row 3");
        assert_eq!(intact(store.verify().expect("verify")).0, 3);
        drop_guards(&store);
        store
    }

    /// A forecast at the close is refused, because `forecast` reveals a call
    /// at `now >= window_close`: at exactly the close, entry shuts and the
    /// reveal begins (design 0028 §2.4; design 0032 §10).
    #[test]
    fn a_forecast_at_exactly_the_close_is_refused() {
        let store = store();
        let p = key(&store);
        let err = forecast_on(&store, &p, "coin-a", 100, 100).expect_err("a forecast at the close");
        assert!(matches!(
            err,
            StoreError::WindowClosed {
                window_close: 100,
                now: 100
            }
        ));
        // One tick before the close is still open.
        forecast_on(&store, &p, "coin-a", 99, 100).expect("a forecast one tick before the close");
    }

    /// After the close a forecast is refused (design 0032 §9).
    #[test]
    fn a_late_forecast_is_refused() {
        let store = store();
        let p = key(&store);
        let err = forecast_on(&store, &p, "coin-a", 101, 100).expect_err("late forecast");
        assert!(matches!(
            err,
            StoreError::WindowClosed {
                window_close: 100,
                now: 101
            }
        ));
        assert!(
            store
                .forecast("r1", "solana", "coin-a", p.as_str(), p.as_str(), 101)
                .expect("read")
                .is_none(),
            "a refused forecast must not have been written"
        );
    }

    /// A duplicate `(round, coin, player)` returns `Duplicate`, and the
    /// first forecast stands (design 0032 §2, §9).
    #[test]
    fn a_duplicate_returns_duplicate_and_the_first_stands() {
        let store = store();
        let p = key(&store);
        forecast_on(&store, &p, "coin-a", 10, 100).expect("first forecast");
        let err = store
            .submit_forecast(
                "r1",
                "solana",
                "coin-a",
                &p,
                Side::Real,
                odds(4_000),
                20,
                100,
            )
            .expect_err("duplicate");
        assert!(matches!(err, StoreError::Duplicate));
        // The refusal rolled its transaction back: without the ROLLBACK the
        // connection stays inside it and every later write fails.
        forecast_on(&store, &p, "coin-b", 21, 100)
            .expect("the connection is usable after a refusal");

        let view = store
            .forecast("r1", "solana", "coin-a", p.as_str(), p.as_str(), 100)
            .expect("read")
            .expect("a forecast exists");
        assert_eq!(
            view.side,
            Side::Rug,
            "the first call stands, not the second"
        );
        assert_eq!(view.submitted_at, 10);
    }

    /// A constraint other than the one-forecast rule is not a duplicate: a
    /// caller that maps `Duplicate` to `409 Conflict` must not tell a player
    /// their call was a repeat when the chain refused it for another reason.
    #[test]
    fn a_non_duplicate_constraint_is_not_reported_as_duplicate() {
        let store = store();
        let p = key(&store);
        store
            .conn
            .execute_batch(
                "CREATE TRIGGER refuse_all BEFORE INSERT ON rows \
                 BEGIN SELECT RAISE(ABORT, 'refused for another reason'); END;",
            )
            .expect("trigger");
        let err = forecast_on(&store, &p, "coin-a", 10, 100).expect_err("refused");
        assert!(
            matches!(err, StoreError::Constraint(_)),
            "expected Constraint, got {err:?}"
        );
    }

    /// A tampered middle row makes `verify` name that row.
    #[test]
    fn a_tampered_middle_row_makes_verify_name_that_row() {
        let store = three_rows_unguarded();
        // Tamper with the middle row's payload directly, the way a host
        // attacker with file access would -- not through this crate's API,
        // which has no update path at all.
        store
            .conn
            .execute(
                "UPDATE rows SET payload = '{\"kind\":\"tampered\"}' WHERE seq = 2",
                [],
            )
            .expect("tamper");
        assert_eq!(broken_at(store.verify().expect("verify")), 2);
    }

    /// The triggers refuse a raw UPDATE and a raw DELETE while they stand,
    /// so tampering needs the file, not just a connection.
    #[test]
    fn a_raw_update_or_delete_on_a_row_is_refused() {
        let store = store();
        let p = key(&store);
        forecast_on(&store, &p, "coin-a", 10, 100).expect("row");
        let update = store
            .conn
            .execute("UPDATE rows SET payload = 'x' WHERE seq = 1", [])
            .expect_err("update");
        assert!(update.to_string().contains("rows are append-only"));
        let delete = store
            .conn
            .execute("DELETE FROM rows WHERE seq = 1", [])
            .expect_err("delete");
        assert!(delete.to_string().contains("rows are append-only"));
        assert_eq!(intact(store.verify().expect("verify")).0, 1);
    }

    /// Deleting a middle row leaves the row after it chained to a row that is
    /// no longer there.
    #[test]
    fn a_deleted_middle_row_is_detected() {
        let store = three_rows_unguarded();
        store
            .conn
            .execute("DELETE FROM rows WHERE seq = 2", [])
            .expect("delete");
        assert_eq!(broken_at(store.verify().expect("verify")), 3);
    }

    /// Swapping two rows' `seq` puts a row after a predecessor it was never
    /// chained to.
    #[test]
    fn swapped_sequence_numbers_are_detected() {
        let store = three_rows_unguarded();
        store
            .conn
            .execute_batch(
                "UPDATE rows SET seq = 99 WHERE seq = 2; \
                 UPDATE rows SET seq = 2 WHERE seq = 3; \
                 UPDATE rows SET seq = 3 WHERE seq = 99;",
            )
            .expect("swap");
        assert_eq!(broken_at(store.verify().expect("verify")), 2);
    }

    /// A truncated tail is still a valid chain: `verify` cannot see it. Only
    /// a head recorded earlier shows it, which is why `Intact` carries one.
    #[test]
    fn a_truncated_tail_is_detected_only_against_a_recorded_head() {
        let store = three_rows_unguarded();
        let (rows, recorded_head) = intact(store.verify().expect("verify"));
        assert_eq!(rows, 3);
        store
            .conn
            .execute("DELETE FROM rows WHERE seq = 3", [])
            .expect("truncate");
        let (rows, head) = intact(store.verify().expect("verify"));
        assert_eq!(rows, 2, "the shorter chain still verifies");
        assert_ne!(
            head, recorded_head,
            "but its head is no longer the recorded one"
        );
    }

    /// A row with no player and a row whose player is the empty string are
    /// different rows: editing one into the other must be caught.
    #[test]
    fn a_null_player_key_turned_empty_is_detected() {
        let store = store();
        store
            .record_outcome("r1", "solana", "coin-a", Outcome::Rugged, "v1", None, 20)
            .expect("outcome (no player)");
        assert_eq!(intact(store.verify().expect("verify")).0, 1);
        drop_guards(&store);
        store
            .conn
            .execute("UPDATE rows SET player_key = '' WHERE seq = 1", [])
            .expect("tamper");
        assert_eq!(broken_at(store.verify().expect("verify")), 1);
    }

    /// An identity delete leaves the chain valid and unlinked: no row holds
    /// the X id or handle, before or after.
    #[test]
    fn an_identity_delete_leaves_the_chain_valid_and_unlinked() {
        let store = store();
        let p = key(&store);
        let identity = Identity {
            player_key: p.clone(),
            x_id: "999999".to_owned(),
            handle: "realtestuser".to_owned(),
            account_created_at: Some(1_600_000_000),
            session_hash: Some("deadbeef".to_owned()),
            signed_in_at: 5,
        };
        store.upsert_identity(&identity).expect("upsert");
        forecast_on(&store, &p, "coin-a", 10, 100).expect("forecast");

        store.delete_identity(&p).expect("delete");

        assert_eq!(store.identity(&p).expect("read"), None);
        assert_eq!(intact(store.verify().expect("verify")).0, 1);

        // No row anywhere in the chain ever held the X id or handle -- the
        // schema never gave them a column to be stored in, so this is the
        // structural guarantee, checked against the payload text directly.
        let payloads: Vec<String> = store
            .conn
            .prepare("SELECT payload FROM rows")
            .expect("prepare")
            .query_map([], |r| r.get(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows");
        for payload in &payloads {
            assert!(!payload.contains(&identity.x_id));
            assert!(!payload.contains(&identity.handle));
        }
    }

    /// Deleting an identity removes its bytes from the file, not only its
    /// row (design 0032 §4). The first read is the instrument check: the
    /// same scan must find the X id while it is stored, so a clean result
    /// after the delete means the bytes are gone and not that the scan is
    /// blind.
    #[test]
    fn a_deleted_identity_leaves_no_bytes_in_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.db");
        let x_id = "x-id-7f3a9c1e5b2d4086";
        let handle = "handle-5d2e8b90c41a";

        let store = Store::open(&path).expect("open");
        let p = key(&store);
        store
            .upsert_identity(&Identity {
                player_key: p.clone(),
                x_id: x_id.to_owned(),
                handle: handle.to_owned(),
                account_created_at: None,
                session_hash: None,
                signed_in_at: 5,
            })
            .expect("upsert");

        let scan = |needle: &str| -> bool {
            let mut found = false;
            for entry in std::fs::read_dir(dir.path()).expect("read_dir") {
                let bytes = std::fs::read(entry.expect("entry").path()).expect("read");
                found |= bytes
                    .windows(needle.len())
                    .any(|window| window == needle.as_bytes());
            }
            found
        };
        assert!(scan(x_id), "the scan must see the X id while it is stored");
        assert!(scan(handle), "the scan must see the handle while stored");

        store.delete_identity(&p).expect("delete");
        drop(store);

        // Every file beside the database (a `-journal` or `-wal`) is scanned.
        assert!(!scan(x_id), "the X id survived the delete in the file");
        assert!(!scan(handle), "the handle survived the delete in the file");
    }

    /// Two player keys differ, and a key is 32 lower-case hex characters
    /// (128 bits).
    #[test]
    fn player_keys_are_distinct_and_32_hex_characters() {
        let store = store();
        let a = key(&store);
        let b = key(&store);
        assert_ne!(a, b);
        for k in [&a, &b] {
            assert_eq!(k.as_str().len(), 32);
            assert!(
                k.as_str()
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                "not lower-case hex: {}",
                k.as_str()
            );
        }
    }

    fn identity_for(player_key: &PlayerKey, x_id: &str, handle: &str) -> Identity {
        Identity {
            player_key: player_key.clone(),
            x_id: x_id.to_owned(),
            handle: handle.to_owned(),
            account_created_at: None,
            session_hash: None,
            signed_in_at: 1,
        }
    }

    /// One X account has one player key: a second identity with the same X
    /// id is refused, and a returning user is found by their X id.
    #[test]
    fn a_second_identity_for_the_same_x_id_is_refused() {
        let store = store();
        let first = key(&store);
        let second = key(&store);
        store
            .upsert_identity(&identity_for(&first, "x-1", "alice"))
            .expect("first identity");
        let err = store
            .upsert_identity(&identity_for(&second, "x-1", "alice-again"))
            .expect_err("same X id, another key");
        assert!(matches!(err, StoreError::XIdTaken), "got {err:?}");

        assert_eq!(
            store.player_for_x_id("x-1").expect("lookup"),
            Some(first.clone())
        );
        assert_eq!(store.player_for_x_id("x-unknown").expect("lookup"), None);
        // The same key may update its own row.
        store
            .upsert_identity(&identity_for(&first, "x-1", "alice-renamed"))
            .expect("same key updates");
        assert_eq!(
            store.identity(&first).expect("read").expect("row").handle,
            "alice-renamed"
        );
    }

    /// A player key never moves to another X account, and a debug print of
    /// an identity carries no X id, handle or session hash.
    #[test]
    fn a_key_cannot_be_moved_to_another_x_account_and_debug_redacts() {
        let store = store();
        let first = key(&store);
        store
            .upsert_identity(&identity_for(&first, "x-1", "alice"))
            .expect("first identity");
        let err = store
            .upsert_identity(&identity_for(&first, "x-2", "mallory"))
            .expect_err("same key, another X id");
        assert!(matches!(err, StoreError::KeyTaken), "got {err:?}");
        let kept = store.identity(&first).expect("read").expect("row");
        assert_eq!((kept.x_id.as_str(), kept.handle.as_str()), ("x-1", "alice"));

        let mut shown = identity_for(&first, "x-secret-id", "secret-handle");
        shown.session_hash = Some("secret-hash".to_owned());
        let printed = format!("{shown:?}");
        for secret in ["x-secret-id", "secret-handle", "secret-hash"] {
            assert!(!printed.contains(secret), "{secret} leaked in {printed}");
        }
    }

    /// A constraint other than the `x_id` uniqueness is not `XIdTaken`: a
    /// caller that tells a user "that X account already has a key" when the
    /// write was refused for another reason sends them down the wrong path.
    #[test]
    fn an_identity_refused_for_another_reason_is_not_reported_as_x_id_taken() {
        let store = store();
        let p = key(&store);
        store
            .conn
            .execute_batch(
                "CREATE TRIGGER refuse_identity BEFORE INSERT ON identity \
                 BEGIN SELECT RAISE(ABORT, 'refused for another reason'); END;",
            )
            .expect("trigger");
        let err = store
            .upsert_identity(&identity_for(&p, "x-1", "alice"))
            .expect_err("refused");
        assert!(
            matches!(err, StoreError::Constraint(_)),
            "expected Constraint, got {err:?}"
        );
    }

    /// Only a constraint failure becomes `Constraint`; any other `SQLite`
    /// failure (a busy database, an I/O error) stays `Sqlite`, so a caller
    /// does not treat "try again" as "your data was refused".
    #[test]
    fn a_non_constraint_sqlite_failure_stays_sqlite() {
        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        );
        assert!(
            matches!(constraint_or_sqlite(busy), StoreError::Sqlite(_)),
            "a busy database is not a constraint"
        );
        let constraint = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
            None,
        );
        assert!(
            matches!(constraint_or_sqlite(constraint), StoreError::Constraint(_)),
            "a constraint failure is one"
        );
    }

    /// `Unresolved` adds no points and no n -- proved at
    /// `realorrug_contest::calls`, reused here rather than re-tested against
    /// a copy, per design 0032 §6 ("do not copy them").
    #[test]
    fn unresolved_adds_no_points_and_no_n() {
        use realorrug_contest::calls::{Outcome as CallOutcome, SettledCall, score_calls};

        let calls = vec![SettledCall {
            player: "player-1".to_owned(),
            coin_id: "coin-a".to_owned(),
            creator_id: "creator-1".to_owned(),
            side: Side::Rug,
            q: odds(6_000),
            outcome: CallOutcome::Unresolved,
            called_at: 1,
            account_age_days: Some(60),
        }];
        let ranking = score_calls(&calls, 30);
        assert!(
            ranking.above_line.is_empty() && ranking.within_luck.is_empty(),
            "an all-Unresolved player has n = 0 and never appears in a ranking"
        );
        assert_eq!(
            ranking.excluded,
            vec![(
                "player-1".to_owned(),
                realorrug_contest::calls::Excluded::TooFewCalls { settled: 0 }
            )],
            "the Unresolved call counted toward neither points nor n"
        );
    }

    /// A hidden forecast is not visible to another player before close, but
    /// is visible to its author.
    #[test]
    fn a_hidden_forecast_is_visible_only_to_its_author_before_close() {
        let store = store();
        let p1 = key(&store);
        let p2 = key(&store);
        forecast_on(&store, &p1, "coin-a", 10, 100).expect("forecast");

        assert!(
            store
                .forecast("r1", "solana", "coin-a", p1.as_str(), p2.as_str(), 50)
                .expect("read")
                .is_none(),
            "another player must not see it before close"
        );
        assert!(
            store
                .forecast("r1", "solana", "coin-a", p1.as_str(), p1.as_str(), 50)
                .expect("read")
                .is_some(),
            "the author sees their own call before close"
        );
        assert!(
            store
                .forecast("r1", "solana", "coin-a", p1.as_str(), p2.as_str(), 100)
                .expect("read")
                .is_some(),
            "anyone sees it once the window has closed"
        );
    }

    /// A stored row whose kind and payload disagree is `Corrupt`, not a
    /// panic.
    #[test]
    fn a_row_whose_kind_and_payload_disagree_is_corrupt_not_a_panic() {
        let store = store();
        for (i, kind) in ["forecast", "outcome"].into_iter().enumerate() {
            store
                .conn
                .execute(
                    "INSERT INTO rows (kind, round, chain, token, player_key, at, payload, \
                     previous_hash, hash) VALUES (?1, 'r1', 'solana', 'coin-a', 'p', 1, \
                     '{\"kind\":\"discussion\",\"comment\":\"x\"}', ?2, 'h')",
                    params![kind, format!("link-{i}")],
                )
                .expect("raw insert");
        }
        let forecast = store.forecast("r1", "solana", "coin-a", "p", "p", 1);
        assert!(
            matches!(forecast, Err(StoreError::Corrupt(_))),
            "{forecast:?}"
        );
        let outcome = store.outcome("r1", "solana", "coin-a");
        assert!(
            matches!(outcome, Err(StoreError::Corrupt(_))),
            "{outcome:?}"
        );
    }

    /// Two `Store`s on one file, appending alternately, keep one chain, and a
    /// second connection's append sees the first's new tail.
    #[test]
    fn two_connections_on_one_file_keep_one_chain() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("store.db");
        let a = Store::open(&path).expect("a");
        let b = Store::open(&path).expect("b");
        let pa = key(&a);
        let pb = key(&b);

        for i in 0..3 {
            a.submit_discussion("r1", "solana", "coin-a", &pa, "from a", i)
                .expect("a appends");
            assert_eq!(
                b.tail().expect("b tail"),
                a.tail().expect("a tail"),
                "b sees the row a just appended"
            );
            b.submit_discussion("r1", "solana", "coin-a", &pb, "from b", i)
                .expect("b appends");
            assert_eq!(
                a.tail().expect("a tail"),
                b.tail().expect("b tail"),
                "a sees the row b just appended"
            );
        }
        let (rows, head) = intact(a.verify().expect("verify"));
        assert_eq!(rows, 6);
        assert_eq!(head, b.tail().expect("tail"));
        assert_eq!(intact(b.verify().expect("verify")).0, 6);
    }

    /// The race itself, made deterministic: a second connection tries to
    /// write in the window between the first's tail read and its INSERT. With
    /// the write lock held from the start it is refused (`SQLITE_BUSY`), so
    /// only one row extends the tail; without it both would chain to the same
    /// tail and fork. Checked on every write path.
    #[test]
    fn a_second_connection_cannot_write_between_a_tail_read_and_the_insert() {
        type Write = fn(&Store, &PlayerKey) -> Result<(), StoreError>;
        let writes: [(&str, Write); 4] = [
            ("forecast", |s, p| forecast_on(s, p, "coin-a", 10, 100)),
            ("evidence", |s, p| {
                s.submit_evidence("r1", "solana", "coin-a", p, "https://x", "note", 10)
            }),
            ("discussion", |s, p| {
                s.submit_discussion("r1", "solana", "coin-a", p, "hi", 10)
            }),
            ("outcome", |s, _| {
                s.record_outcome("r1", "solana", "coin-a", Outcome::Stood, "v1", None, 10)
            }),
        ];
        for (name, write) in writes {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("store.db");
            let a = Store::open(&path).expect("a");
            let b = Store::open(&path).expect("b");
            b.conn
                .busy_timeout(std::time::Duration::ZERO)
                .expect("no waiting");
            let pa = key(&a);
            let pb = key(&b);

            let b_wrote = Rc::new(Cell::new(None));
            let seen = Rc::clone(&b_wrote);
            *a.between_tail_and_insert.borrow_mut() = Some(Box::new(move || {
                seen.set(Some(write(&b, &pb).is_ok()));
            }));

            write(&a, &pa).unwrap_or_else(|e| panic!("{name}: a's write failed: {e}"));

            assert_eq!(
                b_wrote.get(),
                Some(false),
                "{name}: b must be refused while a holds the write lock"
            );
            let (rows, _) = intact(a.verify().expect("verify"));
            assert_eq!(rows, 1, "{name}: only a's row is on the chain");
        }
    }

    /// The schema refuses a second row chained to the same predecessor, even
    /// from a writer that skips the lock.
    #[test]
    fn the_schema_refuses_two_rows_with_the_same_predecessor() {
        let store = store();
        let p = key(&store);
        store
            .submit_discussion("r1", "solana", "coin-a", &p, "first", 1)
            .expect("first row, chained to genesis");
        let err = store
            .conn
            .execute(
                "INSERT INTO rows (kind, round, chain, token, player_key, at, payload, \
                 previous_hash, hash) VALUES ('discussion', 'r1', 'solana', 'coin-a', 'p', 2, \
                 '{}', ?1, 'other')",
                params![chain::GENESIS],
            )
            .expect_err("a second row chained to genesis");
        assert!(
            err.to_string().contains("UNIQUE"),
            "expected a unique-constraint failure, got {err}"
        );
    }

    /// A session maps to its player through the identity table alone, a new
    /// sign-in replaces the old session, and deleting the identity ends it.
    #[test]
    fn a_session_hash_finds_its_player_and_dies_with_the_identity() {
        let store = store();
        let p = key(&store);
        let mut id = identity_for(&p, "x-1", "alice");
        id.session_hash = Some("hash-1".to_owned());
        id.signed_in_at = 500;
        store.upsert_identity(&id).expect("identity");
        assert_eq!(
            store.player_for_session_hash("hash-1").expect("lookup"),
            Some((p.clone(), 500))
        );
        assert_eq!(
            store.player_for_session_hash("other").expect("lookup"),
            None
        );

        id.session_hash = Some("hash-2".to_owned());
        store.upsert_identity(&id).expect("a second sign-in");
        assert_eq!(
            store.player_for_session_hash("hash-1").expect("lookup"),
            None,
            "one identity holds one session; the new sign-in replaced the old"
        );

        store.delete_identity(&p).expect("delete");
        assert_eq!(
            store.player_for_session_hash("hash-2").expect("lookup"),
            None
        );
    }

    #[test]
    fn a_public_list_holds_only_forecasts_whose_close_has_passed_and_names_no_player() {
        let store = store();
        let ann = key(&store);
        forecast_on(&store, &ann, "coinA", 100, 1_000).expect("submitted");
        assert!(
            store.closed_forecasts("r1", 999).expect("read").is_empty(),
            "one second before close a hidden call is in the public list"
        );
        let shown = store.closed_forecasts("r1", 1_000).expect("read");
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].token, "coinA");
        assert!(
            !format!("{shown:?}").contains(ann.as_str()),
            "the public list names its player"
        );
        assert!(
            store
                .closed_forecasts("r2", 5_000)
                .expect("read")
                .is_empty()
        );
    }

    #[test]
    fn settled_forecasts_join_the_latest_outcome_and_skip_open_and_unsettled_calls() {
        let store = store();
        let ann = key(&store);
        forecast_on(&store, &ann, "coinA", 100, 1_000).expect("a");
        forecast_on(&store, &ann, "coinB", 100, 1_000).expect("b");
        store
            .record_outcome("r1", "solana", "coinA", Outcome::Stood, "v1", None, 1_500)
            .expect("outcome");
        store
            .record_outcome("r1", "solana", "coinA", Outcome::Rugged, "v1", None, 1_600)
            .expect("correction");
        assert!(store.settled_forecasts(999).expect("read").is_empty());
        let settled = store.settled_forecasts(2_000).expect("read");
        assert_eq!(settled.len(), 1, "coinB has no outcome and must not appear");
        assert_eq!(settled[0].token, "coinA");
        assert_eq!(settled[0].outcome, Outcome::Rugged);
        assert_eq!(settled[0].settled_at, 1_600);
    }

    /// The settled read trusts a row's payload only under its own kind: a
    /// forecast payload filed as an outcome, or the reverse, is `Corrupt`
    /// rather than read as the other thing.
    #[test]
    fn settled_forecasts_refuse_a_payload_filed_under_the_wrong_kind() {
        let forecast_payload = "{\"kind\":\"forecast\",\"side\":\"Rug\",                                \"q_basis_points\":5000,\"window_close\":1}";
        let outcome_payload = "{\"kind\":\"outcome\",\"outcome\":\"Rugged\",                               \"rule_version\":\"v1\",\"evidence_reference\":null}";
        for (kind, payload) in [("outcome", forecast_payload), ("forecast", outcome_payload)] {
            let store = store();
            store
                .conn
                .execute(
                    "INSERT INTO rows (kind, round, chain, token, player_key, at, payload,                      previous_hash, hash) VALUES (?1, 'r1', 'solana', 'coin-a', 'p', 1, ?2,                      'link-0', 'h')",
                    params![kind, payload],
                )
                .expect("raw insert");
            let read = store.settled_forecasts(1_000);
            assert!(
                matches!(read, Err(StoreError::Corrupt(_))),
                "a '{kind}' row with the other payload was read as fine: {read:?}"
            );
        }
    }
}
