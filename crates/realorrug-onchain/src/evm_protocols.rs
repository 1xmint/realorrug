// SPDX-License-Identifier: Apache-2.0
//! Deployment-scoped fee reads. Registered routes do not establish safety.
use crate::{
    Budget,
    cases::{CaseKey, Network},
    evm_investigation::{address_word, call, eth_call, hex_u64, word},
    investigation::statement,
};
use realorrug_robinhood::Rpc;
use serde_json::{Value, json};
use sha3::{Digest, Keccak256};
use std::fmt::Write as _;

const ZERO: &str = "0x0000000000000000000000000000000000000000";
const CLANKER: &str = "0xe85a59c628f7d27878aceb4bf3b35733630083a9";
const LOCKERS: [&str; 2] = [
    "0x29d17c1a8d851d7d4ca97fae97acadb398d9cce0",
    "0x63d2dfea64b3433f4071a98665bcd7ca14d93496",
];

fn selector(signature: &str) -> String {
    let hash = Keccak256::digest(signature.as_bytes());
    format!(
        "0x{:02x}{:02x}{:02x}{:02x}",
        hash[0], hash[1], hash[2], hash[3]
    )
}
fn query(
    rpc: &Rpc,
    budget: &mut Budget,
    address: &str,
    signature: &str,
    args: &str,
    block: &str,
) -> Result<Vec<String>, String> {
    words(&eth_call(
        rpc,
        budget,
        address,
        &format!("{}{args}", selector(signature)),
        block,
    )?)
}
fn words(hex: &str) -> Result<Vec<String>, String> {
    let digits = hex.strip_prefix("0x").ok_or("ABI prefix missing")?;
    if digits.is_empty()
        || digits.len() % 64 != 0
        || digits.len() > 65_536
        || !digits.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("bounded ABI words missing or malformed".into());
    }
    Ok(digits
        .as_bytes()
        .as_chunks::<64>()
        .0
        .iter()
        .map(|part| format!("0x{}", String::from_utf8_lossy(part)))
        .collect())
}
fn number(data: &[String], index: usize) -> Result<u128, String> {
    word(data.get(index).ok_or("ABI integer absent")?)
}
fn address(data: &[String], index: usize) -> Result<String, String> {
    address_word(data.get(index).ok_or("ABI address absent")?)
}
fn arg(address: &str) -> String {
    format!("{:0>64}", address.trim_start_matches("0x"))
}
fn dynamic(data: &[String], base: usize, index: usize) -> Result<Vec<String>, String> {
    let offset = usize::try_from(number(data, base + index)?).map_err(|e| e.to_string())?;
    if offset % 32 != 0 || offset / 32 < 11 {
        return Err("invalid dynamic tuple offset".into());
    }
    let start = base + offset / 32;
    let n = usize::try_from(number(data, start)?).map_err(|e| e.to_string())?;
    if n == 0 || n > 16 {
        return Err("recipient count exceeds decoder bound".into());
    }
    data.get(start + 1..start + 1 + n)
        .map(<[String]>::to_vec)
        .ok_or_else(|| "truncated recipient vector".into())
}

pub(crate) fn fees(
    rpc: &Rpc,
    case: &CaseKey,
    block: &str,
    budget: &mut Budget,
) -> Result<(Value, Option<String>), String> {
    let mut gaps = Vec::new();
    if case.chain == Network::Base {
        match clanker(rpc, case, block, budget) {
            Ok(result) => return Ok(result),
            Err(e) => gaps.push(format!("Clanker: {e}")),
        }
        match flaunch(rpc, case, block, budget) {
            Ok(result) => return Ok(result),
            Err(e) => gaps.push(format!("Flaunch: {e}")),
        }
    }
    let (value, gap) = match crate::evm_investigation::liquidity(rpc, case, block, budget) {
        Ok(result) => result,
        Err(error) => {
            gaps.push(error);
            return Err(gaps.join("; "));
        }
    };
    if let Some(gap) = gap {
        gaps.push(gap);
    }
    Ok((value, Some(gaps.join("; "))))
}

fn clanker(
    rpc: &Rpc,
    case: &CaseKey,
    block: &str,
    budget: &mut Budget,
) -> Result<(Value, Option<String>), String> {
    let deployment = query(
        rpc,
        budget,
        CLANKER,
        "tokenDeploymentInfo(address)",
        &arg(&case.address),
        block,
    )?;
    if number(&deployment, 0)? != 32 || address(&deployment, 1)? != case.address {
        return Err("not a registered v4.0 token".into());
    }
    let hook = address(&deployment, 2)?;
    let locker = address(&deployment, 3)?;
    if !LOCKERS.iter().any(|known| *known == locker) {
        return Err("unsupported locker generation".into());
    }
    let rewards = query(
        rpc,
        budget,
        &locker,
        "tokenRewards(address)",
        &arg(&case.address),
        block,
    )?;
    let value = clanker_rewards(&rewards, &case.address)?;
    let mut facts = Vec::new();
    let mut related = vec![locker.clone(), hook];
    for share in value["shares"].as_array().ok_or("shares missing")? {
        facts.push(statement(format!("Clanker v4 configured LP-reward share: {} basis points out of 10000 to {}; reward administrator {}.",share["bps"].as_str().ok_or("allocation absent")?,share["recipient"].as_str().ok_or("recipient absent")?,share["admin"].as_str().ok_or("administrator absent")?)));
        related.push(share["recipient"].as_str().unwrap_or("").to_owned());
    }
    facts.push(statement(format!("Registered LP locker {locker} reports {} position(s); this does not establish permanent liquidity.",value["num_positions"].as_str().ok_or("position count absent")?)));
    Ok((json!({"protocol":"clanker_v4_0","registry":CLANKER,"locker":locker,"configuration":value,"related":related,"statements":facts}),
        Some("configured LP-reward shares exclude factory/other fee bases; paid receipts, administrator changes, extensions, current hook fees and position withdrawal rights remain separate checks".into())))
}

