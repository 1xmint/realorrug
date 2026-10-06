// SPDX-License-Identifier: Apache-2.0
//! One recipient's requested denomination; never proof of conversion or revenue.
use super::{LOCKER, arg, call, number, query};
use crate::{Budget, evm_fee_receipts::window::header};
use realorrug_robinhood::{Address, Rpc};
use serde_json::{Value, json};

pub(super) fn read(
    rpc: &Rpc,
    configuration: &Value,
    token: &str,
    context: &Value,
    budget: &mut Budget,
) -> Option<Value> {
    // Both admitted histories exclude preference-change events. A future reward
    // history scope must establish that property before reusing this check.
    if configuration["qualified"] != true
        || ![
            "provider_quiet_block_reward_configuration",
            "provider_ordered_reward_configuration",
        ]
        .contains(&configuration["verification_scope"].as_str().unwrap_or(""))
        || budget.calls_left() < 5
    {
        return None;
    }
    Some(qualify(rpc, configuration, token, context, budget).unwrap_or_else(|gap| {
        json!({"qualified":false,"verification_scope":"unavailable_single_slot_fee_preference","gap":gap})
    }))
}

fn qualify(
    rpc: &Rpc,
    configuration: &Value,
    token: &str,
    context: &Value,
    budget: &mut Budget,
) -> Result<Value, String> {
    let slot = context["recipient_slot"]
        .as_u64()
        .ok_or("recipient slot absent")?;
    if slot > 15 {
        return Err("recipient slot exceeds the reviewed reward bound".into());
    }
    let currencies = context["currencies"]
        .as_array()
        .ok_or("pool currencies absent")?;
    let currencies: Vec<String> = currencies
        .iter()
        .map(|v| {
            v.as_str()
                .ok_or("pool currency absent")?
                .parse::<Address>()
                .map(|a| a.to_string())
                .map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    if currencies.len() != 2
        || currencies[0] == currencies[1]
        || !currencies.iter().any(|a| a == token)
    {
        return Err("fee preference requires one token and one distinct paired currency".into());
    }
    let opening = configuration["opening_block"]
        .as_u64()
        .ok_or("opening block absent")?;
    let closing = configuration["closing_block"]
        .as_u64()
        .ok_or("closing block absent")?;
    let args = format!("{}{slot:064x}", arg(token));
    let mut preferences = Vec::new();
    for block in [opening, closing] {
        let data = query(
            rpc,
            budget,
            LOCKER,
            "feePreferences(address,uint256)",
            &args,
            &format!("0x{block:x}"),
        )?;
        if data.len() != 1 {
            return Err("fee preference getter requires exactly one ABI word".into());
        }
        preferences.push(number(&data, 0)?);
    }
    if preferences[0] != preferences[1] {
        return Err("parent and closing fee preferences differ".into());
    }
    let (mode, requested_assets) = match preferences[0] {
        0 => ("both", currencies.clone()),
        1 => (
            "paired",
            currencies.into_iter().filter(|a| a != token).collect(),
        ),
        2 => ("token", vec![token.to_owned()]),
        _ => return Err("unsupported fee preference enum".into()),
    };
    for (block, field) in [
        (opening, "opening_block_hash"),
        (closing, "closing_block_hash"),
    ] {
        let current = call(
            rpc,
            budget,
            "eth_getBlockByNumber",
            json!([format!("0x{block:x}"), false]),
        )?;
        if header(&current, block)?
            != configuration[field]
                .as_str()
                .ok_or("configuration hash absent")?
        {
            return Err("fee preference checkpoint changed during the read".into());
        }
    }
    Ok(
        json!({"qualified":true,"verification_scope":"provider_single_slot_fee_preference_stability",
        "recipient_slot":slot,"configuration_value":preferences[0],"mode":mode,"requested_assets":requested_assets,
        "opening_block":opening,"closing_block":closing,"opening_block_hash":configuration["opening_block_hash"],
        "closing_block_hash":configuration["closing_block_hash"],
        "gap":"one recipient slot only; parent/closing getter values agree and the qualified locker history excludes fee-preference writes under reviewed-source/provider assumptions; this is not independent completeness proof, a realized swap, conversion correctness, backing or per-token revenue"}),
    )
}

#[cfg(test)]
mod tests;
