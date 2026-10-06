// SPDX-License-Identifier: Apache-2.0
use super::super::tests::{fixture, responses};
use super::*;
use crate::{Budget, evm_investigation::tests::endpoint_checked};

fn encoded(tuple: &[String]) -> Value {
    json!(format!(
        "0x{}",
        tuple
            .iter()
            .map(|w| w.trim_start_matches("0x"))
            .collect::<String>()
    ))
}

fn update(anchor: &Value, signature: &str, index: u64, slot: usize, old: &str, new: &str) -> Value {
    let mut log = anchor.clone();
    log["logIndex"] = json!(format!("0x{index:x}"));
    log["transactionHash"] = json!(format!("0x{index:064x}"));
    log["topics"] = json!([signature, anchor["topics"][1], format!("0x{slot:064x}")]);
    log["data"] = encoded(&[old.to_owned(), new.to_owned()]);
    log
}

fn history() -> (Value, Value, Vec<String>, Vec<String>) {
    let (receipt, _, _, opening) = fixture();
    let anchor = responses()[3].1[0].clone();
    let other = format!("0x{:064x}", 1);
    let mut closing = opening.clone();
    closing[15] = other.clone();
    closing[17] = other.clone();
    let logs = json!([
        update(&anchor, ADMIN, 90, 0, &opening[15], &other),
        update(&anchor, RECIPIENT, 91, 0, &opening[17], &other),
        anchor,
        update(&anchor, RECIPIENT, 109, 0, &other, &opening[17]),
        update(&anchor, RECIPIENT, 110, 0, &opening[17], &other)
    ]);
    (receipt, logs, opening, closing)
}

#[test]
fn ordered_reward_history_reconstructs_event_time_and_retains_round_trip_changes() {
    let (receipt, logs, opening, closing) = history();
    let (anchor, tuple, transitions) = reconstruct(&logs, &receipt, &opening, &closing).unwrap();
    assert_eq!(anchor, 108);
    assert_eq!(tuple, closing);
    assert_eq!(transitions.as_array().unwrap().len(), 4);
    assert_eq!(transitions[0]["field"], "admin");
    assert_eq!(transitions[1]["field"], "recipient");
    assert_eq!(transitions[2]["log_index"], 109);
    assert_eq!(
        transitions[3]["new"],
        "0x0000000000000000000000000000000000000001"
    );
    assert_eq!(transitions[0]["recipient_slot"], 0);
    let anchor = logs[2].clone();
    let only_after = json!([
        anchor,
        update(&anchor, RECIPIENT, 109, 0, &opening[17], &closing[17])
    ]);
    let mut final_tuple = opening.clone();
    final_tuple[17] = closing[17].clone();
    assert_eq!(
        reconstruct(&only_after, &receipt, &opening, &final_tuple)
            .unwrap()
            .1,
        opening
    );
    let only_before = json!([
        update(&anchor, RECIPIENT, 100, 0, &opening[17], &closing[17]),
        anchor
    ]);
    assert_eq!(
        reconstruct(&only_before, &receipt, &opening, &final_tuple)
            .unwrap()
            .1,
        final_tuple
    );
}

#[test]
fn ordered_configuration_uses_six_calls_and_returns_reconstructed_tuple() {
    let (receipt, logs, opening, closing) = history();
    let (_, canonical, code, _) = fixture();
    let mut rows = responses();
    rows[2].1 = encoded(&opening);
    rows[3].1 = logs;
    let (rpc, server) = endpoint_checked(rows);
    let mut budget = Budget::with_compute_units(7, 3, std::time::Duration::from_secs(20), 2000);
    let result = super::super::read(&rpc, &receipt, &canonical, &code, &closing, &mut budget);
    server.join().unwrap();
    assert_eq!(budget.calls_made(), 6);
    assert_eq!(budget.calls_left(), 1);
    assert_eq!(result["qualified"], true);
    assert_eq!(
        result["verification_scope"],
        "provider_ordered_reward_configuration"
    );
    assert_eq!(result["locker_events"], 5);
    assert_eq!(result["collection_log_index"], 108);
    assert_eq!(result["reward_tuple"], json!(closing));
    assert_eq!(result["transitions"].as_array().unwrap().len(), 4);
    assert!(
        result["gap"]
            .as_str()
            .unwrap()
            .contains("not independent completeness proof")
    );
}

