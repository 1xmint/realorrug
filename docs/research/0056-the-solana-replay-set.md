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

All four were first read on 2026-09-21 between 13:59 and 14:06 UTC, and
read again between 14:43 and 14:45 UTC after the fixes below; the saved
sheets are the second read. Both times with
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
| `EYPSU1oha6ELaZ4wN1crMcdnXDb21S6LWkJXohs7pump` | `incomplete-read-versioned-tx` | A bonding-curve token with version 1 transactions in its history, which the reader (before the fix below) refused to read: `dossier` reported `rpc error: Transaction version (1) is not supported by the requesting client. Please try the request again with the following configuration parameter: "maxSupportedTransactionVersion": 1`. The second read, after the fix, reads its launch block; the label is kept from the first read, and the case now guards the version 1 reader. |

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
  hour at read time, per DexScreener's `txns.h1`). `dossier` failed with an
  RPC error naming an unsupported transaction version. This is a different
  fault from the page-budget case: not a rate or size limit, but the reader
  asking for version 0 while some of the token's transactions are version 1.
  Now fixed; see
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

1. **The transaction reader asked for version 0 only, and dropped
   lookup-table accounts. Both are now fixed.** The capture of
   `EYPSU1oha6ELaZ4wN1crMcdnXDb21S6LWkJXohs7pump` failed with `rpc error:
   Transaction version (1) is not supported by the requesting client`.
   Its launch transaction is version 0 and reads fine; 7 of the token's
   85 transactions are version 1, and the node refuses those outright
   when the request says 0. Checking that also found a second fault: the
   parser read only the message's `accountKeys`, while a versioned
   transaction lists some accounts in `meta.loadedAddresses`, so any
   instruction reaching a lookup-table account was dropped and a real
   trade could read as no trade. PR #145 adds the loaded accounts; the
   PR that brings in this note raises the request to version 1. The
   saved sheet for this mint is the second read, after both fixes, and
   its launch block reads.
2. **`getTokenLargestAccounts` failed uniformly, every time, all session.**
   Whether or not the public endpoint is expected to serve that method for
   free at all is a question for whoever owns the RPC choice, but a reader
   that depends on it for every "who holds this" fact has no fallback today
   when the answer is always 429 — see "what's missing," above.

The first is fixed; the second needs an RPC that serves the method.

## What the first replay found in the sheet

Running `realorrug replay` over the first read of these four found three
faults in what the fact sheet hands the reply, all fixed in the PR after
this note's first one:

1. **The venue-fee line carried an instruction to the model.** It rendered
   "250 bps -- THE VENUE FEE ONLY. The measured all-in round trip is 850
   bps. Never present the fee as the cost of trading." The template prints a
   fact's line verbatim, so that sentence and a second round-trip figure
   reached the reply. The line now carries only its qualifier.
2. **Every report failed its own number check.** A skipped reason quoted
   "within 10% of each other", and the report repeats skipped reasons, so
   the fidelity check refused the 10 as a number the sheet never measured.
   The reason now has no digits.
3. **Every readable launch was published as "0 slots (about 0 hours)"
   old.** The Solana reader used the launch block's own slot as the sheet's
   read point, so the age (read slot minus launch slot) was always zero, and
   the live curve balance was stamped with a slot from before any trade. The
   read point is now the curve read, falling back to the launch slot only
   when the curve cannot be read, and the sheet counts an age only from a
   read strictly after the launch. On the second read the ordinary launch
   is 15,231 slots (about 1.7 hours) old and the versioned one 11,818 slots
   (about 1.3 hours).

All four cases still come out `CantTell`, and on Solana today every case
will. The reader records "creator cash flow" as unread on every Solana read
because that read is not built for Solana (`realorrug-onchain`'s
`dossier.rs`), and the sheet counts any unread fact outside its optional list
as a required fact that failed, which forces `CantTell`. The reply showed it
as "part of this could not be read"; it now says "the creator's own buys and
sells were not checked". Whether a Solana verdict may be earned without that
read is a rules question for the owner, not changed here. The early buyers'
funding read also failed on the saved captures, but read cleanly on a
`dossier` run minutes later, so that one is the public endpoint's rate limit.

## Addendum, 2026-09-22: the third read, through Helius, and acceptance

