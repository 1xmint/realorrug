// SPDX-License-Identifier: Apache-2.0
//! The payout: claim the week's creator fees from the Pons escrow, pay exactly
//! what the escrow says was claimed to the winner, read both back, and only then
//! write the ledger.
//!
//! Plan 0001 step 6c, ADR 0025, ADR 0013. On Robinhood Chain a sweep does not
//! pay the creator; it credits the Pons v2 fee escrow, and the creator claims
//! from it (research 0036 §5). So a week's payout is two transactions from the
//! payout wallet, which is the token's creator fee recipient: `claim(amount)`
//! to the escrow, then a plain ETH transfer to the claimed address.
//!
//! # The refusals, and where they live
//!
//! [`realorrug_contest::Payout::permitted`] is the policy: not already paid, not
//! voided, a winner, a claim, the recipient is the claimed address, the amount
//! is at most what was collected, and not below the floor. This crate calls it
//! before the claim and again before the transfer, with the escrow's own figure,
//! and never argues with it. What it adds is what a refusal has to survive:
//!
//! - the chain is Robinhood Chain, and the wallet is the token's creator fee
//!   recipient, or nothing is read further;
//! - the recipient has no code, so the prize never goes to a contract;
//! - both transactions' gas is estimated and funded before the first signature;
//! - Turnkey's signed bytes decode to the fields asked for and recover to the
//!   wallet before they are sent ([`sign_checked`]);
//! - the transfer is read back -- from the wallet, to the claim, for exactly the
//!   claimed wei, with no call data -- before the ledger says paid.
//!
//! # Surviving a crash between the two transactions
//!
//! Before each transaction is sent, its signed bytes and hash are written to
//! `<week>.pending.json` (Radar ADR 0017: the intent before the effect). A run
//! that finds that file resumes that week and does nothing else. A claim that
//! landed is never made again; the transfer pays the claimed amount stored
//! beside it. See [`resume`].
//!
//! # Not a model's hand
//!
//! AGENTS.md rule 1. No model-side crate depends on this one, and
//! `repo-conformance` holds that.

pub mod turnkey;
pub mod tx;

use std::io::Write as _;
use std::path::{Path, PathBuf};

use realorrug_contest::{Balance, Paid, Payout, Record, Refusal, Vault, Week, Wei};
use realorrug_robinhood::escrow::{self, ESCROW};
use realorrug_robinhood::pons::{FACTORY, LaunchedToken};
use realorrug_robinhood::{Address, Hash32, Receipt, Rpc, Tag, Transaction};
use serde::{Deserialize, Serialize};

use crate::tx::{Eip1559, Signed};

/// Robinhood Chain mainnet (research 0035 §1; `eth_chainId` `0x1237` in the
/// captured claim).
pub const CHAIN_ID: u64 = 4663;

/// How many base fees a transaction may pay per gas. Two absorbs a base fee
/// that doubles between estimate and inclusion; the tip is zero, as it was in
/// the captured claim, so the chain charges the base fee and refunds the rest.
pub const FEE_CAP_BASE_FEES: u128 = 2;

/// How many one-second waits for a receipt before the run stops and leaves the
/// pending file for the next one.
pub const RECEIPT_POLLS: u32 = 90;

/// Why nothing was paid, or not all of it.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PayError {
    /// The policy refused. Recorded, never argued with.
    #[error("refused: {0:?}")]
    Refused(Refusal),
    /// The escrow holds nothing for the wallet. Not a refusal: the week is
    /// fine, the pool is empty.
    #[error("nothing collected: the escrow holds 0 wei for {0}")]
    NothingCollected(Address),
    /// The claim is not an address this chain can pay.
    #[error("the claimed address cannot be paid: {0}")]
    BadAddress(String),
    /// The endpoint is not Robinhood Chain.
    #[error("the endpoint is chain {got}, not Robinhood Chain ({CHAIN_ID})")]
    WrongChain {
        /// What it said.
        got: u64,
    },
    /// The wallet is not the token's creator fee recipient, the token is not
    /// known to the factory, or its fees are not paid in ETH.
    #[error("identity: {0}")]
    Identity(String),
    /// The wallet cannot pay for both transactions at the fee cap.
    #[error("gas unfunded: need {need} wei, the wallet holds {have}")]
    GasUnfunded {
        /// What the transactions could cost at the cap, plus any prize to send.
        need: u128,
        /// What the wallet holds.
        have: u128,
    },
    /// Turnkey did not sign: denied by policy, failed, or waiting on consensus.
    #[error("the signer refused: {0}")]
    SignerRefused(String),
    /// The signer returned bytes that are not the transaction asked for, or
    /// not signed by the wallet. Nothing was sent.
    #[error("the signed transaction was not the one asked for: {0}")]
    BadSignature(String),
    /// A pending transaction was never mined and its nonce is now used by
    /// another. Stops for the operator.
    #[error(
        "nonce {nonce} was used by a transaction other than {transaction}; check the wallet by hand"
    )]
    NonceTaken {
        /// The nonce.
        nonce: u64,
        /// The transaction that was expected to use it.
        transaction: Hash32,
    },
    /// The claim reverted. The pending file is cleared and the next run starts
    /// the week over.
    #[error("the claim {0} reverted; the next run starts the week over")]
    ClaimReverted(Hash32),
    /// The chain did not answer, or answered with something unreadable.
    #[error("chain: {0}")]
    Chain(String),
    /// The record, pool or pending file could not be read or written.
    #[error("ledger: {0}")]
    Ledger(String),
    /// A transaction read back does not say what was sent.
    #[error("verification failed: {0}")]
    Verify(String),
    /// Another payout run holds the lock.
    #[error("locked: {0}")]
    Locked(String),
}

