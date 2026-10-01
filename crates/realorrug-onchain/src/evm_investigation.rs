// SPDX-License-Identifier: Apache-2.0
//! Mainnet EVM observations. Every RPC operation spends the shared read budget.
use crate::{
    Budget,
    cases::{CaseKey, Observation, TimeWindow},
    investigation::{Read, Tool, observed, statement},
};
use realorrug_robinhood::Rpc;
use serde_json::{Value, json};

///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
#[allow(
    clippy::needless_pass_by_value,
    reason = "callers pass short lived json! values and the transport borrows them"
)]
pub(crate) fn call(
    rpc: &Rpc,
    budget: &mut Budget,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    budget
        .take_call()
        .map_err(|e| format!("read budget exhausted: {e:?}"))?;
    // Existing instrument units are a conservative work allowance, not a bill.
    budget
        .take_cu(if method == "eth_getLogs" { 75 } else { 20 })
        .map_err(|e| format!("compute allowance exhausted: {e:?}"))?;
    let timeout = budget.time_left().min(std::time::Duration::from_secs(10));
    if timeout.is_zero() {
        return Err("active read deadline exhausted".into());
    }
    rpc.call_bounded(method, &params, timeout)
}

/// Validate the endpoint before reading any token on a configured EVM network.
///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub fn network(rpc: &Rpc, case: &CaseKey, budget: &mut Budget) -> Result<(), String> {
    let id = call(rpc, budget, "eth_chainId", json!([]))?;
    let expected = case.chain.evm_id().ok_or("no expected EVM network id")?;
    if hex_u64(id.as_str().ok_or("chain id missing")?)? != expected {
        return Err("RPC endpoint network does not match the requested chain".into());
    }
    Ok(())
}

///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub(crate) fn read(
    rpc: &Rpc,
    case: &CaseKey,
    read: &Read,
    _window: Option<&TimeWindow>,
    budget: &mut Budget,
    at: u64,
) -> Result<Observation, String> {
    network(rpc, case, budget)?;
    let header = call(
        rpc,
        budget,
        "eth_getBlockByNumber",
        json!(["latest", false]),
    )?;
    let block = header["number"].as_str().ok_or("block number missing")?;
    let hash = header["hash"].as_str().ok_or("block hash missing")?;
    hash.parse::<realorrug_robinhood::Hash32>()
        .map_err(|e| e.to_string())?;
    let point = Some(format!("{} block {} {}", case.chain, hex_u64(block)?, hash));
    let (value, gap) = match read.tool {
        Tool::Token | Tool::Account => token(rpc, read, block, budget)?,
        Tool::Transaction => transaction(rpc, case, read, budget)?,
        Tool::History => history(rpc, case, read, block, budget)?,
        Tool::Source => source(rpc, case, read, block, budget)?,
        Tool::Liquidity => liquidity(rpc, case, block, budget)?,
        Tool::Fees => crate::evm_protocols::fees(rpc, case, block, budget)?,
    };
    let current = call(rpc, budget, "eth_getBlockByNumber", json!([block, false]))?;
    if current["hash"].as_str() != Some(hash) {
        return Err("observed block changed during the read; evidence must be read again".into());
    }
    Ok(observed(case, read, at, point, value, gap))
}

/// Strict hex quantity decoder; malformed data never becomes zero.
///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub fn hex_u64(s: &str) -> Result<u64, String> {
    u64::from_str_radix(s.strip_prefix("0x").ok_or("quantity lacks prefix")?, 16)
        .map_err(|e| e.to_string())
}

///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub(crate) fn word(s: &str) -> Result<u128, String> {
    let digits = s.strip_prefix("0x").ok_or("word lacks hex prefix")?;
    if digits.len() != 64 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("malformed ABI word".into());
    }
    if digits[..32].bytes().any(|b| b != b'0') {
        return Err("ABI integer exceeds the supported exact range".into());
    }
    u128::from_str_radix(&digits[32..], 16).map_err(|e| e.to_string())
}

///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub(crate) fn address_word(s: &str) -> Result<String, String> {
    let digits = s.strip_prefix("0x").ok_or("address word lacks prefix")?;
    if digits.len() != 64
        || !digits.bytes().all(|b| b.is_ascii_hexdigit())
        || digits[..24].bytes().any(|b| b != b'0')
    {
        return Err("not an address word".into());
    }
    format!("0x{}", &digits[24..])
        .parse::<realorrug_robinhood::Address>()
        .map(|a| a.to_string())
        .map_err(|e| e.to_string())
}

