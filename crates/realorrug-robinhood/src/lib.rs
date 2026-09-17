// SPDX-License-Identifier: Apache-2.0
//! Robinhood Chain, read: receipts and the logs in them, typed at the edge.
//!
//! Design 0019 §4.4, plan 0001 step 6. The token lives on Robinhood Chain
//! (ADR 0023), and ADR 0013's constraint 1 -- no dev buy, the curve the only
//! recipient -- is a property of one transaction there. This crate reads that
//! transaction; [`pons`] says what it means, and [`escrow`] reads where the
//! creator's fees wait to be claimed.
//!
//! # Holds no key and signs nothing
//!
//! No key, no signing, no transaction building. [`escrow::claim_call`] encodes
//! a claim's call data, so the payout and its tests share one encoding checked
//! against mainnet. [`Rpc::call`] is public so that `realorrug-payout` can send
//! the transaction it built and had signed elsewhere, but nothing in this crate
//! can produce one to send. The payout that spends on this chain is a separate
//! crate, so this one can be depended on by anything that needs to read the
//! chain without that dependency reaching a key.
//!
//! # Why no Ethereum library
//!
//! A receipt is a JSON object of hex strings, and the logs Pons emits are
//! fixed-width words. Parsing that is a page of code, tested against mainnet
//! captures; a general EVM library would be tens of thousands of lines, most of
//! them for signing, which this crate must not do. Event topics and selectors
//! are constants, checked against the captured transactions that use them, so
//! no Keccak implementation is needed either.

pub mod escrow;
pub mod pons;

use std::fmt;
use std::str::FromStr;

/// Why a value read from the chain was not accepted.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReadError {
    /// A field the value needs is absent, or not a string.
    #[error("missing field: {0}")]
    Missing(&'static str),
    /// A hex string is malformed: no `0x`, an odd length, or a non-hex digit.
    #[error("not hex: {0}")]
    Hex(String),
    /// A fixed-width value has the wrong number of bytes.
    #[error("expected {expected} bytes, got {got}")]
    Width {
        /// What the type holds.
        expected: usize,
        /// What arrived.
        got: usize,
    },
    /// A quantity does not fit the integer it is read into.
    #[error("quantity out of range: {0}")]
    Range(String),
}

/// A 20-byte account address.
///
/// Parsed case-insensitively and printed lowercase. The mixed-case EIP-55
/// checksum is not verified: that needs Keccak, and every address here comes
/// from an RPC response or a constant, never from a person typing one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Address(pub [u8; 20]);

/// A 32-byte value: a transaction hash, a block hash, or a log topic.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash32(pub [u8; 32]);

impl Address {
    /// The zero address, which a mint transfers from.
    pub const ZERO: Self = Self([0; 20]);

    /// An address from a `0x` hex literal, at compile time.
    ///
    /// # Panics
    ///
    /// When the literal is not `0x` and 40 hex digits, which for a constant is
    /// a compile error.
    #[must_use]
    pub const fn from_hex(literal: &str) -> Self {
        Self(fixed(literal))
    }
}

impl Hash32 {
    /// A 32-byte value from a `0x` hex literal, at compile time.
    ///
    /// # Panics
    ///
    /// When the literal is not `0x` and 64 hex digits, which for a constant is
    /// a compile error.
    #[must_use]
    pub const fn from_hex(literal: &str) -> Self {
        Self(fixed(literal))
    }

    /// The address a topic carries, if its twelve high bytes are zero.
    ///
    /// An indexed `address` parameter is left-padded to 32 bytes. Anything
    /// else in those bytes means the topic is not an address, and reading its
    /// low 20 bytes anyway would name an account nobody named.
    #[must_use]
    pub fn address(&self) -> Option<Address> {
        word_address(&self.0)
    }
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("not a hex digit"),
    }
}

const fn fixed<const N: usize>(literal: &str) -> [u8; N] {
    let b = literal.as_bytes();
    assert!(
        b.len() == 2 + 2 * N && b[0] == b'0' && b[1] == b'x',
        "not a 0x literal of the right width"
    );
    let mut out = [0u8; N];
    let mut i = 0;
    while i < N {
        out[i] = nibble(b[2 + 2 * i]) * 16 + nibble(b[3 + 2 * i]);
        i += 1;
    }
    out
}

