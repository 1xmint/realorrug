<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0040 — Pons v2 graduation, price and other launchpads

**Date:** 2026-09-15
**Status:** captured for questions 1–4; question 5 partial. Every number below
is read live from Robinhood Chain mainnet (chain id 4663) through the public
RPC `https://rpc.mainnet.chain.robinhood.com`, between about 22:10 and 23:10
UTC, 2026-09-15, with a Node script kept under this session's scratchpad
(retry/backoff per research 0038 §4: six tries, 4 s pause on a 429 or a
"timed out" error). No transaction was sent. Raw JSON-RPC responses are under
`docs/research/data/0040-*.json` (the three largest are summarised or left out, as noted where cited). This closes research 0038 §7's
open items on the graduation event, the phase word's non-zero value, the
curve's spot-price getter and the holder-sum check; it reuses 0038's own
"Bankr" and "Noxa" sourcing (research 0035 §3, trade press and docs, not
re-verified here) for question 5 and adds one on-chain finding research 0039
had already captured but not named.

## 1. Graduated phase values

**Observed, live, 2026-09-15 (block 63,989,790).** Three tokens already
known to have graduated (research 0036 §6's creator-tax survey,
`0036-creator-tax-survey.json`, read 2026-09-14 and reading `phase_now: 2`
at that time) were re-read today with a fresh `getLaunchedToken` call:

| token | curve | `phase` (word 10) |
|---|---|---|
| `0x63ae1e0f1a756ec4947f3f06efb09c588984ed88` | `0x008089e243a611ace236fc4e2127403a3c9e347b` | **2** |
| `0xe9d3fe8f4e2b29eebecd6bd76d49e0684b598b7d` | `0x305235e2a5b8372cc6e228f0ce17796ec64e195e` | **2** |
| `0x74b3d6520317cd12a42d31525b5de72980507332` | `0x3589b68a3377135db7b91aa44e90bcbdb7d53abe` | **2** |
| `0x22fd486d80b7cce7362ffed59bbf2fd266a148fa` (research 0036's captured launch, not graduated) | `0xddf3afb29e265b00c48015c3aacdedcb10088fcf` | **0** |

Capture: `docs/research/data/0040-phase-check.json` (raw `eth_call` return bytes
for all four, plus the block number the read was pinned at). **So `phase`
takes at least the values 0 (not graduated) and 2 (graduated), confirmed on
three independent real tokens.** What `phase == 1` means — a graduating
transition state, or a value never reached — was not observed: no token in
this session's calls returned `1`, and the graduation transaction itself
(§2) shows the factory's own record jump directly from the pre-graduation
state to `2` in one transaction, with no separate transaction setting it to
`1` first. **Evidence label: observed** (0 and 2, on named tokens, with
tx/block); **not established** whether 1 is ever used.

## 2. The graduation event

**Observed, one real transaction, decoded field by field.** The **factory**
itself — not the curve — emits the graduation log: topic0
`0xcdb72f157fd3666758a6ce201387ffb52038c7562e4fff352828da1096c4b6b4`, from
`0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e` (the Pons v2 factory), one
indexed field (the token) and two data words. Captured example:

- **Transaction:** `0x7352d0f7e3a0e9ce43a810aba7eaf9519da370ebc83d15d1d58cea0eb75e16bf`
- **Block:** 62,263,572, timestamp `2026-09-13T21:07:35Z`
- **Token:** `0x63ae1e0f1a756ec4947f3f06efb09c588984ed88`
- **Data word 1:** `4200000000000000263` wei — **4.2 ETH plus 263 wei**,
  matching `getLaunchConfig(0)`'s graduation threshold exactly (research
  0036 §1: "graduation at 4.2 ETH"; today's `getLaunchedToken` on the same
  token reads `graduationThreshold = 4200000000000000000` wei, i.e. exactly
  4.2 ETH — the event's word is the *quote actually raised* when the
  threshold was crossed, a few hundred wei over the round number).
- **Data word 2:** `285714285714285714285714285` (raw units) — **the exact
  amount of the token's own `Transfer` from the curve to the factory in the
  same transaction** (log `0x63ae1e0f…`'s `Transfer(curve, factory, …)`
  carries the identical value). Full receipt: `docs/research/data/0040-graduation-tx-receipt.json`.