/// What the payout asks of the chain. A trait so every path runs against a
/// fake in tests; the real one is [`Rpc`].
pub trait Chain {
    /// The chain id.
    ///
    /// # Errors
    ///
    /// The transport's or the node's reason.
    fn chain_id(&self) -> Result<u64, String>;
    /// The factory's record of a launch, at the latest block.
    ///
    /// # Errors
    ///
    /// The transport's or the node's reason, or a record that does not read.
    fn launched_token(&self, token: &Address) -> Result<LaunchedToken, String>;
    /// What the escrow holds for `holder`, in wei.
    ///
    /// # Errors
    ///
    /// The transport's or the node's reason.
    fn claimable(&self, holder: &Address) -> Result<u128, String>;
    /// An account's ETH, in wei.
    ///
    /// # Errors
    ///
    /// The transport's or the node's reason.
    fn balance(&self, account: &Address) -> Result<u128, String>;
    /// An account's next nonce at `tag`.
    ///
    /// # Errors
    ///
    /// The transport's or the node's reason.
    fn nonce(&self, account: &Address, tag: Tag) -> Result<u64, String>;
    /// The code at an address.
    ///
    /// # Errors
    ///
    /// The transport's or the node's reason.
    fn code(&self, account: &Address) -> Result<Vec<u8>, String>;
    /// The node's gas estimate for a call.
    ///
    /// # Errors
    ///
    /// The transport's or the node's reason, a revert included.
    fn estimate_gas(
        &self,
        from: &Address,
        to: &Address,
        value: u128,
        data: &[u8],
    ) -> Result<u64, String>;
    /// The latest block's base fee, in wei per gas.
    ///
    /// # Errors
    ///
    /// The transport's or the node's reason.
    fn base_fee(&self) -> Result<u128, String>;
    /// Sends signed bytes and returns the hash the node computed.
    ///
    /// # Errors
    ///
    /// The transport's or the node's reason, a rejection included.
    fn send_raw(&self, raw: &[u8]) -> Result<Hash32, String>;
    /// A receipt, or `None` while the transaction is not in a block.
    ///
    /// # Errors
    ///
    /// The transport's or the node's reason.
    fn receipt(&self, hash: &Hash32) -> Result<Option<Receipt>, String>;
    /// A transaction, or `None` when the node does not know it.
    ///
    /// # Errors
    ///
    /// The transport's or the node's reason.
    fn transaction(&self, hash: &Hash32) -> Result<Option<Transaction>, String>;
    /// Waits between receipt polls: a second on the real chain, nothing in a
    /// test.
    fn pause(&self);
}

