// SPDX-License-Identifier: Apache-2.0
//! Who funded the first buyers -- design 0027 §2.1, slice 3 of §3.
//!
//! # What this reads, and what it refuses to conclude
//!
//! A purchase is a `CurveBuy` the token's own curve emitted inside the launch
//! window. It is not a `Transfer`: a transfer can be a mint, a gift or
//! routing, and the holder walk in `robinhood.rs` already counts those for
//! what they are. The trade's `trader` (who called the curve, possibly a
//! router) and `recipient` (who got the tokens) stay distinct; the
//! *beneficiary* -- the recipient -- is the buyer this module investigates,
//! because it is the wallet that ends up holding the position.
//!
//! At most [`MAX_CANDIDATES`] buyers are checked (the selection rule is
//! [`SELECTION_RULE`], recorded on the result), and the result carries how
//! much of the observed buying those candidates account for, quote-weighted:
//! four wallets that bought 80% of the window say more than twenty that
//! bought dust. "3 of 4 checked" is a fact about four wallets; nothing here
//! turns it into a claim about every buyer.
//!
//! A funding edge is an observed native transfer into a buyer before its
//! first purchase. It proves that a transfer happened, and nothing about who
//! controls either wallet: an exchange hot wallet, a bridge and a launch
//! service all fund strangers. The sheet's wording (`realorrug-roast`) says
//! "the same wallet funded N of M checked" and stops there. Dust is kept as
//! an observation and never counts (`FundingEdge::material`), because a
//! creator can dust strangers on purpose to poison a shared-funder check.
//! A zero nonce before launch means the wallet had sent nothing on this
//! chain, not that it is a new person.
//!
//! # Cost
//!
//! Alchemy's compute-unit table, checked 2026-09-18 (design 0027 §2.4):
//! `alchemy_getAssetTransfers` 120, `eth_getCode` 20,
//! `eth_getTransactionCount` 20, `eth_getLogs` 60. One window read plus four
//! three-call candidates is 60 + 4 × 160 = 700 CU; [`FUNDING_MAX_CU`] caps
//! the per-candidate part at 640 so the enrichment can never eat the
//! dossier's whole 2,000-CU allowance. Core reads run before this one, so
//! the cap here is what is left after them, never more.
//!
//! # Slice 6b: the same investigation on Solana
//!
//! [`Funding`], [`Candidate`], [`Funder`] and [`SharedFunder`] are shared by
//! both chains -- design 0027 row 6 asks for identical finding types across
//! chains so the roast side never needs chain-specific wording. Every
//! address field is a `String` in the chain's own canonical text form
//! (0x-lowercase hex for Robinhood, base58 for Solana) rather than the
//! 20-byte EVM [`realorrug_robinhood::Address`], which cannot hold a Solana
//! key at all.
//!
//! [`investigate_solana`] is the Solana half. It has no `CurveBuy` log to
//! read purchases from, so it does not build [`Purchase`], [`Buyer`] or
//! [`Selection`] -- those three stay Robinhood-only, EVM-`Address`-typed
//! helpers that feed [`investigate`]. Its launch window is the mint's own
//! first [`SOLANA_WINDOW_TRANSACTIONS`] *successful* transactions, read
//! oldest first via [`RpcClient::signatures_back_to_oldest`]: every distinct
//! wallet whose balance of this mint rose in them (`post_token_balances`
//! against `pre_token_balances`, keyed by [`crate::rpc::TokenBalance::owner`])
//! -- excluding the proven bonding-curve PDA
//! ([`realorrug_pumpfun::pda::bonding_curve`]), which is the pool side of
//! every trade and never a beneficiary -- is a buyer in `Funding::buyers`.
//! The first [`MAX_CANDIDATES`] of them by first purchase are checked: a
//! separate read of that wallet's own oldest signature finds the native
//! transfer that funded it, counted only when it landed at or before that
//! wallet's first-purchase slot -- the same way [`investigate`] does not
//! need a `CurveBuy` for that half either. If the mint's own signature
//! history was truncated by the read budget before its window could be
//! read, none of the buyers found in the truncated read are "the early
//! buyers" (a budget that runs out first drops the oldest, undiscovered
//! page), so nothing is checked and the gap says so.

use std::collections::{BTreeMap, BTreeSet};

use realorrug_robinhood::pons::{Side, Trade, topic};
use realorrug_robinhood::{Address, Hash32, Log, LogsError, Rpc, quantity, quantity_u128};

use crate::budget::Budget;
use crate::memory::{CheckRun, Completeness, FundingEdge, Memory};
use crate::rpc::{RpcClient, Transaction};

/// `alchemy_getAssetTransfers`, per the CU table (2026-09-18).
pub const CU_GET_ASSET_TRANSFERS: u32 = 120;
/// `eth_getCode`, per the CU table (2026-09-18).
pub const CU_GET_CODE: u32 = 20;
/// `eth_getTransactionCount`, per the CU table (2026-09-18).
pub const CU_GET_TRANSACTION_COUNT: u32 = 20;
/// `eth_getLogs`, per the CU table (2026-09-18).
pub const CU_GET_LOGS: u32 = 60;

/// The most buyers one investigation checks.
pub const MAX_CANDIDATES: usize = 4;

/// The compute units the per-candidate reads may spend in one dossier:
/// four candidates at 160 CU each, before continuation pages.
pub const FUNDING_MAX_CU: u32 = 640;

/// What one candidate costs if its transfer history fits one page.
const CU_PER_CANDIDATE: u32 = CU_GET_CODE + CU_GET_ASSET_TRANSFERS + CU_GET_TRANSACTION_COUNT;

/// How many blocks after the launch block count as the launch window: about
/// ten minutes at the measured 0.101 s/block (research 0050 §8).
pub const LAUNCH_WINDOW_BLOCKS: u64 = 6_000;

/// How far before a buyer's first purchase its funding is read: about ten
/// hours at 0.101 s/block. Funding from a week earlier is context, not the
/// financing of this purchase (design 0027 §2.1).
pub const FUNDING_LOOKBACK_BLOCKS: u64 = 360_000;

/// Continuation pages one candidate's transfer history may take.
pub const MAX_FUNDING_PAGES: u32 = 2;

/// Gas a purchase is assumed to have needed, added to the quote before the
/// materiality test: 0.0001 ETH, generous for an L2.
pub const GAS_ALLOWANCE_WEI: u128 = 100_000_000_000_000;

/// Gas a Solana buy is assumed to have needed: 0.00005 SOL, ten times a
/// single-signature transaction's ~5,000-lamport base fee (measured
/// 2026-09-18), generous the same way [`GAS_ALLOWANCE_WEI`] is for an L2.
pub const GAS_ALLOWANCE_LAMPORTS: u128 = 50_000;

/// A transfer is material when it covers at least this share of the
/// purchase plus gas, in basis points. Half: a top-up that paid for less than
/// half the buy did not finance it.
pub const MATERIAL_SHARE_BPS: u128 = 5_000;

/// The selection rule, recorded with every result so a reader knows why
/// these four and not four others.
pub const SELECTION_RULE: &str = "the two largest launch-window buyers by quote, the earliest \
                                  remaining buyer, and the remaining buyer with the lowest \
                                  address (a deterministic sample)";

