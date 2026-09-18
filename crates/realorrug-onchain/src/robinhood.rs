// SPDX-License-Identifier: Apache-2.0
//! The second [`ChainReader`], testing ADR 0028 point 2's claim that adding a
//! chain costs one implementation against the shared [`Dossier`].
//!
//! # What this crate reads that `realorrug-robinhood` does not
//!
//! `realorrug-robinhood` decodes Robinhood Chain's raw JSON-RPC shapes --
//! receipts, logs, the factory's `getLaunchedToken` return, the curve's
//! selectors. It holds no key and knows nothing of a [`Dossier`]. This module
//! is the assembler: it spends a [`Budget`], calls that crate's `Rpc`, and
//! turns the answers into the same fact shape [`crate::build`] produces for
//! Solana. Design 0020 §6's crate-boundary section is why the split is drawn
//! here rather than inside `realorrug-robinhood` itself -- `realorrug-types`
//! depends on `realorrug-robinhood` for `ChainAddress::Robinhood`, which makes
//! `realorrug-robinhood` the leaf of the workspace, and a trait defined above
//! it (this crate's `ChainReader`) cannot be implemented inside it without a
//! dependency cycle.
//!
//! # What this dossier holds, and what it does not
//!
//! Only what the reads below can actually support today: the launch record,
//! the curve's graduation flag and quote reserves, the current block as the
//! read point, the launch block with its age and the launcher's own buy, and
//! the holders. Everything Pons v2's curve arithmetic has not been modelled
//! for -- capacity, fees, a real creator-transaction count -- is `None` with an
//! [`Unavailable`] entry naming it, never a default (AGENTS.md §3 rule 8). This
//! reader does not guess at them to look more complete than it is.

use std::collections::HashMap;

use realorrug_robinhood::erc20;
use realorrug_robinhood::pons::{FACTORY, Launched, LaunchedToken, Side, Trade, curve, topic};
use realorrug_robinhood::{Address as RobinhoodAddress, Hash32, Log, LogsError, Rpc};
use realorrug_types::{ChainAddress, ReadAt};

use crate::budget::{Budget, Exhausted};
use crate::dossier::{
    ChainLaunch, ChainReader, CurveFacts, Dossier, Holders, QuoteAsset, Unavailable,
};

/// Why a Robinhood dossier could not be built at all.
///
/// Kept separate from [`Dossier::unavailable`] the same way Solana's
/// [`crate::rpc::RpcError`] is: this is for the one read the dossier cannot
/// exist without -- the factory's own record of the token -- everything else
/// that fails lands as an [`Unavailable`] entry on a dossier that is still
/// returned.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The budget ran out before the factory could even be asked.
    #[error("budget exhausted before the launch record could be read: {0:?}")]
    Budget(Exhausted),
    /// The endpoint's own error, or a return that did not parse.
    #[error("{0}")]
    Rpc(String),
    /// The factory answered, but has no record of this token.
    #[error("the factory has no record of this token")]
    NotLaunched,
}

/// One `eth_call`, budget-checked first.
///
/// Every read in this module takes [`take`] first, so that none of them can
/// skip the budget -- a budget checked only at the start is not a budget.
fn call(
    budget: &mut Budget,
    client: &Rpc,
    to: &RobinhoodAddress,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    take(budget)?;
    client.call_contract(to, data)
}

/// One call's worth of budget, or why there is none.
fn take(budget: &mut Budget) -> Result<(), String> {
    budget
        .take_call()
        .map_err(|e| format!("budget exhausted: {e:?}"))
}

/// A block's number and timestamp (the latest when `number` is `None`),
/// budget-checked.
fn block_time(
    budget: &mut Budget,
    client: &Rpc,
    number: Option<u64>,
) -> Result<(u64, u64), String> {
    take(budget)?;
    client.block_time(number)
}

/// An address as the 32-byte topic an indexed `address` parameter becomes.
fn address_topic(address: &RobinhoodAddress) -> Hash32 {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(&address.0);
    Hash32(word)
}

/// The launch: its block, its age at the read point, and the launcher's own
/// buy in the launch transaction.
///
/// The block comes from the factory's `TokenLaunched` event filtered on the
/// token as its first indexed field -- one log in the whole chain, so the
/// provider's block-range cap never bites. The age is the difference of the
/// two blocks' own timestamps: the chain's clock, not this server's. The buy
/// is read from the launch transaction's receipt, because research 0036 §3
/// captured a launch carrying the launcher's buy in the same transaction.
///
/// Only the block is required. An unreadable timestamp leaves the age `None`
/// and an unreadable receipt leaves the buy `None`, each "could not see", and
/// the launch is still returned: dropping a block that did read because a
/// second read failed would report less than was known.
fn launch_facts(
    budget: &mut Budget,
    client: &Rpc,
    token: &RobinhoodAddress,
    record: &LaunchedToken,
    read_time: Option<u64>,
) -> Result<ChainLaunch, String> {
    take(budget)?;
    let logs = client.logs(&FACTORY, &[topic::TOKEN_LAUNCHED, address_topic(token)])?;
    let log = logs
        .iter()
        .find(|log| Launched::from_log(log).is_some_and(|l| l.token == *token))
        .ok_or_else(|| "the factory emitted no TokenLaunched event for this token".to_owned())?;

    let age_seconds = block_time(budget, client, Some(log.block))
        .ok()
        .zip(read_time)
        .and_then(|((_, launched_at), read_at)| read_at.checked_sub(launched_at));

    let dev_buy_wei = take(budget)
        .and_then(|()| client.receipt(&log.transaction))
        .ok()
        .flatten()
        .map(|receipt| {
            // A reverted transaction's logs never happened.
            if receipt.succeeded {
                launcher_buy(&receipt.logs, record)
            } else {
                0
            }
        });

    Ok(ChainLaunch {
        block: log.block,
        age_seconds,
        dev_buy_wei,
        name: text_field(budget, client, token, erc20::NAME),
        symbol: text_field(budget, client, token, erc20::SYMBOL),
    })
}

/// One ERC-20 string field of `token`, or `None` for every way that can fail.
///
/// Two calls out of the budget's sixty, spent on the only fact that says which
/// token this is. Both are allowed to fail quietly, unlike the reads
/// [`launch_facts`] makes before them: a launch whose event and receipt read is
/// still worth returning without a name, while refusing the whole dossier
/// because a name did not read would report less than was known (rule 8 cuts
/// both ways -- an unread name is not a reason to drop the age and the dev buy
/// that did read).
fn text_field(
    budget: &mut Budget,
    client: &Rpc,
    token: &RobinhoodAddress,
    selector: [u8; 4],
) -> Option<String> {
    call(budget, client, token, &selector)
        .ok()
        .as_deref()
        .and_then(erc20::string_from_return)
}

/// Wei the launcher spent on curve buys among `logs`: buys against this
/// token's own curve, made by or for the deployer.
///
/// The curve is compared because any contract can emit a `CurveBuy`'s bytes
/// (`Trade::from_log`'s own doc comment says so). Saturating, because a sum
/// past `u128` is not a real ETH amount and must not wrap into a small one.
fn launcher_buy(logs: &[Log], record: &LaunchedToken) -> u128 {
    logs.iter()
        .filter_map(Trade::from_log)
        .filter(|t| {
            t.side == Side::Buy
                && t.curve == record.curve
                && (t.trader == record.deployer || t.recipient == record.deployer)
        })
        .fold(0u128, |sum, t| sum.saturating_add(t.quote))
}

/// Who holds the token, from every `Transfer` it emitted.
///
/// Logs arrive in chain order, so a balance cannot truly go below zero; one
/// that would means the answer was incomplete, and the count is refused
/// rather than published from a partial ledger. The curve, the factory and
/// the zero address are machinery, not holders.
fn holders_from(logs: &[Log], record: &LaunchedToken) -> Result<Holders, String> {
    let mut balances: HashMap<RobinhoodAddress, u128> = HashMap::new();
    for log in logs {
        if log.topics.first() != Some(&topic::TRANSFER) {
            continue;
        }
        let (Some(from), Some(to), Some(value)) =
            (log.topic_address(1), log.topic_address(2), log.data_u128(0))
        else {
            return Err("a Transfer log did not decode".to_owned());
        };
        if from != RobinhoodAddress::ZERO {
            let balance = balances.entry(from).or_default();
            *balance = balance
                .checked_sub(value)
                .ok_or_else(|| "transfers spend more than was received".to_owned())?;
        }
        let balance = balances.entry(to).or_default();
        *balance = balance.saturating_add(value);
    }
    let held: Vec<u128> = balances
        .iter()
        .filter(|(who, balance)| {
            **balance > 0
                && **who != RobinhoodAddress::ZERO
                && **who != record.curve
                && **who != FACTORY
        })
        .map(|(_, balance)| *balance)
        .collect();
    let count = u32::try_from(held.len()).unwrap_or(u32::MAX);
    let total = held.iter().fold(0u128, |sum, b| sum.saturating_add(*b));
    let largest_share_bps = held.iter().max().and_then(|largest| {
        // Scale first while it fits; a balance near `u128::MAX` divides the
        // total first instead, which loses precision but never overflows.
        let bps = largest.checked_mul(10_000).map_or_else(
            || largest / (total / 10_000).max(1),
            |scaled| scaled / total,
        );
        u16::try_from(bps.min(10_000)).ok()
    });
    Ok(Holders {
        count,
        largest_share_bps,
    })
}

