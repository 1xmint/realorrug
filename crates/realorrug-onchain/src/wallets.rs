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

use realorrug_robinhood::pons::{self, CreatorRole, LaunchedToken, Side, Trade, Transfer, topic};
use realorrug_robinhood::{Address, Hash32, Log, LogsError, Rpc, quantity, quantity_u128};

use crate::budget::Budget;
use crate::memory::{CheckRun, Completeness, FundingEdge, Memory};
use crate::rpc::{RpcClient, SignatureInfo, Transaction, is_last_page};

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
    /// Tokens received, already decoded off `Trade::tokens` -- the same
    /// field [`Sale::tokens`] keeps for the sell side (S7). Carried through
    /// so a buyer's share of supply can be measured, not just its spend.
    pub tokens: u128,
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
            tokens: t.tokens,
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
    /// Tokens it bought in the window, summed over its purchases
    /// ([`Purchase::tokens`]) the same way `quote` sums theirs.
    pub tokens: u128,
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
            tokens: 0,
            first_block: p.block,
            first_position: p.position,
        });
        buyer.quote = buyer.quote.saturating_add(p.quote);
        buyer.tokens = buyer.tokens.saturating_add(p.tokens);
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
    /// Tokens it bought in the window ([`Buyer::tokens`]). `None` when the
    /// token amount was not read for this chain (Solana's candidates are
    /// built from a balance-rise walk, not a decoded `Trade`, and no token
    /// count is read there today) -- `None`, never a fabricated 0, per
    /// AGENTS.md rule 8 ("absent is not zero").
    pub bought_tokens: Option<u128>,
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
        bought_tokens: Some(buyer.tokens),
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

/// One `CurveBuy`/`CurveSell` whose beneficiary was the deployer or the fee
/// recipient -- design 0027 slice 5.
///
/// A plain ERC-20 `Transfer` out of either account is never turned into one
/// of these: only a decoded curve execution proves a sale (rule (b)), so
/// [`classify_creator_trades`] builds this list from [`Trade::from_log`]
/// alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatorTrade {
    /// Which of the two paid accounts was the beneficiary.
    pub role: CreatorRole,
    /// Buy or sell.
    pub side: Side,
    /// Quote paid in (a buy) or received (a sell), in the chain's own
    /// smallest unit -- wei for Robinhood, lamports for Solana. See
    /// [`CreatorCashFlow::quote_asset`] for which one, and its decimals.
    pub quote: u128,
    /// Tokens received (a buy) or given up (a sell).
    pub tokens: u128,
    /// The block it landed in (a Robinhood block number, or a Solana slot).
    pub block: u64,
    /// The transaction that carried it, in the chain's own canonical text
    /// form (`0x`-hex for Robinhood, base58 for Solana) -- a `String`, not
    /// [`Hash32`], because a Solana signature is 64 raw bytes and cannot fit
    /// that 32-byte type at all. See this module's own doc on why every
    /// cross-chain field here is the chain's own text form rather than a
    /// chain-specific typed one.
    pub transaction: String,
    /// A stable per-log id for memory's `(chain, unique_id)` key.
    pub unique_id: String,
}

/// A per-log identity built from the log's own transaction and position, the
/// same shape [`FundingEdge::unique_id`] uses a provider-supplied id for:
/// two logs in the same transaction still differ by index, so replaying the
/// same read can never collide two distinct events into one row.
#[must_use]
fn log_unique_id(log: &Log) -> String {
    let (transaction_index, log_index) = log
        .position
        .map_or((0, 0), |p| (p.transaction_index, p.log_index));
    format!("{}-{transaction_index}-{log_index}", log.transaction)
}

/// Every `CurveBuy`/`CurveSell` among `logs` whose beneficiary is `record`'s
/// deployer or fee recipient.
///
/// Only the recipient is compared, the same choice [`dev_buys`] makes and for
/// the same reason: the trader can be a router, but the wallet the tokens or
/// quote land with is the one whose cash flow this is.
#[must_use]
pub fn classify_creator_trades(logs: &[Log], record: &LaunchedToken) -> Vec<CreatorTrade> {
    logs.iter()
        .filter_map(|log| Trade::from_log(log).map(|t| (log, t)))
        .filter(|(_, t)| t.curve == record.curve)
        .filter_map(|(log, t)| {
            let role = pons::creator_role(&t.recipient, record)?;
            Some(CreatorTrade {
                role,
                side: t.side,
                quote: t.quote,
                tokens: t.tokens,
                block: log.block,
                transaction: log.transaction.to_string(),
                unique_id: log_unique_id(log),
            })
        })
        .collect()
}

/// How many ERC-20 transfers moved tokens out of the deployer or fee
/// recipient to somewhere other than `curve`.
///
/// A transfer landing on the curve is the token leg of a decoded sale
/// already counted by [`classify_creator_trades`]; counting it again here
/// would report the same movement once as a sale and once as an unexplained
/// transfer. Every other outgoing transfer -- to an exchange, another
/// wallet, anywhere else -- is real movement this reader cannot resolve to a
/// sale, so it is counted, never priced: rule (b) again, from the other
/// direction, since a transfer must never be *treated* as a sale even when it
/// is the only observed thing that happened to the tokens.
#[must_use]
pub fn count_transfers_out(logs: &[Log], record: &LaunchedToken, curve: &Address) -> u32 {
    u32::try_from(
        logs.iter()
            .filter_map(Transfer::from_log)
            .filter(|t| pons::creator_role(&t.from, record).is_some() && t.to != *curve)
            .count(),
    )
    .unwrap_or(u32::MAX)
}

/// The creator's observed cash flow on Pons v2, design 0027 slice 5.
///
/// `trades_complete` is the gate rule (c) needs: it is `true` only when both
/// the curve-trade read and the token-transfer read each returned a complete
/// result for their block range ([`Rpc::logs_range`] either does that or
/// fails explicitly, never truncates silently). When it is `false`,
/// [`Self::proceeds_wei`], [`Self::cost_basis_wei`] and [`Self::net_wei`]
/// all return `None` rather than a number computed from a partial trade
/// list -- absent is not zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatorCashFlow {
    /// Every decoded buy and sell attributed to the deployer or fee
    /// recipient.
    pub trades: Vec<CreatorTrade>,
    /// Outgoing ERC-20 transfers from either account that were not a
    /// decoded sale (never priced; see [`count_transfers_out`]).
    pub transfers_out: u32,
    /// Whether both reads behind `trades` and `transfers_out` were complete.
    pub trades_complete: bool,
    /// Why a read fell short, when it did.
    pub gaps: Vec<String>,
    /// What `CreatorTrade::quote` is denominated in -- native ETH for
    /// Robinhood, native SOL for Solana. Carried on the result itself,
    /// rather than inferred by the caller from which chain it thinks it
    /// asked, so a renderer (`realorrug-roast`'s `push_creator_cash_flow`)
    /// can never print one chain's lamports labelled with another chain's
    /// symbol (AGENTS.md section 3 rule 2: no fabricated fact, which a
    /// wrong unit label would be).
    pub quote_asset: crate::dossier::QuoteAsset,
}

impl CreatorCashFlow {
    /// Total quote received across every decoded sell, or `None` when the
    /// trade history is incomplete.
    #[must_use]
    pub fn proceeds_wei(&self) -> Option<u128> {
        if !self.trades_complete {
            return None;
        }
        Some(
            self.trades
                .iter()
                .filter(|t| t.side == Side::Sell)
                .fold(0u128, |sum, t| sum.saturating_add(t.quote)),
        )
    }

    /// Total quote paid across every decoded buy, or `None` when the trade
    /// history is incomplete.
    #[must_use]
    pub fn cost_basis_wei(&self) -> Option<u128> {
        if !self.trades_complete {
            return None;
        }
        Some(
            self.trades
                .iter()
                .filter(|t| t.side == Side::Buy)
                .fold(0u128, |sum, t| sum.saturating_add(t.quote)),
        )
    }

    /// Proceeds minus cost basis, or `None` when either half is unknown.
    #[must_use]
    pub fn net_wei(&self) -> Option<i128> {
        let proceeds = i128::try_from(self.proceeds_wei()?).unwrap_or(i128::MAX);
        let cost = i128::try_from(self.cost_basis_wei()?).unwrap_or(i128::MAX);
        Some(proceeds - cost)
    }
}

