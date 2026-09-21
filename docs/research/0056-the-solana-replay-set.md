<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0056 — a first Solana replay set, captured from public RPC

**Date:** 2026-09-21.
**Status:** read-only capture work for
[plan 0002](../plans/0002-bot-quality-then-a-solana-launch.md) phase 2. These
four captures are candidate replay cases only — nothing here is an accepted
regression case. The owner accepts or rejects each one by running
`realorrug replay` against it; this note records what was captured, from
where, and why it was picked, so that decision has evidence behind it.

## What was captured

All four were read on 2026-09-21 between 13:59 and 14:06 UTC, with
`realorrug capture <mint> --out docs/research/data/replay-2026-09 --label
<name>`, built once in debug
(`cargo +stable-x86_64-pc-windows-gnullvm build -p realorrug-cli`) and run
directly as `target/debug/realorrug.exe`. No `--rpc` flag was passed, so
every read went through the client's default endpoint,
`https://api.mainnet-beta.solana.com` (`realorrug-onchain`'s
`RpcClient::DEFAULT_RPC`) — the same free public endpoint named in the task,
no key, no account, no paid tier.

| mint | label | what it shows |
|---|---|---|
| `GTBxUiw6wJdmmkCGZgRHLyYxqu1vG4KtRpeox6yDpump` | `graduated-pumpswap` | A token off the pump.fun bonding curve and trading on a PumpSwap AMM pool. |
| `24RwgHxwu8icT1tcDtgH4RwyaDWao86xfacUo2xHpump` | `ordinary-launch` | A fresh, unremarkable bonding-curve launch: the launch block read cleanly, showing 3 recipient token accounts, 4 transactions, and no dev buy found. |
| `DsjPNCjFrQDXGZ96UzohaMm9PQJJUxWFQ6Gy6do9CSLT` | `incomplete-read-page-budget` | A bonding-curve token whose launch block could not be reached: `dossier` reports "this token has more history than the page budget allows, so its launch could not be reached," alongside a 429 on the holder read. |
| `EYPSU1oha6ELaZ4wN1crMcdnXDb21S6LWkJXohs7pump` | `incomplete-read-versioned-tx` | A bonding-curve token whose launch transaction is a versioned (v0) transaction the reader cannot parse: `dossier` reports `rpc error: Transaction version (1) is not supported by the requesting client. Please try the request again with the following configuration parameter: "maxSupportedTransactionVersion": 1`. |

Each `.sheet.json` under `docs/research/data/replay-2026-09/` is exactly what
`capture` wrote; none was hand-edited.

## How each mint was found, and why it counts as that kind

- **`graduated-pumpswap`** — found via DexScreener's public pair-search API
  (`api.dexscreener.com/latest/dex/search?q=pumpswap`), which lists live
  pairs on the `pumpswap` DEX (Solana). A pair showing up there, trading with
  real liquidity ($193k at the time), is only possible for a pump.fun mint
  that has already migrated off its bonding curve — DexScreener would show
  it on the `pump-fun` curve venue, not `pumpswap`, otherwise. The capture's
  own `graduated` fact (`"rendered": "yes"`) and `capacity_after_graduation`
  fact confirm the same thing from the chain side, independent of
  DexScreener's listing.
- **`ordinary-launch`** — found via DexScreener's pair-search for
  `pumpfun`, filtered to a pair whose `pairCreatedAt` was only minutes old at
  read time, i.e. small transaction history by construction. `dossier`
  (run before `capture`, to see the reasoning `capture` freezes) read the
  launch block cleanly: creator `DjirghfmKo5rV8u5KHWMy4hdJEFbyHrozymYhKvA93X7`,
  3 recipient token accounts, 4 transactions, no dev buy. Nothing about that
  shape is unusual for a brand-new pump.fun coin — it is "ordinary" because
  there is no concentration, no bundling, and no anomaly to point at, not
  because a rule scored it low (the rules never ran a verdict: coverage was
  too thin, see below).
- **`incomplete-read-page-budget`** — found the same way as the ordinary
  launch (DexScreener `pumpfun` search), but this mint had enough
  transaction history that the reader's page budget for
  `getSignaturesForAddress` ran out before reaching the launch transaction.
  `dossier` names this directly: "this token has more history than the page
  budget allows, so its launch could not be reached." This is a real,
  reproducible reader limit, not a guess.
