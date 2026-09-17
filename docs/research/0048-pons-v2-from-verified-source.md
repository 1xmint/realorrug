<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0048 — Pons v2, read from its verified factory source

**Date:** 2026-09-15
**Status:** read-only research, the source-reading pass over
[research 0047](0047-pons-v2-admin-surface-from-bytecode.md)'s bytecode-only
capture. Builds on 0047 and on
[research 0038](0038-pons-v2-creators-and-outcomes-read-over-a-range.md) §2,
and answers against
[ADR 0027](../adr/0027-the-bot-gives-informed-verdicts-from-evidence.md)'s verdict rule
and `AGENTS.md` §1's evidence rule.

## 1. Method, its limits, and the curve-instance caveat

**Verified, primary source.** The factory
`0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e` on chain 4663 is verified on
Sourcify with `creationMatch` and `runtimeMatch` both `exact_match`, checked
2026-08-04T17:40:43Z. Its 13 first-party sources were fetched from
`https://sourcify.dev/server/v2/contract/4663/0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e?fields=sources`
and sit at `.orchestrator/runs/20260915-robinhood-7b/sources/`. Every claim
below carries a file and line from that tree, or a live call made today
(2026-09-15). OpenZeppelin and Uniswap v4 library files were not extracted
and are not cited.

**The caveat, stated first, per the packet's instruction not to bury it:**
the curve instance at `0x008089e243a611ace236fc4e2127403a3c9e347b` is **not
itself verified** — Sourcify returns `match: null` for it. `PonsV2BondingCurve.sol`
is the curve source the verified factory's build *compiled against*, not
independently proof that this specific instance runs that exact bytecode; a
curve deployed by an earlier, unverified factory build could differ.

**This was checked, not assumed, by a live call today.** `graduate(address)`
is the curve function the factory calls to pull reserves at graduation
(`.orchestrator/runs/20260915-robinhood-7b/sources/PonsV2LaunchFactory.sol:1137`,
`curve.graduate(address(this))`), yet its selector does not appear anywhere
in research 0047's 44-selector capture or in the packet that built it
(`.orchestrator/runs/20260915-robinhood-7b/packets/0047-selector-scan.md:1`,
searched for `graduate(` — zero matches). That is exactly the shape of
mismatch the caveat warns about, so it was run down rather than left as a
coincidence. `graduate(address)`'s selector, computed with `js-sha3`'s
`keccak256` today and cross-checked against five selectors 0047 already
published (`getReserves()` `0x0902f1ac`, `sweepFees(uint256)` `0x3729bb9a`,
`rescueFees()` `0x52920587`, `buy(uint256,uint256,address)` `0x59a87bc1`,
`getLaunchedToken(address)` `0x3cf28b5a` — all five matched byte-for-byte,
which validates the computation), is `0xff6d8d05`. A live `eth_getCode`
against `0x008089e243a611ace236fc4e2127403a3c9e347b`
(`https://rpc.mainnet.chain.robinhood.com`, 2026-09-15) returns runtime code
containing `63ff6d8d0514610214575f80fd5b`, decoded at byte offset 519:
`PUSH4 0xff6d8d05 EQ PUSH2 0x0214 JUMPI` immediately followed by
`PUSH0 DUP1 REVERT JUMPDEST` — a genuine, well-formed dispatch-table branch
(the final entry in its chain, which is why it lacks the leading `DUP1` that
0047 §1's `8063........14` regex required, and why that regex missed it: the
stack already holds the sole remaining selector copy from the prior failed
comparison, so the compiler skipped the redundant `DUP1`). **`graduate(address)`
is present.** This resolves the caveat in the source's favor: no divergence
was found between the live curve's dispatch table and what this factory
source's paired curve would produce, on the one function checked. It is
**not** proof of exact bytecode identity for the whole contract — that would
need a full bytecode diff, not run here — and Sourcify's `match: null` for
this address stands regardless. Every curve claim below is written as "the
source, which this one live check is consistent with," not as independently
verified for this specific instance.

