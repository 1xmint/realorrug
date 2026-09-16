// SPDX-License-Identifier: Apache-2.0
//! The JSON-RPC client, against a local server that answers with mainnet's
//! captured responses.
//!
//! The client is the one piece of this crate that talks to the network, which
//! is also why it is the easiest to leave untested. A loopback server replays
//! what the chain said, and records what the client asked, so both directions
//! are checked without the network.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use realorrug_robinhood::escrow::{ESCROW, claimable};
use realorrug_robinhood::pons::{FACTORY, LaunchedToken};
use realorrug_robinhood::{Address, Hash32, Receipt, Rpc, Tag, Transaction};

const CLEAN: &str = include_str!("../../../docs/research/data/0036-pons-v2-clean-launch.json");

/// Serves each body in order, one connection each, and records the requests.
fn serve(bodies: Vec<String>) -> (String, Arc<Mutex<Vec<serde_json::Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let url = format!("http://{}", listener.local_addr().expect("an address"));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    std::thread::spawn(move || {
        for body in bodies {
            let (stream, _) = listener.accept().expect("a connection");
            let mut reader = BufReader::new(stream);
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("a header line");
                if line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().expect("a length");
                }
            }
            let mut request = vec![0; length];
            reader.read_exact(&mut request).expect("the body");
            log.lock()
                .expect("the log")
                .push(serde_json::from_slice(&request).expect("the request is JSON"));
            let mut stream = reader.into_inner();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("the response");
        }
    });
    (url, seen)
}

fn capture(index: usize) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_str(CLEAN).expect("JSON");
    value["reads"][index].clone()
}

fn answer(result: &serde_json::Value) -> String {
    serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": result }).to_string()
}

fn tx() -> Hash32 {
    "0x2a43738c97c45c2f1e9a4b4941a8e37ba543b5b1f809e648e294cab50efa59ce"
        .parse()
        .expect("a hash")
}

#[test]
fn a_receipt_is_asked_for_by_hash_and_parsed() {
    let receipt = capture(1)["result"].clone();
    let (url, seen) = serve(vec![answer(&receipt)]);
    let got = Rpc::new(url).receipt(&tx()).expect("an answer");
    assert_eq!(got, Some(Receipt::from_json(&receipt).expect("parses")));
    let request = seen.lock().expect("the log")[0].clone();
    assert_eq!(request["method"], "eth_getTransactionReceipt");
    assert_eq!(request["params"], serde_json::json!([tx().to_string()]));
    assert_eq!(request["jsonrpc"], "2.0");
}

#[test]
fn a_receipt_not_yet_in_a_block_is_none() {
    let (url, _) = serve(vec![answer(&serde_json::Value::Null)]);
    assert_eq!(Rpc::new(url).receipt(&tx()), Ok(None));
}

#[test]
fn a_malformed_receipt_is_an_error_not_a_guess() {
    let (url, _) = serve(vec![answer(&serde_json::json!({ "status": "0x1" }))]);
    let err = Rpc::new(url).receipt(&tx()).expect_err("refused");
    assert!(err.starts_with("receipt: "), "{err}");
}

#[test]
fn the_endpoints_error_is_passed_on_with_the_method() {
    let (url, _) = serve(vec![
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"metadata is not found"}}"#
            .to_owned(),
    ]);
    let err = Rpc::new(url).receipt(&tx()).expect_err("refused");
    assert!(err.starts_with("eth_getTransactionReceipt: "), "{err}");
    assert!(err.contains("metadata is not found"), "{err}");
}

#[test]
fn an_answer_without_a_result_or_json_is_an_error() {
    let (url, _) = serve(vec![
        r#"{"jsonrpc":"2.0","id":1}"#.to_owned(),
        "not json".to_owned(),
    ]);
    let rpc = Rpc::new(url);
    assert_eq!(
        rpc.receipt(&tx()),
        Err("eth_getTransactionReceipt returned no result".to_owned())
    );
    let err = rpc.receipt(&tx()).expect_err("refused");
    assert!(err.starts_with("not json"), "{err}");
}

#[test]
fn nobody_listening_is_an_error() {
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("a port")
        .local_addr()
        .expect("an address")
        .port();
    assert!(
        Rpc::new(format!("http://127.0.0.1:{port}"))
            .receipt(&tx())
            .is_err()
    );
}

