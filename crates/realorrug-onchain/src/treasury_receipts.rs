// SPDX-License-Identifier: Apache-2.0
//! Fee accounting: what the treasury has received on Solana, and what is
//! still unclaimed. ADR 0037's Consequences names this as "fee accounting
//! reads the treasury's receipts on Solana"; `realorrug treasury solana`
//! (`realorrug-cli/src/treasury_solana.rs`) is its caller, run by Josh from
//! `deploy/LAUNCH.md`'s "After launch" step, by hand, to reconcile before he
//! records a payment.
//!
//! # A receipt is a balance movement, not a signer
//!
//! ADR 0037 leaves the treasury's form -- a plain wallet or a multisig --
//! undecided. Identifying a receipt by "the treasury signed a collect
//! instruction" would break the moment it becomes a multisig, because a
//! multisig's *execute* is a different transaction from the *proposal*, and
//! neither necessarily has the treasury itself as a signer of the
//! fee-collecting transaction. Identifying a receipt by "a known fee vault's
//! balance fell, in a transaction the treasury appears in" survives that
//! choice: it is true regardless of who held the pen. Whether a `collect`
//! instruction was present is still recorded, as corroboration -- never as
//! the test itself (design decision, packet `treasury-receipts.md`).
//!
//! # Absent is not zero (AGENTS §3 rule 8)
//!
//! A signature whose transaction could not be read is not "no receipt" --
//! it is a gap, carried in [`Unread`] so the caller's totals can print "at
//! least" rather than a number that looks exact and is not.

use std::collections::HashSet;

use realorrug_decode::{Decoded, Program, decode};
use realorrug_types::{Address, Slot};

use crate::rpc::Transaction;

/// Which fee vault a receipt drained.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VaultKind {
    /// pump.fun's bonding-curve creator vault. Amount is lamports.
    PumpFun,
    /// PumpSwap's coin-creator vault, a WSOL token account. Amount is WSOL
    /// base units (also lamports-denominated, since WSOL is 1:1 with SOL).
    PumpSwap,
}

impl VaultKind {
    /// The label used in a report line.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PumpFun => "pump.fun creator vault",
            Self::PumpSwap => "PumpSwap coin-creator vault",
        }
    }
}

/// The two vault addresses a treasury's receipts are read against, both
/// derived from the treasury address itself: pump.fun and PumpSwap both key
/// their creator vault off the address fees are paid to, which is the
/// treasury (ADR 0037 decision 2, "creator fees go to a disclosed project
/// treasury wallet").
#[derive(Clone, Debug)]
pub struct VaultAddresses {
    /// `realorrug_pumpfun::pda::creator_vault(&treasury)`.
    pub pumpfun: Address,
    /// `realorrug_pumpfun::pda::pumpswap_coin_creator_vault_ata(&treasury)`.
    pub pumpswap: Address,
}

/// One signature and the transaction read for it, or `None` when the read
/// did not succeed -- the caller (`treasury_solana.rs`) attaches the reason
/// separately, in [`Unread::why`].
#[derive(Clone, Debug)]
pub struct SignedTransaction {
    /// The transaction signature.
    pub signature: String,
    /// The transaction, or `None` when it could not be read.
    pub transaction: Option<Transaction>,
}

/// One receipt: a known fee vault's balance fell inside a transaction the
/// treasury appears in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    /// The transaction signature.
    pub signature: String,
    /// The slot it landed in.
    pub slot: Slot,
    /// Which vault fell.
    pub vault: VaultKind,
    /// The vault's decrease, in its own smallest unit (lamports for both
    /// kinds here, since WSOL shares SOL's decimals).
    pub amount: u64,
    /// Whether a `collect_creator_fee`, `collect_creator_fee_v2` or
    /// `collect_coin_creator_fee` instruction (top-level or inner) was
    /// present. Corroboration, never the test: see the module doc.
    pub collect_instruction_seen: bool,
}

