# realorrug: intent

The one place for what Josh wants and why. Code and voice read this before deciding
on his behalf; anything not covered here is a question for Josh, not a guess.

## Status

**Resumed 2026-09-18** (paused 2026-09-16 to 2026-09-18 for the voice-to-code
workflow). Current plan: the daily five, a free real-or-rug calling game on the
site that replaces the engagement contest (design 0028, ADR 0034 recommending),
plus the stock-meta checks. Ledger: the run's RUN.md under .orchestrator/runs/20260915-robinhood-7b (gitignored; on Josh's PC only).

The prize described under Vision below is superseded by the daily five: a
mention earns nothing, and the prize ships switched off until the lawyer answers
the question in design 0028 §7.

## Vision

An X account you summon about a memecoin. It answers with what the chain shows:
who launched it, what was bought in the launch block, what the curve holds.
**Real or rug? It shows the facts. You decide.** A community token funds a weekly
prize for the summons that travelled furthest. It lives on Robinhood Chain (Pons v2).

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

## Open, undecided

- **Bags:** Josh said "eventually the bot may have its own bags". That conflicts with
  AGENTS.md rule 6 / ADR 0013 (the operator holds none of the token, ever).
  **Not decided.** Nothing builds toward it until Josh rules.
- Site review talk-through; art pick A/B (later site slices wait on it).
- Image replies: test whether posting images works first.

## Tests


Tests run on GitHub (Actions CI), never as full suites on the local PC. A merge waits for CI to pass. Where the code runs on Windows, CI runs on Windows too. (Standing rule, 2026-09-16; matches AGENTS.md section 5.)

## Gates (always Josh)

Launching the token · spending funds or signing transactions (includes the payout
dry run and paid X reads for the mention replay) · posting from the X account ·
deploying to a live server (creator index, read memory switch-on) · any change of
project direction.

## Existing tooling

Alerts reach Josh's phone through the Claude mobile app. Don't add another alert service.

## Decision log

Autonomous decisions and reversals: the run ledger (RUN.md under .orchestrator/runs/20260915-robinhood-7b, gitignored), "Decision log".