/// One `CurveBuy` in the launch window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Purchase {
    /// Who called the curve -- possibly a router.
    pub trader: Address,
    /// Who received the tokens: the buyer this module investigates.
    pub beneficiary: Address,
    /// Quote paid, in wei.
    pub quote: u128,
    /// The block it landed in.
    pub block: u64,
    /// Position within the block, for ordering; `(0, 0)` when the provider
    /// omitted it.
    pub position: (u64, u64),
    /// The transaction that carried it.
    pub transaction: Hash32,
}

/// Every purchase among `logs` made against `curve`.
///
/// Sells are not purchases, and any contract can emit a `CurveBuy`'s bytes,
/// so the emitter is compared to the curve (`Trade::from_log`'s own caveat).
#[must_use]
pub fn purchases_from(logs: &[Log], curve: &Address) -> Vec<Purchase> {
    logs.iter()
        .filter_map(|log| Trade::from_log(log).map(|t| (log, t)))
        .filter(|(_, t)| t.side == Side::Buy && t.curve == *curve)
        .map(|(log, t)| Purchase {
            trader: t.trader,
            beneficiary: t.recipient,
            quote: t.quote,
            block: log.block,
            position: log
                .position
                .map_or((0, 0), |p| (p.transaction_index, p.log_index)),
            transaction: log.transaction,
        })
        .collect()
}

/// One buyer: a beneficiary aggregated over its launch-window purchases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buyer {
    /// The beneficiary address.
    pub address: Address,
    /// Quote it bought with in the window, in wei.
    pub quote: u128,
    /// The block of its first purchase.
    pub first_block: u64,
    /// Position of its first purchase within that block.
    pub first_position: (u64, u64),
}

/// Buyers aggregated by beneficiary, in order of first purchase.
#[must_use]
pub fn buyers_of(purchases: &[Purchase]) -> Vec<Buyer> {
    let mut by_address: BTreeMap<[u8; 20], Buyer> = BTreeMap::new();
    for p in purchases {
        let buyer = by_address.entry(p.beneficiary.0).or_insert_with(|| Buyer {
            address: p.beneficiary,
            quote: 0,
            first_block: p.block,
            first_position: p.position,
        });
        buyer.quote = buyer.quote.saturating_add(p.quote);
        // `min` rather than a comparison: two purchases can never share a
        // position, so `<` and `<=` are the same here and a mutation test
        // cannot tell them apart.
        let first = (buyer.first_block, buyer.first_position).min((p.block, p.position));
        buyer.first_block = first.0;
        buyer.first_position = first.1;
    }
    let mut buyers: Vec<Buyer> = by_address.into_values().collect();
    buyers.sort_by_key(|b| (b.first_block, b.first_position, b.address.0));
    buyers
}

/// Which buyers were chosen, and how much of the buying they represent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    /// The chosen buyers, at most [`MAX_CANDIDATES`], largest first.
    pub candidates: Vec<Buyer>,
    /// How many distinct buyers the window held.
    pub buyers: u32,
    /// The candidates' share of all quote spent in the window, in basis
    /// points; 0 when nothing was bought.
    pub coverage_bps: u16,
}

/// Chooses at most [`MAX_CANDIDATES`] buyers by [`SELECTION_RULE`].
#[must_use]
pub fn select(buyers: &[Buyer]) -> Selection {
    let total: u128 = buyers.iter().fold(0u128, |s, b| s.saturating_add(b.quote));
    let mut remaining: Vec<&Buyer> = buyers.iter().collect();
    let mut candidates: Vec<Buyer> = Vec::new();

    // The two largest by quote; ties broken by earliest, then address, so
    // the same window always yields the same four.
    for _ in 0..2 {
        let Some(i) = (0..remaining.len()).max_by(|&a, &b| {
            let (x, y) = (remaining[a], remaining[b]);
            x.quote
                .cmp(&y.quote)
                .then((y.first_block, y.first_position).cmp(&(x.first_block, x.first_position)))
                .then(y.address.0.cmp(&x.address.0))
        }) else {
            break;
        };
        candidates.push(remaining.swap_remove(i).clone());
    }
    // The earliest remaining buyer (`buyers` is already in first-purchase order).
    if let Some(i) = (0..remaining.len())
        .min_by_key(|&i| (remaining[i].first_block, remaining[i].first_position))
    {
        candidates.push(remaining.swap_remove(i).clone());
    }
    // A deterministic sample of the rest: the lowest address. Not a random
    // draw, so two reads of the same window agree and a test can pin it.
    if let Some(i) = (0..remaining.len()).min_by_key(|&i| remaining[i].address.0) {
        candidates.push(remaining.swap_remove(i).clone());
    }

    let covered: u128 = candidates
        .iter()
        .fold(0u128, |s, b| s.saturating_add(b.quote));
    let coverage_bps = covered
        .saturating_mul(10_000)
        .checked_div(total)
        .map_or(0, |bps| u16::try_from(bps).unwrap_or(10_000));
    Selection {
        candidates,
        buyers: u32::try_from(buyers.len()).unwrap_or(u32::MAX),
        coverage_bps,
    }
}

/// Whether `amount` financed a purchase of `quote`, rather than dusting it.
///
/// `gas_allowance` is the chain's own [`GAS_ALLOWANCE_WEI`] or
/// [`GAS_ALLOWANCE_LAMPORTS`] -- kept a parameter rather than hard-coded so
/// this one materiality test serves both chains without guessing a unit.
#[must_use]
pub fn is_material(amount: u128, quote: u128, gas_allowance: u128) -> bool {
    let needed = quote.saturating_add(gas_allowance);
    amount.saturating_mul(10_000) >= needed.saturating_mul(MATERIAL_SHARE_BPS)
}

/// One native transfer into a candidate before its first purchase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Funder {
    /// Who sent it, in the chain's own canonical text form (0x-lowercase
    /// hex for Robinhood, base58 for Solana).
    pub address: String,
    /// The amount sent, in the chain's smallest unit (wei for Robinhood,
    /// lamports for Solana).
    pub amount_wei: u128,
    /// The block (or slot) it landed in.
    pub block: u64,
    /// The transaction that carried it.
    pub transaction: String,
    /// The provider's identity for the transfer.
    pub unique_id: String,
    /// Whether it was material against the candidate's purchase.
    pub material: bool,
}

/// One checked buyer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// The buyer, in the chain's own canonical text form.
    pub address: String,
    /// Quote it bought with in the window, in the chain's smallest unit.
    pub bought_wei: u128,
    /// The block of its first purchase.
    pub first_purchase_block: u64,
    /// Whether the address holds code; `None` when the read failed.
    pub is_contract: Option<bool>,
    /// Transactions it had sent before the launch block; `None` when the
    /// read failed. Zero means nothing sent on this chain, nothing more.
    pub nonce_before_launch: Option<u64>,
    /// Native transfers into it in the lookback window before its first
    /// purchase, oldest first.
    pub funders: Vec<Funder>,
    /// Whether every page of its transfer history was read.
    pub funding_complete: bool,
}

/// A wallet that materially funded more than one checked candidate.
///
/// Observed, not inferred: it says two transfers happened. It does not say
/// the wallets share an owner, and the sheet never says so either.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedFunder {
    /// The funder, in the chain's own canonical text form.
    pub address: String,
    /// How many distinct checked candidates it materially funded.
    pub funded: u32,
}

