// SPDX-License-Identifier: Apache-2.0
//! The Pons v2 fee escrow, decoded from the transactions mainnet accepted.
//!
//! Two captures in `docs/research/data`, described in research 0036 §5:
//!
//! - `0036-escrow-claim.json`: a claim of 4.0149 ETH, with the escrow's record
//!   and both ETH balances in the block before and the block of the claim.
//! - `0036-pons-v2-launch.json`: the fee sweep whose credits land here.
//!
//! The claim is recomputed to the wei: the escrow's record of the claimer fell
//! by the claimed amount, the escrow's ETH fell by it, and the claimer's ETH
//! rose by it less the gas. Then each way a transaction can fail to be a claim
//! is re-applied to it.

use realorrug_robinhood::escrow::{
    BALANCE_OF, CLAIM, Claimed, Credited, ESCROW, NotClaimed, amount_from_return, balance_of_call,
    claim_call, claimed, topic,
};
use realorrug_robinhood::pons::Sweep;
use realorrug_robinhood::{Address, Receipt, hex_bytes};

const CLAIM_CAPTURE: &str = include_str!("../../../docs/research/data/0036-escrow-claim.json");
const LAUNCH: &str = include_str!("../../../docs/research/data/0036-pons-v2-launch.json");

/// The claimed amount, as the captured call and its `Claimed` event carry it.
const AMOUNT: u128 = 4_014_961_601_594_189_201;
const CLAIMER: Address = Address::from_hex("0x6aa025a3292c4ab6a55af3b6a7f7cbf62a5c4d06");

fn read(capture: &str, why: &str) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_str(capture).expect("the capture is JSON");
    value["reads"]
        .as_array()
        .expect("reads")
        .iter()
        .find(|r| r["why"].as_str().is_some_and(|w| w.starts_with(why)))
        .unwrap_or_else(|| panic!("the capture has a read starting {why:?}"))
        .clone()
}

fn result_hex(capture: &str, why: &str) -> Vec<u8> {
    hex_bytes(read(capture, why)["result"].as_str().expect("hex")).expect("the result is hex")
}

fn wei(why: &str) -> u128 {
    let text = read(CLAIM_CAPTURE, why)["result"]
        .as_str()
        .expect("a quantity")
        .to_owned();
    u128::from_str_radix(text.trim_start_matches("0x"), 16).expect("fits")
}

fn claim_receipt() -> Receipt {
    Receipt::from_json(&read(CLAIM_CAPTURE, "the claim receipt")["result"]).expect("parses")
}

#[test]
fn the_constants_are_the_ones_mainnet_used() {
    let tx = read(CLAIM_CAPTURE, "the claim transaction")["result"].clone();
    let input = hex_bytes(tx["input"].as_str().expect("input")).expect("hex");
    assert_eq!(input[..4], CLAIM);
    assert_eq!(
        claim_call(AMOUNT),
        input,
        "the payout's encoding is mainnet's"
    );
    assert_eq!(
        tx["to"].as_str().expect("to").parse::<Address>(),
        Ok(ESCROW)
    );

    let call = read(CLAIM_CAPTURE, "escrow at block 62853729: balanceOf");
    let data = hex_bytes(call["params"][0]["data"].as_str().expect("data")).expect("hex");
    assert_eq!(data[..4], BALANCE_OF);
    assert_eq!(balance_of_call(&CLAIMER), data);

    let receipt = claim_receipt();
    assert_eq!(receipt.logs.len(), 1);
    assert_eq!(receipt.logs[0].topics[0], topic::CLAIMED);
    let sweep = Receipt::from_json(&read(LAUNCH, "a fee sweep")["result"]).expect("parses");
    assert_eq!(
        sweep
            .logs
            .iter()
            .filter(|l| l.topics[0] == topic::CREDITED)
            .count(),
        2
    );
}

