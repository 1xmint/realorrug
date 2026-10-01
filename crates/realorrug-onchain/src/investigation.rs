// SPDX-License-Identifier: Apache-2.0
//! Bounded typed read tools shared by live investigations and offline replay.
use crate::{
    Budget, RpcClient,
    cases::{CaseKey, Network, Observation, TimeWindow},
};
use realorrug_types::Address;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Public read allowlist. No action or arbitrary RPC/URL tool exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    /// Identity and available authorities.
    Token,
    /// Current account/contract state.
    Account,
    /// One transaction and its effects.
    Transaction,
    /// Bounded account activity.
    History,
    /// Supported fee configuration and routes.
    Fees,
    /// Supported pool state.
    Liquidity,
    /// Verified EVM source lookup.
    Source,
}

/// Tool arguments are typed and validated by the outer investigation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Read {
    /// Allowed operation.
    pub tool: Tool,
    /// Address or transaction on the case's network.
    pub subject: String,
    /// Selection explanation, untrusted and never a measured statement.
    pub why: String,
}

/// The seam replay substitutes; only reader-produced data become observations.
pub trait Reader {
    /// Perform a bounded operation on the explicit network.
    ///
    /// # Errors
    /// Unsupported input, exhausted bounds or unavailable chain evidence.
    fn read(
        &mut self,
        case: &CaseKey,
        read: &Read,
        window: Option<&TimeWindow>,
        budget: &mut Budget,
        at: u64,
    ) -> Result<Observation, String>;
}

/// Exact statement produced by code, not synthesized by a model.
pub fn statement(text: impl Into<String>) -> Value {
    json!({"text":text.into()})
}

/// Deterministic observation id retains payload provenance without credentials.
///
/// # Panics
/// Only if the JSON serializer cannot encode an existing JSON value.
#[must_use]
pub fn observed(
    case: &CaseKey,
    read: &Read,
    at: u64,
    point: Option<String>,
    value: Value,
    gap: Option<String>,
) -> Observation {
    let source = format!("{}:{:?}:{}", case.chain, read.tool, read.subject);
    let id = blake3::hash(
        serde_json::to_string(&(source.as_str(), at, &point, &value, &gap))
            .expect("reader values serialize")
            .as_bytes(),
    )
    .to_hex()
    .to_string();
    Observation {
        id,
        kind: format!("{:?}", read.tool).to_lowercase(),
        source,
        at,
        read_point: point,
        value,
        gap,
        version: "investigation-read-v1".into(),
    }
}

/// Configured clients only. The EVM endpoint's chain is checked before reads.
pub struct LiveReader<'a> {
    /// Existing Solana client.
    pub solana: &'a RpcClient,
    /// Explicit Base client, never inferred from address syntax.
    pub base: Option<&'a realorrug_robinhood::Rpc>,
    /// Explicit Ethereum client.
    pub ethereum: Option<&'a realorrug_robinhood::Rpc>,
    /// Legacy Robinhood client.
    pub robinhood: Option<&'a realorrug_robinhood::Rpc>,
}

impl Reader for LiveReader<'_> {
    fn read(
        &mut self,
        case: &CaseKey,
        read: &Read,
        window: Option<&TimeWindow>,
        budget: &mut Budget,
        at: u64,
    ) -> Result<Observation, String> {
        match case.chain {
            Network::Solana => solana_read(self.solana, case, read, window, budget, at),
            Network::Base | Network::Ethereum => {
                let rpc = match case.chain {
                    Network::Base => self.base,
                    _ => self.ethereum,
                }
                .ok_or("no endpoint configured for this network")?;
                crate::evm_investigation::read(rpc, case, read, window, budget, at)
            }
            Network::Robinhood => Err(
                "legacy dossier is available; specialist investigation coverage is not configured"
                    .into(),
            ),
        }
    }
}

fn solana_read(
    client: &RpcClient,
    case: &CaseKey,
    read: &Read,
    window: Option<&TimeWindow>,
    budget: &mut Budget,
    at: u64,
) -> Result<Observation, String> {
    let (point, value, gap) = match read.tool {
        Tool::Transaction => solana_transaction(client, case, read, budget)?,
        Tool::History => solana_history(client, read, window, budget)?,
        Tool::Fees => return crate::pump_fees::read(client, case, read, budget, at),
        Tool::Liquidity => solana_curve(client, case, budget)?,
        Tool::Token | Tool::Account => solana_account(client, read, budget)?,
        Tool::Source => {
            return Err("verified EVM source lookup does not apply to Solana programs".into());
        }
    };
    Ok(observed(case, read, at, point, value, gap))
}

type ReadParts = (Option<String>, Value, Option<String>);

