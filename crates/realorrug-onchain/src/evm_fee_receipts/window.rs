// SPDX-License-Identifier: Apache-2.0
//! The transaction reader's one-block Clanker balance reconciliation.
use super::{CLAIM, CLANKER, FeeReceipts, LIMIT, STORE, credit, decode};
use crate::{
    Budget,
    cases::Network,
    evm_investigation::{call, eth_call, hex_u64, word},
};
use realorrug_robinhood::{Hash32, Rpc};
use serde_json::{Value, json};
use std::collections::HashSet;

const PERMISSIONED: &str = "0xbbedd0d146a671e5a53a384922ca83a6d07af20beeb0fee6e183ac0d98eeccae";

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    owner: String,
    asset: String,
}

// Block numbers are derived from the confirmed receipt, never TimeWindow's
// Unix seconds or a model-selected arbitrary historical RPC argument.
struct BlockWindow {
    block: u64,
    opening: u64,
    hash: String,
}

pub(crate) fn read(
    rpc: &Rpc,
    network: Network,
    receipt: &Value,
    canonical: &Value,
    fees: &FeeReceipts,
    budget: &mut Budget,
) -> Vec<Value> {
    if network != Network::Base {
        return Vec::new();
    }
    let keys: HashSet<Key> = fees
        .credits
        .iter()
        .chain(&fees.claims)
        .filter(|event| event["escrow"] == CLANKER)
        .filter_map(|event| {
            Some(Key {
                owner: event["fee_owner"].as_str()?.into(),
                asset: event["balance_asset"].as_str()?.into(),
            })
        })
        .collect();
    if keys.is_empty() {
        return Vec::new();
    }
    let result = if !fees.coverage_complete {
        Err("submitted receipt coverage is incomplete".into())
    } else if keys.len() != 1 {
        Err("more than one wallet/asset key exceeds this read's one-key bound".into())
    } else if budget.calls_left() < 9 {
        // Eight additional calls and the outer read's final canonical check.
        Err("insufficient shared calls for the balance window and final checkpoint".into())
    } else {
        reconcile(rpc, receipt, canonical, keys.iter().next().unwrap(), budget)
    };
    vec![result.unwrap_or_else(|gap: String| {
        json!({"protocol":"clanker_fee_locker","escrow":CLANKER,
            "reconciled":false,"gap":gap,"pool_attribution":"unresolved",
            "verification_scope":"unavailable_balance_window"})
    })]
}

fn header(value: &Value, block: u64) -> Result<String, String> {
    if hex_u64(value["number"].as_str().ok_or("header number absent")?)? != block {
        return Err("balance window header number disagrees".into());
    }
    let hash = value["hash"].as_str().ok_or("header hash absent")?;
    hash.parse::<Hash32>().map_err(|e| e.to_string())?;
    Ok(hash.to_lowercase())
}

fn balance(rpc: &Rpc, key: &Key, block: u64, budget: &mut Budget) -> Result<u128, String> {
    let data = format!(
        "0x8296535a{:0>64}{:0>64}",
        key.owner.trim_start_matches("0x"),
        key.asset.trim_start_matches("0x")
    );
    word(&eth_call(
        rpc,
        budget,
        CLANKER,
        &data,
        &format!("0x{block:x}"),
    )?)
}