/// Reads the deployer's and fee recipient's on-chain cash flow for one
/// launch: every decoded buy/sell against its own curve, and every other
/// outgoing token transfer.
///
/// Two independent `eth_getLogs` reads, matching the CU table in this
/// module's doc comment: `record.curve`'s own logs (`CU_GET_LOGS`) decode
/// into [`CreatorTrade`]s, and `record.token`'s `Transfer` logs
/// (`CU_GET_LOGS`) count outgoing transfers. Never returns `Err`: a read
/// that fails or that the budget cannot afford degrades to a named gap and
/// `trades_complete = false`, per rule (c) -- a caller here has no total
/// failure to propagate, only a cash flow that may or may not be provably
/// whole.
#[must_use]
pub fn creator_cash_flow(
    client: &Rpc,
    budget: &mut Budget,
    record: &LaunchedToken,
    launch_block: u64,
    read_block: u64,
) -> CreatorCashFlow {
    let mut gaps = Vec::new();

    let trades = match take(budget, CU_GET_LOGS).and_then(|()| {
        client
            .logs_range(&record.curve, &[], launch_block, read_block)
            .map_err(|e| match e {
                LogsError::TooManyResults => {
                    "the token's lifetime curve activity held more trades than one read returns"
                        .to_owned()
                }
                LogsError::Other(why) => why,
            })
    }) {
        Ok(logs) => Some(classify_creator_trades(&logs, record)),
        Err(gap) => {
            gaps.push(format!("creator trade history: {gap}"));
            None
        }
    };

    let transfers_out = match take(budget, CU_GET_LOGS).and_then(|()| {
        client
            .logs_range(&record.token, &[topic::TRANSFER], launch_block, read_block)
            .map_err(|e| match e {
                LogsError::TooManyResults => {
                    "the token's lifetime transfer history held more transfers than one read \
                     returns"
                        .to_owned()
                }
                LogsError::Other(why) => why,
            })
    }) {
        Ok(logs) => Some(count_transfers_out(&logs, record, &record.curve)),
        Err(gap) => {
            gaps.push(format!("creator transfer history: {gap}"));
            None
        }
    };

    let trades_complete = trades.is_some() && transfers_out.is_some();
    CreatorCashFlow {
        trades: trades.unwrap_or_default(),
        transfers_out: transfers_out.unwrap_or(0),
        trades_complete,
        gaps,
        quote_asset: crate::dossier::QuoteAsset::eth(),
    }
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

/// What reading a transaction's lamport balances for one candidate found.
///
/// Kept distinct from a plain `Option` (AGENTS.md rule 8: absent is not
/// zero) because [`funding_search`] must not treat "the node never gave us
/// `preBalances`/`postBalances`" the same as "we read them and there was no
/// inbound move" -- the first is a hole in the read, the second is a
/// measurement.
#[derive(Debug, PartialEq, Eq)]
enum FunderRead {
    /// A candidate for the funder, with the lamport amount moved.
    Found(String, u128),
    /// The balances were read and there was no material inbound move.
    NoInboundMove,
    /// This transaction's lamport balances could not be trusted at all --
    /// missing arrays, a shape mismatch, or the candidate's own account not
    /// among the keys. The search cannot call this transaction's absence
    /// measured.
    Unreadable,
}

/// The funder of one candidate's earliest lamport balance increase, read
/// from a single transaction's `pre_balances`/`post_balances`.
///
/// The account whose own balance rose is the candidate; the account whose
/// balance fell the most is taken as the source, because a transaction can
/// move lamports through several accounts (fees, rent) and the largest drop
/// is the one that plausibly funded the candidate's gain rather than a fee
/// payer's small deduction.
fn funder_of(tx: &Transaction, candidate: &str) -> FunderRead {
    if tx.accounts.len() != tx.pre_balances.len() || tx.accounts.len() != tx.post_balances.len() {
        // A shape this reader cannot trust an index into; see `rpc.rs`'s
        // `lamport_balances` doc on why a real node does not do this.
        //
        // This check is also the load-bearing guard for a versioned
        // transaction whose lookup-table accounts were not merged in: a node
        // that omits `meta.loadedAddresses` still returns `preBalances`/
        // `postBalances` sized for the full account list, so `accounts` and
        // the balance arrays disagree in length and this arm fires before
        // `is_plain_sol_transfer` is ever consulted. Do not drop it in a
        // later refactor on the assumption the lengths always match.
        return FunderRead::Unreadable;
    }
    let Some(candidate_index) = tx.accounts.iter().position(|a| a == candidate) else {
        // The candidate's own account is not even among this transaction's
        // keys, so there is no balance to read for them at all -- a hole in
        // the read, not a measurement that nothing moved.
        return FunderRead::Unreadable;
    };
    let Some(gain) =
        tx.post_balances[candidate_index].checked_sub(tx.pre_balances[candidate_index])
    else {
        // The candidate's own balance fell or stayed level: read
        // successfully, and it was not an inbound move.
        return FunderRead::NoInboundMove;
    };
    if gain == 0 {
        return FunderRead::NoInboundMove;
    }
    let found = tx
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
        .max_by_key(|&(_, d)| d);
    match found {
        Some((from_index, drop)) => {
            FunderRead::Found(tx.accounts[from_index].clone(), u128::from(drop.min(gain)))
        }
        None => FunderRead::NoInboundMove,
    }
}

/// The System Program's own id -- the only program a plain SOL transfer
/// ever invokes.
const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";

/// Programs that ride along on a plain transfer without moving lamports
/// between accounts: a priority-fee setting and a memo cannot make a pool
/// the largest loser, so their presence does not defeat `funder_of`.
///
/// Verified against `solana-sdk-ids` (`compute_budget::declare_id!` in
/// `anza-xyz/solana-sdk`'s `sdk-ids/src/lib.rs`, master branch, fetched
/// 2026-09-22) for the Compute Budget program, and against
/// `solana-program`'s memo interface (`v1`/`v3` modules in
/// `solana-program/memo`'s `interface/src/lib.rs`, main branch, fetched
/// 2026-09-22) for both Memo program ids -- "v3" there is the id most
/// wallets and this list call "Memo v2".
const FEE_ONLY_PROGRAMS: [&str; 3] = [
    "ComputeBudget111111111111111111111111111111",
    "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr",
    "Memo1UhkJRfHyvLMcVucJwxXeuD728EqVDDwQDxFMNo",
];

/// Whether `tx` is shaped like a plain SOL transfer: it has at least one
/// instruction, at least one of them is the System Program, and every
/// instruction, top-level or inner (see [`Transaction::instructions`]'s doc
/// on why inner CPIs are flattened in), belongs to either the System
/// Program or [`FEE_ONLY_PROGRAMS`].
///
/// This is the gate [`funding_search`] applies before trusting
/// [`funder_of`]'s balance heuristic at all. `funder_of` reads only lamport
/// balances -- it cannot tell a genuine transfer from a swap, a sell, a
/// rent refund or a wrapped-SOL unwrap, all of which can move the largest
/// lamport drop in the transaction to an account that never funded anyone
/// (a bonding-curve vault, an AMM pool, the candidate's own closed token
/// account). A withdrawal from an exchange -- the real signal this check
/// exists to catch -- is exactly a System Program transfer, but real
/// wallets (Phantom among them) routinely prepend a `ComputeBudget`
/// priority-fee instruction, and exchanges sometimes append a `Memo`;
/// neither moves lamports between accounts, so neither can turn a pool into
/// the largest loser, and rejecting the transfer over their presence alone
/// would read an unread transfer as a measured absence. The "at least one
/// System instruction" clause still matters: a transaction of nothing but
/// `ComputeBudget` instructions moves no lamports at all and must not pass.
///
/// A real Solana transaction always carries at least one instruction, so an
/// empty list is never a fact about the chain -- it is a fact about this
/// process, which did not read the instructions (a parser gap, a versioned
/// transaction whose address-lookup-table accounts were not merged in, a
/// short read). Reporting `false` here for an empty list is not enough on
/// its own: the caller must also not count that signature as a measured
/// miss (AGENTS.md rule 8, "absent is not zero"), since `false` from a real
/// swap and `false` from an unread transaction mean different things.
fn is_plain_sol_transfer(tx: &Transaction) -> bool {
    !tx.instructions.is_empty()
        && tx
            .instructions
            .iter()
            .any(|ix| ix.program == SYSTEM_PROGRAM)
        && tx.instructions.iter().all(|ix| {
            ix.program == SYSTEM_PROGRAM || FEE_ONLY_PROGRAMS.contains(&ix.program.as_str())
        })
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
    /// The signature of the buyer's own first purchase of this mint --
    /// [`funding_search`]'s starting `before` value, so its walk of the
    /// buyer's signature history begins right at the purchase instead of at
    /// the wallet's newest activity (which, for a wallet that kept trading
    /// afterward, can be entirely unrelated history the funding search has
    /// no reason to page through).
    first_purchase_signature: String,
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
/// own funder is found by [`funding_search`], which pages the candidate's
/// signature history backward *from its first purchase*, newest-first, for
/// the most recent material inbound SOL transfer at or before that purchase
/// -- not its wallet's oldest transaction ever. That is both the more useful
/// fact (a wallet built to buy one launch was funded by the transfer that
/// financed the buy, whoever sent it) and the one the network can actually
/// serve, because `getSignaturesForAddress` already pages newest-first, so
/// walking back from the purchase is the cheap direction (research 0056's
/// "the fourth read" addendum). Funders are tested on the same terms
/// [`funder_of`] already reads for Robinhood (the account whose balance fell
/// the most) and [`is_material`] against [`GAS_ALLOWANCE_LAMPORTS`] (quote 0:
/// no SOL cost of the buy itself is read here, so materiality falls back to
/// "more than dust").
///
/// **If the mint's own history was truncated before its window could be
/// read, none of the buyers found in what *was* read are "the early
/// buyers"** -- a budget that runs out paging backward from the newest
/// drops the oldest, undiscovered page first, so the window is not
/// necessarily the earliest one. Nothing is checked, `Funding::buyers` and
/// `checked` are both empty, and the gap says why. **If a candidate's own
/// search hits [`MAX_FUNDING_SIGNATURE_PAGES`] or
/// [`MAX_FUNDING_TRANSACTIONS`], or a transaction read fails, no funder is
/// recorded and `Candidate::funding_complete` is false** -- a capped or
/// failed search never counts as having measured the absence of a funder;
/// both cases are named in [`Funding::gaps`] (AGENTS.md rule 8).
///
/// `mint_signatures` reuses the mint's signature history dossier step 1
/// already read, rather than walking it a second time: two walks of the same
/// address against the same shared [`Budget`] is exactly the amplifier this
/// crate's whole design exists to prevent (`budget.rs`'s own module doc), and
/// re-walking here was what left this read starved on any mint whose history
/// alone spent the page allowance (finding: a mint history landing at exactly
/// the page cap left this read, and everything after it, with nothing).
/// `None` is for the one caller that has no signatures to hand in --
/// `dossier::build` when the launch block came from memory and step 1's walk
/// never ran -- and falls back to reading them here.
///
/// # Errors
///
/// A string naming why the mint's own signature history could not be read at
/// all (only reachable when `mint_signatures` is `None`). A single candidate
/// or transaction read failure lands in [`Funding::gaps`] instead, on a
/// result that is still returned.
pub fn investigate_solana(
    client: &RpcClient,
    budget: &mut Budget,
    mint: &realorrug_types::Address,
    mint_signatures: Option<&(Vec<SignatureInfo>, bool)>,
) -> Result<Funding, String> {
    let curve = realorrug_pumpfun::pda::bonding_curve(mint).map(|c| c.to_string());
    let mint_key = mint.to_string();
    let owned_signatures;
    let (signatures, mut truncated): (&[SignatureInfo], bool) =
        if let Some((sigs, cut)) = mint_signatures {
            (sigs.as_slice(), *cut)
        } else {
            owned_signatures = client
                .signatures_back_to_oldest(budget, mint)
                .map_err(|e| format!("funding: {e}"))?;
            (owned_signatures.0.as_slice(), owned_signatures.1)
        };

    // A newest-first walk that ran out of page budget before reaching the
    // beginning gets one shot at an ascending-order read instead (research
    // 0056 addendum, 2026-09-23: 7 of 9 real pump.fun mints hit this in
    // production). See `RpcClient::signatures_oldest_first`'s own doc for why
    // its list can be trusted as complete from the start even though the
    // backward walk was not. Any failure there leaves `truncated` as it was,
    // and the gap below still fires (AGENTS.md rule 8: unknown is not safe).
    // Only for a walk made here: a caller that hands its own history in
    // (`dossier::build`) already made this same read before handing it over,
    // so a list still `truncated` means the read failed there, and asking
    // the node again would only spend a second call on the same refusal.
    let owned_ascending;
    let mut ascending = false;
    let signatures: &[SignatureInfo] = if truncated && mint_signatures.is_none() {
        // Same starvation `Budget::grant_pages`'s own doc describes for the
        // other named walks: the mint's own walk above can spend the whole
        // shared page pool getting to `truncated`, so this one-shot call
        // gets the same floor rather than silently finding nothing left.
        budget.grant_pages(1);
        match client.signatures_oldest_first(budget, mint, crate::rpc::PAGE_SIZE) {
            Ok(Some(asc)) if !asc.is_empty() => {
                owned_ascending = asc;
                truncated = false;
                ascending = true;
                owned_ascending.as_slice()
            }
            _ => signatures,
        }
    } else {
        signatures
    };

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

    // `signatures` is newest-first (the shape `getSignaturesForAddress`
    // returns), the mint's own fallback above returns oldest-first already
    // -- walk each so transactions are visited in the order they happened,
    // and stop once `SOLANA_WINDOW_TRANSACTIONS` of them were successfully
    // read -- that window, not the capped candidate list, is where `buyers`
    // comes from.
    let mut gaps = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut window_buyers: Vec<EarlyBuyer> = Vec::new();
    let mut window_read = 0usize;
    let ordered: Vec<&SignatureInfo> = if ascending {
        signatures.iter().collect()
    } else {
        signatures.iter().rev().collect()
    };
    for sig in ordered {
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
                            first_purchase_signature: sig.signature.clone(),
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
    // The candidates' own page floor, granted here and not by the caller:
    // by now `dossier::build` step 1 (and, for a cached launch, the walk
    // above) has usually spent the shared page pool, and every candidate
    // then reported "page budget exhausted" (research 0056's 2026-09-23
    // addendum). Granted by the caller instead, the walk above would draw on
    // it first and could spend it all. A floor, not an addition
    // (`Budget::grant_pages`), sized for every candidate walking to its cap.
    budget.grant_pages(FUNDING_PAGE_FLOOR);
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

/// How many `getTransaction` fetches [`funding_search`] will make for one
/// buyer while looking for its funder.
///
/// A wallet built to buy one launch shows a handful of transactions in this
/// window -- the transfer that funded it, the buy itself, maybe one or two
/// more -- so ten fetches is a wide margin over that shape. A wallet that
/// needs more than ten `getTransaction` calls just to find a signature
/// landing at or before its first purchase is not a fresh wallet created for
/// this launch; it is one a stranger could have chosen specifically to be
/// expensive to read, and this read should stop and say so rather than keep
/// paying for it (AGENTS.md rule 8: a capped search is not a measurement).
pub const MAX_FUNDING_TRANSACTIONS: usize = 10;

/// How many `getSignaturesForAddress` pages [`funding_search`] will walk
/// back through one buyer's history looking for a signature at or before its
/// first purchase.
///
/// Sized the same way [`crate::budget::PAGES_PER_WALK`] is: three pages (up
/// to 3,000 signatures, [`crate::rpc::PAGE_SIZE`] each) covers the entire
/// recent history of a wallet built for this launch, including one that kept
/// trading afterward. A wallet whose most recent 3,000 signatures are *all*
/// newer than its own purchase of this mint is busy enough that finishing
/// the walk would mean reading deep into a history this check has no reason
/// to trust is honestly priced.
pub const MAX_FUNDING_SIGNATURE_PAGES: usize = 3;

/// The call floor [`dossier::build`](crate::dossier::build) grants step 5
/// (`investigate_solana`) with [`crate::budget::Budget::grant_calls`],
/// immediately before that step runs.
///
/// Sized from this module's own inner caps, the same way
/// [`crate::budget::PAGES_PER_WALK`] sizes the page floor from
/// [`crate::budget::DEFAULT_MAX_PAGES`]: each of up to [`MAX_CANDIDATES`]
/// checked candidates can spend up to [`MAX_FUNDING_SIGNATURE_PAGES`] pages
/// (a page is also a call, `Budget::take_page`'s own doc) plus
/// [`MAX_FUNDING_TRANSACTIONS`] `getTransaction` fetches before
/// [`funding_search`] gives up on it -- a per-candidate ceiling this floor
/// does not raise, only guarantees each candidate actually gets to spend
/// against. A floor, not an addition (`grant_calls`'s own doc): a budget
/// that already has this many calls left keeps them, so this can only widen
/// funding's share of an already-starved [`crate::budget::DEFAULT_MAX_CALLS`],
/// never the ceiling itself.
// The cast is exact, not lossy: `MAX_CANDIDATES`, `MAX_FUNDING_SIGNATURE_PAGES`
// and `MAX_FUNDING_TRANSACTIONS` are all small compile-time constants (52
// today), nowhere near `u32::MAX` -- `usize::try_from` is not yet callable in
// a const context on stable, which is the only reason this is `as` rather
// than the fallible conversion the rest of this module uses at runtime.
#[allow(clippy::cast_possible_truncation)]
pub const FUNDING_CALL_FLOOR: u32 =
    (MAX_CANDIDATES * (MAX_FUNDING_SIGNATURE_PAGES + MAX_FUNDING_TRANSACTIONS)) as u32;

/// The page floor [`dossier::build`](crate::dossier::build) grants step 5
/// (`investigate_solana`) with [`crate::budget::Budget::grant_pages`],
/// unconditionally, immediately before that step runs.
///
/// Granted only when `mint_signatures.is_none()` before this fix -- but step
/// 1 (the mint's own signature walk) usually runs first and spends the
/// shared page pool, so by the time step 5 starts, `mint_signatures` is
/// almost always `Some` and this step got no floor of its own at all. Sized
/// the same way [`FUNDING_CALL_FLOOR`] is: each of up to [`MAX_CANDIDATES`]
/// checked candidates can spend up to [`MAX_FUNDING_SIGNATURE_PAGES`] pages
/// before [`funding_search`] gives up on it, so this guarantees every
/// candidate actually gets to spend its own per-candidate page ceiling
/// against, the same way `FUNDING_CALL_FLOOR` guarantees calls. A floor, not
/// an addition (`grant_pages`'s own doc): a budget that already has this
/// many pages left keeps them.
#[allow(clippy::cast_possible_truncation)]
pub const FUNDING_PAGE_FLOOR: u32 = (MAX_CANDIDATES * MAX_FUNDING_SIGNATURE_PAGES) as u32;

/// Searches backward through one buyer's own signature history for the most
/// recent material inbound SOL transfer at or before its first purchase of
/// this mint -- the wallet's funder (research 0056's "the fourth read"
/// addendum), not its oldest transaction ever.
///
/// `getSignaturesForAddress` pages newest-first, so walking backward from the
/// purchase is the cheap direction: a wallet created to buy one launch has
/// its funding transfer somewhere in its most recent history before the buy,
/// and this never needs to reach the wallet's actual beginning to find it.
///
/// Returns `(complete, funder)`:
/// - A material funder found -- `(true, Some(funder))`.
/// - The wallet's own history ended (an empty or short page,
///   [`crate::rpc::is_last_page`], the same test
///   [`RpcClient::signatures_back_to_oldest`] uses) with every eligible
///   signature in it fetched, its lamport balances all readable, and none
///   material -- `(true, None)`. This is a **measured** absence: every
///   signature at or before the first purchase was read, and none of them
///   was a material transfer in.
/// - Either cap in this module was reached, a transaction read failed, or
///   at least one eligible transaction's lamport balances could not be read
///   at all -- `(false, None)`, with a gap pushed naming which. Never
///   returned as a measured absence (AGENTS.md rule 8): a capped, failed or
///   partly-unreadable search says nothing about whether a funder exists
///   past the point it stopped seeing clearly.
///
/// A transaction that is not shaped like a plain SOL transfer
/// ([`is_plain_sol_transfer`]) is neither a funder nor a measured absence
/// for that signature specifically -- it is not an answer to the question
/// at all, so the walk simply continues to the next signature, still
/// counted against [`MAX_FUNDING_TRANSACTIONS`]. The one exception: a
/// material lamport move whose instruction list is empty did not fail the
/// plain-transfer gate on its shape -- there was no shape to judge, because
/// this process never read its instructions. That is the same "could not
/// tell" as [`FunderRead::Unreadable`], so it sets the same `unreadable`
/// flag and pushes a gap naming the transaction, rather than silently
/// moving on as a genuine non-transfer would.
/// Formats a gap for a transaction [`funding_search`] could not use to
/// answer "was this the funder" one way or the other -- shared by the
/// missing-balances and empty-instructions cases, which both set the same
/// `unreadable` flag for the same reason (AGENTS.md rule 8).
fn unreadable_gap(address_key: &str, signature: &str, why: &str) -> String {
    format!("funding of {address_key}: transaction {signature} {why}")
}

/// Turns one signature's `funder_of` read into either a final answer for
/// [`funding_search`] to return (`Some`) or nothing, meaning the walk keeps
/// going (`None`), setting `*unreadable` and pushing a gap along the way
/// when the read could not settle the question. Split out of
/// `funding_search` to keep that function's loop body short -- see its doc
/// comment for why an empty instruction list is treated the same as
/// [`FunderRead::Unreadable`] rather than as a genuine non-transfer.
fn resolve_funder_read(
    read: FunderRead,
    tx: &Transaction,
    signature: &str,
    address_key: &str,
    gaps: &mut Vec<String>,
    unreadable: &mut bool,
) -> Option<(bool, Option<Funder>)> {
    match read {
        FunderRead::Found(from, amount)
            if is_material(amount, 0, GAS_ALLOWANCE_LAMPORTS) && is_plain_sol_transfer(tx) =>
        {
            Some((
                true,
                Some(Funder {
                    address: from,
                    amount_wei: amount,
                    block: tx.slot.0,
                    transaction: signature.to_owned(),
                    unique_id: signature.to_owned(),
                    material: true,
                }),
            ))
        }
        FunderRead::Unreadable => {
            gaps.push(unreadable_gap(
                address_key,
                signature,
                "did not report lamport balances; cannot confirm no funder there",
            ));
            *unreadable = true;
            None
        }
        // `is_material` guards this arm for the same reason it guards the
        // accepted-transfer arm above: an immaterial balance move is not a
        // funder no matter what the (unread) instructions were, so it is
        // not worth reporting a gap over -- do not simplify this to "any
        // empty instruction list is a gap".
        FunderRead::Found(_, amount)
            if is_material(amount, 0, GAS_ALLOWANCE_LAMPORTS) && tx.instructions.is_empty() =>
        {
            gaps.push(unreadable_gap(
                address_key,
                signature,
                "reported no instructions; cannot confirm it was not a funding transfer",
            ));
            *unreadable = true;
            None
        }
        FunderRead::Found(..) | FunderRead::NoInboundMove => None,
    }
}

fn funding_search(
    client: &RpcClient,
    budget: &mut Budget,
    address: &realorrug_types::Address,
    address_key: &str,
    first_purchase_slot: u64,
    first_purchase_signature: &str,
    gaps: &mut Vec<String>,
) -> (bool, Option<Funder>) {
    // Start right at the purchase, not at the wallet's newest activity: the
    // purchase signature is the buyer's own, so `getSignaturesForAddress`
    // with it as `before` returns exactly the history at or before the
    // purchase (Solana JSON-RPC docs), which is the only part of the wallet
    // this search ever wants. Starting at the newest signature instead (the
    // old behaviour) meant a wallet that kept trading after the purchase
    // spent this search's whole page allowance on history that could never
    // contain the funder.
    let mut before: Option<String> = Some(first_purchase_signature.to_owned());
    // Set once, and only matters on the very first iteration: if the node
    // rejects the purchase signature as a `before` value (a transport or
    // node error, not a budget exhaustion), fall back to the old
    // newest-first walk rather than giving up on this candidate outright.
    // In practice the purchase transaction is always in the buyer's own
    // history, since the buyer signed it, so this fallback is a safety net
    // for an uncooperative RPC, not the expected path.
    let mut starting_from_purchase = true;
    let mut pages = 0usize;
    let mut fetched = 0usize;
    // Set when any eligible transaction's lamport balances could not be
    // read at all -- makes the difference (AGENTS.md rule 8) between "we
    // read every eligible signature and none was a funder" and "we could
    // not tell for at least one of them".
    let mut unreadable = false;

    loop {
        if pages >= MAX_FUNDING_SIGNATURE_PAGES {
            gaps.push(format!(
                "funding of {address_key}: more than {MAX_FUNDING_SIGNATURE_PAGES} signature \
                 pages walked without reaching its first purchase or the end of its history; no \
                 funder recorded"
            ));
            return (false, None);
        }
        pages += 1;

        let page = match client.signatures_page(budget, address, before.as_deref()) {
            Ok(Some(p)) => p,
            Ok(None) => {
                gaps.push(format!(
                    "funding of {address_key}: page budget exhausted while searching for its \
                     funder; no funder recorded"
                ));
                return (false, None);
            }
            Err(why) => {
                if pages == 1 && starting_from_purchase {
                    // The purchase-anchored read itself failed -- fall back
                    // to the old newest-first walk and retry, without
                    // spending this failed attempt against the page cap or
                    // recording a gap for it. If the fallback also fails,
                    // the ordinary `Err` handling below reports it.
                    before = None;
                    pages = 0;
                    starting_from_purchase = false;
                    continue;
                }
                gaps.push(format!("funding of {address_key}: {why}"));
                return (false, None);
            }
        };

        let reached_end = page.is_empty() || is_last_page(page.len());
        if let Some(last) = page.last() {
            before = Some(last.signature.clone());
        }

        for sig in &page {
            // Landed after the purchase it would have to finance: cannot be
            // the funder. `>`, not `>=` -- a transfer in the same slot as the
            // purchase can still have landed before it within that slot.
            if sig.slot > first_purchase_slot {
                continue;
            }
            if sig.err.is_some() {
                continue;
            }
            if fetched >= MAX_FUNDING_TRANSACTIONS {
                gaps.push(format!(
                    "funding of {address_key}: more than {MAX_FUNDING_TRANSACTIONS} \
                     transactions fetched; no funder recorded"
                ));
                return (false, None);
            }
            fetched += 1;
            match client.transaction(budget, &sig.signature) {
                Ok(Some(tx)) => {
                    let read = funder_of(&tx, address_key);
                    if let Some(result) = resolve_funder_read(
                        read,
                        &tx,
                        &sig.signature,
                        address_key,
                        gaps,
                        &mut unreadable,
                    ) {
                        return result;
                    }
                }
                Ok(None) => {
                    gaps.push(format!(
                        "funding of {address_key}: transaction {} could not be fetched",
                        sig.signature
                    ));
                    return (false, None);
                }
                Err(why) => {
                    gaps.push(format!(
                        "funding of {address_key}: transaction {}: {why}",
                        sig.signature
                    ));
                    return (false, None);
                }
            }
        }

        if reached_end {
            if fetched == 0 {
                // The candidate's own purchase sits at `first_purchase_slot`
                // in its own signature history, so a walk that read the
                // page(s) back to the end of history without fetching a
                // single eligible transaction did not measure an absence --
                // it never read anything. That means the signature read
                // itself was wrong or filtered (an empty first page, or
                // every signature landing after the purchase), not that the
                // candidate genuinely has no funder.
                gaps.push(format!(
                    "funding of {address_key}: no signature at or before its first purchase \
                     (slot {first_purchase_slot}) was readable; nothing was measured"
                ));
                return (false, None);
            }
            // Every eligible signature in the wallet's whole history was
            // read and none was a material inbound transfer: a measurement,
            // not a guess (AGENTS.md rule 8) -- but only if every one of
            // those reads actually had lamport balances to look at. If any
            // did not, the absence is not measured; say so.
            return (!unreadable, None);
        }
    }
}

/// Reads one early buyer's funder: the most recent material inbound SOL
/// transfer at or before its first purchase ([`funding_search`]). Split out
/// of [`investigate_solana`] so a failure on one candidate is a gap on the
/// overall result, not a reason to abandon the others.
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
        // Not read here: `buyers_in` only reports which wallets' balance of
        // the mint rose, not by how much beyond "some" (the same reason
        // `bought_wei` above is 0, not a quote). `None`, not 0, so a later
        // reader cannot mistake "not read" for "bought nothing".
        bought_tokens: None,
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

    // No extra page floor is granted here (contrast `dossier.rs`'s steps 3
    // and 6): `investigate_solana` grants the whole candidate loop's page
    // floor (`FUNDING_PAGE_FLOOR`) once, immediately before the loop --
    // sized for every candidate it can check, not just one. Granting a second floor per
    // candidate on top of that would let up to `MAX_CANDIDATES` candidates
    // each restack the floor, silently widening the funding step's own
    // share of the shared pool at step 6's expense. `MAX_FUNDING_SIGNATURE_PAGES`
    // and `MAX_FUNDING_TRANSACTIONS` already bound what one candidate can
    // spend, so a candidate low on shared pages simply reports a gap rather
    // than reading past what remains -- the same honest degradation the
    // rest of this module relies on.
    let (complete, funder) = funding_search(
        client,
        budget,
        &address_key,
        &address,
        buyer.first_purchase_slot,
        &buyer.first_purchase_signature,
        gaps,
    );
    candidate.funding_complete = complete;
    if let Some(funder) = funder {
        candidate.funders.push(funder);
    }
    candidate
}

/// How many signatures of the creator's own associated token account
/// [`creator_cash_flow_solana`] will read before giving up and reporting an
/// incomplete history.
///
/// Sized the same way [`DEFAULT_MAX_CALLS`](crate::budget::DEFAULT_MAX_CALLS)
/// is: a deployer who never sold holds far fewer than this many signatures
/// against their own ATA, and a wallet that has traded the mint two hundred
/// times over its life is already outside what a single dossier read should
/// try to reconstruct in full -- reporting "incomplete, here is what we saw"
/// is the honest answer past that point, not paging further into a wallet a
/// stranger could have picked specifically to be expensive.
pub const CREATOR_CASH_FLOW_MAX_SIGNATURES: usize = 200;

/// What one signature in the creator's ATA history turned out to be, as read
/// purely from the transaction's own balance deltas -- no RPC calls, so this
/// is unit-testable against a fixture with no transport at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SolanaCashFlowEvent {
    /// The creator's token balance rose and their own SOL balance fell: a
    /// buy, quoted at the SOL paid.
    Buy { quote: u128, tokens: u128 },
    /// The creator's token balance fell and their own SOL balance rose: a
    /// sell, quoted at the SOL received.
    Sell { quote: u128, tokens: u128 },
    /// The creator's token balance fell with no matching SOL increase --
    /// an outgoing transfer, never priced (the same rule the Robinhood
    /// reader's `count_transfers_out` uses).
    TransferOut,
    /// Nothing this reader attributes to the creator: a received transfer
    /// (token up, SOL not down), or no balance change of either kind for
    /// this creator in this transaction at all.
    Ignored,
}

/// Classifies one transaction against the creator's ATA history, from its
/// own pre/post token and lamport balances alone.
///
/// **The SOL "quote" is the creator's net lamport change in the whole
/// transaction, not the trade's price.** It includes the transaction fee and
/// whatever else the transaction did to the creator's own account -- there is
/// no cheaper, reliable way to isolate "the AMM leg alone" from a raw
/// balance diff without decoding the swap instruction itself, which this
/// reader does not do. Do not invent precision beyond what a balance diff
/// gives: this is a net-change approximation, stated as one, not a fill
/// price.
///
/// `creator` is compared against the transaction's account keys (including a
/// v0 transaction's loaded addresses, already merged into
/// [`Transaction::accounts`] by [`crate::rpc::parse_transaction`]) to find
/// the creator's own account index for the lamport balances; `mint` is
/// compared against each token balance's `mint` field to find the creator's
/// own token balance entries.
fn classify_creator_transaction(
    tx: &Transaction,
    creator: &str,
    mint: &str,
) -> SolanaCashFlowEvent {
    // Sum every entry for this creator and mint, not just the first: a
    // transaction that touches two of the creator's own accounts for the
    // same mint (a temporary account plus the ATA, for instance) would
    // otherwise compare whichever entry happened to be `.find`'s first hit
    // in `pre` against a possibly different one in `post`, and read a
    // same-account round trip as a phantom sale or purchase.
    let token_before: u64 = tx
        .pre_token_balances
        .iter()
        .filter(|b| b.mint == mint && b.owner.as_deref() == Some(creator))
        .map(|b| b.amount)
        .sum();
    let token_after: u64 = tx
        .post_token_balances
        .iter()
        .filter(|b| b.mint == mint && b.owner.as_deref() == Some(creator))
        .map(|b| b.amount)
        .sum();

    let Some(creator_index) = tx.accounts.iter().position(|a| a == creator) else {
        // The creator's own account never appears in this transaction's
        // keys at all, so there is no lamport balance to read for them --
        // whatever the token balances above say, there is nothing to price
        // it against. Falls through to the no-SOL-data branches below.
        return classify_without_lamports(token_before, token_after);
    };
    let lamports_before = tx.pre_balances.get(creator_index).copied();
    let lamports_after = tx.post_balances.get(creator_index).copied();
    let Some((before, after)) = lamports_before.zip(lamports_after) else {
        return classify_without_lamports(token_before, token_after);
    };

    // A token account close refunds its rent (~0.00204 SOL) to its owner,
    // and a closed account has no post balance at all -- read as 0 by the
    // `map_or(0, ..)`-style default above (now a `.sum()` over zero
    // matching entries). That makes a plain burn-and-close, or a
    // transfer-all-and-close with no trade involved, look exactly like a
    // sale (token down, lamports up from the rent refund), and its mirror
    // (receiving tokens while paying rent to create a fresh account) look
    // like a buy. Neither is a trade unless a known trading program was
    // actually invoked in this transaction -- so only *that* case may ever
    // become Buy/Sell; every other token-balance move without a trading
    // program falls back to the same unpriced transfer/ignore rule
    // `classify_without_lamports` already applies when there is no SOL data
    // at all.
    if invokes_known_trading_program(tx) {
        match token_after.cmp(&token_before) {
            std::cmp::Ordering::Greater if after < before => {
                return SolanaCashFlowEvent::Buy {
                    quote: u128::from(before - after),
                    tokens: u128::from(token_after - token_before),
                };
            }
            std::cmp::Ordering::Less if after > before => {
                return SolanaCashFlowEvent::Sell {
                    quote: u128::from(after - before),
                    tokens: u128::from(token_before - token_after),
                };
            }
            _ => {}
        }
    }
    classify_without_lamports(token_before, token_after)
}

/// The known Solana trading venues a creator's balance move is trusted to
/// mean a real trade against, rather than an account-close rent refund or
/// an ordinary transfer that happened to land next to one.
///
/// Every address is the venue's documented program id, quoted in full below
/// so a diff against a fresh capture is a string compare. `realorrug-decode`
/// already owns [`realorrug_decode::pumpfun::PROGRAM_ID`] and
/// [`realorrug_decode::pumpswap::PROGRAM_ID`] (both mainnet-verified there);
/// the rest are not decoded anywhere in this crate yet, so they are named
/// here as plain ids -- this reader only needs to know a trade *venue* was
/// invoked, never what the instruction did.
const KNOWN_TRADING_PROGRAMS: [&str; 7] = [
    // pump.fun bonding curve -- realorrug_decode::pumpfun::PROGRAM_ID.
    "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
    // PumpSwap AMM -- realorrug_decode::pumpswap::PROGRAM_ID.
    "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA",
    // Raydium Liquidity Pool V4 (the original constant-product AMM).
    "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8",
    // Raydium CPMM (`CP-Swap`).
    "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C",
    // Raydium CLMM (concentrated liquidity).
    "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK",
    // Jupiter aggregator v6.
    "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4",
    // Meteora DLMM.
    "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo",
];

/// Whether any instruction in `tx` -- top-level or inner, per
/// [`Transaction::instructions`]'s own doc on why inner CPIs are flattened
/// in -- names a program on [`KNOWN_TRADING_PROGRAMS`].
fn invokes_known_trading_program(tx: &Transaction) -> bool {
    tx.instructions
        .iter()
        .any(|ix| KNOWN_TRADING_PROGRAMS.contains(&ix.program.as_str()))
}

/// The same classification, for a transaction whose lamport balances for the
/// creator could not be read at all (the creator's account key is missing,
/// or the node omitted `preBalances`/`postBalances`). Without a SOL side to
/// compare against, a token-balance fall can only be recorded as an
/// unpriced transfer out, never guessed at as a sale.
fn classify_without_lamports(token_before: u64, token_after: u64) -> SolanaCashFlowEvent {
    if token_after < token_before {
        SolanaCashFlowEvent::TransferOut
    } else {
        SolanaCashFlowEvent::Ignored
    }
}

/// Reads the creator's own on-chain cash flow for a Solana launch: every buy
/// and sell against their associated token account for `mint`, plus a count
/// of unpriced outgoing transfers, on the same terms
/// [`creator_cash_flow`]'s Robinhood reader gives the meaning (design 0027
/// slice 5).
///
/// # Which account is read
///
/// Only the creator's **own** associated token account (`[owner,
/// token_program, mint]` under the ATA program, [`realorrug_pumpfun::pda::associated_token_account`])
/// -- never a second wallet, never the bonding curve's own token account.
/// The token program is read from the mint account's own owner
/// ([`RpcClient::owner_of`], [`realorrug_pumpfun::token::TokenProgram::of`]) rather than assumed to be
/// classic SPL Token, because Token-2022 mints derive a different ATA
/// address entirely.
///
/// # Reading the history
///
/// The ATA's signatures are paged back to the oldest with
/// [`RpcClient::signatures_back_to_oldest`], capped at
/// [`CREATOR_CASH_FLOW_MAX_SIGNATURES`]; every signature is fetched and
/// classified with [`classify_creator_transaction`]. **`trades_complete` is
/// `true` only when every signature back to the ATA's creation was both
/// found and successfully decoded** -- a truncated page walk, a cap hit
/// before the oldest signature, or a single failed transaction fetch all set
/// it `false` with a reason in `gaps`, never a silently partial "complete"
/// result (AGENTS.md rule 8).
///
/// # An ATA that was never created
///
/// A signature history that comes back **empty and not truncated** is
/// treated as proof the ATA was never created, not as an unreadable
/// account: any transaction that ever touched it -- even one that later
/// closed it -- would leave a discoverable signature at that address, so a
/// verified-empty, non-truncated list means there is nothing to find, and
/// zero trades is a real answer rather than an absence dressed as one. An
/// empty list that came back **truncated** (including a shared page budget
/// that had none left before this read started -- see
/// [`RpcClient::signatures_back_to_oldest`]'s own doc on `take_page`
/// returning immediately) proves nothing and is reported incomplete
/// instead.
///
/// # Errors
///
/// A string naming why the read could not even start (the mint's owner, or
/// the ATA derivation itself, could not be resolved). A signature or
/// transaction read failure past that point lands in
/// [`CreatorCashFlow::gaps`] instead, on a result that is still returned.
pub fn creator_cash_flow_solana(
    client: &RpcClient,
    budget: &mut Budget,
    mint: &realorrug_types::Address,
    creator: &realorrug_types::Address,
) -> Result<CreatorCashFlow, String> {
    let owner = client
        .owner_of(budget, mint)
        .map_err(|e| format!("creator cash flow: reading the mint's owner: {e}"))?
        .ok_or_else(|| {
            "creator cash flow: the mint has no account to read an owner from".to_owned()
        })?;
    let owner_address: realorrug_types::Address = owner.parse().map_err(|_| {
        format!("creator cash flow: the mint's owner ({owner}) is not a parseable address")
    })?;
    let token_program =
        realorrug_pumpfun::token::TokenProgram::of(&owner_address).ok_or_else(|| {
            format!(
                "creator cash flow: the mint's owner ({owner}) is neither SPL Token nor Token-2022"
            )
        })?;
    let ata = realorrug_pumpfun::pda::associated_token_account(creator, mint, &token_program.id())
        .ok_or_else(|| {
            "creator cash flow: could not derive the creator's associated token account".to_owned()
        })?;

    let mint_key = mint.to_string();
    let creator_key = creator.to_string();
    let ata_key = ata.to_string();

    // Only the creator's own associated token account is ever read below.
    // Confirm it is the *only* account they hold for this mint, or the read
    // above quietly ignores a second one's whole history -- a real gap in
    // the total, not a cosmetic one (module doc, "Which account is read").
    let multi_account_gap = match client.token_accounts_by_owner_for_mint(budget, creator, mint) {
        Ok(accounts) if accounts.iter().all(|a| *a == ata_key) => None,
        Ok(accounts) => Some(format!(
            "the creator holds {} token account(s) for this mint besides their associated \
             one; only the associated account's history is read, so this cash flow may be \
             incomplete",
            accounts.iter().filter(|a| **a != ata_key).count()
        )),
        Err(e) => Some(format!(
            "could not confirm the creator's associated token account is their only one for \
             this mint: {e}"
        )),
    };

    let (signatures, truncated) = client
        .signatures_back_to_oldest(budget, &ata)
        .map_err(|e| format!("creator cash flow: {e}"))?;

    if truncated {
        return Ok(with_multi_account_gap(
            CreatorCashFlow {
                trades: Vec::new(),
                transfers_out: 0,
                trades_complete: false,
                gaps: vec![
                    "the creator's own token account has more signature history than the page \
                     budget allows (or none was left for this read); its cash flow could not be \
                     read to the account's creation"
                        .to_owned(),
                ],
                quote_asset: crate::dossier::QuoteAsset::sol(),
            },
            multi_account_gap,
        ));
    }

    if signatures.len() > CREATOR_CASH_FLOW_MAX_SIGNATURES {
        return Ok(with_multi_account_gap(
            CreatorCashFlow {
                trades: Vec::new(),
                transfers_out: 0,
                trades_complete: false,
                gaps: vec![format!(
                    "the creator's own token account has more than \
                     {CREATOR_CASH_FLOW_MAX_SIGNATURES} signatures; reading its whole cash flow \
                     history was skipped rather than done partway"
                )],
                quote_asset: crate::dossier::QuoteAsset::sol(),
            },
            multi_account_gap,
        ));
    }

    // A transaction fetch is one call each: if there is not enough budget
    // left to fetch every signature, reading a random subset of them would
    // give the same "complete" shape as a full read while quietly leaving
    // out whichever half ran out last -- report the gap up front instead of
    // reading partway.
    if u32::try_from(signatures.len()).unwrap_or(u32::MAX) > budget.calls_left() {
        return Ok(with_multi_account_gap(
            CreatorCashFlow {
                trades: Vec::new(),
                transfers_out: 0,
                trades_complete: false,
                gaps: vec![format!(
                    "reading {} transactions would exceed the {} calls left in this dossier's \
                     budget; the creator's cash flow was not read rather than read partway",
                    signatures.len(),
                    budget.calls_left()
                )],
                quote_asset: crate::dossier::QuoteAsset::sol(),
            },
            multi_account_gap,
        ));
    }

    // Oldest first: the dev buy in the launch transaction itself is the
    // ATA's own oldest signature, so it is read (and appears as the first
    // trade) exactly like every trade after it.
    let (trades, transfers_out, gaps, complete) =
        read_creator_trades(client, budget, &signatures, &creator_key, &mint_key);

    Ok(with_multi_account_gap(
        CreatorCashFlow {
            trades,
            transfers_out,
            trades_complete: complete,
            gaps,
            quote_asset: crate::dossier::QuoteAsset::sol(),
        },
        multi_account_gap,
    ))
}

/// Folds a multi-account gap (module doc, "Which account is read") into an
/// otherwise-finished [`CreatorCashFlow`], forcing `trades_complete` false
/// when there was one. Split out so every return path in
/// [`creator_cash_flow_solana`] applies it the same way.
fn with_multi_account_gap(mut flow: CreatorCashFlow, gap: Option<String>) -> CreatorCashFlow {
    if let Some(gap) = gap {
        flow.trades_complete = false;
        flow.gaps.push(gap);
    }
    flow
}

/// Fetches and classifies every signature in `signatures` (newest first, as
/// [`RpcClient::signatures_back_to_oldest`] returns them), oldest first.
/// Split out of [`creator_cash_flow_solana`] purely to keep that function's
/// own length within this crate's clippy limit -- it is not independently
/// useful, hence not `pub`.
///
/// Returns `(trades, transfers_out, gaps, complete)`. `complete` is `false`
/// as soon as any transaction in the list could not be fetched or read; a
/// failed signature (`err` set) is skipped without affecting it, on the same
/// terms [`investigate_solana`] already applies to failed signatures.
fn read_creator_trades(
    client: &RpcClient,
    budget: &mut Budget,
    signatures: &[crate::rpc::SignatureInfo],
    creator_key: &str,
    mint_key: &str,
) -> (Vec<CreatorTrade>, u32, Vec<String>, bool) {
    let mut trades = Vec::new();
    let mut transfers_out = 0u32;
    let mut gaps = Vec::new();
    let mut complete = true;
    for sig in signatures.iter().rev() {
        if sig.err.is_some() {
            continue;
        }
        match client.transaction(budget, &sig.signature) {
            Ok(Some(tx)) => {
                if tx.failed {
                    continue;
                }
                if !has_readable_creator_balances(&tx, creator_key, mint_key) {
                    // The node answered but left `meta` off (or, on this
                    // shape, an empty balance side that never even names
                    // this mint): there is no lamport or token move to read
                    // for the creator at all, which is a different fact
                    // from "nothing happened" -- rule 9 forbids reading it
                    // as a quiet no-op.
                    complete = false;
                    gaps.push(format!(
                        "transaction {} did not report balances for the creator or the mint",
                        sig.signature
                    ));
                    continue;
                }
                match classify_creator_transaction(&tx, creator_key, mint_key) {
                    SolanaCashFlowEvent::Buy { quote, tokens } => trades.push(CreatorTrade {
                        role: CreatorRole::Deployer,
                        side: Side::Buy,
                        quote,
                        tokens,
                        block: tx.slot.0,
                        transaction: sig.signature.clone(),
                        unique_id: sig.signature.clone(),
                    }),
                    SolanaCashFlowEvent::Sell { quote, tokens } => trades.push(CreatorTrade {
                        role: CreatorRole::Deployer,
                        side: Side::Sell,
                        quote,
                        tokens,
                        block: tx.slot.0,
                        transaction: sig.signature.clone(),
                        unique_id: sig.signature.clone(),
                    }),
                    SolanaCashFlowEvent::TransferOut => transfers_out += 1,
                    SolanaCashFlowEvent::Ignored => {}
                }
            }
            Ok(None) => {
                complete = false;
                gaps.push(format!(
                    "transaction {} could not be fetched",
                    sig.signature
                ));
            }
            Err(crate::rpc::RpcError::Stopped(_)) => {
                // The shared budget or deadline ran out mid-walk. Every
                // signature still left would fail the exact same way, so
                // one gap naming the stop is the honest report -- a gap per
                // remaining signature would just be the same fact repeated
                // under a growing number.
                complete = false;
                gaps.push(
                    "the call budget ran out before every transaction in the creator's ATA \
                     history could be read; the remaining ones were not fetched"
                        .to_owned(),
                );
                break;
            }
            Err(why) => {
                complete = false;
                gaps.push(format!("transaction {}: {why}", sig.signature));
            }
        }
    }
    (trades, transfers_out, gaps, complete)
}

/// Whether `tx` reported enough of the creator's own balances to classify at
/// all: their lamport balance at both `pre`/`post`, and at least one
/// token-balance entry (either side) naming `mint_key`.
///
/// A node that omitted `meta` entirely -- or returned it with every array
/// empty -- passes [`crate::rpc::parse_transaction`] as `Some` with every
/// balance field defaulted to empty (`crate::rpc::lamport_balances` and
/// `crate::rpc::token_balances` both default a missing field to `Vec::new()`
/// rather than failing the parse), so a transaction like that must never
/// reach [`classify_creator_transaction`]: every one of its "no data"
/// branches there already reads as "nothing happened" rather than "unread",
/// and this is the one place left that can still tell the two apart.
fn has_readable_creator_balances(tx: &Transaction, creator_key: &str, mint_key: &str) -> bool {
    let has_lamports = tx
        .accounts
        .iter()
        .position(|a| a == creator_key)
        .is_some_and(|i| tx.pre_balances.get(i).is_some() && tx.post_balances.get(i).is_some());
    let mint_mentioned = tx.pre_token_balances.iter().any(|b| b.mint == mint_key)
        || tx.post_token_balances.iter().any(|b| b.mint == mint_key);
    has_lamports && mint_mentioned
}

/// How many blocks after the launch block count as "the same window" for a
/// wallet-link check -- research 0052 §2.2, §3.2. Robinhood's blocks are
/// about 0.1 s each, so "same block" alone is a much narrower net than it is
/// on Solana; the factory's own `snipeTaxSeconds()` read as 3 (confirmed
/// twice over, research 0047 §7 -- selector
/// [`realorrug_robinhood::pons::curve::SNIPE_TAX_SECONDS`]) gives the
/// natural width for "still inside the snipe-tax window": 3 s / 0.1 s per
/// block is 30 blocks, ten times a single block.
pub const LINK_WINDOW_BLOCKS: u64 = 30;

/// Whether two buy sizes are within 10% of each other, expressed as the
/// smaller's share of the larger in basis points.
///
/// Exactly 10% apart (9,000 bps) counts as "within": research 0052 §3.2's
/// bands are stated as a closed threshold ("within 10%"), so the boundary
/// itself belongs to the narrower band, not the wider one. Both directions
/// of that boundary have a named test so a `<`/`<=` swap here fails a test,
/// not just a mutation run.
#[must_use]
pub fn sizes_within_ten_percent(a: u128, b: u128) -> bool {
    let (small, large) = (a.min(b), a.max(b));
    // `small >= large - floor(large / 10)` is exactly `small >= 0.9 * large`
    // for whole numbers, with no multiply: a saturating multiply would call
    // two near-`u128::MAX` amounts "within 10%" whatever they were. Two
    // empty buys pass (0 >= 0): calling them "different sizes" would be a
    // claim this reader cannot support.
    small >= large - large / 10
}

/// What is known about two wallets, feeding [`link_confidence`].
///
/// A struct of observations, not a raw score: nothing outside this module
/// can hand `link_confidence` a number and skip the table, because there is
/// no bps field to set. Every field names one thing that was actually
/// checked; a caller with no evidence for a field leaves it `false`, which
/// [`link_confidence`] reads as "not observed", never as "observed absent"
/// (AGENTS.md §3 rule 8 -- this struct only ever grows evidence, it does not
/// carry a way to assert a negative).
// Seven independent observations, not app state to collapse into an enum:
// research 0052 §3.2's table names each one separately and several can be
// true of the same pair of wallets at once (a transfer *and* a same-block
// buy), which a two-variant enum per pair could not represent.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkEvidence {
    /// An ERC-20 `Transfer` moved tokens directly between the two wallets
    /// (either direction), or from the deployer to both; or the two
    /// addresses were declared together in `launchToken`'s exemption
    /// calldata (research 0048 §3). The *open* case: research 0052 §3.2
    /// gives this the top weight precisely because declaring a link is
    /// never scored worse than hiding it.
    pub transfer_or_declared_together: bool,
    /// Both wallets' first purchase landed in the same block.
    pub same_block: bool,
    /// Both wallets were fresh: no on-chain history before the launch block
    /// (`Candidate::nonce_before_launch == Some(0)` for both, or the
    /// equivalent chain-specific check).
    pub both_fresh: bool,
    /// The two wallets' buy sizes are within 10% of each other --
    /// [`sizes_within_ten_percent`] is the reference comparison a caller
    /// should use to set this.
    pub sizes_within_10_percent: bool,
    /// Both wallets acted within [`LINK_WINDOW_BLOCKS`] of the launch block
    /// (a superset of `same_block`; a caller sets this whenever `same_block`
    /// is true too, since same-block evidence is also same-window evidence).
    pub same_window: bool,
    /// Both wallets sold within 50 blocks of each other. Confirms a link
    /// found some other way; research 0052 §3.2 marks it "never the only
    /// link", so [`link_confidence`] only lets it raise a confidence that is
    /// already nonzero from another field.
    pub correlated_sell: bool,
    /// A native-ETH transfer funded one wallet from the other within one
    /// hop. **Not readable today**: nothing in this crate walks a funding
    /// graph more than the direct pre-purchase transfer `wallets.rs`
    /// already reads for [`Funding::checked`] (research 0052 §3.2, §7.2 --
    /// the read this would need is unbuilt, not merely unbudgeted). The
    /// field exists so the evidence type matches the table exactly and so
    /// the day this read is built, no caller needs a new enum variant; until
    /// then nothing in this crate ever sets it `true`.
    pub eth_funding_one_hop: bool,
}

/// Confidence that two wallets are one actor, in bps -- the **strongest**
/// single piece of evidence present, never a sum (research 0052 §3.2: "never
/// added across kinds", design 0027 §"Judgement" -- correlated evidence must
/// not multiply). Reproduces the §3.2 table exactly:
///
/// | evidence | c (bps) |
/// |---|---|
/// | transfer between them, or declared together in calldata | 9,000 |
/// | same launch block + both fresh + sizes within 10% | 7,000 |
/// | same launch block + both fresh | 5,000 |
/// | same [`LINK_WINDOW_BLOCKS`]-block window + sizes within 10% | 4,000 |
/// | same [`LINK_WINDOW_BLOCKS`]-block window only | 2,000 |
/// | ETH funding, one hop (not readable today) | 8,000 |
/// | sold within 50 blocks of each other | 3,000, confirms only |
/// | none of the above | 0 |
///
/// The correlated-sell row never establishes a link by itself: it only
/// raises a confidence some other field already made nonzero, matching
/// "never the only link".
#[must_use]
pub fn link_confidence(evidence: LinkEvidence) -> u16 {
    let mut c: u16 = 0;
    if evidence.same_window {
        c = c.max(2_000);
    }
    if evidence.same_window && evidence.sizes_within_10_percent {
        c = c.max(4_000);
    }
    if evidence.same_block && evidence.both_fresh {
        c = c.max(5_000);
    }
    if evidence.same_block && evidence.both_fresh && evidence.sizes_within_10_percent {
        c = c.max(7_000);
    }
    if evidence.eth_funding_one_hop {
        c = c.max(8_000);
    }
    if evidence.transfer_or_declared_together {
        c = c.max(9_000);
    }
    if evidence.correlated_sell && c > 0 {
        c = c.max(3_000);
    }
    c
}

// --- S7 "correlated selling" (research 0052 §3, M-D-0008) ---------------

/// One `CurveSell` in the launch window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sale {
    /// Who sold: a `CurveSell`'s `seller` topic, `Trade::trader` after
    /// decoding (the position that shrinks, not `recipient`).
    pub seller: Address,
    /// Tokens sold.
    pub tokens: u128,
    /// Quote received, in wei.
    pub quote: u128,
    /// The block it landed in.
    pub block: u64,
    /// Position within the block, for ordering; `(0, 0)` when the provider
    /// omitted it.
    pub position: (u64, u64),
    /// The transaction that carried it.
    pub transaction: Hash32,
}