impl Chain for Rpc {
    fn chain_id(&self) -> Result<u64, String> {
        Self::chain_id(self)
    }
    fn launched_token(&self, token: &Address) -> Result<LaunchedToken, String> {
        let data = self.call_contract(&FACTORY, &LaunchedToken::call_data(token))?;
        LaunchedToken::from_return(&data)
            .ok_or_else(|| format!("getLaunchedToken returned {} unreadable bytes", data.len()))
    }
    fn claimable(&self, holder: &Address) -> Result<u128, String> {
        escrow::claimable(self, holder)
    }
    fn balance(&self, account: &Address) -> Result<u128, String> {
        Self::balance(self, account)
    }
    fn nonce(&self, account: &Address, tag: Tag) -> Result<u64, String> {
        Self::nonce(self, account, tag)
    }
    fn code(&self, account: &Address) -> Result<Vec<u8>, String> {
        Self::code(self, account)
    }
    fn estimate_gas(
        &self,
        from: &Address,
        to: &Address,
        value: u128,
        data: &[u8],
    ) -> Result<u64, String> {
        Self::estimate_gas(self, from, to, value, data)
    }
    fn base_fee(&self) -> Result<u128, String> {
        Self::base_fee(self)
    }
    fn send_raw(&self, raw: &[u8]) -> Result<Hash32, String> {
        let result = self.call(
            "eth_sendRawTransaction",
            &serde_json::json!([realorrug_robinhood::to_hex(raw)]),
        )?;
        result
            .as_str()
            .ok_or("eth_sendRawTransaction returned no hash")?
            .parse()
            .map_err(|e| format!("eth_sendRawTransaction: {e}"))
    }
    fn receipt(&self, hash: &Hash32) -> Result<Option<Receipt>, String> {
        Self::receipt(self, hash)
    }
    fn transaction(&self, hash: &Hash32) -> Result<Option<Transaction>, String> {
        Self::transaction(self, hash)
    }
    fn pause(&self) {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

/// What signs a transaction for the wallet. Turnkey in production, a local key
/// in tests.
pub trait Signer {
    /// The signed bytes for `tx`, unchecked.
    ///
    /// # Errors
    ///
    /// The signer's refusal, in its words.
    fn sign(&self, tx: &Eip1559) -> Result<Vec<u8>, String>;
}

impl Signer for turnkey::Turnkey {
    fn sign(&self, tx: &Eip1559) -> Result<Vec<u8>, String> {
        self.sign_transaction(tx)
    }
}

/// Asks the signer for `tx` and accepts the answer only if it is `tx`, signed
/// by `wallet`.
///
/// Turnkey is trusted to hold the key, not to sign the right thing. The bytes
/// must decode strictly, re-encode to themselves, carry exactly the fields
/// asked for, and recover to the wallet.
///
/// # Errors
///
/// [`PayError::SignerRefused`] when the signer refuses;
/// [`PayError::BadSignature`] when its answer is anything but `tx` from
/// `wallet`.
pub fn sign_checked(
    signer: &dyn Signer,
    tx: &Eip1559,
    wallet: &Address,
) -> Result<Signed, PayError> {
    let raw = signer.sign(tx).map_err(PayError::SignerRefused)?;
    checked(&raw, tx, wallet)
}

fn checked(raw: &[u8], tx: &Eip1559, wallet: &Address) -> Result<Signed, PayError> {
    let signed = tx::decode_signed(raw)
        .map_err(|e| PayError::BadSignature(format!("does not decode: {e}")))?;
    // Strict decoding makes this hold for any input the decoder accepts; it is
    // checked anyway because it is the property that makes the field
    // comparison below a comparison of every byte sent.
    if signed.encode() != raw {
        return Err(PayError::BadSignature("not canonically encoded".to_owned()));
    }
    if signed.tx != *tx {
        return Err(PayError::BadSignature(format!(
            "asked for {tx:?}, signed {:?}",
            signed.tx
        )));
    }
    let from = tx::recover(&signed).map_err(|e| PayError::BadSignature(e.to_string()))?;
    if from != *wallet {
        return Err(PayError::BadSignature(format!(
            "signed by {from}, not the wallet {wallet}"
        )));
    }
    Ok(signed)
}

/// Who pays, for which token, and the floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// The payout wallet: Turnkey's account, and the token's creator fee
    /// recipient.
    pub wallet: Address,
    /// The token whose creator fees are the prize.
    pub token: Address,
    /// The floor, in wei; zero is no floor.
    pub floor: u128,
}

/// The payout floor, in wei, from a getter.
///
/// Design 0007 J2 and design 0009 L4 both say a floor with rollover. **Unset or
/// unreadable means no floor**, which pays out whatever a week collected. A
/// floor is not a permission, it is a threshold below which money is withheld
/// from the person who won it, and the safe direction is paying them; the
/// refusals that bound the key are the others, and none of them defaults open.
#[must_use]
pub fn floor_from(get: &impl Fn(&str) -> Option<String>) -> u128 {
    get("RADAR_PAYOUT_FLOOR_WEI")
        .and_then(|v| Wei::parse(v.trim()))
        .map_or(0, |w| w.0)
}

/// What the run says about the floor it is using, on start, so a mistyped
/// variable is visible rather than found out from a payment.
#[must_use]
pub fn floor_notice(floor: u128) -> String {
    if floor == 0 {
        "realorrug-payout: no floor (RADAR_PAYOUT_FLOOR_WEI unset or unreadable); \
         a week pays out whatever it collected."
            .to_owned()
    } else {
        format!("realorrug-payout: floor {floor} wei; a week below it rolls over unpaid.")
    }
}

/// Checks the endpoint is Robinhood Chain and the wallet is the token's creator
/// fee recipient, whose ETH the escrow holds.
///
/// Only the recipient can claim, so a wallet that is not it would sign a claim
/// that reverts; and a token whose recipient is somebody else is not a token
/// whose fees this contest owns. Before launch the token is unset and `main`
/// never gets here.
///
/// # Errors
///
/// [`PayError::WrongChain`], [`PayError::Identity`], or [`PayError::Chain`].
pub fn preflight(chain: &dyn Chain, config: &Config) -> Result<(), PayError> {
    let got = chain.chain_id().map_err(PayError::Chain)?;
    if got != CHAIN_ID {
        return Err(PayError::WrongChain { got });
    }
    let launch = chain
        .launched_token(&config.token)
        .map_err(PayError::Chain)?;
    if !launch.exists || launch.token != config.token {
        return Err(PayError::Identity(format!(
            "the factory does not know {} as a launch",
            config.token
        )));
    }
    if launch.creator_fee_recipient != config.wallet {
        return Err(PayError::Identity(format!(
            "the creator fee recipient of {} is {}, not the payout wallet {}",
            config.token, launch.creator_fee_recipient, config.wallet
        )));
    }
    // A token paired to an ERC-20 pays its creator in that token, credited as
    // `CreditedToken`, which this payout never claims.
    if let Some(pair) = launch.pair {
        return Err(PayError::Identity(format!(
            "{} is paired to {pair}, so its creator fees are not ETH",
            config.token
        )));
    }
    Ok(())
}

/// A week's payout, planned from the chain and not yet signed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Which week.
    pub week: Week,
    /// The claimed address, parsed.
    pub recipient: Address,
    /// What the escrow holds for the wallet: what is claimed and paid.
    pub amount: u128,
    /// The claim, at the wallet's pending nonce.
    pub claim: Eip1559,
    /// The transfer, at the nonce after it. Rebuilt with a fresh nonce and the
    /// claimed figure when it is actually sent.
    pub transfer: Eip1559,
    /// What both could cost at the fee cap.
    pub need: u128,
    /// What the wallet holds.
    pub have: u128,
}

/// The claimed address, if the chain can pay it.
fn recipient_of(text: &str, wallet: &Address) -> Result<Address, PayError> {
    let address: Address = text
        .parse()
        .map_err(|e| PayError::BadAddress(format!("{text}: {e}")))?;
    if address == Address::ZERO {
        return Err(PayError::BadAddress("the zero address".to_owned()));
    }
    // The operator holds none of the prize (ADR 0013): a claim naming the
    // payout wallet is not a payout.
    if address == *wallet {
        return Err(PayError::BadAddress(format!(
            "{text} is the payout wallet itself"
        )));
    }
    Ok(address)
}

/// Refuses a recipient with code, or whose code cannot be read.
///
/// An ordinary account has none. A contract may refuse the transfer or keep
/// it; an EIP-7702 account (`0xef0100` then an address) runs its delegate's
/// code, which is a contract by another name. Unreadable is unknown, and
/// unknown is not safe (rule 8).
///
/// # Errors
///
/// [`Refusal::NotAWallet`], saying what is there.
pub fn check_wallet(chain: &dyn Chain, recipient: &Address) -> Result<(), PayError> {
    let refuse = |owner| Err(PayError::Refused(Refusal::NotAWallet { owner }));
    match chain.code(recipient) {
        Ok(code) if code.is_empty() => Ok(()),
        Ok(code) if code.len() == 23 && code.starts_with(&[0xef, 0x01, 0x00]) => {
            refuse(Some(format!(
                "EIP-7702 delegation to {}",
                realorrug_robinhood::to_hex(&code[3..])
            )))
        }
        Ok(code) => refuse(Some(format!("contract code, {} bytes", code.len()))),
        Err(_) => refuse(None),
    }
}

