<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0026 — the trader dossier

**Status:** draft, written from Josh's instruction of 2026-09-17 and not yet
reviewed with him. **Amends:** nothing yet; §4 and §7 name the changes to
[design 0020](0020-robinhood-fact-sheet-and-voice.md) that shipping this
requires, and §7's typed composition is a narrowing of 0020's free-text
voice that will need its own ADR before it is built.
**Consequence of:** design 0020 (the fact sheet, the five verdict levels,
the post-generation checks), [design 0021](0021-the-read-memory.md) (the
read memory this extends rather than replaces),
[design 0022](0022-threaded-follow-ups.md) (a follow-up digs into one more
thing), ADR 0027 (code picks the level, never the model),
[research 0050](../research/0050-robinhood-trader-signals.md) §8 (what the free
Alchemy key can and cannot do, measured on the production box).
**Date:** 2026-09-17.
**Does not decide:** the share card, the site, the launch. See §10.

## What Josh asked for

He summoned the bot on a real token on 2026-09-17 and got this back:

> Radar on 0x13e6cdb0470b10afcb96177ae8702ace2ac72cd6: - not known: the
> holders could not be read Launched about 2.8 hours ago. Read at block
> 65543752. Measured, not predicted. Not financial advice.

His words for what is wrong with it: nobody cares which block it read, it
is "a dumb random data generator", and what he wants instead is "an advanced
research trader that anyone can talk to" — bundle analysis, wallet analysis,
tokenomics, liquidity, wash trading, picking out the biggest flags, teaching
the reader something, saying what it thinks even when the truth is brutal,
meme-worthy, and remembering what it learns so the evidence keeps building.
The standing disclaimer moves to the profile, where it is said once.

## Do not re-litigate

These are his decisions. This document builds on them:

- **The bot has an opinion and says it.** Brutal is allowed.
- **The disclaimer lives in the bio**, not in every reply. (Already built:
  `crates/realorrug-analyst/src/bio.rs` folds it into the configured lead.)
- **Every new question is a chance to dig deeper**, and a reply must never
  be built out of stale facts. If a thread already covered something and it
  is still fresh, spin a new angle rather than repeating.
- **Free data first.** No paid plan until measured demand needs one.

And these are the rules that do not bend for any of it (AGENTS.md §3): the
model may not introduce a fact, code picks the verdict level, never the price
or market cap, never an accusation against a named person, absent is not
zero, unknown is not safe.

## 1. The two things actually broken, before any new detector

**The bad reply Josh saw was not written by a model at all.** It is the
deterministic fallback in `realorrug_roast::verdict::template`
(`crates/realorrug-roast/src/verdict.rs:339`), which
`realorrug_roast::voice::write` (`crates/realorrug-roast/src/voice.rs:186`)
ships whenever the provider is absent, unreachable or rejected. That path
already records itself — `answer.rs:365` logs `fellback` with the reason —
so the first operational question is what percentage of live replies fall
back. If it is most of them, every hour spent on the system prompt changes
nothing that anyone reads. **Read the fallback rate before building
anything.**

**The "no invented facts" guarantee is narrower than rule 2 claims.**
`realorrug_roast::fidelity::check`
(`crates/realorrug-roast/src/fidelity.rs:57`) compares the *numerals* in a
reply against the numbers the sheet authorised. A model that writes "the
deployer has done this before" with no number in the sentence passes every
check we have. Rule 2 says the model may not introduce a fact. Today the
code enforces "may not introduce a number". §7 closes that, and it is the
reason this design constrains the voice rather than freeing it.

Everything after this section is worth less than these two paragraphs.

## 2. What the data can actually do

Measured live against Josh's own Alchemy free key from the production box on
2026-09-17; the full record is research 0050 §8, which corrects that
document's own earlier sections. In short:

- `alchemy_getAssetTransfers` **works** for `external` (native) and `erc20`,
  in both directions, with `withMetadata`, `order: asc` and `pageKey` paging.
  That is the address-indexed reverse lookup the earlier research said did
  not exist, and it is what makes funding-source analysis possible at all.
- `internal` category, `trace_block`, `trace_filter` and every `qn_*` method
  are **not available**.
- `debug_traceBlockByNumber` with `callTracer` works, at about 215 ms and
  660 KB for one block. A three-hour-old token is roughly 107,000 blocks
  deep, so tracing a token's history is about 65 GB. It is off by default.
- Archive reads work at every depth tried, including one million blocks
  back. The "ten minutes of history" figure in the older research applies to
  the keyless public endpoint, not to the key.
- `eth_getLogs` caps at 10,000 **results**, not at a block range, so a busy
  token needs range bisection, never a shorter window.
- Block time is 0.101 s, measured (1000 blocks = 101 s).

