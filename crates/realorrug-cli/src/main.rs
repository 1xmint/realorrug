// SPDX-License-Identifier: Apache-2.0
//! `realorrug` — the operator's commands for the public analyst.
//!
//! Every command here either reads (the chain, the reply log, the journal) or
//! prints something for a human to act on. None of them posts, and none signs:
//! `contest pay` plans an unsigned transaction and refuses without `--dry-run`.

mod analyst;
mod audit;
mod contest;
mod dossier;
mod model_prices;
mod roast;

use std::process::ExitCode;

/// The command list.
///
/// At module scope rather than inside [`usage`], because the length lint counts
/// lines in a function and this is a string that grows every time a command is
/// added.
const USAGE: &str = "realorrug <command>

commands:
  dossier <mint> [--rpc URL] [--seconds N]
                                 everything the bot can say about one token,
                                 read from the chain on demand. Read-only,
                                 holds no key
  roast <mint> [--rpc URL] [--rates PATH] [--sheet]
                                 the reply the public analyst would post, built
                                 from the dossier and the published base rates.
                                 A check after generation refuses any number
                                 that is not on the fact sheet, and the
                                 deterministic template ships instead. Prints;
                                 never posts
  analyst --mentions <file.jsonl> [--log <file>]
                                 the whole summoned-reply loop over mentions
                                 from a file: strict parse, admission gate,
                                 dossier, reply, log. Dry run -- it holds no
                                 credential and posts nothing
  contest <pay --dry-run | record-payout --signature <sig> | void --reason <word>> --week N
                                 the payout's manual fallback, through the same
                                 check the automated payout uses
  model-prices <model> [--check] | --list
                                 what to paste into analyst.env for a model,
                                 read from models.dev rather than typed
  audit explain --id <id> [--journal <file>]
                                 everything the journal holds about one mention,
                                 mint, week, claim or signature, in order
  audit verify [--journal <file>] [--from N] [--to N]
                                 walk the chain: intact, torn, or broken
  audit export --week <week> [--journal <file>]
                                 that week's events as JSON
";

fn usage() -> &'static str {
    USAGE
}

/// The value following `name`, if it is there.
///
/// Shared rather than reimplemented per command: a copy that took `i - 1`
/// would silently read the *previous* argument, so `--mint X --wallet Y` would
/// read the mint as `--mint`.
pub(crate) fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        eprint!("{}", usage());
        return ExitCode::FAILURE;
    };

    let result = match command.as_str() {
        "contest" => contest::run(&args),
        "dossier" => dossier::run(&args),
        "roast" => roast::run(&args),
        "analyst" => analyst::run(&args),
        "audit" => audit::run(&args),
        "model-prices" => model_prices::run(&args),
        "-h" | "--help" | "help" => {
            print!("{}", usage());
            return ExitCode::SUCCESS;
        }
        other => Err(format!("unknown command {other}\n\n{}", usage())),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::flag;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_flag_takes_the_value_after_it_not_before() {
        let a = args(&["dossier", "--mint", "X", "--wallet", "Y"]);
        assert_eq!(flag(&a, "--mint").as_deref(), Some("X"));
        assert_eq!(flag(&a, "--wallet").as_deref(), Some("Y"));
    }

    #[test]
    fn a_flag_with_nothing_after_it_is_absent() {
        assert_eq!(flag(&args(&["roast", "--rpc"]), "--rpc"), None);
        assert_eq!(flag(&args(&["roast"]), "--rpc"), None);
    }
}
