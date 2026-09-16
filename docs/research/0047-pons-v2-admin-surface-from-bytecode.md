<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0047 — Pons v2 admin surface, from deployed bytecode

**Date:** 2026-09-15
**Status:** read-only research, captures supplied by the lead. This document
turns those captures into labelled claims; it does not re-run any RPC call
itself. Builds on [research 0038](0038-pons-v2-creators-and-outcomes-read-over-a-range.md),
[research 0039](0039-robinhood-chain-data-on-a-budget.md),
[research 0040](0040-pons-v2-graduation-price-and-other-launchpads.md) and
[research 0044](0044-liquidity-manipulation-and-sell-blocking.md), and answers
against [ADR 0027](../adr/0027-the-bot-gives-verdicts-it-can-prove.md)'s
verdict rule and `AGENTS.md` §1's evidence rule.

## 1. Method, and its limits

**Verified, per the lead, captured 2026-09-15 against the public RPC
`https://rpc.mainnet.chain.robinhood.com` (chain id 4663).** Deployed
bytecode was fetched with `eth_getCode`. Dispatch-table selectors were
extracted by pattern-matching the solc dispatcher shape `DUP1 PUSH4
<selector> EQ` (bytes `8063........14`), then each four-byte selector was
resolved against `https://api.openchain.xyz/signature-database/v1/lookup`
with `filter=true`. Event topics were resolved the same way. Scalars were
read with `eth_call`.

This document did not re-run any of these calls; every number below is
attributed to the lead's capture, not independently re-verified here.

**Limits, stated plainly, per `AGENTS.md` §1's "zero is a measurement about
your instrument":**

- The `8063........14` pattern finds only the standard solc dispatch table.
  A function reached by a different dispatcher (a fallback, a diamond-style
  proxy lookup, a hand-written jump table) would not be found this way. So
  **an absent selector is weaker evidence than a present one** — "no `mint`
  selector was found" means "not found by this pattern," not "cannot mint."
  It settles what may be said publicly today; it does not settle whether a
  path exists.
- A name returned by a signature database is a **preimage match on four
  bytes**, nothing more. `openchain.xyz` is matching the selector against a
  dictionary of known `keccak256(signature)[:4]` values; a collision (two
  different signatures sharing the same four bytes) is possible, and even a
  correct name match is not proof of the function's actual behaviour —
  `sweepFees(uint256)` being the openchain-resolved name for `0x3729bb9a`
  says a function with that signature string exists in some dictionary and
  that this selector equals its hash prefix, not what the function's body
  does.
- **No verified source was read for any contract in this document.**
  Blockscout's verified-source API (`robinhoodchain.blockscout.com`) sits
  behind a Cloudflare challenge that `curl` could not pass (confirmed
  separately in research 0039 §3, 2026-09-15, against the same explorer).
  Every behavioural claim below that goes beyond "this selector is in the
  dispatch table" or "this call returned this value" is **inferred**, not
  verified, and is labelled as such.

## 2. Curve `0x008089e243a611ace236fc4e2127403a3c9e347b`

**Verified** (44 selectors, all resolved by name against the dispatch
table):

| function | selector | label |
|---|---|---|
| `getReserves()` | `0x0902f1ac` | verified present |
| `quoteReserve()` | `0x9da771f4` | verified present |
| `tokenReserve()` | `0xcbcb3171` | verified present |
| `sweepFees(uint256)` | `0x3729bb9a` | verified present, name match only |
| `rescueFees()` | `0x52920587` | verified present, name match only |
| `quoteFeeBalance()` | `0xed479c47` | verified present |
| `creatorTaxBalance()` | `0xdb2bd533` | verified present |
| `feeEscrow()` | `0xc4b7de97` | verified present |
| `realQuoteReserve()` | `0x4f1f58fd` | verified present |
| `phantomQuote()` | `0xc57eadfc` | verified present |
| `exemptFromSnipeTax(address)` | `0x31ff7f22` | verified present |
| `snipeTaxExempt(address)` | `0xd44bdfe7` | verified present |
| `currentSnipeTaxBps(address)` | `0xd7e1ef39` | verified present |
| `snipeTaxStartBps()` | `0x50e25ac2` | verified present |
| `snipeTaxSeconds()` | `0x6783774b` | verified present |
| `setCreatorFeeRecipient(address)` | `0x7b04ea62` | verified present, only setter besides buyback |
| `setBuybackEnabled(bool)` | `0x9a9b567d` | verified present |
| `initialize(address)` | `0xc4d66de8` | verified present |
| `buy(uint256,uint256,address)` | `0x59a87bc1` | verified present |
| `sell(uint256,uint256,address)` | `0xd04c6983` | verified present |
| buyback machinery (`buybackEnabled()`, `buybackVault()`, `buybackQuoteBalance()`, `buybackBurnBps()`, `buybackCreatorRecipient()`) | see 0047 packet | verified present |
| graduation/reserve accounting (`factory()`, `token()`, `pairToken()`, `deployer()`, `graduationThreshold()`, `readyToGraduate()`, `graduated()`, `launchedAt()`, `launchSupply()`, `sellableTokens()`, `reservedTokens()`, `trackedTokens()`, `trackedQuote()`, `isNativeQuote()`, `maxInternalPriceImpactBps()`, `feePolicy()`, `feeBps()`, `protocolFeeShareBps()`, `protocolFeeRecipient()`, `creatorTaxBps()`) | see 0047 packet | verified present |

