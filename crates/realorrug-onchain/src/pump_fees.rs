// SPDX-License-Identifier: Apache-2.0
//! Pump creator-fee sharing: pinned IDL layout, not generic balance inference.
use crate::{
    Budget, RpcClient,
    cases::{CaseKey, Observation},
    investigation::{Read, observed, statement},
};
use realorrug_types::Address;
use serde_json::{Value, json};

/// Decode the published SharingConfig layout. Unknown versions refuse.
///
/// # Panics
/// Fixed-size conversions rely on the preceding checked slice lengths.
///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub fn sharing(data: &[u8], mint: &Address) -> Result<Value, String> {
    if data.len() < 80 || data[..8] != [216, 74, 9, 0, 56, 140, 93, 75] {
        return Err("not a SharingConfig account".into());
    }
    if data[9] != 2 || data[10] > 1 || data[75] > 1 {
        return Err("unsupported sharing version/status".into());
    }
    if data[11..43] != *mint.as_bytes() {
        return Err("sharing config names a different mint".into());
    }
    let n = u32::from_le_bytes(data[76..80].try_into().unwrap()) as usize;
    if n == 0 || n > 27 || data.len() < 80 + n * 34 {
        return Err("invalid shareholder vector".into());
    }
    let mut shares = Vec::new();
    let mut total = 0u32;
    for entry in data[80..80 + n * 34].as_chunks::<34>().0 {
        let address = Address::new(entry[..32].try_into().unwrap());
        let bps = u16::from_le_bytes(entry[32..34].try_into().unwrap());
        total += u32::from(bps);
        if shares
            .iter()
            .any(|s: &Value| s["address"] == address.to_string())
        {
            return Err("duplicate shareholder".into());
        }
        shares.push(json!({"address":address.to_string(),"bps":bps}));
    }
    if total != 10_000 {
        return Err("shares do not total the documented denominator".into());
    }
    Ok(
        json!({"shares":shares,"admin_revoked":data[75]==1,"active":data[10]==1,
        "admin":Address::new(data[43..75].try_into().unwrap()).to_string()}),
    )
}

///
/// # Errors
/// Invalid or unsupported input, exhausted bounds, unavailable data, or storage failure.
pub(crate) fn read(
    client: &RpcClient,
    case: &CaseKey,
    read: &Read,
    budget: &mut Budget,
    at: u64,
) -> Result<Observation, String> {
    let mint: Address = case
        .address
        .parse()
        .map_err(|e: realorrug_types::AddressParseError| e.to_string())?;
    let program = realorrug_pumpfun::pda::FEE_PROGRAM;
    let config = realorrug_pumpfun::pda::find(&[b"sharing-config", mint.as_bytes()], &program)
        .ok_or("sharing PDA derivation failed")?
        .0;
    let accounts = client
        .accounts(budget, &[config])
        .map_err(|e| e.to_string())?;
    let Some(account) = accounts.accounts.first().and_then(Option::as_ref) else {
        return single_creator(client, case, read, &mint, budget, at);
    };
    if account.owner.as_deref() != Some(&program.to_string()) {
        return Err("sharing program identity mismatch".into());
    }
    let mut value = sharing(&account.data, &mint)?;
    let mut facts = Vec::new();
    let mut related = Vec::new();
    for share in value["shares"].as_array().ok_or("shares absent")? {
        facts.push(statement(format!(
            "Configured creator-fee share: {} basis points out of 10000 to {}.",
            share["bps"],
            share["address"].as_str().unwrap_or("")
        )));
        related.push(share["address"].as_str().unwrap_or("").to_owned());
    }
    facts.push(statement(format!(
        "Sharing administrator revoked: {}. Distribution active: {}.",
        value["admin_revoked"], value["active"]
    )));
    value["statements"] = json!(facts);
    value["related"] = json!(related);
    Ok(observed(case,read,at,accounts.slot.map(|s|s.to_string()),value,
        Some("configured shares do not prove distributions, downstream routing or beneficial ownership; platform upgrade powers remain separate".into())))
}

