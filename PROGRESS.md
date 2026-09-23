# Task 9-23-0007 progress

DONE: Wired Signal::HolderConcentration in FactSheet::build on both chains
(push_holders for Robinhood's Kind::LargestHolderShare, push_token_ownership
for Solana's Kind::TokenOwnership), firing at >= 1,000 bps
(HOLDER_CONCENTRATION_FLOOR_BPS in sheet.rs), the documented lower raise
from research 0052 S3.1 row S5. Pool/curve accounts already excluded
(#151). Updated sheet.rs doc comment and docs/design/0020 S3 table row.
Added 6 new sheet.rs tests covering fire/no-fire/absent-read on both chains.

NEXT: Signal::CreatorSoldOut is NOT wired -- design 0020 S7 explicitly
states it needs design 0021's has_prior_balance memory interface (not yet
written) to tell "sold out" apart from "currently holds nothing"; wiring
it from CreatorCashFlow alone would invent semantics design 0020 didn't
approve. Reporting this as BLOCKED rather than guessing a threshold.
Still need: cargo check/clippy/fmt for realorrug-roast, run the two named
tests (accepted_replies_still_pass, the_five_levels_are_earned_and_recorded),
commit, push, open draft PR, watch gh pr checks.

WATCH OUT FOR: another agent editing crates/realorrug-cli/src/replay.rs --
do not touch it. Only one cargo process at a time (check Get-Process
cargo,rustc first).
