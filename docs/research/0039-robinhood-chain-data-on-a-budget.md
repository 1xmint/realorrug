<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0039 — Robinhood Chain data on a budget

**Date:** 2026-09-15
**Status:** read and partly tested. Vendor pricing pages were fetched today and
are quoted with URLs; several are client-rendered and did not yield exact
digits to a scripted fetch — those are marked "documented, page confirmed to
exist, digits not extracted" rather than guessed. Test calls used only
keyless, free endpoints, no sign-ups, no keys, nothing that costs money, per
the packet's boundary. Per-call costs are not on the vendors' pricing pages,
but they are published elsewhere on the vendors' own sites, and were read
there on grading (§2): Alchemy's compute-unit table and QuickNode's API-credit
page. The monthly costs below rest on those, applied to the volume assumed.

## How it was read

Robinhood Chain mainnet (chain id 4663) via the public RPC
`https://rpc.mainnet.chain.robinhood.com`, one `eth_blockNumber` call, on
2026-09-15 (tip at read time: block 63,981,914) — reused, not re-derived,
against research 0038's facts (10,000-log cap, 429s, state pruned after
~100k blocks, block time 0.1019 s, factory first block 26,921,206). Test
calls against DexScreener's and GeckoTerminal's public APIs and Blockscout's
API used `curl` and, where a Cloudflare challenge blocked `curl`, the
project's Chrome-based browser tool. Vendor pricing pages were read with a
web-fetch tool and, where that tool returned no digits (client-rendered
pages), with `curl` against the raw HTML and a `node` script to search a
static build embedded in the page (this recovered real numbers for
QuickNode's and dRPC's Robinhood Chain pages, and for Dune's pricing table
via the browser tool, but not for Hetzner's). Raw responses are under
`docs/research/data/0039-*.json`. The HTML captures used for digit
extraction, and Blockscout's challenge pages, were not kept.

The tokens used for chain reads are the ones research 0036 already captured
from a real launch: factory `0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e`,
curve `0xddf3afb29e265b00c48015c3aacdedcb10088fcf`, token
`0x22fd486d80b7cce7362ffed59bbf2fd266a148fa` — no new transaction was sent.

## Volume assumed (from the packet, stated here so the cost table is checkable)

- Launches now: 5,600 / 4h → 33,600/day → **≈1,008,000/month**. At the stated
  ≈20x growth since 2026-08-03 (confirmed independently in research 0038 §1
  from two separately-read block windows), **3x today ≈ 3,024,000/month** is
  the packet's stress case, not a prediction of when it is reached.
- Per launch indexed: 1 log (`TokenLaunched`, read in batches under the
  10,000-result cap, cheap) + 1 `getLaunchedToken` `eth_call` (or one indexed
  row, same cost shape) → ≈1.0M / 3.0M `eth_call`-equivalents per month.
- Replies: 300/day × 15 reads = 4,500/day → **135,000/month**.
- Seven-days-later check: 300/day × 5 reads = 1,500/day → **45,000/month**
  (steady state, once the bot has been running seven days).
- **Total RPC-shaped reads/month: ≈1.19M now, ≈3.20M at 3x.** Dominated by
  the per-launch `getLaunchedToken` call, not by the bot's own reply logic.
- One-off backfill from block 26,921,206: research 0038's own extrapolate
  (not a full count) is **≈171,000 historical launches** as of 2026-09-15;
  this document does not re-derive that number.

## 1. Summary table — source × need, cost at today's and 3x volume

| source | #1 launch+creator | #2 graduation | #3 trades/first fill | #4 spot price now | #5 holder share | #6 history to block 26.9M | #7 creator track record | monthly cost, today | monthly cost, 3x |
|---|---|---|---|---|---|---|---|---|---|
| **Paid archive RPC (Alchemy/QuickNode/dRPC/etc.), feeding our own index** | yes, direct | yes, direct (phase word; graduating event not yet identified, 0038) | yes, direct (already decoded in `pons.rs`) | **only approximate** (last-trade ratio; no reserves getter read yet) — pre-graduation only | **exact, by summing the token's ERC-20 `Transfer` logs ourselves** — no holder-list call on-chain | yes: logs from genesis everywhere; past *state* only where the provider keeps archive (QuickNode says it does for this chain) | **yes, once we already store every launch — no extra calls** | ≈$2 Alchemy, or $49 flat QuickNode Build (§2) | ≈$30–$45 Alchemy, or the same $49 QuickNode Build |
| **Free public RPC** (already in use) | yes | yes | yes | same limits as above | same limits as above | yes, but "rate-limited, not recommended for production" (0035); 0038 measured a full-day backfill at this endpoint's pace | yes, same caveat as paid RPC | $0 | $0, but likely fails politeness/latency at 3x (0038 §4) |
| **Blockscout API** | documented only — **blocked in this session** (§3) | documented only, blocked | documented only, blocked | not applicable pre-graduation | **would answer directly if reachable** (holders endpoint exists) | documented only | no — one token/creator at a time, not a population query | not established (blocked) | not established |
| **Dune** | yes, via SQL over `robinhood.logs_decoded`/`erc20_robinhood.evt_*` | yes, if the graduation event is decoded there | yes | yes, if a price table exists (not checked) | yes, one query | yes, already-indexed history, no pruning problem | **yes, in one SQL query — this is Dune's actual edge over RPC** | $0 (view-only Free plan cannot run/schedule queries) → **$65/mo** (Analyst, 4,000 credits) is the first tier that can query at all | likely still $65–$349/mo; credit cost per query not sized against our exact SQL (§4) |
| **GeckoTerminal / DexScreener** | no | **implicitly** — a pair appearing at all means the curve graduated (confirmed, §5) | only post-graduation Uniswap trades, not curve trades | **yes, but post-graduation only** | no | no | no | $0 (free, keyless, rate-limited) | $0, same rate-limit risk |
| **Our own Arbitrum Nitro full/archive node** | yes | yes | yes | same approximation limit as paid RPC | same replay-only limit | yes, if run as archive from genesis | yes, once indexed | server + L1 endpoints, see §6 — **not viable on the current 2-core VPS** | same |

## 2. Paid RPC with archive state on chain 4663

All five vendors the packet names, plus two more research 0035 had not
listed that Robinhood's own docs name as supporting the chain (Chainstack,
GlobalStake) — found in §6 below, not chased further here (boundary: no
sign-ups).

