// SPDX-License-Identifier: Apache-2.0
//! Historical collection context, explicitly short of token revenue attribution.
use super::{CLANKER, FeeReceipts};
use crate::{
    Budget,
    cases::{CaseKey, Network},
    evm_investigation::{address_word, call},
    evm_protocols::{
        CLANKER as FACTORY, address, arg, clanker_rewards, encode_hex, hex_bytes, number,
        pool_key_hash, query, words,
    },
    investigation::statement,
};
use realorrug_robinhood::Rpc;
use serde_json::{Value, json};
use sha3::{Digest, Keccak256};
use std::collections::HashSet;

mod configuration;

const LOCKER: &str = "0x63d2dfea64b3433f4071a98665bcd7ca14d93496";
const POOL_MANAGER: &str = "0x498581ff718922c3f8e6a244956af099b2652b2b";
const POSITION_MANAGER: &str = "0x7c5f5a4bbd8fd63184577525326123b519429bdc";
// Captured source/runtime qualification in research 0073, not a source-name guess.
const RUNTIME: &str = "0x49a98b5202a1977a74052a1128c3e01793619d5fe0271bb3bf37276066499e60";
const COLLECTED: &str = "0x21d15f71483b597e8f0009e83b90b2117f6f98c185d7173857dddcae5eb8546a";
const MODIFY: &str = "0xf208f4912782fd25c7f114ca3723a2d5dd6f3bcc3ac8db5af63baa85f711d5ec";

#[derive(Debug)]
struct Collection {
    token: String,
    index: u64,
    rewards: [Vec<u128>; 2],
}

pub(crate) fn read(
    rpc: &Rpc,
    case: &CaseKey,
    receipt: &Value,
    canonical: &Value,
    fees: &FeeReceipts,
    budget: &mut Budget,
) -> Vec<Value> {
    if case.chain != Network::Base || !fees.credits.iter().any(|c| c["depositor"] == LOCKER) {
        return Vec::new();
    }
    let result = if !fees.coverage_complete || fees.credits.len() != 1 {
        Err("collection context requires a complete receipt with one credit".into())
    } else if budget.calls_left() < 8 {
        Err("insufficient shared calls for collection context and final checkpoint".into())
    } else {
        qualify(rpc, case, receipt, canonical, &fees.credits[0], budget)
    };
    vec![result.unwrap_or_else(|gap: String| json!({"qualified":false,"lp_locker":LOCKER,
        "verification_scope":"unavailable_collection_context","pool_attribution":"unresolved","gap":gap}))]
}

fn collection(receipt: &Value, credit: &Value) -> Result<Collection, String> {
    if credit["funding"]["verification_scope"] != "receipt_requested_transfer_match" {
        return Err("collection context requires matched deposit ingress".into());
    }
    let logs = receipt["logs"]
        .as_array()
        .ok_or("collection receipt logs absent")?;
    let candidates: Vec<_> = logs
        .iter()
        .filter(|log| {
            log["address"]
                .as_str()
                .is_some_and(|a| a.eq_ignore_ascii_case(LOCKER))
                && log["topics"][0] == COLLECTED
        })
        .collect();
    if candidates.len() != 1 {
        return Err("no unique supported collection event".into());
    }
    let log = candidates[0];
    let index = super::checkpoint(log, receipt)?;
    if index.cmp(&credit["log_index"].as_u64().unwrap()) != std::cmp::Ordering::Greater
        || log["topics"]
            .as_array()
            .ok_or("collection topics absent")?
            .len()
            != 2
    {
        return Err("collection event order or topics disagree".into());
    }
    let token = address_word(log["topics"][1].as_str().ok_or("collection token absent")?)?;
    let data = words(log["data"].as_str().ok_or("collection data absent")?)?;
    let n = usize::try_from(number(&data, 4)?).map_err(|e| e.to_string())?;
    if !(1..=16).contains(&n)
        || data.len() != 6 + 2 * n
        || number(&data, 2)? != 128
        || number(&data, 3)? != ((5 + n) * 32) as u128
        || number(&data, 5 + n)? != n as u128
    {
        return Err("unsupported collection array layout or recipient bound".into());
    }
    let mut rewards = [Vec::new(), Vec::new()];
    for (asset, start) in [5, 6 + n].into_iter().enumerate() {
        let mut total = 0u128;
        for i in start..start + n {
            let amount = number(&data, i)?;
            total = total
                .checked_add(amount)
                .ok_or("collection total overflow")?;
            rewards[asset].push(amount);
        }
        if total != number(&data, asset)? {
            return Err("collection reward total disagrees".into());
        }
    }
    Ok(Collection {
        token,
        index,
        rewards,
    })
}

