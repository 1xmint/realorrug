// SPDX-License-Identifier: Apache-2.0
//! The fact sheet, and the bounded walk that fills it in.
//!
//! # What this is for
//!
//! Everything the public analyst may assert about one token, gathered from the
//! chain in a bounded number of calls, each figure carrying the slot it was read
//! at. Nothing here decides anything and nothing here is phrased: a
//! [`Dossier`] is *facts*, and Phase 2's verdict and voice sit on top of it.
//!
//! Keeping those apart is the same split `radar-signer` uses. The signer
//! re-reads the bytes it signs; the analyst re-reads the numbers it posts, and
//! it can only do that if the numbers exist as data before any sentence is
//! written about them.
//!
//! # Every field is optional, and that is the design
//!
//! A dossier for a mint whose curve could not be read is a dossier with no
//! curve, not a dossier with a curve of zero. AGENTS.md rule 9 — absent is not
//! zero, unknown is not safe — is why every fact below is an `Option` and why
//! [`Dossier::unavailable`] exists to say which ones are missing and why.
//! "Radar has no record" is a thing the analyst is expected to say plainly, and
//! it can only say it if the absence survives to the top.

use std::time::SystemTime;

use realorrug_pumpfun::curve::BondingCurve;
use realorrug_pumpfun::token::{TokenAccount, TokenProgram};
use realorrug_pumpfun::{Fees, pda};
use realorrug_types::{Address, ChainAddress, ReadAt, Slot};
use serde::{Deserialize, Serialize};

use crate::budget::{Budget, Count};
use crate::launch::{LaunchBlock, Metadata, NotALaunch};
use crate::memory::{Kind, Memory};
use crate::rpc::{RpcClient, RpcError, Transaction};
use crate::wallets::investigate_solana;

/// The impact budget capacity is measured at.
///
/// `Search::DEFAULT`'s 1%, so the number this reports is the same number the
/// rest of Radar reports. Research 0022 is why it must always be published *as*
/// a budget: the resulting figure is a Radar measurement at a chosen impact, and
/// **not a property of the token**. `STATE.md` and `GOAL.md` both described the
/// ~$31 it produces as a venue ceiling for weeks, and it is a setting.
pub const CAPACITY_IMPACT_BPS: u32 = 100;

/// A ceiling on the capacity search, in lamports.
///
/// Ten SOL. The search is a bisection and needs an upper bound; this one is far
/// above anything the curve supports pre-graduation, so it constrains the search
/// rather than the answer.
const CAPACITY_CEILING_LAMPORTS: u64 = 10_000_000_000;

/// Why a fact is missing.
///
/// Recorded per fact rather than as one global failure, because "the curve has
/// graduated" and "the endpoint timed out" lead to different replies and only
/// one of them is about the token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unavailable {
    /// Which fact.
    pub fact: &'static str,
    /// Why, in a form safe to publish.
    pub why: String,
}

/// A quote asset's display identity: what to call it, and how many decimal
/// places its smallest unit has.
///
/// Exists so that a figure and the unit it is in can never be separated --
/// `CurveFacts::quote_reserves` and `quote_capacity` are meaningless numbers on
/// their own (a lamport count and a wei count are both just integers), and this
/// type is what a renderer must be handed alongside them before it may print
/// anything. Set by each [`ChainReader`]: the Solana reader always says SOL
/// with 9 decimals; the Robinhood reader says ETH with 18 for a native-ETH
/// launch, and `None` -- never a guess -- when the launch record names a quote
/// token this reader cannot identify (rule 8: unknown is not safe, and a
/// guessed "ETH" label on a token that is not ETH would be exactly the
/// fabricated fact AGENTS.md section 3 rule 2 forbids).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuoteAsset {
    /// The symbol to print beside the amount, e.g. `"SOL"` or `"ETH"`.
    pub symbol: String,
    /// How many decimal places the smallest unit has (9 for a lamport, 18 for
    /// a wei).
    pub decimals: u8,
    /// The token's own contract address, for an ERC-20 pair named by S1
    /// ("name the pair"). `None` for a chain's native asset (SOL, ETH),
    /// which has no contract to cite. Carried so the sheet can state the
    /// pair with the address it named it from, never a symbol alone --
    /// `symbol()` is untrusted launcher-chosen text (AGENTS.md §3 rule 3),
    /// and the address is what lets a reader check it rather than take the
    /// name on faith.
    pub address: Option<ChainAddress>,
}

impl QuoteAsset {
    /// Native SOL, lamports, 9 decimals.
    #[must_use]
    pub fn sol() -> Self {
        Self {
            symbol: "SOL".to_owned(),
            decimals: 9,
            address: None,
        }
    }

    /// Native ETH, wei, 18 decimals.
    #[must_use]
    pub fn eth() -> Self {
        Self {
            symbol: "ETH".to_owned(),
            decimals: 18,
            address: None,
        }
    }

    /// An ERC-20 pair token, named by its own `symbol()`/`decimals()`
    /// (S1, "name the pair"). `symbol` is the caller's job to sanitise
    /// first -- this constructor does not re-check it, so that the one
    /// sanitiser (`realorrug_onchain::robinhood::sanitised_symbol`) stays
    /// the single place that decision is made.
    #[must_use]
    pub fn token(address: ChainAddress, symbol: String, decimals: u8) -> Self {
        Self {
            symbol,
            decimals,
            address: Some(address),
        }
    }
}

/// What the curve says right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurveFacts {
    /// Whether the curve has graduated to the AMM.
    ///
    /// A complete curve holds no reserves and prices nothing. One of three
    /// tokens sampled on 2026-09-01 was already complete, so this is the
    /// ordinary case rather than the exotic one.
    pub complete: bool,
    /// Quote-asset reserves the curve actually holds, in the quote asset's
    /// smallest unit.
    ///
    /// Named for what it is rather than which chain it was read on -- lamports
    /// on Solana today, and the same figure in whatever unit a future chain's
    /// quote asset uses. `CurveFacts` itself is not chain-specific; only a
    /// reader's *source* for this number is.
    ///
    /// `u128`, not `u64`: a `u64` tops out at about 18.4 ETH in wei, which
    /// every token that raised real money on Robinhood Chain exceeds. Lamports
    /// still fit easily; nothing downstream does arithmetic that can overflow
    /// a `u128`, only division and formatting.
    pub quote_reserves: u128,
    /// How much of the quote asset can be spent before price moves by
    /// [`CAPACITY_IMPACT_BPS`], in the quote asset's smallest unit.
    ///
    /// `None` means **cannot size into this at all**, never "no limit found"
    /// (rule 9). A complete curve is the common reason.
    pub quote_capacity: Option<u128>,
    /// What `quote_reserves` and `quote_capacity` are denominated in.
    ///
    /// `None` means the launch record named a quote asset this reader could
    /// not identify -- not native SOL, not native ETH, and not a token this
    /// reader has a symbol for. A figure with no identified unit must never be
    /// rendered; the sheet puts it on `unknown` instead (rule 8).
    pub quote_asset: Option<QuoteAsset>,
    /// Who launched the token, read from the curve account itself.
    ///
    /// **The only way to get a creator for a graduated coin.** The launch
    /// block is the other route, and `oldest_launch` refuses it when the
    /// signature walk truncates -- which it does for any coin with real
    /// history, so exactly the coins people ask about. This account carries it
    /// regardless of age, and the dossier already reads it.
    pub creator: ChainAddress,
    /// The venue fee, read from the on-chain schedule rather than assumed.
    ///
    /// Research 0023 measured it at 125 bps a side and found the program's own
    /// published IDL declares sixteen accounts for a buy where mainnet passes
    /// eighteen. A first-party reference is not the deployed program
    /// (LEARNINGS 25), so this is read from the chain every time.
    pub fees: Option<Fees>,
}

/// A launch read from the chain's own launch event, for a chain whose launch
/// is not a Solana slot.
///
/// Its own type, not a variant of [`LaunchBlock`]: that type's creator is a
/// Solana key, its buy is lamports and its metadata comes from the launch
/// instruction, and none of the three exists in the same shape on an EVM
/// chain. Stretching it would put a guessed unit next to a real figure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainLaunch {
    /// The block the launch event is in.
    pub block: u64,
    /// Seconds from the launch block to the read point, both timestamps the
    /// chain's own. `None` when either block's time could not be read --
    /// the sheet then says the age is unknown rather than inventing one.
    pub age_seconds: Option<u64>,
    /// Wei the launcher spent buying their own token in the launch
    /// transaction. `Some(0)` means the transaction was read and held no such
    /// buy; `None` means it could not be read, which is not zero (rule 8).
    pub dev_buy_wei: Option<u128>,
    /// Tokens the launcher received from its own `CurveBuy` logs in the
    /// launch transaction -- the same filter `launcher_buy` uses for
    /// [`ChainLaunch::dev_buy_wei`] (same curve, launcher as trader or
    /// recipient), summing `tokensOut` instead of `quoteIn`.
    ///
    /// Free: `CurveBuy` already carries `tokensOut` (research 0052's assumed
    /// launch-block `eth_call` is not needed). `Some(0)` means the launch
    /// transaction was read and held no such buy; a reverted receipt also
    /// reads as `Some(0)`, the same as `dev_buy_wei`. `None` means the
    /// receipt could not be read, which is not zero (rule 8).
    pub dev_buy_tokens: Option<u128>,
    /// The token's total supply, summed from the launch receipt's ERC-20
    /// `Transfer` logs out of the zero address (a mint). Supply is a launch
    /// config value, not a constant, so it is read from the mint rather than
    /// assumed. `None` when the launch receipt carries no such mint, or
    /// could not be read -- absent, not zero (rule 8).
    pub supply: Option<u128>,
    /// The token's `name()`, or `None` when the call could not be read.
    ///
    /// **Untrusted**: it is whatever string the launcher put in their
    /// contract (AGENTS.md rule 3), and it reaches the reader fenced as such,
    /// never as a fact. `None` means the call failed or answered with
    /// something that is not one displayable string -- not that the token has
    /// no name (rule 8).
    ///
    /// It sits on the launch rather than beside it because that is where the
    /// Solana path keeps the same two strings (`LaunchBlock::metadata`), so
    /// the fact sheet has one place per chain to look. It is read from the
    /// token contract, though, not from the launch event: no Pons event or
    /// factory record carries a name at all, which is why the share card drew
    /// a blank where the name goes until 2026-09-17.
    pub name: Option<String>,
    /// The token's `symbol()`, on the same terms as [`ChainLaunch::name`].
    pub symbol: Option<String>,
    /// S7 "correlated selling" (research 0052 §3, M-D-0008): the largest
    /// cluster of linked wallets that sold within one 50-block window, read
    /// from `CurveSell` logs. `None` when it could not be read: the chain has
    /// no such read (Solana), or the read the signal depends on failed --
    /// never "no correlated selling" (rule 8).
    ///
    /// Pre-graduation only. After graduation the pool address is unknown
    /// (research 0044), so there is nothing to read and this stays `None`
    /// with no claim either way.
    pub correlated_selling: Option<crate::wallets::CorrelatedSelling>,
}

