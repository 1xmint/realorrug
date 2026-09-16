// SPDX-License-Identifier: Apache-2.0
//! The payout's whole run against a fake chain and a local key.
//!
//! The fake applies what it is sent the way the chain would -- a claim moves
//! the escrow's balance into the wallet and emits `Claimed`, a transfer moves
//! it on -- so every refusal, every read-back and every resume case runs the
//! real code end to end with no network and no Turnkey. Knobs on the fake make
//! the chain misbehave one way at a time.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use k256::FieldBytes;
use k256::ecdsa::SigningKey;
use realorrug_contest::{Claim, Entry, Metrics, Ranked, Ranking, Rules};
use realorrug_robinhood::Log;
use realorrug_robinhood::escrow::topic;

use super::*;

const WEEK: Week = Week(2960);
const TOKEN: Address = Address([0x70; 20]);
const RECIPIENT: Address = Address([0x9a; 20]);
const PRIZE: u128 = 4_014_961_601_594_189_201;
const BASE_FEE: u128 = 102_000_000;
const CLAIM_GAS: u64 = 40_000;
const TRANSFER_GAS: u64 = 21_000;
/// What both transactions can cost at the cap: (50,000 + 26,250) gas at twice
/// the base fee.
const NEED: u128 = 76_250 * 2 * BASE_FEE;

fn key_from(byte: u8) -> SigningKey {
    SigningKey::from_bytes(&FieldBytes::from([byte; 32])).expect("a key")
}

fn wallet() -> Address {
    tx::address_of(key_from(5).verifying_key())
}

fn config() -> Config {
    Config {
        wallet: wallet(),
        token: TOKEN,
        floor: 0,
    }
}

fn sign_with(key: &SigningKey, tx: &Eip1559) -> Signed {
    let (signature, id) = key.sign_prehash_recoverable(&tx.signing_hash());
    let (r, s) = signature.split_bytes();
    Signed {
        tx: tx.clone(),
        y_parity: id.is_y_odd(),
        r: r.into(),
        s: s.into(),
    }
}

/// A signer with a local key, standing in for Turnkey.
struct Local {
    key: SigningKey,
    calls: Cell<u32>,
    /// Refuse the nth request, 1-based.
    refuse_at: Option<u32>,
    /// Sign something other than what was asked.
    tamper: Option<fn(&mut Eip1559)>,
}

impl Local {
    fn new() -> Self {
        Self {
            key: key_from(5),
            calls: Cell::new(0),
            refuse_at: None,
            tamper: None,
        }
    }
}

impl Signer for Local {
    fn sign(&self, tx: &Eip1559) -> Result<Vec<u8>, String> {
        let n = self.calls.get() + 1;
        self.calls.set(n);
        if self.refuse_at == Some(n) {
            return Err("ACTIVITY_STATUS_FAILED: denied by policy".to_owned());
        }
        let mut tx = tx.clone();
        if let Some(tamper) = self.tamper {
            tamper(&mut tx);
        }
        Ok(sign_with(&self.key, &tx).encode())
    }
}

/// A chain that applies what it is sent.
struct Fake {
    chain_id: Cell<u64>,
    fee_recipient: Cell<Address>,
    pair: Cell<Option<Address>>,
    escrow: Cell<u128>,
    wallet_eth: Cell<u128>,
    recipient_code: RefCell<Result<Vec<u8>, String>>,
    claim_estimate_reverts: Cell<bool>,
    latest: Cell<u64>,
    pending: Cell<u64>,
    /// Whether a sent transaction lands.
    lands: Cell<bool>,
    /// Receipts for transactions to this address are not returned.
    hide_receipts_to: Cell<Option<Address>>,
    escrow_pays_short: Cell<u128>,
    /// Taken from the wallet when a claim lands, standing in for its gas.
    claim_spends: Cell<u128>,
    claim_reverts: Cell<bool>,
    transfer_reverts: Cell<bool>,
    /// Added to the value a transaction reads back with.
    readback_extra: Cell<u128>,
    sent: RefCell<Vec<Vec<u8>>>,
    receipts: RefCell<HashMap<Hash32, Receipt>>,
    txs: RefCell<HashMap<Hash32, Transaction>>,
}

fn fake() -> Fake {
    Fake {
        chain_id: Cell::new(CHAIN_ID),
        fee_recipient: Cell::new(wallet()),
        pair: Cell::new(None),
        escrow: Cell::new(PRIZE),
        wallet_eth: Cell::new(NEED),
        recipient_code: RefCell::new(Ok(Vec::new())),
        claim_estimate_reverts: Cell::new(false),
        latest: Cell::new(0),
        pending: Cell::new(0),
        lands: Cell::new(true),
        hide_receipts_to: Cell::new(None),
        escrow_pays_short: Cell::new(0),
        claim_spends: Cell::new(0),
        claim_reverts: Cell::new(false),
        transfer_reverts: Cell::new(false),
        readback_extra: Cell::new(0),
        sent: RefCell::new(Vec::new()),
        receipts: RefCell::new(HashMap::new()),
        txs: RefCell::new(HashMap::new()),
    }
}

impl Fake {
    fn sent_to(&self, to: Address) -> usize {
        self.sent
            .borrow()
            .iter()
            .filter(|raw| tx::decode_signed(raw).expect("decodes").tx.to == to)
            .count()
    }
}

fn word(value: u128) -> Vec<u8> {
    let mut out = vec![0u8; 16];
    out.extend_from_slice(&value.to_be_bytes());
    out
}

