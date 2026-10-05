// SPDX-License-Identifier: Apache-2.0
//! Wallet-level fee events from reviewed Base deployments, never pool attribution.
use crate::{
    cases::Network,
    evm_investigation::{address_word, hex_u64, transfer, word},
    investigation::statement,
};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(crate) mod collection;
pub(crate) mod window;

const CLANKER: &str = "0xf3622742b1e446d92e45e22923ef11c2fcd55d68";
const FLAUNCH: &str = "0x17fbf54d6d15ebff82eee77e616f701952d08bb4";
const CLAIM: &str = "0xf98eaa9c1f790e5c18b1f227bd5bade62600f9f3e3587c7644b90c50b9bf13c5";
const WITHDRAWAL: &str = "0x87a38faaa35605e96a58e83a8fb6e2e1c7a77407ee979d73edef64461fa1756d";
const ZERO: &str = "0x0000000000000000000000000000000000000000";
const STORE: &str = "0xadf5da7301d0edbace5767201c524369b56f4040e2eafc873897493fc466e35d";
const DEPOSIT: &str = "0xc95ddcaddf83340b68d0d44c01b1703f5d28d0611a3fd87e69d79ba7e2ac21d3";
const LIMIT: usize = 128;

#[derive(Default)]
pub(crate) struct FeeReceipts {
    pub claims: Vec<Value>,
    pub credits: Vec<Value>,
    pub statements: Vec<Value>,
    pub related: Vec<String>,
    pub coverage_complete: bool,
}

struct Claim {
    index: u64,
    protocol: &'static str,
    escrow: &'static str,
    owner: String,
    recipient: String,
    balance_asset: String,
    delivered_asset: String,
    amount: u128,
}

fn checkpoint(log: &Value, receipt: &Value) -> Result<u64, String> {
    if log["removed"].as_bool() != Some(false)
        || ["transactionHash", "blockHash", "blockNumber"]
            .iter()
            .any(|key| log[key].is_null() || log[key] != receipt[key])
    {
        return Err("log checkpoint absent, removed or inconsistent".into());
    }
    hex_u64(log["logIndex"].as_str().ok_or("log index absent")?)
}

fn decode(log: &Value, index: u64) -> Result<Option<Claim>, String> {
    let emitter = log["address"].as_str().unwrap_or("");
    let signature = log["topics"][0].as_str().unwrap_or("");
    let is_clanker = emitter.eq_ignore_ascii_case(CLANKER) && signature == CLAIM;
    let is_flaunch = emitter.eq_ignore_ascii_case(FLAUNCH) && signature == WITHDRAWAL;
    if !is_clanker && !is_flaunch {
        return Ok(None);
    }
    let topics = log["topics"].as_array().ok_or("claim topics absent")?;
    let data = log["data"].as_str().ok_or("claim data absent")?;
    let digits = data.strip_prefix("0x").ok_or("claim ABI prefix absent")?;
    let count = if is_clanker { 1 } else { 5 };
    if topics.len() != if is_clanker { 3 } else { 1 } || digits.len() != count * 64 {
        return Err("unsupported claim ABI layout".into());
    }
    let words: Vec<String> = digits
        .as_bytes()
        .as_chunks::<64>()
        .0
        .iter()
        .map(|part| format!("0x{}", String::from_utf8_lossy(part)))
        .collect();
    let (protocol, escrow, owner, recipient, balance_asset, delivered_asset) = if is_clanker {
        let owner = address_word(topics[1].as_str().ok_or("claim owner absent")?)?;
        let asset = address_word(topics[2].as_str().ok_or("claim asset absent")?)?;
        (
            "clanker_fee_locker",
            CLANKER,
            owner.clone(),
            owner,
            asset.clone(),
            asset,
        )
    } else {
        (
            "flaunch_paired_escrow",
            FLAUNCH,
            address_word(&words[0])?,
            address_word(&words[1])?,
            address_word(&words[2])?,
            address_word(&words[3])?,
        )
    };
    let amount = word(words.last().ok_or("claim amount absent")?)?;
    if amount == 0 {
        return Err("zero claim does not establish a payment".into());
    }
    Ok(Some(Claim {
        index,
        protocol,
        escrow,
        owner,
        recipient,
        balance_asset,
        delivered_asset,
        amount,
    }))
}

fn payment(claim: &Claim, log: &Value) -> bool {
    if !log["address"]
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case(&claim.delivered_asset))
    {
        return false;
    }
    let Ok(value) = transfer(log) else {
        return false;
    };
    value["from"] == claim.escrow
        && value["to"] == claim.recipient
        && value["amount"] == claim.amount.to_string()
}

