# Progress: packet 0037 — repeat launcher prevalence

Task id 9-15-0034, branch `feat/0036-repeat-launcher`.

- [x] `crates/realorrug-roast/src/firstparty.rs` — load/parse the named list
- [x] `docs/research/data/first-party-addresses.json` — seeded with FACTORY + ESCROW
- [x] `crates/realorrug-roast/src/creator.rs` — floor from the index (exclude, then percentile)
- [x] `crates/realorrug-roast/src/sheet.rs` — push `Signal::RepeatLauncher`, thread `Option<&FirstPartyList>`
- [x] `crates/realorrug-roast/src/lib.rs` — `pub mod firstparty;`, `roast()` passes `None` (not wired further, out of scope)
- [x] `docs/design/0020-robinhood-fact-sheet-and-voice.md` §3 — update `RepeatLauncher` row
- [x] tests: above/below floor, exclusion-order, no-list, under-100, pinned percentile
- [x] cargo test -p realorrug-roast, build -p realorrug-analyst / -p realorrug-cli, clippy, repo-conformance, fmt --check
- [x] push

## Watch out for

- Floor must NOT import Radar's REPEAT_FLOOR=3 / INFRASTRUCTURE_FLOOR=100 — different population/window/chain.
- Exclude named-list addresses BEFORE computing the percentile, not after.
- `roast()`'s public signature (called from realorrug-analyst, realorrug-cli) must NOT change — those crates are out of scope.
- fmt --check only as the very last command, after the final edit.

## Finished by the lead after the worker hit its turn cap

The worker committed `68ae0ef` (the named list) and `565e40d` (the floor), then
ran out of turns with the sheet wiring uncommitted and the design document
untouched. Nothing was lost: the tree held a coherent diff with no un-reverted
mutation in it, and the tests it implied all pass.

Design 0020 needed four edits, not one. Three places still said this signal
could not fire:

- the base-rates row, which had `RepeatLauncher` waiting on a published
  snapshot it no longer needs (the floor comes from the index it is handed);
- the design-0021 interface paragraph, which had it asking the read memory for
  `launches_in_window(address, minutes)` -- what shipped reads the **lifetime**
  count the creator index already holds, so it needs nothing from 0021;
- the "not established" bullet, which said both this and `LaunchBlockBundle`
  were designed and could not fire. Only `LaunchBlockBundle` still is.