## 2. Confirming the lead's five findings

All five are **verified** against
`.orchestrator/runs/20260915-robinhood-7b/sources/`:

1. **`sweepFees(uint256)`** (`PonsV2BondingCurve.sol:579-587`): `if (graduated)
   revert AlreadyGraduated()`; caller must be `feePolicy.feeSweepOperator()`
   or `msg.sender != deployer` reverts `NotFeeSweepOperator`. Confirmed
   exactly.
2. **`rescueFees()`** (`PonsV2BondingCurve.sol:850`) is `onlyFactory`
   (`modifier onlyFactory` at line 171-174, applied at line 850). The
   factory's `rescueCurveFees(address)`
   (`PonsV2LaunchFactory.sol:1222`) is `onlyOwner`. Confirmed exactly, and
   `owner()` remains the 2-of-3 Gnosis Safe research 0047 §3 read live
   (`0x263ed295dafae1d9aadd6e56c4b6f9f38ee019dd`, threshold 2, three owners).
3. **Neither reaches the tradeable reserves — confirmed, and the seeding
   claim is now verified, not assumed.** `rescueFees`
   (`PonsV2BondingCurve.sol:850-871`) and the ordinary sweep path `_sweepFees`
   (`:741-822`) both zero `quoteFeeBalance`, `buybackQuoteBalance`,
   `creatorTaxBalance` and subtract only `protocolAmount + creatorAmount`
   from `trackedQuote` (`:804`, `:865`) — `trackedTokens`, the token-side
   reserve, is untouched by either function. The one path that moves whole
   reserves is `graduate()` (`:598-638`), which hands `trackedQuote` and
   `trackedTokens` to the factory, and the factory's
   `rescueSweptGraduation(address,address)`
   (`PonsV2LaunchFactory.sol:1254-1285`) is `onlyOwner`, gated by
   `availableAt = launch.sweptAt + GRADUATION_RESCUE_DELAY` (`:1260-1261`,
   7 days per 0047 §3's live reading). **The seeding function was read, not
   assumed: `createGraduatedPool(address)`
   (`PonsV2LaunchFactory.sol:1193-1211`) is `external nonReentrant` with no
   ownership or role check at all** — its own doc comment says so plainly:
   "Permissionless and retryable: a launch stays in Swept until a seed
   succeeds" (`:1189-1191`), and `rescueSweptGraduation`'s doc comment
   confirms the bound this gives: "seeding stays permissionless throughout
   the wait: any holder can end the window early, and permanently, with one
   call to `createGraduatedPool`" (`:1245-1249`). **This is the single most
   important sentence in this document, and it checks out**: the rescue is a
   bounded recovery a Safe can only reach after 7 days and only if nobody
   seeds the pool first, not an unconditional admin drain.
4. **Correcting the finding, plainly, once (see §4 below for why it
   matters).** The claim as written — "the curve's `deployer` is not the
   launcher" — is confirmed exactly: `PonsV2BondingCurve.sol:183`'s own
   docstring calls the constructor's `deployer_` parameter "Token creator,
   credited as the creator fee recipient," and `setCreatorFeeRecipient`
   (`:343-346`) is `onlyFactory` and reassigns that same storage slot. The
   factory's `LaunchedToken` record keeps `deployer: originalDeployer`
   separately (`PonsV2LaunchFactory.sol:852`), and no setter for that field
   was found anywhere in the factory source — confirmed immutable. **But
   0047 §6 read the curve's `deployer()` getter as if it were that same
   immutable fact, and it is not** — see §4.
5. **`exemptFromSnipeTax` is applied by the factory at launch** — confirmed
   exactly at `PonsV2LaunchFactory.sol:844-846`:
   `PonsV2BondingCurve(curve).exemptFromSnipeTax(originalDeployer)`
   unconditionally, and `exemptFromSnipeTax(creatorFeeRecipient)` only
   `if (creatorFeeRecipient != originalDeployer)`. No comment matching "The
   creator's own addresses never count as snipers on their own launch" was
   found verbatim at these lines in the extracted source; the behaviour the
   lead described is exactly what the code does, whether or not that exact
   sentence exists as a comment in this build.

## 3. `launchToken`'s `address[]` — 0047 §7 candidate 1's unconfirmed question, settled

Research 0047 §7 candidate 1 named this **unconfirmed**: "is it the exemption
set for that specific launch, chosen by the launcher? a pre-approved router
list? something else?" — and said one decoded transaction would settle it.
The source settles it without a transaction decode. `launchToken`'s
four-argument overload docstring
(`PonsV2LaunchFactory.sol:705-711`): "a creator-declared list of wallets
exempted from the snipe tax before trading opens to anyone else. This is the
sanctioned pathway for organized teams that bundle their opening buys across
several wallets: declared wallets clear at the untaxed price during the
launch window while undeclared snipers pay the decaying tax." The two-arg
overload's own docstring (`:690-694`) says plainly: "The caller and their
creator fee recipient are exempted from the snipe tax automatically; a
launch with additional bundle wallets should use the overload that takes an
exemption list." `_exemptFromSnipeTax` (`:746-751`) applies it, bounded by
`MAX_SNIPE_TAX_EXEMPTIONS`, to whichever curve the launch just deployed.
**Confirmed: it is the launcher's own declared bundle-wallet list, not a
router registry or anything else.**

## 4. The correction — 0047 §6's "`deployer()` is immutable" is wrong

**Say when wrong, once, plainly, per `AGENTS.md` §1.** Research 0047 §6
wrote: "`deployer()` is immutable (no setter for it was found in either
contract's dispatch table)," and recommended keying the creator index on
both `deployer()` and `creator_fee_recipient`. **This conflated two
different storage slots that happen to share the selector name `deployer()`
across two contracts.** The curve's own `deployer` (`PonsV2BondingCurve.sol:93`,
selector `0xd5f39488` per 0047 §2) is the **mutable** field §2 finding 4
above just confirmed — it *is* the creator fee recipient, reassigned by
`setCreatorFeeRecipient` (`:343-346`), which is exactly the setter 0047 said
it could not find (0047 read the curve's dispatch table for a function named
`setDeployer` or similar and found none; the actual setter for this field is
named for what it does, not for the getter it also happens to satisfy).
Worse, it is not even an independent second key: the factory's
`_setCreatorFeeRecipient` (`PonsV2LaunchFactory.sol:986-996`) updates
`launch.creatorFeeRecipient` **and** forwards the same new value to
`PonsV2BondingCurve(launch.curve).setCreatorFeeRecipient(newRecipient)`
(`:994-995`) in the same transaction, pre-graduation. **The curve's
`deployer()` and the factory's `creatorFeeRecipient` are kept in permanent
lockstep by the factory itself — reading both gives one fact twice, not
two.** The only field that is actually immutable, and actually distinct, is
the factory's `LaunchedToken.deployer` (`originalDeployer`,
`PonsV2LaunchFactory.sol:852`) — a third value, stored only on the factory,
never mirrored onto the curve at all.

