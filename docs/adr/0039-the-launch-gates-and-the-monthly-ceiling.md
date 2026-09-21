<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0039 — the launch gates, and a $90 monthly ceiling

**Date:** 2026-09-21
**Status:** accepted. **Josh's decisions, recorded**, from his plan of
2026-09-21.
**Reasoning:** [design 0029](../design/0029-bot-quality-then-a-solana-launch.md).
**Order of work:** [plan 0002](../plans/0002-bot-quality-then-a-solana-launch.md).

## Decision

| # | decision |
|---|---|
| 1 | **Gate one — reply quality.** The token does not launch until Josh has read representative Solana replies from the replay set and accepted them, and the evidence-fidelity, unsupported-accusation and unknown-data checks pass on every accepted case |
| 2 | **Gate two — legal.** The review packet ([design 0030](../design/0030-launch-review-packet.md)) goes to counsel first; material objections are resolved before launch |
| 3 | **Gate three — the launch itself.** A verified pump.fun configuration (fee recipient, authorities, terms read on the day) and an operator-reviewed launch transaction. Josh signs; nothing here signs for him |
| 4 | **Automated AI replies on X wait on X's written approval.** Without it, the website and the private evaluation carry on and the X launch waits. X's automation rules are read on the day, not from memory |
| 5 | **Spending stops at $90 a month before there is demand**, counting fixed services and metered use. Fixed costs are taken off first; model, RPC and X budgets share what is left |
| 6 | **Caching, per-user limits and a global spending stop** enforce the ceiling. When a read is served from cache, the reply or page says how old it is; stale data is never shown as current |

## Consequences

- The existing spend meter (`crates/realorrug-provider`) and the analyst's
  budget config carry decision 5; with no budget configured nothing spends
  (AGENTS §3 rule 7).
- Statistical proof of forecasting skill is **not** a launch gate. Calibration
  continues after launch (plan 0002 phase 5), and no probability is published
  before calibration supports it.