impl Chain for Fake {
    fn chain_id(&self) -> Result<u64, String> {
        Ok(self.chain_id.get())
    }
    fn launched_token(&self, token: &Address) -> Result<LaunchedToken, String> {
        Ok(LaunchedToken {
            token: *token,
            curve: Address([1; 20]),
            deployer: Address([2; 20]),
            creator_fee_recipient: self.fee_recipient.get(),
            pair: self.pair.get(),
            graduation_threshold: 0,
            creator_tax_bps: 200,
            buyback: false,
            phase: 0,
            exists: *token == TOKEN,
        })
    }
    fn claimable(&self, holder: &Address) -> Result<u128, String> {
        Ok(if *holder == wallet() {
            self.escrow.get()
        } else {
            0
        })
    }
    fn balance(&self, account: &Address) -> Result<u128, String> {
        Ok(if *account == wallet() {
            self.wallet_eth.get()
        } else {
            0
        })
    }
    fn nonce(&self, _: &Address, tag: Tag) -> Result<u64, String> {
        Ok(match tag {
            Tag::Latest => self.latest.get(),
            Tag::Pending => self.pending.get(),
        })
    }
    fn code(&self, _: &Address) -> Result<Vec<u8>, String> {
        self.recipient_code.borrow().clone()
    }
    fn estimate_gas(&self, _: &Address, to: &Address, _: u128, _: &[u8]) -> Result<u64, String> {
        if *to == ESCROW {
            if self.claim_estimate_reverts.get() {
                return Err("execution reverted".to_owned());
            }
            return Ok(CLAIM_GAS);
        }
        Ok(TRANSFER_GAS)
    }
    fn base_fee(&self) -> Result<u128, String> {
        Ok(BASE_FEE)
    }
    fn send_raw(&self, raw: &[u8]) -> Result<Hash32, String> {
        self.sent.borrow_mut().push(raw.to_vec());
        let signed = tx::decode_signed(raw).map_err(|e| e.to_string())?;
        let hash = signed.hash();
        if self.receipts.borrow().contains_key(&hash) {
            return Err("already known".to_owned());
        }
        if !self.lands.get() {
            self.pending
                .set(self.pending.get().max(signed.tx.nonce + 1));
            return Ok(hash);
        }
        if signed.tx.nonce != self.latest.get() {
            return Err(format!("nonce too low: {}", signed.tx.nonce));
        }
        self.latest.set(self.latest.get() + 1);
        self.pending.set(self.pending.get().max(self.latest.get()));
        let from = tx::recover(&signed).map_err(|e| e.to_string())?;
        let mut logs = Vec::new();
        let succeeded = if signed.tx.to == ESCROW {
            let ok = !self.claim_reverts.get();
            if ok {
                let asked = u128::from_be_bytes(signed.tx.data[20..36].try_into().expect("16"));
                let paid = asked - self.escrow_pays_short.get();
                self.escrow.set(self.escrow.get() - paid);
                self.wallet_eth
                    .set(self.wallet_eth.get() + paid - self.claim_spends.get());
                let mut who = [0u8; 32];
                who[12..].copy_from_slice(&from.0);
                logs.push(Log {
                    address: ESCROW,
                    topics: vec![topic::CLAIMED, Hash32(who)],
                    data: word(paid),
                    block: 1,
                    transaction: hash,
                });
            }
            ok
        } else {
            let ok = !self.transfer_reverts.get();
            if ok {
                self.wallet_eth.set(self.wallet_eth.get() - signed.tx.value);
            }
            ok
        };
        self.receipts.borrow_mut().insert(
            hash,
            Receipt {
                transaction: hash,
                block: 1,
                succeeded,
                from,
                to: Some(signed.tx.to),
                logs,
            },
        );
        self.txs.borrow_mut().insert(
            hash,
            Transaction {
                hash,
                from,
                to: Some(signed.tx.to),
                value: signed.tx.value + self.readback_extra.get(),
                input: signed.tx.data.clone(),
                nonce: signed.tx.nonce,
                block: Some(1),
            },
        );
        Ok(hash)
    }
    fn receipt(&self, hash: &Hash32) -> Result<Option<Receipt>, String> {
        let receipt = self.receipts.borrow().get(hash).cloned();
        Ok(receipt.filter(|r| r.to != self.hide_receipts_to.get()))
    }
    fn transaction(&self, hash: &Hash32) -> Result<Option<Transaction>, String> {
        Ok(self.txs.borrow().get(hash).cloned())
    }
    fn pause(&self) {}
}

fn record_claimed_by(address: &str) -> Record {
    let mut ranking = Ranking::default();
    ranking.ranked.push(Ranked {
        entry: Entry {
            reply_id: "r1".to_owned(),
            summoner: "alice".to_owned(),
            mention_id: None,
            handle: None,
            mint: "M".to_owned(),
            at: WEEK.opens_at() + 10,
            metrics: Metrics {
                likes: 3,
                ..Metrics::default()
            },
        },
        score: 3,
    });
    let mut record = Record::close(WEEK, ranking, &Rules::published(["op"]));
    record.claim = Some(Claim {
        address: address.to_owned(),
        reply_id: "c1".to_owned(),
        at: WEEK.closes_at() + 100,
    });
    record
}

