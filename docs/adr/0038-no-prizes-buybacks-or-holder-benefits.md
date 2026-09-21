<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0038 — no prizes, buybacks or holder benefits

**Date:** 2026-09-21
**Status:** accepted. **Josh's decisions, recorded**, from his plan of
2026-09-21.
**Reasoning:** [design 0029](../design/0029-bot-quality-then-a-solana-launch.md).
**Supersedes:** [ADR 0015](0015-the-prize-is-an-evidence-relay-and-a-winner-is-always-selected.md)
entirely; [ADR 0034](0034-the-prize-is-a-free-calling-game.md) decision 3's
prize (the daily five stays, with nothing to win); ADR 0013 constraint 3
("the fee becomes the prize").
**Keeps:** ADR 0034 decisions 2, 4, 5 and 6.

## Decision

| # | decision |
|---|---|
| 1 | **No valuable prize** of any kind in this release: no weekly prize, no daily-five prize, no giveaway |
| 2 | **No buyback, staking benefit, holder revenue share, yield or airdrop.** Holding the token earns nothing from the project |
| 3 | **Holding buys nothing inside the product.** No holder gets a preferential verdict, a contest advantage, or more access |
| 4 | **The legacy weekly prize is switched off in code**: no week closes, no winner is selected or announced, no claim is prompted, no payout runs. The records already written are kept and stay readable |
| 5 | **The daily five continues as a free forecasting game** with no prize. Its outcomes read "rug observed within the window" or "no qualifying rug observed", never a permanent "real" certification, and a missing observation settles as **unresolved**, which cannot score a survival call as correct |
| 6 | **The "luck line" makes no statistical promise.** Records are published with their sample sizes before any claim of skill |
| 7 | **Reputation has no value**: it cannot be transferred or redeemed, and no airdrop is promised against it |
| 8 | **Community votes rank research; they do not settle facts.** A vote never sets an outcome or moves the bot's verdict |

## Consequences

- `realorrug-analyst` stops calling the week close, the weekly announcement
  and the claim prompt; the daily post stops quoting a pool.
- The site loses its pool, payouts and prize pages as live features; what
  they recorded stays as history.
- A later prize needs its own ADR, funded rules and an enforceable
  eligibility process. Wallet ownership does not establish eligibility, and
  terms cannot waive US sanctions obligations, so no future prize may be
  promised to everyone.
