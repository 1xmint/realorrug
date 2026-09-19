// SPDX-License-Identifier: Apache-2.0
//! A dated market snapshot -- design 0027 §2.2's "Market and exit" row.
//!
//! # Price and market cap always carry their moment
//!
//! AGENTS.md §3 rule 5 (ADR 0033): every price or market cap the analyst may
//! state carries the block or time it was read at. A DexScreener or
//! GeckoTerminal answer is not pinned to this dossier's own read block --
//! those are off-chain aggregators on their own refresh cycle -- so this
//! module stamps [`MarketSnapshot::observed_at`] with wall-clock time at the
//! moment of the call, which is the honest claim: "this is what the
//! aggregator said just now," not "this is what block N held."
//!
//! # Liquidity dollars are not executable depth
//!
//! Design 0027 §2.1's third distinction: supply held, supply available to
//! sell and executable exit depth are different numbers, and "liquidity USD
//! cannot populate `capacity`" is explicit in §2.2's own row for this data.
//! `MarketSnapshot` therefore has **no `capacity` field at all** -- capacity
//! is `CurveFacts::quote_capacity`, computed by the curve-side bisection in
//! `dossier.rs`/`robinhood.rs`, in the quote asset's own smallest unit, not
//! in dollars. [`attach`] only ever sets `Dossier::market`; it is written so
//! that touching `Dossier::curve` from this module is not just discouraged,
//! it does not compile without adding code here that this file's own test
//! would then have to justify.

use std::time::SystemTime;

use crate::budget::Budget;
use crate::dossier::Dossier;

/// Which aggregator answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `api.dexscreener.com`, tried first: free, no key, per design 0027
    /// §2.2's "one free DexScreener token/pair HTTP lookup."
    DexScreener,
    /// `api.geckoterminal.com`, tried when DexScreener fails or has no pair
    /// for this token.
    GeckoTerminal,
}

/// One dated read of a token's market.
#[derive(Clone, Debug, PartialEq)]
pub struct MarketSnapshot {
    /// USD price of one token, when the aggregator reported one.
    pub price_usd: Option<f64>,
    /// USD market capitalisation (or fully-diluted valuation, whichever the
    /// aggregator returned -- [`MarketSnapshot::cap_basis`] says which),
    /// when reported.
    pub market_cap_usd: Option<f64>,
    /// What `market_cap_usd` is a cap *of*: circulating or fully diluted.
    /// `None` alongside a `None` cap; never guessed when the cap is present,
    /// because the two bases are not interchangeable and the sheet must be
    /// able to say which one it is quoting.
    pub cap_basis: Option<&'static str>,
    /// The pool's own reported USD liquidity. **Not capacity, not depth --
    /// see this module's own doc comment.** A dollar figure the aggregator
    /// computed from reserves at its own price feed, useful only as "the
    /// aggregator says the pool is roughly this deep in dollars," never as
    /// an amount that can be sized into.
    pub liquidity_usd: Option<f64>,
    /// The pair address the figures came from, when the aggregator named
    /// one -- more than one pool can exist for a token, and this says which
    /// pool these numbers describe.
    pub pair_address: Option<String>,
    /// Which aggregator answered.
    pub source: Source,
    /// Wall-clock time this snapshot was read at. Always set: a snapshot
    /// with no reading is not constructed at all (see [`snapshot`]'s `Err`).
    pub observed_at: SystemTime,
}

/// A minimal HTTP GET seam, the same shape `rpc.rs`'s `Transport` gives the
/// JSON-RPC path: production uses `ureq`, tests use a canned responder, and
/// nothing above this trait can tell the difference.
pub trait HttpGet: Send + Sync {
    /// Performs a GET and returns the response body.
    ///
    /// # Errors
    ///
    /// A transport-level message, including a non-2xx status if the caller
    /// chooses to surface it that way.
    fn get(&self, url: &str) -> Result<String, String>;
}

/// The real one: `ureq`, ten-second timeout, same as `rpc.rs`'s `RpcClient`.
pub struct Http {
    agent: ureq::Agent,
}

impl Default for Http {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl HttpGet for Http {
    fn get(&self, url: &str) -> Result<String, String> {
        let mut response = self.agent.get(url).call().map_err(|e| e.to_string())?;
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())
    }
}

/// Reads a `f64` out of a JSON value that may be a string or a number --
/// DexScreener and GeckoTerminal both send price fields as strings.
fn number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

/// Parses DexScreener's `/latest/dex/tokens/{address}` response: the first
/// pair in `pairs` that carries a `priceUsd`, because DexScreener does not
/// document an ordering and the first readable pair is the deterministic
/// choice the client can defend.
fn parse_dexscreener(body: &str) -> Result<MarketSnapshot, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("dexscreener: {e}"))?;
    let pairs = value
        .get("pairs")
        .and_then(serde_json::Value::as_array)
        .ok_or("dexscreener: no pairs array")?;
    let pair = pairs
        .iter()
        .find(|p| p.get("priceUsd").is_some())
        .ok_or("dexscreener: no pair with a price")?;
    let price_usd = pair.get("priceUsd").and_then(number);
    let market_cap_usd = pair.get("marketCap").and_then(number);
    let cap_basis = market_cap_usd.map(|_| "circulating, as DexScreener reports it");
    let liquidity_usd = pair
        .get("liquidity")
        .and_then(|l| l.get("usd"))
        .and_then(number);
    let pair_address = pair
        .get("pairAddress")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    Ok(MarketSnapshot {
        price_usd,
        market_cap_usd,
        cap_basis,
        liquidity_usd,
        pair_address,
        source: Source::DexScreener,
        observed_at: SystemTime::now(),
    })
}