- **`incomplete-read-versioned-tx`** — found via DexScreener `pumpfun`
  search, filtered for a very active pair (44 buys / 21 sells in the last
  hour at read time, per DexScreener's `txns.h1`). `dossier` failed to read
  its launch transaction with a specific RPC error naming an unsupported
  transaction version. This is worth flagging separately from the
  page-budget case below, because it is not a rate limit or a size limit —
  it is the reader's `getTransaction` (or equivalent) call not asking for
  `maxSupportedTransactionVersion`, so any launch that used a versioned (v0)
  transaction is unreadable regardless of how much history it has. See
  "what the capture command got wrong," below.

## What's missing, and why

The task asked for six kinds: suspicious launch (heavy concentration or
bundled buys), ordinary launch, graduated to PumpSwap, incomplete read,
creator sold, and misleading concentration (a big holder that is actually a
pool, bonding curve, or exchange). Only three of the six are represented
above (ordinary, graduated, and two flavors of incomplete read) — the
concentration-dependent kinds (suspicious launch, creator sold, misleading
concentration) could not be captured honestly today.

The reason is the same for all three: they all depend on holder data — who
holds the supply, in what shares — which `dossier` reads with
`getTokenLargestAccounts` (`realorrug-onchain/src/dossier.rs`,
`token_ownership`). In this session, **every single call to that read, on
every mint tried (nine total, across graduated and ungraduated tokens,
spaced 3–15 seconds apart), returned `rpc transport: http status: 429`** —
including on `24RwgHxwu8icT1tcDtgH4RwyaDWao86xfacUo2xHpump`, tried twice 90
seconds apart, with the same result both times. That is consistent with the
public `api.mainnet-beta.solana.com` endpoint throttling or refusing
`getTokenLargestAccounts` for anonymous callers rather than with normal
per-caller rate limiting, since spacing out the retries did not change the
outcome. Retrying further would not be "spacing out calls, retrying
politely" (the task's instruction) so much as hammering an endpoint that has
already said no to this method nine times running.

Without a holder read, there is no concentration figure to point at — so a
"suspicious launch" or "misleading concentration" case cannot be
distinguished from an ordinary one, and a "creator sold" case cannot be told
apart from a creator who never held anything, from this endpoint, today.
Getting these three kinds will need either a different free read path for
`getTokenLargestAccounts` (a different public RPC provider, if one answers
that method for anonymous callers), or accepting this endpoint's answer and
widening `realorrug-onchain`'s retry/backoff for that one call — both are
engineering questions for whoever picks this back up, not something to fake
by hand-editing a sheet.

## What the capture command got wrong

Two things `dossier`/`capture` surfaced while working through this that are
worth a look, independent of which crate they belong to:

1. **The launch-block reader does not request `maxSupportedTransactionVersion`.**
   `EYPSU1oha6ELaZ4wN1crMcdnXDb21S6LWkJXohs7pump`'s launch transaction is a
   versioned (v0) transaction, and the RPC call to read it comes back with
   `rpc error: Transaction version (1) is not supported by the requesting
   client. Please try the request again with the following configuration
   parameter: "maxSupportedTransactionVersion": 1`. Versioned transactions
   are common on Solana now; a reader that cannot ask for them will miss the
   launch on any mint that used one, not just this one.
2. **`getTokenLargestAccounts` failed uniformly, every time, all session.**
   Whether or not the public endpoint is expected to serve that method for
   free at all is a question for whoever owns the RPC choice, but a reader
   that depends on it for every "who holds this" fact has no fallback today
   when the answer is always 429 — see "what's missing," above.

Neither of these is something this task's boundary allowed touching (no
Rust changes, capture-only); they are reported here, not fixed here, per the
task.

## Sources

- DexScreener's public pair-search API (`api.dexscreener.com/latest/dex/search`),
  fetched directly, 2026-09-21, for candidate mints (fetched, not searched).
- `target/debug/realorrug.exe dossier <mint>` and `... capture <mint> --out
  ...`, run against `https://api.mainnet-beta.solana.com`, 2026-09-21.