/// The fee cap: [`FEE_CAP_BASE_FEES`] times the latest base fee.
fn fee_cap(chain: &dyn Chain) -> Result<u128, PayError> {
    let base = chain.base_fee().map_err(PayError::Chain)?;
    base.checked_mul(FEE_CAP_BASE_FEES)
        .ok_or_else(|| PayError::Chain(format!("base fee {base} overflows")))
}

/// An estimate with a quarter on top, rounded up. The estimate is the node's
/// view of one block; the margin covers the next.
fn gas_limit(estimate: u64) -> Result<u64, PayError> {
    estimate
        .checked_add(estimate.div_ceil(4))
        .ok_or_else(|| PayError::Chain(format!("gas estimate {estimate} overflows")))
}

fn cost(gas: u64, cap: u128) -> Result<u128, PayError> {
    u128::from(gas)
        .checked_mul(cap)
        .ok_or_else(|| PayError::Chain("gas cost overflows".to_owned()))
}

fn claim_text(record: &Record) -> String {
    // The recipient is the claim, never an argument. An unclaimed week has
    // none and the policy says so first.
    record
        .claim
        .as_ref()
        .map(|c| c.address.clone())
        .unwrap_or_default()
}

/// Plans a week: reads the escrow, writes the pool reading, asks the policy,
/// checks the recipient, estimates and funds the gas. Nothing is signed.
///
/// Steps 3 to 6 of ADR 0025's run. Assumes [`preflight`] passed.
///
/// # Errors
///
/// Any [`PayError`] but the signing and sending ones.
pub fn plan(
    chain: &dyn Chain,
    contest_dir: &str,
    week: Week,
    config: &Config,
    now: u64,
) -> Result<Plan, PayError> {
    if pending_path(contest_dir, week).exists() {
        return Err(PayError::Ledger(format!(
            "week {} has a payout in flight; a run without --dry-run resumes it",
            week.0
        )));
    }
    let record = read_record(contest_dir, week)?;
    let amount = chain.claimable(&config.wallet).map_err(PayError::Chain)?;
    write_vault(
        contest_dir,
        &Vault {
            address: ESCROW.to_string(),
            balance: Balance::Eth {
                holder: config.wallet.to_string(),
                wei: Wei(amount),
            },
            measured_at: now,
        },
    )?;
    let claimed = claim_text(&record);
    Payout::permitted(&record, &claimed, amount, amount, config.floor)
        .map_err(PayError::Refused)?;
    if amount == 0 {
        return Err(PayError::NothingCollected(config.wallet));
    }
    let recipient = recipient_of(&claimed, &config.wallet)?;
    check_wallet(chain, &recipient)?;

    let cap = fee_cap(chain)?;
    let data = escrow::claim_call(amount);
    // A claim that would revert fails here, before any signature.
    let claim_gas = gas_limit(
        chain
            .estimate_gas(&config.wallet, &ESCROW, 0, &data)
            .map_err(|e| PayError::Chain(format!("estimating the claim: {e}")))?,
    )?;
    // Estimated with no value: the wallet does not hold the prize until the
    // claim lands, and a node refuses to estimate a transfer the sender cannot
    // fund. The transfer is estimated again with the real amount after the
    // claim, and the quarter margin covers the value's few bytes of L1 data.
    let transfer_gas = gas_limit(
        chain
            .estimate_gas(&config.wallet, &recipient, 0, &[])
            .map_err(|e| PayError::Chain(format!("estimating the transfer: {e}")))?,
    )?;
    let need = cost(claim_gas, cap)?
        .checked_add(cost(transfer_gas, cap)?)
        .ok_or_else(|| PayError::Chain("gas cost overflows".to_owned()))?;
    let have = chain.balance(&config.wallet).map_err(PayError::Chain)?;
    if have < need {
        return Err(PayError::GasUnfunded { need, have });
    }
    let nonce = chain
        .nonce(&config.wallet, Tag::Pending)
        .map_err(PayError::Chain)?;
    Ok(Plan {
        week,
        recipient,
        amount,
        claim: Eip1559 {
            chain_id: CHAIN_ID,
            nonce,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: cap,
            gas_limit: claim_gas,
            to: ESCROW,
            value: 0,
            data,
        },
        transfer: Eip1559 {
            chain_id: CHAIN_ID,
            nonce: nonce.saturating_add(1),
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: cap,
            gas_limit: transfer_gas,
            to: recipient,
            value: amount,
            data: Vec::new(),
        },
        need,
        have,
    })
}

impl Plan {
    /// What a dry run prints: the claim, the transfer and the gas, and the
    /// unsigned claim in the form Turnkey is asked to sign.
    #[must_use]
    pub fn describe(&self) -> String {
        let line = |name: &str, tx: &Eip1559| {
            format!(
                "{name}: to {}, value {} wei, nonce {}, gas limit {}, fee cap {} wei per gas, tip {}",
                tx.to,
                tx.value,
                tx.nonce,
                tx.gas_limit,
                tx.max_fee_per_gas,
                tx.max_priority_fee_per_gas
            )
        };
        format!(
            "week {}: would claim {} wei from the escrow and pay it to {}\n{}\n{}\ngas: up to {} wei for both at the cap; the wallet holds {}\nunsigned claim: {}",
            self.week.0,
            self.amount,
            self.recipient,
            line("claim", &self.claim),
            line("transfer", &self.transfer),
            self.need,
            self.have,
            realorrug_robinhood::to_hex(&self.claim.unsigned()),
        )
    }
}

/// The nonce the setup proof signs at: far above anything the wallet will
/// reach, so the transaction Turnkey signs can never land.
pub const PROOF_NONCE: u64 = 1_000_000;