#[test]
fn a_contract_call_sends_the_call_data_at_latest_and_returns_the_bytes() {
    let read = capture(2);
    let (url, seen) = serve(vec![answer(&read["result"])]);
    let token: Address = "0xb67f538fa1b65823aab5fdb4e923628f5c65943e"
        .parse()
        .expect("an address");
    let data = LaunchedToken::call_data(&token);
    let got = Rpc::new(url).call_contract(&FACTORY, &data).expect("bytes");
    let record = LaunchedToken::from_return(&got).expect("the record");
    assert_eq!(record.token, token);
    assert_eq!(record.creator_tax_bps, 200);
    let request = seen.lock().expect("the log")[0].clone();
    assert_eq!(request["method"], "eth_call");
    assert_eq!(
        request["params"],
        serde_json::json!([{ "to": FACTORY.to_string(), "data": read["params"][0]["data"] }, "latest"])
    );
}

#[test]
fn a_contract_call_answering_anything_but_hex_is_an_error() {
    let (url, _) = serve(vec![
        answer(&serde_json::json!(7)),
        answer(&serde_json::json!("0xabc")),
    ]);
    let rpc = Rpc::new(url);
    assert_eq!(
        rpc.call_contract(&FACTORY, &[]),
        Err("eth_call returned no hex".to_owned())
    );
    assert_eq!(
        rpc.call_contract(&FACTORY, &[]),
        Err("not hex: 0xabc".to_owned())
    );
}

#[test]
fn the_claimable_balance_asks_the_escrow_and_reads_one_amount() {
    let claim: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/research/data/0036-escrow-claim.json"
    ))
    .expect("JSON");
    let before = claim["reads"][3].clone();
    let claimer: Address = "0x6aa025a3292c4ab6a55af3b6a7f7cbf62a5c4d06"
        .parse()
        .expect("an address");
    let (url, seen) = serve(vec![
        answer(&before["result"]),
        answer(&serde_json::json!("0x00")),
    ]);
    let rpc = Rpc::new(url);
    assert_eq!(claimable(&rpc, &claimer), Ok(4_014_961_601_594_189_201));
    let request = seen.lock().expect("the log")[0].clone();
    assert_eq!(
        request["params"],
        serde_json::json!([{ "to": ESCROW.to_string(), "data": before["params"][0]["data"] }, "latest"])
    );
    assert_eq!(
        claimable(&rpc, &claimer),
        Err("balanceOf returned 1 bytes".to_owned())
    );
}

fn escrow_claim(index: usize) -> serde_json::Value {
    let claim: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/research/data/0036-escrow-claim.json"
    ))
    .expect("JSON");
    claim["reads"][index].clone()
}

fn claimer() -> Address {
    "0x6aa025a3292c4ab6a55af3b6a7f7cbf62a5c4d06"
        .parse()
        .expect("an address")
}

#[test]
fn the_chain_id_balance_and_code_are_read_as_mainnet_answered_them() {
    let (url, seen) = serve(vec![
        answer(&escrow_claim(0)["result"]),
        answer(&escrow_claim(4)["result"]),
        answer(&escrow_claim(9)["result"]),
        answer(&serde_json::json!("0xef0100aa")),
    ]);
    let rpc = Rpc::new(url);
    assert_eq!(rpc.chain_id(), Ok(4663));
    assert_eq!(rpc.balance(&claimer()), Ok(0x0dbd_5a06_13bf_a700));
    assert_eq!(rpc.code(&claimer()), Ok(Vec::new()));
    assert_eq!(rpc.code(&claimer()), Ok(vec![0xef, 0x01, 0x00, 0xaa]));
    let log = seen.lock().expect("the log");
    assert_eq!(log[0]["method"], "eth_chainId");
    assert_eq!(log[1]["method"], "eth_getBalance");
    assert_eq!(
        log[1]["params"],
        serde_json::json!([claimer().to_string(), "latest"])
    );
    assert_eq!(log[2]["method"], "eth_getCode");
    assert_eq!(
        log[2]["params"],
        serde_json::json!([claimer().to_string(), "latest"])
    );
}

#[test]
fn the_block_number_is_read_as_a_quantity() {
    let (url, seen) = serve(vec![
        answer(&serde_json::json!("0x3b7827e")),
        answer(&serde_json::json!(7)),
    ]);
    let rpc = Rpc::new(url);
    assert_eq!(rpc.block_number(), Ok(62_358_142));
    assert!(rpc.block_number().is_err(), "a number is not a quantity");
    let log = seen.lock().expect("the log");
    assert_eq!(log[0]["method"], "eth_blockNumber");
    assert_eq!(log[0]["params"], serde_json::json!([]));
}