fn solana_account(
    client: &RpcClient,
    read: &Read,
    budget: &mut Budget,
) -> Result<ReadParts, String> {
    let address: Address = read
        .subject
        .parse()
        .map_err(|e: realorrug_types::AddressParseError| e.to_string())?;
    let accounts = client
        .accounts(budget, &[address])
        .map_err(|e| e.to_string())?;
    let account = accounts
        .accounts
        .first()
        .and_then(Option::as_ref)
        .ok_or("account does not exist")?;
    let mut facts = vec![statement(format!(
        "Account {} is owned by {}.",
        address,
        account
            .owner
            .as_deref()
            .ok_or("account owner unavailable")?
    ))];
    let mut gap = None;
    if read.tool == Tool::Token {
        let owner = account.owner.as_deref().unwrap_or("");
        if !matches!(
            owner,
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
                | "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        ) {
            return Err("target is not owned by a supported token program".into());
        }
        if account.data.len() < 82 || account.data[45] != 1 {
            return Err("target is not an initialized mint".into());
        }
        let (mint, freeze) = crate::mint_authorities(&account.data).map_err(|e| e.to_string())?;
        facts.push(statement(format!(
            "Mint authority: {}. Freeze authority: {}.",
            mint.map_or_else(|| "revoked".into(), |a| a.to_string()),
            freeze.map_or_else(|| "revoked".into(), |a| a.to_string())
        )));
        if owner.starts_with("Tokenz") {
            gap = Some("Token-2022 extension controls require separate decoding".into());
        }
    }
    Ok((
        accounts.slot.map(|s| s.to_string()),
        json!({"statements":facts,"related":[]}),
        gap,
    ))
}

fn solana_transaction(
    client: &RpcClient,
    case: &CaseKey,
    read: &Read,
    budget: &mut Budget,
) -> Result<ReadParts, String> {
    let tx = client
        .transaction(budget, &read.subject)
        .map_err(|e| e.to_string())?
        .ok_or("transaction not available")?;
    if !tx.meta_present {
        return Err("transaction outcome metadata missing".into());
    }
    if tx.failed {
        return Ok((
            Some(tx.slot.to_string()),
            json!({"statements":[statement("The submitted transaction failed; its attempted transfers are not executed payments.")],"related":[]}),
            None,
        ));
    }
    let mut transfers = Vec::new();
    let mut token_transfers = Vec::new();
    for instruction in &tx.instructions {
        // System transfer's verified little-endian tag and amount. Balances alone
        // also include fees/rent, so they cannot authorize a transfer claim.
        if instruction.program == "11111111111111111111111111111111"
            && instruction.data.len() == 12
            && instruction.data[..4] == 2u32.to_le_bytes()
            && instruction.accounts.len() >= 2
        {
            let lamports = u64::from_le_bytes(instruction.data[4..12].try_into().unwrap());
            transfers.push(json!({"from":instruction.accounts[0],"to":instruction.accounts[1],"lamports":lamports.to_string()}));
        }
        if matches!(
            instruction.program.as_str(),
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
                | "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
        ) && instruction.data.len() == 10
            && instruction.data[0] == 12
            && instruction.accounts.len() >= 4
            && instruction.accounts[1] == case.address
        {
            token_transfers.push(json!({"from_account":instruction.accounts[0],"mint":instruction.accounts[1],"to_account":instruction.accounts[2],
                "amount":u64::from_le_bytes(instruction.data[1..9].try_into().unwrap()).to_string(),"decimals":instruction.data[9]}));
        }
    }
    let mut facts: Vec<Value> = transfers
        .iter()
        .map(|t| {
            statement(format!(
                "Executed System transfer: {} lamports from {} to {}.",
                t["lamports"].as_str().unwrap_or(""),
                t["from"].as_str().unwrap_or(""),
                t["to"].as_str().unwrap_or("")
            ))
        })
        .collect();
    facts.extend(token_transfers.iter().map(|t|statement(format!("Executed TransferChecked reports {} base units of {} from token account {} to token account {}.",
        t["amount"].as_str().unwrap_or(""),case.address,t["from_account"].as_str().unwrap_or(""),t["to_account"].as_str().unwrap_or("")))));
    let related: Vec<String> = transfers
        .iter()
        .flat_map(|t| {
            [
                t["from"].as_str().unwrap_or(""),
                t["to"].as_str().unwrap_or(""),
            ]
        })
        .map(str::to_owned)
        .collect();
    Ok((Some(tx.slot.to_string()),json!({"statements":facts,"transfers":transfers,"token_transfers":token_transfers,"related":related}),
        Some("only decoded System and this mint's TransferChecked instructions are covered; unchecked/extension transfers, account ownership and beneficiary identity remain separate checks".into())))
}

