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

## Addendum, 2026-09-22: the funding gap's own sentence was self-contradicting

Two ordinary launches captured this same day sat at `CantTell` for a reason
that could not be true. Both replies' sole critical gap read "the funding
check did not finish: 4 of 4 chosen early buyers were read" — "did not
finish" and "4 of 4 were read" in the same sentence. `push_funding` in
`crates/realorrug-roast/src/sheet.rs` built this sentence from
`funding.selected`, but `investigate_solana` in
`crates/realorrug-onchain/src/wallets.rs` sets `selected: checked.len()`, so
`checked == selected` always on Solana; the EVM-shaped sentence that assumes
they can differ never fit this chain.

`push_funding` (via a new `funding_gap_message` helper) now says one of three
true things when `funding.gaps` is non-empty: on EVM, where a candidate can
be chosen but never checked, the original sentence stands unchanged ("the
funding check did not finish: N of M chosen early buyers were read"). On
Solana, every checked candidate was attempted, so the gap is instead named
against `Candidate::funding_complete` — if any checked candidate's funding
history is incomplete, "where N of the M checked early buyers got their
money could not be read"; if all of them are complete, the recorded gap must
be a transaction-read failure during the launch-window buyer walk itself,
so "some transactions in the launch window could not be read, so the early
buyers seen may not be all of them".

## Addendum, 2026-09-22: the fourth read, and all four replies accepted

All four mints were re-captured through Helius after the page-budget fix
(#157), the creator cash-flow read (#156) and the gap-sentence fix (#159),
and all four replies were accepted. The accepted set in
`crates/realorrug-roast/tests/replay/` now holds these captures and these
replies; the review file is at
`docs/research/data/replay-2026-09-23/review.md` with a `yes` on every
accept line.

What moved. `the creator's own buys and sells were not checked` is gone from
all four sheets -- the creator's own trading is now read on every one of
them. The ordinary launch went from 25 facts to 27, the versioned-transaction
mint from 17 to 21. Both graduated mints are unchanged at 3 facts: their
launch block is still out of reach, which is a different limit from the one
#157 fixed and is not a regression.

What the honest sentence now exposes. With the gap named truthfully, the
ordinary launch says `where 3 of the 4 checked early buyers got their money
could not be read` and the versioned-transaction mint says `4 of the 4`. The
old wording hid how often this happens. The cause is in
`check_solana_candidate`: tracing a buyer's funding means walking its own
signature history back to its oldest transaction, and an active wallet's
history truncates before that oldest page is reached, so no funder is
recorded at all (deliberately -- a funder read from a non-oldest transaction
would misattribute who financed the buy). On Solana this is the ordinary
case, not the exception, and it is the sole reason both readable mints are
still `CantTell`.

That makes buyer funding the next thing worth building, not another capture:
until a Solana buyer's first inbound transfer can be found without walking
its entire history, every ordinary launch will publish "can't tell" for this
one reason. `getSignaturesForAddress` paged from the oldest end, or a
first-transfer lookup that does not need the full walk, is the shape of the
answer.

Acceptance authority. `crates/realorrug-roast/tests/replay/README.md` says a
case lands there only after the owner has read the review and written `yes`.
For this set the owner delegated reply acceptance explicitly (2026-09-22,
"you can decide if the four replies are good"), and that delegation is what
these four `yes` lines rest on. The delegation covers these replies, not the
rule: a future set still needs the owner, or a fresh delegation.

## Addendum, 2026-09-22: buyer funding stops walking for the oldest transaction

The cause named in the previous addendum is fixed. `check_solana_candidate`
(`crates/realorrug-onchain/src/wallets.rs`) no longer walks a candidate's
signature history back to its oldest transaction; a new `funding_search`
pages the candidate's own history backward *from its first purchase of this
mint*, newest-first, and takes the most recent material inbound SOL transfer
at or before that purchase as the funder. `getSignaturesForAddress` already
pages newest-first, so this is the cheap direction: a wallet built to buy
one launch has its funding transfer somewhere in the handful of transactions
before the buy, not necessarily at the very start of its life, and finding
it never requires reaching the wallet's actual beginning.

Two new caps bound what a search will pay for a wallet a stranger could have
built to be expensive to read: `MAX_FUNDING_SIGNATURE_PAGES = 3` (matching
`Budget::PAGES_PER_WALK`'s own sizing) bounds how many `getSignaturesForAddress`
pages the search walks, and `MAX_FUNDING_TRANSACTIONS = 10` bounds how many
`getTransaction` calls it spends testing candidate signatures for a funder. A
wallet created for one launch shows a handful of transactions in this window
— the funding transfer, the buy, maybe one or two more — so both caps sit
well above that shape without opening the read to unbounded cost against a
wallet with thousands of signatures.

What "complete" means changed with it. `Candidate::funding_complete` is
`true` in exactly two cases: a material funder was found, or the search
paged back to the end of the candidate's own history (an empty or short
page, the same test `RpcClient::signatures_back_to_oldest` uses) having
fetched every eligible signature and found none material — a **measured**
absence, because the whole reachable window was actually read. It is
`false`, with a gap naming which cap was hit or which read failed, whenever
either cap stops the search or a signature-page or transaction read errors.
A capped or failed search is never recorded as a measured absence (AGENTS.md
rule 8): reaching a cap says nothing about whether a funder exists past the
point the search gave up.

`crates/realorrug-roast/src/sheet.rs`'s `funding_gap_message` (added by the
previous addendum) reads `Candidate::funding_complete` and needed no change:
the sentence it builds ("where N of the M checked early buyers got their
money could not be read") was already written for this definition of
complete, not the old one. On the two readable captures in this set —
previously "where 3 of the 4 checked early buyers got their money could not
be read" and "4 of the 4" — a fresh re-read after this fix is expected to
name a funder, or record a measured absence, for candidates that used to
report nothing at all; whether either mint's launch actually clears
`CantTell` is a question for the next capture, not settled here.

## Addendum, 2026-09-22: can a versioned transaction's dropped lookup-table accounts feed the wrong program id to the plain-transfer gate?

Asked while fixing `is_plain_sol_transfer`'s vacuous-empty-list bug
(`crates/realorrug-onchain/src/wallets.rs`, branch
`who-paid-the-solana-buyers-v2`): can `collect_instructions`
(`crates/realorrug-onchain/src/rpc.rs:890`) resolve a program id to the
*wrong* address, or silently drop an instruction, when a versioned
transaction's `accountKeys` omits addresses a lookup table supplied and
`meta.loadedAddresses` is not merged in? Answer, from reading the code, not
from a new capture: **not a wrong address, but yes, an instruction can be
dropped -- and the drop is exactly the shape that could let a swap pass the
plain-transfer gate.**

`RpcClient::transaction` (`rpc.rs:660`) already requests
`maxSupportedTransactionVersion: 1` with `encoding: "json"`, so instructions
carry a numeric `programIdIndex`, never a resolved address string --
`program_of` (`rpc.rs:921`) must look the index up in `accounts` itself.
`parse_transaction` (`rpc.rs:812`) already merges `meta.loadedAddresses`'s
`writable` then `readonly` arrays onto the end of the static `accountKeys`
(added by PR #145, this same document's "what the capture command got
wrong" section) -- the same order Solana's own account-indexing rule uses,
so *when the node returns `loadedAddresses`*, an index into a lookup-table
account resolves to the right address.

The residual case is when it does not: an RPC response for a versioned
transaction that used a lookup table but whose `meta.loadedAddresses` is
missing, null, or short (a different provider's shape, a malformed capture,
a future encoding change). Because dynamically-loaded accounts are always
indexed *after* every static account, an instruction naming one always
carries an index at or past `accountKeys`'s length. `program_of` reads that
index with `accounts.get(index)` inside a `filter_map`
(`collect_instructions`, `rpc.rs:896`), and `Vec::get` on an out-of-range
index returns `None`, not a wraparound or a fallback to some other real
account -- so `program_of` returns `None`, and `collect_instructions` skips
the instruction entirely (`continue`) rather than resolving it to a
different, wrong-but-existing program. **No instruction is ever attributed
to the wrong program by this path; a missing address means an instruction
disappears from `Transaction::instructions`, not that it appears under
someone else's name.**

That disappearance is still dangerous for exactly the reason this branch's
fix exists. If a swap's only non-System instruction is the one whose program
lived in the unmerged lookup table, dropping it can leave
`Transaction::instructions` holding *only* the System Program instructions
that were already present (fee payment, a wrapped-SOL account touch, and
so on) -- a **non-empty** list that reads as "every instruction is the
System Program" to `is_plain_sol_transfer`, because the swap's tell was
silently removed before the gate ever saw it. This is not the same failure
this branch's fix closes: the empty-list fix only catches the case where
*every* instruction was dropped. A partial drop that still leaves one or
more (all-System) instructions behind is not caught by `is_empty()` and
would pass the gate today, wrongly, exactly as the task worried. Whether
this happens in practice depends on whether every RPC provider the crate
talks to reliably returns `meta.loadedAddresses` for every versioned
transaction it can return at all -- not verified here, and not fixed here
per instruction; `rpc.rs` is unchanged by this commit.

## Sources

- DexScreener's public pair-search API (`api.dexscreener.com/latest/dex/search`),
  fetched directly, 2026-09-21, for candidate mints (fetched, not searched).
- `target/debug/realorrug.exe dossier <mint>` and `... capture <mint> --out
  ...`, run against `https://api.mainnet-beta.solana.com`, 2026-09-21.
