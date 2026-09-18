<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0027 — the three layers

**Status:** Proposed. This is a reasoning record of a golden-rounds review,
not an accepted design. Nothing here is implemented; §6 lists the rule
changes it would need and marks them explicitly not decided.
**Consequence of:** Josh's direction below (2026-09-18), design
[0020](0020-robinhood-fact-sheet-and-voice.md) (the fact sheet and voice
this extends), design [0026](0026-the-trader-dossier.md) (the dossier and
levels this refines), ADR [0027](../adr/0027-the-bot-gives-informed-verdicts-from-evidence.md),
ADR [0029](../adr/0029-the-bot-holds-its-own-token-openly.md), ADR
[0031](../adr/0031-the-model-picks-the-story-and-the-evidence-licenses-the-joke.md),
ADR [0032](../adr/0032-the-verdict-is-a-score-with-its-coverage.md), ADR
[0033](../adr/0033-the-analyst-talks-price-and-hints-only-on-a-measured-rate.md),
and [research 0050](../research/0050-robinhood-trader-signals.md) §8 (what
the free Alchemy key measurably can and cannot do).
**Date:** 2026-09-18.
**Source:** the golden-r3-astra output packet under .orchestrator/runs
(gitignored orchestrator run folder, not tracked in this repository), a
sequential golden-ideas review — Fable's round, then Astra's round judging
it — run at Josh's request. That packet is a worker artifact; this design
carries forward what survived the review so it stops living only in an
orchestrator run folder.

## 1. The owner's direction

On 2026-09-18, after reading round 1 of the golden-ideas exercise, Josh gave
this direction (his words, summarised in the run log):

- **Contest money stays on engagement only**: views, likes, reposts, quotes
  and replies in one score. No money for "finding warning signs" — that is
  too easy to game and not what the prize is for.
- **Bought engagement is not a problem; it is the flywheel.** Drop
  "rigging" as a trap to design against.
- **Images**: capture the site's existing verdict card (free) rather than
  generating paid AI images. Skip attaching an image at all if X charges
  substantially more per post that carries one. A paid AI image is maybe a
  later perk for contest winners, not part of the base loop.
- **Receipts must feel human**, not a fixed seven-day job: the next day
  ("down 95% already"), a month later ("called this a month ago"), whenever
  the moment is actually striking.
- **Bio leaders**: yes, build it.
- **Routine posts**: a daily report that curates the community — several
  coins, callbacks, striking facts, contest winners — written for the
  community and for virality, not as a mechanical digest.
- **Voice**: say what it thinks. Hint or joke when it suspects a rug; say it
  plainly when it is certain. Respect reality, never sugar-coat. The only
  limit is that nothing it says may be illegal.
- **Log everything**, including refused drafts, so the loop can be improved
  later with AI models and a human in it.
- **The main ask**: three layers have to be excellent — data, the reasoning
  on top of the data, and turning data plus reasoning into a reply. Round 1
  was too shallow on all three. Go much deeper on funding and wallet
  analysis specifically, through one more round each from two models, each
  judging the round before it.

Round 2 (Fable) and round 3 (Astra, reviewing Fable) both ran under that
instruction. Astra's round is the merged design this document carries
forward, because it is explicit about where Fable's design would have made
the bot sound more certain than the evidence supports — see its §1 for the
line-by-line refutations of Fable's attribution and probability claims. On
2026-09-18, Josh's follow-up was "do what you recommend, lets remember to do
it all. use agents" — read here as approval to record the recommendation and
plan its build, not as approval of the rule changes §6 still lists as
pending.

## 2. The design: an analyst with an evidence ledger

The recommended pipeline is **read plan → observations → findings and
competing explanations → model judgement and writing → checks → durable
publication record.** One evidence packet is meant to feed the reply, the
card, the website, the callbacks and the evaluation loop, so a second model
is never reconstructing "why do you think that?" from scratch.

### 2.1 Wallet evidence before wallet stories

