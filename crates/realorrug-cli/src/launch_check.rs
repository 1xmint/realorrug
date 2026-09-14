// SPDX-License-Identifier: Apache-2.0
//! `realorrug launch-check`: whether a Pons v2 launch meets ADR 0013 constraint 1.
//!
//! `launch-check --tx <hash> --rpc <url>` reads the launch transaction's receipt
//! and the factory's record of the token, and prints either the launch it
//! verified or every reason it is not clean. Read-only: no key, nothing signed.
//!
//! This is the instrument for launch day. The token is launched, this is run on
//! its transaction, and what it prints is what the analyst may say about the
//! launch block -- or the reason the launch has to be disowned.

use realorrug_robinhood::pons::{FACTORY, Launched, LaunchedToken, Unclean, check_launch};
use realorrug_robinhood::{Hash32, Rpc};

/// Runs the command.
///
/// # Errors
///
/// A message when a flag is missing, the chain cannot be read, the transaction
/// is not in a block, or the launch is not clean.
pub fn run(args: &[String]) -> Result<(), String> {
    let tx: Hash32 = crate::flag(args, "--tx")
        .ok_or("--tx <hash> is required")?
        .parse()
        .map_err(|e| format!("--tx: {e}"))?;
    // No default endpoint (rule 7): the public one is rate-limited, and
    // picking it silently would be picking for the operator.
    let rpc = Rpc::new(crate::flag(args, "--rpc").ok_or("--rpc <url> is required")?);
    let receipt = rpc
        .receipt(&tx)?
        .ok_or_else(|| format!("{tx} is not in a block yet"))?;
    let token = receipt
        .logs
        .iter()
        .find_map(Launched::from_log)
        .map(|l| l.token);
    let Some(token) = token else {
        return Err(report(&tx, &Err(vec![Unclean::NoLaunch]), None));
    };
    let record = LaunchedToken::from_return(
        &rpc.call_contract(&FACTORY, &LaunchedToken::call_data(&token))?,
    )
    .ok_or("the factory's record of the token did not decode")?;
    let verdict = check_launch(&receipt, &record);
    let text = report(&tx, &verdict, Some(&record));
    if verdict.is_ok() {
        print!("{text}");
        Ok(())
    } else {
        Err(text)
    }
}