**What this changes for 0038 §2, re-answered from source, not re-decided
here.** 0038 §2 keyed the creator index on `creator_fee_recipient` because it
is "the fee recipient" and is mutable; 0047 §6 proposed adding `deployer()`
alongside it as a stable second key. **`deployer()` cannot do that job — it
drifts with `creator_fee_recipient` by construction.** The one field that
does not drift is the factory's `originalDeployer`
(`LaunchedToken.deployer`, and the same value the `TokenLaunched` event's
third indexed topic carries — confirmed in `crates/realorrug-robinhood/src/pons.rs:88`,
which already reads it that way). **Recommendation: key the stable side of
the index on the factory's `originalDeployer` (the event topic / record
field already named `deployer` in `pons.rs`, not the curve's `deployer()`
selector), and keep `creator_fee_recipient` as the current-payout field
0038 already tracks.** The one-sentence tradeoff is unchanged from 0047's:
this needs an extra field kept alongside the existing key, not a replacement
of it, at the cost of one more address to store and dedupe per launch. This
is a recommendation, not a re-opening of 0038's merged decision — the owner's
call, per the packet.

## 5. Code check — nothing found looks wrong

`crates/realorrug-robinhood/src/pons.rs` and
`crates/realorrug-cli/src/launch_check.rs` were read against the source in
§2 and §4.

