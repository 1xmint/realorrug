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