/// Who holds the token, summed from every `Transfer` it ever emitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holders {
    /// Accounts with a balance above zero, not counting the launch curve,
    /// the factory or the zero address -- the machinery, not holders.
    pub count: u32,
    /// The largest such account's share of what those accounts hold
    /// together, in basis points; `None` when nobody holds any.
    ///
    /// **Of the circulating supply, not the total**: the curve's own stock is
    /// excluded from both sides. The largest holder may still be a pool or a
    /// contract; nothing here can tell a whale from a liquidity pool, so the
    /// sheet never calls it a person.
    pub largest_share_bps: Option<u16>,
}

/// Whether a large owner's role could be proven, or must stay unresolved.
///
/// **`Unresolved` is the default and the safe one.** Excluding an owner from
/// "largest holder" on the strength of a balance's size or shape alone would
/// be exactly the guess AGENTS.md rule 9 forbids. The only accepted proof
/// today is recomputing the pump.fun bonding curve's program-derived address
/// from the mint itself and finding an owner's address matches it -- math a
/// reader can rerun, not an inference from what the balance looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerRole {
    /// This owner is the verified pump.fun bonding curve for this exact
    /// mint, proven by recomputing its program-derived address.
    BondingCurve,
    /// No program identity could be established for this owner. Never
    /// upgraded to a person, or to a role this reader did not establish.
    Unresolved,
}

/// One address's aggregated stake among the largest token accounts sampled.
///
/// **An address, never a person**, same rule as [`Holders`]: a token account
/// belongs to whatever its `owner` field names, and that may be a wallet, a
/// pool vault or a program, none of which this reader can tell apart from a
/// balance alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenOwner {
    /// The wallet or program that owns the aggregated balance.
    pub owner: Address,
    /// How many of the sampled largest accounts aggregate under this owner.
    pub accounts: u32,
    /// The combined balance, in the mint's smallest unit.
    pub amount: u128,
    /// This owner's share of [`TokenOwnership::supply`], in basis points.
    /// `None` when the supply was zero -- never a share against nothing.
    pub share_bps: Option<u16>,
    /// Whether this owner's role could be proven.
    pub role: OwnerRole,
}

/// The largest token accounts for a mint, aggregated by owner, with the
/// supply their shares are measured against and the mint's own authorities.
///
/// Design 0027 §3 row 6 names this read (`getTokenLargestAccounts` +
/// `getMultipleAccounts` + `getTokenSupply`) as the owner half of what row 7
/// also depends on; built here as slice 6a.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenOwnership {
    /// Owners, most-held first. **A sample, not every holder** --
    /// `getTokenLargestAccounts` caps at 20 accounts.
    pub owners: Vec<TokenOwner>,
    /// The denominator every `share_bps` above is a share of -- read fresh
    /// from `getTokenSupply`, never assumed from the largest-accounts total.
    pub supply: u128,
    /// The supply's decimal places.
    pub decimals: u8,
    /// Who may mint more of this token, when anyone may. `None` means
    /// minting is permanently disabled for this mint -- proof supply cannot
    /// be inflated further from here, not proof it never was.
    pub mint_authority: Option<Address>,
    /// Who may freeze an account of this mint, when anyone may. Proof an
    /// account *can* be frozen shut, not proof one has been.
    pub freeze_authority: Option<Address>,
}

/// One address the launch or the factory exempted from the snipe tax (or, for
/// [`Powers::pending_creator_fee_recipient`], is about to receive creator
/// fees), classified by research 0047 §3's list before any numeric floor
/// judges it.
pub type ExemptionSource = realorrug_robinhood::pons::powers::Source;

/// An address the reader found exempt from the snipe tax, and where it
/// stands: [`ExemptionSource::FirstParty`] (Pons's own infrastructure, per
/// research 0047 §3), [`ExemptionSource::Declared`] (named in the launcher's
/// own `launchToken` calldata, research 0048 §3), or
/// [`ExemptionSource::Undeclared`] -- on neither list, which is what S13
/// (research 0052 §3) weighs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exemption {
    /// The exempt address.
    pub address: ChainAddress,
    /// Why it is exempt, or that no reason could be found for it.
    pub source: ExemptionSource,
}

/// S13's "owner powers live": creator tax, the pending creator-fee timelock,
/// and every address the curve holds exempt from the snipe tax, classified.
/// Pons v2 / Robinhood only today (research 0052 §3; task packet M-D-0004).
#[derive(Clone, Debug, PartialEq)]
pub struct Powers {
    /// The creator's cut of every trade, in basis points (0-1,000 on Pons
    /// v2). Read once, from the factory's `getLaunchedToken`, at zero extra
    /// RPC cost -- the same call [`ChainLaunch`]'s sibling facts already pay
    /// for.
    pub creator_tax_bps: u16,
    /// The address a pending 3-day timelock would hand creator fees to, or
    /// `None` when no change is pending. `Some(None)` is impossible to
    /// express here; a failed read is instead named in
    /// [`Dossier::unavailable`] and this field left off the dossier
    /// entirely, so "nothing pending" and "could not read" are never
    /// conflated (AGENTS.md §3 rule 8).
    pub pending_creator_fee_recipient: Option<ChainAddress>,
    /// Every address exempted from the snipe tax on this launch, classified.
    /// Never a default when a candidate's current exemption could not be
    /// confirmed on-chain -- an unconfirmed candidate is left out of this
    /// list and out of the count entirely, not assumed exempt or assumed
    /// clear.
    pub exemptions: Vec<Exemption>,
}

/// Everything the analyst may assert about one token.
#[derive(Clone, Debug, PartialEq)]
pub struct Dossier {
    /// The token asked about.
    pub mint: ChainAddress,
    /// The chain's own read point -- a Solana slot today, and a chain-typed
    /// value for whichever chain reads this dossier next.
    ///
    /// **Every published figure carries this.** A number without the read
    /// point it was read at is unfalsifiable, and the account's entire claim
    /// is that its numbers can be checked on an explorer.
    pub read_at: Option<ReadAt>,
    /// What the launch block held.
    pub launch: Option<LaunchBlock>,
    /// What the curve says.
    pub curve: Option<CurveFacts>,
    /// How many successful transactions the creator's address has.
    ///
    /// **Transactions, not launches**, and named that way because the two are
    /// not the same number and the tempting one is the one this cannot measure.
    /// Counting launches from here would mean fetching and decoding every one of
    /// a prolific creator's transactions, which is exactly the unbounded work
    /// the budget exists to refuse.
    ///
    /// Always a [`Count::AtLeast`]: truncated when the walk hit its page bound,
    /// and still a lower bound when it did not, because a signature history is
    /// activity rather than launches. Phase 2 replaces this with the store's
    /// creator index -- 483,629 rows of `(creator, slot, mint)`, a hash lookup
    /// -- which is both cheaper and an actual launch count.
    pub creator_transactions: Option<Count>,
    /// The launch, read from the chain's own launch event -- for a chain
    /// whose launch is not a Solana slot ([`ChainLaunch`]).
    pub chain_launch: Option<ChainLaunch>,
    /// Who holds the token, where it was read.
    pub holders: Option<Holders>,
    /// Who funded the first buyers, where it was investigated
    /// (`crate::wallets`, Robinhood only today). `None` is "not
    /// investigated"; a reader that tried and failed names "funding" in
    /// `unavailable`.
    pub funding: Option<crate::wallets::Funding>,
    /// A dated market snapshot (`crate::market`, Robinhood only today).
    /// `None` is "not read"; a reader that tried and failed names "market"
    /// in `unavailable`. Its own price/cap always carry the wall-clock
    /// moment they were read at (AGENTS.md §3 rule 5); its liquidity dollars
    /// are never wired into `CurveFacts::quote_capacity` -- see
    /// `crate::market`'s module doc.
    pub market: Option<crate::market::MarketSnapshot>,
    /// The largest token accounts, aggregated by owner, and the mint's own
    /// authorities (design 0027 row 6/7, Solana only today). `None` is "not
    /// investigated"; a reader that tried and failed names "token ownership"
    /// in `unavailable`.
    pub token_ownership: Option<TokenOwnership>,
    /// The creator's observed on-chain cash flow (`crate::wallets`, design
    /// 0027 slice 5, Robinhood only today). `None` is "not investigated"; a
    /// reader that tried and failed names "creator cash flow" in
    /// `unavailable`. Solana has no reader for this yet -- see
    /// `SolanaReader`'s own doc on `creator_cash_flow` below.
    pub creator_cash_flow: Option<crate::wallets::CreatorCashFlow>,
    /// The creator's live powers over this token -- tax, the pending
    /// fee-recipient timelock, and classified snipe-tax exemptions
    /// (`crate::dossier::Powers`, Pons v2 / Robinhood only today, research
    /// 0052 §3). `None` is "not investigated"; a reader that tried and
    /// failed names "powers" (or the specific sub-fact, e.g. "pending
    /// creator fee recipient") in `unavailable`.
    pub powers: Option<Powers>,
    /// Facts that could not be read, and why.
    pub unavailable: Vec<Unavailable>,
    /// RPC calls this dossier cost.
    pub calls: u32,
    /// How long it took, in milliseconds.
    pub elapsed_ms: u128,
}

impl Dossier {
    fn miss(&mut self, fact: &'static str, why: impl std::fmt::Display) {
        self.unavailable.push(Unavailable {
            fact,
            why: why.to_string(),
        });
    }
}