**Absent from the dispatch table (verified, with the instrument caveat
attached): no `mint`, no `pause`, no `blacklist`, no bare `withdraw()`.**
This is a reassuring finding — the standard rug-mechanic function names a
scanner would look for are not in the standard solc dispatcher — but per §1
above, absence here means "not found by the `8063........14` pattern," not
"provably impossible." A function reached by a non-standard dispatcher would
not show up in this scan.

**Fee movement outside the sell/graduation path (inferred, closes
[research 0044](0044-liquidity-manipulation-and-sell-blocking.md) §4 item 1
in part — quoting it exactly): "Whether the deployed Pons v2 curve contract
exposes any owner/admin function that can withdraw `quoteReserve`/
`tokenReserve` outside the normal sell or graduation path — no selector scan
has been run, here or in 0036/0038/0040 (table row 11, phase-notes §2)."**
The selector scan asked for by that item has now been run: `sweepFees(uint256)`
and `rescueFees()` are present, and the dispatch table tracks fees
(`quoteFeeBalance()`, `creatorTaxBalance()`, `feeEscrow()`) as separate
accounting from reserves (`realQuoteReserve()`, `phantomQuote()`). **This is
inferred, not verified: no source was read, so whether `sweepFees` or
`rescueFees` can actually reach the reserve balances — as opposed to only the
fee-tracked balances — is NOT settled.** 0044 §4 item 1 is therefore
**partly closed**: the scan exists now (closing the "no selector scan has
been run" half of the sentence); whether the functions found can move
reserve funds is still open (the underlying "can it withdraw reserves"
question).

**Snipe-tax exemption (inferred behaviour from names; verified presence):**
`exemptFromSnipeTax(address)`, `snipeTaxExempt(address)`,
`currentSnipeTaxBps(address)`, `snipeTaxStartBps()`, `snipeTaxSeconds()` are
present. Neither research 0042 nor 0044 named a snipe-tax exemption
mechanism before this capture.

## 3. Factory `0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e`

**Verified** (59 selectors, all resolved):

**Ownership.** `owner()`, `pendingOwner()`, `acceptOwnership()`,
`transferOwnership(address)`, `renounceOwnership()` are present —
**inferred** to be OpenZeppelin `Ownable2Step`-shaped from the selector set
matching that library's public interface; no source was read to confirm the
implementation is actually that library's code rather than a
selector-compatible rewrite.

`owner()` reads **`0x263ed295dafae1d9aadd6e56c4b6f9f38ee019dd`** (verified,
`eth_call`, 2026-09-15). That address holds 171 bytes of bytecode
**inferred to be a canonical Gnosis Safe proxy**: it returns storage slot 0
for selector `0xa619486e` (`masterCopy()`) and delegatecalls everything
else, a pattern compiled by solc 0.7.6. Reading through the delegatecall
(verified, `eth_call`, 2026-09-15): `VERSION()` = **"1.4.1"**,
`getThreshold()` = **2**, `getOwners()` = three addresses
(`0x1320a2b04a9e9ff511c7209c9669ebfe13cc818e`,
`0x3825e7b3ff17637b08219e99d78b6c622b73f5b7`,
`0xfa31fe751c203a623b52fad26b4063abc2ffdf50`). **Verified: the factory's
owner is a 2-of-3 Gnosis Safe multisig.** `pendingOwner()` reads the zero
address (verified) — no ownership transfer pending as of 2026-09-15.

**Admin surface (verified present, dispatch table):**
`rescueCurveFees(address)` `0x189eb0f5`, `forceSweptGraduation(address)`
`0x7aed273e`, `rescueSweptGraduation(address,address)` `0xdbcb9c76`,
`setWhitelistedLauncher(address,bool)` `0x366f0f3e`,
`setSnipeTaxStartBps(uint256)` `0xb20e51af`,
`setSnipeTaxSeconds(uint256)` `0xd1ec471a`,
`setMaxCreatorTaxBps(uint256)` `0x2260aead`,
`setLaunchEnabled(bool)` `0xf56f05b2`, `setLaunchFee(uint256)` `0x5313be2c`,
`setPairTokenApproved(address,bool)` `0x8763e3dc`,
`setPairTokenEconomics(address,uint256,uint256,uint8)` `0x092c08bd`,
`setGraduationExecutor(address)`, `setLaunchDeployer(address)`,
`setLaunchForwarder(address)`, `setBuybackEnabled(address,bool)`
`0xb18f1db1`, `addLaunchConfig(...)`, `updateLaunchConfig(...)`.

**Who may call these is inferred, not proven.** The factory's owner is the
2-of-3 Safe above, and OpenZeppelin-shaped `Ownable`/`Ownable2Step` gates a
function on `msg.sender == owner()` by convention — but no source was read
for the factory, so whether these specific functions actually carry an
`onlyOwner` modifier, a different role check, or no check at all beyond
name-suggests-admin is **NOT settled**.

Also present (verified): `canLaunch(address)`, `whitelistedLaunchers(address)`,
`approvedPairTokens(address)`, `pairTokenEconomics(address)`,
`getLaunchConfig(uint256)`, `getLaunchFeePolicy(address)`,
`previewLaunchEconomics(uint256,address)`, `createGraduatedPool(address)`,
`getLaunchedToken(address)` `0x3cf28b5a` (already used in
`crates/realorrug-robinhood/src/pons.rs`, per research 0038 §2).

**Creator-fee-recipient transfer is a timelocked two-step (verified
present):** `transferCreatorFeeRecipient(address,address)` `0x2931861b`,
`pendingCreatorFeeRecipient(address)` `0x9beacf4a`,
`executeCreatorFeeRecipientChange(address)` `0x3d3d2d58`,
`cancelCreatorFeeRecipientChange(address)` `0x6e47a188`. See §6 (Finding 7)
below.

### Live scalars, verified, all read 2026-09-15

| call | raw | decoded |
|---|---|---|
| `snipeTaxStartBps()` | `0x26ac` | **9,900 bps = 99%** |
| `snipeTaxSeconds()` | `0x03` | **3 seconds** |
| `maxCreatorTaxBps()` | `0x03e8` | **1,000 bps = 10%** |
| `launchFee()` | `0x1c6bf52634000` | **500,000,000,000,000 wei = 0.0005 ETH** |
| `launchEnabled()` | `0x01` | **true** |
| `launchConfigCount()` | `0x01` | **1** |
| `GRADUATION_RESCUE_DELAY()` | `0x093a80` | **604,800 s = 7 days** |
| `CREATOR_FEE_RECIPIENT_TIMELOCK()` | `0x03f480` | **259,200 s = 3 days** |
| `CREATOR_FEE_RECIPIENT_EXECUTION_WINDOW()` | `0x03f480` | **259,200 s = 3 days** |

`snipeTaxStartBps()` and `snipeTaxSeconds()` **independently confirm**
research 0044's snipe-tax figures (~9,900 bps decaying over ~3 seconds),
which 0044 obtained by a different route. Two independent measurements
agreeing is stronger than either alone, and is worth recording as such.

**These are today's readings, not constants** — every one of them is behind
a setter in the admin surface above (`setSnipeTaxStartBps`,
`setSnipeTaxSeconds`, `setMaxCreatorTaxBps`, `setLaunchFee`,
`addLaunchConfig`). See §5 signal candidate 3.

### Named first-party addresses — research 0045's exclusion list

**Verified, all read 2026-09-15.** These are the addresses [research
0045](0045-wallet-analysis-and-clustering.md) calls its named first-party
exclusion list. Per that document's own framing, **this list must be applied
before any numeric floor** — a heuristic that flags "top holder owns >X%" or
"wallet funded the deployer" must exclude these addresses first, or it will
flag the protocol's own infrastructure as suspicious.

| role | address |
|---|---|
| `graduationExecutor()` | `0xc7819b64a1daecd7ec19856d026cb14efbd89046` |
| `graduationGuard()` | `0xf5695117b99b6f6401e67d4195bd653628176c6c` |
| `buybackVault()` | `0x42df2a798f82289e177311362e8f5ccc45c1219c` |
| `feeEscrow()` | `0xd3afeb2a57f70ef218aa82451c51b2fb0416ac9e` |
| `locker()` | `0x267444d099b10fb5ed7c3cc7b7c767adca574952` |
| `launchDeployer()` | `0x3711cea4feade896c913c68f01eda97cb06d1a42` |
| `launchForwarder()` | `0xe33e9e479df8802cb0866d5d05258bec4cf62948` |
| `memeHook()` | `0xe5e702641ea86f4ae6cc3cdaed2b886f976be044` |
| `poolManager()` | `0x8366a39cc670b4001a1121b8f6a443a643e40951` |
| `positionManager()` | `0x58daec3116aae6d93017baaea7749052e8a04fa7` |
| `permit2()` | `0x000000000022d473030f116ddee9f6b43ac78ba3` |

`permit2()` matches the well-known canonical Uniswap Permit2 deployment
address — verified as a byte-for-byte match, not independently confirmed
against Uniswap's own registry this session.

`launchDeployer` has nonce **1,013,875** (`0xf7473`, verified, `eth_getCode`/
nonce read 2026-09-15) — **inferred** to be consistent with it being the
contract that deploys each token and curve (a high nonce is consistent with,
not proof of, that role; no source confirms the role directly).

**`graduationExecutor` is inferred to be a Uniswap v4 position minter, from
its own dispatch table (verified: three selectors present):**
`mintFullRangePosition(...)` `0xcbba1910`, `positionManager()`, and
`factory()` `0xc45a0155`. Combined with `poolManager()`/`positionManager()`/
`memeHook()` above, the post-graduation venue is **inferred to be Uniswap v4
with a hook**, not a v2/v3 pool — no pool contract's own state was read to
confirm this directly.

This **partly closes** [research 0044](0044-liquidity-manipulation-and-sell-blocking.md)
§4 item 3, quoted exactly: **"The actual post-graduation pool/PoolManager
contract address, its pool key, and any reserves or `Burn`/`Sync`-equivalent
log target — 0040 §3/§6, unchanged; blocks table row 10 from being run at
all today."** What is now known: the PoolManager and PositionManager
addresses, a full-range-position minting pattern, and a hook contract. What
is still not known: the pool key, the pool id, whether `locker()` actually
locks the graduated position, and any direct reserves/price read against the
pool itself.

## 4. The graduation flow, decoded from a real transaction

**Verified.** Receipt of
`0x7352d0f7e3a0e9ce43a810aba7eaf9519da370ebc83d15d1d58cea0eb75e16bf` (the
same transaction [research 0040](0040-pons-v2-graduation-price-and-other-launchpads.md)
§2 decoded), 13 logs, event topics resolved against openchain.xyz:

| topic0 | event |
|---|---|
| `0xcdb72f157fd3666758a6ce201387ffb52038c7562e4fff352828da1096c4b6b4` | `LaunchSwept(address,uint256,uint256)` — factory, token indexed |
| `0xf8d37a90738ae063b8b8058b66f5880cf3cf7ab0c5d4fa78219696591dfbfb67` | `CurveCompleted(address,uint256,uint256)` — curve, nothing indexed |
| `0xec36bf571f136799e8dc0b0b8bea4b04d8bd3d43de838aab0d5fc21d4cbfc455` | `CurveBuy(address,address,uint256,uint256,uint256,uint256)` |
| `0xa69e8258ccc7b9bbb70ab953fc2d1062b4ee28b8ca827534097e1732e87b0262` | `CurveBuyRefunded(address,uint256)` |
| `0x9f4cd7c4ed99d08a797804560c9c5d71d2cf7e101f2e3b5e7d1ca8a24c370e4f` | `FeesSwept(uint256,uint256,uint256)` |
| `0x4e45da441832cf53bdaa69235704fc0575e68210f459ee1562911024b12967d5` | `Credited(address,address,uint256)` — the fee escrow |
| `0x393c1cce9705f5fbf6aa25e4ada933289650f18a8e6d54aec98dbf555f8f128e` | `Deposit(address,bytes)` |

As with §1, these are preimage matches on the topic hash, not proof the
emitting contract's ABI matches the resolved signature exactly.

**Two topics did not resolve and are unknown, not guessed:**
`0x5603e2fc9937b376c2db1cb5e22c4c014732266ee2d1a5976fc7cb90689e6756` and
`0x982b660b9985e587951b1aedb098a83fb5db903099865476bba451abb83416c8`, both
emitted by `0x9689992f5b5c09447f15906d8d11214944488341`.

**The fee escrow credits, it does not transfer (verified from the same
receipt).** Two `Credited` events fire: one to the owner Safe
`0x263ed295dafae1d9aadd6e56c4b6f9f38ee019dd` (the protocol's share) and one
to `0x10fc5d7168b6facb12d04c79f3c8fe7d4b6642b7` (that launch's creator fee
recipient). **This is a pull-payment ledger, not a push transfer.** A
creator's realised fee income is therefore **not visible as a transfer at
graduation** — a wallet that credits but is never observed to withdraw has
not been shown to have taken any money out, and any signal that reads
"creator received fees at graduation" as "creator cashed out" would be
wrong on this contract's own accounting shape.

## 5. The correction — 0044's EOA question, answered the other way

[Research 0044](0044-liquidity-manipulation-and-sell-blocking.md) §4 item 2
asks, quoted exactly: **"What the EOA `0xe4c6b769...`, which receives a
token allocation via a beacon proxy on every graduation, actually is
(protocol treasury, pool seed, team allocation) — flagged unresolved in
0040 §2, unchanged."** [Research 0040](0040-pons-v2-graduation-price-and-other-launchpads.md)
§2 additionally called `0x9689992f...` an "EIP-1967 **beacon proxy**".

**Verified, from the same receipt's `from` field: the address
`0xe4c6b76911f1eab045dfb4e68fa6861e942da4db` sent the graduation
transaction.** It bought through the contract `0x9689992f...`, its buy
crossed the graduation threshold (hence `CurveBuyRefunded` firing — it was
refunded the portion of its buy that overshot), the curve completed, and the
tokens it had just bought were forwarded to it. **It is the buyer whose
purchase graduated the token, receiving the tokens it paid for — not a
protocol treasury, pool seed, or team allocation.** `eth_getCode` returns
`0x` (verified: an EOA, not a contract) and its nonce is **13,968**
(`0x3690`), **inferred** to be consistent with a trading bot given the
transaction volume that nonce implies, not proof of what the wallet is.

**And it is not a constant address.** `eth_getLogs` for `LaunchSwept` on the
factory over blocks `0x3b60000`–`0x3b6ffff` returned **22 graduations**
(verified); four others sampled each had a **different sender and a
different entry contract**: `0xd60ac8ee...`→`0xcdc264b8...`,
`0x33c4c974...`→`0x65050a9b...`, `0xc0b3535e...`→`0x387a453d...`,
`0x01ca0823...`→`0xef161b8b...`. The three entry contracts have bytecode
lengths of 20,458, 1,504 and 9,042 hex characters — **no single router
pattern**; buyers reach the curve by many different routes.

**Correcting 0040/0044's "EIP-1967 beacon proxy" label rather than
substituting a new guess:** `0x9689992f...`'s EIP-1967 beacon storage slot
(`0xa3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50`) reads
**zero** (verified), and its code is 100 bytes with no dispatch table
(verified). The accurate statement is **a small delegating contract of
unidentified pattern** — not an EIP-1967 beacon proxy, since the beacon slot
that pattern requires is empty.

**So: the second graduation flow is normal and expected, and the
"unexplained wallet" framing in 0040/0044 was wrong.** Saying this plainly,
once, per `AGENTS.md` §1: 0040 §2 and 0044 §4 item 2 both flagged this EOA as
an open question worth caution around; it turns out to be the ordinary
mechanics of "whoever's buy crosses the threshold receives their purchase
and the code path that does that runs through a small forwarding contract."
What this correction opens rather than closes: **the identity of the
graduating buyer is now a readable fact** (the receipt's `from` field, on
every graduation, at the cost of one receipt fetch), and **a graduating buy
that traces back to the creator, or to a wallet the creator funded, is a
different and real question** this document does not answer — see §7
signal candidate 4.

## 6. Finding 7 — `creator_fee_recipient` is mutable

**Verified: `creator_fee_recipient` can change after launch, by two
different paths.** On the factory, a timelocked two-step
(`transferCreatorFeeRecipient` → 3-day `CREATOR_FEE_RECIPIENT_TIMELOCK` →
`executeCreatorFeeRecipientChange`, with a 3-day
`CREATOR_FEE_RECIPIENT_EXECUTION_WINDOW` to execute in and a
`cancelCreatorFeeRecipientChange` escape hatch — §3 above). On the curve
itself, directly: `setCreatorFeeRecipient(address)` `0x7b04ea62` (§2 above),
with no timelock observed in the curve's own dispatch table (its absence is
subject to the same instrument caveat as §1 — not proven impossible, only
not found by this scan).

[Research 0038](0038-pons-v2-creators-and-outcomes-read-over-a-range.md) §2
decided, quoted exactly: **"Recommendation: key the index by
`creator_fee_recipient`, not by `deployer`... a payout-relevant 'creator' is
the fee recipient."** That decision rested on a 13-launch sample where
`deployer` and `creator_fee_recipient` differed in 7.7% of cases at launch
time; it did not account for the recipient changing *after* launch, which
this document's capture now shows is possible by two on-chain paths.

**What breaks, stated exactly:** a repeat launcher who rotates their fee
recipient (e.g., moving from a personal wallet to a team multisig) will
split into **two separate creator histories** under a
`creator_fee_recipient`-keyed index — their earlier launches indexed under
the old recipient, their later ones under the new — even though one person
or team launched all of them. Conversely, **two launches that share a
`creator_fee_recipient` today may not have shared one at launch** — a
lookup keyed on the current value cannot tell whether a shared recipient was
true from day one or is the result of one launch's creator rotating onto an
address that happened to match another launch's original recipient (which
would be a false-positive shared-history signal, not a true one).

