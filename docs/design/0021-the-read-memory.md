<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0021 — the read memory

**Status:** recording. The decisions below are the owner's, already made; this
document writes them up and works the one number he asked for by name (§5).
**Built so far:** the store (`crates/realorrug-onchain/src/memory.rs`), and
`realorrug_onchain::build` taking an optional memory in front of the Solana
launch-record read only (§1, "What is served from memory today"). Every
caller still passes none, so no running path uses it yet: turning it on needs
a state path for the file on the box, which is its own change.
**Date:** 2026-09-15.
**Facts from:** [research 0039](../research/0039-robinhood-chain-data-on-a-budget.md)
(the cost numbers and the volume assumptions) and
[research 0041](../research/0041-rules-or-a-reasoning-model-for-token-risk.md)
(code decides what to read; no model chooses reads). Every chain fact and
price cited below is theirs, with its own source and date; this document does
not re-derive them.
**Governed by:** [AGENTS.md](../../AGENTS.md) §1 ("absent is not zero"), §3
rule 7 (deny by default when config is missing) and §4 (no path from a
model-side crate to the payout).
**Scope:** what the analyst remembers about a token or a creator between
chain reads, what a fact costs monthly to keep current, and where the memory
lives. Not the fact sheet's shape or the model's voice (design 0020), not
threaded follow-ups (design 0022), not the checker page (design 0023) — §7.

## 1. What is stored, keyed how

A stored fact is identified by three parts: **what** (a named kind — `holder
count`, `curve reserves`, `creator's prior launches`, and so on), **which**
token or address the fact is about, and **at which block** it was read. The
key is `(what, subject, block)`, not `(what, subject)`.

A key that omits the block is wrong for a reason the shelf-life table in §2
depends on: two facts of the same kind about the same subject, read at two
different blocks, are not the same fact — they are two observations, and the
older one does not stop being true just because a newer one exists. A holder
count read at block N and again at block N+900 are both real; the memory
needs to know which one it is holding and how old it is, and "how old" is
counted in blocks first (a block cannot un-happen, AGENTS.md §1) with the
wall-clock read time carried alongside it only to compare against a shelf
life stated in minutes or days. A key of `(what, subject)` alone would let a
second read silently overwrite the first with no record of which one is
current, and — for the *forever* kind in §2 — would make a re-read look like
it "updated" a fact that cannot change, hiding a bug (a launch record that
came back different) behind a key design that cannot represent two different
answers to have a bug in the first place.

Every stored fact therefore carries: the key `(what, subject, block)`, the
wall-clock time it was read, and the kind it belongs to (§2), which is what
decides its shelf life. The subject is a token address for token-scoped
facts (curve reserves, holder count, the launch record itself) and a
creator address for creator-scoped facts (prior launches, prior
graduations) — this is what makes the creator-level saving in §2's "nothing
bought twice" claim possible: two tokens from the same creator share one
`(creator track record, creator address, block)` entry, not two.

**What is served from memory today.** Only the launch record. It is the
costliest read (signature paging back to the oldest one) and a `Forever`
fact, so serving it cannot make the sheet lie: a sheet states one read point
for everything on it, and a past launch does not depend on when it was read.
The reserves and the creator-activity reads are always bought fresh, even
when a row exists. That is deferred, not rejected: it becomes safe once the
fact sheet (design 0020) carries a read point per fact instead of one for
the whole sheet. A stored launch record that disagrees with a fresh read is
refused as a conflict and the launch is reported unreadable, never resolved
silently either way.

## 2. The shelf-life table