/// Builds a dossier for one mint.
///
/// `memory` is the read memory (design 0021; packet 0039 §1) placed in front
/// of the launch-block read only — the one read of the three below that is
/// [`crate::memory::Kind::Forever`] and cannot make the sheet's single read
/// point lie (see the comment on the launch-block step). The curve read and
/// the creator-activity read are **never** served from memory, even when a
/// row for them happens to exist: deferred, not rejected, until the fact
/// sheet (design 0020) can carry a read point per fact instead of one for
/// the whole sheet (design 0021 §1, packet 0039).
///
/// `None` is not the deny-by-default case AGENTS.md rule 7 governs: a missing
/// memory makes this function slower and more expensive, never wrong, because
/// the chain is still read and is still the authority. A caller with no
/// memory gets exactly today's answers, at today's cost — see
/// [`crate::memory`]'s own module doc and design 0021 §3 for the boundary
/// this rests on: a row in the memory is a receipt of a past chain read, not
/// a substitute for one.
///
/// Never returns `Err` for a fact it could not read — a partial dossier is the
/// product, and the missing halves are named in
/// [`Dossier::unavailable`]. It returns `Err` only when the mint itself cannot
/// be resolved, because a dossier about a token that does not exist is not a
/// partial answer, it is a wrong one.
///
/// # Errors
///
/// [`RpcError`] when the token's own signature history cannot be read at all.
pub fn build(
    client: &RpcClient,
    budget: &mut Budget,
    mint: &Address,
    memory: Option<&Memory>,
) -> Result<Dossier, RpcError> {
    let mint_key = mint.to_string();
    let mut dossier = Dossier {
        mint: ChainAddress::Solana(*mint),
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
    dossier.miss(
        "creator cash flow",
        "Solana not built: design 0027 slice 5 has no `wallets::creator_cash_flow` equivalent \
         for Solana yet",
    );

    // 1. The launch block, from the oldest signature the mint has, or from
    // the read memory ahead of it (packet 0039 §1). `Kind::Forever`: a past
    // block cannot become wrong later (AGENTS.md rule 1), so a hit needs no
    // freshness check and needs no signature-paging call at all -- the
    // memory's whole point, since paging is the read that costs the most.
    // The curve and creator-activity reads below are deliberately NOT given
    // the same treatment: this is the only read `Dossier::read_at` can name
    // as this sheet's slot without contradicting a fresher number read
    // elsewhere in the same build (see `dossier.read_at.is_none()` in step 2).
    let launch_result = if let Some(block) = memory.and_then(|mem| cached_launch(mem, &mint_key)) {
        Ok(block)
    } else {
        let (signatures, truncated) = client.signatures_back_to_oldest(budget, mint)?;
        oldest_launch(client, budget, &signatures, truncated, &mint_key).and_then(|block| {
            if let Some(mem) = memory {
                // `record` refuses to overwrite a `(what, subject, block)`
                // key with a different value rather than replacing it
                // (packet 0039 §1, `Memory::record`'s own doc) -- a launch
                // record that came back different from a fresh read is a
                // bug (AGENTS.md rule 1) and is surfaced here with its
                // error text intact, never silently resolved either way.
                store_launch(mem, &mint_key, &block).map_err(|e| e.to_string())?;
            }
            Ok(block)
        })
    };
    match launch_result {
        Ok(block) => {
            dossier.read_at = Some(ReadAt::Solana(block.slot));
            dossier.launch = Some(block);
        }
        Err(why) => dossier.miss("launch block", why),
    }

    // 2. The curve, and the fee schedule it is priced under.
    match curve_facts(client, budget, mint) {
        Ok((facts, slot)) => {
            // The curve read is the *only* slot a graduated coin has, because
            // its launch block is past the signature-page budget and
            // `oldest_launch` rightly refuses to guess one. Without this the
            // coins people actually ask about published every figure with no
            // slot beside it -- unfalsifiable, which is the one thing this
            // account may not be. Set only when the launch block did not
            // already supply one, so the earlier read still wins.
            if dossier.read_at.is_none() {
                dossier.read_at = slot.map(ReadAt::Solana);
            }
            dossier.curve = Some(facts);
        }
        Err(why) => dossier.miss("curve", why),
    }

    // 3. The creator's activity, bounded.
    if let Some(creator) = dossier.launch.as_ref().map(|l| l.creator) {
        match client.signatures_back_to_oldest(budget, &creator) {
            Ok((sigs, _cut)) => {
                let n = u32::try_from(sigs.iter().filter(|s| s.err.is_none()).count())
                    .unwrap_or(u32::MAX);
                // `AtLeast` whether or not paging was cut short, and the reason
                // is not the budget: a signature history is transactions, and
                // the question anyone actually asks is about launches. Reporting
                // `Exactly` here would be exact about the wrong quantity, which
                // is LEARNINGS 22's shape -- an exemption whose reasoning named
                // the wrong number.
                dossier.creator_transactions = Some(Count::AtLeast(n));
            }
            Err(why) => dossier.miss("creator history", why),
        }
    }

    // 4. Largest token accounts, aggregated by owner, and the mint's own
    // authorities (design 0027 row 6/7). Three calls: `getTokenLargestAccounts`,
    // `getTokenSupply`, and one batched `getMultipleAccounts` covering every
    // sampled account plus the mint itself -- each already drawn from `budget`
    // by `RpcClient::call`, so nothing here needs a separate ceiling. A
    // transport or shape failure on any of the three fails the whole read
    // rather than reporting a partial owner list as complete.
    match token_ownership(client, budget, mint) {
        Ok(facts) => dossier.token_ownership = Some(facts),
        Err(why) => dossier.miss("token ownership", why),
    }

    // 5. Who funded the mint's own early buyers (design 0027 row 6, slice
    // 6b). Independent of step 4: candidates come from the mint's own
    // signature history, not from the current holder sample. A transport
    // failure here names "funding" in `unavailable` rather than a silent
    // empty result, per `investigate_solana`'s own doc.
    match investigate_solana(client, budget, mint) {
        Ok(funding) => dossier.funding = Some(funding),
        Err(why) => dossier.miss("funding", why),
    }

    dossier.calls = budget.calls_made();
    dossier.elapsed_ms = budget.elapsed().as_millis();
    Ok(dossier)
}

/// Builds [`TokenOwnership`]: the largest token accounts for `mint`,
/// aggregated by their decoded `owner` field, plus the mint's own
/// authorities.
///
/// # Errors
///
/// [`RpcError`] on transport, node or shape failures from any of the three
/// reads, or when the budget is spent. Never returns a partial owner list
/// silently -- a caller that could not resolve the mint's own account, for
/// instance, has not established the authorities and must not print `None`
/// for them as if minting were proven disabled (rule 9).
fn token_ownership(
    client: &RpcClient,
    budget: &mut Budget,
    mint: &Address,
) -> Result<TokenOwnership, RpcError> {
    let largest = client.token_largest_accounts(budget, mint)?;
    let supply = client.token_supply(budget, mint)?;

    // One batched read for every sampled account's owner plus the mint's own
    // account, so the whole aggregation shares one slot and costs one call
    // rather than one per address (see `MultiAccountRead`'s own doc on why a
    // shared slot matters).
    let mut addresses: Vec<Address> = largest.iter().map(|a| a.address).collect();
    addresses.push(*mint);
    let multi = client.accounts(budget, &addresses)?;

    let mint_account = multi.accounts.last().ok_or_else(|| {
        RpcError::Malformed("getMultipleAccounts returned an empty list".to_owned())
    })?;
    let Some(mint_account) = mint_account else {
        return Err(RpcError::Malformed(
            "mint has no account, so its authorities cannot be read".to_owned(),
        ));
    };
    let (mint_authority, freeze_authority) = mint_authorities(&mint_account.data)?;

    let curve = pda::bonding_curve(mint);
    let mut owners: Vec<TokenOwner> = Vec::new();
    for account in &multi.accounts[..largest.len()] {
        // A largest-accounts entry the batched read came back empty for is a
        // race between the two calls, not a zero balance (rule 9) -- skipped
        // rather than counted as an owner with nothing in it. Likewise an
        // account whose owning program is neither token program, or whose
        // bytes this reader cannot parse as a token account (an unmodelled
        // Token-2022 extension, say): the balance is real but this reader
        // cannot say who holds it, so it is left out rather than guessed.
        let Some(account) = account else { continue };
        let Some(program) = account
            .owner
            .as_deref()
            .and_then(|o| o.parse::<Address>().ok())
            .and_then(|o| TokenProgram::of(&o))
        else {
            continue;
        };
        let Ok(parsed) = TokenAccount::parse(&account.data, &program.id()) else {
            continue;
        };

        if let Some(existing) = owners.iter_mut().find(|o| o.owner == parsed.owner) {
            existing.accounts += 1;
            existing.amount += u128::from(parsed.amount);
        } else {
            let role = if curve == Some(parsed.owner) {
                OwnerRole::BondingCurve
            } else {
                OwnerRole::Unresolved
            };
            owners.push(TokenOwner {
                owner: parsed.owner,
                accounts: 1,
                amount: u128::from(parsed.amount),
                share_bps: None,
                role,
            });
        }
    }

    let supply_amount = u128::from(supply.amount);
    for owner in &mut owners {
        owner.share_bps = share_bps(owner.amount, supply_amount);
    }
    owners.sort_by_key(|o| std::cmp::Reverse(o.amount));

    Ok(TokenOwnership {
        owners,
        supply: supply_amount,
        decimals: supply.decimals,
        mint_authority,
        freeze_authority,
    })
}

/// `amount`'s basis points of `supply`, or `None` when the supply is zero --
/// never a share against nothing (rule 9).
fn share_bps(amount: u128, supply: u128) -> Option<u16> {
    if supply == 0 {
        return None;
    }
    u16::try_from(amount.saturating_mul(10_000) / supply).ok()
}

/// Reads a mint's two authorities directly from its raw account bytes.
///
/// The SPL Token and Token-2022 layouts share these two `COption<Pubkey>`
/// fields at the same offsets (mint authority's tag at byte 0, its address at
/// byte 4; freeze authority's tag at byte 46, its address at byte 50), so this
/// needs no program identity and no extension walk -- unlike
/// `realorrug_pumpfun::token::MintAccount::parse`, which refuses an
/// unmodelled extension entirely. Refusing an authority fact because of an
/// unrelated extension this reader has never seen would be the wrong trade:
/// the extension might change what a balance is worth, but it cannot move
/// where these two fields sit.
///
/// `MintAccount::parse` already reads `freeze_authority`, but discards
/// `mint_authority`'s address -- it only needs to know minting is possible,
/// not by whom. Repeating the two reads here is smaller than widening that
/// type for the one caller that needs the address.
///
/// # Errors
///
/// [`RpcError::Malformed`] when the account is shorter than a mint's base
/// layout, or an option tag is neither zero nor one.
fn mint_authorities(data: &[u8]) -> Result<(Option<Address>, Option<Address>), RpcError> {
    if data.len() < 82 {
        return Err(RpcError::Malformed(format!(
            "{} bytes, and a mint needs at least 82",
            data.len()
        )));
    }
    let option = |tag_at: usize, addr_at: usize| -> Result<Option<Address>, RpcError> {
        let tag = u32::from_le_bytes(data[tag_at..tag_at + 4].try_into().expect("checked above"));
        match tag {
            0 => Ok(None),
            1 => Ok(Some(Address::new(
                data[addr_at..addr_at + 32]
                    .try_into()
                    .expect("checked above"),
            ))),
            found => Err(RpcError::Malformed(format!(
                "mint authority option tag is {found}, which is neither none nor some"
            ))),
        }
    };
    let mint_authority = option(0, 4)?;
    let freeze_authority = option(46, 50)?;
    Ok((mint_authority, freeze_authority))
}

/// The seam ADR 0028 point 2 names: one chain's reads in, one [`Dossier`] out.
///
/// A third chain costs one implementation of this trait, plus an address
/// parser (`realorrug_types::ChainAddress`) and one `Venue` match arm -- and
/// nothing else, because everything downstream of a `Dossier` (`FactSheet`,
/// `Verdict`, the voice) already reads the shape this trait returns, not any
/// particular chain's client.
///
/// `Client` and `Token` are associated types rather than the trait taking
/// `&RpcClient` and `&Address` directly (Solana's own types) or `&ChainAddress`
/// (the chain-tagged enum): a fixed `RpcClient`/`Address` signature cannot be
/// implemented by a second chain at all, since it has neither Solana's client
/// nor a 32-byte address to put a 20-byte address into. And `&ChainAddress`
/// would compile for every reader but hand each one addresses that are not
/// its own, pushing "is this my chain?" into a runtime branch every
/// implementation has to write and get right. An associated `Token` makes that
/// a compile error instead of a check -- AGENTS.md section 4's "enforce a
/// property at the cheapest level that holds it" -- so [`SolanaReader`] below
/// can only ever be handed a Solana [`Address`], and a Robinhood reader can
/// only ever be handed a Robinhood one.
pub trait ChainReader {
    /// The client this chain's reads go through.
    type Client;
    /// This chain's own address type.
    type Token;
    /// The error a failed read produces.
    type Error;

    /// Reads one mint and returns the dossier `build` already produces.
    ///
    /// # Errors
    ///
    /// When the mint itself cannot be resolved at all -- never for an
    /// individual missing fact, which lands in [`Dossier::unavailable`]
    /// instead.
    fn read(
        &self,
        client: &Self::Client,
        budget: &mut Budget,
        token: &Self::Token,
    ) -> Result<Dossier, Self::Error>;
}

/// The `ChainReader` Solana has always had, wrapping today's [`build`] with no
/// logic change.
///
/// Exists so Solana is on the seam from day one rather than being the one
/// chain that predates it -- a second chain's reader is written against this
/// trait, not against a special case for "the chain that came first."
#[derive(Clone, Copy, Default)]
pub struct SolanaReader<'a> {
    /// Optional durable memory for immutable launch records only.
    pub memory: Option<&'a Memory>,
}

