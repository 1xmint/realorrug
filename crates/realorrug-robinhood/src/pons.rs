// SPDX-License-Identifier: Apache-2.0
//! Pons v2, the launcher: launches, curve trades and fee sweeps, decoded, and
//! the launch check ADR 0013 constraint 1 needs.
//!
//! [Research 0036](../../../docs/research/0036-pons-v2-read-from-a-real-launch.md)
//! is the evidence for every layout here. Pons's published source does not
//! match what is deployed, so a layout is trusted when a captured mainnet
//! transaction decodes under it, and not before; the tests in
//! `tests/pons_as_mainnet_wrote_it.rs` are those decodes.
//!
//! # What "clean" means, and why the mint is not enough
//!
//! Every Pons v2 launch mints its whole supply to its bonding curve, so "the
//! curve is the only recipient of the mint" is true of a launch with a dev buy
//! too. The captured launch `0x1013a302…` minted to its curve and then, in the
//! same transaction, sold 2.83% of supply to its launcher, free of the snipe
//! tax, after exempting three more wallets. [`check_launch`] refuses exactly
//! that: any trade on the curve in the launch transaction, any token transfer
//! but the mint, and any snipe-tax exemption beyond the two the factory grants
//! by itself.

use std::collections::BTreeMap;

use crate::{Address, Hash32, Log, Receipt, word, word_address, word_u128};

/// The Pons v2 launch factory on Robinhood Chain, from
/// [Pons's v2 docs](https://docs.ponsfamily.com/v2) and confirmed by the
/// captured launches naming it.
pub const FACTORY: Address = Address::from_hex("0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e");

/// Event signatures. Each is the Keccak-256 of the signature in its comment,
/// computed once and checked by a test against a captured log that carries it.
pub mod topic {
    use crate::Hash32;

    /// `TokenLaunched(address indexed token, address indexed curve, address indexed deployer, address pairToken, uint256 launchConfigId, uint256 graduationThreshold)`,
    /// from the factory.
    pub const TOKEN_LAUNCHED: Hash32 =
        Hash32::from_hex("0x8d4aad4953d0ca700d468f3753aa14432d1b35b43ec6409f051fb6aa43a89607");
    /// `CurveBuy(address indexed buyer, address indexed recipient, uint256 quoteIn, uint256 tokensOut, uint256 fee, uint256 tax)`.
    pub const CURVE_BUY: Hash32 =
        Hash32::from_hex("0xec36bf571f136799e8dc0b0b8bea4b04d8bd3d43de838aab0d5fc21d4cbfc455");
    /// `CurveSell(address indexed seller, address indexed recipient, uint256 tokensIn, uint256 quoteOut, uint256 fee, uint256 tax)`.
    pub const CURVE_SELL: Hash32 =
        Hash32::from_hex("0x8113d738abdcb6b38357e9d53a54a7157861a09031b453651f0fe7fe151f59df");
    /// `FeesSwept(uint256 protocolAmount, uint256 buybackAmount, uint256 creatorAmount)`.
    pub const FEES_SWEPT: Hash32 =
        Hash32::from_hex("0x9f4cd7c4ed99d08a797804560c9c5d71d2cf7e101f2e3b5e7d1ca8a24c370e4f");
    /// `SnipeTaxExempted(address indexed account)`. Not in Pons's published
    /// source; the name is the one whose hash the deployed curve emits.
    pub const SNIPE_TAX_EXEMPTED: Hash32 =
        Hash32::from_hex("0xe4b7e48fbd47c2f602bacadee76ad33b16542ddb4997cfc0de04c311adcfa8c7");
    /// ERC-20 `Transfer(address indexed from, address indexed to, uint256 value)`.
    pub const TRANSFER: Hash32 =
        Hash32::from_hex("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef");
    /// The factory's graduation event, emitted once per graduating token, one
    /// indexed field (the token) and two data words: quote raised and the
    /// token amount transferred from the curve to the factory in the same
    /// transaction (research 0040 §2, one captured transaction). The name
    /// `Graduated(address indexed token, uint256 quoteRaised, uint256
    /// tokensToFactory)` is inferred from the field values, not recovered
    /// from a verified source -- research 0040 §2 states this explicitly.
    pub const GRADUATED: Hash32 =
        Hash32::from_hex("0xcdb72f157fd3666758a6ce201387ffb52038c7562e4fff352828da1096c4b6b4");
}

/// The selector of the factory's `getLaunchedToken(address)`.
pub const GET_LAUNCHED_TOKEN: [u8; 4] = [0x3c, 0xf2, 0x8b, 0x5a];

