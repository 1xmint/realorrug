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
    decoded_account(
        read,
        &address,
        account,
        accounts.slot.map(|s| s.to_string()),
    )
}

fn decoded_account(
    read: &Read,
    address: &Address,
    account: &crate::OwnedAccount,
    point: Option<String>,
) -> Result<ReadParts, String> {
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
    Ok((point, json!({"statements":facts,"related":[]}), gap))
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
    decoded_transaction(case, &tx)
}

fn decoded_transaction(case: &CaseKey, tx: &crate::rpc::Transaction) -> Result<ReadParts, String> {
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

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::rpc::{RawInstruction, Transaction};
    const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
    pub(crate) fn rpc(responses: Vec<(&'static str, Value)>) -> RpcClient {
        struct Canned(std::sync::Mutex<std::collections::VecDeque<(&'static str, Value)>>);
        impl crate::rpc::Transport for Canned {
            fn post(&self, _: &str, body: String) -> Result<String, String> {
                let (method, value) = self
                    .0
                    .lock()
                    .unwrap()
                    .pop_front()
                    .ok_or("unexpected RPC call")?;
                let request: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(request["method"], method);
                Ok(json!({"jsonrpc":"2.0","id":1,"result":value}).to_string())
            }
        }
        RpcClient::with_transport(
            "http://test.invalid",
            Box::new(Canned(std::sync::Mutex::new(responses.into()))),
        )
    }
    pub(crate) fn account(data: &[u8], owner: &str) -> Value {
        json!({"data":[realorrug_types::b64::encode(data),"base64"],"owner":owner})
    }

    #[test]
    fn pump_curve_requires_its_program_and_carries_state_as_raw_quote_units() {
        let case = CaseKey::new(Network::Solana, &Address::new([1; 32]).to_string()).unwrap();
        let mut bytes = vec![0; 81];
        bytes[..8].copy_from_slice(&realorrug_pumpfun::curve::DISCRIMINATOR);
        bytes[49..81].fill(3);
        bytes[24..32].copy_from_slice(&700u64.to_le_bytes());
        bytes[32..40].copy_from_slice(&800u64.to_le_bytes());
        for owner in [
            realorrug_pumpfun::pda::PROGRAM_ID.to_string(),
            Address::new([9; 32]).to_string(),
        ] {
            let client = rpc(vec![(
                "getMultipleAccounts",
                json!({"context":{"slot":42},"value":[account(&bytes,&owner)]}),
            )]);
            let result = solana_curve(&client, &case, &mut Budget::default());
            if owner != realorrug_pumpfun::pda::PROGRAM_ID.to_string() {
                assert!(result.unwrap_err().contains("program identity"));
                continue;
            }
            let (point, value, gap) = result.unwrap();
            assert_eq!(point.as_deref(), Some("42"));
            assert!(
                value["statements"][0]["text"]
                    .as_str()
                    .unwrap()
                    .contains("complete: false")
            );
            assert!(
                value["statements"][1]["text"]
                    .as_str()
                    .unwrap()
                    .contains("800 raw units; real token reserve: 700")
            );
            assert!(gap.unwrap().contains("historical liquidity"));
        }
    }

    #[test]
    fn pool_discovery_requires_the_requested_mint_in_both_candidate_account_and_verified_pool() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../realorrug-pumpfun/tests/fixtures/pumpswap_reserves.json"
        ))
        .unwrap();
        let captured = &fixture["reads"][0];
        let accounts = captured["accounts"].as_array().unwrap();
        let find = |role: &str| accounts.iter().find(|a| a["role"] == role).unwrap();
        let encoded = |row: &Value| json!({"data":[row["data_b64"],"base64"],"owner":row["owner"]});
        let pool = find("pool");
        for role in ["base_vault", "quote_vault"] {
            let vault = find(role);
            let mint = find(if role == "base_vault" {
                "base_mint"
            } else {
                "quote_mint"
            })["address"]
                .as_str()
                .unwrap()
                .parse::<Address>()
                .unwrap();
            for mismatch in [0, 1, 2] {
                let requested = if mismatch == 0 {
                    mint
                } else {
                    Address::new([9; 32])
                };
                let mut candidate = encoded(vault);
                if mismatch == 2 {
                    let mut data =
                        realorrug_types::b64::decode(vault["data_b64"].as_str().unwrap()).unwrap();
                    data[..32].copy_from_slice(requested.as_bytes());
                    candidate = account(&data, vault["owner"].as_str().unwrap());
                }
                let mut responses = vec![
                    (
                        "getTokenLargestAccounts",
                        json!({"value":[{"address":vault["address"],"amount":"100"}]}),
                    ),
                    (
                        "getMultipleAccounts",
                        json!({"context":{"slot":42},"value":[candidate]}),
                    ),
                ];
                if mismatch != 1 {
                    responses.extend([("getAccountInfo",json!({"context":{"slot":43},"value":encoded(pool)})),("getMultipleAccounts",json!({"context":{"slot":44},"value":[encoded(pool),encoded(find("base_mint")),encoded(find("quote_mint")),encoded(find("base_vault")),encoded(find("quote_vault"))]}))]);
                }
                let result = solana_pool(&rpc(responses), &requested, &mut Budget::default());
                if mismatch != 0 {
                    assert!(result.unwrap_err().contains("no verified"));
                    continue;
                }
                let (address, value) = result.unwrap();
                assert_eq!(address.to_string(), captured["pool"].as_str().unwrap());
                assert_eq!(value["slot"], 44);
                assert!(
                    value["statements"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("raw base reserve")
                );
            }
        }
    }

    #[test]
    fn solana_mints_need_a_supported_owner_and_initialized_authority_layout() {
        let address = Address::new([1; 32]);
        let mut read = Read {
            tool: Tool::Token,
            subject: address.to_string(),
            why: "controls".into(),
        };
        let mut account = crate::OwnedAccount {
            data: vec![0; 82],
            owner: Some(TOKEN.into()),
        };
        account.data[45] = 1;
        let (point, value, gap) =
            decoded_account(&read, &address, &account, Some("42".into())).unwrap();
        assert_eq!(point.as_deref(), Some("42"));
        assert!(
            value["statements"][1]["text"]
                .as_str()
                .unwrap()
                .contains("Mint authority: revoked. Freeze authority: revoked.")
        );
        assert_eq!(gap, None);
        account.data[..4].copy_from_slice(&1u32.to_le_bytes());
        account.data[4..36].fill(2);
        account.data[46..50].copy_from_slice(&1u32.to_le_bytes());
        account.data[50..82].fill(3);
        account.owner = Some("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb".into());
        let (_, value, gap) = decoded_account(&read, &address, &account, None).unwrap();
        let words = value["statements"][1]["text"].as_str().unwrap();
        assert!(words.contains(&Address::new([2; 32]).to_string()));
        assert!(words.contains(&Address::new([3; 32]).to_string()));
        assert!(gap.unwrap().contains("extension controls"));
        for owner in [None, Some("11111111111111111111111111111111".into())] {
            account.owner = owner;
            assert!(decoded_account(&read, &address, &account, None).is_err());
        }
        account.owner = Some(TOKEN.into());
        account.data[45] = 0;
        assert!(decoded_account(&read, &address, &account, None).is_err());
        account.data.truncate(81);
        assert!(decoded_account(&read, &address, &account, None).is_err());
        read.tool = Tool::Account;
        let (_, value, _) = decoded_account(&read, &address, &account, None).unwrap();
        assert_eq!(value["statements"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn solana_payment_claims_require_success_and_matching_instruction_program_mint_and_shape() {
        let case = CaseKey::new(Network::Solana, &Address::new([1; 32]).to_string()).unwrap();
        let mut native = RawInstruction {
            program: "11111111111111111111111111111111".into(),
            data: 2u32.to_le_bytes().to_vec(),
            accounts: vec!["sender".into(), "recipient".into()],
            top_level: true,
        };
        native.data.extend_from_slice(&500u64.to_le_bytes());
        let mut token = RawInstruction {
            program: TOKEN.into(),
            data: vec![12],
            accounts: vec![
                "source-account".into(),
                case.address.clone(),
                "destination-account".into(),
                "authority".into(),
            ],
            top_level: false,
        };
        token.data.extend_from_slice(&900u64.to_le_bytes());
        token.data.push(6);
        let mut tx = Transaction {
            slot: realorrug_types::Slot(42),
            accounts: vec![],
            instructions: vec![native.clone(), token.clone()],
            pre_token_balances: vec![],
            post_token_balances: vec![],
            pre_balances: vec![],
            post_balances: vec![],
            failed: false,
            meta_present: true,
        };
        let (point, value, gap) = decoded_transaction(&case, &tx).unwrap();
        assert_eq!(point.as_deref(), Some("42"));
        assert_eq!(value["transfers"][0]["lamports"], "500");
        assert_eq!(value["token_transfers"][0]["amount"], "900");
        assert_eq!(value["related"], json!(["sender", "recipient"]));
        assert!(gap.unwrap().contains("unchecked/extension"));
        tx.failed = true;
        let (_, value, _) = decoded_transaction(&case, &tx).unwrap();
        assert!(
            value["statements"][0]["text"]
                .as_str()
                .unwrap()
                .contains("failed")
        );
        assert!(value.get("transfers").is_none());
        tx.meta_present = false;
        assert!(
            decoded_transaction(&case, &tx)
                .unwrap_err()
                .contains("metadata")
        );
        tx.meta_present = true;
        tx.failed = false;
        for mutation in 0..5 {
            let mut wrong_native = native.clone();
            let mut wrong_token = token.clone();
            match mutation {
                0 => {
                    wrong_native.program = "wrong-program".into();
                    wrong_token.program = "wrong-program".into();
                }
                1 => {
                    wrong_native.data.push(0);
                    wrong_token.data.push(0);
                }
                2 => {
                    wrong_native.data[0] = 3;
                    wrong_token.data[0] = 3;
                }
                3 => {
                    wrong_native.accounts.truncate(1);
                    wrong_token.accounts.truncate(3);
                }
                _ => {
                    wrong_native.data.truncate(11);
                    wrong_token.accounts[1] = Address::new([9; 32]).to_string();
                }
            }
            tx.instructions = vec![wrong_native, wrong_token];
            let (_, value, _) = decoded_transaction(&case, &tx).unwrap();
            assert_eq!(value["transfers"], json!([]));
            assert_eq!(value["token_transfers"], json!([]));
            assert_eq!(value["statements"], json!([]));
        }
    }
}
