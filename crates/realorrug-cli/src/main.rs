// SPDX-License-Identifier: Apache-2.0
//! `realorrug` — the operator's commands for the public analyst.
//!
//! Every command here either reads (the chain, the reply log, the journal) or
//! prints something for a human to act on. None of them posts, and none signs:
//! `contest pay` plans an unsigned transaction and refuses without `--dry-run`.

mod analyst;
mod audit;
mod bio;
mod capture;
mod contest;
mod creator_index;
mod dossier;
mod label_outcomes;
mod launch_check;
mod model_prices;
mod narratives;
mod record_launches;
mod replay;
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
  roast <mint> [--rpc URL] [--robinhood-rpc URL] [--rates PATH] [--sheet]
                                 the reply the public analyst would post, built
                                 from the dossier and the published base rates.
                                 A check after generation refuses any number
                                 that is not on the fact sheet, and the
                                 deterministic template ships instead. Prints;
                                 never posts
  capture <mint> --out <dir> [--rpc URL] [--label NAME]
                                 reads a mint the way `roast` does and freezes
                                 the fact sheet, the read time, the
                                 deterministic level/assessment and the rules
                                 version to <dir>/<mint>.sheet.json. No model
                                 call, no reply, and nothing is scored twice by
                                 a later `replay`. Read-only
  replay <dir> [--model]        for every <dir>/*.sheet.json, recomputes the
                                 level, the report and the reply offline, with
                                 no chain read -- the deterministic template
                                 by default, the configured provider only with
                                 `--model`. Runs the fidelity, forbidden and
                                 unknown-data checks against both the reply and
                                 the report, and writes <dir>/review.md with a
                                 blank accept line per case
  analyst --mentions <file.jsonl> [--log <file>] [--robinhood-rpc URL]
                                 the whole summoned-reply loop over mentions
                                 from a file: strict parse, admission gate,
                                 dossier, reply, log. Dry run -- it holds no
                                 credential and posts nothing
  contest <pay --dry-run | record-payout --signature <sig> | void --reason <word>> --week N
                                 the payout's manual fallback, through the same
                                 check the automated payout uses
  creator-index --rpc URL --out PATH [--from N] [--to N] [--verify N]
                [--base-rates-out PATH]
                                 who has launched on Robinhood Chain and how
                                 many times, walked out of the Pons v2
                                 factory's own launch events. Writes creator
                                 outcomes and Robinhood population/24h rates
                                 (default docs/research/data/0051-robinhood-base-rates.json).
                                 Read-only chain access
  launch-check --tx <hash> --rpc URL
                                 whether a Pons v2 launch on Robinhood Chain
                                 is clean (ADR 0029): the mint to the curve,
                                 no trade but the launcher's own stated buy,
                                 no extra snipe-tax exemption. Read-only
  label-outcomes [--robinhood-rpc URL] [--memory PATH] [--days N] [--max N]
                 [--dry-run]
                                 what the launches this analyst already judged
                                 turned out to be: rug, failed or alive, read
                                 from the chain and written beside the verdict
                                 that was published at the time. One verdict
                                 plus one outcome is one calibration sample
                                 (research 0052 §5). A launch this reading
                                 cannot settle is left unlabelled, never
                                 guessed. Read-only chain access
  narratives [--memory PATH] [--days N] [--min N] [--top N] [--by-volume]
                                 which words recent launches share, counted
                                 from the names already stored when each
                                 dossier was read, beside what those launches
                                 traded, from the aggregator answers stored
                                 the same way, beside how many different
                                 people used the word in a question. No chain
                                 read, no new HTTP call, no model: a count of
                                 names, a sum of readings and a count of
                                 people, never a claim that the launches
                                 sharing a word are related. `--by-volume`
                                 orders by dollars rather than by how many
                                 launchers picked the word
  record-launches --rpc URL [--memory PATH] [--from N] [--to N] [--max N]
                                 every new Pons v2 launch's name and symbol,
                                 written to the same `token_texts` table a
                                 served dossier writes to, so `narratives`
                                 counts what gets launched rather than only
                                 what a reader happened to ask about. Resumes
                                 from a stored cursor; an unreadable name
                                 still writes a row (rule 8). Read-only chain
                                 access, holds no key, posts nothing
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
  bio --preview [--lead TEXT] [--pool AMOUNT] [--hunters N]
      [--leader HANDLE]... [--last-winner HANDLE]
                                 the exact bio text `write_bio_if_changed`
                                 would post for a sample, and its length.
                                 Writes nothing
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

/// Opens a memory file that is already there, and refuses one that is not.
///
/// These commands are run by hand, from whatever folder the shell is in.
/// Opening would quietly create an empty file under the wrong folder and
/// report "nothing found" -- a wrong answer that reads like a quiet week.
/// Refusing names the file it looked for instead.
pub(crate) fn open_memory(path: &str) -> Result<realorrug_onchain::memory::Memory, String> {
    let file = std::path::Path::new(path);
    if !file.is_file() {
        return Err(format!("cannot open {path}: no memory file there"));
    }
    realorrug_onchain::memory::Memory::open(file).map_err(|e| format!("cannot open {path}: {e}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        eprint!("{}", usage());
        return ExitCode::FAILURE;
    };

    let result = match command.as_str() {
        "bio" => bio::run(&args),
        "capture" => capture::run(&args),
        "contest" => contest::run(&args),
        "dossier" => dossier::run(&args),
        "replay" => replay::run(&args),
        "roast" => roast::run(&args),
        "analyst" => analyst::run(&args),
        "audit" => audit::run(&args),
        "creator-index" => creator_index::run(&args),
        "launch-check" => launch_check::run(&args),
        "label-outcomes" => label_outcomes::run(&args),
        "model-prices" => model_prices::run(&args),
        "narratives" => narratives::run(&args),
        "record-launches" => record_launches::run(&args),
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
    use super::{flag, open_memory};

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

    /// A folder with no memory file in it is refused, and stays empty.
    #[test]
    fn a_memory_that_is_not_there_is_not_made() {
        let dir = std::env::temp_dir().join("realorrug-cli-no-memory");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("memory.sqlite3");
        let Err(err) = open_memory(path.to_str().expect("path")) else {
            panic!("an absent memory was opened");
        };
        assert!(err.ends_with("no memory file there"), "{err}");
        assert!(!path.exists());
    }

    /// A memory file that is there is opened.
    #[test]
    fn a_memory_that_is_there_is_opened() {
        let dir = std::env::temp_dir().join("realorrug-cli-some-memory");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("memory.sqlite3");
        drop(realorrug_onchain::memory::Memory::open(&path).expect("make"));
        assert!(open_memory(path.to_str().expect("path")).is_ok());
    }
}