/// The selector of the ERC-20 `balanceOf(address)`, read against a Pons v2
/// token itself -- not the curve, and not `escrow::BALANCE_OF`, which is the
/// identical four bytes read against the fee escrow instead. An ERC-20
/// selector does not depend on which contract or account it is called
/// against, so the same constant would be correct in both places; it is
/// named separately per module so a reader sees which contract a call site
/// means without following the import.
pub const BALANCE_OF: [u8; 4] = [0x70, 0xa0, 0x82, 0x31];

/// Call data for `balanceOf(account)` against any ERC-20, including a Pons
/// v2 token: the selector, then `account` left-zero-padded to a word.
#[must_use]
pub fn balance_of_call_data(account: &Address) -> Vec<u8> {
    curve::call_data_for(BALANCE_OF, account)
}

/// A `TokenLaunched` event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launched {
    /// The token.
    pub token: Address,
    /// Its bonding curve.
    pub curve: Address,
    /// Who launched it.
    pub deployer: Address,
    /// The quote asset, or `None` for native ETH. The creator is paid in it.
    pub pair: Option<Address>,
    /// Which launch configuration.
    pub config: u128,
    /// Quote reserve at which the curve graduates, in the pair asset's units.
    pub graduation_threshold: u128,
}

impl Launched {
    /// The launch a log records, if it is a `TokenLaunched` from the factory
    /// and every field reads.
    #[must_use]
    pub fn from_log(log: &Log) -> Option<Self> {
        if !log.is(&FACTORY, &topic::TOKEN_LAUNCHED) {
            return None;
        }
        let pair = word(&log.data, 0).and_then(word_address)?;
        Some(Self {
            token: log.topic_address(1)?,
            curve: log.topic_address(2)?,
            deployer: log.topic_address(3)?,
            pair: (pair != Address::ZERO).then_some(pair),
            config: log.data_u128(1)?,
            graduation_threshold: log.data_u128(2)?,
        })
    }
}

/// Which way a curve trade went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Quote in, tokens out.
    Buy,
    /// Tokens in, quote out.
    Sell,
}

/// A `CurveBuy` or `CurveSell` event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trade {
    /// Buy or sell.
    pub side: Side,
    /// The curve it traded against.
    pub curve: Address,
    /// Who called the curve: the trader, or a router acting for one.
    pub trader: Address,
    /// Who received the output.
    pub recipient: Address,
    /// Quote paid in (a buy) or paid out (a sell).
    pub quote: u128,
    /// Tokens paid out (a buy) or taken in (a sell).
    pub tokens: u128,
    /// The base fee, in quote; on a buy inside the snipe window this also
    /// carries the snipe tax (research 0036 §3).
    pub fee: u128,
    /// The creator tax, in quote.
    pub tax: u128,
}

impl Trade {
    /// The trade a log records, if it is a curve buy or sell and every field
    /// reads. Which contract emitted it is not checked here -- any address can
    /// emit these bytes -- so a caller holding a curve address compares it.
    #[must_use]
    pub fn from_log(log: &Log) -> Option<Self> {
        let side = match log.topics.first() {
            Some(t) if *t == topic::CURVE_BUY => Side::Buy,
            Some(t) if *t == topic::CURVE_SELL => Side::Sell,
            _ => return None,
        };
        let (first, second) = (log.data_u128(0)?, log.data_u128(1)?);
        let (quote, tokens) = match side {
            Side::Buy => (first, second),
            Side::Sell => (second, first),
        };
        Some(Self {
            side,
            curve: log.address,
            trader: log.topic_address(1)?,
            recipient: log.topic_address(2)?,
            quote,
            tokens,
            fee: log.data_u128(2)?,
            tax: log.data_u128(3)?,
        })
    }
}

/// A `FeesSwept` event: what one sweep paid, in the pair asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sweep {
    /// To the protocol.
    pub protocol: u128,
    /// Spent on buyback.
    pub buyback: u128,
    /// To the creator fee recipient.
    pub creator: u128,
}

impl Sweep {
    /// The sweep a log records, if it is a `FeesSwept` and every field reads.
    #[must_use]
    pub fn from_log(log: &Log) -> Option<Self> {
        if log.topics.first() != Some(&topic::FEES_SWEPT) {
            return None;
        }
        Some(Self {
            protocol: log.data_u128(0)?,
            buyback: log.data_u128(1)?,
            creator: log.data_u128(2)?,
        })
    }

    /// What a sweep of `trades` pays on a curve without buyback, whose
    /// protocol takes `protocol_share_bps` of the base fee.
    ///
    /// Measured, not taken from source (research 0036 §2): the protocol gets
    /// its share of the summed fee, rounded down, and the creator gets the rest
    /// of the fee plus all of the tax. `None` when a share above 100% is given
    /// or a sum overflows -- a number that cannot be right is not returned.
    ///
    /// A curve with buyback enabled splits the creator's side further, and no
    /// such sweep has been captured, so this does not model it.
    #[must_use]
    pub fn expected(trades: &[Trade], protocol_share_bps: u16) -> Option<Self> {
        if protocol_share_bps > 10_000 {
            return None;
        }
        let mut fee: u128 = 0;
        let mut tax: u128 = 0;
        for t in trades {
            fee = fee.checked_add(t.fee)?;
            tax = tax.checked_add(t.tax)?;
        }
        let protocol = fee.checked_mul(u128::from(protocol_share_bps))? / 10_000;
        Some(Self {
            protocol,
            buyback: 0,
            creator: (fee - protocol).checked_add(tax)?,
        })
    }
}