fn clanker_rewards(data: &[String], token: &str) -> Result<Value, String> {
    if number(data, 0)? != 32 || address(data, 1)? != token {
        return Err("reward tuple target mismatch".into());
    }
    let bps = dynamic(data, 1, 8)?;
    let admins = dynamic(data, 1, 9)?;
    let recipients = dynamic(data, 1, 10)?;
    if bps.len() != admins.len() || bps.len() != recipients.len() {
        return Err("reward array lengths disagree".into());
    }
    let mut shares = Vec::new();
    let mut total = 0u128;
    for i in 0..bps.len() {
        let bps = word(&bps[i])?;
        if bps > 10_000 {
            return Err("invalid reward allocation".into());
        }
        total += bps;
        shares.push(json!({"bps":bps.to_string(),"admin":address_word(&admins[i])?,"recipient":address_word(&recipients[i])?}));
    }
    if total != 10_000 {
        return Err("reward allocation denominator mismatch".into());
    }
    Ok(
        json!({"shares":shares,"position_id":number(data,7)?.to_string(),"num_positions":number(data,8)?.to_string()}),
    )
}

fn flaunch(
    rpc: &Rpc,
    case: &CaseKey,
    block: &str,
    budget: &mut Budget,
) -> Result<(Value, Option<String>), String> {
    // Normal Base hooks only; Any/Game Mode have different semantics.
    for manager in [
        "0x23321f11a6d44fd1ab790044fdfde5758c902fdc",
        "0x588c683ecc450f8b2aadb13d7f63792b840425dc",
    ] {
        let Ok(key) = query(
            rpc,
            budget,
            manager,
            "poolKey(address)",
            &arg(&case.address),
            block,
        ) else {
            continue;
        };
        if key.len() != 5
            || address(&key, 4)? != manager
            || ![address(&key, 0)?, address(&key, 1)?].contains(&case.address)
        {
            continue;
        }
        let pool_id = flaunch_pool_id(&key)?;
        let config = query(
            rpc,
            budget,
            manager,
            "getPoolFeeDistribution(bytes32)",
            &pool_id,
            block,
        )?;
        if config.len() != 4 || number(&config, 3)? > 1 {
            return Err("unsupported Flaunch fee tuple".into());
        }
        let split = query(
            rpc,
            budget,
            manager,
            "feeSplit(bytes32,uint256)",
            &format!("{pool_id}{:064x}", 10_000u32),
            block,
        )?;
        quoted_split(&split)?;
        let creator = query(rpc, budget, &case.address, "creator()", "", block)
            .and_then(|v| address(&v, 0))?;
        let owner =
            query(rpc, budget, manager, "owner()", "", block).and_then(|v| address(&v, 0))?;
        let calculator = query(rpc, budget, manager, "feeCalculator()", "", block)
            .and_then(|v| address(&v, 0))?;
        let mut facts = vec![
            statement(format!(
                "Flaunch feeSplit() on a 10000-unit post-referral fee input quotes {} bid-wall units, {} creator units and {} protocol units; this is configuration, not receipts.",
                number(&split, 0)?,
                number(&split, 1)?,
                number(&split, 2)?
            )),
            statement(format!(
                "Flaunch creator() returns {creator}; hook owner() returns {owner}; fee calculator is {calculator}."
            )),
        ];
        let (route, route_gap) = match flaunch_route(
            rpc,
            budget,
            manager,
            &case.address,
            &creator,
            &pool_id,
            block,
        ) {
            Ok(route) => {
                facts.push(statement(format!("Flaunch revenue NFT {} token id {} reports owner {}; token, NFT and hook registration agree at this read.",route["nft"].as_str().ok_or("NFT absent")?,route["token_id"].as_str().ok_or("NFT id absent")?,creator)));
                facts.push(statement(format!("Flaunch hook reports fee escrow {} and bid wall {}; isBidWallEnabled() returns {} for this pool. Enabled state does not establish a purchase, burn or permanent liquidity.",route["fee_escrow"].as_str().ok_or("escrow absent")?,route["bid_wall"].as_str().ok_or("bid wall absent")?,route["bid_wall_enabled"].as_bool().ok_or("bid wall state absent")?)));
                (route, None)
            }
            Err(error) => (Value::Null, Some(error)),
        };
        let currencies = [address(&key, 0)?, address(&key, 1)?];
        facts.push(statement(format!("Registered Flaunch pool currencies are {} and {}; asset addresses alone do not establish quote value or backing.",currencies[0],currencies[1])));
        let mut related = vec![manager.to_owned(), creator, owner, calculator];
        related.extend(currencies.iter().cloned());
        for field in ["nft", "fee_escrow", "bid_wall"] {
            if let Some(address) = route[field].as_str() {
                related.push(address.to_owned());
            }
        }
        let (receipts, receipt_gap) = match distributed(rpc, budget, manager, &pool_id, block) {
            Ok(v) => (v, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        let gap = flaunch_gap(route_gap.as_deref(), receipt_gap.as_deref());
        return Ok((
            json!({"protocol":"flaunch_normal","hook":manager,"pool_id":format!("0x{pool_id}"),"fee_distribution_words":config,
            "currencies":currencies,"route":route,"route_gap":route_gap,
            "quoted_split":split,"distributed_events":receipts,"receipt_gap":receipt_gap,"related":related,"statements":facts}),
            Some(gap),
        ));
    }
    Err("no supported normal Flaunch pool identified".into())
}

fn flaunch_pool_id(key: &[String]) -> Result<String, String> {
    let raw = key
        .iter()
        .map(|s| s.trim_start_matches("0x"))
        .collect::<String>();
    Ok(encode_hex(&Keccak256::digest(hex_bytes(&raw)?)))
}

fn flaunch_gap(route_gap: Option<&str>, receipt_gap: Option<&str>) -> String {
    let mut gap = "feeSplit excludes referral waterfall and is not total trade fees; distribution events can accrue to escrow rather than reach a beneficiary; custom managers, dynamic fees, bid-wall liquidity, administrative changes and full historical receipts remain unresolved".to_owned();
    if let Some(error) = route_gap {
        write!(gap, "; route read unavailable: {error}").expect("writing a String cannot fail");
    }
    if let Some(error) = receipt_gap {
        write!(gap, "; distribution read unavailable: {error}")
            .expect("writing a String cannot fail");
    }
    gap
}

// These getters identify current custody and routing, not the access-control
// semantics of arbitrary managers or proof that the observed route is immutable.
fn flaunch_route(
    rpc: &Rpc,
    budget: &mut Budget,
    hook: &str,
    token: &str,
    creator: &str,
    pool_id: &str,
    block: &str,
) -> Result<Value, String> {
    let nft = scalar(rpc, budget, hook, "flaunchContract()", "", block)?;
    let nft = address_word(&nft)?;
    let token_nft = scalar(rpc, budget, token, "flaunch()", "", block)?;
    if nft == ZERO || address_word(&token_nft)? != nft {
        return Err("token and hook revenue NFT disagree or are absent".into());
    }
    let token_id = scalar(rpc, budget, &nft, "tokenId(address)", &arg(token), block)?;
    let id = word(&token_id)?;
    let owner = scalar(
        rpc,
        budget,
        &nft,
        "ownerOf(uint256)",
        &format!("{id:064x}"),
        block,
    )?;
    if address_word(&owner)? != creator {
        return Err("revenue NFT owner does not match creator()".into());
    }
    let registered = scalar(
        rpc,
        budget,
        &nft,
        "memecoin(uint256)",
        &format!("{id:064x}"),
        block,
    )?;
    if address_word(&registered)? != token {
        return Err("revenue NFT token registration does not match the case".into());
    }
    let (escrow_getter, escrow_args) = if hook == "0x588c683ecc450f8b2aadb13d7f63792b840425dc" {
        ("pairedTokenFeeEscrow(bytes32)", pool_id)
    } else {
        ("feeEscrow()", "")
    };
    let escrow = address_word(&scalar(
        rpc,
        budget,
        hook,
        escrow_getter,
        escrow_args,
        block,
    )?)?;
    let bid_wall = address_word(&scalar(rpc, budget, hook, "bidWall()", "", block)?)?;
    if escrow == ZERO || bid_wall == ZERO {
        return Err("fee escrow or bid wall is absent".into());
    }
    let enabled = word(&scalar(
        rpc,
        budget,
        &bid_wall,
        "isBidWallEnabled(bytes32)",
        pool_id,
        block,
    )?)?;
    if enabled > 1 {
        return Err("bid wall returned a malformed enabled state".into());
    }
    Ok(
        json!({"nft":nft,"token_id":id.to_string(),"owner":creator,"fee_escrow":escrow,"bid_wall":bid_wall,"bid_wall_enabled":enabled == 1}),
    )
}

fn scalar(
    rpc: &Rpc,
    budget: &mut Budget,
    contract: &str,
    signature: &str,
    args: &str,
    block: &str,
) -> Result<String, String> {
    let data = query(rpc, budget, contract, signature, args, block)?;
    if data.len() != 1 {
        return Err("route getter did not return exactly one ABI word".into());
    }
    Ok(data[0].clone())
}

fn distributed(
    rpc: &Rpc,
    budget: &mut Budget,
    manager: &str,
    pool_id: &str,
    block: &str,
) -> Result<Vec<Value>, String> {
    let event =
        Keccak256::digest(b"PoolFeesDistributed(bytes32,uint256,uint256,uint256,uint256,uint256)");
    let topic = format!("0x{}", encode_hex(&event));
    let end = hex_u64(block)?;
    let logs = call(
        rpc,
        budget,
        "eth_getLogs",
        json!([{"address":manager,"fromBlock":format!("0x{:x}",end.saturating_sub(2000)),"toBlock":block,"topics":[topic,format!("0x{pool_id}")]}]),
    )?;
    let logs = logs.as_array().ok_or("distribution logs absent")?;
    if logs.len() > 32 {
        return Err("distribution range exceeds receipt bound".into());
    }
    let mut values = Vec::new();
    for log in logs {
        if log["address"]
            .as_str()
            .is_none_or(|address| !address.eq_ignore_ascii_case(manager))
            || log["topics"][0].as_str() != Some(topic.as_str())
            || log["topics"][1].as_str() != Some(format!("0x{pool_id}").as_str())
        {
            return Err("distribution log does not match the requested pool and hook".into());
        }
        if log["removed"].as_bool() == Some(true) {
            return Err("distribution log was removed".into());
        }
        let amounts = words(log["data"].as_str().ok_or("event data absent")?)?;
        if amounts.len() != 5 {
            return Err("unsupported fee-distribution event layout".into());
        }
        values.push(json!({"transaction":log["transactionHash"],"block_hash":log["blockHash"],"amounts":amounts}));
    }
    Ok(values)
}

fn quoted_split(split: &[String]) -> Result<(), String> {
    if split.len() != 3 {
        return Err("Flaunch split is not a complete quoted allocation".into());
    }
    let amounts = [number(split, 0)?, number(split, 1)?, number(split, 2)?];
    // Bound each untrusted ABI word before summing, including in debug builds.
    if amounts.iter().any(|amount| *amount > 10_000) || amounts.iter().sum::<u128>() != 10_000 {
        return Err("Flaunch split is not a complete quoted allocation".into());
    }
    Ok(())
}

fn hex_bytes(digits: &str) -> Result<Vec<u8>, String> {
    if !digits.len().is_multiple_of(2) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("malformed bytes".into());
    }
    (0..digits.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&digits[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

pub(crate) fn v3(
    rpc: &Rpc,
    case: &CaseKey,
    block: &str,
    budget: &mut Budget,
) -> Result<(Value, Option<String>), String> {
    let (factory, weth) = if case.chain == Network::Ethereum {
        (
            "0x1f98431c8ad98523631ae4a59f267346ea31f984",
            "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
        )
    } else {
        (
            "0x33128a8fc17869897dce68ed026d694621f6fdfd",
            "0x4200000000000000000000000000000000000006",
        )
    };
    for tier in [500u32, 3000, 10_000, 100] {
        let pool = query(
            rpc,
            budget,
            factory,
            "getPool(address,address,uint24)",
            &format!("{}{}{:064x}", arg(&case.address), arg(weth), tier),
            block,
        )
        .and_then(|v| address(&v, 0))?;
        if pool == ZERO {
            continue;
        }
        let actual =
            query(rpc, budget, &pool, "factory()", "", block).and_then(|v| address(&v, 0))?;
        if actual != factory {
            return Err("v3 pool factory mismatch".into());
        }
        let liquidity =
            query(rpc, budget, &pool, "liquidity()", "", block).and_then(|v| number(&v, 0))?;
        let fee = query(rpc, budget, &pool, "fee()", "", block).and_then(|v| number(&v, 0))?;
        if fee != u128::from(tier) {
            return Err("v3 fee tier mismatch".into());
        }
        let state = query(rpc, budget, &pool, "slot0()", "", block)?;
        if state.len() != 7 {
            return Err("unsupported v3 slot0 layout".into());
        }
        return Ok((json!({"protocol":"uniswap_v3","pool":pool,"fee_tier":tier,"slot0":state,"related":[pool],
            "statements":[statement(format!("Verified Uniswap v3 pool {pool} reports active liquidity {liquidity} and fee tier {tier} millionths; liquidity is not a token reserve or locked-LP proof."))]}),
            Some("only the first WETH pool at an inspected standard tier is covered; inactive ranges, NFT ownership/withdrawal rights, other venues and realized fees remain unresolved".into())));
    }
    Err("no supported Uniswap v2/v3 WETH pool identified; custom protocols and quote assets remain unsupported".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn pool_key_hex_preserves_every_byte_and_refuses_malformed_input() {
        let bytes = (0..=255u8).collect::<Vec<_>>();
        let hex = encode_hex(&bytes);
        assert_eq!(hex_bytes(&hex).unwrap(), bytes);
        assert_eq!(hex_bytes(&hex.to_uppercase()).unwrap(), bytes);
        assert_eq!(hex_bytes("").unwrap(), [] as [u8; 0]);
        for invalid in ["0", "000", "gg", "00gg", "é", "0x00"] {
            assert_eq!(hex_bytes(invalid).unwrap_err(), "malformed bytes");
        }
    }

    #[test]
    fn flaunch_pool_identity_and_fee_tuple_layout_must_match_before_a_quote() {
        let case =
            CaseKey::new(Network::Base, "0x1111111111111111111111111111111111111111").unwrap();
        let manager = "0x23321f11a6d44fd1ab790044fdfde5758c902fdc";
        let valid = vec![
            format!("0x{}", arg(&case.address)),
            numeric(&[0])[0].clone(),
            numeric(&[3000])[0].clone(),
            numeric(&[60])[0].clone(),
            format!("0x{}", arg(manager)),
        ];
        for change in 0..3 {
            let mut key = valid.clone();
            match change {
                0 => key.truncate(4),
                1 => key[4] = format!("0x{}", arg(ZERO)),
                _ => key[0] = format!("0x{}", arg(ZERO)),
            }
            let (rpc, server) =
                endpoint(vec![("eth_call", encoded(&key)), ("eth_call", json!("0x"))]);
            let result = flaunch(&rpc, &case, "0x1000", &mut Budget::default());
            server.join().unwrap();
            assert_eq!(
                result.unwrap_err(),
                "no supported normal Flaunch pool identified"
            );
        }
        for tuple in [
            numeric(&[2500, 5000, 2500]),
            numeric(&[2500, 5000, 2500, 0, 0]),
        ] {
            let (rpc, server) = endpoint(vec![
                ("eth_call", encoded(&valid)),
                ("eth_call", encoded(&tuple)),
            ]);
            let result = flaunch(&rpc, &case, "0x1000", &mut Budget::default());
            server.join().unwrap();
            assert_eq!(result.unwrap_err(), "unsupported Flaunch fee tuple");
        }
    }
    use super::*;
    use crate::evm_investigation::tests::endpoint;
    fn encoded(data: &[String]) -> Value {
        json!(format!(
            "0x{}",
            data.iter()
                .map(|word| word.trim_start_matches("0x"))
                .collect::<String>()
        ))
    }
    fn numeric(values: &[u128]) -> Vec<String> {
        values
            .iter()
            .map(|value| format!("0x{value:064x}"))
            .collect()
    }

    #[test]
    fn abi_bounds_accept_complete_vectors_at_the_limit_and_refuse_missing_bytes() {
        assert_eq!(
            words(&format!("0x{}", "0".repeat(65_536))).unwrap().len(),
            1024
        );
        assert!(words(&format!("0x{}", "0".repeat(65_600))).is_err());
        assert!(words(&format!("0x{}", "g".repeat(64))).is_err());
        let mut tuple = numeric(&[0; 29]);
        tuple[0] = numeric(&[32])[0].clone();
        tuple[1] = numeric(&[352])[0].clone();
        tuple[12] = numeric(&[16])[0].clone();
        assert_eq!(dynamic(&tuple, 1, 0).unwrap().len(), 16);
        tuple[12] = numeric(&[17])[0].clone();
        assert!(dynamic(&tuple, 1, 0).is_err());
        tuple[12] = numeric(&[16])[0].clone();
        assert!(dynamic(&tuple[..28], 1, 0).is_err());
        for count in [0, 17, 18] {
            tuple[12] = numeric(&[count])[0].clone();
            assert_eq!(
                dynamic(&tuple, 1, 0).unwrap_err(),
                "recipient count exceeds decoder bound"
            );
        }
        for offset in [0, 32, 320, 353] {
            tuple[1] = numeric(&[offset])[0].clone();
            assert_eq!(
                dynamic(&tuple, 1, 0).unwrap_err(),
                "invalid dynamic tuple offset"
            );
        }
    }

    #[test]
    fn flaunch_normal_quotes_exclude_referrals_and_do_not_claim_a_beneficiary_receipt() {
        let case =
            CaseKey::new(Network::Base, "0x1111111111111111111111111111111111111111").unwrap();
        for (manager, previous) in [
            ("0x23321f11a6d44fd1ab790044fdfde5758c902fdc", false),
            ("0x588c683ecc450f8b2aadb13d7f63792b840425dc", true),
        ] {
            let key = vec![
                format!("0x{}", arg(&case.address)),
                numeric(&[0])[0].clone(),
                numeric(&[3000])[0].clone(),
                numeric(&[60])[0].clone(),
                format!("0x{}", arg(manager)),
            ];
            for enabled in [0, 1, 2] {
                let mut responses = Vec::new();
                if previous {
                    responses.push(("eth_call", json!("0x")));
                }
                responses.extend([
                    ("eth_call", encoded(&key)),
                    ("eth_call", encoded(&numeric(&[2500, 5000, 2500, enabled]))),
                ]);
                if enabled < 2 {
                    responses.extend([
                        ("eth_call", encoded(&numeric(&[2500, 5000, 2500]))),
                        ("eth_call", json!(format!("0x{}", arg(&case.address)))),
                        ("eth_call", json!(format!("0x{}", arg(ZERO)))),
                        ("eth_call", json!(format!("0x{}", arg(ZERO)))),
                    ]);
                    responses.extend(route_responses(&case.address, &case.address, 1));
                    responses.push(("eth_getLogs", json!([])));
                }
                let (rpc, server) = endpoint(responses);
                let result = flaunch(&rpc, &case, "0x1000", &mut Budget::default());
                server.join().unwrap();
                if enabled == 2 {
                    assert!(result.unwrap_err().contains("unsupported"));
                    continue;
                }
                let (value, gap) = result.unwrap();
                assert_eq!(value["protocol"], "flaunch_normal");
                assert_eq!(value["hook"], manager);
                assert_eq!(
                    value["pool_id"],
                    if previous {
                        "0x2c909d3c79e44a317fda913534a963b179a06319c76211f3f8812fd6d40c4c78"
                    } else {
                        "0x11228e6f6c3484147b8a623637089377d9317ccd8daed797414b2c4b2ebb4131"
                    }
                );
                assert_eq!(value["distributed_events"], json!([]));
                assert_eq!(value["route"]["owner"], case.address);
                assert_eq!(value["route"]["bid_wall_enabled"], true);
                assert_eq!(value["route_gap"], Value::Null);
                assert_eq!(value["currencies"], json!([case.address, ZERO]));
                assert_eq!(
                    value["related"],
                    json!([
                        manager,
                        case.address,
                        ZERO,
                        ZERO,
                        case.address,
                        ZERO,
                        LOCKERS[0],
                        LOCKERS[1],
                        CLANKER
                    ])
                );
                assert!(
                    value["statements"][0]["text"].as_str().unwrap().contains(
                        "2500 bid-wall units, 5000 creator units and 2500 protocol units"
                    )
                );
                assert!(gap.unwrap().contains("referral waterfall"));
            }
        }
    }

    fn route_responses(token: &str, creator: &str, enabled: u128) -> Vec<(&'static str, Value)> {
        let address = |address| json!(format!("0x{}", arg(address)));
        vec![
            ("eth_call", address(LOCKERS[0])),
            ("eth_call", address(LOCKERS[0])),
            ("eth_call", encoded(&numeric(&[2010]))),
            ("eth_call", address(creator)),
            ("eth_call", address(token)),
            ("eth_call", address(LOCKERS[1])),
            ("eth_call", address(CLANKER)),
            ("eth_call", encoded(&numeric(&[enabled]))),
        ]
    }

    #[test]
    fn flaunch_route_checks_registration_custody_and_boolean_without_claiming_execution() {
        let token = "0x1111111111111111111111111111111111111111";
        let creator = "0x2222222222222222222222222222222222222222";
        let hook = "0x588c683ecc450f8b2aadb13d7f63792b840425dc";
        for (hook, escrow_call) in [
            (hook, format!("0x15200ca1{}", "ab".repeat(32))),
            (
                "0x23321f11a6d44fd1ab790044fdfde5758c902fdc",
                "0xc4b7de97".into(),
            ),
        ] {
            for enabled in [0, 1] {
                let params = [
                    (hook, "0x84aa1da0".to_owned()),
                    (token, "0x87211ceb".to_owned()),
                    (LOCKERS[0], format!("0x7ca31724{}", arg(token))),
                    (LOCKERS[0], format!("0x6352211e{:064x}", 2010)),
                    (LOCKERS[0], format!("0xe49c1854{:064x}", 2010)),
                    (hook, escrow_call.clone()),
                    (hook, "0xba3e69b7".to_owned()),
                    (CLANKER, format!("0x4e944d57{}", "ab".repeat(32))),
                ];
                let responses = route_responses(token, creator, enabled)
                    .into_iter()
                    .zip(params)
                    .map(|((method, result), (to, data))| {
                        (method, result, Some(json!([{"to":to,"data":data},"0x10"])))
                    })
                    .collect();
                let (rpc, server) = crate::evm_investigation::tests::endpoint_checked(responses);
                let mut budget = Budget::default();
                let value = flaunch_route(
                    &rpc,
                    &mut budget,
                    hook,
                    token,
                    creator,
                    &"ab".repeat(32),
                    "0x10",
                )
                .unwrap();
                server.join().unwrap();
                assert_eq!(budget.calls_made(), 8);
                assert_eq!(
                    value,
                    json!({"nft":LOCKERS[0],"token_id":"2010","owner":creator,"fee_escrow":LOCKERS[1],"bid_wall":CLANKER,"bid_wall_enabled":enabled == 1})
                );
            }
        }
    }

    #[test]
    fn flaunch_route_refuses_mismatched_registration_roles_missing_contracts_and_bad_words() {
        let token = "0x1111111111111111111111111111111111111111";
        let creator = "0x2222222222222222222222222222222222222222";
        let hook = "0x588c683ecc450f8b2aadb13d7f63792b840425dc";
        for (index, result, calls, reason) in [
            (
                0,
                encoded(&numeric(&[0])),
                2,
                "token and hook revenue NFT disagree or are absent",
            ),
            (
                1,
                json!(format!("0x{}", arg(ZERO))),
                2,
                "token and hook revenue NFT disagree or are absent",
            ),
            (
                2,
                encoded(&numeric(&[1, 2])),
                3,
                "route getter did not return exactly one ABI word",
            ),
            (
                3,
                json!(format!("0x{}", arg(token))),
                4,
                "revenue NFT owner does not match creator()",
            ),
            (
                4,
                json!(format!("0x{}", arg(creator))),
                5,
                "revenue NFT token registration does not match the case",
            ),
            (
                5,
                encoded(&numeric(&[0])),
                7,
                "fee escrow or bid wall is absent",
            ),
            (
                6,
                encoded(&numeric(&[0])),
                7,
                "fee escrow or bid wall is absent",
            ),
            (
                7,
                encoded(&numeric(&[2])),
                8,
                "bid wall returned a malformed enabled state",
            ),
        ] {
            let mut responses = route_responses(token, creator, 1);
            responses[index].1 = result;
            responses.truncate(calls);
            let (rpc, server) = endpoint(responses);
            let mut budget = Budget::default();
            assert_eq!(
                flaunch_route(
                    &rpc,
                    &mut budget,
                    hook,
                    token,
                    creator,
                    &"ab".repeat(32),
                    "0x10"
                )
                .unwrap_err(),
                reason
            );
            server.join().unwrap();
            assert_eq!(budget.calls_made(), u32::try_from(calls).unwrap());
        }
    }

    #[test]
    fn unavailable_flaunch_route_and_receipts_preserve_quote_and_surface_public_gaps() {
        let token = "0x1111111111111111111111111111111111111111";
        let creator = "0x2222222222222222222222222222222222222222";
        let hook = "0x588c683ecc450f8b2aadb13d7f63792b840425dc";
        // An unavailable optional route must preserve the measured quote and
        // reach the public gap, without inventing a recipient or empty receipt.
        let key = vec![
            format!("0x{}", arg(token)),
            numeric(&[0])[0].clone(),
            numeric(&[3000])[0].clone(),
            numeric(&[60])[0].clone(),
            format!("0x{}", arg(hook)),
        ];
        let (rpc, server) = endpoint(vec![
            ("eth_call", json!("0x")),
            ("eth_call", encoded(&key)),
            ("eth_call", encoded(&numeric(&[2500, 5000, 2500, 1]))),
            ("eth_call", encoded(&numeric(&[2500, 5000, 2500]))),
            ("eth_call", json!(format!("0x{}", arg(creator)))),
            ("eth_call", json!(format!("0x{}", arg(ZERO)))),
            ("eth_call", json!(format!("0x{}", arg(ZERO)))),
            ("eth_call", json!("0x")),
            ("eth_getLogs", Value::Null),
        ]);
        let (value, gap) = flaunch(
            &rpc,
            &CaseKey::new(Network::Base, token).unwrap(),
            "0x10",
            &mut Budget::default(),
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(value["route"], Value::Null);
        assert_eq!(value["route_gap"], "bounded ABI words missing or malformed");
        assert_eq!(value["receipt_gap"], "distribution logs absent");
        assert_eq!(value["quoted_split"], json!(numeric(&[2500, 5000, 2500])));
        assert_eq!(
            value["related"],
            json!([hook, creator, ZERO, ZERO, token, ZERO])
        );
        assert_eq!(value["statements"].as_array().unwrap().len(), 3);
        let gap = gap.unwrap();
        assert!(gap.contains("route read unavailable: bounded ABI words missing or malformed"));
        assert!(gap.contains("distribution read unavailable: distribution logs absent"));
    }

    #[test]
    fn distribution_logs_require_the_requested_hook_pool_and_complete_nonremoved_layout() {
        let manager = "0x23321f11a6d44fd1ab790044fdfde5758c902fdc";
        let pool = "ab".repeat(32);
        let topic = format!(
            "0x{}",
            encode_hex(&Keccak256::digest(
                b"PoolFeesDistributed(bytes32,uint256,uint256,uint256,uint256,uint256)"
            ))
        );
        let good = json!({"address":manager,"topics":[topic,format!("0x{pool}")],"data":encoded(&numeric(&[1,2,3,4,5])),"transactionHash":"synthetic-receipt","blockHash":"synthetic-block"});
        let (rpc, server) = endpoint(vec![("eth_getLogs", json!(vec![good.clone(); 32]))]);
        let receipts = distributed(&rpc, &mut Budget::default(), manager, &pool, "0x1000").unwrap();
        server.join().unwrap();
        assert_eq!(receipts.len(), 32);
        assert_eq!(receipts[0]["transaction"], "synthetic-receipt");
        assert_eq!(receipts[0]["amounts"], json!(numeric(&[1, 2, 3, 4, 5])));
        for mutation in 0..6 {
            let mut invalid = good.clone();
            match mutation {
                0 => invalid["address"] = json!(ZERO),
                1 => invalid["topics"][0] = json!("wrong-event"),
                2 => invalid["topics"][1] = json!("wrong-pool"),
                3 => invalid["removed"] = json!(true),
                4 => invalid["data"] = encoded(&numeric(&[1, 2, 3, 4])),
                _ => invalid["data"] = encoded(&numeric(&[1, 2, 3, 4, 5, 6])),
            }
            let (rpc, server) = endpoint(vec![("eth_getLogs", json!([invalid]))]);
            assert!(distributed(&rpc, &mut Budget::default(), manager, &pool, "0x1000").is_err());
            server.join().unwrap();
        }
        let (rpc, server) = endpoint(vec![("eth_getLogs", json!(vec![good; 33]))]);
        assert!(
            distributed(&rpc, &mut Budget::default(), manager, &pool, "0x1000")
                .unwrap_err()
                .contains("bound")
        );
        server.join().unwrap();
    }

    #[test]
    fn v3_requires_factory_tier_and_slot_layout_and_does_not_equate_liquidity_with_locked_lp() {
        for chain in [Network::Base, Network::Ethereum] {
            let case = CaseKey::new(chain, "0x1111111111111111111111111111111111111111").unwrap();
            let factory = if chain == Network::Base {
                "0x33128a8fc17869897dce68ed026d694621f6fdfd"
            } else {
                "0x1f98431c8ad98523631ae4a59f267346ea31f984"
            };
            for mutation in 0..4 {
                let mut responses = vec![
                    ("eth_call", encoded(&numeric(&[0]))),
                    ("eth_call", encoded(&numeric(&[8]))),
                    (
                        "eth_call",
                        json!(format!(
                            "0x{}",
                            arg(if mutation == 1 { ZERO } else { factory })
                        )),
                    ),
                ];
                if mutation != 1 {
                    responses.extend([
                        ("eth_call", encoded(&numeric(&[700]))),
                        (
                            "eth_call",
                            encoded(&numeric(&[if mutation == 2 { 500 } else { 3000 }])),
                        ),
                    ]);
                }
                if mutation == 0 || mutation == 3 {
                    responses.push((
                        "eth_call",
                        encoded(&numeric(&vec![0; if mutation == 3 { 6 } else { 7 }])),
                    ));
                }
                let (rpc, server) = endpoint(responses);
                let result = v3(&rpc, &case, "0x10", &mut Budget::default());
                server.join().unwrap();
                if mutation != 0 {
                    assert!(result.is_err());
                    continue;
                }
                let (value, gap) = result.unwrap();
                assert_eq!(value["protocol"], "uniswap_v3");
                assert_eq!(value["fee_tier"], 3000);
                assert!(
                    value["statements"][0]["text"]
                        .as_str()
                        .unwrap()
                        .contains("active liquidity 700")
                );
                assert!(gap.unwrap().contains("withdrawal rights"));
            }
        }
    }
    fn rewards(token: &str) -> Vec<String> {
        let mut data = vec![format!("0x{:064x}", 0); 18];
        data[0] = format!("0x{:064x}", 32);
        data[1] = format!("0x{}", arg(token));
        for (slot, offset) in [(9, 352), (10, 416), (11, 480)] {
            data[slot] = format!("0x{offset:064x}");
        }
        for slot in [8, 12, 14, 16] {
            data[slot] = format!("0x{:064x}", 1);
        }
        data[13] = format!("0x{:064x}", 10_000);
        data[15] = format!("0x{}", arg(token));
        data[17] = data[15].clone();
        data
    }
    #[test]
    fn registered_clanker_routes_are_configuration_and_unknown_lockers_are_refused() {
        let case =
            CaseKey::new(Network::Base, "0x1111111111111111111111111111111111111111").unwrap();
        let deployment = vec![
            format!("0x{:064x}", 32),
            format!("0x{}", arg(&case.address)),
            format!("0x{}", arg(ZERO)),
            format!("0x{}", arg(LOCKERS[1])),
        ];
        let (rpc, server) = endpoint(vec![
            ("eth_call", encoded(&deployment)),
            ("eth_call", encoded(&rewards(&case.address))),
        ]);
        let mut budget = Budget::default();
        let (value, gap) = fees(&rpc, &case, "0x1", &mut budget).unwrap();
        server.join().unwrap();
        assert_eq!(budget.calls_made(), 2);
        assert_eq!(value["protocol"], "clanker_v4_0");
        assert_eq!(value["configuration"]["shares"][0]["bps"], "10000");
        assert_eq!(
            value["configuration"]["shares"][0]["recipient"],
            case.address
        );
        assert!(
            value["statements"][0]["text"]
                .as_str()
                .unwrap()
                .contains("10000 basis points")
        );
        assert!(gap.unwrap().contains("paid receipts"));
        for (index, replacement, reason) in [
            (0, format!("0x{:064x}", 0), "registered"),
            (1, format!("0x{}", arg(ZERO)), "registered"),
            (3, format!("0x{}", arg(ZERO)), "locker"),
        ] {
            let mut invalid = deployment.clone();
            invalid[index] = replacement;
            let (rpc, server) = endpoint(vec![("eth_call", encoded(&invalid))]);
            let mut budget = Budget::default();
            assert!(
                clanker(&rpc, &case, "0x1", &mut budget)
                    .unwrap_err()
                    .contains(reason)
            );
            assert_eq!(budget.calls_made(), 1);
            server.join().unwrap();
        }
    }
    #[test]
    fn reward_vectors_require_aligned_bounded_offsets_and_matching_recipients() {
        let token = "0x1111111111111111111111111111111111111111";
        let valid = rewards(token);
        for offset in [0, 31, 33, 320, usize::MAX] {
            let mut invalid = valid.clone();
            invalid[9] = format!("0x{offset:064x}");
            assert!(clanker_rewards(&invalid, token).is_err());
        }
        for count in [0, 17, 18] {
            let mut invalid = valid.clone();
            invalid[12] = format!("0x{count:064x}");
            assert!(clanker_rewards(&invalid, token).is_err());
        }
        assert!(clanker_rewards(&valid, ZERO).is_err());
        let mut invalid = valid.clone();
        invalid[14] = format!("0x{:064x}", 2);
        assert!(clanker_rewards(&invalid, token).is_err());
        let mut invalid = valid;
        invalid[13] = format!("0x{:064x}", 10_001);
        assert!(clanker_rewards(&invalid, token).is_err());
        assert_eq!(dynamic(&rewards(token), 1, 8).unwrap().len(), 1);
    }
    #[test]
    fn quoted_allocations_refuse_overflow_and_incomplete_or_excess_shares() {
        let split = |amounts: &[u128]| {
            amounts
                .iter()
                .map(|amount| format!("0x{amount:064x}"))
                .collect::<Vec<_>>()
        };
        quoted_split(&split(&[2500, 7000, 500])).unwrap();
        quoted_split(&split(&[10_000, 0, 0])).unwrap();
        for amounts in [
            vec![],
            vec![10_000],
            vec![2500, 7000, 499],
            vec![2500, 7000, 501],
            vec![u128::MAX, u128::MAX, 2],
        ] {
            assert!(quoted_split(&split(&amounts)).is_err());
        }
        assert_eq!(
            words(&format!("0x{}{}", "01".repeat(32), "ff".repeat(32)))
                .unwrap()
                .len(),
            2
        );
        assert!(words("0x").is_err());
        assert!(words("0x0").is_err());
        assert!(words(&format!("0x{}", "ab".repeat(32_800))).is_err());
    }

    #[test]
    fn dynamic_protocol_tuples_reject_truncation_wrong_target_and_denominators() {
        let token = "0x1111111111111111111111111111111111111111";
        let mut data = vec![format!("0x{:064x}", 0); 18];
        data[0] = format!("0x{:064x}", 32);
        data[1] = format!("0x{}", arg(token));
        for (slot, offset) in [(9, 352), (10, 416), (11, 480)] {
            data[slot] = format!("0x{offset:064x}");
        }
        for slot in [12, 14, 16] {
            data[slot] = format!("0x{:064x}", 1);
        }
        data[13] = format!("0x{:064x}", 10_000);
        data[15] = format!("0x{}", arg(token));
        data[17] = data[15].clone();
        assert_eq!(
            clanker_rewards(&data, token).unwrap()["shares"][0]["bps"],
            "10000"
        );
        assert!(clanker_rewards(&data[..17], token).is_err());
        data[13] = format!("0x{:064x}", 9500);
        assert!(clanker_rewards(&data, token).is_err());
        assert!(words("0xé").is_err());
    }

    #[test]
    fn clanker_multiple_reward_shares_preserve_roles_and_require_the_total_denominator() {
        let token = "0x1111111111111111111111111111111111111111";
        let mut data = numeric(&[0; 21]);
        data[0] = numeric(&[32])[0].clone();
        data[1] = format!("0x{}", arg(token));
        for (slot, offset) in [(9, 352), (10, 448), (11, 544)] {
            data[slot] = numeric(&[offset])[0].clone();
        }
        for slot in [12, 15, 18] {
            data[slot] = numeric(&[2])[0].clone();
        }
        data[16] = format!("0x{}", arg(token));
        data[17] = format!("0x{}", arg(ZERO));
        data[19] = format!("0x{}", arg(ZERO));
        data[20] = format!("0x{}", arg(token));
        for allocations in [[5000, 5000], [0, 10_000], [10_000, 0]] {
            data[13] = numeric(&[allocations[0]])[0].clone();
            data[14] = numeric(&[allocations[1]])[0].clone();
            let value = clanker_rewards(&data, token).unwrap();
            assert_eq!(value["shares"].as_array().unwrap().len(), 2);
            assert_eq!(value["shares"][0]["bps"], allocations[0].to_string());
            assert_eq!(value["shares"][1]["bps"], allocations[1].to_string());
            assert_eq!(value["shares"][0]["admin"], token);
            assert_eq!(value["shares"][0]["recipient"], ZERO);
            assert_eq!(value["shares"][1]["admin"], ZERO);
            assert_eq!(value["shares"][1]["recipient"], token);
        }
        data[13] = numeric(&[10_001])[0].clone();
        assert_eq!(
            clanker_rewards(&data, token).unwrap_err(),
            "invalid reward allocation"
        );
    }
}
