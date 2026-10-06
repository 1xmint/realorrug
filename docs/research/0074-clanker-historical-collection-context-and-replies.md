<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0074 — Clanker historical collection context and replies

**Date:** 2026-10-04. **Status:** bounded historical collection context and
deposit-specific reply prioritization implemented; live captures and offline
reviews retained. Per-token revenue, backing, owner acceptance and ledger open.
**Direction:** [VISION](../../VISION.md), [ROADMAP](../../ROADMAP.md).
**Previous evidence:** [research 0073](0073-clanker-deposit-funding-provenance.md).

**Subsequent implementation:** [research 0075](0075-clanker-quiet-block-reward-configuration.md)
adds a separate quiet-block reward-configuration stability check. The earlier
captures below retain block-end-only context; broader configuration and
per-token revenue attribution remain unresolved.

## Caller and evidence boundary

The existing successful canonical Base transaction reader can now qualify one
historical collection context for the reviewed LP locker
`0x63d2dfea64b3433f4071a98665bcd7ca14d93496`. This is a narrow, code-authored
relationship between a deposit, a reported collection token and registered pool
context. It is **not automatic per-token fee revenue attribution**. There is no
new planner tool, arbitrary RPC method, provider fallback or spending authority.

Admission requires a complete bounded receipt, exactly one reviewed credit and
its unique requested-transfer ingress proof. The LP locker must emit exactly
one recognized `ClaimedRewards` after the deposit. Its two reward arrays must
have exact canonical ABI offsets, equal lengths of 1–16 recipients and checked
totals agreeing with the reported amounts. Seven additional RPC calls share
the existing investigation budget; admission reserves the final outer
checkpoint call. Deadline, call and compute-unit limits remain unchanged.

At the transaction block, runtime bytes must match the deployment qualified in
research 0073. Historical fee-locker, PoolManager and PositionManager getters
must match the reviewed dependencies. Factory registration must name the
reported token and LP locker. The rewards getter must agree with that token
and registered hook; its pool currencies must contain the token and deposit
asset. Exactly one configured recipient slot must name the credit owner, and
its reported reward must equal the **requested**, not inferred net, deposit.

The five-word pool key hashes to the receipt pool ID. All recognized
PoolManager position modifications in the receipt must concern that pool and
the reviewed PositionManager, have zero liquidity delta and precede the funding
transfer. Their unique salts must cover the configured position range, bounded
to 1–16 positions. A historical header recheck must agree. Multiple collections,
multiple credits, inconsistent or unavailable evidence and unsupported
deployments retain a gap instead of a positive context statement.

Success retains `verification_scope: historical_collection_context`, reported
token, case-token match, pool/currencies/hook, recipient slot, position/log
identities, runtime hash and block checkpoint. `pool_attribution` remains
`unresolved`, and the credit's financial state remains `executed`.
Block-end registration and recipient configuration do not establish event-time
configuration. The reviewed older LP locker also lacks the newer shared
PositionManager transient-balance guard. Other transaction actions, fee origin,
conversion correctness and asset backing require separate analysis. Even a
matching case token would not complete that proof.

## Public reply and intelligence

Code selects deposit-specific statements by structured proof roles, not model
wording or labels. The first statement combines requested ingress and the
independently reconciled credit delta **only** when the window's owner, asset,
transaction and credit log identity agree. Otherwise net credit stays unknown.
The next statement describes qualified collection context and explicitly says
whether the reported token matches or differs from the case token. Generic
transaction facts and remaining fee/window statements remain in the dossier.
The existing analyst consumes this order; no model can upgrade the evidence.

This strengthens F2 Compounding Intelligence through a reusable, dated
collection relationship and a negative attribution example: pooled wallet
activity can concern a different token. F1 Attention and Participation gains a
more useful allegation-specific answer; F5 Trust gains inspectable boundaries
and retained failures. F3/F4 economics are unchanged. These examples are
qualification assets, not a measured public success rate or owner acceptance.

## Live captures and failure record

