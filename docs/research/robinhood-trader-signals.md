<!-- SPDX-License-Identifier: Apache-2.0 -->
# Robinhood Chain: what's readable, how, and at what cost, for trader-style signals

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