The four were read a third time on 2026-09-22 through the project's Helius
endpoint (passed with `--rpc`, never written to the captures; the saved
files were checked for the key before commit), first before and then after
PR #151. The saved sheets are the read after it.

- **The PumpSwap pool was read as a holder.** On the Helius read the
  graduated tokens' "largest unidentified wallet" was their own AMM pool
  (85.3% for DsjPN, 4.6% for GTBx). The reader now proves a pool from the
  owner account's program and leaves it out of holder shares, as it already
  did for the bonding curve; the largest real wallet is 2.9% and 2.8%.
- **A non-concern led the report and the reply.** With no signal fired, a
  0.08% holder share sat under "Strongest concern" while the next section
  said no signal fired. The report now says "no signal fired" there and
  moves the ranked fact to a **Context** section, and the reply opens with
  the first gap ("Can't tell yet: ...").
- **The funding read is not the rate limit.** "Who funded the early
  buyers" still fails on every Helius read, so the explanation above is
  wrong for these captures. The cause is found and fixed below.
- The graduated tokens' launch block is still out of reach of the page
  budget on Helius, as it was on the public endpoint.

All six checks pass on all four. The owner delegated acceptance, and the
four replies were accepted on 2026-09-22 and copied, with their sheets, into
`crates/realorrug-roast/tests/replay/` as the first regression cases.
All remain `CantTell` until the Solana creator cash flow read lands.

## Addendum, 2026-09-22: the funding-read failure was a shared page budget

The cause named above as "not yet found" is found. `dossier.rs::build`'s six
steps for one mint all draw pages from the same `Budget`
(`crates/realorrug-onchain/src/budget.rs`'s `DEFAULT_MAX_PAGES = 3`), and
step 1 — the launch block's own signature-history walk — ran first and
against the busy mints in this set, spent the whole page pool getting cut off
before it found a launch. Every later step that pages its own walk of an
address's signature history then found `pages_left == 0` and failed
immediately: step 3 (the creator's transaction count) reported
`Count::AtLeast(0)`, a fake zero (AGENTS.md rule 8) rather than the gap it
actually was, and step 5 (`investigate_solana`, the funding read) re-walked
the mint's own signature history a *second* time — the same address step 1
had already paged — with nothing left to page with, which is exactly "who
funded the early buyers could not be read" on every one of these tokens, not
a rate limit at all.

Fixed in the same PR that lands this paragraph:

1. `investigate_solana` no longer re-walks the mint's signatures; `build`
   passes step 1's already-read `(Vec<SignatureInfo>, bool)` into it
   directly, so the funding read's launch-window signatures cost zero extra
   calls on the common path (a fallback walk, with its own granted page
   floor, remains for the one case step 1 never ran: a launch block served
   from the read memory).
2. `Budget` gained `grant_pages(floor)`, which raises `pages_left` to
   `max(pages_left, floor)` — a floor, not an addition, so a walk that
   already has pages left is never handed a second, stacking allowance.
   `build` calls it with `PAGES_PER_WALK` (`= DEFAULT_MAX_PAGES`) before each
   of the creator-history walk (step 3), the funding read's fallback walk
   (step 5), and the creator cash flow read (step 6), so one busy mint can no
   longer starve the reads that come after it. `DEFAULT_MAX_CALLS` is
   untouched: the overall call ceiling for a `build` call is still global,
   only the *page* pool is now floored per walk.
3. Step 3's creator-history walk now tells a miss (`"creator history"`,
   already rendered by `realorrug-roast/src/sheet.rs`'s `phrase_for` as "the
   creator's history could not be read") apart from a genuine zero: a walk
   cut off before a single signature came back is the absence of a count,
   not a measurement of zero (AGENTS.md rule 8), and `Count::AtLeast(0)` was
   claiming the latter for the former.

See `docs/design/0027-the-three-layers.md`'s slice 6b addendum, same date,
for the funding-read side of this fix in more detail.

## Sources

- DexScreener's public pair-search API (`api.dexscreener.com/latest/dex/search`),
  fetched directly, 2026-09-21, for candidate mints (fetched, not searched).
- `target/debug/realorrug.exe dossier <mint>` and `... capture <mint> --out
  ...`, run against `https://api.mainnet-beta.solana.com`, 2026-09-21.