/// Why [`holders_paged`] gives up rather than keep walking.
///
/// Named as one constant so the reason a caller sees for "too busy" is the
/// same string this module documents and tests, rather than one composed
/// ad hoc at each of the two places the walk can stop early (the page cap,
/// and a single block that alone exceeds the provider's cap).
const TOO_BUSY: &str = "too many transfers to read within this answer's budget";

/// The largest share of a token's remaining `calls_left` that one dossier's
/// holder-log paging may spend, on top of the flat ceiling below.
///
/// Not the only ceiling: see [`MAX_HOLDER_PAGES`]'s own comment for why a
/// second, budget-independent cap is also needed.
const HOLDER_PAGE_BUDGET_DIVISOR: u32 = 2;

/// The largest number of pages [`holders_paged`] may fetch for one token,
/// regardless of how much budget is left.
///
/// Sized from a measurement rather than an estimate. The estimate this
/// constant first carried -- 3-5 pages, from
/// `docs/research/0050-robinhood-trader-signals.md` §0 -- was wrong by a
/// factor of three. Walked in full on 2026-09-17, the live token
/// `0x13e6cdB0470B10AfCB96177Ae8702ace2ac72cD6` held **28,652** transfers
/// across the ~175,000 blocks between its launch and the read, and the walk
/// below reached all of them in **12 requests** (8 pages plus 4 halving
/// retries) taking 7.1 seconds. Twelve was therefore not headroom over the
/// need; it was exactly the need, and the walk stopped one page short of an
/// answer on the very token Josh asked about. Twenty-four is double the
/// measurement, still well under [`crate::budget::DEFAULT_MAX_CALLS`] (60),
/// and in practice the budget's 20-second deadline stops a pathological token
/// long before this does. The actual cap used is the smaller of this and half
/// the calls the budget passed in today has left when the walk starts (see
/// [`HOLDER_PAGE_BUDGET_DIVISOR`]), so a budget constructed smaller than the
/// default cannot be emptied by holder paging alone either.
const MAX_HOLDER_PAGES: u32 = 24;

/// How wide [`holders_paged`]'s first window is, in blocks.
///
/// Deliberately *not* the whole range. Asking for the whole range first reads
/// as the cheap thing to do -- one call answers a quiet token -- but measured
/// on the token above it cost four requests and three seconds of pure refusal
/// before the first page came back, because a busy token's full span never
/// fits. Starting here and doubling after each success read the same ledger in
/// 12 requests and 7.1s against 16 and 10.3s for the full-span start. A quiet
/// token pays almost nothing for the change: the window doubles past a whole
/// day of this chain within five calls, and a token whose entire span is
/// narrower than this still takes exactly one call, because the end of each
/// window is clamped to the block being read at.
///
/// Twenty thousand blocks is roughly half an hour at the ~0.1s blocks measured
/// on this chain -- comfortably inside every provider's block-range limit
/// (Alchemy's is 5,000 blocks *or* 10,000 logs, and this walk relies on the
/// second), while wide enough that a normal token finishes in a handful of
/// calls.
const FIRST_HOLDER_WINDOW: u64 = 20_000;

/// The widest window the doubling will reach for.
///
/// A ceiling on growth only, so a chain quieter than this one cannot send the
/// window climbing into ranges a provider refuses on block count alone rather
/// than on log count. The provider's own cap is what normally stops the
/// growth; this stops it when nothing else would.
const MAX_HOLDER_WINDOW: u64 = 200_000;

/// How many pages [`holders_paged`] may spend on `budget`, chosen from what
/// it has left right now rather than from a constant alone.
fn holder_page_cap(budget: &Budget) -> u32 {
    (budget.calls_left() / HOLDER_PAGE_BUDGET_DIVISOR).clamp(1, MAX_HOLDER_PAGES)
}

/// Every holder-relevant `Transfer` `token` emitted between `from_block` and
/// `to_block`, read a window at a time so a token busy enough to clear the
/// provider's per-call result cap (10,000 logs, `Rpc::logs_range`'s own doc
/// comment) is still readable rather than refused outright.
///
/// Starts at [`FIRST_HOLDER_WINDOW`] blocks (or the whole range, when that is
/// narrower), halves and retries from the same block whenever the provider
/// reports its cap, and **doubles after every success** up to
/// [`MAX_HOLDER_WINDOW`].
///
/// The doubling is the part that is easy to leave out and expensive to leave
/// out. A walk that only ever halves keeps, for the whole rest of the token's
/// history, whatever narrow window the busiest minute of its launch forced --
/// and a launch burst is precisely the part of a token's life that is not
/// representative of the rest. Measured on the live token in
/// [`MAX_HOLDER_PAGES`]'s comment, holding the narrow window cost 38 requests
/// where growing it back cost 12, for the identical answer.
///
/// # Errors
///
/// The endpoint's error, a log that did not parse, or -- when the page cap
/// or a single block alone would need more requests than are allowed --
/// [`TOO_BUSY`]. Never a partial count: every error here is returned before
/// [`holders_from`] runs, so a caller that stops early gets nothing rather
/// than a truncated ledger that would misreport a real balance as zero
/// (AGENTS.md §3 rule 8).
fn holders_paged(
    budget: &mut Budget,
    client: &Rpc,
    token: &RobinhoodAddress,
    from_block: u64,
    to_block: u64,
    record: &LaunchedToken,
) -> Result<Holders, String> {
    let cap = holder_page_cap(budget);
    let mut logs: Vec<Log> = Vec::new();
    let mut start = from_block;
    // Not clamped to the range: `end` below is already clamped to `to_block`,
    // so a token whose whole life is narrower than one window still costs the
    // single request it always did.
    let mut window = FIRST_HOLDER_WINDOW;
    let mut pages = 0u32;
    while start <= to_block {
        if pages >= cap {
            return Err(TOO_BUSY.to_owned());
        }
        take(budget)?;
        pages += 1;
        let end = start.saturating_add(window.saturating_sub(1)).min(to_block);
        match client.logs_range(token, &[topic::TRANSFER], start, end) {
            Ok(page) => {
                logs.extend(page);
                start = end.saturating_add(1);
                window = window.saturating_mul(2).min(MAX_HOLDER_WINDOW);
            }
            Err(LogsError::TooManyResults) => {
                if window <= 1 {
                    return Err(TOO_BUSY.to_owned());
                }
                window = window.div_ceil(2);
            }
            Err(LogsError::Other(e)) => return Err(e),
        }
    }
    holders_from(&logs, record)
}

/// The factory's record of `token`, the one read a Robinhood dossier cannot
/// be built without.
fn launched_token(
    budget: &mut Budget,
    client: &Rpc,
    token: &RobinhoodAddress,
) -> Result<LaunchedToken, Error> {
    let data =
        call(budget, client, &FACTORY, &LaunchedToken::call_data(token)).map_err(Error::Rpc)?;
    LaunchedToken::from_return(&data)
        .ok_or_else(|| Error::Rpc("getLaunchedToken: malformed return".to_owned()))
}