One live trace already found what this is for: a buyer of a real token had
four of its first five native top-ups from one address. Two extra calls per
candidate wallet. Earlier probes came back empty because they sampled
contracts; filtering on `eth_getCode == "0x"` fixed it.

## 3. The signal catalogue

Each signal is a check with an exact read, a threshold, and — this is not
optional — **the innocent explanation that ships beside it**. A trader who
only ever names the guilty reading is not an analyst, and a bot that does it
in public is a liability (§8).

Definitions used below: *Pₜ* is a page of token transfers, *Pₓ* a page of
curve trades, *N ≤ 4* the top launch-block buyers by quote amount, and the
*funding window* the 18,000 blocks (~30 minutes) before the launch block.

| Signal | The read | Flags at | Innocent reading |
|---|---|---|---|
| `CommonFundingCluster` | `eth_getCode` then `alchemy_getAssetTransfers` on each of the top N buyers | ≥3 of 4 share one direct native funder **and** hold ≥50% of launch-block quote | That funder is an exchange, a bridge or a launch service |
| `FreshLaunchCluster` | archive `eth_getTransactionCount` + `eth_getBalance` before the window | ≥3 of 4 had nonce zero and zero balance before the window, funded inside it | New users arrive in bursts around a launch people are watching |
| `LinkedSupplyMajority` | token `Transfer` ledger | linked wallets hold >50% outside curve and pool | Vesting, a treasury, or an exchange's own address |
| `DeployerSoldMajority` | deployer's curve sells vs mint | >50% of the deployer's allocation sold | Taking some off the table is normal |
| `HolderConcentration` | `Transfer` ledger | top unlabeled address >25% outside curve and pool | Contracts hold supply for honest reasons |
| `ThinLiquidity` | pool reserves at the answer block | depth below the launch-block quote total | A young token is thin by definition |
| `LiquidityRemovalObserved` | v4 pool events | a removal completed | A rebalance also removes |
| `RoundTripVolumeConcentrated` | curve trades | one cluster's gross buy+sell volume over half, net exposure near zero | Market making looks identical |
| `SelfFundedChurn` | funding graph over trade ledger | traders funded by one address trading each other | Same |
| `CoordinatedSelling` | trades by block | linked wallets selling in one block | A price move makes everyone sell at once |

The existing eight `LIVE_RISK_SIGNALS` in `verdict.rs` stay. Two of them
change meaning: a nonzero dev buy stops being a risk signal and becomes
context, because on a bonding curve the deployer buying their own launch is
the ordinary case.

**Not worth their cost**, and the reasons are numbers, not taste:
per-answer block tracing (65 GB for a three-hour token); asset-transfer
lookups on every holder (120 credits each); multi-hop ownership inference
(it is a guess dressed as a finding, and §8's first row is what it costs
when it is wrong); `balanceOf` per holder when the transfer ledger already
has it; liquidity-lock scanning until Pons v2's custody is actually decoded.

## 4. The verdict, without the model touching it

Five levels stay. A sixth would be a safety score, and this project does not
publish one.

The change is **what counts as two signals**. Today `verdict::level` counts
enum variants: two correlated readings of one pattern — a common funder and
the fresh wallets that funder paid — count as two votes for the same fact.
Group them into five families instead:

`LaunchStructure`, `DeployerBehaviour`, `Ownership`, `ExitLiquidity`,
`TradingIntegrity`.

- `RugMechanicsLive` requires the curve still live, an `ExitLiquidity`
  finding, and one finding from an **independent** family.
- `Sketchy` is any enabled finding that does not reach that bar.
- `CantTell` still wins over everything when a required fact is missing.

In `crates/realorrug-roast/src/verdict.rs`: replace the flat signal count
with an exhaustive `family(Signal)` mapping (exhaustive so a new signal
cannot be added without deciding what it is evidence *of*), add a typed
`Finding { code, family, evidence, threshold_version }`, keep `Verdict::from`
pure.

In `crates/realorrug-roast/src/sheet.rs`: add `TokenState::{Curve,
Graduated, Unknown}`; give every fact a stable id, an `observed_at` block
and a `complete_through` block; replace the free-text `unknown` list with a
typed `Gap { fact, required, reason }`; apply the price rule to every token,
not only the self-mint.

## 5. What it remembers

Extend the SQLite store already in `crates/realorrug-onchain/src/memory.rs`.
No new service, no new process: this runs on one small VPS.

Four tables:

- `observations`, keyed `(kind, subject, observed_block)` — things that were
  true at a block and stay true about that block forever.
- `check_runs`, keyed `(token, check_id, algorithm_version)` — so a
  re-answer knows which checks it has already paid for, and so raising a
  threshold invalidates the old runs rather than silently changing history.
- `token_wallet_edges` — the funder graph, which is where "this address has
  now shown up in four launches" comes from.