**Recommendation: key on both `deployer()` and `creator_fee_recipient`,
not on `creator_fee_recipient` alone.** `deployer()` is immutable (no setter
for it was found in either contract's dispatch table) and is the address
that chose to launch, which `check_launch` already compares against
snipe-tax exemptions per the original 0038 §2 reasoning — it does not drift.
`creator_fee_recipient` is the address Pons v2 actually pays and should stay
in the index for that reason, but tracking its change history (a
`pendingCreatorFeeRecipient` observed non-zero, or an
`executeCreatorFeeRecipientChange` event, timestamped) alongside the
immutable deployer lets a "this creator's other tokens" query use the stable
key while still surfacing "and this launch's payout address changed on
`<date>`" as its own fact. The one-sentence tradeoff: keying on the mutable
recipient alone is simpler and matches what 0038 already decided, but it
silently fragments or merges creator histories exactly when a creator's
payout setup changes, which is precisely when a reader most wants continuity
in the record. **This does not silently reverse 0038's decision — it is a
recommendation, and it needs the owner's call**, since 0038's key is already
merged and in use.

## 7. Signal candidates

Costs use [research 0039](0039-robinhood-chain-data-on-a-budget.md) §2's own
Alchemy figures: `eth_call` = 26 CU, `eth_getLogs` = 60 CU,
`eth_getTransactionReceipt` = 20 CU.

### 1. `snipeTaxExempt(address)` on the creator and on top holders

**Signal:** an address that can buy or sell without the snipe tax has an
advantage no other buyer has in the first `snipeTaxSeconds()` (3 s,
verified live reading) after launch.

**Innocent twin, named and real, using the exclusion-list table in §3:**
`launchForwarder`, `buybackVault`, `graduationExecutor`, and any router
contract could all be legitimately exempt — a router that batches many
users' buys through one contract address would need the exemption to avoid
taxing every user routed through it, and the buyback vault trading against
the curve is protocol mechanics, not sniping. The exclusion-list table in
§3 is exactly how this is handled: check exemption against a named
first-party address before treating it as a signal at all.

**Unconfirmed possibility worth naming:** `launchToken` takes an
`address[]` parameter (selector `0xa72101af`; `launchTokenFor` similarly,
`0xd6a0eef5`), and its meaning — is it the exemption set for that specific
launch, chosen by the launcher? a pre-approved router list? something
else? — **is not established** by this capture; no source was read and no
call was made against it. **One decoded `launchToken` transaction that
shows a non-empty `address[]` argument, cross-referenced against which of
those addresses later reads `snipeTaxExempt == true` on that token's curve,
would settle it** — a single transaction decode, not a new RPC method.

**Cost:** `snipeTaxExempt(address)` is one `eth_call` (26 CU) per address
checked. Checking the creator plus, say, the top 5 holders is 6 calls
(156 CU) per token, in addition to whatever holder-discovery cost (0040 §4:
a `Transfer`-log sum, one `eth_getLogs` call, 60 CU) is already paid.

**Recommendation: build now, gated by the exclusion list.** The call is
cheap (26 CU), the exclusion list already exists (§3), and a creator or a
funded wallet holding a snipe-tax exemption that is *not* on the
first-party list is a concrete, checkable fact. It must never reach ADR
0027's top two verdict levels alone — a single signal never does, per ADR
0027 rule 4 — and the reply must carry the router/vault innocent twin by
name whenever the exempted address is not creator-linked.

### 2. `pendingCreatorFeeRecipient(token)` non-zero

**Signal:** a creator is partway through the 3-day timelock (verified live
reading) to move their fee stream to a new address.

**Innocent twin:** a team moving its payout to a multisig, or a routine key
rotation — both indistinguishable on-chain from a creator quietly
repositioning before an exit, which is exactly why ADR 0027 requires an
innocent twin before this counts as a signal at all.

**Cost:** one `eth_call` (26 CU) per token, per check. Cheap enough to check
on every reply per research 0039 §"Replies" line, or on the seven-days-later
check.

**Recommendation: build now, as a `Sketchy`-level contributor only, never
alone.** Pair it with §6's finding that this same mechanism is why the
creator index should track `deployer()` as the stable key — a pending
change is the moment the mutable key is about to drift, so this signal and
the indexing fix in §6 are the same underlying fact viewed two ways.

### 3. Platform-wide thresholds settable at runtime

**Signal:** `setSnipeTaxStartBps`, `setSnipeTaxSeconds`,
`setMaxCreatorTaxBps`, `setLaunchFee`, `addLaunchConfig` are all present on
the factory (§3, verified), and today's readings (9,900 bps / 3 s / 1,000
bps / 0.0005 ETH / 1 config) are **live values, not constants**. [Research
0042](0042-detection-intelligence-we-left-in-radar.md)'s port-order item 2 —
a drift alarm on any threshold — applies to exactly these five values.