/// Bytes from a `0x` hex string of any even length.
///
/// # Errors
///
/// [`ReadError::Hex`] when the prefix is missing, the length is odd, or a digit
/// is not hex.
pub fn hex_bytes(text: &str) -> Result<Vec<u8>, ReadError> {
    let digits = text
        .strip_prefix("0x")
        .ok_or_else(|| ReadError::Hex(text.to_owned()))?;
    if digits.len() % 2 != 0 {
        return Err(ReadError::Hex(text.to_owned()));
    }
    (0..digits.len() / 2)
        .map(|i| {
            u8::from_str_radix(&digits[2 * i..2 * i + 2], 16)
                .map_err(|_| ReadError::Hex(text.to_owned()))
        })
        .collect()
}

fn hex_fixed<const N: usize>(text: &str) -> Result<[u8; N], ReadError> {
    let bytes = hex_bytes(text)?;
    <[u8; N]>::try_from(bytes.as_slice()).map_err(|_| ReadError::Width {
        expected: N,
        got: bytes.len(),
    })
}

/// A JSON-RPC quantity wide enough for wei: at most 128 bits.
///
/// # Errors
///
/// [`ReadError::Hex`] when it is not hex; [`ReadError::Range`] when it does not
/// fit in 128 bits. A balance that does not fit is refused, not truncated.
pub fn quantity_u128(text: &str) -> Result<u128, ReadError> {
    let digits = text
        .strip_prefix("0x")
        .filter(|d| !d.is_empty())
        .ok_or_else(|| ReadError::Hex(text.to_owned()))?;
    if !digits.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(ReadError::Hex(text.to_owned()));
    }
    u128::from_str_radix(digits, 16).map_err(|_| ReadError::Range(text.to_owned()))
}

/// Bytes as a `0x` hex string, as JSON-RPC takes them.
#[must_use]
pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(2 + 2 * bytes.len());
    out.push_str("0x");
    for b in bytes {
        // Writing to a String cannot fail.
        let _ = fmt::Write::write_fmt(&mut out, format_args!("{b:02x}"));
    }
    out
}

/// A JSON-RPC quantity: `0x`, then hex digits with no leading zeros required.
///
/// # Errors
///
/// [`ReadError::Hex`] when it is not hex; [`ReadError::Range`] when it does not
/// fit in 64 bits.
pub fn quantity(text: &str) -> Result<u64, ReadError> {
    let digits = text
        .strip_prefix("0x")
        .filter(|d| !d.is_empty())
        .ok_or_else(|| ReadError::Hex(text.to_owned()))?;
    if !digits.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(ReadError::Hex(text.to_owned()));
    }
    u64::from_str_radix(digits, 16).map_err(|_| ReadError::Range(text.to_owned()))
}

impl FromStr for Address {
    type Err = ReadError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        hex_fixed(text).map(Self)
    }
}

impl FromStr for Hash32 {
    type Err = ReadError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        hex_fixed(text).map(Self)
    }
}

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    f.write_str("0x")?;
    bytes.iter().try_for_each(|b| write!(f, "{b:02x}"))
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Display for Hash32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

impl fmt::Debug for Hash32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.0)
    }
}

/// The address in an ABI word, if its twelve high bytes are zero.
#[must_use]
pub fn word_address(word: &[u8; 32]) -> Option<Address> {
    let (high, low) = word.split_at(12);
    if high.iter().any(|&b| b != 0) {
        return None;
    }
    <[u8; 20]>::try_from(low).ok().map(Address)
}

/// An ABI `uint` word as `u128`, if its sixteen high bytes are zero.
///
/// Every amount Pons emits on a curve is wei or token base units, and a supply
/// of a billion tokens at 18 decimals is 10^27, far inside 2^128. A word that
/// does not fit is refused rather than truncated: a truncated amount is a wrong
/// amount that looks right.
#[must_use]
pub fn word_u128(word: &[u8; 32]) -> Option<u128> {
    let (high, low) = word.split_at(16);
    if high.iter().any(|&b| b != 0) {
        return None;
    }
    <[u8; 16]>::try_from(low).ok().map(u128::from_be_bytes)
}

