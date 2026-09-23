// SPDX-License-Identifier: Apache-2.0
//! The JSON-RPC calls a dossier is built from.
//!
//! Modelled on [`radar_sim::rpc`], which is the shape this repository already
//! uses for "fetch an account and parse it": a `ureq` agent, a hand-rolled
//! base64 decoder, and typed errors that distinguish a transport failure from a
//! node error from an account that is not there.
//!
//! # Direct RPC, never the x402 lane
//!
//! AGENTS.md rule 7 forbids the x402 lane on the execution path because on-chain
//! settlement adds 400–800ms. Nothing here executes, so the rule does not apply
//! by its letter — but its *reasoning* does, and more sharply. This path exists
//! to answer while a thread is still alive; a settlement round trip per call,
//! against a budget of up to sixty calls, would put the reply minutes late.
//! Latency is the product here.
//!
//! # Every call is drawn from a budget
//!
//! The client decrements the [`Budget`] itself. That is deliberate: a caller
//! that had to remember would eventually not, and the caller is a path a
//! stranger triggers.

use std::time::Duration;

use realorrug_types::{Address, Slot};
use serde::Deserialize;

use crate::budget::{Budget, Exhausted};

/// A public Solana RPC endpoint. Overridable for a paid one.
///
/// The public endpoint is rate-limited hard enough that it is a development
/// convenience rather than something to serve from — `REALORRUG_RPC` is how a real
/// deployment points this at Helius.
pub const DEFAULT_RPC: &str = "https://api.mainnet-beta.solana.com";

/// Why a read could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    /// The endpoint could not be reached or refused.
    #[error("rpc transport: {0}")]
    Transport(String),
    /// The node answered with an error.
    #[error("rpc error: {0}")]
    Node(String),
    /// The response did not have the shape expected.
    #[error("unreadable rpc response: {0}")]
    Malformed(String),
    /// The read hit a bound before it finished.
    ///
    /// Carried as an error only where the read cannot continue at all. Where a
    /// partial answer is still useful the caller catches this and records a
    /// [`Count::AtLeast`](crate::budget::Count::AtLeast) instead.
    #[error("read stopped: {0:?}")]
    Stopped(Exhausted),
}

impl From<Exhausted> for RpcError {
    fn from(e: Exhausted) -> Self {
        Self::Stopped(e)
    }
}

/// Whether a transport error is the node's HTTP 429, not some other refusal.
///
/// `Http::post` maps ureq's status error through `Display`, which for a 429
/// is exactly the text `"http status: 429"` -- matched by substring rather
/// than parsed, since the transport hands back a string and not a status
/// code. A dropped connection or a malformed body is a different failure and
/// must not be retried the same way: retrying a down endpoint just spends the
/// budget faster, and retrying a malformed body cannot change its shape.
fn is_rate_limited(message: &str) -> bool {
    message.contains("http status: 429")
}

/// One confirmed signature, as `getSignaturesForAddress` returns it.
#[derive(Clone, Debug, Deserialize)]
pub struct SignatureInfo {
    /// The transaction signature.
    pub signature: String,
    /// The slot it landed in.
    pub slot: u64,
    /// Present and non-null when the transaction failed.
    ///
    /// **Load-bearing.** 0006 found that 35 of 97 migration instructions in one
    /// hour were in *failed* transactions, and counting them overstated the
    /// label by more than a third. A failed transaction is not an event.
    #[serde(default)]
    pub err: Option<serde_json::Value>,
}

/// A transaction, in the subset of `getTransaction`'s shape this crate reads.
#[derive(Clone, Debug)]
pub struct Transaction {
    /// The slot it landed in.
    pub slot: Slot,
    /// Account keys, in the order the message lists them.
    pub accounts: Vec<String>,
    /// Every instruction, top-level and inner, as (program, data) pairs.
    ///
    /// Flattened deliberately. pump.fun's `create` is a top-level instruction
    /// but the dev buy that follows it is frequently a CPI, and a decoder that
    /// read only the outer list would report a launch with no dev buy — a
    /// zero that is really an absence, which is rule 9's exact shape.
    pub instructions: Vec<RawInstruction>,
    /// Token balances before, as `(account_index, mint, amount)`.
    pub pre_token_balances: Vec<TokenBalance>,
    /// Token balances after.
    pub post_token_balances: Vec<TokenBalance>,
    /// Native lamport balances before the transaction, indexed the same way
    /// as `accounts`. Empty when the node omitted `meta.preBalances`.
    ///
    /// Needed for `crate::wallets`' Solana funding read: a funding transfer
    /// is a native-lamport move, not a token one, so it cannot be seen in
    /// `pre_token_balances`/`post_token_balances` at all.
    pub pre_balances: Vec<u64>,
    /// Native lamport balances after the transaction, on the same terms as
    /// `pre_balances`.
    pub post_balances: Vec<u64>,
    /// Whether the transaction failed.
    pub failed: bool,
}

/// One instruction, reduced to what a discriminator match needs.
#[derive(Clone, Debug)]
pub struct RawInstruction {
    /// The program that owns it.
    pub program: String,
    /// Its data, decoded from base58.
    pub data: Vec<u8>,
    /// The account addresses it names, in order.
    pub accounts: Vec<String>,
}

/// One token balance entry.
#[derive(Clone, Debug)]
pub struct TokenBalance {
    /// Index into the transaction's account keys.
    pub account_index: usize,
    /// The mint this balance is of.
    pub mint: String,
    /// The raw amount.
    pub amount: u64,
    /// The account's owner, when the node reported one.
    pub owner: Option<String>,
}

/// One entry from `getTokenLargestAccounts`.
///
/// **Not who holds the balance.** This is a token account's own address and
/// its raw amount; the account that actually owns those units is a separate
/// field inside the account's data, only visible after reading it with
/// [`RpcClient::accounts`] (see `crate::dossier`'s aggregation of this list
/// by owner, and its module doc on why an account is not a person).
#[derive(Clone, Debug)]
pub struct LargestTokenAccount {
    /// The token account's own address.
    pub address: Address,
    /// The raw balance, in the mint's smallest unit.
    pub amount: u64,
}

/// `getTokenSupply`'s answer.
#[derive(Clone, Copy, Debug)]
pub struct TokenSupply {
    /// The raw total, in the mint's smallest unit.
    pub amount: u64,
    /// The mint's decimal places.
    pub decimals: u8,
}

#[derive(Deserialize)]
struct LargestAccountsEnvelope {
    value: Option<Vec<LargestAccountValue>>,
}

#[derive(Deserialize)]
struct LargestAccountValue {
    address: String,
    amount: String,
}

#[derive(Deserialize)]
struct TokenAccountsEnvelope {
    value: Option<Vec<TokenAccountEntry>>,
}

#[derive(Deserialize)]
struct TokenAccountEntry {
    pubkey: String,
}

#[derive(Deserialize)]
struct TokenSupplyEnvelope {
    value: Option<TokenSupplyValue>,
}

#[derive(Deserialize)]
struct TokenSupplyValue {
    amount: String,
    decimals: u8,
}

#[derive(Deserialize)]
struct Envelope<T> {
    result: Option<T>,
    error: Option<NodeError>,
}

#[derive(Deserialize)]
struct NodeError {
    message: String,
}

/// The `data` half of `getTransactionsForAddress`'s response.
///
/// `paginationToken` is ignored: [`RpcClient::signatures_oldest_first`], the
/// only caller, takes exactly one page and never pages forward.
#[derive(Deserialize)]
struct TransactionsForAddressEnvelope {
    data: Vec<SignatureInfo>,
}

/// The `data` half of `getTransactionsForAddress`'s **full**-mode response
/// (`transactionDetails: "full"`, [`RpcClient::funding_transactions_page`]):
/// each row is shaped exactly like `getTransaction`'s own single result --
/// `transaction`/`meta`/`slot` -- confirmed against a real Helius response
/// (research 0056 addendum, 2026-09-23), so [`parse_transaction`] reads a row
/// here unmodified rather than this crate carrying a second parser for it.
/// `paginationToken` is `Some` while more history remains past this page.
#[derive(Deserialize)]
struct FullTransactionsEnvelope {
    data: Vec<serde_json::Value>,
    #[serde(rename = "paginationToken")]
    pagination_token: Option<String>,
}

#[derive(Deserialize)]
struct AccountEnvelope {
    value: Option<AccountValue>,
    /// The node's own statement of when it read this.
    ///
    /// Optional because a node that omits it must not turn a good read into an
    /// error -- the slot is carried forward as `None`, which is rule 9: the
    /// caller then knows it has no slot rather than being handed a wrong one.
    context: Option<AccountContext>,
}