/// The curve's graduation flag and quote reserves.
///
/// **`complete` is read from the curve's own `graduated()`, not the factory's
/// `phase == 2` on the same [`LaunchedToken`] record already in hand.** Both
/// are the same fact from opposite sides of the same launch (research 0040
/// §1's phase, `pons::curve::GRADUATED`'s own getter), so only one is read: the
/// curve is asked because [`quote_reserves`] below is already a curve-side
/// call in the same read, and asking the curve for both keeps the two figures
/// from ever being read a block apart.
///
/// `quote_reserves` is `curve::REAL_QUOTE_RESERVE`, the quote the curve
/// actually holds -- not `curve::QUOTE_RESERVE`, which research 0040 §3 notes
/// includes a phantom amount the curve does not hold. `real_sol_reserves` is
/// the field Solana's own [`CurveFacts::quote_reserves`] doc comment names as
/// the equivalent fact, so this is the one that keeps the same name meaning
/// the same thing on both chains.
fn curve_facts(
    budget: &mut Budget,
    client: &Rpc,
    record: &LaunchedToken,
) -> Result<CurveFacts, String> {
    let graduated_data = call(
        budget,
        client,
        &record.curve,
        &curve::call_data(curve::GRADUATED),
    )?;
    let complete = curve::bool_return(&graduated_data)
        .ok_or_else(|| "graduated(): malformed return".to_owned())?;

    let reserves_data = call(
        budget,
        client,
        &record.curve,
        &curve::call_data(curve::REAL_QUOTE_RESERVE),
    )?;
    // `u128`, not `u64`: a curve holding more than about 18.4 ETH in wei does
    // not fit a `u64`, and that is every token that raised real money, not an
    // exotic case. Nothing downstream does arithmetic that can overflow a
    // `u128`; the sheet only ever divides and formats it.
    let quote_reserves = curve::uint_return(&reserves_data)
        .ok_or_else(|| "realQuoteReserve(): malformed return".to_owned())?;

    Ok(CurveFacts {
        complete,
        quote_reserves,
        // Pons v2's curve arithmetic has not been modelled (research 0040 is
        // reads, not a priced model), so there is no `buy_within_impact`
        // equivalent to call. `None` here means "cannot size into this at
        // all" to every reader downstream (AGENTS.md §3 rule 8), which is why
        // `build` below also records this as an `Unavailable` entry rather
        // than letting a bare `None` speak for itself.
        quote_capacity: None,
        // `record.pair` is `None` for native ETH (`pons.rs`: "The quote
        // asset, or `None` for native ETH") -- the common case, and the only
        // one this reader can name a unit for. When the record *does* name a
        // pair token, this reader has no ERC-20 symbol/decimals lookup (no
        // new provider, per the packet), so it must not guess ETH: guessing
        // would print a wei figure with the wrong asset's name on it, which
        // is precisely the fabricated fact AGENTS.md §3 rule 2 forbids.
        // `None` here carries the absence forward so the sheet puts the unit
        // on `unknown` instead of rendering anything.
        quote_asset: record.pair.is_none().then(QuoteAsset::eth),
        // "Who launched it" -- the same fact Solana's `CurveFacts::creator`
        // doc comment names -- is the launch record's `deployer`, not
        // `creator_fee_recipient` (who the creator's *fees* are paid to,
        // which can be a different account, e.g. a multisig).
        creator: ChainAddress::Robinhood(record.deployer),
        // `realorrug_pumpfun::Fees` is a Solana venue's schedule; Pons v2 has
        // its own, unread here (`build` records this as `Unavailable`).
        fees: None,
    })
}

/// Builds a dossier for one Robinhood Chain token.
///
/// Mirrors [`crate::build`]'s shape: a hard [`Error`] only when the mint
/// itself cannot be resolved (the factory read fails, or the factory has no
/// record of this token); every other missing fact lands in
/// [`Dossier::unavailable`] on a dossier that is still returned.
///
/// # Errors
///
/// [`Error`] when the factory's own record of `token` cannot be read at all.
pub fn build(
    client: &Rpc,
    budget: &mut Budget,
    token: &RobinhoodAddress,
) -> Result<Dossier, Error> {
    let mut dossier = Dossier {
        mint: ChainAddress::Robinhood(*token),
        read_at: None,
        launch: None,
        curve: None,
        creator_transactions: None,
        chain_launch: None,
        holders: None,
        unavailable: Vec::new(),
        calls: 0,
        elapsed_ms: 0,
    };

    // 1. The launch record. The one read this dossier cannot exist without --
    // everything below is read against the curve address this returns.
    let record = launched_token(budget, client, token)?;
    if !record.exists {
        return Err(Error::NotLaunched);
    }

    // 2. The read point. Optional in the same sense Solana's launch-slot read
    // is: a budget spent by the time this runs is a truncated dossier, not a
    // failed one.
    let mut read_time = None;
    match block_time(budget, client, None) {
        Ok((n, at)) => {
            dossier.read_at = Some(ReadAt::Robinhood(n));
            read_time = Some(at);
        }
        Err(why) => dossier.unavailable.push(Unavailable {
            fact: "read point",
            why,
        }),
    }

    // 3. The curve: graduation and quote reserves.
    match curve_facts(budget, client, &record) {
        Ok(facts) => dossier.curve = Some(facts),
        Err(why) => dossier.unavailable.push(Unavailable { fact: "curve", why }),
    }

    // 4. The launch block, its age and the launcher's own buy. Required by
    // design 0020 §1, so a miss is named and the sheet treats it as unread.
    match launch_facts(budget, client, token, &record, read_time) {
        Ok(launch) => dossier.chain_launch = Some(launch),
        Err(why) => dossier.unavailable.push(Unavailable {
            fact: "launch block",
            why,
        }),
    }

    // 5. The holders. Required too. A busy token pages the read by block
    // range instead of one unbounded call, so the provider's per-call result
    // cap (research/robinhood-trader-signals.md §0) no longer collapses
    // every actively-traded token's holder count to unread. Paging needs a
    // concrete upper block, so it only runs when the read point (step 2)
    // was actually read; the launch block (step 4) bounds the low end when
    // known and falls back to genesis otherwise -- the same fallback
    // `Rpc::logs` always used. A read point that could not be read here
    // means the budget or deadline was already tight enough that this read
    // falls back to the prior one-shot call, which fails the same way it
    // always did rather than paging against a block number that was never
    // learned.
    let from_block = dossier.chain_launch.as_ref().map_or(0, |l| l.block);
    let holders = match dossier.read_at {
        Some(ReadAt::Robinhood(to_block)) => {
            holders_paged(budget, client, token, from_block, to_block, &record)
        }
        _ => take(budget)
            .and_then(|()| client.logs(token, &[topic::TRANSFER]))
            .and_then(|logs| holders_from(&logs, &record)),
    };
    match holders {
        Ok(h) => dossier.holders = Some(h),
        Err(why) => dossier.unavailable.push(Unavailable {
            fact: "holders",
            why,
        }),
    }

    // 6. Facts this reader cannot supply at all yet, regardless of budget.
    // AGENTS.md §3 rule 8: absent is not zero, so each is named rather than
    // left as a silent `None`.
    dossier.unavailable.push(Unavailable {
        fact: "capacity",
        why: "Pons v2's curve arithmetic has not been modelled, so a \
              1%-impact size cannot be computed"
            .to_owned(),
    });
    dossier.unavailable.push(Unavailable {
        fact: "fees",
        why: "realorrug_pumpfun::Fees is a Solana venue's schedule; Pons v2's \
              own fee schedule is not read here"
            .to_owned(),
    });
    dossier.unavailable.push(Unavailable {
        fact: "creator transactions",
        why: "an account's nonce counts transactions the key sent, including \
              reverted ones, which is not what Count::AtLeast means"
            .to_owned(),
    });

    dossier.calls = budget.calls_made();
    dossier.elapsed_ms = budget.elapsed().as_millis();
    Ok(dossier)
}

/// The [`ChainReader`] ADR 0028 point 2 asks a second chain to supply: one
/// implementation, wrapping [`build`], against the same [`Dossier`] Solana's
/// [`crate::SolanaReader`] already produces.
#[derive(Clone, Copy, Debug, Default)]
pub struct RobinhoodReader;

impl ChainReader for RobinhoodReader {
    type Client = Rpc;
    type Token = RobinhoodAddress;
    type Error = Error;

    fn read(
        &self,
        client: &Rpc,
        budget: &mut Budget,
        token: &RobinhoodAddress,
    ) -> Result<Dossier, Error> {
        build(client, budget, token)
    }
}

/// How many logs one window aims to bring back.
///
/// Under both providers' 10,000-log cap, with room for the estimate to be
/// wrong. The window is resized from what each answer actually held (see
/// [`walk_launches`]), so this is the target it converges on, not a limit
/// anything enforces.
const LOGS_PER_WINDOW: u64 = 8_000;

