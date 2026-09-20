<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0036 — a paid, facts-only endpoint, priced in USDC on Base

**Date:** 2026-09-19
**Status:** accepted. Josh's decision, recorded, 2026-09-19, after reading
research 0054: build a paid, facts-only endpoint, $0.05 per call, paid in
USDC on Base via x402. Buyers are both agents/bots and people. The X bot
stays free as the shop window.
**Consequence lands in:** `crates/realorrug-serve/src/facts.rs` (new),
`crates/realorrug-serve/src/lib.rs`, `crates/realorrug-serve/Cargo.toml`,
`crates/realorrug-onchain/src/memory.rs`, `docs/design/0023-the-public-checker-page.md`,
`docs/design/0028-the-daily-five.md` §7 (the legal question).

## Context

Research 0054 (`docs/research/0054-paid-api-x402-and-postgres.md`) measured
what a paid API on this tree would cost and what it could sell. Two findings
drive this decision:

- The weighted score and level (`realorrug_roast::Assessment`) are held back
  from calibration per research 0052 — nothing sells a number nobody has yet
  checked against outcomes.
- The measured facts underneath the score (`FactSheet`: prices with their
  read-point, holder shares, curve facts, coverage gaps) cost nothing extra
  to produce beyond the chain reads the free bot already does, and involve no
  model call at all. Selling exactly that, and nothing the model wrote, keeps
  AGENTS rule 2 ("the model may not introduce a fact") true of the paid
  surface by construction: there is no model output in it to introduce one.

x402 (an HTTP 402 payment protocol governed by a Coinbase/Cloudflare
foundation) settles on Base or Solana today; no official Robinhood Chain
support exists. The token being *described* lives on Robinhood Chain; the
*payment* for describing it settles on Base. These are different chains and
this ADR keeps them visibly different in the code (`ChainAddress` for the
token, a flat `network: "base"` string for the payment).

## Decision

1. **Facts only, until calibration.** The response is `FactSheet`'s measured
   facts (values, rendered text, the block/time each price was read at,
   coverage gaps and skipped reads) and `realorrug_roast::assessment::factors`'
   list of signals with their evidence and grade (`Measured` / `Inferred` /
   `SelfReported`). It never includes `Assessment::risk_index`, `score_bps`,
   `level`, or `score_level`, and it never includes a factor's `delta_bps`
   (the per-factor score weight) — that number is part of the same
   uncalibrated scoring mechanism the level comes from, and research 0052's
   hold covers it too even though it never reaches the model.
2. **Price: $0.05 per call**, fixed in code (`facts::PRICE_ATOMIC_USDC`), not
   an environment variable — a price is a decision this ADR records, not a
   runtime knob an operator can drift by accident.
3. **Base, USDC, receive-only.** Payment settles on Base in USDC. The route
   holds `REALORRUG_X402_PAY_TO`, a receive-only address, the same name and
   the same structural separation from any signing key that
   `crates/realorrug-model/src/codex.rs` and `-agent/src/lib.rs` already use
   as a stand-in proving a subprocess never inherits a payout key. Nothing in
   `realorrug-serve` gains a dependency on `realorrug-payout`; `repo-conformance`'s
   `no_crate_that_holds_a_model_can_reach_the_payout` continues to hold and
   this ADR adds no exception to it.
4. **Deny by default: no pay-to, no route.** `facts::FactsState::from_vars`
   returns `None` when `REALORRUG_X402_PAY_TO` is unset, and `app()` merges
   `facts::router` only when it returns `Some`. An unmounted route 404s the
   ordinary axum way, the same pattern `realorrug-agent`'s doc comment
   already names for the x402 surface ("routes should not be mounted at all
   in this state, the same way the x402 surface returns 404 rather than
   serving free").
5. **Settle after the facts are produced, not before.** The handler order is
   verify the payment (does the `X-PAYMENT` header pay enough, in the right
   asset, on the right network — this does not move money), read the chain
   and build the `FactSheet`, and only then settle (this does move money,
   once, via the facilitator). A read that fails (bad address, no endpoint
   configured, the chain call errors) returns its ordinary error status and
   never calls settle, so a buyer is never charged for an answer they did not
   get.
6. **Deviation from research 0054's suggestion: no `x402-rs` dependency.**
   Research 0054 named `x402-rs` as the crate to reach for, but flagged that
   it had not been read or verified that session. `deny.toml`'s stated bar
   for a new dependency is "the alternative is writing it ourselves and
   getting it wrong" — and here it is not: the actual cryptographic
   verification and on-chain settlement is the facilitator's job, reached
   over HTTP (`ureq`, already a workspace dependency used identically by five
   other crates); this crate only builds the x402 JSON shapes
   (`accepts`/`X-PAYMENT`/`X-PAYMENT-RESPONSE`) and calls the facilitator's
   `/verify` and `/settle` endpoints. That is little enough wire format to
   hand-roll and keep in view, and it avoids taking on a crate nobody here has
   read. If a later PR finds the facilitator's exact response shape differs
   from what this PR guessed (unverified this session, see research 0054),
   that PR fixes the field names; it does not need to re-open this decision.
7. **Every price carries its block or time (ADR 0033).** Each fact in the
   response that carries a `ReadAt` serializes it inline; a price fact with
   no read point is not possible because `FactSheet::build` does not produce
   one.
8. **The legal question is added, not answered.** `docs/design/0028-the-daily-five.md`
   §7 already holds two questions waiting on the lawyer reviewing the
   project. This ADR adds a third to the same list: whether selling measured,
   non-advisory on-chain facts about a token for a fixed per-call fee, to
   automated buyers as well as people, needs anything beyond what the free
   bot already does (no model output, no verdict, no recommendation).
   Nothing in this endpoint waits on the answer — it ships facts only, the
   same content the free bot already publishes per token, just on demand and
   priced — but the question is recorded rather than assumed away.
9. **Postgres is not adopted.** SQLite stays. The paid route opens the same
   `Memory` file the analyst daemon already writes
   (`REALORRUG_ANALYST_DIR`/`data/analyst/memory.sqlite3`), so the API and
   the daemon share one file on one box, as research 0054 recommended.
   Today they are two OS processes on the same machine sharing that file, not
   literally one process; if the paid API ever needs to run on a separate
   host from the daemon, that is the trigger research 0054 named for
   revisiting Postgres, and it has not happened.
10. **Every settled request is recorded.** `Memory::record_paid_request`
    (token, block, response hash, payer, amount, settlement tx hash, served
    time) is called once, after settlement succeeds, never before — a
    verified-but-unsettled or failed-read request never reaches the table,
    so it stays exactly "what was actually sold", which is what the owner
    wants kept for the moat and later calibration.

## What ships next, not here

- The people-facing page and its wallet-connect button. This PR's buyers are
  anyone who can construct an x402 request by hand or with an existing x402
  client (bots first); a page with a "pay and see" button is deliberately a
  separate PR so this one stays reviewable.
- Deploying this, setting `REALORRUG_X402_PAY_TO`/the facilitator URL, or
  creating any wallet. Those are the owner's actions, not this PR's.
