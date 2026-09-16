# Progress — Packet 0035 (read point and age)

Branch: feat/0034-read-point-and-age

## Status
- [x] 1. FactSheet.read_at -> Option<ReadAt>; verdict::template one sentence on both chains; authorised() pushes chain-correct number; Solana rendering pinned byte-for-byte
- [x] 2. Age: real age fact (Solana slot delta + approx hours) vs ageless (Robinhood, must say age unreadable) vs neither (refuse, unchanged); check_required_age rewritten in forbidden.rs
- [x] 3. docs/design/0020-robinhood-fact-sheet-and-voice.md §4 updated with the CantTell-demotion-rejected rationale
- [x] 4. Tests: robinhood read point in authorised/template; solana byte-pin; nothinguglyyet-with-age-but-no-age-stated refused; ageless-nothinguglyyet without "could not be read" refused; template passes check_required at all 5 levels both chains
- [x] 5. clippy/fmt/tests across roast, analyst(build only), cli(build only), model(build only), repo-conformance

## Next
All five done. Verified on this branch, in this order:

- `cargo test -p realorrug-roast` — 181 + 19 pass.
- `clippy -p realorrug-roast --all-targets -- -D warnings` — clean (one
  `single_match_else` found and fixed).
- `build -p realorrug-analyst`, `-p realorrug-cli`, `-p realorrug-model` — all
  finished.
- `cargo test -p repo-conformance` — 19 pass.
- `cargo fmt --check` last, immediately before push.

Two defects were found beyond the packet and fixed here, because the packet's
own rule was unsatisfiable without them:

1. `FactSheet::render()` did not print the read point, and `render()` is the
   model's entire input (`voice.rs`). So the ageless branch of
   `check_required_age`, which requires the reply to state the read point,
   refused every possible reply: every Robinhood `NothingUglyYet` would have
   fallen back to the template with nothing saying so. Fixed, pinned by two
   tests, verified by re-applying the bug at the one mutated line.
2. Design 0020 §1's table and §3's bullet stated the rejected alternative as
   the rule ("a sheet that cannot read this must not reach that level").
   Both now say what ships.

## Watch out for
- voice.rs (same crate, not in "owns" list) has one FactSheet test fixture with `read_at: Some(Slot(...))` that must be updated for compilation — flagged as a necessary minimal touch, not scope creep.
- fmt --check must run LAST, after the final edit, right before push.
- Never touch realorrug-analyst/, realorrug-onchain/, realorrug-cli/.
