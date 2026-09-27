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
| 2 | **Gate two — legal.** The review packet ([design 0030](../design/0030-launch-review-packet.md)) goes to counsel first; material objections are resolved before launch. **Amended by [ADR 0042](0042-claudes-review-replaces-counsel-at-gate-two.md)** (2026-09-26): no counsel is retained; Claude performs the legal and tax review instead, and every finding is fixed or ruled on by Josh |
| 3 | **Gate three — the launch itself.** A verified pump.fun configuration (fee recipient, authorities, terms read on the day) and an operator-reviewed launch transaction. Josh signs; nothing here signs for him |
| 4 | **Automated AI replies on X wait on X's written approval.** Without it, the website and the private evaluation carry on and the X launch waits. X's automation rules are read on the day, not from memory |
| 5 | **Spending stops at $90 a month before there is demand**, counting fixed services and metered use. Fixed costs are taken off first; model, RPC and X budgets share what is left |
| 6 | **Caching, per-user limits and a global spending stop** enforce the ceiling. When a read is served from cache, the reply or page says how old it is; stale data is never shown as current |

## Consequences

- The existing spend meter (`crates/realorrug-provider`) and the analyst's
  budget config carry decision 5; with no budget configured nothing spends
  (AGENTS §3 rule 7). Concretely: `Meter`/`Ledger` in
  `crates/realorrug-provider/src/cost.rs` add a monthly ceiling alongside the
  existing daily and per-call ones, and
  `crates/realorrug-analyst/src/daemon.rs` reads it from
  `REALORRUG_MONTHLY_USD` and `REALORRUG_FIXED_MONTHLY_USD`
  (`deploy/analyst.env.example`), with fixed costs taken off first as decision
  5 requires. One shared `Spend`/`Meter`/`Ledger` already covers both the X
  lane and the Telegram lane, so no separate wiring was needed for them to
  share the one monthly total. Both variables must be set on the live server
  before a build with this stop is deployed, or the bot refuses every paid
  call. RPC is counted as a fixed cost, which holds only while the RPC plan
  is a flat fee with no per-request overage; the meter does not count RPC
  calls.
- Statistical proof of forecasting skill is **not** a launch gate. Calibration
  continues after launch (plan 0002 phase 5), and no probability is published
  before calibration supports it.