fn reconcile(
    rpc: &Rpc,
    receipt: &Value,
    canonical: &Value,
    key: &Key,
    budget: &mut Budget,
) -> Result<Value, String> {
    let block = hex_u64(
        receipt["blockNumber"]
            .as_str()
            .ok_or("receipt block absent")?,
    )?;
    let window = BlockWindow {
        block,
        opening: block
            .checked_sub(1)
            .ok_or("genesis has no opening checkpoint")?,
        hash: header(canonical, block)?,
    };
    let opening_header = call(
        rpc,
        budget,
        "eth_getBlockByNumber",
        json!([format!("0x{:x}", window.opening), false]),
    )?;
    let opening_hash = header(&opening_header, window.opening)?;
    if canonical["parentHash"].as_str() != Some(&opening_hash) {
        return Err("balance window boundary headers are not parent-linked".into());
    }
    let opening = balance(rpc, key, window.opening, budget)?;
    let closing = balance(rpc, key, window.block, budget)?;
    let owner = format!("0x{:0>64}", key.owner.trim_start_matches("0x"));
    let asset = format!("0x{:0>64}", key.asset.trim_start_matches("0x"));
    let mut logs = Vec::new();
    for topics in [
        json!([STORE, null, owner, asset]),
        json!([CLAIM, owner, asset]),
        json!([PERMISSIONED, owner, asset]),
    ] {
        let value = call(
            rpc,
            budget,
            "eth_getLogs",
            json!([{"address":CLANKER,
            "fromBlock":format!("0x{:x}", window.block),"toBlock":format!("0x{:x}", window.block),"topics":topics}]),
        )?;
        let found = value.as_array().ok_or("balance window logs absent")?;
        if found.len() + logs.len() > LIMIT {
            return Err("balance window event bound exceeded".into());
        }
        for log in found {
            // A provider returning a different event for this filter cannot be
            // silently ignored: that would advertise nonexistent coverage.
            if log["topics"][0] != topics[0] {
                return Err("balance window query returned an unexpected signature".into());
            }
            if topics[0] == PERMISSIONED {
                return Err(
                    "permissioned claim layout is unsupported in this balance window".into(),
                );
            }
            logs.push(log.clone());
        }
    }
    let events = roll_forward(&window, key, receipt, &logs, opening, closing)?;
    for (number, expected) in [
        (window.opening, opening_hash.as_str()),
        (window.block, window.hash.as_str()),
    ] {
        let current = call(
            rpc,
            budget,
            "eth_getBlockByNumber",
            json!([format!("0x{number:x}"), false]),
        )?;
        if header(&current, number)? != expected {
            return Err("balance window checkpoint changed during the read".into());
        }
    }
    Ok(json!({"protocol":"clanker_fee_locker","escrow":CLANKER,
        "fee_owner":key.owner,"balance_asset":key.asset,"from_block":window.block,"to_block":window.block,
        "opening_block":window.opening,"opening_block_hash":opening_hash,"closing_block_hash":window.hash,
        "opening_balance":opening.to_string(),"closing_balance":closing.to_string(),"events":events,
        "reconciled":true,"verification_scope":"provider_balance_window","pool_attribution":"unresolved",
        "gap":"one wallet/asset and one block only; provider observations are not proof of complete history, backing, beneficial ownership or token revenue; opening balance origin is outside this window; claims and matching delivery are one withdrawal, never additional earnings"}))
}

