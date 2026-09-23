# realorrug: intent

The one place for what Josh wants and why. Code and voice read this before deciding
on his behalf; anything not covered here is a question for Josh, not a guess.

## Status

**Direction changed 2026-09-21** (Josh's plan, recorded as ADRs 0037–0039,
design 0029, plan 0002). Current milestone: plan 0002 phase 2 (every doc, page
and code path describes the pump.fun launch; the weekly prize is switched off)
plus the Solana reply-review loop. The Robinhood launch, the weekly prize and
the payout signer are retired; the daily five stays as a free game with
nothing to win.

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
Partly applied: ADR 0027 was renamed. Files still using "prove" (checked 2026-09-18):
`site/src/HowItWorks.tsx`, `site/src/Leaderboard.tsx`, `site/src/ui/index.tsx`,
`crates/realorrug-roast/src/forbidden.rs`, `crates/realorrug-roast/src/sheet.rs`, ADRs 0015 and 0025.

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

Autonomous decisions and reversals: the run ledger (RUN.md under .orchestrator/runs/20260915-robinhood-7b, gitignored), "Decision log".