**Inference (name only, not from source):** this reads as
`Graduated(address indexed token, uint256 quoteRaised, uint256 tokensToFactory)`
or similarly named — the signature string was not recovered (no verified
source for the deployed factory; research 0036 already found the deployed
contracts diverge from the published source), only that it is emitted
exactly once per graduating token, by the factory, with these two fields.

**The same transaction also shows, decoded but not fully interpreted (kept
here as observed mechanics, not overclaimed):** the curve moved its whole
remaining token balance out in two transfers — 285,714,285.71… tokens to the
factory (matching the event above) and 5,622,402.01… tokens to a small
contract (`0x9689992f5b5c09447f15906d8d11214944488341`, 100 bytes of code,
an EIP-1967 **beacon proxy** — its storage at the beacon slot
`0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bb`
identifies the pattern) which forwarded the same amount on to
`0xe4c6b76911f1eab045dfb4e68fa6861e942da4db`, an address that holds **no
code** (`eth_getCode` returns `0x`) — so it is a plain EOA or an
undeployed address, not a pool contract. A `FeesSwept` fired first (a final
sweep before graduation), and two more logs came from the beacon proxy and
from a contract at `0x6fb4460e4bebf662fcd9bfa5ce6d6231732bb86c` (2,060 bytes
of code) whose event signatures were not decoded. **What exactly these two
token flows represent (protocol treasury vs. pool seeding vs. a
creator/team allocation) is not established this session** — flagged rather
than guessed, since AGENTS.md §3 rule 2 forbids introducing a fact not
checked.

**Finding all graduations with `eth_getLogs`, under the 10,000-result cap.**
Because the event comes from the **factory address alone** (not per-curve),
one call suffices today: `eth_getLogs({address: FACTORY, topics:
[0xcdb72f15…], fromBlock: "0x1", toBlock: "latest"})` returned **8,262**
matching logs in a single request, well under the cap (`docs/research/data/0040-graduations-summary.json`: count, first, last and the example; the full response was not kept).
Both other known-graduated tokens from research 0036 §6 (`0xe9d3fe8f…`,
`0x74b3d652…`) are present in this set, confirming the topic and address are
right. **This is a materially larger, and cheaper, way to enumerate
graduations than the per-curve scan 0038 assumed would be needed** — it
needs no address list of curves at all, only the factory's own logs, the
same shape as the `TokenLaunched` count in 0038 §1. Against 0038's
extrapolated ≈171,000 total launches, 8,262 graduations is **≈4.8% of
launches**, a real live figure (not extrapolated, since the query ran to
completion), though the launch denominator underneath it is 0038's own
extrapolation, not a fresh count this session. **Checked on grading:** the
same whole-chain query timed out (`log query timed out`) when repeated, and a
65,536-block window (0x3b60000 to 0x3b70000) returned 22 graduations,
including the example above at block 62,263,572 with 4.2 ETH raised. At that
pace, about 280 a day, the count passes 10,000 within a week, so an indexer
must page by block range from the start. The same
recursive split-on-cap approach 0038 §1 used for `TokenLaunched` applies
unchanged, since it is the identical query shape against the identical
factory address.

**Evidence label: observed** (topic, address, decoded fields, real tx/block,
and a full-chain enumeration that ran to completion).

## 3. A spot-price read

**Observed: the curve answers `getReserves()` — the same four-byte selector
as a Uniswap V2 pair, `0x0902f1ac` — and it is not a coincidence of hash
collision: it returns two `uint256` words that separately match two other
selectors that also succeed, `quoteReserve()` (`0x9da771f4`) and
`tokenReserve()` (`0xcbcb3171`).** Selectors were computed with a from-scratch
Keccak-256 implementation (verified first against `balanceOf(address)` →
`0x70a08231`, `transfer(address,uint256)` → `0xa9059cbb`,
`totalSupply()` → `0x18160ddd`, `getReserves()` → `0x0902f1ac`, all matching
the well-known values), then tried by `eth_call` against 0036's captured
curve `0xddf3afb29e265b00c48015c3aacdedcb10088fcf` alongside ~20 other
guessed names (`price()`, `spotPrice()`, `getPrice()`, `phase()`,
`virtualReserveQuote()`, etc.), all of which reverted. Full probe:
`docs/research/data/0040-curve-getter-probe.json`.

