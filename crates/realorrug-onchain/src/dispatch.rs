// SPDX-License-Identifier: Apache-2.0
//! The one place that decides which chain an address is on.
//!
//! # The bug this exists to remove
//!
//! Both callers that answer a question about a mint parsed it the same way:
//! `mint_text.parse::<realorrug_types::Address>()`, Solana's own base58
//! decoder. A Robinhood `0x…` address has never been valid base58 (see
//! [`realorrug_types::ChainAddress`]'s own doc comment for why the two
//! alphabets cannot overlap), so every such address failed that parse and was
//! answered "not an address" -- the bot denying a real token exists, in
//! public, about the chain this project is being built for.
//!
//! # Why one function
//!
//! "Which chain is this address on" is a single decision. Writing that match
//! twice -- once per call site -- is the exact defect one level up: two copies
//! agree today and drift the day only one of them is edited. Every caller
//! that needs a dossier for a piece of mint text calls [`read`] instead of
//! parsing and matching on its own.
//!
//! # Deny by default (AGENTS.md §3 rule 7)
//!
//! A [`ChainAddress::Robinhood`] with no [`Clients::robinhood`] configured
//! answers [`Error::Unreadable`], never a Solana read (the address is not
//! Solana's to read) and never [`Error::NotAnAddress`] (the text *is* a real
//! address; the operator simply has not wired up that chain yet). The two
//! failure shapes stay distinguishable because they mean different things to
//! the person reading the reply: one means "you typed something that is not a
//! token", the other means "Radar could not read it".

use realorrug_types::{Address, ChainAddress};

use crate::budget::Budget;
use crate::dossier::{ChainReader, Dossier, SolanaReader};
use crate::robinhood::RobinhoodReader;
use crate::rpc::RpcClient;

/// Every client a dispatch might need, one per chain.
///
/// `robinhood` is `Option` because there is no default Robinhood endpoint
/// (unlike Solana's `RpcClient::from_vars`, which falls back to a public
/// one) -- the public Robinhood endpoint is rate-limited, so an operator who
/// has not configured `--rpc` (or the analyst's own equivalent) has made a
/// choice, and that choice is "Radar cannot read this chain yet", not "read
/// it anyway against a default this crate invented."
pub struct Clients<'a> {
    /// Solana's reader always has somewhere to read from.
    pub solana: &'a RpcClient,
    /// Robinhood Chain's reader, or `None` when no endpoint is configured.
    pub robinhood: Option<&'a realorrug_robinhood::Rpc>,
    /// A market-data seam (design 0027 §2.2, "Market and exit"), or `None`
    /// when no market read is configured. Deny-by-default, the same shape as
    /// `robinhood` above: `None` means "no market read", never a read
    /// against a default this crate invented. Wired into the Robinhood arm
    /// only -- see [`robinhood`]'s own doc comment.
    pub market: Option<&'a dyn crate::market::HttpGet>,
}

/// Why [`read`] did not produce a dossier.
#[derive(Debug)]
pub enum Error {
    /// The text is neither a Robinhood nor a Solana address, by shape.
    NotAnAddress,
    /// The text names a real address, but the chain it is on could not be
    /// read -- an RPC failure, a budget exhausted, or (for Robinhood) no
    /// endpoint configured at all.
    Unreadable(String),
}

/// Reads whichever chain `mint_text` names, dispatching purely on the
/// address's own shape.
///
/// A `0x` address is always Robinhood's, a base58 one is always Solana's
/// (`ChainAddress::from_str`'s own doc comment proves the two shapes cannot
/// overlap), so no configuration and no guessing decides which chain a
/// question is about -- the address does. One [`Budget`], the crate's own
/// default, per read: `answer.rs` already explains why that is not restated
/// at every call site, and this function is the one place left to restate it
/// in.
///
/// # Errors
///
/// [`Error::NotAnAddress`] when `mint_text` is neither shape, and
/// [`Error::Unreadable`] when it names a real address on a chain that could
/// not be read -- including a Robinhood address with no endpoint configured.
pub fn read(mint_text: &str, clients: &Clients<'_>) -> Result<Dossier, Error> {
    let mut budget = Budget::default();
    read_with_memory(mint_text, clients, None, &mut budget)
}