/// Every `TokenLaunched` the factory emitted between two blocks, in order,
/// handed to `sink` one at a time.
///
/// # Why this pages instead of asking once
///
/// [`Rpc::logs`] would answer the whole range in one call if the answer fit,
/// and it does not: research 0038 §1 extrapolated about 171,000 Pons v2
/// launches, against a cap of 10,000 logs per answer. A capped answer arrives
/// as [`LogsError::TooManyResults`] rather than as a short list, which is the
/// only reason this can be written safely at all -- a provider that silently
/// truncated would give a creator index that was wrong in the direction that
/// flatters, every missing launch reading as a launch that never happened.
///
/// # Why the window resizes from the answer rather than being fixed
///
/// Launch density is not constant along the chain, and a fixed window sized
/// for the busiest stretch would spend tens of thousands of calls on the
/// quiet ones. Each answer says how many logs that many blocks held, so the
/// next window is scaled toward [`LOGS_PER_WINDOW`] from a count already
/// paid for. Growth is capped at four times per step so that crossing from a
/// dead stretch into a live one overshoots once, not catastrophically;
/// shrinking is a halving on the cap error, which is the only signal the
/// provider gives.
///
/// # Why `sink` rather than a returned `Vec`
///
/// The caller is building a map keyed by launcher and never needs two
/// launches at once. Handing them over one at a time keeps a full-history
/// walk's memory flat and, more usefully, lets a caller checkpoint: the walk
/// is long enough that being interrupted partway is normal, and the block
/// this returns is the watermark of what was actually delivered.
///
/// # Errors
///
/// The provider's error, or a window of a single block that still answers
/// "too many results" -- which cannot be halved further, and is reported
/// rather than skipped, because skipping it would drop every launch in that
/// block while the walk went on looking complete.
pub fn walk_launches<F, S>(
    from_block: u64,
    to_block: u64,
    fetch: F,
    mut sink: S,
) -> Result<u64, String>
where
    F: FnMut(u64, u64) -> Result<Vec<Log>, LogsError>,
    S: FnMut(Launched),
{
    let mut launches = 0_u64;
    // The walk's own tallies are dropped here on purpose: this function's
    // contract is "how many launches", and `Walked::logs` would be the same
    // number only by coincidence of every log decoding.
    let _ = walk_logs(from_block, to_block, fetch, |log| {
        if let Some(launch) = Launched::from_log(log) {
            sink(launch);
            launches = launches.saturating_add(1);
        }
    })?;
    Ok(launches)
}

/// Every log between two blocks matching whatever filter `fetch` already
/// carries (an address, a topic list, or both), in order, handed to `sink`
/// one at a time, undecoded.
///
/// [`walk_launches`] is this with a `TokenLaunched` decode wired in; it is
/// pulled out on its own because the `creator-index` command runs the same
/// windowed walk two more times over the same block range for `Graduated`
/// (still scoped to the factory) and `CurveBuy` (scoped to no address at
/// all, since a curve's own address is not known until its `TokenLaunched`
/// is seen) -- the resizing logic that survives the provider's result cap is
/// the part worth sharing, not the decode.
///
/// # Errors
///
/// The provider's error, or a window of a single block that still answers
/// "too many results" -- which cannot be halved further, and is reported
/// rather than skipped, because skipping it would drop every log in that
/// block while the walk went on looking complete.
pub fn walk_logs<F>(
    from_block: u64,
    to_block: u64,
    mut fetch: F,
    mut sink: impl FnMut(&Log),
) -> Result<Walked, String>
where
    F: FnMut(u64, u64) -> Result<Vec<Log>, LogsError>,
{
    let mut at = from_block;
    let mut window: u64 = 1;
    let mut seen = 0_u64;
    let mut fetches = 0_u64;
    while at <= to_block {
        let end = at.saturating_add(window - 1).min(to_block);
        fetches = fetches.saturating_add(1);
        match fetch(at, end) {
            Ok(logs) => {
                let held = u64::try_from(logs.len()).unwrap_or(u64::MAX);
                for log in &logs {
                    sink(log);
                    seen = seen.saturating_add(1);
                }
                at = end.saturating_add(1);
                window = next_window(window, held);
            }
            Err(LogsError::TooManyResults) => {
                if window == 1 {
                    return Err(format!(
                        "eth_getLogs: block {at} alone holds more logs than the provider will \
                         return, and a window cannot be narrower than one block"
                    ));
                }
                window /= 2;
            }
            Err(LogsError::Other(e)) => return Err(e),
        }
    }
    Ok(Walked {
        requests: fetches,
        logs: seen,
    })
}

/// What one [`walk_logs`] cost and what it found.
///
/// `requests` is here rather than counted by each caller inside its own
/// `fetch` closure, which is where it lived until 2026-09-17. A caller's
/// counter cannot be tested without a fake endpoint, so the three walks in
/// `creator-index` each had an untested `calls += 1` deciding the cost figure
/// an operator reads to answer "can I afford to run this on the free plan".
/// Counted here, it is the walk's own arithmetic and a plain `fetch` closure
/// in a test can prove it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Walked {
    /// How many times `fetch` was called -- **including** the retries after a
    /// "too many results" answer, because a capped request is a request the
    /// provider counted against the plan's quota just like any other.
    pub requests: u64,
    /// How many logs reached `sink`.
    pub logs: u64,
}

