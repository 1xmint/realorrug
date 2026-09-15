// SPDX-License-Identifier: Apache-2.0
//! The Turnkey client against a loopback server standing in for Turnkey.
//!
//! What is checked is what Turnkey reads and what the payout does with the
//! answer: the exact path, the stamp header over the exact body, the body's
//! fields, a completed answer parsed, every other answer refused -- and a
//! signed transaction that is not the one asked for, or not from the wallet,
//! refused before anything could send it.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use k256::FieldBytes;
use k256::ecdsa::SigningKey;
use p256::ecdsa::Signature;
use p256::ecdsa::signature::Verifier as _;
use realorrug_payout::turnkey::{ApiKey, Turnkey, hex};
use realorrug_payout::tx::{Eip1559, Signed, address_of};
use realorrug_payout::{PayError, setup_proof, sign_checked};
use realorrug_robinhood::escrow::{ESCROW, claim_call};

/// A request as the server saw it.
#[derive(Debug, Clone)]
struct Seen {
    line: String,
    headers: HashMap<String, String>,
    body: String,
}

/// Serves each `(status, body)` in order, one connection each.
fn serve(answers: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let url = format!("http://{}", listener.local_addr().expect("an address"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for (status, body) in answers {
            let (stream, _) = listener.accept().expect("a connection");
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).expect("the request line");
            let mut headers = HashMap::new();
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).expect("a header line");
                if header == "\r\n" {
                    break;
                }
                if let Some((k, v)) = header.split_once(':') {
                    headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
                }
            }
            let length = headers
                .get("content-length")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let mut request = vec![0; length];
            reader.read_exact(&mut request).expect("the body");
            log.lock().expect("the log").push(Seen {
                line: line.trim().to_owned(),
                headers,
                body: String::from_utf8(request).expect("utf-8"),
            });
            let mut stream = reader.into_inner();
            write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("the response");
        }
    });
    (url, seen)
}

const API_SECRET: [u8; 32] = [0x11; 32];

/// The API key's P-256 signing key; the wallet's keys below are secp256k1.
fn api_signing_key() -> p256::ecdsa::SigningKey {
    p256::ecdsa::SigningKey::from_bytes(&p256::FieldBytes::from(API_SECRET)).expect("a key")
}

fn api_key() -> ApiKey {
    let public = hex(api_signing_key()
        .verifying_key()
        .to_sec1_point(true)
        .as_bytes());
    ApiKey::from_text(&format!("0x{}", hex(&API_SECRET)), &public).expect("loads")
}

fn wallet_key() -> SigningKey {
    SigningKey::from_bytes(&FieldBytes::from([5u8; 32])).expect("a key")
}

fn claim() -> Eip1559 {
    Eip1559 {
        chain_id: 4663,
        nonce: 1_000_000,
        max_priority_fee_per_gas: 0,
        max_fee_per_gas: 0x0c44_8890,
        gas_limit: 0xa661,
        to: ESCROW,
        value: 0,
        data: claim_call(0),
    }
}

fn signed_by(key: &SigningKey, tx: &Eip1559) -> Vec<u8> {
    let (signature, id) = key.sign_prehash_recoverable(&tx.signing_hash());
    let (r, s) = signature.split_bytes();
    Signed {
        tx: tx.clone(),
        y_parity: id.is_y_odd(),
        r: r.into(),
        s: s.into(),
    }
    .encode()
}

fn completed(raw: &[u8]) -> (u16, String) {
    (
        200,
        serde_json::json!({ "activity": {
            "id": "a1",
            "status": "ACTIVITY_STATUS_COMPLETED",
            "result": { "signTransactionResult": { "signedTransaction": hex(raw) } },
        } })
        .to_string(),
    )
}

fn client(url: String) -> Turnkey {
    Turnkey::new(
        url,
        "org-1",
        address_of(wallet_key().verifying_key()),
        api_key(),
    )
}

