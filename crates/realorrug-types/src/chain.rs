// SPDX-License-Identifier: Apache-2.0
//! The two things that differ between chains: an address, and a read point.
//!
//! ADR 0028 point 2 names the seam a third chain has to fill: "an address
//! parser, one `ChainReader` impl, one `Venue` arm and one line in
//! `mention.rs`." [`ChainAddress`] is that address parser's return type, and
//! [`ReadAt`] is the read-point type every [`ChainReader`](crate) (well, the
//! trait actually lives in `realorrug-onchain`, beside `Dossier` — this
//! module supplies the two chain-neutral leaves it is built from) fills in.
//!
//! Neither type replaces a chain's own address or clock. `realorrug_types::Address`
//! stays a 32-byte Solana key and `realorrug_robinhood::Address` stays a
//! 20-byte Robinhood Chain key; this module only wraps them so one type can
//! name "an address, on whichever chain it turned out to be."

use core::fmt;
use core::str::FromStr;

use crate::{Address, AddressParseError, Slot};

/// An address on any chain Radar reads, tagged by which one.
///
/// Parsed **by shape**, not by an explicit tag, which is what makes this the
/// "address parser" ADR 0028 point 2 says a third chain costs: a `0x` prefix
/// is Robinhood Chain's fixed-width hex address (`realorrug_robinhood::Address`,
/// 20 bytes); anything else is tried as Solana's base58
/// (`realorrug_types::Address`, 32 bytes). A string that is neither is
/// refused, never guessed at -- there is no third arm that falls back to
/// "assume Solana."
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ChainAddress {
    /// A 32-byte Solana account address.
    Solana(Address),
    /// A 20-byte Robinhood Chain account address.
    Robinhood(realorrug_robinhood::Address),
}

/// Why a string could not be read as a [`ChainAddress`].
///
/// Carries which shape was attempted, and why that attempt failed, rather
/// than a single flattened message: a `0x`-prefixed string that is the wrong
/// length is a different mistake from a base58 string with a bad character,
/// and an operator debugging a refused mention benefits from knowing which
/// parser actually ran.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChainAddressParseError {
    /// Looked Robinhood-shaped (`0x`-prefixed) but did not parse as one.
    #[error("not a Robinhood address: {0}")]
    Robinhood(#[source] realorrug_robinhood::ReadError),
    /// Did not look Robinhood-shaped, and did not parse as Solana either.
    #[error("not a Solana address: {0}")]
    Solana(#[source] AddressParseError),
}

impl FromStr for ChainAddress {
    type Err = ChainAddressParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        // The shape check ADR 0028 point 2 names: `0x` is committed to
        // Robinhood and never falls through to a Solana attempt, so a
        // truncated or malformed Robinhood address is reported as exactly
        // that instead of being retried as base58 (which would almost always
        // also fail, on a much less useful error).
        if text.starts_with("0x") || text.starts_with("0X") {
            return realorrug_robinhood::Address::from_str(text)
                .map(ChainAddress::Robinhood)
                .map_err(ChainAddressParseError::Robinhood);
        }
        Address::from_str(text)
            .map(ChainAddress::Solana)
            .map_err(ChainAddressParseError::Solana)
    }
}

impl fmt::Display for ChainAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Solana(address) => fmt::Display::fmt(address, f),
            Self::Robinhood(address) => fmt::Display::fmt(address, f),
        }
    }
}

/// When a dossier was read, in the reading chain's own unit.
///
/// A Solana slot and a Robinhood block number are both, underneath,
/// `u64`s that count up roughly once per chain tick -- and exactly that
/// resemblance is what design 0021 §1 warns against flattening into one
/// bare `u64`: "how old" is counted in the chain's own unit first, and a
/// slot compared to a block number as if they were the same clock would be
/// silently wrong. Keeping them as two enum arms means there is no shared
/// arithmetic or comparison operator for a caller to reach for by mistake --
/// a `ReadAt::Solana` and a `ReadAt::Robinhood` carrying the same number are
/// unequal, because they are different chains' clocks that happen to agree
/// on a digit.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum ReadAt {
    /// A Solana slot.
    Solana(Slot),
    /// A Robinhood Chain block number.
    Robinhood(u64),
}

