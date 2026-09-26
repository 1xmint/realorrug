<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0041 — the serve crate may hold one store

**Date:** 2026-09-26
**Status:** recommending, until Josh rules — as [ADR 0034](0034-the-prize-is-a-free-calling-game.md)
recorded its own decisions.
**Reasoning:** [design 0032](../design/0032-the-research-store.md).
**Order of work:** [plan 0002](../plans/0002-bot-quality-then-a-solana-launch.md),
phase 4; decision log entry of 2026-09-25 ("move phase 4 before launch,
switched on after").

## Context

`crates/realorrug-serve/Cargo.toml` describes the crate as reading
"published files, never a store," and design 0023 §0 records the same
property in prose. Plan 0002's phase 4 (research, forecasts, reputation)
needs a write path a signed-in visitor controls — immutable forecasts
hidden until a window closes, evidence submissions, discussion, and
reputation records (design 0029 §4) — which a published file cannot provide.
Plan 0002's decision log entry of 2026-09-25 records that Josh moved phase 4
earlier so it is ready to build on a private box with test config, but
because it changes this recorded property, AGENTS.md §2 requires a design
document and this ADR before any code.

## Decision

| # | decision |
|---|---|
| 1 | `realorrug-serve` may hold one SQLite store for phase 4's records (forecasts, outcomes, evidence submissions, reputation records, discussion/votes); `crates/realorrug-serve/Cargo.toml`'s description changes from "reads published files, never a store" only when the code that holds the store lands, not before |
| 2 | The store is append-only and hash-chained, the way `realorrug-journal` chains its events (`crates/realorrug-journal/src/file.rs`) — a row is never edited or deleted, and a verify routine reports the first break in the chain |
| 3 | Sign-in is with X only, and the store denies by default without configuration: no path configured, no sign-in configured, means the store takes nothing and every existing route answers exactly as before (AGENTS §3 rule 7) |
| 4 | Reputation (contribution and forecasting, kept separate) carries no prize, no holder benefit, and no transferable or redeemable value (design 0029 §4; ADR 0038) |
| 5 | `Unresolved` is an explicit outcome for a forecast whose horizon closes without a definitive `Rugged`/`Stood` reading; it never becomes a win or a loss for the forecast scored against it (design 0032 §2) |
| 6 | Plan 0002 phase 5's calibration job reads these rows and nothing else — it does not write to the store, and it does not read a community claim as a verified label without its own separate check (plan 0002 phase 5, "keep community claims apart from verified labels") |

## Consequences

- **`crates/realorrug-serve/Cargo.toml`'s description is not touched by this
  ADR.** It changes in the same commit as the code that first opens the new
  store — the ADR authorizes the change; it does not make it.
- **ADR 0024 ("the bot stands alone") is not touched.** That ADR's decision
  is about this repository's independence from Radar's repository (no
  shared git dependency, copied crates instead) — it says nothing about
  whether `realorrug-serve` may hold a store, and reading it in full finds
  no line that forbids one. It stands exactly as it is.
- **What stays true:** the five existing public routes
  (`/v1/public/stats`, `/leaderboard`, `/pool`, `/weeks`, `/hunters`,
  `/recent`) keep reading published files; `realorrug-contest` stays pure
  ("no clock, no network, no key") and is never the store; nothing gains a
  spending key (AGENTS §3 rule 1); and every gate in `INTENT.md` — live
  deploy, a production X app, posting, and any spend — still applies before
  any of this reaches the live server (design 0032 §7).
- **Not decided here:** where the store's code lives — a new
  `realorrug-store` crate or a module inside `realorrug-serve` — is design
  0032 §6's recommendation, not this ADR's decision; either way,
  `realorrug-serve` is the process that writes.