The design adds typed evidence — `EvidenceRef`, `Observation<T>`,
`Coverage`, `TradeActor`, `FundingEdge`, `Holding`, `Finding` and `Gap` — so
every observation carries its chain, subject, unit, denominator, source,
block or slot and hash where available, observation time, queried interval,
completeness and decoder version. A finding links evidence IDs and a
competing explanation; it is not allowed to silently upgrade a transfer into
ownership.

Three distinctions stay visible everywhere: **transfer destination, trade
caller and beneficiary** are different facts (a transfer can be a gift or
routing, not a purchase); **shared funding, likely coordination and common
control** are different strengths of claim (a funding edge is observed, a
coordination hypothesis needs converging behaviour, and common beneficial
ownership is normally unknown); and **supply held, supply available to
sell and executable exit depth** are different numbers (a pool's balance is
not "safe" just because it is a known contract, and liquidity dollars are
not the same as executable depth). Funding investigation starts with at
most four candidates — the two largest launch-window buyers, the earliest
remaining buyer and a deterministic sample of the rest — with the selection
rule and their share of observed buying recorded, expanding to twelve only
when a specific unresolved question earns it.

### 2.2 What to read, and where the money went

This is the corrected minimum inventory. Counts below are bounded plans,
not latency promises; Robinhood funding and archive capability follows
research 0050 §8, measured on the production box on 2026-09-17.

| Question | Exact read and incremental calls | Limit or innocent explanation |
|---|---|---|
| Early purchases, allocation and atomicity | `eth_getLogs` on verified curve, `CurveBuy`/`CurveSell` topics, bounded launch window: usually 1; receipt per selected transaction if reconciliation needs it | Mint/gift is not purchase; router caller is not necessarily beneficiary; failed receipts excluded |
| Funding and wallet activity | Per candidate: `eth_getCode`, `alchemy_getAssetTransfers` with `toAddress`, `external`, bounded `fromBlock`/`toBlock`, then pre-launch `eth_getTransactionCount`: 3 plus continuation pages | Missing internal/bridge flows; nonce is chain-local activity; dust and common services |
| Holdings and churn | Persist balances from existing token `Transfer` walk; read only the uncovered block suffix next time | Full ledger required for exact balances; churn in addresses is not churn in people; snapshot retention and entry/exit flows are different measures |
| Deployer sales and routed sales | Curve trade/Transfer suffix, `balanceOf` at read block; selected receipts and ERC-20 transfers for downstream wallets: bounded pages plus 1 state call | Balance zero can mean transfer, custody or burn; only decoded execution establishes a sale |
| Fees and proceeds | `getLaunchedToken` supplies fee recipient; decode `FeesSwept`; outgoing external/ERC-20 transfers from both deployer and fee recipient | Internal ETH payments may require one selected block `debug_traceBlockByNumber` with `callTracer`; no sweeping traces |
| Prior launches and shared wallets | Local indexed joins on deployer, fee recipient, direct funder and repeated buyer sets; 0 RPC on known edges | Same launch service is not same operator; no "all launches" claim from summon-only coverage |
| Graduation, custody and powers | Factory graduation logs; cached verified deployment/code identity; `isLocked(token)` and, when necessary, `lockedPositions`/`ownerOf`: 1–3 calls | Permanent locker code is conditional on this token's custody and that deployment; admin powers and hook behaviour need their own evidence |
| Market and exit | One free DexScreener token/pair HTTP lookup; GeckoTerminal fallback; PoolId-filtered v4 swaps for a selected interval; validated pool-state/quote reads for depth | Wash volume, stale prices, wrong pool and tiny price-setting trades; liquidity USD cannot populate `capacity` |
| Solana buyers/funding | Existing mint signature paging and `getTransaction`; per selected owner, bounded `getSignaturesForAddress` pages plus transaction decodes, including inner instructions | Newest-first, potentially many calls; failed/pruned transactions and shared fee payers; common slot does not prove a bundle |
| Solana supply/authority/exit | `getTokenLargestAccounts` + `getMultipleAccounts` for owners/mint + `getTokenSupply`; pool/vault batch reads and existing PumpSwap decoder | Top accounts are not all owners; verify program, mint/freeze authorities and applicable token extensions; no assumed venue constant |