/// The investigation's result: what was checked, what it found, what it
/// could not do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Funding {
    /// Distinct buyers in the launch window.
    pub buyers: u32,
    /// Candidates chosen, whether or not they were checked.
    pub selected: u32,
    /// The chosen candidates' share of all launch-window buying, in basis
    /// points. `None` on Solana, which has no quote amount to weigh a share
    /// against -- rendering it as 0% would claim they bought nothing, which
    /// is false, not merely unmeasured.
    pub coverage_bps: Option<u16>,
    /// The rule that chose them.
    pub rule: &'static str,
    /// The candidates actually checked.
    pub checked: Vec<Candidate>,
    /// Funders that materially funded at least two checked candidates,
    /// most-funded first.
    pub shared: Vec<SharedFunder>,
    /// What could not be read, each named (AGENTS.md §3 rule 8).
    pub gaps: Vec<String>,
    /// Compute units this investigation spent.
    pub cu_spent: u32,
}

/// Funders that materially funded two or more of `checked`.
///
/// Chain-agnostic: it works from `Funder::address`'s canonical text form and
/// never parses or compares raw address bytes, so the same function serves
/// Robinhood's 0x-hex and Solana's base58 without a per-chain branch.
#[must_use]
pub fn shared_funders(checked: &[Candidate]) -> Vec<SharedFunder> {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for candidate in checked {
        let mut seen: Vec<&str> = Vec::new();
        for funder in candidate.funders.iter().filter(|f| f.material) {
            // One candidate counts once per funder however many top-ups it got.
            if !seen.contains(&funder.address.as_str()) {
                seen.push(&funder.address);
                *counts.entry(funder.address.clone()).or_default() += 1;
            }
        }
    }
    let mut shared: Vec<SharedFunder> = counts
        .into_iter()
        .filter(|(_, n)| *n >= 2)
        .map(|(address, funded)| SharedFunder { address, funded })
        .collect();
    shared.sort_by(|a, b| b.funded.cmp(&a.funded).then(a.address.cmp(&b.address)));
    shared
}

/// One call's worth of calls and `cu`, or why there is none.
fn take(budget: &mut Budget, cu: u32) -> Result<(), String> {
    budget
        .take_cu(cu)
        .and_then(|()| budget.take_call())
        .map_err(|e| format!("budget exhausted: {e:?}"))
}

/// One page of `alchemy_getAssetTransfers`: native transfers into `to`
/// between the blocks, oldest first, and the key of the next page if any.
///
/// One transfer: `(from, amount_wei, block, transaction hash, uniqueId)`.
type TransferRow = (Address, u128, u64, String, String);

fn transfers_page(
    client: &Rpc,
    to: &Address,
    from_block: u64,
    to_block: u64,
    page_key: Option<&str>,
) -> Result<(Vec<TransferRow>, Option<String>), String> {
    let mut params = serde_json::json!({
        "fromBlock": format!("{from_block:#x}"),
        "toBlock": format!("{to_block:#x}"),
        "toAddress": to.to_string(),
        "category": ["external"],
        "order": "asc",
        "withMetadata": false,
        "excludeZeroValue": true,
        "maxCount": "0x64",
    });
    if let Some(key) = page_key {
        params["pageKey"] = serde_json::Value::String(key.to_owned());
    }
    let result = client.call("alchemy_getAssetTransfers", &serde_json::json!([params]))?;
    let transfers = result
        .get("transfers")
        .and_then(serde_json::Value::as_array)
        .ok_or("alchemy_getAssetTransfers: no transfers array")?;
    let mut page = Vec::with_capacity(transfers.len());
    for t in transfers {
        let text = |key: &str| t.get(key).and_then(serde_json::Value::as_str);
        let from: Address = text("from")
            .ok_or("alchemy_getAssetTransfers: transfer without from")?
            .parse()
            .map_err(|e| format!("alchemy_getAssetTransfers: from: {e}"))?;
        // `rawContract.value` is the exact wei; `value` is a float in ETH
        // and would lose the low digits a materiality test compares.
        let wei = t
            .get("rawContract")
            .and_then(|r| r.get("value"))
            .and_then(serde_json::Value::as_str)
            .ok_or("alchemy_getAssetTransfers: transfer without rawContract.value")?;
        let amount = quantity_u128(wei).map_err(|e| format!("alchemy_getAssetTransfers: {e}"))?;
        let block = quantity(
            text("blockNum").ok_or("alchemy_getAssetTransfers: transfer without blockNum")?,
        )
        .map_err(|e| format!("alchemy_getAssetTransfers: {e}"))?;
        let hash = text("hash")
            .ok_or("alchemy_getAssetTransfers: transfer without hash")?
            .to_owned();
        let unique_id = text("uniqueId")
            .ok_or("alchemy_getAssetTransfers: transfer without uniqueId")?
            .to_owned();
        page.push((from, amount, block, hash, unique_id));
    }
    let next = result
        .get("pageKey")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok((page, next))
}

/// Checks one candidate: code, funding history, pre-launch nonce.
///
/// Each read is allowed to fail on its own: an unread nonce does not drop
/// the funding that did read. The one failure that stops the whole
/// investigation is the provider not answering
/// `alchemy_getAssetTransfers` at all, which the caller learns from
/// [`Checked::provider_gap`].
fn check_candidate(
    client: &Rpc,
    budget: &mut Budget,
    buyer: &Buyer,
    launch_block: u64,
    gaps: &mut Vec<String>,
) -> Candidate {
    let mut candidate = Candidate {
        address: buyer.address.to_string(),
        bought_wei: buyer.quote,
        first_purchase_block: buyer.first_block,
        is_contract: None,
        nonce_before_launch: None,
        funders: Vec::new(),
        funding_complete: false,
    };

    match take(budget, CU_GET_CODE).and_then(|()| client.code(&buyer.address)) {
        Ok(code) => candidate.is_contract = Some(!code.is_empty()),
        Err(why) => gaps.push(format!("code of {}: {why}", buyer.address)),
    }

    // Funding before the first purchase: the block before it, back to the
    // lookback bound.
    let to_block = buyer.first_block.saturating_sub(1);
    let from_block = buyer.first_block.saturating_sub(FUNDING_LOOKBACK_BLOCKS);
    let mut page_key: Option<String> = None;
    let mut pages = 0u32;
    loop {
        if pages >= MAX_FUNDING_PAGES {
            gaps.push(format!(
                "funding of {}: more than {MAX_FUNDING_PAGES} pages; later transfers unread",
                buyer.address
            ));
            break;
        }
        if let Err(why) = take(budget, CU_GET_ASSET_TRANSFERS) {
            gaps.push(format!("funding of {}: {why}", buyer.address));
            break;
        }
        pages += 1;
        match transfers_page(
            client,
            &buyer.address,
            from_block,
            to_block,
            page_key.as_deref(),
        ) {
            Ok((page, next)) => {
                for (from, amount, block, hash, unique_id) in page {
                    candidate.funders.push(Funder {
                        address: from.to_string(),
                        amount_wei: amount,
                        block,
                        transaction: hash,
                        unique_id,
                        material: is_material(amount, buyer.quote, GAS_ALLOWANCE_WEI),
                    });
                }
                if let Some(key) = next {
                    page_key = Some(key);
                } else {
                    candidate.funding_complete = true;
                    break;
                }
            }
            Err(why) => {
                gaps.push(format!("funding of {}: {why}", buyer.address));
                break;
            }
        }
    }

    // The nonce at the block before launch: what the wallet had sent before
    // this token existed. The launch block's own end would include the buy.
    let before_launch = launch_block.saturating_sub(1);
    match take(budget, CU_GET_TRANSACTION_COUNT).and_then(|()| {
        client
            .call(
                "eth_getTransactionCount",
                &serde_json::json!([buyer.address.to_string(), format!("{before_launch:#x}")]),
            )
            .and_then(|v| {
                v.as_str()
                    .ok_or_else(|| "eth_getTransactionCount: not a quantity".to_owned())
                    .and_then(|s| quantity(s).map_err(|e| e.to_string()))
            })
    }) {
        Ok(nonce) => candidate.nonce_before_launch = Some(nonce),
        Err(why) => gaps.push(format!("nonce of {}: {why}", buyer.address)),
    }

    candidate
}