#[derive(Deserialize)]
struct AccountContext {
    slot: u64,
}

#[derive(Deserialize)]
struct MultiEnvelope {
    value: Option<Vec<Option<AccountValue>>>,
    context: Option<AccountContext>,
}

#[derive(Deserialize)]
struct AccountValue {
    data: Vec<String>,
    owner: Option<String>,
}

/// One account out of a multi-account read.
#[derive(Clone, Debug)]
pub struct OwnedAccount {
    /// The account's raw data.
    pub data: Vec<u8>,
    /// The program that owns it, when the node said.
    ///
    /// `Option` because the node's shape allows it to be absent, and because a
    /// caller that needs the owner has to refuse rather than guess: an account's
    /// owner is what says whether it is an SPL token account, a Token-2022 one,
    /// or something that is not a token account at all.
    pub owner: Option<String>,
}

/// Several accounts read in one call, at one slot.
///
/// # Why this type exists rather than a `Vec<AccountRead>`
///
/// Reserves are a ratio between two balances. Read one vault at slot N and the
/// other at slot N+40 and the ratio is a price that never existed on the chain
/// — and it looks exactly like a price that did. `getMultipleAccounts` answers
/// with a **single** `context.slot` covering every account in the request, so
/// the one slot here is the slot all of them were read at, by construction, and
/// there is no per-account slot to be tempted into mixing.
///
/// `slot` is still `Option`: a node that omits the context has not told us when
/// it read, and rule 9 says that is a refusal for the caller to make rather than
/// a zero to invent.
#[derive(Clone, Debug)]
pub struct MultiAccountRead {
    /// The slot every account below was read at, when the node said.
    pub slot: Option<Slot>,
    /// The accounts, in the order they were asked for. `None` where nothing
    /// exists at that address — which is a different fact from an empty account.
    pub accounts: Vec<Option<OwnedAccount>>,
}

/// One account read, with the slot the node read it at.
///
/// The two travel together **by construction**. Every figure this account
/// publishes has to carry the slot it was read at or it cannot be checked on an
/// explorer, and returning the bytes alone made dropping the slot the path of
/// least resistance -- which is what happened: `Dossier::read_at` came only
/// from the launch block, and the launch block is unreadable for any coin with
/// enough history to be worth asking about.
#[derive(Clone, Debug)]
pub struct AccountRead {
    /// The account's raw data.
    pub data: Vec<u8>,
    /// The slot the node served it at, when the node said.
    pub slot: Option<Slot>,
}

/// How a JSON-RPC body gets to a node and back.
///
/// A trait so the read path can be tested **without a network**. Every method on
/// [`RpcClient`] is one HTTP call deep, and mutating any of them to return a
/// constant survived mutation testing for exactly that reason: nothing without
/// an endpoint could observe the difference. `.cargo/mutants.toml` is explicit
/// that "no test covers it" means write the test rather than record an
/// exclusion, so the transport became injectable instead.
///
/// The same shape as `realorrug_model::Provider` and
/// [`crate::rpc`]'s callers elsewhere: the impure edge is one small object, and
/// everything above it is testable.
pub trait Transport: Send + Sync {
    /// Posts a body and returns the response text.
    ///
    /// # Errors
    ///
    /// A transport-level message. A non-2xx whose body explains the problem is
    /// still a success at this layer — ClickHouse and Solana nodes both answer
    /// a bad request with a body worth reading, and swallowing it costs a
    /// debugging round trip.
    fn post(&self, endpoint: &str, body: String) -> Result<String, String>;
}

/// The real one.
struct Http {
    agent: ureq::Agent,
}

impl Transport for Http {
    fn post(&self, endpoint: &str, body: String) -> Result<String, String> {
        let mut response = self
            .agent
            .post(endpoint)
            .content_type("application/json")
            .send(body)
            .map_err(|e| e.to_string())?;
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())
    }
}

/// Reads accounts, signatures and transactions.
pub struct RpcClient {
    endpoint: String,
    transport: Box<dyn Transport>,
}

impl core::fmt::Debug for RpcClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RpcClient")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl Default for RpcClient {
    fn default() -> Self {
        Self::new(DEFAULT_RPC)
    }
}

