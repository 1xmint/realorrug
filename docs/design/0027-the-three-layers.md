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
| 3. Bounded funding investigation — **built 2026-09-18**, see below | onchain `wallets.rs`, `budget.rs`, `robinhood.rs`, `memory.rs`; roast `sheet.rs`, `clause.rs`, `fidelity.rs` | Four selected buyers produce evidenced funding paths and sample coverage; dust/shared-service fixtures cannot become ownership claims; CU cap enforced | 2 |
| 4. Roles, market and venue mechanics | onchain roles.rs (new), `market.rs` (new), `robinhood.rs`; robinhood `pons.rs`; roast `sheet.rs` | Pool/locker excluded only with proof; dated market snapshot, role-correct concentration, custody and verified admin facts available; liquidity dollars never become capacity | 2 |
| 5. Creator cash-flow ledger | onchain wallets.rs, `memory.rs`; robinhood `pons.rs`; roast `sheet.rs` | Fee recipient differs from deployer in fixture; transfers cannot masquerade as sales; incomplete basis cannot print profit | 3, 4 |
| 6. Solana owner/funding adapter — **built 2026-09-18 (slice 6a owner half, slice 6b funding half), see below** | onchain `rpc.rs`, `dossier.rs`, `wallets.rs` | Largest accounts aggregated by owner; paged funding preserves incomplete history; identical finding types across chains | 2, 3 |
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

### Slice 3 as built (2026-09-18)

Recording, not recommending. `crates/realorrug-onchain/src/wallets.rs`
reads the launch window's `CurveBuy` events on the verified curve (launch
block to launch block + 6,000, capped at the dossier's read point) and
aggregates them by **beneficiary** (`Trade::recipient`), keeping the trade
caller separate: a Transfer-only recipient is never a buyer. It chooses at
most four candidates — the two largest launch-window buyers by quote, the
earliest of the rest, and the lowest remaining address as a deterministic
sample — and records the rule and the candidates' quote-weighted share of
the window (`Funding::coverage_bps`). Per candidate it reads `eth_getCode`,
`alchemy_getAssetTransfers` (external, into the address, from 360,000 blocks
before its first purchase to the block before it, at most two pages) and
`eth_getTransactionCount` at the block before launch; a zero nonce means no
prior outgoing transactions, nothing more. A funder is *material* when its
transfer covers at least half of the purchase plus a gas allowance; dust is
kept as an observation (`FundingEdge::material = false`) and never counts
toward a shared funder. `Funding::shared` lists addresses that materially
funded two or more of the checked candidates.

`budget.rs` gained a compute-unit ceiling beside the call ceiling
(`Budget::with_compute_units`, 2,000 CU per cold dossier by default). The
investigation runs after the core reads and caps itself at the smaller of
640 CU and what the dossier has left; when the cap stops it mid-way the gap
is recorded in `Funding::gaps` and the dossier is still built. A provider
that does not serve `alchemy_getAssetTransfers` degrades the read the same
way. With a memory, the edges are remembered as events keyed by the
provider's `uniqueId` (`funding_edges`) and a `funding` check run records
the window and whether every candidate was fully read.

The sheet (`realorrug-roast/src/sheet.rs::push_funding`) publishes two
facts: `FundingChecked` ("4 of 4", with the coverage) and `SharedFunder`
("the same address funded 3 of the 4 early buyers checked"). The
denominator is always the checked count, "checked" is in the words, and
the sentence names the innocent reading (an exchange paying out
withdrawals). It never says "one person", "insiders", "the same owner" or
"common control", and its tests fail if it does. salience.rs ranks the
two as one bundle: when one address funded more than half of the checked
buyers it leads over concentration (below only a creator record); fewer,
or no checked count, ranks it below concentration. Slices 5 and 6 build on
`wallets::investigate`, `Funding`, `Candidate`, `Funder` and
`Memory::funding_edges`.

### Slice 4: the market read is now built, custody is not (2026-09-18)

Recording, not recommending, and recording a partial start honestly rather
than marking row 4 built before it is. `crates/realorrug-onchain/src/
roles.rs` adds the type this row's "only with proof" requirement needed:
`Role`/`Proof`/`RoleClaim` name a pool, curve, factory, locker or the zero
address, but only via a `Proof::VerifiedAddress` or `Proof::DecodedEvent` —
there is no code path that assigns a `Role` from balance size or "looks
like a contract." `concentration()` splits a balance list into proven
infrastructure and everything else, states the non-infrastructure
denominator, and flags anything at or over 5% of it as unresolved rather
than naming a role for it. `robinhood.rs`'s `holders_from` now calls
`roles::verified_infrastructure` for its curve/factory/zero exclusion
instead of a bare address array, with the same output.