fn dir_with(name: &str, record: &Record) -> String {
    let dir = std::env::temp_dir().join(format!("realorrug-payout-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let dir = dir.to_string_lossy().into_owned();
    write_record(&dir, record).expect("record");
    dir
}

fn claimed_dir(name: &str) -> String {
    dir_with(name, &record_claimed_by(&RECIPIENT.to_string()))
}

const NOW: u64 = 1_790_600_000;

fn pay_once(chain: &Fake, signer: &Local, dir: &str) -> Result<Payout, PayError> {
    pay(chain, signer, dir, WEEK, &config(), NOW)
}

#[test]
fn the_happy_path_claims_then_pays_exactly_what_the_escrow_paid_and_records_both() {
    let d = claimed_dir("happy");
    let chain = fake();
    let signer = Local::new();
    let payout = pay_once(&chain, &signer, &d).expect("paid");

    let sent = chain.sent.borrow();
    assert_eq!(sent.len(), 2, "a claim and a transfer");
    let claim = tx::decode_signed(&sent[0]).expect("claim");
    let transfer = tx::decode_signed(&sent[1]).expect("transfer");
    assert_eq!(claim.tx.to, ESCROW);
    assert_eq!(claim.tx.data, escrow::claim_call(PRIZE));
    assert_eq!(claim.tx.value, 0);
    assert_eq!(claim.tx.chain_id, CHAIN_ID);
    assert_eq!(claim.tx.max_fee_per_gas, 2 * BASE_FEE);
    assert_eq!(claim.tx.max_priority_fee_per_gas, 0);
    assert_eq!(claim.tx.gas_limit, 50_000, "the estimate and a quarter");
    assert_eq!((claim.tx.nonce, transfer.tx.nonce), (0, 1));
    assert_eq!(transfer.tx.to, RECIPIENT);
    assert_eq!(transfer.tx.value, PRIZE);
    assert!(transfer.tx.data.is_empty());
    assert_eq!(transfer.tx.gas_limit, 26_250);

    assert_eq!(
        payout,
        Payout {
            recipient: RECIPIENT.to_string(),
            paid: Paid::Eth {
                wei: Wei(PRIZE),
                claim_tx: claim.hash().to_string(),
                transfer_tx: transfer.hash().to_string(),
            },
            at: NOW,
        }
    );
    assert_eq!(read_record(&d, WEEK).expect("record").payout, Some(payout));
    assert!(!pending_path(&d, WEEK).exists(), "the pending file is gone");
    let pool = Vault::from_json(&std::fs::read_to_string(format!("{d}/pool.json")).expect("pool"))
        .expect("reads");
    assert_eq!(pool.address, ESCROW.to_string());
    assert_eq!(
        pool.balance,
        Balance::Eth {
            holder: wallet().to_string(),
            wei: Wei(PRIZE)
        }
    );

    // Paying again refuses as already paid and signs nothing more.
    assert!(matches!(
        pay_once(&chain, &signer, &d),
        Err(PayError::Refused(Refusal::AlreadyPaid { .. }))
    ));
    assert_eq!(signer.calls.get(), 2);
}

#[test]
fn a_policy_refusal_or_an_unpayable_claim_signs_nothing_and_sends_nothing() {
    type Case = (&'static str, Record, u128, u128, fn(&PayError) -> bool);
    let cases: Vec<Case> = vec![
        (
            "unclaimed",
            {
                let mut r = record_claimed_by(&RECIPIENT.to_string());
                r.claim = None;
                r
            },
            PRIZE,
            0,
            |e| matches!(e, PayError::Refused(Refusal::Unclaimed)),
        ),
        (
            "voided",
            {
                let mut r = record_claimed_by(&RECIPIENT.to_string());
                r.voided = Some(realorrug_contest::Voided {
                    at: 1,
                    reason: "bought".to_owned(),
                });
                r
            },
            PRIZE,
            0,
            |e| matches!(e, PayError::Refused(Refusal::Voided { .. })),
        ),
        (
            "below-floor",
            record_claimed_by(&RECIPIENT.to_string()),
            PRIZE,
            PRIZE + 1,
            |e| matches!(e, PayError::Refused(Refusal::BelowFloor { .. })),
        ),
        (
            "empty-escrow",
            record_claimed_by(&RECIPIENT.to_string()),
            0,
            0,
            |e| matches!(e, PayError::NothingCollected(_)),
        ),
        (
            // A claim from before the move: a Solana address, refused before
            // anything is signed.
            "solana-claim",
            record_claimed_by("So11111111111111111111111111111111111111112"),
            PRIZE,
            0,
            |e| matches!(e, PayError::BadAddress(_)),
        ),
        (
            "zero-address",
            record_claimed_by(&Address::ZERO.to_string()),
            PRIZE,
            0,
            |e| matches!(e, PayError::BadAddress(_)),
        ),
        (
            "the-wallet-itself",
            record_claimed_by(&wallet().to_string()),
            PRIZE,
            0,
            |e| matches!(e, PayError::BadAddress(_)),
        ),
    ];
    for (name, record, escrow_holds, floor, expected) in cases {
        let d = dir_with(name, &record);
        let chain = fake();
        chain.escrow.set(escrow_holds);
        let signer = Local::new();
        let got = pay(
            &chain,
            &signer,
            &d,
            WEEK,
            &Config { floor, ..config() },
            NOW,
        );
        assert!(got.as_ref().is_err_and(expected), "{name}: {got:?}");
        assert_eq!(signer.calls.get(), 0, "{name}: nothing signed");
        assert!(chain.sent.borrow().is_empty(), "{name}: nothing sent");
        assert!(!pending_path(&d, WEEK).exists(), "{name}");
    }
}

#[test]
fn the_wrong_chain_the_wrong_wallet_or_a_token_paid_in_a_token_stops_the_run() {
    // Re-apply by deleting the recipient comparison: a wallet that is not the
    // creator fee recipient passes, and would sign a claim that reverts.
    assert_eq!(preflight(&fake(), &config()), Ok(()));

    let chain = fake();
    chain.chain_id.set(1);
    assert_eq!(
        preflight(&chain, &config()),
        Err(PayError::WrongChain { got: 1 })
    );

    let chain = fake();
    chain.fee_recipient.set(Address([0x33; 20]));
    assert!(matches!(
        preflight(&chain, &config()),
        Err(PayError::Identity(why)) if why.contains("creator fee recipient")
    ));

    let chain = fake();
    chain.pair.set(Some(Address([0x44; 20])));
    assert!(matches!(
        preflight(&chain, &config()),
        Err(PayError::Identity(why)) if why.contains("not ETH")
    ));

    assert!(matches!(
        preflight(
            &fake(),
            &Config {
                token: Address([0x71; 20]),
                ..config()
            }
        ),
        Err(PayError::Identity(why)) if why.contains("does not know")
    ));
}

#[test]
fn a_recipient_with_code_is_refused_whatever_the_code_is() {
    // A contract, an EIP-7702 delegation, and code that cannot be read.
    // Re-apply by accepting 7702 code: the second case pays a delegate.
    type Case = (Result<Vec<u8>, String>, Option<&'static str>);
    let mut delegation = vec![0xef, 0x01, 0x00];
    delegation.extend_from_slice(&[0x55; 20]);
    let cases: Vec<Case> = vec![
        (
            Ok(vec![0x60, 0x80, 0x60, 0x40]),
            Some("contract code, 4 bytes"),
        ),
        // Twenty-three bytes of ordinary code is a contract, not a delegation.
        // Re-apply `||` for `&&` in the 7702 test: this reads as one.
        (Ok(vec![0x60; 23]), Some("contract code, 23 bytes")),
        (Ok(delegation), Some("EIP-7702 delegation to 0x5555")),
        (Err("timeout".to_owned()), None),
    ];
    for (i, (code, says)) in cases.into_iter().enumerate() {
        let d = claimed_dir(&format!("code-{i}"));
        let chain = fake();
        *chain.recipient_code.borrow_mut() = code;
        let signer = Local::new();
        match pay_once(&chain, &signer, &d) {
            Err(PayError::Refused(Refusal::NotAWallet { owner })) => match says {
                Some(prefix) => assert!(
                    owner.as_deref().is_some_and(|o| o.starts_with(prefix)),
                    "{owner:?}"
                ),
                None => assert_eq!(owner, None),
            },
            other => panic!("case {i}: {other:?}"),
        }
        assert_eq!(signer.calls.get(), 0);
        assert!(chain.sent.borrow().is_empty());
    }
}

#[test]
fn gas_that_cannot_be_paid_at_the_cap_signs_nothing() {
    // Re-apply by comparing with `<=`: exactly enough is refused and the
    // second half fails.
    let d = claimed_dir("gas");
    let chain = fake();
    chain.wallet_eth.set(NEED - 1);
    let signer = Local::new();
    assert_eq!(
        pay_once(&chain, &signer, &d),
        Err(PayError::GasUnfunded {
            need: NEED,
            have: NEED - 1
        })
    );
    assert_eq!(signer.calls.get(), 0);
    assert!(chain.sent.borrow().is_empty());

    chain.wallet_eth.set(NEED);
    pay_once(&chain, &signer, &d).expect("exactly enough pays");

    // A claim that would revert fails at its estimate, before any signature.
    let d = claimed_dir("gas-revert");
    let chain = fake();
    chain.claim_estimate_reverts.set(true);
    let signer = Local::new();
    assert!(matches!(
        pay_once(&chain, &signer, &d),
        Err(PayError::Chain(why)) if why.contains("estimating the claim")
    ));
    assert_eq!(signer.calls.get(), 0);
}

#[test]
fn a_claim_that_pays_less_than_asked_is_paid_on_at_the_escrows_figure() {
    // Re-apply by transferring `planned.amount`: the transfer asks for more
    // than the escrow paid, and the value assertion fails.
    let d = claimed_dir("short");
    let chain = fake();
    chain.escrow_pays_short.set(7);
    let payout = pay_once(&chain, &Local::new(), &d).expect("paid");
    assert!(matches!(payout.paid, Paid::Eth { wei, .. } if wei == Wei(PRIZE - 7)));
    let transfer = tx::decode_signed(&chain.sent.borrow()[1]).expect("transfer");
    assert_eq!(transfer.tx.value, PRIZE - 7);
}

#[test]
fn a_transfer_that_reads_back_wrong_is_not_recorded() {
    // Re-apply by writing the record before `verify_transfer`: the ledger says
    // paid with a transfer the chain reads back as a different amount.
    let d = claimed_dir("readback");
    let chain = fake();
    chain.readback_extra.set(1);
    let got = pay_once(&chain, &Local::new(), &d);
    assert!(
        matches!(got, Err(PayError::Verify(ref why)) if why.contains("sent")),
        "{got:?}"
    );
    assert_eq!(read_record(&d, WEEK).expect("record").payout, None);
    let pending = read_pending(&d, WEEK).expect("kept");
    assert!(pending.transfer.is_some(), "kept for the operator to see");
}

#[test]
fn turnkey_refusing_is_an_answer_at_either_step_and_a_claimed_prize_is_not_claimed_again() {
    // At the claim: nothing sent, nothing written down.
    let d = claimed_dir("refuse-claim");
    let chain = fake();
    let signer = Local {
        refuse_at: Some(1),
        ..Local::new()
    };
    assert!(matches!(
        pay_once(&chain, &signer, &d),
        Err(PayError::SignerRefused(why)) if why.contains("denied by policy")
    ));
    assert!(chain.sent.borrow().is_empty());
    assert!(!pending_path(&d, WEEK).exists());

    // At the transfer: the claim landed, and the pending file keeps what it
    // paid. The next run transfers without claiming.
    let d = claimed_dir("refuse-transfer");
    let chain = fake();
    let signer = Local {
        refuse_at: Some(2),
        ..Local::new()
    };
    assert!(matches!(
        pay_once(&chain, &signer, &d),
        Err(PayError::SignerRefused(_))
    ));
    assert_eq!(
        read_pending(&d, WEEK).expect("kept").claim.claimed_wei,
        Some(Wei(PRIZE))
    );
    let payout = pay_once(&chain, &Local::new(), &d).expect("resumed");
    assert!(matches!(payout.paid, Paid::Eth { wei, .. } if wei == Wei(PRIZE)));
    assert_eq!(chain.sent_to(ESCROW), 1, "one claim, ever");
    assert_eq!(chain.sent_to(RECIPIENT), 1);
}

#[test]
fn a_signature_over_other_fields_or_from_another_key_is_never_sent() {
    // Re-apply by skipping the field comparison: the doubled value is sent.
    // Re-apply by skipping the recovery: the stranger's claim is sent.
    let d = claimed_dir("tamper");
    let chain = fake();
    let signer = Local {
        tamper: Some(|tx| tx.max_fee_per_gas *= 2),
        ..Local::new()
    };
    assert!(matches!(
        pay_once(&chain, &signer, &d),
        Err(PayError::BadSignature(why)) if why.contains("asked for")
    ));
    assert!(chain.sent.borrow().is_empty());

    let d = claimed_dir("stranger");
    let chain = fake();
    let signer = Local {
        key: key_from(6),
        ..Local::new()
    };
    assert!(matches!(
        pay_once(&chain, &signer, &d),
        Err(PayError::BadSignature(why)) if why.contains("not the wallet")
    ));
    assert!(chain.sent.borrow().is_empty());
    assert!(!pending_path(&d, WEEK).exists());

    // Bytes that are not a transaction at all.
    let tx = fake_plan_claim();
    assert!(matches!(
        checked(&[0x02, 0xc0], &tx, &wallet()),
        Err(PayError::BadSignature(_))
    ));
}

fn fake_plan_claim() -> Eip1559 {
    Eip1559 {
        chain_id: CHAIN_ID,
        nonce: 0,
        max_priority_fee_per_gas: 0,
        max_fee_per_gas: 1,
        gas_limit: 1,
        to: ESCROW,
        value: 0,
        data: escrow::claim_call(1),
    }
}

#[test]
fn a_claim_sent_but_not_mined_is_sent_again_byte_for_byte_and_never_signed_twice() {
    let d = claimed_dir("unmined");
    let chain = fake();
    chain.lands.set(false);
    let signer = Local::new();
    assert!(matches!(
        pay_once(&chain, &signer, &d),
        Err(PayError::Chain(why)) if why.contains("not in a block")
    ));
    assert_eq!(signer.calls.get(), 1);
    assert!(pending_path(&d, WEEK).exists());

    // The node drops it, the nonce is still free, the rerun sends the same
    // bytes. Re-apply by signing a fresh claim on resume: the signer count
    // below is three and the two claim byte strings differ.
    chain.lands.set(true);
    pay_once(&chain, &signer, &d).expect("resumed and paid");
    let sent = chain.sent.borrow();
    assert_eq!(sent[0], sent[1], "the same claim, rebroadcast");
    assert_eq!(signer.calls.get(), 2, "the claim once, the transfer once");
}

#[test]
fn a_claim_whose_nonce_another_transaction_took_stops_for_the_operator() {
    let d = claimed_dir("taken");
    let chain = fake();
    chain.lands.set(false);
    let signer = Local::new();
    let _ = pay_once(&chain, &signer, &d);
    // Something else from the wallet landed at nonce 0.
    chain.latest.set(1);
    chain.lands.set(true);
    match pay_once(&chain, &signer, &d) {
        Err(PayError::NonceTaken { nonce: 0, .. }) => {}
        other => panic!("{other:?}"),
    }
    assert_eq!(chain.sent.borrow().len(), 1, "not rebroadcast");
    assert_eq!(signer.calls.get(), 1, "and not signed again");
    assert!(pending_path(&d, WEEK).exists(), "left for the operator");
}

#[test]
fn a_reverted_claim_clears_the_file_and_the_week_starts_over() {
    let d = claimed_dir("reverted");
    let chain = fake();
    chain.claim_reverts.set(true);
    let signer = Local::new();
    assert!(matches!(
        pay_once(&chain, &signer, &d),
        Err(PayError::ClaimReverted(_))
    ));
    assert!(!pending_path(&d, WEEK).exists());
    chain.claim_reverts.set(false);
    pay_once(&chain, &signer, &d).expect("started over and paid");
    assert_eq!(
        chain.sent_to(ESCROW),
        2,
        "the reverted claim and the new one"
    );
}

#[test]
fn the_claim_landed_and_the_run_died_so_the_rerun_transfers_without_claiming() {
    // The case the pending file exists for. Re-apply by planning afresh when
    // the claim has no stored figure: the escrow now holds nothing, and the
    // rerun reports NothingCollected with the prize sitting in the wallet.
    let d = claimed_dir("died-after-claim");
    let chain = fake();
    chain.hide_receipts_to.set(Some(ESCROW));
    let signer = Local::new();
    assert!(pay_once(&chain, &signer, &d).is_err());
    assert_eq!(chain.escrow.get(), 0, "the claim landed");
    assert_eq!(
        read_pending(&d, WEEK).expect("kept").claim.claimed_wei,
        None
    );

    chain.hide_receipts_to.set(None);
    let payout = pay_once(&chain, &signer, &d).expect("resumed");
    assert!(matches!(payout.paid, Paid::Eth { wei, .. } if wei == Wei(PRIZE)));
    assert_eq!(chain.sent_to(ESCROW), 1, "never claimed again");
    assert_eq!(chain.sent_to(RECIPIENT), 1);
    assert_eq!(signer.calls.get(), 2);
}

#[test]
fn a_transfer_that_landed_before_the_run_died_is_verified_and_recorded_not_sent_again() {
    let d = claimed_dir("died-after-transfer");
    let chain = fake();
    chain.hide_receipts_to.set(Some(RECIPIENT));
    let signer = Local::new();
    assert!(pay_once(&chain, &signer, &d).is_err());
    assert!(read_pending(&d, WEEK).expect("kept").transfer.is_some());

    chain.hide_receipts_to.set(None);
    let payout = pay_once(&chain, &signer, &d).expect("recorded");
    assert_eq!(read_record(&d, WEEK).expect("record").payout, Some(payout));
    assert_eq!(chain.sent.borrow().len(), 2, "nothing more sent");
    assert_eq!(signer.calls.get(), 2);
    assert!(!pending_path(&d, WEEK).exists());
}

#[test]
fn a_reverted_transfer_is_dropped_and_sent_afresh_by_the_next_run() {
    let d = claimed_dir("transfer-reverted");
    let chain = fake();
    chain.transfer_reverts.set(true);
    let signer = Local::new();
    assert!(matches!(
        pay_once(&chain, &signer, &d),
        Err(PayError::Verify(why)) if why.contains("reverted")
    ));
    let pending = read_pending(&d, WEEK).expect("kept");
    assert_eq!(pending.transfer, None);
    assert_eq!(pending.claim.claimed_wei, Some(Wei(PRIZE)));

    chain.transfer_reverts.set(false);
    let payout = pay_once(&chain, &signer, &d).expect("paid");
    let second = tx::decode_signed(&chain.sent.borrow()[2]).expect("the new transfer");
    assert_eq!(payout.transaction(), second.hash().to_string());
    assert_eq!(second.tx.nonce, 2, "a fresh nonce");
    assert_eq!(chain.sent_to(ESCROW), 1);
}

#[test]
fn a_pending_file_edited_by_hand_is_not_sent() {
    let d = claimed_dir("edited");
    let chain = fake();
    chain.lands.set(false);
    let signer = Local::new();
    let _ = pay_once(&chain, &signer, &d);
    let mut pending = read_pending(&d, WEEK).expect("kept");
    // One byte of the call data: the amount claimed.
    let mut raw = realorrug_robinhood::hex_bytes(&pending.claim.sent.raw).expect("hex");
    let at = raw.len() - 70;
    raw[at] ^= 1;
    pending.claim.sent.raw = realorrug_robinhood::to_hex(&raw);
    write_pending(&d, WEEK, &pending).expect("write");
    chain.lands.set(true);
    assert!(matches!(
        pay_once(&chain, &signer, &d),
        Err(PayError::Ledger(why)) if why.contains("do not hash")
    ));
    assert_eq!(chain.sent.borrow().len(), 1);
}

#[test]
fn a_week_already_recorded_clears_a_pending_file_that_names_the_same_transfer_only() {
    let d = claimed_dir("recorded");
    let chain = fake();
    let signer = Local::new();
    let payout = pay_once(&chain, &signer, &d).expect("paid");
    // The run died between writing the record and clearing the file.
    let pending = Pending {
        claim: PendingClaim {
            sent: Sent {
                raw: "0x".to_owned(),
                hash: "0xc1".to_owned(),
                nonce: 0,
            },
            asked_wei: Wei(PRIZE),
            claimed_wei: Some(Wei(PRIZE)),
        },
        transfer: Some(PendingTransfer {
            sent: Sent {
                raw: "0x".to_owned(),
                hash: payout.transaction().to_owned(),
                nonce: 1,
            },
            wei: Wei(PRIZE),
        }),
    };
    write_pending(&d, WEEK, &pending).expect("write");
    assert_eq!(pay_once(&chain, &signer, &d), Ok(payout));
    assert!(!pending_path(&d, WEEK).exists());

    // A pending file naming a different transfer is not tidied away.
    let mut other = pending;
    other.transfer.as_mut().expect("t").sent.hash = "0xother".to_owned();
    write_pending(&d, WEEK, &other).expect("write");
    assert!(matches!(
        pay_once(&chain, &signer, &d),
        Err(PayError::Verify(why)) if why.contains("by hand")
    ));
    assert!(pending_path(&d, WEEK).exists());
}

#[test]
fn a_week_with_a_payout_in_flight_is_not_planned_again_and_records_in_ignores_its_file() {
    let d = claimed_dir("in-flight");
    let chain = fake();
    chain.lands.set(false);
    let _ = pay_once(&chain, &Local::new(), &d);
    assert!(matches!(
        plan(&chain, &d, WEEK, &config(), NOW),
        Err(PayError::Ledger(why)) if why.contains("in flight")
    ));
    assert_eq!(pending_weeks(&d), vec![WEEK]);
    let weeks: Vec<Week> = realorrug_contest::records_in(Path::new(&d))
        .into_iter()
        .map(|r| r.week)
        .collect();
    assert_eq!(weeks, vec![WEEK], "the pending file is not a record");
}

#[test]
fn a_run_holds_the_lock_and_a_second_is_refused_until_it_is_released() {
    let d = claimed_dir("lock");
    let held = Lock::acquire(&d).expect("the first run takes it");
    assert!(matches!(Lock::acquire(&d), Err(PayError::Locked(_))));
    drop(held);
    Lock::acquire(&d).expect("released on drop");
}

#[test]
fn the_manual_fallback_records_only_what_reads_back_as_this_weeks_payout() {
    // Pay week A through the fake so the chain holds a real claim and
    // transfer, then offer those hashes to the fallback in other states.
    let paid_dir = claimed_dir("fallback-source");
    let chain = fake();
    let payout = pay_once(&chain, &Local::new(), &paid_dir).expect("paid");
    let Paid::Eth {
        claim_tx,
        transfer_tx,
        ..
    } = &payout.paid
    else {
        panic!("eth");
    };
    let claim_tx: Hash32 = claim_tx.parse().expect("hash");
    let transfer_tx: Hash32 = transfer_tx.parse().expect("hash");

    // The same week, unpaid: recorded, with the escrow's figure.
    let d = claimed_dir("fallback-good");
    let got =
        record_payout(&chain, &d, WEEK, &wallet(), &claim_tx, &transfer_tx, NOW).expect("recorded");
    assert_eq!(got.paid, payout.paid);

    // And a second week offered the same transactions is refused. Re-apply by
    // deleting the loop over `records_in`: one payment is recorded twice.
    let mut next = record_claimed_by(&RECIPIENT.to_string());
    next.week = Week(WEEK.0 + 1);
    write_record(&d, &next).expect("write");
    assert!(matches!(
        record_payout(&chain, &d, next.week, &wallet(), &claim_tx, &transfer_tx, NOW),
        Err(PayError::Verify(why)) if why.contains("already records")
    ));

    // A claim to somebody else: the transfer does not read back to it.
    let d = dir_with(
        "fallback-other",
        &record_claimed_by(&Address([0x9b; 20]).to_string()),
    );
    assert!(matches!(
        record_payout(&chain, &d, WEEK, &wallet(), &claim_tx, &transfer_tx, NOW),
        Err(PayError::Verify(_))
    ));
    assert_eq!(read_record(&d, WEEK).expect("record").payout, None);

    // Another wallet's claim.
    let d = claimed_dir("fallback-stranger");
    assert!(matches!(
        record_payout(&chain, &d, WEEK, &Address([0x11; 20]), &claim_tx, &transfer_tx, NOW),
        Err(PayError::Verify(why)) if why.contains("OtherSender")
    ));

    // A transaction that is not in a block.
    assert!(matches!(
        record_payout(&chain, &d, WEEK, &wallet(), &Hash32([1; 32]), &transfer_tx, NOW),
        Err(PayError::Verify(why)) if why.contains("not in a block")
    ));
}

#[test]
fn the_floor_is_read_in_wei_and_the_run_says_which_it_is_using() {
    assert_eq!(floor_from(&|_| None), 0);
    assert_eq!(floor_from(&|_| Some(" 0.1 ".to_owned())), 0);
    assert_eq!(
        floor_from(&|k| (k == "RADAR_PAYOUT_FLOOR_WEI").then(|| " 20000000000000000 ".to_owned())),
        20_000_000_000_000_000
    );
    assert!(floor_notice(0).contains("no floor"));
    assert!(floor_notice(5).contains("5 wei"));
}

#[test]
fn a_raw_transaction_is_sent_as_hex_and_the_nodes_hash_is_read_back() {
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let hash = format!("0x{}", "ab".repeat(32));
    let answer = format!(r#"{{"jsonrpc":"2.0","id":1,"result":"{hash}"}}"#);
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("a connection");
        let mut reader = BufReader::new(stream);
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("header");
            if line == "\r\n" {
                break;
            }
            if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = v.trim().parse().expect("length");
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).expect("body");
        let mut stream = reader.into_inner();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
            answer.len()
        )
        .expect("respond");
        serde_json::from_slice::<serde_json::Value>(&body).expect("json")
    });
    let got = Chain::send_raw(&Rpc::new(url), &[0x02, 0xc0]);
    assert_eq!(got, Ok(hash.parse().expect("hash")));
    let request = server.join().expect("served");
    assert_eq!(request["method"], "eth_sendRawTransaction");
    assert_eq!(request["params"], serde_json::json!(["0x02c0"]));
}

#[test]
fn the_dry_run_names_the_amount_the_recipient_both_transactions_and_the_unsigned_claim() {
    // What an operator reads before signing by hand. Re-apply by returning an
    // empty string: every assertion fails.
    let d = claimed_dir("describe");
    let planned = plan(&fake(), &d, WEEK, &config(), NOW).expect("planned");
    let text = planned.describe();
    assert!(
        text.starts_with(&format!("week {}: would claim {PRIZE} wei", WEEK.0)),
        "{text}"
    );
    assert!(text.contains(&RECIPIENT.to_string()), "{text}");
    assert!(
        text.contains(&format!(
            "claim: to {ESCROW}, value 0 wei, nonce 0, gas limit 50000"
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "transfer: to {RECIPIENT}, value {PRIZE} wei, nonce 1"
        )),
        "{text}"
    );
    assert!(text.contains(&format!("gas: up to {NEED} wei")), "{text}");
    assert!(
        text.ends_with(&realorrug_robinhood::to_hex(&planned.claim.unsigned())),
        "{text}"
    );
}

#[test]
fn a_transfer_is_verified_field_by_field_in_the_receipt_and_the_transaction() {
    // Each field wrong on its own, with every other field right, so an `||`
    // turned into `&&` anywhere lets exactly one case through.
    let hash = Hash32([7; 32]);
    let good_receipt = Receipt {
        transaction: hash,
        block: 1,
        succeeded: true,
        from: wallet(),
        to: Some(RECIPIENT),
        logs: Vec::new(),
    };
    let good_tx = Transaction {
        hash,
        from: wallet(),
        to: Some(RECIPIENT),
        value: PRIZE,
        input: Vec::new(),
        nonce: 1,
        block: Some(1),
    };
    let check = |receipt: Receipt, sent: Transaction| {
        let chain = fake();
        chain.txs.borrow_mut().insert(hash, sent);
        verify_transfer(&chain, &receipt, &wallet(), &RECIPIENT, PRIZE)
    };
    assert_eq!(check(good_receipt.clone(), good_tx.clone()), Ok(()));
    let stranger = Address([0x33; 20]);
    let cases: Vec<(&str, Receipt, Transaction)> = vec![
        (
            "reverted",
            Receipt {
                succeeded: false,
                ..good_receipt.clone()
            },
            good_tx.clone(),
        ),
        (
            "receipt from",
            Receipt {
                from: stranger,
                ..good_receipt.clone()
            },
            good_tx.clone(),
        ),
        (
            "receipt to",
            Receipt {
                to: Some(stranger),
                ..good_receipt.clone()
            },
            good_tx.clone(),
        ),
        (
            "tx hash",
            good_receipt.clone(),
            Transaction {
                hash: Hash32([8; 32]),
                ..good_tx.clone()
            },
        ),
        (
            "tx from",
            good_receipt.clone(),
            Transaction {
                from: stranger,
                ..good_tx.clone()
            },
        ),
        (
            "tx to",
            good_receipt.clone(),
            Transaction {
                to: Some(stranger),
                ..good_tx.clone()
            },
        ),
        (
            "value",
            good_receipt.clone(),
            Transaction {
                value: PRIZE - 1,
                ..good_tx.clone()
            },
        ),
        (
            "input",
            good_receipt.clone(),
            Transaction {
                input: vec![0],
                ..good_tx.clone()
            },
        ),
    ];
    for (name, receipt, sent) in cases {
        assert!(
            matches!(check(receipt, sent), Err(PayError::Verify(_))),
            "{name} wrong must not verify"
        );
    }
}

#[test]
fn the_transfer_is_refused_when_the_claim_left_too_little_for_its_gas() {
    // The balance check before the transfer, at its boundary. Re-apply `<=`:
    // exactly enough is refused. Re-apply `==`: one wei short is sent.
    let transfer_need = 26_250 * 2 * BASE_FEE + PRIZE;
    let spends_to_exactly = NEED - 26_250 * 2 * BASE_FEE;

    let d = claimed_dir("transfer-gas-short");
    let chain = fake();
    chain.claim_spends.set(spends_to_exactly + 1);
    assert_eq!(
        pay_once(&chain, &Local::new(), &d),
        Err(PayError::GasUnfunded {
            need: transfer_need,
            have: transfer_need - 1
        })
    );
    assert_eq!(chain.sent_to(RECIPIENT), 0);
    assert_eq!(
        read_pending(&d, WEEK).expect("kept").claim.claimed_wei,
        Some(Wei(PRIZE)),
        "the claim is kept for the run after the float is topped up"
    );

    let d = claimed_dir("transfer-gas-exact");
    let chain = fake();
    chain.claim_spends.set(spends_to_exactly);
    pay_once(&chain, &Local::new(), &d).expect("exactly enough pays");
}
#[test]
fn a_transfer_the_node_does_not_know_is_not_verified() {
    let receipt = Receipt {
        transaction: Hash32([7; 32]),
        block: 1,
        succeeded: true,
        from: wallet(),
        to: Some(RECIPIENT),
        logs: Vec::new(),
    };
    assert!(matches!(
        verify_transfer(&fake(), &receipt, &wallet(), &RECIPIENT, PRIZE),
        Err(PayError::Verify(why)) if why.contains("does not know")
    ));
}

#[test]
fn the_fallback_refuses_either_transaction_already_recorded_and_nothing_else() {
    let paid_dir = claimed_dir("reuse-source");
    let chain = fake();
    let payout = pay_once(&chain, &Local::new(), &paid_dir).expect("paid");
    let Paid::Eth {
        claim_tx,
        transfer_tx,
        ..
    } = &payout.paid
    else {
        panic!("eth");
    };
    let claim: Hash32 = claim_tx.parse().expect("hash");
    let transfer: Hash32 = transfer_tx.parse().expect("hash");

    let other_week = |claim_tx: &str, transfer_tx: &str| {
        let mut record = record_claimed_by(&RECIPIENT.to_string());
        record.week = Week(WEEK.0 - 1);
        record.payout = Some(Payout {
            recipient: RECIPIENT.to_string(),
            paid: Paid::Eth {
                wei: Wei(1),
                claim_tx: claim_tx.to_owned(),
                transfer_tx: transfer_tx.to_owned(),
            },
            at: 1,
        });
        record
    };
    // Another week paid by other transactions does not block this one.
    // Re-apply `!=` for `==` in either comparison: this is refused.
    let d = claimed_dir("reuse-unrelated");
    write_record(&d, &other_week("0xaa", "0xbb")).expect("write");
    record_payout(&chain, &d, WEEK, &wallet(), &claim, &transfer, NOW).expect("recorded");

    // Only the claim reused, or only the transfer: each refused. Re-apply `&&`
    // for `||`: both of these record.
    for (name, reused) in [
        ("claim", other_week(claim_tx, "0xbb")),
        ("transfer", other_week("0xaa", transfer_tx)),
    ] {
        let d = claimed_dir(&format!("reuse-{name}"));
        write_record(&d, &reused).expect("write");
        assert!(
            matches!(
                record_payout(&chain, &d, WEEK, &wallet(), &claim, &transfer, NOW),
                Err(PayError::Verify(why)) if why.contains("already records")
            ),
            "{name}"
        );
        assert_eq!(read_record(&d, WEEK).expect("record").payout, None);
    }
}

/// A JSON-RPC node that answers each connection with the next result.
fn node(results: Vec<&'static str>) -> String {
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    std::thread::spawn(move || {
        for result in results {
            let (stream, _) = listener.accept().expect("a connection");
            let mut reader = BufReader::new(stream);
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("header");
                if line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().expect("length");
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).expect("body");
            let answer = format!(r#"{{"jsonrpc":"2.0","id":1,"result":{result}}}"#);
            let mut stream = reader.into_inner();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                answer.len()
            )
            .expect("respond");
        }
    });
    url
}

