# Progress: packet 0037 — repeat launcher prevalence

Task id 9-15-0034, branch `feat/0036-repeat-launcher`.

- [ ] `crates/realorrug-roast/src/firstparty.rs` — load/parse the named list
- [ ] `docs/research/data/first-party-addresses.json` — seeded with FACTORY + ESCROW
- [ ] `crates/realorrug-roast/src/creator.rs` — floor from the index (exclude, then percentile)
- [ ] `crates/realorrug-roast/src/sheet.rs` — push `Signal::RepeatLauncher`, thread `Option<&FirstPartyList>`
- [ ] `crates/realorrug-roast/src/lib.rs` — `pub mod firstparty;`, `roast()` passes `None` (not wired further, out of scope)
- [ ] `docs/design/0020-robinhood-fact-sheet-and-voice.md` §3 — update `RepeatLauncher` row
- [ ] tests: above/below floor, exclusion-order, no-list, under-100, pinned percentile
- [ ] cargo test -p realorrug-roast, build -p realorrug-analyst / -p realorrug-cli, clippy, repo-conformance, fmt --check
- [ ] push

## Watch out for

- Floor must NOT import Radar's REPEAT_FLOOR=3 / INFRASTRUCTURE_FLOOR=100 — different population/window/chain.
- Exclude named-list addresses BEFORE computing the percentile, not after.
- `roast()`'s public signature (called from realorrug-analyst, realorrug-cli) must NOT change — those crates are out of scope.
- fmt --check only as the very last command, after the final edit.
