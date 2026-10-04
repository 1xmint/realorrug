<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0070 — bounded Base fee delivery matching

**Date:** 2026-10-03. **Status:** implemented bounded receipt matcher with a
positive live Clanker capture; complete financial reconciliation, native and
conversion delivery, representative coverage and owner acceptance remain open.
**Direction:** [VISION](../../VISION.md), [ROADMAP](../../ROADMAP.md).
**Reviewed deployments/layouts:** [research 0068](0068-base-fee-routing-and-retained-controls.md)
and [research 0069](0069-clanker-current-locker-and-wallet-receipts.md).

## Delivered behavior

The investigator's transaction reader now recognizes two Base deployment/event
pairs: `ClaimTokens(address,address,uint256)` from the reviewed Clanker fee
locker and `Withdrawal(address,address,address,address,uint256)` from the
reviewed current Flaunch paired-token escrow. Recognition is scoped to chain,
address and exact event layout. Other generations, permissioned claim events
and arbitrary ABI declarations are not interpreted as supported fee claims.

The caller first requires a successful transaction with matching transaction,
receipt and canonical block identities. The matcher uses the first 128 receipt
logs without new RPC requests. Log transaction/block fields must match the
receipt, `removed` must be explicitly false, and indices must be present and
unique. Missing, inconsistent or truncated coverage prevents verification.
Malformed recognized claims remain refusals rather than inferred payments.

A direct ERC-20 delivery requires exactly one Transfer event with the claimed
asset, escrow sender, intended recipient and exact base-unit amount. The same
transfer cannot verify several claims; multiple eligible transfers are
ambiguous. Duplicate indices are retained only once and invalidate coverage.
Malformed Transfer events of the delivered asset also prevent verification.
No transaction-wide amount sum or heuristic beneficiary inference is used.

Each claim records its event index, protocol, escrow, fee-balance owner,
recipient, balance asset, delivered asset, amount and evidence scope:

| State | Required evidence | Meaning |
|---|---|---|
| `executed` / `claim_event_only` | A well-formed claim event in a successful canonical receipt. | The contract emitted a claim; delivery is unresolved with an explicit gap. |
| `verified` / `receipt_event_match` | The claim plus one unambiguous matching ERC-20 receipt event. | This wallet-level delivery has matching receipt evidence. It is not an independent audit or balance reconciliation. |

Every claim has `pool_attribution: unresolved`. Both protocols aggregate fee
balances by wallet and asset, so a withdrawal cannot automatically be counted
as revenue from the case token. Receipt matching does not resolve beneficial
ownership, token behavior or the contribution of individual pools. Financial
allocation and accrual remain separate from claim/delivery observations.

Native delivery and an escrow asset converted into a different delivered
asset retain the observed claim but require separate supported verification.
They are not forced into ERC-20 matching. Failed transactions cannot produce
executed or verified fee claims. Coverage metadata describes receipt bounds
and checks, not universal protocol comprehension or complete wallet history.

## Live capture and replay

The [request](data/0070-base/base-clanker-claim-delivery.request.json) supplies
the actual transaction found in research 0069 while retaining the token case
as investigation context. The
[fresh live capture](data/0070-base/base-clanker-claim-delivery.capture.json)
uses an anonymous public Base RPC. Its transaction observation records the
historical receipt block separately from the current observation read point.

It matches **3440630801955 base units** of asset
`0x4200000000000000000000000000000000000006`, from Clanker fee locker
`0xf3622742b1e446d92e45e22923ef11c2fcd55d68` to
`0xce165ce10c2f1bac8bc6b1e4009e87b84ddd8eaa`. The claim event has index 326;
the matching transfer has index 325, both in successful transaction
`0xfe327960aff5839e02ef4a52f8bccdb90e0dfa3bd4278ed3a8ec7361be829c09`
at Base block **52139202**. The case token is a different asset and its current
reward recipient is a different wallet. The reader explicitly leaves token
revenue attribution unresolved rather than joining these unrelated roles.

The deterministic reply includes the wallet-level finding and its attribution
limit. The [offline review](data/0070-base/review/base-clanker-claim-delivery.review.md)
retains all findings and gaps in the existing SQLite case history; checkpoint
verification passes. Assessment remains partial `CantTell`, and owner
acceptance is unchecked. No model call or public post was made.

Clanker has this positive live receipt evidence. Flaunch direct-delivery tests
use fixtures built from its reviewed deployed ABI; this slice does not claim
a positive live Flaunch withdrawal capture or verified unwrap/native behavior.

## Validation and next boundary

Named tests cover both exact layouts, wrong asset/sender/recipient/amount,
checkpoint refusals, duplicate indices, ambiguous/reused transfers, the
128/129-log boundary, late claims, malformed claims/transfers, failed receipts,
unsupported chains and native/conversion gaps. An integration test reuses the
retained real transaction/receipt/block and checks that the case-token
transfer list remains separate from its paired-asset claim. Strict scoped
Clippy and CLI capture/review dispatch pass locally. Full suites and mutation
gates run on [PR 210](https://github.com/1xmint/realorrug/pull/210).

An additional result-publication regression uses a fixed timestamp and a
previous-day spend ledger to require rollover before charging and to prevent
a second charge on repeat delivery. This checks the established runtime
budget policy without changing its limits or adding a live publisher.

This is an investigation evidence reader, not an implemented treasury ledger
or spending mechanism. Next, reconcile contributing credits, claims and
balances across explicit windows, publish unresolved attribution and avoid
counting one claim/transfer pair twice. Qualify remaining custody powers,
Flaunch payout variants and concrete launch/operating costs before choosing
the provider, pairing and allocation. Base remains preferred; the proposed
25% burn / 25% liquidity split remains unadopted. Runtime spending limits,
manual treasury policy and startup funding remain unchanged.