/// The factory's record of one launch, from `getLaunchedToken(token)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchedToken {
    /// The token.
    pub token: Address,
    /// Its bonding curve.
    pub curve: Address,
    /// Who launched it.
    pub deployer: Address,
    /// Who the creator's fees are paid to.
    pub creator_fee_recipient: Address,
    /// The quote asset, or `None` for native ETH.
    pub pair: Option<Address>,
    /// Quote reserve at which the curve graduates.
    pub graduation_threshold: u128,
    /// The creator tax, chosen once at launch.
    pub creator_tax_bps: u16,
    /// Whether part of the creator's side of the fee buys back tokens.
    pub buyback: bool,
    /// The launch's phase. Documented only as an integer: research 0040 §1
    /// observed `0` pre-graduation and `2` post-graduation on three real
    /// tokens, so `0` is **inferred** to mean "still on the curve" (design
    /// 0020 §3), but this is inference from the field's position and use in
    /// `getLaunchedToken`'s return, not from a captured state transition.
    /// What `1` means, if it is ever used, is not established.
    pub phase: u8,
    /// Whether the factory knows the token at all.
    pub exists: bool,
}

impl LaunchedToken {
    /// The record from the call's return bytes.
    ///
    /// The struct is fifteen static words: token, curve, deployer, fee
    /// recipient, pair, threshold, pool fee, tick spacing, creator tax,
    /// buyback, phase, swept quote, swept tokens, swept at, exists. Layout
    /// confirmed by the captured calls (research 0036).
    #[must_use]
    pub fn from_return(data: &[u8]) -> Option<Self> {
        let address = |i| word(data, i).and_then(word_address);
        let flag = |i| match word(data, i).and_then(word_u128) {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => None,
        };
        let pair = address(4)?;
        Some(Self {
            token: address(0)?,
            curve: address(1)?,
            deployer: address(2)?,
            creator_fee_recipient: address(3)?,
            pair: (pair != Address::ZERO).then_some(pair),
            graduation_threshold: word(data, 5).and_then(word_u128)?,
            creator_tax_bps: word(data, 8)
                .and_then(word_u128)
                .and_then(|v| u16::try_from(v).ok())?,
            buyback: flag(9)?,
            phase: word(data, 10)
                .and_then(word_u128)
                .and_then(|v| u8::try_from(v).ok())?,
            exists: flag(14)?,
        })
    }

    /// The call data that asks the factory for `token`'s record.
    #[must_use]
    pub fn call_data(token: &Address) -> Vec<u8> {
        let mut data = Vec::with_capacity(36);
        data.extend_from_slice(&GET_LAUNCHED_TOKEN);
        data.extend_from_slice(&[0; 12]);
        data.extend_from_slice(&token.0);
        data
    }
}

/// One reason a launch transaction is not a clean launch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unclean {
    /// The transaction reverted, so it launched nothing.
    Failed,
    /// No readable `TokenLaunched` from the factory.
    NoLaunch,
    /// More than one launch in one transaction.
    ManyLaunches(usize),
    /// The factory's record is for a different token or curve than the event.
    RecordMismatch,
    /// The token was not minted to its curve in this transaction.
    NoMint,
    /// A token transfer other than the one mint to the curve.
    TokenMoved {
        /// Sender.
        from: Address,
        /// Receiver.
        to: Address,
        /// Amount, when it reads.
        amount: Option<u128>,
    },
    /// A trade on the curve inside the launch transaction: a dev buy, or a sell.
    Traded {
        /// Who received the output.
        recipient: Address,
        /// Tokens moved.
        tokens: u128,
    },
    /// A snipe-tax exemption beyond the deployer and the creator fee recipient:
    /// a wallet allowed to buy in the launch window at the untaxed price.
    Exempted(Address),
    /// A log from the curve or token that should decode and does not.
    Unreadable(Hash32),
}

