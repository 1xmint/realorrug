<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0066 — investigator and Library qualification

**Date:** 2026-10-01. **Status:** initial implementation evidence; launch qualification open.
**Direction:** [ADR 0044](../adr/0044-three-chain-investigations-and-shared-cases.md),
[plan 0003](../plans/0003-three-chain-investigator-and-library.md).

## Implemented boundary

One React/Rust/SQLite path preserves the allegation, chain, target, submitted
wallets/transactions and requested time window. A durable queue precedes work;
append-only case events precede publication. Cases are isolated by network and
address; X thread associations survive restart. Contributions are claims until
readers establish evidence. Correcting an observation invalidates its derived
relationships and dependent findings while preserving original revisions.

The planner selects registered tools and established leads within two planning
calls, one evidence-selection call and two transfer hops. Every model call
reserves and persists the existing shared spending allowance before dispatch.
Unknown effects remain charged. A storage failure prevents subsequent model
calls and publication; interrupted work is quarantined instead of retried.
The enabled worker rejects an unreadable existing ledger and uses an OS lease.

Exact reader-authored statements authorize findings. The model can rank those
statements, but cannot introduce facts or select a verdict. **Initial results
remain partial `CantTell` assessments**, including successful bounded reads.
This does not yet implement a complete claim-resolution/risk assessment engine.
Prior observations supply dated leads and must be read again for current claims.
The initial mechanism library includes legitimate fee splitters, migration,
shared infrastructure, upgrades and state changes as alternative explanations.

## Current reader coverage

| Reader | Implemented observations | Explicit limits |
|---|---|---|
| Solana token/accounts | Program ownership, initialized mint, mint/freeze authorities | Token-2022 extensions and nonstandard powers need deeper readers |
| Pump fees | SharingConfig version/layout, shares/denominator, admin status; otherwise curve creator and derived fee vault with bounded decreases | Configuration is not payment; creator vaults aggregate tokens; beneficiary/onward routing and platform powers remain unresolved |
| Pump/PumpSwap liquidity | Curve state and graduation; bounded discovery of a verified PumpSwap pool with atomic raw reserves | Other venues, LP rights, history and migration beyond discovered candidates remain unresolved |
| Solana transactions/history | Successful System transfers and this mint's TransferChecked; one recent signature page | Unchecked/extension transfers, complete wallet history and interval coverage remain gaps |
| Base/Ethereum generic | Chain identity; current block/hash; code, supply/decimals, owner getter, ERC-1967 slots; canonical successful receipts and this token's Transfer logs | Standard getter/slot absence does not prove no powers; no generic beneficial-ownership, internal-call or full-history claim |
| Base Clanker | Canonical v4.0 registry and two known lockers; reward recipients, shares/admins and position registration | Other generations/extensions, realized fee payments and withdrawal/upgrade powers require qualification |
| Base Flaunch | Two normal hooks; pool key, fee distribution/split, creator, owner, calculator and bounded distribution events | Referral waterfall, escrow vs receipt, dynamic fees, custom managers, Any/Game generations and bid-wall rights remain unresolved |
| Uniswap v2/v3 | Known Base/Ethereum factories and wrapped-native pairs; v2 reserves, supply/zero/dead LP balances and feeTo; v3 tier/liquidity/slot state | Other pairs, remaining LP holders/lock expiry, v3 NFT withdrawal rights and realized fees are not fully covered |
| Source | Fixed Sourcify v2 endpoint, runtime-code match, bounded interface names and response hash | Source verification is not an audit; full ABI/code reasoning and implementation semantics remain future qualification |

Requested time windows are retained but recent signature/log reads do not yet
prove that interval was exhaustively scanned. Instrument compute units bound
work; they are not RPC invoice units. No universal chain/protocol coverage is
advertised. Robinhood retains its existing checker; new hex intake needs a
network rather than inheriting the old default.

## Source checks and captures

