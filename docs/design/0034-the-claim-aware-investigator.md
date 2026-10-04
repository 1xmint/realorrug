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
