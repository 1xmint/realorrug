<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0055 — stock data, narratives, and what to record now, for a paid API that follows the meta

**Date:** 2026-09-19.
**Status:** read-only research, gathering facts for a direction question
(AGENTS.md §2). The owner's framing: realorrug becomes a paid API (x402,
$0.05 facts endpoint on Base, being built now — per research 0054) that
"follows the narratives, learns from the data, and squeezes out value for
what a wide variety of agents and humans want," one step ahead of the meta —
e.g. if the narrative becomes pairing stocks with crypto, the API should
already have stock data — and "record everything even though we're not
using it, so our moat compounds." Nothing here decides the roadmap; it
answers the five packet questions with numbers and dates so the conversation
can happen. CHECKED where a page was fetched directly and quoted, or a
file:line was read in this tree today; UNVERIFIED where a finding rests on a
search-engine summary that was not independently fetched. No code changed,
no sign-up performed, no key used, per the packet's boundary.

## 1. Stock data on Robinhood Chain itself

**Primary source, fetched today.** `docs.robinhood.com/chain/stock-tokens/`
(fetched 2026-09-19): "Stock Tokens are tokenised debt securities issued by
Robinhood Assets (Jersey) Limited ('RHJ')" giving "economic exposure to
underlying securities like US shares and ETFs," but holders "do not grant
investors any legal or beneficial rights in, or against the issuer of, those
underlying securities." They are "issued as standard ERC-20 tokens that can
be held, transferred, and composed into applications onchain," 18 decimals.
Minting/burning happens during a "tokenization window (Monday–Saturday,
02:00 CET/CEST)" by market makers; "Only Authorized Participants may
directly subscribe from RHJ after KYB onboarding" — a third-party building
on the chain "build[s] by composing with existing tokens rather than
minting."