impl RpcClient {
    /// A client against the given endpoint.
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build();
        Self {
            endpoint: endpoint.into(),
            transport: Box::new(Http {
                agent: config.into(),
            }),
        }
    }

    /// A client over a supplied transport.
    ///
    /// For tests, and for anything that needs to record or replay what a node
    /// said. The endpoint is still carried because it is reported to operators
    /// and because a recorded exchange is meaningless without knowing who
    /// answered it.
    #[must_use]
    pub fn with_transport(endpoint: impl Into<String>, transport: Box<dyn Transport>) -> Self {
        Self {
            endpoint: endpoint.into(),
            transport,
        }
    }

    /// The endpoint from `REALORRUG_RPC`, or the public one.
    ///
    /// Not deny-by-default, and the distinction from rule 8 is worth stating:
    /// rule 8 governs config whose absence would let Radar *spend* or *permit*
    /// something. A missing RPC endpoint permits nothing — the fallback is a
    /// free public node, and the cost of getting it wrong is a slow answer
    /// rather than an unmetered one. The spend that does need a meter is the
    /// model call and the reply, and those are Phase 2 and Phase 3.
    /// The endpoint from a lookup function.
    ///
    /// Takes the lookup rather than reading the environment itself, so the
    /// choice is testable: `std::env::set_var` is `unsafe` in edition 2024, this
    /// crate forbids `unsafe`, and a test mutating global state would race every
    /// other test in the binary anyway. Same shape as `realorrug_model::from_vars`,
    /// which exists for the same reason.
    ///
    /// There is deliberately **no** `from_env` wrapper. One existed, it was a
    /// single delegating line, and it was the only function in the crate that no
    /// test could reach — so mutating it away survived. A convenience that
    /// cannot be checked is not worth the line it saves; callers pass
    /// `&|k| std::env::var(k).ok()`.
    #[must_use]
    pub fn from_vars(get: &impl Fn(&str) -> Option<String>) -> Self {
        realorrug_types::env::env_or_legacy("REALORRUG_RPC", "RADAR_RPC", get)
            .map_or_else(Self::default, Self::new)
    }

    /// Which endpoint this client talks to.
    ///
    /// Exposed so an operator can be told what is actually running, and so
    /// [`RpcClient::from_env`] is testable — replacing that function with
    /// `Default::default()` survived mutation because nothing could observe
    /// which endpoint had been chosen. The consequence of that mutant is a
    /// deployment configured for a paid archival node silently using the free
    /// public one, which rate-limits and cannot reach the history a launch
    /// lookup needs.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn call<T: for<'de> Deserialize<'de>>(
        &self,
        budget: &mut Budget,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<T, RpcError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": method, "params": params,
        });

        let text = self.post_with_retry(budget, &body.to_string())?;

        let envelope: Envelope<T> =
            serde_json::from_str(&text).map_err(|e| RpcError::Malformed(e.to_string()))?;

        if let Some(err) = envelope.error {
            return Err(RpcError::Node(err.message));
        }
        envelope
            .result
            .ok_or_else(|| RpcError::Malformed(format!("{method} returned no result")))
    }

    /// Posts `body`, retrying while the node refuses with HTTP 429.
    ///
    /// Evidence (VPS recapture of the 9-token replay set against main
    /// 17d6c5f, 2026-09-23): 6 of 9 cases hit `rpc transport: http status:
    /// 429` (5, 7, 11, 23 and 18 occurrences in four of them, one case with
    /// 5), each recorded as a gap and pushing the case to `CantTell`. The
    /// other three cases, and a `getSlot` sent a minute after one of the
    /// failures, answered 200 -- this is a burst/per-second limit, not an
    /// exhausted account, and a request refused for that reason is worth
    /// sending again rather than turning into a gap.
    ///
    /// The pause is a fixed constant, not a doubling backoff: an exponential
    /// schedule is arithmetic a mutation test cannot pin down (halving the
    /// base or dropping the multiplier still "backs off", just slower, and
    /// the test would have to assert an exact wall-clock curve to catch it),
    /// while "did it wait at least the fixed pause" is a single, checkable
    /// fact.
    ///
    /// `budget.take_call()` runs before every attempt, including retries, so
    /// a rate-limited read cannot spend more calls or more wall clock than
    /// the budget allows -- AGENTS.md rule 7 (a retry must never outlive the
    /// budget).
    fn post_with_retry(&self, budget: &mut Budget, body: &str) -> Result<String, RpcError> {
        /// Retries after the first attempt. Three extra tries turns a
        /// burst limit into a wait of a few seconds; it does not chase an
        /// endpoint that is down or an account that is actually exhausted,
        /// both of which fail a different way (see [`is_rate_limited`]).
        const RETRIES: u32 = 3;
        /// The wait between attempts. Fixed for the reason on
        /// [`RpcClient::post_with_retry`]: no arithmetic for a mutation to
        /// hide in.
        const PAUSE: Duration = Duration::from_secs(1);

        let mut last_err = String::new();
        // `0..=RETRIES` rather than a hand-written counter: a mutated
        // counter that never reaches its limit loops forever, and CI reports
        // that as a timeout rather than a failing assertion. A range has no
        // counter to mutate.
        for _ in 0..=RETRIES {
            budget.take_call()?;
            match self.transport.post(&self.endpoint, body.to_owned()) {
                Ok(text) => return Ok(text),
                Err(e) if is_rate_limited(&e) => {
                    last_err = e;
                    std::thread::sleep(PAUSE);
                }
                Err(e) => return Err(RpcError::Transport(e)),
            }
        }
        // Every retry was also rate-limited. Return the 429 unchanged so the
        // gap wording stays what it is today.
        Err(RpcError::Transport(last_err))
    }

    /// Reads an account's raw data. `None` when no account exists there.
    ///
    /// The distinction is not cosmetic: an address with no account is not a
    /// token whose properties are unknown, it is not a token. Collapsing the
    /// two would let a typo'd mint produce a dossier full of defaults.
    ///
    /// # Errors
    ///
    /// [`RpcError`] on transport, node or shape failures, or when the budget is
    /// spent.
    pub fn account(
        &self,
        budget: &mut Budget,
        address: &Address,
    ) -> Result<Option<AccountRead>, RpcError> {
        let result: AccountEnvelope = self.call(
            budget,
            "getAccountInfo",
            &serde_json::json!([address.to_string(), { "encoding": "base64" }]),
        )?;
        let slot = result.context.map(|c| Slot(c.slot));

        let Some(account) = result.value else {
            return Ok(None);
        };
        let encoded = account
            .data
            .first()
            .ok_or_else(|| RpcError::Malformed("account data was empty".to_owned()))?;
        let data = decode_base64(encoded)
            .ok_or_else(|| RpcError::Malformed("account data was not base64".to_owned()))?;
        Ok(Some(AccountRead { data, slot }))
    }

    /// Reads several accounts in one call, so that they share one slot.
    ///
    /// This is the only read in the crate that can answer a question about more
    /// than one account at the same instant, and that is the whole reason it
    /// exists: a pool's two vault balances read separately are two instants, and
    /// the ratio between them is a price nobody could find on an explorer.
    ///
    /// The returned vector is the same length as `addresses` and in the same
    /// order, with `None` where no account exists — checked here rather than
    /// left to the caller, because a node that returned a short list would
    /// otherwise shift every account onto the wrong address.
    ///
    /// # Errors
    ///
    /// [`RpcError`] on transport, node or shape failures, or when the budget is
    /// spent. A response whose list is a different length from the request is
    /// [`Malformed`](RpcError::Malformed).
    pub fn accounts(
        &self,
        budget: &mut Budget,
        addresses: &[Address],
    ) -> Result<MultiAccountRead, RpcError> {
        let asked: Vec<String> = addresses.iter().map(ToString::to_string).collect();
        let result: MultiEnvelope = self.call(
            budget,
            "getMultipleAccounts",
            &serde_json::json!([asked, { "encoding": "base64" }]),
        )?;
        let slot = result.context.map(|c| Slot(c.slot));
        let values = result.value.ok_or_else(|| {
            RpcError::Malformed("getMultipleAccounts returned no list".to_owned())
        })?;
        if values.len() != addresses.len() {
            return Err(RpcError::Malformed(format!(
                "asked for {} accounts and the node returned {}",
                addresses.len(),
                values.len()
            )));
        }

        let mut accounts = Vec::with_capacity(values.len());
        for value in values {
            let Some(account) = value else {
                accounts.push(None);
                continue;
            };
            let encoded = account
                .data
                .first()
                .ok_or_else(|| RpcError::Malformed("account data was empty".to_owned()))?;
            let data = decode_base64(encoded)
                .ok_or_else(|| RpcError::Malformed("account data was not base64".to_owned()))?;
            accounts.push(Some(OwnedAccount {
                data,
                owner: account.owner,
            }));
        }
        Ok(MultiAccountRead { slot, accounts })
    }

    /// Which program owns an address, or `None` when nothing is there.
    ///
    /// # Why a payout path needs this
    ///
    /// A Solana address is 32 bytes of base58 and **a wallet looks exactly like
    /// a coin's mint**. The only way to tell them apart is to ask the chain who
    /// owns the account: a wallet is owned by the system program, or does not
    /// exist yet; a mint is owned by the token program; a token account, a PDA
    /// and a program are owned by something else again.
    ///
    /// `None` for an address with no account is **correct and is a wallet**. A
    /// keypair that has never received lamports has no account on chain, and
    /// refusing to pay one would refuse a legitimate winner who generated a
    /// fresh address for the prize -- which is the sensible thing to do with a
    /// public payout.
    ///
    /// # Errors
    ///
    /// [`RpcError`] when the node cannot be reached or answers with something
    /// this cannot read. **Never treat an error as "it is a wallet"**: the
    /// caller refuses, because an unreadable owner is an unknown one and rule 9
    /// says unknown is not safe.
    pub fn owner_of(
        &self,
        budget: &mut Budget,
        address: &Address,
    ) -> Result<Option<String>, RpcError> {
        let result: AccountEnvelope = self.call(
            budget,
            "getAccountInfo",
            &serde_json::json!([address.to_string(), { "encoding": "base64" }]),
        )?;
        Ok(result.value.and_then(|a| a.owner))
    }

    /// Walks back through an address's signatures to the oldest one.
    ///
    /// Returns the signatures found and whether paging stopped at a bound. The
    /// flag is what turns "this is the launch" into "this is the oldest we
    /// looked at", and the caller must not conflate them: for a token with more
    /// signatures than the page budget allows, the oldest signature seen is an
    /// ordinary trade and reading it as a launch would invent a launch block.
    ///
    /// # Errors
    ///
    /// [`RpcError`] on transport, node or shape failures. A budget spent while
    /// paging is **not** an error — it is the `truncated` flag, because the
    /// signatures already collected are still worth having.
    pub fn signatures_back_to_oldest(
        &self,
        budget: &mut Budget,
        address: &Address,
    ) -> Result<(Vec<SignatureInfo>, bool), RpcError> {
        let mut all: Vec<SignatureInfo> = Vec::new();
        let mut before: Option<String> = None;

        loop {
            let page = match self.signatures_page(budget, address, before.as_deref()) {
                Ok(p) => p,
                Err(RpcError::Stopped(_)) => return Ok((all, true)),
                Err(e) => return Err(e),
            };
            let Some(page) = page else {
                // `None` means the budget was spent before the call was made.
                return Ok((all, true));
            };

            let Some(last) = page.last() else {
                // An empty page means the history ended, which is the only way
                // this loop concludes that it has reached the beginning.
                return Ok((all, false));
            };
            before = Some(last.signature.clone());
            let short = is_last_page(page.len());
            all.extend(page);
            if short {
                return Ok((all, false));
            }
        }
    }

    /// Fetches one page of `getSignaturesForAddress`, newest-first.
    ///
    /// `before`, when given, is a signature to page backwards from (the
    /// node's own cursor); omit it to start at the newest signature. Returns
    /// `Ok(None)` when the page budget is spent before the call is made, so a
    /// caller paging its own walk can tell "no pages left" apart from "the
    /// node returned nothing" without inspecting the budget itself.
    ///
    /// This is the one place that builds `getSignaturesForAddress`'s params;
    /// [`RpcClient::signatures_back_to_oldest`] is built on top of it rather
    /// than duplicating the call.
    ///
    /// # Errors
    ///
    /// [`RpcError`] on transport, node or shape failures.
    pub fn signatures_page(
        &self,
        budget: &mut Budget,
        address: &Address,
        before: Option<&str>,
    ) -> Result<Option<Vec<SignatureInfo>>, RpcError> {
        if budget.take_page().is_err() {
            return Ok(None);
        }
        let params = before.map_or_else(
            || serde_json::json!([address.to_string(), { "limit": PAGE_SIZE }]),
            |b| serde_json::json!([address.to_string(), { "limit": PAGE_SIZE, "before": b }]),
        );
        let page: Vec<SignatureInfo> = self.call(budget, "getSignaturesForAddress", &params)?;
        Ok(Some(page))
    }

    /// Reaches a busy address's launch in one call by asking for its
    /// signature history oldest-first, instead of paging backward from the
    /// newest.
    ///
    /// `getSignaturesForAddress` (see [`RpcClient::signatures_back_to_oldest`])
    /// only pages newest-first: a mint with more history than the page
    /// budget allows never reaches its own launch that way (research 0056
    /// addendum, 2026-09-23 -- 7 of 9 real pump.fun mints hit this in
    /// production). Helius's `getTransactionsForAddress` accepts
    /// `sortOrder: "asc"` and starts reading at the very beginning of the
    /// address's history, so **the first entry of the list it returns IS
    /// the mint's first transaction**, and because the call started at the
    /// beginning rather than wherever a newest-first walk happened to stop,
    /// the list is complete from the start -- never truncated, even when
    /// the backward walk was.
    ///
    /// Helius-only: every other RPC this repo has tried answers
    /// `getTransactionsForAddress` with a JSON-RPC "method not found" (or
    /// similar) error. That comes back through [`RpcError::Node`] like any
    /// other node error -- `Err`, never a false "complete" -- so a caller on
    /// a non-Helius endpoint keeps today's truncated-history gap rather than
    /// inventing a launch (AGENTS.md rule 8: unknown is not safe).
    ///
    /// This is a fallback, not a replacement for
    /// [`RpcClient::signatures_back_to_oldest`]: callers should try it only
    /// after that walk reports `truncated`. It costs one extra call (10
    /// Helius credits per Helius's docs, for a signatures-only read) per
    /// busy mint.
    ///
    /// Returns `Ok(None)` when the page budget is spent before the call is
    /// made, the same convention [`RpcClient::signatures_page`] uses.
    ///
    /// # Errors
    ///
    /// [`RpcError`] on transport, node (including method-not-found) or shape
    /// failures.
    pub fn signatures_oldest_first(
        &self,
        budget: &mut Budget,
        address: &Address,
        limit: usize,
    ) -> Result<Option<Vec<SignatureInfo>>, RpcError> {
        if budget.take_page().is_err() {
            return Ok(None);
        }
        let limit = limit.min(PAGE_SIZE);
        let result: TransactionsForAddressEnvelope = self.call(
            budget,
            "getTransactionsForAddress",
            &serde_json::json!([
                address.to_string(),
                {
                    "transactionDetails": "signatures",
                    "sortOrder": "asc",
                    "limit": limit,
                }
            ]),
        )?;
        Ok(Some(result.data))
    }

    /// One page of a Solana address's transaction history in Helius's
    /// **full** mode: up to [`FULL_TRANSACTIONS_PAGE_SIZE`] whole
    /// transactions per call, each already carrying its balances and
    /// instructions, instead of one `getSignaturesForAddress` entry plus a
    /// separate `getTransaction` call per signature (research 0056
    /// addendum, 2026-09-23 -- an active buyer routinely has more
    /// pre-purchase transactions than the old per-transaction walk's caps
    /// allowed it to read).
    ///
    /// `at_or_before_slot` becomes Helius's `filters.slot.lte`: only
    /// transactions at or before that slot come back, newest first
    /// (`sortOrder: "desc"`) -- callers wanting the same-slot inclusion
    /// [`crate::wallets::funding_search`] relies on should pass the
    /// purchase's own slot, not one less. `pagination_token`, when `Some`,
    /// is a previous call's own `paginationToken` and continues that walk;
    /// `None` starts at the newest matching transaction.
    ///
    /// Returns the page's raw rows unparsed -- callers run them through
    /// [`parse_transaction`] themselves, the same parser
    /// [`RpcClient::transaction`] uses, so the two paths never carry two
    /// copies of the parsing -- alongside the response's own
    /// `paginationToken` for the next call, or `None` when the node did not
    /// send one (the walk has nothing further to page through).
    ///
    /// Helius-only, like [`RpcClient::signatures_oldest_first`]: every other
    /// RPC this repo has tried answers `getTransactionsForAddress` with a
    /// JSON-RPC "method not found" error, which surfaces as `Err` here, never
    /// a false empty page -- a caller on a non-Helius endpoint sees an `Err`
    /// on its first call and can fall back rather than mistaking "unsupported"
    /// for "no history" (AGENTS.md rule 8).
    ///
    /// Returns `Ok(None)` when the page budget is spent before the call is
    /// made, the same convention [`RpcClient::signatures_page`] uses. Costs
    /// one page and one call against the [`Budget`], like any other read --
    /// no separate accounting for its larger response.
    ///
    /// # Errors
    ///
    /// [`RpcError`] on transport, node (including method-not-found) or shape
    /// failures.
    pub fn funding_transactions_page(
        &self,
        budget: &mut Budget,
        address: &Address,
        at_or_before_slot: u64,
        pagination_token: Option<&str>,
    ) -> Result<Option<FundingTransactionsPage>, RpcError> {
        if budget.take_page().is_err() {
            return Ok(None);
        }
        let mut options = serde_json::json!({
            "transactionDetails": "full",
            "sortOrder": "desc",
            "limit": FULL_TRANSACTIONS_PAGE_SIZE,
            "encoding": "json",
            // 1, not 0, for the same reason as `transaction`: with 0 Helius
            // refuses the whole page (-32015) the moment one row is a
            // version-1 transaction, and a quarter of one buyer's first
            // hundred were (VPS, 2026-09-23, research 0056).
            "maxSupportedTransactionVersion": 1,
            "filters": { "status": "succeeded", "slot": { "lte": at_or_before_slot } },
        });
        if let Some(token) = pagination_token {
            options["paginationToken"] = serde_json::json!(token);
        }
        let result: FullTransactionsEnvelope = self.call(
            budget,
            "getTransactionsForAddress",
            &serde_json::json!([address.to_string(), options]),
        )?;
        Ok(Some((result.data, result.pagination_token)))
    }

    /// Reads one transaction.
    ///
    /// # Errors
    ///
    /// [`RpcError`] on transport, node or shape failures, or when the budget is
    /// spent.
    pub fn transaction(
        &self,
        budget: &mut Budget,
        signature: &str,
    ) -> Result<Option<Transaction>, RpcError> {
        let raw: serde_json::Value = self.call(
            budget,
            "getTransaction",
            &serde_json::json!([
                signature,
                // 1, not 0: the node refuses a newer transaction outright
                // rather than returning it in an older shape, and version 1
                // transactions are already live (7 of 85 on one pump.fun
                // token, research 0056). Version 1 carries the same
                // `accountKeys` and `loadedAddresses` the parser reads.
                { "encoding": "json", "maxSupportedTransactionVersion": 1 }
            ]),
        )?;
        Ok(parse_transaction(&raw))
    }

    /// The token accounts holding the most of a mint, as the node ranks them.
    ///
    /// Solana caps this at 20 accounts. **This is a sample, not every
    /// holder**, and it names token accounts, not owners -- resolving who
    /// actually holds each balance needs [`RpcClient::accounts`] on the
    /// addresses this returns.
    ///
    /// # Errors
    ///
    /// [`RpcError`] on transport, node or shape failures, or when the budget
    /// is spent. An address or amount the node's own list cannot parse is
    /// [`Malformed`](RpcError::Malformed).
    pub fn token_largest_accounts(
        &self,
        budget: &mut Budget,
        mint: &Address,
    ) -> Result<Vec<LargestTokenAccount>, RpcError> {
        let result: LargestAccountsEnvelope = self.call(
            budget,
            "getTokenLargestAccounts",
            &serde_json::json!([mint.to_string()]),
        )?;
        let values = result.value.ok_or_else(|| {
            RpcError::Malformed("getTokenLargestAccounts returned no list".to_owned())
        })?;
        values
            .into_iter()
            .map(|v| {
                let address = v.address.parse().map_err(|_| {
                    RpcError::Malformed(format!("{} is not a base58 address", v.address))
                })?;
                let amount = v.amount.parse().map_err(|_| {
                    RpcError::Malformed(format!("{} is not an integer amount", v.amount))
                })?;
                Ok(LargestTokenAccount { address, amount })
            })
            .collect()
    }

    /// Every one of `owner`'s token accounts for `mint` (`getTokenAccountsByOwner`,
    /// filtered by mint).
    ///
    /// Exists so a caller that derived a single associated token account can
    /// still tell whether that address is the *only* account `owner` holds
    /// for this mint -- a second one (a temporary account, a pre-ATA-era
    /// account, or one made by a different wallet app) would carry balance
    /// history this reader never looks at, and a cash-flow total that
    /// ignored it would be wrong in a way nothing else here can catch.
    ///
    /// # Errors
    ///
    /// [`RpcError`] on transport, node or shape failures, or when the budget
    /// is spent.
    pub fn token_accounts_by_owner_for_mint(
        &self,
        budget: &mut Budget,
        owner: &Address,
        mint: &Address,
    ) -> Result<Vec<String>, RpcError> {
        let result: TokenAccountsEnvelope = self.call(
            budget,
            "getTokenAccountsByOwner",
            &serde_json::json!([
                owner.to_string(),
                { "mint": mint.to_string() },
                { "encoding": "base64" }
            ]),
        )?;
        let values = result.value.ok_or_else(|| {
            RpcError::Malformed("getTokenAccountsByOwner returned no list".to_owned())
        })?;
        Ok(values.into_iter().map(|v| v.pubkey).collect())
    }

    /// The mint's total supply, read from the chain rather than assumed from
    /// the largest-accounts list, which is only a sample.
    ///
    /// # Errors
    ///
    /// [`RpcError`] on transport, node or shape failures, or when the budget
    /// is spent.
    pub fn token_supply(
        &self,
        budget: &mut Budget,
        mint: &Address,
    ) -> Result<TokenSupply, RpcError> {
        let result: TokenSupplyEnvelope = self.call(
            budget,
            "getTokenSupply",
            &serde_json::json!([mint.to_string()]),
        )?;
        let value = result
            .value
            .ok_or_else(|| RpcError::Malformed("getTokenSupply returned no value".to_owned()))?;
        let amount = value.amount.parse().map_err(|_| {
            RpcError::Malformed(format!("{} is not an integer amount", value.amount))
        })?;
        Ok(TokenSupply {
            amount,
            decimals: value.decimals,
        })
    }
}