/// Whether a provider error says the method itself is not served, as
/// opposed to a bad answer to a served one.
fn method_unsupported(why: &str) -> bool {
    let lower = why.to_ascii_lowercase();
    lower.contains("method not found")
        || lower.contains("not supported")
        || lower.contains("unsupported method")
        || lower.contains("does not exist")
}

/// Reads the launch window's purchases, chooses candidates and checks them,
/// inside `budget` and [`FUNDING_MAX_CU`].
///
/// `read_block` bounds the window's upper end so the read describes the
/// same state as the rest of the dossier. With a `memory`, the funding
/// edges are remembered as events and the run recorded with its coverage.
///
/// # Errors
///
/// A string naming why the launch window itself could not be read; every
/// later failure lands in [`Funding::gaps`] on a result that is still
/// returned.
// The candidate loop and the memory write are one unit of work: splitting
// them would only move the length somewhere a reader has to follow.
#[allow(clippy::too_many_lines)]
pub fn investigate(
    client: &Rpc,
    budget: &mut Budget,
    token: &Address,
    curve: &Address,
    launch_block: u64,
    read_block: u64,
    memory: Option<&Memory>,
) -> Result<Funding, String> {
    let cu_before = budget.cu_spent();
    let calls_before = budget.calls_made();
    let to_block = read_block.min(launch_block.saturating_add(LAUNCH_WINDOW_BLOCKS));
    take(budget, CU_GET_LOGS)?;
    let logs = client
        .logs_range(curve, &[topic::CURVE_BUY], launch_block, to_block)
        .map_err(|e| match e {
            LogsError::TooManyResults => {
                "the launch window held more purchases than one read returns".to_owned()
            }
            LogsError::Other(why) => why,
        })?;
    let purchases = purchases_from(&logs, curve);
    let buyers = buyers_of(&purchases);
    let selection = select(&buyers);

    let mut gaps = Vec::new();
    let mut checked = Vec::new();
    // The per-candidate cap: the slice's own ceiling, or what the dossier
    // has left after its core reads, whichever is smaller.
    let cap = FUNDING_MAX_CU.min(budget.cu_left());
    let mut spent = 0u32;
    for buyer in &selection.candidates {
        if spent.saturating_add(CU_PER_CANDIDATE) > cap {
            gaps.push(format!(
                "compute-unit cap of {cap} CU reached: {} of {} candidates checked",
                checked.len(),
                selection.candidates.len()
            ));
            break;
        }
        let before = budget.cu_spent();
        let mut candidate_gaps = Vec::new();
        let candidate = check_candidate(client, budget, buyer, launch_block, &mut candidate_gaps);
        spent = spent.saturating_add(budget.cu_spent().saturating_sub(before));
        let provider_gap = candidate_gaps
            .iter()
            .any(|g| g.starts_with("funding of") && method_unsupported(g));
        gaps.append(&mut candidate_gaps);
        checked.push(candidate);
        if provider_gap {
            gaps.push(
                "the provider does not serve alchemy_getAssetTransfers; funding unread for the \
                 remaining candidates"
                    .to_owned(),
            );
            break;
        }
    }

    let shared = shared_funders(&checked);
    let funding = Funding {
        buyers: selection.buyers,
        selected: u32::try_from(selection.candidates.len()).unwrap_or(u32::MAX),
        coverage_bps: Some(selection.coverage_bps),
        rule: SELECTION_RULE,
        checked,
        shared,
        gaps,
        cu_spent: budget.cu_spent().saturating_sub(cu_before),
    };

    if let Some(memory) = memory {
        let chain = "robinhood";
        let token_key = token.to_string();
        let edges: Vec<FundingEdge> = funding
            .checked
            .iter()
            .flat_map(|c| {
                c.funders.iter().map(move |f| FundingEdge {
                    recipient: c.address.clone(),
                    funder: f.address.clone(),
                    block: f.block,
                    transaction: f.transaction.clone(),
                    unique_id: f.unique_id.clone(),
                    amount: f.amount_wei,
                    material: f.material,
                })
            })
            .collect();
        memory
            .record_funding_edges(chain, &token_key, &edges)
            .map_err(|e| e.to_string())?;
        let all_checked = funding.checked.len() == funding.selected as usize
            && funding.checked.iter().all(|c| c.funding_complete);
        memory
            .record_check_run(&CheckRun {
                chain: chain.to_owned(),
                token: token_key,
                what: "funding".to_owned(),
                parameters: format!(
                    "rule={SELECTION_RULE}; max_candidates={MAX_CANDIDATES}; \
                     cu_cap={cap}; lookback_blocks={FUNDING_LOOKBACK_BLOCKS}"
                ),
                from_block: launch_block,
                to_block,
                completeness: if all_checked {
                    Completeness::Complete
                } else {
                    Completeness::Truncated
                },
                calls: budget.calls_made().saturating_sub(calls_before),
                note: format!(
                    "{} of {} candidates checked, {} buyers, coverage {}; {}",
                    funding.checked.len(),
                    funding.selected,
                    funding.buyers,
                    funding
                        .coverage_bps
                        .map_or_else(|| "unmeasured".to_owned(), |bps| format!("{bps} bps")),
                    funding.gaps.join("; ")
                ),
                ran_at: std::time::SystemTime::now(),
            })
            .map_err(|e| e.to_string())?;
    }

    Ok(funding)
}

/// How many of the mint's own earliest successful transactions define the
/// Solana launch window. Read oldest first, capped by the page budget the
/// same way [`RpcClient::signatures_back_to_oldest`] is; a failed read (err
/// on the signature, or a transaction that could not be fetched) is a gap
/// and does not count toward this cap, so 25 always means 25 transactions
/// actually read, not 25 attempts.
pub const SOLANA_WINDOW_TRANSACTIONS: usize = 25;

/// The Solana selection rule, recorded on every [`investigate_solana`]
/// result on the same terms [`SELECTION_RULE`] is for Robinhood: every
/// distinct buyer in the mint's own first [`SOLANA_WINDOW_TRANSACTIONS`]
/// successful transactions (read oldest first) is the launch window
/// (`Funding::buyers`); the first [`MAX_CANDIDATES`] of them by first
/// purchase are checked.
pub const SOLANA_SELECTION_RULE: &str = "every distinct buyer in the mint's first 25 successful \
                                          transactions read oldest first, excluding the proven \
                                          bonding curve; the first wallets by first purchase, up \
                                          to the same limit as Robinhood, are checked";