fn single_creator(
    client: &RpcClient,
    case: &CaseKey,
    read: &Read,
    mint: &Address,
    budget: &mut Budget,
    at: u64,
) -> Result<Observation, String> {
    let curve_address =
        realorrug_pumpfun::pda::bonding_curve(mint).ok_or("curve derivation failed")?;
    let accounts = client
        .accounts(budget, &[curve_address])
        .map_err(|e| e.to_string())?;
    let account = accounts
        .accounts
        .first()
        .and_then(Option::as_ref)
        .ok_or("no Pump curve or sharing configuration")?;
    if account
        .owner
        .as_deref()
        .and_then(|owner| owner.parse::<Address>().ok())
        != Some(realorrug_pumpfun::pda::PROGRAM_ID)
    {
        return Err("curve owner mismatch".into());
    }
    let curve =
        realorrug_pumpfun::BondingCurve::parse(&account.data).map_err(|e| format!("{e:?}"))?;
    let creator = curve.creator;
    let pump =
        realorrug_pumpfun::pda::creator_vault(&creator).ok_or("creator vault derivation failed")?;
    let swap = realorrug_pumpfun::pda::pumpswap_coin_creator_vault_ata(&creator)
        .ok_or("swap fee vault derivation failed")?;
    let mut facts = vec![statement(format!(
        "Pump curve creator field is {creator}; derived creator fee vault is {pump}. This is a configured route, not beneficiary identity."
    ))];
    let mut gaps=vec!["creator vaults aggregate fees across tokens; balances/decreases are not per-token recipient receipts or downstream ownership proof".to_owned()];
    if let Ok(Some(balance)) = client.account(budget, &pump)
        && let Some(lamports) = balance.lamports
    {
        facts.push(statement(format!("Creator fee vault native balance is {lamports} lamports at slot {}; this includes any account reserve.",balance.slot.map_or_else(||"unreported".into(),|s|s.to_string()))));
    }
    let mut transactions = Vec::new();
    match client.signatures_page(budget, &pump, None) {
        Ok(Some(signatures)) => {
            for signature in signatures.iter().filter(|s| s.err.is_none()).take(2) {
                let transaction = match client.transaction(budget, &signature.signature) {
                    Ok(tx) => tx,
                    Err(e) => {
                        gaps.push(e.to_string());
                        None
                    }
                };
                transactions.push(crate::treasury_receipts::SignedTransaction {
                    signature: signature.signature.clone(),
                    transaction,
                });
            }
        }
        Ok(None) => gaps.push("signature pagination exhausted".into()),
        Err(e) => gaps.push(e.to_string()),
    }
    let (receipts, unread) = crate::treasury_receipts::find_receipts(
        &crate::treasury_receipts::VaultAddresses {
            pumpfun: pump,
            pumpswap: swap,
        },
        &transactions,
    );
    let mut evidence = Vec::new();
    for receipt in receipts {
        facts.push(statement(format!("Derived {:?} fee vault decreased by {} base units in successful transaction {} at slot {}; this is a vault decrease, not a named recipient's net receipt.",receipt.vault,receipt.amount,receipt.signature,receipt.slot)));
        evidence.push(json!({"signature":receipt.signature,"slot":receipt.slot,"amount":receipt.amount.to_string(),"collect_instruction_seen":receipt.collect_instruction_seen}));
    }
    gaps.extend(unread.into_iter().map(|u| u.why));
    Ok(observed(
        case,
        read,
        at,
        accounts.slot.map(|s| s.to_string()),
        json!({"protocol":"pump_single_creator","creator":creator.to_string(),"fee_vault":pump.to_string(),
        "statements":facts,"vault_decreases":evidence,"related":[creator.to_string(),pump.to_string(),swap.to_string()]}),
        Some(gaps.join("; ")),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fee_shares_need_matching_mint_version_and_complete_denominator() {
        let mint = Address::new([3; 32]);
        let mut data = vec![0; 148];
        data[..8].copy_from_slice(&[216, 74, 9, 0, 56, 140, 93, 75]);
        data[9] = 2;
        data[10] = 1;
        data[11..43].copy_from_slice(mint.as_bytes());
        data[75] = 1;
        data[76..80].copy_from_slice(&2u32.to_le_bytes());
        data[80..112].fill(4);
        data[114..146].fill(5);
        data[112..114].copy_from_slice(&9500u16.to_le_bytes());
        data[146..148].copy_from_slice(&500u16.to_le_bytes());
        assert_eq!(sharing(&data, &mint).unwrap()["shares"][0]["bps"], 9500);
        assert!(sharing(&data, &Address::new([9; 32])).is_err());
        data[9] = 1;
        assert!(sharing(&data, &mint).is_err());
        data[9] = 2;
        data[146..148].copy_from_slice(&100u16.to_le_bytes());
        assert!(sharing(&data, &mint).is_err());
    }
}