/// The next window, scaled from what the last one actually held.
///
/// Separate from [`walk_launches`] so the arithmetic can be read and tested on
/// its own: it is the part that decides how many calls a full walk costs, and
/// the part where an off-by-one turns into either a stalled walk (a window
/// that rounds to zero) or a shower of capped calls.
fn next_window(window: u64, held: u64) -> u64 {
    let ceiling = window.saturating_mul(4);
    if held == 0 {
        return ceiling;
    }
    let scaled = window.saturating_mul(LOGS_PER_WINDOW) / held;
    // At least one block, or the walk stops advancing; at most four times the
    // last window, so a quiet stretch does not launch a single enormous query
    // into a busy one.
    //
    // `clamp` rather than the two comparisons written out, even though that
    // costs this function its `const` (`Ord::clamp` is not const-callable
    // yet): a hand-written clamp has two boundary comparisons whose `<`/`<=`
    // and `>`/`>=` forms behave identically, so they are mutants no test can
    // ever kill. One call with no operators of our own has no such corner.
    scaled.clamp(1, ceiling)
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    use super::*;

    /// Serves each body in order, one connection each. Lifted from
    /// `realorrug-robinhood/tests/rpc_over_http.rs`'s own `serve`: a loopback
    /// server replaying canned JSON-RPC responses is that crate's established
    /// way to test its `Rpc` client with no network, and `Rpc` here is the
    /// same type, so the same technique tests this module's calls through it.
    fn serve(bodies: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let url = format!("http://{}", listener.local_addr().expect("an address"));
        std::thread::spawn(move || {
            for body in bodies {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let mut reader = BufReader::new(stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut request = vec![0; length];
                let _ = reader.read_exact(&mut request);
                let mut stream = reader.into_inner();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        url
    }

    fn answer(result: &serde_json::Value) -> String {
        serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": result }).to_string()
    }

    fn token() -> RobinhoodAddress {
        RobinhoodAddress([0x11; 20])
    }

    fn record(exists: bool, curve: RobinhoodAddress, deployer: RobinhoodAddress) -> LaunchedToken {
        LaunchedToken {
            token: token(),
            curve,
            deployer,
            creator_fee_recipient: deployer,
            pair: None,
            graduation_threshold: 4_200_000_000_000_000_000,
            creator_tax_bps: 100,
            buyback: false,
            phase: 0,
            exists,
        }
    }

    fn word_u(v: u128) -> Vec<u8> {
        let mut w = [0u8; 32];
        w[16..].copy_from_slice(&v.to_be_bytes());
        w.to_vec()
    }

    fn word_addr(a: &RobinhoodAddress) -> Vec<u8> {
        let mut w = [0u8; 32];
        w[12..].copy_from_slice(&a.0);
        w.to_vec()
    }

    fn word_bool(b: bool) -> Vec<u8> {
        word_u(u128::from(b))
    }

    /// A `getLaunchedToken` return matching [`record`] exactly -- the fifteen
    /// static words `LaunchedToken::from_return` reads.
    fn launched_token_return(record: &LaunchedToken) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend(word_addr(&record.token));
        data.extend(word_addr(&record.curve));
        data.extend(word_addr(&record.deployer));
        data.extend(word_addr(&record.creator_fee_recipient));
        data.extend(word_addr(
            record.pair.as_ref().unwrap_or(&RobinhoodAddress::ZERO),
        ));
        data.extend(word_u(record.graduation_threshold));
        data.extend(word_u(0)); // pool fee, unread
        data.extend(word_u(0)); // tick spacing, unread
        data.extend(word_u(u128::from(record.creator_tax_bps)));
        data.extend(word_bool(record.buyback));
        data.extend(word_u(u128::from(record.phase)));
        data.extend(word_u(0)); // swept quote, unread
        data.extend(word_u(0)); // swept tokens, unread
        data.extend(word_u(0)); // swept at, unread
        data.extend(word_bool(record.exists));
        data
    }

    fn hex(data: &[u8]) -> serde_json::Value {
        serde_json::Value::from(realorrug_robinhood::to_hex(data))
    }

    fn budget() -> Budget {
        Budget::new(60, 3, Duration::from_secs(30))
    }

    /// An `eth_getBlockByNumber` answer.
    fn block(number: u64, timestamp: u64) -> String {
        answer(&serde_json::json!({
            "number": format!("{number:#x}"),
            "timestamp": format!("{timestamp:#x}"),
        }))
    }

    fn topic_of(a: &RobinhoodAddress) -> Hash32 {
        address_topic(a)
    }

    fn log_json(
        address: &RobinhoodAddress,
        topics: &[Hash32],
        data: &[u8],
        block: u64,
    ) -> serde_json::Value {
        serde_json::json!({
            "address": address.to_string(),
            "topics": topics.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "data": realorrug_robinhood::to_hex(data),
            "blockNumber": format!("{block:#x}"),
            "transactionHash": LAUNCH_TX,
        })
    }

    const LAUNCH_TX: &str = "0x5555555555555555555555555555555555555555555555555555555555555555";
    const LAUNCH_BLOCK: u64 = 0x40;

    fn launch_log(rec: &LaunchedToken) -> serde_json::Value {
        let mut data = word_addr(&RobinhoodAddress::ZERO);
        data.extend(word_u(0));
        data.extend(word_u(rec.graduation_threshold));
        log_json(
            &FACTORY,
            &[
                topic::TOKEN_LAUNCHED,
                topic_of(&rec.token),
                topic_of(&rec.curve),
                topic_of(&rec.deployer),
            ],
            &data,
            LAUNCH_BLOCK,
        )
    }

    fn buy_log(
        curve: &RobinhoodAddress,
        trader: &RobinhoodAddress,
        quote: u128,
    ) -> serde_json::Value {
        let mut data = word_u(quote);
        data.extend(word_u(1_000));
        data.extend(word_u(0));
        data.extend(word_u(0));
        log_json(
            curve,
            &[topic::CURVE_BUY, topic_of(trader), topic_of(trader)],
            &data,
            LAUNCH_BLOCK,
        )
    }

    #[test]
    fn a_launcher_buy_counts_when_either_the_buyer_or_the_recipient_is_the_launcher() {
        // A launch can buy through a router (the launcher receives, someone
        // else is the trader) or buy for someone else (the launcher trades, a
        // different address receives). Both are the launcher's money moving
        // in the launch transaction; requiring both would miss either.
        let curve = RobinhoodAddress([0x41; 20]);
        let launcher = RobinhoodAddress([0x42; 20]);
        let rec = record(true, curve, launcher);
        let one_sided = |trader: &RobinhoodAddress, recipient: &RobinhoodAddress, quote| {
            let mut data = word_u(quote);
            data.extend(word_u(1_000));
            data.extend(word_u(0));
            data.extend(word_u(0));
            Log::from_json(&log_json(
                &curve,
                &[topic::CURVE_BUY, topic_of(trader), topic_of(recipient)],
                &data,
                LAUNCH_BLOCK,
            ))
            .expect("a log")
        };
        let logs = [
            one_sided(&launcher, &ALICE, 7),
            one_sided(&BOB, &launcher, 11),
        ];
        assert_eq!(launcher_buy(&logs, &rec), 18);
        let strangers = [one_sided(&ALICE, &BOB, 13)];
        assert_eq!(launcher_buy(&strangers, &rec), 0);
    }

    fn transfer(from: &RobinhoodAddress, to: &RobinhoodAddress, value: u128) -> serde_json::Value {
        transfer_at(from, to, value, LAUNCH_BLOCK)
    }

    fn transfer_at(
        from: &RobinhoodAddress,
        to: &RobinhoodAddress,
        value: u128,
        block: u64,
    ) -> serde_json::Value {
        log_json(
            &token(),
            &[topic::TRANSFER, topic_of(from), topic_of(to)],
            &word_u(value),
            block,
        )
    }

    /// A raw JSON-RPC error body carrying the provider's own result-count
    /// cap wording, byte-identical to the one reproduced live 2026-09-17
    /// against a real graduated token
    /// (docs/research/0050-robinhood-trader-signals.md §0).
    fn too_many_results_error() -> String {
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"logs matched by query exceeds limit of 10000"}}"#
            .to_owned()
    }

    const ALICE: RobinhoodAddress = RobinhoodAddress([0xa1; 20]);
    const BOB: RobinhoodAddress = RobinhoodAddress([0xb0; 20]);
    const DAVE: RobinhoodAddress = RobinhoodAddress([0xd0; 20]);
    const DEV_BUY: u128 = 500_000_000_000_000_000;

    /// Every answer a full read of `rec` takes, in order, with the launch
    /// transaction's status `status`.
    fn full_bodies(rec: &LaunchedToken, status: &str) -> Vec<String> {
        let receipt = serde_json::json!({
            "status": status,
            "transactionHash": LAUNCH_TX,
            "blockNumber": format!("{LAUNCH_BLOCK:#x}"),
            "from": rec.deployer.to_string(),
            "to": FACTORY.to_string(),
            "logs": [
                launch_log(rec),
                buy_log(&rec.curve, &rec.deployer, DEV_BUY),
                // Someone else's buy in the same transaction is not the launcher's.
                buy_log(&rec.curve, &ALICE, 7),
                // The launcher's buy against a different curve is not this launch.
                buy_log(&RobinhoodAddress([0xee; 20]), &rec.deployer, 9),
            ],
        });
        let (curve, dev) = (rec.curve, rec.deployer);
        let transfers = serde_json::json!([
            transfer(&RobinhoodAddress::ZERO, &curve, 1_000),
            transfer(&curve, &dev, 100),
            transfer(&curve, &ALICE, 300),
            transfer(&ALICE, &BOB, 100),
            transfer(&curve, &FACTORY, 50),
            transfer(&curve, &DAVE, 40),
            transfer(&DAVE, &curve, 40),
        ]);
        vec![
            answer(&hex(&launched_token_return(rec))),
            block(0x64, 10_000),
            answer(&hex(&word_bool(false))),
            answer(&hex(&word_u(1))),
            answer(&serde_json::json!([launch_log(rec)])),
            block(LAUNCH_BLOCK, 6_400),
            answer(&receipt),
            answer(&hex(&abi_string(b"Pepe Token"))),
            answer(&hex(&abi_string(b"PEPE"))),
            answer(&transfers),
        ]
    }

    /// One string, ABI-encoded as a token's `name()` returns it.
    fn abi_string(text: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        out[31] = 32;
        let length = u64::try_from(text.len()).expect("a test string fits in u64");
        out[56..64].copy_from_slice(&length.to_be_bytes());
        out.extend_from_slice(text);
        out.resize(64 + text.len().div_ceil(32) * 32, 0);
        out
    }

    #[test]
    fn a_launch_reads_its_block_age_launcher_buy_and_holders() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let client = Rpc::new(serve(full_bodies(&rec, "0x1")));
        let mut b = budget();

        let dossier = build(&client, &mut b, &token()).expect("a dossier");
        assert_eq!(
            dossier.chain_launch,
            Some(ChainLaunch {
                block: LAUNCH_BLOCK,
                age_seconds: Some(3_600),
                dev_buy_wei: Some(DEV_BUY),
                // Two `eth_call`s the reader did not make before 2026-09-17.
                // No Pons event or factory record carries a name, so without
                // these the share card drew its verdict over a blank.
                name: Some("Pepe Token".to_owned()),
                symbol: Some("PEPE".to_owned()),
            })
        );
        // The launcher 100, Alice 200, Bob 100. The curve and the factory are
        // machinery, and Dave sold everything back.
        assert_eq!(
            dossier.holders,
            Some(Holders {
                count: 3,
                largest_share_bps: Some(5_000),
            })
        );
        for fact in ["launch block", "holders"] {
            assert!(
                !dossier.unavailable.iter().any(|u| u.fact == fact),
                "{fact} read but was named unavailable"
            );
        }
        // Ten, not the eight this read cost before 2026-09-17: `name()` and
        // `symbol()` are two calls the reader now makes so the share card can
        // say which token it is. Pinned rather than left loose because the
        // default budget is sixty calls and a read that quietly grows is how
        // a plan's daily quota goes without anyone choosing to spend it.
        assert_eq!(dossier.calls, 10);
    }

    #[test]
    fn a_reverted_launch_transaction_bought_nothing() {
        let rec = record(
            true,
            RobinhoodAddress([0x25; 20]),
            RobinhoodAddress([0x36; 20]),
        );
        let client = Rpc::new(serve(full_bodies(&rec, "0x0")));
        let mut b = budget();

        let dossier = build(&client, &mut b, &token()).expect("a dossier");
        assert_eq!(dossier.chain_launch.expect("a launch").dev_buy_wei, Some(0));
    }

    #[test]
    fn a_launch_older_than_the_read_point_has_no_age_not_a_wrapped_one() {
        let rec = record(
            true,
            RobinhoodAddress([0x27; 20]),
            RobinhoodAddress([0x38; 20]),
        );
        let mut bodies = full_bodies(&rec, "0x1");
        // The launch block's clock reads later than the latest block's.
        bodies[5] = block(LAUNCH_BLOCK, 10_001);
        let client = Rpc::new(serve(bodies));
        let mut b = budget();

        let launch = build(&client, &mut b, &token())
            .expect("a dossier")
            .chain_launch
            .expect("the block still read");
        assert_eq!(launch.age_seconds, None);
        assert_eq!(launch.dev_buy_wei, Some(DEV_BUY));
    }

    #[test]
    fn a_token_with_no_launch_event_names_the_launch_block_unavailable() {
        let rec = record(
            true,
            RobinhoodAddress([0x29; 20]),
            RobinhoodAddress([0x39; 20]),
        );
        let mut bodies = full_bodies(&rec, "0x1");
        bodies[4] = answer(&serde_json::json!([]));
        // With no launch there is no timestamp, receipt, name or symbol read:
        // the transfers come next. Taken from the end rather than by index,
        // because an index here is a second copy of `full_bodies`'s order that
        // goes quietly wrong the next time a read is added -- as it did when
        // `name()` and `symbol()` were.
        let transfers = bodies.pop().expect("the transfers are the last body");
        bodies.truncate(5);
        bodies.push(transfers);
        let client = Rpc::new(serve(bodies));
        let mut b = budget();

        let dossier = build(&client, &mut b, &token()).expect("a dossier");
        assert_eq!(dossier.chain_launch, None);
        assert!(dossier.unavailable.iter().any(|u| u.fact == "launch block"));
        assert_eq!(dossier.holders.expect("holders").count, 3);
    }

    #[test]
    fn a_ledger_that_spends_more_than_it_received_is_refused() {
        let rec = record(
            true,
            RobinhoodAddress([0x2a; 20]),
            RobinhoodAddress([0x3a; 20]),
        );
        let logs: Vec<Log> = [transfer(&ALICE, &BOB, 1)]
            .iter()
            .map(|v| Log::from_json(v).expect("a log"))
            .collect();
        assert!(holders_from(&logs, &rec).is_err());
    }

    #[test]
    fn a_transfer_that_does_not_decode_refuses_the_count() {
        let rec = record(
            true,
            RobinhoodAddress([0x2b; 20]),
            RobinhoodAddress([0x3b; 20]),
        );
        let mut log = Log::from_json(&transfer(&RobinhoodAddress::ZERO, &ALICE, 1)).expect("a log");
        log.data.clear();
        assert!(holders_from(&[log], &rec).is_err());
    }

    #[test]
    fn a_share_of_a_supply_near_the_integer_limit_does_not_overflow() {
        let rec = record(
            true,
            RobinhoodAddress([0x2c; 20]),
            RobinhoodAddress([0x3c; 20]),
        );
        let logs: Vec<Log> = [
            transfer(&RobinhoodAddress::ZERO, &ALICE, u128::MAX / 2),
            transfer(&RobinhoodAddress::ZERO, &BOB, u128::MAX / 4),
        ]
        .iter()
        .map(|v| Log::from_json(v).expect("a log"))
        .collect();
        let holders = holders_from(&logs, &rec).expect("holders");
        assert_eq!(holders.count, 2);
        assert_eq!(holders.largest_share_bps, Some(6_666));
    }

    #[test]
    fn a_token_whose_transfers_come_back_in_three_pages_sums_to_the_right_holders() {
        // The provider's cap fires on the first window (20,000 blocks) and on
        // the first halving too (10,000), so the walk settles at 5,000 and
        // then widens: two failed attempts, then three successful pages
        // covering 0-4,999, 5,000-14,999 and 15,000-34,999. Each page
        // contributes a transfer, and the three joined in order give the same
        // balances a single unbounded call would have, which is the property
        // this walk exists to keep.
        let curve = RobinhoodAddress([0x51; 20]);
        let deployer = RobinhoodAddress([0x52; 20]);
        let rec = record(true, curve, deployer);
        let client = Rpc::new(serve(vec![
            too_many_results_error(),
            too_many_results_error(),
            answer(&serde_json::json!([transfer_at(
                &RobinhoodAddress::ZERO,
                &curve,
                1_000,
                0
            )])),
            answer(&serde_json::json!([transfer_at(
                &curve, &ALICE, 400, 5_000
            )])),
            answer(&serde_json::json!([transfer_at(&curve, &BOB, 600, 20_000)])),
        ]));
        let mut b = budget();

        let holders = holders_paged(&mut b, &client, &token(), 0, 34_999, &rec).expect("holders");
        assert_eq!(holders.count, 2);
        assert_eq!(holders.largest_share_bps, Some(6_000));
        assert_eq!(b.calls_made(), 5);
    }

    #[test]
    fn a_window_narrowed_by_a_busy_stretch_widens_again_once_pages_succeed() {
        // The launch burst is the densest stretch of a token's life and the
        // least representative of the rest of it. Here the first two windows
        // (20,000 then 10,000 blocks) are refused and 5,000 succeeds -- and a
        // walk that kept 5,000 would then need 131 more pages to cross the
        // remaining 640,000 blocks, nearly all of them empty.
        //
        // Doubling after each success crosses the whole range in eight pages:
        // 5k, 10k, 20k, 40k, 80k, 160k, then two at the 200,000-block ceiling.
        // Ten canned answers are served, which is exactly what the widening
        // walk asks for, so deleting the doubling fails this test twice over
        // -- on the call count, and on running out of server.
        let curve = RobinhoodAddress([0x59; 20]);
        let deployer = RobinhoodAddress([0x5a; 20]);
        let rec = record(true, curve, deployer);
        let empty = || answer(&serde_json::json!([]));
        let client = Rpc::new(serve(vec![
            too_many_results_error(),
            too_many_results_error(),
            answer(&serde_json::json!([transfer_at(
                &RobinhoodAddress::ZERO,
                &curve,
                1_000,
                0
            )])),
            empty(),
            empty(),
            answer(&serde_json::json!([transfer_at(
                &curve, &ALICE, 400, 40_000
            )])),
            empty(),
            empty(),
            empty(),
            answer(&serde_json::json!([transfer_at(
                &curve, &BOB, 600, 600_000
            )])),
        ]));
        let mut b = budget();

        let holders = holders_paged(&mut b, &client, &token(), 0, 655_359, &rec).expect("holders");
        assert_eq!(b.calls_made(), 10, "two refusals and eight widening pages");
        // And the ledger is whole: a page missed anywhere in the range would
        // leave the curve holding what it never sent.
        assert_eq!(holders.count, 2);
        assert_eq!(holders.largest_share_bps, Some(6_000));
    }

    #[test]
    fn a_wide_window_over_the_limit_halves_and_a_narrower_one_succeeds() {
        let curve = RobinhoodAddress([0x53; 20]);
        let deployer = RobinhoodAddress([0x54; 20]);
        let rec = record(true, curve, deployer);
        let client = Rpc::new(serve(vec![
            too_many_results_error(),
            answer(&serde_json::json!([transfer_at(
                &RobinhoodAddress::ZERO,
                &ALICE,
                10,
                0
            )])),
            answer(&serde_json::json!([transfer_at(&ALICE, &BOB, 4, 15_000)])),
        ]));
        let mut b = budget();

        // 20,000 blocks is refused, 10,000 answers, and the widened second
        // page (20,000 again) reaches the end: one refusal, two pages.
        let holders = holders_paged(&mut b, &client, &token(), 0, 29_999, &rec).expect("holders");
        assert_eq!(holders.count, 2);
        assert_eq!(b.calls_made(), 3);
    }

    #[test]
    fn a_page_budget_that_runs_out_mid_walk_is_unread_not_a_partial_count() {
        // A small budget derives a small page cap (`holder_page_cap`): with
        // 2 calls left the cap is 1, so a token that needs a second page to
        // finish its range is refused outright rather than answering with
        // only the first page's balances -- a partial ledger presented as
        // complete is exactly what AGENTS.md rule 8 forbids.
        let curve = RobinhoodAddress([0x55; 20]);
        let deployer = RobinhoodAddress([0x56; 20]);
        let rec = record(true, curve, deployer);
        let client = Rpc::new(serve(vec![too_many_results_error()]));
        let mut b = Budget::new(2, 3, Duration::from_secs(30));

        let err = holders_paged(&mut b, &client, &token(), 0, 3, &rec).expect_err("refused");
        assert_eq!(err, TOO_BUSY);
    }

    #[test]
    fn a_single_page_token_still_costs_one_request() {
        let curve = RobinhoodAddress([0x57; 20]);
        let deployer = RobinhoodAddress([0x58; 20]);
        let rec = record(true, curve, deployer);
        let client = Rpc::new(serve(vec![answer(&serde_json::json!([transfer_at(
            &RobinhoodAddress::ZERO,
            &ALICE,
            10,
            1
        )]))]));
        let mut b = budget();

        let holders = holders_paged(&mut b, &client, &token(), 0, 100, &rec).expect("holders");
        assert_eq!(holders.count, 1);
        assert_eq!(b.calls_made(), 1);
    }

    #[test]
    fn a_reader_names_the_token_it_was_asked_for_and_the_block_it_read_at() {
        let curve = RobinhoodAddress([0x22; 20]);
        let deployer = RobinhoodAddress([0x33; 20]);
        let rec = record(true, curve, deployer);
        let url = serve(vec![
            answer(&hex(&launched_token_return(&rec))),
            block(0x64, 0),                       // block 100
            answer(&hex(&word_bool(false))),      // not graduated
            answer(&hex(&word_u(6_186_150_833))), // quote reserves
        ]);
        let client = Rpc::new(url);
        let mut b = budget();

        let dossier = build(&client, &mut b, &token()).expect("a dossier");
        assert_eq!(dossier.mint, ChainAddress::Robinhood(token()));
        assert_eq!(dossier.read_at, Some(ReadAt::Robinhood(100)));
        let curve_facts = dossier.curve.expect("curve facts");
        assert!(!curve_facts.complete);
        assert_eq!(curve_facts.quote_reserves, 6_186_150_833);
        assert_eq!(curve_facts.creator, ChainAddress::Robinhood(deployer));
        assert_eq!(curve_facts.quote_capacity, None);
        assert_eq!(curve_facts.fees, None);
        assert_eq!(curve_facts.quote_asset, Some(QuoteAsset::eth()));
        assert_eq!(dossier.launch, None);
        assert_eq!(dossier.creator_transactions, None);
    }

    /// The case that fails today (packet 0032): a curve holding more than
    /// 18.4 ETH -- the point a `u64` overflows in wei -- must still read
    /// cleanly rather than turn the whole dossier read into a hard error.
    #[test]
    fn a_curve_holding_more_than_18_point_4_eth_reads_without_error() {
        let curve = RobinhoodAddress([0x24; 20]);
        let deployer = RobinhoodAddress([0x35; 20]);
        let rec = record(true, curve, deployer);
        // 20 ETH in wei: 20 * 10^18, well past u64::MAX (~18.4 * 10^18).
        let big: u128 = 20_000_000_000_000_000_000;
        let url = serve(vec![
            answer(&hex(&launched_token_return(&rec))),
            block(0x1, 0),
            answer(&hex(&word_bool(false))),
            answer(&hex(&word_u(big))),
        ]);
        let client = Rpc::new(url);
        let mut b = budget();

        let dossier = build(&client, &mut b, &token()).expect("a dossier");
        let curve_facts = dossier.curve.expect("curve facts, not an overflow error");
        assert_eq!(curve_facts.quote_reserves, big);
        assert!(!dossier.unavailable.iter().any(|u| u.fact == "curve"));
    }

    /// The launch record names a quote token, but this reader has no symbol
    /// for it: the unit must be carried as absent, never guessed as ETH.
    #[test]
    fn a_named_but_unidentified_pair_asset_carries_no_unit() {
        let curve = RobinhoodAddress([0x26; 20]);
        let deployer = RobinhoodAddress([0x37; 20]);
        let mut rec = record(true, curve, deployer);
        rec.pair = Some(RobinhoodAddress([0x42; 20]));
        let url = serve(vec![
            answer(&hex(&launched_token_return(&rec))),
            block(0x1, 0),
            answer(&hex(&word_bool(false))),
            answer(&hex(&word_u(1))),
        ]);
        let client = Rpc::new(url);
        let mut b = budget();

        let dossier = build(&client, &mut b, &token()).expect("a dossier");
        assert_eq!(dossier.curve.expect("curve facts").quote_asset, None);
    }

    #[test]
    fn a_graduated_curve_reports_complete() {
        let rec = record(
            true,
            RobinhoodAddress([0x44; 20]),
            RobinhoodAddress([0x55; 20]),
        );
        let url = serve(vec![
            answer(&hex(&launched_token_return(&rec))),
            block(0x1, 0),
            answer(&hex(&word_bool(true))), // graduated
            answer(&hex(&word_u(0))),
        ]);
        let client = Rpc::new(url);
        let mut b = budget();

        let dossier = build(&client, &mut b, &token()).expect("a dossier");
        assert!(dossier.curve.expect("curve facts").complete);
    }

    #[test]
    fn a_reader_answers_exactly_what_build_answers() {
        let rec = record(
            true,
            RobinhoodAddress([0x66; 20]),
            RobinhoodAddress([0x77; 20]),
        );
        // Every read answered: an error string names the server's port, and
        // two servers have two ports.
        let bodies = || full_bodies(&rec, "0x1");
        let client_a = Rpc::new(serve(bodies()));
        let mut budget_a = budget();
        let direct = build(&client_a, &mut budget_a, &token());

        let client_b = Rpc::new(serve(bodies()));
        let mut budget_b = budget();
        let via_reader = RobinhoodReader.read(&client_b, &mut budget_b, &token());

        match (direct, via_reader) {
            (Ok(d), Ok(r)) => {
                assert_eq!(d.mint, r.mint);
                assert_eq!(d.read_at, r.read_at);
                assert_eq!(d.curve, r.curve);
                assert_eq!(d.chain_launch, r.chain_launch);
                assert_eq!(d.holders, r.holders);
                assert_eq!(d.unavailable, r.unavailable);
                assert_eq!(d.calls, r.calls);
            }
            (d, r) => panic!("build and RobinhoodReader disagreed: {d:?} vs {r:?}"),
        }
    }

    #[test]
    fn a_token_the_factory_has_no_record_of_is_an_error_not_an_empty_dossier() {
        let rec = record(
            false,
            RobinhoodAddress([0x88; 20]),
            RobinhoodAddress([0x99; 20]),
        );
        let url = serve(vec![answer(&hex(&launched_token_return(&rec)))]);
        let client = Rpc::new(url);
        let mut b = budget();

        let err = build(&client, &mut b, &token()).expect_err("no record, no dossier");
        assert!(matches!(err, Error::NotLaunched));
    }

    #[test]
    fn a_budget_exhausted_partway_leaves_unavailable_entries_not_an_error() {
        let rec = record(
            true,
            RobinhoodAddress([0xaa; 20]),
            RobinhoodAddress([0xbb; 20]),
        );
        // Only the launch-record read is served; the budget has exactly one
        // call, so the block-number and curve reads never reach the wire.
        let url = serve(vec![answer(&hex(&launched_token_return(&rec)))]);
        let client = Rpc::new(url);
        let mut b = Budget::new(1, 3, Duration::from_secs(30));

        let dossier = build(&client, &mut b, &token()).expect("a partial dossier, not an error");
        assert_eq!(dossier.read_at, None);
        assert_eq!(dossier.curve, None);
        assert!(dossier.unavailable.iter().any(|u| u.fact == "read point"));
        assert!(dossier.unavailable.iter().any(|u| u.fact == "curve"));
        assert_eq!(dossier.calls, 1);
    }

    #[test]
    fn every_reader_names_the_facts_it_cannot_supply_yet() {
        let rec = record(
            true,
            RobinhoodAddress([0xcc; 20]),
            RobinhoodAddress([0xdd; 20]),
        );
        let url = serve(vec![
            answer(&hex(&launched_token_return(&rec))),
            block(0x1, 0),
            answer(&hex(&word_bool(false))),
            answer(&hex(&word_u(0))),
        ]);
        let client = Rpc::new(url);
        let mut b = budget();

        let dossier = build(&client, &mut b, &token()).expect("a dossier");
        for fact in ["capacity", "fees", "creator transactions"] {
            assert!(
                dossier.unavailable.iter().any(|u| u.fact == fact),
                "missing an Unavailable entry for {fact}"
            );
        }
    }

    #[test]
    fn a_second_chains_reader_compiles_against_the_real_seam() {
        // The point of this test, alongside `dossier.rs`'s `FakeRobinhoodReader`,
        // is that a *real* implementation -- not only a fake one -- compiles
        // against `ChainReader` with Robinhood's own client and token types.
        fn accepts_any_reader<R: ChainReader>(_reader: &R) {}
        accepts_any_reader(&RobinhoodReader);
    }

    /// A `TokenLaunched` log from the factory, at `block`, by `deployer`.
    ///
    /// Built as a struct rather than through the JSON path the other tests
    /// use, because these tests are about the paging loop and not about
    /// parsing: a fake provider that spoke JSON would be testing
    /// `parse_logs` again, one layer away from the thing that can go wrong.
    fn factory_launch(block: u64, deployer: u8, token: u8) -> Log {
        let mut data = word_addr(&RobinhoodAddress::ZERO);
        data.extend(word_u(0));
        data.extend(word_u(1_000));
        Log {
            address: FACTORY,
            topics: vec![
                topic::TOKEN_LAUNCHED,
                topic_of(&RobinhoodAddress([token; 20])),
                topic_of(&RobinhoodAddress([token ^ 0xff; 20])),
                topic_of(&RobinhoodAddress([deployer; 20])),
            ],
            data,
            block,
            transaction: Hash32::from_hex(LAUNCH_TX),
        }
    }

    /// A provider holding `logs`, refusing any window that would return more
    /// than `cap` of them -- the one behaviour `walk_launches` is written
    /// around.
    fn capped_at(
        cap: usize,
        logs: Vec<Log>,
    ) -> impl FnMut(u64, u64) -> Result<Vec<Log>, LogsError> {
        move |from, to| {
            let window: Vec<Log> = logs
                .iter()
                .filter(|l| l.block >= from && l.block <= to)
                .cloned()
                .collect();
            if window.len() > cap {
                return Err(LogsError::TooManyResults);
            }
            Ok(window)
        }
    }

    #[test]
    fn every_launch_in_the_range_is_delivered_once_however_the_window_moves() {
        // Density deliberately uneven: a quiet opening, then a stretch dense
        // enough to force several halvings, then quiet again. A walk that
        // only ever grew its window would drop the middle; one that never
        // grew would still be correct, so the count alone is not the test --
        // the identities are.
        let mut logs = Vec::new();
        for block in 0..2_000_u64 {
            let here = if (900..1_100).contains(&block) { 20 } else { 1 };
            for n in 0..here {
                if block % 7 == 0 || here > 1 {
                    let token = u8::try_from((block + n) % 251).expect("under 251");
                    logs.push(factory_launch(
                        block,
                        u8::try_from(block % 13).expect("under 13"),
                        token,
                    ));
                }
            }
        }
        let expected = logs.len();
        assert!(
            expected > 4_000,
            "the dense stretch must exceed the cap many times over, got {expected}"
        );

        let mut seen: Vec<(u64, RobinhoodAddress)> = Vec::new();
        let delivered = walk_launches(0, 1_999, capped_at(50, logs.clone()), |l| {
            seen.push((0, l.deployer));
        })
        .expect("the walk completes");

        assert_eq!(
            usize::try_from(delivered).expect("fits"),
            expected,
            "the walk reported a different number of launches than the provider held"
        );
        assert_eq!(
            seen.len(),
            expected,
            "a launch was delivered twice or not at all across a window that halved and grew"
        );
    }

    #[test]
    fn the_request_count_includes_the_capped_attempts_a_provider_still_bills() {
        // The number an operator reads to answer "can I run this on the free
        // plan". A count of only the successful windows understates it by
        // exactly the halvings, which is worst on the busiest range -- the one
        // where being wrong about the cost matters.
        let mut attempts = 0_u64;
        let logs: Vec<Log> = (0_u8..60)
            .map(|n| factory_launch(u64::from(n) % 8, 7, n))
            .collect();
        let walked = walk_logs(
            0,
            7,
            |from, to| {
                attempts += 1;
                capped_at(20, logs.clone())(from, to)
            },
            |_| (),
        )
        .expect("eight blocks of sixty logs, capped at twenty, splits fine");

        assert_eq!(
            walked.requests, attempts,
            "the walk counted a different number of requests than it made"
        );
        assert_eq!(
            usize::try_from(walked.logs).expect("fits"),
            logs.len(),
            "every log the provider held must reach the sink"
        );
        // Re-apply the bug by moving `fetches` past the `match`: the window
        // starts at one block and grows, so it must have been capped at least
        // once here, and only a count that includes the capped attempt can
        // equal `attempts`.
        assert!(
            walked.requests > 8,
            "eight blocks took {} requests, so nothing was ever capped and this \
             test proves nothing about retries",
            walked.requests
        );
    }

    #[test]
    fn one_block_over_the_cap_is_an_error_rather_than_a_silent_gap() {
        // The window cannot be narrower than a block, so there is nothing to
        // retry. Reporting it is the only honest move: carrying on would
        // leave a hole in the index that reads exactly like a launcher who
        // never launched.
        let logs: Vec<Log> = (0..60)
            .map(|n| factory_launch(4, 7, u8::try_from(n).expect("under 60")))
            .collect();
        let err =
            walk_launches(0, 9, capped_at(50, logs), |_| ()).expect_err("no window can hold it");
        assert!(
            err.contains("block 4") && err.contains("one block"),
            "the error must name the block that cannot be split: {err}"
        );
    }

    #[test]
    fn an_empty_stretch_costs_a_handful_of_calls_not_one_per_block() {
        // Without growth this walk is a million calls, which is the
        // difference between a backfill that finishes and one that does not.
        let mut calls = 0_u32;
        let walked = walk_launches(
            0,
            1_000_000,
            |_, _| {
                calls += 1;
                Ok(Vec::new())
            },
            |_| (),
        )
        .expect("an empty range still completes");
        assert_eq!(walked, 0);
        assert!(calls < 15, "an empty million blocks took {calls} calls");
    }

    #[test]
    fn the_window_never_rounds_down_to_zero_and_stalls() {
        // A window scaled by an answer far over the target rounds toward
        // nothing; at zero the walk stops advancing and never returns. One
        // block is the floor.
        assert_eq!(next_window(1, u64::MAX), 1);
        assert_eq!(next_window(2, 1_000_000), 1);
        // And an empty answer grows rather than standing still.
        assert_eq!(next_window(64, 0), 256);
        // An answer already at the target holds the window where it is: the
        // loop converges rather than drifting up to the ceiling every step.
        assert_eq!(next_window(100, LOGS_PER_WINDOW), 100);
    }

    #[test]
    fn a_window_of_n_blocks_asks_for_exactly_n_blocks() {
        // The ranges themselves, not just the launches that came back. A
        // window that asked for one block more than it meant to would still
        // deliver every launch and still finish in few calls -- both other
        // tests would pass -- while every answer was a block wider than the
        // size the cap was measured against, which is how a walk that has
        // been tuned to stay under a limit quietly stops staying under it.
        let mut asked: Vec<(u64, u64)> = Vec::new();
        walk_launches(
            1_000,
            1_010,
            |from, to| {
                asked.push((from, to));
                Ok(Vec::new())
            },
            |_| (),
        )
        .expect("an empty range completes");
        // One block, then four, then the rest: windows 1, 4, 16 against an
        // empty answer, each range exactly as wide as the window and the last
        // one cut off at `to_block`.
        assert_eq!(asked, vec![(1_000, 1_000), (1_001, 1_004), (1_005, 1_010)]);
    }
}
