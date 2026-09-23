# Task 9-23-0007 progress

DONE: Wired Signal::HolderConcentration in FactSheet::build on both chains
(push_holders for Robinhood's Kind::LargestHolderShare, push_token_ownership
for Solana's Kind::TokenOwnership), firing at >= 1,000 bps
(HOLDER_CONCENTRATION_FLOOR_BPS in sheet.rs), the documented lower raise
from research 0052 S3.1 row S5. Pool/curve accounts already excluded
(#151). Updated sheet.rs doc comment and docs/design/0020 S3 table row.
Added 6 new sheet.rs tests covering fire/no-fire/absent-read on both chains.
CI's full suite caught one test this workstation's narrower local run
couldn't: the_live_robinhood_sheet_prints_its_factors_with_grades asserted
an empty factor list, written back when the signal never fired -- now that
it does, that fixture's real 50.22% concentration produces a genuine
Measured factor. Fixed the test to expect it (commit cca4e89), reran
locally (clean check/clippy/fmt, both named tests + the fixed test pass),
pushed. Draft PR #167: https://github.com/1xmint/realorrug/pull/167.

NEXT: Watch `gh pr checks 167` on the latest commit (cca4e89) for green.
Signal::CreatorSoldOut is NOT wired -- design 0020 S7 explicitly states it
needs design 0021's has_prior_balance memory interface (not yet written)
to tell "sold out" apart from "currently holds nothing"; wiring it from
CreatorCashFlow alone would invent semantics design 0020 didn't approve.
Reported as BLOCKED in the PR body rather than guessing a threshold.

WATCH OUT FOR: another agent editing crates/realorrug-cli/src/replay.rs --
do not touch it. Only one cargo process at a time (check Get-Process
cargo,rustc first). CreatorSoldOut still needs design 0021 before anyone
wires it. Local test runs here are narrower than CI's `just tests` --
CI can surface fixture assertions a scoped local run misses.
