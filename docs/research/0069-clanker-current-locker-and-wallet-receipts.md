<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0069 — current Clanker locker and wallet receipts

**Date:** 2026-10-03. **Status:** bounded live qualification; launchpad selection,
complete revenue attribution and production ledger remain open.
**Direction:** [VISION](../../VISION.md), [ROADMAP](../../ROADMAP.md).
**Previous evidence:** [research 0068](0068-base-fee-routing-and-retained-controls.md).

## Implemented reader coverage

The Base Clanker fee reader now accepts the additional deployed locker
`0xffa37784d619f228d8b379d287a4d7282e500762`. It uses the existing bounded
reward-tuple decoder. Both reward-pool currencies become observations and
bounded leads. The pool must contain the case token, and its hook must match
the factory registration; disagreement refuses the configuration. Other
locker addresses remain unsupported. The serialized `clanker_v4_0` identifier
continues to identify the factory adapter, not every component's version.

The [pinned official SDK address map](https://github.com/clanker-devco/clanker-sdk/blob/4f4d2bbf41c7f10543559dc043c85f443a6d452e/src/utils/clankers.ts)
corroborates the locker and fee-locker addresses. The
[identity/discovery record](data/0069-base/clanker-identities-and-claim-discovery.json)
retains RPC responses, source URLs/hashes and relevant ABI entries at Base
block **52139773**. The new locker reports version `1.1`, factory
`0xe85a59c628f7d27878aceb4bf3b35733630083a9` and fee locker
`0xf3622742b1e446d92e45e22923ef11c2fcd55d68`.

Sourcify returned HTTP 404 for both Clanker addresses in this investigation.
Blockscout returned source/ABI with **partial verification**; its returned
deployed bytecode matches the RPC runtime bytes at the fixed block. No proxy
implementation was reported. This supports the narrow decoder extension,
not an independent audit, local recompilation, guarantee against every
upgrade mechanism or endorsement. Hashes identify the reviewed sources.

The [fresh capture](data/0069-base/base-clanker-current-route.capture.json)
and [offline review](data/0069-base/review/base-clanker-current-route.review.md)
revisit token `0xbc8a7388c6cba5ad74b24627f58f9bd3affd5b07`, previously refused
for its locker generation. It reports five positions, a pool containing the
token and `0x4200000000000000000000000000000000000006`, and 10000 basis points
of LP rewards to `0x9d390d90c8e4346f296b76fd366a5c6ce3beacd8`, also the reward
administrator. Read points and hashes are retained per tool. This denominator
excludes other fee bases. Five positions do not establish permanent liquidity.
The assessment remains partial `CantTell`; owner acceptance is unchecked.

## Current administrative powers

[Read-only simulations](data/0069-base/clanker-control-simulations.json) at
block **52139849** tested changing reward index zero's recipient, administrator
and conversion preference. All three calls succeeded when simulated from
the measured administrator and reverted from a separate non-administrator.
They used `eth_call`; no signature, broadcast, persisted change or spending
occurred. The fixed block's hash was checked before and after the reads.

The getter returned conversion preference `1`, named `Paired` by the reviewed
interface. Additional simulations at the same block tested values `0` (`Both`)
and `2` (`Clanker`): both succeeded from the administrator and reverted from
the non-administrator. These do not demonstrate a conversion or payout. Source inspection
also exposes owner token/ETH withdrawal functions; complete rights, position
custody and historical changes remain separate qualification work. The
production reader does not simulate setters or decode conversion preferences.
Current recipients and administrators must not be advertised as immutable.

## An actual wallet-level fee claim

The discovery queried only **2001 blocks**, 52137773–52139773 inclusive.
It returned two `ClaimTokens` events from the Clanker fee locker and no
`Withdrawal` events from the current Flaunch escrow in that window. Those
results say nothing about earlier activity or other escrow generations.

Publicnode returned HTTP 403 for the selected receipt; that failure remains
in the discovery record. A separate free Base RPC supplied the
[transaction, canonical block and receipt](data/0069-base/clanker-wallet-claim.json)
for `0xfe327960aff5839e02ef4a52f8bccdb90e0dfa3bd4278ed3a8ec7361be829c09`.
Its chain id is 8453, receipt status is successful, transaction identities
agree, and block **52139202** matches the receipt block hash.

The receipt contains a `ClaimTokens` event reporting **3440630801955 base
units** of asset `0x4200000000000000000000000000000000000006` for wallet
`0xce165ce10c2f1bac8bc6b1e4009e87b84ddd8eaa`. An ERC-20 `Transfer` event from
that asset reports the same amount, from the fee locker to that wallet, in
the same successful receipt. This is evidence of that wallet-level delivery,
not RealOrRug revenue, beneficial ownership or revenue from the case token.
No currency conversion or valuation is inferred.

The reviewed Clanker source accumulates balances by **fee owner and asset**,
without a pool key in the claim event. Flaunch's matched escrow source
similarly aggregates by **payee and escrow asset**: deposits carry a pool id,
withdrawals do not. Its withdrawal also distinguishes the escrow balance
asset from the delivered asset, which may be an underlying token or native
currency. A claim event alone cannot establish the underlying delivery.

## Ledger implications and next implementation

Keep configured allocations, accrued balances, successful claim events and
verified deliveries separate. Count a claim and its matching transfer as
one receipt, never two revenue entries. Attribute a pooled withdrawal to an
individual token only after reconciling contributing credits and balances;
otherwise display wallet-level revenue with attribution unresolved. Native
delivery needs additional supported evidence, not an ERC-20 assumption.

The current investigator transaction reader retains case-token Transfer
events, so this paired-asset receipt research is **not yet an automated
production ledger or protocol-claim decoder**. The next bounded slice should
recognize allowlisted claim layouts and their delivered assets, match exact
recipient/amount/asset/receipt identities, refuse removed, malformed,
duplicate or ambiguous logs, and retain missing/truncated coverage as unknown.
Then reconcile accrual, claims, custody and balances across explicit windows.

Named regressions check all three allowlisted lockers, exact target/calldata/
block, both token currency positions and refusal of wrong pool/hook identities.
Scoped strict Clippy and the capture/review CLI regression pass; full suites
and mutation checks run on [PR 210](https://github.com/1xmint/realorrug/pull/210).
Offline review/checkpoint verification used the existing SQLite foundation.
No model call, paid service, production worker, publication, launch transaction
or financial execution was enabled. Provider, pairing, costs and allocation
remain unresolved; the proposed 25% burn / 25% liquidity split is unadopted.