| kind | shelf life | why that number | on expiry |
|---|---|---|---|
| **forever** — launch record, creator address, launch block, the graduation event and the quote it raised | never expires | forced by a chain fact, not chosen: a past block is settled the moment the network accepts it (AGENTS.md §1, "a capture disposes"), so nothing later can make a block-26,921,206 read wrong. Storing an expiry for it would imply a past fact could go stale, which is false. | never — the fact is read once, ever, per subject, and every later question is answered from the stored copy. This is the largest share of "nothing bought twice": research 0039's own dominant cost line, one `getLaunchedToken` call per launch, is exactly this kind, and it is already never repeated once the memory holds the answer. |
| **~10 minutes** — holders, balances, curve reserves, trade counts | 10 minutes | the **owner's choice**, not forced. These move continuously (a swap changes reserves and balances in the same block it lands in), so any shelf life is a bet between staleness and re-fetch cost; ten minutes was picked as a round number close to "a reply is read within minutes of being posted" (research 0039's reply-latency framing) without pinning it to a specific block count, since Robinhood Chain's ≈0.1 s block time (research 0038, cited via 0039) would make a block-denominated shelf life for this kind an oddly large number of blocks for a human-legible ten minutes. | the stored copy becomes **stale**, not deleted (§3) — it stays in the memory, keyed to the block it was read at, available if the analyst decides a stale-but-recent number is still useful context, but it no longer satisfies a read that requires a fresh fact. |
| **daily** — the creator's track record (prior launches, prior graduations) | 24 hours | the **owner's choice**. A creator's history changes only when that creator launches or graduates again, which research 0038 (via 0039) shows is not a per-minute event even for an active serial launcher; a day trades a little lag for one shared read serving every token that creator has ever launched, all day, at any of their other tokens' mention volume. | same as above: stale, kept, not deleted. |

## 3. Stale versus missing

**The rule: a stale fact is not deleted.** It stays in the memory with its
`(what, subject, block)` key, its read time, and its kind — a reader (human
or the fact-sheet builder) can see that a fact exists and how old it is. A
**missing** fact is one the memory has never successfully read at all, or
one the RPC call for it failed and the memory holds nothing to fall back on.
The two are different states with a different consequence:

- **Missing** is silence: the analyst does not know, and did not recently
  know either. Nothing to compare against, nothing to hedge with.
- **Stale** is a known answer with a known age: the analyst knew, as of
  block B, and it is choosing whether block B is recent enough to answer
  with.

**Does a stale required fact make the verdict `CantTell`, the same as a
missing one? Yes — for the same reason ADR 0027 gives for missing, and no
softer treatment is defensible.** [ADR 0027](../adr/0027-the-bot-gives-verdicts-it-can-prove.md)
requires `CantTell` whenever "a fact we needed could not be read," and is
explicit that getting this backwards "would turn a blind spot into an
endorsement, which is worse than the shrug." A stale required fact is a
blind spot with a timestamp on it, not a lesser one: the verdict ladder's
top two levels (`Rugged`, `RugMechanicsLive`) are reached by a *combination*
of live signals (ADR 0027 rule 4), and a combination built partly from a
ten-minute-old reserves figure on a chain that produces roughly six blocks a
second is a combination the analyst cannot actually stand behind at the
moment it speaks — the ten minutes is exactly the owner's own bet in §2 that
this kind of fact might already be wrong. Treating a stale required fact as
still good enough would mean the shelf-life table in §2 stops doing any
work: if a fact is used as fresh right up until (and past) the moment it is
deleted, there was no point choosing a shelf life shorter than "never." The
one place staleness is allowed to answer a question at all is a place that
does not claim freshness — a display of "as of block B, N minutes ago" is
honest about its own age and is not what "required fact" means here. So: a
missing fact and a stale required fact are the same `CantTell` trigger,
reached by different paths (never read vs. read-but-aged-out), and the
document that wires this into the fact sheet (design 0020) should treat
"the memory has a fresh answer for this key" as the one predicate that
gates whether a required fact counts as present, folding both "never read"
and "read but expired" into its negative case rather than only checking for
a stored value.

The boundary in the other direction: the memory is never the source of a
fact the bot could not read today. A row is a receipt of a past chain read,
not a substitute for one. A missing memory, or a row that no longer decodes,
means read the chain; it makes an answer slower and costlier, never
different. That is why no memory is not rule 7's deny-by-default case.

## 4. Where it lives

**Recommendation: SQLite, one file on disk, in `realorrug-onchain`.** The
deciding tradeoff is the one the packet names: this runs as a daemon on one
small box and must survive a restart. That rules out memory-only outright —
a restart (deploy, crash, reboot) would silently return the analyst to a
cold cache, and every stored forever fact (§2), including the ones research
0039 shows are the most expensive to re-earn, would need re-buying on the
next question about each token. A restart-safe design must write to disk.