/// A signature this reader could not settle: no transaction to check, so it
/// is neither a receipt nor safely "not a receipt".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unread {
    /// The transaction signature.
    pub signature: String,
    /// Why it could not be read.
    pub why: String,
}

/// The pure core: every receipt a batch of transactions implies, plus
/// everything it could not read. No RPC call here -- `treasury_solana.rs`
/// does every read and hands the results in, which is what makes this
/// testable with synthetic transactions (packet `treasury-receipts.md`,
/// "Keep a pure function").
///
/// A signature repeated in `transactions` counts once (plan 0002 names a
/// duplicated fee claim as a failure to test): the first entry for a given
/// signature wins and the rest are skipped outright, read or not.
#[must_use]
pub fn find_receipts(
    vaults: &VaultAddresses,
    transactions: &[SignedTransaction],
) -> (Vec<Receipt>, Vec<Unread>) {
    let mut receipts = Vec::new();
    let mut unread = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();

    for entry in transactions {
        if !seen.insert(entry.signature.as_str()) {
            continue;
        }
        let Some(tx) = &entry.transaction else {
            unread.push(Unread {
                signature: entry.signature.clone(),
                why: "transaction could not be read".to_owned(),
            });
            continue;
        };
        // Without `meta` there are no balances to compare, so every vault
        // would read as unmoved and the transaction would vanish from the
        // report as "no fee collected" (rule 8: absent is not zero).
        if !tx.meta_present {
            unread.push(Unread {
                signature: entry.signature.clone(),
                why: "the node returned no outcome for this transaction".to_owned(),
            });
            continue;
        }
        // A failed transaction moved nothing -- SignatureInfo::err already
        // filters most of these out before a caller gets here, but a
        // caller building `transactions` some other way (a test, or a
        // future full-transaction page) should not have to filter twice.
        if tx.failed {
            continue;
        }

        if let Some(amount) = lamports_decrease(tx, &vaults.pumpfun) {
            receipts.push(Receipt {
                signature: entry.signature.clone(),
                slot: tx.slot,
                vault: VaultKind::PumpFun,
                amount,
                collect_instruction_seen: has_collect_instruction(tx, Program::PumpFun),
            });
        }
        if let Some(amount) = token_decrease(tx, &vaults.pumpswap) {
            receipts.push(Receipt {
                signature: entry.signature.clone(),
                slot: tx.slot,
                vault: VaultKind::PumpSwap,
                amount,
                collect_instruction_seen: has_collect_instruction(tx, Program::PumpSwap),
            });
        }
    }

    (receipts, unread)
}

/// The pump.fun vault's native-lamport decrease inside `tx`, if it fell.
fn lamports_decrease(tx: &Transaction, vault: &Address) -> Option<u64> {
    let vault_s = vault.to_string();
    let index = tx.accounts.iter().position(|a| *a == vault_s)?;
    let before = *tx.pre_balances.get(index)?;
    let after = *tx.post_balances.get(index)?;
    before.checked_sub(after).filter(|d| *d > 0)
}

/// The PumpSwap WSOL vault's token-balance decrease inside `tx`, if it fell.
fn token_decrease(tx: &Transaction, vault: &Address) -> Option<u64> {
    let vault_s = vault.to_string();
    let index = tx.accounts.iter().position(|a| *a == vault_s)?;
    let before = tx
        .pre_token_balances
        .iter()
        .find(|b| b.account_index == index)
        .map_or(0, |b| b.amount);
    let after = tx
        .post_token_balances
        .iter()
        .find(|b| b.account_index == index)
        .map_or(0, |b| b.amount);
    before.checked_sub(after).filter(|d| *d > 0)
}

