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

use std::collections::BTreeMap;

use realorrug_robinhood::pons::{Side, Trade, topic};
use realorrug_robinhood::{Address, Hash32, Log, LogsError, Rpc, quantity, quantity_u128};

use crate::budget::Budget;
use crate::memory::{CheckRun, Completeness, FundingEdge, Memory};

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
        if (p.block, p.position) < (buyer.first_block, buyer.first_position) {
            buyer.first_block = p.block;
            buyer.first_position = p.position;
        }
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
#[must_use]
pub fn is_material(amount: u128, quote: u128) -> bool {
    let needed = quote.saturating_add(GAS_ALLOWANCE_WEI);
    amount.saturating_mul(10_000) >= needed.saturating_mul(MATERIAL_SHARE_BPS)
}

/// One native transfer into a candidate before its first purchase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Funder {
    /// Who sent it.
    pub address: Address,
    /// Wei sent.
    pub amount_wei: u128,
    /// The block it landed in.
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
    /// The buyer.
    pub address: Address,
    /// Quote it bought with in the window, in wei.
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
    /// The funder.
    pub address: Address,
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
    /// points.
    pub coverage_bps: u16,
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
#[must_use]
pub fn shared_funders(checked: &[Candidate]) -> Vec<SharedFunder> {
    let mut counts: BTreeMap<[u8; 20], u32> = BTreeMap::new();
    for candidate in checked {
        let mut seen: Vec<[u8; 20]> = Vec::new();
        for funder in candidate.funders.iter().filter(|f| f.material) {
            // One candidate counts once per funder however many top-ups it got.
            if !seen.contains(&funder.address.0) {
                seen.push(funder.address.0);
                *counts.entry(funder.address.0).or_default() += 1;
            }
        }
    }
    let mut shared: Vec<SharedFunder> = counts
        .into_iter()
        .filter(|(_, n)| *n >= 2)
        .map(|(a, funded)| SharedFunder {
            address: Address(a),
            funded,
        })
        .collect();
    shared.sort_by(|a, b| b.funded.cmp(&a.funded).then(a.address.0.cmp(&b.address.0)));
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
        address: buyer.address,
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
                        address: from,
                        amount_wei: amount,
                        block,
                        transaction: hash,
                        unique_id,
                        material: is_material(amount, buyer.quote),
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
        coverage_bps: selection.coverage_bps,
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
                    recipient: c.address.to_string(),
                    funder: f.address.to_string(),
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
                    "{} of {} candidates checked, {} buyers, coverage {} bps; {}",
                    funding.checked.len(),
                    funding.selected,
                    funding.buyers,
                    funding.coverage_bps,
                    funding.gaps.join("; ")
                ),
                ran_at: std::time::SystemTime::now(),
            })
            .map_err(|e| e.to_string())?;
    }

    Ok(funding)
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
            address: addr(b),
            bought_wei: quote,
            first_purchase_block: 10,
            is_contract: Some(false),
            nonce_before_launch: Some(0),
            funders: funders
                .iter()
                .enumerate()
                .map(|(i, (f, amount))| Funder {
                    address: addr(*f),
                    amount_wei: *amount,
                    block: 5,
                    transaction: format!("0x{i}"),
                    unique_id: format!("0x{i}:external:0"),
                    material: is_material(*amount, quote),
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
        assert!(is_material(needed / 2, quote));
        assert!(!is_material(needed / 2 - 1, quote));
        // Dust against a real buy is never material.
        assert!(!is_material(1_000, quote));
        // A wallet that bought nothing is only "funded" by the gas allowance.
        assert!(is_material(GAS_ALLOWANCE_WEI / 2, 0));
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
                    address: addr(0xf0),
                    funded: 3
                },
                SharedFunder {
                    address: addr(0xf1),
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
    }
}
