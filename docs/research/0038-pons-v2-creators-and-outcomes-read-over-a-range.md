<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0038 — Pons v2 creators and outcomes, read over a range

**Date:** 2026-09-15
**Status:** partial. Sections 1, 4 and the deployer/fee-recipient split in §2 are
read from mainnet and quoted below. The launch count in §1 is a measured
partial sum, extrapolated, not a full enumeration — the RPC's own query timed
out partway through. Graduation's phase word is confirmed to exist and to read
zero on every fresh launch sampled; no non-zero example was found in the
budget available, so its non-zero values are not established. §3's thresholds
and §5's mechanics lean on research 0036 and on inference, marked as such.
**Feeds:** plan 0001 step 7b, [ADR 0026](../adr/0026-realorrug-reads-nothing-from-radar.md).

## How it was read

Robinhood Chain mainnet (chain id 4663), through the public RPC
`https://rpc.mainnet.chain.robinhood.com`, on 2026-09-15 between about 19:40
and 21:30 UTC. Every call is `eth_getLogs`, `eth_call`, `eth_getBlockByNumber`,
`eth_getCode` or `eth_blockNumber`, made directly by a Node script in this
session (kept under `AppData/Local/Temp/claude/.../scratchpad/`, not part of
this repository). No transaction was sent. The factory address, event topics
and `getLaunchedToken` layout are the ones [research 0036](0036-pons-v2-read-from-a-real-launch.md)
and `crates/realorrug-robinhood/src/pons.rs` already settled; this document
adds the range reads plan 0001 step 7 says are still missing.

## 1. Enumerating launches

**Factory and event, already known and reused, not re-derived here**: factory
`0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e`;
`TokenLaunched(address indexed token, address indexed curve, address indexed deployer, address pairToken, uint256 launchConfigId, uint256 graduationThreshold)`,
topic `0x8d4aad4953d0ca700d468f3753aa14432d1b35b43ec6409f051fb6aa43a89607`
(`crates/realorrug-robinhood/src/pons.rs`, confirmed against mainnet in
research 0036).

**Factory deployment, verified.** A binary search over `eth_getLogs` for the
`TokenLaunched` topic (no address filter, then with it) found the first
matching log at block **27,027,321** (`0x389193691a40fc306c3114b1d96070b43a189b7b3f1a056cbf16fb517484913b`,
timestamp `2026-08-03T19:50:47Z`). The factory's own earliest log of any kind —
an `OwnershipTransferred(address,address)` (topic `0x8be0079c…`, the standard
OpenZeppelin `Ownable` signature) from `0x0` to a deployer — is at block
**26,921,206** (`2026-08-03T14:41:19Z`), about five hours before the first
launch. Taken together: the factory deployed on 2026-08-03, about a month
after Robinhood Chain's 2026-07-01 mainnet (research 0035 §1).

**Total launches to date: not fully counted — a measured partial sum,
extrapolated.** A recursive count (split any range whose result exceeds the
node's cap, sum the leaves) covered blocks 27,027,321 to 50,081,459 — 23.05M of
the 36.89M blocks between factory deployment and the chain tip at read time
(block 63,913,942) — and summed to **107,108** `TokenLaunched` events before the
node returned `{"code":-32000,"message":"log query timed out"}` on the next
leaf, which the script did not retry (budget). That is **62.5% of the range,
by block count**, not by launch count, since launch density is not uniform
(§ below). Extrapolating the measured rate (0.004646 launches/block) across
the full range gives **≈171,000 launches**, which is an estimate, not a count.
Launch density more than doubled from the earliest chunks to the latest:
8,656 in the first 4.6M blocks (0.00188/block) versus 116 in the *most recent*
3,000 blocks read separately (0.0387/block) — a fully separate, later
measurement, consistent with the recursive count's later chunks (5,000–9,410
per ~250–580k-block leaf). **So the launch rate has grown roughly 20-fold since
factory deployment**, verified by these two independently-read samples, not
by a single trend line.