/// Whether a launch transaction is clean, per ADR 0013 constraint 1.
///
/// `record` is the factory's `getLaunchedToken` for the launched token, which
/// names the creator fee recipient the factory exempts by itself. Every reason
/// the launch is unclean is returned, not only the first: the operator reading
/// a refusal should see the whole of it.
///
/// # Errors
///
/// Every [`Unclean`] reason found.
pub fn check_launch(receipt: &Receipt, record: &LaunchedToken) -> Result<Launched, Vec<Unclean>> {
    if !receipt.succeeded {
        return Err(vec![Unclean::Failed]);
    }
    let launches: Vec<&Log> = receipt
        .logs
        .iter()
        .filter(|l| l.is(&FACTORY, &topic::TOKEN_LAUNCHED))
        .collect();
    let launch = match launches.as_slice() {
        [] => return Err(vec![Unclean::NoLaunch]),
        [one] => Launched::from_log(one).ok_or_else(|| vec![Unclean::NoLaunch])?,
        many => return Err(vec![Unclean::ManyLaunches(many.len())]),
    };
    let mut unclean = Vec::new();
    if record.token != launch.token || record.curve != launch.curve {
        unclean.push(Unclean::RecordMismatch);
    }

    let mut minted = false;
    for log in &receipt.logs {
        if log.is(&launch.token, &topic::TRANSFER) {
            match (log.topic_address(1), log.topic_address(2)) {
                (Some(from), Some(to)) => {
                    let amount = log.data_u128(0);
                    if from == Address::ZERO && to == launch.curve && amount.is_some() && !minted {
                        minted = true;
                    } else {
                        unclean.push(Unclean::TokenMoved { from, to, amount });
                    }
                }
                _ => unclean.push(Unclean::Unreadable(log.topics[0])),
            }
        } else if log.address == launch.curve {
            let event = log.topics.first().copied();
            if event == Some(topic::CURVE_BUY) || event == Some(topic::CURVE_SELL) {
                match Trade::from_log(log) {
                    Some(t) => unclean.push(Unclean::Traded {
                        recipient: t.recipient,
                        tokens: t.tokens,
                    }),
                    None => unclean.push(Unclean::Unreadable(log.topics[0])),
                }
            } else if event == Some(topic::SNIPE_TAX_EXEMPTED) {
                match log.topic_address(1) {
                    Some(a) if a == launch.deployer || a == record.creator_fee_recipient => {}
                    Some(a) => unclean.push(Unclean::Exempted(a)),
                    None => unclean.push(Unclean::Unreadable(topic::SNIPE_TAX_EXEMPTED)),
                }
            }
        }
    }
    if !minted {
        unclean.insert(0, Unclean::NoMint);
    }
    if unclean.is_empty() {
        Ok(launch)
    } else {
        Err(unclean)
    }
}

/// A `Graduated` event from the factory: whether a token graduated, and the
/// two words its graduation carried. `phase == 2` on `getLaunchedToken`
/// already answers "whether" (design 0020 §1); this answers "when, and how
/// much" from the same event.
///
/// Layout confirmed by one captured transaction (research 0040 §2). The
/// event's real name and full signature were not recovered from a verified
/// source, only the topic hash and these two decoded data words, so the
/// field names below are this crate's own, not the deployed contract's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Graduated {
    /// The token that graduated.
    pub token: Address,
    /// The quote raised when the threshold was crossed, in the pair asset's
    /// units -- observed a few hundred wei over the round graduation
    /// threshold, not exactly equal to it (research 0040 §2).
    pub quote_raised: u128,
    /// The token amount transferred from the curve to the factory in the
    /// same transaction as the event (research 0040 §2: identical to the
    /// captured `Transfer(curve, factory, …)` log's value).
    pub tokens_to_factory: u128,
}

impl Graduated {
    /// The graduation a log records, if it is a `Graduated` from the factory
    /// and every field reads.
    #[must_use]
    pub fn from_log(log: &Log) -> Option<Self> {
        if !log.is(&FACTORY, &topic::GRADUATED) {
            return None;
        }
        Some(Self {
            token: log.topic_address(1)?,
            quote_raised: log.data_u128(0)?,
            tokens_to_factory: log.data_u128(1)?,
        })
    }
}

/// An ERC-20 `Transfer` log, decoded generically.
///
/// Which contract emitted it is not checked here, the same choice
/// [`Trade::from_log`] makes: a caller reading a token's own logs already
/// scoped the `eth_getLogs` call to that token's address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transfer {
    /// The sender. The zero address for a mint.
    pub from: Address,
    /// The receiver. The zero address for a burn.
    pub to: Address,
    /// Token base units.
    pub amount: u128,
}

impl Transfer {
    /// The transfer a log records, if it is an ERC-20 `Transfer` and every
    /// field reads.
    #[must_use]
    pub fn from_log(log: &Log) -> Option<Self> {
        if log.topics.first() != Some(&topic::TRANSFER) {
            return None;
        }
        Some(Self {
            from: log.topic_address(1)?,
            to: log.topic_address(2)?,
            amount: log.data_u128(0)?,
        })
    }
}

