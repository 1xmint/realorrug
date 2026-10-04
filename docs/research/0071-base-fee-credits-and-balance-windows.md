<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0071 — Base fee credits and balance windows

**Date:** 2026-10-04. **Status:** bounded credit-event reader implemented;
positive Clanker capture and two research balance windows reconciled. Automated
window reconciliation, backing, token attribution and owner acceptance remain open.
**Direction:** [VISION](../../VISION.md), [ROADMAP](../../ROADMAP.md).
**Previous evidence:** [research 0070](0070-bounded-base-fee-delivery-matching.md).

## Implemented credit observations

The successful canonical transaction reader decodes `StoreTokens` from the
reviewed Base Clanker fee locker and `Deposit` from the current Flaunch paired
escrow. Deployment addresses remain those in
[research 0069](0069-clanker-current-locker-and-wallet-receipts.md) and
[research 0068](0068-base-fee-routing-and-retained-controls.md). Other deployments
and layouts remain unsupported.

Credits share the first-128-log bound and transaction/block/index checks with
claims. Duplicate/inconsistent identities are excluded; malformed recognized
credits invalidate coverage and prevent claim delivery verification. The
existing `fee_claim_receipt_coverage_complete` flag covers supported credit
layouts too. No additional RPC call is made. `fee_credits` are separate from
`fee_claims` and case-token `transfers`. Their `financial_state: executed` and
`verification_scope: credit_event_only` mean the event executed, not that token
origin, backing or a treasury receipt has been independently verified.

| Event | Preserved roles and values | Unresolved |
|---|---|---|
| Clanker `StoreTokens(address,address,address,uint256,uint256)` | Indexed depositor, fee owner, asset; reported cumulative balance after credit; requested transfer amount. | Received delta requires preceding balance. Requested amount is not substituted for it. No pool id is present. |
| Flaunch `Deposit(bytes32,address,address,uint256)` | Indexed reported pool label; fee owner, escrow asset, reported credit amount. | Depositor is absent. Pool label is caller-supplied; delivery, backing and revenue origin require separate evidence. |

Both retain `credited_delta: null` and `pool_attribution: unresolved`. Native
and zero-amount deposits remain event observations without inferred delivery
or new earnings. Credits are not added to withdrawn revenue.