**Innocent twin:** the owner Safe tuning parameters is ordinary protocol
operation (raising a fee, adding a launch config for a new pair token) —
the same action shape as a rug-adjacent change (loosening the snipe tax to
favor an insider, or adding a launch config with worse economics for future
launchers). The twin here is "is this a governance action with no
individual victim, or a change that benefits one party" — not resolvable
from the threshold value alone.

**Recommendation: build now, as a drift alarm, not a per-token signal.**
Re-read the nine scalars in §3 on a fixed cadence — hourly is one
`eth_call` (26 CU) per scalar, 9 calls/hour, ≈6,500 calls/month, negligible
against research 0039's ≈1.19M/month baseline — and diff against the last
read. On a change: log it, and surface it as a platform-level note (never a
verdict on an individual token, since it did not act on any one launch) with
the old and new values and the block the change happened at. This is
`AGENTS.md` §1's "check a number before deciding on it" applied
continuously rather than once.

### 4. The graduating buyer's identity versus the creator

**Signal, established by §5:** the receipt of any graduation transaction
names its `from` address, at the cost of one `eth_getTransactionReceipt`
(20 CU) if the transaction hash is already known from the `LaunchSwept` log,
or one `eth_getLogs` (60 CU) to find it. A graduating buy sent by the
token's own `deployer()` or `creator_fee_recipient()`, or by a wallet that
received its starting balance directly from one of those addresses, is a
concrete, checkable fact that §5 shows was previously being read as an
unexplained wallet and is now nameable.