/// What the operator reads.
pub(crate) fn report(
    tx: &Hash32,
    verdict: &Result<Launched, Vec<Unclean>>,
    record: Option<&LaunchedToken>,
) -> String {
    let mut lines = match verdict {
        Ok(launch) => vec![
            format!("CLEAN launch {tx}"),
            format!("  token     {}", launch.token),
            format!("  curve     {}", launch.curve),
            format!("  deployer  {}", launch.deployer),
            format!(
                "  pair      {}",
                launch.pair.map_or_else(|| "ETH".to_owned(), |p| p.to_string())
            ),
            "  the mint went only to the curve; no trade, no other transfer, no extra snipe-tax exemption"
                .to_owned(),
        ],
        Err(reasons) => std::iter::once(format!("NOT CLEAN: launch {tx}"))
            .chain(reasons.iter().map(|r| format!("  - {}", describe(r))))
            .collect(),
    };
    if let Some(r) = record {
        lines.push(format!(
            "  creator tax {} bps, fees to {}, buyback {}",
            r.creator_tax_bps,
            r.creator_fee_recipient,
            if r.buyback { "on" } else { "off" }
        ));
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn describe(reason: &Unclean) -> String {
    match reason {
        Unclean::Failed => "the transaction reverted".to_owned(),
        Unclean::NoLaunch => "no launch from the Pons v2 factory".to_owned(),
        Unclean::ManyLaunches(n) => format!("{n} launches in one transaction"),
        Unclean::RecordMismatch => {
            "the factory's record names a different token or curve".to_owned()
        }
        Unclean::NoMint => "the token was not minted to its curve".to_owned(),
        Unclean::TokenMoved { from, to, amount } => format!(
            "tokens moved {from} -> {to}: {}",
            amount.map_or_else(|| "an unreadable amount".to_owned(), |a| a.to_string())
        ),
        Unclean::Traded { recipient, tokens } => {
            format!(
                "a trade on the curve in the launch transaction: {tokens} tokens to {recipient}"
            )
        }
        Unclean::Exempted(who) => format!("{who} exempted from the snipe tax"),
        Unclean::Unreadable(event) => format!("a log with event {event} did not decode"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use realorrug_robinhood::Address;

    #[test]
    fn a_clean_launch_says_so_and_names_what_it_verified() {
        let launch = Launched {
            token: Address([1; 20]),
            curve: Address([2; 20]),
            deployer: Address([3; 20]),
            pair: None,
            config: 0,
            graduation_threshold: 0,
        };
        let record = LaunchedToken {
            token: launch.token,
            curve: launch.curve,
            deployer: launch.deployer,
            creator_fee_recipient: Address([4; 20]),
            pair: None,
            graduation_threshold: 0,
            creator_tax_bps: 200,
            buyback: false,
            exists: true,
        };
        let tx = Hash32([9; 32]);
        let text = report(&tx, &Ok(launch.clone()), Some(&record));
        assert!(text.starts_with(&format!("CLEAN launch {tx}\n")), "{text}");
        assert!(text.contains(&format!("token     {}", Address([1; 20]))));
        assert!(text.contains(&format!("curve     {}", Address([2; 20]))));
        assert!(text.contains(&format!("deployer  {}", Address([3; 20]))));
        assert!(text.contains("pair      ETH"));
        assert!(text.contains(&format!(
            "creator tax 200 bps, fees to {}, buyback off",
            Address([4; 20])
        )));
        let paired = Launched {
            pair: Some(Address([5; 20])),
            ..launch
        };
        let on = LaunchedToken {
            buyback: true,
            ..record
        };
        let text = report(&tx, &Ok(paired), Some(&on));
        assert!(text.contains(&format!("pair      {}", Address([5; 20]))));
        assert!(text.contains("buyback on"));
        assert!(!report(&tx, &Err(vec![]), None).contains("creator tax"));
    }

    #[test]
    fn an_unclean_launch_lists_every_reason() {
        let a = Address([7; 20]);
        let reasons = vec![
            Unclean::Failed,
            Unclean::NoLaunch,
            Unclean::ManyLaunches(2),
            Unclean::RecordMismatch,
            Unclean::NoMint,
            Unclean::TokenMoved {
                from: a,
                to: a,
                amount: Some(5),
            },
            Unclean::TokenMoved {
                from: a,
                to: a,
                amount: None,
            },
            Unclean::Traded {
                recipient: a,
                tokens: 6,
            },
            Unclean::Exempted(a),
            Unclean::Unreadable(Hash32([8; 32])),
        ];
        let text = report(&Hash32([9; 32]), &Err(reasons.clone()), None);
        assert!(text.starts_with("NOT CLEAN: launch "), "{text}");
        let lines: Vec<&str> = text.lines().skip(1).collect();
        assert_eq!(lines.len(), reasons.len());
        for (line, reason) in lines.iter().zip(&reasons) {
            assert_eq!(*line, format!("  - {}", describe(reason)));
        }
        let expected = [
            "the transaction reverted".to_owned(),
            "no launch from the Pons v2 factory".to_owned(),
            "2 launches in one transaction".to_owned(),
            "the factory's record names a different token or curve".to_owned(),
            "the token was not minted to its curve".to_owned(),
            format!("tokens moved {a} -> {a}: 5"),
            format!("tokens moved {a} -> {a}: an unreadable amount"),
            format!("a trade on the curve in the launch transaction: 6 tokens to {a}"),
            format!("{a} exempted from the snipe tax"),
            format!("a log with event {} did not decode", Hash32([8; 32])),
        ];
        for (reason, want) in reasons.iter().zip(expected) {
            assert_eq!(describe(reason), want);
        }
    }

    // The command end to end, against a loopback server replaying mainnet's
    // captured answers. The same small server is in `realorrug-robinhood`'s
    // `tests/rpc_over_http.rs`; it is copied rather than shared because
    // sharing it would put test code in a library a key-holding binary links.

    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    const DIRTY: &str = include_str!("../../../docs/research/data/0036-pons-v2-launch.json");
    const CLEAN: &str = include_str!("../../../docs/research/data/0036-pons-v2-clean-launch.json");

    fn serve(bodies: Vec<String>) -> (String, Arc<Mutex<Vec<serde_json::Value>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let url = format!("http://{}", listener.local_addr().expect("an address"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        std::thread::spawn(move || {
            for body in bodies {
                let (stream, _) = listener.accept().expect("a connection");
                let mut reader = BufReader::new(stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).expect("a header line");
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().expect("a length");
                    }
                }
                let mut request = vec![0; length];
                reader.read_exact(&mut request).expect("the body");
                log.lock()
                    .expect("the log")
                    .push(serde_json::from_slice(&request).expect("JSON"));
                let mut stream = reader.into_inner();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("the response");
            }
        });
        (url, seen)
    }

    fn result(capture: &str, why: &str) -> serde_json::Value {
        let value: serde_json::Value = serde_json::from_str(capture).expect("JSON");
        value["reads"]
            .as_array()
            .expect("reads")
            .iter()
            .find(|r| r["why"].as_str().is_some_and(|w| w.starts_with(why)))
            .expect("the read")["result"]
            .clone()
    }

    fn answer(result: &serde_json::Value) -> String {
        serde_json::json!({ "jsonrpc": "2.0", "id": 1, "result": result }).to_string()
    }

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("launch-check")
            .chain(list.iter().copied())
            .map(ToOwned::to_owned)
            .collect()
    }

    const CLEAN_TX: &str = "0x2a43738c97c45c2f1e9a4b4941a8e37ba543b5b1f809e648e294cab50efa59ce";
    const DIRTY_TX: &str = "0x1013a302930cfbc10d55c2fec9cd9e93670f516933ea6a907081ee928cfcf8b7";

    #[test]
    fn the_clean_launch_passes_and_the_factory_is_asked_about_its_token() {
        let (url, seen) = serve(vec![
            answer(&result(CLEAN, "the launch receipt")),
            answer(&result(CLEAN, "factory: getLaunchedToken")),
        ]);
        assert_eq!(run(&args(&["--tx", CLEAN_TX, "--rpc", &url])), Ok(()));
        let requests = seen.lock().expect("the log").clone();
        assert_eq!(requests[0]["params"], serde_json::json!([CLEAN_TX]));
        let token: realorrug_robinhood::Address = "0xb67f538fa1b65823aab5fdb4e923628f5c65943e"
            .parse()
            .expect("an address");
        let data: String = std::iter::once("0x".to_owned())
            .chain(
                LaunchedToken::call_data(&token)
                    .iter()
                    .map(|b| format!("{b:02x}")),
            )
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(requests[1]["params"][0]["data"], data.as_str());
        assert_eq!(requests[1]["params"][0]["to"], FACTORY.to_string());
    }

    #[test]
    fn the_launch_with_a_dev_buy_is_refused_with_its_reasons() {
        let (url, _) = serve(vec![
            answer(&result(DIRTY, "the launch receipt")),
            answer(&result(
                DIRTY,
                "factory at the launch block: getLaunchedToken",
            )),
        ]);
        let err = run(&args(&["--tx", DIRTY_TX, "--rpc", &url])).expect_err("refused");
        assert!(
            err.starts_with(&format!("NOT CLEAN: launch {DIRTY_TX}\n")),
            "{err}"
        );
        assert!(
            err.contains("a trade on the curve in the launch transaction"),
            "{err}"
        );
        assert!(err.contains("creator tax 100 bps"), "{err}");
    }

    #[test]
    fn a_transaction_with_no_launch_is_refused_without_asking_the_factory() {
        let mut receipt = result(CLEAN, "the launch receipt");
        receipt["logs"] = serde_json::json!([]);
        let (url, seen) = serve(vec![answer(&receipt)]);
        let err = run(&args(&["--tx", CLEAN_TX, "--rpc", &url])).expect_err("refused");
        assert_eq!(
            err,
            format!("NOT CLEAN: launch {CLEAN_TX}\n  - no launch from the Pons v2 factory\n")
        );
        assert_eq!(seen.lock().expect("the log").len(), 1);
    }

    #[test]
    fn a_transaction_not_in_a_block_or_a_record_that_does_not_decode_is_an_error() {
        let (url, _) = serve(vec![answer(&serde_json::Value::Null)]);
        assert_eq!(
            run(&args(&["--tx", CLEAN_TX, "--rpc", &url])),
            Err(format!("{CLEAN_TX} is not in a block yet"))
        );
        let (url, _) = serve(vec![
            answer(&result(CLEAN, "the launch receipt")),
            answer(&serde_json::json!("0x00")),
        ]);
        assert_eq!(
            run(&args(&["--tx", CLEAN_TX, "--rpc", &url])),
            Err("the factory's record of the token did not decode".to_owned())
        );
    }

    #[test]
    fn flags_are_required_and_the_hash_must_be_one() {
        assert_eq!(
            run(&args(&["--rpc", "http://127.0.0.1:1"])),
            Err("--tx <hash> is required".to_owned())
        );
        assert_eq!(
            run(&args(&["--tx", CLEAN_TX])),
            Err("--rpc <url> is required".to_owned())
        );
        let err = run(&args(&["--tx", "0x12", "--rpc", "http://127.0.0.1:1"])).expect_err("bad");
        assert!(err.starts_with("--tx: "), "{err}");
    }
}