impl ReadAt {
    /// The Solana slot, if this read point is Solana's.
    ///
    /// `None` for a Robinhood read, deliberately -- there is no lossy
    /// conversion from a block number into a slot, so a caller that only
    /// knows how to display a slot gets nothing to misinterpret rather than
    /// a block number wearing a slot's label.
    #[must_use]
    pub const fn as_slot(self) -> Option<Slot> {
        match self {
            Self::Solana(slot) => Some(slot),
            Self::Robinhood(_) => None,
        }
    }
}

impl fmt::Display for ReadAt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Solana(slot) => write!(f, "slot {}", slot.get()),
            Self::Robinhood(block) => write!(f, "block {block}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_0x_string_parses_robinhood_and_a_base58_string_parses_solana() {
        // Names the wrong implementation: a parser that tried both shapes and
        // took whichever succeeded first, rather than dispatching on the
        // prefix.
        let robinhood = "0x1111111111111111111111111111111111111111";
        match ChainAddress::from_str(robinhood).expect("a robinhood address") {
            ChainAddress::Robinhood(_) => {}
            ChainAddress::Solana(_) => panic!("a 0x address must not parse as Solana"),
        }

        // Wrapped SOL's mint, a real 32-byte Solana address.
        let solana = "So11111111111111111111111111111111111111112";
        match ChainAddress::from_str(solana).expect("a solana address") {
            ChainAddress::Solana(_) => {}
            ChainAddress::Robinhood(_) => panic!("a base58 address must not parse as Robinhood"),
        }
    }

    #[test]
    fn a_string_that_is_neither_shape_is_refused_not_guessed_at() {
        // Names the wrong implementation: a parser that fell back to
        // whichever chain's parser happened not to error, instead of
        // surfacing the failure of the shape it actually matched.
        assert!(ChainAddress::from_str("not an address").is_err());
        // Empty. Neither `0x`-prefixed nor valid base58.
        assert!(ChainAddress::from_str("").is_err());
        // `0x`-shaped but the wrong length: must fail as a Robinhood parse,
        // never silently retried as Solana.
        match ChainAddress::from_str("0x1234") {
            Err(ChainAddressParseError::Robinhood(_)) => {}
            other => panic!("expected a Robinhood parse error, got {other:?}"),
        }
    }

    #[test]
    fn a_read_at_cannot_silently_compare_a_slot_to_a_block_number() {
        // Names the wrong implementation: `ReadAt` collapsed to a bare `u64`,
        // under which a Solana slot and a Robinhood block carrying the same
        // number would compare equal. Two different chains' clocks that agree
        // on a digit are not the same read point.
        let slot = ReadAt::Solana(Slot(444_007_820));
        let block = ReadAt::Robinhood(444_007_820);
        assert_ne!(slot, block);
        assert_eq!(slot.as_slot(), Some(Slot(444_007_820)));
        assert_eq!(block.as_slot(), None);
    }

    #[test]
    fn read_at_serialises_externally_tagged_by_chain() {
        // The tag is the point (§1 of packet 0036): a bare number in a log is
        // meaningless without the clock it was read against, so the chain
        // name has to travel with it. Pins the default serde representation
        // rather than a custom one, because a third chain should cost an
        // enum arm and nothing else.
        let slot = ReadAt::Solana(Slot(444_007_820));
        assert_eq!(
            serde_json::to_string(&slot).expect("json"),
            r#"{"Solana":444007820}"#
        );
        let block = ReadAt::Robinhood(100);
        assert_eq!(
            serde_json::to_string(&block).expect("json"),
            r#"{"Robinhood":100}"#
        );

        let back: ReadAt = serde_json::from_str(r#"{"Robinhood":100}"#).expect("round-trips");
        assert_eq!(back, block);
    }
}