/// ADR 0025's setup proof: four requests to Turnkey that move no money.
///
/// 1. `whoami` answers for the API key.
/// 2. Signing a call to another contract, with call data, is **denied**.
/// 3. Signing `claim(0)` to the escrow at [`PROOF_NONCE`] is **allowed**, and
///    the answer is the transaction asked for, signed by the wallet.
/// 4. Signing a plain transfer of 1 wei at [`PROOF_NONCE`] is **allowed**, the
///    same way.
///
/// Nothing is sent to any chain. Together 2 and 3 settle what the docs could
/// not: that Turnkey parses chain 4663's transactions and matches the policy on
/// the function. 4 settles how the policy sees empty call data, which the docs
/// do not say; without it a wrong guess would first show after a live claim,
/// with the prize held in the wallet.
///
/// # Errors
///
/// The report so far, when any of the four does not hold.
pub fn setup_proof(
    turnkey: &turnkey::Turnkey,
    wallet: &Address,
) -> Result<Vec<String>, Vec<String>> {
    let mut lines = Vec::new();
    let mut held = true;
    match turnkey.whoami() {
        Ok(who) => lines.push(format!(
            "1. whoami answered, as user {}",
            who["username"].as_str().unwrap_or("(no username)")
        )),
        Err(e) => {
            held = false;
            lines.push(format!("1. whoami failed: {e}"));
        }
    }
    let denied = Eip1559 {
        chain_id: CHAIN_ID,
        nonce: PROOF_NONCE,
        max_priority_fee_per_gas: 0,
        // The captured claim's fee cap and gas, so the only differences between
        // this request and the next are the recipient and the call data.
        max_fee_per_gas: 0x0c44_8890,
        gas_limit: 0xa661,
        to: FACTORY,
        value: 0,
        data: vec![0xde, 0xad, 0xbe, 0xef],
    };
    if let Err(e) = turnkey.sign_transaction(&denied) {
        lines.push(format!("2. a call to the factory was denied: {e}"));
    } else {
        held = false;
        lines.push(
            "2. a call to the factory was SIGNED: the policy allows more than the claim and a transfer"
                .to_owned(),
        );
    }
    let allowed = Eip1559 {
        to: ESCROW,
        data: escrow::claim_call(0),
        ..denied
    };
    match sign_checked(turnkey, &allowed, wallet) {
        Ok(signed) => lines.push(format!(
            "3. claim(0) at nonce {PROOF_NONCE} was signed by {wallet}, hash {}; not sent, and it cannot land",
            signed.hash()
        )),
        Err(e) => {
            held = false;
            lines.push(format!("3. claim(0) was not signed as asked: {e}"));
        }
    }
    let transfer = Eip1559 {
        gas_limit: 21_000,
        to: *wallet,
        value: 1,
        data: Vec::new(),
        ..denied
    };
    match sign_checked(turnkey, &transfer, wallet) {
        Ok(signed) => lines.push(format!(
            "4. a 1 wei transfer at nonce {PROOF_NONCE} was signed by {wallet}, hash {}; not sent, and it cannot land",
            signed.hash()
        )),
        Err(e) => {
            held = false;
            lines.push(format!("4. a plain transfer was not signed as asked: {e}"));
        }
    }
    if held { Ok(lines) } else { Err(lines) }
}

/// A transaction written down before it was sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sent {
    /// The signed bytes, `0x` hex: rebroadcast as they are, so at most one copy
    /// can land.
    pub raw: String,
    /// Their hash.
    pub hash: String,
    /// Their nonce.
    pub nonce: u64,
}

impl Sent {
    fn of(signed: &Signed) -> Self {
        Self {
            raw: realorrug_robinhood::to_hex(&signed.encode()),
            hash: signed.hash().to_string(),
            nonce: signed.tx.nonce,
        }
    }

    /// The signed transaction, checked again: a pending file is a file, and a
    /// torn or edited one must not be rebroadcast.
    fn signed(&self, wallet: &Address) -> Result<Signed, PayError> {
        let bad =
            |why: String| PayError::Ledger(format!("pending transaction {}: {why}", self.hash));
        let raw = realorrug_robinhood::hex_bytes(&self.raw).map_err(|e| bad(e.to_string()))?;
        let signed = tx::decode_signed(&raw).map_err(|e| bad(e.to_string()))?;
        if signed.hash().to_string() != self.hash || signed.tx.nonce != self.nonce {
            return Err(bad("its bytes do not hash to it".to_owned()));
        }
        if tx::recover(&signed).map_err(|e| bad(e.to_string()))? != *wallet {
            return Err(bad("not signed by the wallet".to_owned()));
        }
        Ok(signed)
    }
}

/// The claim, as written before it was sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingClaim {
    /// The transaction.
    #[serde(flatten)]
    pub sent: Sent,
    /// What was asked for.
    pub asked_wei: Wei,
    /// What the escrow said it paid, once the claim is read back. Present
    /// means the claim is done and must never be made again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimed_wei: Option<Wei>,
}

/// The transfer, as written before it was sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingTransfer {
    /// The transaction.
    #[serde(flatten)]
    pub sent: Sent,
    /// What it sends.
    pub wei: Wei,
}

/// `<week>.pending.json`: a payout between its first signature and its record.
///
/// `records_in` ignores the name (it is not `<number>.json`), and serve never
/// publishes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    /// The claim.
    pub claim: PendingClaim,
    /// The transfer, once signed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<PendingTransfer>,
}

/// Where a week's pending payout lives.
#[must_use]
pub fn pending_path(contest_dir: &str, week: Week) -> PathBuf {
    Path::new(contest_dir).join(format!("{}.pending.json", week.0))
}

/// Every week with a pending payout, ascending.
#[must_use]
pub fn pending_weeks(contest_dir: &str) -> Vec<Week> {
    let Ok(listing) = std::fs::read_dir(contest_dir) else {
        return Vec::new();
    };
    let mut weeks: Vec<Week> = listing
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            name.to_str()?
                .strip_suffix(".pending.json")?
                .parse::<u64>()
                .ok()
                .map(Week)
        })
        .collect();
    weeks.sort_unstable();
    weeks
}