`crates/realorrug-onchain/src/market.rs` adds `MarketSnapshot` (price,
market cap with its basis, liquidity, pair address, source, and the
wall-clock moment it was read, per ADR 0033) behind an `HttpGet` seam
mirroring `rpc.rs`'s `Transport`, reading DexScreener first and
GeckoTerminal on failure. `attach()` writes only `Dossier::market`; a
regression test (`liquidity_dollars_never_reach_capacity`) asserts it
cannot touch `Dossier::curve.quote_capacity`, which is this row's
"liquidity dollars never become capacity" requirement made a running
check rather than a reviewer's promise.

**Now built (2026-09-18, a later session): the market read is wired into
the live path.** `crates/realorrug-onchain/src/dispatch.rs`'s `Clients`
gained `market: Option<&dyn market::HttpGet>` — deny-by-default, the same
shape as `Clients.robinhood`'s "no endpoint configured" rule — and its
`robinhood()` arm calls `market::snapshot` and `market::attach` only when
`Some`. A failed read (both DexScreener and GeckoTerminal) never zeroes
anything; it pushes `Dossier::Unavailable { fact: "market", .. }`, the
same gap-naming path every other optional read uses. `crates/realorrug-cli/src/roast.rs`,
`crates/realorrug-analyst/src/daemon.rs` and `crates/realorrug-analyst/src/answer.rs`
each pass `Some(&market::Http::default())`; every other `Clients` site,
including test fixtures and `crates/realorrug-serve/src/check.rs` (a
route the market read is deliberately not extended to in this slice),
passes `None`.

`crates/realorrug-roast/src/clause.rs` gained a `Kind::Market`;
`crates/realorrug-roast/src/fidelity.rs` maps it to `Subject::Token`, the
same subject `Kind::Graduated`/`Kind::Age` already use, since a price or
cap is a fact about the token, not the pool's tradeable depth
(`Subject::Liquidity`). `crates/realorrug-roast/src/sheet.rs`'s new
`push_market` renders the snapshot's own `observed_at` wall clock — never
`Dossier::read_at`'s chain block, a different clock for a different
question, per ADR 0033 — into the fact's sentence, so the moment a price
was read travels with the number itself; it never reaches into
`liquidity_usd` for anything but its own sentence, keeping
`market.rs`'s own `liquidity_dollars_never_reach_capacity` regression the
only writer of that boundary. A failed market read joins the
`capacity`/`fees` skip-list in `FactSheet::build()`: the named gap still
reaches the operator, but does not degrade `verdict::level`'s severity,
since an off-chain aggregator hiccup is not a required fact the way a
missing launch block is. `crates/realorrug-roast/src/salience.rs`'s new
`market()` candidate ranks at priority 40, below every risk-bearing
candidate — `launch_recipients` at 70 is the lowest of those — so a price
is recorded, per AGENTS.md §3 rule 5, but never leads a reply.

Custody and admin-power facts (`isLocked(token)` etc.) remain not
implemented: `pons.rs`'s existing selector table is verified against
deployed bytecode by a documented methodology, and no real locker
contract's bytecode has been verified that way in this sandbox —
inventing one would be introducing a fact AGENTS.md rule 2 forbids, not
recording one. Row 4 stays open in the table above until a session with
that verification closes it.

### Slice 6a as built (2026-09-18)

Recording, not recommending. The owner half of row 6:
`crates/realorrug-onchain/src/rpc.rs` gained `token_largest_accounts`
(`getTokenLargestAccounts`) and `token_supply` (`getTokenSupply`);
`crates/realorrug-onchain/src/dossier.rs`'s `build` reads both plus one
batched `RpcClient::accounts` call (`getMultipleAccounts`) covering every
sampled account and the mint itself, and aggregates the sampled accounts by
their decoded `owner` field into `TokenOwnership`/`TokenOwner`. Each owner
carries its combined balance, how many of the sampled accounts fell under
it, and its `share_bps` of the supply `getTokenSupply` reported — never the
largest-accounts total, which is only a sample capped at 20 accounts. The
mint's own account supplies `mint_authority` and `freeze_authority`
(`dossier::mint_authorities`, reading the shared SPL/Token-2022
`COption<Pubkey>` layout directly rather than widening
`realorrug_pumpfun::token::MintAccount`, which discards the mint-authority
address and refuses an unmodelled extension it does not need to reject to
answer this).