The [deposit request](data/0074-base/base-clanker-historical-collection.request.json)
and [final capture](data/0074-base/base-clanker-historical-collection.capture.json)
inspect transaction
`0x362fbb0a65298739c72e82ec9ba4c335ce9a36136b9818cfcc0eda54be4db46f`
at block **52139195**. Requested and independently measured received credit
are each **29478578528827** base units of WETH in this capture. Deposit log 107,
incoming transfer 106 and collection 108 agree with historical context for
token `0xad794ad19350a1755d90d53380102dc7213b8b07`, which differs from the case
token `0xbc8a7388c6cba5ad74b24627f58f9bd3affd5b07`. Pool
`0x787392934b75093f7e79e384410b4115b35265f72210020b43c9239d019a8992`
has five matching zero-liquidity position events, IDs 897035–897039. These are
one deposit's different evidence layers, not additional income entries.

The transaction read uses **21 calls**; the complete investigator uses **34**.
Later token and liquidity reads hit HTTP 429 and retain gaps. The
[claim request](data/0074-base/base-clanker-claim-prioritized.request.json) and
[capture](data/0074-base/base-clanker-claim-prioritized.capture.json) preserve the
claim scenario from research 0072: direct delivery of **3440630801955** WETH
base units, reconciled withdrawal, and no collection context. Its transaction
uses **14 calls** and the full investigator **33**; liquidity hits HTTP 429.
Neither receipt is a new RealOrRug treasury receipt.

[All 67 RPC records](data/0074-base/clanker-collection-rpc.json) use one anonymous
upstream, `https://mainnet.base.org`, through a temporary read-only recorder.
The recorder supplies a User-Agent and paces `eth_call` by **800 ms** for these
two captures. This is capture-harness behavior, not a production transport
change, quota guarantee or adopted provider. Failures remain in the trace;
there is no runtime fallback or retry. The recorder was stopped afterwards.

The [initial capture](data/0074-base/base-clanker-historical-collection.initial.capture.json)
and [28-call trace](data/0074-base/clanker-collection-initial-rpc.json) retain an
implementation error: the computed runtime hash omitted its `0x` prefix and
was compared with a prefixed constant. The underlying runtime bytes matched;
the refusal did **not** demonstrate a changed deployment. The prefix is fixed.
The subsequent [unpaced capture](data/0074-base/base-clanker-historical-collection.rate-limited.capture.json)
and [32-call trace](data/0074-base/clanker-collection-rate-limited-rpc.json)
retain a distinct provider failure: HTTP 429 at the historical registry call.
Deposit ingress and measured credit survive that unavailable context. The
successful paced capture does not erase either earlier failure.

[Deposit review](data/0074-base/review/base-clanker-historical-collection.review.md)
and [assessment](data/0074-base/review/base-clanker-historical-collection.assessment.json),
plus [claim review](data/0074-base/review/base-clanker-claim-prioritized.review.md)
and [assessment](data/0074-base/review/base-clanker-claim-prioritized.assessment.json),
retain separate durable checkpoints. Both are partial `CantTell`, with owner
acceptance unchecked. Their four replay tool calls are not original live RPC
cost. No model call, payment, post, signing or financial action occurred.

## Validation and next boundary

Named tests replay exact recorded RPC arguments and serialized observations,
require 21 deposit/14 claim calls, exercise runtime/dependency/registration and
reorg refusals with exact failure boundaries, and enforce collection layout,
recipient uniqueness, independent currency/slot selection and position bounds.
The analyst replay keeps an unverified 95% allegation out of the reply and
preserves partial `CantTell`. Deliberately weakening recipient uniqueness makes
the named slot regression fail; restored source passes. Scoped strict Clippy,
named tests and documentation checks run locally; full suites and mutation
shards run on [PR 210](https://github.com/1xmint/realorrug/pull/210).

Next: qualify event-time configuration and fee-origin/conversion/shared
PositionManager boundaries, with representative owner-reviewed replies, before
projecting these proofs into a treasury ledger. Wider windows, native delivery,
Flaunch balance/funding evidence, custody powers and measured launch costs
remain open. Base stays preferred; launchpad, pairing and financial allocations
remain unselected. Current funding, runtime limits and model authority remain.