impl ChainReader for SolanaReader<'_> {
    type Client = RpcClient;
    type Token = Address;
    type Error = RpcError;

    fn read(
        &self,
        client: &RpcClient,
        budget: &mut Budget,
        mint: &Address,
    ) -> Result<Dossier, RpcError> {
        build(client, budget, mint, self.memory)
    }
}

/// The `what` [`Memory::record`]/[`Memory::latest`] use for the launch record
/// (packet 0039 §1). `memory.rs` uses the same text for
/// [`Memory::record_launch`]/[`Memory::launches_in_window`], keyed on the
/// *creator's* address -- a different fact under the same name. The two never
/// collide: the key also includes `subject`, and this one's subject is always
/// the *mint*, never a creator address.
const LAUNCH_RECORD: &str = "launch";

/// [`LaunchBlock`] as [`Memory`] stores it: text in, text out (see
/// [`crate::memory`]'s own doc on why a fact's value is a string, not a typed
/// column), and every field that isn't the block number or the mint itself,
/// since those two are already the key ([`Memory::record`]'s `block` and
/// `subject`).
#[derive(Serialize, Deserialize)]
struct StoredLaunch {
    creator: String,
    recipients: StoredCount,
    transactions: StoredCount,
    dev_buy_lamports: Option<u64>,
    name: String,
    symbol: String,
    uri: String,
}

/// [`Count`] has no `Serialize`/`Deserialize` of its own -- it is
/// `realorrug-onchain`'s own type, not `realorrug-types`', and giving it a
/// wire format for one caller's cache row is not this packet's call to make.
/// This mirrors it for [`StoredLaunch`] alone.
#[derive(Serialize, Deserialize)]
struct StoredCount {
    n: u32,
    truncated: bool,
}

impl From<Count> for StoredCount {
    fn from(count: Count) -> Self {
        Self {
            n: count.lower_bound(),
            truncated: count.is_truncated(),
        }
    }
}

impl From<StoredCount> for Count {
    fn from(stored: StoredCount) -> Self {
        if stored.truncated {
            Count::AtLeast(stored.n)
        } else {
            Count::Exactly(stored.n)
        }
    }
}

/// Looks up a cached launch record for `mint` (packet 0039 §1).
///
/// `Memory::latest`, not an exact-key `get`: the launch block itself is not
/// known until it has been read once, so there is no block number to key an
/// exact lookup on before that first read has happened. `(what, subject)`
/// alone is enough here because a mint has exactly one launch, ever -- the
/// newest (and only) row `latest` can find *is* the answer.
///
/// A row that fails to decode (a schema change, a hand-edited file) is
/// treated the same as no row at all, never surfaced as an error: design
/// 0021 §3's boundary is that the memory can only make this function
/// *slower*, by missing a real hit, never wrong -- the chain read that
/// follows a `None` here is always the true fallback.
fn cached_launch(memory: &Memory, mint: &str) -> Option<LaunchBlock> {
    let fact = memory.latest(LAUNCH_RECORD, mint).ok().flatten()?;
    let stored: StoredLaunch = serde_json::from_str(&fact.value).ok()?;
    Some(LaunchBlock {
        slot: Slot(fact.block),
        creator: stored.creator.parse().ok()?,
        recipients: stored.recipients.into(),
        transactions: stored.transactions.into(),
        dev_buy_lamports: stored.dev_buy_lamports,
        metadata: Metadata {
            name: stored.name,
            symbol: stored.symbol,
            uri: stored.uri,
        },
    })
}

/// Records a freshly-read launch block so the next build skips the read
/// (packet 0039 §1).
///
/// `Kind::Forever`: a past block cannot become wrong later (AGENTS.md rule
/// 1). [`Memory::record`] itself refuses to overwrite a `(what, subject,
/// block)` key with a different value rather than silently replacing it --
/// the caller is expected to let that refusal reach whoever asked, intact,
/// because a launch record that came back different from a fresh read is
/// exactly the bug that refusal exists to surface, not a race to paper over.
///
/// # Errors
///
/// Whatever [`Memory::record`] returns: [`crate::memory::Error::Conflict`]
/// on a same-key, different-value re-read, or [`crate::memory::Error::Sqlite`]
/// if the write itself fails.
fn store_launch(
    memory: &Memory,
    mint: &str,
    block: &LaunchBlock,
) -> Result<crate::memory::Recorded, crate::memory::Error> {
    let stored = StoredLaunch {
        creator: block.creator.to_string(),
        recipients: block.recipients.into(),
        transactions: block.transactions.into(),
        dev_buy_lamports: block.dev_buy_lamports,
        name: block.metadata.name.clone(),
        symbol: block.metadata.symbol.clone(),
        uri: block.metadata.uri.clone(),
    };
    // `expect` only on a type this module itself defined and fully controls
    // the shape of -- a `StoredLaunch` cannot fail to serialise to JSON, so
    // this is not a chain-dependent fallibility being swallowed.
    let value = serde_json::to_string(&stored).expect("StoredLaunch always serialises");
    memory.record(
        LAUNCH_RECORD,
        mint,
        block.slot.get(),
        Kind::Forever,
        &value,
        SystemTime::now(),
    )
}

/// Finds the launch transaction and rebuilds its block.
fn oldest_launch(
    client: &RpcClient,
    budget: &mut Budget,
    signatures: &[crate::rpc::SignatureInfo],
    truncated: bool,
    mint: &str,
) -> Result<LaunchBlock, String> {
    if truncated {
        // The oldest signature seen is not the oldest signature there is, so
        // reading it as the launch would invent a launch block out of an
        // ordinary trade. Refused rather than guessed -- AGENTS.md section 2:
        // when something is unknown, record it as unknown.
        return Err("this token has more history than the page budget allows, \
                    so its launch could not be reached"
            .to_owned());
    }
    let Some(oldest) = signatures.iter().rev().find(|s| s.err.is_none()) else {
        return Err("no successful transactions for this mint".to_owned());
    };

    let launch_slot = oldest.slot;
    let launch_tx = client
        .transaction(budget, &oldest.signature)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "the launch transaction could not be fetched".to_owned())?;

    // Every other transaction in the same slot that touched this mint. These
    // are the same-slot coordinated buys the recipient count is about.
    let mut block: Vec<Transaction> = vec![launch_tx.clone()];
    let mut stopped = false;
    for sig in signatures
        .iter()
        .filter(|s| also_in_block(s, launch_slot, &oldest.signature))
    {
        match client.transaction(budget, &sig.signature) {
            Ok(Some(tx)) => block.push(tx),
            Ok(None) => {}
            // Out of budget mid-block. The block is partial, so both counts
            // become "at least" rather than being published as complete.
            Err(RpcError::Stopped(_)) => {
                stopped = true;
                break;
            }
            Err(e) => return Err(e.to_string()),
        }
    }

    crate::launch::assemble(&launch_tx, &block, mint, stopped)
        .map_err(|e: NotALaunch| e.to_string())
}

/// Whether a signature belongs to the launch block, other than the launch
/// itself.
///
/// Extracted from the loop so each of its three conditions can be tested. All
/// three were surviving mutants: `just mutants` flipped `&&` to `||`, `==` to
/// `!=` and `!=` to `==` here and every test still passed, which meant the
/// filter was doing nothing any assertion could see.
///
/// Each condition is load-bearing in a different way. The **slot** is what makes
/// this the launch *block* rather than the token's whole history. The **error**
/// check keeps failed transactions out, which 0006 records as worth a third of
/// a label. And excluding the **launch signature** stops the launch transaction
/// being fetched and counted twice.
fn also_in_block(sig: &crate::rpc::SignatureInfo, launch_slot: u64, launch_sig: &str) -> bool {
    sig.slot == launch_slot && sig.err.is_none() && sig.signature != launch_sig
}

