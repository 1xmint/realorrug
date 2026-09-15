<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0044 — Liquidity manipulation and sell-blocking: what bytecode and logs actually show

**Date:** 2026-09-15
**Status:** read-only research. No code touched, no file written outside this
one. Builds on [research 0039](0039-robinhood-chain-data-on-a-budget.md) (the
data budget), [research 0040](0040-pons-v2-graduation-price-and-other-launchpads.md)
and [docs/research/0036](0036-pons-v2-read-from-a-real-launch.md)
(Pons v2 mechanics, both already-run against real Robinhood Chain mainnet
launches), and goes one level deeper than
[research 0041](0041-rules-or-a-reasoning-model-for-token-risk.md) on
*mechanism* — 0041 established that GoPlus/RugCheck/TokenSniffer/Bubblemaps
all describe themselves as fixed rule engines, not ML; this document asks
**how each rule is actually computed** (bytecode pattern, storage slot,
simulated call, or source dependency) and what defeats it. New reads this
session: GoPlus's own field-description docs (re-read for mechanism, not
just field names), Honeypot.is's own API docs, TokenSniffer's methodology
description, one academic paper measuring simulation-based detection against
a named commercial vendor, and Team Finance's/UNCX's own description of how
an LP locker works. All dated 2026-09-15 unless noted. Labels used
throughout: **documented** (a primary source states it), **vendor** (the
claim is the vendor's own, unverified independently), **observed** (this
project or a cited research note read it from the live chain), **inferred**
(reasoned from the above, not itself read), **not established**.

## 1. Check-by-check table

Costs use 0039's own read numbers: Alchemy `eth_call` = 26 CU, `eth_getLogs`
= 60 CU, `eth_getBlockByNumber`/`eth_getTransactionReceipt` = 20 CU,
`eth_blockNumber` = 10 CU (alchemy.com/docs/reference/compute-unit-costs,
read 2026-09-15 per 0039 §2); QuickNode = 20 credits per call for
"all methods*" on Ethereum-style chains, Robinhood not separately confirmed
(0039 §2, itself flagged "not established" there). **`eth_getCode` and
`eth_getStorageAt` are not in either vendor's published per-method table**
read in 0039 — costs below for those two methods are *inferred* as
`eth_call`-equivalent (same JSON-RPC shape, a single contract-state read),
not confirmed against either vendor's own pricing; this is carried forward
into every row that uses them, not re-stated per row.

