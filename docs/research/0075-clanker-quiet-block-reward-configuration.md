<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0075 — Clanker quiet-block reward configuration

**Date:** 2026-10-05. **Status:** bounded reward-configuration stability check
implemented; positive live qualification/capture, provider failures and offline
reviews retained. Busy-block reconstruction, fee origin and owner acceptance open.
**Direction:** [VISION](../../VISION.md), [ROADMAP](../../ROADMAP.md).
**Previous evidence:** [research 0074](0074-clanker-historical-collection-context-and-replies.md).

## Caller and precise claim

The existing transaction reader's qualified historical Clanker collection can
now carry a separate `reward_configuration` result. It checks the reviewed LP
locker at the **parent and closing block**, rather than treating closing state
alone as configuration used during the deposit. This strengthens the dated
case/playbook evidence for F2 and makes F5's uncertainty inspectable.

Qualification requires an initialized position, parent-linked canonical
headers, identical locker runtime bytes and identical complete reward tuples
at both checkpoints. One unfiltered, single-block `eth_getLogs` query must
return **exactly one locker event**, identical to the already-qualified
collection anchor in the submitted receipt. Parent and closing headers are
then rechecked. The reviewed source emits events for recipient/admin changes
and refuses reward reinitialization when positions already exist. Under those
source and provider assumptions, this supports stable reward configuration
through this quiet block, including the collection.

This is **not independent proof of provider completeness** or a trace of every
storage write. Explorer source verification remains partial as recorded in
[research 0073](0073-clanker-deposit-funding-provenance.md); no independent
recompile or audit was performed. A provider could omit an unrelated
transaction's change/reversal events. Any additional locker event, unknown
signature, missing/altered anchor, changed tuple/runtime, unavailable read or
changed boundary refuses the stability result. Busy blocks require a separate
qualified transition decoder; this implementation reconstructs no changes.

Six additional calls share the existing budget and preserve the outer final
checkpoint call. Insufficient capacity produces a gap without new calls.
Failure of this optional check preserves the already-qualified collection
context. Success uses `provider_quiet_block_reward_configuration`, retains the
tuple, collection index and both block hashes, and changes the public statement
to name its assumptions. It does not upgrade financial state or verdict.
Factory registration history, fee-preference history, conversion correctness,
shared PositionManager transient balances, backing and per-token revenue remain
unresolved. The reported collection token still differs from the case token.

## Live qualification and capture

[Eight manual read-only calls](data/0075-base/clanker-configuration-qualification.json)
to `https://mainnet.base.org` confirm Base chain identity, parent/closing headers,
parent runtime/reward tuple, the sole locker event and unchanged checkpoints.
Runtime and tuple agree with the retained closing observations from research
0074. This is a distinct qualification run, not a runtime fallback.

The [deposit request](data/0075-base/base-clanker-quiet-block-configuration.request.json)
and [positive capture](data/0075-base/base-clanker-quiet-block-configuration.capture.json)
use `https://base.drpc.org`, explicitly chosen for a separate run. Transaction
`0x362fbb0a65298739c72e82ec9ba4c335ce9a36136b9818cfcc0eda54be4db46f`
qualifies in **27 calls**, compared with research 0074's 21. Opening block
**52139194** and closing block **52139195** agree; log **108** is the sole
reported locker event. Requested deposit and independently received credit
remain **29478578528827** WETH base units. The subsequent token and liquidity
observations retain `Deadline` gaps under the existing investigation limit.

The [claim request](data/0075-base/base-clanker-claim-configuration-boundary.request.json)
and [capture](data/0075-base/base-clanker-claim-configuration-boundary.capture.json)
retain direct delivery and reconciled withdrawal in **14 transaction calls**,
with no collection/configuration read. Later liquidity fails with a global
timeout. [The combined trace](data/0075-base/clanker-configuration-rpc.json)
retains **67 recorded attempts**, 33 for deposit and 34 for claim, including
reverting `owner()` probes. A recorded late response is not a successful
investigator observation after its deadline.

The temporary recorder supplies a User-Agent and paces `eth_call` at **1200 ms**
for the positive captures. This is capture-only behavior; production transport,
deadline, call and spending limits are unchanged. Each run uses one fixed
upstream; neither production fallback/retry nor provider adoption was added.
All temporary recorders were stopped afterwards.

Two failed mainnet.base.org runs are preserved: the
[first capture](data/0075-base/base-clanker-quiet-block-configuration.rate-limited.capture.json)
and [30-attempt trace](data/0075-base/clanker-configuration-rate-limited-rpc.json)
hit HTTP 429 at historical factory registration, leaving collection unavailable.
The [second capture](data/0075-base/base-clanker-quiet-block-configuration.parent-rate-limited.capture.json)
and [29-attempt trace](data/0075-base/clanker-configuration-parent-rate-limited-rpc.json)
qualify collection but hit HTTP 429 at the parent reward getter. Its optional
configuration remains unavailable, alongside later timeout/deadline gaps.
Slower capture pacing did not eliminate this provider failure. Neither failed
run is replaced by the successful separate-endpoint result.

[Deposit review](data/0075-base/review/base-clanker-quiet-block-configuration.review.md)
and [assessment](data/0075-base/review/base-clanker-quiet-block-configuration.assessment.json),
plus [claim review](data/0075-base/review/base-clanker-claim-configuration-boundary.review.md)
and [assessment](data/0075-base/review/base-clanker-claim-configuration-boundary.assessment.json),
retain separate SQLite checkpoints, partial `CantTell` and unchecked owner
acceptance. Their three/four replay tools are not original RPC cost. No model
call, post, payment, signing or financial execution occurred.

## Validation and next boundary

Named tests require exact six-call arguments and final-call reservation,
refuse changed state/events/checkpoints at exact call boundaries, and reject
altered anchors and change/reversal sequences. Weakening the one-event guard
to accept any nonempty event list makes the named refusal regression fail;
restored source passes. Integration replay reproduces the exact retained
deposit/claim observations in 27/14 calls, and analyst replay preserves provider
assumptions, unresolved revenue and partial `CantTell` while excluding an
unverified 95% accusation. Scoped strict Clippy and documentation checks run
locally; full suites and mutation shards run on
[PR 210](https://github.com/1xmint/realorrug/pull/210).

Next: qualify configuration-change histories and fee preferences, then examine
conversion and shared PositionManager/fee-origin boundaries with representative
owner-reviewed replies before a treasury ledger projection. Wider histories,
Flaunch funding/balances, backing, native delivery, custody and measured launch
costs remain open. Base is preferred; launchpad, pairing and economics remain
unselected. Current runtime limits and model authority remain unchanged.
