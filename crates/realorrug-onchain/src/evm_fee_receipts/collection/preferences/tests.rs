// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::evm_investigation::tests::endpoint_checked;

fn fixture() -> (Value, String, Value) {
    let capture: Value = serde_json::from_str(include_str!(
        "../../../../../../docs/research/data/0076-base/base-clanker-ordered-configuration-regression.capture.json"
    )).unwrap();
    let context = &capture["reads"][0]["observation"]["value"]["fee_collection_contexts"][0];
    (
        context["reward_configuration"].clone(),
        context["reported_token"].as_str().unwrap().to_owned(),
        context["context"].clone(),
    )
}

fn responses() -> Vec<(&'static str, Value, Option<Value>)> {
    let trace: Value = serde_json::from_str(include_str!(
        "../../../../../../docs/research/data/0077-base/clanker-fee-preference-qualification.json"
    ))
    .unwrap();
    [
        "eth_call",
        "eth_call",
        "eth_getBlockByNumber",
        "eth_getBlockByNumber",
    ]
    .into_iter()
    .zip(&trace["records"].as_array().unwrap()[1..5])
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
fn fee_preference_stability_reserves_the_final_call_and_distinguishes_denominations() {
    let (configuration, token, context) = fixture();
    let paired = "0x4200000000000000000000000000000000000006";
    for mode in [0, 1, 2] {
        for slot in [0, 15] {
            let mut config = configuration.clone();
            let mut context = context.clone();
            context["recipient_slot"] = json!(slot);
            if slot == 15 {
                config["verification_scope"] = json!("provider_ordered_reward_configuration");
                context["currencies"].as_array_mut().unwrap().reverse();
            }
            let mut rows = responses();
            for row in &mut rows[..2] {
                row.1 = json!(format!("0x{mode:064x}"));
                let data = row.2.as_ref().unwrap()[0]["data"].as_str().unwrap();
                row.2.as_mut().unwrap()[0]["data"] =
                    json!(format!("{}{slot:064x}", &data[..data.len() - 64]));
            }
            let (rpc, server) = endpoint_checked(rows);
            let mut budget =
                Budget::with_compute_units(5, 3, std::time::Duration::from_secs(20), 2000);
            let result = read(&rpc, &config, &token, &context, &mut budget).unwrap();
            server.join().unwrap();
            assert_eq!(budget.calls_made(), 4);
            assert_eq!(budget.calls_left(), 1);
            assert_eq!(result["qualified"], true, "{result}");
            assert_eq!(
                result["verification_scope"],
                "provider_single_slot_fee_preference_stability"
            );
            assert_eq!(result["recipient_slot"], slot);
            assert_eq!(result["configuration_value"], mode);
            assert_eq!(result["opening_block"], 52_139_194);
            assert_eq!(result["closing_block"], 52_139_195);
            assert_eq!(result["mode"], ["both", "paired", "token"][mode]);
            let assets = match mode {
                0 => context["currencies"].clone(),
                1 => json!([paired]),
                _ => json!([token]),
            };
            assert_eq!(result["requested_assets"], assets);
            assert!(
                result["gap"]
                    .as_str()
                    .unwrap()
                    .contains("not independent completeness proof")
            );
            assert!(
                result["gap"]
                    .as_str()
                    .unwrap()
                    .contains("conversion correctness")
            );
            let collection = json!({"qualified":true,"reported_token":token,
                "case_token_matches":false,"context":context,"reward_configuration":config,
                "fee_preference":result});
            let facts = super::super::statements(&[collection], &super::super::tests::case());
            let phrase = [
                "both pool assets",
                "the paired pool asset",
                "the collection token",
            ][mode];
            assert!(
                facts[0]["text"]
                    .as_str()
                    .unwrap()
                    .contains(&format!("requests fees in {phrase}"))
            );
            assert!(
                facts[0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("not proof of a realized swap")
            );
        }
    }
}

#[test]
fn fee_preference_refuses_changed_values_unknown_enums_and_checkpoints() {
    let (config, token, context) = fixture();
    for change in 0..10 {
        let mut rows = responses();
        let (calls, gap) = match change {
            0 => {
                rows[0].1 = Value::Null;
                (1, "eth_call")
            }
            1 => {
                rows[0].1 = json!("0x");
                (1, "ABI")
            }
            2 => {
                rows[0].1 = json!(format!("0x{:0128x}", 1));
                (1, "exactly one")
            }
            3 => {
                rows[0].1 = json!(format!("0x{:064x}", 0));
                (2, "preferences differ")
            }
            4 => {
                for row in &mut rows[..2] {
                    row.1 = json!(format!("0x{:064x}", 3));
                }
                (2, "unsupported fee preference enum")
            }
            5 => {
                rows[1].1 = Value::Null;
                (2, "eth_call")
            }
            6 => {
                rows[2].1["hash"] = json!(format!("0x{:064x}", 1));
                (3, "checkpoint changed")
            }
            7 => {
                rows[2].1["number"] = json!("0x1");
                (3, "header number")
            }
            8 => {
                rows[3].1["hash"] = json!(format!("0x{:064x}", 1));
                (4, "checkpoint changed")
            }
            _ => {
                rows[3].1["number"] = json!("0x1");
                (4, "header number")
            }
        };
        rows.truncate(calls);
        let (rpc, server) = endpoint_checked(rows);
        let mut budget = Budget::with_compute_units(5, 3, std::time::Duration::from_secs(20), 2000);
        let result = read(&rpc, &config, &token, &context, &mut budget).unwrap();
        server.join().unwrap();
        assert_eq!(budget.calls_made() as usize, calls);
        assert_eq!(result["qualified"], false);
        assert_eq!(
            result["verification_scope"],
            "unavailable_single_slot_fee_preference"
        );
        assert!(
            result["gap"].as_str().unwrap().contains(gap),
            "change {change}: {result}"
        );
        assert!(result["mode"].is_null());
        assert!(result["requested_assets"].is_null());
    }
}

#[test]
fn fee_preference_admission_preserves_budget_and_requires_supported_reward_history() {
    let (config, token, context) = fixture();
    let (rpc, server) = endpoint_checked(Vec::new());
    for calls in [0, 4] {
        let mut budget =
            Budget::with_compute_units(calls, 3, std::time::Duration::from_secs(20), 2000);
        assert!(read(&rpc, &config, &token, &context, &mut budget).is_none());
        assert_eq!(budget.calls_made(), 0);
    }
    for change in 0..4 {
        let mut config = config.clone();
        match change {
            0 => config["qualified"] = json!(false),
            1 => config["qualified"] = Value::Null,
            2 => config["verification_scope"] = json!("future_scope"),
            _ => config["verification_scope"] = Value::Null,
        }
        let mut budget = Budget::default();
        assert!(read(&rpc, &config, &token, &context, &mut budget).is_none());
        assert_eq!(budget.calls_made(), 0);
    }
    for change in 0..9 {
        let mut context = context.clone();
        match change {
            0 => context["recipient_slot"] = Value::Null,
            1 => context["recipient_slot"] = json!(16),
            2 => context["currencies"] = Value::Null,
            3 => context["currencies"][0] = json!("malformed"),
            4 => context["currencies"] = json!([token, token]),
            5 => context["currencies"] = json!(["0x4200000000000000000000000000000000000006"]),
            6 => context["currencies"] = json!([token]),
            7 => {
                context["currencies"] = json!([
                    "0x4200000000000000000000000000000000000006",
                    "0x0000000000000000000000000000000000000001"
                ]);
            }
            _ => {
                context["currencies"] = json!([
                    token,
                    "0x4200000000000000000000000000000000000006",
                    "0x0000000000000000000000000000000000000001"
                ]);
            }
        }
        let mut budget = Budget::default();
        let result = read(&rpc, &config, &token, &context, &mut budget).unwrap();
        assert_eq!(result["qualified"], false, "{result}");
        assert_eq!(budget.calls_made(), 0);
    }
    server.join().unwrap();
}

#[test]
fn collection_retains_single_slot_preference_without_upgrading_deposit_or_revenue() {
    use super::super::{read as collection_read, statements, tests as collection_tests};
    let (receipt, fees, _, _) = collection_tests::fixture();
    let trace: Value = serde_json::from_str(include_str!(
        "../../../../../../docs/research/data/0075-base/clanker-configuration-qualification.json"
    ))
    .unwrap();
    let canonical = trace["records"][1]["response"]["result"].clone();
    let mut rows = collection_tests::responses();
    rows.extend(
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
        .map(|(m, r)| {
            (
                m,
                r["response"]["result"].clone(),
                Some(r["params"].clone()),
            )
        }),
    );
    rows.extend(responses());
    for failure in [false, true] {
        let mut rows = rows.clone();
        if failure {
            rows[13].1 = Value::Null;
            rows.truncate(14);
        }
        let (rpc, server) = endpoint_checked(rows);
        let mut budget =
            Budget::with_compute_units(18, 3, std::time::Duration::from_secs(20), 2000);
        let result = collection_read(
            &rpc,
            &collection_tests::case(),
            &receipt,
            &canonical,
            &fees,
            &mut budget,
        );
        server.join().unwrap();
        assert_eq!(budget.calls_made(), if failure { 14 } else { 17 });
        assert_eq!(budget.calls_left(), if failure { 4 } else { 1 });
        assert_eq!(result[0]["qualified"], true);
        assert_eq!(result[0]["reward_configuration"]["qualified"], true);
        assert_eq!(result[0]["fee_preference"]["qualified"], !failure);
        if failure {
            assert!(
                result[0]["fee_preference"]["gap"]
                    .as_str()
                    .unwrap()
                    .contains("eth_call")
            );
        } else {
            assert_eq!(result[0]["fee_preference"]["mode"], "paired");
        }
        assert_eq!(result[0]["pool_attribution"], "unresolved");
        assert_eq!(fees.credits[0]["financial_state"], "executed");
        let facts = statements(&result, &collection_tests::case());
        let text = facts[0]["text"].as_str().unwrap();
        assert_eq!(
            text.contains("requests fees in the paired pool asset"),
            !failure
        );
        assert_eq!(text.contains("not proof of a realized swap"), !failure);
        assert_eq!(text.contains("fee preferences, conversion"), failure);
    }
}
