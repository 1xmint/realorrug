<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0035 — selecting a Base launch route

**Date:** 2026-10-02. **Status:** accepted reasoning for the chain preference; launchpad recommendation under evaluation.
**Decision:** [ADR 0045](../adr/0045-base-is-the-preferred-launch-chain.md).
**Evidence:** [research 0067](../research/0067-base-launch-platform-comparison.md).

Josh prefers Base and leaves the launchpad open. Record that preference without
turning a provider recommendation, native pairing or default split into an
accepted promise. The investigator still covers Solana, Base and Ethereum.

The earlier pump.fun choice optimized for existing launch tooling. Base now
has investigative readers and an existing Base-USDC payment foundation.
Keeping token receipts, treasury accounting and eventual service payments on
one chain can reduce reconciliation and bridging work. Live payment fulfillment
and a Base launch checker remain implementation work.

Recommend qualifying Flaunch first: its native fee-to-bid-wall path could
support F4 without building our own executor. Compare Clanker for a simpler
fee-to-operations path and stable-asset runway. Use our existing Rust/React/
SQLite agent; a hosted agent runtime or an agent-controlled signing wallet is
not necessary to benefit from either launch platform.

The deciding evidence is a deployed fee/control trace and an affordable
operating plan, not a platform's automation label. A fixed share can coexist
with transferable revenue rights, owner rescue or disabled execution. A bid
wall is not automatically a supply burn or permanent additional liquidity.
Preserve financial states and disclose every retained power.

Keep the current manual treasury policy while comparing. Native financial
mechanisms require a separate adopted configuration and review before launch
or advertising. No split, signer, spending authority or launchpad is adopted
by the chain preference. Next work qualifies candidate deployments and useful
fee-routing answers before building the chosen launch readback and ledger.
