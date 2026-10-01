<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0044 — three-chain investigations and shared cases

**Date:** 2026-10-01. **Status:** accepted by Josh.
**Reasoning:** [design 0034](../design/0034-the-claim-aware-investigator.md).
**Delivery:** [plan 0003](../plans/0003-three-chain-investigator-and-library.md).

Josh requires Solana, Base and Ethereum investigations before the joint launch,
including comparable fee, contract-control, liquidity and transfer coverage.
The token still launches on Solana/pump.fun with SOL pairing. Unknown protocol
versions and unavailable histories remain explicit gaps, not inferred safety.

Cases identify chain and token, preserve allegations separately from measured
observations, and append revisions, corrections and evidence dependencies.
The on-chain SQLite memory holds shared cases and bounded request jobs; the
account/forecast store keeps its existing role. New EVM requests require a
network. ADR 0028's assumption that hex means Robinhood is superseded for new
investigations; historical records and existing Robinhood readers are preserved.

The public investigator has read tools only. User text, source code and prior
cases remain untrusted evidence. Code controls factual authorization and risk
levels. Initial limits: two planning calls, one writing call, two transfer hops,
existing shared RPC ceilings, 45 seconds per model call, three minutes overall.
Costs are reserved before calls and recovered conservatively after a crash.

Free Library browsing and authenticated lead/correction submissions consume
this state. Missing admission, storage, worker or budget disables intake. Paid
requests, financial execution, forecasts and reputation are subsequent work.
Startup funding and current runtime spending limits remain unchanged. Builds
do not prove live coverage: captures, recovery checks and Josh's reply review
remain launch gates. Production posting and deployment gates still apply.
