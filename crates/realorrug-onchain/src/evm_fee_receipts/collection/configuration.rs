// SPDX-License-Identifier: Apache-2.0
//! Bounded reward history; changes inside the collection transaction stay unknown.
use super::{COLLECTED, LOCKER, address_word, arg, call, number, query};
use crate::{Budget, evm_fee_receipts::window::header, evm_investigation::hex_u64};
use realorrug_robinhood::Rpc;
use serde_json::{Value, json};

mod history;

pub(super) fn read(
    rpc: &Rpc,
    receipt: &Value,
    canonical: &Value,
    closing_code: &Value,
    closing_tuple: &[String],
    budget: &mut Budget,
) -> Value {
    let result = if budget.calls_left() < 7 {
        Err("insufficient shared calls for reward configuration and final checkpoint".into())
    } else {
        qualify(rpc, receipt, canonical, closing_code, closing_tuple, budget)
    };
    result.unwrap_or_else(|gap: String| {
        json!({"qualified":false,"verification_scope":"unavailable_reward_configuration","gap":gap})
    })
}

fn qualify(
    rpc: &Rpc,
    receipt: &Value,
    canonical: &Value,
    closing_code: &Value,
    closing_tuple: &[String],
    budget: &mut Budget,
) -> Result<Value, String> {
    if number(closing_tuple, 7)? == 0 {
        return Err("uninitialized position cannot establish stable reward configuration".into());
    }
    let block = hex_u64(
        receipt["blockNumber"]
            .as_str()
            .ok_or("receipt block absent")?,
    )?;
    let opening = block
        .checked_sub(1)
        .ok_or("genesis has no opening checkpoint")?;
    let closing_hash = header(canonical, block)?;
    let tag = format!("0x{opening:x}");
    let parent = call(rpc, budget, "eth_getBlockByNumber", json!([tag, false]))?;
    let opening_hash = header(&parent, opening)?;
    if canonical["parentHash"].as_str() != Some(opening_hash.as_str()) {
        return Err("reward configuration headers are not parent-linked".into());
    }
    let code = call(rpc, budget, "eth_getCode", json!([LOCKER, tag]))?;
    if code != *closing_code {
        return Err("parent locker runtime differs from the qualified closing runtime".into());
    }
    let token = address_word(closing_tuple.get(1).ok_or("reward token absent")?)?;
    let tuple = query(
        rpc,
        budget,
        LOCKER,
        "tokenRewards(address)",
        &arg(&token),
        &tag,
    )?;
    let logs = call(
        rpc,
        budget,
        "eth_getLogs",
        json!([{"address":LOCKER,
        "fromBlock":format!("0x{block:x}"),"toBlock":format!("0x{block:x}")}]),
    )?;
    let quiet = logs.as_array().is_some_and(|logs| logs.len() == 1);
    let (anchor, reward_tuple, transitions) = if quiet {
        let anchor = only_collection(&logs, receipt)?;
        if tuple != closing_tuple {
            return Err(
                "parent and closing reward tuples differ without a supported event history".into(),
            );
        }
        (anchor, tuple, json!([]))
    } else {
        history::reconstruct(&logs, receipt, &tuple, closing_tuple)?
    };
    for (number, hash) in [
        (opening, opening_hash.as_str()),
        (block, closing_hash.as_str()),
    ] {
        let current = call(
            rpc,
            budget,
            "eth_getBlockByNumber",
            json!([format!("0x{number:x}"), false]),
        )?;
        if header(&current, number)? != hash {
            return Err("reward configuration checkpoint changed during the read".into());
        }
    }
    if !quiet {
        return Ok(
            json!({"qualified":true,"verification_scope":"provider_ordered_reward_configuration",
        "opening_block":opening,"opening_block_hash":opening_hash,"closing_block":block,
        "closing_block_hash":closing_hash,"collection_log_index":anchor,
        "locker_events":logs.as_array().unwrap().len(),"reward_tuple":reward_tuple,
        "transitions":transitions,"gap":"bounded provider event history reconciles parent and closing reward tuples; reviewed runtime/source imply recipient/admin writes emit events; no change in the collection transaction is admitted; this is not independent completeness proof, registry history, fee-preference history, conversion correctness, backing or per-token revenue"}),
        );
    }
    Ok(
        json!({"qualified":true,"verification_scope":"provider_quiet_block_reward_configuration",
        "opening_block":opening,"opening_block_hash":opening_hash,"closing_block":block,
        "closing_block_hash":closing_hash,"collection_log_index":anchor,"locker_events":1,
        "reward_tuple":closing_tuple,"gap":"provider reports exactly one locker event in this block; reviewed runtime/source imply reward writes emit events and existing positions prevent reinitialization; this is not independent completeness proof, registry history, fee-preference history, conversion correctness, backing or per-token revenue"}),
    )
}