- On the not-yet-graduated curve above: `quoteReserve = 1.687061302752159 ETH`,
  `tokenReserve = 995,814,436.18 tokens` (of the 1,000,000,000 minted) — so
  `price ≈ quoteReserve / tokenReserve ≈ 1.694 × 10⁻⁹ ETH/token`, a genuine
  point read, not the last-trade approximation 0038 §5 fell back to.
- On the graduated curve `0x008089e243a611ace236fc4e2127403a3c9e347b`
  (§2): `quoteReserve = 1.68 ETH exactly`, `tokenReserve = 0`. **1.68 ETH is
  exactly research 0036 §1's "phantom reserve"** for launch config 0 — so
  `quoteReserve` reads as *virtual reserve including the phantom baseline*,
  and after graduation the curve is left holding only that phantom floor
  with no tokens, confirming the getter keeps answering (not reverting)
  after graduation rather than becoming unreadable.

**So there is a real, working view getter for the curve's own reserves,
before and after graduation, at selector `0x0902f1ac`** — this settles
0038's and 0039's "no reserves/spot-price getter has been read" gap.
AGENTS.md §3 rule 5 still means the bot never states this number publicly;
it is useful only for an internal outcome check (e.g., "did the reserve
move" or "how close to the 4.2 ETH threshold").

**After graduation, which pool, and how to read it: documented and partly
inferred, not settled by a decoded pool read this session.** Corroborating
evidence gathered here, on top of research 0036 §5's source-only finding
("the meme hook... published source"): `getLaunchedToken`'s `poolFee` word
reads **0** and `tickSpacing` reads **200** on all four tokens checked in
§1 — a fee/tick-spacing pair is a Uniswap V3/V4-shaped parameter (Uniswap
V2 pairs carry neither), and a `poolFee` of 0 with a hook present is
consistent with Uniswap V4's dynamic-fee flag (the fee is not fixed in the
pool key when a hook sets it). The graduation transaction's beacon-proxy
contract (§2) is consistent with a per-token pool/hook instance being spun
up at graduation, cloned from a single beacon per research 0036 §5's
"meme hook" naming. **What was not found this session:** the actual
Uniswap PoolManager or pair/pool contract address for a graduated token,
a `slot0`/tick or `getReserves`-equivalent read against it, or the exact
pool-key derivation. **Evidence label: reserves getter — observed. Pool
version and address — inferred from field shapes and the published-source
paragraph in 0036, not observed directly; flagged, not guessed as fact.**

## 4. Holders from `Transfer` logs

**Observed and matched to the wei, on a real token, current block.** Token
`0x63ae1e0f1a756ec4947f3f06efb09c588984ed88` (the graduated example above)
emitted **8,614** ERC-20 `Transfer` logs across its whole life (topic
`0xddf252ad…`, one `eth_getLogs` call, under the cap;
its 5.4 MB log capture was not kept; the result is in `docs/research/data/0040-holder-check2.json`). Summing every log
(`balance[to] += value; balance[from] -= value`, skipping the zero-address
mint source) gives **95 distinct addresses with a positive balance** at the
read block. The five largest were each checked against a live `balanceOf`
call at the same block (`latest`, block 63,989,790 or the block immediately
after depending on call timing) and **matched exactly**, to the last wei
(`docs/research/data/0040-holder-check2.json`):

| holder | summed from logs | `balanceOf` |
|---|---|---|
| `0x8366a39cc670b4001a1121b8f6a443a643e40951` | 861,769,343,136,162,414,596,698,110 | same |
| `0x267444d099b10fb5ed7c3cc7b7c767adca574952` | 81,632,653,061,224,486,144,665,279 | same |
| `0x8a33145836fba5c9f1f96336e1876d839125510e` | 27,620,589,322,659,474,739,215,888 | same |
| `0x6ac8a857b7bf7cd0663dcb3552140ea27b5c8eab` | 9,426,074,495,229,796,271,479,750 | same |
| `0x33ba61b4530f36befc324b211eacaa877dbe7801` | 5,152,420,557,245,758,230,362,064 | same |

A second, smaller token (`0x8d55168977bf2f28eb20b3a61d6fc277084ff8d6`, 71
`Transfer` logs) summed to **zero external holders** — every buyer had
fully sold back to the curve by the read block, leaving the curve as the
only nonzero balance, itself confirmed against `balanceOf`
(`docs/research/data/0040-holder-check.json`). **This is a second, independent
confirmation of the same method on a token with a different shape (all
churn, no standing holders), not a failure of the method.**