/// The `index`th 32-byte word of ABI-encoded bytes.
#[must_use]
pub fn word(data: &[u8], index: usize) -> Option<&[u8; 32]> {
    let start = index.checked_mul(32)?;
    data.get(start..start.checked_add(32)?)?.try_into().ok()
}

/// One event log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Log {
    /// The contract that emitted it.
    pub address: Address,
    /// The event signature first, then the indexed parameters.
    pub topics: Vec<Hash32>,
    /// The non-indexed parameters, ABI-encoded.
    pub data: Vec<u8>,
    /// The block it is in.
    pub block: u64,
    /// The transaction that emitted it.
    pub transaction: Hash32,
}

fn field<'a>(value: &'a serde_json::Value, name: &'static str) -> Result<&'a str, ReadError> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .ok_or(ReadError::Missing(name))
}

impl Log {
    /// A log from its JSON-RPC object, as `eth_getLogs` and receipts carry it.
    ///
    /// # Errors
    ///
    /// A [`ReadError`] naming the field that is missing or malformed.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, ReadError> {
        let topics = value
            .get("topics")
            .and_then(serde_json::Value::as_array)
            .ok_or(ReadError::Missing("topics"))?
            .iter()
            .map(|t| {
                t.as_str()
                    .ok_or(ReadError::Missing("topics"))
                    .and_then(str::parse)
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            address: field(value, "address")?.parse()?,
            topics,
            data: hex_bytes(field(value, "data")?)?,
            block: quantity(field(value, "blockNumber")?)?,
            transaction: field(value, "transactionHash")?.parse()?,
        })
    }

    /// Whether this log is `event` emitted by `contract`.
    #[must_use]
    pub fn is(&self, contract: &Address, event: &Hash32) -> bool {
        self.address == *contract && self.topics.first() == Some(event)
    }

    /// The address in indexed parameter `index` (1-based, after the signature).
    #[must_use]
    pub fn topic_address(&self, index: usize) -> Option<Address> {
        self.topics.get(index).and_then(Hash32::address)
    }

    /// Non-indexed word `index` as `u128`.
    #[must_use]
    pub fn data_u128(&self, index: usize) -> Option<u128> {
        word(&self.data, index).and_then(word_u128)
    }
}

/// A transaction's receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    /// The transaction.
    pub transaction: Hash32,
    /// The block it landed in.
    pub block: u64,
    /// Whether it succeeded. A failed transaction's logs were reverted.
    pub succeeded: bool,
    /// The sender.
    pub from: Address,
    /// The contract or account called; absent for a deployment.
    pub to: Option<Address>,
    /// The logs, in order.
    pub logs: Vec<Log>,
}

impl Receipt {
    /// A receipt from its `eth_getTransactionReceipt` object.
    ///
    /// # Errors
    ///
    /// A [`ReadError`] naming the field that is missing or malformed. A status
    /// other than `0x1` or `0x0` is malformed, not a failure: unknown is not
    /// safe, and it is not a success either.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, ReadError> {
        let succeeded = match field(value, "status")? {
            "0x1" => true,
            "0x0" => false,
            other => return Err(ReadError::Hex(other.to_owned())),
        };
        let to = match value.get("to") {
            None | Some(serde_json::Value::Null) => None,
            Some(_) => Some(field(value, "to")?.parse()?),
        };
        let logs = value
            .get("logs")
            .and_then(serde_json::Value::as_array)
            .ok_or(ReadError::Missing("logs"))?
            .iter()
            .map(Log::from_json)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            transaction: field(value, "transactionHash")?.parse()?,
            block: quantity(field(value, "blockNumber")?)?,
            succeeded,
            from: field(value, "from")?.parse()?,
            to,
            logs,
        })
    }
}

/// A transaction as `eth_getTransactionByHash` returns it: the fields a payout
/// reads back after sending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    /// Its hash.
    pub hash: Hash32,
    /// The sender.
    pub from: Address,
    /// The recipient; absent for a deployment.
    pub to: Option<Address>,
    /// Wei sent.
    pub value: u128,
    /// Call data.
    pub input: Vec<u8>,
    /// The sender's nonce.
    pub nonce: u64,
    /// The block it is in, or `None` while it is pending.
    pub block: Option<u64>,
}