/// Parses GeckoTerminal's `/networks/{network}/tokens/{address}` response.
fn parse_geckoterminal(body: &str) -> Result<MarketSnapshot, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("geckoterminal: {e}"))?;
    let attrs = value
        .get("data")
        .and_then(|d| d.get("attributes"))
        .ok_or("geckoterminal: no data.attributes")?;
    let price_usd = attrs.get("price_usd").and_then(number);
    let market_cap_usd = attrs
        .get("market_cap_usd")
        .and_then(number)
        .or_else(|| attrs.get("fdv_usd").and_then(number));
    let cap_basis = if attrs.get("market_cap_usd").and_then(number).is_some() {
        Some("circulating, as GeckoTerminal reports it")
    } else if market_cap_usd.is_some() {
        Some("fully diluted (GeckoTerminal reported no circulating cap)")
    } else {
        None
    };
    let liquidity_usd = attrs.get("total_reserve_in_usd").and_then(number);
    Ok(MarketSnapshot {
        price_usd,
        market_cap_usd,
        cap_basis,
        liquidity_usd,
        pair_address: None,
        source: Source::GeckoTerminal,
        observed_at: SystemTime::now(),
    })
}

/// Reads one dated market snapshot: DexScreener first, GeckoTerminal on
/// failure, bounded to at most two HTTP calls and priced one call each
/// against `budget` (design 0027's "every new external read is bounded and
/// priced in the budget").
///
/// # Errors
///
/// A string naming why, when both the budget and both aggregators refused --
/// the caller (`robinhood.rs`) turns this into a named gap on the dossier
/// rather than failing the whole build, per AGENTS.md §3 rule 8.
pub fn snapshot(
    http: &dyn HttpGet,
    budget: &mut Budget,
    dexscreener_url: &str,
    geckoterminal_url: &str,
) -> Result<MarketSnapshot, String> {
    budget
        .take_call()
        .map_err(|e| format!("budget exhausted before DexScreener: {e:?}"))?;
    let first = http
        .get(dexscreener_url)
        .map_err(|e| format!("dexscreener: {e}"))
        .and_then(|body| parse_dexscreener(&body));
    if let Ok(snapshot) = first {
        return Ok(snapshot);
    }
    let dexscreener_err = first.unwrap_err();

    budget
        .take_call()
        .map_err(|e| format!("budget exhausted before GeckoTerminal: {e:?}"))?;
    http.get(geckoterminal_url)
        .map_err(|e| format!("geckoterminal: {e}"))
        .and_then(|body| parse_geckoterminal(&body))
        .map_err(|gecko_err| format!("{dexscreener_err}; fallback also failed: {gecko_err}"))
}

