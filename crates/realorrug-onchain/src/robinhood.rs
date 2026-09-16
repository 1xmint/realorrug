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
//! the curve's graduation flag and quote reserves, and the current block as
//! the read point. Everything Pons v2's curve arithmetic has not been modelled
//! for -- capacity, fees, the launch block, a real creator-transaction count --
//! is `None` with an [`Unavailable`] entry naming it, never a default (AGENTS.md
//! §3 rule 8). Filling those in is later work; this reader does not guess at
//! them to look more complete than it is.

use realorrug_robinhood::pons::{FACTORY, LaunchedToken, curve};
use realorrug_robinhood::{Address as RobinhoodAddress, Rpc};
use realorrug_types::{ChainAddress, ReadAt};

use crate::budget::{Budget, Exhausted};
use crate::dossier::{ChainReader, CurveFacts, Dossier, QuoteAsset, Unavailable};

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
/// Every read in this module goes through this function or [`block_number`]
/// so that none of them can skip the budget -- a budget checked only at the
/// start is not a budget.
fn call(
    budget: &mut Budget,
    client: &Rpc,
    to: &RobinhoodAddress,
    data: &[u8],
) -> Result<Vec<u8>, String> {
    budget
        .take_call()
        .map_err(|e| format!("budget exhausted: {e:?}"))?;
    client.call_contract(to, data)
}

/// `eth_blockNumber`, budget-checked.
fn block_number(budget: &mut Budget, client: &Rpc) -> Result<u64, String> {
    budget
        .take_call()
        .map_err(|e| format!("budget exhausted: {e:?}"))?;
    client.block_number()
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
    match block_number(budget, client) {
        Ok(n) => dossier.read_at = Some(ReadAt::Robinhood(n)),
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

    // 4. Facts this reader cannot supply at all yet, regardless of budget.
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
        fact: "launch block",
        why: "LaunchBlock is Solana-slot-shaped; a Robinhood launch-block read \
              is a separate task"
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
        data.extend(word_addr(record.pair.as_ref().unwrap_or(&RobinhoodAddress::ZERO)));
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

    #[test]
    fn a_reader_names_the_token_it_was_asked_for_and_the_block_it_read_at() {
        let curve = RobinhoodAddress([0x22; 20]);
        let deployer = RobinhoodAddress([0x33; 20]);
        let rec = record(true, curve, deployer);
        let url = serve(vec![
            answer(&hex(&launched_token_return(&rec))),
            answer(&serde_json::json!("0x64")),   // block 100
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
            answer(&serde_json::json!("0x1")),
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
            answer(&serde_json::json!("0x1")),
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
            answer(&serde_json::json!("0x1")),
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
        let bodies = || {
            vec![
                answer(&hex(&launched_token_return(&rec))),
                answer(&serde_json::json!("0x9")),
                answer(&hex(&word_bool(false))),
                answer(&hex(&word_u(1))),
            ]
        };
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
            answer(&serde_json::json!("0x1")),
            answer(&hex(&word_bool(false))),
            answer(&hex(&word_u(0))),
        ]);
        let client = Rpc::new(url);
        let mut b = budget();

        let dossier = build(&client, &mut b, &token()).expect("a dossier");
        for fact in ["capacity", "fees", "launch block", "creator transactions"] {
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
}