fn read_pending(contest_dir: &str, week: Week) -> Result<Pending, PayError> {
    let path = pending_path(contest_dir, week);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| PayError::Ledger(format!("{}: {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| PayError::Ledger(format!("{}: {e}", path.display())))
}

fn write_pending(contest_dir: &str, week: Week, pending: &Pending) -> Result<(), PayError> {
    let text =
        serde_json::to_string_pretty(pending).map_err(|e| PayError::Ledger(e.to_string()))?;
    write_atomically(&pending_path(contest_dir, week).to_string_lossy(), &text)
}

fn clear_pending(contest_dir: &str, week: Week) -> Result<(), PayError> {
    let path = pending_path(contest_dir, week);
    std::fs::remove_file(&path).map_err(|e| PayError::Ledger(format!("{}: {e}", path.display())))
}

/// Sends signed bytes, checks the node computed the same hash, and waits for
/// the receipt.
fn broadcast(chain: &dyn Chain, signed: &Signed) -> Result<Receipt, PayError> {
    let hash = signed.hash();
    let returned = chain.send_raw(&signed.encode()).map_err(PayError::Chain)?;
    if returned != hash {
        return Err(PayError::Verify(format!(
            "the node says it received {returned}; the bytes sent hash to {hash}"
        )));
    }
    wait_for_receipt(chain, &hash)
}

fn wait_for_receipt(chain: &dyn Chain, hash: &Hash32) -> Result<Receipt, PayError> {
    for _ in 0..RECEIPT_POLLS {
        if let Some(receipt) = chain.receipt(hash).map_err(PayError::Chain)? {
            return Ok(receipt);
        }
        chain.pause();
    }
    Err(PayError::Chain(format!(
        "{hash} is not in a block after {RECEIPT_POLLS} polls; the next run resumes it"
    )))
}

/// A transaction written down in an earlier run: its receipt if it landed, or
/// the same bytes sent again if its nonce is still free.
fn settle(chain: &dyn Chain, wallet: &Address, signed: &Signed) -> Result<Receipt, PayError> {
    let hash = signed.hash();
    if let Some(receipt) = chain.receipt(&hash).map_err(PayError::Chain)? {
        return Ok(receipt);
    }
    let landed = chain.nonce(wallet, Tag::Latest).map_err(PayError::Chain)?;
    if landed > signed.tx.nonce {
        // Asked again, because it may have landed between the two reads.
        if let Some(receipt) = chain.receipt(&hash).map_err(PayError::Chain)? {
            return Ok(receipt);
        }
        return Err(PayError::NonceTaken {
            nonce: signed.tx.nonce,
            transaction: hash,
        });
    }
    // Same bytes, same nonce, same hash: at most one copy can land. The node's
    // answer is usually "already known", which is not a failure; the receipt
    // is what decides.
    let _ = chain.send_raw(&signed.encode());
    wait_for_receipt(chain, &hash)
}

/// Reads a claim's receipt: cleared and refused if it reverted, otherwise the
/// escrow's own figure, stored before anything else happens.
fn settle_claim(
    contest_dir: &str,
    week: Week,
    wallet: &Address,
    pending: &mut Pending,
    receipt: &Receipt,
) -> Result<u128, PayError> {
    if !receipt.succeeded {
        clear_pending(contest_dir, week)?;
        return Err(PayError::ClaimReverted(receipt.transaction));
    }
    let claimed = escrow::claimed(receipt, wallet)
        .map_err(|why| PayError::Verify(format!("the claim {}: {why:?}", receipt.transaction)))?;
    pending.claim.claimed_wei = Some(Wei(claimed));
    write_pending(contest_dir, week, pending)?;
    Ok(claimed)
}

/// Reads a transfer back: it succeeded, from the wallet, to the recipient, and
/// the transaction itself sent exactly `wei` with no call data.
///
/// The step the automated run and the manual fallback share.
///
/// # Errors
///
/// [`PayError::Verify`] with what differed; [`PayError::Chain`] when it could
/// not be read.
pub fn verify_transfer(
    chain: &dyn Chain,
    receipt: &Receipt,
    wallet: &Address,
    recipient: &Address,
    wei: u128,
) -> Result<(), PayError> {
    let hash = receipt.transaction;
    if !receipt.succeeded {
        return Err(PayError::Verify(format!("the transfer {hash} reverted")));
    }
    if receipt.from != *wallet || receipt.to != Some(*recipient) {
        return Err(PayError::Verify(format!(
            "the transfer {hash} went from {} to {:?}; the payout is from {wallet} to {recipient}",
            receipt.from, receipt.to
        )));
    }
    let sent = chain
        .transaction(&hash)
        .map_err(PayError::Chain)?
        .ok_or_else(|| PayError::Verify(format!("the node does not know {hash}")))?;
    if sent.hash != hash || sent.from != *wallet || sent.to != Some(*recipient) {
        return Err(PayError::Verify(format!(
            "{hash} reads back as {} from {} to {:?}",
            sent.hash, sent.from, sent.to
        )));
    }
    if sent.value != wei || !sent.input.is_empty() {
        return Err(PayError::Verify(format!(
            "{hash} sent {} wei with {} bytes of call data; the payout is {wei} wei and none",
            sent.value,
            sent.input.len()
        )));
    }
    Ok(())
}

/// Pays a week: plans, claims, transfers, reads back, records. Resumes instead
/// when the week has a pending file.
///
/// # Errors
///
/// Any [`PayError`]. Nothing is signed before the plan passes.
pub fn pay(
    chain: &dyn Chain,
    signer: &dyn Signer,
    contest_dir: &str,
    week: Week,
    config: &Config,
    now: u64,
) -> Result<Payout, PayError> {
    if pending_path(contest_dir, week).exists() {
        return resume(chain, signer, contest_dir, week, config, now);
    }
    let planned = plan(chain, contest_dir, week, config, now)?;
    let claim = sign_checked(signer, &planned.claim, &config.wallet)?;
    let mut pending = Pending {
        claim: PendingClaim {
            sent: Sent::of(&claim),
            asked_wei: Wei(planned.amount),
            claimed_wei: None,
        },
        transfer: None,
    };
    write_pending(contest_dir, week, &pending)?;
    let receipt = broadcast(chain, &claim)?;
    let claimed = settle_claim(contest_dir, week, &config.wallet, &mut pending, &receipt)?;
    transfer(
        chain,
        signer,
        contest_dir,
        week,
        config,
        now,
        claimed,
        &mut pending,
    )
}

/// Finishes a week whose pending file says a payout began.
///
/// - The claim not yet read back: its receipt, or the same bytes again while
///   its nonce is free, or a stop when another transaction took the nonce. A
///   reverted claim clears the file and the next run starts over.
/// - The claim read back: **never claimed again.** The transfer pays the stored
///   figure; one already sent is settled and verified, one that reverted is
///   dropped and sent afresh by the next run.
///
/// # Errors
///
/// Any [`PayError`].
pub fn resume(
    chain: &dyn Chain,
    signer: &dyn Signer,
    contest_dir: &str,
    week: Week,
    config: &Config,
    now: u64,
) -> Result<Payout, PayError> {
    let mut pending = read_pending(contest_dir, week)?;
    let record = read_record(contest_dir, week)?;
    if let Some(paid) = &record.payout {
        // Recorded by an earlier run that died before clearing the file, or by
        // the manual fallback. Cleared only when they name the same transfer.
        let same = pending
            .transfer
            .as_ref()
            .is_some_and(|t| t.sent.hash == paid.transaction());
        if same {
            clear_pending(contest_dir, week)?;
            return Ok(paid.clone());
        }
        return Err(PayError::Verify(format!(
            "week {} is recorded as paid by {}, and a pending payout names another transfer; check both by hand",
            week.0,
            paid.transaction()
        )));
    }
    let claimed = if let Some(claimed) = pending.claim.claimed_wei {
        claimed.0
    } else {
        let claim = pending.claim.sent.signed(&config.wallet)?;
        let receipt = settle(chain, &config.wallet, &claim)?;
        settle_claim(contest_dir, week, &config.wallet, &mut pending, &receipt)?
    };
    if let Some(sent) = pending.transfer.clone() {
        let outgoing = sent.sent.signed(&config.wallet)?;
        let receipt = settle(chain, &config.wallet, &outgoing)?;
        return finish(
            chain,
            contest_dir,
            week,
            config,
            now,
            &record,
            &mut pending,
            &receipt,
        );
    }
    transfer(
        chain,
        signer,
        contest_dir,
        week,
        config,
        now,
        claimed,
        &mut pending,
    )
}

/// Sends the claimed wei to the claim, then finishes.
#[allow(
    clippy::too_many_arguments,
    reason = "one run's state, threaded rather than bundled"
)]
fn transfer(
    chain: &dyn Chain,
    signer: &dyn Signer,
    contest_dir: &str,
    week: Week,
    config: &Config,
    now: u64,
    claimed: u128,
    pending: &mut Pending,
) -> Result<Payout, PayError> {
    let record = read_record(contest_dir, week)?;
    let text = claim_text(&record);
    // The escrow's figure, never the wallet balance, and no floor: the claim
    // has already moved the money, and withholding it now would leave the prize
    // in the operator's wallet.
    Payout::permitted(&record, &text, claimed, claimed, 0).map_err(PayError::Refused)?;
    let recipient = recipient_of(&text, &config.wallet)?;
    check_wallet(chain, &recipient)?;
    let cap = fee_cap(chain)?;
    let gas = gas_limit(
        chain
            .estimate_gas(&config.wallet, &recipient, claimed, &[])
            .map_err(|e| PayError::Chain(format!("estimating the transfer: {e}")))?,
    )?;
    let need = cost(gas, cap)?
        .checked_add(claimed)
        .ok_or_else(|| PayError::Chain("transfer cost overflows".to_owned()))?;
    let have = chain.balance(&config.wallet).map_err(PayError::Chain)?;
    if have < need {
        return Err(PayError::GasUnfunded { need, have });
    }
    let nonce = chain
        .nonce(&config.wallet, Tag::Pending)
        .map_err(PayError::Chain)?;
    let outgoing = sign_checked(
        signer,
        &Eip1559 {
            chain_id: CHAIN_ID,
            nonce,
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: cap,
            gas_limit: gas,
            to: recipient,
            value: claimed,
            data: Vec::new(),
        },
        &config.wallet,
    )?;
    pending.transfer = Some(PendingTransfer {
        sent: Sent::of(&outgoing),
        wei: Wei(claimed),
    });
    write_pending(contest_dir, week, pending)?;
    let receipt = broadcast(chain, &outgoing)?;
    finish(
        chain,
        contest_dir,
        week,
        config,
        now,
        &record,
        pending,
        &receipt,
    )
}