fn qualify(
    rpc: &Rpc,
    case: &CaseKey,
    receipt: &Value,
    canonical: &Value,
    credit: &Value,
    budget: &mut Budget,
) -> Result<Value, String> {
    let event = collection(receipt, credit)?;
    let block = receipt["blockNumber"]
        .as_str()
        .ok_or("collection block absent")?;
    let code = call(rpc, budget, "eth_getCode", json!([LOCKER, block]))?;
    let hash = format!(
        "0x{}",
        encode_hex(&Keccak256::digest(hex_bytes(
            code.as_str()
                .ok_or("locker runtime absent")?
                .strip_prefix("0x")
                .ok_or("runtime prefix absent")?,
        )?))
    );
    if hash != RUNTIME {
        return Err("historical locker runtime differs from the reviewed deployment".into());
    }
    for (getter, expected) in [
        ("feeLocker()", CLANKER),
        ("poolManager()", POOL_MANAGER),
        ("positionManager()", POSITION_MANAGER),
    ] {
        if address(&query(rpc, budget, LOCKER, getter, "", block)?, 0)? != expected {
            return Err("historical locker getter differs from the reviewed dependencies".into());
        }
    }
    let deployment = query(
        rpc,
        budget,
        FACTORY,
        "tokenDeploymentInfo(address)",
        &arg(&event.token),
        block,
    )?;
    if number(&deployment, 0)? != 32
        || address(&deployment, 1)? != event.token
        || address(&deployment, 3)? != LOCKER
    {
        return Err("historical token/locker registration disagrees".into());
    }
    let data = query(
        rpc,
        budget,
        LOCKER,
        "tokenRewards(address)",
        &arg(&event.token),
        block,
    )?;
    let context = link(receipt, credit, &event, &deployment, &data)?;
    let current = call(rpc, budget, "eth_getBlockByNumber", json!([block, false]))?;
    if current["number"] != block || current["hash"] != canonical["hash"] {
        return Err("collection context checkpoint changed during the read".into());
    }
    let configuration = configuration::read(rpc, receipt, canonical, &code, &data, budget);
    Ok(
        json!({"qualified":true,"verification_scope":"historical_collection_context",
        "lp_locker":LOCKER,"registry":FACTORY,"runtime_hash":hash,"collection_log_index":event.index,
        "reported_token":event.token,"case_token_matches":event.token==case.address,
        "block":block,"block_hash":canonical["hash"],"context":context,"reward_configuration":configuration,"pool_attribution":"unresolved",
        "gap":"single supported collection and deposit only; registration and recipients are block-end state, not proven at every intra-block event; collection context does not prove asset backing, conversion correctness or per-token revenue; shared PositionManager transient balances and other transaction actions require separate analysis"}),
    )
}