#[test]
fn collection_links_the_event_time_recipient_instead_of_the_closing_recipient() {
    use super::super::super::{read, statements, tests as collection_tests};
    let (receipt, fees, _, tuple) = collection_tests::fixture();
    let (_, canonical, _, _) = fixture();
    let anchor = responses()[3].1[0].clone();
    let other = format!("0x{:064x}", 1);
    for before in [false, true] {
        let mut opening = tuple.clone();
        let mut closing = tuple.clone();
        let index = if before { 100 } else { 109 };
        if before {
            opening[17].clone_from(&other);
        } else {
            closing[17].clone_from(&other);
        }
        let event = update(&anchor, RECIPIENT, index, 0, &opening[17], &closing[17]);
        let events = if before {
            json!([event, anchor])
        } else {
            json!([anchor, event])
        };
        let mut rows = collection_tests::responses();
        rows[5].1 = encoded(&closing);
        let mut history = responses();
        history[2].1 = encoded(&opening);
        history[3].1 = events;
        rows.extend(history);
        let (rpc, server) = endpoint_checked(rows);
        let mut budget =
            Budget::with_compute_units(14, 3, std::time::Duration::from_secs(20), 2000);
        let contexts = read(
            &rpc,
            &collection_tests::case(),
            &receipt,
            &canonical,
            &fees,
            &mut budget,
        );
        server.join().unwrap();
        assert_eq!(budget.calls_made(), 13);
        assert_eq!(budget.calls_left(), 1);
        assert_eq!(
            contexts[0]["qualified"], true,
            "before={before}: {}",
            contexts[0]
        );
        assert_eq!(contexts[0]["context"]["recipient_slot"], 0);
        assert_eq!(
            contexts[0]["reward_configuration"]["reward_tuple"],
            json!(tuple)
        );
        assert_eq!(contexts[0]["pool_attribution"], "unresolved");
        assert!(
            contexts[0]["gap"]
                .as_str()
                .unwrap()
                .contains("registration remains block-end")
        );
        let facts = statements(&contexts, &collection_tests::case());
        assert!(
            facts[0]["text"]
                .as_str()
                .unwrap()
                .contains("reconstructed collection tuple")
        );
        assert!(
            facts[0]["text"]
                .as_str()
                .unwrap()
                .contains("per-token revenue remain unresolved")
        );
    }
}

#[test]
fn ordered_reward_history_refuses_incomplete_malformed_and_in_transaction_changes() {
    let (receipt, logs, opening, closing) = history();
    for change in 0..25 {
        let mut events = logs.clone();
        let mut submitted = receipt.clone();
        let mut end = closing.clone();
        match change {
            0 => events = Value::Null,
            1 => events = json!([]),
            2 => events[0]["address"] = json!(super::super::super::POOL_MANAGER),
            3 => events[0]["transactionHash"] = Value::Null,
            4 => events[0]["transactionHash"] = json!("malformed"),
            5 => events[0]["removed"] = json!(true),
            6 => events[0]["blockHash"] = json!(format!("0x{:064x}", 1)),
            7 => events[0]["blockNumber"] = json!("0x1"),
            8 => events[1]["logIndex"] = events[0]["logIndex"].clone(),
            9 => events.as_array_mut().unwrap().swap(0, 1),
            10 => events[0]["topics"][0] = json!("unsupported"),
            11 => {
                events[0]["transactionHash"] = json!(format!(
                    "0x{}",
                    submitted["transactionHash"].as_str().unwrap()[2..].to_uppercase()
                ));
            }
            12 => events[0]["topics"]
                .as_array_mut()
                .unwrap()
                .push(json!("extra")),
            13 => events[0]["topics"][1] = json!(format!("0x{:064x}", 1)),
            14 => events[0]["topics"][2] = json!(format!("0x{:064x}", 1)),
            15 => events[0]["data"] = json!("0x"),
            16 => events[0]["data"] = encoded(&[opening[15].clone()]),
            17 => events[0]["data"] = encoded(&[format!("0x{:064x}", 1), closing[15].clone()]),
            18 => end[15] = opening[15].clone(),
            19 => {
                events.as_array_mut().unwrap().remove(2);
            }
            20 => submitted["logs"] = json!([]),
            21 => {
                let mut missing_change = events[0].clone();
                missing_change["transactionHash"] = submitted["transactionHash"].clone();
                submitted["logs"]
                    .as_array_mut()
                    .unwrap()
                    .push(missing_change);
            }
            22 => {
                events[0]["data"] = encoded(&[
                    opening[15].clone(),
                    closing[15].clone(),
                    opening[15].clone(),
                ]);
            }
            23 => {
                // A one-past admin slot aliases the recipient vector length.
                // A no-op would otherwise reconcile, masking a missing bound.
                let count = format!("0x{:064x}", 1);
                events
                    .as_array_mut()
                    .unwrap()
                    .push(update(&logs[2], ADMIN, 111, 1, &count, &count));
            }
            _ => {
                // A larger invalid admin slot can alias a recipient address.
                events.as_array_mut().unwrap().push(update(
                    &logs[2],
                    ADMIN,
                    111,
                    2,
                    &closing[17],
                    &closing[17],
                ));
            }
        }
        assert!(
            reconstruct(&events, &submitted, &opening, &end).is_err(),
            "change {change}"
        );
    }
    // Same-transaction changes are refused on both sides of the collection.
    for at in [0, 3] {
        let mut events = logs.clone();
        events[at]["transactionHash"] = receipt["transactionHash"].clone();
        assert!(
            reconstruct(&events, &receipt, &opening, &closing)
                .unwrap_err()
                .contains("collection transaction")
        );
    }
    let mut duplicate = logs;
    let mut anchor = duplicate[2].clone();
    anchor["logIndex"] = json!("0x6f");
    let mut submitted = receipt;
    submitted["logs"]
        .as_array_mut()
        .unwrap()
        .push(anchor.clone());
    duplicate.as_array_mut().unwrap().push(anchor);
    assert!(reconstruct(&duplicate, &submitted, &opening, &closing).is_err());
}