**So Pons v2 tokens are ordinary ERC-20s: summing `Transfer` logs gives
every balance exactly, both pre- and post-graduation, no separate holders
API needed.** State pruning (research 0038 §4) does not bite here since the
check reads `balanceOf` at the *current* block only, never an old one.
**Evidence label: observed**, two tokens, balances matched to the wei on
five sampled holders plus a zero-holder case.

## 5. Bankr and other launchpads on this chain

**Not chased with the same on-chain rigor as §§1–4, given this document's
budget; what follows layers one new on-chain observation onto research
0035's press/docs sourcing, which is not re-verified here.**

- **Bankr:** research 0035 §3 (documented, first-party docs, 2026-09-13) —
  "Bankr (Doppler, Uniswap v4) — 0.665% of volume... your token and WETH" —
  is unchanged and not re-checked this session; **no Bankr factory or
  Doppler-deployer address was found or searched for on-chain**, so whether
  it shares any contract with Pons v2 (it should not, per the docs
  describing a wholly different fee mechanic) is inferred from the
  documentation, not observed from a deployed contract.
- **Noxa:** research 0035 §3 — "stopped on 2026-07-13" per trade press —
  unchanged, not re-checked.
- **A third launchpad found in this session's data, previously uncatalogued
  by name: "flapsh".** Re-examining `docs/research/data/0039-dexscreener-search.json`
  (already captured 2026-09-15, this session only re-read it, no new
  network call) shows a real pair with `"dexId":"flapsh"` on `"chainId":"robinhood"`:
  pair `0x6C655CD7c071E90e9dCF66ee70771dfBb33740Ac`, token "The Robinhood"
  (`ROBINHOOD`) paired with WETH, **$9,588.80 of live liquidity**, a real
  price (`priceUsd: 0.000004330`) and a `pairCreatedAt` timestamp — this is
  a functioning pool, not a stub or a search false positive (contrast: a
  direct DexScreener search for "bankr" on this chain returned only
  keyword-matching noise, no genuine Bankr-labelled pairs, so that avenue
  did not confirm Bankr's on-chain footprint this session). **flapsh's own
  factory address, launch volume, and whether it shares any contract with
  Pons v2 or Bankr were not investigated** — this is a name and one live
  pair, not a sized launchpad.
- **DexScreener's `dexId` field cannot distinguish launchpads once a token
  graduates to a Uniswap-family pool**, since Pons v2's own graduated pools
  also show `dexId: "uniswap"` (research 0039 §5) — so a population count of
  "how many tokens came from which launchpad" needs each launchpad's own
  factory address, not a DEX aggregator's labels, confirming 0038 §6's
  conclusion that an index built only from Pons v2's factory misses every
  creator who used a different launcher, and that sizing the gap needs
  launchpad-specific on-chain work not done here.

**Evidence label: Bankr/Noxa — documented (unchanged from 0035, not
re-verified). flapsh's existence and one live pair — observed (from an
already-captured file, re-read this session). Rough launch counts for any
of the three — not established.**

## 6. Not established

- What `phase` reads at a value other than 0 or 2 (whether 1 is ever used,
  as a mid-graduation state or otherwise).
- The graduation event's real name and full signature (only its topic hash,
  emitting contract, and two decoded data fields are confirmed; no verified
  source for the deployed factory exists to name it against).
- What the two token flows in the graduation transaction represent exactly
  (treasury vs. pool seeding vs. an allocation) — decoded as amounts and
  addresses, not interpreted as intent.
- The actual post-graduation pool contract address, its pool key, and a
  direct reserves/price read against it (Uniswap version is inferred from
  field shapes and 0036's source paragraph, not observed from a pool
  contract's own state).
- Bankr's and Noxa's on-chain factory addresses, launch counts, or any
  shared contract with Pons v2 — still resting on research 0035's press/docs
  sourcing, not re-verified or measured on-chain this session.
- flapsh's factory address, launch volume, and total footprint on Robinhood
  Chain — one live pair confirms it exists; nothing about its scale is
  known.
- Whether any other launchpad besides Pons v2, Bankr, Noxa and flapsh
  operates on Robinhood Chain — not searched for beyond what one
  already-captured DexScreener sample happened to contain.
