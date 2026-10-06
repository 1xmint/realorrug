<!-- SPDX-License-Identifier: Apache-2.0 -->
# Clanker single-slot fee-preference stability

**Status:** implemented bounded stability check with a live quiet-block positive;
preference-change histories, conversions and representative live busy-block
qualification remain open. **Date:** 2026-10-06.

## Caller and evidence boundary

The historical collection reader now optionally checks the denomination requested
by the one recipient slot linked to the investigated deposit. This strengthens
F2 Compounding Intelligence and F5 Trust: the public dossier can distinguish a
requested asset from an observed payment or swap. It selects no launchpad, pairing,
allocation, provider or execution policy.

The caller is `crates/realorrug-onchain/src/evm_fee_receipts/collection.rs`; the
bounded check is `crates/realorrug-onchain/src/evm_fee_receipts/collection/preferences.rs`.
It requires a qualified quiet-block or ordered reward-configuration result from
[research 0075](0075-clanker-quiet-block-reward-configuration.md) or
[research 0076](0076-clanker-ordered-reward-configuration-history.md). Both admitted
scopes exclude all preference-change events under their existing source/provider
assumptions. An unknown future scope cannot silently reuse this property.

The selected slot must be within the reviewed 16-slot bound. Exactly two valid
pool currencies must contain the reported token once and a distinct paired asset.
Two historical `feePreferences(address,uint256)` calls read parent and closing
state. Exactly one ABI word and an equal supported enum are required. Two final
header reads must agree with the already qualified configuration checkpoints.
Four additional calls share the existing budget; admission requires five remaining
calls to reserve the outer final checkpoint. No added retry, provider fallback,
budget increase or money movement is introduced.

The retained [source metadata](data/0073-base/clanker-depositor-source.json)
defines `FeeIn` as Both (0), Paired (1) and Clanker (2). Zero is explicitly measured
and retained as both assets; absent data never defaults to zero. Metadata is only
partially explorer-verified, not independently rebuilt or audited. Stability
depends on the reviewed runtime/source semantics, checkpoint identity and provider
reporting of the block's events. It is not independent completeness proof.

Success has the separate scope `provider_single_slot_fee_preference_stability`
and retains slot, enum, mode, requested assets and both block/hash boundaries.
Failure retains `unavailable_single_slot_fee_preference` and its gap without
erasing qualified collection or reward-configuration evidence. An inadmissible
scope or insufficient call budget adds no preference result. Public wording
identifies the denomination preference and its limits. Unexamined slots,
preference changes, realized swaps, conversion correctness, shared PositionManager
balances, historical registration, backing and per-token revenue remain unresolved.
The credit's financial state remains `executed`; no treasury receipt is inferred.

## Live capture and retained failures

The [manual qualification](data/0077-base/clanker-fee-preference-qualification.json)
records five successful read-only calls on one fixed anonymous endpoint,
`https://base.drpc.org`: chain ID, two preference getters and two headers. For
reported token `0xad794ad19350a1755d90d53380102dc7213b8b07`, recipient slot zero
returns enum one at parent 52139194 and closing block 52139195. The paired asset
is WETH, `0x4200000000000000000000000000000000000006`.

The [request](data/0077-base/base-clanker-single-slot-fee-preference.request.json),
[capture](data/0077-base/base-clanker-single-slot-fee-preference.capture.json) and
[RPC trace](data/0077-base/clanker-fee-preference-rpc.json) rerun the known deposit.
The transaction observation completes in 31 calls with quiet-block reward and
single-slot preference stability. Received credit remains 29478578528827 WETH
base units, measured independently from the requested amount. The collection's
reported token still differs from the case token.

The complete trace records 46 attempted calls, including a reverted optional
token getter and a later liquidity read that exceeded the shared deadline. The
capture harness spaces `eth_call` by 800 ms; this is capture pacing only. A proxy
can record a response after the caller times out; that does not establish a
completed observation. The [offline review](data/0077-base/review/base-clanker-single-slot-fee-preference.review.md)
and [assessment](data/0077-base/review/base-clanker-single-slot-fee-preference.assessment.json)
retain partial `CantTell`, unchecked owner acceptance and SQLite checkpoint
`ffabf439fcd0fd964a947ea4b41beeba66bfa3964687d925334363bd0426ca98`.
Three replay tools are not the original RPC cost.

The [change discovery](data/0077-base/clanker-change-discovery.json) retains two
anonymous explorer queries for recipient/admin event signatures. Both returned
HTTP 403. No representative live busy-block collection was discovered; this
failure says nothing about whether historical changes exist. The ordered-history
positive fixtures remain synthetic. No model call, post, purchase, signing or
financial execution occurred.

## Validation and next work

Named regressions cover all three denominations, slots zero and fifteen, both
supported reward scopes, reversed currency ordering, exact RPC arguments,
final-call reservation, malformed/changed getter values, unsupported enums,
changed header hashes/numbers, unknown histories and insufficient budgets.
Caller tests preserve qualified collection evidence and unchanged financial
state when the optional preference read fails, and check public wording.
Temporarily removing parent/closing preference agreement makes the named refusal
regression fail by continuing past the changed-value boundary; restored checks
pass.
The fresh 31-call transaction observation replays exactly from its trace.
Older captures replay their recorded prefixes with capacity limited to 28 calls
for credits and 15 for claims; these are replay constraints, not the original
captures' configured capacities. They contain no preference responses.

Scoped strict Clippy, formatting and named documentation checks run locally;
full suites and mutation testing run on [PR 210](https://github.com/1xmint/realorrug/pull/210).
Next: representative live configuration-change evidence, preference-change
histories and conversion/shared PositionManager boundaries, then fee origin and
owner-reviewed replies before a treasury ledger. Wider windows, Flaunch
reconciliation, backing, native delivery, custody powers and measured launch
costs remain open. Base is preferred; launchpad, pairing and economics remain
unselected. Existing spending limits and model authority are unchanged.