#[test]
fn a_signing_request_is_the_exact_body_stamped_and_posted_to_the_exact_path() {
    let tx = claim();
    let raw = signed_by(&wallet_key(), &tx);
    let (url, seen) = serve(vec![completed(&raw)]);
    let turnkey = client(url);
    assert_eq!(turnkey.sign_transaction(&tx), Ok(raw));

    let request = seen.lock().expect("the log")[0].clone();
    assert_eq!(
        request.line,
        "POST /public/v1/submit/sign_transaction HTTP/1.1"
    );
    let body: serde_json::Value = serde_json::from_str(&request.body).expect("json");
    assert_eq!(body["type"], "ACTIVITY_TYPE_SIGN_TRANSACTION_V2");
    assert_eq!(body["organizationId"], "org-1");
    assert!(
        body["timestampMs"]
            .as_str()
            .is_some_and(|t| t.parse::<u128>().is_ok_and(|ms| ms > 1_700_000_000_000)),
        "a string of milliseconds: {body}"
    );
    assert_eq!(
        body["parameters"],
        serde_json::json!({
            "signWith": address_of(wallet_key().verifying_key()).to_string(),
            "type": "TRANSACTION_TYPE_ETHEREUM",
            "unsignedTransaction": hex(&tx.unsigned()),
        })
    );
    assert!(
        !body["parameters"]["unsignedTransaction"]
            .as_str()
            .expect("hex")
            .starts_with("0x"),
        "hex without 0x, as Turnkey's viem adapter sends it"
    );

    // The stamp is over these exact bytes. Re-apply by stamping a
    // re-serialised body: key order or spacing differ and this fails.
    let stamp = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(request.headers.get("x-stamp").expect("an X-Stamp header"))
        .expect("base64url");
    let stamp: serde_json::Value = serde_json::from_slice(&stamp).expect("json");
    assert_eq!(stamp["publicKey"], api_key().public_hex());
    assert_eq!(stamp["scheme"], "SIGNATURE_SCHEME_TK_API_P256");
    let der =
        realorrug_robinhood::hex_bytes(&format!("0x{}", stamp["signature"].as_str().expect("hex")))
            .expect("hex");
    let signature = Signature::from_der(&der).expect("DER");
    api_signing_key()
        .verifying_key()
        .verify(request.body.as_bytes(), &signature)
        .expect("the stamp verifies over the body as sent");
    assert_eq!(
        request.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
}

#[test]
fn anything_but_a_completed_activity_is_a_refusal_in_turnkeys_words() {
    let failed = |status: &str| {
        (
            200,
            serde_json::json!({ "activity": { "status": status,
                "failure": { "code": 7, "message": "policy evaluation" } } })
            .to_string(),
        )
    };
    let (url, _) = serve(vec![
        failed("ACTIVITY_STATUS_CONSENSUS_NEEDED"),
        failed("ACTIVITY_STATUS_FAILED"),
        (
            403,
            r#"{"code":7,"message":"PERMISSION_DENIED: policy denied"}"#.to_owned(),
        ),
        (200, "not json".to_owned()),
    ]);
    let turnkey = client(url);
    let wallet = address_of(wallet_key().verifying_key());
    for expected in [
        "ACTIVITY_STATUS_CONSENSUS_NEEDED",
        "ACTIVITY_STATUS_FAILED",
        "HTTP 403",
        "not json",
    ] {
        match sign_checked(&turnkey, &claim(), &wallet) {
            Err(PayError::SignerRefused(why)) => assert!(why.contains(expected), "{why}"),
            other => panic!("{expected}: {other:?}"),
        }
    }
}

#[test]
fn a_signed_answer_that_is_not_the_request_or_not_the_wallets_is_refused() {
    // Turnkey is trusted to hold the key, not to sign the right thing.
    // Re-apply by returning Turnkey's bytes unchecked: both of these pass.
    let tx = claim();
    let mut other_fields = tx.clone();
    other_fields.data = claim_call(1);
    let stranger = SigningKey::from_bytes(&FieldBytes::from([6u8; 32])).expect("a key");
    let (url, _) = serve(vec![
        completed(&signed_by(&wallet_key(), &other_fields)),
        completed(&signed_by(&stranger, &tx)),
    ]);
    let turnkey = client(url);
    let wallet = address_of(wallet_key().verifying_key());
    assert!(matches!(
        sign_checked(&turnkey, &tx, &wallet),
        Err(PayError::BadSignature(why)) if why.contains("asked for")
    ));
    assert!(matches!(
        sign_checked(&turnkey, &tx, &wallet),
        Err(PayError::BadSignature(why)) if why.contains("not the wallet")
    ));
}

/// The setup proof's plain transfer: 1 wei from the wallet to itself, no call
/// data, at the proof's nonce.
fn proof_transfer() -> Eip1559 {
    Eip1559 {
        gas_limit: 21_000,
        to: address_of(wallet_key().verifying_key()),
        value: 1,
        data: Vec::new(),
        ..claim()
    }
}

#[test]
fn the_setup_proof_holds_only_when_whoami_answers_the_factory_is_denied_and_the_claim_and_transfer_are_signed()
 {
    let wallet = address_of(wallet_key().verifying_key());
    let whoami = (
        200,
        r#"{"organizationId":"org-1","userId":"u1","username":"realorrug-payout"}"#.to_owned(),
    );
    let denied = (403, r#"{"message":"policy denied"}"#.to_owned());
    let (url, seen) = serve(vec![
        whoami.clone(),
        denied.clone(),
        completed(&signed_by(&wallet_key(), &claim())),
        completed(&signed_by(&wallet_key(), &proof_transfer())),
    ]);
    let lines = setup_proof(&client(url), &wallet).expect("the proof holds");
    assert_eq!(lines.len(), 4);
    assert!(lines[0].contains("realorrug-payout"), "{lines:?}");
    assert!(lines[2].contains("not sent"), "{lines:?}");
    assert!(lines[3].contains("1 wei transfer"), "{lines:?}");
    let log = seen.lock().expect("the log");
    assert_eq!(log[0].line, "POST /public/v1/query/whoami HTTP/1.1");
    let second: serde_json::Value = serde_json::from_str(&log[1].body).expect("json");
    assert!(
        second["parameters"]["unsignedTransaction"]
            .as_str()
            .is_some_and(|u| u.contains("deadbeef")),
        "the denied request carries call data to another contract"
    );
    let fourth: serde_json::Value = serde_json::from_str(&log[3].body).expect("json");
    assert_eq!(
        fourth["parameters"]["unsignedTransaction"],
        hex(&proof_transfer().unsigned()),
        "the transfer asked for is 1 wei to the wallet with no call data"
    );
    drop(log);

    // A policy that signs the factory call fails the proof. Re-apply by
    // counting step 2 as passed whatever the answer: this returns Ok.
    let (url, _) = serve(vec![
        whoami.clone(),
        completed(&signed_by(&wallet_key(), &claim())),
        completed(&signed_by(&wallet_key(), &claim())),
        completed(&signed_by(&wallet_key(), &proof_transfer())),
    ]);
    let lines = setup_proof(&client(url), &wallet).expect_err("too wide");
    assert!(lines[1].contains("SIGNED"), "{lines:?}");

    // A policy that denies the plain transfer fails the proof, though the
    // claim was signed. Re-apply by ignoring step 4's answer: this returns Ok.
    let (url, _) = serve(vec![
        whoami,
        denied,
        completed(&signed_by(&wallet_key(), &claim())),
        (403, r#"{"message":"policy denied"}"#.to_owned()),
    ]);
    let lines = setup_proof(&client(url), &wallet).expect_err("the transfer is denied");
    assert_eq!(lines.len(), 4);
    assert!(lines[3].contains("not signed"), "{lines:?}");
}
