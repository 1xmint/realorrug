# realorrug: intent

The one place for what Josh wants and why. Code and voice read this before deciding
on his behalf; anything not covered here is a question for Josh, not a guess.

## Status

**Direction changed 2026-09-21** (Josh's plan, recorded as ADRs 0037–0039,
design 0029, plan 0002). Phase 1 and phase 2's engineering are merged (main at
`ba6b8f8`, 2026-09-24), and phase 3's one engineering row — the pump.fun launch
check and treasury fee receipts — is merged too (PRs #182, #184, #185, #186;
research `docs/research/0062-the-launch-check-on-a-real-pump-fun-launch.md`).
Current milestone: **plan 0002 phase 2b** — the 20-second read deadline
(`crates/realorrug-onchain/src/budget.rs:69`) cut 6 of the 10 cases in
`docs/research/data/replay-2026-09-24b/review.md` short and left them
`CantTell` for the clock, not the token (the same mints read clean earlier
the same day on the same code, so the fault is in the read path under a slow
endpoint, not the constant), so engineering fixes the read path and
recaptures a fair replay set at the live budget before Josh reviews real
replies. What remains
before launch is Josh's: accepting replies, counsel, X's written approval, the
treasury form, the dev buy size, and the launch signature. Per Josh's decision
of 2026-09-25, phase 4 (research, forecasts, reputation; see plan 0002) is
being built before launch and switched on after, stopping only at the gates
this document lists below — deploying to the live server, X credentials in
production, posting from the X account, and any spend; everything up to a
build that runs on a private box with test config is engineering's, starting
with a design document and an ADR (AGENTS.md §2). The Robinhood launch, the
weekly prize and the payout signer are retired; the daily five stays as a free
game with nothing to win.

## Vision

An evidence-backed memecoin analyst you summon about a token. It answers with
what the chain shows. **Real or rug? It shows the facts. You decide.** A
community token launched through pump.fun on Solana pays, through its creator
fees, for the project's disclosed operating costs. No prizes, buybacks or
holder benefits. The loop: useful verdicts → people share and investigate →
more timestamped evidence and outcomes → better analysis → repeat users and
paid API demand. The token's price is not part of that loop.

## Framing: informed judgement, not proof

Josh pushed back on "prove" (2026-09-16). The bot makes **informed, evidence-backed
judgements from on-chain data**. It does not prove claims. Wording everywhere should
say that: site copy, docs, ADR titles, code comments, reply text.
Partly applied: ADR 0027 was renamed. Re-checked 2026-09-25
(`grep -rn "prove" site/src docs/adr crates/realorrug-roast/src`): the earlier
list is stale. `site/src/Leaderboard.tsx` no longer exists; `HowItWorks.tsx`
has no "prove"; `site/src/ui/index.tsx:30` says "provenance", a different word,
not the claim. What is left is all code comments about what cannot be proven,
not claims the bot proves things: `crates/realorrug-roast/src/forbidden.rs`
(lines 25, 373, 1568) and `crates/realorrug-roast/src/sheet.rs` (lines 448,
468) and similar comments elsewhere in that crate; ADRs 0015 and 0025 are
historical records of past decisions, not current claims. Nothing here reads
as the bot claiming it proves things; no site or reply copy uses "prove".

## Settled decisions

- The bot answers about $REALORRUG exactly like any other token.
- Off-topic joke replies: approved ("cheap but effective").
- Environment names are `REALORRUG_*`; old `RADAR_*` names still work as fallbacks (PR #63, merged c1f9e65).
- Pons v2 launch contracts verified against published source; no hidden admin drain (research/pons-bytecode-verification.md). Launch blocker closed.
- Brand art: keep the existing X profile picture and banner.
- CoinMarketCap data (temporary Startup key, 2026-09-22): Josh says the project has an exception to CMC's no-redistribution terms; collect freely (deploy/cmc/).

## Open, undecided

- **Bags:** settled by ADR 0029 (the bot holds its token openly, trades none).
- **Treasury form** (plain wallet or multisig) and the **dev buy's size**: Josh's, before launch.
- Site review talk-through; art pick A/B (later site slices wait on it).
- Image replies: test whether posting images works first.

## Tests


Tests run on GitHub (Actions CI), never as full suites on the local PC. A merge waits for CI to pass. Where the code runs on Windows, CI runs on Windows too. (Standing rule, 2026-09-16; matches AGENTS.md section 5.)

## Gates (always Josh)

Accepting the bot's replies (the launch gate) · launching the token · spending funds or signing transactions (includes
paid X reads for the mention replay) · posting from the X account ·
deploying to a live server (creator index, read memory switch-on) · any change of
project direction.

## Existing tooling

Alerts reach Josh's phone through the Claude mobile app. Don't add another alert service.

## Decision log

Autonomous decisions and reversals: `docs/plans/0002-bot-quality-then-a-solana-launch.md` (`.orchestrator/` run ledgers are gitignored and not tracked history).