- `thread_evidence`, keyed `(conversation_id, token, fact_or_finding_id,
  emitted_at)` — what this thread has already been told.

That last one should replace `ThreadRecord.answered`
(`crates/realorrug-analyst/src/followup.rs:187`), which is recorded nowhere
and read nowhere: it is initialised empty, asserted empty in one test, and
never written. Design 0022's "a second 'is it bundled' is answered from what
the thread already holds" is not implemented today.

**Freshness, which is Josh's correction in table form.** Phase, reserves and
deployer balance go stale after 600 blocks (about a minute). Ledgers are
stale when 1,200 blocks behind. Public pool depth is good for 60 seconds.
Historical observations never expire. A second question re-reads whatever is
stale — it does not answer from a warm cache and call it fresh.

**Getting deeper, answer by answer.** Answer one runs the core. Answer two
adds common funding and fresh wallets if they have never run. Answer three
adds self-funded volume or coordinated selling. That is design 0022's "one
more thing" made concrete, and it is why a thread rewards asking twice.

**Cross-token memory** accumulates over 90 days, flags a funder at three or
more analysed launches, and only after named infrastructure is excluded.
Edges expire at 90 days, dynamic snapshots 30 days after a token's last
question, whole store capped at 2 GB, WAL on, raw traces never stored.

## 6. The budget, in numbers

Alchemy's free tier is 30M compute units a month. Cost per call: `eth_call`
26, `eth_getLogs` 60, ordinary reads 20, `alchemy_getAssetTransfers` 120.

`Budget` (`crates/realorrug-onchain/src/budget.rs:70`) counts calls as
equal today. They are not. Replace `take_call` with a weighted reservation:
**1,200 CU and 20 calls per answer**, 3 log pages, 20 seconds of wall clock,
paced at 240 CU/s — and a check reserves its worst case before it starts, so
a budget can never run out halfway through a finding and publish half of it.

Measured shapes: cold core 8 requests / ~292 CU; warm refresh 5 / ~192 CU;
the four-wallet funding check 12 / 640 CU; self-funding 8 / 960 CU.

Six hundred answers a day is 21.6M CU, plus about 2.6M for keeping the
launch-factory index current: ~24.2M against 30M, leaving ~5.8M of headroom.
**No paid plan.** Revisit only when measured daily requests approach the
allowance — not before, and not on a vendor's recommendation.

An exhausted budget still buys the ~192–292 CU core, which is a real answer
rather than today's "holders could not be read" shrug.

## 7. Voice — and where I disagree with the ask

The reply leads with the strongest token-specific finding, explains in one
line why it matters, carries the innocent twin whenever the verdict is not
conclusive, and ends on the level the code chose. No disclaimer.

**Where I disagree:** "a human expert writing freely" and "the model may not
introduce a fact" cannot both be true, and §1 shows the check that is
supposed to reconcile them only looks at numerals. Free prose about wallets
and funders is exactly where a fluent invention would land, and it is the
kind that gets a bot sued rather than corrected.

What I would ship instead is a **typed reply plan**. The model chooses a
finding, the facts that support it, a register, and one of a fixed set of
opinion angles — nonfactual reactions like "the exit door was painted on",
owned by code and enabled only at levels that earn them. Code renders the
factual clauses and appends the verdict. The model cannot write prose
outside those choices. The deterministic fallback composes through the same
path, so a provider outage sounds like the product instead of a database
dump — which fixes §1's first problem as a side effect.

That is less free than design 0020's current prompt. It is the version where
rule 2 is actually true.

Length: aim for 220–240 weighted characters, validate against X's weighted
rules, hard-refuse over 280. Do **not** truncate at 280 the way
`render::for_publication` (`crates/realorrug-roast/src/render.rs:68`) does
today — cutting the tail can sever the evidence from the verdict it earned.
On overflow, re-render deterministically with one finding instead of two. No
second model call.

### Three replies this would produce

**Rugged.** Sheet: pool depth at the last complete read 31.8 ETH; now 0.4
ETH; 73 addresses still holding outside the pool; a small-sell simulation at
block 65543752 failed.

> Pool depth went from 31.8 ETH to 0.4 ETH while 73 addresses still held
> tokens, and the small-sell check failed at block 65543752. The exit door
> was painted on. Rugged.

Every numeral is a sheet fact; the metaphor is a code-owned angle the level
unlocked; "Rugged" is appended, not chosen.

**Sketchy.** Sheet: largest unlabeled address holds 61.4% outside curve and
pool; its type is not established; the twin says it may be a vesting or
exchange contract; `HolderConcentration` with no completed exit finding.

> One unlabeled address holds 61.4% of the supply outside the curve and
> pool. It may be vesting or an exchange contract, but that concentration is
> the whole story here. Sketchy.