/// Reads the bonding curve and the fee schedule.
fn curve_facts(
    client: &RpcClient,
    budget: &mut Budget,
    mint: &Address,
) -> Result<(CurveFacts, Option<Slot>), String> {
    let address = pda::bonding_curve(mint).ok_or_else(|| "no bonding-curve PDA".to_owned())?;
    let read = client
        .account(budget, &address)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no bonding-curve account: this is not a pump.fun token".to_owned())?;
    let curve = BondingCurve::parse(&read.data).map_err(|e| format!("{e:?}"))?;

    // A complete curve has no reserves. Asking it for capacity produces either a
    // division by zero or, worse, a plausible number out of stale fields -- so
    // the flag is checked before the arithmetic rather than after.
    let capacity = if curve.is_tradeable() {
        curve.buy_within_impact(CAPACITY_IMPACT_BPS, CAPACITY_CEILING_LAMPORTS)
    } else {
        None
    };

    Ok((
        CurveFacts {
            complete: curve.complete,
            quote_reserves: u128::from(curve.real_sol_reserves),
            quote_capacity: capacity.map(u128::from),
            quote_asset: Some(QuoteAsset::sol()),
            creator: ChainAddress::Solana(curve.creator),
            fees: fee_schedule(client, budget, &curve),
        },
        read.slot,
    ))
}

/// Reads the fee schedule off the chain.
///
/// `None` rather than a constant when it cannot be read. 0023's whole finding is
/// that the fee is a *schedule* and that the program's published interface is
/// incomplete, so substituting a remembered 125 bps would be asserting a number
/// the chain was not asked for.
fn fee_schedule(client: &RpcClient, budget: &mut Budget, curve: &BondingCurve) -> Option<Fees> {
    let address = pda::fee_config()?;
    let data = client.account(budget, &address).ok()??.data;
    let config = realorrug_pumpfun::FeeConfig::parse(&data).ok()?;
    fees_for(&config, curve)
}