The retained Clanker
[source record](data/0069-base/clanker-identities-and-claim-discovery.json) and
[explorer source](https://base.blockscout.com/api/v2/smart-contracts/0xf3622742b1e446d92e45e22923ef11c2fcd55d68)
show `storeFees` measures the received token balance delta, credits that delta,
then emits cumulative balance and requested amount. Transfer-tax or unusual
tokens can make requested and received amounts differ.
The current Flaunch [source record](data/0068-base/flaunch-paired-identities.json)
qualifies its escrow. Its `allocateFees` is permissionless: a caller can fund a
deposit labelled with another pool. The owner/indexer condition on a separate
aggregate getter does not establish trading-derived creator revenue. A pool
label alone must not become verified token revenue attribution.

## Positive Clanker capture

[Discovery](data/0071-base/clanker-credit-discovery.json) covered 11 historical
blocks, **52139192–52139202**, and returned one StoreTokens event. The
[successful transaction, receipt and canonical block](data/0071-base/clanker-credit-transaction.json)
identify transaction `0x362fbb0a65298739c72e82ec9ba4c335ce9a36136b9818cfcc0eda54be4db46f`
at block **52139195**, event index **107**. It reports fee owner
`0x8b4eb0cd07f398357657369a684e6e0685a76ae2`, depositor
`0x63d2dfea64b3433f4071a98665bcd7ca14d93496`, and asset
`0x4200000000000000000000000000000000000006`. Requested transfer is
**29478578528827 base units**; cumulative balance is **69398738209465 base units**.

The [request](data/0071-base/base-clanker-credit-event.request.json),
[live investigator capture](data/0071-base/base-clanker-credit-event.capture.json)
and [offline review](data/0071-base/review/base-clanker-credit-event.review.md)
retain the token case as context without attributing this different wallet's
credit to it. The reply includes the prior-balance and attribution gaps.
Historical receipt block and current observation read point stay separate.
SQLite checkpoint verification passes with
`82ee58ea38afa6d22c2228e390efb5e4062505c680dbf512b685a71aef0ad1f9`.
Assessment remains partial `CantTell`; owner acceptance is unchecked.

## Research balance windows

These are research captures, **not an automated production window reader or
public treasury ledger**. Both call `availableFees(address,address)` at the
end of the block immediately before the window and its closing block. Raw
records retain targets, calldata, blocks and responses. Logs cover the exact
escrow, owner and asset with at most 128 events. StoreTokens, ClaimTokens and
the interface's permissioned-claim topic are queried separately; a permissioned
event would refuse this rollforward rather than disappear. None appeared here.

Sort by block/log index, require unique identities and canonical event hashes,
and recheck boundary/event headers afterward. Each StoreTokens delta is its
reported new balance minus the preceding running balance. Each supported
full-balance claim must equal that balance and resets it to zero. Final balance
must match the closing getter. Negative deltas, mismatches and unavailable
reads refuse reconciliation. This compares provider observations; it does not
cryptographically prove complete history, backing or absence of omitted
offsetting events.

All amounts are base units of the asset named above:

| Retained window | Fee owner | Opening | Credit deltas | Claimed | Closing |
|---|---|---:|---:|---:|---:|
| [52137773–52139773](data/0071-base/clanker-wallet-asset-window.json) | `0xce165ce10c2f1bac8bc6b1e4009e87b84ddd8eaa` | 3440630801955 | 0 | 3440630801955 | 0 |
| [52139192–52139202](data/0071-base/clanker-credit-asset-window.json) | `0x8b4eb0cd07f398357657369a684e6e0685a76ae2` | 39920159680638 | 29478578528827 | 0 | 69398738209465 |

Both satisfy **opening + credit deltas − claims = closing**. The first confirms
the research 0070 claim consumed an existing balance, with earlier credits
outside the window. The second measures the new delta, which happens to equal
the requested amount here. Neither proves origin from the token case or
beneficial ownership. The wallets are different; do not splice their histories.
A claim and its matched delivery remain one withdrawal, never two earnings.

Anonymous public RPCs supplied the observations without paid services. Initial
[HTTP 400](data/0071-base/clanker-window-provider-failure.json) and
[HTTP 403](data/0071-base/clanker-window-second-provider-failure.json) failures
are retained. Successful records identify each endpoint: Base's public RPC
supplied logs and the other anonymous Base RPC supplied balances/blocks.
Failures are instrument limitations, not absence of fees.

## Validation and next boundary

Named regressions cover exact roles, cumulative/requested values, untrusted
labels, malformed/overflowing ABI, wrong deployments/chains, failed receipts,
checkpoints, duplicates and bounded late events. A retained-receipt integration
test requires three RPC calls and unresolved attribution. Strict scoped Clippy
and capture/review dispatch pass. Full suites and mutation checks run on
[PR 210](https://github.com/1xmint/realorrug/pull/210). Flaunch remains ABI-fixture
tested; no positive live Flaunch deposit or withdrawal is claimed here.

Manually substituting the requested amount for `credited_delta: null` made the
named credit-role regression fail at its unknown-delta assertion. Restoring
the source made it pass. This checks the tempting wrong inference directly;
no local full suite or mutation runner was used.

Next, turn measured reconciliation into a bounded typed read with explicit
opening/closing checkpoints, all supported balance-changing events, deduplicated
receipt linkage and historical-coverage gaps. Resolve funding provenance before
per-token attribution. Backing, native/conversion delivery and custody are
separate checks. Provider, pairing, financial split and runtime limits remain
unchanged. No model call, public post, signing, paid purchase or deployment occurred.