- `pons.rs:67,88` reads `deployer` from the `TokenLaunched` event's third
  indexed topic — the factory's `originalDeployer`
  (`PonsV2LaunchFactory.sol:872`, `emit TokenLaunched(token, curve,
  originalDeployer, ...)`), not the curve's mutable `deployer()` selector.
  **This is already the correct, immutable field** — §4's correction changes
  the *research recommendation*, not this code.
- `pons.rs:367` compares an unexpected `SnipeTaxExempted` address against
  `launch.deployer || record.creator_fee_recipient` — exactly the two
  addresses the factory unconditionally exempts at launch
  (`PonsV2LaunchFactory.sol:844-846`, §2 finding 5 above). **Correct as
  written.** It does not yet account for §3's launcher-declared bundle-wallet
  list (`snipeTaxExemptions`) as a third legitimate category — an exemption
  granted through that list will currently register as
  `Unclean::Exempted(a)` even when it is the sanctioned bundle-wallet
  pathway. This is not a bug in what the code claims to check (launch
  cleanliness against the two automatic exemptions), but §3's finding means
  a clean read from this function is a narrower bar than "no bundle wallets
  were declared" — worth naming in a future decode, not a code change this
  document makes.
- `launch_check.rs:65,79-83` prints `launch.deployer` and
  `record.creator_fee_recipient` separately and labels them "deployer" and
  "fees to" — already distinguishes the two concepts §4 shows must stay
  distinct. **Correct as written.**

No decoder in either file reads the curve's own `deployer()` selector at
all, so §4's correction does not invalidate anything already built.

## 6. Every other 0047 §7 signal candidate, checked against source

- **Candidate 1 (`snipeTaxExempt`) — changed.** §3 above settles the
  previously-unconfirmed question: the third legitimate exemption category
  is a creator-declared bundle-wallet list, not a mystery. **Restate the
  signal precisely: an address holding `snipeTaxExempt == true` that is
  neither `originalDeployer`, nor `creatorFeeRecipient`, nor on 0047 §3's
  first-party exclusion list, nor traceable to a `launchToken`/`launchTokenFor`
  call's `snipeTaxExemptions` argument for that token.** The innocent twin
  0047 already named (routers, the buyback vault, `graduationExecutor`)
  still holds, and now has a fourth, concrete, checkable case: a legitimate
  team bundling opening buys across several of its own wallets is
  indistinguishable on-chain from an insider pre-exempting sniping wallets,
  unless the exempted addresses are cross-referenced against the launch
  transaction's own calldata. Recommendation unchanged from 0047: build now,
  gated by the exclusion list, never alone.
- **Candidate 2 (`pendingCreatorFeeRecipient`) — still good.** Unaffected by
  §4's correction; the timelock mechanics (`PonsV2LaunchFactory.sol:946-996`)
  match 0047 §3's description exactly. Build now, `Sketchy`-level
  contributor only, unchanged.
- **Candidate 3 (platform-wide thresholds) — still good, and the "who may
  call" gap 0047 §3 flagged as inferred is now verified.**
  `setSnipeTaxStartBps`, `setSnipeTaxSeconds`, `setMaxCreatorTaxBps`,
  `setLaunchFee`, `addLaunchConfig` are every one `onlyOwner`
  (`PonsV2LaunchFactory.sol:406,427,550,564,579`). Recommendation unchanged:
  build now, as a drift alarm, not a per-token signal.
