// SPDX-License-Identifier: Apache-2.0
//! Pons v2, decoded from the transactions mainnet accepted.
//!
//! Two captures, both in `docs/research/data` and described in research 0036:
//!
//! - `0036-pons-v2-launch.json`: a launch **with** a dev buy and three extra
//!   snipe-tax exemptions, its first taxed buy, and a fee sweep with the 30
//!   trades it paid out.
//! - `0036-pons-v2-clean-launch.json`: a launch with neither.
//!
//! The launch check must refuse the first for exactly its three exemptions --
//! its dev buy is allowed and named (ADR 0029) -- and pass the second. Each way a launch can be unclean is then
//! re-applied to the clean one, so a check that stopped looking would fail
//! here rather than on launch day.

use realorrug_robinhood::pons::{
    FACTORY, GET_LAUNCHED_TOKEN, Launched, LaunchedToken, Side, Sweep, Trade, Unclean,
    check_launch, dev_buys, topic,
};
use realorrug_robinhood::{Address, Log, Receipt, hex_bytes};

const DIRTY: &str = include_str!("../../../docs/research/data/0036-pons-v2-launch.json");
const CLEAN: &str = include_str!("../../../docs/research/data/0036-pons-v2-clean-launch.json");

fn read(capture: &str, why: &str) -> serde_json::Value {
    let value: serde_json::Value = serde_json::from_str(capture).expect("the capture is JSON");
    value["reads"]
        .as_array()
        .expect("reads")
        .iter()
        .find(|r| r["why"].as_str().is_some_and(|w| w.starts_with(why)))
        .unwrap_or_else(|| panic!("the capture has a read starting {why:?}"))
        .clone()
}

fn receipt(capture: &str, why: &str) -> Receipt {
    Receipt::from_json(&read(capture, why)["result"]).expect("the receipt parses")
}

fn call_return(capture: &str, why: &str) -> Vec<u8> {
    hex_bytes(read(capture, why)["result"].as_str().expect("hex")).expect("the return is hex")
}

fn record(capture: &str, why: &str) -> LaunchedToken {
    LaunchedToken::from_return(&call_return(capture, why)).expect("the record decodes")
}

fn addr(text: &str) -> Address {
    text.parse().expect("an address")
}

fn dirty() -> (Receipt, LaunchedToken) {
    (
        receipt(DIRTY, "the launch receipt"),
        record(DIRTY, "factory at the launch block: getLaunchedToken"),
    )
}

fn clean() -> (Receipt, LaunchedToken) {
    (
        receipt(CLEAN, "the launch receipt"),
        record(CLEAN, "factory: getLaunchedToken"),
    )
}

#[test]
fn the_constants_are_the_ones_mainnet_used() {
    // The selector is the first four bytes of the captured call's input.
    for (capture, why) in [
        (DIRTY, "factory at the launch block: getLaunchedToken"),
        (CLEAN, "factory: getLaunchedToken"),
    ] {
        let call = read(capture, why);
        let input = hex_bytes(call["params"][0]["data"].as_str().unwrap()).unwrap();
        assert_eq!(input[..4], GET_LAUNCHED_TOKEN);
        assert_eq!(addr(call["params"][0]["to"].as_str().unwrap()), FACTORY);
        let token = Address(input[16..36].try_into().expect("twenty bytes"));
        assert_eq!(LaunchedToken::call_data(&token), input);
    }

    // Each topic is carried by a log that decodes under it.
    let (launch, _) = dirty();
    let has = |r: &Receipt, t: realorrug_robinhood::Hash32| {
        r.logs.iter().filter(|l| l.topics[0] == t).count()
    };
    assert_eq!(has(&launch, topic::TOKEN_LAUNCHED), 1);
    assert_eq!(has(&launch, topic::TRANSFER), 2);
    assert_eq!(has(&launch, topic::SNIPE_TAX_EXEMPTED), 5);
    assert_eq!(has(&launch, topic::CURVE_BUY), 1);
    let sweep = receipt(DIRTY, "a fee sweep");
    assert_eq!(has(&sweep, topic::FEES_SWEPT), 1);
    let trades = sweep_trades();
    assert!(
        trades.iter().any(|t| t.side == Side::Sell),
        "a sell decodes"
    );
    assert!(trades.iter().any(|t| t.side == Side::Buy), "a buy decodes");
}

