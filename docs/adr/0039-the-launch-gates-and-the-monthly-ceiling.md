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
- **Amended 2026-09-28 (orchestrator decision, 9-27-0026b defect 2):** the
  daemon's `Spend`/`Meter`/`Ledger` is shared by the X and Telegram lanes
  because both run inside that one process, but `realorrug analyst`,
  `realorrug roast` and `realorrug replay --model` are separate processes run
  by hand, and were found opening the daemon's own ledger file rather than
  sharing its in-memory total -- a hand-run charge and the daemon's next save
  erased each other, uncounted against the cap either way. Each process now
  meters its own ledger file beside the daemon's (`crate::spend` in
  `realorrug-cli`, `cli-ledger.json`), and the $90 ceiling is split into
  slices that cannot sum past it: `REALORRUG_CLI_MONTHLY_USD` for the CLI
  (unset closes it, rule 7) and `REALORRUG_SERVE_MONTHLY_USD` (design 0032's
  name) for `realorrug-serve`, both taken off what
  `REALORRUG_MONTHLY_USD − REALORRUG_FIXED_MONTHLY_USD` leaves before the
  daemon's own allowance is computed; if the slices leave nothing, the
  daemon's budget is `Budget::CLOSED` too, never negative. Enforced two ways,
  not only described here: `cli_monthly_allowance_from` itself refuses a CLI
  slice that, added to the serve slice, exceeds what is left (a review
  finding on PR #205, 2026-09-28 -- the function used to hand back its own
  env var unbounded), and `daemon_monthly_allowance_from` closes the daemon
  the same way a slice's own caller closes itself when that slice is set but
  will not parse, rather than reading an invalid slice as zero. See
  `crates/realorrug-provider/src/cost.rs`
  (`daemon_monthly_allowance_from`/`cli_monthly_allowance_from`) and
  `deploy/analyst.env.example`.
- **Amended 2026-09-28, same review:** because the CLI's ledger is a plain
  file beside the daemon's, two hand-run paid commands started at once would
  each load the same starting total, spend independently, and overwrite each
  other's save -- exactly the cross-process race decision 5 exists to close,
  now one file over instead of one process over. `realorrug-cli`'s
  `crate::spend::open()` holds an exclusive OS lock on `cli-ledger.lock` for
  the life of the run (`std::fs::File::try_lock`, released automatically on
  exit or crash); a second `open()` while the first is still running is
  refused, naming the lock path, rather than racing it. One hand-run paid
  command at a time on a given box.
- Statistical proof of forecasting skill is **not** a launch gate. Calibration
  continues after launch (plan 0002 phase 5), and no probability is published
  before calibration supports it.
