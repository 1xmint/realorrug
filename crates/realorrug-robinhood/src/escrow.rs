// SPDX-License-Identifier: Apache-2.0
//! The Pons v2 fee escrow: where a sweep puts the creator's money, and where
//! the payout takes it from.
//!
//! A sweep does not pay the creator. It credits this contract, and the creator
//! claims from it later ([research 0036](../../../docs/research/0036-pons-v2-read-from-a-real-launch.md) §5).
//! So the week's prize is the creator's balance here, and a payout is two
//! transactions: a claim that moves that balance to the payout wallet, then a
//! transfer to the winner. This module reads the first: what the escrow holds
//! for an address, and what a claim transaction took out of it.
//!
//! Every layout is confirmed by `tests/escrow_as_mainnet_wrote_it.rs` against a
//! captured claim, in which the claimer's ETH rose by the claimed amount less
//! the gas, to the wei.

use crate::{Address, Log, Receipt, Rpc, word, word_u128};

/// The escrow, as Pons's docs name it, and the contract the captured sweep and
/// claim both went through.
pub const ESCROW: Address = Address::from_hex("0xd3afeb2a57f70ef218aa82451c51b2fb0416ac9e");

/// Event signatures, each the Keccak-256 of the signature in its comment and
/// checked by a test against a captured log that carries it.
pub mod topic {
    use crate::Hash32;

    /// `Credited(address indexed recipient, address indexed source, uint256 amount)`.
    /// The source is the curve or hook that paid in.
    pub const CREDITED: Hash32 =
        Hash32::from_hex("0x4e45da441832cf53bdaa69235704fc0575e68210f459ee1562911024b12967d5");
    /// `Claimed(address indexed recipient, uint256 amount)`.
    pub const CLAIMED: Hash32 =
        Hash32::from_hex("0xd8138f8a3f377c5259ca548e70e4c2de94f129f5a11036a15b69513cba2b426a");
}

/// The selector of `balanceOf(address)`: what the escrow holds for an address.
pub const BALANCE_OF: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];

/// The selector of `claim(uint256)`. The published interface shows `claim()`
/// with no argument; the deployed escrow was called with an amount, so the
/// captured call is the reference.
pub const CLAIM: [u8; 4] = [0x37, 0x96, 0x07, 0xf5];

/// A `Credited` event from the escrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Credited {
    /// Who may claim it.
    pub recipient: Address,
    /// The curve or hook that paid it in.
    pub source: Address,
    /// Wei.
    pub amount: u128,
}

impl Credited {
    /// The credit a log records, if the escrow emitted it and every field reads.
    #[must_use]
    pub fn from_log(log: &Log) -> Option<Self> {
        if !log.is(&ESCROW, &topic::CREDITED) {
            return None;
        }
        Some(Self {
            recipient: log.topic_address(1)?,
            source: log.topic_address(2)?,
            amount: log.data_u128(0)?,
        })
    }
}

/// A `Claimed` event from the escrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Claimed {
    /// Who was paid.
    pub recipient: Address,
    /// Wei.
    pub amount: u128,
}

impl Claimed {
    /// The claim a log records, if the escrow emitted it and every field reads.
    #[must_use]
    pub fn from_log(log: &Log) -> Option<Self> {
        if !log.is(&ESCROW, &topic::CLAIMED) {
            return None;
        }
        Some(Self {
            recipient: log.topic_address(1)?,
            amount: log.data_u128(0)?,
        })
    }
}

/// The call data asking what the escrow holds for `recipient`.
#[must_use]
pub fn balance_of_call(recipient: &Address) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&BALANCE_OF);
    data.extend_from_slice(&[0; 12]);
    data.extend_from_slice(&recipient.0);
    data
}

/// The call data claiming `amount` wei.
#[must_use]
pub fn claim_call(amount: u128) -> Vec<u8> {
    let mut data = Vec::with_capacity(36);
    data.extend_from_slice(&CLAIM);
    data.extend_from_slice(&[0; 16]);
    data.extend_from_slice(&amount.to_be_bytes());
    data
}

/// An amount from a call's return bytes: exactly one word that fits `u128`.
///
/// Longer returns are refused, not read from the front: a function that
/// returns more than one value is not the function asked.
#[must_use]
pub fn amount_from_return(data: &[u8]) -> Option<u128> {
    if data.len() != 32 {
        return None;
    }
    word(data, 0).and_then(word_u128)
}

/// What the escrow holds for `recipient` now, in wei.
///
/// # Errors
///
/// The endpoint's error, or a return that is not one amount.
pub fn claimable(rpc: &Rpc, recipient: &Address) -> Result<u128, String> {
    let data = rpc.call_contract(&ESCROW, &balance_of_call(recipient))?;
    amount_from_return(&data).ok_or_else(|| format!("balanceOf returned {} bytes", data.len()))
}

/// Why a transaction is not a claim by the given account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotClaimed {
    /// It reverted, so nothing moved.
    Failed,
    /// It was sent to something other than the escrow.
    NotToEscrow(Option<Address>),
    /// Someone else sent it.
    OtherSender(Address),
    /// It holds this many readable `Claimed` events from the escrow, not one.
    Claims(usize),
    /// The one claim paid someone else.
    OtherRecipient(Address),
}

/// The wei the escrow paid `claimant` in this transaction.
///
/// A payout reads its claim back through this before paying anyone: the
/// transaction succeeded, `claimant` sent it to the escrow, and the escrow says
/// it paid exactly one claim, to `claimant`. The amount is the escrow's, not
/// the one asked for; the caller compares them.
///
/// # Errors
///
/// The first [`NotClaimed`] reason, in the order listed there.
pub fn claimed(receipt: &Receipt, claimant: &Address) -> Result<u128, NotClaimed> {
    if !receipt.succeeded {
        return Err(NotClaimed::Failed);
    }
    if receipt.to != Some(ESCROW) {
        return Err(NotClaimed::NotToEscrow(receipt.to));
    }
    if receipt.from != *claimant {
        return Err(NotClaimed::OtherSender(receipt.from));
    }
    let claims: Vec<Claimed> = receipt.logs.iter().filter_map(Claimed::from_log).collect();
    match claims.as_slice() {
        [one] if one.recipient == *claimant => Ok(one.amount),
        [one] => Err(NotClaimed::OtherRecipient(one.recipient)),
        many => Err(NotClaimed::Claims(many.len())),
    }
}