#[test]
fn the_captured_launch_decodes_to_what_research_0036_states() {
    let (launch, record) = dirty();
    assert!(launch.succeeded);
    assert_eq!(
        launch.to,
        Some(addr("0xe33e9e479df8802cb0866d5d05258bec4cf62948"))
    );
    let event = launch
        .logs
        .iter()
        .find_map(Launched::from_log)
        .expect("a launch");
    assert_eq!(
        event,
        Launched {
            token: addr("0x22fd486d80b7cce7362ffed59bbf2fd266a148fa"),
            curve: addr("0xddf3afb29e265b00c48015c3aacdedcb10088fcf"),
            deployer: addr("0x3f927788d627d3561ff59bbfef67ebcbf0fb10a9"),
            pair: None,
            config: 0,
            graduation_threshold: 4_200_000_000_000_000_000,
        }
    );
    assert_eq!(
        record,
        LaunchedToken {
            token: event.token,
            curve: event.curve,
            deployer: event.deployer,
            creator_fee_recipient: event.deployer,
            pair: None,
            graduation_threshold: 4_200_000_000_000_000_000,
            creator_tax_bps: 100,
            buyback: false,
            phase: 0,
            exists: true,
        }
    );
    let buy = launch
        .logs
        .iter()
        .find_map(Trade::from_log)
        .expect("the bundled buy");
    assert_eq!(
        buy,
        Trade {
            side: Side::Buy,
            curve: event.curve,
            trader: addr("0xe33e9e479df8802cb0866d5d05258bec4cf62948"),
            recipient: event.deployer,
            quote: 50_000_000_000_000_000,
            tokens: 28_340_080_971_659_919_028_340_080,
            fee: 500_000_000_000_000,
            tax: 500_000_000_000_000,
        }
    );
}

#[test]
fn the_launch_with_a_dev_buy_is_refused_for_its_exemptions_and_names_the_buy() {
    let (launch, record) = dirty();
    let deployer = addr("0x3f927788d627d3561ff59bbfef67ebcbf0fb10a9");
    let curve = addr("0xddf3afb29e265b00c48015c3aacdedcb10088fcf");
    assert_eq!(
        check_launch(&launch, &record),
        Err(vec![
            Unclean::Exempted(addr("0x7f5cf80c6075c06253cc8573881d9b76eda657d5")),
            Unclean::Exempted(addr("0x92f4e778076d006c3f30392284315fd3beeb3247")),
            Unclean::Exempted(addr("0x0e7502964aa30b2f7c4689eae8c7c3f5c109567d")),
        ])
    );
    let parsed = Launched::from_log(
        launch
            .logs
            .iter()
            .find(|l| l.is(&FACTORY, &topic::TOKEN_LAUNCHED))
            .expect("the launch"),
    )
    .expect("it decodes");
    let buys = dev_buys(&launch, &parsed, &record);
    assert_eq!(buys.len(), 1, "{buys:?}");
    assert_eq!(buys[0].curve, curve);
    assert_eq!(buys[0].recipient, deployer);
    assert_eq!(buys[0].tokens, 28_340_080_971_659_919_028_340_080);
}

#[test]
fn the_clean_launch_passes() {
    let (launch, record) = clean();
    assert_eq!(
        launch.to,
        Some(FACTORY),
        "launched on the factory directly, no router"
    );
    assert_eq!(record.creator_tax_bps, 200);
    assert_eq!(
        check_launch(&launch, &record),
        Ok(Launched {
            token: addr("0xb67f538fa1b65823aab5fdb4e923628f5c65943e"),
            curve: addr("0x1b45231650ca724fd3e98d7f3eb2569d4ddf0751"),
            deployer: addr("0x139f144b5187df68a1580ac614da02f0a04233a7"),
            pair: None,
            config: 0,
            graduation_threshold: 4_200_000_000_000_000_000,
        })
    );
}

/// The clean launch with `change` applied, checked.
fn clean_with(
    change: impl FnOnce(&mut Receipt, &mut LaunchedToken),
) -> Result<Launched, Vec<Unclean>> {
    let (mut launch, mut record) = clean();
    change(&mut launch, &mut record);
    check_launch(&launch, &record)
}

fn index_of(r: &Receipt, t: realorrug_robinhood::Hash32) -> usize {
    r.logs
        .iter()
        .position(|l| l.topics[0] == t)
        .expect("the log is there")
}