**Innocent twin:** a creator buying their own token at graduation is not
itself wrongdoing — a creator supporting their own launch, or simply being
one of many buyers whose purchase happened to cross the threshold, looks
identical on-chain to a creator manufacturing their own graduation. The
twin only breaks in the creator's favor when combined with other evidence
(e.g., whether the same wallet later sold into the graduated pool), which
this document does not capture.

**Cost:** 1 `eth_getLogs` (60 CU) to find `LaunchSwept` for a token, plus 1
`eth_getTransactionReceipt` (20 CU) to read `from` — 80 CU per token,
one-time per graduation, not recurring.

**Recommendation: build later, not now.** The mechanism is cheap and
proven (§5), but it needs a second, unbuilt piece — matching the buyer
address against the creator or a wallet the creator funded — to be more
than "here is who bought last." Funding-tracing is exactly research 0045's
wallet-clustering territory, not scanned here; wire this signal up once that
match exists rather than shipping a half-signal that names an address
without context.

### 5. Admin-controlled fee sweeps (`sweepFees`/`rescueFees`/`rescueCurveFees`)

**Signal:** §2 shows fee-sweep functions exist on the curve and §3 shows
`rescueCurveFees(address)` exists on the factory, gated (inferred, not
proven — see §3) by the 2-of-3 Safe.