fn credit(log: &Value, index: u64) -> Result<Option<Value>, String> {
    let emitter = log["address"].as_str().unwrap_or("");
    let signature = log["topics"][0].as_str().unwrap_or("");
    let clanker = emitter.eq_ignore_ascii_case(CLANKER) && signature == STORE;
    let flaunch = emitter.eq_ignore_ascii_case(FLAUNCH) && signature == DEPOSIT;
    if !clanker && !flaunch {
        return Ok(None);
    }
    let topics = log["topics"].as_array().ok_or("credit topics absent")?;
    let data = log["data"].as_str().ok_or("credit data absent")?;
    let digits = data.strip_prefix("0x").ok_or("credit ABI prefix absent")?;
    let count = if clanker { 2 } else { 3 };
    if topics.len() != if clanker { 4 } else { 2 } || digits.len() != count * 64 {
        return Err("unsupported credit ABI layout".into());
    }
    let words: Vec<String> = digits
        .as_bytes()
        .as_chunks::<64>()
        .0
        .iter()
        .map(|part| format!("0x{}", String::from_utf8_lossy(part)))
        .collect();
    let (protocol, escrow, depositor, owner, asset, pool, balance_after, amount) = if clanker {
        (
            "clanker_fee_locker",
            CLANKER,
            Some(address_word(topics[1].as_str().ok_or("depositor absent")?)?),
            address_word(topics[2].as_str().ok_or("credit owner absent")?)?,
            address_word(topics[3].as_str().ok_or("credit asset absent")?)?,
            None,
            Some(word(&words[0])?.to_string()),
            word(&words[1])?,
        )
    } else {
        let pool = topics[1].as_str().ok_or("pool label absent")?;
        pool.parse::<realorrug_robinhood::Hash32>()
            .map_err(|e| e.to_string())?;
        (
            "flaunch_paired_escrow",
            FLAUNCH,
            None,
            address_word(&words[0])?,
            address_word(&words[1])?,
            Some(pool.to_lowercase()),
            None,
            word(&words[2])?,
        )
    };
    // StoreTokens reports the requested transfer and the NEW cumulative balance,
    // not the received delta. Deposit's pool label is permissionlessly supplied.
    Ok(Some(
        json!({"protocol":protocol,"escrow":escrow,"log_index":index,
        "depositor":depositor,"fee_owner":owner,"balance_asset":asset,
        "requested_amount":if clanker {Some(amount.to_string())} else {None},
        "reported_credit_amount":if flaunch {Some(amount.to_string())} else {None},
        "reported_balance_after":balance_after,"credited_delta":null,
        "reported_pool_id":pool,"pool_attribution":"unresolved",
        "financial_state":"executed","verification_scope":"credit_event_only"}),
    ))
}

fn report_credit(value: Value, out: &mut FeeReceipts) {
    let owner = value["fee_owner"].as_str().unwrap_or("");
    let asset = value["balance_asset"].as_str().unwrap_or("");
    let details = if let Some(balance) = value["reported_balance_after"].as_str() {
        format!(
            "Clanker credit event reports cumulative balance {balance} base units and requested transfer {} base units; received credit delta requires prior balance reconciliation",
            value["requested_amount"].as_str().unwrap_or("")
        )
    } else {
        format!(
            "Flaunch credit event reports {} base units with a caller-supplied pool label; that label does not prove this token generated fees",
            value["reported_credit_amount"].as_str().unwrap_or("")
        )
    };
    out.statements.push(statement(format!("{details}, for fee owner {owner} and asset {asset}. Credit events are not withdrawals or new treasury receipts; net received amount, backing and individual-token revenue attribution remain unresolved by the credit event.")));
    out.related.extend([owner.to_owned(), asset.to_owned()]);
    if let Some(depositor) = value["depositor"].as_str() {
        out.related.push(depositor.to_owned());
    }
    out.credits.push(value);
}

fn funds(value: &Value, index: u64, log: &Value) -> bool {
    if !log["address"]
        .as_str()
        .is_some_and(|s| s.eq_ignore_ascii_case(value["balance_asset"].as_str().unwrap_or("")))
    {
        return false;
    }
    let Ok(payment) = transfer(log) else {
        return false;
    };
    index.cmp(&value["log_index"].as_u64().unwrap()) == std::cmp::Ordering::Less
        && payment["from"] == value["depositor"]
        && payment["to"] == value["escrow"]
        && payment["amount"] == value["requested_amount"]
}