/// Every sale among `logs` made against `curve`. Mirrors [`purchases_from`]
/// for the other side of `Trade`.
#[must_use]
pub fn sells_from(logs: &[Log], curve: &Address) -> Vec<Sale> {
    logs.iter()
        .filter_map(|log| Trade::from_log(log).map(|t| (log, t)))
        .filter(|(_, t)| t.side == Side::Sell && t.curve == *curve)
        .map(|(log, t)| Sale {
            seller: t.trader,
            tokens: t.tokens,
            quote: t.quote,
            block: log.block,
            position: log
                .position
                .map_or((0, 0), |p| (p.transaction_index, p.log_index)),
            transaction: log.transaction,
        })
        .collect()
}

/// How confident two selling wallets must look, by how they *bought*, before
/// their sells count toward the same S7 cluster (research 0052 §3, §3.2).
///
/// The "same [`LINK_WINDOW_BLOCKS`]-block window + sizes within 10%" tier
/// (4,000 bps): the tier above it, "both fresh", needs an
/// `eth_getTransactionCount` per wallet that S7's own cost row (research
/// 0052 §7.1: "60 CU per window", one `eth_getLogs`) does not budget for.
/// This deliberately never looks at `LinkEvidence::correlated_sell` (the
/// sells' own timing) to decide linkage here: that field's own doc says it
/// is "never the only link", and using the sells to link the sells would be
/// exactly that. So [`largest_sell_cluster`] only ever reads how a wallet
/// bought to decide whether it belongs in a selling cluster, never how it
/// sold.
pub const SELL_CLUSTER_LINK_THRESHOLD_BPS: u16 = 4_000;