///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub(crate) fn eth_call(
    rpc: &Rpc,
    budget: &mut Budget,
    address: &str,
    data: &str,
    block: &str,
) -> Result<String, String> {
    let value = call(
        rpc,
        budget,
        "eth_call",
        json!([{"to":address,"data":data},block]),
    )?;
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "eth_call returned no hex bytes".into())
}

fn token(
    rpc: &Rpc,
    read: &Read,
    block: &str,
    budget: &mut Budget,
) -> Result<(Value, Option<String>), String> {
    let code = call(rpc, budget, "eth_getCode", json!([read.subject, block]))?;
    let code = code.as_str().ok_or("bytecode absent")?;
    if code == "0x" {
        return Err("address has no deployed contract code at this block".into());
    }
    if code.len() > 100_000 {
        return Err("contract code exceeds read bound".into());
    }
    let mut facts = vec![statement(format!(
        "Contract code was observed at {}.",
        read.subject
    ))];
    if read.tool == Tool::Token {
        let supply = word(&eth_call(rpc, budget, &read.subject, "0x18160ddd", block)?)?;
        facts.push(statement(format!(
            "totalSupply() returned {supply} base units."
        )));
        let decimals = word(&eth_call(rpc, budget, &read.subject, "0x313ce567", block)?)?;
        if decimals > 36 {
            return Err("unsupported decimals response".into());
        }
        facts.push(statement(format!("decimals() returned {decimals}.")));
    }
    let mut related = Vec::new();
    // Read-only getter response is not proof of complete ownership semantics.
    if let Ok(owner) =
        eth_call(rpc, budget, &read.subject, "0x8da5cb5b", block).and_then(|s| address_word(&s))
    {
        facts.push(statement(format!(
            "owner() returned {owner}; this getter alone does not establish all powers."
        )));
        related.push(owner);
    }
    for (label, slot) in [
        (
            "ERC-1967 implementation",
            "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc",
        ),
        (
            "ERC-1967 admin",
            "0xb53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103",
        ),
        (
            "ERC-1967 beacon",
            "0xa3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50",
        ),
    ] {
        let storage = call(
            rpc,
            budget,
            "eth_getStorageAt",
            json!([read.subject, slot, block]),
        )?;
        let address = address_word(storage.as_str().ok_or("proxy slot unavailable")?)?;
        if address != "0x0000000000000000000000000000000000000000" {
            facts.push(statement(format!("{label} slot contains {address}.")));
            related.push(address);
        }
    }
    Ok((json!({"statements":facts,"related":related,"code_hash":blake3::hash(code.as_bytes()).to_hex().to_string()}),
        Some("getters and standard slots do not prove absence of mint, pause, blacklist or nonstandard upgrade powers; verify implementation semantics".into())))
}