## 2. Who the creator is

`getLaunchedToken(address)` (selector `0x3c, 0xf2, 0x8b, 0x5a`) returns fifteen
words; word 2 is the deployer, word 3 is `creator_fee_recipient`
(`crates/realorrug-robinhood/src/pons.rs`, `LaunchedToken::from_return`).
**Verified against mainnet, decoded word by word**, on one launch from the
factory's most recent 3,000 blocks: `token=0x2f29ad9a…`, `curve=0xdbba9365…`,
`deployer=0x2b5678a3…`, `feeRecipient=0x7ece25d9…` — **different**. Twelve more
tokens, sampled from a window about 1.5M blocks earlier (≈1.7 days), all had
`deployer == feeRecipient`. So in **13 launches sampled this session, 1
differed (7.7%)** — a small, unweighted sample, not a population figure.

**Recommendation: key the index by `creator_fee_recipient`, not by
`deployer`.** It is the address `pons::LaunchedToken` already carries as the
one Pons v2 pays (research 0036 §2, §5 — the escrow credits it, not the
deployer), so it is the identity a "this creator's other tokens" record should
track even when the two differ. The deployer is still worth keeping alongside
it (it is the address that chose to launch, and the one `check_launch` already
compares against snipe-tax exemptions), but a payout-relevant "creator" is the
fee recipient.

## 3. Outcomes per launch

**The phase word exists and is decoded, at last: word 10 of `getLaunchedToken`
(the comment in `pons.rs` already names it "phase" but `LaunchedToken::from_return`
does not extract it — this is the gap plan 0001 step 7 names).** Verified: on
all 13 tokens sampled in §2 — including a token launched only minutes before
the read — the phase word reads exactly `0`. **No non-zero example was found
in the budget available**: the only source with a known-graduated sample
(research 0036 §6, "3 of 250 had graduated") is 1.2% of launches, and the 12
tokens sampled from a similarly-aged window in this session (§2) held none.
**So `phase == 0` is verified to mean "not graduated"; what a graduated
launch's phase reads is not established this session** — it needs either a
wider sample or a targeted look-up of one of research 0036's three graduated
tokens, neither done here.

**Instant/organic/stillborn, as analogues of the pump.fun-shaped index in
`realorrug-roast/src/creator.rs`:** that code's `instant` is "curve bought
within 3 slots"; Pons v2's blocks are the unit here (no separate "slot"), and
research 0036 §3 already measured the launch-block window directly: the
captured launch's own launch-block buy was the launcher's own dev buy, and the
first buy by anyone else landed **two seconds** (≈20 blocks at 0.102 s/block,
§4) after launch. **Recommendation, inferred, not measured over a sample this
session:** "instant" = curve graduates within the same block as launch, or
within the first few seconds/blocks (an exact cutoff needs a distribution of
launch-to-graduation gaps across real graduated launches, which — given
§1/§3's low graduation rate and this session's budget — was not gathered).
"Organic" = graduates later than that. "Stillborn" = an analogue of "almost no
trades" would read as few or zero `CurveBuy`/`CurveSell` logs from the curve in
some fixed window (for example seven days, matching the reply-aging window in
§5) — not calibrated against a real distribution this session.

**Graduation rate: not established from a fresh sample this session; the one
number available is research 0036 §6's, 3 of 250 ETH-paired launches (1.2%),
read 2026-09-14 over launches about a day old at read time** — likely an
undercount of the *eventual* graduation rate for that cohort, since not enough
time had passed, and not re-verified here.

## 4. Reading a range cheaply

**`eth_getLogs`'s result cap, triggered and quoted, 2026-09-15:**
`{"code":-32000,"message":"logs matched by query exceeds limit of 10000"}`.
Triggered over a 500,000-block range near the chain tip (2,122 logs in a
50,000-block sample scaled up), and even over the full range from block 1.
**No separate block-span cap was found**: a query for a topic with zero matches
over a 30,000,000-block range (block 1 to tip, minus a filter that cannot
match) returned `{"result":[]}` cleanly — about 80% of the chain's whole
history at read time. So the only hard limit measured is the 10,000-*result*
cap, not a range width.