/// Which tier of the schedule this curve pays.
///
/// The pure half of [`fee_schedule`], split out so it can be tested — the outer
/// function is three network calls deep and mutating it to return `None` or a
/// default survived, because nothing without an endpoint could observe it.
///
/// The tier depends on where the token sits on the curve, which is the whole of
/// 0023's finding: **the fee is a schedule, not a rate.** Virtual SOL reserves
/// are what `realorrug-pumpfun`'s own mainnet test looks the tier up by — 30 SOL,
/// "a curve at launch reserves" — so this asks the schedule the same question
/// that test asks it.
///
/// `None` when no tier covers the curve, and never a remembered 125 bps: 0023
/// also found the program's published interface incomplete, so substituting a
/// constant here would be asserting a number the chain was not asked for.
fn fees_for(config: &realorrug_pumpfun::FeeConfig, curve: &BondingCurve) -> Option<Fees> {
    config.fees_at_market_cap(u128::from(curve.virtual_sol_reserves))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mint's base layout, exactly 82 bytes: the mint authority option at
    /// 0 (address at 4) and the freeze authority option at 46 (address at 50).
    fn mint_bytes(mint: Option<u8>, freeze: Option<u8>) -> Vec<u8> {
        let mut data = vec![0u8; 82];
        if let Some(fill) = mint {
            data[0] = 1;
            data[4..36].fill(fill);
        }
        if let Some(fill) = freeze {
            data[46] = 1;
            data[50..82].fill(fill);
        }
        data
    }

    #[test]
    fn a_mint_of_exactly_the_base_size_yields_both_authorities() {
        let (mint, freeze) = mint_authorities(&mint_bytes(Some(7), Some(9))).expect("82 bytes");
        assert_eq!(mint, Some(Address::new([7u8; 32])));
        assert_eq!(freeze, Some(Address::new([9u8; 32])));
        let (mint, freeze) = mint_authorities(&mint_bytes(None, None)).expect("82 bytes");
        assert_eq!((mint, freeze), (None, None));
    }

    #[test]
    fn a_short_mint_or_a_bad_option_tag_is_malformed() {
        let short = &mint_bytes(Some(7), Some(9))[..81];
        assert!(matches!(
            mint_authorities(short),
            Err(RpcError::Malformed(_))
        ));
        let mut bad = mint_bytes(None, None);
        bad[46] = 2;
        assert!(matches!(
            mint_authorities(&bad),
            Err(RpcError::Malformed(_))
        ));
    }

    #[test]
    fn a_dossier_names_what_it_could_not_read() {
        let mut dossier = Dossier {
            mint: ChainAddress::Solana(Address::new([1u8; 32])),
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
        dossier.miss("curve", "no bonding-curve account");
        assert_eq!(dossier.unavailable.len(), 1);
        assert_eq!(dossier.unavailable[0].fact, "curve");
        assert_eq!(dossier.unavailable[0].why, "no bonding-curve account");
        // And the fact itself stays absent rather than becoming a default.
        assert!(dossier.curve.is_none());
    }

    fn sig(signature: &str, slot: u64, failed: bool) -> crate::rpc::SignatureInfo {
        crate::rpc::SignatureInfo {
            signature: signature.to_owned(),
            slot,
            err: failed.then(|| serde_json::json!("boom")),
        }
    }

    #[test]
    fn the_launch_block_filter_needs_all_three_of_its_conditions() {
        // Every one of these was a surviving mutant. Asserted separately so a
        // failure names which condition stopped working rather than only that
        // the filter did.
        assert!(also_in_block(&sig("other", 100, false), 100, "launch"));
        // A different slot is a different block -- this is what keeps the read
        // to the launch block rather than the token's whole history.
        assert!(!also_in_block(&sig("other", 101, false), 100, "launch"));
        // A failed transaction is not an event (0006).
        assert!(!also_in_block(&sig("other", 100, true), 100, "launch"));
        // The launch itself is already in the block; including it again would
        // fetch and count it twice.
        assert!(!also_in_block(&sig("launch", 100, false), 100, "launch"));
    }

    fn curve_at(virtual_sol: u64) -> BondingCurve {
        BondingCurve {
            virtual_token_reserves: 889_566_950_293_959,
            virtual_sol_reserves: virtual_sol,
            real_token_reserves: 609_666_950_293_959,
            real_sol_reserves: 6_186_150_833,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
            creator: Address::new([1u8; 32]),
        }
    }

    #[test]
    fn the_fee_comes_from_the_tier_the_curve_is_in() {
        // Two tiers with different fees, so the lookup has something to get
        // wrong. Returning `None` or a default here survived mutation because
        // the only caller is three network calls deep.
        let config = realorrug_pumpfun::FeeConfig {
            flat: Fees {
                lp_bps: 0,
                protocol_bps: 0,
                creator_bps: 0,
            },
            tiers: vec![
                realorrug_pumpfun::fees::Tier {
                    threshold_lamports: 0,
                    fees: Fees {
                        lp_bps: 0,
                        protocol_bps: 95,
                        creator_bps: 30,
                    },
                },
                realorrug_pumpfun::fees::Tier {
                    threshold_lamports: 100_000_000_000,
                    fees: Fees {
                        lp_bps: 0,
                        protocol_bps: 10,
                        creator_bps: 5,
                    },
                },
            ],
        };

        // A curve at launch reserves pays the low tier: 125 bps a side, which
        // is exactly what 0023 measured off mainnet.
        let low = fees_for(&config, &curve_at(30_130_000_000)).expect("a tier");
        assert_eq!(low.total_bps(), 125);
        assert_eq!(low.round_trip_bps(), 250);

        // A curve further along pays the other one, so the schedule is being
        // read rather than the first row returned.
        let high = fees_for(&config, &curve_at(200_000_000_000)).expect("a tier");
        assert_eq!(high.total_bps(), 15);
        assert_ne!(low, high);
    }

    /// A fee-config account, built to the layout `FeeConfig::parse` reads.
    ///
    /// Synthesised rather than captured, because what is under test here is
    /// that [`fee_schedule`] reads the chain at all — `realorrug-pumpfun` already
    /// asserts the parse against real mainnet bytes, and duplicating that
    /// capture would be two places to update when the layout moves.
    fn fee_config_account(threshold: u128, protocol_bps: u64, creator_bps: u64) -> Vec<u8> {
        let mut data = realorrug_pumpfun::fees::FEE_CONFIG_DISCRIMINATOR.to_vec();
        data.push(0); // bump
        data.extend_from_slice(&[0u8; 32]); // admin
        // The flat fees, three u64.
        data.extend_from_slice(&0u64.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes()); // one tier
        data.extend_from_slice(&threshold.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes()); // lp
        data.extend_from_slice(&protocol_bps.to_le_bytes());
        data.extend_from_slice(&creator_bps.to_le_bytes());
        data
    }

    /// A transport that answers every call with the same body.
    struct Always(String);

    impl crate::rpc::Transport for Always {
        fn post(&self, _: &str, _: String) -> Result<String, String> {
            Ok(self.0.clone())
        }
    }

    fn account_response(data: &[u8]) -> String {
        // The node returns account data base64-encoded.
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut encoded = String::new();
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    encoded.push(ALPHABET[((n >> (18 - i * 6)) & 0x3F) as usize] as char);
                } else {
                    encoded.push('=');
                }
            }
        }
        format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":{{"value":{{"data":["{encoded}","base64"]}}}}}}"#
        )
    }

    #[test]
    fn the_fee_schedule_is_read_from_the_chain_rather_than_assumed() {
        // Mutating `fee_schedule` to `None` or a default survived until the
        // transport became injectable. Both consequences are bad in opposite
        // directions: `None` silently drops the fee from every reply, and a
        // default asserts a fee nobody read.
        let response = account_response(&fee_config_account(0, 95, 30));
        let client = RpcClient::with_transport("http://test.invalid", Box::new(Always(response)));
        let mut budget = Budget::new(60, 3, std::time::Duration::from_secs(30));

        let fees =
            fee_schedule(&client, &mut budget, &curve_at(30_130_000_000)).expect("a fee schedule");
        // 125 bps a side is what 0023 measured off mainnet.
        assert_eq!(fees.total_bps(), 125);
        assert_eq!(fees.round_trip_bps(), 250);
    }

    #[test]
    fn a_missing_fee_config_account_is_no_fee_rather_than_a_remembered_one() {
        // Rule 9 through the whole path: a fee that could not be read is a
        // refusal to price, never a constant recalled from a research note.
        let client = RpcClient::with_transport(
            "http://test.invalid",
            Box::new(Always(
                r#"{"jsonrpc":"2.0","id":1,"result":{"value":null}}"#.to_owned(),
            )),
        );
        let mut budget = Budget::new(60, 3, std::time::Duration::from_secs(30));
        assert_eq!(
            fee_schedule(&client, &mut budget, &curve_at(30_130_000_000)),
            None
        );
    }

    #[test]
    fn a_schedule_that_covers_nothing_is_no_fee_rather_than_a_free_trade() {
        // Rule 9. A fee that could not be read is a refusal to price, never a
        // zero -- a trade priced at no fee is a trade that looks profitable.
        let config = realorrug_pumpfun::FeeConfig {
            flat: Fees {
                lp_bps: 0,
                protocol_bps: 0,
                creator_bps: 0,
            },
            tiers: Vec::new(),
        };
        assert_eq!(fees_for(&config, &curve_at(30_130_000_000)), None);
    }

    #[test]
    fn capacity_is_measured_at_the_budget_the_rest_of_radar_uses() {
        // 0022: the ~$31 figure this produces is the output of this setting, not
        // a property of the venue. If this constant and `Search::DEFAULT` drift
        // apart, the analyst and the kernel publish different numbers for the
        // same token.
        assert_eq!(CAPACITY_IMPACT_BPS, 100);
    }

    #[test]
    fn a_solana_reader_answers_exactly_what_build_answers() {
        // Names the wrong implementation: `SolanaReader::read` growing its own
        // logic instead of forwarding to `build`, so the two drift the moment
        // one is edited and not the other. Run on identical clients and fresh
        // budgets so the only thing that can differ is which function ran.
        //
        // The transport here answers every call with an account-shaped
        // response, which is the wrong shape for the signature list `build`
        // asks for first -- so both paths fail identically, on the same
        // malformed-response error, rather than completing a real read. That
        // is enough: the property under test is that the two call paths agree,
        // not that this particular mock produces a successful dossier.
        let response = account_response(&fee_config_account(0, 95, 30));
        let mint = Address::new([1u8; 32]);

        let client_a =
            RpcClient::with_transport("http://test.invalid", Box::new(Always(response.clone())));
        let mut budget_a = Budget::new(60, 3, std::time::Duration::from_secs(30));
        let direct = build(&client_a, &mut budget_a, &mint, None);

        let client_b = RpcClient::with_transport("http://test.invalid", Box::new(Always(response)));
        let mut budget_b = Budget::new(60, 3, std::time::Duration::from_secs(30));
        let via_reader = SolanaReader::default().read(&client_b, &mut budget_b, &mint);

        match (direct, via_reader) {
            (Ok(d), Ok(r)) => {
                // Compared field by field rather than derived equality on the
                // whole struct: `elapsed_ms` is wall-clock and is expected to
                // differ between two separate calls, and asserting it equal
                // would make this test flaky rather than meaningful.
                assert_eq!(d.mint, r.mint);
                assert_eq!(d.read_at, r.read_at);
                assert_eq!(d.launch, r.launch);
                assert_eq!(d.curve, r.curve);
                assert_eq!(d.creator_transactions, r.creator_transactions);
                assert_eq!(d.unavailable, r.unavailable);
                assert_eq!(d.calls, r.calls);
            }
            (Err(d), Err(r)) => assert_eq!(d.to_string(), r.to_string()),
            (d, r) => panic!("build and SolanaReader disagreed on success: {d:?} vs {r:?}"),
        }
    }

    /// A second, non-Solana `ChainReader`. It never runs a real read -- it
    /// exists only so this crate compiles it against the trait, which is the
    /// one thing a Solana-shaped `ChainReader` (the defect this seam fixes)
    /// could never do. `Client = ()` because this fake makes no calls at all,
    /// and `Token = realorrug_robinhood::Address` because that 20-byte shape
    /// is exactly what would not fit in a `&Address` (Solana's 32 bytes) if
    /// the trait still hard-coded Solana's own types.
    struct FakeRobinhoodReader;

    impl ChainReader for FakeRobinhoodReader {
        type Client = ();
        type Token = realorrug_robinhood::Address;
        type Error = core::convert::Infallible;

        fn read(
            &self,
            _client: &(),
            _budget: &mut Budget,
            token: &realorrug_robinhood::Address,
        ) -> Result<Dossier, core::convert::Infallible> {
            Ok(Dossier {
                mint: ChainAddress::Solana(Address::new([9u8; 32])),
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
                unavailable: vec![Unavailable {
                    fact: "robinhood reads",
                    why: format!("fake reader, token {:?}", token.0),
                }],
                calls: 0,
                elapsed_ms: 0,
            })
        }
    }

    #[test]
    fn a_second_chains_reader_compiles_against_the_seam() {
        // The point of this test is that it compiles at all: a `ChainReader`
        // whose client is not `RpcClient` and whose token is not Solana's
        // `Address` implements the trait with no special case. That is the
        // property ADR 0028 point 2 names, and a runtime assertion on Solana
        // types alone would never notice it regressing.
        let token = realorrug_robinhood::Address([7u8; 20]);
        let mut budget = Budget::new(60, 3, std::time::Duration::from_secs(30));
        let dossier = FakeRobinhoodReader
            .read(&(), &mut budget, &token)
            .expect("the fake reader never fails");
        assert_eq!(dossier.mint, ChainAddress::Solana(Address::new([9u8; 32])));
        assert_eq!(dossier.unavailable.len(), 1);
        assert_eq!(dossier.unavailable[0].fact, "robinhood reads");
    }

    // -- packet 0039: the read memory in front of the launch-block read --

    /// A `getSignaturesForAddress` page too large to be the last one, built
    /// with an iterator rather than a hand-rolled counter (a `+= 1` loop here
    /// is exactly the shape a mutation can stall on without a test noticing,
    /// per the packet's mutation-gate notes).
    fn big_signature_page() -> String {
        let entries: Vec<String> = (0..1000)
            .map(|i| format!(r#"{{"signature":"sig{i}","slot":1,"err":null}}"#))
            .collect();
        format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":[{}]}}"#,
            entries.join(",")
        )
    }

    /// A transport that tells the mint's own signature history apart from
    /// every other address it is asked about.
    ///
    /// This is why test 1 below can assert against the *budget* rather than
    /// only the dossier: a transport that answered every address alike could
    /// not tell "the mint's own, expensive, unboundedly-paging history" from
    /// "the creator's, comparatively cheap one", so a hit's saved budget could
    /// end up silently spent by the creator-activity read gaining more room to
    /// page instead -- two different call counts that add up to the same
    /// total, which a dossier-only assertion (and a naive budget-only one)
    /// both miss.
    struct HitVsMiss(String);

    impl crate::rpc::Transport for HitVsMiss {
        fn post(&self, _: &str, body: String) -> Result<String, String> {
            if !body.contains("getSignaturesForAddress") {
                // Every account read (curve PDA, fee-config) misses cleanly.
                return Ok(r#"{"jsonrpc":"2.0","id":1,"result":{"value":null}}"#.to_owned());
            }
            if body.contains(&self.0) {
                // The mint's own history: a page too large to be the last,
                // so paging keeps going until the page budget runs out.
                Ok(big_signature_page())
            } else {
                // Any other address's history (the creator's): one empty,
                // immediately-final page.
                Ok(r#"{"jsonrpc":"2.0","id":1,"result":[]}"#.to_owned())
            }
        }
    }

    #[test]
    fn a_cached_launch_skips_signature_paging_and_spends_no_budget_on_it() {
        // Test 1 of 5 (packet 0039): the property under test is that a hit
        // makes the expensive call *at all*, not merely that the returned
        // dossier looks the same either way -- a mutation that reads the
        // chain and simply discards the cached value would still produce an
        // identical `Dossier`, which is why this asserts `calls_made`.
        let mint = Address::new([2u8; 32]);
        let mint_key = mint.to_string();
        let dir = tempfile::tempdir().expect("tempdir");
        let mem = Memory::open(&dir.path().join("mem.sqlite3")).expect("open");

        let cached = LaunchBlock {
            slot: Slot(500),
            creator: Address::new([3u8; 32]),
            recipients: Count::Exactly(1),
            transactions: Count::Exactly(1),
            dev_buy_lamports: None,
            metadata: Metadata {
                name: "Name".to_owned(),
                symbol: "SYM".to_owned(),
                uri: "uri".to_owned(),
            },
        };
        store_launch(&mem, &mint_key, &cached).expect("seed the cache");

        // Page budget of 2: the mint's own history (a page too large to be
        // last) costs exactly 2 calls before paging is cut off, so a MISS
        // (paging once, then hitting the creator's empty page in step 3
        // only if a launch was found -- which it was not) totals fewer calls
        // than a HIT would if a hit re-read the chain, and *more* than a HIT
        // that skips step 1 outright and instead pays for step 3's read of
        // the (now-known) creator's history.
        let mut miss_budget = Budget::new(60, 2, std::time::Duration::from_secs(30));
        let client =
            RpcClient::with_transport("http://test.invalid", Box::new(HitVsMiss(mint_key.clone())));
        let miss = build(&client, &mut miss_budget, &mint, None).expect("no transport error");
        // Truncated paging refuses to guess a launch (rule 9), so the miss
        // path never learns a creator to read step 3 from.
        assert!(miss.launch.is_none());
        // 2 paging + 1 curve miss + 1 token-ownership miss (getTokenLargestAccounts
        // fails immediately on this transport's `value: null`, so step 4 never
        // reaches getTokenSupply or getMultipleAccounts).
        assert_eq!(miss_budget.calls_made(), 4);

        let mut hit_budget = Budget::new(60, 2, std::time::Duration::from_secs(30));
        let hit = build(&client, &mut hit_budget, &mint, Some(&mem)).expect("no transport error");
        assert_eq!(hit.launch.as_ref(), Some(&cached));
        // 0 launch paging (the hit) + 1 curve miss + 1 creator history (empty
        // page) + 1 token-ownership miss + 1 page of the funding read, which
        // walks the mint's history on its own (slice 6b). On the miss the
        // launch walk already spent the page budget, so funding reads nothing
        // there; the two walks do not yet share their pages, which is why a
        // hit now costs as much as a miss rather than less.
        assert_eq!(hit_budget.calls_made(), 4);
        assert!(hit_budget.calls_made() <= miss_budget.calls_made());
    }

    /// A transport that answers `getSignaturesForAddress` and `getTransaction`
    /// with fixed bodies and everything else (the curve/fee reads) with no
    /// account there.
    struct SuccessfulLaunch {
        signatures: String,
        transaction: String,
    }

    #[test]
    fn the_dispatcher_reuses_a_launch_after_the_memory_is_reopened() {
        // Exercise the caller seam, not just `build`: leaving None in the
        // reader or dispatcher must bring back the expensive signature walk.
        let mint = Address::new([2u8; 32]);
        let mint_key = mint.to_string();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("memory.sqlite3");
        let memory = Memory::open(&path).expect("open");
        let launch = LaunchBlock {
            slot: Slot(500),
            creator: Address::new([3u8; 32]),
            recipients: Count::Exactly(1),
            transactions: Count::Exactly(1),
            dev_buy_lamports: None,
            metadata: Metadata {
                name: "Name".to_owned(),
                symbol: "SYM".to_owned(),
                uri: "uri".to_owned(),
            },
        };
        store_launch(&memory, &mint_key, &launch).expect("store launch");
        drop(memory);
        let memory = Memory::open(&path).expect("reopen");
        let client =
            RpcClient::with_transport("http://test.invalid", Box::new(HitVsMiss(mint_key.clone())));
        let clients = crate::dispatch::Clients {
            solana: &client,
            robinhood: None,
            market: None,
        };
        let mut budget = Budget::default();
        let dossier =
            crate::dispatch::read_with_memory(&mint_key, &clients, Some(&memory), &mut budget)
                .expect("read");
        assert_eq!(dossier.launch, Some(launch));
        // 1 curve miss + 1 creator history (empty page) + 1 token-ownership
        // miss (this transport answers `getTokenLargestAccounts` with
        // `value: null`, so step 4 fails after its first call) + 2 calls of
        // the funding read's own walk of the mint's history (slice 6b).
        assert_eq!(dossier.calls, 5);
        assert_eq!(budget.calls_made(), 5);
    }

    impl crate::rpc::Transport for SuccessfulLaunch {
        fn post(&self, _: &str, body: String) -> Result<String, String> {
            if body.contains("getSignaturesForAddress") {
                Ok(self.signatures.clone())
            } else if body.contains("getTransaction") {
                Ok(self.transaction.clone())
            } else {
                Ok(r#"{"jsonrpc":"2.0","id":1,"result":{"value":null}}"#.to_owned())
            }
        }
    }

    #[test]
    fn an_empty_memory_reads_the_chain_and_the_row_lands_in_memory_afterwards() {
        // Test 2 of 5 (packet 0039). A genuinely successful read, built from
        // the same raw pump.fun `create` payload `launch.rs`'s own tests use,
        // so `assemble` has a real launch to decode rather than a mock that
        // only exercises the miss branch.
        let mint = Address::new([4u8; 32]);
        let creator = Address::new([9u8; 32]);

        let mut discriminator = vec![0x18, 0x1e, 0xc8, 0x28, 0x05, 0x1c, 0x07, 0x77];
        for s in ["Name", "SYM", "uri"] {
            discriminator.extend_from_slice(&u32::try_from(s.len()).expect("short").to_le_bytes());
            discriminator.extend_from_slice(s.as_bytes());
        }
        discriminator.extend_from_slice(creator.as_bytes());
        let data_b58 = base58_encode(&discriminator);

        let program = realorrug_decode::pumpfun::PROGRAM_ID.to_string();
        let one_signature =
            r#"{"jsonrpc":"2.0","id":1,"result":[{"signature":"launch-sig","slot":10,"err":null}]}"#
                .to_owned();
        let transaction = format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":{{"slot":10,"meta":{{"err":null}},
               "transaction":{{"message":{{"accountKeys":["{creator}"],
               "instructions":[{{"programId":"{program}","data":"{data_b58}","accounts":[]}}]}}}}}}}}"#
        );

        let client = RpcClient::with_transport(
            "http://test.invalid",
            Box::new(SuccessfulLaunch {
                signatures: one_signature,
                transaction,
            }),
        );
        let mut budget = Budget::new(60, 10, std::time::Duration::from_secs(30));
        let dir = tempfile::tempdir().expect("tempdir");
        let mem = Memory::open(&dir.path().join("mem.sqlite3")).expect("open");

        let dossier = build(&client, &mut budget, &mint, Some(&mem)).expect("no transport error");
        let launch = dossier.launch.expect("a launch was read");
        assert_eq!(launch.slot, Slot(10));
        assert_eq!(launch.creator, creator);

        // The row is now in memory, so a second build with no transport at
        // all can still answer.
        let dead_client = RpcClient::with_transport(
            "http://test.invalid",
            Box::new(Always(
                r#"{"jsonrpc":"2.0","id":1,"result":{"value":null}}"#.to_owned(),
            )),
        );
        let mut second_budget = Budget::new(60, 10, std::time::Duration::from_secs(30));
        let second =
            build(&dead_client, &mut second_budget, &mint, Some(&mem)).expect("no transport error");
        assert_eq!(second.launch, Some(launch));
    }

    #[test]
    fn a_none_memory_behaves_exactly_as_today() {
        // Test 3 of 5 (packet 0039): the `Option` really is optional. Same
        // transport, same budget shape, with and without a memory that would
        // have missed anyway -- the two builds must agree in every field
        // that is not wall-clock.
        let response = account_response(&fee_config_account(0, 95, 30));
        let mint = Address::new([5u8; 32]);

        let client_a =
            RpcClient::with_transport("http://test.invalid", Box::new(Always(response.clone())));
        let mut budget_a = Budget::new(60, 3, std::time::Duration::from_secs(30));
        let without_memory = build(&client_a, &mut budget_a, &mint, None);

        let dir = tempfile::tempdir().expect("tempdir");
        let mem = Memory::open(&dir.path().join("mem.sqlite3")).expect("open");
        let client_b = RpcClient::with_transport("http://test.invalid", Box::new(Always(response)));
        let mut budget_b = Budget::new(60, 3, std::time::Duration::from_secs(30));
        let with_empty_memory = build(&client_b, &mut budget_b, &mint, Some(&mem));

        // This transport answers an account-shaped response to the
        // signature-page request too (same mock `a_solana_reader_answers_
        // exactly_what_build_answers` above uses), so both paths fail
        // identically on the same malformed-response error rather than
        // completing a real read -- an empty memory that missed cleanly
        // changes nothing about that outcome.
        match (without_memory, with_empty_memory) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.launch, b.launch);
                assert_eq!(a.curve, b.curve);
                assert_eq!(a.creator_transactions, b.creator_transactions);
                assert_eq!(a.unavailable, b.unavailable);
                assert_eq!(a.calls, b.calls);
            }
            (Err(a), Err(b)) => assert_eq!(a.to_string(), b.to_string()),
            (a, b) => panic!("None vs Some(empty memory) disagreed on success: {a:?} vs {b:?}"),
        }
    }

    #[test]
    fn a_stored_launch_that_disagrees_with_a_fresh_read_is_a_conflict_not_a_silent_pick() {
        // Test 4 of 5 (packet 0039): `Memory::record`'s refusal must reach
        // the caller with its own text intact, never resolved either
        // direction. `store_launch` is exactly the call `build` makes after
        // a genuine miss, so exercising it directly here tests the same
        // refusal `build` would surface as a "launch block" miss, without
        // needing a `cached_launch` hit (keyed on `(what, subject)` alone) to
        // get out of the way first.
        let mint = Address::new([6u8; 32]);
        let mint_key = mint.to_string();
        let stored_creator = Address::new([7u8; 32]);
        let fresh_creator = Address::new([8u8; 32]);

        let dir = tempfile::tempdir().expect("tempdir");
        let mem = Memory::open(&dir.path().join("mem.sqlite3")).expect("open");
        let block = |creator: Address| LaunchBlock {
            slot: Slot(10),
            creator,
            recipients: Count::Exactly(1),
            transactions: Count::Exactly(1),
            dev_buy_lamports: None,
            metadata: Metadata {
                name: "Name".to_owned(),
                symbol: "SYM".to_owned(),
                uri: "uri".to_owned(),
            },
        };

        store_launch(&mem, &mint_key, &block(stored_creator)).expect("first read is recorded");
        let err = store_launch(&mem, &mint_key, &block(fresh_creator))
            .expect_err("a same-block, different-creator re-read is a conflict");
        let text = err.to_string();
        assert!(text.contains("refusing to overwrite"));
        assert!(text.contains(&mint_key));

        // Neither value was silently kept: the stored row is exactly what
        // the first call wrote, untouched by the refused second write.
        let still_stored = cached_launch(&mem, &mint_key).expect("the first write stands");
        assert_eq!(still_stored.creator, stored_creator);
    }

    #[test]
    fn the_curve_and_creator_reads_are_never_served_from_memory() {
        // Test 5 of 5 (packet 0039), section 1's boundary: only the launch
        // record is ever read from memory. Seeded with a "curve" row under
        // the mint's own key, at the exact block the real curve read below
        // will name -- if `build` ever grew a memory lookup for the curve
        // fact, this row would satisfy it and the assertion on `curve.slot`
        // below (via `read_at`) would see the seeded row's block, 999,
        // instead of the real one the chain mock reports.
        let mint = Address::new([12u8; 32]);
        let mint_key = mint.to_string();

        let dir = tempfile::tempdir().expect("tempdir");
        let mem = Memory::open(&dir.path().join("mem.sqlite3")).expect("open");
        mem.record(
            "curve",
            &mint_key,
            999,
            Kind::TenMinutes,
            "seeded, never real",
            SystemTime::now(),
        )
        .expect("seed a curve row");

        // Truncated paging (page budget of 0) refuses to name a launch, so
        // `read_at` can only come from the curve read below -- making this
        // test fail loudly (a wrong slot) if the curve read were ever served
        // from the seeded row instead of the chain the mock actually answers.
        let response = account_response(&fee_config_account(0, 95, 30));
        let client = RpcClient::with_transport("http://test.invalid", Box::new(Always(response)));
        let mut budget = Budget::new(60, 0, std::time::Duration::from_secs(30));

        let dossier = build(&client, &mut budget, &mint, Some(&mem)).expect("no transport error");
        assert!(dossier.launch.is_none());
        // `account_response(fee_config_account(..))` is fee-config bytes, not
        // a bonding-curve account, so the real chain read genuinely fails to
        // parse -- the curve fact stays absent here regardless of memory.
        // What this test guards is that it is *absent*, never the seeded
        // string: there is no code path today that could turn a "curve" row
        // into `dossier.curve`, and this fails loudly the day one is added
        // without also adding a per-fact read point (design 0021 §1).
        assert!(dossier.curve.is_none());
        assert!(dossier.unavailable.iter().any(|u| u.fact == "curve"));
        assert_ne!(dossier.read_at, Some(ReadAt::Solana(Slot(999))));
    }

    /// Encodes bytes as base64, the same alphabet `account_response` above
    /// inlines, factored out so the token-ownership tests below can build a
    /// `getMultipleAccounts`-shaped body (several accounts, not one).
    fn base64_encode(data: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut encoded = String::new();
        for chunk in data.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for i in 0..4 {
                if i <= chunk.len() {
                    encoded.push(ALPHABET[((n >> (18 - i * 6)) & 0x3F) as usize] as char);
                } else {
                    encoded.push('=');
                }
            }
        }
        encoded
    }

    /// A minimal SPL token account: `mint` at bytes 0..32, `owner` at 32..64,
    /// `amount` at 64..72, every other field zeroed to mean "none" -- no
    /// delegate, initialized, not native, no close authority. Exactly
    /// [`realorrug_pumpfun::token::TOKEN_ACCOUNT_LEN`] bytes, the length
    /// `TokenAccount::parse` requires for the un-extended (classic SPL)
    /// shape.
    fn token_account_bytes(mint: Address, owner: Address, amount: u64) -> Vec<u8> {
        let mut data = vec![0u8; 165];
        data[0..32].copy_from_slice(mint.as_bytes());
        data[32..64].copy_from_slice(owner.as_bytes());
        data[64..72].copy_from_slice(&amount.to_le_bytes());
        data[108] = 1; // AccountState::Initialized
        data
    }

    /// A minimal SPL mint account: the mint-authority `COption<Pubkey>` at
    /// byte 0 (tag) / byte 4 (address), the freeze-authority one at byte 46 /
    /// byte 50, matching `mint_authorities`'s own doc on the shared layout.
    fn mint_account_bytes(
        mint_authority: Option<Address>,
        freeze_authority: Option<Address>,
    ) -> Vec<u8> {
        let mut data = vec![0u8; 82];
        if let Some(a) = mint_authority {
            data[0..4].copy_from_slice(&1u32.to_le_bytes());
            data[4..36].copy_from_slice(a.as_bytes());
        }
        data[45] = 1; // is_initialized
        if let Some(a) = freeze_authority {
            data[46..50].copy_from_slice(&1u32.to_le_bytes());
            data[50..82].copy_from_slice(a.as_bytes());
        }
        data
    }

    /// A `getMultipleAccounts`-shaped body naming each account's owning
    /// program as the classic SPL token program, in the order given.
    fn multi_accounts_body(accounts: &[Option<&[u8]>]) -> String {
        let values: Vec<String> = accounts
            .iter()
            .map(|a| match a {
                Some(bytes) => format!(
                    r#"{{"data":["{}","base64"],"owner":"{}"}}"#,
                    base64_encode(bytes),
                    realorrug_pumpfun::token::SPL_TOKEN_PROGRAM
                ),
                None => "null".to_owned(),
            })
            .collect();
        format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":{{"value":[{}],"context":{{"slot":1}}}}}}"#,
            values.join(",")
        )
    }

    fn largest_accounts_body(entries: &[(Address, u64)]) -> String {
        let values: Vec<String> = entries
            .iter()
            .map(|(addr, amount)| format!(r#"{{"address":"{addr}","amount":"{amount}"}}"#))
            .collect();
        format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":{{"value":[{}]}}}}"#,
            values.join(",")
        )
    }

    fn supply_body(amount: u64, decimals: u8) -> String {
        format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":{{"value":{{"amount":"{amount}","decimals":{decimals}}}}}}}"#
        )
    }

    /// A transport that routes to canned bodies for the three
    /// `token_ownership` reads by method name, and errors on anything else --
    /// nothing else should ever be called from a bare `token_ownership` test.
    struct TokenOwnershipMock {
        largest: String,
        supply: String,
        multi: String,
    }

    impl crate::rpc::Transport for TokenOwnershipMock {
        fn post(&self, _: &str, body: String) -> Result<String, String> {
            if body.contains("getTokenLargestAccounts") {
                Ok(self.largest.clone())
            } else if body.contains("getTokenSupply") {
                Ok(self.supply.clone())
            } else if body.contains("getMultipleAccounts") {
                Ok(self.multi.clone())
            } else {
                Err(format!("unexpected call: {body}"))
            }
        }
    }

    /// A transport whose `getMultipleAccounts` answer is a transport-level
    /// error, so `token_ownership` fails on the batched read specifically
    /// rather than on the two calls ahead of it.
    struct FailingMultiAccounts {
        largest: String,
        supply: String,
    }

    impl crate::rpc::Transport for FailingMultiAccounts {
        fn post(&self, _: &str, body: String) -> Result<String, String> {
            if body.contains("getTokenLargestAccounts") {
                Ok(self.largest.clone())
            } else if body.contains("getTokenSupply") {
                Ok(self.supply.clone())
            } else if body.contains("getMultipleAccounts") {
                Err("connection reset".to_owned())
            } else {
                Err(format!("unexpected call: {body}"))
            }
        }
    }

    #[test]
    fn two_accounts_under_one_owner_aggregate_to_one_holder() {
        // Requirement (a): two of the sampled largest accounts share a
        // decoded `owner`, so they must land as one `TokenOwner` with
        // `accounts: 2` and a summed `amount`, not two separate entries.
        let mint = Address::new([20u8; 32]);
        let owner = Address::new([21u8; 32]);
        let account_a = Address::new([22u8; 32]);
        let account_b = Address::new([23u8; 32]);

        let largest = largest_accounts_body(&[(account_a, 600), (account_b, 400)]);
        let supply = supply_body(1_000, 6);
        let multi = multi_accounts_body(&[
            Some(&token_account_bytes(mint, owner, 600)),
            Some(&token_account_bytes(mint, owner, 400)),
            Some(&mint_account_bytes(None, None)),
        ]);

        let client = RpcClient::with_transport(
            "http://test.invalid",
            Box::new(TokenOwnershipMock {
                largest,
                supply,
                multi,
            }),
        );
        let mut budget = Budget::new(60, 10, std::time::Duration::from_secs(30));
        let facts = token_ownership(&client, &mut budget, &mint).expect("a token ownership read");

        assert_eq!(facts.owners.len(), 1);
        let only = &facts.owners[0];
        assert_eq!(only.owner, owner);
        assert_eq!(only.accounts, 2);
        assert_eq!(only.amount, 1_000);
        // 1,000 of 1,000 is the whole supply: 10,000 bps, not a rounded 9,999
        // or a truncated-to-zero mutant.
        assert_eq!(only.share_bps, Some(10_000));
    }

    #[test]
    fn a_curve_owned_account_is_excluded_with_proof_and_an_unproven_one_is_kept_unresolved() {
        // Requirement (b): the bonding curve's owner is proven by recomputing
        // its own PDA from the mint, never by inference from balance size --
        // a second, equally large owner with no such proof must stay
        // unresolved rather than being guessed at from its balance alone.
        let mint = Address::new([30u8; 32]);
        let curve = pda::bonding_curve(&mint).expect("a bonding curve PDA for this mint");
        let unproven_owner = Address::new([31u8; 32]);
        let curve_account = Address::new([32u8; 32]);
        let other_account = Address::new([33u8; 32]);

        let largest = largest_accounts_body(&[(curve_account, 500), (other_account, 500)]);
        let supply = supply_body(1_000, 6);
        let multi = multi_accounts_body(&[
            Some(&token_account_bytes(mint, curve, 500)),
            Some(&token_account_bytes(mint, unproven_owner, 500)),
            Some(&mint_account_bytes(None, None)),
        ]);

        let client = RpcClient::with_transport(
            "http://test.invalid",
            Box::new(TokenOwnershipMock {
                largest,
                supply,
                multi,
            }),
        );
        let mut budget = Budget::new(60, 10, std::time::Duration::from_secs(30));
        let facts = token_ownership(&client, &mut budget, &mint).expect("a token ownership read");

        assert_eq!(facts.owners.len(), 2);
        let curve_owner = facts
            .owners
            .iter()
            .find(|o| o.owner == curve)
            .expect("the curve owner is still in the list, just proven");
        assert_eq!(curve_owner.role, OwnerRole::BondingCurve);
        let other = facts
            .owners
            .iter()
            .find(|o| o.owner == unproven_owner)
            .expect("the unproven owner is kept, not dropped");
        assert_eq!(other.role, OwnerRole::Unresolved);
    }

    #[test]
    fn a_failed_batched_account_read_is_a_gap_not_a_zero() {
        // Requirement (c): a `getMultipleAccounts` transport failure must
        // surface as an `Err` from `token_ownership` -- which `build` records
        // in `Dossier::unavailable` under "token ownership" and leaves
        // `dossier.token_ownership` as `None` -- never an empty or
        // zero-valued `TokenOwnership`.
        let mint = Address::new([40u8; 32]);
        let largest = largest_accounts_body(&[(Address::new([41u8; 32]), 100)]);
        let supply = supply_body(1_000, 6);

        let client = RpcClient::with_transport(
            "http://test.invalid",
            Box::new(FailingMultiAccounts { largest, supply }),
        );
        let mut budget = Budget::new(60, 10, std::time::Duration::from_secs(30));
        let err = token_ownership(&client, &mut budget, &mint)
            .expect_err("a transport failure on the batched read must not become a value");
        assert!(matches!(err, RpcError::Transport(_)));

        // The same failure, exercised through `build`, must land as a named
        // gap rather than aborting the whole dossier (AGENTS.md rule 8/9).
        let client = RpcClient::with_transport(
            "http://test.invalid",
            Box::new(FailingMultiAccounts {
                largest: largest_accounts_body(&[(Address::new([41u8; 32]), 100)]),
                supply: supply_body(1_000, 6),
            }),
        );
        let mut budget = Budget::new(60, 0, std::time::Duration::from_secs(30));
        let dossier = build(&client, &mut budget, &mint, None).expect("a partial dossier, not Err");
        assert!(dossier.token_ownership.is_none());
        assert!(
            dossier
                .unavailable
                .iter()
                .any(|u| u.fact == "token ownership")
        );
    }

    /// Encodes bytes as base58, mirroring `rpc::decode_base58`'s own alphabet
    /// -- test-only, so a chain read can be mocked without adding `bs58` as a
    /// dependency this packet did not ask for.
    fn base58_encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
        let mut digits: Vec<u8> = vec![0];
        for &byte in bytes {
            let mut carry = u32::from(byte);
            for digit in &mut digits {
                carry += u32::from(*digit) << 8;
                *digit = u8::try_from(carry % 58).unwrap_or(0);
                carry /= 58;
            }
            while carry > 0 {
                digits.push(u8::try_from(carry % 58).unwrap_or(0));
                carry /= 58;
            }
        }
        let leading_zeros = bytes.iter().take_while(|&&b| b == 0).count();
        let mut out: String = std::iter::repeat_n('1', leading_zeros).collect();
        out.extend(digits.iter().rev().map(|&d| ALPHABET[d as usize] as char));
        out
    }
}