fn transaction(
    rpc: &Rpc,
    case: &CaseKey,
    read: &Read,
    budget: &mut Budget,
) -> Result<(Value, Option<String>), String> {
    let tx = call(
        rpc,
        budget,
        "eth_getTransactionByHash",
        json!([read.subject]),
    )?;
    let receipt = call(
        rpc,
        budget,
        "eth_getTransactionReceipt",
        json!([read.subject]),
    )?;
    if tx.is_null() || receipt.is_null() {
        return Err("transaction is not confirmed or not available".into());
    }
    if tx["hash"].as_str() != Some(&read.subject)
        || receipt["transactionHash"].as_str() != Some(&read.subject)
        || tx["blockHash"] != receipt["blockHash"]
        || tx["blockNumber"] != receipt["blockNumber"]
    {
        return Err("transaction and receipt identities/checkpoints disagree".into());
    }
    let block = receipt["blockNumber"]
        .as_str()
        .ok_or("transaction block missing")?;
    let canonical = call(rpc, budget, "eth_getBlockByNumber", json!([block, false]))?;
    if canonical["hash"] != receipt["blockHash"] || canonical["hash"].is_null() {
        return Err("receipt block is no longer canonical".into());
    }
    let status = receipt["status"].as_str().ok_or("receipt outcome absent")?;
    if hex_u64(status)? != 1 {
        return Ok((
            json!({"statements":[statement("The transaction failed; attempted payments are not executed transfers.")],"related":[]}),
            None,
        ));
    }
    let from = tx["from"].as_str().ok_or("transaction sender absent")?;
    let to = tx["to"].as_str();
    crate::cases::CaseKey::new(case.chain, from).map_err(|e| e.to_string())?;
    if let Some(to) = to {
        crate::cases::CaseKey::new(case.chain, to).map_err(|e| e.to_string())?;
    }
    let amount = tx["value"].as_str().ok_or("transaction value absent")?;
    word(&format!(
        "0x{:0>64}",
        amount
            .strip_prefix("0x")
            .ok_or("native value has no hex prefix")?
    ))?;
    let mut facts = vec![statement(format!(
        "Successful transaction {} from {} to {} has native value {} (hex wei).",
        read.subject,
        from,
        to.unwrap_or("contract creation"),
        amount
    ))];
    let mut related = vec![from.to_owned()];
    if let Some(to) = to {
        related.push(to.to_owned());
    }
    let mut transfers = Vec::new();
    if let Some(logs) = receipt["logs"].as_array() {
        for log in logs.iter().take(128) {
            if log["address"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case(&case.address))
                && let Ok(transfer) = transfer(log)
            {
                facts.push(statement(format!(
                    "Token Transfer event reports {} base units from {} to {}.",
                    transfer["amount"].as_str().unwrap_or(""),
                    transfer["from"].as_str().unwrap_or(""),
                    transfer["to"].as_str().unwrap_or("")
                )));
                related.push(transfer["to"].as_str().unwrap_or("").to_owned());
                transfers.push(transfer);
            }
        }
    }
    Ok((json!({"statements":facts,"related":related,"transfers":transfers,
        "transaction_block":receipt["blockNumber"],"transaction_block_hash":receipt["blockHash"],
        "receipt_logs_present":receipt["logs"].is_array(),"receipt_logs_truncated":receipt["logs"].as_array().is_some_and(|logs|logs.len()>128)}),
        Some("only the first 128 receipt logs are considered; unavailable, removed or malformed logs do not establish token payments; internal native transfers, custom token semantics and beneficiary identity require separate verification".into())))
}

const TRANSFER: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
fn transfer(log: &Value) -> Result<Value, String> {
    if log["removed"].as_bool() == Some(true) {
        return Err("removed Transfer event".into());
    }
    let topics = log["topics"].as_array().ok_or("log topics missing")?;
    if topics.len() != 3 || topics[0].as_str() != Some(TRANSFER) {
        return Err("not an ERC-20 Transfer event".into());
    }
    let from = address_word(topics[1].as_str().ok_or("from topic missing")?)?;
    let to = address_word(topics[2].as_str().ok_or("to topic missing")?)?;
    let amount = word(log["data"].as_str().ok_or("amount word missing")?)?;
    Ok(
        json!({"from":from,"to":to,"amount":amount.to_string(),"transaction":log["transactionHash"]}),
    )
}

fn history(
    rpc: &Rpc,
    case: &CaseKey,
    read: &Read,
    block: &str,
    budget: &mut Budget,
) -> Result<(Value, Option<String>), String> {
    let end = hex_u64(block)?;
    let start = end.saturating_sub(2000);
    let address = read.subject.trim_start_matches("0x");
    let topic = format!("0x{address:0>64}");
    let mut transfers = Vec::new();
    let mut truncated = false;
    for topics in [json!([TRANSFER, topic]), json!([TRANSFER, null, topic])] {
        let logs = call(
            rpc,
            budget,
            "eth_getLogs",
            json!([{"address":case.address,"fromBlock":format!("0x{start:x}"),
            "toBlock":block,"topics":topics}]),
        )?;
        let logs = logs.as_array().ok_or("logs missing")?;
        truncated |= logs.len() > 64;
        for log in logs.iter().take(64) {
            if log["address"]
                .as_str()
                .is_some_and(|address| address.eq_ignore_ascii_case(&case.address))
                && let Ok(value) = transfer(log)
            {
                transfers.push(value);
            }
        }
    }
    let refs: Vec<String> = transfers
        .iter()
        .filter_map(|t| t["transaction"].as_str().map(str::to_owned))
        .collect();
    Ok((json!({"statements":[statement(format!("A bounded token-transfer log read covered blocks {start} through {end}; it is not full wallet history."))],
        "transfers":transfers,"signatures":refs,"related":[],"truncated":truncated}),
        Some("at most 64 events per direction are retained; removed/malformed events are excluded; only this token's recent Transfer logs are covered; requested historical time coverage, native funding and internal calls are unresolved".into())))
}