**Innocent twin:** routine protocol fee collection by the owner is the
expected, designed use of these functions — this is not evidence of
anything wrong by itself, and is the "reassuring absence" framing from §2
cutting the other way: **presence** of a sweep function, used normally, is
not a signal at all.

**Recommendation: refuse as a per-token signal; watch as a platform event
only.** No per-token call cheaply distinguishes "the owner swept accrued
protocol fees" from "the owner swept a specific token's reserves" without
decoding the sweep transaction's arguments and comparing the amount against
that token's known fee balance — work not done in this capture. If ever
built, it belongs at the `eth_getLogs`-on-the-factory level (watch for
`FeesSwept`/`Credited` amounts inconsistent with `quoteFeeBalance()`), not
as a general "an admin function exists" flag, since §1's instrument caveat
means the absence of blacklist/mint/pause selectors already establishes
these are the *only* concerning functions found, and treating their mere
existence as a signal would flag correct, designed protocol behaviour on
every token.

## 8. Which of 0044 §4's "Not established" items this closes

Quoting [research 0044](0044-liquidity-manipulation-and-sell-blocking.md)
§4 exactly:

- **Item 1, partly closed.** "Whether the deployed Pons v2 curve contract
  exposes any owner/admin function that can withdraw `quoteReserve`/
  `tokenReserve` outside the normal sell or graduation path — no selector
  scan has been run..." **The scan has now been run** (§2): `sweepFees`,
  `rescueFees` exist, fees are tracked separately from reserves in the
  dispatch table. **Still open:** whether either function can actually reach
  reserve balances — no source was read.