An owner is excluded from "largest holder" only with proof: its address is
recomputed as the pump.fun bonding curve's program-derived address from the
mint itself (`realorrug_pumpfun::pda::bonding_curve`), a check a reader can
rerun. Every other owner keeps `OwnerRole::Unresolved` — never upgraded to a
person or a named role — regardless of its balance's size or shape. A
transport, node or shape failure on any of the three reads fails the whole
owner read rather than reporting a partial list as complete; `build` records
it as "token ownership" in `Dossier::unavailable` and still returns the rest
of the dossier (rule 8/9). Fixture tests in `dossier.rs` cover two sampled
accounts under one owner aggregating to one holder, a proven curve owner
kept alongside an unproven one left unresolved, and a failed batched read
producing that named gap rather than an empty or zeroed `TokenOwnership`.

**Not done in this slice**: nothing in `realorrug-roast` reads
`Dossier::token_ownership` yet. Wiring it into the sheet needs a new
`Kind` in `realorrug-roast/src/clause.rs` and a ranking entry in
`salience.rs`, both outside this slice's file scope (onchain `rpc.rs`,
`dossier.rs`; roast `sheet.rs` only for wording an existing fact already
needs) — left for the slice that adds the fact to the sheet. The funding
half of row 6 is built in slice 6b below.

### Slice 6b as built (2026-09-18)

Recording, not recommending. The funding half of row 6, and the
chain-agnostic finding types row 6 asked for.

