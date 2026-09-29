// SPDX-License-Identifier: Apache-2.0
//! The identity table (design 0032 §4): separate, mutable, outside the
//! append-only chain. This is the only place an X id or handle is ever
//! stored; the chain rows (`row.rs`) never carry them.

use rusqlite::{Connection, OptionalExtension, params};

use crate::StoreError;

/// A player's sign-in identity.
///
/// Never referenced from a chain row: a forecast, an evidence submission or
/// a discussion comment names its player only by [`Identity::player_key`],
/// so deleting this row (`Store::delete_identity`) leaves the chain rows
/// standing with nothing left to link them to a person (§4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// The random key chain rows name this player by.
    pub player_key: String,
    /// The X user id.
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

pub(crate) fn upsert(conn: &Connection, identity: &Identity) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO identity (player_key, x_id, handle, account_created_at, session_hash, \
         signed_in_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(player_key) DO UPDATE SET \
         x_id = excluded.x_id, handle = excluded.handle, \
         account_created_at = excluded.account_created_at, \
         session_hash = excluded.session_hash, signed_in_at = excluded.signed_in_at",
        params![
            identity.player_key,
            identity.x_id,
            identity.handle,
            identity.account_created_at,
            identity.session_hash,
            identity.signed_in_at,
        ],
    )?;
    Ok(())
}

pub(crate) fn delete(conn: &Connection, player_key: &str) -> Result<(), StoreError> {
    conn.execute("DELETE FROM identity WHERE player_key = ?1", params![player_key])?;
    Ok(())
}

pub(crate) fn read(conn: &Connection, player_key: &str) -> Result<Option<Identity>, StoreError> {
    conn.query_row(
        "SELECT player_key, x_id, handle, account_created_at, session_hash, signed_in_at \
         FROM identity WHERE player_key = ?1",
        params![player_key],
        |r| {
            Ok(Identity {
                player_key: r.get(0)?,
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
