# Progress — Packet 0035 (read point and age)

Branch: feat/0034-read-point-and-age

## Status
- [ ] 1. FactSheet.read_at -> Option<ReadAt>; verdict::template one sentence on both chains; authorised() pushes chain-correct number; Solana rendering pinned byte-for-byte
- [ ] 2. Age: real age fact (Solana slot delta + approx hours) vs ageless (Robinhood, must say age unreadable) vs neither (refuse, unchanged); check_required_age rewritten in forbidden.rs
- [ ] 3. docs/design/0020-robinhood-fact-sheet-and-voice.md §4 updated with the CantTell-demotion-rejected rationale
- [ ] 4. Tests: robinhood read point in authorised/template; solana byte-pin; nothinguglyyet-with-age-but-no-age-stated refused; ageless-nothinguglyyet without "could not be read" refused; template passes check_required at all 5 levels both chains
- [ ] 5. clippy/fmt/tests across roast, analyst(build only), cli(build only), model(build only), repo-conformance

## Next
Start with sheet.rs: retype read_at, add push_age + Kind::Age.

## Watch out for
- voice.rs (same crate, not in "owns" list) has one FactSheet test fixture with `read_at: Some(Slot(...))` that must be updated for compilation — flagged as a necessary minimal touch, not scope creep.
- fmt --check must run LAST, after the final edit, right before push.
- Never touch realorrug-analyst/, realorrug-onchain/, realorrug-cli/.