/// The net balance per address a set of already-fetched `Transfer` logs
/// implies.
///
/// Pure: takes logs, does not fetch them (§1's holder-count and
/// largest-non-curve-holder facts are built on this). The zero address is
/// not tracked as a holder -- it is where a mint comes from and a burn
/// goes to, never an account -- so it never appears as a key.
///
/// `None` only when an amount does not fit `i128` or a running balance
/// overflows `i128`; realistic token amounts (word_u128's own comment: a
/// billion-token supply at 18 decimals is 10^27) are far inside that range,
/// so this is a data-integrity signal, not an expected path.
///
/// A negative entry in the returned map is returned as-is, not clamped to
/// zero: it means the log set handed to this function is incomplete -- a
/// `Transfer` out was counted without the `Transfer` in that funded it --
/// and AGENTS.md §3 rule 8 says an absent read must not be reported as a
/// safe zero. The caller decides what an incomplete log set means for the
/// fact it is building; this function only refuses to hide it.
#[must_use]
pub fn holdings(transfers: &[Transfer]) -> Option<BTreeMap<Address, i128>> {
    let mut balances: BTreeMap<Address, i128> = BTreeMap::new();
    for t in transfers {
        let amount = i128::try_from(t.amount).ok()?;
        if t.from != Address::ZERO {
            let entry = balances.entry(t.from).or_insert(0);
            *entry = entry.checked_sub(amount)?;
        }
        if t.to != Address::ZERO {
            let entry = balances.entry(t.to).or_insert(0);
            *entry = entry.checked_add(amount)?;
        }
    }
    Some(balances)
}

/// Curve-side reads: reserves, graduation state, launch parameters and the
/// snipe tax, called directly on a Pons v2 curve rather than the factory.
///
/// # Where these selectors came from
///
/// Read off the **deployed bytecode** of the live curve
/// `0x008089e243a611ace236fc4e2127403a3c9e347b` on 2026-09-15, by extracting
/// the solc dispatch pattern `DUP1 PUSH4 <selector> EQ` and resolving each
/// against openchain.xyz's signature database (task packet 0024). They are
/// **verified present in that contract's dispatch table**; nothing here is
/// confirmed against a captured call unless its own doc comment says so.
pub mod curve {
    use crate::{Address, word, word_u128};

    /// `getReserves()`. Return shape **not established** -- see
    /// [`Reserves::from_return`].
    pub const GET_RESERVES: [u8; 4] = [0x09, 0x02, 0xf1, 0xac];
    /// `quoteReserve()`, returns `uint`. Preferred over `getReserves()`:
    /// unambiguous, a single word.
    pub const QUOTE_RESERVE: [u8; 4] = [0x9d, 0xa7, 0x71, 0xf4];
    /// `tokenReserve()`, returns `uint`. Preferred over `getReserves()`:
    /// unambiguous, a single word.
    pub const TOKEN_RESERVE: [u8; 4] = [0xcb, 0xcb, 0x31, 0x71];
    /// `realQuoteReserve()`, returns `uint`.
    pub const REAL_QUOTE_RESERVE: [u8; 4] = [0x4f, 0x1f, 0x58, 0xfd];
    /// `phantomQuote()`, returns `uint`.
    pub const PHANTOM_QUOTE: [u8; 4] = [0xc5, 0x7e, 0xad, 0xfc];
    /// `graduated()`, returns `bool`. A curve-side view of the same fact
    /// `LaunchedToken::phase == 2` reads from the factory.
    pub const GRADUATED: [u8; 4] = [0xe7, 0xc2, 0xb7, 0x72];
    /// `readyToGraduate()`, returns `bool`.
    pub const READY_TO_GRADUATE: [u8; 4] = [0xc6, 0x83, 0x60, 0xa5];
    /// `graduationThreshold()`, returns `uint`. Same fact
    /// `LaunchedToken::graduation_threshold` reads from the factory; this is
    /// the curve's own copy.
    pub const GRADUATION_THRESHOLD: [u8; 4] = [0x8b, 0x0b, 0xc5, 0x01];
    /// `launchedAt()`, returns `uint`. Unit **not established**: seconds or
    /// block number. A decode of this word is named `launched_at`, never
    /// `block` or `timestamp`, until a captured call against a known block
    /// time settles which it is.
    pub const LAUNCHED_AT: [u8; 4] = [0xbf, 0x56, 0xb3, 0x71];
    /// `launchSupply()`, returns `uint`.
    pub const LAUNCH_SUPPLY: [u8; 4] = [0x3f, 0x7e, 0xd6, 0xb7];
    /// `sellableTokens()`, returns `uint`.
    pub const SELLABLE_TOKENS: [u8; 4] = [0x80, 0x8b, 0xcd, 0xdc];
    /// `snipeTaxExempt(address)`, returns `bool`. Takes one `address`
    /// argument -- [`call_data_for`], not [`call_data`].
    pub const SNIPE_TAX_EXEMPT: [u8; 4] = [0xd4, 0x4b, 0xdf, 0xe7];
    /// `currentSnipeTaxBps(address)`, returns `uint`. Takes one `address`
    /// argument -- [`call_data_for`], not [`call_data`].
    pub const CURRENT_SNIPE_TAX_BPS: [u8; 4] = [0xd7, 0xe1, 0xef, 0x39];
    /// `snipeTaxStartBps()`, returns `uint`.
    pub const SNIPE_TAX_START_BPS: [u8; 4] = [0x50, 0xe2, 0x5a, 0xc2];
    /// `snipeTaxSeconds()`, returns `uint`.
    pub const SNIPE_TAX_SECONDS: [u8; 4] = [0x67, 0x83, 0x77, 0x4b];