/// Reads with the caller's launch memory and budget. Keeping the budget at
/// the caller lets it count calls even when no dossier can be returned.
///
/// # Errors
///
/// The same address and chain failures as [`read`].
pub fn read_with_memory(
    mint_text: &str,
    clients: &Clients<'_>,
    memory: Option<&crate::memory::Memory>,
    budget: &mut Budget,
) -> Result<Dossier, Error> {
    let address: ChainAddress = mint_text.parse().map_err(|_| Error::NotAnAddress)?;
    match address {
        ChainAddress::Solana(mint) => solana(clients.solana, budget, &mint, memory),
        ChainAddress::Robinhood(token) => {
            robinhood(clients.robinhood, clients.market, budget, &token, memory)
        }
    }
}

/// The Solana arm, split out so [`read`]'s match stays one line per chain.
fn solana(
    client: &RpcClient,
    budget: &mut Budget,
    mint: &Address,
    memory: Option<&crate::memory::Memory>,
) -> Result<Dossier, Error> {
    SolanaReader { memory }
        .read(client, budget, mint)
        .map_err(|e| Error::Unreadable(e.to_string()))
}

/// The Robinhood arm. `client` is `None` exactly when no endpoint is
/// configured, and that is answered [`Error::Unreadable`] before any read is
/// attempted -- there is nothing to fall back to, and falling back to Solana
/// would be answering about the wrong chain entirely.
///
/// `market` is the deny-by-default market-data seam (design 0027 §2.2).
/// `None` attaches nothing -- not a gap, because no read was configured, the
/// same distinction `Clients::robinhood` draws for "no endpoint". `Some`
/// that fails to read names `"market"` in [`Dossier::unavailable`]
/// (AGENTS.md §3 rule 8: absent is not zero), never a dossier with zeros
/// standing in for a price nobody read. Wired here, on the Robinhood arm
/// only: DexScreener and GeckoTerminal price EVM-style tokens, and the
/// Solana arm has no equivalent read wired up yet (design 0027's slice 4
/// note).
fn robinhood(
    client: Option<&realorrug_robinhood::Rpc>,
    market: Option<&dyn crate::market::HttpGet>,
    budget: &mut Budget,
    token: &realorrug_robinhood::Address,
    memory: Option<&crate::memory::Memory>,
) -> Result<Dossier, Error> {
    let Some(client) = client else {
        return Err(Error::Unreadable(
            "no Robinhood endpoint is configured, so this token's chain cannot be read".to_owned(),
        ));
    };
    let mut dossier = RobinhoodReader { memory }
        .read(client, budget, token)
        .map_err(|e| Error::Unreadable(e.to_string()))?;
    if let Some(http) = market {
        let token_text = token.to_string();
        // DexScreener's token lookup takes a bare address, no chain slug --
        // one endpoint answers for every chain it indexes. GeckoTerminal
        // needs a network slug per its own path shape; "robinhood" is the
        // same identifier `robinhood.rs::MEMORY_CHAIN` already uses to name
        // this chain elsewhere in this crate, kept in sync with it rather
        // than a second name invented here.
        let dexscreener_url = format!("https://api.dexscreener.com/latest/dex/tokens/{token_text}");
        let geckoterminal_url =
            format!("https://api.geckoterminal.com/api/v2/networks/robinhood/tokens/{token_text}");
        match crate::market::snapshot(http, budget, &dexscreener_url, &geckoterminal_url) {
            Ok(snapshot) => crate::market::attach(&mut dossier, snapshot),
            Err(why) => dossier.unavailable.push(crate::dossier::Unavailable {
                fact: "market",
                why,
            }),
        }
    }
    Ok(dossier)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Solana client pointed at an address nothing answers on, so a test
    /// that reaches it fails loudly rather than hanging on the network --
    /// and, for the "no Robinhood endpoint" tests, so a wrongly-taken Solana
    /// path is distinguishable from a correctly-refused one: this client
    /// would error (or hang) if it were ever asked.
    fn unreachable_solana() -> RpcClient {
        RpcClient::new("http://127.0.0.1:1")
    }

    const ROBINHOOD_SHAPED: &str = "0x1111111111111111111111111111111111111111";
    const SOLANA_SHAPED: &str = "So11111111111111111111111111111111111111112";

    #[test]
    fn a_robinhood_shaped_address_is_not_answered_not_an_address() {
        // The exact failure the bug report names: a real `0x…` address must
        // not come back looking like it was never a token at all. With no
        // Robinhood endpoint configured the read still fails, but on the
        // *chain*, not on the *shape*.
        let solana = unreachable_solana();
        let clients = Clients {
            solana: &solana,
            robinhood: None,
            market: None,
        };
        let err = read(ROBINHOOD_SHAPED, &clients).expect_err("no endpoint, no dossier");
        assert!(
            matches!(err, Error::Unreadable(_)),
            "a real address must never be reported as not-an-address: {err:?}"
        );
    }

    #[test]
    fn a_robinhood_address_with_no_endpoint_never_reads_solana() {
        // Distinct from the test above: this asserts the *reason* is right,
        // not only that it is not `NotAnAddress`. Pointing the Solana client
        // at an address that errors immediately means a dispatcher that
        // mistakenly fell through to a Solana read would surface a different
        // error (or hang) rather than this one.
        let solana = unreachable_solana();
        let clients = Clients {
            solana: &solana,
            robinhood: None,
            market: None,
        };
        let err = read(ROBINHOOD_SHAPED, &clients).expect_err("no endpoint, no dossier");
        let Error::Unreadable(why) = err else {
            panic!("expected Unreadable, got {err:?}");
        };
        assert!(why.contains("Robinhood"), "{why}");
        assert!(why.contains("no"), "{why}");
    }

    #[test]
    fn something_genuinely_neither_shape_is_not_an_address() {
        let solana = unreachable_solana();
        let clients = Clients {
            solana: &solana,
            robinhood: None,
            market: None,
        };
        for text in ["", "not an address", "0xshort", &"a".repeat(60)] {
            let err = read(text, &clients).expect_err("neither shape, no dossier");
            assert!(
                matches!(err, Error::NotAnAddress),
                "{text:?} should be refused as not-an-address, got {err:?}"
            );
        }
    }

    #[test]
    fn a_robinhood_read_through_dispatch_reaches_the_memory() {
        // The reader remembers what it read only if dispatch hands it the
        // memory; a `RobinhoodReader::default()` here would read fine and
        // remember nothing, which this checkpoint would show.
        use crate::memory::Memory;
        use crate::robinhood::tests::{full_bodies, record, serve, token};

        let rec = record(
            true,
            realorrug_robinhood::Address([0x23; 20]),
            realorrug_robinhood::Address([0x34; 20]),
        );
        let robinhood = realorrug_robinhood::Rpc::new(serve(full_bodies(&rec, "0x1")));
        let solana = unreachable_solana();
        let clients = Clients {
            solana: &solana,
            robinhood: Some(&robinhood),
            market: None,
        };
        let memory = Memory::open_in_memory().expect("a memory");
        let mut budget = Budget::default();
        let dossier = read_with_memory(&token().to_string(), &clients, Some(&memory), &mut budget)
            .expect("a dossier");
        assert!(dossier.holders.is_some());
        let checkpoint = memory
            .token_checkpoint("robinhood", &token().to_string())
            .expect("read");
        assert_eq!(checkpoint.map(|c| c.block), Some(0x64));
    }

    /// A GET that always fails, so `market::snapshot` fails both aggregators
    /// and `robinhood`'s market arm has something to name a gap about.
    struct AlwaysFails;

    impl crate::market::HttpGet for AlwaysFails {
        fn get(&self, _url: &str) -> Result<String, String> {
            Err("connection refused".to_owned())
        }
    }

    /// `Clients.market: None` is deny-by-default (AGENTS.md §3 rule 7): no
    /// read is attempted, so no `"market"` gap is named and `Dossier::market`
    /// stays `None` -- absent because nothing was configured, not because a
    /// read failed. Distinct from the next test, which configures a client
    /// that *fails*.
    #[test]
    fn no_market_client_means_no_market_fact_and_no_gap() {
        use crate::robinhood::tests::{full_bodies, record, serve, token};

        let rec = record(
            true,
            realorrug_robinhood::Address([0x23; 20]),
            realorrug_robinhood::Address([0x34; 20]),
        );
        let robinhood = realorrug_robinhood::Rpc::new(serve(full_bodies(&rec, "0x1")));
        let solana = unreachable_solana();
        let clients = Clients {
            solana: &solana,
            robinhood: Some(&robinhood),
            market: None,
        };
        let dossier = read(&token().to_string(), &clients).expect("a dossier");
        assert!(dossier.market.is_none());
        assert!(
            !dossier.unavailable.iter().any(|u| u.fact == "market"),
            "no market client configured must never name a market gap: {:?}",
            dossier.unavailable
        );
    }

    /// A configured market client that fails names the gap as `"market"` on
    /// `Dossier::unavailable`, per AGENTS.md §3 rule 8 (absent is not zero):
    /// the rest of the dossier still comes back, and the price is missing,
    /// never zero.
    #[test]
    fn a_failed_market_read_names_the_market_gap() {
        use crate::robinhood::tests::{full_bodies, record, serve, token};

        let rec = record(
            true,
            realorrug_robinhood::Address([0x23; 20]),
            realorrug_robinhood::Address([0x34; 20]),
        );
        let robinhood = realorrug_robinhood::Rpc::new(serve(full_bodies(&rec, "0x1")));
        let solana = unreachable_solana();
        let market = AlwaysFails;
        let clients = Clients {
            solana: &solana,
            robinhood: Some(&robinhood),
            market: Some(&market),
        };
        let dossier = read(&token().to_string(), &clients).expect("a dossier");
        assert!(dossier.market.is_none());
        assert!(
            dossier.unavailable.iter().any(|u| u.fact == "market"),
            "a failed market read must name the gap: {:?}",
            dossier.unavailable
        );
    }

    #[test]
    fn a_solana_address_still_takes_the_solana_path() {
        // Pinned so a regression in the shared dispatch path is loud: a
        // base58 address's answer must be exactly what `SolanaReader` alone
        // produces, with no Robinhood client involved at all.
        use crate::rpc::Transport;

        struct Always(String);
        impl Transport for Always {
            fn post(&self, _: &str, _: String) -> Result<String, String> {
                Ok(self.0.clone())
            }
        }

        // The wrong shape for the signature-list call `SolanaReader` makes
        // first, so both paths fail identically on the same malformed-response
        // error -- enough to prove the two call paths agree, the same
        // technique `dossier.rs`'s own reader-parity test uses.
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{"value":null}}"#.to_owned();
        let mint = Address::new([1u8; 32]);

        let client_a =
            RpcClient::with_transport("http://test.invalid", Box::new(Always(body.clone())));
        let mut budget_a = Budget::default();
        let direct = SolanaReader::default().read(&client_a, &mut budget_a, &mint);

        let client_b = RpcClient::with_transport("http://test.invalid", Box::new(Always(body)));
        let clients = Clients {
            solana: &client_b,
            robinhood: None,
            market: None,
        };
        let via_dispatch = read(SOLANA_SHAPED, &clients);

        match (direct, via_dispatch) {
            (Ok(d), Ok(r)) => {
                assert_eq!(d.mint, r.mint);
                assert_eq!(d.unavailable, r.unavailable);
            }
            (Err(d), Err(Error::Unreadable(r))) => assert_eq!(d.to_string(), r),
            (d, r) => panic!("SolanaReader and dispatch disagreed: {d:?} vs {r:?}"),
        }
    }
}
