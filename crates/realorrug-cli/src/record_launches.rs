// SPDX-License-Identifier: Apache-2.0
//! `realorrug record-launches` — writes the name and symbol of every new
//! Pons v2 launch into `token_texts`, so `realorrug narratives` counts what
//! gets launched, not only what a reader happened to ask about.
//!
//! # The bias this closes
//!
//! `crates/realorrug-serve/src/record.rs`'s `token_text` writes a row only
//! when a route serves a dossier -- which only happens for a launch someone
//! asked about, or one the bot chose to reply to. A launch nobody mentioned
//! never gets a dossier, so it never gets a row, and a narrative count built
//! only from `token_texts` would be a count of what people asked about
//! wearing the clothes of a count of what launched. This command is the
//! other half: it walks the factory's own `TokenLaunched` log, which has no
//! opinion about who is watching, and writes every launch it sees. The two
//! writers share one table and one dedupe rule (`INSERT OR IGNORE`, first
//! reading wins), so nothing downstream has to know which one wrote a row.
//!
//! # Where it resumes
//!
//! Each run's finished range is remembered in `memory.rs`'s `walk_cursors`
//! table under the name [`CURSOR_NAME`], so a second run starts after the
//! first's last finished block instead of re-walking the whole chain. With
//! no `--from` and no cursor on file yet, the walk starts one day of blocks
//! behind the head -- research 0039 §"Disk growth" derives ≈9.8 blocks/second
//! from research 0038's measured 0.1019 s/block, giving ≈846,720 blocks/day
//! ([`ONE_DAY_BLOCKS`]) -- rather than the whole chain from genesis, which a
//! first run has no reason to re-walk.
//!
//! # What a name that will not read means
//!
//! `name()`/`symbol()` are two more `eth_call`s per launch, on the same
//! selectors and the same decoder `crates/realorrug-onchain/src/robinhood.rs`
//! already uses for a dossier (`realorrug_robinhood::erc20`). A call that
//! fails, or a return `erc20::string_from_return` will not decode, leaves
//! that field `None` -- the row is still written (rule 8: absent is not
//! zero, and a launch this reader could not name is still a launch, so it
//! stays in the denominator `realorrug narratives` counts against).

use std::time::SystemTime;

use realorrug_onchain::memory::TokenText;
use realorrug_robinhood::pons::{FACTORY, Launched, topic};
use realorrug_robinhood::{Rpc, erc20};

use crate::flag;

/// The chain `token_texts` rows from this walk are filed under -- the same
/// spelling `realorrug-serve`'s writer and `realorrug narratives` both use.
const CHAIN: &str = "robinhood";

/// The name this walk's progress is kept under in `walk_cursors`, so a
/// second named walk (a different chain, a different event) never reads or
/// overwrites this one's row.
const CURSOR_NAME: &str = "record-launches";

/// How many launches one run will read a name and symbol for, absent
/// `--max`. A polite default: `--max` exists so an operator with a large
/// unwalked backlog can choose to spend more, not so every run defaults to
/// an unbounded number of calls.
const DEFAULT_MAX: u64 = 2_000;

/// One day of blocks on Robinhood Chain, cited rather than guessed: research
/// 0039 ("Disk growth, inferred from research 0038's 0.1019 s/block")
/// computes "~9.8 blocks/second... ≈846,720 blocks/day" from that measured
/// block time. Used only as the starting window for a first run with no
/// cursor on file; every later run resumes from the cursor instead.
const ONE_DAY_BLOCKS: u64 = 846_720;

/// Where a walk with no `--from` starts: right after the cursor if one is on
/// file, or one day of blocks behind the head if it is the walk's first run.
///
/// Pulled out of [`run`] (which needs a live cursor read and a live chain
/// head) so the choice itself has a direct unit test, per the packet's
/// requirement that a start-block decision be pinned.
fn start_block(cursor: Option<u64>, head: u64) -> u64 {
    match cursor {
        Some(c) => c.saturating_add(1),
        None => head.saturating_sub(ONE_DAY_BLOCKS),
    }
}

/// How many of `total` launches this run processes, given `--max`.
///
/// Pulled out of [`run`] so the cap itself -- not just the RPC calls it
/// guards -- has a direct unit test: a `min` written the other way round, or
/// a `+1`, would silently read one more or one fewer token than `--max` asked
/// for.
fn capped_len(total: usize, max: u64) -> usize {
    usize::try_from(max).unwrap_or(usize::MAX).min(total)
}