fn solana_history(
    client: &RpcClient,
    read: &Read,
    _window: Option<&TimeWindow>,
    budget: &mut Budget,
) -> Result<ReadParts, String> {
    let address: Address = read
        .subject
        .parse()
        .map_err(|e: realorrug_types::AddressParseError| e.to_string())?;
    let page = client
        .signatures_page(budget, &address, None)
        .map_err(|e| e.to_string())?
        .ok_or("signature pagination exhausted")?;
    let refs: Vec<String> = page
        .iter()
        .filter(|s| s.err.is_none())
        .take(16)
        .map(|s| s.signature.clone())
        .collect();
    Ok((None,json!({"statements":[statement("A bounded recent signature page was read; it is not the wallet's complete history.")],
        "signatures":refs,"related":[]}),Some("history is limited to a recent page and retained transaction leads; missing times cannot establish interval coverage".into())))
}

fn solana_curve(
    client: &RpcClient,
    case: &CaseKey,
    budget: &mut Budget,
) -> Result<ReadParts, String> {
    let mint: Address = case
        .address
        .parse()
        .map_err(|e: realorrug_types::AddressParseError| e.to_string())?;
    let address = realorrug_pumpfun::pda::bonding_curve(&mint).ok_or("curve derivation failed")?;
    let read = client
        .accounts(budget, &[address])
        .map_err(|e| e.to_string())?;
    let account = read
        .accounts
        .first()
        .and_then(Option::as_ref)
        .ok_or("Pump curve unavailable")?;
    if account
        .owner
        .as_deref()
        .and_then(|owner| owner.parse::<Address>().ok())
        != Some(realorrug_pumpfun::pda::PROGRAM_ID)
    {
        return Err("curve program identity mismatch".into());
    }
    let curve =
        realorrug_pumpfun::BondingCurve::parse(&account.data).map_err(|e| format!("{e:?}"))?;
    let facts = vec![
        statement(format!(
            "Pump bonding curve reports graduation complete: {}.",
            curve.complete
        )),
        statement(format!(
            "Curve real quote reserve: {} raw units; real token reserve: {} base units. Quote currency requires a separate check.",
            curve.real_sol_reserves, curve.real_token_reserves
        )),
    ];
    let mut value = json!({"statements":facts,"related":[curve.creator.to_string()]});
    let gap = if curve.complete {
        match solana_pool(client, &mint, budget) {
            Ok((pool, value_pool)) => {
                value["pumpswap"] = value_pool;
                value["related"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(pool.to_string()));
                let statements = value["pumpswap"]["statements"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                value["statements"]
                    .as_array_mut()
                    .unwrap()
                    .extend(statements);
                Some("graduation is not a drain; this pool's current reserves do not establish LP withdrawal rights or full historical changes".into())
            }
            Err(e) => Some(format!(
                "graduation is not a drain; PumpSwap pool read unavailable: {e}"
            )),
        }
    } else {
        Some("curve state alone does not establish historical liquidity changes or platform upgrade powers".into())
    };
    Ok((read.slot.map(|s| s.to_string()), value, gap))
}

fn solana_pool(
    client: &RpcClient,
    mint: &Address,
    budget: &mut Budget,
) -> Result<(Address, Value), String> {
    let largest = client
        .token_largest_accounts(budget, mint)
        .map_err(|e| e.to_string())?;
    let keys: Vec<Address> = largest.iter().take(5).map(|a| a.address).collect();
    if keys.is_empty() {
        return Err("no pool-discovery candidates".into());
    }
    let accounts = client.accounts(budget, &keys).map_err(|e| e.to_string())?;
    for account in accounts.accounts.iter().flatten() {
        let Some(owner) = account.owner.as_deref() else {
            continue;
        };
        let Ok(program) = owner.parse::<Address>() else {
            continue;
        };
        let Ok(token) = realorrug_pumpfun::token::TokenAccount::parse(&account.data, &program)
        else {
            continue;
        };
        if token.mint != *mint {
            continue;
        }
        let Ok(pool) = crate::reserves::read(client, budget, &token.owner) else {
            continue;
        };
        if pool.pool.base_mint != *mint && pool.pool.quote_mint != *mint {
            continue;
        }
        return Ok((
            pool.address,
            json!({"pool":pool.address.to_string(),"slot":pool.slot,"base_mint":pool.pool.base_mint.to_string(),"quote_mint":pool.pool.quote_mint.to_string(),
            "statements":[statement(format!("PumpSwap pool {} at slot {} reports raw base reserve {} and raw quote reserve {}; quote mint {}. These are separate asset units.",pool.address,pool.slot,pool.base.raw,pool.quote.raw,pool.pool.quote_mint))]}),
        ));
    }
    Err(
        "no verified PumpSwap pool among five largest-account candidates; other pools may exist"
            .into(),
    )
}
