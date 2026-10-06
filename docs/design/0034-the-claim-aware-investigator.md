<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0034 — the claim-aware investigator

**Date:** 2026-10-01. **Status:** accepted reasoning for [ADR 0044](../adr/0044-three-chain-investigations-and-shared-cases.md).

The old parser discards questions and selects the first address; thread memory
disappears on restart. Keep the allegation as untrusted evidence and resolve
the target before attaching work to a durable chain-qualified case.

Use existing on-chain SQLite memory rather than turn the forecast chain into
an agent database. Append case events; mutable jobs track operational state.
Transactions serialize admission, claims and completed revisions. Admission
precedes work; results precede publication. Corrections invalidate dependent
findings without deleting history. Retrieve prior cases and playbooks as leads,
with provenance and age visible; re-read dynamic state.

The planner selects typed registered reads, never arbitrary URLs, RPC methods,
SQL or paths. Leads must be submitted or established by previous observations.
One budget bounds the whole investigation. Missing data, unsupported decoders
and partial histories stay explicit. Configured fee shares are not receipts,
and receiving a transfer does not prove beneficial ownership.

Reuse typed EVM primitives behind read-only adapters; keep Pons decoding on
Robinhood. Hex syntax is an address family, not a chain. Pump/PumpSwap,
Clanker v4/Flaunch and Uniswap v2/v3 are initial specialist families, gated by
verified deployment/version. Sourcify v2 source lookup remains untrusted data.
Generic reads do not advertise universal protocol comprehension.

Clanker reward reads require the pool's case token and hook to agree with the
factory registration. Both currencies are observed without assuming conversion
preferences or realized payments. [Research 0069](../research/0069-clanker-current-locker-and-wallet-receipts.md)
qualifies the additional current Base locker and documents why pooled wallet
claims require separate asset delivery and per-token attribution checks.

The transaction reader now matches reviewed Base fee-claim events to exact
direct ERC-20 deliveries within a successful canonical receipt. Unique log
identities and unambiguous claim/transfer matches are required; native and
conversion delivery remain gaps. [Research 0070](../research/0070-bounded-base-fee-delivery-matching.md)
records the scope and live evidence. Wallet-level receipt verification never
implicitly attributes pooled revenue to the case token or implements a ledger.

The same caller retains reviewed credit events separately from withdrawals.
Clanker's requested amount and new cumulative balance remain separate;
Flaunch's reported pool label remains untrusted attribution. No received delta,
backing or treasury receipt is inferred from a deposit alone.
[Research 0071](../research/0071-base-fee-credits-and-balance-windows.md) records
the live credit and measured research windows. Its subsequent
[research 0072](../research/0072-clanker-transaction-balance-windows.md) implements
a one-block wallet/asset window for submitted Clanker transactions. Opening and
closing getter observations, ordered credit/claim events, receipt anchors and
parent-linked canonical checkpoints must agree. At most one key and 128 events
are admitted; eight additional RPC calls share the existing budget and reserve
the outer checkpoint call. Reconciliation remains provider evidence, separate
from receipt delivery and token revenue attribution. Wider windows, Flaunch
balance reconciliation and the treasury ledger remain subsequent work.

[Research 0073](../research/0073-clanker-deposit-funding-provenance.md) adds
receipt-level Clanker ingress evidence to the same transaction caller, without
additional RPC calls. One preceding transfer must match the requested asset,
depositor, escrow and amount, with complete coverage and no ambiguous reuse.
Its proof remains separate from received credit, backing and token/pool origin;
the credit's financial state is not upgraded. Live historical upstream
qualification demonstrates a different-token collection context. The dossier
retains this proof. [Research 0074](../research/0074-clanker-historical-collection-context-and-replies.md)
now prioritizes deposit ingress and independently matched received credit in
short replies. Seven additional shared-budget calls qualify one reviewed LP
locker's historical runtime, dependencies, registration, pool, recipient slot
and zero-liquidity position events, while preserving the outer checkpoint.
The qualified context names its reported token and case-token relationship;
block-end configuration does not establish event-time configuration, backing,
conversion correctness or per-token revenue. Unsupported/multiple collections
or deposits retain gaps. Historical fee-origin and shared PositionManager
boundaries, representative owner review and the ledger remain subsequent work.

[Research 0075](../research/0075-clanker-quiet-block-reward-configuration.md)
adds an optional quiet-block reward-configuration check to that qualified
collection. Parent/closing tuples and runtime must agree, headers must be
parent-linked and rechecked, and the block-wide locker query must return only
the exact submitted collection. Six additional shared-budget calls reserve the
final checkpoint. Source/provider assumptions stay visible; any additional
event or failed check retains unresolved timing without erasing collection
evidence. Configuration-change reconstruction, fee preferences, registry
history, conversion correctness and per-token revenue remain separate work.

[Research 0076](../research/0076-clanker-ordered-reward-configuration-history.md)
adds a separate ordered-history scope for supported recipient/admin updates
in other transactions in the collection block. Parent-state replay must
reconcile closing state, and collection linkage uses the reconstructed tuple.
Changes inside the collection transaction and unsupported or missing events
remain unresolved. Positive busy-block coverage is synthetic; a live quiet-block
regression and bounded search failures are retained. A representative live busy
block, fee preferences, registration history, conversions and revenue proof
remain subsequent work.

[Research 0077](../research/0077-clanker-single-slot-fee-preference-stability.md)
now qualifies one recipient slot's requested denomination when the supported
reward history excludes preference writes and parent/closing getters and headers
agree. Four shared-budget calls reserve the final checkpoint. A live quiet-block
capture requests WETH; failed optional reads preserve collection evidence.
This narrows the preceding preference gap for that single stability scope.
Preference changes, unexamined slots, conversions and revenue remain separate
proof obligations; financial state is unchanged.

Normal Flaunch fee observations also record pool currencies and a route snapshot
after token/hook/NFT registration and current ownership agree. The legacy hook
uses its global escrow getter; the current Base hook resolves escrow per pool.
Bid-wall enabled state is a boolean observation, never proof of execution or
permanent liquidity. Optional route/distribution failures retain their quote
and surface in the public gap. [Research 0068](../research/0068-base-fee-routing-and-retained-controls.md)
records the live version/control qualification; arbitrary manager powers and
historical receipts remain unresolved.

Offline replay produces drafts, chosen reads, evidence, gaps, accounting and
checks without posting. Live model evaluation is explicit and metered. Library
projections and authenticated submissions share cases and quotas. Interrupted
work cannot repeat model calls or publication silently. Compact case history
survives raw retention; backups/checkpoints remain under operator custody.

The serialized worker checks queued work and refreshes its heartbeat between
mention polls. An idle enabled worker waits at most ten seconds, preserving
the paid X/Telegram polling deadline instead of polling those lanes again.
Active investigations remain bounded and can delay a due poll. A disabled
worker retains the existing polling sleep. Pending publication uses the
correction-aware assessment for its exact request, so a withdrawn clause
cannot survive merely because its delivery was already queued.