impl Transaction {
    /// A transaction from its JSON-RPC object.
    ///
    /// # Errors
    ///
    /// A [`ReadError`] naming the field that is missing or malformed.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, ReadError> {
        let optional = |name: &'static str| match value.get(name) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(_) => field(value, name).map(Some),
        };
        Ok(Self {
            hash: field(value, "hash")?.parse()?,
            from: field(value, "from")?.parse()?,
            to: optional("to")?.map(str::parse).transpose()?,
            value: quantity_u128(field(value, "value")?)?,
            input: hex_bytes(field(value, "input")?)?,
            nonce: quantity(field(value, "nonce")?)?,
            block: optional("blockNumber")?.map(quantity).transpose()?,
        })
    }
}

/// Which state a nonce is read at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    /// The latest block: transactions that have landed.
    Latest,
    /// The node's pending state: landed, plus what it holds to send.
    Pending,
}

impl Tag {
    const fn name(self) -> &'static str {
        match self {
            Self::Latest => "latest",
            Self::Pending => "pending",
        }
    }
}

/// Robinhood Chain over JSON-RPC.
///
/// The endpoint is always configured, never defaulted (AGENTS.md rule 7): the
/// public endpoint is "rate-limited and not recommended for production use"
/// (research 0035 §1), and choosing it silently would be choosing for the
/// operator.
///
/// Several endpoints may be configured, comma-separated, and each call tries
/// them in order until one answers. The operator runs two free plans (Alchemy
/// and QuickNode, 2026-09-17), and one alone hitting its rate limit on a busy
/// day would turn every reply into "could not be read". Parsing the list here,
/// rather than adding a second variable, means every caller that already reads
/// `REALORRUG_ROBINHOOD_RPC` -- the analyst, the checker, the payout -- gets
/// the fallback without its own configuration code.
#[derive(Clone, Debug)]
pub struct Rpc {
    endpoints: Vec<String>,
}

impl Rpc {
    /// A client for one endpoint, or several separated by commas, tried in
    /// the order written.
    #[must_use]
    pub fn new(endpoint: impl Into<String>) -> Self {
        let text = endpoint.into();
        let endpoints = text
            .split(',')
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(str::to_owned)
            .collect();
        Self { endpoints }
    }

    /// One JSON-RPC call, returning its `result`.
    ///
    /// Any failure moves on to the next endpoint, including an `error` the
    /// endpoint answered with: a rate limit arrives as either a transport
    /// error or a JSON-RPC error depending on the provider, and telling them
    /// apart per provider is more code than the one repeated call a genuine
    /// error costs. Only when every endpoint fails is the call an error, so a
    /// read is never guessed (rule 8).
    ///
    /// # Errors
    ///
    /// The last endpoint's failure: the transport's error, a body that is not
    /// JSON, the endpoint's `error` with the method named, or an answer with
    /// no `result`. No endpoint configured at all is an error too.
    pub fn call(
        &self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let mut last = Err(format!("{method}: no endpoint configured"));
        for endpoint in &self.endpoints {
            last = Self::call_one(endpoint, method, params);
            if last.is_ok() {
                break;
            }
        }
        last
    }