**A second error, hit while counting (§1): `{"code":-32000,"message":"log
query timed out"}`**, on a leaf query for a ~290,000-block range that had
already been split down from failures on wider ranges. Quoted verbatim; not
triggered deliberately, and its own threshold (block width, or log count, or
wall time) was not isolated before the session's budget ran out.

**Rate limiting, triggered and quoted:** `{"jsonrpc":"2.0","error":{"code":429,"message":"Too Many Requests"}}`,
on the very next call after a burst of five requests in quick succession.
Recovered within about one retry at a 4-second backoff (observed: failed once,
succeeded on the second attempt after 20 s in one earlier probe; the
`call()` helper used for every other read in this session retries up to six
times at a 4 s pause and did not exhaust that budget once after the first
probe).

**Old state is pruned aggressively; logs are not.** `eth_getCode` for the
factory at 100,000 blocks back (≈2.8 hours) already failed:
`{"code":-32000,"message":"metadata is not found, 63812511"}` — quoted, and
the same at 2,000,000 blocks back. Research 0036 already found this at
~57,000 blocks back for `eth_call`; this session confirms the same failure
mode holds for `eth_getCode`, and that logs from the same old blocks were
still served without complaint throughout §1's count. **So a backfill must
read outcomes from logs and from the factory's `getLaunchedToken` at the
*current* block only — never from an old `eth_call`.**

**Block time, verified from two blocks 1,555,299 apart in this session
(2026-09-13 to 2026-09-15):** **0.1019 s/block** (158,418 s / 1,555,299
blocks) — about 9.8 blocks/second, much faster than Ethereum L1's ~12 s.

**Cost estimate for a full backfill, inferred from the above, not measured end
to end.** At ~171,000 launches (§1's extrapolation) and a 10,000-log cap, a
`TokenLaunched`-only backfill needs on the order of 20–40 `eth_getLogs` calls
if chunked by pre-measured density (more if density is guessed too high and a
chunk needs splitting, as §1's count shows — it needed 33 queries to cover
just 62.5% of the range because several chunks required one or two splits).
Reading each launch's `getLaunchedToken` for the fee recipient and phase is
one more `eth_call` per token — **171,000 calls**, the dominant cost by far.
At the public endpoint's rate limit (empirically, roughly one request every
few seconds sustained without 429s, from this session's pacing), that is on
the order of a full day of continuous, polite calling for the *history*
alone — **not something to run from a shared two-core VPS against the public
endpoint**, and AGENTS.md §5's "keep request volume polite" argues for
batching `getLaunchedToken` reads to *new* launches only, going forward,
rather than backfilling every historical one's phase and fee recipient.
**An incremental update every few hours** is cheap by contrast: at the
current rate (~0.04 launches/block, §1) a 4-hour window is about 141,000
blocks and roughly 5,600 new launches — one to two `eth_getLogs` calls (under
the 10,000 cap) plus one `eth_call` per new launch. **Whether the free public
endpoint suffices: plausible for the incremental case, not verified over a
sustained run; the endpoint's own docs call it "rate-limited and not
recommended for production use" (research 0035 §1, unchanged), and a paid
Alchemy price was not re-checked this session** (research 0035 §5's
$0.525/million compute units, read 2026-09-13, is the only price on file).

## 5. Seven days later on Robinhood Chain

**Not run against a real seven-day-old reply this session** (the bot has not
launched; there is no reply log yet). What follows is what the primitives
already read (§§1–4, research 0036) support, marked exact or estimate.

- **Graduated or not, at T+7: exact, one `eth_call`.** `getLaunchedToken(token).phase`
  (word 10) is a point read of current state; §3 verified it reads `0` before
  graduation. **When it graduated is not a point read** — no graduation event
  was seen in the factory's logs in this session's samples (§1's 3,000-block
  window held only `TokenLaunched`), so the graduating log is presumed to come
  from the curve or the hook, not captured or confirmed this session. Finding
  the block needs a bounded `eth_getLogs` scan of the curve's own logs (cheap,
  since one curve trades far less than the whole factory — research 0036 §2's
  captured sweep covered 30 trades), watching for whatever event flips the
  phase; that event was not identified this session.
