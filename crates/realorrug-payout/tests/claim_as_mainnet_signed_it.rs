// SPDX-License-Identifier: Apache-2.0
//! The encoder against a transaction mainnet accepted.
//!
//! `docs/research/data/0036-escrow-claim.json` holds the escrow claim
//! `0x07cab768…` as `eth_getTransactionByHash` returned it: every signed field,
//! the signature, the hash and the sender. Rebuilding it from those fields and
//! getting the same hash proves the encoding to the byte, with no key and
//! nothing sent; recovering the sender proves the signing hash and the
//! recovery. A reference proposes, a capture disposes (AGENTS.md §1).
//!
//! Each of these breaks it, and was the reason for writing it this way: drop
//! the empty access list, write an integer with a leading zero byte (nonce 3 as
//! `0x82 0x00 0x03`), or put `max_priority_fee_per_gas` after
//! `max_fee_per_gas`. The hash is Keccak of every byte, so any of them changes
//! it.

use realorrug_payout::tx::{Eip1559, Signed, decode_signed, recover};
use realorrug_robinhood::escrow::ESCROW;
use realorrug_robinhood::{Address, Hash32, hex_bytes, quantity};

const CLAIM: &str = include_str!("../../../docs/research/data/0036-escrow-claim.json");

fn captured() -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_str(CLAIM).expect("JSON");
    let read = value["reads"][1].clone();
    assert_eq!(read["method"], "eth_getTransactionByHash");
    read["result"].clone()
}

fn text<'a>(v: &'a serde_json::Value, name: &str) -> &'a str {
    v[name].as_str().unwrap_or_else(|| panic!("{name}"))
}

fn word(v: &serde_json::Value, name: &str) -> [u8; 32] {
    let bytes = hex_bytes(text(v, name)).expect("hex");
    let mut out = [0u8; 32];
    out[32 - bytes.len()..].copy_from_slice(&bytes);
    out
}

/// The captured claim, from its JSON fields alone.
fn rebuilt() -> Signed {
    let tx = captured();
    assert_eq!(text(&tx, "type"), "0x2");
    assert_eq!(tx["accessList"], serde_json::json!([]));
    Signed {
        tx: Eip1559 {
            chain_id: quantity(text(&tx, "chainId")).expect("chain id"),
            nonce: quantity(text(&tx, "nonce")).expect("nonce"),
            max_priority_fee_per_gas: quantity(text(&tx, "maxPriorityFeePerGas"))
                .expect("tip")
                .into(),
            max_fee_per_gas: quantity(text(&tx, "maxFeePerGas")).expect("fee").into(),
            gas_limit: quantity(text(&tx, "gas")).expect("gas"),
            to: text(&tx, "to").parse().expect("to"),
            value: quantity(text(&tx, "value")).expect("value").into(),
            data: hex_bytes(text(&tx, "input")).expect("input"),
        },
        y_parity: match text(&tx, "yParity") {
            "0x0" => false,
            "0x1" => true,
            other => panic!("y parity {other}"),
        },
        r: word(&tx, "r"),
        s: word(&tx, "s"),
    }
}

#[test]
fn the_captured_claim_rebuilt_from_its_fields_hashes_as_mainnet_hashed_it() {
    let signed = rebuilt();
    // The fields are the ones the payout builds: chain 4663, no tip, the
    // escrow, no value, and a claim's call data.
    assert_eq!(signed.tx.chain_id, 4663);
    assert_eq!(signed.tx.nonce, 3);
    assert_eq!(signed.tx.max_priority_fee_per_gas, 0);
    assert_eq!(signed.tx.to, ESCROW);
    assert_eq!(signed.tx.value, 0);
    assert_eq!(
        signed.tx.data,
        realorrug_robinhood::escrow::claim_call(4_014_961_601_594_189_201)
    );

    let mainnet: Hash32 = text(&captured(), "hash").parse().expect("hash");
    assert_eq!(signed.hash(), mainnet);
    assert_eq!(
        mainnet.to_string(),
        "0x07cab768bdf8dcf67edc9b0bc74d9d2d70cdd4f88b4cbe8ada2b6ec85f44fa7b"
    );
}

#[test]
fn the_captured_claims_signature_recovers_to_the_account_mainnet_says_sent_it() {
    let from: Address = text(&captured(), "from").parse().expect("from");
    assert_eq!(
        from.to_string(),
        "0x6aa025a3292c4ab6a55af3b6a7f7cbf62a5c4d06"
    );
    assert_eq!(recover(&rebuilt()), Ok(from));
}

#[test]
fn the_captured_claims_bytes_decode_back_to_its_fields() {
    // What checks Turnkey's answer: signed bytes in, the fields out, compared.
    let signed = rebuilt();
    let raw = signed.encode();
    assert_eq!(raw[0], 0x02);
    assert_eq!(decode_signed(&raw), Ok(signed.clone()));

    // One changed byte in the call data decodes to different fields, so the
    // comparison would refuse it; and the sender it recovers is not the
    // claimer, so the recovery would too.
    let mut tampered = raw.clone();
    let last = tampered.len() - 70;
    tampered[last] ^= 1;
    let decoded = decode_signed(&tampered).expect("still well-formed");
    assert_ne!(decoded.tx, signed.tx);
    assert_ne!(recover(&decoded), recover(&signed));
}