#[test]
fn a_buy_for_someone_else_added_to_the_clean_launch_is_refused() {
    let (dirty_launch, _) = dirty();
    let bundled: Log = dirty_launch.logs[index_of(&dirty_launch, topic::CURVE_BUY)].clone();
    let result = clean_with(|launch, _| {
        let mut buy = bundled;
        buy.address = launch.logs[index_of(launch, topic::SNIPE_TAX_EXEMPTED)].address;
        launch.logs.push(buy);
    });
    assert_eq!(
        result,
        Err(vec![Unclean::Traded {
            recipient: addr("0x3f927788d627d3561ff59bbfef67ebcbf0fb10a9"),
            tokens: 28_340_080_971_659_919_028_340_080,
        }])
    );

    // A sell is a trade too.
    let (_, sell_log) = sweep_logs()
        .into_iter()
        .find(|(t, _)| t.side == Side::Sell)
        .unwrap();
    let result = clean_with(|launch, _| {
        let mut sell = sell_log;
        sell.address = launch.logs[index_of(launch, topic::SNIPE_TAX_EXEMPTED)].address;
        launch.logs.push(sell);
    });
    assert!(
        matches!(
            result.as_ref().map_err(Vec::as_slice),
            Err([Unclean::Traded { .. }])
        ),
        "{result:?}"
    );
}

/// The dirty launch's buy, re-aimed at the clean launch's curve and paying
/// `recipient`, with the transfer that pays it when `paid` is `Some(amount)`.
fn launcher_buy(launch: &mut Receipt, recipient: Address, paid: Option<u128>) {
    let (dirty_launch, _) = dirty();
    let mut buy = dirty_launch.logs[index_of(&dirty_launch, topic::CURVE_BUY)].clone();
    let curve = launch.logs[index_of(launch, topic::SNIPE_TAX_EXEMPTED)].address;
    buy.address = curve;
    buy.topics[2] = word_of(recipient);
    if let Some(amount) = paid {
        let mut transfer = launch.logs[index_of(launch, topic::TRANSFER)].clone();
        transfer.topics[1] = word_of(curve);
        transfer.topics[2] = word_of(recipient);
        transfer.data = [[0u8; 16].as_slice(), &amount.to_be_bytes()].concat();
        launch.logs.push(transfer);
    }
    launch.logs.push(buy);
}

fn word_of(a: Address) -> realorrug_robinhood::Hash32 {
    let mut word = [0u8; 32];
    word[12..].copy_from_slice(&a.0);
    realorrug_robinhood::Hash32(word)
}

#[test]
fn the_launchers_own_buy_passes_only_when_its_tokens_arrive() {
    const TOKENS: u128 = 28_340_080_971_659_919_028_340_080;
    let (_, record) = clean();
    let deployer = addr("0x139f144b5187df68a1580ac614da02f0a04233a7");
    let curve = addr("0x1b45231650ca724fd3e98d7f3eb2569d4ddf0751");
    assert!(clean_with(|l, _| launcher_buy(l, deployer, Some(TOKENS))).is_ok());
    assert!(clean_with(|l, _| launcher_buy(l, record.creator_fee_recipient, Some(TOKENS))).is_ok());
    // Bought, never paid: refused as a trade, once.
    assert_eq!(
        clean_with(|l, _| launcher_buy(l, deployer, None)),
        Err(vec![Unclean::Traded {
            recipient: deployer,
            tokens: TOKENS,
        }])
    );
    // Paid the wrong amount: the transfer and the unpaid buy are both named.
    assert_eq!(
        clean_with(|l, _| launcher_buy(l, deployer, Some(TOKENS + 1))),
        Err(vec![
            Unclean::TokenMoved {
                from: curve,
                to: deployer,
                amount: Some(TOKENS + 1),
            },
            Unclean::Traded {
                recipient: deployer,
                tokens: TOKENS,
            },
        ])
    );
}