`crates/realorrug-onchain/src/wallets.rs`'s `Funding`, `Candidate`, `Funder`
and `SharedFunder` had their address fields changed from the 20-byte
`realorrug_robinhood::Address` to `String`, holding each chain's own
canonical text (0x-lowercase hex for Robinhood, base58 for Solana) — the
type that cannot fail to hold either chain's key, so the roast layer needs no
chain-specific wording (row 6's own requirement). `is_material` gained a
`gas_allowance: u128` parameter so a Solana read is not compared against an
EVM gas floor; `GAS_ALLOWANCE_LAMPORTS` (0.00005 SOL) is Solana's. Callers in
`robinhood.rs` and the `sheet.rs` test fixture were updated for the type
change; no other crate builds these three types.

`investigate_solana` (`crates/realorrug-onchain/src/wallets.rs`) is the
Solana funding read. It has no `CurveBuy` log to read purchases from, so its
launch window is the mint's own first `SOLANA_WINDOW_TRANSACTIONS` (25)
*successful* transactions, read oldest first
(`RpcClient::signatures_back_to_oldest` on the mint) and walked in that
order: each transaction's `post_token_balances` against `pre_token_balances`
gives a distinct wallet whose balance of the mint rose, keyed by
`TokenBalance::owner`. The bonding-curve PDA
(`realorrug_pumpfun::pda::bonding_curve`, the same proven check slice 6a uses
to exclude the curve as an owner) is never counted as a buyer — it is the
pool side of every trade. `Funding::buyers` is every distinct buyer found in
that 25-transaction window, not the capped candidate count; the first
`MAX_CANDIDATES` (4, matching Robinhood) of them by first purchase are kept
as candidates, in the order their balance first rose, and that transaction's
slot is the candidate's first-purchase slot. Each candidate's own oldest
signature is then read separately for the native lamport transfer that
funded it (`funder_of`, the same largest-lamport-drop-in-one-transaction
reader used before this slice), but the transfer only counts as a funder
when its transaction's slot is at or before the candidate's first-purchase
slot — a transfer that landed afterward did not finance the buy, and is
dropped with a gap explaining why rather than recorded as a funder.

**If the mint's own signature history is longer than the page budget
allows, the buyers found in that partial read are not "the early buyers"**
(a budget that runs out first drops the oldest, undiscovered page, per
AGENTS.md rule 8) **so `investigate_solana` returns no checked candidates,
`buyers: 0`, and a gap saying the launch's first buyers could not be reached
within the read budget — never a partial buyer list presented as
complete.** The same rule applies per candidate: if a candidate's own
signature history is truncated before its oldest transaction,
`signatures.last()` is not that transaction, so no funder is recorded (only
the gap, with `Candidate::funding_complete` left `false`) rather than a
funder read from whatever transaction the truncated page happened to reach.
A transport failure reading the mint's own history fails the whole call,
which `dossier.rs::build` records as "funding" in `Dossier::unavailable`; a
failure reading one candidate's history, or one transaction, is a named gap
on a result that is still returned.

`Funding::coverage_bps` is `Option<u16>`: `Some` on Robinhood, where the
quote-weighted share of the checked candidates' buying is a real number, and
`None` on Solana, which has no quote amount to weigh a share against —
`Some(0)` would claim the checked wallets bought nothing, which is false,
not merely unmeasured. `crates/realorrug-roast/src/sheet.rs`'s
`push_funding` drops the "they bought {pct} of what the launch window
bought" clause (and the parallel "({pct} of the window's buys)" clause in
the blunt voice) entirely when `coverage_bps` is `None`, rather than
rendering a percentage that isn't there.

`dossier.rs::build` calls `investigate_solana` as an independent step, not
gated on the owner-sample step 4: funding candidates come from the mint's
own history, not from `TokenOwnership`. Fixture tests in
`crates/realorrug-onchain/src/wallets.rs` cover two buyers found in the
mint's history and funded by the same address (a shared funder), a
page-budget cap producing an incomplete result rather than an absent one, a
transport failure surfacing as a named "funding" error rather than an empty
result, a truncated mint history checking nobody instead of the wrong
buyers, a truncated candidate history recording no funder, a funder
transaction that landed after the first purchase not counting, and
`Funding::buyers` reflecting the whole window rather than the capped
candidate count; `crates/realorrug-roast/src/sheet.rs` has a fixture
confirming a `None` coverage never renders a percentage. The Robinhood path
(`investigate`, `purchases_from`, `SELECTION_RULE`) is untouched by this
slice.

**Not done in this slice**: the SOL cost of the observed buy itself is not
read, so `is_material` is applied with `quote = 0` for a Solana candidate,
the same "more than dust" fallback it already applies to an unpriced
transfer — a materiality figure weighted against the actual buy price is
left for a slice that reads the buy's own lamport cost. Nothing in
`realorrug-roast` reads `Dossier::funding` for the Solana path differently
than for Robinhood's; both already share `push_funding`'s wording since the
finding types are chain-agnostic.

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

## 5. The scam catalog — approved 2026-09-18

A second golden round (Fable's plan, then a review started by Astra and
completed by Sonnet when Astra's quota ran out; the round-5 final packet
in the gitignored run folder) answered two questions from Josh: can a model
judge real-vs-scam more accurately than code, and should the bot keep a
growing catalog of scam methods and multi-step schemes (fee-bait and the
like) that counts every sighting in context.

**The answers, as recorded.** Whether a model beats code here is unknown,
so it earns a trial, not a role: code keeps choosing the published band,
and a model runs in shadow against a hand-labelled set of about 500 cases,
compared with strong rules rather than a weak checklist, on held-out time
and held-out wallet families. The catalog is worth building on one
condition — every eligible check is recorded, including absent and unread
results, with the evidence available at that moment. Without the negatives
there is no denominator and no way to tell whether the catalog improved.
One operator's repeated launches count once per wallet family, and every
occurrence is tagged by how the token was seen (summoned, unsummoned
sample, outcome refresh) so a count never claims more than its sample.

**Approved by Josh on 2026-09-18** ("yes to those 5 things"):

1. Failed and truncated check runs are kept past `MAX_CHECK_RUNS` (design
   [0021](0021-the-read-memory.md) §9); complete runs keep the existing cap.
2. The seed catalog of methods and playbooks is accepted, and a playbook
   match — full or partial — never raises a band on its own; it can only
   explain findings code already scored.
3. A shadow adviser on the hard minority of cases, budgeted at about $2 a
   day, never publishing a band.
4. A weekly catalog-review job, about $3–10 a week, that proposes
   candidates only; a candidate's PR must show its back-test on both
   held-out splits before a human merges it.
5. The rule against naming a person as a scammer is enforced structurally
   in `crates/realorrug-roast/src/forbidden.rs` (a claim about a person is
   rejected by construction), not by a word list.

**What it adds to §3.** Three slices, inserted without reordering the
twelve: **7b** catalog, matcher and packet log (roast catalog.rs (new),
playbook.rs (new); onchain `memory.rs` gains `packets`, `method_occurrences`,
`playbook_occurrences` and `wallet_families`; seeds under
a new docs/research/catalog folder), after 7; **7c** shadow adviser (analyst
`answer.rs`), after 7b; **9b** weekly catalog review writing only to
that folder's candidates area, after 9 and 7b. Slice 7 gains a
`causal_episode_id` so correlated findings count once; slice 8's
calibration becomes the evaluation harness comparing code, model and a
non-model learned baseline; slice 10 carries item 5; slice 12 may publish a
"playbook of the week" whose every count states its sample. The
per-reply model ceiling of $0.002 is recommended to rise to about $0.02 so
the reply is not forced onto the cheapest model; that is a recommendation,
not yet a decision.

**The good side, approved by Josh on 2026-09-18** ("yes"). The catalog
also holds patterns of healthy launches, not only scams. Each entry is
tagged as either a *warning* or a *reassurance*, and both live in the same
catalog, matcher and packet log (slice 7b), so nothing extra is built.

- **Why.** Without it the bot can only say "nothing ugly yet". With it a
  reply can name what went right with the same specificity as a warning
  (a creator whose last launches filled their curve, early buyers funded
  from unrelated sources). That is fairer to honest launches, gives
  holders a reply worth sharing, and explains an odd-looking launch that
  has an innocent reason.
- **A reassurance must separate.** Scammers copy good signs on purpose (a
  locked pool, a renounced mint). An entry is measured on the labelled set
  against rugs as well as survivors; one that is common among rugs stays
  in the catalog as a checked fact but carries no weight.
- **What a reassurance may do:** support `NothingUglyYet` and give the
  reply a specific reason for it.
- **What it may never do:** say "safe"; lower a live rug mechanic, which
  code scored and a match cannot cancel; or fill in for a check that was
  never read (absent is not zero, and unknown is not safe). The mirror of
  item 2: a match never lowers a band on its own either.
- **The weekly review (9b)** proposes both kinds, and a reassurance
  candidate's PR shows the same back-test on both held-out splits.

Still pending from §4: model band choice stays shadow-only, contest
weights, the honeypot and wallet-language gates, and age prose.

### Slice 6a's sheet follow-up as built (2026-09-18)

Recording, not recommending. Closes slice 6a's "not done" note: nothing in
`realorrug-roast` read `Dossier::token_ownership`; now something does.

`crates/realorrug-roast/src/clause.rs` gained `Kind::TokenOwnership`;
`fidelity.rs` maps it to `Subject::Holders`, beside `Kind::Holders` and
`Kind::LargestHolderShare` -- it is a different reading of the same
question, who holds the supply, not a fact about the token as a thing.
`sheet.rs`'s new `push_token_ownership` finds the largest owner among
`TokenOwnership::owners` (already amount-sorted) whose role is not
`OwnerRole::BondingCurve`, and renders its `share_bps` of the *total*
`getTokenSupply` reading. Every such owner is `OwnerRole::Unresolved` by
construction -- the reader in `dossier.rs` proves only the bonding curve,
nothing else -- so the sentence always says "one unidentified wallet",
never a role the sheet did not establish (AGENTS.md §4's last bullet). A
sample where every owner is the curve, or an owner whose share is
unmeasurable (`share_bps: None`, a zero supply), writes no fact rather than
one about zero. A failed read joins the `capacity`/`fees`/`market`
skip-list in `FactSheet::build()`, since Solana's dossier does not set
`holders` today and so no verdict currently depends on a holder-
concentration read for that chain.

**This is a second, different concentration reading, not a restatement of
`concentration()`'s `Holders`/`LargestHolderShare`.** That one is every
circulating token account's share of the circulating supply, with the
curve's own stock excluded from both sides -- and it is Robinhood-only
today. `TokenOwnership` is a capped twenty-account sample's share of the
*total* supply, with only the bonding curve's address proven-excluded --
and it is Solana-only today. The two never fire on the same dossier as a
result, but `salience.rs`'s new `token_ownership()` candidate is still
ranked one step below `concentration` (85, not 90) rather than at an
unrelated priority, so that property -- whichever fires, the fuller
non-sampled reading would win the tie -- holds by construction rather than
by the two chains happening to stay disjoint forever. Still above
`launch_recipients` (70): an address-level concentration finding, even a
sampled one, is more specific than a bare recipient count with nothing to
weigh it against.