fn link(
    receipt: &Value,
    credit: &Value,
    event: &Collection,
    deployment: &[String],
    data: &[String],
) -> Result<Value, String> {
    let configuration = clanker_rewards(data, &event.token)?;
    let currencies = [address(data, 2)?, address(data, 3)?];
    if !currencies.contains(&event.token) || address(data, 6)? != address(deployment, 2)? {
        return Err("historical collection pool token or hook disagrees".into());
    }
    let asset = currencies
        .iter()
        .position(|a| *a == credit["balance_asset"])
        .ok_or("deposit asset is outside the reported pool currencies")?;
    let shares = configuration["shares"].as_array().unwrap();
    let slots: Vec<_> = shares
        .iter()
        .enumerate()
        .filter(|(_, share)| share["recipient"] == credit["fee_owner"])
        .collect();
    if slots.len() != 1 || shares.len() != event.rewards[0].len() {
        return Err(
            "historical recipient slot is absent, ambiguous or disagrees with collection arrays"
                .into(),
        );
    }
    let slot = slots[0].0;
    if event.rewards[asset][slot].to_string() != credit["requested_amount"] {
        return Err("collection slot does not match the requested deposit".into());
    }
    let pool = format!("0x{}", pool_key_hash(&data[2..7])?);
    let positions = positions(receipt, credit, &pool, number(data, 7)?, number(data, 8)?)?;
    Ok(
        json!({"pool_id":pool,"currencies":currencies,"hook":address(data,6)?,
        "recipient_slot":slot,"reported_requested_reward":event.rewards[asset][slot].to_string(),
        "deposit_log_index":credit["log_index"],"funding_log_index":credit["funding"]["log_index"],
        "positions":positions}),
    )
}

fn positions(
    receipt: &Value,
    credit: &Value,
    pool: &str,
    first: u128,
    count: u128,
) -> Result<Vec<Value>, String> {
    if !(1..=16).contains(&count) {
        return Err("collection position count exceeds bound".into());
    }
    let end = first.checked_add(count).ok_or("position range overflow")?;
    let logs: Vec<_> = receipt["logs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|log| {
            log["address"]
                .as_str()
                .is_some_and(|a| a.eq_ignore_ascii_case(POOL_MANAGER))
                && log["topics"][0] == MODIFY
        })
        .collect();
    if logs.len() as u128 != count {
        return Err("receipt position modification count disagrees".into());
    }
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for log in logs {
        let index = super::checkpoint(log, receipt)?;
        let data = words(log["data"].as_str().ok_or("position event data absent")?)?;
        if log["topics"]
            .as_array()
            .ok_or("position topics absent")?
            .len()
            != 3
            || log["topics"][1] != pool
            || address_word(log["topics"][2].as_str().ok_or("position actor absent")?)?
                != POSITION_MANAGER
            || data.len() != 4
            || number(&data, 2)? != 0
            || index.cmp(&credit["funding"]["log_index"].as_u64().unwrap())
                != std::cmp::Ordering::Less
        {
            return Err("position pool, actor, zero-liquidity layout or order disagrees".into());
        }
        let position = number(&data, 3)?;
        if !(first..end).contains(&position) || !seen.insert(position) {
            return Err("position identity is outside the registered range or repeated".into());
        }
        result.push(json!({"position_id":position.to_string(),"log_index":index}));
    }
    Ok(result)
}

