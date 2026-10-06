<!-- SPDX-License-Identifier: Apache-2.0 -->
# Clanker ordered reward-configuration history

**Status:** implemented bounded recipient/admin reconstruction; synthetic busy-block
qualification and live quiet-block regression retained. A representative live
busy-block collection is still required. **Date:** 2026-10-05.

## Caller and scope

The Base transaction investigator's historical collection reader now reconstructs
the reward tuple used during a collection when a single reviewed LP locker reports
supported recipient/admin updates in other transactions in that same block.
This strengthens F2 Compounding Intelligence and F5 Trust: dated control changes
become retained evidence, and a later recipient cannot silently replace the
recipient used by the investigated deposit. It does not add a ledger or select a
launchpad, pairing, allocation, RPC provider or payment rail.

This extends [research 0075](0075-clanker-quiet-block-reward-configuration.md).
Its historical quiet-block captures remain valid. The caller remains
`crates/realorrug-onchain/src/evm_fee_receipts/collection.rs`; the bounded replay
is in `crates/realorrug-onchain/src/evm_fee_receipts/collection/configuration/history.rs`.
There are no additional RPC calls beyond the optional six-call configuration
check, and the final outer checkpoint stays reserved. Existing spending limits
and model authority are unchanged.

## Source and reconstruction assumptions

The retained [source metadata](data/0073-base/clanker-depositor-source.json) and
[runtime qualification](data/0073-base/clanker-depositor-qualification.json)
identify the existing LP locker `0x63d2dfea64b3433f4071a98665bcd7ca14d93496`.
Metadata is partially explorer-verified; this is not an independent rebuild or
audit. Parent and closing runtime bytes must agree with the reviewed deployment.
Both canonical headers must agree, be parent-linked and pass final rechecks.

The reviewed source emits `RewardRecipientUpdated(address,uint256,address,address)`
and `RewardAdminUpdated(address,uint256,address,address)` after its corresponding
write. The token and reward slot are indexed; old/new addresses occupy exactly
two data words. These functions check the existing reward administrator. The
replay relies on those runtime/source semantics; it does not independently
recover the caller of every internal invocation. Existing nonzero positions
prevent reinitialization under those same source assumptions.

The unfiltered one-block locker query must contain 2–128 ordered, checkpointed
events. The parent tuple uses the reviewed canonical packed layout, with 1–16
reward slots. Each update must address this token and a valid slot, and its old
value must match the reconstructed state. Every admitted update is retained with
field, slot, old/new address, log index and transaction hash. Change-and-reversal
sequences remain visible even when closing state equals opening state.

The exact submitted collection must occur once with matching receipt fields.
At that event, replay retains the current tuple; all later updates still must
reconcile the complete closing tuple. The collection reader uses this event-time
tuple when checking the deposit recipient and pool/position context. Its separate
scope is `provider_ordered_reward_configuration`, distinct from the existing
`provider_quiet_block_reward_configuration` scope.

The source reads reward state more than once during collection and fee handling.
Any recipient/admin change in the submitted collection transaction is refused,
on either side of the collection event. Extra locker events visible in that
receipt also prevent qualification, even if omitted from the block query.
Log evidence alone cannot locate a callback relative to those internal reads.
Unknown events, fee-preference changes, other tokens, additional collections,
duplicate/unordered indices, absent or changed anchors, malformed words,
out-of-range slots and unreconciled closing state retain an unresolved gap.
Provider reporting remains an assumption, not an independent proof of complete
event coverage. Only the supported single-collection history is admitted.

Historical factory registration is still closing state. Fee preferences,
conversions, shared PositionManager balances, backing and per-token revenue
remain unresolved. A successful configuration check does not upgrade the
deposit's financial state or establish treasury income.

## Live evidence and its limits

The [request](data/0076-base/base-clanker-ordered-configuration-regression.request.json),
[capture](data/0076-base/base-clanker-ordered-configuration-regression.capture.json)
and [RPC trace](data/0076-base/clanker-reward-history-rpc.json) rerun the known
deposit through the current reader on one fixed anonymous endpoint,
`https://base.drpc.org`. The capture harness spaces `eth_call` by 1,200 ms;
production has no added pacing, retry or provider fallback.

The transaction observation completes in 27 calls and retains the existing
quiet-block qualification at block 52139195, with parent 52139194 and collection
log 108. Independently reconciled credit remains 29478578528827 WETH base units.
The collection's reported token differs from the case token. The complete trace
records 40 attempted calls, including a reverted optional getter and a later
liquidity read that exceeded the shared deadline. A proxy may record a response
after the caller has already timed out; this does not establish a completed
observation. The [review](data/0076-base/review/base-clanker-ordered-configuration-regression.review.md)
and [assessment](data/0076-base/review/base-clanker-ordered-configuration-regression.assessment.json)
retain partial `CantTell`, unchecked owner acceptance and a separate SQLite
checkpoint. Three replay tools are not the original RPC cost.

The [candidate search](data/0076-base/clanker-reward-change-search.json) retains
live provider failures on 10,000- and 1,000-block queries. A 100-block query,
52216570–52216669, returned no matching recipient/admin updates. The provider's
1,000-block error text mentions a 10,000-block free-plan limit; this does not
establish a reliable accepted range. An empty bounded search says nothing about
all historical changes. No representative live busy-block collection was found;
the new ordered-history positives below are synthetic fixtures, not on-chain
captures. No model call, post, purchase, signing or financial execution occurred.

## Validation and next work

Five named tests exercise ordered histories, before/after collection changes,
round trips, exact six-call arguments and final-call reservation, end-to-end
recipient selection, malformed/missing histories, receipt-visible omissions,
canonical-array alias rejection, 16-slot and 128-event bounds. Temporarily
restoring closing-tuple attribution makes the end-to-end recipient test fail;
restored reconstruction passes. Existing quiet-block refusal, historical
collection and deposit/claim replay tests continue to apply. The transaction
replay also reproduces the new live observation exactly from its RPC trace.
Refusal fixtures also keep closing state reconcilable when the old event value
is wrong, add trailing data and a no-op one-past admin slot that would otherwise
alias an array length. Disabling the old-value or slot check, or accepting
trailing data, makes the refusal test fail; restored validation passes. These
cases isolate the intended guard from later reconciliation failures.
Scoped strict Clippy, format and documentation checks run locally; full suites
and mutation testing run on [PR 210](https://github.com/1xmint/realorrug/pull/210).

Mutation testing is rebalanced from eight to sixteen zero-indexed shards. The
previous [run](https://github.com/1xmint/realorrug/actions/runs/37342752069)
passed all 963 mutants, but one shard took 29m58s against the 30-minute job
limit. Every mutant remains in the set; job/per-mutant limits, serial execution
inside each shard and aggregate success requirements are unchanged.

Next: retain a representative live busy-block collection, qualify fee-preference
history and conversion/shared PositionManager boundaries, then fee origin and
owner-reviewed replies ahead of the treasury ledger. Wider histories, Flaunch
funding/balances, backing, native delivery, custody powers and measured launch
costs remain open. Base is preferred; launchpad, pairing and economics remain
unselected.

Subsequent [research 0077](0077-clanker-single-slot-fee-preference-stability.md)
adds single-slot preference stability for these supported histories when no
preference writes are reported. It retains a live quiet-block positive; preference
changes, representative live busy-block qualification and conversion proof remain
open. The preceding capture and historical limitations are preserved.