- **Item 2, closed the other way.** "What the EOA `0xe4c6b769...`...
  actually is (protocol treasury, pool seed, team allocation)." **Answered:
  it is the buyer whose purchase graduated the token** (§5), not a protocol
  wallet — the opposite of what the caution in 0040/0044 implied was worth
  checking for.
- **Item 3, partly closed.** "The actual post-graduation pool/PoolManager
  contract address, its pool key, and any reserves or `Burn`/`Sync`-
  equivalent log target." **Now known:** PoolManager and PositionManager
  addresses, a full-range-position-minting pattern, a hook contract (§3).
  **Still open:** the pool key, the pool id, whether `locker()` locks the
  position, any direct reserves read.
- **Item 4, left open.** "Whether Unicrypt/UNCX Network or Team Finance (or
  any locker vendor) has deployed on Robinhood Chain (chain id 4663) — not
  checked this session." Not addressed by this capture.
- **Item 5, left open.** "The actual detection mechanism behind GoPlus's
  `hidden_owner`, `can_take_back_ownership`, `is_blacklisted` and De.Fi's
  admin-function scan..." Not addressed by this capture.

## 9. Not established

- **No verified source was read for any contract in this document** — every
  behavioural claim beyond "this selector/topic/value is present" is
  inferred from a name match or a field shape, not from source code.