fn roll_forward(
    window: &BlockWindow,
    key: &Key,
    receipt: &Value,
    logs: &[Value],
    opening: u128,
    closing: u128,
) -> Result<Vec<Value>, String> {
    let mut ordered = Vec::new();
    let mut indices = HashSet::new();
    for log in logs {
        let index = super::checkpoint(
            log,
            &json!({"transactionHash":log["transactionHash"],
            "blockHash":window.hash,"blockNumber":format!("0x{:x}", window.block)}),
        )?;
        log["transactionHash"]
            .as_str()
            .ok_or("event transaction absent")?
            .parse::<Hash32>()
            .map_err(|e| e.to_string())?;
        if !indices.insert(index) {
            return Err("duplicate balance window log index".into());
        }
        let claim = decode(log, index)?;
        let credit = credit(log, index)?;
        let matches_key = claim
            .as_ref()
            .is_some_and(|c| c.owner == key.owner && c.balance_asset == key.asset)
            || credit
                .as_ref()
                .is_some_and(|c| c["fee_owner"] == key.owner && c["balance_asset"] == key.asset);
        if !matches_key {
            return Err("balance window event emitter, owner or asset disagrees".into());
        }
        ordered.push((index, log, claim, credit));
    }
    // Include the submitted events even when offsetting omitted events would
    // accidentally leave the closing balance equal to the opening balance.
    for anchor in receipt["logs"].as_array().ok_or("receipt logs absent")? {
        if !anchor["address"]
            .as_str()
            .is_some_and(|address| address.eq_ignore_ascii_case(CLANKER))
        {
            continue;
        }
        let index = super::hex_u64(anchor["logIndex"].as_str().ok_or("anchor index absent")?)?;
        let claim = decode(anchor, index)?;
        let credit = credit(anchor, index)?;
        if (claim.is_some_and(|c| c.owner == key.owner && c.balance_asset == key.asset)
            || credit
                .is_some_and(|c| c["fee_owner"] == key.owner && c["balance_asset"] == key.asset))
            && !logs.iter().any(|log| {
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
                .all(|field| log[field] == anchor[field])
            })
        {
            return Err("submitted fee event is absent or changed in the window".into());
        }
    }
    ordered.sort_by_key(|(index, _, _, _)| *index);
    let mut current = opening;
    let mut events = Vec::new();
    for (index, log, claim, credit) in ordered {
        let before = current;
        let mut event = json!({"log_index":index,"transaction":log["transactionHash"],
            "balance_before":before.to_string()});
        if let Some(credit) = credit {
            let after = credit["reported_balance_after"]
                .as_str()
                .ok_or("credit balance absent")?
                .parse::<u128>()
                .map_err(|e| e.to_string())?;
            let delta = after
                .checked_sub(before)
                .ok_or("credit cumulative balance decreased")?;
            event["kind"] = json!("credit");
            event["credited_delta"] = json!(delta.to_string());
            event["requested_amount"] = credit["requested_amount"].clone();
            event["depositor"] = credit["depositor"].clone();
            current = after;
        } else if let Some(claim) = claim {
            if claim.amount != current {
                return Err("full-balance claim disagrees with the running balance".into());
            }
            event["kind"] = json!("claim");
            event["claimed_amount"] = json!(claim.amount.to_string());
            current = 0;
        }
        event["balance_after"] = json!(current.to_string());
        events.push(event);
    }
    if current != closing {
        return Err("events do not reconcile with the closing balance".into());
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evm_investigation::tests::endpoint_checked;

    fn key() -> Key {
        Key {
            owner: format!("0x{:040x}", 1),
            asset: format!("0x{:040x}", 2),
        }
    }
    fn window() -> BlockWindow {
        BlockWindow {
            block: 16,
            opening: 15,
            hash: format!("0x{:064x}", 16),
        }
    }
    fn header_at(block: u64) -> Value {
        json!({"number":format!("0x{block:x}"),"hash":format!("0x{block:064x}"),"parentHash":format!("0x{:064x}", block-1)})
    }
    fn event(index: u64, stored: bool, amount: u128) -> Value {
        let key = key();
        let owner = format!("0x{:0>64}", &key.owner[2..]);
        let asset = format!("0x{:0>64}", &key.asset[2..]);
        json!({"address":CLANKER,"removed":false,"blockNumber":"0x10",
            "blockHash":window().hash,"transactionHash":format!("0x{:064x}", 3),"logIndex":format!("0x{index:x}"),
            "topics":if stored {json!([STORE,format!("0x{:064x}",4),owner,asset])} else {json!([CLAIM,owner,asset])},
            "data":if stored {format!("0x{amount:064x}{:064x}",99)} else {format!("0x{amount:064x}")}})
    }
    fn receipt(logs: Vec<Value>) -> Value {
        let mut value = json!({"status":"0x1","blockNumber":"0x10","blockHash":window().hash,
            "transactionHash":format!("0x{:064x}",3)});
        value["logs"] = Value::Array(logs);
        value
    }
    fn responses(
        opening: u128,
        closing: u128,
        stores: Value,
        claims: Value,
        permissioned: Value,
    ) -> Vec<(&'static str, Value, Option<Value>)> {
        let key = key();
        let getter = format!("0x8296535a{:0>64}{:0>64}", &key.owner[2..], &key.asset[2..]);
        let owner = format!("0x{:0>64}", &key.owner[2..]);
        let asset = format!("0x{:0>64}", &key.asset[2..]);
        let mut rows = vec![(
            "eth_getBlockByNumber",
            header_at(15),
            Some(json!(["0xf", false])),
        )];
        for (block, balance) in [("0xf", opening), ("0x10", closing)] {
            rows.push((
                "eth_call",
                json!(format!("0x{balance:064x}")),
                Some(json!([{"to":CLANKER,"data":getter},block])),
            ));
        }
        for (topics, logs) in [
            (json!([STORE, null, owner, asset]), stores),
            (json!([CLAIM, owner, asset]), claims),
            (json!([PERMISSIONED, owner, asset]), permissioned),
        ] {
            rows.push(("eth_getLogs",logs,Some(json!([{"address":CLANKER,"fromBlock":"0x10","toBlock":"0x10","topics":topics}]))));
        }
        rows.push((
            "eth_getBlockByNumber",
            header_at(15),
            Some(json!(["0xf", false])),
        ));
        rows.push((
            "eth_getBlockByNumber",
            header_at(16),
            Some(json!(["0x10", false])),
        ));
        rows
    }

    #[test]
    fn balance_rollforward_orders_credits_and_claims_without_counting_requested_or_delivery_twice()
    {
        let stored = event(1, true, 15);
        let claimed = event(2, false, 15);
        let logs = vec![claimed.clone(), stored.clone()];
        let result = roll_forward(&window(), &key(), &receipt(logs.clone()), &logs, 10, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["kind"], "credit");
        assert_eq!(result[0]["balance_before"], "10");
        assert_eq!(result[0]["balance_after"], "15");
        assert_eq!(result[0]["credited_delta"], "5");
        assert_eq!(result[0]["requested_amount"], "99");
        assert_eq!(result[1]["claimed_amount"], "15");
        assert_eq!(result[1]["balance_after"], "0");
        let unrelated = json!({"address":super::super::FLAUNCH,"logIndex":"0x3",
            "topics":[super::super::WITHDRAWAL],"data":format!("0x{:064x}{:064x}{:064x}{:064x}{:064x}",1,1,2,2,10)});
        let mixed_receipt = receipt(vec![stored.clone(), claimed.clone(), unrelated]);
        assert_eq!(
            roll_forward(&window(), &key(), &mixed_receipt, &logs, 10, 0).unwrap(),
            result
        );
        assert_eq!(
            roll_forward(&window(), &key(), &receipt(vec![]), &[], 10, 10).unwrap(),
            [] as [Value; 0]
        );
        let unchanged = event(1, true, 10);
        let result = roll_forward(
            &window(),
            &key(),
            &receipt(vec![unchanged.clone()]),
            &[unchanged],
            10,
            10,
        )
        .unwrap();
        assert_eq!(result[0]["credited_delta"], "0");
    }

    #[test]
    fn balance_rollforward_refuses_bad_identity_missing_anchors_and_inconsistent_arithmetic() {
        let stored = event(1, true, 15);
        for (field, value) in [
            ("removed", json!(true)),
            ("removed", Value::Null),
            ("blockHash", json!(format!("0x{:064x}", 17))),
            ("blockNumber", json!("0x11")),
            ("address", json!(key().asset)),
            ("transactionHash", json!("not-a-hash")),
            ("logIndex", Value::Null),
            ("topics", json!([STORE])),
            ("data", json!("0x")),
            (
                "topics",
                json!([
                    STORE,
                    format!("0x{:064x}", 4),
                    format!("0x{:064x}", 9),
                    format!("0x{:064x}", 2)
                ]),
            ),
            (
                "topics",
                json!([CLAIM, format!("0x{:064x}", 1), format!("0x{:064x}", 9)]),
            ),
        ] {
            let mut bad = stored.clone();
            bad[field] = value;
            assert!(
                roll_forward(&window(), &key(), &receipt(vec![]), &[bad], 10, 15).is_err(),
                "{field}"
            );
        }
        let good_receipt = receipt(vec![stored.clone()]);
        for (logs, opening, closing) in [
            (vec![stored.clone(), stored.clone()], 10, 15),
            (vec![], 10, 10),
            (vec![event(1, true, 9)], 10, 9),
            (vec![event(1, false, 11)], 10, 0),
            (vec![stored.clone()], 10, 16),
            (vec![event(1, true, 15)], 16, 15),
        ] {
            assert!(
                roll_forward(&window(), &key(), &good_receipt, &logs, opening, closing).is_err()
            );
        }
        let wrong_claim = event(1, false, 11);
        assert!(
            roll_forward(
                &window(),
                &key(),
                &receipt(vec![wrong_claim.clone()]),
                &[wrong_claim],
                10,
                0
            )
            .unwrap_err()
            .contains("full-balance")
        );
        let falling = event(1, true, 9);
        assert!(
            roll_forward(
                &window(),
                &key(),
                &receipt(vec![falling.clone()]),
                &[falling],
                10,
                9
            )
            .unwrap_err()
            .contains("decreased")
        );
    }

    #[test]
    fn transaction_window_uses_exact_key_filters_getters_and_canonical_boundaries() {
        let stored = event(1, true, 15);
        let receipt = receipt(vec![stored.clone()]);
        let fees = super::super::read(Network::Base, &receipt);
        let (rpc, server) =
            endpoint_checked(responses(10, 15, json!([stored]), json!([]), json!([])));
        let mut budget = Budget::default();
        let result = read(
            &rpc,
            Network::Base,
            &receipt,
            &header_at(16),
            &fees,
            &mut budget,
        );
        server.join().unwrap();
        assert_eq!(budget.calls_made(), 8);
        assert_eq!(result[0]["reconciled"], true);
        assert_eq!(result[0]["opening_block"], 15);
        assert_eq!(result[0]["from_block"], 16);
        assert_eq!(result[0]["to_block"], 16);
        assert_eq!(result[0]["opening_block_hash"], header_at(15)["hash"]);
        assert_eq!(result[0]["closing_block_hash"], header_at(16)["hash"]);
        assert_eq!(result[0]["opening_balance"], "10");
        assert_eq!(result[0]["closing_balance"], "15");
        assert_eq!(result[0]["fee_owner"], key().owner);
        assert_eq!(result[0]["balance_asset"], key().asset);
        assert_eq!(result[0]["events"][0]["credited_delta"], "5");
        assert_eq!(result[0]["pool_attribution"], "unresolved");
        assert_eq!(result[0]["verification_scope"], "provider_balance_window");
        assert!(result[0]["financial_state"].is_null());
        assert!(
            result[0]["gap"]
                .as_str()
                .unwrap()
                .contains("opening balance origin")
        );
        let full_bound: Vec<Value> = (1..=128).map(|index| event(index, true, 15)).collect();
        let (rpc, server) =
            endpoint_checked(responses(10, 15, json!(full_bound), json!([]), json!([])));
        let result = read(
            &rpc,
            Network::Base,
            &receipt,
            &header_at(16),
            &fees,
            &mut Budget::default(),
        );
        server.join().unwrap();
        assert_eq!(result[0]["reconciled"], true);
        assert_eq!(result[0]["events"].as_array().unwrap().len(), 128);
    }

    #[test]
    fn balance_window_refuses_caps_permissioned_events_wrong_filters_and_reorgs() {
        let stored = event(1, true, 15);
        let receipt = receipt(vec![stored.clone()]);
        let fees = super::super::read(Network::Base, &receipt);
        for scenario in 0..10 {
            let mut rows = responses(10, 15, json!([stored.clone()]), json!([]), json!([]));
            let mut canonical = header_at(16);
            match scenario {
                0 => {
                    rows[3].1 = json!(vec![stored.clone(); 129]);
                    rows.truncate(4);
                }
                1 => {
                    rows[5].1 = json!([{"topics":[PERMISSIONED]}]);
                    rows.truncate(6);
                }
                2 => {
                    rows[3].1 = json!([event(1, false, 10)]);
                    rows.truncate(4);
                }
                3 => {
                    rows[6].1 = header_at(14);
                    rows.truncate(7);
                }
                4 => rows[7].1["hash"] = json!(format!("0x{:064x}", 17)),
                5 => {
                    canonical["parentHash"] = json!(format!("0x{:064x}", 14));
                    rows.truncate(1);
                }
                6 => {
                    rows[1].1 = json!("0x");
                    rows.truncate(2);
                }
                7 => {
                    rows[3].1 = Value::Null;
                    rows.truncate(4);
                }
                8 => {
                    rows[3].1 = json!(vec![stored.clone(); 128]);
                    rows[4].1 = json!([event(2, false, 15)]);
                    rows.truncate(5);
                }
                _ => {
                    canonical["number"] = json!("0x11");
                    rows.clear();
                }
            }
            let (rpc, server) = endpoint_checked(rows);
            let result = read(
                &rpc,
                Network::Base,
                &receipt,
                &canonical,
                &fees,
                &mut Budget::default(),
            );
            server.join().unwrap();
            assert_eq!(result[0]["reconciled"], false, "{scenario}");
            assert_eq!(
                result[0]["verification_scope"],
                "unavailable_balance_window"
            );
            assert!(result[0]["events"].is_null());
        }
    }

    #[test]
    fn balance_window_admission_preserves_shared_limits_and_deduplicates_the_same_key() {
        let receipt = receipt(vec![event(1, true, 15), event(2, false, 15)]);
        let mut fees = super::super::read(Network::Base, &receipt);
        let (rpc, server) = endpoint_checked(vec![]);
        for network in [Network::Solana, Network::Ethereum] {
            assert_eq!(
                read(
                    &rpc,
                    network,
                    &receipt,
                    &header_at(16),
                    &fees,
                    &mut Budget::default()
                ),
                [] as [Value; 0]
            );
        }
        for calls in [0, 8] {
            let mut budget = Budget::new(calls, 3, std::time::Duration::from_secs(20));
            let result = read(
                &rpc,
                Network::Base,
                &receipt,
                &header_at(16),
                &fees,
                &mut budget,
            );
            assert!(
                result[0]["gap"]
                    .as_str()
                    .unwrap()
                    .contains("insufficient shared calls")
            );
            assert_eq!(budget.calls_made(), 0);
        }
        fees.coverage_complete = false;
        assert!(
            read(
                &rpc,
                Network::Base,
                &receipt,
                &header_at(16),
                &fees,
                &mut Budget::default()
            )[0]["gap"]
                .as_str()
                .unwrap()
                .contains("receipt coverage")
        );
        fees.coverage_complete = true;
        let mut second = fees.credits[0].clone();
        second["fee_owner"] = json!(format!("0x{:040x}", 9));
        fees.credits.push(second);
        assert!(
            read(
                &rpc,
                Network::Base,
                &receipt,
                &header_at(16),
                &fees,
                &mut Budget::default()
            )[0]["gap"]
                .as_str()
                .unwrap()
                .contains("one-key bound")
        );
        assert_eq!(
            read(
                &rpc,
                Network::Base,
                &receipt,
                &header_at(16),
                &FeeReceipts::default(),
                &mut Budget::default()
            ),
            [] as [Value; 0]
        );
        server.join().unwrap();
        let fees = super::super::read(Network::Base, &receipt);
        let (rpc, server) = endpoint_checked(responses(
            10,
            0,
            json!([event(1, true, 15)]),
            json!([event(2, false, 15)]),
            json!([]),
        ));
        let mut budget = Budget::new(9, 3, std::time::Duration::from_secs(20));
        let result = read(
            &rpc,
            Network::Base,
            &receipt,
            &header_at(16),
            &fees,
            &mut budget,
        );
        server.join().unwrap();
        assert_eq!(result[0]["reconciled"], true);
        assert_eq!(budget.calls_left(), 1);
        assert_eq!(result[0]["events"].as_array().unwrap().len(), 2);
    }
}