/// How many signatures one page asks for.
///
/// `pub(crate)` so [`crate::wallets`]'s own backward-paging search can tell a
/// short page from a full one without duplicating this number.
pub(crate) const PAGE_SIZE: usize = 1000;

/// Whether a page short of the requested size means the history ended.
///
/// Extracted because `<` survived being turned into `==`, `>` and `<=`, and the
/// consequence of each is different and bad. `>` would treat every full page as
/// the last one and stop at the newest thousand signatures, so **the launch of
/// any active token would never be reached** and every dossier would report an
/// ordinary trade as a launch block. `<=` would page forever past the end.
///
/// `pub(crate)`, same reason as [`PAGE_SIZE`]: [`crate::wallets`] pages
/// `getSignaturesForAddress` too and needs the same test.
pub(crate) const fn is_last_page(returned: usize) -> bool {
    returned < PAGE_SIZE
}

/// How many whole transactions one [`RpcClient::funding_transactions_page`]
/// call asks Helius for. Fixed at Helius's own observed page size (research
/// 0056 addendum, 2026-09-23), not a tunable: the whole point of full mode is
/// reading a candidate's history in as few calls as its actual size allows,
/// and a caller wanting a short page can just stop reading rows early.
pub const FULL_TRANSACTIONS_PAGE_SIZE: usize = 100;

