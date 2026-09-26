<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0002 — bot quality, then a Solana launch, then a research community

**Date:** 2026-09-21
**Status:** open. Replaces the Robinhood launch path of
[plan 0001](0001-after-the-split.md). Reasoning in
[design 0029](../design/0029-bot-quality-then-a-solana-launch.md); decisions in
ADRs 0037, 0038 and 0039. Phase 1 and phase 2's engineering are done (main at
`ba6b8f8`, 2026-09-24). The current milestone is **phase 2b**: fix the
20-second read-deadline clock in `crates/realorrug-onchain/src/budget.rs`
that produced `CantTell` verdicts instead of real ones, recapture a fair
replay set, then Josh reviews (see Decision log).

Kept as they are: the Rust engine, the website, fact sheets, the checks after
generation, the journals and the Solana readers.

## Phase 1 — one product, described the same way everywhere

| item | proof |
|---|---|
| ADRs 0037–0039, design 0029, the review packet (design 0030), this plan | these files |
| README, INTENT, AGENTS rule 1, `deploy/LAUNCH.md` rewritten for pump.fun | same PR as the ADRs |
| Weekly prize switched off in the analyst: no week close, no winner post, no claim prompt, no pool in the daily post | the prize-off PR |
| Payout binary refuses to run; its units and key script leave `deploy/`; it leaves the Linux release | the prize-off PR |
| Site: pool and payout pages retired, token and terms pages state the pump.fun economics, no prize or holder-benefit wording (enforced by `site/src/honesty.ts`) | the site PR |

**Done when** config, code paths and public statements describe the same product.

## Phase 2 — consistently useful on Solana

Scope: pump.fun launches and graduated PumpSwap tokens.

| item | proof |
|---|---|
| `realorrug capture <mint>` saves the fact sheet with its observation time | the replay PR |
| A report beside every reply: strongest concern and its evidence, alternatives, missing checks and read time, what would change it | the report module in `realorrug-roast` |
| `realorrug replay <dir>` prints each case's verdict, report, reply and checks into a review file | the replay PR |
| Evidence-fidelity, unsupported-accusation and unknown-data checks run on reply and report | the replay PR |
| A replay set from real captures: suspicious launch, ordinary launch, graduation, incomplete read, creator sale, misleading concentration | captures under `docs/research/data/` |
| **Josh reviews real replies**; accepted ones become regression cases that CI replays | accepted cases in the roast crate's tests |

**Done when** Josh approves representative replies and the three checks pass
on them. Models are compared only after documented failures.

## Phase 3 — the launch (Josh's gates)

1. Josh accepts the replies (phase 2).
2. Counsel reviews design 0030; material objections are resolved.
3. A pump.fun launch check: fee recipient, mint and freeze authorities,
   terms read on the day, a readback of the launch transaction. Fee
   accounting reads the treasury's receipts on Solana.
4. X's written approval before automated replies; without it the site and
   private evaluation continue.
5. Josh reviews and signs the launch transaction. Nothing here holds a key.

## Phase 4 — research, forecasts, reputation (after launch)

Sign-in, profiles, evidence submissions, discussion; a research assistant
that proposes forecasts for the user to confirm; immutable forecasts on the
daily five (five launches, six-hour window, fourteen-day horizon, shown after
close, explicit unresolved outcome); separate contribution and forecasting
reputation with no value. The luck line loses its statistical promise;
records are shown with sample sizes. Server: authenticated submission and
public round, outcome and profile reads, every record carrying chain, token
address, timestamps, evidence references and rule versions; SQLite on the one
host.

**Done when** a user can submit, inspect and reproduce a forecast outcome with
no money moving.

## Phase 5 — calibration and paid demand

Store each evidence snapshot, rule version, forecast time, horizon, later
observations and outcome definition; keep community claims apart from
verified labels. Evaluate on later periods and separate creator groups;
report coverage, unresolved cases, false positives and baselines. Probabilities
only after calibration supports them. Before the paid API takes money: a
checked facilitator, replay protection, settlement recovery and re-fetch of an
interrupted paid answer. Watchlists and usage tiers when demand pays for them.

## Verification that runs throughout

Unsupported claims, injected instructions and missing evidence; graduation,
partial reads, stale snapshots and unresolved outcomes; duplicate or late
forecasts, hidden-entry leakage and deterministic replay; wrong fee recipient,
duplicate fee claims and transaction readback; payment failures, retries and
budget exhaustion. Suites run in CI.

## Decision log

Josh's autonomous decisions and reversals, dated.

- **2026-09-25 — insert phase 2b before Josh's review.** Josh's decision: 6
  of the 10 cases in `docs/research/data/replay-2026-09-24b/review.md` came
  back `CantTell` because the 20-second read deadline in
  `crates/realorrug-onchain/src/budget.rs:69` cut the read short, not because
  the token gave no evidence. Reviewing that set would have judged the clock,
  not the bot, so engineering fixes the deadline and recaptures a fair replay
  set first; Josh's review of real replies (phase 2, last row) waits for it.
- **2026-09-25 — move phase 4 before launch, switched on after.** Josh's
  decision: phase 4 (research, forecasts, reputation) was planned for after
  launch; Josh moved its build earlier so it is ready to switch on once phase
  3 clears, rather than starting design work only after launch. Engineering
  may build and run it on a private box with test config up to, but not past,
  the gates INTENT.md lists: deploying to the live server, X credentials in
  production, posting from the X account, and any spend. Because phase 4
  changes realorrug-serve's recorded property that it reads published files
  and is never a store, it starts with a design document and an ADR
  (AGENTS.md §2) before any code.