- **Alchemy.** Reconfirmed today: "Free 30M CU per month", "25 requests per
  second", pay-as-you-go **"$0.525/1M CUs"**
  ([alchemy.com/pricing](https://www.alchemy.com/pricing), read 2026-09-15;
  unchanged from research 0035's 2026-09-13 read). Archive data is on all
  plans at the same per-CU rate; Enterprise adds "Increased eth_getLogs()
  ranges" — implying free/pay-as-you-go tiers have a *narrower* getLogs range
  than Enterprise, on top of the 10,000-result cap already measured on the
  public endpoint (0038). **No per-method compute-unit table (what an
  `eth_call` or `eth_getLogs` costs in CU) is on Alchemy's own pricing page**
  — but Alchemy publishes one on its docs site
  ([compute-unit costs](https://www.alchemy.com/docs/reference/compute-unit-costs),
  read 2026-09-15 on grading): **`eth_call` 26 CU, `eth_getLogs` 60 CU,
  `eth_getBlockByNumber` and `eth_getTransactionReceipt` 20 CU,
  `eth_blockNumber` 10 CU.** At today's volume: 1.01M launch `eth_call`s
  (26.2M CU), `eth_getLogs` once a minute for new launches (43,200 calls,
  2.6M CU), and 180,000 reply and seven-days reads, costed as `eth_call`
  (4.7M CU): **≈33.5M CU/month, ≈$18 at $0.525/1M, or ≈$2 if the free 30M
  still counts once paying.** At 3x: **≈86M CU, ≈$29–$45/month.** Which of
  the two readings of the free allowance applies is not stated on the
  pricing page. Whether Alchemy keeps archive state for chain 4663
  specifically was not read.
- **QuickNode.** Robinhood Chain page confirmed genuine (chain-specific FAQ
  content, not a generic template — verified by grepping the raw page for
  "4663" and the actual sequencer/explorer text, not just trusting a
  fetch-tool summary): "Robinhood Chain mainnet uses chain ID 4663... Both
  use ETH as the native gas token... the Blockscout explorer at
  robinhoodchain.blockscout.com" and **"Quicknode runs full archive nodes for
  Robinhood Chain mainnet and testnet with no pruning"**
  ([quicknode.com/chains/robinhood](https://www.quicknode.com/chains/robinhood),
  read 2026-09-15). Plan prices from
  [quicknode.com/pricing](https://www.quicknode.com/pricing) (read
  2026-09-15): **Build $49/mo, 80M API credits; Accelerate $249/mo, 450M;
  Scale $499/mo, 950M; Business $999/mo, 2B**; a $0/mo free trial gives 10M
  credits; Build allows 50 requests a second, and overage is $0.62 per
  million credits. QuickNode's
  [API credits page](https://www.quicknode.com/api-credits/eth) (read
  2026-09-15 on grading) prices Ethereum-style chains at **"All methods*" =
  20 credits**, the asterisk excepting "Advanced APIs and Large Calls".
  Robinhood Chain is not named on that page, so the same rate is inferred for
  it. At today's ≈1.24M calls a month that is **≈25M credits; at 3x, ≈65M**:
  both inside Build's 80M, so **$49/month flat** covers the stress case with
  room. Archive access is not a separate charge.
- **dRPC.** Listed on Robinhood's own full-node doc with a Robinhood-specific
  URL, `drpc.org/chainlist/robinhood-testnet-rpc` (quoted in §6), and
  confirmed present on `drpc.org/chainlist` (raw HTML contains "robinhood"
  and "4663", read 2026-09-15). **Its pricing page
  (drpc.org/pricing) is client-rendered and returned no digits to either
  fetch method tried this session** — documented as existing, price not
  captured.
- **Blockdaemon.** Named on Robinhood's own docs with
  `docs.blockdaemon.com/docs/how-to-connect-to-robinhood` (quoted in §6,
  confirming Blockdaemon supports the chain). Its public pricing page was
  not found at the URL guessed (`blockdaemon.com/pricing`, 404, read
  2026-09-15) — Blockdaemon's RPC pricing is enterprise/sales-quoted, not
  posted; **documented, not tested, price not public.**
- **Validation Cloud.** Named on Robinhood's own docs at
  `validationcloud.io/robinhood` (§6). The guessed pricing URL
  (`validationcloud.io/pricing`) 404'd, read 2026-09-15; **documented, not
  tested, price not public.**
- **Ankr / others.** Not named on Robinhood's own connecting page (research
  0035 §1) or on the full-node doc's provider list (§6); not chased further
  — absence from both first-party lists is itself the finding, not proof
  Ankr cannot serve the chain.

## 3. Blockscout (robinhoodchain.blockscout.com)

**Blocked in this session, both by scripted `curl` and by the project's
Chrome-based browser tool.** `curl -A "Mozilla/5.0..." https://robinhoodchain.blockscout.com/api/v2/tokens/0x22fd486d80b7cce7362ffed59bbf2fd266a148fa`
returned **HTTP 403** with a Cloudflare "Just a moment..." challenge page
(read 2026-09-15; the challenge page was not kept), and the same
for `/holders` and `/transfers`. Loading the same URL in the browser tool
showed the identical "Performing security verification" challenge. (The
browser tab then showed `cabalhunter.org`, this project's own site: the tab
was shared with another task in the same session that navigated it there.
It was not a redirect from Blockscout.) Rechecked on grading with `curl`:
still 403 with the same challenge. **Net finding: Blockscout's API could not
be used keylessly from a script today.** Its documented
free-tier limits and API-key tiers were not read this session (would need a
docs page, not the API itself, to check without further attempts against a
site that just misbehaved) — marked not established rather than assumed
working or broken.

## 4. Dune

Robinhood Chain tables exist, confirmed today from
[docs.dune.com/data-catalog/evm/robinhood/overview](https://docs.dune.com/data-catalog/evm/robinhood/overview)
(read 2026-09-15, unchanged in substance from 0035's 2026-09-13 read): raw
`blocks`, `transactions`, `logs`, `traces`, `creation_traces` under a
`robinhood` schema; decoded `robinhood.contracts`, `robinhood.signatures`,
`robinhood.logs_decoded`, `robinhood.traces_decoded`; per-token
`erc20_robinhood.evt_*` / `erc721_robinhood.evt_*`; and a
`"{project}_robinhood.{contract}_evt_{Event}"` pattern that would fit a
decoded Pons v2 factory table if one has been indexed by a Dune user — **not
checked whether a Pons v2-specific decoded table already exists**, only that
the schema convention supports one.

**Pricing, read from `dune.com/pricing` via the browser tool on 2026-09-15**
(this is a client-rendered SPA; a plain fetch returned nothing, so this
number is from the rendered page, not raw HTML):

| plan | price | credits/mo | price/extra credit | API calls/min |
|---|---|---|---|---|
| Free | $0/mo | — | — | — (view-only; **cannot run or schedule queries or call the API**) |
| Analyst | $65/mo, 3 seats | 4,000 | $0.016 | 40 |
| Plus | $349/mo, 10 seats | 25,000 | $0.014 | 200 |
| Enterprise | custom | 100,000+ | custom | custom |

Sample credit costs shown on the same page: a "Small" engine query, 120s
timeout, "≈3.27 credits" for one example query that ran 63s; "Medium",
"≈8.46 credits" for 22s; "Large", "≈18.72 credits" for 7s. **These are Dune's
own example numbers for an unspecified query, not our workload** — how many
credits req #7's "this creator's track record across all launches, and how
this launch compares to the whole population" query would cost was not
measured (would need an actual account to run it; boundary forbids
sign-ups). **What is established: the cheapest tier that can run any query
at all is $65/month**, and Dune's real advantage — one SQL query answering
req #7 across the whole population, versus one RPC call per historical
launch — is architectural, not proven cheaper in dollars this session.
Freshness lag (how far behind chain tip Dune's Robinhood tables run) was
**not found on the docs page and not tested** (no keyless query route
exists to check it without an account).

## 5. DEX/price APIs

**GeckoTerminal.** The network id **`robinhood`** exists in GeckoTerminal's
own network list — confirmed by paging `api.geckoterminal.com/api/v2/networks`
(pages 1-3, 229 networks total) and finding
`{"id":"robinhood","type":"network","attributes":{"name":"Robinhood",...}}`
(`docs/research/data/0039-geckoterminal-networks-p2.json`, read 2026-09-15).
Token- and pool-level test calls (`/networks/robinhood/tokens/{addr}`,
`/networks/robinhood/pools`) both returned **HTTP 429** ("You've exceeded
the Rate Limit... use the onchain endpoints", quoted verbatim,
`docs/research/data/0039-geckoterminal-token.json`) even after a 20-second
backoff — the free keyless tier's rate limit is tight enough that this
session could not get a positive token-level read; the chain-support finding
stands regardless.

**DexScreener.** `api.dexscreener.com/latest/dex/tokens/{Pons v2 token}` and
`.../token-pairs/v1/robinhood/{token}` both returned empty
(`{"pairs":null}` and `[]`, `docs/research/data/0039-dexscreener-token.json`,
`0039-dexscreener-pairs.json`, read 2026-09-15) for the still-on-curve token
captured in research 0036. A broader search,
`api.dexscreener.com/latest/dex/search?q=robinhood`
(`docs/research/data/0039-dexscreener-search.json`), **did** return real pairs
with `"chainId":"robinhood"`, `"dexId":"uniswap"`, labelled `"v3"` and
`"v4"`, with live price/volume/liquidity fields. **This settles it: both
DexScreener and GeckoTerminal cover Robinhood Chain, but only *graduated*
Uniswap pools** — the empty result for the un-graduated Pons v2 token is
explained by it never having graduated, not by the chain being unsupported.
**Since research 0036 measured only 3 of 250 ETH-paired launches graduating
(1.2%)**, these free price APIs answer req #4 (spot price now) for a small
minority of tokens; for the rest, price must come from the curve's own
trades or state, which neither API can see.

**Birdeye, Codex.io.** Not confirmed to cover Robinhood Chain this session:
Birdeye's docs URL redirected and no Robinhood mention was found in the
homepage HTML fetched; Codex.io's pricing page loaded but contained no
Robinhood-chain-specific text checked for. **Documented as attempted, not
established either way** — a further check would need their chain-list
pages specifically, not attempted given budget.

## 6. Running our own node

**A sequencer is operator-only, inferred, one line quoted.** Arbitrum's own
docs describe "Arbitrum's long-term vision includes transitioning from a
centralized Sequencer to a decentralized, fair sequencing model"
([docs.arbitrum.io/how-arbitrum-works/sequencer](https://docs.arbitrum.io/how-arbitrum-works/sequencer),
read 2026-09-15) — today's sequencer is centralized, and Robinhood's own
full-node doc (below) offers only *reading* the sequencer's feed to a
follower node, never *running* one; no page found offers third parties a way
to become or replace the sequencer.

**Robinhood does publish chain config, genesis and a feed URL for a
third-party Nitro node — confirmed, quoted, from
[docs.robinhood.com/chain/run-a-full-node](https://docs.robinhood.com/chain/run-a-full-node)
(read 2026-09-15, raw HTML fetched directly since the page is static enough
to parse):**

> "Robinhood Chain is an Arbitrum Chain running Arbitrum Nitro... Download
> the config files for your target network: Mainnet: chain info
> [robinhood-chain-info.json] and genesis [robinhood-genesis.json]"

Actual file URLs, from the page's own links:
`https://cdn.robinhood.com/assets/generated_assets/hoodchain_docsite/chain-node-configs/robinhood-chain-info.json`
and `.../robinhood-genesis.json`. The image/container: **"offchainlabs/nitro-node:v3.11.2-3599aca"**.
The feed: **`wss://feed.mainnet.chain.robinhood.com`** (testnet:
`wss://feed.testnet.chain.robinhood.com`).

**Hardware, quoted:**

> "CPU: Modern multi-core (8+) CPU with strong single-core performance...
> RAM: 64 GB RAM (128 GB recommended)... Storage: Locally attached NVMe SSD;
> (2 × current chain size) + 20% buffer. Several TBs of data... Node Type:
> Full node (Archive nodes require substantially more disk capacity)"

**L1 requirement, quoted:** "your node needs access to an Ethereum (L1)
endpoint — your own or via a provider... An L1 execution RPC endpoint... An
L1 beacon (consensus) endpoint — required to read blob data." An L1
execution+beacon endpoint is itself a paid product on the usual providers
(Alchemy/QuickNode/etc. all sell Ethereum mainnet RPC with beacon access on
similar per-CU terms to §2; not separately re-priced here since it is the
same vendors and the same unconfirmed per-method-cost gap).

**Disk growth, inferred from research 0038's 0.1019 s/block.** At ~9.8
blocks/second that is ≈846,720 blocks/day. Robinhood's own doc gives no
per-block or per-day size figure (only "several TBs" total so far, with no
date attached, so a growth rate cannot be derived from it) — **disk-growth
rate itself is not established**, only that the *current* size is already in
the multi-TB range four and a half months after 2026-07-01 mainnet.

**Robinhood's own provider list (§2 already covers Alchemy/QuickNode/
Blockdaemon/dRPC/Validation Cloud) adds two more, quoted verbatim from the
same full-node page:** Chainstack
(`https://chainstack.com/build-better-with-robinhood-chain/`) and GlobalStake
(`https://GlobalStake.io`) — neither checked for pricing this session
(budget; neither was in the packet's named list).

**Hetzner, price not captured — a real limitation, not a shortcut taken.**
`hetzner.com/cloud` (read 2026-09-15, both via a web-fetch tool and raw
`curl`) confirms three tiers exist — "Cost-Optimized" (shared, "extremely
cost-effective"), "Regular Performance" (shared, "best price-performance
ratio"), "General Purpose" (dedicated vCPUs) — but **the page renders its
"starting from" digits client-side; neither fetch method extracted a euro
figure**, and `docs.hetzner.com/cloud/servers/overview/` explicitly defers
pricing to the same page. **No dollar/euro number for a Hetzner box is
quoted in this document** — a real gap, not an assumption. What can be
said: a 64GB-RAM, 8-core, multi-TB-NVMe box needed for even a *pruned* full
node is well above Hetzner's cheapest shared tiers (those top out around
8GB/4-core in the general market this size of provider serves) and would
sit in "General Purpose" dedicated territory — a category, not a price.

**Our current VPS: cannot host this.** The full-node hardware line above
("Modern multi-core (8+) CPU... 64 GB RAM (128 GB recommended)... several
TBs of NVMe") is far past a two-core VPS on every axis (cores, RAM, disk) —
**this is a direct mismatch, not close enough to matter.** Running our own
node is not viable on the box we have today regardless of what a bigger box
costs.

## 7. Recommendation

**Use QuickNode's Build plan ($49/month) as the only new subscription, and
build our own index from it** (amended on grading; the researcher left
QuickNode and Alchemy open). QuickNode is chosen over Alchemy, which is
cheaper today, for three reasons: it states archive nodes "with no pruning"
for this chain by name, and Alchemy's archive for chain 4663 was not read;
its price does not move with launch volume, which has risen twentyfold since
August; and 3x today's volume still fits its 80M credits. The index is the one the
volume assumptions already require for req #1 (one row per launch) doubles
as the answer to req #7 (creator track record, population comparison) with
no additional calls or vendor.** Concretely: swap the free public RPC
(`rpc.mainnet.chain.robinhood.com`, still fine for occasional manual checks,
but 0038 already showed it 429s under burst and is undocumented for
sustained backfill pace) for a paid provider once launch volume or backfill
needs exceed what "polite" calling can sustain — which, per 0038 §4, is
already true for a from-genesis backfill (171,000 `getLaunchedToken` calls)
and will only get truer as the ≈20x-since-August growth continues.

**Monthly cost: $49 flat**, using ≈25M of 80M credits today and ≈65M at 3x
(§2). Past ≈4x today's volume it spills into overage at $0.62 per million
credits, or the $249 Accelerate plan. Alchemy pay-as-you-go would be ≈$2–$18
today and ≈$29–$45 at 3x: it stays the fallback if QuickNode disappoints.

**What this setup gives up:**

- **No vendor answers spot price or holders before graduation.** No curve
  reserves or spot-price getter has been read on any Pons v2 curve (0038,
  unchanged here), so the pre-graduation price signal is the last trade's
  ratio. Holder concentration is exact but ours to compute: every Pons v2
  token is an ERC-20, so summing its `Transfer` logs gives every balance.
  DexScreener and GeckoTerminal index only post-graduation Uniswap pools (§5),
  and Blockscout's holder endpoint was blocked (§3). (The bot never states a
  price anyway, AGENTS.md §3 rule 5.)
- **The graduation event itself is still unidentified** (0038's gap,
  unchanged): the phase word flips from 0 to something, but which log marks
  it, and from which contract, was not found this session either.
- **Dune is not part of this recommendation**, even though it is the only
  source that could answer req #7 without our own index — because we need
  the index for req #1 regardless, making Dune's $65-349/month redundant
  rather than cheaper.
- **Blockscout is left unverified**, not ruled out — it may work fine
  with an API key; today's keyless attempts hit a Cloudflare challenge (§3).

**What needs Josh to sign up or add a key:** a QuickNode account
and API key (regular paid usage, no special approval needed from either
vendor per their public pages); nothing else in this recommendation requires
a sign-up. Dune, Blockscout with an API key, Birdeye, and Codex.io are all
explicitly **not** part of the recommendation, so no sign-up for those is
needed unless a later document reopens req #4/#5 for pre-graduation tokens.

## 8. Not established

- Whether QuickNode's 20-credit rate applies to Robinhood Chain (the credits
  page names Ethereum-style chains, not this one), and whether Alchemy's free
  30M CU still counts on pay-as-you-go.
- Whether Alchemy keeps archive state for chain 4663.
- Whether Blockscout's API is usable keylessly from any request path, or
  with an API key: blocked by a Cloudflare challenge in every attempt (§3).
- Dune's actual credit cost for the specific req #7 query (creator history +
  population comparison) — only generic example-query costs were read.
- Dune's data freshness lag behind chain tip for the `robinhood` schema.
- dRPC's, Blockdaemon's and Validation Cloud's own prices for Robinhood
  Chain — all three are confirmed to support the chain (Robinhood's own
  full-node doc names them), none published a public price this session.
- A euro/dollar figure for any Hetzner Cloud plan — the pricing page is
  client-rendered and no digits were extracted by either fetch method tried.
- Whether Birdeye or Codex.io cover Robinhood Chain at all.
- A reserves/spot-price getter on the Pons v2 curve contract, and the
  graduation event/block for any real launch — both still open from
  research 0038, unchanged by this session.
- Robinhood Chain's per-day or per-block disk-growth rate for a full/archive
  node — only a total-so-far ("several TBs") with no date was found.