#[test]
fn the_nonce_is_asked_at_the_named_tag() {
    // Pending for a new transaction, so one the node holds is not reused;
    // latest for "has this nonce landed". Re-apply by sending "latest" for
    // both: the second params assertion fails.
    let (url, seen) = serve(vec![
        answer(&serde_json::json!("0x3")),
        answer(&serde_json::json!("0x4")),
        answer(&serde_json::json!(3)),
    ]);
    let rpc = Rpc::new(url);
    assert_eq!(rpc.nonce(&claimer(), Tag::Latest), Ok(3));
    assert_eq!(rpc.nonce(&claimer(), Tag::Pending), Ok(4));
    assert!(
        rpc.nonce(&claimer(), Tag::Latest).is_err(),
        "a number is not a quantity"
    );
    let log = seen.lock().expect("the log");
    assert_eq!(log[0]["method"], "eth_getTransactionCount");
    assert_eq!(
        log[0]["params"],
        serde_json::json!([claimer().to_string(), "latest"])
    );
    assert_eq!(
        log[1]["params"],
        serde_json::json!([claimer().to_string(), "pending"])
    );
}

#[test]
fn gas_is_estimated_for_the_exact_call_and_the_base_fee_read_from_the_latest_block() {
    let (url, seen) = serve(vec![
        answer(&serde_json::json!("0x7fa7")),
        answer(&serde_json::json!({ "number": "0x1", "baseFeePerGas": "0x4255720" })),
        answer(&serde_json::json!({ "number": "0x1" })),
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":3,"message":"execution reverted"}}"#.to_owned(),
    ]);
    let rpc = Rpc::new(url);
    let data = realorrug_robinhood::escrow::claim_call(5);
    assert_eq!(
        rpc.estimate_gas(&claimer(), &ESCROW, 0x1_0000_0000_0000_0000, &data),
        Ok(0x7fa7)
    );
    assert_eq!(rpc.base_fee(), Ok(0x0425_5720));
    assert_eq!(
        rpc.base_fee(),
        Err("eth_getBlockByNumber: no baseFeePerGas".to_owned())
    );
    let reverted = rpc.estimate_gas(&claimer(), &ESCROW, 0, &data);
    assert!(
        reverted
            .as_ref()
            .is_err_and(|e| e.contains("execution reverted")),
        "{reverted:?}"
    );
    let log = seen.lock().expect("the log");
    assert_eq!(log[0]["method"], "eth_estimateGas");
    assert_eq!(
        log[0]["params"],
        serde_json::json!([{
            "from": claimer().to_string(),
            "to": ESCROW.to_string(),
            "value": "0x10000000000000000",
            "data": realorrug_robinhood::to_hex(&data),
        }])
    );
    assert_eq!(log[1]["method"], "eth_getBlockByNumber");
    assert_eq!(log[1]["params"], serde_json::json!(["latest", false]));
}

#[test]
fn a_transaction_is_read_by_hash_with_its_value_and_input() {
    let captured = escrow_claim(1)["result"].clone();
    let mut pending = captured.clone();
    pending["blockNumber"] = serde_json::Value::Null;
    let (url, seen) = serve(vec![
        answer(&captured),
        answer(&pending),
        answer(&serde_json::Value::Null),
    ]);
    let rpc = Rpc::new(url);
    let hash: Hash32 = "0x07cab768bdf8dcf67edc9b0bc74d9d2d70cdd4f88b4cbe8ada2b6ec85f44fa7b"
        .parse()
        .expect("a hash");
    let got = rpc.transaction(&hash).expect("read").expect("known");
    assert_eq!(
        got,
        Transaction {
            hash,
            from: claimer(),
            to: Some(ESCROW),
            value: 0,
            input: realorrug_robinhood::escrow::claim_call(4_014_961_601_594_189_201),
            nonce: 3,
            block: Some(0x03bf_1262),
        }
    );
    assert_eq!(
        rpc.transaction(&hash).expect("read").expect("known").block,
        None
    );
    assert_eq!(rpc.transaction(&hash), Ok(None));
    let log = seen.lock().expect("the log");
    assert_eq!(log[0]["method"], "eth_getTransactionByHash");
    assert_eq!(log[0]["params"], serde_json::json!([hash.to_string()]));
}