/// The window S7 looks for a cluster of linked sellers in -- research 0052
/// §3.1's S7 row and §7.1's cost row both say 50 blocks.
pub const SELL_CLUSTER_WINDOW_BLOCKS: u64 = 50;

/// How many linked wallets selling inside one window fires S7's first
/// factor -- research 0052 §3.1.
pub const SELL_CLUSTER_WALLET_THRESHOLD: u32 = 3;

/// How much of supply the cluster must have sold to fire S7's second
/// factor, in basis points -- research 0052 §3.1.
pub const SELL_CLUSTER_VOLUME_BPS_THRESHOLD: u16 = 1_000;

/// How many seconds the cluster's sells can spread over before S7's third
/// factor lowers the weight instead of raising it -- research 0052 §3.1.
pub const SELL_CLUSTER_SPREAD_SECONDS_THRESHOLD: u64 = 3_600;

/// The largest cluster [`largest_sell_cluster`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SellCluster {
    members: Vec<Address>,
    tokens_sold: u128,
    first_block: u64,
    last_block: u64,
}

/// The largest cluster of linked sellers inside one
/// [`SELL_CLUSTER_WINDOW_BLOCKS`]-block window, or `None` when no seller has
/// a buy on record.
///
/// Built from individual sales, never from a wallet's lifetime: every sale
/// whose seller bought inside the launch window is tried as the start (the
/// anchor) of a window running `anchor.block ..= anchor.block + 50`, and the
/// cluster is the sales in that window whose seller is the anchor's own
/// wallet or links to it by how it bought. `tokens_sold`, `first_block` and
/// `last_block` describe those sales only. A wallet-level summary would let
/// linked wallets hide a same-block dump behind one tiny earlier sell each,
/// and would credit the cluster with sales made days outside its window.
///
/// A seller with no matching [`Buyer`] record (a transfer-in, or a buy
/// before the window this dossier read) has no evidence to link it to
/// anything, so it never joins another seller's cluster and never anchors
/// one: absent buy-side evidence is not evidence of no link (rule 8), but it
/// is not evidence of a link either.
///
/// Deterministic: sales are ordered by block then seller, and a later
/// cluster replaces the kept one only when it has strictly more distinct
/// sellers, so the same logs always give the same cluster.
fn largest_sell_cluster(sells: &[Sale], buyers: &[Buyer]) -> Option<SellCluster> {
    let by_buyer: BTreeMap<[u8; 20], &Buyer> = buyers.iter().map(|b| (b.address.0, b)).collect();
    let mut ordered: Vec<&Sale> = sells.iter().collect();
    ordered.sort_by_key(|sale| (sale.block, sale.seller.0));

    let mut best: Option<SellCluster> = None;
    for anchor in &ordered {
        let Some(anchor_buy) = by_buyer.get(&anchor.seller.0) else {
            continue;
        };
        let mut members: Vec<Address> = Vec::new();
        let mut tokens = 0u128;
        let mut last_block = anchor.block;
        for sale in &ordered {
            // The window's far edge is inside it: "within 50 blocks" is a
            // closed bound, the same reading `sizes_within_ten_percent`
            // gives "within 10%". A named test pins both sides.
            match sale.block.checked_sub(anchor.block) {
                Some(0..=SELL_CLUSTER_WINDOW_BLOCKS) => {}
                _ => continue,
            }
            let joins = sale.seller == anchor.seller
                || by_buyer
                    .get(&sale.seller.0)
                    .is_some_and(|buy| buys_link(anchor_buy, buy));
            if !joins {
                continue;
            }
            if !members.contains(&sale.seller) {
                members.push(sale.seller);
            }
            tokens = tokens.saturating_add(sale.tokens);
            last_block = last_block.max(sale.block);
        }
        // `>` rather than `>=`: the first cluster of a given size found is
        // the one kept, so re-running this over the same logs cannot pick a
        // different same-size cluster.
        let better = best
            .as_ref()
            .is_none_or(|b| members.len() > b.members.len());
        if better {
            best = Some(SellCluster {
                members,
                tokens_sold: tokens,
                first_block: anchor.block,
                last_block,
            });
        }
    }
    best
}

/// Whether two buyers look linked by how they bought, at
/// [`SELL_CLUSTER_LINK_THRESHOLD_BPS`] or more.
///
/// Only the window and size evidence is set. Same-block buying adds weight
/// in [`link_confidence`] only alongside "both fresh", which S7 does not
/// read, so setting it here would change nothing and hide that.
fn buys_link(a: &Buyer, b: &Buyer) -> bool {
    let evidence = LinkEvidence {
        same_window: a.first_block.abs_diff(b.first_block) <= LINK_WINDOW_BLOCKS,
        sizes_within_10_percent: sizes_within_ten_percent(a.quote, b.quote),
        ..LinkEvidence::default()
    };
    link_confidence(evidence) >= SELL_CLUSTER_LINK_THRESHOLD_BPS
}

/// S7's result: the largest linked-seller cluster, its share of supply, and
/// how long its sells spread over.
///
/// `sells_read` is the gate rule 8 needs: when it is `false`, the other
/// fields were never measured and must not be published as "no correlated
/// selling" -- only as unknown. When it is `true`, `linked_sellers == 0` is
/// a real, measured zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorrelatedSelling {
    /// Wallets in the largest cluster [`largest_sell_cluster`] found.
    pub linked_sellers: u32,
    /// That cluster's tokens sold as a share of supply, in basis points.
    /// `None` when supply could not be read, even when `sells_read` is
    /// `true`.
    pub sold_bps_of_supply: Option<u16>,
    /// Seconds between the cluster's earliest and latest sell, by the
    /// chain's own block timestamps. `Some(0)` when every sell in the
    /// cluster shares one block; `None` when a timestamp read failed or
    /// could not be afforded.
    pub spread_seconds: Option<u64>,
    /// Whether the `CurveSell` logs behind every field above were actually
    /// read.
    pub sells_read: bool,
}

/// Reads S7 "correlated selling" for one pre-graduation launch window.
///
/// One `eth_getLogs` read over the whole window, the same cost and shape as
/// [`creator_cash_flow`]'s: `record.curve`'s own logs, unfiltered by topic,
/// decode into both buys (for link evidence, via [`buyers_of`]) and sells
/// (via [`sells_from`]), so buys and sells come from one read. It repeats
/// the read [`creator_cash_flow`] makes (same curve, same range); sharing
/// that one read is the obvious saving, left for when the budget needs it. Naturally pre-graduation only: a
/// graduated curve prices nothing and stops emitting `CurveSell` at all
/// (research 0044), so a window that runs past graduation still only picks
/// up the sells that happened before it, with no special case needed here.
///
/// Never fails outright: a read that cannot be afforded or that the
/// provider rejects comes back with `sells_read: false`, on the same terms
/// as [`creator_cash_flow`].
#[must_use]
pub fn correlated_selling(
    client: &Rpc,
    budget: &mut Budget,
    record: &LaunchedToken,
    launch_block: u64,
    read_block: u64,
    supply: Option<u128>,
) -> CorrelatedSelling {
    let none_read = CorrelatedSelling {
        linked_sellers: 0,
        sold_bps_of_supply: None,
        spread_seconds: None,
        sells_read: false,
    };
    let Ok(logs) = take(budget, CU_GET_LOGS).and_then(|()| {
        client
            .logs_range(&record.curve, &[], launch_block, read_block)
            .map_err(|e| match e {
                LogsError::TooManyResults => {
                    "the token's lifetime curve activity held more trades than one read returns"
                        .to_owned()
                }
                LogsError::Other(why) => why,
            })
    }) else {
        return none_read;
    };

    let buyers = buyers_of(&purchases_from(&logs, &record.curve));
    let sells = sells_from(&logs, &record.curve);
    let Some(cluster) = largest_sell_cluster(&sells, &buyers) else {
        return CorrelatedSelling {
            sells_read: true,
            ..none_read
        };
    };

    // `checked_div` is the zero-supply guard: no share of nothing.
    let sold_bps_of_supply = supply.and_then(|supply| {
        cluster
            .tokens_sold
            .saturating_mul(10_000)
            .checked_div(supply)
            .map(|bps| u16::try_from(bps).unwrap_or(u16::MAX))
    });

    // A cluster whose sells share one block spread over no time at all,
    // measured without a timestamp read.
    let spread_seconds = if cluster.first_block == cluster.last_block {
        Some(0)
    } else {
        // `eth_getBlockByNumber` costs a call but no named CU here, the same
        // terms `robinhood.rs`'s own `block_time` wrapper reads it on.
        let mut read_time = |number: u64| -> Option<u64> {
            budget
                .take_call()
                .ok()
                .and_then(|()| client.block_time(Some(number)).ok())
                .map(|(_, timestamp)| timestamp)
        };
        match (
            read_time(cluster.first_block),
            read_time(cluster.last_block),
        ) {
            (Some(first), Some(last)) => Some(last.saturating_sub(first)),
            _ => None,
        }
    };

    CorrelatedSelling {
        linked_sellers: u32::try_from(cluster.members.len()).unwrap_or(u32::MAX),
        sold_bps_of_supply,
        spread_seconds,
        sells_read: true,
    }
}

// --- S6 "linked-wallet holdings" (research 0052 §2.3, §3.1 S6, §3.2) ------

/// The sum of what a set of wallets holds, each wallet counted once even
/// when it links to several others, scaled by how confident the link is.
///
/// `holdings` is `(wallet, share_bps, link_confidence_bps)`: `share_bps` is
/// that wallet's own share of supply, `link_confidence_bps` the strongest
/// [`link_confidence`] tying it to the wallet under investigation (research
/// 0052 §3.2's own rule: confidence is taken from the strongest evidence,
/// never summed across kinds -- a caller with more than one piece of
/// evidence for the same wallet must already have reduced it to one
/// `link_confidence_bps` via `link_confidence` before calling this).
///
/// Per wallet the effective holding is `share_bps * link_confidence_bps /
/// 10_000` (research 0052 §3.2's own formula; whole-number division, so
/// 15% at 7,000 bps confidence is 1,050 bps, not 1,500). The same wallet
/// address can appear more than once in `holdings` -- e.g. read once from
/// the buyer set and again from a holder-balance read -- and must be
/// counted once, not twice: this function keeps the *strongest* effective
/// value seen per address (never their sum, for the same reason confidence
/// itself is not summed) and sums across distinct addresses.
///
/// No caller reads this yet; S2's raise ("same-window buyers hold >= 1,000
/// bps together", research 0052 §3.1 row S2) and S1's linked-wallet share
/// are the intended callers, wired in a follow-up slice that also carries
/// the `Powers.exemptions`/`Funding.checked` reads this needs.
///
/// Integer math only: every multiply is `u32` (bps values fit in `u16`, so
/// their product fits comfortably), every sum saturates so a pathological
/// input clamps at `u16::MAX` instead of wrapping.
#[must_use]
pub fn linked_holdings_bps(holdings: &[(Address, u16, u16)]) -> u16 {
    let mut strongest: BTreeMap<[u8; 20], u16> = BTreeMap::new();
    for &(wallet, share_bps, confidence_bps) in holdings {
        let effective = u32::from(share_bps) * u32::from(confidence_bps) / 10_000;
        let effective = u16::try_from(effective).unwrap_or(u16::MAX);
        strongest
            .entry(wallet.0)
            .and_modify(|current| *current = (*current).max(effective))
            .or_insert(effective);
    }
    strongest
        .values()
        .fold(0u16, |sum, &v| sum.saturating_add(v))
}

#[cfg(test)]
mod linked_holdings_tests {
    use super::*;

    fn addr(b: u8) -> Address {
        Address([b; 20])
    }

    #[test]
    fn confidence_zero_contributes_nothing() {
        assert_eq!(linked_holdings_bps(&[(addr(1), 5_000, 0)]), 0);
    }

    #[test]
    fn confidence_full_contributes_all_of_the_share() {
        assert_eq!(linked_holdings_bps(&[(addr(1), 5_000, 10_000)]), 5_000);
    }

    #[test]
    fn partial_confidence_scales_the_share_down() {
        // 15% at 7,000 bps confidence: 1,500 * 7,000 / 10,000 = 1,050.
        assert_eq!(linked_holdings_bps(&[(addr(1), 1_500, 7_000)]), 1_050);
    }

    #[test]
    fn a_wallet_linked_twice_is_counted_once_at_its_strongest_reading() {
        // Same wallet appears from two evidence paths; must not sum.
        let holdings = &[(addr(1), 1_000, 2_000), (addr(1), 1_000, 9_000)];
        // Weak reading alone would be 200; strong alone 900; summed 1,100.
        // The rule is "strongest, not sum": 900.
        assert_eq!(linked_holdings_bps(holdings), 900);
    }

    #[test]
    fn distinct_wallets_sum() {
        let holdings = &[(addr(1), 1_000, 10_000), (addr(2), 500, 10_000)];
        assert_eq!(linked_holdings_bps(holdings), 1_500);
    }

