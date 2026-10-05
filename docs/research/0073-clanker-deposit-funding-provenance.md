<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0073 — Clanker deposit funding provenance

**Date:** 2026-10-04. **Status:** bounded receipt ingress matching implemented;
live capture, historical depositor qualification and offline review retained.
Automatic pool attribution, backing, owner acceptance and ledger remain open.
**Direction:** [VISION](../../VISION.md), [ROADMAP](../../ROADMAP.md).
**Previous evidence:** [research 0072](0072-clanker-transaction-balance-windows.md).

**Subsequent implementation:** [research 0074](0074-clanker-historical-collection-context-and-replies.md)
adds bounded automatic historical collection context and deposit-specific
reply prioritization. The capture and open status below describe this earlier
slice; context still does not settle per-token revenue attribution.

## Caller and evidence boundary

The successful canonical Base transaction reader now attaches `funding` to its
reviewed fee credits. This uses the already admitted receipt, with no additional
RPC calls, budget increase, model argument or financial action. The Clanker
fee locker source reviewed in [research 0069](0069-clanker-current-locker-and-wallet-receipts.md)
uses a transfer from the depositor and a before/after balance measurement;
`StoreTokens` reports the requested amount and new cumulative owner balance.

A match requires one preceding ERC-20 Transfer with the credit's exact asset,
depositor, escrow and **requested amount**. Execution order uses checked
block-global log indices, not array order. The same transfer cannot establish
funding for several credits. Complete receipt coverage, at most 128 logs and
unique canonical log checkpoints are required. Malformed transfers of that
asset, missing or repeated candidates, ambiguous reuse, zero/native requests
and unsupported Flaunch funding retain an explicit gap.

A matched result has `verification_scope: receipt_requested_transfer_match`,
the transfer's index, asset, sender, receiver, amount and transaction identity.
The credit remains `financial_state: executed` and `credit_event_only`, with
an unknown received delta in that event. Matching a reported transfer does not
measure net credit, establish asset backing or attribute a shared wallet's
revenue to the case token. Custom/fee-on-transfer semantics can disagree with
the requested amount; this bounded matcher refuses rather than fabricating
delivery. An independent balance window may separately measure received credit.
The credit, its transfer and its balance change are **one deposit**, not three
income entries. A claim and withdrawal remain separate from deposit evidence.

## Live capture and review

[Request](data/0073-base/base-clanker-deposit-funding.request.json) and
[capture](data/0073-base/base-clanker-deposit-funding.capture.json) use the
existing investigator against a single anonymous upstream, `https://mainnet.base.org`,
through the same temporary read-only recording harness as research 0072.
[All 27 RPC calls](data/0073-base/clanker-deposit-funding-rpc.json) are retained.
The transaction read succeeds in 14 calls, including its balance window and
canonical rechecks. The later token and liquidity reads encounter HTTP 429;
these failures remain gaps, without retries, fallback or inferred zeroes.
The recorder supplies a User-Agent; this does not qualify every direct runtime
transport or anonymous endpoint. It was stopped after capture.

Transaction `0x362fbb0a65298739c72e82ec9ba4c335ce9a36136b9818cfcc0eda54be4db46f`
at block **52139195** records these distinct observations, in base units:

| Evidence | Log | Meaning |
|---|---:|---|
| Transfer into LP locker | 104 | Asset `0x4200000000000000000000000000000000000006`, amount 29478578528827, from `0x498581ff718922c3f8e6a244956af099b2652b2b` to `0x63d2dfea64b3433f4071a98665bcd7ca14d93496` |
| Transfer into fee locker | 106 | Same asset/amount, from that LP locker to `0xf3622742b1e446d92e45e22923ef11c2fcd55d68`; production ingress match |
| StoreTokens | 107 | Owner `0x8b4eb0cd07f398357657369a684e6e0685a76ae2`, requested 29478578528827, cumulative balance 69398738209465 |
| ClaimedRewards | 108 | LP locker event reports token `0xad794ad19350a1755d90d53380102dc7213b8b07`; exploratory context, not an automatic credit-to-pool attribution |