    /// Call data for a no-argument curve read: the selector alone, four bytes.
    #[must_use]
    pub fn call_data(selector: [u8; 4]) -> Vec<u8> {
        selector.to_vec()
    }

    /// Call data for a curve read taking one `address`
    /// ([`SNIPE_TAX_EXEMPT`], [`CURRENT_SNIPE_TAX_BPS`]): the selector, then
    /// the address left-zero-padded to a word. Same pattern as
    /// [`super::LaunchedToken::call_data`].
    #[must_use]
    pub fn call_data_for(selector: [u8; 4], account: &Address) -> Vec<u8> {
        let mut data = Vec::with_capacity(36);
        data.extend_from_slice(&selector);
        data.extend_from_slice(&[0; 12]);
        data.extend_from_slice(&account.0);
        data
    }

    /// A single-word `uint` return, e.g. `quoteReserve()`, `tokenReserve()`,
    /// `launchedAt()`, `sellableTokens()`.
    ///
    /// `None` when the return is not exactly one word, or the word does not
    /// fit `u128` (the same rule [`word_u128`] enforces everywhere else in
    /// this crate) -- a short or oversized return gets no partial credit.
    #[must_use]
    pub fn uint_return(data: &[u8]) -> Option<u128> {
        if data.len() != 32 {
            return None;
        }
        word(data, 0).and_then(word_u128)
    }

    /// A single-word `bool` return, e.g. `graduated()`, `readyToGraduate()`,
    /// `snipeTaxExempt(address)`.
    ///
    /// Accepts **only** a word of exactly 0 or 1, the same rule
    /// `LaunchedToken::from_return`'s private `flag` closure applies: any
    /// other word is `None`, because a malformed return is not a `false`.
    #[must_use]
    pub fn bool_return(data: &[u8]) -> Option<bool> {
        if data.len() != 32 {
            return None;
        }
        match word(data, 0).and_then(word_u128) {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => None,
        }
    }

    /// `getReserves()`'s return.
    ///
    /// **Not confirmed against a captured call.** The name matches Uniswap
    /// V2's `(uint112, uint112, uint32)`, but this is a different contract
    /// and nothing here confirms that layout. Decoded as a length-checked
    /// read of three ABI words (96 bytes, the shape a Solidity `(uint112,
    /// uint112, uint32)` or `(uint256, uint256, uint256)` return both take
    /// once ABI-encoded): `None` on any other length. Prefer
    /// [`QUOTE_RESERVE`] and [`TOKEN_RESERVE`] as the primary path -- those
    /// are unambiguous.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Reserves {
        /// The first reserve word, decoded as `u128` (wide enough for a
        /// `uint112` with room to spare).
        pub reserve0: u128,
        /// The second reserve word.
        pub reserve1: u128,
        /// The third word, if it fits a `u32` the way Uniswap V2's trailing
        /// timestamp does. `None` when it does not -- this is not confirmed
        /// to be a timestamp at all.
        pub third_word_as_u32: Option<u32>,
    }

    impl Reserves {
        /// The reserves a `getReserves()` return holds, if it is exactly
        /// three words and the first two fit `u128`. `None` on any other
        /// length: a return that does not fit the assumed shape is refused,
        /// not truncated or padded.
        #[must_use]
        pub fn from_return(data: &[u8]) -> Option<Self> {
            if data.len() != 96 {
                return None;
            }
            let reserve0 = word(data, 0).and_then(word_u128)?;
            let reserve1 = word(data, 1).and_then(word_u128)?;
            let third_word_as_u32 = word(data, 2)
                .and_then(word_u128)
                .and_then(|v| u32::try_from(v).ok());
            Some(Self {
                reserve0,
                reserve1,
                third_word_as_u32,
            })
        }
    }
}