/// Whether `tx` carries a collect instruction for `program`, top-level or
/// inner -- [`Transaction::instructions`] is already flattened, so this is
/// one scan rather than two.
fn has_collect_instruction(tx: &Transaction, program: Program) -> bool {
    let program_id = match program {
        Program::PumpFun => realorrug_decode::pumpfun::PROGRAM_ID.to_string(),
        Program::PumpSwap => realorrug_decode::pumpswap::PROGRAM_ID.to_string(),
    };
    tx.instructions.iter().any(|ix| {
        ix.program == program_id
            && matches!(decode(program, &ix.data), Decoded::Known(found) if is_collect(found))
    })
}

/// Whether a decoded instruction is one of the three collect variants this
/// module cares about.
fn is_collect(ix: realorrug_decode::Instruction) -> bool {
    if let Some(p) = ix.pumpfun() {
        return matches!(
            p,
            realorrug_decode::pumpfun::Instruction::CollectCreatorFee
                | realorrug_decode::pumpfun::Instruction::CollectCreatorFeeV2
        );
    }
    matches!(
        ix.pumpswap(),
        Some(realorrug_decode::pumpswap::Instruction::CollectCoinCreatorFee)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::{RawInstruction, TokenBalance};

    fn addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn vaults() -> VaultAddresses {
        VaultAddresses {
            pumpfun: addr(1),
            pumpswap: addr(2),
        }
    }

    fn base_tx(accounts: &[Address]) -> Transaction {
        let len = accounts.len();
        Transaction {
            slot: Slot(100),
            accounts: accounts.iter().map(ToString::to_string).collect(),
            instructions: Vec::new(),
            pre_token_balances: Vec::new(),
            post_token_balances: Vec::new(),
            pre_balances: vec![0; len],
            post_balances: vec![0; len],
            failed: false,
            meta_present: true,
        }
    }

    fn collect_ix(program: &Address, discriminator: [u8; 8]) -> RawInstruction {
        RawInstruction {
            program: program.to_string(),
            data: discriminator.to_vec(),
            accounts: Vec::new(),
        }
    }

    #[test]
    fn a_plain_pumpfun_receipt_is_counted() {
        let v = vaults();
        let mut tx = base_tx(&[addr(9), v.pumpfun]);
        tx.pre_balances = vec![0, 5_000_000_000];
        tx.post_balances = vec![0, 1_000_000_000];
        tx.instructions = vec![collect_ix(
            &realorrug_decode::pumpfun::PROGRAM_ID,
            realorrug_decode::pumpfun::Instruction::CollectCreatorFee
                .discriminator()
                .as_bytes()
                .to_owned(),
        )];

        let (receipts, unread) = find_receipts(
            &v,
            &[SignedTransaction {
                signature: "sig1".to_owned(),
                transaction: Some(tx),
            }],
        );

        assert!(unread.is_empty());
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].vault, VaultKind::PumpFun);
        assert_eq!(receipts[0].amount, 4_000_000_000);
        assert!(receipts[0].collect_instruction_seen);
    }

    #[test]
    fn a_pumpswap_wsol_receipt_is_counted() {
        let v = vaults();
        let mut tx = base_tx(&[addr(9), v.pumpswap]);
        tx.pre_token_balances = vec![TokenBalance {
            account_index: 1,
            mint: "wsol".to_owned(),
            amount: 3_000_000,
            owner: None,
        }];
        tx.post_token_balances = vec![TokenBalance {
            account_index: 1,
            mint: "wsol".to_owned(),
            amount: 500_000,
            owner: None,
        }];

        let (receipts, unread) = find_receipts(
            &v,
            &[SignedTransaction {
                signature: "sig2".to_owned(),
                transaction: Some(tx),
            }],
        );

        assert!(unread.is_empty());
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].vault, VaultKind::PumpSwap);
        assert_eq!(receipts[0].amount, 2_500_000);
        assert!(!receipts[0].collect_instruction_seen);
    }

    #[test]
    fn a_multisig_style_receipt_is_counted_without_the_treasury_signing() {
        // The treasury never has to be a signer of the transaction that
        // collects into its vault -- a multisig's execute transaction is
        // signed by its own signers, not by the multisig account itself.
        // This is the whole point of testing balance movement rather than
        // "the treasury signed": nothing here reads who signed at all.
        let v = vaults();
        let mut tx = base_tx(&[addr(9), v.pumpfun]);
        tx.pre_balances = vec![0, 2_000_000_000];
        tx.post_balances = vec![0, 0];
        tx.instructions = vec![collect_ix(
            &realorrug_decode::pumpfun::PROGRAM_ID,
            realorrug_decode::pumpfun::Instruction::CollectCreatorFeeV2
                .discriminator()
                .as_bytes()
                .to_owned(),
        )];

        let (receipts, _) = find_receipts(
            &v,
            &[SignedTransaction {
                signature: "sig3".to_owned(),
                transaction: Some(tx),
            }],
        );

        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].amount, 2_000_000_000);
    }

    #[test]
    fn a_transaction_with_no_vault_decrease_is_not_a_receipt() {
        let v = vaults();
        let mut tx = base_tx(&[addr(9), v.pumpfun, v.pumpswap]);
        // The pump.fun vault's balance is unchanged and the PumpSwap vault
        // is not even present in this transaction's balances.
        tx.pre_balances = vec![0, 1_000_000_000, 0];
        tx.post_balances = vec![0, 1_000_000_000, 0];

        let (receipts, unread) = find_receipts(
            &v,
            &[SignedTransaction {
                signature: "sig4".to_owned(),
                transaction: Some(tx),
            }],
        );

        assert!(receipts.is_empty());
        assert!(unread.is_empty());
    }

    #[test]
    fn the_same_signature_twice_counts_once() {
        let v = vaults();
        let mut tx = base_tx(&[addr(9), v.pumpfun]);
        tx.pre_balances = vec![0, 1_000_000_000];
        tx.post_balances = vec![0, 0];

        let entry = SignedTransaction {
            signature: "sig5".to_owned(),
            transaction: Some(tx),
        };
        let (receipts, _) = find_receipts(&v, &[entry.clone(), entry]);

        assert_eq!(receipts.len(), 1);
    }

    #[test]
    fn an_unreadable_transaction_is_carried_as_unread_not_zero() {
        let v = vaults();
        let (receipts, unread) = find_receipts(
            &v,
            &[SignedTransaction {
                signature: "sig6".to_owned(),
                transaction: None,
            }],
        );

        assert!(receipts.is_empty());
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].signature, "sig6");
    }

    #[test]
    fn a_failed_transaction_is_not_a_receipt() {
        let v = vaults();
        let mut tx = base_tx(&[addr(9), v.pumpfun]);
        tx.pre_balances = vec![0, 1_000_000_000];
        tx.post_balances = vec![0, 0];
        tx.failed = true;

        let (receipts, unread) = find_receipts(
            &v,
            &[SignedTransaction {
                signature: "sig7".to_owned(),
                transaction: Some(tx),
            }],
        );

        assert!(receipts.is_empty());
        assert!(unread.is_empty());
    }

    /// A transaction the node returned without `meta` has no balances, so
    /// without the guard it would read as "no vault moved" and disappear.
    #[test]
    fn a_transaction_without_its_outcome_is_listed_as_unread() {
        let v = vaults();
        let mut tx = base_tx(&[addr(9), v.pumpfun]);
        tx.pre_balances = Vec::new();
        tx.post_balances = Vec::new();
        tx.meta_present = false;

        let (receipts, unread) = find_receipts(
            &v,
            &[SignedTransaction {
                signature: "sig8".to_owned(),
                transaction: Some(tx),
            }],
        );

        assert!(receipts.is_empty());
        assert_eq!(
            unread,
            vec![Unread {
                signature: "sig8".to_owned(),
                why: "the node returned no outcome for this transaction".to_owned(),
            }]
        );
    }
}
