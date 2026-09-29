// SPDX-License-Identifier: Apache-2.0
//! The row shapes (design 0032 §2) and the verify routine (§3) that walks
//! them.

use realorrug_contest::calls::{Outcome, Side};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::StoreError;
use crate::chain;

/// The kind-specific fields of one chain row, serialised into the `payload`
/// column. `#[serde(tag = "kind")]` so a stored row names its own kind in
/// its JSON, which is what lets `verify` read old rows back without knowing
/// which kind-specific struct grew a field after they were written.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RowPayload {
    /// A player's real-or-rug call (design 0032 §2, "Forecast").
    #[serde(rename = "forecast")]
    Forecast {
        /// Which way the player called it.
        side: Side,
        /// The odds at the moment of listing, fixed then (design 0028 §3).
        q_basis_points: u16,
        /// When this round's entry window closes.
        window_close: i64,
    },
    /// What the chain settled the coin to (design 0032 §2, "Outcome").
    #[serde(rename = "outcome")]
    Outcome {
        /// `Rugged`, `Stood` or `Unresolved` -- see [`Outcome`].
        outcome: Outcome,
        /// The rule version that scored or will score it (§2, §5).
        rule_version: String,
        /// What the outcome rests on, not a copy of it (§2).
        evidence_reference: Option<String>,
    },
    /// A link or a sheet reference plus the submitter's note (§2, "Evidence
    /// submission").
    #[serde(rename = "evidence")]
    Evidence {
        /// The link or sheet reference.
        reference: String,
        /// The submitter's own note.
        note: String,
    },
    /// A comment on a token's page (§2, "Discussion / vote").
    #[serde(rename = "discussion")]
    Discussion {
        /// The comment text.
        comment: String,
    },
}

/// A forecast, as read back for someone allowed to see it.
#[derive(Clone, Debug, PartialEq)]
pub struct ForecastView {
    /// The player's key (never their X id or handle -- §4).
    pub player_key: String,
    /// Which way they called it.
    pub side: Side,
    /// The odds at the moment of listing.
    pub q_basis_points: u16,
    /// When it was submitted.
    pub submitted_at: i64,
    /// When this round's window closes.
    pub window_close: i64,
}

/// A forecast as the public may read it: after its close, with no player in it.
///
/// There is deliberately no player field. A per-coin list that named its
/// callers would be a second board beside the one that counts, and the raw key
/// is a stable pseudonym that must never sit next to a handle (design 0032
/// §11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedForecast {
    /// The chain the coin is on.
    pub chain: String,
    /// The coin.
    pub token: String,
    /// Which way it was called.
    pub side: Side,
    /// The odds at the moment of listing.
    pub q_basis_points: u16,
    /// When it was submitted.
    pub submitted_at: i64,
}

/// A closed forecast joined to the outcome recorded for its coin, for the
/// board's hit/miss count. Only forecasts whose own close has passed, and only
/// those with an outcome row, appear.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettledForecast {
    /// The player's key. The store's caller must not print this beside a handle.
    pub player_key: String,
    /// The round.
    pub round: String,
    /// The chain the coin is on.
    pub chain: String,
    /// The coin.
    pub token: String,
    /// Which way it was called.
    pub side: Side,
    /// The odds at the moment of listing.
    pub q_basis_points: u16,
    /// The latest outcome recorded for the coin.
    pub outcome: Outcome,
    /// When the forecast was submitted.
    pub submitted_at: i64,
    /// When the outcome was recorded.
    pub settled_at: i64,
}

/// An outcome, as read back.
#[derive(Clone, Debug, PartialEq)]
pub struct OutcomeView {
    /// `Rugged`, `Stood` or `Unresolved`.
    pub outcome: Outcome,
    /// The rule version that scored it.
    pub rule_version: String,
    /// What it rests on, if anything was cited.
    pub evidence_reference: Option<String>,
    /// When it was settled.
    pub settled_at: i64,
}

/// What walking the chain established.
///
/// Two states, not `realorrug_journal`'s three: SQLite's own durability
/// (`Store::append`'s transaction) does not leave the "complete but no
/// terminator" shape a hand-rolled append-only file can, so there is no
/// `Torn` case here to distinguish from `Broken`.
///
/// `Intact` proves the rows that are there chain from genesis. It cannot
/// prove rows were not cut off the end: deleting the last rows leaves a
/// shorter chain that is still valid. A truncated tail is detected only by
/// comparing `head` against a head recorded earlier, somewhere the writer of
/// this file cannot reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verified {
    /// Every row's digest matched, chained from [`chain::GENESIS`].
    Intact {
        /// How many rows.
        rows: usize,
        /// The last row's hash ([`chain::GENESIS`] for an empty store): the
        /// value to record and later compare, since it is the only thing that
        /// shows a truncated tail.
        head: String,
    },
    /// A row's digest did not match what its own fields and the row before
    /// it should have produced.
    Broken {
        /// The `seq` the fault was found at.
        at: i64,
        /// What was wrong, in words.
        why: String,
    },
}

/// Walks every row in `seq` order and recomputes each digest from its own
/// fields and the row before it, the same shape `realorrug_journal::verify`
/// uses for its file.
pub(crate) fn verify(conn: &Connection) -> Result<Verified, StoreError> {
    let mut statement = conn.prepare(
        "SELECT seq, kind, round, chain, token, player_key, at, payload, previous_hash, hash \
         FROM rows ORDER BY seq ASC",
    )?;
    let mut rows = statement.query([])?;

    let mut previous = chain::GENESIS.to_owned();
    let mut count = 0usize;
    while let Some(row) = rows.next()? {
        let seq: i64 = row.get(0)?;
        let kind: String = row.get(1)?;
        let round: String = row.get(2)?;
        let chain_name: String = row.get(3)?;
        let token: String = row.get(4)?;
        let player_key: Option<String> = row.get(5)?;
        let at: i64 = row.get(6)?;
        let payload: String = row.get(7)?;
        let previous_hash: String = row.get(8)?;
        let hash: String = row.get(9)?;

        if previous_hash != previous {
            return Ok(Verified::Broken {
                at: seq,
                why: "this row's previous_hash does not match the row before it".to_owned(),
            });
        }
        let expected = chain::digest(
            &previous,
            &[
                kind.as_str(),
                round.as_str(),
                chain_name.as_str(),
                token.as_str(),
                &chain::player_field(player_key.as_deref()),
                &at.to_string(),
                payload.as_str(),
            ],
        );
        if expected != hash {
            return Ok(Verified::Broken {
                at: seq,
                why: "this row's own hash does not match its stored fields".to_owned(),
            });
        }
        previous = hash;
        count += 1;
    }
    Ok(Verified::Intact {
        rows: count,
        head: previous,
    })
}