Deployment/layout starting points were checked against the
[Pump IDL](https://github.com/pump-fun/pump-public-docs/blob/main/idl/pump.json),
[Pump fee IDL](https://github.com/pump-fun/pump-public-docs/blob/main/idl/pump_fees.json),
[Clanker v4 source](https://github.com/clanker-devco/v4-contracts),
[Flaunch source](https://github.com/flayerlabs/flaunchgg-contracts),
[Uniswap deployment documentation](https://docs.uniswap.org/contracts/v3/reference/deployments),
[ERC-1967](https://eips.ethereum.org/EIPS/eip-1967) and
[Sourcify API v2](https://docs.sourcify.dev/docs/api/index.html).
Documentation and deployment addresses are starting points, not runtime
qualification or proof that all administrative powers are understood.

The [capture directory](data/0066-investigator) retains requests, live reader
outputs and offline assessments/review reports. Initial Solana reads exposed a
wrong duplicated program address, replaced with the repository's verified
program constant. Preserve that failed capture beside the corrected read.
Initial Base public reads returned rate limits; the first Ethereum relay
returned a transport error. Those outputs establish missing coverage rather
than token safety. Subsequent reads are separate operator captures, not silent
automatic retries. Capture timestamps and read points govern their freshness.

Network/target starting points: [Base network details](https://docs.base.org/get-started/connect-to-base),
[Circle USDC addresses](https://developers.circle.com/stablecoins/usdc-contract-addresses),
[1RPC public endpoint listing](https://www.1rpc.io/) and
[PublicNode Ethereum endpoint](https://ethereum.publicnode.com/).
No subscription, signing, paid model call or production post is used here.
Captures retain compact reader output, not every raw RPC payload. Stablecoin
controls are a generic/unsupported-launchpad control case; they do not qualify
Clanker or Flaunch launch tokens. No owner acceptance is inferred from a build.

After the constant correction, Solana reports the creator route, native vault
balance, mint authorities and current curve state. Base reports token getters
with liquidity rate-limit gaps. Ethereum reports token getters and a verified
v2 pool, reserves and LP balances. A newly discovered Clanker token registers
with a newer locker outside the two supported addresses; it remains unresolved.
An established Clanker token discovered through the public API produced a
positive registered-locker configuration capture and an offline review:
[fee capture](data/0066-investigator/base-clanker-fees.capture.json),
[review](data/0066-investigator/review/base-clanker-fees.review.md).
Its configuration reports a 10000-basis-point LP reward share and one position;
realized payment and withdrawal rights remain unresolved. An earlier request
without an explicit fee allegation captured only generic reads; the subsequent
fee-specific request is retained separately. The capture stores quoted JSON
numeric strings in its text; later formatting removes those quotes without
rewriting this historical evidence.
The documented Flaunch discovery API returned HTTP 521, so no positive Flaunch
runtime qualification is claimed. These observations do not accept the full
representative quality set. Current Pump curves can use a quote asset; the new
reader labels reserve quantities as raw quote units rather than assuming SOL.

## Verification and release boundary

Named tests exercise restart recovery, network isolation, idempotent admission,
correction projections (including Library summaries), relationship invalidation,
snapshot restore, publication-attempt recovery, wrong-chain/reorganized blocks,
malformed ABI/fee shares, instruction injection, invented leads, stale memory,
unknown costs, accounting failure, missing configuration, authentication/CSRF,
lost-response recovery, offline provenance/tamper refusal and unsafe filenames.
Scoped all-target Clippy passes with warnings denied. The site passes its
129 tests after reconciling the forecasting pages from main, type check and
production build; browser review checks responsive
Library layout. The first CI run passed 2329 Rust tests, build, MSRV,
dependencies, formatting and the site. The second run passed 2347 Rust tests
and the ordinary build checks, but failed newer Clippy test-assertion rules and
all four mutation shards. Survivors fell from 275 to 150; this is progress,
not acceptance. The next revision adds boundary/refusal tests for model time,
two-hop leads, replay custody, protocol quotes/receipts, Solana transfer/pool
identity, EVM transactions and Library intake. No mutation gate is disabled or
excluded to obtain a pass. **CI acceptance remains open.** Full Rust suites
and mutation checks belong to CI.

Free browsing requires configured existing case storage, not a session. New
website work additionally needs authenticated identity/CSRF, shared daily
quotas, budget/prices and worker liveness. Browse/request rate limits are bounded.
Community contributions enter the same bounded investigation queue as unverified
questions; there is no separate unchecked event-append capability. Operator
corrections use the custody command and cannot be submitted as verified web facts.
Corrections apply to the exact-request assessment used for queued publication
as well as the latest dossier. Challenged findings therefore cannot be selected
as measured clauses for a pending reply; original assessments remain in history.
The mention poll advances its cursor after durable intake but does not count a
queued investigation as an answered mention; the worker records that outcome.
Unavailability never becomes an empty successful Library. The live publication
gate is separate from worker enablement and retains existing X approval rules.
Checkpoints detect changes to retained records; operator custody is not
independent immutability. Preserve/reconcile backups before enabling a worker
against important existing storage. No history pruning is implemented in this
slice; monitor growth and define archival policy before increasing quotas.

Before launch: qualify deployed Clanker/Flaunch and representative Solana/EVM
cases, demonstrate the required fee-diversion/legitimate-splitter/migration/
ambiguous-role/shared-infrastructure/partial/stale/correction cases, accept
useful replies, verify capacity within actual funded RPC/model/X limits, and
finish the financial ledger and joint-launch gates. Before deeper conclusions:
implement code-authorized claim resolution, broader powers/LP rights and
historical interval coverage. Paid portal, bulk jobs, forecasts, reputation and
financial execution remain outside this implementation slice.
