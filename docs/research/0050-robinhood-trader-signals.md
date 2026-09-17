<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0050 — Robinhood Chain: what is readable, how, and at what cost, for trader-style signals

**Date:** 2026-09-17
**Status:** read-only research, CHECKED tier on live-tested claims, CONDITIONAL
where it rests on vendor docs read but not exercised with a paid key (no
sign-ups per the packet's boundary). Builds directly on
[research 0038](0038-pons-v2-creators-and-outcomes-read-over-a-range.md),
[research 0039](0039-robinhood-chain-data-on-a-budget.md),
[research 0043](0043-wash-trading-detection.md),
[research 0044](0044-liquidity-manipulation-and-sell-blocking.md),
[research 0045](0045-wallet-analysis-and-clustering.md) and
[research 0048](0048-pons-v2-from-verified-source.md), all still current —
their 2026-09-15 pricing reads were re-verified live today (§1) and found
unchanged. This document's own contribution: it reproduces the live
"holders could not be read" failure against the exact token named in the
packet, confirms three method-not-available errors on the public RPC that
0045 had flagged as unpriced/untested, and reorganizes the prior documents'
findings around the six trader-judgement signals the owner asked for,
instead of around detection-technique or venue.

## 0. The holder-read failure, reproduced today

The packet's claim: on the live box, a 2.8-hour-old graduated token came
back "holders could not be read." Reproduced directly against
`0x13e6cdb0470b10afcb96177ae8702ace2ac72cd6` (token `HEY`, "hey.lol") on the
public RPC the bot uses by default
(`crates/realorrug-robinhood/src/lib.rs:585`'s `logs()`, called from
`crates/realorrug-onchain/src/robinhood.rs`'s `holders_from` path, which
takes `fromBlock: "0x0", toBlock: "latest"` with no pagination):

```
$ curl -s -X POST https://rpc.mainnet.chain.robinhood.com -d '{"jsonrpc":"2.0","id":1,"method":"eth_getLogs",
  "params":[{"address":"0x13e6cdb0470b10afcb96177ae8702ace2ac72cd6","fromBlock":"0x0","toBlock":"latest",
  "topics":["0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"]}]}'
{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"logs matched by query exceeds limit of 10000"}}
```
(tested 2026-09-17, this session)

**This is a result-count cap, not a block-range cap.** The error text is
byte-identical to the one research 0038 already triggered and quoted
(`docs/research/0038-pons-v2-creators-and-outcomes-read-over-a-range.md:123-124`:
`"logs matched by query exceeds limit of 10000"`, read 2026-09-15) and to
`lib.rs:585`'s own doc comment ("Alchemy: 10,000 logs, measured
2026-09-17 on this chain"). A block-*range* cap would reject the query
before counting anything (Alchemy free/growth enforces this on
`eth_getLogs` via a documented max block span on most chains — not the
error shape seen here); this error names a *result* count, and GeckoTerminal's
own read on the same token (below) shows why the count is high: the pool
graduated at `2026-09-17T14:41:52Z` and by the time of this test (a few
hours later) had **5,856 buys + 5,586 sells in the trailing 6h window
alone** (`api.geckoterminal.com/api/v2/networks/robinhood/tokens/0x13e6.../pools`,
read 2026-09-17) — each swap emits at least one ERC-20 `Transfer` (often
two or three, for a multi-hop v4 router path), so the Transfer-log count for
this one young, but heavily-traded, token plausibly clears 10,000 well
inside its first few hours. **The packet's implied hypothesis (a free-tier
*range* cap explaining the failure) does not hold — the actual cause is a
*volume* cap that any busy token hits regardless of RPC tier**, because
0039 §2 already established the public endpoint and Alchemy enforce the
same 10,000-*result* number, and QuickNode's per-method result cap for this
chain was not separately confirmed (0039 §8, still open). A quieter,
equally old token would not trip this; a loud one — exactly the graduated,
actively-traded tokens the owner most wants judged — reliably will, under
the reader's current one-shot, unbounded `eth_getLogs` call.

**What fixes it, cheaply:** page the same call by block range (binary-search
or fixed-size windows) instead of one `fromBlock:0x0` call, summing balances
across pages the same way `holders_from` already folds a single page — this
is a client-side change, not a new data source, and every page still costs
one `eth_getLogs` (60 CU / 20 credits on 0039's Alchemy/QuickNode figures).
A token that clears 10,000 Transfer logs needs roughly (total transfers /
9,999) pages; `HEY`'s ~23,000+ total daily-rate transfers (extrapolating the
6h rate) would need roughly 3-5 pages once its full history is summed, not
one.

## 1. Plain RPC on Alchemy free and QuickNode free (re-verified 2026-09-17)

Both re-fetched live today; both **unchanged from the 2026-09-15 reads in
research 0039 §2**:

- **Alchemy** (`alchemy.com/pricing`, fetched 2026-09-17): "Free 30M CU per
  month", "25 requests per second", pay-as-you-go "$0.525/1M CUs". Per-method
  costs (`alchemy.com/docs/reference/compute-unit-costs`, quoted in 0039,
  not re-fetched today — no reason to expect it moved): `eth_call` 26 CU,
  `eth_getLogs` 60 CU, `eth_getBlockByNumber`/`eth_getTransactionReceipt` 20
  CU, `eth_blockNumber` 10 CU. **Result cap**, confirmed live today against
  Robinhood Chain specifically (§0): 10,000 logs per `eth_getLogs` call,
  independent of block range — the call above spanned genesis-to-latest
  (block `0x3e84a58` = 65,338,968, read today) and failed on result count,
  not a range-too-wide error. Whether Alchemy's *free* tier additionally
  caps the block range itself (its Enterprise-tier language implies
  free/pay-as-you-go have a narrower range than Enterprise — 0039 §2) was
  not separately isolated, because the result cap fires first on any busy
  token regardless.