#[test]
fn a_paid_trade_passes_only_as_a_buy_for_the_launcher() {
    // Each trade here is paid in full, so only who and which way can refuse it.
    const TOKENS: u128 = 28_340_080_971_659_919_028_340_080;
    const QUOTE: u128 = 50_000_000_000_000_000;
    let deployer = addr("0x139f144b5187df68a1580ac614da02f0a04233a7");
    let curve = addr("0x1b45231650ca724fd3e98d7f3eb2569d4ddf0751");
    let stranger = Address([0x42; 20]);
    let fees = Address([0x43; 20]);

    assert_eq!(
        clean_with(|l, _| launcher_buy(l, stranger, Some(TOKENS))),
        Err(vec![
            Unclean::TokenMoved {
                from: curve,
                to: stranger,
                amount: Some(TOKENS),
            },
            Unclean::Traded {
                recipient: stranger,
                tokens: TOKENS,
            },
        ])
    );

    // A sell's first data word is its tokens, so the "payment" is QUOTE.
    let sold = clean_with(|l, _| {
        launcher_buy(l, deployer, Some(QUOTE));
        l.logs.last_mut().expect("the trade").topics[0] = topic::CURVE_SELL;
    });
    assert_eq!(
        sold,
        Err(vec![
            Unclean::TokenMoved {
                from: curve,
                to: deployer,
                amount: Some(QUOTE),
            },
            Unclean::Traded {
                recipient: deployer,
                tokens: QUOTE,
            },
        ])
    );

    // A fee recipient that is not the deployer: both may buy, nobody else.
    let with_fees = |who: Address| {
        clean_with(move |l, r| {
            r.creator_fee_recipient = fees;
            launcher_buy(l, who, Some(TOKENS));
        })
    };
    assert!(with_fees(fees).is_ok());
    assert!(with_fees(deployer).is_ok());
    assert!(with_fees(stranger).is_err());
}

#[test]
fn a_trade_on_some_other_curve_is_not_this_launchs_business() {
    let (dirty_launch, _) = dirty();
    let other: Log = dirty_launch.logs[index_of(&dirty_launch, topic::CURVE_BUY)].clone();
    assert!(clean_with(|launch, _| launch.logs.push(other)).is_ok());
}