fn many(tuple: &[String], n: usize) -> Vec<String> {
    let mut data = tuple[..12].to_vec();
    data[10] = format!("0x{:064x}", 352 + 32 * (n + 1));
    data[11] = format!("0x{:064x}", 352 + 64 * (n + 1));
    for values in [
        vec![format!("0x{:064x}", 0); n],
        vec![tuple[15].clone(); n],
        vec![tuple[17].clone(); n],
    ] {
        data.push(format!("0x{n:064x}"));
        data.extend(values);
    }
    data[13] = format!("0x{:064x}", 10000);
    data
}

#[test]
fn ordered_reward_history_checks_packed_arrays_slots_and_event_bounds() {
    let (receipt, logs, opening, closing) = history();
    for change in 0..12 {
        let mut tuple = opening.clone();
        match change {
            0 => tuple[0] = format!("0x{:064x}", 64),
            1 => tuple[7] = format!("0x{:064x}", 0),
            2 => tuple[9] = format!("0x{:064x}", 384),
            3 => tuple[10] = format!("0x{:064x}", 448),
            4 => tuple[11] = format!("0x{:064x}", 512),
            5 => tuple[12] = format!("0x{:064x}", 0),
            6 => tuple[14] = format!("0x{:064x}", 2),
            7 => tuple[16] = format!("0x{:064x}", 2),
            8 => {
                tuple.pop();
            }
            9 => tuple.push(format!("0x{:064x}", 0)),
            10 => tuple[13] = format!("0x{:064x}", 9999),
            _ => tuple[15] = format!("0x{}", "f".repeat(64)),
        }
        assert!(
            reconstruct(&logs, &receipt, &tuple, &closing).is_err(),
            "layout {change}"
        );
    }
    let anchor = logs[2].clone();
    assert!(
        reconstruct(&json!([anchor]), &receipt, &opening, &opening)
            .unwrap_err()
            .contains("2..128")
    );
    // The general tuple decoder permits these aliases, but in-place replay must
    // require the reviewed getter's canonical layout before changing a slot.
    for offset in [9, 10, 11] {
        let mut aliased = opening.clone();
        aliased[offset] = format!("0x{:064x}", if offset == 10 { 480 } else { 416 });
        if offset == 9 {
            aliased[15] = format!("0x{:064x}", 10000);
        }
        clanker_rewards(&aliased, &address_word(&aliased[1]).unwrap()).unwrap();
        assert!(
            layout(&aliased).unwrap_err().contains("packed"),
            "offset {offset}"
        );
    }
    let mut extra = opening.clone();
    extra.push(format!("0x{:064x}", 0));
    assert!(layout(&extra).unwrap_err().contains("packed"));
    let other = format!("0x{:064x}", 1);
    for n in [2, 16, 17] {
        let opening = many(&opening, n);
        let mut closing = opening.clone();
        closing[15 + 2 * n + n - 1] = other.clone();
        let events = json!([
            anchor,
            update(
                &anchor,
                RECIPIENT,
                109,
                n - 1,
                &opening[15 + 2 * n + n - 1],
                &other
            )
        ]);
        assert_eq!(
            reconstruct(&events, &receipt, &opening, &closing).is_ok(),
            n <= 16
        );
    }
    for n in [128, 129] {
        let mut events = vec![anchor.clone()];
        for index in 109..109 + n - 1 {
            events.push(update(
                &anchor,
                RECIPIENT,
                index,
                0,
                &opening[17],
                &opening[17],
            ));
        }
        assert_eq!(
            reconstruct(&json!(events), &receipt, &opening, &opening).is_ok(),
            n == 128
        );
    }
}