**NothingUglyYet.** Sheet: about 6.3 hours old; 184 addresses hold it
outside the curve; top address 8.7%; all required checks complete through
the answer block.

> At 6.3 hours old, 184 addresses hold it and the top address has 8.7%
> outside the curve. Nothing ugly showed up in the checks run yet — the
> absence of a flag is a timestamp, not a halo.

None states a price, none names a person, none carries a disclaimer, and the
third refuses to be read as reassurance, which is the whole reason that
level is worded the way it is.

## 8. What goes wrong

| Failure | Cheapest guard |
|---|---|
| The common funder is an exchange or a bridge | Named-infrastructure exclusions; say "direct funder"; always ship the twin; never infer common ownership |
| "Wash trading" becomes an allegation of intent | Never publish the phrase. Publish gross round-trip volume, net exposure, funding relationships |
| A partial log scan reads as low activity | Every ledger carries `complete_through`; a 10,000-result answer bisects; an unfinished range is unknown, never zero |
| Facts read at different blocks | Pin state reads to one answer block, keep its hash; a finding spanning blocks states both observations |
| A reorg erases an observation | Store block hashes; invalidate when a later canonical read disagrees |
| Funding arrived by an internal transfer we cannot see | Claim only "direct transfer observed". Never "no connection" |
| A pool API returns the wrong pair | Accept only the factory-derived pool address; discard symbol search and every price field |
| An address is described as a person | Keep the word "address" in the fact and in the sentence; no "whale", no "insider" |
| Correlated signals double-count | Count families, not variants (§4) |
| An uncalibrated threshold sounds certain | Every statistical finding ships in shadow mode first, with a versioned sample and a minimum population, before it can move a level |
| The model invents a non-numeric claim | §7's typed composition. `fidelity` cannot guard this today |
| Overlong or broken copy | Weighted length check, then a deterministic shorter render; never blind truncation |
| Dropping the disclaimer raises advice exposure | Bio keeps it; `forbidden.rs` keeps the unconditional advice and prediction bans; replies state facts, not recommendations |
| "Brutal" turns into defamation | Brutality lands on the measured setup, never on a named actor |
| Rate limits or a spent month | §6's weighted reservation, pacing, cursors, admission ceiling, no default tracing |
| SQLite fills the VPS | WAL, summaries not traces, 30/90-day expiry, 2 GB ceiling |
| The new voice never ships because everything falls back | Dashboard `fellback` before rollout and alert on the rate (§1) |

## 9. The order to build it

1. **A safe floor that already sounds like a trader.** Typed composer over
   the facts we already read, disclaimer out of replies, blind truncation
   gone. No new chain reads at all. *Proof:* one test per level showing the
   fallback passes every check, carries no disclaimer, fits X's weighted
   length, and changes its opening line when the strongest fact changes.
2. **Per-fact freshness and the weighted budget.** *Proof:* a stale required
   fact forces a re-read; a failed re-read lands on `CantTell`; a missing
   optional fact does not; a check cannot start without its full
   reservation.
3. **Complete ledgers.** Range bisection for transfers and curve trades,
   cursors in SQLite. *Proof:* a fixture that overflows 10,000 bisects to
   the same ledger an uncapped answer gives; dropping one page makes the
   fact unavailable, not smaller.
4. **The core dossier.** Supply split, real deployer sells, deployer
   balance, phase and reserves, local creator history; dev buy demoted to
   context. *Proof:* a transfer is not counted as a sell; a 50%-sold fixture
   fires; an unlabeled pool is excluded.
5. **The launch funding graph.** Top-four sampling, direct funders, archive
   nonce and balance, fresh-cluster findings, cross-token funder memory.
   *Proof:* three-of-four fires and two-of-four does not; named
   infrastructure suppresses it; an unobservable internal path yields scoped
   uncertainty, never "no link"; the whole check stays under 1,200 CU.
6. **Liquidity and trading integrity.** Verified pool depth, v4 removals,
   round-trip volume, linked same-block selling — thresholds in shadow mode.
   *Proof:* wrong-pool data is rejected; a rebalance alone cannot reach
   `Rugged`; incomplete history fires nothing; linked same-block sellers
   trigger where unrelated ones do not.
7. **Families and progressive follow-ups.** `verdict::level` by family, only
   calibrated signals enabled, every fresh question refreshes stale core and
   runs one unseen check, emitted facts recorded. *Proof:* two correlated
   launch findings stay `Sketchy`; an exit finding plus an independent
   family on a live curve reaches `RugMechanicsLive`, and the same pair on a
   graduated token does not; three questions about one token get
   progressively deeper without repeating the lead.

## 10. Not decided here

The share card image, the site, the launch checklist, the payout rehearsal.
The ADR narrowing design 0020's free-text voice to §7's typed composition is
not written; slice 1 cannot merge without it.