The separate window measures opening balance 39920159680638 and received credit
29478578528827. Agreement with the requested transfer in this case does not
establish a rule that all requests equal net receipts.

[Offline review](data/0073-base/review/base-clanker-deposit-funding.review.md)
and [assessment](data/0073-base/review/base-clanker-deposit-funding.assessment.json)
retain a durable checkpoint, partial `CantTell` and unchecked owner acceptance.
The dossier retains the ingress proof, but the current short fallback reply
selects the transaction/balance statements ahead of this ingress statement.
Claim-specific reply prioritization remains subsequent work; a captured proof
must not be represented as an accepted public response. No model call, posting,
payment, signing or financial execution occurred.

## Upstream qualification and the token mismatch

[Explorer source and ABI metadata](data/0073-base/clanker-depositor-source.json)
identify the depositor as `ClankerLpLockerFeeConversion`. This is **partial
verification**, not an independent recompile or audit. Seven read-only calls to
one separate endpoint, `https://base.drpc.org`, qualify its historical deployment
at the same transaction block in
[the retained trace](data/0073-base/clanker-depositor-qualification.json).
Runtime bytes agree with the explorer metadata; before/after block headers agree
with the transaction's block hash. Historical getters report the reviewed fee
locker and the PoolManager/PositionManager addresses listed in the
[official Uniswap Base deployment map](https://developers.uniswap.org/docs/protocols/v4/deployments).
No proxy is reported by the explorer; this is not a general immutability claim.

The historical `tokenRewards(address)` getter for the event-reported token
returns that token, its two-currency pool key, position information and the
observed credit owner as reward recipient. Hashing the five ABI pool-key words
derives `0x787392934b75093f7e79e384410b4115b35265f72210020b43c9239d019a8992`,
matching the pool IDs in the receipt's PoolManager events. The source collects
fees through zero-liquidity position actions, measures LP-locker asset changes,
distributes according to recipient configuration and emits `ClaimedRewards`.
The event signature and ABI, runtime comparison, raw getter words and pool-ID
derivation are retained rather than inferred solely from event proximity.

This context concerns **another token** than the dossier's
`0xbc8a7388c6cba5ad74b24627f58f9bd3affd5b07`. Neither a pooled wallet movement,
a transaction sender, a requested amount nor an adjacent pool event establishes
the investigated token generated this deposit. Production therefore keeps
`pool_attribution: unresolved`. A future attribution adapter must qualify the
specific historical factory/locker registration, pool currencies/hook,
position/collection linkage, recipient slot, conversion path and event sequence,
including multiple collections or deposits in one transaction. Provider state
at block end alone does not establish configuration at every intra-block event.

## Validation and next slice

Named regressions exercise exact roles/amount/order, ambiguous matches/reuse,
canonical identities, malformed evidence, the 128-log boundary and unsupported
funding. A retained receipt regression keeps the deposit distinct from token
revenue. The integration regression requires exact RPC arguments, 14 calls and
identical serialized current observations for this new capture and the unchanged
research 0072 claim capture. Historical 0072 credit evidence remains preserved;
it describes the reader before the additive funding field and statement.
Scoped Clippy, named tests and documentation checks run locally; full suites
and mutation shards run on [PR 210](https://github.com/1xmint/realorrug/pull/210).
Manually substituting the cumulative balance for the requested amount makes
the named funding regression fail; restoring the source makes it pass.

Next: qualify a bounded historical pool/collection attribution scenario and
make deposit-specific evidence surface in the short reply before designing a
treasury ledger projection. Backing, native/conversion delivery, wider windows,
Flaunch balance/funding support, custody powers and measured launch costs remain
open. Base is preferred; launchpad, pairing and financial allocations remain
unselected. Spending authority and runtime limits are unchanged.