/// The funder of one candidate's earliest lamport balance increase, read
/// from a single transaction's `pre_balances`/`post_balances`.
///
/// The account whose own balance rose is the candidate; the account whose
/// balance fell the most is taken as the source, because a transaction can
/// move lamports through several accounts (fees, rent) and the largest drop
/// is the one that plausibly funded the candidate's gain rather than a fee
/// payer's small deduction.
fn funder_of(tx: &Transaction, candidate: &str) -> Option<(String, u128)> {
    if tx.accounts.len() != tx.pre_balances.len() || tx.accounts.len() != tx.post_balances.len() {
        // A shape this reader cannot trust an index into; see `rpc.rs`'s
        // `lamport_balances` doc on why a real node does not do this.
        return None;
    }
    let candidate_index = tx.accounts.iter().position(|a| a == candidate)?;
    let gain = tx.post_balances[candidate_index].checked_sub(tx.pre_balances[candidate_index])?;
    if gain == 0 {
        return None;
    }
    let (from_index, drop) = tx
        .accounts
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != candidate_index)
        .filter_map(|(i, _)| {
            tx.pre_balances[i]
                .checked_sub(tx.post_balances[i])
                .filter(|d| *d > 0)
                .map(|d| (i, d))
        })
        .max_by_key(|&(_, d)| d)?;
    Some((tx.accounts[from_index].clone(), u128::from(drop.min(gain))))
}

/// Distinct wallets whose balance of `mint` rose in `tx`, read from its
/// token balances -- the Solana stand-in for a `CurveBuy` log entry. `curve`,
/// when known, is never a buyer: its own token account is the pool side of
/// every trade in and out of it, not a beneficiary of one.
fn buyers_in(tx: &Transaction, mint: &str, curve: Option<&str>) -> Vec<String> {
    let mut before_by_index: BTreeMap<usize, u64> = BTreeMap::new();
    for balance in &tx.pre_token_balances {
        if balance.mint == mint {
            before_by_index.insert(balance.account_index, balance.amount);
        }
    }
    let mut buyers = Vec::new();
    for balance in &tx.post_token_balances {
        if balance.mint != mint {
            continue;
        }
        let before = before_by_index
            .get(&balance.account_index)
            .copied()
            .unwrap_or(0);
        if balance.amount <= before {
            continue;
        }
        let Some(owner) = balance.owner.as_deref() else {
            continue;
        };
        if Some(owner) == curve {
            continue;
        }
        buyers.push(owner.to_owned());
    }
    buyers
}

/// One early buyer found while walking the mint's own earliest transactions.
struct EarlyBuyer {
    address: String,
    first_purchase_slot: u64,
}

/// Checks funding for a Solana mint's early buyers.
///
/// The launch window is the mint's own first [`SOLANA_WINDOW_TRANSACTIONS`]
/// successful transactions, read oldest first
/// ([`RpcClient::signatures_back_to_oldest`]); every distinct wallet whose
/// balance of this mint rose in them ([`buyers_in`]), excluding the verified
/// bonding-curve PDA ([`realorrug_pumpfun::pda::bonding_curve`]), is a buyer
/// (`Funding::buyers` -- see [`SOLANA_SELECTION_RULE`]). The first
/// [`MAX_CANDIDATES`] of them by first purchase are checked: each candidate's
/// own oldest signature is read separately for the native lamport transfer
/// that funded it, on the same terms [`funder_of`] already reads for
/// Robinhood (the account whose balance fell the most), counted only when
/// [`Transaction::slot`] is at or before the candidate's first-purchase slot
/// -- a transfer after the purchase it is supposed to finance did not fund
/// it -- and subject to [`is_material`] against [`GAS_ALLOWANCE_LAMPORTS`]
/// (quote 0: no SOL cost of the buy itself is read here, so materiality
/// falls back to "more than dust").
///
/// **If the mint's own history was truncated before its window could be
/// read, none of the buyers found in what *was* read are "the early
/// buyers"** -- a budget that runs out paging backward from the newest
/// drops the oldest, undiscovered page first, so the window is not
/// necessarily the earliest one. Nothing is checked, `Funding::buyers` and
/// `checked` are both empty, and the gap says why. **If a candidate's own
/// signature history is truncated before its oldest transaction, no funder
/// is recorded for it** -- `signatures.last()` there is not its oldest
/// transaction either -- again never as "no funder found"; both cases are
/// named in [`Funding::gaps`] (AGENTS.md rule 8).
///
/// # Errors
///
/// A string naming why the mint's own signature history could not be read at
/// all. A single candidate or transaction read failure lands in
/// [`Funding::gaps`] instead, on a result that is still returned.
pub fn investigate_solana(
    client: &RpcClient,
    budget: &mut Budget,
    mint: &realorrug_types::Address,
) -> Result<Funding, String> {
    let curve = realorrug_pumpfun::pda::bonding_curve(mint).map(|c| c.to_string());
    let mint_key = mint.to_string();
    let (signatures, truncated) = client
        .signatures_back_to_oldest(budget, mint)
        .map_err(|e| format!("funding: {e}"))?;

    if truncated {
        // A truncated mint history means the transactions this reader could
        // see are not necessarily the mint's *earliest* ones -- paging from
        // the newest backward, a budget that runs out first drops the
        // oldest, undiscovered page. Whatever buyers were found in what was
        // read are not "the early buyers"; checking them would put a funder
        // on the wrong candidate. Nothing is checked, and the gap says why
        // (AGENTS.md rule 8: absent is not zero).
        return Ok(Funding {
            buyers: 0,
            selected: 0,
            coverage_bps: None,
            rule: SOLANA_SELECTION_RULE,
            checked: Vec::new(),
            shared: Vec::new(),
            gaps: vec![
                "the mint's signature history is longer than the page budget allows; the \
                 launch's first buyers could not be reached within the read budget"
                    .to_owned(),
            ],
            cu_spent: 0,
        });
    }

    // `signatures` is newest-first, the same order every other reader in
    // this crate gets from `getSignaturesForAddress`; walk it in reverse to
    // see transactions in the order they happened, and stop once
    // `SOLANA_WINDOW_TRANSACTIONS` of them were successfully read -- that
    // window, not the capped candidate list, is where `buyers` comes from.
    let mut gaps = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut window_buyers: Vec<EarlyBuyer> = Vec::new();
    let mut window_read = 0usize;
    for sig in signatures.iter().rev() {
        if window_read >= SOLANA_WINDOW_TRANSACTIONS {
            break;
        }
        if sig.err.is_some() {
            continue;
        }
        match client.transaction(budget, &sig.signature) {
            Ok(Some(tx)) => {
                window_read += 1;
                for buyer in buyers_in(&tx, &mint_key, curve.as_deref()) {
                    if seen.insert(buyer.clone()) {
                        window_buyers.push(EarlyBuyer {
                            address: buyer,
                            first_purchase_slot: sig.slot,
                        });
                    }
                }
            }
            Ok(None) => gaps.push(format!(
                "transaction {} could not be fetched",
                sig.signature
            )),
            Err(why) => gaps.push(format!("transaction {}: {why}", sig.signature)),
        }
    }

    let buyers = u32::try_from(window_buyers.len()).unwrap_or(u32::MAX);
    let mut checked = Vec::new();
    for buyer in window_buyers.iter().take(MAX_CANDIDATES) {
        checked.push(check_solana_candidate(client, budget, buyer, &mut gaps));
    }

    let shared = shared_funders(&checked);
    Ok(Funding {
        buyers,
        selected: u32::try_from(checked.len()).unwrap_or(u32::MAX),
        coverage_bps: None,
        rule: SOLANA_SELECTION_RULE,
        checked,
        shared,
        gaps,
        cu_spent: 0,
    })
}

