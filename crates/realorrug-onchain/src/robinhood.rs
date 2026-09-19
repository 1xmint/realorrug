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
use std::time::SystemTime;

use realorrug_robinhood::erc20;
use realorrug_robinhood::pons::{FACTORY, Launched, LaunchedToken, Side, Trade, curve, powers, topic};
use realorrug_robinhood::{
    Address as RobinhoodAddress, BlockHeader, Hash32, Log, LogsError, Receipt, Rpc,
};
use realorrug_types::{ChainAddress, ReadAt};

use crate::budget::{Budget, Exhausted};
use crate::dossier::{
    ChainLaunch, ChainReader, CurveFacts, Dossier, Exemption, Holders, Powers, QuoteAsset,
    Unavailable,
};
use crate::memory::{
    CheckRun, Checkpoint, Completeness, Kind as MemoryKind, Memory, REORG_DEPTH, TransferEvent,
};
use crate::wallets;

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

/// One `eth_call`, budget-checked first, pinned to block `at` when the
/// dossier's read point is known.
///
/// Every read in this module takes [`take`] first, so that none of them can
/// skip the budget -- a budget checked only at the start is not a budget.
///
/// `at` is `None` only for the reads that happen before the read point is
/// learned (the factory record) or after it failed to read. Everything else
/// in one dossier is pinned to the same block (design 0021 §9), so a curve
/// answered at one block and a name at the next cannot describe two states.
fn call(
    budget: &mut Budget,
    client: &Rpc,
    to: &RobinhoodAddress,
    data: &[u8],
    at: Option<u64>,
) -> Result<Vec<u8>, String> {
    take(budget)?;
    match at {
        Some(block) => client.call_contract_at(to, data, block),
        None => client.call_contract(to, data),
    }
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

/// A block's number, hash and timestamp (the latest when `number` is
/// `None`), budget-checked. The hash is what makes the read point a
/// *block* rather than a number: a checkpoint remembered by number alone
/// cannot tell a reorg from a quiet day.
fn block_header(
    budget: &mut Budget,
    client: &Rpc,
    number: Option<u64>,
) -> Result<BlockHeader, String> {
    take(budget)?;
    client.block_header(number)
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
///
/// Also hands back the launch transaction's hash and its receipt (already
/// paid for above, to compute `dev_buy_wei`): [`powers_facts`] needs the
/// same receipt's `SnipeTaxExempted` events and the same transaction's
/// calldata, and re-fetching either there would spend budget this read
/// already spent.
fn launch_facts(
    budget: &mut Budget,
    client: &Rpc,
    token: &RobinhoodAddress,
    record: &LaunchedToken,
    read: Option<&BlockHeader>,
) -> Result<(ChainLaunch, Hash32, Option<Receipt>), String> {
    let read_time = read.map(|r| r.timestamp);
    let at = read.map(|r| r.number);
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

    let receipt = take(budget)
        .and_then(|()| client.receipt(&log.transaction))
        .ok()
        .flatten();

    let dev_buy_wei = receipt.as_ref().map(|receipt| {
        // A reverted transaction's logs never happened.
        if receipt.succeeded {
            launcher_buy(&receipt.logs, record)
        } else {
            0
        }
    });

    Ok((
        ChainLaunch {
            block: log.block,
            age_seconds,
            dev_buy_wei,
            name: text_field(budget, client, token, erc20::NAME, at),
            symbol: text_field(budget, client, token, erc20::SYMBOL, at),
        },
        log.transaction,
        receipt,
    ))
}

/// S13's "owner powers live" (research 0052 §3; task packet M-D-0004):
/// creator tax (free -- already on `record`, from the same `getLaunchedToken`
/// call every dossier already pays for), the pending creator-fee timelock,
/// and every address the curve holds exempt from the snipe tax, classified
/// against research 0047 §3's first-party list and the launch's own declared
/// list (research 0048 §3).
///
/// Never fails outright, the same shape `wallets::creator_cash_flow` uses: a
/// sub-read that fails is named in `unavailable` and the rest of `Powers`
/// still returned, rather than a single failure erasing every field that did
/// read (AGENTS.md §3 rule 8).
///
/// # Extra RPC calls
///
/// Two fixed, beyond what `launch_facts` already paid for: one
/// `pendingCreatorFeeRecipient` `eth_call`, and one
/// `eth_getTransactionByHash` for the launch transaction's calldata (the
/// declared list). Then one `snipeTaxExempt` `eth_call` per *distinct*
/// candidate address the launch receipt's `SnipeTaxExempted` events name --
/// confirmed rather than assumed, because an event at launch is not proof
/// nothing has revoked the exemption since. Candidates are found for free:
/// the receipt is the one `launch_facts` already fetched.
fn powers_facts(
    budget: &mut Budget,
    client: &Rpc,
    token: &RobinhoodAddress,
    record: &LaunchedToken,
    transaction: &Hash32,
    receipt: Option<&Receipt>,
    at: Option<u64>,
    unavailable: &mut Vec<Unavailable>,
) -> Powers {
    let mut pending_creator_fee_recipient = None;
    match call(
        budget,
        client,
        &FACTORY,
        &powers::pending_creator_fee_recipient_call_data(token),
        at,
    ) {
        Ok(data) => match powers::pending_recipient_from_return(&data) {
            Some(value) => pending_creator_fee_recipient = value,
            None => unavailable.push(Unavailable {
                fact: "pending creator fee recipient",
                why: "pendingCreatorFeeRecipient returned a malformed word".to_owned(),
            }),
        },
        Err(why) => unavailable.push(Unavailable {
            fact: "pending creator fee recipient",
            why,
        }),
    }

    let declared = match take(budget).and_then(|()| client.transaction(transaction)) {
        Ok(Some(tx)) => match powers::declared_exemptions(&tx.input) {
            Some(list) => Some(list),
            None => {
                unavailable.push(Unavailable {
                    fact: "declared snipe-tax exemptions",
                    why: "the launch transaction's calldata did not decode under \
                          launchToken's confirmed shape (research 0048 §3)"
                        .to_owned(),
                });
                None
            }
        },
        Ok(None) => {
            unavailable.push(Unavailable {
                fact: "declared snipe-tax exemptions",
                why: "the launch transaction could not be found".to_owned(),
            });
            None
        }
        Err(why) => {
            unavailable.push(Unavailable {
                fact: "declared snipe-tax exemptions",
                why,
            });
            None
        }
    };

    let mut exemptions = Vec::new();
    let mut seen: Vec<RobinhoodAddress> = Vec::new();
    if let Some(receipt) = receipt {
        for log in &receipt.logs {
            if log.address != record.curve
                || log.topics.first().copied() != Some(topic::SNIPE_TAX_EXEMPTED)
            {
                continue;
            }
            let Some(address) = log.topic_address(1) else {
                unavailable.push(Unavailable {
                    fact: "snipe tax exemption",
                    why: "a SnipeTaxExempted event carried no address topic".to_owned(),
                });
                continue;
            };
            if seen.contains(&address) {
                // Already confirmed and classified this address from an
                // earlier event in the same receipt -- a second confirmation
                // call would answer the same question again.
                continue;
            }
            seen.push(address);

            let confirmed = call(
                budget,
                client,
                &record.curve,
                &curve::call_data_for(curve::SNIPE_TAX_EXEMPT, &address),
                at,
            )
            .ok()
            .as_deref()
            .and_then(curve::bool_return);

            match confirmed {
                Some(true) => {
                    // `classify` already checks the first-party list before
                    // the declared one; when the declared list itself could
                    // not be read, a non-first-party address cannot be told
                    // apart from declared vs. undeclared, so it is named
                    // rather than guessed either way (rule 8).
                    let source = match &declared {
                        Some(list) => Some(powers::classify(&address, list)),
                        None if powers::FIRST_PARTY.contains(&address) => {
                            Some(powers::Source::FirstParty)
                        }
                        None => {
                            unavailable.push(Unavailable {
                                fact: "snipe tax exemption classification",
                                why: format!(
                                    "{address} is exempt but the declared list could not be \
                                     read, so declared and undeclared cannot be told apart"
                                ),
                            });
                            None
                        }
                    };
                    if let Some(source) = source {
                        exemptions.push(Exemption {
                            address: ChainAddress::Robinhood(address),
                            source,
                        });
                    }
                }
                Some(false) => {}
                None => unavailable.push(Unavailable {
                    fact: "snipe tax exemption",
                    why: format!("snipeTaxExempt could not be confirmed for {address}"),
                }),
            }
        }
    }

    Powers {
        creator_tax_bps: record.creator_tax_bps,
        pending_creator_fee_recipient: pending_creator_fee_recipient.map(ChainAddress::Robinhood),
        exemptions,
    }
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
    at: Option<u64>,
) -> Option<String> {
    call(budget, client, token, &selector, at)
        .ok()
        .as_deref()
        .and_then(erc20::string_from_return)
}

/// The `what` a pair token's symbol/decimals are cached under in
/// [`Memory`], S1's ("name the pair") reuse of design 0021's generic fact
/// store rather than a second table: a pair token's identity cannot change,
/// the same reason a launch record is [`MemoryKind::Forever`].
const PAIR_QUOTE_ASSET_FACT: &str = "pair quote asset";

/// A quote-pair symbol, sanitised for the sheet and the model's context.
///
/// Stricter than [`erc20::string_from_return`]'s general "readable" rule,
/// which keeps any printable text a token's own name/symbol may carry: a
/// pair symbol is about to be quoted directly next to an address in a
/// sentence the model reads (`"paired with HIMS (0x...)"`), so nothing that
/// could pass for an instruction, a mention or another script survives --
/// ASCII alphanumerics and `.`, `-`, `_` only, no more than 32 characters
/// (real tickers are a handful; anything longer is not a ticker). `None` is
/// "could not read", never a truncated or escaped version of what came back
/// (AGENTS.md §3 rule 8).
fn sanitised_symbol(raw: &str) -> Option<String> {
    const MAX_CHARS: usize = 32;
    if raw.is_empty() || raw.chars().count() > MAX_CHARS {
        return None;
    }
    raw.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .then(|| raw.to_owned())
}

/// A cached pair fact's stored text, back into a [`QuoteAsset`].
///
/// The stored form is `"{symbol}\u{1}{decimals}"` -- `\u{1}` because a
/// sanitised symbol can never contain it (only ASCII alphanumerics, `.`,
/// `-`, `_` survive [`sanitised_symbol`]), so it cannot be mistaken for part
/// of the symbol on the way back out.
fn quote_asset_from_cached(pair: &RobinhoodAddress, value: &str) -> Option<QuoteAsset> {
    let (symbol, decimals) = value.split_once('\u{1}')?;
    let decimals: u8 = decimals.parse().ok()?;
    Some(QuoteAsset::token(
        ChainAddress::Robinhood(*pair),
        symbol.to_owned(),
        decimals,
    ))
}

/// Names a Pons v2 quote pair by its own `symbol()`/`decimals()` (S1, "name
/// the pair"), rather than leaving every non-ETH launch's unit unknown.
///
/// Reads `memory` first when given one: a pair token's symbol and decimals
/// cannot change, so a second dossier for the same launch spends no extra
/// budget on them (design 0021's `Kind::Forever`, the same reasoning a
/// launch record is cached under). A cache miss spends two calls -- the same
/// two [`text_field`] already spends on the launched token's own name and
/// symbol -- counted against `budget` the same way every other read here is.
///
/// # Errors
///
/// A safe-to-publish reason the pair could not be named: a call that failed,
/// a `symbol()` return that is not plain text, a symbol [`sanitised_symbol`]
/// refuses (untrusted token metadata, AGENTS.md §3 rule 3), or a
/// `decimals()` return past [`erc20::MAX_DECIMALS`]. The caller records this
/// as an `Unavailable` "quote asset" fact rather than guessing ETH.
fn pair_quote_asset(
    budget: &mut Budget,
    client: &Rpc,
    pair: &RobinhoodAddress,
    at: Option<u64>,
    memory: Option<&Memory>,
) -> Result<QuoteAsset, String> {
    let subject = pair.to_string();
    if let Some(memory) = memory
        && let Ok(Some(fact)) = memory.latest(PAIR_QUOTE_ASSET_FACT, &subject)
        && let Some(asset) = quote_asset_from_cached(pair, &fact.value)
    {
        return Ok(asset);
    }

    let symbol_data = call(budget, client, pair, &erc20::SYMBOL, at)?;
    let raw_symbol = erc20::string_from_return(&symbol_data)
        .ok_or_else(|| "symbol(): the pair's return was not a plain string".to_owned())?;
    let symbol = sanitised_symbol(&raw_symbol).ok_or_else(|| {
        "symbol(): the pair's name is not one Real or Rug will print as a ticker".to_owned()
    })?;

    let decimals_data = call(budget, client, pair, &erc20::DECIMALS, at)?;
    let decimals = erc20::decimals_from_return(&decimals_data).ok_or_else(|| {
        "decimals(): the pair's return was not a plausible decimals value".to_owned()
    })?;

    if let Some(memory) = memory {
        let value = format!("{symbol}\u{1}{decimals}");
        // A cache write that fails leaves the next read to spend the two
        // calls again -- slower, not wrong -- so it is dropped rather than
        // turned into an `Unavailable` entry over a fact that did read.
        let _ = memory.record(
            PAIR_QUOTE_ASSET_FACT,
            &subject,
            0,
            MemoryKind::Forever,
            &value,
            SystemTime::now(),
        );
    }

    Ok(QuoteAsset::token(
        ChainAddress::Robinhood(*pair),
        symbol,
        decimals,
    ))
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
    // `roles::verified_infrastructure` names the same three addresses this
    // used to spell out as a bare `[ZERO, curve, FACTORY]` array, but now the
    // "only with proof" rule (design 0027 §2.1's third distinction) is a type
    // this function calls into rather than a convention every caller has to
    // keep re-stating -- see `crate::roles` for the six tests that pin it
    // down. Each is a `Proof::VerifiedAddress`: the zero address is a fixed
    // constant, and the curve and factory are read from the factory's own
    // `LaunchedToken` record, never guessed from balance size.
    let claims = crate::roles::verified_infrastructure(record.curve, FACTORY);
    Ok(holders_of(
        balances
            .into_iter()
            .filter(|(who, _)| !claims.iter().any(|c| c.address == *who))
            .map(|(_, balance)| balance),
    ))
}

/// The holder count and largest share from the balances that count: the
/// caller has already dropped the machinery (zero address, curve, factory)
/// and `holders_of` drops the empties.
fn holders_of(balances: impl Iterator<Item = u128>) -> Holders {
    let held: Vec<u128> = balances.filter(|balance| *balance > 0).collect();
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
    Holders {
        count,
        largest_share_bps,
    }
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
    let logs = transfer_logs_paged(budget, client, token, from_block, to_block)?;
    holders_from(&logs, record)
}

/// The Transfer logs of `token` over `from_block..=to_block`, walked page by
/// page: the loop [`holders_paged`] documents, split out so the memory-backed
/// read below can walk only the blocks it has not yet seen.
fn transfer_logs_paged(
    budget: &mut Budget,
    client: &Rpc,
    token: &RobinhoodAddress,
    from_block: u64,
    to_block: u64,
) -> Result<Vec<Log>, String> {
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
    Ok(logs)
}

/// The chain name every Robinhood row in the read memory is keyed under.
const MEMORY_CHAIN: &str = "robinhood";

/// The `what` a Transfer walk is recorded as in the memory's check runs.
const TRANSFERS_CHECK: &str = "transfers";

/// Who holds the token, remembering every Transfer read so the next summon
/// reads only the blocks after the last one (design 0021 §9).
///
/// The memory's checkpoint (block number and hash) says how far the stored
/// ledger is complete. When the chain still has that hash at that number,
/// the walk starts at the next block; when it does not, the chain reorged
/// past the checkpoint and the last [`REORG_DEPTH`] blocks of the ledger are
/// forgotten and re-read. Either way the balances the count is computed
/// from are the memory's, after the new suffix is applied atomically with
/// the new checkpoint -- so a walk that dies halfway leaves the old
/// checkpoint and balances untouched, and a range read twice (a retry after
/// a lost answer) inserts nothing the second time.
///
/// Every walk, empty or not, succeeded or not, is recorded as a check run,
/// so "nothing happened in this range" and "the range could not be read"
/// stay two different records (AGENTS.md §3 rule 8).
fn holders_remembered(
    budget: &mut Budget,
    client: &Rpc,
    token: &RobinhoodAddress,
    launch_block: u64,
    read: &BlockHeader,
    record: &LaunchedToken,
    memory: &Memory,
) -> Result<Holders, String> {
    let key = token.to_string();
    let calls_before = budget.calls_made();
    let from_block = resume_point(budget, client, &key, launch_block, read, memory)?;
    let outcome = if from_block > read.number {
        Ok(Vec::new())
    } else {
        transfer_logs_paged(budget, client, token, from_block, read.number).and_then(|logs| {
            logs.iter()
                .map(transfer_event)
                .collect::<Result<Vec<_>, _>>()
        })
    };
    let (completeness, note) = match &outcome {
        Ok(_) => (Completeness::Complete, String::new()),
        Err(why) if why == TOO_BUSY => (Completeness::Truncated, why.clone()),
        Err(why) => (Completeness::Failed, why.clone()),
    };
    let run = CheckRun {
        chain: MEMORY_CHAIN.to_owned(),
        token: key.clone(),
        what: TRANSFERS_CHECK.to_owned(),
        parameters: "Transfer(address,address,uint256) by block range".to_owned(),
        from_block,
        to_block: read.number,
        completeness,
        calls: budget.calls_made().saturating_sub(calls_before),
        note,
        ran_at: std::time::SystemTime::now(),
    };
    // A check run that cannot be recorded is a memory problem, not a chain
    // one; the holders read still answers from what was read.
    let _ = memory.record_check_run(&run);
    let events = outcome?;
    let through = Checkpoint {
        block: read.number,
        hash: read.hash.to_string(),
    };
    memory
        .extend_transfers(MEMORY_CHAIN, &key, &events, &through)
        .map_err(|e| e.to_string())?;
    let machinery = [
        RobinhoodAddress::ZERO.to_string(),
        record.curve.to_string(),
        FACTORY.to_string(),
    ];
    let balances = memory
        .token_balances(MEMORY_CHAIN, &key)
        .map_err(|e| e.to_string())?;
    Ok(holders_of(
        balances
            .into_iter()
            .filter(|(who, _)| !machinery.contains(who))
            .map(|(_, balance)| balance),
    ))
}

/// The first block a memory-backed walk must read, after checking the
/// remembered checkpoint against the chain.
///
/// Costs one call (the checkpoint block's header) when there is a
/// checkpoint below the read point, none otherwise. A checkpoint at the read
/// point itself is compared against the header already in hand.
fn resume_point(
    budget: &mut Budget,
    client: &Rpc,
    key: &str,
    launch_block: u64,
    read: &BlockHeader,
    memory: &Memory,
) -> Result<u64, String> {
    let Some(checkpoint) = memory
        .token_checkpoint(MEMORY_CHAIN, key)
        .map_err(|e| e.to_string())?
    else {
        return Ok(launch_block);
    };
    let still_canonical = match checkpoint.block.cmp(&read.number) {
        std::cmp::Ordering::Equal => checkpoint.hash == read.hash.to_string(),
        std::cmp::Ordering::Less => {
            let header = block_header(budget, client, Some(checkpoint.block))?;
            checkpoint.hash == header.hash.to_string()
        }
        // The provider answered from behind our checkpoint: either it is
        // lagging or the chain reorged. Both are "do not trust the suffix".
        std::cmp::Ordering::Greater => false,
    };
    if still_canonical {
        return Ok(checkpoint.block.saturating_add(1));
    }
    let keep = checkpoint.block.saturating_sub(REORG_DEPTH);
    memory
        .roll_back_transfers_after(MEMORY_CHAIN, key, keep)
        .map_err(|e| e.to_string())?;
    Ok(keep.saturating_add(1).max(launch_block))
}

/// A Transfer log as the memory stores it. Needs the log's position: a
/// provider that omits `blockHash`/`transactionIndex`/`logIndex` gives no
/// identity to dedupe by, so its logs are refused rather than stored twice.
fn transfer_event(log: &Log) -> Result<TransferEvent, String> {
    let id = log.event_id().ok_or_else(|| {
        "a Transfer log carries no position, so it cannot be remembered".to_owned()
    })?;
    let (Some(from), Some(to), Some(amount)) =
        (log.topic_address(1), log.topic_address(2), log.data_u128(0))
    else {
        return Err("a Transfer log did not decode".to_owned());
    };
    Ok(TransferEvent {
        block: id.block,
        block_hash: id.block_hash.to_string(),
        transaction_index: id.transaction_index,
        log_index: id.log_index,
        transaction: log.transaction.to_string(),
        from: (from != RobinhoodAddress::ZERO).then(|| from.to_string()),
        to: to.to_string(),
        amount,
    })
}

/// The factory's record of `token`, the one read a Robinhood dossier cannot
/// be built without.
fn launched_token(
    budget: &mut Budget,
    client: &Rpc,
    token: &RobinhoodAddress,
) -> Result<LaunchedToken, Error> {
    // Unpinned: this is the read that learns whether there is a token at all,
    // before the read point is chosen. The fields used from it (curve,
    // deployer) are set once at launch and never change, so reading them a
    // block earlier than everything else describes the same launch.
    let data = call(
        budget,
        client,
        &FACTORY,
        &LaunchedToken::call_data(token),
        None,
    )
    .map_err(Error::Rpc)?;
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
    at: Option<u64>,
) -> Result<CurveFacts, String> {
    let graduated_data = call(
        budget,
        client,
        &record.curve,
        &curve::call_data(curve::GRADUATED),
        at,
    )?;
    let complete = curve::bool_return(&graduated_data)
        .ok_or_else(|| "graduated(): malformed return".to_owned())?;

    let reserves_data = call(
        budget,
        client,
        &record.curve,
        &curve::call_data(curve::REAL_QUOTE_RESERVE),
        at,
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
        // asset, or `None` for native ETH") -- the common case. `None` here
        // for a token pair is not a guess held over from before this reader
        // could name one (S1, "name the pair"): naming it needs its own
        // `eth_call`s, pinned to the same read point every other call in
        // this dossier is, so `build_with_memory`'s caller fills in
        // `quote_asset` for a token pair after this function returns, from
        // [`pair_quote_asset`], and records why on a failed read rather than
        // guessing ETH -- guessing would print a wei figure with the wrong
        // asset's name on it, precisely the fabricated fact AGENTS.md §3
        // rule 2 forbids.
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
    build_with_memory(client, budget, token, None)
}

/// [`build`], remembering the token's Transfers in `memory` when one is
/// given so the next summon reads only what the chain added since.
///
/// # Errors
///
/// The same as [`build`]'s.
// The read order is the point of this function; three lines over the limit
// after the funding step is not worth a split that hides it.
#[allow(clippy::too_many_lines)]
pub fn build_with_memory(
    client: &Rpc,
    budget: &mut Budget,
    token: &RobinhoodAddress,
    memory: Option<&Memory>,
) -> Result<Dossier, Error> {
    let mut dossier = Dossier {
        mint: ChainAddress::Robinhood(*token),
        read_at: None,
        launch: None,
        curve: None,
        creator_transactions: None,
        chain_launch: None,
        holders: None,
        funding: None,
        market: None,
        token_ownership: None,
        creator_cash_flow: None,
        powers: None,
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
    // Read with its hash, and every contract read below is pinned to this
    // block (design 0021 §9): one dossier describes one state.
    let mut read = None;
    match block_header(budget, client, None) {
        Ok(header) => {
            dossier.read_at = Some(ReadAt::Robinhood(header.number));
            read = Some(header);
        }
        Err(why) => dossier.unavailable.push(Unavailable {
            fact: "read point",
            why,
        }),
    }
    let at = read.as_ref().map(|r| r.number);

    // 3. The curve: graduation and quote reserves. When the launch record
    // names a pair token (S1, "name the pair"), name it here too, at the
    // same read point as everything else -- a failed naming read never
    // takes the curve reads it succeeded alongside down with it, so it is
    // its own `Unavailable` entry, not folded into `curve`'s.
    match curve_facts(budget, client, &record, at) {
        Ok(mut facts) => {
            if let Some(pair) = record.pair {
                match pair_quote_asset(budget, client, &pair, at, memory) {
                    Ok(asset) => facts.quote_asset = Some(asset),
                    Err(why) => dossier.unavailable.push(Unavailable {
                        fact: "quote asset",
                        why,
                    }),
                }
            }
            dossier.curve = Some(facts);
        }
        Err(why) => dossier.unavailable.push(Unavailable { fact: "curve", why }),
    }

    // 4. The launch block, its age and the launcher's own buy. Required by
    // design 0020 §1, so a miss is named and the sheet treats it as unread.
    let mut launch_transaction = None;
    let mut launch_receipt = None;
    match launch_facts(budget, client, token, &record, read.as_ref()) {
        Ok((launch, transaction, receipt)) => {
            dossier.chain_launch = Some(launch);
            launch_transaction = Some(transaction);
            launch_receipt = receipt;
        }
        Err(why) => dossier.unavailable.push(Unavailable {
            fact: "launch block",
            why,
        }),
    }

    // 4b. S13's "owner powers live" (research 0052 §3; task packet
    // M-D-0004). Needs the launch transaction's hash to read the declared
    // exemption list from its calldata; without it (step 4 above failed)
    // there is nothing to classify against, so the whole fact is named once
    // rather than built on an empty declared list that would read as "no
    // wallet was ever declared" instead of "unknown".
    match launch_transaction {
        Some(transaction) => {
            dossier.powers = Some(powers_facts(
                budget,
                client,
                token,
                &record,
                &transaction,
                launch_receipt.as_ref(),
                at,
                &mut dossier.unavailable,
            ));
        }
        None => dossier.unavailable.push(Unavailable {
            fact: "powers",
            why: "the launch transaction hash could not be read".to_owned(),
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
    //
    // With a memory, the walk covers only the blocks after the remembered
    // checkpoint and the count comes from the remembered balances.
    let from_block = dossier.chain_launch.as_ref().map_or(0, |l| l.block);
    let holders = match (&read, memory) {
        (Some(header), Some(memory)) => {
            holders_remembered(budget, client, token, from_block, header, &record, memory)
        }
        (Some(header), None) => {
            holders_paged(budget, client, token, from_block, header.number, &record)
        }
        (None, _) => take(budget)
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

    // 5b. Who funded the first buyers (design 0027 §2.1, slice 3). Runs
    // after every core read so the enrichment spends only what those left,
    // and needs both ends of the launch window: the launch block and the
    // read point. Its own failures land in its `gaps`; only an unreadable
    // window is a miss named here.
    match (dossier.chain_launch.as_ref(), read.as_ref()) {
        (Some(launch), Some(header)) => {
            match wallets::investigate(
                client,
                budget,
                token,
                &record.curve,
                launch.block,
                header.number,
                memory,
            ) {
                Ok(funding) => dossier.funding = Some(funding),
                Err(why) => dossier.unavailable.push(Unavailable {
                    fact: "funding",
                    why,
                }),
            }
        }
        _ => dossier.unavailable.push(Unavailable {
            fact: "funding",
            why: "the launch window needs both the launch block and the read point".to_owned(),
        }),
    }

    // 5c. The creator's own observed cash flow (design 0027 slice 5):
    // every decoded buy/sell against the curve attributed to the deployer
    // or fee recipient, and every other outgoing token transfer from
    // either account. Needs the same launch window as step 5b; a window
    // that could not be established is a named miss rather than a cash
    // flow computed from block 0. `wallets::creator_cash_flow` never
    // fails outright -- an unreadable half degrades to a named gap inside
    // the result and `trades_complete = false` -- so there is no `Err`
    // arm here to miss.
    match (dossier.chain_launch.as_ref(), read.as_ref()) {
        (Some(launch), Some(header)) => {
            dossier.creator_cash_flow = Some(wallets::creator_cash_flow(
                client,
                budget,
                &record,
                launch.block,
                header.number,
            ));
        }
        _ => dossier.unavailable.push(Unavailable {
            fact: "creator cash flow",
            why: "the launch window needs both the launch block and the read point".to_owned(),
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
///
/// `memory` mirrors [`crate::SolanaReader::memory`]: `None` reads every
/// Transfer every time, `Some` reads only what the memory has not seen.
#[derive(Clone, Copy, Default)]
pub struct RobinhoodReader<'a> {
    /// The read memory to remember Transfers in, if the caller has one.
    pub memory: Option<&'a Memory>,
}

impl ChainReader for RobinhoodReader<'_> {
    type Client = Rpc;
    type Token = RobinhoodAddress;
    type Error = Error;

    fn read(
        &self,
        client: &Rpc,
        budget: &mut Budget,
        token: &RobinhoodAddress,
    ) -> Result<Dossier, Error> {
        build_with_memory(client, budget, token, self.memory)
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
pub(crate) mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;

    /// Serves each body in order, one connection each. Lifted from
    /// `realorrug-robinhood/tests/rpc_over_http.rs`'s own `serve`: a loopback
    /// server replaying canned JSON-RPC responses is that crate's established
    /// way to test its `Rpc` client with no network, and `Rpc` here is the
    /// same type, so the same technique tests this module's calls through it.
    pub(crate) fn serve(bodies: Vec<String>) -> String {
        serve_recording(bodies).0
    }

    /// [`serve`], keeping every request body it answered so a test can read
    /// what was asked, not only what was answered.
    fn serve_recording(bodies: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let url = format!("http://{}", listener.local_addr().expect("an address"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
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
                if let Ok(mut seen) = seen.lock() {
                    seen.push(String::from_utf8_lossy(&request).into_owned());
                }
                let mut stream = reader.into_inner();
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (url, requests)
    }

    fn answer(result: &serde_json::Value) -> String {
        serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": result }).to_string()
    }

    pub(crate) fn token() -> RobinhoodAddress {
        RobinhoodAddress([0x11; 20])
    }

    pub(crate) fn record(
        exists: bool,
        curve: RobinhoodAddress,
        deployer: RobinhoodAddress,
    ) -> LaunchedToken {
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
        block_with_hash(number, timestamp, &block_hash(number))
    }

    fn block_with_hash(number: u64, timestamp: u64, hash: &Hash32) -> String {
        answer(&serde_json::json!({
            "number": format!("{number:#x}"),
            "hash": hash.to_string(),
            "timestamp": format!("{timestamp:#x}"),
        }))
    }

    /// The hash every fixture gives block `number`, so a header and the logs
    /// in it agree.
    fn block_hash(number: u64) -> Hash32 {
        let mut hash = [0xbb; 32];
        hash[24..].copy_from_slice(&number.to_be_bytes());
        Hash32(hash)
    }

    /// Each fixture log gets its own `logIndex`: the memory's event identity
    /// is (block, transaction index, log index), and two logs in one block
    /// sharing an index would be one event to it.
    fn next_log_index() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        NEXT.fetch_add(1, Ordering::Relaxed)
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
            "blockHash": block_hash(block).to_string(),
            "transactionHash": LAUNCH_TX,
            "transactionIndex": "0x0",
            "logIndex": format!("{:#x}", next_log_index()),
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
    pub(crate) fn full_bodies(rec: &LaunchedToken, status: &str) -> Vec<String> {
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
        // Eleven before design 0027 slice 5 added two more: `name()` and
        // `symbol()` calls the reader makes so the share card can say which
        // token it is, plus `wallets::creator_cash_flow`'s own two
        // `eth_getLogs` reads (the curve's trade log and the token's
        // Transfer log) now that both the launch block and the read point
        // are present. Pinned rather than left loose because the default
        // budget is sixty calls and a read that quietly grows is how a
        // plan's daily quota goes without anyone choosing to spend it.
        assert_eq!(dossier.calls, 13);
    }

    // ---- slice 3: who funded the first buyers -------------------------

    const ETH: u128 = 1_000_000_000_000_000_000;
    /// Four launch-window buyers, largest first; distinct quotes so the
    /// selection order is not a tie.
    const BUYERS: [(RobinhoodAddress, u128); 4] = [
        (RobinhoodAddress([0x41; 20]), 4 * ETH),
        (RobinhoodAddress([0x42; 20]), 3 * ETH),
        (RobinhoodAddress([0x43; 20]), 2 * ETH),
        (RobinhoodAddress([0x44; 20]), ETH),
    ];
    const HUB: RobinhoodAddress = RobinhoodAddress([0xf0; 20]);
    const OTHER_FUNDER: RobinhoodAddress = RobinhoodAddress([0xf4; 20]);

    /// The launch window's `CurveBuy` logs for [`BUYERS`].
    fn window_logs(rec: &LaunchedToken) -> String {
        let logs: Vec<serde_json::Value> = BUYERS
            .iter()
            .map(|(who, quote)| buy_log(&rec.curve, who, *quote))
            .collect();
        answer(&serde_json::json!(logs))
    }

    /// One `alchemy_getAssetTransfers` page: `funders` each sent `to` the
    /// given wei before its first purchase.
    fn transfers_into(to: &RobinhoodAddress, funders: &[(RobinhoodAddress, u128)]) -> String {
        let transfers: Vec<serde_json::Value> = funders
            .iter()
            .enumerate()
            .map(|(i, (from, wei))| {
                serde_json::json!({
                    "blockNum": "0x20",
                    "uniqueId": format!("0x{}{i:02x}:external", to),
                    "hash": format!("0x{:0>64}", format!("{}{i:02x}", &to.to_string()[2..])),
                    "from": from.to_string(),
                    "to": to.to_string(),
                    "rawContract": { "value": format!("{wei:#x}"), "address": null, "decimal": "0x12" },
                })
            })
            .collect();
        answer(&serde_json::json!({ "transfers": transfers }))
    }

    /// The bodies one candidate's check reads: code, one transfers page,
    /// pre-launch nonce.
    fn candidate_bodies(
        to: &RobinhoodAddress,
        funders: &[(RobinhoodAddress, u128)],
    ) -> Vec<String> {
        vec![
            answer(&serde_json::json!("0x")),
            transfers_into(to, funders),
            answer(&serde_json::json!("0x0")),
        ]
    }

    #[test]
    fn three_of_four_early_buyers_funded_by_one_wallet_is_said_with_its_denominator() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let mut bodies = full_bodies(&rec, "0x1");
        bodies.push(window_logs(&rec));
        bodies.extend(candidate_bodies(&BUYERS[0].0, &[(HUB, 5 * ETH)]));
        bodies.extend(candidate_bodies(&BUYERS[1].0, &[(HUB, 5 * ETH)]));
        bodies.extend(candidate_bodies(&BUYERS[2].0, &[(HUB, 5 * ETH)]));
        bodies.extend(candidate_bodies(&BUYERS[3].0, &[(OTHER_FUNDER, 2 * ETH)]));
        let memory = Memory::open_in_memory().expect("a memory");
        let client = Rpc::new(serve(bodies));
        let mut b = budget();

        let dossier =
            build_with_memory(&client, &mut b, &token(), Some(&memory)).expect("a dossier");
        let funding = dossier.funding.expect("funding read");
        // Bob received a Transfer in the holders fixture but never bought on
        // the curve: a transfer-only recipient is not a buyer.
        assert_eq!(funding.buyers, 4);
        assert!(
            !funding.checked.iter().any(|c| c.address == BOB.to_string()),
            "a transfer-only recipient was checked as a buyer"
        );
        assert_eq!(funding.selected, 4);
        assert_eq!(funding.coverage_bps, Some(10_000));
        assert_eq!(
            funding
                .checked
                .iter()
                .map(|c| c.address.clone())
                .collect::<Vec<_>>(),
            BUYERS
                .iter()
                .map(|(a, _)| a.to_string())
                .collect::<Vec<_>>(),
            "largest buyers first"
        );
        assert_eq!(
            funding.shared,
            vec![wallets::SharedFunder {
                address: HUB.to_string(),
                funded: 3
            }]
        );
        assert!(funding.gaps.is_empty(), "gaps: {:?}", funding.gaps);
        assert_eq!(funding.cu_spent, 60 + 4 * 160);
        assert!(funding.checked.iter().all(|c| c.is_contract == Some(false)));
        assert!(
            funding
                .checked
                .iter()
                .all(|c| c.nonce_before_launch == Some(0))
        );
        assert_eq!(
            dossier.calls,
            10 + 1 + 4 * 3 + 2,
            "the core reads, the window, three per candidate, and \
             creator_cash_flow's two eth_getLogs reads (design 0027 slice 5)"
        );

        let key = token().to_string();
        let edges = memory
            .funding_edges(MEMORY_CHAIN, &key)
            .expect("funding edges");
        assert_eq!(edges.len(), 4);
        assert!(edges.iter().all(|e| e.material));
        let run = memory
            .latest_check_run(MEMORY_CHAIN, &key, "funding")
            .expect("check run")
            .expect("a funding run was recorded");
        assert_eq!(run.completeness, Completeness::Complete);
        assert_eq!((run.from_block, run.to_block), (LAUNCH_BLOCK, 0x64));
    }

    #[test]
    fn a_dust_sender_to_every_buyer_is_an_observation_not_a_shared_funder() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let mut bodies = full_bodies(&rec, "0x1");
        bodies.push(window_logs(&rec));
        for (who, _) in &BUYERS {
            bodies.extend(candidate_bodies(who, &[(HUB, 1_000_000_000_000)]));
        }
        let memory = Memory::open_in_memory().expect("a memory");
        let client = Rpc::new(serve(bodies));
        let mut b = budget();

        let dossier =
            build_with_memory(&client, &mut b, &token(), Some(&memory)).expect("a dossier");
        let funding = dossier.funding.expect("funding read");
        assert_eq!(funding.checked.len(), 4);
        assert!(funding.shared.is_empty(), "dust made a shared funder");
        // The dust is kept as an observation, marked as one.
        let edges = memory
            .funding_edges(MEMORY_CHAIN, &token().to_string())
            .expect("funding edges");
        assert_eq!(edges.len(), 4);
        assert!(
            edges
                .iter()
                .all(|e| !e.material && e.funder == HUB.to_string())
        );
    }

    #[test]
    fn the_compute_unit_cap_stops_the_investigation_and_records_the_gap() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let mut bodies = full_bodies(&rec, "0x1");
        bodies.push(window_logs(&rec));
        for (who, _) in &BUYERS {
            bodies.extend(candidate_bodies(who, &[(HUB, 5 * ETH)]));
        }
        let memory = Memory::open_in_memory().expect("a memory");
        let client = Rpc::new(serve(bodies));
        // Sixty for the window's logs, two candidates' worth, and thirty
        // short of a third at 160 CU -- but not short of one costed at 120,
        // which is what a candidate would cost if the nonce or code read
        // were dropped from its price.
        let mut b = Budget::with_compute_units(60, 3, Duration::from_secs(30), 60 + 2 * 160 + 130);

        let dossier =
            build_with_memory(&client, &mut b, &token(), Some(&memory)).expect("a dossier");
        assert!(dossier.holders.is_some(), "the core reads came first");
        let funding = dossier
            .funding
            .expect("a partial investigation is still a result");
        assert_eq!(funding.selected, 4);
        assert_eq!(funding.checked.len(), 2);
        assert_eq!(
            funding.gaps,
            vec!["compute-unit cap of 450 CU reached: 2 of 4 candidates checked".to_owned()]
        );
        // Two checked, both funded by the hub: still a shared funder, of two.
        assert_eq!(
            funding.shared,
            vec![wallets::SharedFunder {
                address: HUB.to_string(),
                funded: 2
            }]
        );
        let run = memory
            .latest_check_run(MEMORY_CHAIN, &token().to_string(), "funding")
            .expect("check run")
            .expect("a funding run was recorded");
        assert_eq!(run.completeness, Completeness::Truncated);
    }

    #[test]
    fn a_funding_history_longer_than_two_pages_is_cut_and_the_next_buyer_still_read() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let mut bodies = full_bodies(&rec, "0x1");
        // Two buyers only: the first with a history that keeps paging.
        let logs: Vec<serde_json::Value> = BUYERS[..2]
            .iter()
            .map(|(who, quote)| buy_log(&rec.curve, who, *quote))
            .collect();
        bodies.push(answer(&serde_json::json!(logs)));
        let paged = |page: &str| {
            let mut value: serde_json::Value =
                serde_json::from_str(&transfers_into(&BUYERS[0].0, &[(HUB, 5 * ETH)]))
                    .expect("a body");
            value["result"]["pageKey"] = serde_json::Value::String(page.to_owned());
            value.to_string()
        };
        bodies.push(answer(&serde_json::json!("0x")));
        bodies.push(paged("page-2"));
        bodies.push(paged("page-3"));
        // No third page is served: the cut comes before it is asked for,
        // so the next body is the nonce.
        bodies.push(answer(&serde_json::json!("0x0")));
        bodies.extend(candidate_bodies(&BUYERS[1].0, &[(HUB, 5 * ETH)]));
        let client = Rpc::new(serve(bodies));
        let mut b = budget();

        let dossier = build(&client, &mut b, &token()).expect("a dossier");
        let funding = dossier.funding.expect("funding read");
        assert_eq!(
            funding.checked.len(),
            2,
            "a cut history does not stop the next buyer"
        );
        let first = &funding.checked[0];
        assert_eq!(first.funders.len(), 2, "one funder per page read");
        assert!(!first.funding_complete);
        assert_eq!(
            first.nonce_before_launch,
            Some(0),
            "the nonce is read after the cut"
        );
        assert_eq!(
            funding.gaps,
            vec![format!(
                "funding of {}: more than 2 pages; later transfers unread",
                BUYERS[0].0
            )]
        );
        assert!(funding.checked[1].funding_complete);
        assert_eq!(
            dossier.calls,
            10 + 1 + 4 + 3 + 2,
            "plus creator_cash_flow's two reads"
        );
    }

    #[test]
    fn a_provider_without_asset_transfers_degrades_the_funding_read() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let mut bodies = full_bodies(&rec, "0x1");
        bodies.push(window_logs(&rec));
        bodies.push(answer(&serde_json::json!("0x")));
        bodies.push(
            serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "error": { "code": -32601, "message": "Method not found" }
            })
            .to_string(),
        );
        bodies.push(answer(&serde_json::json!("0x0")));
        let client = Rpc::new(serve(bodies));
        let mut b = budget();

        let dossier = build(&client, &mut b, &token()).expect("a dossier");
        let funding = dossier.funding.expect("the window still read");
        assert_eq!(
            funding.checked.len(),
            1,
            "the investigation stopped at the first refusal"
        );
        assert!(funding.checked[0].funders.is_empty());
        assert!(!funding.checked[0].funding_complete);
        assert!(funding.shared.is_empty());
        assert!(
            funding
                .gaps
                .iter()
                .any(|g| g.contains("does not serve alchemy_getAssetTransfers")),
            "gaps: {:?}",
            funding.gaps
        );
    }

    #[test]
    fn every_contract_read_after_the_read_point_is_pinned_to_it() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let (url, requests) = serve_recording(full_bodies(&rec, "0x1"));
        let client = Rpc::new(url);
        let mut b = budget();
        build(&client, &mut b, &token()).expect("a dossier");

        let requests = requests.lock().expect("requests");
        let calls: Vec<serde_json::Value> = requests
            .iter()
            .map(|r| serde_json::from_str(r).expect("a JSON-RPC request"))
            .filter(|r: &serde_json::Value| r["method"] == "eth_call")
            .collect();
        // The factory record is read before the read point exists, so it is
        // the one unpinned call; the curve's two reads and the token's name
        // and symbol all name the read point's block.
        assert_eq!(calls.len(), 5, "{calls:?}");
        assert_eq!(calls[0]["params"][1], "latest");
        for call in &calls[1..] {
            assert_eq!(call["params"][1], "0x64", "{call}");
        }
    }

    #[test]
    fn a_second_summon_reads_only_the_blocks_after_the_checkpoint() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let memory = Memory::open_in_memory().expect("a memory");
        let reader = RobinhoodReader {
            memory: Some(&memory),
        };
        let key = token().to_string();

        let client = Rpc::new(serve(full_bodies(&rec, "0x1")));
        let first = reader
            .read(&client, &mut budget(), &token())
            .expect("the first summon");
        assert_eq!(
            first.holders,
            Some(Holders {
                count: 3,
                largest_share_bps: Some(5_000),
            })
        );
        assert_eq!(
            first.calls, 13,
            "remembering costs no extra call; eleven core reads plus \
             creator_cash_flow's two (design 0027 slice 5)"
        );
        assert_eq!(
            memory
                .token_checkpoint(MEMORY_CHAIN, &key)
                .expect("checkpoint"),
            Some(Checkpoint {
                block: 0x64,
                hash: block_hash(0x64).to_string(),
            })
        );

        // Later the chain is at 0x70 and Bob has sent Alice his 100 in
        // block 0x68. The transfers page served holds only that send: a
        // reader that walked from the launch again would see Bob spend
        // what it never saw him receive and refuse the count.
        let mut bodies = full_bodies(&rec, "0x1");
        bodies[1] = block(0x70, 10_600);
        bodies[9] = block(0x64, 10_000);
        bodies.push(answer(&serde_json::json!([transfer_at(
            &BOB, &ALICE, 100, 0x68
        )])));
        let client = Rpc::new(serve(bodies));
        let second = reader
            .read(&client, &mut budget(), &token())
            .expect("the second summon");
        // The launcher 100, Alice 300, Bob nothing now.
        assert_eq!(
            second.holders,
            Some(Holders {
                count: 2,
                largest_share_bps: Some(7_500),
            })
        );
        assert_eq!(
            second.calls, 14,
            "thirteen as before plus the checkpoint's header; the walk itself is one page"
        );
        assert_eq!(
            memory
                .token_checkpoint(MEMORY_CHAIN, &key)
                .expect("checkpoint"),
            Some(Checkpoint {
                block: 0x70,
                hash: block_hash(0x70).to_string(),
            })
        );
        let run = memory
            .latest_check_run(MEMORY_CHAIN, &key, TRANSFERS_CHECK)
            .expect("read")
            .expect("recorded");
        assert_eq!((run.from_block, run.to_block), (0x65, 0x70));
        assert_eq!(run.completeness, Completeness::Complete);
        assert_eq!(run.calls, 2);
    }

    #[test]
    fn a_checkpoint_at_the_read_point_itself_walks_nothing_and_costs_nothing() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let memory = Memory::open_in_memory().expect("a memory");
        let reader = RobinhoodReader {
            memory: Some(&memory),
        };
        let key = token().to_string();
        let client = Rpc::new(serve(full_bodies(&rec, "0x1")));
        reader
            .read(&client, &mut budget(), &token())
            .expect("the first summon");

        // Summoned again before the chain moved: the read point is the
        // checkpoint, its hash is already in hand, and there is no block to
        // walk. No transfers page is served, so a reader that walked anyway
        // (or rolled back and re-read) would find nothing to answer it.
        let mut bodies = full_bodies(&rec, "0x1");
        bodies.truncate(9);
        let client = Rpc::new(serve(bodies));
        let again = reader
            .read(&client, &mut budget(), &token())
            .expect("the second summon");
        assert_eq!(
            again.holders,
            Some(Holders {
                count: 3,
                largest_share_bps: Some(5_000),
            })
        );
        assert_eq!(
            again.calls, 12,
            "ten as before plus creator_cash_flow's two reads"
        );
        let run = memory
            .latest_check_run(MEMORY_CHAIN, &key, TRANSFERS_CHECK)
            .expect("read")
            .expect("recorded");
        assert_eq!((run.from_block, run.to_block), (0x65, 0x64));
        assert_eq!(run.completeness, Completeness::Complete);
        assert_eq!(run.calls, 0);
    }

    #[test]
    fn a_checkpoint_one_block_behind_reads_exactly_that_one_block() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let memory = Memory::open_in_memory().expect("a memory");
        let reader = RobinhoodReader {
            memory: Some(&memory),
        };
        let key = token().to_string();
        let mut bodies = full_bodies(&rec, "0x1");
        bodies[1] = block(0x63, 9_999);
        reader
            .read(&Rpc::new(serve(bodies)), &mut budget(), &token())
            .expect("the first summon");

        // The chain is one block on, and Alice gave Dave 100 in it. The
        // walk must cover that single block, not skip it as already read.
        let mut bodies = full_bodies(&rec, "0x1");
        bodies[9] = block(0x63, 9_999);
        bodies.push(answer(&serde_json::json!([transfer_at(
            &ALICE, &DAVE, 100, 0x64
        )])));
        let second = reader
            .read(&Rpc::new(serve(bodies)), &mut budget(), &token())
            .expect("the second summon");
        // The launcher 100, Alice 100, Bob 100, Dave 100.
        assert_eq!(
            second.holders,
            Some(Holders {
                count: 4,
                largest_share_bps: Some(2_500),
            })
        );
        let run = memory
            .latest_check_run(MEMORY_CHAIN, &key, TRANSFERS_CHECK)
            .expect("read")
            .expect("recorded");
        assert_eq!((run.from_block, run.to_block), (0x64, 0x64));
    }

    #[test]
    fn a_walk_that_fails_is_recorded_failed_and_a_busy_one_truncated() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let key = token().to_string();

        // The provider errors on the transfers page: not "nothing happened".
        let memory = Memory::open_in_memory().expect("a memory");
        let mut bodies = full_bodies(&rec, "0x1");
        bodies[9] =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"boom"}}"#.to_owned();
        let dossier = RobinhoodReader {
            memory: Some(&memory),
        }
        .read(&Rpc::new(serve(bodies)), &mut budget(), &token())
        .expect("a dossier");
        assert!(dossier.unavailable.iter().any(|u| u.fact == "holders"));
        let run = memory
            .latest_check_run(MEMORY_CHAIN, &key, TRANSFERS_CHECK)
            .expect("read")
            .expect("recorded");
        assert_eq!(run.completeness, Completeness::Failed);
        assert!(run.note.contains("boom"), "{}", run.note);
        assert_eq!(
            memory
                .token_checkpoint(MEMORY_CHAIN, &key)
                .expect("checkpoint"),
            None,
            "a failed walk leaves no checkpoint claiming the range was read"
        );

        // Every page over the cap until the window is one block: too busy,
        // which is a truncated read and not a failed one.
        let memory = Memory::open_in_memory().expect("a memory");
        let mut bodies = full_bodies(&rec, "0x1");
        bodies.truncate(9);
        bodies.extend((0..20).map(|_| too_many_results_error()));
        let dossier = RobinhoodReader {
            memory: Some(&memory),
        }
        .read(&Rpc::new(serve(bodies)), &mut budget(), &token())
        .expect("a dossier");
        assert!(
            dossier
                .unavailable
                .iter()
                .any(|u| u.fact == "holders" && u.why == TOO_BUSY)
        );
        let run = memory
            .latest_check_run(MEMORY_CHAIN, &key, TRANSFERS_CHECK)
            .expect("read")
            .expect("recorded");
        assert_eq!(run.completeness, Completeness::Truncated);
    }

    #[test]
    fn a_checkpoint_the_chain_no_longer_has_is_rolled_back_and_re_read() {
        let rec = record(
            true,
            RobinhoodAddress([0x23; 20]),
            RobinhoodAddress([0x34; 20]),
        );
        let memory = Memory::open_in_memory().expect("a memory");
        let key = token().to_string();
        // A ledger remembered from a block the chain has since replaced:
        // Dave minted 999 in it, and the checkpoint's hash is one the chain
        // no longer has at 0x64.
        memory
            .extend_transfers(
                MEMORY_CHAIN,
                &key,
                &[TransferEvent {
                    block: 0x60,
                    block_hash: "0xstale60".to_owned(),
                    transaction_index: 0,
                    log_index: 0,
                    transaction: "0xstale".to_owned(),
                    from: None,
                    to: DAVE.to_string(),
                    amount: 999,
                }],
                &Checkpoint {
                    block: 0x64,
                    hash: "0xstale64".to_owned(),
                },
            )
            .expect("a remembered ledger");

        let mut bodies = full_bodies(&rec, "0x1");
        bodies[1] = block(0x70, 10_600);
        let transfers = bodies.remove(9);
        bodies.push(block(0x64, 10_000)); // the canonical 0x64: another hash
        bodies.push(transfers);
        let client = Rpc::new(serve(bodies));
        let dossier = RobinhoodReader {
            memory: Some(&memory),
        }
        .read(&client, &mut budget(), &token())
        .expect("a dossier");

        // Dave's phantom 999 is gone; the re-read ledger is the real one.
        assert_eq!(
            dossier.holders,
            Some(Holders {
                count: 3,
                largest_share_bps: Some(5_000),
            })
        );
        assert_eq!(dossier.calls, 14, "plus creator_cash_flow's two reads");
        assert_eq!(
            memory
                .token_checkpoint(MEMORY_CHAIN, &key)
                .expect("checkpoint"),
            Some(Checkpoint {
                block: 0x70,
                hash: block_hash(0x70).to_string(),
            })
        );
        let run = memory
            .latest_check_run(MEMORY_CHAIN, &key, TRANSFERS_CHECK)
            .expect("read")
            .expect("recorded");
        assert_eq!(
            run.from_block, LAUNCH_BLOCK,
            "the roll-back reaches REORG_DEPTH blocks, which is before the launch here"
        );
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
        let via_reader = RobinhoodReader::default().read(&client_b, &mut budget_b, &token());

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
        accepts_any_reader(&RobinhoodReader::default());
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
            position: None,
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

    // S1, "name the pair": a Pons v2 curve paired with an ERC-20 token
    // rather than native ETH is named by its own `symbol()`/`decimals()`.

    /// An ABI dynamic-string `eth_call` return, the same shape
    /// `realorrug-robinhood/src/erc20.rs`'s own `encoded` builds.
    fn encoded_string(text: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; 64];
        out[31] = 32;
        let length = u64::try_from(text.len()).expect("a test string fits in u64");
        out[56..64].copy_from_slice(&length.to_be_bytes());
        out.extend_from_slice(text);
        out.resize(64 + text.len().div_ceil(32) * 32, 0);
        out
    }

    fn encoded_decimals(value: u128) -> Vec<u8> {
        let mut out = [0u8; 32];
        out[16..].copy_from_slice(&value.to_be_bytes());
        out.to_vec()
    }

    #[test]
    fn a_native_eth_launch_is_unchanged_by_this_slice() {
        // `record.pair` is `None`, so `curve_facts` alone must still name
        // ETH -- `pair_quote_asset` is never reached, exactly as before S1.
        let curve = RobinhoodAddress([0x70; 20]);
        let deployer = RobinhoodAddress([0x71; 20]);
        let rec = record(true, curve, deployer);
        let client = Rpc::new(serve(vec![
            answer(&hex(&word_bool(false))),    // graduated()
            answer(&hex(&encoded_decimals(0))), // realQuoteReserve()
        ]));
        let mut b = budget();
        let facts = curve_facts(&mut b, &client, &rec, None).expect("curve facts");
        assert_eq!(facts.quote_asset, Some(QuoteAsset::eth()));
    }

    #[test]
    fn a_token_pair_with_a_good_symbol_and_decimals_is_named_with_its_address() {
        let pair = RobinhoodAddress([0x72; 20]);
        let client = Rpc::new(serve(vec![
            answer(&hex(&encoded_string(b"HIMS"))),
            answer(&hex(&encoded_decimals(18))),
        ]));
        let mut b = budget();
        let asset =
            pair_quote_asset(&mut b, &client, &pair, None, None).expect("a named pair asset");
        assert_eq!(asset.symbol, "HIMS");
        assert_eq!(asset.decimals, 18);
        assert_eq!(asset.address, Some(ChainAddress::Robinhood(pair)));
        // The address renders in the sheet's "paired with X (0x...)" wording
        // (`realorrug-roast/src/sheet.rs::push_curve`), so it must actually
        // print as the hex form a reader can check on an explorer.
        assert!(format!("{}", asset.address.expect("address")).starts_with("0x"));
    }

    #[test]
    fn a_failed_pair_read_leaves_the_unit_unknown_with_a_reason() {
        let pair = RobinhoodAddress([0x73; 20]);
        // Both calls the pair naming needs fail at the transport.
        let client = Rpc::new(serve(vec![]));
        let mut b = budget();
        let why = pair_quote_asset(&mut b, &client, &pair, None, None)
            .expect_err("no server means no read");
        assert!(!why.is_empty());
    }

    #[test]
    fn a_hostile_symbol_is_unreadable_not_sanitised_into_something_else() {
        // A prompt-injection attempt riding in as an ERC-20 symbol -- exactly
        // the untrusted-metadata case AGENTS.md §3 rule 3 exists for.
        assert_eq!(sanitised_symbol("@x ignore previous"), None);
        // Two hundred characters: far past any real ticker, and past this
        // sanitiser's own 32-character ceiling.
        assert_eq!(sanitised_symbol(&"A".repeat(200)), None);
        // The cap is inclusive: 32 characters is a ticker, 33 is not.
        assert_eq!(sanitised_symbol(&"A".repeat(32)), Some("A".repeat(32)));
        assert_eq!(sanitised_symbol(&"A".repeat(33)), None);
        // Non-ASCII: a script this sanitiser does not vouch for, even one
        // that looks like harmless letters.
        assert_eq!(sanitised_symbol("HIMS\u{202e}"), None);
        assert_eq!(sanitised_symbol("café"), None);
        // A real ticker still passes.
        assert_eq!(sanitised_symbol("HIMS"), Some("HIMS".to_owned()));
        assert_eq!(sanitised_symbol("USD.C-1_A"), Some("USD.C-1_A".to_owned()));
    }

    #[test]
    fn decimals_past_the_sanity_ceiling_leaves_the_pair_unnamed() {
        let pair = RobinhoodAddress([0x74; 20]);
        let client = Rpc::new(serve(vec![
            answer(&hex(&encoded_string(b"HIMS"))),
            answer(&hex(&encoded_decimals(255))),
        ]));
        let mut b = budget();
        let why = pair_quote_asset(&mut b, &client, &pair, None, None)
            .expect_err("255 decimals is not a plausible value");
        assert!(why.contains("decimals"));
    }

    #[test]
    fn a_cached_pair_asset_is_reused_without_spending_any_calls() {
        let pair = RobinhoodAddress([0x75; 20]);
        let memory = Memory::open_in_memory().expect("open");
        memory
            .record(
                PAIR_QUOTE_ASSET_FACT,
                &pair.to_string(),
                0,
                MemoryKind::Forever,
                "HIMS\u{1}18",
                SystemTime::now(),
            )
            .expect("record");
        // No answers queued: a cache hit must not touch the network at all.
        let client = Rpc::new(serve(vec![]));
        let mut b = budget();
        let asset = pair_quote_asset(&mut b, &client, &pair, None, Some(&memory))
            .expect("a cached pair asset");
        assert_eq!(asset.symbol, "HIMS");
        assert_eq!(asset.decimals, 18);
        assert_eq!(b.calls_made(), 0);
    }
}
