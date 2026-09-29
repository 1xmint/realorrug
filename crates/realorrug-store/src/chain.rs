// SPDX-License-Identifier: Apache-2.0
//! The hash chain every row rides on (design 0032 §3).
//!
//! One digest function, called by every write path in [`crate::Store`], so
//! the chain's shape cannot drift between the forecast writer and the
//! outcome writer. The same primitive `realorrug-journal` chains its own
//! events with (`crates/realorrug-journal/src/event.rs`, `Event::digest`):
//! blake3 over the row's own fields plus the previous row's digest, so an
//! alteration anywhere in the middle is visible from every row after it,
//! relative to a trusted checkpoint -- and, per that crate's own doc, this
//! "does not resist a host attacker who rewrites the whole chain."

/// The digest of the first row a chain has ever had nothing before it.
///
/// Named rather than an empty string inline, so a reader of a `verify`
/// failure sees a word, not a blank.
pub const GENESIS: &str = "genesis";

/// Computes the digest of one row.
///
/// `fields` is every column that makes the row what it is, in a fixed order
/// the caller controls -- this function does not know or care what a
/// forecast or an outcome looks like, only that the same fields in the same
/// order must always produce the same digest (design 0032 §5: a replay must
/// settle the same way every time).
#[must_use]
pub fn digest(previous: &str, fields: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(previous.as_bytes());
    for field in fields {
        // A length-prefixed separator, not a plain byte like `|`: a field
        // that happens to contain the separator must not be able to shift
        // bytes from one field into its neighbour and still hash the same.
        hasher.update(&(field.len() as u64).to_le_bytes());
        hasher.update(field.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one property `verify` in `lib.rs` leans on: change one field, get
    /// a different digest. Spelled out here, at the unit that computes it,
    /// so a change to the field-separation scheme is caught at its source.
    #[test]
    fn a_changed_field_changes_the_digest() {
        let a = digest(GENESIS, &["round-1", "coin-a", "player-1"]);
        let b = digest(GENESIS, &["round-1", "coin-a", "player-2"]);
        assert_ne!(a, b);
    }

    /// Splitting bytes across a field boundary must not collide with moving
    /// them: the length prefix is what stops `["ab", "c"]` from hashing the
    /// same as `["a", "bc"]`.
    #[test]
    fn field_boundaries_are_not_shiftable() {
        let a = digest(GENESIS, &["ab", "c"]);
        let b = digest(GENESIS, &["a", "bc"]);
        assert_ne!(a, b);
    }
}