fn only_collection(value: &Value, receipt: &Value) -> Result<u64, String> {
    let logs = value.as_array().ok_or("locker block events absent")?;
    // This helper anchors one collection. The history decoder separately checks
    // additional updates, including a change followed by its reversal.
    if logs.len() != 1 {
        return Err("quiet-block check requires exactly one locker event; changes, additional collections or missing coverage remain unresolved".into());
    }
    let log = &logs[0];
    if !log["address"]
        .as_str()
        .is_some_and(|a| a.eq_ignore_ascii_case(LOCKER))
        || log["topics"][0] != COLLECTED
    {
        return Err("locker query returned a different emitter or signature".into());
    }
    let index = super::super::checkpoint(log, receipt)?;
    if receipt["logs"]
        .as_array()
        .ok_or("receipt logs absent")?
        .iter()
        .filter(|log| {
            log["address"]
                .as_str()
                .is_some_and(|a| a.eq_ignore_ascii_case(LOCKER))
        })
        .count()
        > 1
    {
        return Err("additional locker events in the collection receipt remain unresolved".into());
    }
    receipt["logs"]
        .as_array()
        .ok_or("receipt logs absent")?
        .iter()
        .find(|anchor| {
            [
                "address",
                "topics",
                "data",
                "removed",
                "transactionHash",
                "blockHash",
                "blockNumber",
                "logIndex",
            ]
            .iter()
            .all(|field| anchor[field] == log[field])
        })
        .ok_or("submitted collection is absent or changed in the block query")?;
    // The parent reader already checks this exact anchor's layout and collection order.
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evm_investigation::tests::endpoint_checked;

    pub(super) fn fixture() -> (Value, Value, Value, Vec<String>) {
        let trace: Value = serde_json::from_str(include_str!(
            "../../../../../docs/research/data/0075-base/clanker-configuration-qualification.json"
        ))
        .unwrap();
        let (receipt, _, _, tuple) = super::super::tests::fixture();
        let canonical = trace["records"][1]["response"]["result"].clone();
        let code = trace["records"][3]["response"]["result"].clone();
        (receipt, canonical, code, tuple)
    }

    pub(super) fn responses() -> Vec<(&'static str, Value, Option<Value>)> {
        let trace: Value = serde_json::from_str(include_str!(
            "../../../../../docs/research/data/0075-base/clanker-configuration-qualification.json"
        ))
        .unwrap();
        [
            "eth_getBlockByNumber",
            "eth_getCode",
            "eth_call",
            "eth_getLogs",
            "eth_getBlockByNumber",
            "eth_getBlockByNumber",
        ]
        .into_iter()
        .zip(&trace["records"].as_array().unwrap()[2..8])
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

    #[test]
    fn quiet_block_configuration_uses_six_exact_calls_and_preserves_final_budget() {
        let (receipt, canonical, code, tuple) = fixture();
        let (rpc, server) = endpoint_checked(responses());
        let mut budget = Budget::with_compute_units(7, 3, std::time::Duration::from_secs(20), 2000);
        let result = read(&rpc, &receipt, &canonical, &code, &tuple, &mut budget);
        server.join().unwrap();
        assert_eq!(budget.calls_made(), 6);
        assert_eq!(budget.calls_left(), 1);
        assert_eq!(result["qualified"], true);
        assert_eq!(
            result["verification_scope"],
            "provider_quiet_block_reward_configuration"
        );
        assert_eq!(result["collection_log_index"], 108);
        assert_eq!(result["opening_block"], 52_139_194);
        assert_eq!(result["closing_block"], 52_139_195);
        assert_eq!(result["reward_tuple"], json!(tuple));
        assert!(
            result["gap"]
                .as_str()
                .unwrap()
                .contains("not independent completeness proof")
        );
    }

    #[test]
    fn quiet_block_configuration_refuses_changed_state_events_and_checkpoints() {
        let (receipt, canonical, code, tuple) = fixture();
        for change in 0..13 {
            let mut rows = responses();
            let mut closing = canonical.clone();
            let (calls, gap) = match change {
                0 => {
                    rows[0].1["number"] = json!("0x1");
                    (1, "header number")
                }
                1 => {
                    rows[0].1["hash"] = json!("malformed");
                    (1, "not hex")
                }
                2 => {
                    closing["parentHash"] = json!(format!("0x{:064x}", 1));
                    (1, "parent-linked")
                }
                3 => {
                    rows[1].1 = Value::Null;
                    (2, "runtime differs")
                }
                4 => {
                    rows[1].1 = json!("0x00");
                    (2, "runtime differs")
                }
                5 => {
                    rows[2].1 = json!("0x00");
                    (3, "ABI")
                }
                6 => {
                    rows[2].1 = json!(format!(
                        "0x{}",
                        tuple
                            .iter()
                            .enumerate()
                            .map(|(i, w)| if i == 17 {
                                format!("{:064x}", 1)
                            } else {
                                w.trim_start_matches("0x").to_owned()
                            })
                            .collect::<String>()
                    ));
                    (4, "tuples differ")
                }
                7 => {
                    rows[3].1 = Value::Null;
                    (4, "events absent")
                }
                8 => {
                    rows[3].1 = json!([]);
                    (4, "missing coverage")
                }
                9 => {
                    let extra = rows[3].1[0].clone();
                    rows[3].1.as_array_mut().unwrap().push(extra);
                    (4, "repeated or out of order")
                }
                10 => {
                    rows[4].1["hash"] = json!(format!("0x{:064x}", 1));
                    (5, "checkpoint changed")
                }
                11 => {
                    rows[5].1["hash"] = json!(format!("0x{:064x}", 1));
                    (6, "checkpoint changed")
                }
                _ => {
                    rows[5].1["number"] = json!("0x1");
                    (6, "header number")
                }
            };
            rows.truncate(calls);
            let (rpc, server) = endpoint_checked(rows);
            let mut budget = Budget::default();
            let result = read(&rpc, &receipt, &closing, &code, &tuple, &mut budget);
            server.join().unwrap();
            assert_eq!(budget.calls_made() as usize, calls);
            assert_eq!(result["qualified"], false);
            assert_eq!(
                result["verification_scope"],
                "unavailable_reward_configuration"
            );
            assert!(
                result["gap"].as_str().unwrap().contains(gap),
                "change {change}: {result}"
            );
            assert!(result["reward_tuple"].is_null());
        }
    }

    #[test]
    fn quiet_block_requires_the_exact_collection_anchor_and_refuses_round_trip_changes() {
        let (receipt, _, _, _) = fixture();
        let logs = responses()[3].1.clone();
        assert_eq!(only_collection(&logs, &receipt).unwrap(), 108);
        for field in [
            "address",
            "topics",
            "data",
            "removed",
            "transactionHash",
            "blockHash",
            "blockNumber",
            "logIndex",
        ] {
            let mut changed = logs.clone();
            changed[0][field] = Value::Null;
            assert!(
                only_collection(&changed, &receipt).is_err(),
                "field {field}"
            );
        }
        let mut unknown = logs.clone();
        unknown[0]["topics"][0] = json!(format!("0x{:064x}", 1));
        let mut altered_receipt = receipt.clone();
        let at = altered_receipt["logs"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["topics"][0] == COLLECTED)
            .unwrap();
        altered_receipt["logs"][at] = unknown[0].clone();
        assert!(
            only_collection(&unknown, &altered_receipt)
                .unwrap_err()
                .contains("signature")
        );
        unknown = logs.clone();
        unknown[0]["address"] = json!(super::super::POOL_MANAGER);
        altered_receipt["logs"][at] = unknown[0].clone();
        assert!(
            only_collection(&unknown, &altered_receipt)
                .unwrap_err()
                .contains("emitter")
        );
        let mut round_trip = logs.clone();
        for index in [109, 110] {
            let mut update = logs[0].clone();
            update["logIndex"] = json!(format!("0x{index:x}"));
            update["topics"][0] = json!("reward recipient update then reversal");
            round_trip.as_array_mut().unwrap().push(update);
        }
        assert!(
            only_collection(&round_trip, &receipt)
                .unwrap_err()
                .contains("exactly one")
        );
        let mut missing = receipt;
        missing["logs"] = json!([]);
        assert!(
            only_collection(&logs, &missing)
                .unwrap_err()
                .contains("absent or changed")
        );
    }

    #[test]
    fn quiet_block_admission_preserves_budget_and_refuses_genesis_and_uninitialized_positions() {
        let (receipt, canonical, code, tuple) = fixture();
        let (rpc, server) = endpoint_checked(Vec::new());
        for calls in [0, 6] {
            let mut budget =
                Budget::with_compute_units(calls, 3, std::time::Duration::from_secs(20), 2000);
            let result = read(&rpc, &receipt, &canonical, &code, &tuple, &mut budget);
            assert_eq!(budget.calls_made(), 0);
            assert!(
                result["gap"]
                    .as_str()
                    .unwrap()
                    .contains("insufficient shared calls")
            );
        }
        let mut genesis = receipt.clone();
        genesis["blockNumber"] = json!("0x0");
        assert!(
            read(
                &rpc,
                &genesis,
                &canonical,
                &code,
                &tuple,
                &mut Budget::default()
            )["gap"]
                .as_str()
                .unwrap()
                .contains("genesis")
        );
        let mut uninitialized = tuple;
        uninitialized[7] = format!("0x{:064x}", 0);
        assert!(
            read(
                &rpc,
                &receipt,
                &canonical,
                &code,
                &uninitialized,
                &mut Budget::default()
            )["gap"]
                .as_str()
                .unwrap()
                .contains("uninitialized")
        );
        server.join().unwrap();
    }
}
