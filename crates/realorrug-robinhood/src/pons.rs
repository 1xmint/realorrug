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
}

/// The selector of the factory's `getLaunchedToken(address)`.
pub const GET_LAUNCHED_TOKEN: [u8; 4] = [0x3c, 0xf2, 0x8b, 0x5a];

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