#[test]
fn the_real_chain_passes_each_read_through_to_the_node() {
    // The trait's `Rpc` impl is a row of one-line forwards, and a forward that
    // returned a constant would pass every fake-chain test. Re-apply by
    // returning `Ok(0)` from any of them: its assertion fails.
    let rpc = Rpc::new(node(vec![
        r#""0x1237""#,
        r#"{"number":"0x1","baseFeePerGas":"0x4255720"}"#,
        r#""0x2a""#,
        r#""0x3""#,
        r#""0x6001""#,
        r#""0x7fa7""#,
    ]));
    assert_eq!(Chain::chain_id(&rpc), Ok(4663));
    assert_eq!(Chain::base_fee(&rpc), Ok(0x0425_5720));
    assert_eq!(Chain::balance(&rpc, &wallet()), Ok(42));
    assert_eq!(Chain::nonce(&rpc, &wallet(), Tag::Pending), Ok(3));
    assert_eq!(Chain::code(&rpc, &RECIPIENT), Ok(vec![0x60, 0x01]));
    assert_eq!(
        Chain::estimate_gas(&rpc, &wallet(), &RECIPIENT, 1, &[]),
        Ok(0x7fa7)
    );
}

#[test]
fn the_real_chain_waits_a_second_between_receipt_polls() {
    // Without the wait, ninety polls take milliseconds and a claim that needs
    // a few blocks is abandoned to the next day's run. Re-apply by making
    // `pause` do nothing: this fails.
    let started = std::time::Instant::now();
    Chain::pause(&Rpc::new("http://127.0.0.1:1"));
    assert!(started.elapsed() >= std::time::Duration::from_secs(1));
}