/// Attaches `snapshot` to `dossier`'s market field, and nothing else.
///
/// The whole reason this function exists rather than callers writing
/// `dossier.market = Some(snapshot)` directly: it is the one place a review
/// or a mutation test can point at to confirm `Dossier::curve` (and
/// specifically `quote_capacity`) is never touched by a market read. See
/// `liquidity_dollars_never_reach_capacity` below.
pub fn attach(dossier: &mut Dossier, snapshot: MarketSnapshot) {
    dossier.market = Some(snapshot);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dossier::{ChainLaunch, CurveFacts, QuoteAsset};
    use realorrug_types::ChainAddress;

    struct Canned {
        pages: std::sync::Mutex<Vec<Result<String, String>>>,
    }

    impl HttpGet for Canned {
        fn get(&self, _url: &str) -> Result<String, String> {
            self.pages.lock().unwrap().remove(0)
        }
    }

    fn canned(pages: Vec<Result<String, String>>) -> Canned {
        Canned {
            pages: std::sync::Mutex::new(pages),
        }
    }

    fn empty_dossier() -> Dossier {
        Dossier {
            mint: ChainAddress::Solana(realorrug_types::Address::new([0u8; 32])),
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
        }
    }

    #[test]
    fn dexscreener_is_parsed_for_price_cap_and_liquidity() {
        let body = serde_json::json!({
            "pairs": [{
                "priceUsd": "0.00042",
                "marketCap": 420_000,
                "liquidity": {"usd": 15_000.5},
                "pairAddress": "0xabc"
            }]
        })
        .to_string();
        let snap = parse_dexscreener(&body).unwrap();
        assert!((snap.price_usd.unwrap() - 0.00042).abs() < 1e-12);
        assert!((snap.market_cap_usd.unwrap() - 420_000.0).abs() < f64::EPSILON);
        assert!((snap.liquidity_usd.unwrap() - 15_000.5).abs() < f64::EPSILON);
        assert_eq!(snap.pair_address.as_deref(), Some("0xabc"));
        assert_eq!(snap.source, Source::DexScreener);
    }

    #[test]
    fn a_pair_with_no_price_is_skipped_for_one_that_has_it() {
        let body = serde_json::json!({
            "pairs": [
                {"pairAddress": "0x1"},
                {"priceUsd": "1.5", "pairAddress": "0x2"}
            ]
        })
        .to_string();
        let snap = parse_dexscreener(&body).unwrap();
        assert_eq!(snap.pair_address.as_deref(), Some("0x2"));
    }

    #[test]
    fn geckoterminal_falls_back_to_fdv_and_names_the_basis() {
        let body = serde_json::json!({
            "data": {"attributes": {"price_usd": "2.0", "fdv_usd": "9000", "total_reserve_in_usd": "500"}}
        })
        .to_string();
        let snap = parse_geckoterminal(&body).unwrap();
        assert!((snap.market_cap_usd.unwrap() - 9_000.0).abs() < f64::EPSILON);
        assert_eq!(
            snap.cap_basis,
            Some("fully diluted (GeckoTerminal reported no circulating cap)")
        );
    }

    #[test]
    fn dexscreener_failure_falls_back_to_geckoterminal() {
        let mut budget = Budget::default();
        let gecko_body = serde_json::json!({
            "data": {"attributes": {"price_usd": "3.0"}}
        })
        .to_string();
        let http = canned(vec![Err("timed out".to_owned()), Ok(gecko_body)]);
        let snap = snapshot(
            &http,
            &mut budget,
            "https://dex.invalid",
            "https://gecko.invalid",
        )
        .expect("gecko fallback succeeds");
        assert_eq!(snap.source, Source::GeckoTerminal);
        assert_eq!(budget.calls_made(), 2, "both lookups were priced");
    }

    #[test]
    fn both_aggregators_failing_names_both_reasons() {
        let mut budget = Budget::default();
        let http = canned(vec![
            Err("timed out".to_owned()),
            Err("also down".to_owned()),
        ]);
        let err = snapshot(
            &http,
            &mut budget,
            "https://dex.invalid",
            "https://gecko.invalid",
        )
        .unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        assert!(err.contains("also down"), "{err}");
    }

    #[test]
    fn a_budget_with_no_calls_left_never_reaches_the_network() {
        let mut budget = Budget::new(0, 1, std::time::Duration::from_secs(1));
        let http = canned(vec![]);
        let err = snapshot(
            &http,
            &mut budget,
            "https://dex.invalid",
            "https://gecko.invalid",
        )
        .unwrap_err();
        assert!(err.contains("budget exhausted"), "{err}");
    }

    /// The regression this whole module exists to prevent: a market read's
    /// liquidity dollars must never populate the curve's executable
    /// capacity. `attach` only ever touches `Dossier::market`; this test
    /// fails if a future edit makes it also touch `Dossier::curve`.
    #[test]
    fn liquidity_dollars_never_reach_capacity() {
        let mut dossier = empty_dossier();
        dossier.curve = Some(CurveFacts {
            complete: false,
            quote_reserves: 1_000,
            quote_capacity: Some(777),
            quote_asset: Some(QuoteAsset::eth()),
            creator: realorrug_types::ChainAddress::Solana(realorrug_types::Address::new(
                [0u8; 32],
            )),
            fees: None,
        });
        let snap = MarketSnapshot {
            price_usd: Some(1.0),
            market_cap_usd: Some(1_000_000.0),
            cap_basis: Some("test"),
            // A liquidity figure deliberately different from `quote_capacity`
            // so any accidental cross-wiring would change the assertion below.
            liquidity_usd: Some(999_999.0),
            pair_address: None,
            source: Source::DexScreener,
            observed_at: SystemTime::now(),
        };
        attach(&mut dossier, snap);
        assert_eq!(
            dossier.curve.as_ref().unwrap().quote_capacity,
            Some(777),
            "attaching a market snapshot must never move the curve's capacity"
        );
        assert!(dossier.market.is_some());
    }

    #[test]
    fn chain_launch_type_is_unaffected_by_a_market_attach() {
        // A second sanity check on the same boundary, from the other typed
        // field a market read could plausibly (and wrongly) be routed
        // through if someone "simplified" `attach` later.
        let mut dossier = empty_dossier();
        dossier.chain_launch = Some(ChainLaunch {
            block: 5,
            age_seconds: Some(10),
            dev_buy_wei: Some(0),
            dev_buy_tokens: Some(0),
            supply: None,
            name: None,
            symbol: None,
            correlated_selling: None,
        });
        attach(
            &mut dossier,
            MarketSnapshot {
                price_usd: None,
                market_cap_usd: None,
                cap_basis: None,
                liquidity_usd: Some(1.0),
                pair_address: None,
                source: Source::GeckoTerminal,
                observed_at: SystemTime::now(),
            },
        );
        assert_eq!(dossier.chain_launch.as_ref().unwrap().block, 5);
    }
}