fn source(
    rpc: &Rpc,
    case: &CaseKey,
    read: &Read,
    block: &str,
    budget: &mut Budget,
) -> Result<(Value, Option<String>), String> {
    let code = call(rpc, budget, "eth_getCode", json!([read.subject, block]))?;
    budget
        .take_call()
        .map_err(|e| format!("source budget exhausted: {e:?}"))?;
    let url = format!(
        "https://sourcify.dev/server/v2/contract/{}/{}?fields=abi,runtimeBytecode.onchainBytecode",
        case.chain.evm_id().ok_or("EVM chain absent")?,
        read.subject
    );
    let mut response = ureq::get(&url)
        .config()
        .timeout_global(Some(
            budget.time_left().min(std::time::Duration::from_secs(10)),
        ))
        .build()
        .call()
        .map_err(|e| e.to_string())?;
    let text = response
        .body_mut()
        .with_config()
        .limit(262_144)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let deployed = value["runtimeBytecode"]["onchainBytecode"]
        .as_str()
        .ok_or("verified deployed bytecode absent")?;
    if value["abi"].as_array().is_none_or(Vec::is_empty) {
        return Err("verified ABI absent".into());
    }
    if !code
        .as_str()
        .is_some_and(|c| c.eq_ignore_ascii_case(deployed))
    {
        return Err("verified source does not match code at the observed block".into());
    }
    Ok((json!({"statements":[statement("Sourcify returned an ABI with deployed bytecode matching the observed contract.")],
        "interface_names":value["abi"].as_array().map(|abi|abi.iter().filter_map(|entry|entry["name"].as_str()).filter(|name|name.len()<=256).take(64).collect::<Vec<_>>()),
        "source_response_hash":blake3::hash(text.as_bytes()).to_hex().to_string(),"source_url":url,"match":value["match"],"related":[]}),
        Some("source verification is not a safety audit; ABI entries alone do not establish access-control semantics".into())))
}

