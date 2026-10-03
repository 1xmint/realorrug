<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0068 — Base fee routing and retained controls

**Date:** 2026-10-02. **Status:** positive bounded live captures and read-only
control simulations; platform selection and launch qualification remain open.
**Direction:** [VISION](../../VISION.md), [ROADMAP](../../ROADMAP.md).
**Comparison:** [research 0067](0067-base-launch-platform-comparison.md).

## What changed

The investigator now records normal Flaunch pool currencies, revenue-NFT
registration/ownership, the relevant escrow and bid-wall enabled state. The
current allowlisted Base hook resolves escrow per pool; the legacy hook uses
its global getter. Token, hook and NFT registration must agree before the
route becomes measured evidence. Malformed words, mismatched owners and
missing contracts produce a route gap. A failed optional route or distribution
read preserves the quote and appears in the public unresolved text. Derived
addresses become bounded investigation leads, without implying common control.

These are read-only snapshots. The production reader does not simulate setters,
infer arbitrary manager permissions or establish immutable routing. The
qualification simulations below are separate operator research. Every new
finding remains in the dossier even when the short deterministic reply selects
only its first measured clauses. Assessments remain partial `CantTell`.

## Captured Flaunch case

Discovery used the current revenue NFT's next token id and the three preceding
registrations, avoiding the previously unavailable discovery API:
[discovery responses](data/0068-base/flaunch-discovery.json). This is a small
convenience sample, not representative platform quality or endorsement.

The case token is `0xb28ad96e19b0cdbfe5705b5948223c74ac1f8149`.
[Initial quote](data/0068-base/base-flaunch-current-fees.capture.json) is retained
separately from the [extended capture](data/0068-base/base-flaunch-current-route.capture.json)
and [offline review](data/0068-base/review/base-flaunch-current-route.review.md).
The extended fee observation is anchored to Base block **52092673** and its
recorded hash; other tools have their own read points.

| Observation at that fee read | Limit |
|---|---|
| A 10000-unit post-referral input quotes 0 bid-wall, 9000 creator and 1000 protocol units. | Not a percentage of trade volume, paid receipts or our proposed allocation. |
| Pool assets are the case token and `0xb2000000000000000000008501b13360000cb2ec`. | No ETH/SOL denomination, wrapper backing or market value is assumed. |
| Revenue NFT `0x475a09618bfd00fa4cb03b8504e95b62075e6f7d`, id 2010, matches the token/hook; its owner is `0x8aa48dfe58f84d85d41706543432e5d8511e90da`. | Current custody; not original deployer identity or permanently fixed ownership. |
| Hook `0x588c683ecc450f8b2aadb13d7f63792b840425dc` resolves escrow `0x17fbf54d6d15ebff82eee77e616f701952d08bb4`; bid wall `0x0dae90b70f62ce3b1d5278f4763bd1f595d6a687` reports enabled. | Enabled does not mean funded or executed. This quote allocates nothing to it. |
| No matching distribution events returned for blocks 52090673–52092673 inclusive. | No claim about earlier receipts, all market activity or beneficiary payment. |

The live reader verified the read's block hash again after its calls. Public
RPC availability is not a production capacity promise. Initial Python requests
without a browser user agent received HTTP 403; the successful records name the
public endpoint. No paid RPC/model access was used.

## Retained powers: a material qualification finding

[Runtime identities](data/0068-base/flaunch-paired-identities.json) retain
chain responses, runtime Keccak hashes and Sourcify ABI/runtime comparisons for
the hook, NFT, bid wall and escrow at block **52092581**. Sourcify's on-chain
bytes matched the RPC bytes. Its hook/NFT match labels are `match`, and the bid
wall/escrow labels are `exact_match`; this is not an independent audit or a
local recompilation. [Source review manifest](data/0068-base/flaunch-source-review.json)
records the source response and reviewed file hashes. The
[matched hook source](https://sourcify.dev/server/v2/contract/8453/0x588c683ecc450f8b2aadb13d7f63792b840425dc?fields=sources)
allows the current revenue-NFT holder to change creator allocation and retains
hook-owner fee-distribution/calculator powers. This qualifies the earlier
general-documentation description of a fixed launch allocation: it must not
be advertised as immutable for this deployment.

[Control simulations](data/0068-base/flaunch-control-simulations.json), anchored
to block **52092760**, tested two concrete calls. Changing creator allocation
to 5000 and disabling the bid wall both returned success when simulated from
the measured NFT owner. Both reverted from a separate non-owner address.
These were `eth_call` simulations: no signature, broadcast, persisted state
change or spending occurred. They demonstrate retained control at that block;
they do not establish every permission, future behavior or a transaction receipt.

The failed legacy `feeEscrow()` attempt at the current hook remains in
[the earlier identity trace](data/0068-base/flaunch-identities.json). The
working current getter is `pairedTokenFeeEscrow(bytes32)`, corroborated by its
matched ABI and live response. Never replace an unavailable getter with zero.

## Clanker comparison

The [refreshed supported case](data/0068-base/base-clanker-fees-refresh.capture.json)
and [review](data/0068-base/review/base-clanker-fees-refresh.review.md) use token
`0x1bbf51f3742c13d67fa89c56d09a979d40c891f1`. At block **52092675**, registry
`0xe85a59c628f7d27878aceb4bf3b35733630083a9` identifies locker
`0x63d2dfea64b3433f4071a98665bcd7ca14d93496`. It reports one position and
10000 basis points of configured LP rewards to
`0x2a293c59e0c2bfdd5cc0312c7094be9cf6f9e164`, also the reward administrator.
That is the LP-reward denominator, not total trading fees. Claims, both reward
assets, recipient/admin change history and liquidity withdrawal rights remain
unresolved. This supported older case does not qualify the newer locker
refused in [research 0066](0066-the-investigator-and-library-qualification.md).
[Research 0069](0069-clanker-current-locker-and-wallet-receipts.md) subsequently
qualifies that additional locker, checks current administrator powers and
retains a separate wallet-level claim/delivery trace. It does not establish
complete per-token revenue attribution or permanent liquidity.

## Implication and next acceptance work

Flaunch can support F3/F4 connections, but a native bid wall and a launch-time
split do not prove a permanent tokenomics commitment. Clanker remains the
comparison for simpler fee-funded operations; its configuration is not proof
of realized runway. Neither provider is selected by these captures.

Next, trace actual fee accrual → claim → treasury receipt for each viable
generation, identify revenue custody and all relevant change/withdrawal powers,
and qualify thresholds, backing and execution effects. Obtain concrete launch
and operating cost quotes within the $200 startup cap. Only then choose the
launchpad, pairing and allocation, and derive the financial ledger. The 25%
burn / 25% permanent-liquidity proposal remains unadopted.

Named regressions cover both hook-specific escrow calls, exact block/target/
calldata, NFT registration/roles, boolean boundaries and partial-read gaps.
Scoped strict Clippy passes. Full suites and mutation checks run on
[PR 210](https://github.com/1xmint/realorrug/pull/210); their green status cannot
substitute for representative quality and owner acceptance. Offline reviews
retain custody checkpoints in a dedicated local SQLite store. No production
worker, publication, paid portal, new infrastructure or financial execution
was enabled.
