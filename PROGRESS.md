# Task 9-23-0007 progress

DONE: Wired Signal::HolderConcentration in FactSheet::build on both chains
(push_holders for Robinhood's Kind::LargestHolderShare, push_token_ownership
for Solana's Kind::TokenOwnership), firing at >= 1,000 bps
(HOLDER_CONCENTRATION_FLOOR_BPS in sheet.rs), the documented lower raise
from research 0052 S3.1 row S5. Pool/curve accounts already excluded
(#151). Updated sheet.rs doc comment and docs/design/0020 S3 table row.
Added 6 new sheet.rs tests covering fire/no-fire/absent-read on both chains.
cargo check/clippy/fmt all clean, both named tests pass. Committed as
108f448, pushed to origin/worktree-agent-a84e4f1a5109019b0, draft PR #167
open: https://github.com/1xmint/realorrug/pull/167.

NEXT: Signal::CreatorSoldOut is NOT wired -- design 0020 S7 explicitly
states it needs design 0021's has_prior_balance memory interface (not yet
written) to tell "sold out" apart from "currently holds nothing"; wiring
it from CreatorCashFlow alone would invent semantics design 0020 didn't
approve. Reported as BLOCKED in the PR body rather than guessing a
threshold. Remaining: check `gh pr checks 167` once CI has had time to
run; report final status to orchestrator.

WATCH OUT FOR: another agent editing crates/realorrug-cli/src/replay.rs --
do not touch it. Only one cargo process at a time (check Get-Process
cargo,rustc first). CreatorSoldOut still needs design 0021 before anyone
wires it.