///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub(crate) fn liquidity(
    rpc: &Rpc,
    case: &CaseKey,
    block: &str,
    budget: &mut Budget,
) -> Result<(Value, Option<String>), String> {
    let (factory, weth) = if case.chain == crate::cases::Network::Ethereum {
        (
            "0x5C69bEe701ef814a2B6a3EDD4B1652CB9cc5aA6f",
            "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        )
    } else {
        (
            "0x8909dc15e40173ff4699343b6eb8132c65e18ec6",
            "4200000000000000000000000000000000000006",
        )
    };
    let data = format!(
        "0xe6a43905{:0>64}{:0>64}",
        case.address.trim_start_matches("0x"),
        weth
    );
    let pair = address_word(&eth_call(rpc, budget, factory, &data, block)?)?;
    if pair == "0x0000000000000000000000000000000000000000" {
        return crate::evm_protocols::v3(rpc, case, block, budget);
    }
    let actual = address_word(&eth_call(rpc, budget, &pair, "0xc45a0155", block)?)?;
    if !actual.eq_ignore_ascii_case(factory) {
        return Err("pool factory identity mismatch".into());
    }
    let reserves = eth_call(rpc, budget, &pair, "0x0902f1ac", block)?;
    let digits = reserves
        .strip_prefix("0x")
        .ok_or("reserves missing prefix")?;
    if digits.len() != 192 {
        return Err("malformed pool reserves".into());
    }
    let a = word(&format!("0x{}", &digits[..64]))?;
    let b = word(&format!("0x{}", &digits[64..128]))?;
    let fee_to = address_word(&eth_call(rpc, budget, factory, "0x017e7e58", block)?)?;
    let supply = word(&eth_call(rpc, budget, &pair, "0x18160ddd", block)?)?;
    let zero = word(&eth_call(
        rpc,
        budget,
        &pair,
        &format!("0x70a08231{:064x}", 0),
        block,
    )?)?;
    let dead = word(&eth_call(
        rpc,
        budget,
        &pair,
        &format!("0x70a08231{:064x}", 0xdeadu32),
        block,
    )?)?;
    Ok((json!({"statements":[statement(format!("Verified Uniswap v2 pair {pair} reports reserve0 {a} and reserve1 {b} in base units.")),
        statement(format!("Factory feeTo() returns {fee_to}; this is configuration, not a payout receipt.")),
        statement(format!("LP total supply is {supply} base units; zero address holds {zero}, and the conventional dead address holds {dead}. These observations do not establish all withdrawal rights."))],"related":[pair,fee_to],
        "protocol":"uniswap_v2","pool":pair,"lp_supply":supply.to_string(),"lp_zero_balance":zero.to_string(),"lp_dead_balance":dead.to_string()}),Some("other pools, remaining LP ownership, locker expiry, position withdrawal rights and fee realization require additional reads".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn endpoint(responses: Vec<(&'static str, Value)>) -> (Rpc, std::thread::JoinHandle<()>) {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let handle = std::thread::spawn(move || {
            for (method, response) in responses {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "reader stopped before expected {method}"
                            );
                            std::thread::sleep(std::time::Duration::from_millis(2));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                // Windows accepted sockets inherit the listener's nonblocking mode.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut part = [0; 1024];
                loop {
                    let n = stream.read(&mut part).unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&part[..n]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&bytes[..end]);
                        let length = header
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|n| n.trim().parse::<usize>().unwrap())
                            })
                            .unwrap();
                        if bytes.len() >= end + 4 + length {
                            let body: Value =
                                serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                            assert_eq!(body["method"], method);
                            break;
                        }
                    }
                }
                let body = json!({"jsonrpc":"2.0","id":1,"result":response}).to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        (Rpc::new(format!("http://{address}")), handle)
    }
    #[test]
    fn malformed_transfer_words_never_become_zero_or_an_owner_claim() {
        let good = json!({"topics":[TRANSFER,format!("0x{:0>64}","1"),format!("0x{:0>64}","2")],
            "data":format!("0x{:0>64}","ff"),"transactionHash":"tx"});
        assert_eq!(transfer(&good).unwrap()["amount"], "255");
        let mut bad = good.clone();
        bad["data"] = json!("0x");
        assert!(transfer(&bad).is_err());
        bad = good.clone();
        bad["topics"][1] = json!("0x123");
        assert!(transfer(&bad).is_err());
        assert!(hex_u64("0xnothex").is_err());
        assert!(address_word(&format!("0x{}", "é".repeat(32))).is_err());
        bad = good;
        bad["removed"] = json!(true);
        assert!(transfer(&bad).is_err());
    }

    #[test]
    fn wrong_network_and_changed_block_cannot_authorize_evidence() {
        let case = CaseKey::new(
            crate::cases::Network::Base,
            "0x1111111111111111111111111111111111111111",
        )
        .unwrap();
        let read = Read {
            tool: Tool::Account,
            subject: case.address.clone(),
            why: "fixture".into(),
        };
        let (rpc, server) = endpoint(vec![("eth_chainId", json!("0x1"))]);
        let mut budget = Budget::default();
        assert!(
            super::read(&rpc, &case, &read, None, &mut budget, 1)
                .unwrap_err()
                .contains("does not match")
        );
        server.join().unwrap();
        assert_eq!(budget.calls_made(), 1);
        let hash = format!("0x{:064x}", 1);
        let changed = format!("0x{:064x}", 2);
        let zero = format!("0x{:064x}", 0);
        let (rpc, server) = endpoint(vec![
            ("eth_chainId", json!("0x2105")),
            ("eth_getBlockByNumber", json!({"number":"0x1","hash":hash})),
            ("eth_getCode", json!("0x6000")),
            ("eth_call", json!(zero)),
            ("eth_getStorageAt", json!(zero)),
            ("eth_getStorageAt", json!(zero)),
            ("eth_getStorageAt", json!(zero)),
            (
                "eth_getBlockByNumber",
                json!({"number":"0x1","hash":changed}),
            ),
        ]);
        let mut budget = Budget::default();
        assert!(
            super::read(&rpc, &case, &read, None, &mut budget, 1)
                .unwrap_err()
                .contains("changed during")
        );
        server.join().unwrap();
        assert_eq!(budget.calls_made(), 8);
        assert_eq!(budget.cu_spent(), 160);
    }
}