A profit ledger is tracked separately from the dossier: deployer inventory,
linked wallets' inventory, purchase cost, observed sale receipts, creator
fees, transaction costs and outgoing transfers, by asset. The strongest
supported quantity gets named honestly — "received in sales," "received in
fees," or "observed net cash flow" — and only closed, fully traced
inventory with known cost basis and fee treatment is called realised
profit.

### 2.3 Persistent memory that preserves receipts

The design extends `crates/realorrug-onchain/src/memory.rs`, whose callers
are `dispatch`, the answer/follow-up reads, and a new CLI observation job.
It stores `observations` and `edges` (as events, not a collapsing key that
merges repeated transfers), `token_state` (balances, role provenance and a
complete-through checkpoint, committed atomically with the watermark),
`check_runs` (versioned parameters, covered interval, completeness, spend
and resume cursor), and `publications`, `claims` and `thread_evidence`
(immutable packet ID, exact claim, time, published ID and what a given
conversation already heard). Edges are indexed both directions and by
token/time so a query can walk buyer → funding source → other funded
wallets → their purchases, returning explicit paths rather than an
irreversible cluster.

### 2.4 Cost and latency the free tier can support

The [Alchemy compute-unit table](https://www.alchemy.com/docs/reference/compute-unit-costs),
checked 2026-09-18, lists Transfers at **120 CU**, code and nonce reads at
**20 CU each**, contract calls at 26 and logs at 60. Four three-call funding
candidates therefore cost **640 CU** before continuation pages; twelve cost
**1,920 CU**. The [Alchemy pricing page](https://www.alchemy.com/pricing),
checked the same day, confirms **30M monthly free CU**, **25 requests/second**
and a separate **500 throughput-CU/second** allowance.

The proposed admission budget reserves at most **2,000 CU per ordinary cold
dossier**, including retries, inside the existing 60-call/20-second bounds
in `crates/realorrug-onchain/src/budget.rs`. An illustrative monthly
allocation — 200 cold dossiers/day × 30 × 2,000 CU = **12M CU**, plus 6M for
index upkeep, 6M for outcome refresh and 6M reserve — totals the 30M free
budget; these are proposed envelopes, not measured workloads, and five
hundred cold dossiers a day would exhaust the free allowance before any
background work runs. No paid data plan is needed to start; if one is later
approved, Alchemy's page states **$0.525/M CU for all pay-as-you-go usage**,
not only usage past the free tier.

Model and X spending are separate budgets. Proposed model-spend ceilings
(vendor prices, not measured bills) are **$0.002 per ordinary draft, $0.02
per incremental escalation and $0.05 per daily editorial pass**; at 500
drafts/day, 5% escalation and 30 days that bounds spend at $30 + $15 +
$1.50 = **$46.50/month**, if the selected models fit those ceilings. The
[X API rate card](https://docs.x.com/x-api/getting-started/pricing), checked
2026-09-18, lists **$0.010 per summoned post, $0.015 per create and $0.200
per create-with-URL**; a daily post costs an estimated $0.45–$6/month, and
100 extra receipt posts a day would cost $45–$600/month at those rates.
Metrics reads are paid too: at the card's **$0.005 per post-resource** rate,
one new daily read of 500 posts budgets $75/month. All of these are rate-
card scenarios, checked on the date given, not measured account invoices.

### Judgement (§3 of the source review)

The review replaces a likelihood-ratio scoring scheme — multiplying invented
weights, which manufactures false confidence out of correlated observations
— with three separated targets: the **observed mechanism or event**,
**suspicious organisation**, and **future market outcome**. A token
recovering after a real liquidity extraction does not undo the extraction; a
token collapsing without misconduct does not validate an accusation.
Findings would group into launch structure, ownership, creator activity,
exit mechanics and trading behaviour, computing a **risk index** —
explicitly not a probability — alongside coverage over the checks
applicable to this venue and phase, with critical gaps reported separately.
Direct proof of an adverse mechanism can outrank low overall coverage;
creator selling alone, or low liquidity alone, would not be enough for
`Rugged`.

The most consequential proposal is giving the model a real analytical job:
it would see supporting and opposing findings, matched cohorts and prior
public claims, then choose the dominant explanation and write freely,
returning a public rationale tied to evidence IDs. It could choose a
provisional band from a code-computed admissible set, but code would keep
the risk index, the hard evidence requirements and any established-event
floor — it could not invent a mechanism or promote a hunch to `Rugged`. This
changes AGENTS rule 4 and needs approval; until then, run it in shadow, with
code alone supplying the published band.

### Replies, callbacks and the daily (§4 of the source review)

Label matching in the headline and template would be replaced by one typed
salience service ranking findings with their supporting fact bundles, not
isolated numbers, prioritising observed harm, explanatory power, materiality
and the user's actual question, and penalising redundancy and facts already
told in the thread. The model could choose a different lead than the
fallback and say why, since renaming a prose label cannot remove evidence
underneath it. Voice ranges from curious, when the picture is incomplete, to
blunt, when a mechanism is directly observed — matching Josh's "say it
plainly when it is certain."

Each public claim would be stored with a published ID, and background
sampling at chosen horizons would create evidence without automatically
creating a post — a callback surfaces when a user returns, the mechanism
changes, a material outcome arrives, or new facts contradict an earlier
judgement, which is the mechanism behind Josh's "feels human, not a fixed
seven-day job." A `DailyPacket` would replace the old Radar-dependent join
noted in ADR 0026:22, selecting a short mix: the day's defining pattern,
several distinct coins, the best-supported callback, an honest correction
when one exists, and the contest's leaders and settled winners — each
token's numbers kept in its own fact namespace so figures cannot migrate
between coins. Contest scoring itself would move to one versioned formula
over views, likes, reposts, quotes and replies, with anti-gaming scoring
removed and old weeks preserved under their original version, matching
Josh's engagement-only direction.

Card images would come from screenshotting the existing site's verdict-card
component from the immutable publication packet, not from a paid AI image
and not from a live `/check` result that could drift from what was
published — matching Josh's "capture the site's existing card." Every draft,
including refused ones, plus the evidence packet, checks, cost, latency and
publication IDs, would be logged, matching his "log everything ... human in
the loop."

## 3. The twelve-slice build order

The review lays out twelve reviewable slices. `crates/realorrug-<name>/src/`
is the shorthand below; "(new)" marks a file that does not exist on
`origin/main` today. Each slice would update its governing design or ADR in
the same change. Acceptance examples are proposed checks to write, not
tests already run.

| PR | Files | Done means | Depends on |
|---|---|---|---|
| 1. Typed story selection and audit | roast salience.rs (new), `verdict.rs`, `voice.rs`, `sheet.rs`; analyst `log.rs` | Renaming labels cannot change selection; a 529-holder/50.2%-concentration fixture surfaces both with unresolved-role wording; all draft bytes and selected IDs logged | None |
| 2. Snapshot and event memory | robinhood `lib.rs`; onchain `memory.rs`, `dispatch.rs`, `robinhood.rs`, `dossier.rs` | Explicit read block, durable ordered event identity, transactional suffix checkpoint; duplicate/reorg fixtures preserve balances; Robinhood receives memory | None |
| 3. Bounded funding investigation | onchain wallets.rs (new), `budget.rs`, `robinhood.rs`; roast `sheet.rs`, `clause.rs` | Four selected buyers produce evidenced funding paths and sample coverage; dust/shared-service fixtures cannot become ownership claims; CU cap enforced | 2 |
| 4. Roles, market and venue mechanics | onchain roles.rs (new), `market.rs` (new), `robinhood.rs`; robinhood `pons.rs`; roast `sheet.rs` | Pool/locker excluded only with proof; dated market snapshot, role-correct concentration, custody and verified admin facts available; liquidity dollars never become capacity | 2 |
| 5. Creator cash-flow ledger | onchain wallets.rs, `memory.rs`; robinhood `pons.rs`; roast `sheet.rs` | Fee recipient differs from deployer in fixture; transfers cannot masquerade as sales; incomplete basis cannot print profit | 3, 4 |
| 6. Solana owner/funding adapter | onchain `rpc.rs`, `dossier.rs`, wallets.rs | Largest accounts aggregated by owner; paged funding preserves incomplete history; identical finding types across chains | 2, 3 |
| 7. Assessment and judgement boundary | roast assessment.rs (new), `verdict.rs`, `voice.rs`; analyst `answer.rs`; serve `check.rs` | Correlated flags count once; critical gaps survive coverage; analyst/site share packet; model band choice shadowed pending approval | 3–6 |
| 8. Outcome calibration | cli `creator_index.rs`; roast `baserates.rs`; `docs/research/data/` versioned outputs | Mature/censored outcomes separated; time/family-held-out evaluation; curve peaks never advertised as executable returns; scoped rates reach sheet only when eligible | 7 |
| 9. Observation jobs and callbacks | cli observe.rs (new), `main.rs`; analyst `followup.rs`, `daemon.rs`; onchain `memory.rs`; deployment timer | Standalone outcome refresh; published claim links; no "called it" from a neutral old post; unchanged token produces no automatic callback | 2, 5, 8 |
| 10. Voice checks and shared card publication | roast `forbidden.rs`, `voice.rs`, `fidelity.rs`; analyst `publish.rs`, `x.rs`; serve `card.rs`; site `Check.tsx` | Evidence-specific vocabulary and historical callbacks pass; unsupported identity fails evaluation; snapshot card matches text; partial publication resumes | 1, 7, 9; approved rules and measured delivery cost |
| 11. Engagement-only contest rule | contest `score.rs`, `ledger.rs`; analyst `x.rs`, `daemon.rs`; site `Leaderboard.tsx` | All five metrics count under recorded weights; no anti-gaming score; old weeks replay unchanged; model has no payout authority | Recorded weights/version boundary |
| 12. Curated daily and review loop | analyst `daily.rs`, `daemon.rs`, `log.rs`; cli observe.rs; site `History.tsx` | Multi-coin report cites packets, corrections and authoritative standings; per-token numbers cannot cross; human-reviewed weekly draft/outcome export | 8–11 |

The first three slices are meant to fix the visible failure (label matching
that can be defeated by renaming a word), make evidence reusable across the
reply, card and website, and deliver the owner's most distinctive signal
(funding evidence) before anything downstream depends on it.

## 4. Decisions pending the owner

These are **not decided**. They are the concrete rule changes the build
order above would need before their slices could publish, carried forward
from the source review's own table of what still needs Josh's approval:

- **Model band choice (AGENTS rule 4).** The recommendation lets the model
  choose a provisional verdict band from a code-computed admissible set,
  while code keeps the risk index, the hard evidence requirements and any
  established-event floor. This is a real change to AGENTS §3 rule 4's
  "code computes ... the model writes the words." **Run it in shadow only,
  with code alone supplying the published band, until Josh approves moving
  the choice itself to the model.**
- **Contest weights.** The direction to score engagement only (views,
  likes, reposts, quotes, replies) in one formula is already binding — but
  the five metrics' relative weights and the effective-week version
  boundary are not yet chosen. `crates/realorrug-contest/src/score.rs`
  needs a recorded set of weights before slice 11 can ship.
- **Honeypot and wallet-language gates.** The recommendation is to replace
  the unconditional bans on words like "honeypot," "exit," and "dumping" in
  `crates/realorrug-roast/src/forbidden.rs` with evidence-specific
  requirements — sell-blocking evidence would license honeypot language,
  a different rug mechanism would not — and to permit "wallet purchased" or
  "wallet funded" language independent of a coordination finding, without
  licensing "insiders" or common-ownership language from a shared-funder
  threshold alone. Neither narrowing is approved.
- **Mandatory age prose.** The recommendation is to make launch-age context
  a permitted fact rather than a compulsory line in every reply, while
  keeping price-moment disclosure mandatory under ADR 0033. This is a voice
  change to `crates/realorrug-roast/src/forbidden.rs` and is not approved.

Everything else AGENTS §3 already settles — model judgement never moves
money, untrusted content is never an instruction, holdings stay public and
nothing trades — is unaffected by any of the above and does not need
re-approval.