Between a plain file and SQLite, the deciding factor is the key shape in
§1: `(what, subject, block)` is a lookup by two or three fields at once
(is there a fresh `curve reserves` row for this token; is there *any*
`creator track record` row for this creator newer than 24 hours ago), which
a flat file forces into a full scan or a hand-rolled index, while SQLite
gives an index on exactly that key for free, with atomic single-writer
commits that survive a crash mid-write without corrupting the store — a
property a hand-rolled file format would have to earn the hard way. SQLite
also needs no server process, no network port and no second thing to keep
running on a two-core box (research 0039 §6 already found that box cannot
host even a pruned chain node; it should not be asked to run a database
server either).

**The crate: `realorrug-onchain`.** Its own description is "Reads a token's
launch block and curve from RPC on demand, for a question asked about a
mint that may be forty seconds old" — the memory is what decides whether
"on demand" actually needs to touch RPC at all, so it belongs in front of
the calls that crate already makes, not in a new crate duplicating its
job. `realorrug-onchain` has no dependency on a payout crate and no crate
that depends on it reaches the payout: `realorrug-roast` (the fact-sheet
builder, which needs the memory to answer §3's fresh-or-not question)
depends on `realorrug-onchain`, and `realorrug-payout` depends on
`realorrug-contest`, a separate line entirely. Putting the memory here
keeps `AGENTS.md` §4's rule intact: there is still no path from a
model-holding crate to the payout key, because the memory only ever
answers "do I already know this," never anything the payout binary reads.

## 5. The monthly cost, in credits and in money

Volumes are research 0039's own, cited, not re-derived: **1,008,000 launches
indexed/month**, **300 replies/day (≈135,000/month)** and **300
seven-days-later checks/day × 5 reads (≈45,000/month)** at today's rate.

**Backfill/index cost (separate from the per-question cost, unaffected by
the memory).** Every launch's forever facts (§2) are read exactly once, at
index time, by design — the memory cannot make a first read cheaper, only
stop it from happening twice. Per research 0039 §2: one batched
`eth_getLogs` scan for new launches (≈43,200 calls/month, once a minute)
plus one `getLaunchedToken` `eth_call` per launch (1,008,000/month).
**Index total: 1,051,200 calls/month.** (The one-off historical backfill,
≈171,000 calls from block 26,921,206 per research 0039, is a single spike
whenever it is run, not a recurring monthly cost, and is not added to the
monthly total below.)

**Per-question cost, with the memory.** A reply's forever and daily facts
(launch record, creator address, launch block, graduation+quote, creator
track record) are already in the memory by the time anyone asks, because
indexing wrote them — those cost nothing marginal per question. Only the
~10-minute kind (holders, balances, curve reserves, trade counts) can be
stale by the time a question arrives. **Assumption, labelled:** 4 of a
reply's reads are this kind, and 2 of the seven-days-later check's 5 reads
are; **assumption, labelled:** 30% of those requests land inside an
already-fresh 10-minute window — a threaded follow-up on a trending token,
or a second question about the same token minutes after the first — so 70%
still fetch fresh.

- Replies: 300/day × 4 × 0.70 = 840/day → **25,200/month**.
- Seven-day checks: 300/day × 2 × 0.70 = 420/day → **12,600/month**.
- **Question-side total with memory: 37,800/month.**

**The saving the memory buys.** Without it, every read in research 0039's
own 15-reads-a-reply and 5-reads-a-check figures is fetched fresh every
time: 135,000 + 45,000 = **180,000/month, question-side, no memory.** With
the memory, question-side drops to 37,800/month — **a 79% cut** on the part
of the bill the memory controls. It does not touch the 1,051,200/month
index cost, which is the larger number either way and is already
"never bought twice" by construction (§2), which is why research 0039 found
the launch-indexing call dominates the bill, not the bot's own reply logic.

**Total calls/month with the memory: 1,051,200 + 37,800 = 1,089,000.**
Without it: 1,051,200 + 180,000 = 1,231,200.

### QuickNode (main; free trial 10,000,000 credits, 20 credits/call)

| | with memory | without memory |
|---|---|---|
| calls/month | 1,089,000 | 1,231,200 |
| credits/month | 21,780,000 | 24,624,000 |

**The free trial does not cover a month, with or without the memory.** It
is a one-time 10,000,000-credit pool, not a monthly grant — 10,000,000 ÷ 20
= **500,000 calls total**, and the index alone (1,051,200 calls/month)
exceeds that in the first two weeks regardless of question volume: at
21,780,000 credits/month (with memory) the trial lasts 10,000,000 ÷
21,780,000 ≈ **0.46 month, ≈13.8 days**; without memory, ≈12.2 days. The
memory buys about a day and a half of extra trial life — a modest gain,
because the trial's ceiling is set by indexing, which the memory cannot
shrink. **On QuickNode's paid Build plan** ($49/month flat, 80,000,000
credits, per research 0039 §2), 21,780,000 credits is ≈27% of the plan:
**$49/month flat**, same conclusion research 0039 already reached,
confirmed rather than changed by adding the memory.