#[cfg(test)]
mod curve_read_tests {
    use super::curve::{
        CURRENT_SNIPE_TAX_BPS, GET_RESERVES, GRADUATED, GRADUATION_THRESHOLD, LAUNCH_SUPPLY,
        LAUNCHED_AT, PHANTOM_QUOTE, QUOTE_RESERVE, READY_TO_GRADUATE, REAL_QUOTE_RESERVE, Reserves,
        SELLABLE_TOKENS, SNIPE_TAX_EXEMPT, SNIPE_TAX_SECONDS, SNIPE_TAX_START_BPS, TOKEN_RESERVE,
        bool_return, call_data, call_data_for, uint_return,
    };
    use super::{
        Address, BALANCE_OF, Graduated, Log, Transfer, balance_of_call_data, holdings, topic,
    };
    use crate::Hash32;

    /// Every selector from task packet 0024's table, read off the deployed
    /// curve's bytecode -- one test catching any transcription slip.
    #[test]
    fn selectors_match_the_verified_table() {
        let table: [([u8; 4], [u8; 4]); 15] = [
            (GET_RESERVES, [0x09, 0x02, 0xf1, 0xac]),
            (QUOTE_RESERVE, [0x9d, 0xa7, 0x71, 0xf4]),
            (TOKEN_RESERVE, [0xcb, 0xcb, 0x31, 0x71]),
            (REAL_QUOTE_RESERVE, [0x4f, 0x1f, 0x58, 0xfd]),
            (PHANTOM_QUOTE, [0xc5, 0x7e, 0xad, 0xfc]),
            (GRADUATED, [0xe7, 0xc2, 0xb7, 0x72]),
            (READY_TO_GRADUATE, [0xc6, 0x83, 0x60, 0xa5]),
            (GRADUATION_THRESHOLD, [0x8b, 0x0b, 0xc5, 0x01]),
            (LAUNCHED_AT, [0xbf, 0x56, 0xb3, 0x71]),
            (LAUNCH_SUPPLY, [0x3f, 0x7e, 0xd6, 0xb7]),
            (SELLABLE_TOKENS, [0x80, 0x8b, 0xcd, 0xdc]),
            (SNIPE_TAX_EXEMPT, [0xd4, 0x4b, 0xdf, 0xe7]),
            (CURRENT_SNIPE_TAX_BPS, [0xd7, 0xe1, 0xef, 0x39]),
            (SNIPE_TAX_START_BPS, [0x50, 0xe2, 0x5a, 0xc2]),
            (SNIPE_TAX_SECONDS, [0x67, 0x83, 0x77, 0x4b]),
        ];
        for (got, expected) in table {
            assert_eq!(got, expected);
        }
        assert_eq!(BALANCE_OF, [0x70, 0xa0, 0x82, 0x31]);
    }

    #[test]
    fn no_argument_call_data_is_four_bytes_of_selector() {
        let data = call_data(QUOTE_RESERVE);
        assert_eq!(data.len(), 4);
        assert_eq!(data, QUOTE_RESERVE.to_vec());
    }

    #[test]
    fn address_argument_call_data_is_selector_then_padded_word() {
        let account = Address([0x11; 20]);
        let data = call_data_for(SNIPE_TAX_EXEMPT, &account);
        assert_eq!(data.len(), 36);
        assert_eq!(&data[..4], &SNIPE_TAX_EXEMPT);
        assert_eq!(&data[4..16], &[0u8; 12]);
        assert_eq!(&data[16..], &account.0);
    }

    #[test]
    fn balance_of_call_data_is_the_same_shape() {
        let account = Address([0x22; 20]);
        let data = balance_of_call_data(&account);
        assert_eq!(data.len(), 36);
        assert_eq!(&data[..4], &BALANCE_OF);
        assert_eq!(&data[16..], &account.0);
    }

    #[test]
    fn uint_return_rejects_a_short_or_long_return() {
        assert_eq!(uint_return(&[0u8; 16]), None, "short");
        assert_eq!(uint_return(&[0u8; 64]), None, "long");
        assert_eq!(uint_return(&[]), None, "empty");
    }

    #[test]
    fn uint_return_decodes_a_full_word() {
        let mut word = [0u8; 32];
        word[31] = 42;
        assert_eq!(uint_return(&word), Some(42));
    }

    #[test]
    fn bool_return_rejects_a_short_return() {
        assert_eq!(bool_return(&[0u8; 8]), None);
    }

    #[test]
    fn bool_return_accepts_only_zero_or_one() {
        let mut word = [0u8; 32];
        assert_eq!(bool_return(&word), Some(false));
        word[31] = 1;
        assert_eq!(bool_return(&word), Some(true));
        word[31] = 2;
        assert_eq!(
            bool_return(&word),
            None,
            "a malformed return is not a false"
        );
    }

    #[test]
    fn reserves_from_return_rejects_anything_but_three_words() {
        assert_eq!(Reserves::from_return(&[0u8; 64]), None, "two words");
        assert_eq!(Reserves::from_return(&[0u8; 128]), None, "four words");
        assert_eq!(Reserves::from_return(&[]), None, "empty");
    }