#[test]
fn an_extra_exemption_is_refused_and_the_fee_recipients_is_not() {
    let stranger = Address([0x42; 20]);
    let exempt = |who: Address| {
        move |launch: &mut Receipt, _: &mut LaunchedToken| {
            let mut log = launch.logs[index_of(launch, topic::SNIPE_TAX_EXEMPTED)].clone();
            let mut word = [0u8; 32];
            word[12..].copy_from_slice(&who.0);
            log.topics[1] = realorrug_robinhood::Hash32(word);
            launch.logs.push(log);
        }
    };
    assert_eq!(
        clean_with(exempt(stranger)),
        Err(vec![Unclean::Exempted(stranger)])
    );

    // A fee recipient other than the deployer is exempted by the factory itself.
    let result = clean_with(|launch, record| {
        record.creator_fee_recipient = stranger;
        exempt(stranger)(launch, record);
    });
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn a_mint_anywhere_but_the_curve_is_refused() {
    let elsewhere = Address([0x42; 20]);
    let result = clean_with(|launch, _| {
        let i = index_of(launch, topic::TRANSFER);
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(&elsewhere.0);
        launch.logs[i].topics[2] = realorrug_robinhood::Hash32(word);
    });
    assert_eq!(
        result,
        Err(vec![
            Unclean::NoMint,
            Unclean::TokenMoved {
                from: Address::ZERO,
                to: elsewhere,
                amount: Some(1_000_000_000_000_000_000_000_000_000),
            },
        ])
    );

    // A second mint to the curve is a second transfer, not a bigger first one.
    let result = clean_with(|launch, _| {
        let mint = launch.logs[index_of(launch, topic::TRANSFER)].clone();
        launch.logs.push(mint);
    });
    assert!(
        matches!(
            result.as_ref().map_err(Vec::as_slice),
            Err([Unclean::TokenMoved { .. }])
        ),
        "{result:?}"
    );

    // A mint whose amount does not read is not a mint this check can vouch for.
    let result = clean_with(|launch, _| {
        let i = index_of(launch, topic::TRANSFER);
        launch.logs[i].data.clear();
    });
    assert_eq!(
        result,
        Err(vec![
            Unclean::NoMint,
            Unclean::TokenMoved {
                from: Address::ZERO,
                to: addr("0x1b45231650ca724fd3e98d7f3eb2569d4ddf0751"),
                amount: None,
            },
        ])
    );

    // A transfer to the curve from anyone but the zero address is not a mint.
    let result = clean_with(|launch, _| {
        let i = index_of(launch, topic::TRANSFER);
        launch.logs[i].topics[1] = realorrug_robinhood::Hash32([0; 32]);
        launch.logs[i].topics[1].0[31] = 9;
    });
    assert!(
        matches!(
            result.as_ref().map_err(Vec::as_slice),
            Err([Unclean::NoMint, Unclean::TokenMoved { .. }])
        ),
        "{result:?}"
    );

    // No mint at all.
    let result = clean_with(|launch, _| {
        let i = index_of(launch, topic::TRANSFER);
        launch.logs.remove(i);
    });
    assert_eq!(result, Err(vec![Unclean::NoMint]));
}

#[test]
fn a_transfer_of_some_other_token_is_not_this_launchs_business() {
    let result = clean_with(|launch, _| {
        let mut other = launch.logs[index_of(launch, topic::TRANSFER)].clone();
        other.address = Address([0x42; 20]);
        launch.logs.push(other);
    });
    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn a_launch_that_is_not_one_launch_from_the_factory_is_refused() {
    assert_eq!(
        clean_with(|launch, _| launch.succeeded = false),
        Err(vec![Unclean::Failed])
    );
    assert_eq!(
        clean_with(|launch, _| {
            let i = index_of(launch, topic::TOKEN_LAUNCHED);
            launch.logs[i].address = Address([0x42; 20]);
        }),
        Err(vec![Unclean::NoLaunch]),
        "the same event from an impostor factory launches nothing"
    );
    assert_eq!(
        clean_with(|launch, _| {
            let i = index_of(launch, topic::TOKEN_LAUNCHED);
            let twice = launch.logs[i].clone();
            launch.logs.push(twice);
        }),
        Err(vec![Unclean::ManyLaunches(2)])
    );
    assert_eq!(
        clean_with(|launch, _| {
            let i = index_of(launch, topic::TOKEN_LAUNCHED);
            launch.logs[i].data.truncate(64);
        }),
        Err(vec![Unclean::NoLaunch])
    );
    assert_eq!(
        clean_with(|_, record| record.curve = Address([0x42; 20])),
        Err(vec![Unclean::RecordMismatch])
    );
    assert_eq!(
        clean_with(|_, record| record.token = Address([0x42; 20])),
        Err(vec![Unclean::RecordMismatch])
    );
}

#[test]
fn a_curve_log_that_does_not_decode_is_refused_not_skipped() {
    let result = clean_with(|launch, _| {
        let i = index_of(launch, topic::SNIPE_TAX_EXEMPTED);
        let mut bad = launch.logs[i].clone();
        bad.topics.truncate(1);
        launch.logs.push(bad);
    });
    assert_eq!(
        result,
        Err(vec![Unclean::Unreadable(topic::SNIPE_TAX_EXEMPTED)])
    );

    let (dirty_launch, _) = dirty();
    let bundled = dirty_launch.logs[index_of(&dirty_launch, topic::CURVE_BUY)].clone();
    let result = clean_with(|launch, _| {
        let mut bad = bundled;
        bad.address = launch.logs[index_of(launch, topic::SNIPE_TAX_EXEMPTED)].address;
        bad.data.truncate(32);
        launch.logs.push(bad);
    });
    assert_eq!(result, Err(vec![Unclean::Unreadable(topic::CURVE_BUY)]));

    let result = clean_with(|launch, _| {
        let i = index_of(launch, topic::TRANSFER);
        launch.logs[i].topics.truncate(2);
    });
    assert_eq!(
        result,
        Err(vec![Unclean::NoMint, Unclean::Unreadable(topic::TRANSFER)])
    );
}

/// The trades the captured sweep paid out, with their logs.
fn sweep_logs() -> Vec<(Trade, Log)> {
    read(DIRTY, "that curve's trades and sweeps")["result"]
        .as_array()
        .expect("logs")
        .iter()
        .map(|v| Log::from_json(v).expect("a log"))
        .filter_map(|l| Trade::from_log(&l).map(|t| (t, l)))
        .collect()
}

fn sweep_trades() -> Vec<Trade> {
    sweep_logs().into_iter().map(|(t, _)| t).collect()
}

#[test]
fn the_sweep_pays_what_the_measured_split_predicts() {
    let sweep_receipt = receipt(DIRTY, "a fee sweep");
    let curve = addr("0x36f815a2d12ad8016eb93065a0a5a23708da1aa2");
    let paid = sweep_receipt
        .logs
        .iter()
        .filter(|l| l.address == curve)
        .find_map(Sweep::from_log)
        .expect("the sweep");
    let trades = sweep_trades();
    assert_eq!(trades.len(), 30);
    assert!(trades.iter().all(|t| t.curve == curve));
    let share = call_return(
        DIRTY,
        "that curve at the sweep block: protocolFeeShareBps()",
    );
    assert_eq!(
        realorrug_robinhood::word(&share, 0).and_then(realorrug_robinhood::word_u128),
        Some(3_000)
    );
    assert_eq!(
        paid,
        Sweep {
            protocol: 5_086_595_949_856_946,
            buyback: 0,
            creator: 28_824_043_715_856_030,
        }
    );
    assert_eq!(Sweep::expected(&trades, 3_000), Some(paid));
    // A different share predicts a different sweep: the split is not a coincidence of zeros.
    assert_ne!(Sweep::expected(&trades, 2_500), Some(paid));
    assert_ne!(Sweep::expected(&trades[1..], 3_000), Some(paid));
}

#[test]
fn a_prediction_that_cannot_be_right_is_not_made() {
    let trade = |fee, tax| Trade {
        side: Side::Buy,
        curve: Address::ZERO,
        trader: Address::ZERO,
        recipient: Address::ZERO,
        quote: 0,
        tokens: 0,
        fee,
        tax,
    };
    assert_eq!(
        Sweep::expected(&[], 3_000),
        Some(Sweep {
            protocol: 0,
            buyback: 0,
            creator: 0
        })
    );
    assert_eq!(
        Sweep::expected(&[trade(10_001, 7)], 10_000),
        Some(Sweep {
            protocol: 10_001,
            buyback: 0,
            creator: 7
        })
    );
    assert_eq!(
        Sweep::expected(&[trade(10_001, 7)], 3_333),
        Some(Sweep {
            protocol: 3_333,
            buyback: 0,
            creator: 6_668 + 7
        }),
        "the protocol's share rounds down; the creator gets the remainder"
    );
    assert_eq!(Sweep::expected(&[trade(1, 0)], 10_001), None);
    assert_eq!(
        Sweep::expected(&[trade(u128::MAX, 0), trade(1, 0)], 0),
        None
    );
    assert_eq!(
        Sweep::expected(&[trade(0, u128::MAX), trade(0, 1)], 0),
        None
    );
    assert_eq!(Sweep::expected(&[trade(u128::MAX, 0)], 2), None);
    assert_eq!(
        Sweep::expected(&[trade(0, u128::MAX)], 0),
        Some(Sweep {
            protocol: 0,
            buyback: 0,
            creator: u128::MAX
        })
    );
    assert_eq!(
        Sweep::expected(&[trade(1, u128::MAX)], 0),
        None,
        "fee plus tax overflows"
    );
}

#[test]
fn a_record_that_does_not_decode_is_none() {
    let good = call_return(CLEAN, "factory: getLaunchedToken");
    assert!(LaunchedToken::from_return(&good).is_some());
    assert_eq!(
        LaunchedToken::from_return(&good[..32 * 14]),
        None,
        "fifteen words"
    );
    for (word, value) in [(9, 2u8), (14, 2)] {
        let mut bad = good.clone();
        bad[32 * word + 31] = value;
        assert_eq!(
            LaunchedToken::from_return(&bad),
            None,
            "a flag of {value} in word {word}"
        );
    }
    let mut not_exists = good.clone();
    not_exists[32 * 14 + 31] = 0;
    assert!(!LaunchedToken::from_return(&not_exists).unwrap().exists);
    let mut buyback = good.clone();
    buyback[32 * 9 + 31] = 1;
    assert!(LaunchedToken::from_return(&buyback).unwrap().buyback);
    let mut big_tax = good.clone();
    big_tax[32 * 8 + 29] = 1;
    assert_eq!(
        LaunchedToken::from_return(&big_tax),
        None,
        "a tax beyond u16"
    );
    let mut pair = good;
    pair[32 * 4 + 31] = 1;
    assert_eq!(
        LaunchedToken::from_return(&pair).unwrap().pair,
        Some(Address({
            let mut a = [0; 20];
            a[19] = 1;
            a
        }))
    );
}

#[test]
fn events_of_the_wrong_kind_do_not_decode() {
    let (launch, _) = dirty();
    let mint = &launch.logs[index_of(&launch, topic::TRANSFER)];
    assert_eq!(Launched::from_log(mint), None);
    assert_eq!(Trade::from_log(mint), None);
    assert_eq!(Sweep::from_log(mint), None);
    let mut paired = launch.logs[index_of(&launch, topic::TOKEN_LAUNCHED)].clone();
    paired.data[31] = 7;
    assert_eq!(
        Launched::from_log(&paired).map(|l| l.pair),
        Some(Some(Address({
            let mut a = [0; 20];
            a[19] = 7;
            a
        })))
    );
}
