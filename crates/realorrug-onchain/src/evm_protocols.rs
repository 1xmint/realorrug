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
        let raw = key
            .iter()
            .map(|s| s.trim_start_matches("0x"))
            .collect::<String>();
        let bytes = hex_bytes(&raw)?;
        let hash = Keccak256::digest(&bytes);
        let pool_id = encode_hex(&hash);
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
        let facts = vec![
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
        let (receipts, receipt_gap) = match distributed(rpc, budget, manager, &pool_id, block) {
            Ok(v) => (v, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        return Ok((json!({"protocol":"flaunch_normal","hook":manager,"pool_id":format!("0x{pool_id}"),"fee_distribution_words":config,
            "quoted_split":split,"distributed_events":receipts,"receipt_gap":receipt_gap,"related":[creator,owner,calculator],"statements":facts}),
            Some("feeSplit excludes referral waterfall and is not total trade fees; distribution events can accrue to escrow rather than reach a beneficiary; custom managers, dynamic fees, bid-wall liquidity, administrative changes and full historical receipts remain unresolved".into())));
    }
    Err("no supported normal Flaunch pool identified".into())
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
        let (value, gap) = clanker(&rpc, &case, "0x1", &mut budget).unwrap();
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
}
