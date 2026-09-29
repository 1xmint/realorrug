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
//! Every row carries the previous row's digest ([`chain::digest`]), the same
//! primitive `realorrug-journal` chains its own events with. A row is never
//! updated or deleted; a correction is a new row. [`Store::verify`] walks the
//! chain and reports the first break.
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

pub use identity::Identity;
pub use row::{ForecastView, OutcomeView, RowPayload, Verified};

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
}

/// The research store: one SQLite [`Connection`] behind `&self`, because
/// SQLite serialises writes on one file regardless
/// (`crates/realorrug-onchain/src/memory.rs`'s `Memory`, same doc comment,
/// same reason).
pub struct Store {
    conn: Connection,
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
             CREATE TABLE IF NOT EXISTS identity (
                player_key         TEXT    PRIMARY KEY,
                x_id                TEXT    NOT NULL,
                handle              TEXT    NOT NULL,
                account_created_at  INTEGER,
                session_hash        TEXT,
                signed_in_at        INTEGER NOT NULL
             );",
        )?;
        Ok(Self { conn })
    }

    /// The digest of the last row written, or [`chain::GENESIS`] if the
    /// store is empty. Read fresh on every write rather than cached on
    /// `self`: design 0032 §3 names `realorrug-serve` as the store's only
    /// writer, but nothing in this crate needs to assume that to stay
    /// correct -- a second process appending between two calls is still
    /// chained correctly because this always reads the tail it is about to
    /// extend.
    fn tail(&self) -> Result<String, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT hash FROM rows ORDER BY seq DESC LIMIT 1", [], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or_else(|| chain::GENESIS.to_owned()))
    }

    /// Appends one row to the chain. Every public write method funnels
    /// through here, so the chain's shape cannot drift between them.
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
        player_key: Option<&str>,
        at: i64,
        payload: &RowPayload,
    ) -> Result<(), StoreError> {
        let previous = self.tail()?;
        let payload_json = serde_json::to_string(payload)?;
        let digest = chain::digest(
            &previous,
            &[
                kind,
                round,
                chain_name,
                token,
                player_key.unwrap_or(""),
                &at.to_string(),
                &payload_json,
            ],
        );
        self.conn.execute(
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
        )?;
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
        player_key: &str,
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
        match self.append(
            "forecast",
            round,
            chain_name,
            token,
            Some(player_key),
            now,
            &payload,
        ) {
            Err(StoreError::Sqlite(rusqlite::Error::SqliteFailure(e, _)))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(StoreError::Duplicate)
            }
            other => other,
        }
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
            unreachable!("a row stored under kind 'forecast' always deserialises to Forecast")
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
            unreachable!("a row stored under kind 'outcome' always deserialises to Outcome")
        };
        Ok(Some(OutcomeView {
            outcome,
            rule_version,
            evidence_reference,
            settled_at: at,
        }))
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
        player_key: &str,
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
        player_key: &str,
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
    pub fn delete_identity(&self, player_key: &str) -> Result<(), StoreError> {
        identity::delete(&self.conn, player_key)
    }

    /// Reads a player's identity, if one is on file.
    ///
    /// # Errors
    ///
    /// [`StoreError::Sqlite`] on a read failure.
    pub fn identity(&self, player_key: &str) -> Result<Option<Identity>, StoreError> {
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

    fn odds(bp: u16) -> Odds {
        Odds::new(bp).expect("valid odds")
    }

    fn store() -> Store {
        Store::open_in_memory().expect("open")
    }

    /// A forecast at the close is refused, because `forecast` reveals a call
    /// at `now >= window_close`: at exactly the close, entry shuts and the
    /// reveal begins (design 0028 §2.4; design 0032 §10).
    #[test]
    fn a_forecast_at_exactly_the_close_is_refused() {
        let store = store();
        let err = store
            .submit_forecast(
                "r1",
                "solana",
                "coin-a",
                "player-1",
                Side::Rug,
                odds(6_000),
                100,
                100,
            )
            .expect_err("a forecast at the close");
        assert!(matches!(
            err,
            StoreError::WindowClosed {
                window_close: 100,
                now: 100
            }
        ));
        // One tick before the close is still open.
        store
            .submit_forecast(
                "r1",
                "solana",
                "coin-a",
                "player-1",
                Side::Rug,
                odds(6_000),
                99,
                100,
            )
            .expect("a forecast one tick before the close");
    }

    /// After the close a forecast is refused (design 0032 §9).
    #[test]
    fn a_late_forecast_is_refused() {
        let store = store();
        let err = store
            .submit_forecast(
                "r1",
                "solana",
                "coin-a",
                "player-1",
                Side::Rug,
                odds(6_000),
                101,
                100,
            )
            .expect_err("late forecast");
        assert!(matches!(
            err,
            StoreError::WindowClosed {
                window_close: 100,
                now: 101
            }
        ));
        assert!(
            store
                .forecast("r1", "solana", "coin-a", "player-1", "player-1", 101)
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
        store
            .submit_forecast(
                "r1",
                "solana",
                "coin-a",
                "player-1",
                Side::Rug,
                odds(6_000),
                10,
                100,
            )
            .expect("first forecast");
        let err = store
            .submit_forecast(
                "r1",
                "solana",
                "coin-a",
                "player-1",
                Side::Real,
                odds(4_000),
                20,
                100,
            )
            .expect_err("duplicate");
        assert!(matches!(err, StoreError::Duplicate));

        let view = store
            .forecast("r1", "solana", "coin-a", "player-1", "player-1", 100)
            .expect("read")
            .expect("a forecast exists");
        assert_eq!(
            view.side,
            Side::Rug,
            "the first call stands, not the second"
        );
        assert_eq!(view.submitted_at, 10);
    }

    /// A tampered middle row makes `verify` name that row -- re-applying the
    /// bug (skipping the tamper) must make this test fail, which is why the
    /// assertion checks the *specific* sequence the tamper landed on rather
    /// than only "is broken."
    #[test]
    fn a_tampered_middle_row_makes_verify_name_that_row() {
        let store = store();
        store
            .submit_forecast(
                "r1",
                "solana",
                "coin-a",
                "player-1",
                Side::Rug,
                odds(6_000),
                10,
                100,
            )
            .expect("row 1");
        store
            .submit_forecast(
                "r1",
                "solana",
                "coin-b",
                "player-1",
                Side::Real,
                odds(5_000),
                11,
                100,
            )
            .expect("row 2");
        store
            .submit_forecast(
                "r1",
                "solana",
                "coin-c",
                "player-1",
                Side::Rug,
                odds(3_000),
                12,
                100,
            )
            .expect("row 3");

        assert_eq!(
            store.verify().expect("verify"),
            Verified::Intact { rows: 3 }
        );

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

        match store.verify().expect("verify") {
            Verified::Broken { at, .. } => assert_eq!(at, 2, "the tamper landed on row 2"),
            intact @ Verified::Intact { .. } => panic!("expected Broken, got {intact:?}"),
        }
    }

    /// An identity delete leaves the chain valid and unlinked: no row holds
    /// the X id or handle, before or after.
    #[test]
    fn an_identity_delete_leaves_the_chain_valid_and_unlinked() {
        let store = store();
        let identity = Identity {
            player_key: "player-1".to_owned(),
            x_id: "999999".to_owned(),
            handle: "realtestuser".to_owned(),
            account_created_at: Some(1_600_000_000),
            session_hash: Some("deadbeef".to_owned()),
            signed_in_at: 5,
        };
        store.upsert_identity(&identity).expect("upsert");
        store
            .submit_forecast(
                "r1",
                "solana",
                "coin-a",
                "player-1",
                Side::Rug,
                odds(6_000),
                10,
                100,
            )
            .expect("forecast");

        store.delete_identity("player-1").expect("delete");

        assert_eq!(store.identity("player-1").expect("read"), None);
        assert_eq!(
            store.verify().expect("verify"),
            Verified::Intact { rows: 1 }
        );

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
        store
            .upsert_identity(&Identity {
                player_key: "player-1".to_owned(),
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

        store.delete_identity("player-1").expect("delete");
        drop(store);

        // Every file beside the database (a `-journal` or `-wal`) is scanned.
        assert!(!scan(x_id), "the X id survived the delete in the file");
        assert!(!scan(handle), "the handle survived the delete in the file");
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
        store
            .submit_forecast(
                "r1",
                "solana",
                "coin-a",
                "player-1",
                Side::Rug,
                odds(6_000),
                10,
                100,
            )
            .expect("forecast");

        assert!(
            store
                .forecast("r1", "solana", "coin-a", "player-1", "player-2", 50)
                .expect("read")
                .is_none(),
            "another player must not see it before close"
        );
        assert!(
            store
                .forecast("r1", "solana", "coin-a", "player-1", "player-1", 50)
                .expect("read")
                .is_some(),
            "the author sees their own call before close"
        );
        assert!(
            store
                .forecast("r1", "solana", "coin-a", "player-1", "player-2", 100)
                .expect("read")
                .is_some(),
            "anyone sees it once the window has closed"
        );
    }
}