**This is the single most load-bearing fact for this section: stock tokens
on Robinhood Chain are themselves a debt-security wrapper issued by a
regulated entity (RHJ), not a permissionless memecoin-style deploy.** That
is a different legal category from the Pons v2 launches this repo already
reads, and it is exactly the risk ADR 0034 decision 1 already named without
this document's detail: "A pair with a tokenised stock would pay fees and
prizes in tokens US persons may not receive" (`docs/adr/0034-the-prize-is-a-
free-calling-game.md:34`, CHECKED, read 2026-09-19). That ADR was reasoning
about *pairing realorrug's own token* with a stock token; this finding
extends the same caution to *reading and republishing* stock-token data for
money — describing a security-like instrument's price/holders for a paid
API is a narrower act than pairing or trading it, but it is not obviously
outside the shape of question ADR 0023 decision 5 already deferred to a
lawyer ("the legal review runs in parallel"). This document flags it, per
AGENTS.md §2.3's pattern in research 0054, rather than concluding it is fine
or unfine.

**Which stocks, and contracts.** Robinhood's own API (`docs.robinhood.com/
chain/stock-token-apis/`, fetched 2026-09-19) exposes asset metadata via an
`/assets` endpoint returning "One entry per chain" under `deployments[]`,
each with a `contractAddress` field ("EIP-55 checksummed address") — i.e.
Robinhood itself publishes the authoritative contract-address list via API,
not a static page this session enumerated. A third-party aggregator,
TrustSwap (`trustswap.com/robinhood/stock-tokens-list`, found via search
2026-09-19, **UNVERIFIED, not fetched directly**), titles its page "Robinhood
Stock Tokens List — 95 Equities on Chain," consistent with a search-summary
figure of "96 stocks and ETFs" tokenized and "95 Chainlink-priced equities as
of early August 2026" (**UNVERIFIED**, search summary only, not Robinhood's
own page). One concrete address surfaced by search and **not independently
verified against Robinhood's `/assets` endpoint this session**: a token
called TSLA at an address resembling `0x322F0929c4625eD5bAd873c95208D54E1c
003b2d` (malformed-looking hex in the search summary itself — **do not use
this address without re-deriving it from Robinhood's own `/assets` call**).
**Not established this session:** the exact current list of tokenized
stocks/ETFs and their addresses — that requires one live call to Robinhood's
own `/assets` endpoint, not attempted here (out of the read-only/no-sign-up
boundary; the endpoint's own auth requirement was not checked).

**Are trades/prices/holders readable onchain with the reads we already
do?** Partially yes, by inference from what is already established in this
tree, not tested against a real stock-token address this session:

- **Holders/transfers: yes, same technique as Pons v2 tokens.** Since stock
  tokens are "standard ERC-20 tokens" (quoted above), the same method
  research 0039 §1 already names for Pons v2 — "exact, by summing the
  token's ERC-20 `Transfer` logs ourselves — no holder-list call on-chain" —
  applies unchanged. `crates/realorrug-onchain/src/robinhood.rs`'s existing
  `walk_logs`/`walk_launches` machinery (CHECKED, file read 2026-09-19,
  `pub fn walk_logs`, `pub fn walk_launches` at lines 1402/1361) reads
  `Transfer`-shaped logs generically; nothing in it is Pons-v2-specific by
  contract address, only by which factory/curve addresses are chased for
  launch discovery. Reading a stock token's transfers is the same RPC shape
  already paid for in research 0039's $49/month QuickNode recommendation —
  **no new data vendor needed for holders/transfers.**
- **Price: Robinhood's own API is the better source, not the chain.**
  `stock-token-apis/` states its `/prices/{symbol}` endpoint returns "Live
  token-denominated USD bid/ask," explicitly noting "Prices are the raw
  underlying-equity bid/ask passed through as-is — they are **not**
  multiplier-adjusted" (quoted, fetched 2026-09-19). This is a first-party,
  presumably licence-clean feed for Robinhood's *own* tokenized product
  (distinct from redistributing a stock exchange's raw feed, §2 below) —
  whether it is free/keyless or requires a partner agreement was **not
  established this session** (the fetched summary describes the endpoint's
  shape, not its access terms; that requires reading the API's auth section
  directly, not done here). On-chain, price would have to come from a DEX
  pool if one exists for a stock token (same graduated-pool logic research
  0039 §5 used for DexScreener/GeckoTerminal on Pons v2) — **whether stock
  tokens trade on public Robinhood Chain DEX pools at all, versus only via
  Robinhood's own mint/burn venue, was not checked this session.**
- **Trades: not established.** Whether stock-token transfers on public
  pools constitute "trades" in the same sense as a Pons v2 curve trade, or
  whether all real trading happens off-chain at Robinhood and the on-chain
  leg is just custody/settlement, was not read this session.

**Net for §1: this may be the cheapest, licence-free-*sounding* stock angle
on data cost (reuses the RPC subscription already recommended in research
0039, no new vendor), but "licence-free" is not established — RHJ's own debt-
security framing is a legal question mark that should be resolved before
"stock data" and "paid API" are combined on this specific source,** distinct
from and probably sharper than the general redistribution question in §2.

## 2. Off-chain stock market data and redistribution terms

**Alpaca.** Alpaca's own support page states plainly (title read via search
2026-09-19, `alpaca.markets/support/redistribute-alpaca-api`,
**UNVERIFIED — page title only, full text not fetched this session**): "Can
I redistribute Alpaca API data via my platform?" — a search-engine summary
of the answer states redistribution is **not permitted** under Alpaca's
terms. Pricing (from `alpaca.markets/data` and `docs.alpaca.markets`, search
summary read 2026-09-19, **UNVERIFIED**): a free "Basic" plan covers IEX-only
real-time equities data; "Algo Trader Plus" at **$99/month** adds full SIP
(consolidated tape) real-time data and OPRA options. Data is sourced from
"the CTA (Consolidated Tape Association), administered by NYSE, and the UTP
(Unlisted Trading Privileges) stream, administered by Nasdaq" (same summary).
**If this redistribution-not-permitted reading is correct, Alpaca is a data
source for realorrug's own internal use, not something whose output can be
resold through a paid facts endpoint** — this needs a direct read of
Alpaca's actual terms of service before relying on it either way; not done
this session.

**Polygon.io (rebranded Massive, early 2026).** `massive.com/terms/
market_data_terms.pdf`, titled "POLYGON.IO, INC. MARKET DATA TERMS OF
SERVICE, Last Updated: October 9, 2024" (found via search 2026-09-19,
**UNVERIFIED — PDF not fetched and read directly this session**, only its
existence and title confirmed). A GitHub summary describes the product as
"real-time and historical market data APIs... delivered through REST
endpoints and WebSocket streams... plus an S3 flat files product, with
tiered subscription plans" (api-evangelist/polygon, search summary,
**UNVERIFIED**). A secondary source (search summary, **UNVERIFIED**) states
"US market-data products may need to account for display and non-display
usage, exchange entitlements, redistribution rules, customer classifications
and audit requirements" — i.e. redistribution is gated by exchange
entitlement agreements layered on top of Polygon's own plan, the standard
pattern for equities data (see Nasdaq/NYSE below), not something a retail
API tier grants by default. **Not established this session:** Polygon/
Massive's actual current price tiers or a confirmed redistribution clause —
the terms PDF needs a direct read before this is more than a plausible
inference from the pattern common to this data category.

**Exchange licensing (Nasdaq/NYSE/Cboe), general pattern — not independently
priced this session.** The consistent shape across every provider surfaced
today (Alpaca's CTA/UTP sourcing, Polygon's "exchange entitlements" language)
is that real-time consolidated-tape equities data ultimately traces back to
NYSE (via CTA) and Nasdaq (via UTP), and *redistributing* it — as opposed to
*displaying* it to a single end user inside your own app — typically
requires a separate "non-display" or "redistribution" data agreement with
the exchange(s) directly, priced and negotiated per-vendor, not published as
a self-serve rate card. **No first-party Nasdaq/NYSE/Cboe redistribution
price was fetched this session** — this is a real gap, not a shortcut: those
agreements are typically sales-quoted, and finding an actual number would
need a direct inquiry, not a search.

**What counts as "derived data" that may be redistributable.** Not
established this session with a primary source. The general industry
concept (unverified, not sourced to a specific page today) is that a feed
transformed enough — e.g. a computed score, a delayed price, an aggregate —
can fall outside "market data" redistribution restrictions, while a raw
real-time quote pass-through cannot. **This is exactly the kind of number
AGENTS.md §1 says must be checked before a decision rests on it — not done
this session, flagged as the next thing to check if the stock angle is
pursued at all.**

**Cheapest compliant path, given what is established today:** Robinhood's
own `/prices/{symbol}` endpoint for its own tokenized stocks (§1) is the
only source seen this session that is plausibly free of a *third-party*
exchange-redistribution problem, because it is Robinhood's own product
describing its own token, not a raw NYSE/Nasdaq feed — but its access terms
and whether *that* counts as redistributable were not read. Providers named
in the packet (Databento, Finnhub, Tiingo, a post-IEX-Cloud successor) were
**not reached this session at all** (budget) — a real gap in this section,
not a finding that they are worse or better.

## 3. Narrative tracking — what exists in the repo, what would need building

**What already exists: base rates, not narrative detection.**
`crates/realorrug-roast/src/baserates.rs` (CHECKED, file read 2026-09-19) is
the closest thing in this tree to "tracking the meta," and it is explicitly
not that: its own doc comment says "The store's job in this product is
**base rates, not lookup**... measured once, published as
`docs/research/data/0024-base-rates.json`," with a hard `STALE_AFTER_DAYS =
60` because "`0008`'s figures were wrong by 2.7× after nine days." This is a
periodic snapshot of population statistics (recipient counts, graduation
rates), not a live trend/narrative detector, and it explicitly refuses to
extrapolate between snapshots. **No file named `narrative`, `trend`,
`salience-of-topic`, or similar was found doing cross-token topic detection**
— the repo's other hits for "narrative"/"trend" (grep run 2026-09-19 across
`docs/` and `crates/`) are all this same base-rate/salience machinery for
*ranking facts within one reply* (`realorrug-roast/src/salience.rs`, per
AGENTS.md §4: "ranks candidates from `Fact::kind`... the headline... draw
from the same ranking"), not detection of a market-wide theme like "stocks
paired with crypto." **Finding: narrative/meta detection does not exist in
this codebase today; it would be new work, not a rename of something already
built.**

**What data the repo already collects that a narrative detector could run
on, without new collection:**
- **Onchain token names/symbols and launch clustering on Pons v2** — already
  read per launch (research 0038/0036), stored as part of the launch record
  research 0039 assumes for req #1/#7. A clustering pass over token
  name/symbol text (e.g. spiking use of "stock", a ticker-like symbol, or a
  crypto-project name) could run entirely off data already indexed for other
  reasons — this is the cheapest possible narrative signal, because it adds
  no new reads, only new analysis of stored rows.
- **The X mentions/summons stream.** AGENTS.md §3 rule 3 and ADR 0034
  decision 5 ("Suggestions are logged, never obeyed... the bot never acts on
  it") already establish the pattern needed: mention text is data, logged,
  never a system-prompt instruction, and never a source the model acts on
  directly. A narrative signal built from mention frequency (e.g. how often
  "stock" or a ticker appears in summons text) would need to be a **counted,
  logged aggregate that a human or a rule reads**, structurally identical to
  the append-only suggestion log ADR 0034 already specifies — not a new
  category of risk, an extension of one already decided. Cost: the X API
  read price already measured in research 0051 §1 applies (`$0.005 per
  post-resource` for reads, CHECKED there 2026-09-18) — reading more of the
  existing mention stream for text-frequency counting is not a new API
  product, just more reads at the same rate.
- **Volume shifts** — DefiLlama's DEX-volume API (already used in research
  0035 §4, free, keyless) gives Robinhood Chain's aggregate DEX volume by
  day; a shift in what fraction of new launches include a stock-sounding or
  crypto-sounding name, cross-referenced against that volume trend, is
  buildable from sources already in evidence in this tree — again no new
  vendor.

**What would be new:** the actual clustering/counting code (a "narrative
score" is not a `Fact::kind` today), a place to store it (base-rate-shaped:
dated, refreshed on a schedule, with a stale-after policy like
`baserates.rs`'s 60-day rule, per the same reasoning that broke `0008`), and
a rule for what a paid-API customer is allowed to see versus what stays
internal analysis — none of this touches AGENTS.md §3 rule 2 ("the model may
not introduce a fact") as long as a narrative score is itself a fact
computed by code and placed on the sheet before the model ever sees it,
following the same shape as `level()` in `verdict.rs` already does for the
rug-check score (research 0054 §2.2, CHECKED there).

## 4. "Record everything" — what is recoverable later versus lost if not captured now

**Recoverable later, at a cost, from archive RPC:**
- **Chain history.** Research 0039 §2 (CHECKED, this session's own read
  above) already prices this: QuickNode's Robinhood Chain archive access
  ("no pruning") is inside the same $49/month plan already recommended for
  live indexing — a full historical backfill from block 26,921,206 is
  ≈171,000 `getLaunchedToken` calls (research 0038's extrapolate, reused by
  0039), i.e. **replaying chain history later costs API calls against an
  archive node, not a separate storage bill, as long as an archive-node
  vendor keeps serving chain 4663.** The risk is not cost, it is
  **availability**: research 0039 §8 flags "whether Alchemy keeps archive
  state for chain 4663" as not established, and free public RPC has no
  archive guarantee at all (0035: "rate-limited and not recommended for
  production"). If every archive provider ever prunes or drops the chain,
  history becomes unrecoverable — but today, at least one vendor (QuickNode)
  states it does not prune this chain, so chain history is **recoverable
  later at low marginal cost, conditional on that vendor continuing to offer
  it.**

**Lost if not recorded now — no replay exists for these:**
- **X posts at the moment they were posted.** X's own Developer Agreement
  (research 0051 §1, CHECKED 2026-09-18) requires syncing deletions "within
  24 hours" — a post captured, then deleted by its author, cannot be
  legitimately kept past that window even if scraped once; a post never
  captured at all cannot be recovered later at any price, because X's API
  serves current state, not a historical firehose of already-deleted
  content, for a keyless/low-tier caller.
- **Prices at a moment**, for anything without a block-stamped onchain
  record — e.g. Robinhood's own `/prices/{symbol}` bid/ask (§1) if it is a
  live-only feed with no historical endpoint (not checked whether one
  exists), or any off-chain vendor's real-time tick that its own plan does
  not entitle to historical replay.
- **Mempool.** Never persisted anywhere by any RPC provider named in this
  tree's research; a pending transaction that never confirms, or a
  front-run/back-run relationship visible only in mempool ordering, is gone
  the moment it is not captured, permanently — no vendor sells mempool
  history because none exists to sell.
- **Our own verdicts/sheets alongside later outcomes.** This is the one
  most specific to this project's stated goal (calibration, research 0052
  §8's M-D-0006 hold, and research 0054 §2.2's finding that the level is
  "not yet calibrated"): a verdict issued today, paired with what actually
  happened to that token seven days later, is a training/calibration pair
  that **cannot be reconstructed after the fact** if the original verdict
  (the exact facts on the sheet, the exact level published, the exact
  moment) was not stored — recomputing "what would we have said then" from
  today's chain state is not the same claim as "what we actually said then."
  This is the single highest-value item to record starting now, because it
  is both cheap (already-computed data, no extra reads) and irreplaceable.
- **API request logs**, once the paid endpoint exists: who asked what and
  when is gone the moment the request completes unless logged — recoverable
  from nowhere else, and directly useful for both moat-building (what do
  buyers actually ask for, feeding back into §5) and abuse/billing disputes
  under x402's no-chargeback model (research 0054 §1.5, CHECKED: "no
  chargeback path, no dispute window").

**Storage-size estimate for the lost-if-not-recorded set, SQLite on the box.**
No measured figure exists in this tree for any of these specifically (the
closest analog, `design 0021`'s wallet-memory sizing, states outright "No
bytes-per-wallet figure is claimed here; the size on disk is... measured,
not asserted" — CHECKED, `docs/design/0021-the-read-memory.md:446-448`, read
2026-09-19). Order-of-magnitude, reasoned from volumes already established
in this tree, not measured:
- **Verdicts+sheets:** at research 0039's own volume assumption (300
  replies/day), one JSON-shaped sheet+verdict pair per reply at a few KB
  each (comparable in shape to the `Entry` structs already logged by
  `crates/realorrug-analyst/src/log.rs`, CHECKED, file read 2026-09-19, an
  append-only log format already in production use) is roughly **300 × 3KB
  ≈ 1 MB/day, ≈30 MB/month** — trivially small.
- **X posts/mentions text:** at a similar few-hundred-per-day mention
  volume (research 0051's framing), plain text plus metadata at well under
  1 KB each is **well under 1 MB/day**, negligible.
- **API request logs**, once the paid endpoint launches: depends entirely on
  call volume, which does not exist yet (research 0054 §1.7's own finding
  that x402 volume today is mostly wash/test traffic industry-wide) — not
  estimable with a real number, but at even 10,000 calls/day and 1 KB/call
  metadata that is **≈10 MB/day, ≈300 MB/month**, the largest item in this
  set by an order of magnitude if the paid API succeeds at any real scale.
- **Total, rough order of magnitude: tens of MB/month today, growing to
  hundreds of MB/month only if the paid API reaches real volume** — nowhere
  close to forcing anything on its own.

**When this forces Postgres.** Research 0054 §4.1 (CHECKED, this session's
own earlier read) already answered the *architectural* trigger, unchanged by
this document's numbers: "Postgres becomes the right call the moment there
is a separate API host... because that is two processes wanting the same
state without a shared filesystem" — not a storage-size trigger. The sizes
estimated above (tens to low hundreds of MB/month) would not force SQLite to
its knees on disk size alone for years; the concurrency shape research 0054
already named is still the actual trigger, and Neon's pay-as-you-go tier
(0054 §4.2, CHECKED against neon.com/pricing, read 2026-09-19) remains the
cheapest first step whenever that trigger fires.

**Recommendation on what to record starting now (cheap and irreplaceable
first):**
1. **Every verdict/sheet pair, paired with a later-outcome check** — already
   partially structured (design 0021's memory, `realorrug-analyst`'s log),
   cheapest to extend, and the one item this project's own calibration work
   (research 0052) directly needs and cannot get any other way.
2. **API request logs**, from day one of the paid endpoint — needed for
   billing-dispute defense under x402's no-refund model (research 0054
   §1.5) regardless of the moat argument.
3. **Raw mention/summons text**, beyond what AGENTS.md already requires
   logging for the suggestion-log rule (ADR 0034 decision 5) — cheap, and
   the raw material for §3's narrative-detection idea if it is ever built.
4. **Chain history: do not hoard it.** Per §4 above, it is recoverable later
   from an archive-RPC vendor at low marginal cost; spending storage budget
   duplicating what QuickNode already promises not to prune is the one item
   on this list that fails AGENTS.md §4's "smallest change" test.

## 5. What agents and humans actually buy from onchain-analytics APIs today

**Nansen.** API credit pricing (search summary of `docs.nansen.ai/getting-
started/credits`, read 2026-09-19, **UNVERIFIED — not fetched directly**):
"$10 for every 1,000 API credits," with bulk discounts over 5 million
credits; per-endpoint credit cost varies. Nansen's product is broadly
wallet-labeling, smart-money flow tracking and portfolio analytics — sold as
a research/dashboard product with an API layer on top, not primarily a
per-call micro-payment product.

**Arkham.** Not priced this session — search returned no specific API
pricing figure (only that Arkham Intel exists); **not established**, a real
gap.

**Birdeye.** Pricing pages exist (`birdeye.so/data-api/pricing`,
`docs.birdeye.so/docs/pricing`, found via search 2026-09-19) but **no
specific dollar figures were extracted this session** — pages describe
"flexible plans for individuals, teams & enterprises" without a number
surfacing in the summary. **Not established**, a real gap; a direct fetch of
Birdeye's pricing page would be the next step.

**GoPlus Security.** Describes itself as "an open, license-free Web3.0
security data API service" (search summary, gopluslabs.io, read 2026-09-19,
**UNVERIFIED**) with "Pro, Ultra or Enterprise Packages" for customization —
no dollar figures surfaced. GoPlus's own promotional post (X, dated per the
post itself, read via search 2026-09-19, **UNVERIFIED, self-reported**)
claims "23.1M+ GoPlus Request Token Count... 8.08M+ Token API calls serving
industry leaders like... DexScreener" — i.e., **GoPlus's actual business
model, confirmed by its own marketing, is B2B2C: other platforms (DexScreener
among them) embed its honeypot/rug-check API and pay for volume, rather than
end users buying GoPlus access directly.** This is a meaningfully different
shape from realorrug's own X-bot-plus-facts-endpoint model, and is the
closest existing analog to "sell the fact sheet to other tools," not just to
individual traders.

**DexScreener, Bubblemaps.** No subscription/API pricing surfaced for either
this session (DexScreener is known, from general familiarity rather than a
source fetched today, to monetize primarily via paid token-page boosts and
ad placements rather than a metered data API — **this is not sourced to a
fetched page this session and should be treated as background context, not
a verified finding**). **Not established**, a real gap for both.

**Net finding, weakly supported but directionally clear from GoPlus's own
numbers:** the highest-confirmed-volume pattern in this space is **other
tools embedding a security/data check as a feature**, not end-users paying
per query directly — consistent with x402's own finding in research 0054
§1.7 that real agent-to-agent commerce is still thin. This suggests the
"next thing to squeeze in" after the $0.05 facts endpoint is a
**pluggable check** (a honeypot/rug flag, a creator-history badge) that
other bots, wallets, or explorers would want to embed for their own users,
mirroring GoPlus's actual traction — not necessarily a bigger dashboard
aimed at end-user traders.

## Ranked recommendation and what each costs

1. **Ship the $0.05 facts endpoint first, stock-free, exactly as research
   0054 already recommends** (selling the measured fact sheet, not the
   uncalibrated verdict) — **cost: near-zero incremental**, the plumbing is
   already named in this tree (research 0054 §5, ≈$0.013/call worst-case
   floor). This is not new work from this document; it is the prerequisite
   everything below sits on.
2. **Start recording verdict+sheet pairs against later outcomes, and API
   request logs, now** (§4) — **cost: tens of MB/month on the existing
   SQLite file, effectively free**, and it is the one thing in this whole
   document that cannot be bought back later at any price if skipped.
3. **Build the cheap narrative signal from data already collected**
   (Pons v2 launch name/symbol clustering + DefiLlama volume + the existing
   mention log, §3) as an internal dashboard, not a paid feature yet — **cost:
   engineering time only, no new vendor, no new AGENTS.md rule conflict** —
   and defer both the stock-data angle (§1's unresolved RHJ debt-security
   legal question) and any off-chain equities vendor (§2's unresolved
   redistribution terms) until the narrative signal itself shows stocks
   actually entering the token-launch or mention data, which is the
   evidence that would justify paying for either.

**Deliberately not ranked yet: a Robinhood stock-token reader or an
off-chain equities feed.** Both are technically cheap to add to the existing
RPC subscription (§1) or a $99/month Alpaca-class plan (§2), but §1's
"tokenised debt securities... issued by Robinhood Assets (Jersey) Limited"
finding and §2's unread redistribution terms are exactly the kind of
unchecked number AGENTS.md §1 says must be verified before a decision rests
on it — building this before either question is answered risks selling data
whose resale isn't clearly permitted, which is a worse position than being
one step behind the narrative.

## Questions only the owner can answer

1. Does the lawyer's still-open review (ADR 0023 decision 5, research 0054's
   question 1) cover *reading and republishing* a Robinhood stock token's
   onchain/price data for a paid endpoint, or only pairing/trading it as ADR
   0034 already considered — because §1's RHJ "tokenised debt securities"
   framing may put even a read-only stock-data product in the same
   regulatory shape, and only the owner (via his lawyer) can say whether
   that question has already been asked.
2. Is "record everything" meant to include content this project has no
   clear right to keep past its source's own deletion rules — e.g. an X post
   after its author deletes it (X's 24-hour sync duty, §4) — or is the
   instruction scoped to data this project generates itself (verdicts,
   request logs) plus chain data, which has no deletion-rights question at
   all? The recommendation in §4 assumes the latter; the former would need
   its own compliance read.
3. Given §5's finding that the clearest analog business (GoPlus) sells to
   *other platforms embedding a check*, not to end-users querying directly,
   does the owner want the paid API's second product to target other bots/
   wallets/explorers as B2B customers, or individual traders/agents as
   direct end-users — this changes what "squeezed out value" should look
   like next, and this document does not have enough pricing data (Arkham,
   Birdeye, DexScreener, Bubblemaps all came back unpriced this session) to
   make that call on numbers alone.

## Confidence and what would change it

**CHECKED**: everything cited from this repository's own files
(`robinhood.rs`, `baserates.rs`, `salience.rs` reference, `memory.rs`/design
0021, `realorrug-analyst/src/log.rs`, AGENTS.md, ADR 0034, research
0038/0039/0051/0052/0054's own prior findings) and the two Robinhood-chain
pages fetched directly today (`docs.robinhood.com/chain/stock-tokens/`,
`docs.robinhood.com/chain/stock-token-apis/`). **UNVERIFIED**: every
off-chain data-vendor pricing and redistribution claim in §2 and every
onchain-analytics pricing claim in §5 — all came from search-engine
summaries, none from a page fetched and read directly this session; this is
this document's largest gap and the first thing a follow-up should fix
before a dollar figure from §2 or §5 is used to decide anything. **A second
search could not change §1's core legal-shape finding** (RHJ's own docs
already state the "tokenised debt securities" framing in the clearest
possible first-party language) without Robinhood changing its own
documentation — that narrow point is settled for now. **What would change
the ranked recommendation:** a direct fetch of Alpaca's and Polygon/Massive's
actual terms-of-service redistribution clauses (§2); a direct read of
Robinhood's `/assets` and `/prices` endpoints' access terms (§1); and a
direct fetch of Birdeye's, Arkham's and DexScreener's pricing/monetization
pages (§5) — none of which were reached this session on the time budget
available.