    fn call_one(
        endpoint: &str,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let body =
            serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        let mut response = ureq::post(endpoint)
            .content_type("application/json")
            .send(body.to_string())
            .map_err(|e| e.to_string())?;
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("not json: {e}"))?;
        if let Some(err) = value.get("error") {
            return Err(format!("{method}: {err}"));
        }
        value
            .get("result")
            .cloned()
            .ok_or_else(|| format!("{method} returned no result"))
    }

    /// A transaction's receipt, or `None` while it is not yet in a block.
    ///
    /// # Errors
    ///
    /// The endpoint's error, or why the receipt did not parse.
    pub fn receipt(&self, transaction: &Hash32) -> Result<Option<Receipt>, String> {
        let result = self.call(
            "eth_getTransactionReceipt",
            &serde_json::json!([transaction.to_string()]),
        )?;
        if result.is_null() {
            return Ok(None);
        }
        Receipt::from_json(&result)
            .map(Some)
            .map_err(|e| format!("receipt: {e}"))
    }

    /// Every log `address` emitted whose leading topics match `topics`, over
    /// the whole chain.
    ///
    /// One call, block 0 to the latest. The provider caps the size of the
    /// answer rather than the range (Alchemy: 10,000 logs, measured
    /// 2026-09-17 on this chain), so one token's own launch event, or its own
    /// transfers until it is busy, fit in a single answer. A capped answer is
    /// the endpoint's error and reaches the caller as one -- never as a short
    /// list that reads as complete.
    ///
    /// # Errors
    ///
    /// The endpoint's error, or a log that did not parse.
    pub fn logs(&self, address: &Address, topics: &[Hash32]) -> Result<Vec<Log>, String> {
        let topics: Vec<String> = topics.iter().map(ToString::to_string).collect();
        let result = self.call(
            "eth_getLogs",
            &serde_json::json!([{
                "address": address.to_string(),
                "fromBlock": "0x0",
                "toBlock": "latest",
                "topics": topics,
            }]),
        )?;
        result
            .as_array()
            .ok_or("eth_getLogs returned no list")?
            .iter()
            .map(|log| Log::from_json(log).map_err(|e| format!("log: {e}")))
            .collect()
    }

    /// A block's number and its timestamp in seconds: block `number`, or the
    /// latest block when `number` is `None`.
    ///
    /// # Errors
    ///
    /// The endpoint's error, no such block, or a field that is not a quantity.
    pub fn block_time(&self, number: Option<u64>) -> Result<(u64, u64), String> {
        let tag = number.map_or_else(|| "latest".to_owned(), |n| format!("{n:#x}"));
        let result = self.call("eth_getBlockByNumber", &serde_json::json!([tag, false]))?;
        if result.is_null() {
            return Err("eth_getBlockByNumber: no such block".to_owned());
        }
        let read = |name: &'static str| {
            field(&result, name)
                .and_then(quantity)
                .map_err(|e| format!("eth_getBlockByNumber: {e}"))
        };
        Ok((read("number")?, read("timestamp")?))
    }

    /// A read-only contract call at the latest block.
    ///
    /// # Errors
    ///
    /// The endpoint's error, or a result that is not hex.
    pub fn call_contract(&self, to: &Address, data: &[u8]) -> Result<Vec<u8>, String> {
        let result = self.call(
            "eth_call",
            &serde_json::json!([{ "to": to.to_string(), "data": to_hex(data) }, "latest"]),
        )?;
        let text = result.as_str().ok_or("eth_call returned no hex")?;
        hex_bytes(text).map_err(|e| e.to_string())
    }

    /// A call whose result is one quantity.
    fn quantity_of(&self, method: &str, params: &serde_json::Value) -> Result<u128, String> {
        let result = self.call(method, params)?;
        let text = result
            .as_str()
            .ok_or_else(|| format!("{method} returned no quantity"))?;
        quantity_u128(text).map_err(|e| format!("{method}: {e}"))
    }

    /// The chain's id. Robinhood Chain mainnet is 4663.
    ///
    /// # Errors
    ///
    /// The endpoint's error, or a result that is not a 64-bit quantity.
    pub fn chain_id(&self) -> Result<u64, String> {
        let id = self.quantity_of("eth_chainId", &serde_json::json!([]))?;
        u64::try_from(id).map_err(|_| format!("eth_chainId: {id} does not fit"))
    }

    /// The latest block number.
    ///
    /// # Errors
    ///
    /// The endpoint's error, or a result that is not a 64-bit quantity.
    pub fn block_number(&self) -> Result<u64, String> {
        let n = self.quantity_of("eth_blockNumber", &serde_json::json!([]))?;
        u64::try_from(n).map_err(|_| format!("eth_blockNumber: {n} does not fit"))
    }

    /// An account's ETH at the latest block, in wei.
    ///
    /// # Errors
    ///
    /// The endpoint's error, or a result that is not a quantity.
    pub fn balance(&self, account: &Address) -> Result<u128, String> {
        self.quantity_of(
            "eth_getBalance",
            &serde_json::json!([account.to_string(), "latest"]),
        )
    }

    /// How many transactions an account has sent, at `tag`: the next nonce.
    ///
    /// # Errors
    ///
    /// The endpoint's error, or a result that is not a 64-bit quantity.
    pub fn nonce(&self, account: &Address, tag: Tag) -> Result<u64, String> {
        let n = self.quantity_of(
            "eth_getTransactionCount",
            &serde_json::json!([account.to_string(), tag.name()]),
        )?;
        u64::try_from(n).map_err(|_| format!("eth_getTransactionCount: {n} does not fit"))
    }

    /// The code at an address at the latest block: empty for an ordinary
    /// account.
    ///
    /// # Errors
    ///
    /// The endpoint's error, or a result that is not hex.
    pub fn code(&self, account: &Address) -> Result<Vec<u8>, String> {
        let result = self.call(
            "eth_getCode",
            &serde_json::json!([account.to_string(), "latest"]),
        )?;
        let text = result.as_str().ok_or("eth_getCode returned no hex")?;
        hex_bytes(text).map_err(|e| format!("eth_getCode: {e}"))
    }

    /// The gas a call from `from` would use, as the node estimates it.
    ///
    /// On an Arbitrum chain this includes the L1 data component, which is why
    /// a plain transfer is estimated rather than assumed to be 21,000
    /// (research 0035 §1). A call that would revert is an error here, before
    /// anything is signed.
    ///
    /// # Errors
    ///
    /// The endpoint's error, a revert included, or a result that is not a
    /// 64-bit quantity.
    pub fn estimate_gas(
        &self,
        from: &Address,
        to: &Address,
        value: u128,
        data: &[u8],
    ) -> Result<u64, String> {
        let gas = self.quantity_of(
            "eth_estimateGas",
            &serde_json::json!([{
                "from": from.to_string(),
                "to": to.to_string(),
                "value": format!("{value:#x}"),
                "data": to_hex(data),
            }]),
        )?;
        u64::try_from(gas).map_err(|_| format!("eth_estimateGas: {gas} does not fit"))
    }

    /// The latest block's base fee, in wei per gas.
    ///
    /// # Errors
    ///
    /// The endpoint's error, or a block with no readable `baseFeePerGas`.
    pub fn base_fee(&self) -> Result<u128, String> {
        let block = self.call(
            "eth_getBlockByNumber",
            &serde_json::json!(["latest", false]),
        )?;
        let text = block
            .get("baseFeePerGas")
            .and_then(serde_json::Value::as_str)
            .ok_or("eth_getBlockByNumber: no baseFeePerGas")?;
        quantity_u128(text).map_err(|e| format!("baseFeePerGas: {e}"))
    }

    /// A transaction by hash, or `None` when the node does not know it.
    ///
    /// # Errors
    ///
    /// The endpoint's error, or why the transaction did not parse.
    pub fn transaction(&self, hash: &Hash32) -> Result<Option<Transaction>, String> {
        let result = self.call(
            "eth_getTransactionByHash",
            &serde_json::json!([hash.to_string()]),
        )?;
        if result.is_null() {
            return Ok(None);
        }
        Transaction::from_json(&result)
            .map(Some)
            .map_err(|e| format!("transaction: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_reads_in_either_case_and_prints_lowercase() {
        let lower: Address = "0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e"
            .parse()
            .unwrap();
        let mixed: Address = "0x7eD598BcEf8bd9Edd8C97A195C6d13f40801EC7e"
            .parse()
            .unwrap();
        assert_eq!(lower, mixed);
        assert_eq!(
            lower,
            Address::from_hex("0x7eD598BcEf8bd9Edd8C97A195C6d13f40801EC7e")
        );
        assert_eq!(
            mixed.to_string(),
            "0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e"
        );
        assert_eq!(format!("{mixed:?}"), mixed.to_string());
    }

    #[test]
    fn malformed_hex_is_refused_not_guessed() {
        assert_eq!(hex_bytes("0x"), Ok(vec![]));
        assert_eq!(hex_bytes("0x0aff"), Ok(vec![0x0a, 0xff]));
        for bad in ["0aff", "0x0af", "0x0g", "0Xff"] {
            assert_eq!(hex_bytes(bad), Err(ReadError::Hex(bad.to_owned())), "{bad}");
        }
        assert_eq!(
            "0x00".parse::<Address>(),
            Err(ReadError::Width {
                expected: 20,
                got: 1
            })
        );
        let hash = format!("0x{}", "ab".repeat(32));
        assert_eq!(
            hash.parse::<Hash32>().map(|h| h.to_string()),
            Ok(hash.clone())
        );
        assert_eq!(hash.parse::<Hash32>().map(|h| format!("{h:?}")), Ok(hash));
    }

    #[test]
    fn a_quantity_is_hex_and_fits_or_is_refused() {
        assert_eq!(quantity("0x0"), Ok(0));
        assert_eq!(quantity("0x3b7827e"), Ok(62_358_142));
        assert_eq!(quantity("0xffffffffffffffff"), Ok(u64::MAX));
        assert_eq!(
            quantity("0x10000000000000000"),
            Err(ReadError::Range("0x10000000000000000".to_owned()))
        );
        for bad in ["0x", "12", "0x-1", "0x+1", "0xz"] {
            assert_eq!(quantity(bad), Err(ReadError::Hex(bad.to_owned())), "{bad}");
        }
    }

    #[test]
    fn a_wide_quantity_is_hex_and_fits_128_bits_or_is_refused() {
        assert_eq!(quantity_u128("0x0"), Ok(0));
        assert_eq!(
            quantity_u128("0x68d4142c335ad9669c"),
            Ok(1_933_743_271_700_447_454_876)
        );
        assert_eq!(
            quantity_u128(&format!("0x{}", "f".repeat(32))),
            Ok(u128::MAX)
        );
        let over = format!("0x1{}", "0".repeat(32));
        assert_eq!(quantity_u128(&over), Err(ReadError::Range(over.clone())));
        for bad in ["0x", "12", "0x-1", "0x+1", "0xz"] {
            assert_eq!(
                quantity_u128(bad),
                Err(ReadError::Hex(bad.to_owned())),
                "{bad}"
            );
        }
        assert_eq!(to_hex(&[]), "0x");
        assert_eq!(to_hex(&[0x0a, 0xff]), "0x0aff");
    }

    #[test]
    fn a_word_is_an_address_or_an_amount_only_when_its_high_bytes_are_zero() {
        let mut w = [0u8; 32];
        w[12..].copy_from_slice(&[0x11; 20]);
        assert_eq!(word_address(&w), Some(Address([0x11; 20])));
        assert_eq!(Hash32(w).address(), Some(Address([0x11; 20])));
        w[11] = 1;
        assert_eq!(word_address(&w), None);

        let mut amount = [0u8; 32];
        amount[16..].copy_from_slice(&u128::MAX.to_be_bytes());
        assert_eq!(word_u128(&amount), Some(u128::MAX));
        amount[15] = 1;
        assert_eq!(word_u128(&amount), None);
    }

    #[test]
    fn words_are_read_whole_or_not_at_all() {
        let data: Vec<u8> = (0..70).collect();
        assert_eq!(word(&data, 0).map(|w| w[0]), Some(0));
        assert_eq!(word(&data, 1).map(|w| w[31]), Some(63));
        assert_eq!(word(&data, 2), None, "six bytes are not a word");
        assert_eq!(word(&data, usize::MAX), None);
    }

    #[test]
    fn a_receipt_status_is_success_failure_or_malformed() {
        let base = serde_json::json!({
            "transactionHash": format!("0x{}", "11".repeat(32)),
            "blockNumber": "0x10",
            "from": format!("0x{}", "22".repeat(20)),
            "to": null,
            "logs": [],
        });
        let with = |status: &str| {
            let mut v = base.clone();
            v["status"] = serde_json::Value::from(status);
            Receipt::from_json(&v)
        };
        let ok = with("0x1").unwrap();
        assert!(ok.succeeded);
        assert_eq!(ok.to, None);
        assert_eq!(ok.block, 16);
        assert!(!with("0x0").unwrap().succeeded);
        assert_eq!(with("0x2"), Err(ReadError::Hex("0x2".to_owned())));
        assert_eq!(Receipt::from_json(&base), Err(ReadError::Missing("status")));
    }
}
