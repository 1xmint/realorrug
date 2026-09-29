// SPDX-License-Identifier: Apache-2.0
//! The identity table (design 0032 §4): separate, mutable, outside the
//! append-only chain. This is the only place an X id or handle is ever
//! stored; the chain rows (`row.rs`) never carry them.

use rusqlite::{Connection, OptionalExtension, params};

use crate::StoreError;

/// The random key chain rows name a player by (design 0032 §4), never an X id.
///
/// Only the store builds one: [`Store::new_player_key`](crate::Store::new_player_key)
/// (128 bits from SQLite's `randomblob`), or reading one back from an identity
/// row. The field is private and there is no `From<String>`, so a write method
/// that takes `&PlayerKey` cannot be handed an X id, a handle or a
/// guessable counter by mistake.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlayerKey(String);

impl PlayerKey {
    /// Wraps a key the store itself produced or read from its own table.
    pub(crate) const fn from_stored(key: String) -> Self {
        Self(key)
    }

    /// The key as text, for a read that takes one.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A player's sign-in identity.
///
/// Never referenced from a chain row: a forecast, an evidence submission or
/// a discussion comment names its player only by [`Identity::player_key`],
/// so deleting this row (`Store::delete_identity`) leaves the chain rows
/// standing with nothing left to link them to a person (§4).
#[derive(Clone, PartialEq, Eq)]
pub struct Identity {
    /// The random key chain rows name this player by.
    pub player_key: PlayerKey,
    /// The X user id. Unique: one X account has one player key, so a
    /// returning user keeps their record ([`Store::player_for_x_id`](crate::Store::player_for_x_id)).
    pub x_id: String,
    /// The X handle.
    pub handle: String,
    /// `created_at` from `/2/users/me`, if it was read (design 0032 §9,
    /// "Account age").
    pub account_created_at: Option<i64>,
    /// A SHA-256 hash of the session token -- never the raw token (§9,
    /// "Sessions").
    pub session_hash: Option<String>,
    /// When this identity row was written (sign-in time, the caller's
    /// clock).
    pub signed_in_at: i64,
}

/// Written by hand so a `{:?}` in a log or an error never carries the X id,
/// handle or session hash: a log line is out of `delete_identity`'s reach.
impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("player_key", &self.player_key)
            .field("x_id", &"<redacted>")
            .field("handle", &"<redacted>")
            .field("account_created_at", &self.account_created_at)
            .field("session_hash", &"<redacted>")
            .field("signed_in_at", &self.signed_in_at)
            .finish()
    }
}

pub(crate) fn upsert(conn: &Connection, identity: &Identity) -> Result<(), StoreError> {
    // A key never moves to another X account: the update is conditional on the
    // stored `x_id`, and a refused update changes no row. Updating `x_id` too
    // would hand one person's whole chain history to someone else's account.
    let changed = conn.execute(
        "INSERT INTO identity (player_key, x_id, handle, account_created_at, session_hash,          signed_in_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)          ON CONFLICT(player_key) DO UPDATE SET          handle = excluded.handle,          account_created_at = excluded.account_created_at,          session_hash = excluded.session_hash, signed_in_at = excluded.signed_in_at          WHERE identity.x_id = excluded.x_id",
        params![
            identity.player_key.as_str(),
            identity.x_id,
            identity.handle,
            identity.account_created_at,
            identity.session_hash,
            identity.signed_in_at,
        ],
    )
    .map_err(|e| match &e {
        // `x_id` is UNIQUE, and `ON CONFLICT(player_key)` does not cover it:
        // a second key for an X account that already has one is refused, so
        // one person cannot split their record across two keys.
        rusqlite::Error::SqliteFailure(f, _)
            if f.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            StoreError::XIdTaken
        }
        _ => crate::constraint_or_sqlite(e),
    })?;
    if changed == 0 {
        return Err(StoreError::KeyTaken);
    }
    Ok(())
}

pub(crate) fn delete(conn: &Connection, player_key: &PlayerKey) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM identity WHERE player_key = ?1",
        params![player_key.as_str()],
    )?;
    Ok(())
}

pub(crate) fn read(
    conn: &Connection,
    player_key: &PlayerKey,
) -> Result<Option<Identity>, StoreError> {
    conn.query_row(
        "SELECT player_key, x_id, handle, account_created_at, session_hash, signed_in_at          FROM identity WHERE player_key = ?1",
        params![player_key.as_str()],
        |r| {
            Ok(Identity {
                player_key: PlayerKey::from_stored(r.get(0)?),
                x_id: r.get(1)?,
                handle: r.get(2)?,
                account_created_at: r.get(3)?,
                session_hash: r.get(4)?,
                signed_in_at: r.get(5)?,
            })
        },
    )
    .optional()
    .map_err(StoreError::from)
}

pub(crate) fn player_for_x_id(
    conn: &Connection,
    x_id: &str,
) -> Result<Option<PlayerKey>, StoreError> {
    conn.query_row(
        "SELECT player_key FROM identity WHERE x_id = ?1",
        params![x_id],
        |r| Ok(PlayerKey::from_stored(r.get(0)?)),
    )
    .optional()
    .map_err(StoreError::from)
}