/// Verifies a transfer's receipt and writes the payout, or drops a reverted
/// transfer so the next run sends another.
#[allow(
    clippy::too_many_arguments,
    reason = "one run's state, threaded rather than bundled"
)]
fn finish(
    chain: &dyn Chain,
    contest_dir: &str,
    week: Week,
    config: &Config,
    now: u64,
    record: &Record,
    pending: &mut Pending,
    receipt: &Receipt,
) -> Result<Payout, PayError> {
    let claimed = pending.claim.claimed_wei.ok_or_else(|| {
        PayError::Ledger("a transfer is pending with no claimed amount".to_owned())
    })?;
    if !receipt.succeeded {
        pending.transfer = None;
        write_pending(contest_dir, week, pending)?;
        return Err(PayError::Verify(format!(
            "the transfer {} reverted; the next run sends it again",
            receipt.transaction
        )));
    }
    let text = claim_text(record);
    let recipient = recipient_of(&text, &config.wallet)?;
    verify_transfer(chain, receipt, &config.wallet, &recipient, claimed.0)?;
    let payout = Payout {
        recipient: text,
        paid: Paid::Eth {
            wei: claimed,
            claim_tx: pending.claim.sent.hash.clone(),
            transfer_tx: receipt.transaction.to_string(),
        },
        at: now,
    };
    let mut record = record.clone();
    record.payout = Some(payout.clone());
    write_record(contest_dir, &record)?;
    clear_pending(contest_dir, week)?;
    Ok(payout)
}