/// The cursor to write after processing `processed` (already cut to the cap
/// by [`capped_len`]) out of a walk that covered up to `range_end`.
///
/// The last processed launch's own block when there was one, so a range the
/// cap cut short resumes just past the last launch actually written rather
/// than skipping straight to `range_end`. `range_end` itself only when
/// nothing needed a row -- an empty range, every launch already in
/// `token_texts`, or (equivalently for this purpose) a cap of zero -- so an
/// empty range still counts as finished rather than being re-walked forever.
fn next_cursor(processed: &[(realorrug_robinhood::Address, u64)], range_end: u64) -> u64 {
    processed.last().map_or(range_end, |(_, block)| *block)
}

/// Writes one launch's row, with whatever `name`/`symbol` this run managed
/// to read -- `None` for either is "could not read", not "has none" (rule
/// 8), and the row is written either way so the launch stays in
/// `realorrug narratives`' denominator.
fn write_row(
    memory: &realorrug_onchain::memory::Memory,
    token: &realorrug_robinhood::Address,
    _block: u64,
    name: Option<String>,
    symbol: Option<String>,
) -> Result<(), realorrug_onchain::memory::Error> {
    memory.record_token_text(&TokenText {
        chain: CHAIN.to_owned(),
        token: token.to_string(),
        name,
        symbol,
        first_seen: SystemTime::now(),
    })
}

/// Reads `name()` and `symbol()` for one launched token, using the exact
/// decoder `crates/realorrug-onchain/src/robinhood.rs`'s dossier build uses
/// (`realorrug_robinhood::erc20::string_from_return`) so a name this walk
/// writes and a name a dossier read agree on what "readable" means. A failed
/// call or an undecodable return is `None`, never an empty string (rule 8).
fn read_text(rpc: &Rpc, token: &realorrug_robinhood::Address) -> (Option<String>, Option<String>) {
    let name = rpc
        .call_contract(token, &erc20::NAME)
        .ok()
        .and_then(|data| erc20::string_from_return(&data));
    let symbol = rpc
        .call_contract(token, &erc20::SYMBOL)
        .ok()
        .and_then(|data| erc20::string_from_return(&data));
    (name, symbol)
}

/// Runs the command.
///
/// # Errors
///
/// A missing `--rpc`, a memory file that cannot be opened (deny by default,
/// rule 7 -- `crate::open_memory` refuses to create one), or a chain read
/// that fails outright (the head block, or the `TokenLaunched` walk itself).
/// A single token's `name()`/`symbol()` call failing is not an error: it
/// leaves that row's fields `None` and the run continues.
pub fn run(args: &[String]) -> Result<(), String> {
    // No default endpoint (rule 7): an operator who did not pass one has
    // chosen "this chain cannot be read", not "read it against an endpoint
    // this command invented".
    let rpc_url = flag(args, "--rpc").ok_or("--rpc <url> is required")?;
    let rpc = Rpc::new(&rpc_url);

    let path = flag(args, "--memory").unwrap_or_else(|| {
        let dir = std::env::var("REALORRUG_ANALYST_DIR")
            .or_else(|_| std::env::var("RADAR_ANALYST_DIR"))
            .unwrap_or_else(|_| "data/analyst".to_owned());
        format!("{dir}/memory.sqlite3")
    });
    let memory = crate::open_memory(&path)?;

    let explicit_from = number(args, "--from")?;
    let explicit_to = number(args, "--to")?;
    let max = number(args, "--max")?.unwrap_or(DEFAULT_MAX);

    let (head, _) = rpc.block_time(None).map_err(|e| format!("--rpc: {e}"))?;
    let to = explicit_to.unwrap_or(head);

    let cursor = memory.cursor(CURSOR_NAME).map_err(|e| e.to_string())?;
    let from = explicit_from.unwrap_or_else(|| start_block(cursor, head));
    if from > to {
        println!("nothing to do: the next block to walk ({from}) is past --to ({to})");
        return Ok(());
    }

    let mut launches: Vec<(realorrug_robinhood::Address, u64)> = Vec::new();
    realorrug_onchain::robinhood::walk_logs(
        from,
        to,
        |a, b| rpc.logs_range(&FACTORY, &[topic::TOKEN_LAUNCHED], a, b),
        |log| {
            if let Some(launch) = Launched::from_log(log) {
                launches.push((launch.token, log.block));
            }
        },
    )
    .map_err(|e| format!("TokenLaunched walk: {e}"))?;

    let seen = launches.len() as u64;
    let capped = &launches[..capped_len(launches.len(), max)];

    let mut named = 0_u64;
    let mut unreadable = 0_u64;
    for (token, block) in capped {
        let (name, symbol) = read_text(&rpc, token);
        if name.is_some() {
            named += 1;
        } else {
            unreadable += 1;
        }
        write_row(&memory, token, *block, name, symbol)
            .map_err(|e| format!("cannot write {token}: {e}"))?;
    }

    // The cursor only advances over what was actually written: the last
    // processed launch's own block, or the range's own end when nothing in
    // it needed a row (an empty range, or every launch already written by
    // `realorrug-serve` -- `record_token_text` still runs, `INSERT OR
    // IGNORE` just makes it a no-op). A failed write above returns before
    // this line runs, so a range that failed partway leaves the cursor where
    // it was, never past what was actually recorded.
    let new_cursor = next_cursor(capped, to);
    memory
        .set_cursor(CURSOR_NAME, new_cursor)
        .map_err(|e| e.to_string())?;

    println!(
        "{seen} launch(es) seen, {named} name(s) read, {unreadable} unreadable, cursor now {new_cursor}"
    );
    Ok(())
}