### Alchemy (fallback; free plan 30,000,000 CU/month, 26 CU/`eth_call`, 60 CU/`eth_getLogs`)

| | with memory | without memory |
|---|---|---|
| index CU (1,008,000 × 26) + (43,200 × 60) | 26,208,000 + 2,592,000 = 28,800,000 | same, 28,800,000 |
| question-side CU (calls × 26) | 37,800 × 26 = 982,800 | 180,000 × 26 = 4,680,000 |
| **total CU/month** | **29,782,800** | **33,480,000** |

**The free plan covers today's volume only with the memory.** 29,782,800 is
99.3% of the 30,000,000/month cap — essentially no headroom, but it fits.
Without the memory, 33,480,000 exceeds the cap by 3,480,000 CU (about 111.6%
of the allowance): **the free plan alone would not cover it.** Put the other
way: after indexing's fixed 28,800,000 CU, only 1,200,000 CU/month of free
headroom remains for question-side traffic — 1,200,000 ÷ 26 ≈ **46,150
eth_call-equivalent reads/month** before the free plan runs out. With the
memory the analyst uses 37,800 of that budget (room to spare); without it,
180,000 blows past the 46,150 ceiling almost fourfold. **If paying
pay-as-you-go instead of the free plan** (research 0039's $0.525/1,000,000
CU rate): 29,782,800 CU ≈ **$15.64/month** with the memory, ≈$17.58/month
without — a small dollar gap, because Alchemy's marginal rate is cheap; the
memory's real effect on Alchemy is keeping it on the free plan at all, not
the few dollars saved on the paid one.

**Monthly totals, both providers, today's volume, with the memory:**
QuickNode ≈21.78M credits (no plan at $0 covers this; $49/month Build plan
does); Alchemy ≈29.78M CU (the free plan covers it, at 99% utilization).

## 6. The fallback