    #[test]
    fn reserves_from_return_decodes_three_words() {
        let mut data = [0u8; 96];
        data[31] = 10; // reserve0
        data[63] = 20; // reserve1
        data[92..96].copy_from_slice(&300u32.to_be_bytes()); // third word
        let reserves = Reserves::from_return(&data).expect("three words decode");
        assert_eq!(reserves.reserve0, 10);
        assert_eq!(reserves.reserve1, 20);
        assert_eq!(reserves.third_word_as_u32, Some(300));
    }

    #[test]
    fn reserves_from_return_leaves_the_third_word_none_when_it_does_not_fit_u32() {
        let mut data = [0u8; 96];
        data[31] = 10;
        data[63] = 20;
        data[64] = 1; // a high byte in the third word: too big for u32
        let reserves = Reserves::from_return(&data).expect("first two words still decode");
        assert_eq!(reserves.third_word_as_u32, None);
    }

    fn log(address: Address, topics: Vec<Hash32>, data: Vec<u8>) -> Log {
        Log {
            address,
            topics,
            data,
            block: 1,
            transaction: Hash32([0; 32]),
        }
    }

    fn word_addr(a: &Address) -> [u8; 32] {
        let mut w = [0u8; 32];
        w[12..].copy_from_slice(&a.0);
        w
    }

    fn word_u(v: u128) -> [u8; 32] {
        let mut w = [0u8; 32];
        w[16..].copy_from_slice(&v.to_be_bytes());
        w
    }

    #[test]
    fn graduated_from_log_requires_the_factory_and_the_topic() {
        let token = Address([0x33; 20]);
        let mut data = Vec::new();
        data.extend_from_slice(&word_u(1));
        data.extend_from_slice(&word_u(2));
        let not_factory = log(
            Address([0x44; 20]),
            vec![topic::GRADUATED, Hash32(word_addr(&token))],
            data.clone(),
        );
        assert_eq!(Graduated::from_log(&not_factory), None);

        let wrong_topic = log(
            super::FACTORY,
            vec![topic::TRANSFER, Hash32(word_addr(&token))],
            data.clone(),
        );
        assert_eq!(Graduated::from_log(&wrong_topic), None);

        let good = log(
            super::FACTORY,
            vec![topic::GRADUATED, Hash32(word_addr(&token))],
            data,
        );
        assert_eq!(
            Graduated::from_log(&good),
            Some(Graduated {
                token,
                quote_raised: 1,
                tokens_to_factory: 2,
            })
        );
    }

    #[test]
    fn transfer_from_log_requires_the_topic_and_two_addresses() {
        let from = Address([0x01; 20]);
        let to = Address([0x02; 20]);
        let mut data = Vec::new();
        data.extend_from_slice(&word_u(500));
        let good = log(
            Address([0x99; 20]),
            vec![
                topic::TRANSFER,
                Hash32(word_addr(&from)),
                Hash32(word_addr(&to)),
            ],
            data.clone(),
        );
        assert_eq!(
            Transfer::from_log(&good),
            Some(Transfer {
                from,
                to,
                amount: 500
            })
        );

        let wrong_topic = log(
            Address([0x99; 20]),
            vec![
                topic::GRADUATED,
                Hash32(word_addr(&from)),
                Hash32(word_addr(&to)),
            ],
            data,
        );
        assert_eq!(Transfer::from_log(&wrong_topic), None);
    }

    #[test]
    fn holdings_sums_transfers_and_skips_the_zero_address() {
        let a = Address([0xaa; 20]);
        let b = Address([0xbb; 20]);
        let transfers = vec![
            Transfer {
                from: Address::ZERO,
                to: a,
                amount: 100,
            },
            Transfer {
                from: a,
                to: b,
                amount: 40,
            },
        ];
        let balances = holdings(&transfers).expect("fits i128");
        assert_eq!(balances.get(&a), Some(&60));
        assert_eq!(balances.get(&b), Some(&40));
        assert_eq!(
            balances.get(&Address::ZERO),
            None,
            "the zero address is not a holder"
        );
    }

    #[test]
    fn holdings_returns_a_visible_negative_balance_rather_than_clamping_to_zero() {
        // A transfer out with no matching transfer in: the log set is
        // incomplete, and the caller must see that, not a false zero.
        let a = Address([0xcc; 20]);
        let b = Address([0xdd; 20]);
        let transfers = vec![Transfer {
            from: a,
            to: b,
            amount: 30,
        }];
        let balances = holdings(&transfers).expect("fits i128");
        assert_eq!(balances.get(&a), Some(&-30));
        assert_eq!(balances.get(&b), Some(&30));
    }
}