    #[test]
    fn empty_holdings_is_zero() {
        assert_eq!(linked_holdings_bps(&[]), 0);
    }
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
            tokens: 0,
            first_block: block,
            first_position: (0, 0),
        }
    }

    fn candidate(b: u8, quote: u128, funders: &[(u8, u128)]) -> Candidate {
        Candidate {
            address: addr(b).to_string(),
            bought_wei: quote,
            bought_tokens: Some(0),
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
        let log = |trader: u8, recipient: u8, quote: u128, tokens: u128, block: u64| {
            let mut data = Vec::new();
            for v in [quote, tokens, 0, 0] {
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
        let mut sell = log(1, 1, 99, 1_000, 1);
        sell.topics[0] = topic::CURVE_SELL;
        let mut other_curve = log(1, 1, 99, 1_000, 1);
        other_curve.address = addr(0xdd);
        let logs = vec![
            log(0x22, 0x01, 5, 300, 3), // a router buys for 01
            log(0x01, 0x01, 7, 400, 2), // 01 buys directly, earlier
            log(0x22, 0x02, 9, 900, 3), // the same router buys for 02
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
        assert_eq!(
            purchases[0].tokens, 300,
            "the decoded token amount must survive purchases_from"
        );
        let buyers = buyers_of(&purchases);
        assert_eq!(buyers.len(), 2, "the router is not a buyer");
        assert_eq!(buyers[0].address, addr(0x01));
        assert_eq!(buyers[0].quote, 12);
        assert_eq!(
            buyers[0].tokens, 700,
            "two buys by one wallet (300 + 400) must sum, not just the last one"
        );
        assert_eq!(buyers[0].first_block, 2);
        assert_eq!(buyers[1].address, addr(0x02));
        assert_eq!(buyers[1].tokens, 900);
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
            r#"{{"result":{{"slot":1,"meta":{{"err":null,"preBalances":[1000000,0],"postBalances":[{pre_left},{amount}],"preTokenBalances":[],"postTokenBalances":[]}},"transaction":{{"message":{{"accountKeys":["{from}","{to}"],"instructions":[{{"programId":"11111111111111111111111111111111","accounts":[0,1]}}]}}}}}},"error":null}}"#,
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

    /// The creator's own associated token account for `mint`, on the same
    /// SPL-token derivation `creator_cash_flow_solana` uses when the mint's
    /// owner is the classic token program (every test fixture's `ata_owner`).
    fn spl_ata(creator: realorrug_types::Address, mint: realorrug_types::Address) -> String {
        realorrug_pumpfun::pda::associated_token_account(
            &creator,
            &mint,
            &realorrug_pumpfun::token::TokenProgram::Spl.id(),
        )
        .expect("derivable ata")
        .to_string()
    }

    /// A `getTokenAccountsByOwner` answer naming exactly the accounts given
    /// (a bare pubkey list is all [`RpcClient::token_accounts_by_owner_for_mint`]
    /// reads).
    fn token_accounts_response(accounts: &[&str]) -> String {
        let entries: Vec<String> = accounts
            .iter()
            .map(|a| format!(r#"{{"pubkey":"{a}"}}"#))
            .collect();
        format!(
            r#"{{"result":{{"value":[{}]}},"error":null}}"#,
            entries.join(",")
        )
    }

    /// A single-page `getSignaturesForAddress` answer at a chosen `slot`,
    /// for tests that need control over the slot a signature landed at.
    fn signatures_page_at(signature: &str, slot: u64) -> String {
        format!(r#"{{"result":[{{"signature":"{signature}","slot":{slot}}}],"error":null}}"#)
    }

    /// A `getTransaction` answer whose only lamport move is `from` funding
    /// `to` by `amount`, landing at a chosen `slot`, carrying the one
    /// instruction a genuine System Program transfer has. This is the
    /// positive case [`is_plain_sol_transfer`] must accept on its own
    /// merits -- an empty instruction list here would let every funding
    /// test pass through the gate's unreadable-is-vacuously-true bug
    /// instead of proving it accepts a real transfer's shape.
    fn funding_tx_at(from: &str, to: &str, amount: u64, slot: u64) -> String {
        format!(
            r#"{{"result":{{"slot":{slot},"meta":{{"err":null,"preBalances":[1000000,0],"postBalances":[{pre_left},{amount}],"preTokenBalances":[],"postTokenBalances":[]}},"transaction":{{"message":{{"accountKeys":["{from}","{to}"],"instructions":[{{"programId":"11111111111111111111111111111111","accounts":[0,1]}}]}}}}}},"error":null}}"#,
            pre_left = 1_000_000u64.saturating_sub(amount),
        )
    }

    /// The same balance shape [`funding_tx_at`] produces -- a material
    /// lamport move from `from` to `to` -- but with no instructions at all,
    /// the shape a versioned transaction whose address-lookup-table
    /// accounts were not merged in can produce. This is not a plain
    /// transfer's absence of a non-System instruction; it is this process
    /// never having read the instructions, so it must not be scored as a
    /// measured "no funder here".
    fn unreadable_instructions_tx_at(from: &str, to: &str, amount: u64, slot: u64) -> String {
        format!(
            r#"{{"result":{{"slot":{slot},"meta":{{"err":null,"preBalances":[1000000,0],"postBalances":[{pre_left},{amount}],"preTokenBalances":[],"postTokenBalances":[]}},"transaction":{{"message":{{"accountKeys":["{from}","{to}"],"instructions":[]}}}}}},"error":null}}"#,
            pre_left = 1_000_000u64.saturating_sub(amount),
        )
    }

    /// The same balance shape [`funding_tx_at`] produces -- `from` losing
    /// lamports, `to` gaining them -- but carrying one instruction naming a
    /// program that is not the System Program. A swap, a sell or anything
    /// else that moves lamports as a side effect is shaped like this, not
    /// like an empty instruction list; a real transfer is never shaped like
    /// this.
    fn non_transfer_tx_at(from: &str, to: &str, amount: u64, slot: u64, program: &str) -> String {
        format!(
            r#"{{"result":{{"slot":{slot},"meta":{{"err":null,"preBalances":[1000000,0],"postBalances":[{pre_left},{amount}],"preTokenBalances":[],"postTokenBalances":[]}},"transaction":{{"message":{{"accountKeys":["{from}","{to}"],"instructions":[{{"programId":"{program}","accounts":[0,1]}}]}}}}}},"error":null}}"#,
            pre_left = 1_000_000u64.saturating_sub(amount),
        )
    }

    /// The same balance shape [`funding_tx_at`] produces, but carrying
    /// several instructions naming the given `programs` in order (each
    /// referencing the same two accounts). For Finding 1's fixtures: a real
    /// wallet's priority-fee instruction ahead of the transfer, or an
    /// all-`ComputeBudget` transaction that moves no lamports at all.
    fn multi_instruction_tx_at(
        from: &str,
        to: &str,
        amount: u64,
        slot: u64,
        programs: &[&str],
    ) -> String {
        let instructions: Vec<String> = programs
            .iter()
            .map(|program| format!(r#"{{"programId":"{program}","accounts":[0,1]}}"#))
            .collect();
        format!(
            r#"{{"result":{{"slot":{slot},"meta":{{"err":null,"preBalances":[1000000,0],"postBalances":[{pre_left},{amount}],"preTokenBalances":[],"postTokenBalances":[]}},"transaction":{{"message":{{"accountKeys":["{from}","{to}"],"instructions":[{}]}}}}}},"error":null}}"#,
            instructions.join(","),
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

    /// A full, 1,000-signature page at a chosen `slot` -- for tests that need
    /// a page that is *not* the last one (forcing `funding_search` to fetch
    /// another) while also controlling whether its signatures are eligible
    /// (at or before a candidate's first purchase).
    fn full_signatures_page_at(prefix: &str, slot: u64) -> String {
        let entries: Vec<String> = (0..1000)
            .map(|i| format!(r#"{{"signature":"{prefix}-{i}","slot":{slot}}}"#))
            .collect();
        format!(r#"{{"result":[{}],"error":null}}"#, entries.join(","))
    }

    /// A short `getSignaturesForAddress` page naming several signatures at
    /// chosen slots, newest first -- for tests that need more than one
    /// signature in a single (final) page.
    fn signatures_page_many(entries: &[(&str, u64)]) -> String {
        let items: Vec<String> = entries
            .iter()
            .map(|(sig, slot)| format!(r#"{{"signature":"{sig}","slot":{slot}}}"#))
            .collect();
        format!(r#"{{"result":[{}],"error":null}}"#, items.join(","))
    }

    /// A `getTransaction` answer for a signature naming `to`, at a chosen
    /// `slot`, whose node omitted `meta.preBalances`/`meta.postBalances`
    /// entirely -- the shape [`funder_of`] cannot trust an index into
    /// ([`FunderRead::Unreadable`]), distinct from a transaction that was
    /// read and simply had no inbound move.
    fn unreadable_balances_tx_at(to: &str, slot: u64) -> String {
        format!(
            r#"{{"result":{{"slot":{slot},"meta":{{"err":null,"preTokenBalances":[],"postTokenBalances":[]}},"transaction":{{"message":{{"accountKeys":["{to}"],"instructions":[]}}}}}},"error":null}}"#
        )
    }

    /// A `getTransaction` answer shaped like a partial address-lookup-table
    /// read: `accountKeys` names only `to`, but `preBalances`/`postBalances`
    /// are sized for two accounts, as a node that resolved the balances
    /// against the full loaded-address set but did not report
    /// `loadedAddresses` back would produce. Finding 4/6: `funder_of`'s
    /// length check must reject this as [`FunderRead::Unreadable`] rather
    /// than reading `post_balances[0]` and trusting an index into an account
    /// list it does not actually describe.
    fn short_accounts_tx_at(to: &str, slot: u64) -> String {
        format!(
            r#"{{"result":{{"slot":{slot},"meta":{{"err":null,"preBalances":[1000000,0],"postBalances":[500000,500000],"preTokenBalances":[],"postTokenBalances":[]}},"transaction":{{"message":{{"accountKeys":["{to}"],"instructions":[{{"programId":"11111111111111111111111111111111","accounts":[0,1]}}]}}}}}},"error":null}}"#
        )
    }

    /// A malformed `getTransaction` answer -- not valid JSON at all -- for
    /// tests exercising a transaction-read failure on one candidate without
    /// aborting the others.
    fn malformed_response() -> String {
        "not json".to_owned()
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
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

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
    fn a_busy_mints_launch_window_is_reached_via_the_ascending_fallback() {
        // Task 9-23-0011: the newest-first walk above truncates on a busy
        // mint (production evidence, 2026-09-23: 7 of 9 real pump.fun
        // mints). The ascending `getTransactionsForAddress` call is tried
        // once and its buyer is found, instead of reporting the gap this
        // crate used to report unconditionally on any truncated walk.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let responses = [
            full_signatures_page(),
            r#"{"result":{"data":[{"signature":"buy-sig","slot":5,"err":null}],"paginationToken":"x"}}"#
                .to_owned(),
            buy_tx(&mint_key, &[(&buyer, 500)]),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = Budget::new(60, 1, std::time::Duration::from_secs(30));
        let funding = investigate_solana(&client, &mut budget, &mint, None)
            .expect("the fallback reaches the window");

        assert_eq!(funding.buyers, 1, "gaps: {:?}", funding.gaps);
        assert!(
            !funding
                .gaps
                .iter()
                .any(|g| g.contains("could not be reached within the read budget")),
            "gaps: {:?}",
            funding.gaps
        );
    }

    #[test]
    fn a_method_not_found_ascending_call_leaves_the_original_gap() {
        // Helius-only: every other RPC refuses `getTransactionsForAddress`.
        // That must not be mistaken for "no early buyers" -- the original
        // truncated-history gap has to survive unchanged (AGENTS.md rule 8).
        let mint = solana_addr(9);
        let responses = [
            full_signatures_page(),
            r#"{"error":{"code":-32601,"message":"Method not found"}}"#.to_owned(),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = Budget::new(60, 1, std::time::Duration::from_secs(30));
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.buyers, 0);
        assert!(funding.checked.is_empty());
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
    fn an_empty_ascending_answer_leaves_the_original_gap() {
        // A node that answers the oldest-first call with no transactions has
        // not shown the launch window is empty -- the mint plainly has a
        // history, the walk above just paged through it. Taking the empty
        // list as the window would drop the gap and report no early buyers
        // as if that had been read (AGENTS.md rule 8: absent is not zero).
        let mint = solana_addr(9);
        let responses = [
            full_signatures_page(),
            r#"{"result":{"data":[],"paginationToken":null}}"#.to_owned(),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = Budget::new(60, 1, std::time::Duration::from_secs(30));
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.buyers, 0);
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
    fn investigate_solana_does_not_rewalk_the_mints_signatures_when_given_them() {
        // The bug this fixes: `dossier::build` reads the mint's signature
        // history once (step 1) and used to make `investigate_solana` read
        // it again (step 5) against the same shared, already-spent page
        // budget -- on a busy mint, step 1 alone could exhaust it, leaving
        // this read with nothing. Handing the signatures in must mean no
        // second `getSignaturesForAddress` call for the mint: the canned
        // transport below supplies no such answer at all, only the buy
        // transaction, so a re-walk would consume that response in its
        // place and fail to parse it as a signature list.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let signatures = vec![crate::rpc::SignatureInfo {
            signature: "mint-sig".to_owned(),
            slot: 1,
            err: None,
        }];
        let responses = [buy_tx(&mint_key, &[(&buyer, 500)])];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, Some(&(signatures, false)))
            .expect("no re-walk: the buy transaction is the only response supplied");

        assert_eq!(funding.buyers, 1);
    }

    #[test]
    fn a_truncated_candidate_history_records_no_funder() {
        // The overall budget allows exactly one page, and the mint's own read
        // spends it; the candidates' page floor then lets the candidate walk,
        // but its history is three full pages of signatures landed after its
        // purchase, so the walk hits `MAX_FUNDING_SIGNATURE_PAGES` without
        // reaching the purchase or the end. The old code trusted a partial
        // read as if it ended at the oldest transaction.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let responses = [
            signatures_page("mint-sig"),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            full_signatures_page_at("later", 1_000_000),
            full_signatures_page_at("later", 1_000_000),
            full_signatures_page_at("later", 1_000_000),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = Budget::new(60, 1, std::time::Duration::from_secs(30));
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

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
        // The candidate's only signature landed at slot 9, after its first
        // purchase at slot 5 -- so whatever moved lamports there did not
        // fund the buy; it happened afterward. funding_search skips it (its
        // slot is past the purchase), and that was the wallet's whole page,
        // so the walk reaches the end having fetched zero eligible
        // transactions. Finding 2: the candidate's own purchase must itself
        // sit at or before slot 5 in its own history, so fetching nothing at
        // all means the signature read was wrong, not that there is
        // genuinely no funder -- this must not be published as a measured
        // absence.
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
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(!funding.checked[0].funding_complete);
        assert!(funding.checked[0].funders.is_empty());
        assert!(
            funding.gaps.iter().any(|g| g.contains(&buyer)
                && g.contains("no signature at or before its first purchase")),
            "gaps: {:?}",
            funding.gaps
        );
    }

    #[test]
    fn a_fresh_wallets_funder_is_found_on_the_first_page() {
        // The simplest case: the candidate's whole history is one page with
        // one signature, and that signature's transaction is a material
        // inbound transfer landing at or before the first purchase.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let funder = solana_addr(0xf0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_at("buyer-sig", 3),
            funding_tx_at(&funder, &buyer, 500_000, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete);
        assert_eq!(funding.checked[0].funders.len(), 1);
        assert_eq!(funding.checked[0].funders[0].address, funder);
    }

    #[test]
    fn a_busy_wallets_funder_is_found_several_pages_back() {
        // The first page is full (1,000 signatures, all landing after the
        // first purchase so none is fetched) -- a full page is not the last
        // one, so `funding_search` must walk backward to a second page to
        // find the material transfer that funded the purchase.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let funder = solana_addr(0xf0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            full_signatures_page_at("recent", 100),
            signatures_page_at("buyer-sig", 3),
            funding_tx_at(&funder, &buyer, 500_000, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete);
        assert_eq!(funding.checked[0].funders.len(), 1);
        assert_eq!(funding.checked[0].funders[0].address, funder);
    }

    #[test]
    fn a_transfer_after_the_first_purchase_is_skipped_for_an_older_material_one() {
        // Newest-first, the first signature landed after the purchase (must
        // be skipped, not fetched at all) and the second landed before it
        // and is a material transfer -- demonstrating the skip-then-find
        // behavior distinctly from a plain end-of-history absence.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let funder = solana_addr(0xf0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_many(&[("after-sig", 9), ("before-sig", 3)]),
            funding_tx_at(&funder, &buyer, 500_000, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete);
        assert_eq!(funding.checked[0].funders.len(), 1);
        assert_eq!(funding.checked[0].funders[0].address, funder);
        assert_eq!(funding.checked[0].funders[0].transaction, "before-sig");
    }

    #[test]
    fn an_immaterial_dust_transfer_is_skipped_for_a_later_material_one() {
        // Newest-first, the first eligible signature is a dust transfer
        // (below the materiality threshold: `is_material` needs at least
        // half of `GAS_ALLOWANCE_LAMPORTS`) and must be passed over in favor
        // of the next, larger transfer.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let dust_sender = solana_addr(0xd0).to_string();
        let funder = solana_addr(0xf0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_many(&[("dust-sig", 4), ("material-sig", 3)]),
            funding_tx_at(&dust_sender, &buyer, 1_000, 4),
            funding_tx_at(&funder, &buyer, 500_000, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete);
        assert_eq!(funding.checked[0].funders.len(), 1);
        assert_eq!(funding.checked[0].funders[0].address, funder);
    }

    #[test]
    fn the_end_of_a_candidates_history_with_no_material_funder_is_a_measured_absence() {
        // The candidate's whole history (a short, final page) is read and
        // every eligible signature is fetched, but none is material: this is
        // a measurement, not a guess, so `funding_complete` is true with no
        // gap even though no funder was found.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let dust_sender = solana_addr(0xd0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_at("dust-sig", 3),
            funding_tx_at(&dust_sender, &buyer, 1_000, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete);
        assert!(funding.checked[0].funders.is_empty());
        assert!(funding.gaps.is_empty(), "gaps: {:?}", funding.gaps);
    }

    #[test]
    fn unreadable_balances_at_the_end_of_history_is_not_a_measured_absence() {
        // Finding 3: the candidate's whole history is one page with one
        // signature, and that transaction's node omitted the lamport
        // balance arrays entirely -- `funder_of` cannot tell "nothing moved"
        // from "cannot see what moved" here, so this must not read the same
        // as `the_end_of_a_candidates_history_with_no_material_funder_is_a_measured_absence`:
        // `funding_complete` must be false, with a gap naming the
        // unreadable transaction, even though the page itself ended the
        // walk.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_at("unreadable-sig", 3),
            unreadable_balances_tx_at(&buyer, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(!funding.checked[0].funding_complete);
        assert!(funding.checked[0].funders.is_empty());
        assert!(
            funding
                .gaps
                .iter()
                .any(|g| g.contains("did not report lamport balances")),
            "gaps: {:?}",
            funding.gaps
        );
    }

    #[test]
    fn an_empty_instruction_list_is_treated_as_unreadable_not_a_measured_miss() {
        // The candidate's whole history is one page with one signature, and
        // that transaction has the exact balance shape of a genuine
        // transfer (the funder loses lamports, the buyer gains them, the
        // amount clears materiality) but reports no instructions at all --
        // the shape a versioned transaction whose lookup-table accounts
        // never got merged into `accountKeys` produces (research 0056).
        // `is_plain_sol_transfer` must not treat "nothing here is a
        // non-System instruction" as true when there is nothing here at
        // all: an empty list never means "this process read the
        // instructions and found only System Program calls", so this must
        // read the same as an unreadable-balances transaction, not as a
        // measured absence. If the `is_empty` guard is removed this
        // transaction is (wrongly) accepted as a plain transfer and this
        // test fails on `funding_complete` and the missing gap.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let funder = solana_addr(0xf0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_at("empty-ix-sig", 3),
            unreadable_instructions_tx_at(&funder, &buyer, 500_000, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(!funding.checked[0].funding_complete);
        assert!(funding.checked[0].funders.is_empty());
        assert!(
            funding
                .gaps
                .iter()
                .any(|g| g.contains("reported no instructions")),
            "gaps: {:?}",
            funding.gaps
        );
    }

    #[test]
    fn a_readable_absence_still_reports_complete_ahead_of_an_unreadable_one() {
        // Contrast case for Finding 3, spelled out explicitly rather than
        // left implicit in the measured-absence test above: a transaction
        // whose balances were read and simply had no inbound move
        // (`FunderRead::NoInboundMove`) does not trip the same flag an
        // unreadable one does, so a search that only ever sees readable,
        // uninteresting transactions still ends as a measured absence.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let dust_sender = solana_addr(0xd0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_at("dust-sig", 3),
            funding_tx_at(&dust_sender, &buyer, 1_000, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete);
        assert!(funding.gaps.is_empty(), "gaps: {:?}", funding.gaps);
    }

    #[test]
    fn a_swap_shaped_balance_move_is_not_recorded_as_a_funder() {
        // Newest-first, the first eligible signature has the exact balance
        // shape `funder_of` would accept -- the "funder" loses lamports, the
        // buyer gains them -- but it invokes a non-System-Program
        // instruction, so it is really a swap (or a sell, a rent refund, a
        // wrapped-SOL unwrap: none of them a transfer). This is Finding 1:
        // without the plain-transfer gate, a stranger's unrelated trade
        // against a different pool would be recorded as having funded this
        // buyer. The walk must not stop or record a gap here -- it keeps
        // going backward and finds the genuine transfer sitting right
        // behind it.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let pool_vault = solana_addr(0xaa).to_string();
        let real_funder = solana_addr(0xf0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_many(&[("swap-sig", 4), ("transfer-sig", 3)]),
            non_transfer_tx_at(
                &pool_vault,
                &buyer,
                500_000,
                4,
                "SomeOtherProgram11111111111111111111111",
            ),
            funding_tx_at(&real_funder, &buyer, 500_000, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete);
        assert_eq!(funding.checked[0].funders.len(), 1);
        assert_eq!(funding.checked[0].funders[0].address, real_funder);
        assert_ne!(funding.checked[0].funders[0].address, pool_vault);
        assert!(funding.gaps.is_empty(), "gaps: {:?}", funding.gaps);
    }

    #[test]
    fn a_priority_fee_instruction_does_not_defeat_the_plain_transfer_gate() {
        // Finding 1: a real wallet (Phantom among them) routinely prepends a
        // ComputeBudget priority-fee instruction ahead of the actual
        // transfer. That instruction moves no lamports between accounts, so
        // it cannot make a pool the largest loser -- the gate must still
        // accept this as a plain transfer and record the funder, not skip it
        // and walk off the end of history.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let funder = solana_addr(0xf0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_at("funded-sig", 3),
            multi_instruction_tx_at(
                &funder,
                &buyer,
                500_000,
                3,
                &[
                    "ComputeBudget111111111111111111111111111111",
                    SYSTEM_PROGRAM,
                ],
            ),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete);
        assert_eq!(funding.checked[0].funders.len(), 1);
        assert_eq!(funding.checked[0].funders[0].address, funder);
        assert!(funding.gaps.is_empty(), "gaps: {:?}", funding.gaps);
    }

    #[test]
    fn an_all_compute_budget_transaction_is_not_accepted_as_a_plain_transfer() {
        // The "at least one System instruction" clause of the gate matters
        // on its own: a transaction carrying only ComputeBudget instructions
        // moves no lamports through the System Program at all, so it must
        // not be accepted just because every instruction is in
        // FEE_ONLY_PROGRAMS. The walk keeps going and finds the genuine
        // transfer behind it, the same as the swap-shaped case.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let pool_vault = solana_addr(0xaa).to_string();
        let real_funder = solana_addr(0xf0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_many(&[("cb-sig", 4), ("transfer-sig", 3)]),
            multi_instruction_tx_at(
                &pool_vault,
                &buyer,
                500_000,
                4,
                &["ComputeBudget111111111111111111111111111111"],
            ),
            funding_tx_at(&real_funder, &buyer, 500_000, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete);
        assert_eq!(funding.checked[0].funders.len(), 1);
        assert_eq!(funding.checked[0].funders[0].address, real_funder);
        assert_ne!(funding.checked[0].funders[0].address, pool_vault);
        assert!(funding.gaps.is_empty(), "gaps: {:?}", funding.gaps);
    }

    #[test]
    fn a_partial_lookup_table_read_is_unreadable_not_a_funder() {
        // Finding 4/6: a node that resolved a versioned transaction's
        // lookup-table accounts internally but did not report them back in
        // `accountKeys` still returns preBalances/postBalances sized for the
        // full account list. `funder_of`'s length check (wallets.rs:1010)
        // must reject this as Unreadable rather than trusting an index into
        // an `accounts` list shorter than the balance arrays -- a gap, not a
        // guessed funder or a measured absence.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_at("short-sig", 3),
            short_accounts_tx_at(&buyer, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(!funding.checked[0].funding_complete);
        assert!(funding.checked[0].funders.is_empty());
        assert!(
            funding
                .gaps
                .iter()
                .any(|g| g.contains(&buyer) && g.contains("lamport balances")),
            "gaps: {:?}",
            funding.gaps
        );
    }

    #[test]
    fn a_closed_token_accounts_rent_refund_is_not_recorded_as_a_funder() {
        // The buyer's own lamport balance rises because a token account
        // they closed refunded its rent (~2,039,280 lamports) -- the
        // "largest loser" in `funder_of`'s balance-only view is the
        // account being closed, which happens to belong to the buyer
        // themself. A non-System-Program instruction (the token program's
        // close) is exactly what marks this as not a plain transfer, so the
        // gate must reject it and keep walking to the real transfer behind
        // it, never naming the buyer as their own funder.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let closed_token_account = solana_addr(0xcc).to_string();
        let real_funder = solana_addr(0xf0).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page_many(&[("close-sig", 4), ("transfer-sig", 3)]),
            non_transfer_tx_at(
                &closed_token_account,
                &buyer,
                2_039_280,
                4,
                "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
            ),
            funding_tx_at(&real_funder, &buyer, 500_000, 3),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete);
        assert_eq!(funding.checked[0].funders.len(), 1);
        assert_eq!(funding.checked[0].funders[0].address, real_funder);
        assert_ne!(funding.checked[0].funders[0].address, buyer);
        assert!(funding.gaps.is_empty(), "gaps: {:?}", funding.gaps);
    }

    #[test]
    fn more_than_max_funding_signature_pages_walked_reports_incomplete() {
        // Every page returned is full (1,000 signatures, none eligible, so
        // none is fetched) -- `funding_search` never reaches the end of the
        // candidate's history, so after `MAX_FUNDING_SIGNATURE_PAGES` pages
        // it must stop and say so, not report a false absence.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let responses = [
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            full_signatures_page_at("p1", 100),
            full_signatures_page_at("p2", 100),
            full_signatures_page_at("p3", 100),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(!funding.checked[0].funding_complete);
        assert!(funding.checked[0].funders.is_empty());
        assert!(
            funding
                .gaps
                .iter()
                .any(|g| g.contains("signature pages walked")),
            "gaps: {:?}",
            funding.gaps
        );
    }

    #[test]
    fn more_than_max_funding_transactions_fetched_reports_incomplete() {
        // One page names 11 eligible signatures (at or before the purchase,
        // none failed): the 11th exceeds `MAX_FUNDING_TRANSACTIONS` (10)
        // before it is ever fetched, so the search stops and says so rather
        // than reporting a false absence after reading only part of the
        // page's eligible signatures.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let dust_sender = solana_addr(0xd0).to_string();
        let entries: Vec<(&str, u64)> = (0..11)
            .map(|i| {
                (
                    [
                        "s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10",
                    ][i],
                    3,
                )
            })
            .collect();
        let mut responses = vec![
            signatures_page_at("mint-sig", 5),
            buy_tx(&mint_key, &[(&buyer, 500)]),
        ];
        responses.push(signatures_page_many(&entries));
        for _ in 0..10 {
            responses.push(funding_tx_at(&dust_sender, &buyer, 1_000, 3));
        }
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(!funding.checked[0].funding_complete);
        assert!(funding.checked[0].funders.is_empty());
        assert!(
            funding
                .gaps
                .iter()
                .any(|g| g.contains("transactions fetched")),
            "gaps: {:?}",
            funding.gaps
        );
    }

    #[test]
    fn a_failed_transaction_read_for_one_candidate_does_not_abort_the_others() {
        // Two candidates share the mint's one buy transaction. The first
        // candidate's own transaction read comes back malformed -- a named
        // gap, `funding_complete == false` -- but the second candidate is
        // still checked and its own funder still found.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer1 = solana_addr(1).to_string();
        let buyer2 = solana_addr(2).to_string();
        let funder = solana_addr(0xf0).to_string();
        let responses = [
            signatures_page("mint-sig"),
            buy_tx(&mint_key, &[(&buyer1, 500), (&buyer2, 500)]),
            signatures_page("buyer1-sig"),
            malformed_response(),
            signatures_page("buyer2-sig"),
            funding_tx(&funder, &buyer2, 500_000),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 2);
        assert!(!funding.checked[0].funding_complete);
        assert!(funding.checked[0].funders.is_empty());
        assert!(funding.checked[1].funding_complete);
        assert_eq!(funding.checked[1].funders.len(), 1);
        assert_eq!(funding.checked[1].funders[0].address, funder);
        assert!(
            funding.gaps.iter().any(|g| g.contains("buyer1-sig")),
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
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.buyers, 6);
        assert_eq!(funding.selected, u32::try_from(MAX_CANDIDATES).unwrap());
        assert_eq!(funding.checked.len(), MAX_CANDIDATES);
    }

    // The floor is every candidate's worst case, not merely "enough for the
    // fixture below": that test's four candidates need about a dozen calls,
    // so a floor of 17 or 120 passes it too (both survived cargo-mutants on
    // #168). Pinned to the arithmetic it claims: 4 x (3 pages + 10 transactions).
    #[test]
    fn the_funding_call_floor_covers_every_candidate_walking_to_both_caps() {
        assert_eq!(MAX_CANDIDATES, 4);
        assert_eq!(MAX_FUNDING_SIGNATURE_PAGES, 3);
        assert_eq!(MAX_FUNDING_TRANSACTIONS, 10);
        assert_eq!(FUNDING_CALL_FLOOR, 52);
    }

    #[test]
    fn a_call_floor_lets_every_candidate_be_checked_when_earlier_steps_spent_the_shared_calls() {
        // Regression for the starvation research 0056's 2026-09-23 addendum
        // documents: `dossier::build`'s steps 1 through 4 share one call
        // budget with step 5 (this function), and on a real capture they
        // routinely leave it too little of that shared pool to check every
        // candidate -- even though step 5's own *pages* are floored
        // (`Budget::grant_pages`), its *calls* never were before this fix.
        //
        // Four candidates share the mint's one buy transaction. Reading the
        // window costs 2 calls (the mint's own signature page, then that one
        // transaction); each candidate that is actually checked costs 2 more
        // (its own signature page, then the funding transaction found on
        // it) -- 10 calls in total when nothing is starved. A budget left
        // with only 6 calls (as if steps 1 through 4 had already spent 54 of
        // `DEFAULT_MAX_CALLS`) checks only the first two candidates and
        // reports a `Calls` gap for the rest -- the bug -- unless it is
        // first raised to `FUNDING_CALL_FLOOR` the way `dossier::build` now
        // does immediately before calling this function, in which case the
        // identical transport lets every candidate be checked.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyers: Vec<String> = (1..=4u8).map(|b| solana_addr(b).to_string()).collect();
        let funder = solana_addr(0xf0).to_string();

        let mut responses = vec![
            signatures_page("mint-sig"),
            buy_tx(
                &mint_key,
                &buyers.iter().map(|b| (b.as_str(), 500)).collect::<Vec<_>>(),
            ),
        ];
        for (i, buyer) in buyers.iter().enumerate() {
            responses.push(signatures_page(&format!("buyer{i}-sig")));
            responses.push(funding_tx(&funder, buyer, 100));
        }
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();

        let starved_client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut starved_budget = Budget::new(6, 60, std::time::Duration::from_secs(30));
        let starved = investigate_solana(&starved_client, &mut starved_budget, &mint, None)
            .expect("a result");
        assert_eq!(starved.checked.len(), MAX_CANDIDATES);
        let complete = starved
            .checked
            .iter()
            .filter(|c| c.funding_complete)
            .count();
        assert_eq!(complete, 2, "{:?}", starved.gaps);
        assert!(
            starved.gaps.iter().any(|g| g.contains("Calls")),
            "{:?}",
            starved.gaps
        );

        let floored_client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut floored_budget = Budget::new(6, 60, std::time::Duration::from_secs(30));
        floored_budget.grant_calls(FUNDING_CALL_FLOOR);
        let floored = investigate_solana(&floored_client, &mut floored_budget, &mint, None)
            .expect("a result");
        assert_eq!(floored.checked.len(), MAX_CANDIDATES);
        assert!(
            floored.checked.iter().all(|c| c.funding_complete),
            "{:?}",
            floored.gaps
        );
        assert!(floored.gaps.is_empty(), "{:?}", floored.gaps);
    }

    // The floor is every candidate's worst case, the same way
    // `FUNDING_CALL_FLOOR`'s pin above is: pinned to the arithmetic it
    // claims, 4 candidates x 3 pages each.
    #[test]
    fn the_funding_page_floor_covers_every_candidate_walking_to_both_caps() {
        assert_eq!(MAX_CANDIDATES, 4);
        assert_eq!(MAX_FUNDING_SIGNATURE_PAGES, 3);
        assert_eq!(FUNDING_PAGE_FLOOR, 12);
    }

    #[test]
    fn a_page_floor_lets_every_candidate_be_checked_when_earlier_steps_spent_the_shared_pages() {
        // Regression for the shape research 0056's 2026-09-23 addendum
        // actually captured on the VPS: `dossier::build`'s step 1 (the
        // mint's own signature walk) routinely spends the whole shared page
        // pool before step 5 (this function) starts, and the page floor
        // step 5 used to be granted only when step 1 was skipped
        // (`mint_signatures.is_none()`) -- which almost never happens,
        // since step 1 usually succeeds. A budget left with zero pages (as
        // if step 1 had already spent every one of them) must still check
        // the candidate, because this function grants `FUNDING_PAGE_FLOOR`
        // itself just before its candidate loop. Remove that grant and the
        // candidate reports "page budget exhausted" instead.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let funder = solana_addr(0xf0).to_string();
        let signatures = vec![crate::rpc::SignatureInfo {
            signature: "mint-sig".to_owned(),
            slot: 1,
            err: None,
        }];

        let responses = [
            buy_tx(&mint_key, &[(&buyer, 500)]),
            signatures_page("buyer-sig"),
            funding_tx(&funder, &buyer, 100_000),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();

        let floored_client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut floored_budget = Budget::new(60, 0, std::time::Duration::from_secs(30));
        let floored = investigate_solana(
            &floored_client,
            &mut floored_budget,
            &mint,
            Some(&(signatures, false)),
        )
        .expect("a result");
        assert_eq!(floored.checked.len(), 1);
        assert!(floored.checked[0].funding_complete, "{:?}", floored.gaps);
        assert_eq!(floored.checked[0].funders.len(), 1);
        assert_eq!(floored.checked[0].funders[0].address, funder);
        assert!(floored.gaps.is_empty(), "{:?}", floored.gaps);
    }

    #[test]
    fn a_rejected_purchase_anchored_read_falls_back_to_the_newest_first_walk() {
        // The node refuses the first, purchase-anchored signature read; the
        // search must retry newest-first and still find the funder, with no
        // gap for the refused attempt. Only the first page may fall back --
        // an error on any later page is a real gap, not a retry.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let funder = solana_addr(0xf0).to_string();
        let signatures = vec![crate::rpc::SignatureInfo {
            signature: "mint-sig".to_owned(),
            slot: 1,
            err: None,
        }];

        let responses = [
            buy_tx(&mint_key, &[(&buyer, 500)]),
            r#"{"result":null,"error":{"code":-32602,"message":"Invalid param: before"}}"#
                .to_owned(),
            signatures_page("buyer-sig"),
            funding_tx(&funder, &buyer, 100_000),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = Budget::new(60, 3, std::time::Duration::from_secs(30));
        let funding = investigate_solana(&client, &mut budget, &mint, Some(&(signatures, false)))
            .expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(funding.checked[0].funding_complete, "{:?}", funding.gaps);
        assert_eq!(funding.checked[0].funders.len(), 1);
        assert_eq!(funding.checked[0].funders[0].address, funder);
        assert!(funding.gaps.is_empty(), "{:?}", funding.gaps);
    }

    /// A transport for [`funding_search`]'s purchase-anchored-start test:
    /// routes a `getSignaturesForAddress` call naming `buyer` to a canned
    /// page chosen by that call's own `before` cursor, and every other call
    /// (the mint's own signature page, every `getTransaction`) to a fixed
    /// FIFO queue -- unlike [`Canned`], whose single queue answers every
    /// call the same way regardless of `before` and so cannot tell a
    /// purchase-anchored request from a newest-first one apart, which is
    /// exactly the distinction
    /// [`funding_search_starts_at_the_purchase_not_the_newest_signature`]
    /// exists to prove.
    struct BeforeRouted {
        buyer: String,
        fixed: std::sync::Mutex<Vec<String>>,
        by_before: std::collections::HashMap<String, String>,
    }

    /// The value of a `"before":"..."` field in a raw JSON-RPC request
    /// body, or `None` when the request carries no `before` at all (the
    /// walk's very first page).
    fn before_in(body: &str) -> Option<String> {
        let marker = "\"before\":\"";
        let start = body.find(marker)? + marker.len();
        let rest = &body[start..];
        let end = rest.find('"')?;
        Some(rest[..end].to_owned())
    }

    impl crate::rpc::Transport for BeforeRouted {
        fn post(&self, _: &str, body: String) -> Result<String, String> {
            if body.contains("getSignaturesForAddress") && body.contains(&self.buyer) {
                let before = before_in(&body).unwrap_or_default();
                return self
                    .by_before
                    .get(before.as_str())
                    .cloned()
                    .ok_or_else(|| format!("no canned page for before={before:?}"));
            }
            self.fixed
                .lock()
                .map_err(|_| "poisoned".to_owned())?
                .pop()
                .ok_or_else(|| "the client asked for more than the test supplied".to_owned())
        }
    }

    #[test]
    fn funding_search_starts_at_the_purchase_not_the_newest_signature() {
        // A buyer that kept trading after its purchase: more than
        // `MAX_FUNDING_SIGNATURE_PAGES` pages of its own newest history are
        // unrelated post-purchase activity, and its funding transfer sits
        // right before the purchase instead. Starting the walk at
        // `before = Some(purchase signature)` reaches the funder in a
        // single page; starting at `before = None` (the pre-fix behaviour)
        // spends the whole page cap on the post-purchase pages and never
        // gets there. Temporarily reverting `funding_search` to start at
        // `before = None` and re-running this test reproduces that: it then
        // fails with a "more than 3 signature pages walked" gap instead of
        // finding the funder.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let buyer = solana_addr(1).to_string();
        let funder = solana_addr(0xf0).to_string();

        let mut by_before = std::collections::HashMap::new();
        by_before.insert(String::new(), full_signatures_page_at("junk1", 100));
        by_before.insert(
            "junk1-999".to_owned(),
            full_signatures_page_at("junk2", 100),
        );
        by_before.insert(
            "junk2-999".to_owned(),
            full_signatures_page_at("junk3", 100),
        );
        by_before.insert("mint-sig".to_owned(), signatures_page_at("funding-sig", 1));

        let fixed = vec![
            signatures_page("mint-sig"),
            buy_tx(&mint_key, &[(&buyer, 500)]),
            funding_tx_at(&funder, &buyer, 500_000, 1),
        ];
        let transport = BeforeRouted {
            buyer: buyer.clone(),
            fixed: std::sync::Mutex::new(fixed.into_iter().rev().collect()),
            by_before,
        };
        let client = RpcClient::with_transport("http://test.invalid", Box::new(transport));
        let mut budget = solana_budget();
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(funding.checked.len(), 1);
        assert!(
            funding.checked[0].funding_complete,
            "gaps: {:?}",
            funding.gaps
        );
        assert_eq!(funding.checked[0].funders.len(), 1);
        assert_eq!(funding.checked[0].funders[0].address, funder);
        assert!(funding.gaps.is_empty(), "gaps: {:?}", funding.gaps);
    }

    /// A bare transaction for the pure readers below, with only the fields
    /// they look at filled in.
    fn bare_tx(accounts: &[&str], pre: &[u64], post: &[u64]) -> crate::rpc::Transaction {
        crate::rpc::Transaction {
            slot: realorrug_types::Slot(1),
            accounts: accounts.iter().map(|a| (*a).to_owned()).collect(),
            instructions: Vec::new(),
            pre_token_balances: Vec::new(),
            post_token_balances: Vec::new(),
            pre_balances: pre.to_vec(),
            post_balances: post.to_vec(),
            failed: false,
        }
    }

    #[test]
    fn a_transaction_missing_post_balances_names_no_funder() {
        // Accounts and pre-balances agree but post-balances are short: an
        // index into them cannot be trusted, so there is no funder, not a
        // guess (and not a panic) -- and it is unreadable, not a measured
        // absence (Finding 3).
        let tx = bare_tx(&["candidate", "funder"], &[0, 1_000], &[500]);
        assert_eq!(funder_of(&tx, "candidate"), FunderRead::Unreadable);
    }

    #[test]
    fn a_holder_whose_balance_fell_is_not_a_buyer() {
        // The account held 100 before and 50 after: a sale. Reading its
        // before-balance is what keeps it from looking like a buy of 50.
        let mut tx = bare_tx(&[], &[], &[]);
        let balance = |amount| crate::rpc::TokenBalance {
            account_index: 0,
            mint: "mint".to_owned(),
            amount,
            owner: Some("seller".to_owned()),
        };
        tx.pre_token_balances.push(balance(100));
        tx.post_token_balances.push(balance(50));
        assert!(buyers_in(&tx, "mint", None).is_empty());
    }

    #[test]
    fn the_launch_window_stops_after_its_transactions_are_read() {
        // One more successful transaction than the window holds, each with
        // its own buyer: the last one is past the window and never read.
        let mint = solana_addr(9);
        let mint_key = mint.to_string();
        let total = SOLANA_WINDOW_TRANSACTIONS + 1;
        let entries: Vec<String> = (0..total)
            .map(|i| format!(r#"{{"signature":"w{i}","slot":1}}"#))
            .collect();
        let mut responses = vec![format!(
            r#"{{"result":[{}],"error":null}}"#,
            entries.join(",")
        )];
        let owners: Vec<String> = (0..total).map(|i| format!("buyer{i}")).collect();
        for owner in &owners {
            responses.push(buy_tx(&mint_key, &[(owner.as_str(), 500)]));
        }
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = Budget::new(200, 200, std::time::Duration::from_secs(30));
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

        assert_eq!(
            funding.buyers,
            u32::try_from(SOLANA_WINDOW_TRANSACTIONS).unwrap()
        );
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
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

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
        let funding = investigate_solana(&client, &mut budget, &mint, None).expect("a result");

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
        let err = investigate_solana(&client, &mut budget, &mint, None)
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

    fn sale(seller: u8, tokens: u128, block: u64) -> Sale {
        Sale {
            seller: addr(seller),
            tokens,
            quote: tokens,
            block,
            position: (0, 0),
            transaction: Hash32([1; 32]),
        }
    }

    /// Two wallets that bought in the same window with matched sizes clear
    /// [`SELL_CLUSTER_LINK_THRESHOLD_BPS`] (4,000, the "same window + sizes"
    /// tier) -- the fixture both boundary tests below reuse.
    fn linked_buyers() -> Vec<Buyer> {
        vec![buyer(1, 100, 10), buyer(2, 100, 10)]
    }

    #[test]
    fn a_sell_landing_exactly_50_blocks_after_the_anchor_joins_the_cluster() {
        let sells = vec![sale(1, 10, 100), sale(2, 10, 150)];
        let cluster = largest_sell_cluster(&sells, &linked_buyers()).unwrap();
        assert_eq!(
            cluster.members.len(),
            2,
            "50 blocks apart is inside the window"
        );
    }

    fn linked_trio_and_pair() -> Vec<Buyer> {
        // 1, 2 bought together; 3, 4, 5 bought together 190 blocks later,
        // too far from 1 and 2 to link to them.
        vec![
            buyer(1, 100, 10),
            buyer(2, 100, 10),
            buyer(3, 100, 200),
            buyer(4, 100, 200),
            buyer(5, 100, 200),
        ]
    }

    #[test]
    fn a_later_bigger_cluster_replaces_an_earlier_smaller_one() {
        let sells = vec![
            sale(1, 10, 100),
            sale(2, 10, 100),
            sale(3, 10, 500),
            sale(4, 10, 500),
            sale(5, 10, 510),
        ];
        let cluster = largest_sell_cluster(&sells, &linked_trio_and_pair()).unwrap();
        assert_eq!(cluster.members, vec![addr(3), addr(4), addr(5)]);
    }

    #[test]
    fn of_two_same_size_clusters_the_earlier_is_kept() {
        let sells = vec![
            sale(1, 10, 100),
            sale(2, 10, 100),
            sale(3, 10, 500),
            sale(4, 10, 500),
        ];
        let cluster = largest_sell_cluster(&sells, &linked_trio_and_pair()).unwrap();
        assert_eq!(cluster.members, vec![addr(1), addr(2)]);
    }

    #[test]
    fn a_tiny_early_sell_per_wallet_does_not_hide_a_later_joint_dump() {
        let buyers = vec![buyer(1, 100, 10), buyer(2, 100, 10), buyer(3, 100, 10)];
        let sells = vec![
            sale(1, 1, 100),
            sale(2, 1, 1_000),
            sale(3, 1, 2_000),
            sale(1, 100, 10_000),
            sale(2, 100, 10_000),
            sale(3, 100, 10_000),
        ];
        let cluster = largest_sell_cluster(&sells, &buyers).unwrap();
        assert_eq!(cluster.members.len(), 3);
        assert_eq!(cluster.tokens_sold, 300);
        assert_eq!((cluster.first_block, cluster.last_block), (10_000, 10_000));
    }

    #[test]
    fn a_members_sale_outside_the_window_counts_neither_tokens_nor_spread() {
        let sells = vec![sale(1, 10, 100), sale(2, 10, 100), sale(1, 1_000, 5_000)];
        let cluster = largest_sell_cluster(&sells, &linked_buyers()).unwrap();
        assert_eq!(cluster.members.len(), 2);
        assert_eq!(cluster.tokens_sold, 20);
        assert_eq!(cluster.last_block, 100);
    }

    #[test]
    fn buys_exactly_30_blocks_apart_link_and_31_do_not() {
        let sells = vec![sale(1, 10, 100), sale(2, 10, 100)];
        let at_edge = vec![buyer(1, 100, 10), buyer(2, 100, 40)];
        let past_edge = vec![buyer(1, 100, 10), buyer(2, 100, 41)];
        assert_eq!(
            largest_sell_cluster(&sells, &at_edge)
                .unwrap()
                .members
                .len(),
            2
        );
        assert_eq!(
            largest_sell_cluster(&sells, &past_edge)
                .unwrap()
                .members
                .len(),
            1
        );
    }

    #[test]
    fn sellers_whose_buys_only_share_a_window_do_not_link() {
        // Same 30-block window but sizes 2x apart: 2,000 bps, under the
        // 4,000 bps "window + matched sizes" line.
        let buyers = vec![buyer(1, 100, 10), buyer(2, 200, 20)];
        let sells = vec![sale(1, 10, 100), sale(2, 10, 110)];
        let cluster = largest_sell_cluster(&sells, &buyers).unwrap();
        assert_eq!(cluster.members.len(), 1);
    }

    #[test]
    fn a_seller_with_no_buy_on_record_joins_no_cluster() {
        let sells = vec![sale(1, 10, 100), sale(3, 10, 110)];
        let cluster = largest_sell_cluster(&sells, &linked_buyers()).unwrap();
        assert_eq!(cluster.members, vec![addr(1)]);
    }

    #[test]
    fn a_seller_in_the_anchors_own_block_joins_the_cluster() {
        let sells = vec![sale(2, 10, 100), sale(1, 10, 100)];
        let cluster = largest_sell_cluster(&sells, &linked_buyers()).unwrap();
        assert_eq!(cluster.members.len(), 2);
        assert_eq!((cluster.first_block, cluster.last_block), (100, 100));
        assert_eq!(cluster.tokens_sold, 20);
    }

    #[test]
    fn a_sell_landing_51_blocks_after_the_anchor_does_not_join_the_cluster() {
        let sells = vec![sale(1, 10, 100), sale(2, 10, 151)];
        let cluster = largest_sell_cluster(&sells, &linked_buyers()).unwrap();
        assert_eq!(
            cluster.members.len(),
            1,
            "51 blocks apart is just outside the window"
        );
    }
    // -- creator_cash_flow_solana ---------------------------------------

    /// A token balance entry for `owner`'s holding of `mint` at
    /// `account_index`.
    fn token_balance(
        account_index: usize,
        mint: &str,
        owner: &str,
        amount: u64,
    ) -> crate::rpc::TokenBalance {
        crate::rpc::TokenBalance {
            account_index,
            mint: mint.to_owned(),
            amount,
            owner: Some(owner.to_owned()),
        }
    }

    /// A bare transaction for [`classify_creator_transaction`], with the
    /// creator at account index 0 and only the fields the classifier reads
    /// filled in.
    fn creator_tx(
        creator: &str,
        mint: &str,
        token_before: u64,
        token_after: u64,
        lamports_before: u64,
        lamports_after: u64,
    ) -> crate::rpc::Transaction {
        let mut tx = crate::rpc::Transaction {
            slot: realorrug_types::Slot(7),
            accounts: vec![creator.to_owned()],
            instructions: Vec::new(),
            pre_token_balances: Vec::new(),
            post_token_balances: Vec::new(),
            pre_balances: vec![lamports_before],
            post_balances: vec![lamports_after],
            failed: false,
        };
        tx.pre_token_balances
            .push(token_balance(0, mint, creator, token_before));
        tx.post_token_balances
            .push(token_balance(0, mint, creator, token_after));
        tx
    }

    /// A top-level instruction naming a known trading venue, for tests that
    /// must show a real trade was invoked rather than an incidental balance
    /// move next to an account close or a plain transfer.
    fn trading_instruction() -> crate::rpc::RawInstruction {
        crate::rpc::RawInstruction {
            program: KNOWN_TRADING_PROGRAMS[0].to_owned(),
            data: Vec::new(),
            accounts: Vec::new(),
        }
    }

    #[test]
    fn a_token_increase_with_a_sol_decrease_is_a_buy() {
        // Token balance rose by 500, SOL fell by 1_000: a buy, quoted at
        // the SOL paid -- through a known trading program, so the buy is
        // trusted rather than mistaken for an account-open side effect.
        let mut tx = creator_tx("creator", "mint", 0, 500, 10_000, 9_000);
        tx.instructions.push(trading_instruction());
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::Buy {
                quote: 1_000,
                tokens: 500
            }
        );
    }

    #[test]
    fn a_token_decrease_with_a_sol_increase_is_a_sell() {
        // Token balance fell by 500, SOL rose by 1_200 (the sale proceeds
        // net of fees): a sell, quoted at the SOL received -- through a
        // known trading program.
        let mut tx = creator_tx("creator", "mint", 500, 0, 9_000, 10_200);
        tx.instructions.push(trading_instruction());
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::Sell {
                quote: 1_200,
                tokens: 500
            }
        );
    }

    #[test]
    fn an_account_close_with_no_trading_program_is_an_unpriced_transfer_out() {
        // The token balance falls to nothing (the account closed) and
        // lamports rise sharply (the rent refund), but no known trading
        // program was invoked -- this must never be read as a sale.
        let tx = creator_tx("creator", "mint", 500, 0, 10_000, 2_039_275);
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::TransferOut
        );
    }

    #[test]
    fn an_account_close_through_a_trading_program_is_still_a_sell() {
        // The same balances as above, but a known trading program (pump.fun)
        // was actually invoked: the rent refund lands in the same
        // transaction as a real sale, and the sale still counts.
        let mut tx = creator_tx("creator", "mint", 500, 0, 10_000, 2_039_275);
        tx.instructions.push(trading_instruction());
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::Sell {
                quote: 2_029_275,
                tokens: 500
            }
        );
    }

    #[test]
    fn two_creator_owned_entries_for_the_mint_are_summed_not_the_first_found() {
        // A temporary account and the ATA both hold `mint` for the creator
        // in the same transaction: the pre and post sides must each be
        // summed across every matching entry, not just the first `.find`
        // would have hit.
        let mut tx = creator_tx("creator", "mint", 0, 0, 10_000, 9_000);
        tx.pre_token_balances = vec![
            token_balance(0, "mint", "creator", 100),
            token_balance(1, "mint", "creator", 200),
        ];
        tx.post_token_balances = vec![
            token_balance(0, "mint", "creator", 300),
            token_balance(1, "mint", "creator", 200),
        ];
        tx.instructions.push(trading_instruction());
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::Buy {
                quote: 1_000,
                tokens: 200
            }
        );
    }

    #[test]
    fn a_token_decrease_with_no_sol_increase_is_an_unpriced_transfer_out() {
        // Token balance fell with SOL merely paying its own fee (also a
        // fall): not a sale, since nothing came back for the tokens -- an
        // ordinary transfer out, never priced.
        let tx = creator_tx("creator", "mint", 500, 0, 10_000, 9_995);
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::TransferOut
        );
    }

    #[test]
    fn a_token_increase_with_no_sol_decrease_is_ignored() {
        // A received transfer: tokens arrived without the creator's own SOL
        // balance falling, so it is not a purchase.
        let tx = creator_tx("creator", "mint", 0, 500, 9_995, 10_000);
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::Ignored
        );
    }

    #[test]
    fn pre_balance_noise_for_a_different_owner_or_mint_is_never_summed_in() {
        // The pre-side filter must require *both* the mint and the owner to
        // match (an `&&`, not an `||`): a same-mint entry owned by someone
        // else, and a creator-owned entry of a different mint, must both be
        // excluded from `token_before`. With no trading program invoked, the
        // result falls straight through to `classify_without_lamports`, so
        // the token totals alone decide it.
        let mut tx = creator_tx("creator", "mint", 0, 0, 10_000, 9_999);
        tx.pre_token_balances = vec![
            token_balance(0, "mint", "someone-else", 500),
            token_balance(1, "othermint", "creator", 300),
        ];
        tx.post_token_balances = vec![token_balance(2, "mint", "creator", 0)];
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::Ignored,
            "neither noise entry matches both mint and owner, so token_before must read 0"
        );
    }

    #[test]
    fn post_balance_noise_for_a_different_owner_or_mint_is_never_summed_in() {
        // The mirror of the test above for the post side: a same-mint entry
        // owned by someone else, and a creator-owned entry of a different
        // mint, must both be excluded from `token_after`. The real balance
        // falls from 500 to 0 (an unpriced transfer out); if the noise
        // leaked in, `token_after` would read 1300 instead and flip the
        // result to `Ignored`.
        let mut tx = creator_tx("creator", "mint", 0, 0, 10_000, 9_999);
        tx.pre_token_balances = vec![token_balance(0, "mint", "creator", 500)];
        tx.post_token_balances = vec![
            token_balance(1, "mint", "someone-else", 600),
            token_balance(2, "othermint", "creator", 700),
        ];
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::TransferOut,
            "neither noise entry matches both mint and owner, so token_after must read 0"
        );
    }

    #[test]
    fn a_token_increase_with_unchanged_lamports_is_never_a_buy() {
        // Token balance rose, but the creator's own lamport balance did not
        // move at all -- the `after < before` guard must read false here
        // (and stay false under a `<=` swap, since `after == before`), so
        // this must fall through to `classify_without_lamports` rather than
        // becoming a `Buy` with a phantom zero-SOL quote.
        let mut tx = creator_tx("creator", "mint", 0, 500, 10_000, 10_000);
        tx.instructions.push(trading_instruction());
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::Ignored
        );
    }

    #[test]
    fn a_token_decrease_with_unchanged_lamports_is_never_a_sell() {
        // The mirror case: token balance fell, lamports unchanged -- the
        // `after > before` guard must read false (and stay false under a
        // `>=` swap), so this is an unpriced transfer out, never a `Sell`
        // with a phantom zero-SOL quote.
        let mut tx = creator_tx("creator", "mint", 500, 0, 10_000, 10_000);
        tx.instructions.push(trading_instruction());
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::TransferOut
        );
    }

    #[test]
    fn a_partial_sell_reports_the_tokens_actually_sold_not_their_sum() {
        // token_before (500) - token_after (200) = 300 tokens sold. Both
        // operands are non-zero here, so a `-`/`+` swap on the sell's token
        // count is only caught with a token_after that is not zero: the
        // existing full-exit sell test above has `token_after == 0`, where
        // subtraction and addition happen to agree.
        let mut tx = creator_tx("creator", "mint", 500, 200, 9_000, 10_200);
        tx.instructions.push(trading_instruction());
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::Sell {
                quote: 1_200,
                tokens: 300
            }
        );
    }

    #[test]
    fn a_creator_missing_from_the_account_keys_never_guesses_a_sale_price() {
        // The creator's own account is not among this transaction's keys at
        // all (e.g. a v0 transaction whose loaded addresses did not include
        // it) -- there is no lamport balance to compare against, so a token
        // fall is recorded as an unpriced transfer, never a guessed sale.
        let mut tx = creator_tx("someone-else", "mint", 0, 0, 1, 1);
        tx.accounts = vec!["someone-else".to_owned()];
        tx.pre_token_balances = vec![token_balance(0, "mint", "creator", 500)];
        tx.post_token_balances = vec![token_balance(0, "mint", "creator", 0)];
        assert_eq!(
            classify_creator_transaction(&tx, "creator", "mint"),
            SolanaCashFlowEvent::TransferOut
        );
    }

    /// A `getTransaction` answer for the creator's own ATA history: the
    /// creator's balance of `mint` moved from `token_before` to
    /// `token_after`, and their own lamport balance (account index 0) moved
    /// from `lamports_before` to `lamports_after`.
    fn creator_cash_flow_tx(
        creator: &str,
        mint: &str,
        token_before: u64,
        token_after: u64,
        lamports_before: u64,
        lamports_after: u64,
    ) -> String {
        format!(
            r#"{{"result":{{"slot":7,"meta":{{"err":null,"preBalances":[{lamports_before}],"postBalances":[{lamports_after}],"preTokenBalances":[{{"accountIndex":0,"mint":"{mint}","owner":"{creator}","uiTokenAmount":{{"amount":"{token_before}"}}}}],"postTokenBalances":[{{"accountIndex":0,"mint":"{mint}","owner":"{creator}","uiTokenAmount":{{"amount":"{token_after}"}}}}]}},"transaction":{{"message":{{"accountKeys":["{creator}"],"instructions":[{{"programId":"{trading_program}"}}]}}}}}},"error":null}}"#,
            trading_program = KNOWN_TRADING_PROGRAMS[0],
        )
    }

    /// A failed `getTransaction` answer: `meta.err` is set, so nothing it
    /// moved counts.
    fn failed_tx(creator: &str, mint: &str) -> String {
        format!(
            r#"{{"result":{{"slot":7,"meta":{{"err":{{"InstructionError":[0,"Custom"]}},"preBalances":[10000],"postBalances":[9000],"preTokenBalances":[],"postTokenBalances":[]}},"transaction":{{"message":{{"accountKeys":["{creator}"],"instructions":[]}}}}}},"error":null}}"#,
        )
        .replace("{mint}", mint)
    }

    #[test]
    fn the_dev_buy_is_the_first_trade_read() {
        // The ATA's own oldest signature is the launch transaction's dev
        // buy: reading oldest-first, it is the first trade in the result.
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let mint_key = mint.to_string();
        let creator_key = creator.to_string();
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            signatures_page("dev-buy-sig"),
            creator_cash_flow_tx(&creator_key, &mint_key, 0, 1_000, 10_000, 9_000),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(flow.trades_complete);
        assert_eq!(flow.trades.len(), 1);
        assert_eq!(flow.trades[0].side, Side::Buy);
        assert_eq!(flow.trades[0].quote, 1_000);
        assert_eq!(flow.trades[0].tokens, 1_000);
        assert_eq!(flow.trades[0].role, CreatorRole::Deployer);
        assert_eq!(flow.quote_asset.symbol, "SOL");
    }

    #[test]
    fn a_sell_after_the_dev_buy_is_recorded_too() {
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let mint_key = mint.to_string();
        let creator_key = creator.to_string();
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            signatures_page("sell-sig"),
            creator_cash_flow_tx(&creator_key, &mint_key, 1_000, 0, 9_000, 10_200),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(flow.trades_complete);
        assert_eq!(flow.trades.len(), 1);
        assert_eq!(flow.trades[0].side, Side::Sell);
        assert_eq!(flow.trades[0].quote, 1_200);
    }

    #[test]
    fn a_failed_transaction_is_skipped_entirely() {
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let mint_key = mint.to_string();
        let creator_key = creator.to_string();
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            signatures_page("failed-sig"),
            failed_tx(&creator_key, &mint_key),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(flow.trades_complete);
        assert!(flow.trades.is_empty());
        assert_eq!(flow.transfers_out, 0);
    }

    #[test]
    fn an_ata_with_no_signature_history_at_all_is_a_complete_zero_trade_read() {
        // An empty, non-truncated signature list is proof the ATA was never
        // created (any touch, ever, would leave a discoverable signature) --
        // zero trades is a real answer here, not an absence dressed as one.
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            r#"{"result":[],"error":null}"#.to_owned(),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(flow.trades_complete);
        assert!(flow.trades.is_empty());
        assert!(flow.gaps.is_empty());
    }

    #[test]
    fn a_shared_budget_with_no_pages_left_reports_incomplete_not_zero() {
        // The coordinator's own finding: `dossier::build()` draws every
        // page-based walk from one shared `Budget`. If an earlier step spent
        // every page before this read starts, `signatures_back_to_oldest`
        // returns immediately with an empty, *truncated* list -- this must
        // never be read as "the ATA was never created".
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = Budget::new(60, 0, std::time::Duration::from_secs(30));
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(!flow.trades_complete);
        assert!(flow.trades.is_empty());
        assert!(
            flow.gaps.iter().any(|g| g.contains("page budget")),
            "gaps: {:?}",
            flow.gaps
        );
    }

    #[test]
    fn more_signatures_than_the_cap_reports_incomplete_not_a_partial_history() {
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let entries: Vec<String> = (0..=CREATOR_CASH_FLOW_MAX_SIGNATURES)
            .map(|i| format!(r#"{{"signature":"sig-{i}","slot":1}}"#))
            .collect();
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            format!(r#"{{"result":[{}],"error":null}}"#, entries.join(",")),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = Budget::new(500, 500, std::time::Duration::from_secs(30));
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(!flow.trades_complete);
        assert!(flow.trades.is_empty());
        assert!(
            flow.gaps.iter().any(|g| g.contains("more than")),
            "gaps: {:?}",
            flow.gaps
        );
    }

    #[test]
    fn a_transaction_with_no_meta_at_all_is_reported_as_a_gap_not_a_no_op() {
        // `getTransaction` answered, but with no `meta` key: `parse_transaction`
        // still returns `Some`, with every balance array defaulted to empty
        // (rpc.rs's own doc on `lamport_balances`/`token_balances`). That must
        // not be read as "nothing happened" -- it is unread data.
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let creator_key = creator.to_string();
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let no_meta_tx = format!(
            r#"{{"result":{{"slot":7,"transaction":{{"message":{{"accountKeys":["{creator_key}"],"instructions":[]}}}}}},"error":null}}"#
        );
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            signatures_page("no-meta-sig"),
            no_meta_tx,
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(!flow.trades_complete);
        assert!(flow.trades.is_empty());
        assert!(
            flow.gaps
                .iter()
                .any(|g| g.contains("did not report balances")),
            "gaps: {:?}",
            flow.gaps
        );
    }

    #[test]
    fn a_second_token_account_for_the_mint_is_reported_incomplete_with_its_count() {
        // The creator holds a second account for this mint besides their
        // associated one: its history is never read, so the total may be
        // short, and that has to be said rather than reported as a clean
        // complete read.
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let mint_key = mint.to_string();
        let creator_key = creator.to_string();
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata, "SomeOtherTokenAccount11111111111111111111"]),
            signatures_page("dev-buy-sig"),
            creator_cash_flow_tx(&creator_key, &mint_key, 0, 1_000, 10_000, 9_000),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(!flow.trades_complete);
        assert!(
            flow.gaps.iter().any(|g| g.contains('1')),
            "gaps: {:?}",
            flow.gaps
        );
    }

    #[test]
    fn a_failed_check_of_other_token_accounts_is_reported_incomplete() {
        // `getTokenAccountsByOwner` itself fails: the reader cannot confirm
        // the ATA is the creator's only account for this mint, so the read
        // is incomplete even though the ATA's own history read cleanly.
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            r#"{"result":null,"error":{"message":"node is down"}}"#.to_owned(),
            r#"{"result":[],"error":null}"#.to_owned(),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(!flow.trades_complete);
        assert!(
            flow.gaps.iter().any(|g| g.contains("could not confirm")),
            "gaps: {:?}",
            flow.gaps
        );
    }

    #[test]
    fn more_signatures_than_the_remaining_budget_skips_the_fetch_entirely() {
        // Two signatures, but only one call left in the budget after the
        // reads already spent above: fetching would run out partway, so
        // nothing is fetched at all and the gap says why.
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            r#"{"result":[{"signature":"a","slot":1},{"signature":"b","slot":1}],"error":null}"#
                .to_owned(),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        // 3 calls total: owner_of + token_accounts_by_owner + the signature
        // page leave none for the two transactions this history has.
        let mut budget = Budget::new(3, 60, std::time::Duration::from_secs(30));
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(!flow.trades_complete);
        assert!(flow.trades.is_empty());
        assert!(
            flow.gaps.iter().any(|g| g.contains("calls left")),
            "gaps: {:?}",
            flow.gaps
        );
    }

    #[test]
    fn a_missing_transaction_answer_is_a_gap() {
        // `getTransaction` answers `{"result":null}`: the transaction is
        // simply not there for this reader, which must count against
        // completeness rather than being skipped silently.
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            signatures_page("missing-sig"),
            r#"{"result":null,"error":null}"#.to_owned(),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        // The client reports a null result as an unreadable response, so
        // the gap names the transaction rather than a fixed phrase.
        assert!(!flow.trades_complete);
        assert!(flow.trades.is_empty());
        assert!(
            flow.gaps.iter().any(|g| g.contains("missing-sig")),
            "gaps: {:?}",
            flow.gaps
        );
    }

    #[test]
    fn an_rpc_error_fetching_a_transaction_is_a_gap() {
        // `getTransaction` answers with a node error: the same completeness
        // rule applies as a missing result.
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            signatures_page("error-sig"),
            r#"{"result":null,"error":{"message":"rate limited"}}"#.to_owned(),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(!flow.trades_complete);
        assert!(flow.trades.is_empty());
        assert!(
            flow.gaps.iter().any(|g| g.contains("rate limited")),
            "gaps: {:?}",
            flow.gaps
        );
    }

    #[test]
    fn an_unchanged_token_balance_is_never_a_transfer_out() {
        // `classify_without_lamports`'s guard is `token_after < token_before`;
        // an unchanged balance must read `false` (`Ignored`), not `true`
        // under a `<=` swap (`TransferOut`).
        assert_eq!(
            classify_without_lamports(500, 500),
            SolanaCashFlowEvent::Ignored
        );
    }

    // -- has_readable_creator_balances -----------------------------------

    #[test]
    fn readable_balances_with_only_the_pre_side_mentioning_the_mint() {
        let mut tx = creator_tx("creator", "mint", 0, 0, 10_000, 9_999);
        tx.pre_token_balances = vec![token_balance(0, "mint", "someone-else", 500)];
        tx.post_token_balances = Vec::new();
        assert!(has_readable_creator_balances(&tx, "creator", "mint"));
    }

    #[test]
    fn readable_balances_with_only_the_post_side_mentioning_the_mint() {
        let mut tx = creator_tx("creator", "mint", 0, 0, 10_000, 9_999);
        tx.pre_token_balances = Vec::new();
        tx.post_token_balances = vec![token_balance(0, "mint", "someone-else", 500)];
        assert!(has_readable_creator_balances(&tx, "creator", "mint"));
    }

    #[test]
    fn unreadable_when_neither_side_mentions_the_mint() {
        let mut tx = creator_tx("creator", "mint", 0, 0, 10_000, 9_999);
        tx.pre_token_balances = Vec::new();
        tx.post_token_balances = Vec::new();
        assert!(!has_readable_creator_balances(&tx, "creator", "mint"));
    }

    #[test]
    fn unreadable_when_the_creators_lamport_balance_is_missing() {
        // The creator's own account key is absent, so there is no lamport
        // balance to read at either index -- unreadable even though the
        // mint is mentioned.
        let mut tx = creator_tx("someone-else", "mint", 0, 0, 1, 1);
        tx.accounts = vec!["someone-else".to_owned()];
        tx.pre_token_balances = vec![token_balance(0, "mint", "creator", 500)];
        tx.post_token_balances = Vec::new();
        assert!(!has_readable_creator_balances(&tx, "creator", "mint"));
    }

    #[test]
    fn unreadable_when_only_one_side_of_the_creators_lamports_is_present() {
        // A before balance with no after balance (or the reverse) cannot
        // price anything, so one side alone is not a readable balance.
        let mut tx = creator_tx("creator", "mint", 500, 0, 10_000, 9_999);
        tx.post_balances = Vec::new();
        assert!(!has_readable_creator_balances(&tx, "creator", "mint"));
        let mut tx = creator_tx("creator", "mint", 500, 0, 10_000, 9_999);
        tx.pre_balances = Vec::new();
        assert!(!has_readable_creator_balances(&tx, "creator", "mint"));
    }

    #[test]
    fn unreadable_when_only_a_different_mint_is_mentioned() {
        let mut tx = creator_tx("creator", "mint", 0, 0, 10_000, 9_999);
        tx.pre_token_balances = vec![token_balance(0, "othermint", "creator", 500)];
        tx.post_token_balances = vec![token_balance(1, "othermint", "creator", 0)];
        assert!(!has_readable_creator_balances(&tx, "creator", "mint"));
    }

    #[test]
    fn transfers_out_counts_every_one_not_just_one() {
        // `read_creator_trades`' `transfers_out += 1` must accumulate: two
        // unpriced transfer-outs in the history must read as 2, not 0 (a
        // `*=` swap would leave the counter at its zero start forever) or
        // some other wrong value (a `-=` swap).
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let mint_key = mint.to_string();
        let creator_key = creator.to_string();
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let sigs =
            r#"{"result":[{"signature":"t2","slot":2},{"signature":"t1","slot":1}],"error":null}"#
                .to_owned();
        // No trading instruction on either transaction: a token fall with
        // no matching SOL rise is an unpriced transfer out.
        let tx1 = format!(
            r#"{{"result":{{"slot":1,"meta":{{"err":null,"preBalances":[10000],"postBalances":[9995],"preTokenBalances":[{{"accountIndex":0,"mint":"{mint_key}","owner":"{creator_key}","uiTokenAmount":{{"amount":"500"}}}}],"postTokenBalances":[{{"accountIndex":0,"mint":"{mint_key}","owner":"{creator_key}","uiTokenAmount":{{"amount":"0"}}}}]}},"transaction":{{"message":{{"accountKeys":["{creator_key}"],"instructions":[]}}}}}},"error":null}}"#
        );
        let tx2 = format!(
            r#"{{"result":{{"slot":2,"meta":{{"err":null,"preBalances":[9995],"postBalances":[9990],"preTokenBalances":[{{"accountIndex":0,"mint":"{mint_key}","owner":"{creator_key}","uiTokenAmount":{{"amount":"300"}}}}],"postTokenBalances":[{{"accountIndex":0,"mint":"{mint_key}","owner":"{creator_key}","uiTokenAmount":{{"amount":"0"}}}}]}},"transaction":{{"message":{{"accountKeys":["{creator_key}"],"instructions":[]}}}}}},"error":null}}"#
        );
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            sigs,
            tx1,
            tx2,
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        let mut budget = solana_budget();
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(flow.trades_complete);
        assert!(flow.trades.is_empty());
        assert_eq!(flow.transfers_out, 2);
    }

    #[test]
    fn exactly_the_signature_cap_falls_through_to_the_next_check() {
        // `signatures.len() > CREATOR_CASH_FLOW_MAX_SIGNATURES` must read
        // `false` when the two are exactly equal (an `==`/`>=` swap would
        // read `true`): with no calls left for the transaction fetches that
        // would follow, the read must report the "calls left" gap, never
        // the "more than" cap gap, proving the cap branch was never taken.
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let entries: Vec<String> = (0..CREATOR_CASH_FLOW_MAX_SIGNATURES)
            .map(|i| format!(r#"{{"signature":"sig-{i}","slot":1}}"#))
            .collect();
        assert_eq!(entries.len(), CREATOR_CASH_FLOW_MAX_SIGNATURES);
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            format!(r#"{{"result":[{}],"error":null}}"#, entries.join(",")),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        // 3 calls total (owner_of + token_accounts_by_owner + the signature
        // page) leave none for any transaction fetch.
        let mut budget = Budget::new(3, 60, std::time::Duration::from_secs(30));
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(!flow.trades_complete);
        assert!(
            flow.gaps.iter().any(|g| g.contains("calls left")),
            "an exact-cap signature count must fall through to the calls-left check, not the \
             cap check: {:?}",
            flow.gaps
        );
        assert!(
            !flow.gaps.iter().any(|g| g.contains("more than")),
            "the cap branch must not have been taken: {:?}",
            flow.gaps
        );
    }

    #[test]
    fn a_signature_count_exactly_equal_to_calls_left_is_still_read() {
        // `signatures.len() > budget.calls_left()` must read `false` when
        // the two are exactly equal (a `>=` swap would read `true` and skip
        // the fetch that should have happened).
        let mint = solana_addr(9);
        let creator = solana_addr(1);
        let mint_key = mint.to_string();
        let creator_key = creator.to_string();
        let ata_owner = realorrug_pumpfun::token::TokenProgram::Spl.id().to_string();
        let ata = spl_ata(creator, mint);
        let responses = [
            format!(r#"{{"result":{{"value":{{"data":[],"owner":"{ata_owner}"}}}},"error":null}}"#),
            token_accounts_response(&[&ata]),
            signatures_page("dev-buy-sig"),
            creator_cash_flow_tx(&creator_key, &mint_key, 0, 1_000, 10_000, 9_000),
        ];
        let refs: Vec<&str> = responses.iter().map(String::as_str).collect();
        let client = RpcClient::with_transport("http://test.invalid", Canned::boxed(&refs));
        // 3 calls consumed by setup (owner_of + token_accounts_by_owner +
        // the signature page) leave exactly 1 -- the same as the single
        // signature this history has.
        let mut budget = Budget::new(4, 60, std::time::Duration::from_secs(30));
        let flow =
            creator_cash_flow_solana(&client, &mut budget, &mint, &creator).expect("a result");

        assert!(
            flow.trades_complete,
            "an exactly-equal signature count must still be fetched, not skipped: {:?}",
            flow.gaps
        );
        assert_eq!(flow.trades.len(), 1);
    }
}

#[cfg(test)]
mod creator_cash_flow_tests {
    use realorrug_robinhood::pons::CreatorRole;

    use super::*;

    fn addr(b: u8) -> Address {
        Address([b; 20])
    }

    fn record(deployer: u8, fee_recipient: u8, curve: u8) -> LaunchedToken {
        LaunchedToken {
            token: addr(0xee),
            curve: addr(curve),
            deployer: addr(deployer),
            creator_fee_recipient: addr(fee_recipient),
            pair: None,
            graduation_threshold: 0,
            creator_tax_bps: 0,
            buyback: false,
            phase: 0,
            exists: true,
        }
    }

    fn word_addr(b: u8) -> Hash32 {
        let mut w = [0u8; 32];
        w[12..].copy_from_slice(&[b; 20]);
        Hash32(w)
    }

    fn word_u128(v: u128) -> [u8; 32] {
        let mut w = [0u8; 32];
        w[16..].copy_from_slice(&v.to_be_bytes());
        w
    }

    /// A `CurveBuy`/`CurveSell` log, curve as emitter, `trader`/`recipient`
    /// as topics 1/2, `(quote, tokens, fee, tax)` as the data words in the
    /// order [`Trade::from_log`] reads them for the given side.
    fn trade_log(
        sell: bool,
        curve: u8,
        trader: u8,
        recipient: u8,
        quote: u128,
        tokens: u128,
    ) -> Log {
        let mut data = Vec::new();
        let (first, second) = if sell {
            (tokens, quote)
        } else {
            (quote, tokens)
        };
        for v in [first, second, 0, 0] {
            data.extend(word_u128(v));
        }
        Log {
            address: addr(curve),
            topics: vec![
                if sell {
                    topic::CURVE_SELL
                } else {
                    topic::CURVE_BUY
                },
                word_addr(trader),
                word_addr(recipient),
            ],
            data,
            block: 1,
            transaction: Hash32([1; 32]),
            position: None,
        }
    }

    fn transfer_log(token: u8, from: u8, to: u8, amount: u128) -> Log {
        Log {
            address: addr(token),
            topics: vec![topic::TRANSFER, word_addr(from), word_addr(to)],
            data: word_u128(amount).to_vec(),
            block: 1,
            transaction: Hash32([2; 32]),
            position: None,
        }
    }

    /// Done criterion (a): a fixture where the fee recipient differs from
    /// the deployer shows both, each recognised on its own log.
    #[test]
    fn a_sale_by_either_the_deployer_or_the_fee_recipient_is_classified_by_its_own_role() {
        let record = record(0xaa, 0xbb, 0xcc);
        let logs = vec![
            trade_log(true, 0xcc, 0x22, 0xaa, 100, 40), // deployer sells
            trade_log(true, 0xcc, 0x22, 0xbb, 50, 20),  // fee recipient sells
            trade_log(true, 0xcc, 0x22, 0xdd, 10, 5),   // a stranger sells: not creator cash flow
        ];
        let trades = classify_creator_trades(&logs, &record);
        assert_eq!(trades.len(), 2, "the stranger's sale is not the creator's");
        assert_eq!(trades[0].role, CreatorRole::Deployer);
        assert_eq!(trades[0].quote, 100);
        assert_eq!(trades[1].role, CreatorRole::FeeRecipient);
        assert_eq!(trades[1].quote, 50);
    }

    /// Done criterion (b): a plain token transfer out of the creator is
    /// never counted among the sales, and the reverse -- a transfer landing
    /// on the curve, the token leg of an already-decoded sale -- is not
    /// double-counted as an extra unexplained transfer.
    #[test]
    fn a_plain_transfer_out_is_counted_separately_from_a_decoded_sale_and_never_as_one() {
        let record = record(0xaa, 0xbb, 0xcc);
        let sale = trade_log(true, 0xcc, 0x22, 0xaa, 100, 40);
        let trades = classify_creator_trades(&[sale], &record);
        assert_eq!(trades.len(), 1);

        let logs = vec![
            transfer_log(0xee, 0xaa, 0xcc, 40), // the sale's own token leg, to the curve
            transfer_log(0xee, 0xaa, 0xff, 15), // a real transfer out, not a sale
            transfer_log(0xee, 0xff, 0xaa, 5),  // inbound: not an outgoing count
            transfer_log(0xee, 0xbb, 0xdd, 7),  // the fee recipient's own transfer out
        ];
        let out = count_transfers_out(&logs, &record, &record.curve);
        assert_eq!(
            out, 2,
            "the curve-bound leg of the sale must not also count as a transfer"
        );
    }

    /// Done criterion (c): when either read behind a cash flow is
    /// incomplete, `trades_complete` is false and every priced accessor
    /// answers `None` rather than a number built from a partial history.
    #[test]
    fn an_incomplete_history_prints_no_profit_number() {
        let complete = CreatorCashFlow {
            trades: vec![
                CreatorTrade {
                    role: CreatorRole::Deployer,
                    side: Side::Sell,
                    quote: 100,
                    tokens: 40,
                    block: 1,
                    transaction: Hash32([1; 32]).to_string(),
                    unique_id: "0x01-0-0".to_owned(),
                },
                CreatorTrade {
                    role: CreatorRole::Deployer,
                    side: Side::Buy,
                    quote: 30,
                    tokens: 50,
                    block: 1,
                    transaction: Hash32([3; 32]).to_string(),
                    unique_id: "0x03-0-0".to_owned(),
                },
            ],
            transfers_out: 0,
            trades_complete: true,
            gaps: Vec::new(),
            quote_asset: crate::dossier::QuoteAsset::eth(),
        };
        assert_eq!(complete.proceeds_wei(), Some(100));
        assert_eq!(complete.cost_basis_wei(), Some(30));
        assert_eq!(complete.net_wei(), Some(70), "net is proceeds minus cost");

        let mut incomplete = complete.clone();
        incomplete.trades_complete = false;
        assert_eq!(incomplete.proceeds_wei(), None);
        assert_eq!(incomplete.cost_basis_wei(), None);
        assert_eq!(incomplete.net_wei(), None);
    }

    /// Rule (c) needs *both* reads: when the trade read succeeds but the
    /// budget cannot afford the transfer read, the history is still
    /// incomplete and no ETH number may come out of it.
    #[test]
    fn one_failed_read_of_two_leaves_the_history_incomplete() {
        let record = record(0xaa, 0xbb, 0xcc);
        let client = Rpc::new(crate::robinhood::tests::serve(vec![
            r#"{"jsonrpc":"2.0","id":1,"result":[]}"#.to_owned(),
        ]));
        let mut budget =
            Budget::with_compute_units(60, 60, std::time::Duration::from_secs(30), CU_GET_LOGS);
        let flow = creator_cash_flow(&client, &mut budget, &record, 1, 10);
        assert!(!flow.trades_complete, "gaps: {:?}", flow.gaps);
        assert_eq!(flow.proceeds_wei(), None);
        assert_eq!(flow.gaps.len(), 1, "gaps: {:?}", flow.gaps);
        assert!(flow.gaps[0].starts_with("creator transfer history"));
    }

    /// A `CurveBuy`/`CurveSell` log as an `eth_getLogs` result entry.
    fn trade_json(
        sell: bool,
        trader: u8,
        quote: u128,
        tokens: u128,
        block: u64,
        index: u8,
    ) -> serde_json::Value {
        let log = trade_log(sell, 0xcc, trader, trader, quote, tokens);
        serde_json::json!({
            "address": log.address.to_string(),
            "topics": log.topics.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "data": realorrug_robinhood::to_hex(&log.data),
            "blockNumber": format!("{block:#x}"),
            "transactionHash": Hash32([index; 32]).to_string(),
            "transactionIndex": "0x0",
            "logIndex": format!("{index:#x}"),
        })
    }

    fn answer(result: &serde_json::Value) -> String {
        serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": result }).to_string()
    }

    fn block_answer(number: u64, timestamp: u64) -> String {
        answer(&serde_json::json!({
            "number": format!("{number:#x}"),
            "hash": Hash32([0xbb; 32]).to_string(),
            "timestamp": format!("{timestamp:#x}"),
        }))
    }

    fn s7_budget() -> Budget {
        Budget::with_compute_units(60, 60, std::time::Duration::from_secs(30), 10_000)
    }

    /// Research 0052 §3.1's S7 case, captured as one curve read: three
    /// wallets bought matched sizes inside one link window, then sold inside
    /// one 50-block window; a fourth, unlinked wallet sold in the same
    /// window and is not counted.
    #[test]
    fn a_captured_sell_cluster_of_three_linked_wallets_reads_its_size_share_and_spread() {
        let logs = serde_json::Value::Array(vec![
            trade_json(false, 0x11, 100, 400, 10, 0),
            trade_json(false, 0x22, 100, 300, 10, 1),
            trade_json(false, 0x33, 100, 300, 12, 2),
            trade_json(false, 0x44, 5, 50, 500, 3),
            trade_json(true, 0x11, 90, 400, 600, 4),
            trade_json(true, 0x44, 4, 50, 610, 5),
            trade_json(true, 0x22, 70, 300, 620, 6),
            trade_json(true, 0x33, 70, 300, 650, 7),
        ]);
        let client = Rpc::new(crate::robinhood::tests::serve(vec![
            answer(&logs),
            block_answer(600, 1_000),
            block_answer(650, 1_600),
        ]));
        let read = correlated_selling(
            &client,
            &mut s7_budget(),
            &record(0xaa, 0xbb, 0xcc),
            1,
            700,
            Some(10_000),
        );
        assert_eq!(
            read,
            CorrelatedSelling {
                linked_sellers: 3,
                sold_bps_of_supply: Some(1_000),
                spread_seconds: Some(600),
                sells_read: true,
            }
        );
    }

    /// Rule 8: a read the provider refuses is unknown, never "no
    /// correlated selling".
    #[test]
    fn a_refused_curve_read_is_unread_not_zero() {
        let client = Rpc::new(crate::robinhood::tests::serve(vec![
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"no"}}"#.to_owned(),
        ]));
        let read = correlated_selling(
            &client,
            &mut s7_budget(),
            &record(0xaa, 0xbb, 0xcc),
            1,
            700,
            Some(10_000),
        );
        assert!(!read.sells_read);
    }

    /// Sells in one block spread over zero seconds, measured without a
    /// timestamp read (the mock has none to give); zero supply gives no
    /// share rather than a division by zero.
    #[test]
    fn a_same_block_cluster_spreads_over_zero_seconds_and_zero_supply_gives_no_share() {
        let logs = serde_json::Value::Array(vec![
            trade_json(false, 0x11, 100, 400, 10, 0),
            trade_json(false, 0x22, 100, 300, 10, 1),
            trade_json(true, 0x11, 90, 400, 600, 2),
            trade_json(true, 0x22, 70, 300, 600, 3),
        ]);
        let client = Rpc::new(crate::robinhood::tests::serve(vec![answer(&logs)]));
        let read = correlated_selling(
            &client,
            &mut s7_budget(),
            &record(0xaa, 0xbb, 0xcc),
            1,
            700,
            Some(0),
        );
        assert_eq!(read.linked_sellers, 2, "same-block sellers join each other");
        assert_eq!(read.spread_seconds, Some(0));
        assert_eq!(read.sold_bps_of_supply, None);
    }

    /// A curve that was read but has no sells is read, with nothing to
    /// report -- not unread, which the sheet would count as a gap.
    #[test]
    fn a_read_curve_with_no_sells_is_read_and_reports_no_cluster() {
        let logs = serde_json::Value::Array(vec![
            trade_json(false, 0x11, 100, 400, 10, 0),
            trade_json(false, 0x22, 100, 300, 10, 1),
        ]);
        let client = Rpc::new(crate::robinhood::tests::serve(vec![answer(&logs)]));
        let read = correlated_selling(
            &client,
            &mut s7_budget(),
            &record(0xaa, 0xbb, 0xcc),
            1,
            700,
            Some(10_000),
        );
        assert!(read.sells_read);
        assert_eq!(read.linked_sellers, 0);
    }

    /// Only sells, and only on this token's own curve, are sales: a buy on
    /// the same curve and a sell on another curve are both left out.
    #[test]
    fn sells_from_keeps_only_this_curves_sells() {
        let logs = vec![
            trade_log(true, 0xcc, 0x11, 0x11, 90, 400),
            trade_log(false, 0xcc, 0x22, 0x22, 100, 300),
            trade_log(true, 0xdd, 0x33, 0x33, 70, 300),
        ];
        let sales = sells_from(&logs, &addr(0xcc));
        assert_eq!(sales.len(), 1);
        assert_eq!(sales[0].seller, addr(0x11));
    }
}

#[cfg(test)]
mod link_confidence_tests {
    use super::*;

    #[test]
    fn link_confidence_takes_strongest_not_sum() {
        // Transfer (9,000) plus same block + fresh (5,000): the strongest
        // alone, 9,000, never 16,000 or any other sum.
        let evidence = LinkEvidence {
            transfer_or_declared_together: true,
            same_block: true,
            both_fresh: true,
            ..LinkEvidence::default()
        };
        assert_eq!(link_confidence(evidence), 9_000);
    }

    #[test]
    fn sizes_within_ten_percent_is_exact_near_u128_max() {
        assert!(sizes_within_ten_percent(
            u128::MAX,
            u128::MAX - u128::MAX / 10
        ));
        assert!(!sizes_within_ten_percent(u128::MAX, u128::MAX / 2));
    }

    #[test]
    fn same_block_or_fresh_alone_is_not_a_link() {
        // Both are needed: a shared block is common in a busy launch, and
        // two fresh wallets on different blocks are just two new buyers.
        let block_only = LinkEvidence {
            same_block: true,
            ..LinkEvidence::default()
        };
        let fresh_only = LinkEvidence {
            both_fresh: true,
            ..LinkEvidence::default()
        };
        assert_eq!(link_confidence(block_only), 0);
        assert_eq!(link_confidence(fresh_only), 0);
    }

    #[test]
    fn no_evidence_gives_zero() {
        assert_eq!(link_confidence(LinkEvidence::default()), 0);
    }

    #[test]
    fn transfer_or_declared_together_gives_9000() {
        let evidence = LinkEvidence {
            transfer_or_declared_together: true,
            ..LinkEvidence::default()
        };
        assert_eq!(link_confidence(evidence), 9_000);
    }

    #[test]
    fn same_block_fresh_and_matched_sizes_gives_7000() {
        let evidence = LinkEvidence {
            same_block: true,
            both_fresh: true,
            sizes_within_10_percent: true,
            ..LinkEvidence::default()
        };
        assert_eq!(link_confidence(evidence), 7_000);
    }

    #[test]
    fn same_block_and_fresh_alone_gives_5000() {
        let evidence = LinkEvidence {
            same_block: true,
            both_fresh: true,
            ..LinkEvidence::default()
        };
        assert_eq!(link_confidence(evidence), 5_000);
    }

    #[test]
    fn same_window_and_matched_sizes_gives_4000() {
        let evidence = LinkEvidence {
            same_window: true,
            sizes_within_10_percent: true,
            ..LinkEvidence::default()
        };
        assert_eq!(link_confidence(evidence), 4_000);
    }

    #[test]
    fn same_window_alone_gives_2000() {
        let evidence = LinkEvidence {
            same_window: true,
            ..LinkEvidence::default()
        };
        assert_eq!(link_confidence(evidence), 2_000);
    }

    #[test]
    fn eth_funding_one_hop_gives_8000() {
        // Not readable today (no caller in this crate can set this field),
        // but the evidence type and its weight are reproduced from the
        // table regardless, so the day the read exists nothing here changes.
        let evidence = LinkEvidence {
            eth_funding_one_hop: true,
            ..LinkEvidence::default()
        };
        assert_eq!(link_confidence(evidence), 8_000);
    }

    #[test]
    fn correlated_sell_confirms_an_existing_link_but_is_never_the_only_one() {
        // Alone, correlated selling proves nothing about identity -- the
        // table marks it "never the only link" -- so it must not lift
        // confidence off zero.
        let alone = LinkEvidence {
            correlated_sell: true,
            ..LinkEvidence::default()
        };
        assert_eq!(link_confidence(alone), 0);

        // Alongside a weaker link (same window only, 2,000), it confirms and
        // raises to its own 3,000.
        let confirming = LinkEvidence {
            same_window: true,
            correlated_sell: true,
            ..LinkEvidence::default()
        };
        assert_eq!(link_confidence(confirming), 3_000);

        // Alongside a stronger link, it never lowers what is already there.
        let strong = LinkEvidence {
            transfer_or_declared_together: true,
            correlated_sell: true,
            ..LinkEvidence::default()
        };
        assert_eq!(link_confidence(strong), 9_000);
    }

    #[test]
    fn sizes_within_ten_percent_boundary_is_closed_at_exactly_10_percent() {
        // 90 vs 100 is exactly 10% apart: within.
        assert!(sizes_within_ten_percent(90, 100));
        assert!(sizes_within_ten_percent(100, 90), "order must not matter");
    }

    #[test]
    fn sizes_within_ten_percent_boundary_excludes_just_over_10_percent() {
        // 89 vs 100 is just over 10% apart: not within.
        assert!(!sizes_within_ten_percent(89, 100));
        assert!(!sizes_within_ten_percent(100, 89), "order must not matter");
    }

    #[test]
    fn sizes_within_ten_percent_treats_two_zero_buys_as_matched() {
        assert!(sizes_within_ten_percent(0, 0));
    }
}
