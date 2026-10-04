<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0072 — Clanker transaction balance windows

**Date:** 2026-10-04. **Status:** one-block reconciliation implemented with live
credit/claim captures and offline reviews; wider history, attribution, backing,
Flaunch balance windows and owner acceptance remain open.
**Direction:** [VISION](../../VISION.md), [ROADMAP](../../ROADMAP.md).
**Previous evidence:** [research 0071](0071-base-fee-credits-and-balance-windows.md).

## Caller and bounded behavior

The existing successful Base transaction reader now attempts a balance window
when its complete receipt establishes exactly one wallet/asset key at the
reviewed Clanker fee locker, `0xf3622742b1e446d92e45e22923ef11c2fcd55d68`.
The named production caller is the investigator's `transaction` read, exercised
by `investigation-capture`; no new model-selected RPC arguments, payment action
or request schema is introduced. Internal typed keys and block boundaries are
derived from decoded receipt events. The request's `TimeWindow` remains Unix
seconds and is never reinterpreted as block numbers.

The scope is **the entire block containing the submitted transaction**, for one
wallet and asset. The opening getter is at its parent block; the closing getter
is at the transaction block. This includes other matching transactions within
the same block, so an earlier transaction in that block cannot be mistaken for
the opening balance. The reader does not promise the wider research 0071
intervals or arbitrary requested time coverage.

Eight additional RPC calls use the existing configured endpoint, deadline,
call and compute budgets: an opening header, two `availableFees(address,address)`
calls, three exact-key event queries and two boundary header rechecks. Admission
requires nine shared calls remaining to preserve the outer read's final
checkpoint. No endpoint fallback or budget increase is added. Both headers must
have the expected numbers and hashes and be parent-linked. Getter words must
fit the existing exact unsigned range; unavailable state remains unknown.

The queries cover `StoreTokens`, `ClaimTokens` and the ABI-declared
`ClaimTokensPermissioned` signature. The reviewed implementation changes balances
through the first two; any permissioned event refuses this unsupported layout.
Across all queries at most 128 events are admitted. Wrong query signatures,
wrong emitter/key, removed or incomplete checkpoints, malformed ABI, duplicate
block-global log indices and invalid transaction hashes refuse reconciliation.
Submitted fee events must also be present with matching identity and payload;
matching endpoints alone could conceal omitted offsetting events.

Events are sorted by log index. A credit's received delta is its new cumulative
balance minus the running balance, **not its requested transfer amount**. A
claim must equal the running balance and sets it to zero. The final balance
must agree with the closing getter; canonical boundaries are then rechecked.

## Evidence and financial meaning

`fee_balance_windows` retains the wallet, asset, boundary numbers/hashes, getter
balances and ordered event linkage. A successful result is `reconciled: true`
with `verification_scope: provider_balance_window`. A refused/failed read keeps
an explicit gap and no invented zero or partial successful roll-forward.
Multiple wallet/asset keys exceed the current one-key bound. Credits and claims
for the **same** key share one window rather than causing duplicate reads.

This is agreement between provider observations, not cryptographic proof of
complete logs, backing, beneficial ownership or absence of omitted events.
The opening balance's origin is outside this window. A claim and its matching
delivery remain one withdrawal. Reconciliation does not add another treasury
receipt or upgrade a credit into a delivered payment. Existing `fee_credits`
keep their event-only unknown delta; the independently measured delta lives
in the window event. Individual-token revenue attribution remains unresolved.
Flaunch receipt decoding remains available but has no automated balance window.

## Retained live captures

Both captures use one anonymous public upstream, `https://mainnet.base.org`.
The read-only recording harness adds a User-Agent and records every request
and response in [the RPC trace](data/0072-base/clanker-block-window-rpc.json).
The configured runtime client points to the local recorder; this capture
qualifies the reader against that single upstream, not direct transport behavior
for every public endpoint. The temporary recorder was stopped afterward.
No paid provider, model call, post, signing or financial execution occurred.

Both wallet windows concern asset `0x4200000000000000000000000000000000000006`.
Amounts below are **base units**, not dollars or earnings of the case token:

| Capture | Block | Opening | Received credit | Claimed | Closing |
|---|---:|---:|---:|---:|---:|
| [Credit](data/0072-base/base-clanker-credit-balance-window.capture.json) | 52139195 | 39920159680638 | 29478578528827 | 0 | 69398738209465 |
| [Claim](data/0072-base/base-clanker-claim-balance-window.capture.json) | 52139202 | 3440630801955 | 0 | 3440630801955 | 0 |

The credit belongs to wallet `0x8b4eb0cd07f398357657369a684e6e0685a76ae2`
and transaction `0x362fbb0a65298739c72e82ec9ba4c335ce9a36136b9818cfcc0eda54be4db46f`,
at log index 107. The claim belongs to **a different wallet**,
`0xce165ce10c2f1bac8bc6b1e4009e87b84ddd8eaa`, and transaction
`0xfe327960aff5839e02ef4a52f8bccdb90e0dfa3bd4278ed3a8ec7361be829c09`,
at index 326, with the existing direct delivery match at index 325.
Each transaction read costs 14 RPC calls including network/latest/final checks;
the complete deterministic investigator captures each cost 27 calls.

[Credit review](data/0072-base/review/base-clanker-credit-balance-window.review.md)
and [claim review](data/0072-base/review/base-clanker-claim-balance-window.review.md)
retain the public drafts and separate SQLite checkpoints. Both remain partial
`CantTell`; owner acceptance is unchecked. Their capture requests, assessments
and raw observations are retained alongside them. Offline replay accounting
counts replayed tools, not the original live RPC costs.

## Validation and next boundary

Named tests exercise ordered credit/claim arithmetic, unchanged balances,
requested/received differences, shared-key deduplication, call admission,
event caps, malformed evidence, missing anchors, mismatched headers and reorgs.
A retained-live integration test requires exact RPC arguments, 14 calls per
transaction and identical serialized observations for both captures. Existing
claim/credit tests also retain useful receipt evidence when the window lacks
its reserved calls. Scoped all-target Clippy and document checks are run locally;
full suites and all mutation shards run on
[PR 210](https://github.com/1xmint/realorrug/pull/210).

Manually replacing the measured delta with the requested amount caused the
arithmetic regression to fail with 99 versus 5. Restoring the source made that
named test pass. No full suite or mutation runner was run on the workstation.

The initial `1cf0a2a` CI run passed 2397 Rust tests and 129 frontend tests but
found six mutation survivors. Cap refusals now assert the exact reason and
call count so a later transport failure cannot impersonate an event-limit
check. Valid claim/credit fixtures with independently wrong owners and assets
now constrain key matching. The receipt-anchor pass was simplified: complete
one-key admission already establishes its key, so every decoded Clanker anchor
must be present without a second key filter that could hide a missing claim.
These changes add no mutation exclusions or weakened gates. Manually replaying
the surviving cap multiplication and loose claim-key mutations now fails the
named regressions; restored source passes. Retained live observations remain
identical after the anchor simplification.

Next: establish deposit funding provenance and the limits of per-token
attribution, then define the ledger projection from qualified evidence. Wider
history, native/conversion delivery, backing and Flaunch reconciliation remain
distinct coverage expansions. Base is still preferred; launchpad, pairing,
custody and financial allocation are not selected by this reader. Runtime
spending limits and the proposal-only economics remain unchanged.