/// One page from [`RpcClient::funding_transactions_page`]: its raw,
/// unparsed rows, and the node's own cursor to the next page (`None` when
/// there is not one).
pub type FundingTransactionsPage = (Vec<serde_json::Value>, Option<String>);

/// Reads the subset of `getTransaction`'s JSON this crate needs.
///
/// Split out and `pub(crate)` so it can be tested against captured mainnet
/// responses without a network. The alternative — asserting the parse through a
/// live call — is a test whose failures are the endpoint's rather than the
/// code's.
#[must_use]
pub fn parse_transaction(raw: &serde_json::Value) -> Option<Transaction> {
    let slot = Slot(raw.get("slot")?.as_u64()?);
    let meta = raw.get("meta");
    let failed = meta
        .and_then(|m| m.get("err"))
        .is_some_and(|e| !e.is_null());

    let message = raw.get("transaction")?.get("message")?;
    let mut accounts: Vec<String> = message
        .get("accountKeys")?
        .as_array()?
        .iter()
        .filter_map(|k| {
            // Two shapes: a bare string, or an object with a `pubkey` when the
            // node was asked for parsed accounts. Both occur in the wild.
            k.as_str()
                .map(ToOwned::to_owned)
                .or_else(|| k.get("pubkey")?.as_str().map(ToOwned::to_owned))
        })
        .collect();
    // A versioned transaction names some accounts through a lookup table, and
    // the node returns those in `meta.loadedAddresses`, not in `accountKeys`.
    // Instruction indexes and the balance arrays count them after the static
    // keys, writable first, then read-only. Without them an index past the
    // static list resolves to nothing and the instruction drops out, so a
    // real trade reads as no trade (research 0056).
    if let Some(loaded) = meta.and_then(|m| m.get("loadedAddresses")) {
        for field in ["writable", "readonly"] {
            if let Some(list) = loaded.get(field).and_then(|l| l.as_array()) {
                accounts.extend(
                    list.iter()
                        .filter_map(|k| k.as_str().map(ToOwned::to_owned)),
                );
            }
        }
    }

    let mut instructions = Vec::new();
    if let Some(list) = message.get("instructions").and_then(|i| i.as_array()) {
        collect_instructions(list, &accounts, &mut instructions);
    }
    // Inner instructions carry the CPIs, which is where a dev buy usually lives.
    if let Some(inner) = meta
        .and_then(|m| m.get("innerInstructions"))
        .and_then(|i| i.as_array())
    {
        for group in inner {
            if let Some(list) = group.get("instructions").and_then(|i| i.as_array()) {
                collect_instructions(list, &accounts, &mut instructions);
            }
        }
    }

    Some(Transaction {
        slot,
        pre_token_balances: token_balances(meta, "preTokenBalances"),
        post_token_balances: token_balances(meta, "postTokenBalances"),
        pre_balances: lamport_balances(meta, "preBalances"),
        post_balances: lamport_balances(meta, "postBalances"),
        accounts,
        instructions,
        failed,
    })
}

/// Reads `meta.preBalances`/`meta.postBalances`: a plain array of lamport
/// counts, one per account key, in the same order as `accountKeys`.
///
/// A missing or malformed entry is dropped rather than defaulted to zero
/// (rule 9) -- a caller that indexes past what came back must treat that
/// account's balance as unread, not as empty.
fn lamport_balances(meta: Option<&serde_json::Value>, field: &str) -> Vec<u64> {
    let Some(list) = meta.and_then(|m| m.get(field)).and_then(|b| b.as_array()) else {
        return Vec::new();
    };
    list.iter().filter_map(serde_json::Value::as_u64).collect()
}

fn collect_instructions(
    list: &[serde_json::Value],
    accounts: &[String],
    out: &mut Vec<RawInstruction>,
) {
    for ix in list {
        let Some(program) = program_of(ix, accounts) else {
            continue;
        };
        let data = ix
            .get("data")
            .and_then(|d| d.as_str())
            .and_then(decode_base58)
            .unwrap_or_default();
        let named = ix
            .get("accounts")
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|i| accounts.get(usize::try_from(i.as_u64()?).ok()?).cloned())
                    .collect()
            })
            .unwrap_or_default();
        out.push(RawInstruction {
            program,
            data,
            accounts: named,
        });
    }
}

fn program_of(ix: &serde_json::Value, accounts: &[String]) -> Option<String> {
    if let Some(id) = ix.get("programId").and_then(|p| p.as_str()) {
        return Some(id.to_owned());
    }
    let index = usize::try_from(ix.get("programIdIndex")?.as_u64()?).ok()?;
    accounts.get(index).cloned()
}

fn token_balances(meta: Option<&serde_json::Value>, field: &str) -> Vec<TokenBalance> {
    let Some(list) = meta.and_then(|m| m.get(field)).and_then(|b| b.as_array()) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry| {
            Some(TokenBalance {
                account_index: usize::try_from(entry.get("accountIndex")?.as_u64()?).ok()?,
                mint: entry.get("mint")?.as_str()?.to_owned(),
                // A balance the node reports as a string that will not parse is
                // absent, not zero. Rule 9: a zero here would read as "this
                // account received nothing", which is a different fact.
                amount: entry
                    .get("uiTokenAmount")?
                    .get("amount")?
                    .as_str()?
                    .parse()
                    .ok()?,
                owner: entry
                    .get("owner")
                    .and_then(|o| o.as_str())
                    .map(ToOwned::to_owned),
            })
        })
        .collect()
}

/// Decodes standard base64.
///
/// Hand-rolled for the same reason [`radar_sim::rpc::decode_base64`] is: this
/// and base58 are the only encodings the crate needs, and a dependency here
/// would land in the tree of anything that links the analyst.
#[must_use]
pub fn decode_base64(s: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;

    for byte in s.bytes() {
        if byte == b'=' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        let value = ALPHABET.iter().position(|c| *c == byte)?;
        buffer = (buffer << 6) | u32::try_from(value).ok()?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xFF).unwrap_or(0));
        }
    }
    Some(out)
}

