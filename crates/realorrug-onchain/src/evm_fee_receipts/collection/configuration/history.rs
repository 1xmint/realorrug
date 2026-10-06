// SPDX-License-Identifier: Apache-2.0
//! Replay only the reviewed recipient/admin transitions for one existing token.
use super::{COLLECTED, LOCKER, address_word, number, only_collection};
use crate::evm_protocols::{clanker_rewards, words};
use realorrug_robinhood::Hash32;
use serde_json::{Value, json};

const RECIPIENT: &str = "0x189a28a3be2ec38788a4a7ff2146b2d9f4f852de2353c9287fed2564845a05d8";
const ADMIN: &str = "0x2aad5ba8634f3d476ba298c3c089253d053106f0dd2caac6bae480ed9db420bf";

#[cfg(test)]
mod tests;

// Canonical packed arrays avoid aliasing a recipient write into another field.
fn layout(tuple: &[String]) -> Result<usize, String> {
    let token = address_word(tuple.get(1).ok_or("reward token absent")?)?;
    let configuration = clanker_rewards(tuple, &token)?;
    let n = configuration["shares"].as_array().unwrap().len();
    if tuple.len() != 15 + 3 * n
        || number(tuple, 9)? != 352
        || number(tuple, 10)? != (352 + 32 * (n + 1)) as u128
        || number(tuple, 11)? != (352 + 64 * (n + 1)) as u128
        || number(tuple, 7)? == 0
    {
        return Err("unsupported packed reward history layout or uninitialized position".into());
    }
    Ok(n)
}

pub(super) fn reconstruct(
    value: &Value,
    receipt: &Value,
    opening: &[String],
    closing: &[String],
) -> Result<(u64, Vec<String>, Value), String> {
    let logs = value.as_array().ok_or("locker block events absent")?;
    if !(2..=128).contains(&logs.len()) {
        return Err("reward history requires 2..128 locker events; missing coverage or excess remains unresolved".into());
    }
    let n = layout(opening)?;
    let token = address_word(&opening[1])?;
    let mut state = opening.to_vec();
    let mut snapshot = None;
    let mut previous = None;
    let mut transitions = Vec::new();
    for log in logs {
        if !log["address"]
            .as_str()
            .is_some_and(|a| a.eq_ignore_ascii_case(LOCKER))
        {
            return Err("reward history returned a different emitter".into());
        }
        let tx = log["transactionHash"]
            .as_str()
            .ok_or("history transaction hash absent")?;
        tx.parse::<Hash32>().map_err(|e| e.to_string())?;
        let index = super::super::super::checkpoint(
            log,
            &json!({
                "transactionHash":tx,"blockHash":receipt["blockHash"],"blockNumber":receipt["blockNumber"]
            }),
        )?;
        if previous.is_some_and(|last| index <= last) {
            return Err("reward history indices are repeated or out of order".into());
        }
        previous = Some(index);
        if log["topics"][0] == COLLECTED {
            let anchor = only_collection(&json!([log]), receipt)?;
            // Exact receipt anchoring and unique indices admit this once only.
            snapshot = Some((anchor, state.clone()));
            continue;
        }
        let field = match log["topics"][0].as_str() {
            Some(RECIPIENT) => ("recipient", 15 + 2 * n),
            Some(ADMIN) => ("admin", 14 + n),
            _ => return Err("unsupported locker event in reward history".into()),
        };
        // The collection reads state more than once. Logs alone cannot locate
        // those reads relative to a callback; refuse the entire transaction.
        if receipt["transactionHash"]
            .as_str()
            .is_some_and(|hash| tx.eq_ignore_ascii_case(hash))
        {
            return Err(
                "reward change inside the collection transaction remains unresolved".into(),
            );
        }
        let topics = log["topics"]
            .as_array()
            .ok_or("reward update topics absent")?;
        if topics.len() != 3
            || address_word(topics[1].as_str().ok_or("update token absent")?)? != token
        {
            return Err("reward update topic layout or token disagrees".into());
        }
        let slot = usize::try_from(number(
            &topics
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_owned())
                .collect::<Vec<_>>(),
            2,
        )?)
        .map_err(|e| e.to_string())?;
        if slot >= n {
            return Err("reward update slot exceeds configured recipients".into());
        }
        let data = words(log["data"].as_str().ok_or("reward update data absent")?)?;
        if data.len() != 2 {
            return Err("reward update requires exactly two address words".into());
        }
        let old = address_word(&data[0])?;
        let new = address_word(&data[1])?;
        let at = field.1 + slot;
        if address_word(&state[at])? != old {
            return Err("reward update old value disagrees with reconstructed state".into());
        }
        state[at].clone_from(&data[1]);
        transitions.push(
            json!({"log_index":index,"transaction_hash":tx,"field":field.0,
            "recipient_slot":slot,"old":old,"new":new}),
        );
    }
    if state != closing {
        return Err("reward event history does not reconcile the closing tuple".into());
    }
    let (anchor, tuple) = snapshot.ok_or("submitted collection absent from reward history")?;
    Ok((anchor, tuple, json!(transitions)))
}