fn funding<'a>(
    value: &Value,
    credits: &[Value],
    logs: &'a [(u64, &Value)],
    complete: bool,
) -> Result<(u64, &'a Value), &'static str> {
    if !complete {
        return Err("receipt log coverage is missing, truncated or inconsistent");
    }
    if value["protocol"] != "clanker_fee_locker" {
        return Err("Flaunch deposit funding requires separate supported evidence");
    }
    if value["balance_asset"] == ZERO || value["requested_amount"] == "0" {
        return Err("native or zero requested deposits do not establish ERC-20 funding");
    }
    if logs.iter().any(|(_, log)| {
        log["address"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(value["balance_asset"].as_str().unwrap()))
            && log["topics"][0].as_str() == Some(crate::evm_investigation::TRANSFER)
            && transfer(log).is_err()
    }) {
        return Err("malformed transfer of the deposit asset prevents verification");
    }
    let candidates: Vec<_> = logs
        .iter()
        .filter(|(index, log)| funds(value, *index, log))
        .collect();
    if candidates.len() != 1 {
        return Err("no unique preceding transfer matching the requested deposit");
    }
    let (index, log) = *candidates[0];
    // The same ingress cannot fund two owners' credits, even with equal requests.
    if credits.iter().filter(|c| funds(c, index, log)).count() != 1 {
        return Err("the matching transfer is ambiguous across fee credits");
    }
    Ok((index, log))
}

fn report_funding(value: &mut Value, proof: Result<(u64, &Value), &str>) {
    value["funding"] = match proof {
        Ok((index, log)) => {
            json!({"verification_scope":"receipt_requested_transfer_match",
                "log_index":index,"asset":value["balance_asset"],"transfer":transfer(log).unwrap(),
                "pool_attribution":"unresolved","gap":null})
        }
        Err(gap) => json!({"verification_scope":"unavailable_deposit_funding","gap":gap}),
    };
}

/// Rank by the checked funding role, never by matching words in a statement.
pub(crate) fn deposit_statements(
    fees: &FeeReceipts,
    windows: &[Value],
    transaction: &Value,
) -> Vec<Value> {
    fees.credits.iter().filter(|credit| credit["funding"]["verification_scope"] == "receipt_requested_transfer_match")
        .map(|credit| {
            let received = windows.iter().filter(|window| window["reconciled"] == true
                && window["fee_owner"] == credit["fee_owner"] && window["balance_asset"] == credit["balance_asset"])
                .flat_map(|window| window["events"].as_array().into_iter().flatten())
                .find(|event| event["kind"] == "credit" && event["transaction"] == *transaction && event["log_index"] == credit["log_index"])
                .and_then(|event| event["credited_delta"].as_str());
            let measurement = received.map_or_else(|| "Net received credit is unresolved.".into(), |delta| format!("Independently reconciled received credit is {delta} base units across the transaction block."));
            statement(format!("Clanker requested deposit {} base units of asset {} matches one preceding ERC-20 Transfer from depositor {} into the fee locker for fee owner {}. {measurement} This is provider/receipt evidence, not backing, upstream pool origin or this token's revenue; the credit and transfer are one deposit, not separate income.", credit["requested_amount"].as_str().unwrap(), credit["balance_asset"].as_str().unwrap(), credit["depositor"].as_str().unwrap(), credit["fee_owner"].as_str().unwrap()))
        }).collect()
}

fn delivery<'a>(
    claim: &Claim,
    claims: &[Claim],
    logs: &'a [(u64, &Value)],
    complete: bool,
) -> Result<(u64, &'a Value), &'static str> {
    if !complete {
        return Err("receipt log coverage is missing, truncated or inconsistent");
    }
    if claim.delivered_asset == ZERO {
        return Err("native delivery needs separate supported evidence");
    }
    if claim.balance_asset != claim.delivered_asset {
        return Err("wrapped-asset conversion needs separate supported evidence");
    }
    if logs.iter().any(|(_, log)| {
        log["address"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(&claim.delivered_asset))
            && log["topics"][0].as_str() == Some(crate::evm_investigation::TRANSFER)
            && transfer(log).is_err()
    }) {
        return Err("malformed transfer of the delivered asset prevents verification");
    }
    let candidates: Vec<_> = logs.iter().filter(|(_, log)| payment(claim, log)).collect();
    if candidates.len() != 1 {
        return Err("no unique matching asset transfer in this receipt");
    }
    let (index, log) = *candidates[0];
    // One transfer cannot verify several claims, even when amounts happen to agree.
    if claims.iter().filter(|other| payment(other, log)).count() != 1 {
        return Err("the matching transfer is ambiguous across fee claims");
    }
    Ok((index, log))
}

fn report(claim: &Claim, proof: Result<(u64, &Value), &str>, out: &mut FeeReceipts) {
    let (state, scope, delivery, gap) = match proof {
        Ok((index, log)) => (
            "verified",
            "receipt_event_match",
            json!({"log_index":index,"asset":claim.delivered_asset,"transfer":transfer(log).ok()}),
            None,
        ),
        Err(gap) => ("executed", "claim_event_only", Value::Null, Some(gap)),
    };
    out.claims.push(json!({"protocol":claim.protocol,"escrow":claim.escrow,"log_index":claim.index,
        "fee_owner":claim.owner,"recipient":claim.recipient,"balance_asset":claim.balance_asset,
        "delivered_asset":claim.delivered_asset,"amount":claim.amount.to_string(),"financial_state":state,
        "verification_scope":scope,"pool_attribution":"unresolved","delivery":delivery,"gap":gap}));
    let evidence = gap.unwrap_or("one matching ERC-20 Transfer event from the escrow");
    out.statements.push(statement(format!("{} claim event reports {} base units of asset {} for recipient {}; delivery check: {evidence}. This is wallet-level evidence with individual-token revenue attribution unresolved.", claim.protocol, claim.amount, claim.delivered_asset, claim.recipient)));
    out.related.extend([
        claim.owner.clone(),
        claim.recipient.clone(),
        claim.balance_asset.clone(),
        claim.delivered_asset.clone(),
    ]);
}

/// Caller has already checked canonical transaction/receipt identity. No RPC calls.
pub(crate) fn read(chain: Network, receipt: &Value) -> FeeReceipts {
    let mut out = FeeReceipts::default();
    if chain != Network::Base || receipt["status"].as_str().and_then(|s| hex_u64(s).ok()) != Some(1)
    {
        return out;
    }
    let Some(logs) = receipt["logs"].as_array() else {
        return out;
    };
    let mut complete = logs.len() <= LIMIT;
    let mut indices = HashSet::new();
    let mut claims = Vec::new();
    let mut credits = Vec::new();
    let mut observed = Vec::new();
    let mut refused = 0;
    for log in logs.iter().take(LIMIT) {
        let Ok(index) = checkpoint(log, receipt) else {
            complete = false;
            continue;
        };
        if !indices.insert(index) {
            complete = false;
            continue;
        }
        observed.push((index, log));
        match decode(log, index) {
            Ok(Some(claim)) => claims.push(claim),
            Err(_) => {
                complete = false;
                refused += 1;
            }
            Ok(None) => {}
        }
        match credit(log, index) {
            Ok(Some(value)) => credits.push(value),
            Err(_) => {
                complete = false;
                refused += 1;
            }
            Ok(None) => {}
        }
    }
    for value in &credits {
        let mut reported = value.clone();
        report_funding(&mut reported, funding(value, &credits, &observed, complete));
        report_credit(reported, &mut out);
    }
    for claim in &claims {
        report(
            claim,
            delivery(claim, &claims, &observed, complete),
            &mut out,
        );
    }
    out.coverage_complete = complete;
    if refused > 0 {
        out.statements.push(statement(format!("{refused} allowlisted fee-event log(s) could not be decoded; they do not establish credits or payments.")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    const OWNER: &str = "0x1111111111111111111111111111111111111111";
    const RECIPIENT: &str = "0x2222222222222222222222222222222222222222";
    const ASSET: &str = "0x4200000000000000000000000000000000000006";
    const TRANSFER: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
    fn addr(address: &str) -> String {
        format!("{:0>64}", &address[2..])
    }
    fn log(index: u64, address: &str, topics: Value, data: String) -> Value {
        let mut value = json!({"address":address,"removed":false,
            "logIndex":format!("0x{index:x}"),"transactionHash":format!("0x{:064x}",1),
            "blockHash":format!("0x{:064x}",2),"blockNumber":"0x10"});
        value["topics"] = topics;
        value["data"] = Value::String(data);
        value
    }
    fn claim(flaunch: bool) -> Value {
        if flaunch {
            log(
                1,
                FLAUNCH,
                json!([WITHDRAWAL]),
                format!(
                    "0x{}{}{}{}{:064x}",
                    addr(OWNER),
                    addr(RECIPIENT),
                    addr(ASSET),
                    addr(ASSET),
                    42
                ),
            )
        } else {
            log(
                1,
                CLANKER,
                json!([
                    CLAIM,
                    format!("0x{}", addr(OWNER)),
                    format!("0x{}", addr(ASSET))
                ]),
                format!("0x{:064x}", 42),
            )
        }
    }
    fn paid(flaunch: bool) -> Value {
        log(
            0,
            ASSET,
            json!([
                TRANSFER,
                format!("0x{}", addr(if flaunch { FLAUNCH } else { CLANKER })),
                format!("0x{}", addr(if flaunch { RECIPIENT } else { OWNER }))
            ]),
            format!("0x{:064x}", 42),
        )
    }
    fn receipt(logs: Vec<Value>) -> Value {
        let mut value = json!({"transactionHash":format!("0x{:064x}",1),"blockHash":format!("0x{:064x}",2),"blockNumber":"0x10","status":"0x1"});
        value["logs"] = Value::Array(logs);
        value
    }

    fn stored(flaunch: bool) -> Value {
        if flaunch {
            log(
                2,
                FLAUNCH,
                json!([DEPOSIT, format!("0x{:064x}", 7)]),
                format!("0x{}{}{:064x}", addr(OWNER), addr(ASSET), 42),
            )
        } else {
            log(
                2,
                CLANKER,
                json!([
                    STORE,
                    format!("0x{}", addr(RECIPIENT)),
                    format!("0x{}", addr(OWNER)),
                    format!("0x{}", addr(ASSET))
                ]),
                format!("0x{:064x}{:064x}", 109, 42),
            )
        }
    }

    fn incoming(index: u64) -> Value {
        log(
            index,
            ASSET,
            json!([
                TRANSFER,
                format!("0x{}", addr(RECIPIENT)),
                format!("0x{}", addr(CLANKER))
            ]),
            format!("0x{:064x}", 42),
        )
    }

    #[test]
    fn deposit_funding_matches_requested_transfer_without_inventing_net_credit_or_pool_revenue() {
        // Array order is not execution order; only checked block-global indices count.
        let out = read(Network::Base, &receipt(vec![stored(false), incoming(0)]));
        let value = &out.credits[0];
        let proof = &value["funding"];
        assert_eq!(
            proof["verification_scope"],
            "receipt_requested_transfer_match"
        );
        assert_eq!(proof["log_index"], 0);
        assert_eq!(proof["asset"], ASSET);
        assert_eq!(proof["transfer"]["from"], RECIPIENT);
        assert_eq!(proof["transfer"]["to"], CLANKER);
        assert_eq!(proof["transfer"]["amount"], "42");
        assert_eq!(proof["pool_attribution"], "unresolved");
        assert!(proof["gap"].is_null());
        assert_eq!(value["financial_state"], "executed");
        assert_eq!(value["verification_scope"], "credit_event_only");
        assert_eq!(value["reported_balance_after"], "109");
        assert!(value["credited_delta"].is_null());
        assert_eq!(out.claims, [] as [Value; 0]);
        assert!(deposit_statements(&out, &[], &json!("tx")).iter().any(|s| {
            s["text"]
                .as_str()
                .unwrap()
                .contains("one deposit, not separate income")
        }));
        for change in 0..5 {
            let mut wrong = incoming(0);
            match change {
                0 => wrong["address"] = json!(OWNER),
                1 => wrong["topics"][1] = json!(format!("0x{}", addr(OWNER))),
                2 => wrong["topics"][2] = json!(format!("0x{}", addr(OWNER))),
                3 => wrong["data"] = json!(format!("0x{:064x}", 109)),
                _ => wrong["logIndex"] = json!("0x3"),
            }
            let result = read(Network::Base, &receipt(vec![wrong, stored(false)]));
            assert_eq!(
                result.credits[0]["funding"]["verification_scope"],
                "unavailable_deposit_funding"
            );
            assert!(result.credits[0]["funding"]["transfer"].is_null());
        }
        let mut upper = incoming(0);
        upper["address"] = json!(ASSET.to_uppercase());
        assert_eq!(
            read(Network::Base, &receipt(vec![upper, stored(false)])).credits[0]["funding"]["log_index"],
            0
        );
    }

    #[test]
    fn deposit_funding_refuses_ambiguous_incomplete_native_zero_and_unsupported_evidence() {
        let check = |logs: Vec<Value>, gap: &str| {
            let out = read(Network::Base, &receipt(logs));
            assert!(
                !out.credits.is_empty(),
                "fixture must retain a decoded credit"
            );
            for c in out.credits {
                assert_eq!(c["financial_state"], "executed");
                assert_eq!(
                    c["funding"]["verification_scope"],
                    "unavailable_deposit_funding"
                );
                assert!(c["funding"]["transfer"].is_null());
                assert!(
                    c["funding"]["gap"].as_str().unwrap().contains(gap),
                    "{}",
                    c["funding"]
                );
            }
        };
        check(vec![stored(false)], "no unique preceding");
        check(
            vec![incoming(0), incoming(1), stored(false)],
            "no unique preceding",
        );
        let mut second = stored(false);
        second["logIndex"] = json!("0x3");
        second["topics"][2] = json!(format!("0x{}", addr(RECIPIENT)));
        check(
            vec![incoming(0), stored(false), second],
            "ambiguous across fee credits",
        );
        check(vec![incoming(0), stored(true)], "Flaunch");
        for (asset, amount) in [(ZERO, 42), (ASSET, 0)] {
            let mut c = stored(false);
            c["topics"][3] = json!(format!("0x{}", addr(asset)));
            c["data"] = json!(format!("0x{:064x}{amount:064x}", 109));
            check(vec![incoming(0), c], "native or zero");
        }
        let mut malformed = incoming(1);
        malformed["data"] = json!("0x");
        check(
            vec![incoming(0), malformed, stored(false)],
            "malformed transfer",
        );
        for (asset, signature) in [(OWNER, TRANSFER), (ASSET, "0xother")] {
            let unrelated = log(1, asset, json!([signature]), "0x".into());
            assert_eq!(
                read(
                    Network::Base,
                    &receipt(vec![incoming(0), stored(false), unrelated])
                )
                .credits[0]["funding"]["log_index"],
                0
            );
        }
        for field in [
            "removed",
            "transactionHash",
            "blockHash",
            "blockNumber",
            "logIndex",
        ] {
            let mut wrong = incoming(0);
            wrong[field] = Value::Null;
            check(vec![wrong, stored(false)], "inconsistent");
        }
        check(
            vec![incoming(0), incoming(0), stored(false)],
            "inconsistent",
        );
        let mut malformed = stored(false);
        malformed["logIndex"] = json!("0x3");
        malformed["data"] = json!("0x");
        check(vec![incoming(0), stored(false), malformed], "inconsistent");
        for n in [128, 129] {
            let mut logs = vec![incoming(0), stored(false)];
            logs.extend((3..=n).map(|i| log(i, OWNER, json!([]), "0x".into())));
            let out = read(Network::Base, &receipt(logs));
            assert_eq!(out.coverage_complete, n == 128);
            assert_eq!(
                out.credits[0]["funding"]["verification_scope"],
                if n == 128 {
                    "receipt_requested_transfer_match"
                } else {
                    "unavailable_deposit_funding"
                }
            );
        }
    }

    #[test]
    fn retained_credit_receipt_links_ingress_but_not_the_investigated_token() {
        let raw: Value = serde_json::from_str(include_str!(
            "../../../docs/research/data/0071-base/clanker-credit-transaction.json"
        ))
        .unwrap();
        let out = read(Network::Base, &raw["receipt"]);
        assert!(out.coverage_complete);
        assert_eq!(out.credits.len(), 1);
        let c = &out.credits[0];
        assert_eq!(c["log_index"], 107);
        assert_eq!(c["funding"]["log_index"], 106);
        assert_eq!(
            c["funding"]["transfer"]["from"],
            "0x63d2dfea64b3433f4071a98665bcd7ca14d93496"
        );
        assert_eq!(c["funding"]["transfer"]["amount"], "29478578528827");
        assert_eq!(c["funding"]["pool_attribution"], "unresolved");
        assert!(c["credited_delta"].is_null());
        assert!(c["reported_pool_id"].is_null());
        assert_eq!(out.claims, [] as [Value; 0]);
    }

    #[test]
    fn deposit_reply_uses_only_the_matching_reconciled_anchor_and_preserves_unknowns() {
        let out = read(Network::Base, &receipt(vec![incoming(0), stored(false)]));
        let window = json!({"reconciled":true,"fee_owner":OWNER,"balance_asset":ASSET,
            "events":[{"kind":"credit","transaction":"tx","log_index":2,"credited_delta":"5"}]});
        let text = |windows: &[Value]| {
            deposit_statements(&out, windows, &json!("tx"))[0]["text"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        assert!(text(std::slice::from_ref(&window)).contains("received credit is 5 base units"));
        assert!(text(&[]).contains("Net received credit is unresolved"));
        for field in ["reconciled", "fee_owner", "balance_asset"] {
            let mut wrong = window.clone();
            wrong[field] = json!("foreign");
            assert!(text(&[wrong]).contains("Net received credit is unresolved"));
        }
        for field in ["kind", "transaction", "log_index", "credited_delta"] {
            let mut wrong = window.clone();
            wrong["events"][0][field] = Value::Null;
            assert!(text(&[wrong]).contains("Net received credit is unresolved"));
        }
        let mut wrong = window.clone();
        wrong["events"] = Value::Null;
        assert!(text(&[wrong]).contains("Net received credit is unresolved"));
        assert_eq!(
            deposit_statements(
                &read(Network::Base, &receipt(vec![stored(false)])),
                &[window],
                &json!("tx")
            ),
            [] as [Value; 0]
        );
    }

    #[test]
    fn credit_events_preserve_requested_cumulative_and_caller_supplied_roles() {
        let out = read(Network::Base, &receipt(vec![stored(false), stored(true)]));
        // Duplicate indices are never counted twice, even across protocols.
        assert_eq!(out.credits.len(), 1);
        assert!(!out.coverage_complete);
        for flaunch in [false, true] {
            let out = read(Network::Base, &receipt(vec![stored(flaunch)]));
            assert!(out.coverage_complete);
            assert_eq!(out.claims, [] as [Value; 0]);
            assert_eq!(out.credits.len(), 1);
            let value = &out.credits[0];
            assert_eq!(value["log_index"], 2);
            assert_eq!(value["fee_owner"], OWNER);
            assert_eq!(value["balance_asset"], ASSET);
            assert_eq!(value["financial_state"], "executed");
            assert_eq!(value["verification_scope"], "credit_event_only");
            assert_eq!(value["pool_attribution"], "unresolved");
            assert!(value["credited_delta"].is_null());
            assert!(out.related.contains(&OWNER.into()));
            assert!(out.related.contains(&ASSET.into()));
            if flaunch {
                assert_eq!(value["protocol"], "flaunch_paired_escrow");
                assert_eq!(value["escrow"], FLAUNCH);
                assert_eq!(value["reported_credit_amount"], "42");
                assert_eq!(value["reported_pool_id"], format!("0x{:064x}", 7));
                assert!(value["depositor"].is_null());
                assert!(value["requested_amount"].is_null());
                assert!(value["reported_balance_after"].is_null());
                assert!(
                    out.statements[0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("caller-supplied")
                );
            } else {
                assert_eq!(value["protocol"], "clanker_fee_locker");
                assert_eq!(value["escrow"], CLANKER);
                assert_eq!(value["depositor"], RECIPIENT);
                assert_eq!(value["requested_amount"], "42");
                assert_eq!(value["reported_balance_after"], "109");
                assert!(value["reported_credit_amount"].is_null());
                assert!(value["reported_pool_id"].is_null());
                assert!(out.related.contains(&RECIPIENT.into()));
                assert!(
                    out.statements[0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("prior balance reconciliation")
                );
            }
            assert!(
                out.statements[0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("not withdrawals")
            );
        }
    }

    #[test]
    fn malformed_credit_events_and_incomplete_receipts_cannot_verify_payments() {
        for flaunch in [false, true] {
            for mutation in [
                "short", "extra", "address", "overflow", "hex", "pool", "topics",
            ] {
                if mutation == "pool" && !flaunch {
                    continue;
                }
                let mut event = stored(flaunch);
                match mutation {
                    "short" => {
                        event["data"] = json!("0x");
                    }
                    "extra" => {
                        event["data"] =
                            json!(format!("{}{:064x}", event["data"].as_str().unwrap(), 0));
                    }
                    "address" => {
                        if flaunch {
                            event["data"] =
                                json!(format!("0x{}{}{:064x}", "f".repeat(64), addr(ASSET), 42));
                        } else {
                            event["topics"][2] = json!(format!("0x{}", "f".repeat(64)));
                        }
                    }
                    "overflow" => {
                        event["data"] = json!(format!(
                            "{}{}",
                            if flaunch {
                                format!("0x{}{}", addr(OWNER), addr(ASSET))
                            } else {
                                format!("0x{:064x}", 109)
                            },
                            "f".repeat(64)
                        ));
                    }
                    "hex" => {
                        event["data"] = json!(event["data"].as_str().unwrap().replace("2a", "zz"));
                    }
                    "pool" => {
                        event["topics"][1] = json!("0x7");
                    }
                    "topics" => {
                        event["topics"].as_array_mut().unwrap().push(json!("0x0"));
                    }
                    _ => unreachable!(),
                }
                let out = read(
                    Network::Base,
                    &receipt(vec![event, paid(false), claim(false)]),
                );
                assert!(out.credits.is_empty(), "{flaunch} {mutation}");
                assert!(!out.coverage_complete);
                assert_eq!(out.claims[0]["financial_state"], "executed");
                assert!(
                    out.statements
                        .iter()
                        .any(|s| s["text"].as_str().unwrap().contains("could not be decoded"))
                );
            }
            for field in ["removed", "blockHash", "transactionHash", "logIndex"] {
                let mut event = stored(flaunch);
                event[field] = Value::Null;
                let out = read(Network::Base, &receipt(vec![event]));
                assert_eq!(out.credits, [] as [Value; 0]);
                assert!(!out.coverage_complete);
            }
            let mut wrong = stored(flaunch);
            wrong["address"] = json!(OWNER);
            assert_eq!(
                read(Network::Base, &receipt(vec![wrong])).credits,
                [] as [Value; 0]
            );
            let mut failed = receipt(vec![stored(flaunch)]);
            failed["status"] = json!("0x0");
            assert_eq!(read(Network::Base, &failed).credits, [] as [Value; 0]);
            assert_eq!(
                read(Network::Ethereum, &receipt(vec![stored(flaunch)])).credits,
                [] as [Value; 0]
            );
        }
        let mut logs = vec![json!({}); LIMIT];
        logs.push(stored(false));
        let out = read(Network::Base, &receipt(logs));
        assert_eq!(out.credits, [] as [Value; 0]);
        assert!(!out.coverage_complete);
    }

    #[test]
    fn direct_claims_match_exact_asset_sender_recipient_amount_without_pool_attribution() {
        for flaunch in [true, false] {
            let result = read(Network::Base, &receipt(vec![paid(flaunch), claim(flaunch)]));
            let c = &result.claims[0];
            assert_eq!(result.claims.len(), 1);
            assert_eq!(result.statements.len(), 1);
            assert!(result.coverage_complete);
            assert_eq!(c["financial_state"], "verified");
            assert_eq!(c["pool_attribution"], "unresolved");
            assert_eq!(c["verification_scope"], "receipt_event_match");
            assert_eq!(c["delivery"]["log_index"], 0);
            assert_eq!(c["amount"], "42");
            assert_eq!(c["fee_owner"], OWNER);
            assert_eq!(c["balance_asset"], ASSET);
            assert_eq!(c["delivered_asset"], ASSET);
            assert_eq!(c["recipient"], if flaunch { RECIPIENT } else { OWNER });
            assert_eq!(c["escrow"], if flaunch { FLAUNCH } else { CLANKER });
            assert_eq!(
                c["protocol"],
                if flaunch {
                    "flaunch_paired_escrow"
                } else {
                    "clanker_fee_locker"
                }
            );
            assert!(c["gap"].is_null());
            assert_eq!(c["delivery"]["transfer"]["amount"], "42");
            assert!(result.related.contains(&OWNER.to_owned()));
            assert!(result.related.contains(&ASSET.to_owned()));
            assert!(
                result
                    .related
                    .contains(&if flaunch { RECIPIENT } else { OWNER }.to_owned())
            );
            assert!(
                result.statements[0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("individual-token revenue attribution unresolved")
            );
            for mutation in 0..4 {
                let mut wrong = paid(flaunch);
                match mutation {
                    0 => wrong["address"] = json!(OWNER),
                    1 => wrong["topics"][1] = json!(format!("0x{}", addr(ZERO))),
                    2 => wrong["topics"][2] = json!(format!("0x{}", addr(ZERO))),
                    _ => wrong["data"] = json!(format!("0x{:064x}", 41)),
                }
                let result = read(Network::Base, &receipt(vec![wrong, claim(flaunch)]));
                assert_eq!(result.claims[0]["financial_state"], "executed");
                assert_eq!(result.claims[0]["verification_scope"], "claim_event_only");
                assert!(result.claims[0]["delivery"].is_null());
            }
        }
    }

    #[test]
    fn log_checkpoints_and_unique_indices_are_required_before_delivery_verification() {
        for field in [
            "transactionHash",
            "blockHash",
            "blockNumber",
            "logIndex",
            "removed",
        ] {
            for value in [Value::Null, json!("wrong"), json!(true)] {
                let mut wrong = paid(false);
                wrong[field] = value;
                let result = read(Network::Base, &receipt(vec![wrong, claim(false)]));
                assert_eq!(result.claims[0]["financial_state"], "executed");
                assert!(
                    result.claims[0]["gap"]
                        .as_str()
                        .unwrap()
                        .contains("inconsistent")
                );
                assert!(!result.coverage_complete);
                let mut wrong = claim(false);
                wrong[field] = Value::Null;
                assert_eq!(
                    read(Network::Base, &receipt(vec![paid(false), wrong])).claims,
                    Vec::<Value>::new()
                );
            }
        }
        let mut duplicate = paid(false);
        duplicate["logIndex"] = json!("0x01");
        let result = read(Network::Base, &receipt(vec![claim(false), duplicate]));
        assert_eq!(result.claims.len(), 1);
        assert_eq!(result.claims[0]["financial_state"], "executed");
        let result = read(
            Network::Base,
            &receipt(vec![paid(false), claim(false), claim(false)]),
        );
        assert_eq!(result.claims.len(), 1);
        assert_eq!(result.claims[0]["financial_state"], "executed");
    }

    #[test]
    fn ambiguous_reused_transfers_and_incomplete_coverage_never_verify_claims() {
        let mut duplicate = paid(false);
        duplicate["logIndex"] = json!("0x2");
        let result = read(
            Network::Base,
            &receipt(vec![paid(false), duplicate, claim(false)]),
        );
        assert_eq!(result.claims[0]["financial_state"], "executed");
        let mut second_claim = claim(false);
        second_claim["logIndex"] = json!("0x2");
        let result = read(
            Network::Base,
            &receipt(vec![paid(false), claim(false), second_claim]),
        );
        assert_eq!(result.claims.len(), 2);
        for c in result.claims {
            assert_eq!(c["financial_state"], "executed");
            assert!(c["gap"].as_str().unwrap().contains("ambiguous"));
        }
        for n in [128, 129] {
            let mut logs = vec![paid(false), claim(false)];
            logs.extend((2..n).map(|index| log(index, OWNER, json!([]), "0x".into())));
            let result = read(Network::Base, &receipt(logs));
            assert_eq!(
                result.claims[0]["financial_state"],
                if n == 128 { "verified" } else { "executed" }
            );
        }
        let result = read(Network::Base, &receipt(vec![claim(false)]));
        assert_eq!(result.claims[0]["financial_state"], "executed");
        for (asset, event) in [(OWNER, TRANSFER), (ASSET, "0xother-event")] {
            let unrelated = log(2, asset, json!([event]), "0x".into());
            let result = read(
                Network::Base,
                &receipt(vec![paid(false), claim(false), unrelated]),
            );
            assert_eq!(result.claims[0]["financial_state"], "verified");
        }
        let mut malformed = paid(false);
        malformed["logIndex"] = json!("0x2");
        malformed["data"] = json!("0x");
        let result = read(
            Network::Base,
            &receipt(vec![paid(false), claim(false), malformed]),
        );
        assert!(
            result.claims[0]["gap"]
                .as_str()
                .unwrap()
                .contains("malformed transfer")
        );
        let mut malformed = claim(false);
        malformed["logIndex"] = json!("0x2");
        malformed["data"] = json!("0x");
        let result = read(
            Network::Base,
            &receipt(vec![paid(false), claim(false), malformed]),
        );
        assert_eq!(result.claims[0]["financial_state"], "executed");
        assert!(!result.coverage_complete);
        let mut tail: Vec<Value> = (0..127)
            .map(|index| log(index, OWNER, json!([]), "0x".into()))
            .collect();
        let mut last_payment = paid(false);
        last_payment["logIndex"] = json!("0x7f");
        let mut late_claim = claim(false);
        late_claim["logIndex"] = json!("0x80");
        tail.push(last_payment);
        tail.push(late_claim);
        let result = read(Network::Base, &receipt(tail));
        assert_eq!(result.claims, Vec::<Value>::new());
        assert!(!result.coverage_complete);
    }

    #[test]
    fn unsupported_chain_native_conversion_and_malformed_claims_remain_unverified() {
        let good = receipt(vec![paid(true), claim(true)]);
        for chain in [Network::Ethereum, Network::Solana, Network::Robinhood] {
            assert_eq!(read(chain, &good).claims, Vec::<Value>::new());
        }
        for status in [Value::Null, json!("0x0"), json!("0x2"), json!("bad")] {
            let mut failed = good.clone();
            failed["status"] = status;
            assert_eq!(read(Network::Base, &failed).claims, Vec::<Value>::new());
        }
        let mut missing = good.clone();
        missing["logs"] = Value::Null;
        assert_eq!(read(Network::Base, &missing).claims, Vec::<Value>::new());
        for asset in [ZERO, OWNER] {
            let mut converted = claim(true);
            converted["data"] = json!(format!(
                "0x{}{}{}{}{:064x}",
                addr(OWNER),
                addr(RECIPIENT),
                addr(ASSET),
                addr(asset),
                42
            ));
            let result = read(Network::Base, &receipt(vec![converted]));
            assert_eq!(result.claims[0]["financial_state"], "executed");
            assert!(
                result.claims[0]["gap"]
                    .as_str()
                    .unwrap()
                    .contains(if asset == ZERO {
                        "native"
                    } else {
                        "conversion"
                    })
            );
        }
        for flaunch in [false, true] {
            for data in [
                "0x".to_owned(),
                "0xgg".to_owned(),
                format!("0x{}", "0".repeat(if flaunch { 320 } else { 64 })),
                format!("0x{}", "f".repeat(if flaunch { 320 } else { 64 })),
            ] {
                let mut malformed = claim(flaunch);
                malformed["data"] = json!(data);
                let result = read(Network::Base, &receipt(vec![malformed]));
                assert_eq!(result.claims, Vec::<Value>::new());
                assert_eq!(result.statements.len(), 1);
            }
            let mut foreign = claim(flaunch);
            foreign["address"] = json!(OWNER);
            assert_eq!(
                read(Network::Base, &receipt(vec![foreign])).claims,
                Vec::<Value>::new()
            );
            let mut other_event = claim(flaunch);
            other_event["topics"][0] = json!(TRANSFER);
            assert_eq!(
                read(Network::Base, &receipt(vec![other_event])).claims,
                Vec::<Value>::new()
            );
        }
    }
}