/// Decodes base58, which is how `getTransaction` returns instruction data in
/// `json` encoding.
///
/// Refuses a character outside the alphabet rather than skipping it. A
/// truncated or shifted payload would hand the discriminator matcher eight
/// bytes that are not the ones the chain carried, and the failure would look
/// like an unrecognised instruction rather than a decoding bug.
#[must_use]
pub fn decode_base58(s: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    let mut bytes: Vec<u8> = Vec::with_capacity(s.len());
    for c in s.bytes() {
        let mut carry = ALPHABET.iter().position(|a| *a == c)?;
        for byte in &mut bytes {
            carry += usize::from(*byte) * 58;
            *byte = u8::try_from(carry & 0xFF).ok()?;
            carry >>= 8;
        }
        // Bounded by the width of the type: `carry` is a `usize` and each round
        // shifts it right by eight, so it reaches zero within `size_of` rounds.
        //
        // Written as a bound rather than as `while carry > 0` because that form
        // mutated to `while carry == 0` never ends -- it pushes a zero byte for
        // ever and grows the vector until the process dies. CI reported it as a
        // five-minute timeout, which is an expensive way to be told.
        for _ in 0..size_of::<usize>() {
            if carry == 0 {
                break;
            }
            bytes.push(u8::try_from(carry & 0xFF).ok()?);
            carry >>= 8;
        }
    }
    // Leading '1's are leading zero bytes, and they are significant: a 32-byte
    // key that lost one is a different key.
    let leading = s.bytes().take_while(|c| *c == b'1').count();
    bytes.extend(std::iter::repeat_n(0u8, leading));
    bytes.reverse();
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base58_decodes_the_shapes_solana_returns() {
        // The pump.fun program id, which is the value most likely to be checked
        // by hand against an explorer.
        let decoded = decode_base58("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P").expect("base58");
        assert_eq!(decoded.len(), 32);
        assert_eq!(decode_base58("1").as_deref(), Some(&[0u8][..]));
        assert_eq!(decode_base58("2").as_deref(), Some(&[1u8][..]));
        assert_eq!(decode_base58("").as_deref(), Some(&[][..]));
    }

    #[test]
    fn base58_preserves_leading_zero_bytes() {
        // A key that lost a leading zero is a different key, and it would still
        // be 31 bytes of plausible-looking address.
        assert_eq!(decode_base58("11").as_deref(), Some(&[0u8, 0][..]));
        assert_eq!(decode_base58("112").as_deref(), Some(&[0u8, 0, 1][..]));
    }

    #[test]
    fn base58_refuses_characters_outside_the_alphabet() {
        // '0', 'O', 'I' and 'l' are excluded from base58 precisely because they
        // are confusable, so accepting them would defeat the point.
        assert_eq!(decode_base58("0"), None);
        assert_eq!(decode_base58("O"), None);
        assert_eq!(decode_base58("I"), None);
        assert_eq!(decode_base58("l"), None);
        assert_eq!(decode_base58("abc!"), None);
    }

    /// A transport that answers each call from a queue.
    ///
    /// Ordered rather than keyed on method, because the *sequence* is part of
    /// what these tests check: a paging loop that asked twice when it should
    /// have asked once is a bug worth catching.
    struct Canned(std::sync::Mutex<Vec<String>>);

    impl Canned {
        fn boxed(responses: &[&str]) -> Box<dyn Transport> {
            Box::new(Self(std::sync::Mutex::new(
                responses.iter().rev().map(|s| (*s).to_owned()).collect(),
            )))
        }
    }

    impl Transport for Canned {
        fn post(&self, _: &str, _: String) -> Result<String, String> {
            self.0
                .lock()
                .map_err(|_| "poisoned".to_owned())?
                .pop()
                .ok_or_else(|| "the client asked for more than the test supplied".to_owned())
        }
    }

    fn client(responses: &[&str]) -> RpcClient {
        RpcClient::with_transport("http://test.invalid", Canned::boxed(responses))
    }

    fn budget() -> Budget {
        Budget::new(60, 3, Duration::from_secs(30))
    }

    /// A transport that pops `Ok`/`Err` results from a queue, unlike
    /// [`Canned`] which only ever answers `Ok`. Needed to drive a rate-limit
    /// retry: the sequence under test is an `Err("...http status: 429...")`
    /// followed by an `Ok`, or a non-429 `Err`, or 429 forever.
    struct Scripted(std::sync::Mutex<Vec<Result<String, String>>>);

    impl Scripted {
        fn boxed(responses: Vec<Result<String, String>>) -> Box<dyn Transport> {
            let mut responses = responses;
            responses.reverse();
            Box::new(Self(std::sync::Mutex::new(responses)))
        }
    }

    impl Transport for Scripted {
        fn post(&self, _: &str, _: String) -> Result<String, String> {
            let popped = self.0.lock().map_err(|_| "poisoned".to_owned())?.pop();
            popped.unwrap_or_else(|| {
                Err("the client asked for more than the test supplied".to_owned())
            })
        }
    }

    /// Keeps every request body it is sent, and answers each with `answer`.
    struct Recorder(std::sync::Arc<std::sync::Mutex<Vec<String>>>, &'static str);

    impl Transport for Recorder {
        fn post(&self, _: &str, body: String) -> Result<String, String> {
            self.0.lock().map_err(|_| "poisoned".to_owned())?.push(body);
            Ok(self.1.to_owned())
        }
    }

    #[test]
    fn a_full_mode_page_accepts_version_one_transactions() {
        // With 0 here Helius refused a buyer's whole page (-32015) because a
        // quarter of its rows were version 1, and every later candidate
        // fell back to the ten-transaction path (VPS, 2026-09-23).
        let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let client = RpcClient::with_transport(
            "http://test.invalid",
            Box::new(Recorder(
                std::sync::Arc::clone(&sent),
                r#"{"jsonrpc":"2.0","id":1,"result":{"data":[],"paginationToken":null}}"#,
            )),
        );
        let mut b = budget();
        client
            .funding_transactions_page(&mut b, &Address::new([1u8; 32]), 5, None)
            .expect("a page");
        let sent = sent.lock().expect("the record");
        assert_eq!(sent.len(), 1);
        assert!(
            sent[0].contains(r#""maxSupportedTransactionVersion":1"#),
            "{}",
            sent[0]
        );
    }

    fn rate_limited() -> Result<String, String> {
        Err("rpc transport: http status: 429".to_owned())
    }

    const PONG: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":1},"value":null}}"#;

    /// Reads an account through a scripted transport, which is enough to
    /// exercise `post_with_retry` without adding a second call surface just
    /// for these tests.
    fn read_through(c: &RpcClient, budget: &mut Budget) -> Result<Option<AccountRead>, RpcError> {
        c.account(budget, &Address::new([1u8; 32]))
    }

    #[test]
    fn a_rate_limited_call_is_retried_and_the_retry_can_succeed() {
        // Pins both the retry and the pause: removing the retry (re-applying
        // the bug) makes this fail on the first `expect`, since a single 429
        // would be returned as-is rather than followed by the queued success.
        let c = RpcClient::with_transport(
            "http://test.invalid",
            Scripted::boxed(vec![rate_limited(), Ok(PONG.to_owned())]),
        );
        let mut b = budget();
        let started = std::time::Instant::now();
        read_through(&c, &mut b).expect("the retry succeeds");
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "the retry waits for the fixed pause: {:?}",
            started.elapsed()
        );
        assert_eq!(b.calls_made(), 2, "the retry spends a second call");
    }

    #[test]
    fn a_non_rate_limit_transport_error_is_not_retried() {
        // Pins the 429 guard: if it were mutated to always retry, this would
        // consume the queued success too and calls_made() would read 2.
        let c = RpcClient::with_transport(
            "http://test.invalid",
            Scripted::boxed(vec![
                Err("connection refused".to_owned()),
                Ok(PONG.to_owned()),
            ]),
        );
        let mut b = budget();
        let err = read_through(&c, &mut b).expect_err("a non-429 transport error is not retried");
        assert!(
            matches!(&err, RpcError::Transport(m) if m == "connection refused"),
            "{err}"
        );
        assert_eq!(b.calls_made(), 1);
    }

    #[test]
    fn a_rate_limit_that_never_clears_is_returned_as_the_429_it_is() {
        const RETRIES: u32 = 3;
        let c = RpcClient::with_transport(
            "http://test.invalid",
            Scripted::boxed(vec![rate_limited(); (RETRIES + 1) as usize]),
        );
        let mut b = budget();
        let err = read_through(&c, &mut b).expect_err("still refused after every retry");
        assert!(
            err.to_string().contains("http status: 429"),
            "the gap wording is unchanged: {err}"
        );
        assert_eq!(b.calls_made(), 1 + RETRIES);
    }

    #[test]
    fn a_spent_budget_stops_the_retry_loop_instead_of_outlasting_it() {
        // AGENTS.md rule 7: a retry must never outlive the budget. With a
        // budget of 2 calls and a node that always answers 429, the third
        // attempt is refused by the budget, not sent.
        let c = RpcClient::with_transport(
            "http://test.invalid",
            Scripted::boxed(vec![rate_limited(); 4]),
        );
        let mut b = Budget::new(2, 3, Duration::from_secs(30));
        let err = read_through(&c, &mut b).expect_err("the budget runs out first");
        assert!(matches!(&err, RpcError::Stopped(Exhausted::Calls)), "{err}");
        assert_eq!(b.calls_made(), 2);
    }

    #[test]
    fn the_http_transport_sends_the_body_and_returns_what_came_back() {
        // The one genuinely impure line in the crate, tested against a
        // listener on loopback rather than excluded. No external network: the
        // port is chosen by the OS and the server is this test.
        //
        // Mutating `Http::post` to return a constant survived until this
        // existed, and the consequence is a client that reports the same answer
        // for every token.
        use std::io::{Read as _, Write as _};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
        let addr = listener.local_addr().expect("an address");
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("a connection");
            // Read until the request body has arrived. A single `read` can
            // return just the headers -- the client is free to split them
            // across segments, and a test that assumed otherwise would fail
            // occasionally rather than never, which is the worst kind.
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            loop {
                let read = socket.read(&mut buffer).expect("a request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                let text = String::from_utf8_lossy(&request);
                if let Some((head, rest)) = text.split_once("\r\n\r\n") {
                    let length: usize = head
                        .lines()
                        .find_map(|l| {
                            l.strip_prefix("Content-Length: ")
                                .or_else(|| l.strip_prefix("content-length: "))
                        })
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0);
                    if rest.len() >= length {
                        break;
                    }
                }
            }

            let body = r#"{"jsonrpc":"2.0","id":1,"result":"pong"}"#;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .expect("a response");
            socket.flush().expect("a flush");
            // Half-close so the client sees a clean end of stream rather than a
            // reset, which is what the abrupt drop produced on Windows.
            let _ = socket.shutdown(std::net::Shutdown::Write);
            String::from_utf8_lossy(&request).to_string()
        });

        let client = RpcClient::new(format!("http://{addr}"));
        let answer = client
            .transport
            .post(client.endpoint(), r#"{"ping":true}"#.to_owned())
            .expect("a round trip");

        let request = server.join().expect("the server thread");
        assert!(
            request.contains(r#"{"ping":true}"#),
            "the body is sent: {request}"
        );
        assert!(answer.contains("pong"), "the response comes back: {answer}");
    }

    #[test]
    fn debug_names_the_endpoint_so_an_operator_can_see_what_is_running() {
        // Mutating the `Debug` impl to `Ok(())` survived. It is small, but the
        // whole reason `endpoint` is exposed is that "which node are we
        // actually talking to" is a question worth being able to answer.
        let shown = format!("{:?}", RpcClient::new("http://somewhere.invalid"));
        assert!(shown.contains("http://somewhere.invalid"), "{shown}");
    }

    #[test]
    fn an_account_is_decoded_from_what_the_node_returned() {
        // `account` -> Ok(None) / Ok(Some(vec![])) all survived, because
        // nothing without an endpoint could see the difference. "QUJD" is
        // base64 for "ABC".
        let c = client(&[
            r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":42},"value":{"data":["QUJD","base64"]}}}"#,
        ]);
        let got = c
            .account(&mut budget(), &Address::new([1u8; 32]))
            .expect("a read")
            .expect("an account");
        assert_eq!(got.data, b"ABC".to_vec());
        // The slot travels with the bytes. For a graduated coin this is the
        // only slot the dossier gets -- its launch block is past the page
        // budget -- so dropping it here publishes numbers nobody can check.
        assert_eq!(got.slot, Some(realorrug_types::Slot(42)));
    }

    #[test]
    fn a_node_that_omits_the_context_gives_no_slot_rather_than_a_wrong_one() {
        // Rule 9 at the transport. Defaulting to slot 0 would be worse than
        // having none: a checkable-looking number that checks out as nothing.
        let c =
            client(&[r#"{"jsonrpc":"2.0","id":1,"result":{"value":{"data":["QUJD","base64"]}}}"#]);
        let got = c
            .account(&mut budget(), &Address::new([1u8; 32]))
            .expect("a read")
            .expect("an account");
        assert_eq!(got.slot, None);
    }

    #[test]
    fn no_account_is_none_and_is_not_an_empty_account() {
        // The distinction that stops a typo'd mint producing a dossier full of
        // defaults: an address with no account is not a token whose properties
        // are unknown, it is not a token.
        let c = client(&[r#"{"jsonrpc":"2.0","id":1,"result":{"value":null}}"#]);
        let got = c
            .account(&mut budget(), &Address::new([1u8; 32]))
            .expect("a read");
        assert!(got.is_none());
    }

    #[test]
    fn a_multi_account_read_carries_one_slot_for_every_account_in_it() {
        // The property the whole reserves path rests on: one call, one
        // `context.slot`, and no per-account slot to be tempted into mixing.
        // "QUJD" is base64 for "ABC" and "REVG" for "DEF".
        let c = client(&[concat!(
            r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":9},"value":["#,
            r#"{"data":["QUJD","base64"],"owner":"11111111111111111111111111111111"},"#,
            r#"null,"#,
            r#"{"data":["REVG","base64"],"owner":"11111111111111111111111111111111"}]}}"#,
        )]);
        let got = c
            .accounts(
                &mut budget(),
                &[
                    Address::new([1u8; 32]),
                    Address::new([2u8; 32]),
                    Address::new([3u8; 32]),
                ],
            )
            .expect("a read");

        assert_eq!(got.slot, Some(Slot(9)));
        assert_eq!(got.accounts.len(), 3);
        assert_eq!(
            got.accounts[0].as_ref().expect("an account").data,
            b"ABC".to_vec(),
        );
        // A hole stays a hole and keeps its place, because the caller maps the
        // list back onto the addresses by position.
        assert!(got.accounts[1].is_none());
        assert_eq!(
            got.accounts[2].as_ref().expect("an account").data,
            b"DEF".to_vec(),
        );
    }

    #[test]
    fn a_multi_account_read_of_the_wrong_length_is_refused() {
        // Two accounts against three addresses would shift every account onto
        // the wrong address, and every field of the result would look ordinary.
        // Truncating to what came back would be the same bug with fewer rows.
        let c = client(&[concat!(
            r#"{"jsonrpc":"2.0","id":1,"result":{"context":{"slot":9},"value":["#,
            r#"{"data":["QUJD","base64"],"owner":"11111111111111111111111111111111"},null]}}"#,
        )]);
        let refused = c
            .accounts(
                &mut budget(),
                &[
                    Address::new([1u8; 32]),
                    Address::new([2u8; 32]),
                    Address::new([3u8; 32]),
                ],
            )
            .expect_err("three were asked for");
        assert!(
            matches!(&refused, RpcError::Malformed(why) if why.contains("asked for 3")),
            "{refused:?}",
        );
    }

    #[test]
    fn a_node_error_is_an_error_rather_than_an_absent_account() {
        // These two must not collapse. "The node is broken" and "this token
        // does not exist" lead to different replies, and only one is about the
        // token.
        let c = client(&[r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"boom"}}"#]);
        let err = c
            .account(&mut budget(), &Address::new([1u8; 32]))
            .expect_err("a node error");
        assert!(
            matches!(err, RpcError::Node(ref m) if m == "boom"),
            "{err:?}"
        );
    }

    #[test]
    fn a_short_first_page_ends_the_walk_in_one_call() {
        // `signatures_back_to_oldest` -> Ok((vec![], _)) survived. The queue
        // holds ONE response, so a loop that paged twice would fail with "the
        // client asked for more than the test supplied".
        let c = client(&[r#"{"jsonrpc":"2.0","id":1,"result":[
            {"signature":"newest","slot":200,"err":null},
            {"signature":"oldest","slot":100,"err":null}
        ]}"#]);
        let (sigs, truncated) = c
            .signatures_back_to_oldest(&mut budget(), &Address::new([1u8; 32]))
            .expect("a walk");
        assert_eq!(sigs.len(), 2);
        assert_eq!(sigs[1].signature, "oldest");
        assert!(!truncated, "a short page means the history ended");
    }

    #[test]
    fn the_page_budget_truncates_rather_than_failing() {
        // The property that turns a bound into an "at least" rather than an
        // error: a token with more history than the budget allows still yields
        // what was read, flagged.
        let full: String = format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":[{}]}}"#,
            (0..1000)
                .map(|i| format!(r#"{{"signature":"s{i}","slot":100,"err":null}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        let c =
            RpcClient::with_transport("http://test.invalid", Canned::boxed(&[&full, &full, &full]));
        let mut b = Budget::new(60, 2, Duration::from_secs(30));
        let (sigs, truncated) = c
            .signatures_back_to_oldest(&mut b, &Address::new([1u8; 32]))
            .expect("a walk");
        assert_eq!(sigs.len(), 2000, "two pages of allowance, two pages read");
        assert!(truncated, "stopping at the page bound must be reported");
    }

    #[test]
    fn signatures_oldest_first_parses_the_documented_helius_response_shape() {
        // The exact shape observed on the VPS against the production Helius
        // endpoint, 2026-09-23 (task 9-23-0011): a "data" array of signature
        // entries carrying fields this crate does not read
        // (transactionIndex, memo, blockTime, confirmationStatus) alongside
        // the ones it does, plus a `paginationToken` this call never follows.
        let c = client(&[concat!(
            r#"{"jsonrpc":"2.0","id":1,"result":{"data":[{"signature":"4wi3",""#,
            r#"slot":448485736,"transactionIndex":975,"err":null,"memo":null,""#,
            r#"blockTime":1789841756,"confirmationStatus":"finalized"}],""#,
            r#"paginationToken":"448485750:24"}}"#,
        )]);
        let sigs = c
            .signatures_oldest_first(&mut budget(), &Address::new([1u8; 32]), 3)
            .expect("a call")
            .expect("the page budget was not spent");
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].signature, "4wi3");
        assert_eq!(sigs[0].slot, 448_485_736);
        assert!(sigs[0].err.is_none());
    }

    #[test]
    fn signatures_oldest_first_reports_method_not_found_as_an_error() {
        // Helius-only: every other RPC this repo has tried refuses
        // `getTransactionsForAddress`. That must come back `Err`, never a
        // false "complete" empty list (AGENTS.md rule 8).
        let c = client(&[
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}"#,
        ]);
        let err = c
            .signatures_oldest_first(&mut budget(), &Address::new([1u8; 32]), 3)
            .expect_err("a non-Helius endpoint refuses the method");
        assert!(
            matches!(err, RpcError::Node(ref m) if m.contains("not found")),
            "{err:?}"
        );
    }

    #[test]
    fn a_transaction_is_read_through_the_transport() {
        // `transaction` -> Ok(None) survived.
        let c = client(&[r#"{"jsonrpc":"2.0","id":1,"result":{
            "slot": 441040080,
            "meta": {"err": null},
            "transaction": {"message": {"accountKeys": ["A"], "instructions": []}}
        }}"#]);
        let tx = c
            .transaction(&mut budget(), "sig")
            .expect("a read")
            .expect("a transaction");
        assert_eq!(tx.slot, Slot(441_040_080));
        assert_eq!(tx.accounts, vec!["A".to_owned()]);
    }

    #[test]
    fn only_a_short_page_means_the_history_ended() {
        // Each mutant here has a different and bad consequence. `>` would treat
        // every full page as the last, so the launch of any active token would
        // never be reached and an ordinary trade would be read as its launch
        // block. `<=` would page past the end forever.
        assert!(is_last_page(0));
        assert!(is_last_page(999));
        assert!(!is_last_page(1000));
    }

    #[test]
    fn the_endpoint_comes_from_the_environment_when_one_is_set() {
        // `from_env` -> Default::default() survived, and the consequence is a
        // deployment pointed at a paid archival node silently using the free
        // public one -- which rate-limits, and cannot reach the history a
        // launch lookup needs.
        //
        // Through an injected lookup rather than the real environment: this
        // crate forbids `unsafe`, `set_var` is `unsafe` in edition 2024, and a
        // test mutating global state would race every other test in the binary.
        let set = |_: &str| Some("https://example.invalid/rpc".to_owned());
        let unset = |_: &str| None;

        assert_eq!(
            RpcClient::from_vars(&set).endpoint(),
            "https://example.invalid/rpc"
        );
        assert_eq!(RpcClient::from_vars(&unset).endpoint(), DEFAULT_RPC);
    }

    #[test]
    fn base64_decodes_the_shapes_solana_returns() {
        assert_eq!(decode_base64("QUJD").as_deref(), Some(&b"ABC"[..]));
        assert_eq!(decode_base64("QUI=").as_deref(), Some(&b"AB"[..]));
        assert_eq!(decode_base64("QU!D"), None);
    }

    #[test]
    fn a_transaction_parses_inner_instructions_as_well_as_outer() {
        // The dev buy is usually a CPI. A parser that read only the outer list
        // would report a launch with no dev buy, which is an absence rendered
        // as a zero -- rule 9's exact shape.
        let raw = serde_json::json!({
            "slot": 441_040_080u64,
            "meta": {
                "err": null,
                "innerInstructions": [
                    { "instructions": [ { "programIdIndex": 1, "data": "2", "accounts": [0] } ] }
                ],
                "preTokenBalances": [],
                "postTokenBalances": [
                    { "accountIndex": 0, "mint": "Mint", "owner": "Owner",
                      "uiTokenAmount": { "amount": "42" } }
                ]
            },
            "transaction": { "message": {
                "accountKeys": ["AccountOne", "ProgramTwo"],
                "instructions": [ { "programIdIndex": 1, "data": "3", "accounts": [0] } ]
            }}
        });
        let tx = parse_transaction(&raw).expect("a transaction");
        assert_eq!(tx.slot, Slot(441_040_080));
        assert!(!tx.failed);
        assert_eq!(tx.instructions.len(), 2, "outer and inner, not just outer");
        assert_eq!(tx.instructions[0].program, "ProgramTwo");
        assert_eq!(tx.instructions[0].data, vec![2]);
        assert_eq!(tx.instructions[0].accounts, vec!["AccountOne".to_owned()]);
        assert_eq!(tx.instructions[1].data, vec![1]);
        assert_eq!(tx.post_token_balances[0].amount, 42);
        assert_eq!(tx.post_token_balances[0].owner.as_deref(), Some("Owner"));
    }

    #[test]
    fn a_failed_transaction_is_marked_failed() {
        // 0006: 35 of 97 migration instructions in one hour were in failed
        // transactions, and counting them overstated the label by a third.
        let raw = serde_json::json!({
            "slot": 1u64,
            "meta": { "err": { "InstructionError": [0, "Custom"] } },
            "transaction": { "message": { "accountKeys": [], "instructions": [] } }
        });
        assert!(parse_transaction(&raw).expect("a transaction").failed);
    }

    #[test]
    fn account_keys_parse_in_both_shapes_the_node_returns() {
        let raw = serde_json::json!({
            "slot": 1u64,
            "transaction": { "message": {
                "accountKeys": ["Bare", { "pubkey": "Wrapped" }],
                "instructions": []
            }}
        });
        let tx = parse_transaction(&raw).expect("a transaction");
        assert_eq!(tx.accounts, vec!["Bare".to_owned(), "Wrapped".to_owned()]);
    }

    #[test]
    fn lookup_table_accounts_follow_the_static_keys_writable_first() {
        // Trimmed from a mainnet version 0 transaction (research 0056). The
        // program sits in the lookup table, so its index (3) is past the two
        // static keys; a parser that ignored `loadedAddresses` dropped the
        // instruction and the trade with it.
        let raw = serde_json::json!({
            "slot": 1u64,
            "version": 0,
            "meta": {
                "err": null,
                "loadedAddresses": { "writable": ["LoadedW"], "readonly": ["LoadedR"] }
            },
            "transaction": { "message": {
                "accountKeys": ["Signer", "Static"],
                "addressTableLookups": [],
                "instructions": [ { "programIdIndex": 3, "data": "2", "accounts": [0, 2] } ]
            }}
        });
        let tx = parse_transaction(&raw).expect("a transaction");
        assert_eq!(tx.accounts, ["Signer", "Static", "LoadedW", "LoadedR"]);
        assert_eq!(
            tx.instructions.len(),
            1,
            "the lookup-table instruction survives"
        );
        assert_eq!(tx.instructions[0].program, "LoadedR");
        assert_eq!(tx.instructions[0].accounts, ["Signer", "LoadedW"]);
    }
}