/// A whole-number flag, refusing one that will not parse rather than
/// inventing a value for it.
fn number(args: &[String], name: &str) -> Result<Option<u64>, String> {
    match flag(args, name) {
        None => Ok(None),
        Some(v) => v
            .parse()
            .map(Some)
            .map_err(|_| format!("{name}: {v:?} is not a whole number")),
    }
}

#[cfg(test)]
mod tests {
    use realorrug_onchain::memory::Memory;
    use realorrug_robinhood::Address;

    use super::{capped_len, next_cursor, start_block, write_row};

    /// A cursor on file resumes right after it -- the packet's own wording,
    /// "each run starts after the last block it finished". Re-walking the
    /// cursor's own block would be the off-by-one that double-reads every
    /// run's last launch.
    #[test]
    fn a_cursor_present_starts_the_next_block_after_it() {
        assert_eq!(start_block(Some(1_000), 50_000_000), 1_001);
    }

    /// With no cursor -- the walk's first run -- it starts one day of blocks
    /// behind the head rather than at genesis, which a first run has no
    /// reason to re-walk in full.
    #[test]
    fn an_absent_cursor_starts_one_day_behind_the_head() {
        assert_eq!(
            start_block(None, 1_000_000),
            1_000_000 - super::ONE_DAY_BLOCKS
        );
    }

    /// A head shorter than one day of blocks (a young or a test chain) must
    /// saturate to block 0 rather than underflow past it.
    #[test]
    fn an_absent_cursor_on_a_short_chain_saturates_at_zero() {
        assert_eq!(start_block(None, 10), 0);
    }

    /// `--max` actually binds: fewer launches than the cap are all kept, and
    /// more are cut to exactly the cap, not one more or one fewer.
    #[test]
    fn the_cap_binds_exactly() {
        assert_eq!(capped_len(5, 10), 5, "fewer than the cap: all of them");
        assert_eq!(capped_len(10, 5), 5, "more than the cap: cut to it");
        assert_eq!(capped_len(5, 5), 5, "exactly the cap: all of them");
        assert_eq!(capped_len(5, 0), 0, "a cap of zero processes none");
    }

    /// A range with no processed launches (an empty walk, or a cap of zero)
    /// finishes at the range's own end; a range that processed some resumes
    /// from the last one's block, not the range's end -- the whole reason a
    /// capped run does not skip the launches past the cap.
    #[test]
    fn the_cursor_follows_the_last_processed_launch_or_the_range_end() {
        assert_eq!(
            next_cursor(&[], 900),
            900,
            "nothing processed: the range end"
        );
        let processed = [(Address([0x11; 20]), 100), (Address([0x22; 20]), 250)];
        assert_eq!(
            next_cursor(&processed, 900),
            250,
            "something processed: its own block, not the range end"
        );
    }

    fn memory_at(name: &str) -> (std::path::PathBuf, Memory) {
        let dir = std::env::temp_dir().join("realorrug-record-launches");
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join(format!("{name}.sqlite3"));
        let _ = std::fs::remove_file(&path);
        let memory = Memory::open(&path).expect("open");
        (path, memory)
    }

    /// A token whose `name()` could not be read still gets a row -- absent
    /// is not zero (rule 8), and a launch this reader could not name must
    /// stay in `realorrug narratives`' denominator rather than quietly
    /// disappearing from the count of what launched.
    #[test]
    fn an_unreadable_name_still_writes_a_row() {
        let (path, memory) = memory_at("unreadable");
        let token = Address([0x33; 20]);
        write_row(&memory, &token, 42, None, None).expect("write");

        let rows = memory
            .token_texts_since(super::CHAIN, std::time::SystemTime::UNIX_EPOCH)
            .expect("read");
        assert_eq!(rows.len(), 1, "the row was written despite no name");
        assert_eq!(rows[0].name, None);
        assert_eq!(rows[0].symbol, None);
        let _ = std::fs::remove_file(&path);
    }
}