/// Reads one early buyer's own oldest signature for the native transfer that
/// funded it before its first purchase. Split out of [`investigate_solana`]
/// so a failure on one candidate is a gap on the overall result, not a
/// reason to abandon the others.
fn check_solana_candidate(
    client: &RpcClient,
    budget: &mut Budget,
    buyer: &EarlyBuyer,
    gaps: &mut Vec<String>,
) -> Candidate {
    let address = buyer.address.clone();
    let mut candidate = Candidate {
        address: address.clone(),
        bought_wei: 0,
        first_purchase_block: buyer.first_purchase_slot,
        is_contract: None,
        nonce_before_launch: None,
        funders: Vec::new(),
        funding_complete: true,
    };

    let Ok(address_key) = address.parse::<realorrug_types::Address>() else {
        gaps.push(format!("funding of {address}: not a parseable address"));
        candidate.funding_complete = false;
        return candidate;
    };

    match client.signatures_back_to_oldest(budget, &address_key) {
        Ok((signatures, truncated)) => {
            candidate.funding_complete = !truncated;
            if truncated {
                // `signatures.last()` is only the oldest transaction seen so
                // far, not the candidate's actual oldest -- a truncated page
                // walk drops the earlier, undiscovered pages first (same
                // reasoning as the mint-level truncation above). Recording a
                // funder from a non-oldest transaction would misattribute
                // who financed the buy, so no funder is recorded at all; the
                // gap says why (AGENTS.md rule 8).
                gaps.push(format!(
                    "funding of {address}: signature history truncated before its oldest \
                     transaction; no funder recorded"
                ));
            } else if let Some(oldest) = signatures.last() {
                match client.transaction(budget, &oldest.signature) {
                    Ok(Some(tx)) => {
                        if tx.slot.0 > buyer.first_purchase_slot {
                            // A transfer after the purchase it is supposed to
                            // finance did not fund it; only a transfer at or
                            // before the first-purchase slot can have.
                            gaps.push(format!(
                                "funding of {address}: its oldest transaction landed after its \
                                 first purchase; no funder recorded"
                            ));
                        } else if let Some((from, amount)) = funder_of(&tx, &address) {
                            candidate.funders.push(Funder {
                                address: from,
                                amount_wei: amount,
                                block: tx.slot.0,
                                transaction: oldest.signature.clone(),
                                unique_id: oldest.signature.clone(),
                                material: is_material(amount, 0, GAS_ALLOWANCE_LAMPORTS),
                            });
                        }
                    }
                    Ok(None) => {
                        gaps.push(format!(
                            "funding of {address}: oldest transaction not found"
                        ));
                        candidate.funding_complete = false;
                    }
                    Err(why) => {
                        gaps.push(format!("funding of {address}: {why}"));
                        candidate.funding_complete = false;
                    }
                }
            }
        }
        Err(why) => {
            gaps.push(format!("funding of {address}: {why}"));
            candidate.funding_complete = false;
        }
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(b: u8) -> Address {
        Address([b; 20])
    }

    fn buyer(b: u8, quote: u128, block: u64) -> Buyer {
        Buyer {
            address: addr(b),
            quote,
            first_block: block,
            first_position: (0, 0),
        }
    }

    fn candidate(b: u8, quote: u128, funders: &[(u8, u128)]) -> Candidate {
        Candidate {
            address: addr(b).to_string(),
            bought_wei: quote,
            first_purchase_block: 10,
            is_contract: Some(false),
            nonce_before_launch: Some(0),
            funders: funders
                .iter()
                .enumerate()
                .map(|(i, (f, amount))| Funder {
                    address: addr(*f).to_string(),
                    amount_wei: *amount,
                    block: 5,
                    transaction: format!("0x{i}"),
                    unique_id: format!("0x{i}:external:0"),
                    material: is_material(*amount, quote, GAS_ALLOWANCE_WEI),
                })
                .collect(),
            funding_complete: true,
        }
    }

    #[test]
    fn the_two_largest_the_earliest_remaining_and_the_lowest_address_are_chosen() {
        // Six buyers: quotes 5, 50, 40, 5, 5, 5 in first-purchase order.
        let buyers = vec![
            buyer(0x0a, 5, 1),
            buyer(0x0b, 50, 2),
            buyer(0x0c, 40, 3),
            buyer(0x09, 5, 4),
            buyer(0x0d, 5, 5),
            buyer(0x01, 5, 6),
        ];
        let s = select(&buyers);
        let chosen: Vec<u8> = s.candidates.iter().map(|b| b.address.0[0]).collect();
        // Largest two (0b, 0c), earliest remaining (0a), lowest address of
        // the rest (01) -- not 09, which is neither earliest nor lowest.
        assert_eq!(chosen, vec![0x0b, 0x0c, 0x0a, 0x01]);
        assert_eq!(s.buyers, 6);
        // (50 + 40 + 5 + 5) / 110 = 90.9%
        assert_eq!(s.coverage_bps, 9_090);
    }

    #[test]
    fn fewer_than_four_buyers_are_all_chosen_and_cover_everything() {
        let s = select(&[buyer(1, 3, 1), buyer(2, 7, 2)]);
        assert_eq!(s.candidates.len(), 2);
        assert_eq!(s.coverage_bps, 10_000);
        assert_eq!(select(&[]).coverage_bps, 0);
        assert!(select(&[]).candidates.is_empty());
    }

    #[test]
    fn largest_ties_break_toward_the_earlier_buyer() {
        let s = select(&[buyer(5, 10, 9), buyer(3, 10, 2), buyer(4, 10, 2)]);
        // 3 and 4 tie on block too; the lower address wins there.
        let chosen: Vec<u8> = s.candidates.iter().map(|b| b.address.0[0]).collect();
        assert_eq!(chosen, vec![3, 4, 5]);
    }

    #[test]
    fn a_buyer_is_aggregated_by_beneficiary_not_by_trader() {
        let log = |trader: u8, recipient: u8, quote: u128, block: u64| {
            let mut data = Vec::new();
            for v in [quote, 1_000, 0, 0] {
                let mut w = [0u8; 32];
                w[16..].copy_from_slice(&v.to_be_bytes());
                data.extend(w);
            }
            let mut t = [0u8; 32];
            t[12..].copy_from_slice(&[trader; 20]);
            let mut r = [0u8; 32];
            r[12..].copy_from_slice(&[recipient; 20]);
            Log {
                address: addr(0xcc),
                topics: vec![topic::CURVE_BUY, Hash32(t), Hash32(r)],
                data,
                block,
                transaction: Hash32([1; 32]),
                position: None,
            }
        };
        let mut sell = log(1, 1, 99, 1);
        sell.topics[0] = topic::CURVE_SELL;
        let mut other_curve = log(1, 1, 99, 1);
        other_curve.address = addr(0xdd);
        let logs = vec![
            log(0x22, 0x01, 5, 3), // a router buys for 01
            log(0x01, 0x01, 7, 2), // 01 buys directly, earlier
            log(0x22, 0x02, 9, 3), // the same router buys for 02
            sell,
            other_curve,
        ];
        let purchases = purchases_from(&logs, &addr(0xcc));
        assert_eq!(
            purchases.len(),
            3,
            "sells and other curves are not purchases"
        );
        assert_eq!(purchases[0].trader, addr(0x22));
        assert_eq!(purchases[0].beneficiary, addr(0x01));
        let buyers = buyers_of(&purchases);
        assert_eq!(buyers.len(), 2, "the router is not a buyer");
        assert_eq!(buyers[0].address, addr(0x01));
        assert_eq!(buyers[0].quote, 12);
        assert_eq!(buyers[0].first_block, 2);
        assert_eq!(buyers[1].address, addr(0x02));
    }

    #[test]
    fn materiality_is_half_the_purchase_plus_gas() {
        let quote = 1_000_000_000_000_000_000; // 1 ETH
        let needed = quote + GAS_ALLOWANCE_WEI;
        assert!(is_material(needed / 2, quote, GAS_ALLOWANCE_WEI));
        assert!(!is_material(needed / 2 - 1, quote, GAS_ALLOWANCE_WEI));
        // Dust against a real buy is never material.
        assert!(!is_material(1_000, quote, GAS_ALLOWANCE_WEI));
        // A wallet that bought nothing is only "funded" by the gas allowance.
        assert!(is_material(GAS_ALLOWANCE_WEI / 2, 0, GAS_ALLOWANCE_WEI));
        // The same rule, on Solana's own gas allowance and unit.
        let lamports_needed = quote + GAS_ALLOWANCE_LAMPORTS;
        assert!(is_material(
            lamports_needed / 2,
            quote,
            GAS_ALLOWANCE_LAMPORTS
        ));
        assert!(!is_material(
            lamports_needed / 2 - 1,
            quote,
            GAS_ALLOWANCE_LAMPORTS
        ));
    }

    #[test]
    fn a_shared_funder_needs_two_materially_funded_candidates() {
        const ETH: u128 = 1_000_000_000_000_000_000;
        let checked = [
            candidate(1, ETH, &[(0xf0, ETH)]),
            candidate(2, ETH, &[(0xf0, ETH), (0xf0, ETH)]), // twice, counts once
            candidate(3, ETH, &[(0xf0, ETH), (0xf1, ETH)]),
            candidate(4, ETH, &[(0xf1, ETH)]),
        ];
        let shared = shared_funders(&checked);
        assert_eq!(
            shared,
            vec![
                SharedFunder {
                    address: addr(0xf0).to_string(),
                    funded: 3
                },
                SharedFunder {
                    address: addr(0xf1).to_string(),
                    funded: 2
                },
            ]
        );
        // Exactly one candidate funded is not shared.
        assert!(shared_funders(&checked[..1]).is_empty());
    }

    #[test]
    fn a_dust_sender_to_every_candidate_is_not_a_shared_funder() {
        let one_eth = 1_000_000_000_000_000_000;
        let checked: Vec<Candidate> = (1..=4)
            .map(|b| candidate(b, one_eth, &[(0xd0, 1_000)]))
            .collect();
        assert!(checked.iter().all(|c| !c.funders[0].material));
        assert!(shared_funders(&checked).is_empty());
    }

    // -- Slice 6b: investigate_solana ---------------------------------

    fn solana_addr(b: u8) -> realorrug_types::Address {
        realorrug_types::Address::new([b; 32])
    }

    /// A queue-based transport, mirroring `rpc.rs`'s own `Canned` test
    /// fixture (private to that module, so this crate's other test modules
    /// each keep a small copy rather than share one across a test boundary
    /// Rust does not have).
    struct Canned(std::sync::Mutex<Vec<String>>);

    impl Canned {
        fn boxed(responses: &[&str]) -> Box<dyn crate::rpc::Transport> {
            Box::new(Self(std::sync::Mutex::new(
                responses.iter().rev().map(|s| (*s).to_owned()).collect(),
            )))
        }
    }

    impl crate::rpc::Transport for Canned {
        fn post(&self, _: &str, _: String) -> Result<String, String> {
            self.0
                .lock()
                .map_err(|_| "poisoned".to_owned())?
                .pop()
                .ok_or_else(|| "the client asked for more than the test supplied".to_owned())
        }
    }

    /// A single-page `getSignaturesForAddress` answer: one signature, which
    /// is also the oldest -- a page shorter than 1,000 ends the walk.
    fn signatures_page(signature: &str) -> String {
        format!(r#"{{"result":[{{"signature":"{signature}","slot":1}}],"error":null}}"#)
    }

    /// A `getTransaction` answer whose only lamport move is `from` funding
    /// `to` by `amount`.
    fn funding_tx(from: &str, to: &str, amount: u64) -> String {
        format!(
            r#"{{"result":{{"slot":1,"meta":{{"err":null,"preBalances":[1000000,0],"postBalances":[{pre_left},{amount}],"preTokenBalances":[],"postTokenBalances":[]}},"transaction":{{"message":{{"accountKeys":["{from}","{to}"],"instructions":[]}}}}}},"error":null}}"#,
            pre_left = 1_000_000u64.saturating_sub(amount),
        )
    }

    /// A `getTransaction` answer for the mint's own history: each `buyers`
    /// entry's balance of `mint` rose from 0 to `amount`, at a distinct
    /// `accountIndex` -- the shape `buyers_in` reads a decoded buy from.
    fn buy_tx(mint: &str, buyers: &[(&str, u64)]) -> String {
        let entries: Vec<String> = buyers
            .iter()
            .enumerate()
            .map(|(i, (owner, amount))| {
                format!(
                    r#"{{"accountIndex":{i},"mint":"{mint}","owner":"{owner}","uiTokenAmount":{{"amount":"{amount}"}}}}"#
                )
            })
            .collect();
        format!(
            r#"{{"result":{{"slot":1,"meta":{{"err":null,"preBalances":[],"postBalances":[],"preTokenBalances":[],"postTokenBalances":[{}]}},"transaction":{{"message":{{"accountKeys":[],"instructions":[]}}}}}},"error":null}}"#,
            entries.join(",")
        )
    }

    fn solana_budget() -> Budget {
        Budget::new(60, 60, std::time::Duration::from_secs(30))
    }

    /// A single-page `getSignaturesForAddress` answer at a chosen `slot`,
    /// for tests that need control over the slot a signature landed at.
    fn signatures_page_at(signature: &str, slot: u64) -> String {
        format!(r#"{{"result":[{{"signature":"{signature}","slot":{slot}}}],"error":null}}"#)
    }

    /// A `getTransaction` answer whose only lamport move is `from` funding
    /// `to` by `amount`, landing at a chosen `slot`.
    fn funding_tx_at(from: &str, to: &str, amount: u64, slot: u64) -> String {
        format!(
            r#"{{"result":{{"slot":{slot},"meta":{{"err":null,"preBalances":[1000000,0],"postBalances":[{pre_left},{amount}],"preTokenBalances":[],"postTokenBalances":[]}},"transaction":{{"message":{{"accountKeys":["{from}","{to}"],"instructions":[]}}}}}},"error":null}}"#,
            pre_left = 1_000_000u64.saturating_sub(amount),
        )
    }

    /// A full, 1,000-signature `getSignaturesForAddress` page -- the shape
    /// that makes `signatures_back_to_oldest` try a second page, so a page
    /// budget of one exhausts on it and reports a truncated read.
    fn full_signatures_page() -> String {
        let entries: Vec<String> = (0..1000)
            .map(|i| format!(r#"{{"signature":"sig-{i}","slot":1}}"#))
            .collect();
        format!(r#"{{"result":[{}],"error":null}}"#, entries.join(","))
    }

    #[test]
    fn a_truncated_mint_history_checks_nobody_instead_of_the_wrong_buyers() {
        // A full page means there may be more signatures older than it; with
        // only one page in budget, the walk cannot reach the mint's oldest
        // transactions. Whatever buyers happen to be in the page read are
        // NOT the early buyers, so nothing may be checked -- reporting them
        // would put the sheet's "early buyers" sentence on data that isn't
        // the launch window.
        let mint = solana_addr(9);
        let responses = [full_signatures_page()];
        let client = RpcClient::with_transport(
            "http://test.invalid",
            Canned::boxed(&responses.iter().map(String::as_str).collect::<Vec<_>>()),
        );
        let mut budget = Budget::new(60, 1, std::time::Duration::from_secs(30));
        let funding = investigate_solana(&client, &mut budget, &mint).expect("a result");

        assert_eq!(funding.buyers, 0);
        assert!(funding.checked.is_empty());
        assert_eq!(funding.coverage_bps, None);
        assert!(
            funding
                .gaps
                .iter()
                .any(|g| g.contains("could not be reached within the read budget")),
            "gaps: {:?}",
            funding.gaps
        );
    }

    #[test]
    fn a_truncated_candidate_history_records_no_funder() {
        // The overall budget allows exactly one page: the mint's own read
        // spends it, so the candidate's own `signatures_back_to_oldest` call
        // fails before returning anything. `signatures.last()` is unusable
        // (there's no `last()` to take) -- the old code trusted a partial
        // read as if it ended at the oldest transaction.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let responses = [
            signatures_page("mint-sig"),
            buy_tx(&mint_key, &[(&buyer, 500)]),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = Budget::new(60, 1, std::time::Duration::from_secs(30));
        let funding = investigate_solana(&client, &mut budget, &mint).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(!funding.checked[0].funding_complete);
        assert!(funding.checked[0].funders.is_empty());
        assert!(
            funding
                .gaps
                .iter()
                .any(|g| g.contains("no funder recorded")),
            "gaps: {:?}",
            funding.gaps
        );
    }

    #[test]
    fn a_funder_transaction_after_the_first_purchase_does_not_count() {
        // The candidate's own oldest transaction landed at slot 9, after its
        // first purchase at slot 5 -- so whatever moved lamports there did
        // not fund the buy; it happened afterward, and recording it as "who
        // funded the early buyer" would be false.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let funder = solana_addr(0xf0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_at("buyer-sig", 9),
            funding_tx_at(&funder, &buyer, 500_000, 9),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete);
        assert!(funding.checked[0].funders.is_empty());
        assert!(
            funding
                .gaps
                .iter()
                .any(|g| g.contains("after its first purchase")),
            "gaps: {:?}",
            funding.gaps
        );
    }

    #[test]
    fn buyers_counts_the_whole_window_not_just_the_checked_candidates() {
        // Six distinct buyers appear in the launch window, but only
        // `MAX_CANDIDATES` are checked; `Funding::buyers` must still report
        // all six, not the capped count -- the sheet's "of the N early
        // buyers" number describes the window, not who got checked.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyers: Vec<String> = (1..=6u8).map(|b| solana_addr(b).to_string()).collect();
        let funder = solana_addr(0xf0).to_string();

        let mut responses = vec![
            signatures_page("mint-sig"),
            buy_tx(
                &mint_key,
                &buyers.iter().map(|b| (b.as_str(), 500)).collect::<Vec<_>>(),
            ),
        ];
        for (i, buyer) in buyers.iter().take(MAX_CANDIDATES).enumerate() {
            responses.push(signatures_page(&format!("buyer{i}-sig")));
            responses.push(funding_tx(&funder, buyer, 100));
        }
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint).expect("a result");

        assert_eq!(funding.buyers, 6);
        assert_eq!(funding.selected, u32::try_from(MAX_CANDIDATES).unwrap());
        assert_eq!(funding.checked.len(), MAX_CANDIDATES);
    }

    #[test]
    fn two_buyers_funded_by_one_address_are_a_shared_funder() {
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer1 = solana_addr(1).to_string();
        let buyer2 = solana_addr(2).to_string();
        let funder = solana_addr(0xf0).to_string();

        let responses = [
            // The mint's own signature history: one transaction, both
            // buyers' balances rise in it.
            signatures_page("mint-sig"),
            buy_tx(&mint_key, &[(&buyer1, 500), (&buyer2, 500)]),
            // Each candidate's own oldest signature, funded by the same
            // address.
            signatures_page("buyer1-sig"),
            funding_tx(&funder, &buyer1, 500_000),
            signatures_page("buyer2-sig"),
            funding_tx(&funder, &buyer2, 500_000),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint).expect("a result");

        assert_eq!(funding.checked.len(), 2);
        assert!(funding.checked.iter().all(|c| c.funding_complete));
        assert_eq!(
            funding.shared,
            vec![SharedFunder {
                address: funder,
                funded: 2
            }]
        );
    }

    #[test]
    fn a_capped_page_is_incomplete_never_absent() {
        // A page budget of zero means `take_page` fails before any call is
        // made: the mint's history is unread, not empty, so this must never
        // report "no early buyers" as if the check had actually run.
        let mint = solana_addr(9);
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&[]));
        let mut budget = Budget::new(60, 0, std::time::Duration::from_secs(30));
        let funding = investigate_solana(&client, &mut budget, &mint).expect("a result");

        assert!(funding.checked.is_empty());
        assert!(
            funding.gaps.iter().any(|g| g.contains("budget")),
            "{:?}",
            funding.gaps
        );
    }

    #[test]
    fn a_failed_read_is_a_named_gap_not_a_silent_empty_result() {
        struct AlwaysFails;
        impl crate::rpc::Transport for AlwaysFails {
            fn post(&self, _: &str, _: String) -> Result<String, String> {
                Err("connection refused".to_owned())
            }
        }
        let mint = solana_addr(9);
        let client = RpcClient::with_transport("http://test.invalid", Box::new(AlwaysFails));
        let mut budget = solana_budget();
        let err = investigate_solana(&client, &mut budget, &mint)
            .expect_err("a transport failure must surface, not disappear");
        assert!(err.contains("funding"), "{err}");
    }

    #[test]
    fn a_provider_that_lacks_the_method_is_told_apart_from_a_bad_answer() {
        assert!(method_unsupported(
            "alchemy_getAssetTransfers: Method not found"
        ));
        assert!(method_unsupported(
            "the method alchemy_getAssetTransfers does not exist"
        ));
        assert!(!method_unsupported(
            "alchemy_getAssetTransfers: no transfers array"
        ));
        // Each wording on its own is enough; none is required with another.
        for alone in ["not supported", "unsupported method", "does not exist"] {
            assert!(method_unsupported(alone), "{alone}");
        }
    }
}