/// Records a payout the operator made by hand, after reading both transactions
/// back through the same checks.
///
/// The fallback (design 0007 C5). The claim's receipt gives the escrow's
/// figure; the transfer must send exactly that, from the wallet, to the claim.
/// **No floor**: this reads back money that already left, and refusing to
/// record it would leave a paid week looking unpaid.
///
/// # Errors
///
/// Any [`PayError`], including a transaction already recorded for another week.
pub fn record_payout(
    chain: &dyn Chain,
    contest_dir: &str,
    week: Week,
    wallet: &Address,
    claim_tx: &Hash32,
    transfer_tx: &Hash32,
    now: u64,
) -> Result<Payout, PayError> {
    let got = chain.chain_id().map_err(PayError::Chain)?;
    if got != CHAIN_ID {
        return Err(PayError::WrongChain { got });
    }
    let (claim, transfer) = (claim_tx.to_string(), transfer_tx.to_string());
    // One claim pays one week. Without this the same pair of transactions
    // could be recorded against every unpaid week in the directory.
    for other in realorrug_contest::records_in(Path::new(contest_dir)) {
        if let Some(Payout {
            paid:
                Paid::Eth {
                    claim_tx: used_claim,
                    transfer_tx: used_transfer,
                    ..
                },
            ..
        }) = &other.payout
            && (*used_claim == claim || *used_transfer == transfer)
        {
            return Err(PayError::Verify(format!(
                "week {} already records {used_claim} and {used_transfer}",
                other.week.0
            )));
        }
    }
    let record = read_record(contest_dir, week)?;
    let read = |hash: &Hash32| {
        chain
            .receipt(hash)
            .map_err(PayError::Chain)?
            .ok_or_else(|| PayError::Verify(format!("{hash} is not in a block")))
    };
    let claimed = escrow::claimed(&read(claim_tx)?, wallet)
        .map_err(|why| PayError::Verify(format!("the claim {claim_tx}: {why:?}")))?;
    let text = claim_text(&record);
    Payout::permitted(&record, &text, claimed, claimed, 0).map_err(PayError::Refused)?;
    let recipient = recipient_of(&text, wallet)?;
    verify_transfer(chain, &read(transfer_tx)?, wallet, &recipient, claimed)?;
    let payout = Payout {
        recipient: text,
        paid: Paid::Eth {
            wei: Wei(claimed),
            claim_tx: claim,
            transfer_tx: transfer,
        },
        at: now,
    };
    let mut record = record;
    record.payout = Some(payout.clone());
    write_record(contest_dir, &record)?;
    Ok(payout)
}

/// An exclusively created `payout.lock`, removed when dropped.
///
/// The timer's oneshot cannot overlap itself; this stops a manual run beside
/// it. Two runs would each read the same nonce and ask for the same claim.
#[derive(Debug)]
pub struct Lock {
    path: PathBuf,
}

impl Lock {
    /// Takes the lock.
    ///
    /// # Errors
    ///
    /// [`PayError::Locked`] when the file exists: another run holds it, or one
    /// died holding it. A stale lock stops payouts until the operator checks
    /// nothing is running and deletes it, which is the safe way round.
    pub fn acquire(contest_dir: &str) -> Result<Self, PayError> {
        let path = Path::new(contest_dir).join("payout.lock");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| {
                PayError::Locked(format!(
                    "{}: {e}. If no payout is running, a run died holding it: check the wallet and any .pending.json, then delete the lock",
                    path.display()
                ))
            })?;
        let _ = writeln!(file, "pid {}", std::process::id());
        Ok(Self { path })
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Where a week's record lives.
#[must_use]
pub fn record_path(contest_dir: &str, week: Week) -> String {
    format!("{contest_dir}/{}.json", week.0)
}

/// Reads a week's record.
///
/// # Errors
///
/// [`PayError::Ledger`] when it is missing or does not parse.
pub fn read_record(contest_dir: &str, week: Week) -> Result<Record, PayError> {
    let path = record_path(contest_dir, week);
    let text =
        std::fs::read_to_string(&path).map_err(|e| PayError::Ledger(format!("{path}: {e}")))?;
    Record::from_json(&text).map_err(|e| PayError::Ledger(format!("{path}: {e}")))
}

/// Writes a record via a sibling and a rename, so a reader never sees half.
///
/// # Errors
///
/// [`PayError::Ledger`] with the I/O reason.
pub fn write_record(contest_dir: &str, record: &Record) -> Result<(), PayError> {
    let text = record
        .to_json()
        .map_err(|e| PayError::Ledger(e.to_string()))?;
    write_atomically(&record_path(contest_dir, record.week), &text)
}

/// Writes the pool reading the public pool page serves.
///
/// # Errors
///
/// [`PayError::Ledger`] with the I/O reason.
pub fn write_vault(contest_dir: &str, vault: &Vault) -> Result<(), PayError> {
    let text = vault
        .to_json()
        .map_err(|e| PayError::Ledger(e.to_string()))?;
    write_atomically(&format!("{contest_dir}/pool.json"), &text)
}

fn write_atomically(path: &str, text: &str) -> Result<(), PayError> {
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, text).map_err(|e| PayError::Ledger(format!("{tmp}: {e}")))?;
    std::fs::rename(&tmp, path).map_err(|e| PayError::Ledger(format!("{path}: {e}")))
}

#[cfg(test)]
mod tests;