**What must change.** Nothing in the tree today reads either
`REALORRUG_RPC_URL` or `REALORRUG_RPC_FALLBACK_URL` — confirmed by grep
against the source tree during this write-up. Two different variables are
actually read: `REALORRUG_RPC` (falling back to the legacy `RADAR_RPC`) in
`crates/realorrug-onchain/src/rpc.rs`
(`RpcClient::from_vars`), and `REALORRUG_ROBINHOOD_RPC` in
`crates/realorrug-payout/src/main.rs`, on the pattern already documented in
`deploy/payout.env.example` ("Its own name, so the analyst's Solana
`RADAR_RPC_URL` is never picked up"). `deploy/analyst.env.example` itself
records the last time a name mismatch like this happened: it said
`RADAR_RPC_URL` until 2026-09-06 while the code read `RADAR_RPC`, silently
handing an operator the free public endpoint instead of the paid one they
thought they configured, with nothing that said so. The memory's on-demand
reads (§2's ~10-minute and daily kinds, and any cold-cache forever read)
need a Robinhood Chain RPC client with both a primary and a fallback
endpoint wired to whatever names are actually read — `REALORRUG_RPC_URL`
and `REALORRUG_RPC_FALLBACK_URL` if the owner's names are adopted going
forward, reconciled with the existing `REALORRUG_ROBINHOOD_RPC` naming
already in `deploy/payout.env.example` so the tree does not end up with two
different names for the same Robinhood Chain endpoint the way it once had
two names for the Solana one. Which exact name wins is a naming decision
this document does not make (§8) — the design point is that the memory's
RPC client must read whichever names are decided, and `deploy/*.env.example`
must be corrected in the same commit that wires it up, not left to repeat
the `RADAR_RPC` / `RADAR_RPC_URL` drift.

**When the fallback is used.** Specifically: (1) the primary returns a
budget/quota-exhaustion response (QuickNode credit exhaustion, or an HTTP
429 of the kind research 0038/0039 already measured against the free public
endpoint); (2) the primary is rate-limited (a 429, or a connection refused
under burst); (3) the primary returns a transport or server error (timeout,
5xx, malformed response) rather than a valid JSON-RPC result. A read that
the primary answers normally, including one it answers with an on-chain
"not found," is not a fallback trigger — only a failure of the RPC *call
itself* is.

**When both are missing.** `AGENTS.md` §3 rule 7 is deny by default: no
credential posts nothing, no budget spends. Applied here, an analyst with
neither `REALORRUG_RPC_URL`-equivalent set has no primary to read from at
all, which is not a "use the free public endpoint" default to invent — that
would repeat the exact silent-degrade failure `deploy/analyst.env.example`
already documents once (§ above), just at the fallback layer instead of the
primary. The analyst treats every read as **missing** (§3): no RPC call is
attempted, the fact is absent, and any required fact built from it makes
the verdict `CantTell`. This is not a special case of §3's rule — it is the
same rule, applied to a missing endpoint instead of a missing chain fact.

## 7. What this does not decide

- **Design 0020** (the fact sheet and the voice, in flight) owns how a
  `CantTell`-triggering stale or missing required fact is actually wired
  into the verdict computation and into `forbidden.rs`'s level check (ADR
  0027 rule 6) — this document only settles that a stale required fact must
  trigger it (§3), not the code that reads the memory to decide.
- **Design 0022** (threaded follow-ups) owns whether a second question in
  the same thread, seconds or minutes after the first, is treated as "the
  same mention" for rate-limit and dedupe purposes distinct from this
  document's "same fact, still fresh" question — this document's §5 assumes
  a 30% within-window hit rate for that pattern but does not decide how a
  thread is detected or bounded; design 0022 owes this document the actual
  detection rule so the hit-rate assumption can be checked against it.
- **Design 0023** (the checker page) owns whether a page view reads the
  memory the same way a bot mention does, or issues its own reads outside
  this budget — this document's cost table in §5 counts only mention
  traffic (research 0039's 300/day), and design 0023 owes this document its
  own expected read volume before that page's cost can be added to §5's
  totals.

## 8. Not established

- The final environment-variable names for the primary and fallback
  Robinhood Chain RPC endpoints, and who reconciles `REALORRUG_ROBINHOOD_RPC`
  (already in `deploy/payout.env.example`) against the owner's stated
  `REALORRUG_RPC_URL` / `REALORRUG_RPC_FALLBACK_URL` — recorded as a
  decision still to make in §6, not resolved here.
- Whether QuickNode's 20-credits-per-call rate actually applies to Robinhood
  Chain specifically — research 0039 §8 already flagged this as unread from
  QuickNode's own per-chain pricing.
- Whether Alchemy keeps archive state for chain 4663, which would matter for
  any cold-cache forever-kind read the memory has to serve for a token
  older than Alchemy's non-archive retention window — research 0039 §8,
  unchanged here.
- The actual within-window repeat-mention rate assumed at 30% in §5 — no
  mention-volume data exists yet to check it against; design 0022's thread
  model (§7) is the document that could eventually measure it.
- SQLite's specific schema, migration path, and the exact crate dependency
  that would add it to `realorrug-onchain` — this document recommends the
  engine and the crate (§4), not the table layout, which is implementation,
  not design.