#[test]
fn the_claim_reads_back_and_the_eth_arrived_to_the_wei() {
    let receipt = claim_receipt();
    assert_eq!(claimed(&receipt, &CLAIMER), Ok(AMOUNT));
    assert_eq!(
        Claimed::from_log(&receipt.logs[0]),
        Some(Claimed {
            recipient: CLAIMER,
            amount: AMOUNT
        })
    );

    // The escrow's record: all of it before, none after.
    let before = result_hex(CLAIM_CAPTURE, "escrow at block 62853729: balanceOf");
    let after = result_hex(CLAIM_CAPTURE, "escrow at block 62853730: balanceOf");
    assert_eq!(amount_from_return(&before), Some(AMOUNT));
    assert_eq!(amount_from_return(&after), Some(0));

    // The ETH: out of the escrow, into the claimer less what the claim cost.
    let raw = read(CLAIM_CAPTURE, "the claim receipt")["result"].clone();
    let quantity = |name: &str| {
        u128::from_str_radix(
            raw[name]
                .as_str()
                .expect("a quantity")
                .trim_start_matches("0x"),
            16,
        )
        .expect("fits")
    };
    let gas = quantity("gasUsed") * quantity("effectiveGasPrice");
    assert_eq!(
        wei("escrow's ETH at block 62853729") - wei("escrow's ETH at block 62853730"),
        AMOUNT
    );
    assert_eq!(
        wei("claimer's ETH at block 62853730") - wei("claimer's ETH at block 62853729"),
        AMOUNT - gas
    );
    // A wallet, not a contract that could have forwarded the ETH on.
    assert_eq!(read(CLAIM_CAPTURE, "claimer's code")["result"], "0x");
}

#[test]
fn a_sweep_credits_the_escrow_what_it_says_it_swept() {
    let receipt = Receipt::from_json(&read(LAUNCH, "a fee sweep")["result"]).expect("parses");
    let curve = Address::from_hex("0x36f815a2d12ad8016eb93065a0a5a23708da1aa2");
    let credits: Vec<Credited> = receipt.logs.iter().filter_map(Credited::from_log).collect();
    let swept = receipt
        .logs
        .iter()
        .find_map(Sweep::from_log)
        .expect("a sweep");
    assert_eq!(
        credits,
        [
            Credited {
                recipient: Address::from_hex("0x263ed295dafae1d9aadd6e56c4b6f9f38ee019dd"),
                source: curve,
                amount: swept.protocol,
            },
            Credited {
                recipient: Address::from_hex("0x3f927788d627d3561ff59bbfef67ebcbf0fb10a9"),
                source: curve,
                amount: swept.creator,
            },
        ]
    );
    // The same bytes from another contract are not the escrow's.
    let mut forged = receipt.logs[0].clone();
    forged.address = curve;
    assert_eq!(Credited::from_log(&forged), None);
    forged = receipt.logs[0].clone();
    forged.topics[0] = topic::CLAIMED;
    assert_eq!(Credited::from_log(&forged), None);
}

#[test]
fn each_way_a_transaction_is_not_the_claim_is_refused() {
    let base = claim_receipt();
    let other = Address::from_hex("0x1111111111111111111111111111111111111111");

    let mut r = base.clone();
    r.succeeded = false;
    assert_eq!(claimed(&r, &CLAIMER), Err(NotClaimed::Failed));

    r = base.clone();
    r.to = Some(other);
    assert_eq!(
        claimed(&r, &CLAIMER),
        Err(NotClaimed::NotToEscrow(Some(other)))
    );
    r.to = None;
    assert_eq!(claimed(&r, &CLAIMER), Err(NotClaimed::NotToEscrow(None)));

    assert_eq!(
        claimed(&base, &other),
        Err(NotClaimed::OtherSender(CLAIMER))
    );

    r = base.clone();
    r.logs.clear();
    assert_eq!(claimed(&r, &CLAIMER), Err(NotClaimed::Claims(0)));

    r = base.clone();
    r.logs.push(r.logs[0].clone());
    assert_eq!(claimed(&r, &CLAIMER), Err(NotClaimed::Claims(2)));

    // A claim event from an impostor contract does not count.
    r = base.clone();
    r.logs[0].address = other;
    assert_eq!(claimed(&r, &CLAIMER), Err(NotClaimed::Claims(0)));

    // Nor one whose amount does not read.
    r = base.clone();
    r.logs[0].data.truncate(31);
    assert_eq!(claimed(&r, &CLAIMER), Err(NotClaimed::Claims(0)));

    // The escrow paid someone else.
    r = base.clone();
    r.logs[0].topics[1].0[12..].copy_from_slice(&other.0);
    assert_eq!(
        claimed(&r, &CLAIMER),
        Err(NotClaimed::OtherRecipient(other))
    );
}

#[test]
fn an_amount_is_one_word_that_fits() {
    let mut word = [0u8; 32];
    word[16..].copy_from_slice(&u128::MAX.to_be_bytes());
    assert_eq!(amount_from_return(&word), Some(u128::MAX));
    assert_eq!(amount_from_return(&word[..31]), None);
    assert_eq!(amount_from_return(&[word, word].concat()), None);
    assert_eq!(amount_from_return(&[]), None);
    word[15] = 1;
    assert_eq!(
        amount_from_return(&word),
        None,
        "too big is refused, not truncated"
    );
}