| # | check | what it detects | what it needs to read | cost (calls; CU/credits) | reliability — false-negative / false-positive modes | innocent twin | build now / later / refuse |
|---|---|---|---|---|---|---|---|
| 1 | Mint-selector presence | a callable `mint`-shaped function exists in the dispatcher | `eth_getCode`, grep 4-byte selectors | 1 call, ≈26 CU / 20 credits (inferred rate) | **FN:** logic lives behind a proxy/diamond facet not visible in this address's own code (see #7, #15); selector computed at runtime rather than a literal `PUSH4`, missed by a naive grep. **FP:** template boilerplate (e.g. OpenZeppelin capped-mint scaffolding) ships a `mint` selector that is never wired to a caller anyone can reach | legitimate bridge/wrapped-asset tokens need a real mint function; template contracts carry a dead one nobody calls | build now — cheap, one weak signal, never alone past `Sketchy` (ADR 0027 rule 4) |
| 2 | Pause-selector presence | trading/transfer can be halted by an admin | `eth_getCode` | 1 call, same rate | **FN:** custom flag (`tradingEnabled`) uses a non-standard selector outside a fixed dictionary. **FP:** pause function present but ownership already renounced (see #6), so it is permanently dead code | OpenZeppelin's own `Pausable` is a mainstream, disclosed safety feature on well-regarded tokens | build now, must be read together with #6 |
| 3 | Blacklist-selector presence | transfers can be blocked per-address | `eth_getCode`; **no standard selector exists for "blacklist"** across projects, unlike ERC-20's own methods — a raw selector grep is weak here specifically because the function name (and therefore the selector) is not standardized. GoPlus's own field doc states only the outcome ("Describes whether the blacklist function is not included in the contract," docs.gopluslabs.io/reference/response-details, read 2026-09-15) and not the detection method | 1 call if a dictionary hit; realistically needs a decompiler or verified source to catch renamed variants, neither of which is in the JSON-RPC-only budget (0039) | **FN dominant** — no canonical selector; renamed or inline-checked (`if (blacklisted[from]) revert`) logic is invisible to a selector grep. **FP:** sanctions-compliance blacklisting (OFAC-style, as on centralized stablecoins) is legitimate and disclosed | sanctioned-address blocking on regulated stablecoin-style tokens is normal, not a rug tell | build later — needs a decompiler or corpus we don't have budgeted |
| 4 | Transfer-fee/tax-switch presence and live rate | whether tax can change, and what it currently is | `eth_getCode` for the switch selector; a **simulated buy+sell** to read the live effective tax (selector presence alone doesn't say the current rate) | switch: 1 call; live-tax simulation: 2–6 `eth_call`s with state override (buy leg + sell leg, possibly several trade sizes since tax can be size-dependent) ≈ 52–156 CU / 40–120 credits per token | **FN:** proxied fee-setter. **FP, load-bearing on this venue specifically:** Pons v2's own curve carries a **snipe tax starting at 9,900 bps (99%) and decaying to near-zero over 3 seconds** (observed, docs.md / [0036 §1, §3](0036-pons-v2-read-from-a-real-launch.md) — measured live: "a fee of 1% plus 72,156,075,574,116 wei [19 bps]" at two seconds post-launch, inferred to be the decaying snipe tax folded into the `fee` field). A same-block or few-block simulated sell on a fresh Pons v2 launch will read a near-100% "sell tax" that is the platform's own anti-snipe mechanic, not malice | Pons v2's built-in decaying snipe tax (documented, own platform mechanic) mimics a high-tax honeypot signature for the first ~3 seconds/~30 blocks (at 0.1019 s/block, research 0038) of *every* launch on this venue | build the simulated-tax read later (needs the curve's own buy/sell selectors confirmed and a snipe-window suppression rule first); refuse the selector-only version standalone — too noisy given #4's own innocent twin is systemic, not occasional |
| 5 | Max-tx / anti-whale limiter presence | transaction or wallet-size caps exist | `eth_getCode`, weak dictionary match (more standardized naming than blacklist, still heuristic) | 1 call | **FN:** renamed variable. **FP:** anti-whale caps are a *majority-common*, disclosed, pro-community feature on ordinary fair-launch tokens — the innocent case is the base rate | most legitimate meme-token launches ship a max-tx/max-wallet cap specifically to prevent one whale from dumping | refuse as an independent signal — base rate of legitimate use is too high to carry weight even at low verdict levels |
| 6 | Ownership-renounced | whether `owner()` (or the Ownable storage slot) is the zero address | 1 `eth_call` to `owner()` (0x8da5cb5b), or `eth_getStorageAt` on slot 0 | 1 call, ≈26 CU / 20 credits | **FN — the important one:** a "hidden owner" keeps privileged power in a *second*, non-`owner()`-named variable after the visible one is renounced; this is exactly what GoPlus's own `hidden_owner` field targets ("used by developers to maintain ownership ability even after abandoning ownership," docs.gopluslabs.io, read 2026-09-15) — GoPlus documents *what* the field means, not *how* it finds the hidden variable. **FP:** renouncing ownership does not undo malicious logic already baked into immutable bytecode before the renounce — "renounced" is not "safe" | a non-renounced owner is completely normal for an early-stage or actively-governed project; most tokens in their first hours/days have not renounced, and many long-lived legitimate projects never do | build now (one cheap call), never let it move the verdict past `Sketchy` alone, and always pair with #7/#14 before treating a renounce as reassuring |
| 7 | Proxy detection (EIP-1967 standard slots) | whether the contract delegates its logic elsewhere | `eth_getStorageAt` on the implementation slot (`0x360894a1...`), beacon slot, admin slot | 1–3 calls | Catches the **standard** EIP-1967 pattern reliably — this is a fixed, well-known slot, not a heuristic, and was already confirmed workable on this exact chain: 0040 §2 identified a real beacon proxy at graduation this way ("100 bytes of code, an EIP-1967 beacon proxy — its storage at the beacon slot... identifies the pattern"). **FN:** a non-standard delegatecall dispatcher using an arbitrary slot, or an EIP-2535 diamond (many facets, no single implementation slot) — both read as "not a proxy" with no error raised | upgradeable proxies are the industry-standard pattern for actively-maintained, legitimate DeFi protocols; proxy-ness alone is not a red flag, only unrestricted upgrade rights are | build now — cheap, already measured on Robinhood Chain, and a prerequisite for correctly targeting every other bytecode check (checking the proxy's own code instead of the implementation's checks the wrong contract) |
| 8 | Simulated buy+sell (honeypot.is / TrapdoorAnalyser style) | whether a sell actually reverts; real live buy/sell/transfer tax, via the real code path | `eth_call` with state override (fund a scratch address, execute the real router/curve swap call) or a forked EVM | 2 calls minimum, 4–8 realistic (multiple sizes/times) ≈ 100–210 CU / 80–160 credits per token | **This is the strongest check here** because it exercises the real code path instead of pattern-matching it. Documented mechanism: "A honeypot simulation trades the token without spending anything: it runs a buy and then a sell in a single gas-free `eth_call` with a state override... This works without any API because `eth_call` is a read-only simulation with no gas spent and no transaction broadcast" (community/third-party description, dev.to, read 2026-09-15 via WebSearch — **this specific mechanism description is third-party, not Honeypot.is's own docs**: Honeypot.is's own API reference page, docs.honeypot.is/ishoneypot, read directly 2026-09-15, does **not** state whether it uses `eth_call` state override, a forked EVM, or a real sent transaction — flagged as vendor-undisclosed). **Measured, academic:** TrapdoorAnalyser (arXiv:2309.04700, "From Programming Bugs to Multimillion-Dollar Scams," read 2026-09-15) combines this exact buy-and-sell simulation with a bytecode-semantic check on ≈30,000 labeled Uniswap V2 tokens and reports "outperform[ing] the state-of-the-art commercial tool GoPlus in accuracy" and detecting "with very high accuracy even Trapdoor tokens with no available Solidity source code" — **the numeric accuracy/false-positive figure itself was not captured this session** (only the abstract was read); treat as measured-but-number-not-in-hand. **FN — load-bearing on Pons v2:** a single-block simulation cannot see a **time-gated** trap, and Pons v2's own snipe tax (see #4) is exactly that: a mechanic that changes behavior over the first ~30 blocks. **FN also:** wallet-size-gated traps the simulation didn't probe. **FP:** pre-graduation on Pons v2 there is **no Uniswap router or pool at all** — the curve is a bespoke contract (0040 §3), so simulating "a sell" against a standard router will simply fail to find a route, which is "no market exists yet," not "cannot sell" — AGENTS.md §1's "zero is a statement about the instrument" applies directly | Pons v2's decaying snipe tax reads, for the first few seconds of every launch, exactly like a temporary sell-block/high-tax honeypot signature — this must be suppressed or time-shifted past, or every fresh launch on this venue false-positives | build later — best-evidenced method in this whole table, but needs the curve's own sell-function selector confirmed (0036 names the `CurveBuy`/`CurveSell` *events*, not the call selectors) and the snipe-tax suppression window built first, or it misfires on every young launch |
| 9 | Static bytecode pattern-match against a scam-code corpus (TokenSniffer-style) | code similarity to known-scam templates | `eth_getCode` once, matched against a maintained pattern database | 1 call *if we had the corpus* — the corpus itself, not the RPC call, is the real cost, and is out of this document's scope (a separate build-or-license project) | **Vendor claim, not independently measured here:** "Analyzes both smart contract source code and bytecode against a database of over 10,000 scam code patterns built from five years of research" (tokensniffer.readme.io/reference/introduction, quoted in 0041, re-confirmed 2026-09-15) — no independent accuracy number was found for this claim in either 0041 or this session. **FN:** any genuinely novel bytecode misses a corpus built on prior patterns; 0036 already showed **Pons v2's own deployed code diverges from its published GitHub source** in named functions and constants, so any corpus built from published source would already be reading the wrong bytecode for this venue. **FP mode unstated by vendor** | forked/copy-pasted code from an already-flagged scam template is extremely common among *legitimate* first-time deployers who don't know the template's history — bytecode similarity is evidence about code lineage, not deployer intent | refuse for now — no corpus in-budget (0039's budget is JSON-RPC only, no indexer or third-party database in the hot path), and the vendor's own claim is unmeasured by anyone outside the vendor |
| 10 | Liquidity removal — Uniswap-style pool (post-graduation) | LP-token burn / large reserve outflow not backed by a swap | `eth_getLogs` on the pool/pair contract for `Burn`/`Sync`-shaped events | 1 `eth_getLogs` per window, ≈60 CU / 20 credits, **cheap** — the real barrier is that the target address is unknown, not the cost | On a standard Uniswap V2 pair this is a hard-to-fake, well-defined signal (a `Burn` event is the removal, to the wei) — **FN near-zero for the mechanical event itself. FP:** removal ≠ rug without attributing the withdrawer to the token's own creator/deployer address (available for Pons v2 from the launch tx, 0036 §1) — an unrelated LP's own legitimate withdrawal reads identically in the logs | a team migrating its own liquidity to a new pool/fee-tier removes 100% of one pool in a way indistinguishable, from the `Burn` event alone, from abandonment | build now for the raw event watch once a pool address exists (currently blocked — see Pons v2 phase notes below); pair with the creator-address match from 0036 as a build-now companion, not a later add-on |
| 11 | Liquidity removal / drain — bonding curve (pre-graduation, Pons v2) | whether the curve's own reserves can leave outside the priced buy/sell path | `eth_getCode` selector scan of the deployed curve for any admin/owner withdraw function; `getReserves()`/`quoteReserve()`/`tokenReserve()` (selector `0x0902f1ac` etc., confirmed working, 0040 §3) to watch the balance itself | reserves read: 1 call, ≈26 CU/20 credits; the admin-withdraw selector scan is the same 1-call `eth_getCode` cost — **it has simply never been run**, in this document or in 0036/0038/0040 | **Genuinely not established, not inferred as fine.** 0040 §2 confirmed reserves only leave the curve, observed, via (a) the priced sell path, (b) the fee sweep to a separate escrow that the creator later claims (fully instrumented to the wei, 0036 §5 — this moves *fees*, not principal reserves, so it is not a leak), and (c) the graduation transaction itself. **No search was run for a privileged withdraw function on the curve** — the deployed bytecode is known to diverge from the published GitHub source in other named functions (0036, "How it was read"), so a source-only assumption of "no such function" would be exactly the kind of unchecked claim AGENTS.md §3 rule 2 forbids | none available — this row is a genuine capability gap, not a signal with a known innocent explanation, until the scan is run |
| 12 | "Spoofing" | — | — | — | **There is no order book on a bonding curve or an AMM pool**, so the CEX meaning of spoofing (fake resting orders cancelled before fill) has no on-chain mechanism here at all. The nearest things colloquially called "spoofing" are: (a) wash trading — same/colluding wallets trading back and forth, detectable via funding-source clustering (Bubblemaps' method, described in 0041 §1/§3, not built here); (b) an unbacked "locked liquidity" claim (see #13); (c) sandwich/front-running that moves the curve's displayed price without net demand, which the reserves getter (#11) cannot itself distinguish from genuine trading. Conflating any of these with "spoofing" would be introducing an unchecked fact (AGENTS.md §3 rule 2) | n/a | refuse the word "spoofing" as a Pons v2 signal name; if wash-trade clustering is built later, name it that |
| 13 | Locked-LP verification (Unicrypt/UNCX Network, Team Finance, PinkLock) | whether an LP token sits in a known locker contract, unexpired | requires (a) the pool/LP-token address (**not established for Pons v2 post-graduation**, 0040 §3/§6) and (b) confirmation the locker vendors have deployed on Robinhood Chain (chain id 4663) at all — **not checked this session** | once both are known: 1 `eth_call` (balance/ownership in the locker) + 1 `eth_call` (unlock timestamp) per candidate token, cheap — gated on two unknowns, not on cost | **Documented, vendor's own description:** "the locker contract records the deposit amount, the lock duration, and the owner address. Until the unlock timestamp is reached, no withdrawal function will execute: the contract simply reverts" (paraphrasing Team Finance's own docs, docs.team.finance/services/token-locks/liquidity-locks, read via WebSearch 2026-09-15) — a hard on-chain guarantee when it resolves, not a heuristic. **FN:** genuinely locked liquidity in a smaller/regional/bespoke locker we don't have the address for reads identically to "not locked." **FP near-zero for a positive finding itself**, but a positive finding says nothing about the *unlock date* — a lock expiring in 3 days functions like no lock at all for a token trying to establish trust today, so the expiry must be shown every time, or the check misleads by omission | Pons v2's pre-graduation curve holds reserves in the curve contract itself, not an LP token at all (0040 §3) — "locked or not" is a category error before graduation, not a "no" answer; protocol-held liquidity with *no* withdraw function at all is stronger than a lock that eventually expires, and would score as "not locked" by a naive locker-address check | build later — needs the graduated pool address (open item, 0040) and confirmation the locker vendors serve this chain (not checked); pre-graduation, refuse asking the question at all |
| 14 | Hidden admin via non-standard storage slot (beyond EIP-1967) | a privileged address stored somewhere the ABI doesn't call `owner` | brute-force `eth_getStorageAt` across slots, or a decompiler/verified source | unbounded without source — cost is not the limiter, the analysis capability is | This is exactly the mechanism behind GoPlus's own `hidden_owner` field, and GoPlus's docs describe *what* it means, not *how* it's found (checked directly this session, docs.gopluslabs.io/reference/response-details) — **vendor claim, method undisclosed, not independently reproducible from public docs alone**. Distinguishing "this address-shaped storage value is the admin" from "an unrelated variable that happens to look like an address" needs source or a decompiler, neither in the JSON-RPC-only budget (0039) | legitimate multisig/DAO-governed contracts often store an admin address (a Gnosis Safe, a timelock, a governor) in a non-`owner()`-named, fully-disclosed variable that would look identical from bytecode alone to a "hidden" admin | refuse for now — no decompiler or verified-source pipeline fits the budget, and the vendor doing this doesn't publish a reproducible method |
| 15 | Selector-obfuscation / dispatcher defeat (cross-cutting, not its own check) | — | — | — | Every selector-presence check above (#1–#5) is defeated by: (a) a proxy where real logic sits in a different address (#7 helps only for standard EIP-1967); (b) an EIP-2535 diamond, where there is no single implementation slot, only a per-selector facet mapping needing a `facetAddress(bytes4)` call per selector of interest; (c) hand-rolled assembly dispatch computing selectors at runtime rather than as a literal `PUSH4`, invisible to a naive byte-prefix grep; (d) selector collisions — two different function signatures can hash to the same 4 bytes, so a match proves the dispatcher *responds* to that value, not which named function it routes to, without decompiling the jump target | n/a | this is the reliability ceiling for rows 1–5, stated once here rather than repeated |

## 2. Pons v2 — what's pullable, by whom, at which phase

**Pre-graduation (`phase == 0`, observed on the un-graduated token in 0040
§1).** Reserves are directly readable, cheaply: `getReserves()` /
`quoteReserve()` / `tokenReserve()` at selector `0x0902f1ac` and its
component selectors all answer on the live curve, confirmed 2026-09-15
(0040 §3: "there is a real, working view getter for the curve's own
reserves"). Reserves are **not** an LP token — there is no ERC-20 position
to lock or unlock (a locked-LP check is a category error here, row 13).
The only observed way tokens/quote assets leave the curve pre-graduation
are: (a) the priced buy/sell path itself, which pays a 100 bps base fee
(30% protocol / 70% creator) plus a 0–1,000 bps creator tax the launcher
chose once (documented, 0036 §1–§2); (b) a fee sweep to a separate escrow
contract, fully instrumented to the wei in 0036 §5 (a sweep credits the
escrow, not the creator directly; the creator later calls `claim` — every
step of one real claim was matched to the wei, 0036 §5), which moves fees,
not principal reserves. **Whether the deployed curve exposes any
owner/admin withdraw function reachable outside those two paths is not
established** — no selector scan for one has been run in this document or
in 0036/0038/0040, despite costing a single `eth_getCode` call. This is
table row 11's genuine gap, not an inferred "probably safe."

**At graduation (`phase` observed to flip 0→2 in one transaction, no
intermediate value ever observed, 0040 §1).** The curve empties its whole
remaining token balance in exactly two transfers, observed on one real
graduation (0040 §2, tx `0x7352d0f7...`, block 62,263,572): 285,714,285.71…
tokens to the **factory** (matching the graduation event's own data word,
which also carries the exact quote raised — 4.2 ETH + 263 wei, matching
`getLaunchConfig(0)`'s threshold exactly), and 5,622,402.01… tokens through
an EIP-1967 **beacon proxy** (`0x9689992f...`) forwarded on to
`0xe4c6b769...`, an address holding **no code** (`eth_getCode` returns
`0x`) — a plain EOA or undeployed address, not a pool contract. **What this
second flow represents (protocol treasury, pool seeding, or a
creator/team allocation) is unresolved** — flagged, not guessed, in 0040
§2, unchanged here. This is exactly the shape a liquidity-removal detector
would flag ("tokens moved to an unexplained wallet at the moment of a
phase change") — but because the flow was observed on the one graduation
decoded in detail, and 0040 §2 separately found the factory's graduation
log fires **8,262 times** to date (a full-chain enumeration that ran to
completion), if this second flow is a constant of every graduation it
cannot discriminate one bad launch from a normal one, and must not be
scored per-token until the EOA's role is identified. A `FeesSwept` event
also fires in the same transaction (a final sweep before graduation,
0040 §2) — expected, not anomalous, given §1's escrow mechanic above.

**Post-graduation (`phase == 2`).** Fees continue: the deployed **meme
hook** (`0xe5e70264...`, published-source-named, 0036 §5) answers
`hookFeeBps()` = 100 and `protocolFeeShareBps()` = 3,000 — identical rates
to the curve — and emits "more than 10,000 logs in 10,000 blocks," so
graduated pools trade (0036 §5). `getLaunchedToken`'s `poolFee` = 0 and
`tickSpacing` = 200 on every graduated token checked (0040 §3) are
consistent with a Uniswap V3/V4-shaped pool, `poolFee` 0 with a hook is
consistent with V4's dynamic-fee flag — **inferred from field shapes, not
observed from a decoded pool contract.** The actual PoolManager or
pair/pool address, its pool key, and any reserves or removal (`Burn`/
`Sync`-equivalent) log target were **not found** in 0040 §3/§6. This means
table row 10 (the Uniswap-style removal watch) **cannot currently be run
against any real graduated Pons v2 pool** — not for cost reasons (a
`Burn`-log `eth_getLogs` call is 60 CU/20 credits, trivial), but because
the target address itself is missing. No hook sweep has been decoded, so
whether the creator tax and hook fee split the same way post-graduation as
pre-graduation rests on the published source only, not a captured
transaction (0036 §5, "the split after graduation is source-only").

**Summary table, who can pull what, by phase:**

| phase | what could move | observed mechanism | who | verified? |
|---|---|---|---|---|
| pre-graduation | curve's ETH/token reserves | buy/sell path (fee+tax); fee sweep to escrow | any trader; a keeper sweeps fees (not creator/protocol directly, 0036 §5) | observed, to the wei, for the fee path; **admin-withdraw path not searched for** |
| graduation instant | curve's entire remaining balance | two transfers: to factory, and via beacon proxy to an unlabeled EOA | protocol-controlled (factory + a proxy the protocol deployed) | observed mechanically; **purpose of the EOA leg unresolved** |
| post-graduation | pool liquidity (if any) | inferred Uniswap V3/V4-family pool + meme hook fee sweep | whoever holds/controls the pool position; hook fee split source-only | **not observed** — pool address unfound |

## 3. Recommendations

Per-check recommendations are the last column of §1's table. Summarized:

- **Build now (cheap, single-call, already measured on this chain):**
  proxy resolution (#7), ownership-renounced (#6), mint/pause selector
  presence (#1, #2) as weak-only signals, curve-reserves watch (#11's
  reading half), and graduation-log enumeration + creator-address
  attribution for a future Burn-style watch (#10's reading half, once a
  pool address exists). None of these may individually exceed `Sketchy`
  per ADR 0027 rule 4 — every one carries a real, stated innocent twin
  above.
- **Build later (needs work we haven't done or data we don't have yet):**
  simulated buy/sell tax read (#4, #8) — needs the curve's own sell
  selector and a snipe-tax suppression window; locked-LP verification
  (#13) — needs the graduated pool address and locker-vendor chain
  coverage confirmed; blacklist/max-tx-variant selector scanning beyond a
  narrow dictionary (#3 partially) — needs a decompiler.
- **Refuse (doesn't fit the JSON-RPC-only budget, or the base rate makes
  it noise, or the vendor's own method is undisclosed and unreproducible):**
  scam-pattern-corpus matching (#9), hidden non-standard-slot admin
  brute-forcing (#14), max-tx presence as a standalone signal (#5), and
  the word "spoofing" as a Pons v2 signal name (#12).
- **The one open item worth running before any other product decision
  here:** a single `eth_getCode` selector scan of the deployed Pons v2
  curve for an admin/owner withdraw function (row 11) — it is one call,
  and right now we genuinely do not know whether pre-graduation liquidity
  removal by anyone other than a trader is even possible on this venue.

**One overall sentence:** build the cheap, already-measured bytecode/
storage/log reads now as weak, twin-carrying signals that never alone
clear `Sketchy`, treat the simulated-trade check as the strongest available
method but defer it until Pons v2's own sell path and snipe-tax window are
understood, and refuse anything that needs a corpus, a decompiler, or an
undisclosed vendor method we cannot reproduce inside the JSON-RPC-only
budget.

## 4. Not established

- Whether the deployed Pons v2 curve contract exposes any owner/admin
  function that can withdraw `quoteReserve`/`tokenReserve` outside the
  normal sell or graduation path — no selector scan has been run, here or
  in 0036/0038/0040 (table row 11, phase-notes §2).
- What the EOA `0xe4c6b769...`, which receives a token allocation via a
  beacon proxy on every graduation, actually is (protocol treasury, pool
  seed, team allocation) — flagged unresolved in 0040 §2, unchanged.
- The actual post-graduation pool/PoolManager contract address, its pool
  key, and any reserves or `Burn`/`Sync`-equivalent log target — 0040
  §3/§6, unchanged; blocks table row 10 from being run at all today.
- Whether Unicrypt/UNCX Network or Team Finance (or any locker vendor) has
  deployed on Robinhood Chain (chain id 4663) — not checked this session.
- The actual detection mechanism behind GoPlus's `hidden_owner`,
  `can_take_back_ownership`, `is_blacklisted` and De.Fi's admin-function
  scan — both vendors' own docs (GoPlus's response-details page, read
  directly 2026-09-15; De.Fi's public description via secondary sources
  per 0041) state what the fields mean, not how they are computed.
- Honeypot.is's own stated simulation mechanism — its own API docs
  (docs.honeypot.is/ishoneypot, read directly 2026-09-15) do not say
  whether it uses `eth_call` with state override, a forked EVM, or a real
  transaction; the `eth_call`-state-override description used in table row
  8 came from third-party/community write-ups (dev.to posts, found via
  WebSearch), not the vendor's own documentation — flagged, not treated as
  vendor-confirmed.
- TrapdoorAnalyser's (arXiv:2309.04700) actual numeric accuracy and
  false-positive rate against GoPlus — only the abstract was read this
  session; it claims outperformance in prose but no number was captured.
- Per-method CU/credit cost for `eth_getCode` and `eth_getStorageAt`
  specifically on Alchemy or QuickNode — absent from both vendors'
  published per-method tables as re-used from 0039; every cost using these
  two methods above is inferred as `eth_call`-equivalent, not confirmed.
- TokenSniffer's, GoPlus's, and De.Fi's own measured false-positive/
  false-negative rates for any of their checks — none of the three
  publishes an independently-verifiable accuracy number; only RugCheck's
  and TrapdoorAnalyser's academic comparison touch measured numbers at all
  (per 0041 and this document), and even TrapdoorAnalyser's own number
  was not captured (above).
- Selector dictionaries for non-standard blacklist/max-tx function names —
  not built or sized this session (table rows 3, 5).
- Cross-checked against [research 0042](0042-detection-intelligence-we-left-in-radar.md):
  no contradiction found. 0042 independently confirms Radar itself never
  built a liquidity-spoofing/removal detector or a honeypot/sell-blocking
  detector — both appear only in a third-party brainstorm document, never
  as measured Radar code — consistent with this document's own finding
  that the strongest available method (simulated buy/sell) is
  vendor-and-academic-sourced, not something either project has built or
  measured itself yet.

**Confidence: CHECKED**, on the vendor-mechanism question (GoPlus,
Honeypot.is, TokenSniffer's own docs were read directly this session,
narrower than 0041's field-name-level pass) and on the Pons v2 phase
mechanics (resting on 0036/0040's own live chain reads, not re-derived
here). **SPECULATION** only on table row 11 (pre-graduation admin-withdraw
existence) and row 13 (locker-vendor chain coverage) — both are named as
open, one-call-away checks rather than answered. A second pass that ran
the row-11 `eth_getCode` scan on the live curve, or found the actual
post-graduation pool address, would change §2 and could downgrade or
resolve two "not established" items above; it would not change the
per-check build/later/refuse recommendations in §3, which rest on the
JSON-RPC-only budget (0039) and ADR 0027's single-signal ceiling, neither
of which a chain read would change.