- **Whether it traded since the reply: exact, one bounded `eth_getLogs`.**
  `eth_getLogs` on the curve address for `CurveBuy`/`CurveSell` topics,
  `fromBlock` = the reply's block, `toBlock` = latest. A 7-day window is
  about 5.9M blocks (§4's block time), which risks the 10,000-result cap only
  if the curve itself is extremely active; research 0036's captured curve
  took 30 trades across its life, so for a typical curve this is a single
  cheap call, not a chunked backfill.
- **Price now vs first fill: partly exact, partly not established.** The first
  fill's price is exact and already decodable — the first `CurveBuy`'s
  `quote`/`tokens` ratio, from a log, immutable (`pons::Trade`, already in
  this crate). **The current price is not established this session**: no
  curve view function (a spot-price or reserves getter) was read or captured,
  so "price now" as a point read is a gap, not a number. The nearest
  approximation available without a new capture is the *last* trade's
  quote/tokens ratio from the same bounded `eth_getLogs` above — an estimate,
  since a bonding curve's instantaneous price is not exactly its last trade's
  average price.

## 6. What would make a creator record misleading

- **Fresh wallets per launch: plausible, not measured to a count this
  session.** Research 0036 §1 already found 298 distinct deployers among 350
  launches in a 20,000-block window (2026-09-13) — 1.17 launches per deployer
  on average, consistent with most launchers using a wallet once, though that
  number does not distinguish "one launch ever" from "one launch, so far."
  `launchEnabled` is `true` (0036 §1: anyone may launch, no allowlist), so
  nothing on-chain raises the cost of a fresh wallet beyond the 0.0005 ETH
  launch fee.
- **Deployer ≠ fee recipient, in 1 of 13 sampled this session (§2).** A
  creator index keyed on deployer would misfile that launch's income; keyed on
  `creator_fee_recipient` (this document's recommendation), it would not.
- **Other launchpads on the same chain: not measured on-chain this session,
  reported only from research 0035 §3 (trade press and first-party docs, read
  2026-09-13).** Bankr (Doppler/Uniswap v4) is a separate launcher with its
  own fee recipient mechanic; Noxa, another Robinhood Chain launchpad,
  "stopped on 2026-07-13" per trade press. **An index built only from Pons v2's
  factory misses every creator who only ever used a different launcher** —
  how many that is was not counted this session and is not established in any
  document on file.

## 7. Not established

- The full, exact count of Pons v2 launches to date (§1 stopped at 62.5% of
  the range on an RPC timeout; ≈171,000 is an extrapolation).
- What the phase word reads for a graduated launch (only `0`, "not graduated,"
  was observed).
- The event that marks graduation and the block it happened in, for any real
  launch.
- A real distribution of launch-to-graduation gaps, trade counts, or any other
  quantity needed to set defensible `instant`/`organic`/`stillborn`
  thresholds for Pons v2 — this document proposes an analogue of pump.fun's
  shape (instant = graduates within seconds/blocks of launch) without
  calibrating it.
- A current graduation rate (only 0036's day-old 1.2% sample is on file).
- A spot-price or reserves getter on the curve, for a true "price now."
- The `eth_getLogs` timeout's own trigger (block width vs log count vs wall
  time) — only that it happened, on one ~290,000-block leaf.
- Any measurement of Bankr, Noxa or another launchpad's launch volume on
  Robinhood Chain, to size what a Pons-only index would miss.
- A sustained-run check of whether the public RPC alone can carry the
  incremental (every-few-hours) update politely, end to end.