pub(crate) fn statements(contexts: &[Value], case: &CaseKey) -> Vec<Value> {
    contexts.iter().filter(|context| context["qualified"] == true).map(|context| {
        let relation = if context["case_token_matches"] == true {"matches"} else {"differs from"};
        let timing = if context["reward_configuration"]["qualified"] == true {
            "Parent and closing reward tuples and locker runtimes agree, and the provider reports this collection as the block's only locker event. This supports reward-configuration stability under reviewed-source/provider assumptions; registry history, fee preferences, conversion correctness, backing and per-token revenue remain unresolved."
        } else {
            "This is collection context using block-end configuration, not proof of this token's fee revenue, backing or configuration at the instant of the deposit."
        };
        statement(format!("The reviewed LP locker reports a collection for token {}, which {relation} case token {}; historical registration and zero-liquidity position events agree with pool {} and the deposit recipient slot. {timing}", context["reported_token"].as_str().unwrap(), case.address, context["context"]["pool_id"].as_str().unwrap()))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evm_investigation::tests::endpoint_checked;
    use std::fmt::Write as _;

    fn encoded(data: &[u128]) -> String {
        data.iter().fold(String::from("0x"), |mut out, value| {
            write!(out, "{value:064x}").unwrap();
            out
        })
    }

    pub(super) fn fixture() -> (Value, FeeReceipts, Vec<String>, Vec<String>) {
        let raw: Value = serde_json::from_str(include_str!(
            "../../../../docs/research/data/0071-base/clanker-credit-transaction.json"
        ))
        .unwrap();
        let receipt = raw["receipt"].clone();
        let fees = super::super::read(Network::Base, &receipt);
        let historical: Value = serde_json::from_str(include_str!(
            "../../../../docs/research/data/0073-base/clanker-depositor-qualification.json"
        ))
        .unwrap();
        let data = words(
            historical["getters"]["tokenRewards(address)"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        // Synthetic registration fixture; the live registry response is tested separately.
        let deployment = vec![
            format!("0x{:064x}", 32),
            format!("0x{}", arg("0xad794ad19350a1755d90d53380102dc7213b8b07")),
            data[6].clone(),
            format!("0x{}", arg(LOCKER)),
        ];
        (receipt, fees, deployment, data)
    }

    fn replace_word(log: &mut Value, index: usize, value: &str) {
        let mut data = words(log["data"].as_str().unwrap()).unwrap();
        data[index] = value.into();
        log["data"] = json!(format!(
            "0x{}",
            data.iter()
                .map(|s| s.trim_start_matches("0x"))
                .collect::<String>()
        ));
    }

    fn responses() -> Vec<(&'static str, Value, Option<Value>)> {
        let trace: Value = serde_json::from_str(include_str!(
            "../../../../docs/research/data/0074-base/clanker-collection-rpc.json"
        ))
        .unwrap();
        let methods = [
            "eth_getCode",
            "eth_call",
            "eth_call",
            "eth_call",
            "eth_call",
            "eth_call",
            "eth_getBlockByNumber",
        ];
        methods
            .into_iter()
            .zip(&trace["records"].as_array().unwrap()[13..20])
            .map(|(method, row)| {
                assert_eq!(row["method"], method);
                (
                    method,
                    row["response"]["result"].clone(),
                    Some(row["params"].clone()),
                )
            })
            .collect()
    }

    fn case() -> CaseKey {
        CaseKey::new(Network::Base, "0xbc8a7388c6cba5ad74b24627f58f9bd3affd5b07").unwrap()
    }

    #[test]
    fn historical_collection_read_uses_exact_parameters_seven_calls_and_qualified_context_only() {
        let (receipt, fees, _, _) = fixture();
        let canonical = json!({"hash":receipt["blockHash"]});
        for same in [false, true] {
            let mut subject = case();
            if same {
                subject.address = "0xad794ad19350a1755d90d53380102dc7213b8b07".into();
            }
            let (rpc, server) = endpoint_checked(responses());
            let mut budget =
                Budget::with_compute_units(8, 3, std::time::Duration::from_secs(20), 2000);
            let contexts = read(&rpc, &subject, &receipt, &canonical, &fees, &mut budget);
            server.join().unwrap();
            assert_eq!(budget.calls_made(), 7);
            assert_eq!(budget.calls_left(), 1);
            let c = &contexts[0];
            assert_eq!(c["qualified"], true);
            assert_eq!(c["case_token_matches"], same);
            assert_eq!(c["verification_scope"], "historical_collection_context");
            assert_eq!(c["pool_attribution"], "unresolved");
            assert_eq!(c["collection_log_index"], 108);
            assert_eq!(c["context"]["positions"].as_array().unwrap().len(), 5);
            assert!(c["gap"].as_str().unwrap().contains("block-end"));
            let facts = statements(&contexts, &subject);
            assert_eq!(facts.len(), 1);
            assert!(facts[0]["text"].as_str().unwrap().contains(if same {
                "which matches case"
            } else {
                "which differs from case"
            }));
        }
    }

    #[test]
    fn historical_collection_read_refuses_runtime_dependencies_registration_and_reorgs() {
        let (receipt, fees, _, _) = fixture();
        let canonical = json!({"hash":receipt["blockHash"]});
        for change in 0..14 {
            let mut rows = responses();
            match change {
                0 => rows[0].1 = Value::Null,
                1 => rows[0].1 = json!("abcd"),
                2 => rows[0].1 = json!("0xgg"),
                3 => rows[0].1 = json!("0x00"),
                4..=6 => rows[change - 3].1 = json!(format!("0x{}", arg(FACTORY))),
                7 => {
                    let mut d = words(rows[4].1.as_str().unwrap()).unwrap();
                    d[0] = format!("0x{:064x}", 64);
                    rows[4].1 = json!(format!(
                        "0x{}",
                        d.iter()
                            .map(|s| s.trim_start_matches("0x"))
                            .collect::<String>()
                    ));
                }
                8..=9 => {
                    let mut d = words(rows[4].1.as_str().unwrap()).unwrap();
                    d[if change == 8 { 1 } else { 3 }] = format!("0x{}", arg(FACTORY));
                    rows[4].1 = json!(format!(
                        "0x{}",
                        d.iter()
                            .map(|s| s.trim_start_matches("0x"))
                            .collect::<String>()
                    ));
                }
                10 => rows[5].1 = Value::Null,
                11 => rows[6].1["hash"] = json!(format!("0x{:064x}", 1)),
                12 => rows[6].1["number"] = json!("0x1"),
                _ => rows[6].1 = Value::Null,
            }
            let (calls, gap) = match change {
                0 => (1, "runtime absent"),
                1 => (1, "runtime prefix absent"),
                2 => (1, "malformed bytes"),
                3 => (1, "runtime differs"),
                4..=6 => (change - 2, "getter differs"),
                7..=9 => (5, "registration disagrees"),
                10 => (6, "eth_call"),
                _ => (7, "checkpoint changed"),
            };
            rows.truncate(calls);
            let (rpc, server) = endpoint_checked(rows);
            let mut budget = Budget::default();
            let contexts = read(&rpc, &case(), &receipt, &canonical, &fees, &mut budget);
            server.join().unwrap();
            assert_eq!(budget.calls_made() as usize, calls);
            assert!(
                contexts[0]["gap"].as_str().unwrap().contains(gap),
                "mutation {change}: {}",
                contexts[0]
            );
            assert_eq!(contexts[0]["qualified"], false, "mutation {change}");
            assert_eq!(
                contexts[0]["verification_scope"],
                "unavailable_collection_context"
            );
            assert!(contexts[0]["context"].is_null());
            assert_eq!(statements(&contexts, &case()), [] as [Value; 0]);
        }
    }

    #[test]
    fn collection_admission_preserves_budgets_chain_and_single_credit_bound() {
        let (receipt, mut fees, _, _) = fixture();
        let canonical = json!({"hash":receipt["blockHash"]});
        let (rpc, server) = endpoint_checked(Vec::new());
        for count in [0, 7] {
            let mut budget =
                Budget::with_compute_units(count, 3, std::time::Duration::from_secs(20), 2000);
            let result = read(&rpc, &case(), &receipt, &canonical, &fees, &mut budget);
            assert_eq!(budget.calls_made(), 0);
            assert!(
                result[0]["gap"]
                    .as_str()
                    .unwrap()
                    .contains("insufficient shared calls")
            );
        }
        for network in [Network::Solana, Network::Ethereum] {
            let mut subject = case();
            subject.chain = network;
            assert_eq!(
                read(
                    &rpc,
                    &subject,
                    &receipt,
                    &canonical,
                    &fees,
                    &mut Budget::default()
                ),
                [] as [Value; 0]
            );
        }
        fees.coverage_complete = false;
        assert!(
            read(
                &rpc,
                &case(),
                &receipt,
                &canonical,
                &fees,
                &mut Budget::default()
            )[0]["gap"]
                .as_str()
                .unwrap()
                .contains("complete receipt")
        );
        fees.coverage_complete = true;
        fees.credits.push(fees.credits[0].clone());
        assert!(
            read(
                &rpc,
                &case(),
                &receipt,
                &canonical,
                &fees,
                &mut Budget::default()
            )[0]["gap"]
                .as_str()
                .unwrap()
                .contains("one credit")
        );
        fees.credits.clear();
        assert_eq!(
            read(
                &rpc,
                &case(),
                &receipt,
                &canonical,
                &fees,
                &mut Budget::default()
            ),
            [] as [Value; 0]
        );
        server.join().unwrap();
    }

    #[test]
    fn collection_layout_requires_unique_ordered_event_and_exact_bounded_reward_arrays() {
        let (receipt, fees, _, _) = fixture();
        let c = &fees.credits[0];
        let event = collection(&receipt, c).unwrap();
        assert_eq!(event.index, 108);
        assert_eq!(event.token, "0xad794ad19350a1755d90d53380102dc7213b8b07");
        assert_eq!(event.rewards, [vec![29_478_578_528_827], vec![0]]);
        let at = receipt["logs"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["topics"][0] == COLLECTED)
            .unwrap();
        for change in 0..13 {
            let mut bad = receipt.clone();
            match change {
                0 => bad["logs"][at]["address"] = json!(CLANKER),
                1 => bad["logs"][at]["topics"][0] = json!(MODIFY),
                2 => bad["logs"][at]["logIndex"] = json!("0x69"),
                3 => bad["logs"][at]["removed"] = json!(true),
                4 => bad["logs"][at]["topics"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("extra")),
                5 => bad["logs"][at]["topics"][1] = json!("0xbad"),
                6 => bad["logs"][at]["data"] = json!("0x"),
                7 => replace_word(&mut bad["logs"][at], 4, &format!("0x{:064x}", 0)),
                8 => replace_word(&mut bad["logs"][at], 2, &format!("0x{:064x}", 96)),
                9 => replace_word(&mut bad["logs"][at], 3, &format!("0x{:064x}", 128)),
                10 => replace_word(&mut bad["logs"][at], 6, &format!("0x{:064x}", 2)),
                11 => replace_word(&mut bad["logs"][at], 7, &format!("0x{:064x}", 1)),
                _ => {
                    let duplicate = bad["logs"][at].clone();
                    bad["logs"].as_array_mut().unwrap().push(duplicate);
                }
            }
            assert!(collection(&bad, c).is_err(), "mutation {change}");
        }
        let mut unfunded = c.clone();
        unfunded["funding"] = Value::Null;
        assert!(
            collection(&receipt, &unfunded)
                .unwrap_err()
                .contains("matched deposit")
        );
        for n in [2, 16, 17] {
            let mut modified = receipt.clone();
            let mut data = vec![0u128, 0, 128, ((5 + n) * 32) as u128, n as u128];
            data.extend(vec![0; n]);
            data.push(n as u128);
            data.extend(vec![0; n]);
            modified["logs"][at]["data"] = json!(encoded(&data));
            assert_eq!(collection(&modified, c).is_ok(), n != 17);
            if n == 2 {
                replace_word(
                    &mut modified["logs"][at],
                    5,
                    &format!("0x{:064x}", u128::MAX),
                );
                replace_word(&mut modified["logs"][at], 6, &format!("0x{:064x}", 1));
                assert!(collection(&modified, c).unwrap_err().contains("overflow"));
            }
        }
    }

    #[test]
    fn collection_recipient_slot_and_currency_are_selected_independently() {
        let (receipt, fees, deployment, mut data) = fixture();
        let mut credit = fees.credits[0].clone();
        let owner = format!("0x{}", arg(credit["fee_owner"].as_str().unwrap()));
        let other = format!("0x{}", arg(CLANKER));
        data.truncate(12);
        for (index, offset) in [(9, 352), (10, 448), (11, 544)] {
            data[index] = format!("0x{offset:064x}");
        }
        data.extend([
            format!("0x{:064x}", 2),
            format!("0x{:064x}", 5000),
            format!("0x{:064x}", 5000),
            format!("0x{:064x}", 2),
            other.clone(),
            owner.clone(),
            format!("0x{:064x}", 2),
            other,
            owner.clone(),
        ]);
        let mut event = collection(&receipt, &credit).unwrap();
        event.rewards = [vec![0, 29_478_578_528_827], vec![0, 7]];
        let first = link(&receipt, &credit, &event, &deployment, &data).unwrap();
        assert_eq!(first["recipient_slot"], 1);
        assert_eq!(first["reported_requested_reward"], "29478578528827");
        credit["balance_asset"] = json!(event.token);
        credit["requested_amount"] = json!("7");
        let second = link(&receipt, &credit, &event, &deployment, &data).unwrap();
        assert_eq!(second["recipient_slot"], 1);
        assert_eq!(second["reported_requested_reward"], "7");
        data[19] = owner;
        assert!(
            link(&receipt, &credit, &event, &deployment, &data)
                .unwrap_err()
                .contains("ambiguous")
        );
        data[19] = format!("0x{}", arg(CLANKER));
        event.rewards = [vec![7], vec![7]];
        assert!(
            link(&receipt, &credit, &event, &deployment, &data)
                .unwrap_err()
                .contains("collection arrays")
        );
    }

    #[test]
    fn collection_link_checks_pool_hook_recipient_requested_amount_and_positions() {
        let (receipt, fees, deployment, data) = fixture();
        let credit = &fees.credits[0];
        let event = collection(&receipt, credit).unwrap();
        let context = link(&receipt, credit, &event, &deployment, &data).unwrap();
        assert_eq!(
            context["pool_id"],
            "0x787392934b75093f7e79e384410b4115b35265f72210020b43c9239d019a8992"
        );
        assert_eq!(context["recipient_slot"], 0);
        assert_eq!(context["positions"].as_array().unwrap().len(), 5);
        for change in 0..5 {
            let mut bad = data.clone();
            let mut c = credit.clone();
            match change {
                0 => bad[2] = format!("0x{}", arg(CLANKER)),
                1 => bad[6] = format!("0x{}", arg(CLANKER)),
                2 => c["balance_asset"] = json!(CLANKER),
                3 => c["fee_owner"] = json!(CLANKER),
                _ => c["requested_amount"] = json!("1"),
            }
            assert!(
                link(&receipt, &c, &event, &deployment, &bad).is_err(),
                "mutation {change}"
            );
        }
        let mut foreign = data.clone();
        foreign[3] = format!("0x{}", arg(CLANKER));
        assert!(
            link(&receipt, credit, &event, &deployment, &foreign)
                .unwrap_err()
                .contains("pool token")
        );
        let mut extra = event;
        extra.rewards = [vec![29_478_578_528_827, 0], vec![0, 0]];
        assert!(
            link(&receipt, credit, &extra, &deployment, &data)
                .unwrap_err()
                .contains("recipient slot")
        );
        let at = receipt["logs"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["topics"][0] == MODIFY)
            .unwrap();
        for change in 0..8 {
            let mut bad = receipt.clone();
            match change {
                0 => bad["logs"][at]["address"] = json!(CLANKER),
                1 => bad["logs"][at]["topics"][1] = json!(format!("0x{:064x}", 1)),
                2 => bad["logs"][at]["topics"][2] = json!(format!("0x{}", arg(CLANKER))),
                3 => replace_word(&mut bad["logs"][at], 2, &format!("0x{:064x}", 1)),
                4 => bad["logs"][at]["logIndex"] = json!("0x6d"),
                5 => replace_word(&mut bad["logs"][at], 3, &format!("0x{:064x}", 897_034)),
                6 => replace_word(&mut bad["logs"][at], 3, &format!("0x{:064x}", 897_040)),
                _ => replace_word(&mut bad["logs"][at + 1], 3, &format!("0x{:064x}", 897_035)),
            }
            assert!(link(&bad, credit, &extra, &deployment, &data).is_err());
            assert!(
                positions(
                    &bad,
                    credit,
                    context["pool_id"].as_str().unwrap(),
                    897_035,
                    5
                )
                .is_err(),
                "mutation {change}"
            );
        }
        for count in [0, 17] {
            assert!(
                positions(&receipt, credit, "pool", 1, count)
                    .unwrap_err()
                    .contains("bound")
            );
        }
        assert!(
            positions(&receipt, credit, "pool", u128::MAX, 1)
                .unwrap_err()
                .contains("overflow")
        );
    }
}