- **Who may call `sweepFees`, `rescueFees` (curve) and `rescueCurveFees`
  (factory) is inferred from the factory's ownership shape (the 2-of-3
  Safe), not proven.** No source confirms an `onlyOwner`-equivalent
  modifier actually gates these specific functions.
- **The meaning of `launchToken`'s and `launchTokenFor`'s `address[]`
  parameter** — named in §7 candidate 1 as an open, one-transaction-away
  question.
- **The two unresolved event topics** from the graduation receipt
  (`0x5603e2fc...`, `0x982b660b...`), both emitted by `0x9689992f...`.
- **`getReserves()`'s return layout** — its selector matches the Uniswap V2
  `getReserves()` four-byte hash and research 0040 §3 read two of its return
  words against `quoteReserve()`/`tokenReserve()` separately, but the exact
  tuple shape (whether it also returns a timestamp word, as Uniswap V2's
  does) was not captured this session.
- **`launchedAt()`'s unit** — present in the curve's dispatch table (§2);
  neither its value nor its unit (block number vs. Unix timestamp) was read
  this session.
- **The Uniswap v4 pool key, and whether `locker()` actually locks the
  graduated position** — named in §3 and §8 item 3.
- **Whether the 22-graduation block range sampled in §5 is representative**
  of graduation traffic generally — it is one 65,536-block window, not a
  statistically chosen sample, and the "no single router pattern" finding
  rests on four transactions within it.

**Confidence: CHECKED**, on every claim in §§2–6 that is stated as verified
— each rests on a specific `eth_getCode`/`eth_call`/`eth_getLogs` capture
with a block, address or transaction hash attached, per the lead's method
in §1. **CONDITIONAL** on the inferred claims (Ownable2Step shape, Uniswap
v4 identification, "trading bot" framing, who may call the admin
functions) — each is conditional on no source having been read, and each
says so at the point it is made. **SPECULATION** only on the §7 signal
candidates' recommendations themselves — build-now/build-later/refuse is
this document's judgement call, not a captured fact, and a future capture
that reads source code or decodes a `launchToken` transaction could change
candidate 1's recommendation specifically.