- **Candidate 4 (graduating buyer's identity) — still good, unaffected.**
  Nothing in the extracted source bears on transaction-level routing;
  `buy()`/`sell()` (`PonsV2BondingCurve.sol`) accept any caller and any
  `recipient`, consistent with 0047 §5's finding that graduations arrive
  through many different forwarding contracts. Build later, unchanged.
- **Candidate 5 (admin fee sweeps) — changed from inferred to verified, same
  recommendation.** §2 finding 3 above verifies, rather than infers, that
  `sweepFees`/`rescueFees`/`rescueCurveFees` only ever touch
  `quoteFeeBalance`/`creatorTaxBalance`/`buybackQuoteBalance` and
  `trackedQuote`'s fee-sized slice, never `trackedTokens` or a graduated
  reserve outright. Recommendation unchanged: refuse as a per-token signal,
  watch as a platform event only — now on a verified, not inferred, basis.

## 7. Which of 0044 §4's items this closes further

Quoting [research 0044](0044-liquidity-manipulation-and-sell-blocking.md) §4
exactly, continuing from where [research 0047](0047-pons-v2-admin-surface-from-bytecode.md)
§8 left off:

- **Item 1, now closed.** "Whether the deployed Pons v2 curve contract
  exposes any owner/admin function that can withdraw `quoteReserve`/
  `tokenReserve` outside the normal sell or graduation path." §2 finding 3
  above reads both functions' bodies: neither touches `trackedTokens`, and
  both only ever subtract the fee-sized slice they just zeroed from
  `trackedQuote`. **Closed: no, within the read source.**
- **Item 5, still left open.** "The actual detection mechanism behind
  GoPlus's `hidden_owner`, `can_take_back_ownership`, `is_blacklisted` and
  De.Fi's admin-function scan." Not addressed by this source read either —
  it is about a third party's tooling, not Pons v2's own contracts.

Items 2, 3, and 4 were already closed or left open by research 0047 §8 on
grounds this document's source read does not change.

## 8. Not established

- **The curve instance's exact bytecode is still not independently
  verified.** §1's live check found one selector (`graduate(address)`)
  present and consistent with the source; it did not diff the whole
  contract. Sourcify's `match: null` for
  `0x008089e243a611ace236fc4e2127403a3c9e347b` stands.
- **The library sources** (OpenZeppelin, Uniswap v4 interfaces) were
  deliberately not extracted, so anything that depends on their internals
  (the exact shape of `Ownable2Step`, the V4 pool math) is still read from
  the calling contract's usage, not the library itself.
- **`getReserves()`'s full return layout, and `launchedAt()`'s unit** —
  neither was traced through `PonsV2BondingCurveMath.sol` or
  `PonsV2GraduationMath.sol`, which were not read this session (present on
  disk, not opened).
- **The two unresolved event topics** from research 0047 §4
  (`0x5603e2fc...`, `0x982b660b...`, both emitted by `0x9689992f...`) — not
  addressed; that address's contract is not one of the 13 first-party files.
- **The Uniswap v4 pool key, and whether `locker()` actually locks the
  graduated position** — `PonsV2LaunchLocker.sol` is on disk but was not
  read this session.
- **Whether `MAX_SNIPE_TAX_EXEMPTIONS` (§3) is a number worth naming as a
  signal input** — its value was not read this session.

**Confidence: CHECKED**, on §2's five confirmations, §3, §4's correction, and
§5's code check — each carries a file and line from the verified factory's
own source, or a live call made today with its computation cross-checked
against five known-good values. **CONDITIONAL** on §1's curve-instance
caveat: the one live selector check is evidence for, not proof of, the live
curve running this exact compiled bytecode. **SPECULATION** only on §6's
recommendations themselves, unchanged from 0047 where this document did not
find new grounds to move them.