- **QuickNode** (`quicknode.com/pricing`, fetched 2026-09-17): Free trial
  $0/mo, 10M credits; Build $49/mo, 80M credits, 50 req/s; Accelerate
  $249/mo, 450M; Scale $499/mo, 950M; Business $999/mo, 2B. "All methods* =
  20 credits" for Ethereum-style chains (`quicknode.com/api-credits/eth`,
  read 2026-09-15 per 0039, not separately re-fetched today); Robinhood
  Chain is not named on that page, so the rate is inferred, not confirmed,
  for this chain specifically. **Result cap for QuickNode on Robinhood
  Chain specifically was not tested this session** (no QuickNode key,
  per the packet's no-sign-up boundary) — 0039 §8 already flagged this as
  open, unchanged here.
- **Trace/debug APIs and enhanced Transfers/Token-balance APIs: tested live
  today against the public RPC, all three absent.**
  ```
  trace_filter            -> {"code":-32601,"message":"the method trace_filter does not exist/is not available"}
  debug_traceBlockByNumber -> {"code":-32601,"message":"the method debug_traceBlockByNumber does not exist/is not available"}
  alchemy_getAssetTransfers -> {"code":-32601,"message":"the method alchemy_getAssetTransfers does not exist/is not available"}
  ```
  (all three tested 2026-09-17 against `rpc.mainnet.chain.robinhood.com`).
  These are method-not-found errors from the node itself, not auth
  rejections, so they say the public endpoint's RPC surface excludes them —
  they do **not** prove Alchemy's or QuickNode's *paid* endpoints lack them
  (both vendors gate `debug_traceTransaction`/`trace_` methods and
  Alchemy's Enhanced APIs behind specific plans on other chains, and
  whether either enables them for chain 4663 specifically was not found on
  either vendor's docs this session — a URL guessed at Alchemy's
  chain-specific API-endpoints page for Robinhood 404'd today). **This
  confirms research 0045 §2's flagged gap rather than closing it**: native
  ETH funding transfers (the dominant funding-source signal for bundling,
  per 0045 §3) emit no ERC-20 log at all, so finding a wallet's *first
  funding transaction* needs a trace API or an address-indexed explorer —
  neither is priced or confirmed available on this chain today, on any
  tier, keyless or not.

## 2. Blockscout (or another explorer API) for Robinhood Chain

**Still blocked keylessly, reconfirmed live today**, both a token page and
a holders page, same token as the reproduction above:
```
curl -A "Mozilla/5.0" https://robinhoodchain.blockscout.com/api/v2/tokens/0x13e6cdb0470b10afcb96177ae8702ace2ac72cd6
-> HTTP 403, Cloudflare "Just a moment..." challenge page
curl -A "Mozilla/5.0" https://robinhoodchain.blockscout.com/api/v2/tokens/0x13e6.../holders
-> HTTP 403, same challenge
```
(tested 2026-09-17, this session — identical outcome to research 0039 §3's
2026-09-15 test). Blockscout's v2 API does document exactly the endpoints
this task needs — `/api/v2/tokens/{address}/holders` (top holders),
`/api/v2/tokens/{address}/transfers` (transfer history), and
`/api/v2/addresses/{address}/internal-transactions` (a funding-source path
Blockscout indexes that plain RPC cannot see at all, since internal/
native-ETH transfers carry no log) — but none of the three could be called
keylessly this session or last. **Not established: whether an API key
changes this** (Blockscout's own docs describe a public, keyless API by
design on most deployments; a Cloudflare challenge in front of it on this
specific instance is unusual and was not explained by anything found in
Blockscout's or Robinhood's docs). This remains the single largest gap in
the whole picture: Blockscout's holders and internal-transfer endpoints, if
reachable, would answer both the holder-read failure (§0) and the
funding-source gap (§1's trace-API absence) directly and cheaply, and
neither is confirmed usable.

## 3. Indexers, subgraphs, and DEX aggregators

**No Pons v2-specific subgraph or indexer was found** (unchanged from 0039;
not re-searched this session — no new evidence to add). **Dune** has raw
and decoded Robinhood Chain tables (`docs.dune.com/data-catalog/evm/robinhood/overview`,
quoted in 0039 §4, not re-fetched today) including `robinhood.traces` and
`robinhood.traces_decoded` — this is notable against §1's gap, because Dune's
own traces table would answer the native-ETH-funding question RPC cannot,
*if* a Pons v2-aware query were written against it. Pricing (0039 §4,
2026-09-15, not re-verified today): Free tier is view-only, cannot run or
schedule queries or call the API at all; **Analyst ($65/mo, 3 seats, 4,000
credits/mo) is the cheapest tier that can run any query**, including via
API. The actual credit cost of the bundling/funding-source query this
project would need was not measured (would require an account; boundary
forbids signing up).

**DexScreener and GeckoTerminal both cover Robinhood Chain, confirmed live
today against the packet's own token, post-graduation only:**
```
GET api.dexscreener.com/latest/dex/tokens/0x13e6...
-> real pair data: HEY/WETH, dexId "uniswap", labels ["v4"], pairAddress 0xfa3092...,
   priceUsd 0.00001466, liquidity.usd 11140.94, volume.h24 1102368.45, h6 buys 5864/sells 5598

GET api.geckoterminal.com/api/v2/networks/robinhood/tokens/0x13e6...
-> launchpad_details: {"graduation_percentage":100.0,"completed":true,
   "completed_at":"2026-09-17T14:41:52.000Z",
   "migrated_destination_pool_address":"0xfa3092..."}
-> pool attributes: reserve_in_usd 11730.45, h6 buys 5856/sells 5586, h1 buys 59/sells 71
```
(both tested 2026-09-17; raw responses not saved to `docs/research/data/`
this session, per the packet's directive to write only to the research doc).
Both are **free, keyless**, and — new information this session —
**GeckoTerminal's own `launchpad_details` field states graduation status
and the exact graduation timestamp directly**, which is a cheaper source for
"curve vs graduated" than reading the curve's `graduated()` getter by RPC,
*for any token GeckoTerminal has indexed* (post-graduation only; pre-
graduation curve tokens do not appear, confirmed by 0039 §5's empty-pairs
test on an un-graduated token, unchanged today). Neither API exposes
holders, bundlers, or funding sources — both are price/liquidity/volume
only. GeckoTerminal's rate limit (429 under light testing, 0039 §5) was not
re-tested today; treat the free tier as fragile for anything beyond
occasional lookups, consistent with 0039's finding.

## 4. Per-signal: cheapest reliable read, calls/token, and what can't be read

| signal | cheapest reliable method | calls/token | what can't be read at all |
|---|---|---|---|
| **Bundling** — same-block/first-block buyers, common funding source | Same-block buyers: bounded `eth_getLogs` on the curve for `CurveBuy` in the launch block ± a few blocks (cheap, 1 call, matches 0036's already-decoded `Trade` shape). **Common funding source: not readable from RPC at all** — see below | 1 (buyers only) | **Whether those buyers share a funding wallet.** Confirmed live today (§1): `trace_filter`/`debug_traceBlockByNumber`/`alchemy_getAssetTransfers` all absent on the public RPC; a native-ETH funding transfer to a fresh wallet emits no log an `eth_getLogs` scan can see (0045 §2, "not obtainable from eth_getLogs at all"). Needs a trace API (unpriced/unconfirmed on any Robinhood-Chain vendor tier, this session), Blockscout's internal-transactions endpoint (blocked, §2), or Dune's `robinhood.traces` table (needs a $65+/mo seat, query cost unmeasured). **This is the single least-affordable signal on the owner's list today.** |
| **Wallet analysis — top holders / concentration** | Sum every `Transfer` log for the token, **paginated by block range** (fixes §0's failure) | ~1–5 `eth_getLogs` pages for a busy young token (§0); 1 for a quiet one | Off-chain identity (CEX-labeled wallets, known-entity tags) — no free source for Robinhood Chain found this or prior sessions |
| **Wallet analysis — fresh wallets (age, first tx)** | `eth_getTransactionCount` or first-appearance-by-log-scan per candidate address; cheap per address but is *N* calls for *N* holders | 1 `eth_call`-equivalent per address checked | A wallet's true "first ever" activity if its first move was a plain ETH receipt (same trace-API gap as bundling) |
| **Wallet analysis — deployer's history of earlier launches, outcomes** | Already free with the recommended own-index (§ recommendation below): filter the factory's `TokenLaunched` events by `deployer`, cross-reference graduation status per token. Confirmed workable in research 0038/0040 (factory events already decoded) | 0 marginal calls once the index exists; 1 `eth_getLogs` per address if done ad hoc against the factory | Nothing — this is the one signal fully answerable from data already flowing through the reader, given an index |
| **Wallet analysis — snipers** | Same-block/near-block buys after `TokenLaunched`, from curve `CurveBuy` logs (already decoded by `pons.rs`'s `Trade`) | 1 `eth_getLogs`, bounded to the launch block window | Whether a sniper is a bot vs. a fast human — behavioral inference only, not a hard fact |
| **Wallet analysis — who is selling** | `CurveSell` logs pre-graduation (same call as snipers, opposite side); post-graduation, Uniswap v4 pool swaps via `eth_getLogs` on the pool address once known, or DexScreener's `txns`/GeckoTerminal's `transactions` buy/sell counts (free, confirmed live today, §3) for an aggregate view without per-wallet detail | 1 (pre-grad) or 0 (post-grad aggregate, from §3's free APIs) | Per-wallet seller identity post-graduation without the pool address (0044 §2: "the actual PoolManager or pair/pool address... were not found" as of 0040/0044; not re-checked this session) |
| **Tokenomics — supply split, dev buy** | Factory's `getLaunchedToken` return (already read by `robinhood.rs`'s `launched_token`) + launch-transaction receipt for the deployer's own curve buy (already read by `launch_facts`/`launcher_buy` in the current reader) | 2 (already paid for in the current reader; no new cost) | Nothing new — this is already built and working per the source read above |
| **Tokenomics — locked/burned** | Pre-graduation: category error — Pons v2's curve holds reserves directly, no LP token exists to lock (0044 §2, confirmed by source read in 0048). Post-graduation: **not established** — whether `PonsV2LaunchLocker.sol` actually locks the graduated Uniswap v4 position was flagged unread in research 0048 §8 ("`PonsV2LaunchLocker.sol` is on disk but was not read this session"), unchanged here (out of this document's scope; a source read, not an RPC read) | n/a pre-grad; unknown post-grad | Whether the locker contract is actually wired into the graduation path, and its unlock terms — needs a source/bytecode read of `PonsV2LaunchLocker.sol`, not an RPC call |
| **Tokenomics — curve vs graduated** | Curve's own `graduated()` getter (already read by `curve_facts`), **or**, for free and zero marginal RPC cost on any token GeckoTerminal has indexed, its `launchpad_details.completed`/`completed_at` fields (confirmed live today, §3) | 1 `eth_call` (RPC) or 0 (GeckoTerminal, post-graduation only) | Nothing — this is answerable both ways today |
| **Liquidity — pool depth post-graduation** | DexScreener's/GeckoTerminal's `liquidity.usd`/`reserve_in_usd` fields, free, confirmed live today against the packet's token (§3: $11,140.94 / $11,730.45, two independent sources within 5% of each other) | 0 marginal RPC calls | Pre-graduation depth (no pool exists yet — curve reserves are the only depth, already read by `curve_facts`) |
| **Liquidity — LP locked or removable** | Same gap as tokenomics' locked/burned row above: locker wiring unconfirmed (0048 §8), and the pool/PoolManager address itself was still unfound as of 0044 §2/0040 (not re-checked this session — would need a fresh RPC probe against a graduated pool, e.g. this document's own `HEY` token, to see if that gap has closed; out of this session's scope) | unknown | Both the locker's actual effect and, unresolved, the pool address itself |
| **Wash trading — same wallets cycling buys/sells** | Bounded `eth_getLogs` on the curve (pre-grad) or pool (post-grad, once addressed) for buy+sell logs, clustered by address, cross-referenced against the bundling funding-source signal above for self-funded volume | Same call as snipers/sellers rows — no new cost for the raw event read; clustering is compute, not calls | Self-*funding* (as opposed to same-wallet cycling, which *is* visible in logs) hits the exact trace-API gap bundling does — 0043 already treats "self-funded volume" as needing the funding-source signal this document confirms is the expensive one |
| **Cross-token memory (funding wallet seen in several bundles)** | Store every funding wallet this project *does* manage to resolve (from whichever of the above eventually answers it) in the project's own database, keyed by address, and match on repeat sightings | 0 marginal RPC calls — this is a local-storage/query problem once addresses are known, not a new chain read | Bounded entirely by the bundling gap above: memory can only be as complete as the funding-source signal that feeds it, which is currently the weakest-answered signal on the list |

## 5. Recommended data setup for ~100–500 token checks/day, and monthly cost

This does not change research 0039 §7's recommendation, which already
covers the RPC-shaped read volume for this scale; it adds what this
session's live tests changed about *how* to use it:

- **Keep QuickNode Build ($49/month flat, 80M credits)** as the paid RPC
  (0039 §7's reasoning: named archive support for this chain, flat price
  regardless of launch-volume growth, headroom to 3x today's volume). At
  100–500 checks/day this project is nowhere near QuickNode's credit
  ceiling — 0039's own volume case was ~1.2M–3.2M calls/month system-wide,
  of which per-check reads (this signal set) are a small slice.
  **Alchemy pay-as-you-go remains the cheaper fallback** (~$2–$18/month at
  this scale, per 0039 §2's math, re-verified live today), chosen against in
  0039 specifically for the archive-guarantee and flat-price reasons, not
  cost.
- **Fix the holder read to page `eth_getLogs` by block range** (§0) —
  free, no new vendor, the single highest-value code change this document's
  live test surfaces. Without it, every busy graduated token (exactly the
  ones worth judging) will keep failing "holders could not be read."
- **Do not add Dune or Blockscout to the budget yet.** Dune's cheapest
  usable tier is $65/month and its actual per-query cost for this project's
  bundling/funding-source query is unmeasured (0039 §4, unchanged); Blockscout
  is blocked keylessly and its behavior with a key is untested by anyone
  this session or last (§2). Both remain the standing candidates for closing
  the bundling gap (§4's worst row) — recommend Josh trial Blockscout with a
  free API key first (lowest cost to find out if the Cloudflare block lifts
  with one) before paying for Dune.
- **Use DexScreener + GeckoTerminal, free and keyless, for every
  post-graduation liquidity/price/volume read** (§3, §4) — this removes
  those reads from the RPC budget entirely for the roughly 1-in-80 tokens
  research 0036 measured graduating (1.2%), or more precisely for whichever
  fraction of the 100–500/day sample has graduated by check time.
- **Total new monthly cost: $49 (QuickNode Build), same as 0039's
  recommendation** — this session found no new source that changes that
  number at 100–500 checks/day. The unresolved cost is **not** a monthly
  fee but a **capability gap**: bundling's funding-source question has no
  confirmed affordable answer today at any price this document tested.

## 6. Not verified this session (carried forward or new)

- Whether Alchemy's or QuickNode's *paid* tiers expose `debug_traceTransaction`/
  `trace_`-class methods or Alchemy's Enhanced Transfers API for chain 4663
  specifically — only the public RPC's absence of all three was tested live
  today; no paid key was used (boundary).
- Whether Blockscout's Cloudflare challenge lifts with an API key — not
  testable without a key (boundary); this is the single most consequential
  open question in this document, since it would settle both §0's and §4's
  worst gaps at once if it works.
- QuickNode's actual `eth_getLogs` result-cap number for Robinhood Chain
  (only the public RPC and Alchemy's 10,000 were tested live; carried
  forward from 0039 §8, unchanged).
- Whether the graduated Uniswap v4 pool/PoolManager address and
  `PonsV2LaunchLocker.sol`'s actual wiring have been found since research
  0044/0048 (2026-09-15) — not re-checked this session; a fresh probe
  against `HEY`'s pool (`0xfa309242187ec19ffa6a467bdfea620f3fe626e0f7ff43e1e356b1004c1defaf`,
  read live today from DexScreener/GeckoTerminal, §3) could settle this
  cheaply and is the natural next step, out of scope here.
- Dune's actual credit cost for the bundling/funding-source query this
  project needs, and its freshness lag behind chain tip — both still open
  from 0039 §4/§8, not re-tested (would need a paid seat).

## Confidence and what would change it

**§0 (holder-read failure cause) and §1's three method-absence tests are
PROVED** — reproduced live today with raw request/response pairs quoted
above, against the exact token and RPC endpoint the packet named. **§1's
pricing figures and §3's DexScreener/GeckoTerminal coverage are CHECKED** —
re-fetched live today, matching the 2026-09-15 reads in research 0039
byte-for-byte on every price. **§2 (Blockscout) is REFUTED as a keyless
option today**, same as two sessions running, and **CONDITIONAL** on an API
key untested by anyone yet. **§4's tokenomics/curve-vs-graduated rows are
PROVED** (already-working reader code, confirmed by source read in 0048).
**§4's bundling and LP-lock rows are GAP** — no source, paid or free, was
found this session that answers them; a trace-API price quote from any
vendor for chain 4663, or a successful Blockscout key test, would close the
bundling gap; a read of `PonsV2LaunchLocker.sol` would close the LP-lock
gap. A second search of the same vendor pricing pages could not change §1;
that boundary was reached. Blockscout with a key, and `PonsV2LaunchLocker.sol`'s
source, are the two searches that could still change this document's answer.

## §7 Follow-up: closing finding #6, the funding-source gap (2026-09-17)

Requested by the coordinator as the deciding gap. Five items, answered in
order below, all tested/read today against the packet's token
`0x13e6cdb0470b10afcb96177ae8702ace2ac72cd6` ("HEY") where a live read applies.

### 1. Paid-tier trace/debug/Transfers-API coverage for chain 4663

**Alchemy — NO for the funding-source read, contradicting its own marketing
copy.** Alchemy's chain-specific method table,
[`alchemy.com/docs/robinhood-chain/robinhood-chain-api-overview`](https://www.alchemy.com/docs/robinhood-chain/robinhood-chain-api-overview)
(fetched 2026-09-17), titled "Robinhood Chain API Endpoints", enumerates
**52 methods this chain actually exposes** — the standard `eth_*`/`net_*`/
`web3_*`/`txpool_content` set, plus exactly two `debug_*` methods:
`debug_executionWitness` and `debug_executionWitnessByHash` (a Reth-style
state-witness API for stateless-client proofs, not a transaction tracer).
**`debug_traceTransaction`, `debug_traceBlockByNumber`, and every
`trace_*` method are absent from this table.** The same page also carries a
generic "Related APIs" bullet list, unscoped to any chain, claiming "The
following Alchemy APIs are also supported on Robinhood Chain: ... Debug
API ... Transfers API ..." — but each bullet links to Alchemy's *generic*
product-quickstart page, not a Robinhood-Chain-specific endpoint page, and
none of those methods appear in the chain's own enumerated table above it on
the same page. **PROVED (self-contradiction in a primary source, dated
today): Alchemy's own definitive per-chain method list for Robinhood Chain
does not include a transaction tracer or `alchemy_getAssetTransfers`,
regardless of what the marketing bullets on the same page claim.** This
was checked without a key — the table is public documentation, not a
metered response, so no sign-up was needed to read it.

**QuickNode — inferred yes for tracing, not independently key-tested.**
[`quicknode.com/chains/robinhood`](https://www.quicknode.com/chains/robinhood)
(fetched 2026-09-17, re-verified today to rule out generic-template copy —
the raw HTML names "Robinhood" 14 times and chain ID "4663" 5 times in
chain-specific FAQ/JSON-LD blocks, not boilerplate) states verbatim:
"Yes. Quicknode runs full archive nodes for Robinhood Chain mainnet and
testnet with no pruning, plus the Debug API for `debug_traceTransaction`
and `debug_traceBlockByNumber`. You can query historical state and trace
transactions for indexing, simulation, and analytics," and separately:
"Run `debug_traceTransaction` and `trace_block` against Robinhood Chain
archive nodes to replay internal calls, decode state changes, and power
indexing, simulation, and trading forensics." No `trace_filter` and no
address-indexed transfer-history method (no `alchemy_getAssetTransfers`
equivalent) appears anywhere on the page — QuickNode's Robinhood offering,
as documented, is trace-by-transaction/trace-by-block, not
trace-by-address. The page does not say which paid tier (Build $49/mo vs.
higher) gates trace/debug access, only that "paid plans... scale with...
features like archive access" — **this tier-gating detail is UNVERIFIED,
not found on this page**. **Marked INFERRED, not verified**: this is a
directly-quoted, dated, chain-specific vendor claim, but was not exercised
with a real key, per the packet's no-sign-up boundary.

**Net for item 1: QuickNode's own documentation is the more useful lead —
it claims per-transaction and per-block tracing is real on this chain;
Alchemy's own table shows it is not.** Neither vendor documents an
address-indexed "all transfers/fundings for wallet X" call for Robinhood
Chain (Alchemy's Transfers API bullet is unsupported by its own method
table; QuickNode names no Transfers/Blockbook address-history feature for
this chain specifically). So even the paid, trace-capable path
(QuickNode) answers "who funded this specific wallet's specific
transaction" only if you already know which transaction to trace — it
does **not** give a reverse index ("show me every inbound transfer to
address X"). That distinction matters for item 2 below.

### 2. Best bundling proxy from data that is actually readable

Given item 1, a funding-source read (native-ETH transfer graph by address)
is not available keylessly, and even QuickNode's paid trace API is
transaction-scoped, not address-scoped — so building a funding graph would
mean tracing every transaction in a launch window, an O(blocks) sized
job, not an O(wallets) one. Six candidates, all built from data this bot
can already read (`eth_getLogs` on `Transfer`/`Swap` events, standard
`eth_get*` reads) or could read cheaply:

| Proxy | Cost | Evidence strength | Innocent twin |
|---|---|---|---|
| Buys in launch block or first N blocks | 1 `eth_getLogs` call already made for holders/snipers | Weak alone — a launch tweet reaching real buyers in the same minute looks identical | Any well-marketed launch draws organic buyers in block 1 |
| Near-identical buy sizes | Free (derived from the same `Transfer` logs, no extra call) | Moderate — same-amount buys across unrelated wallets are unusual but not rare (round-number presets: 0.1/0.5/1 ETH UI buttons) | A UI with quick-buy buttons produces identical amounts from strangers |
| Sequential nonces across the buyer set | 1 `eth_getTransactionCount` per wallet **at the launch block** (needs the address list already in hand) — an archive read, see item 5 | Strong — sequential nonces across *different* addresses only happens if one funder deployed/funded them in a batch immediately beforehand | None found — this is the hardest pattern to produce by accident; the one candidate worth spending calls on |
| Wallet's first-ever tx is this buy (`eth_getTransactionCount == 1` at launch block) | 1 `eth_getTransactionCount` per wallet at the launch block (same archive read as above, can reuse) | Moderate-strong — fresh wallets buying immediately at launch is a known bundle signature, but also the normal shape of a brand-new user's very first trade | A new crypto user's first-ever transaction is legitimately often a token buy |
| Gas-price/timing clustering | Free (from tx receipts/blocks already fetched for other reads) | Weak — Arbitrum-family chains (Robinhood is Nitro-based) have chain-set gas prices most of the time, so "same gas price" is close to universal, not diagnostic | Nearly everyone pays the same gas price on this chain by default |
| Later coordinated selling (same wallets selling in a tight window) | 1 more `eth_getLogs` on `Transfer` post-launch, already budgeted for the "sellers" signal | Moderate — coordinated exit is real bundle behavior, but a shared trending/alert bot pinging many independent holders at once produces the same shape | A trending-token alert firing to thousands of independent watchers causes synchronized, uncoordinated selling |

**Recommended primary proxy: sequential nonces at the launch block among
the launch-block buyer set**, cross-checked against near-identical buy
sizes (free) and later coordinated selling (already-budgeted call) as
corroborating, not primary, evidence. **Cost: 1 `eth_getLogs` (already
paid for holders) + 1 `eth_getTransactionCount` per distinct buyer address
at the launch block** — for a launch with ~20 buyers in the first block,
that is 20 extra calls, all against a block that is only readable if it
falls inside the archive window (see item 5). **The bot must never accuse**
per AGENTS.md rule 4: even sequential nonces are consistent with one person
running a personal batch of wallets for reasons that are not bundling
(testing, a market maker's own inventory wallets, a CEX's hot-wallet
rotation) — this proxy can only ever raise a *pattern* into the fact
sheet ("N of the top M holders bought in the launch block with sequential
nonces"), never a verdict word.

### 3. Uniswap v4 pool/hook address for the graduated HEY token

**Found and confirmed live — closing the "address still unfound" gap in
0044/0048.** Derived independently from Pons v2's own `PoolKey` fields
(currency0 = native ETH `0x0`, currency1 = the token, fee = `0`,
tickSpacing = `200`, hooks = the deployed `PonsV2MemeHook`
`0xe5e702641ea86f4ae6cc3cdaed2b886f976be044`, address confirmed by exact
Sourcify bytecode match in the 2026-09-16 verification run) via
`keccak256(abi.encode(PoolKey))` computed with a from-scratch,
validated-against-known-selectors Keccak-256 implementation (no network
dependency, no third-party library). Result:
`0xfa309242187ec19ffa6a467bdfea620f3fe626e0f7ff43e1e356b1004c1defaf`.
**This exactly matches DexScreener's/GeckoTerminal's own reported pool
identifier for this token's graduated pool**, byte for byte. Live-verified
today by filtering the Uniswap v4 singleton `PoolManager`
(`0x8366a39cc670b4001a1121b8f6a443a643e40951`) for `Swap` events with this
PoolId as the indexed topic over a recent ~3,000-block window on the
public RPC — real swap logs returned. Caveat for future per-wallet-seller
work: the `Swap` event's `sender` field is the router/hook contract that
called `PoolManager`, not necessarily the end-trader's EOA — attributing a
swap to a human wallet needs the calling transaction's `from`, not the
event's `sender`. **PROVED.**

### 4. LP-lock question, from `PonsV2LaunchLocker.sol` and a live read

**Yes, permanently locked; no one can withdraw it, including the
contract's own owner.** Read in full at
`.orchestrator/runs/20260915-robinhood-7b/sources/PonsV2LaunchLocker.sol`
(deployed at `0x267444d099b10fb5ed7c3cc7b7c767adca574952`, confirmed exact
bytecode match in the 2026-09-16 verification run). The contract's own doc
comment states its intent directly: "Permanently holds the graduated
Uniswap V4 position NFT for every pons v2 launch... This contract exposes
no withdrawal or arbitrary-call function, so locked liquidity can never be
removed by an administrator." Two structural facts back that claim up:
(1) there is no `withdraw`/`transferPosition`/`collectFees`-shaped
function anywhere in the contract — `lockPosition` and `lockTokenSupply`
are one-way, `onlyFactory`-gated, and nothing moves an NFT back out; (2)
`renounceOwnership()` is overridden to unconditionally `revert
OwnershipCannotBeRenounced()`, which sounds like the opposite of a
safety guarantee but isn't one either way here, because ownership itself
carries no power over a locked position — it only gates the one-time
`setFactory` wiring call. **Live-proved against the packet's own token**:
called `ownerOf(2843787)` (tokenId read from the locker's own
`lockedPositions(HEY)` mapping) on the Uniswap v4 `PositionManager`
(`0x58daec3116aae6d93017baaea7749052e8a04fa7`) and got back
`0x267444d099b10fb5ed7c3cc7b7c767adca574952` — the locker's own address,
exactly. Cross-checked `isLocked(HEY)` on the locker itself, which
returned `true`. **PROVED**, both from source and from a live on-chain
read, cost 3 `eth_call`s total (`lockedPositions`, `isLocked`, `ownerOf`).

### 5. Archive depth, live-tested

**Public RPC keeps a much shorter window than research 0038/0039's prior
estimate of "~100k blocks" — re-tested today and found narrower.** Tested
`eth_getBalance` at decreasing offsets from the chain head
(`0x3e87257` at test time) on the public keyless RPC:

| Blocks back | Result |
|---|---|
| 1,000 | OK |
| 2,000–6,000 | OK |
| 8,000 | `{"code":-32000,"message":"historical state ... is not available"}` |
| 10,000 and beyond (tested to 500,000) | same error |

**The pruning boundary sits between 6,000 and 8,000 blocks back on the
public RPC** — not the ~100k previously recorded. At this chain's ~100ms
block time (QuickNode's own FAQ figure, quoted in §1 above), 6,000–8,000
blocks is roughly **10–13 minutes of retained history**, not the hours the
prior estimate implied. This is a **REFUTATION of the earlier ~100k-block
estimate** with a fresh, dated, live measurement; either the chain's
pruning policy changed between 2026-09-15 and 2026-09-17, or the prior
figure was measured against a different node/method and was wrong — this
document cannot tell which, only that today's number is 6,000–8,000, not
100,000. **Consequence for item 2's nonce-based proxy: it is only usable
in the same short window a bot is already watching a launch in real
time** — checking nonces at the launch block *minutes* after graduation is
fine; checking them for a token discovered hours or days later, against
the free public RPC, will hit this wall and fail exactly the way the
packet's original holder-read failure did. QuickNode's dated marketing
claim ("Full historical Robinhood Chain state with no pruning... from
genesis") is the only lead that would lift this limit, and it is
untested here per the no-sign-up boundary — **item 5 is CHECKED for the
free tier (live, reproducible, quoted above) and INFERRED (vendor claim
only) for the paid tier.**

### Coordinator's requested format

1. Alchemy trace/Transfers coverage for chain 4663: **No** — its own
   per-chain method table excludes every trace/debug-tracer method and
   `alchemy_getAssetTransfers`, contradicting its generic marketing
   bullets on the same page. QuickNode debug/trace coverage: **inferred
   yes** (dated, chain-specific vendor quote; not key-tested).
2. Best bundle proxy: **sequential nonces among launch-block buyers**,
   corroborated by near-identical buy sizes and later coordinated
   selling. Cost: **1 `eth_getLogs` + 1 `eth_getTransactionCount` per
   distinct buyer address at the launch block** (≈20 extra calls for a
   ~20-buyer launch block) — never used to accuse, only to surface a
   pattern in the fact sheet.
3. Uniswap v4 pool/hook address: **Found and confirmed live** —
   PoolId `0xfa309242187ec19ffa6a467bdfea620f3fe626e0f7ff43e1e356b1004c1defaf`,
   matches DexScreener/GeckoTerminal exactly, real `Swap` logs pulled
   from the `PoolManager` today.
4. LP lock: **Yes, permanent, no withdrawal function exists for anyone**
   — proved from source and from a live `ownerOf()` read matching the
   locker's own address for the packet's token.
5. Archive depth: **Free public RPC: ~6,000–8,000 blocks (~10–13
   minutes), live-tested today, narrower than the prior ~100k-block
   estimate.** Paid (QuickNode): claimed unlimited/no-pruning, not
   independently tested.

**Do paid tiers close the funding-source gap? Partially, and only for
QuickNode, and only in a weaker form than "read who funded this wallet."**
QuickNode's documented Debug API (`debug_traceTransaction`,
`debug_traceBlockByNumber`, `trace_block`) would let a paid bot replay a
specific transaction or block to see internal ETH transfers within it —
but neither vendor documents an address-indexed reverse lookup ("all
transfers into wallet X, ever") for this chain, so finding *which*
transaction funded a wallet still means scanning blocks, not querying an
address. The practical, affordable path to a bundling signal remains the
proxy in item 2 (nonces/amounts/timing on data already read for other
signals), not a genuine funding-source trace — that is unchanged from
finding #6 in the original document, now narrowed to: paid tracing exists
on this chain (QuickNode, inferred) but nothing turns it into a
funding-source *index*, so the proxy is still the cheapest and only
currently-affordable answer.

## §8 Correction: what the owner's own Alchemy free key actually does (2026-09-17, live)

**Sections 1, 4, §7.1 and §7.5 above are wrong about capability, and the
"funding-source gap" is closed.** Those sections were written from vendor
documentation and from the *keyless public* endpoint. Everything below was
measured against the key the bot already runs on, from the production box, on
2026-09-17. `AGENTS.md` §1 puts a live capture above a vendor's own page, so
where the two disagree, this section is the record.

Probe scripts: `scratchpad/probe_rpc.sh` through `probe5.sh` (session-local,
not committed; each reads the endpoint out of the serve env file and never
prints it).

### What works, measured

| Read | Result on the key |
|---|---|
| `alchemy_getAssetTransfers`, category `external` (native ETH) | **works**, `fromAddress` and `toAddress`, `order: "asc"`, `withMetadata: true`, `pageKey` paging |
| `alchemy_getAssetTransfers`, category `erc20` | **works**, both directions, `contractAddresses` filter honoured |
| `alchemy_getAssetTransfers`, category `internal` | not supported on this chain |
| `debug_traceBlockByNumber`, `callTracer` | **works** — one recent block: ~215 ms, ~668 KB of JSON |
| `trace_block`, `trace_filter`, any `qn_*` | not available |
| `eth_getBalance` / `eth_getTransactionCount` at old blocks | **works** far past the public RPC's window (see below) |
| `eth_getLogs` | works; capped at **10,000 results per call**, not by block range |

### The funding-source gap is not a gap

§7's conclusion — "neither vendor documents an address-indexed reverse lookup
('all transfers into wallet X, ever') for this chain" — is refuted.
`alchemy_getAssetTransfers` **is** that index, and it answers on this chain on
the free key. One call returns, oldest-first, every native-ETH transfer into a
given address with block number, sender, amount and timestamp.

Live example, from the packet's own token
(`0x13e6cdb0470b10afcb96177ae8702ace2ac72cd6`): pulled its `Transfer` logs over
the last 3,000 blocks, kept only addresses whose `eth_getCode` is `"0x"` (real
wallets — the earlier probes sampled contracts, which is why their transfer
lists looked empty), then asked for each one's first five incoming native
transfers. One buyer, `0x2a4acb164a2bcfd963f92f798b59db137cabae53`, nonce
`0xb3`, was funded like this:

```
0xe0050f2a8aa898da5829c5f1a270315062c56d30   0.01594850357357177
0x623198efeb5da687f71653f889798f22a0bae02b   0.005
0x623198efeb5da687f71653f889798f22a0bae02b   0.01201634222542658
0x623198efeb5da687f71653f889798f22a0bae02b   0.01
0x623198efeb5da687f71653f889798f22a0bae02b   0.004
```

Four of its first five top-ups came from one address. That is the shared-funder
signal itself, read directly, at **2 calls per candidate wallet**
(`eth_getCode`, then `alchemy_getAssetTransfers`) on top of the one
`eth_getLogs` that produced the candidates. The nonce proxy in §7.2 is still
useful as a cheap corroborator, but it is no longer the only affordable answer.

### Archive depth: deep on the key, shallow only without one

§7.5's "6,000–8,000 blocks (~10–13 minutes)" is a fact about the **keyless
public** endpoint and must not be read as a limit on the bot. On the key,
`eth_getBalance` for a busy address answered at every depth tested, from head
`65566989`:

| Blocks back | Balance returned |
|---|---|
| 1,000 | `0x12d8e19dbb3ba171` |
| 10,000 | `0x12e816a8aeb125c1` |
| 100,000 | `0x14d56fb718c2f070` |
| 1,000,000 | `0xc33f7574b5e26fe` |
| 10,000,000 | `0x0` |
| 50,000,000 | `0x0` |

The two `0x0` answers are **not** pruning errors — no error was returned, and
`eth_getBlockByNumber` at 10,000,000 blocks back still served a full header.
That address simply held nothing then. Historical state is available at least
1,000,000 blocks back and the node answers, rather than refuses, far beyond it.

### Block time, measured

1,000 blocks spanned **101 seconds** — 0.101 s per block. So one hour is ~35,600
blocks and a three-hour-old token sits ~107,000 blocks deep. Every "how many
blocks back" figure elsewhere in this document should be converted with this
number, not with an assumed 1-second or 2-second block.

### What this changes

- The bundle, fresh-wallet-cluster and shared-funder checks are affordable on
  the free tier today. No paid plan is required to ship them.
- `debug_traceBlockByNumber` is available but expensive per block (~668 KB);
  treat it as a last-resort read for a single block of interest, never a sweep.
- §7's recommendation to buy a QuickNode plan for tracing is withdrawn. Revisit
  cost only if measured daily request counts approach the free allowance.
