<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0003 — three-chain investigator and Library

**Date:** 2026-10-01. **Status:** implementation delivered for review; launch qualification open.
**Decision:** [ADR 0044](../adr/0044-three-chain-investigations-and-shared-cases.md).
**Direction:** [VISION](../../VISION.md), [ROADMAP](../../ROADMAP.md).

## Ordered delivery

- [x] Shared chain-qualified cases, durable requests, evidence and corrections.
- [x] Claim-preserving Solana reads, bounded planner and local reply review.
- [x] Base/Ethereum shared EVM reads and deployment-scoped protocol adapters.
- [x] Free Library, authenticated contributions and request progress.
- [x] Recovery, backup, provenance/fidelity tests and capture/review tooling.
- [ ] Representative live protocol qualification and Josh's reply acceptance.

Three chains are required before launch. Full Rust suites run in CI; local
checks are scoped builds and individual named tests. Paid investigations,
financial automation, deployment and production posting are outside this work.

## Handback / current evidence

Implementation started 2026-10-01 from the owner's local checkout. Existing
documentation edits and user attachments are preserved. No deployment, token
launch, signing, production post or paid model call occurs in this slice.

[Research 0066](../research/0066-the-investigator-and-library-qualification.md)
records implementation boundaries and verification. Live Solana, Base and
Ethereum observations produce offline review dossiers in its capture directory;
the reviewed database snapshot restores with a matching checkpoint. Tests cover
restart/idempotency, corrections and relationship reuse, reorg/network mismatch,
injection/tamper refusal, accounting failure, authenticated CSRF intake and
duplicate recovery. After reconciling the forecasting pages from main, the site
passes 129 tests, type checking and build. Browser
review verified the populated Library and mobile dossier wrapping. Scoped
all-target Clippy passes; full suites/mutation checks run in CI.

[Research 0068](../research/0068-base-fee-routing-and-retained-controls.md)
adds current Flaunch pool/escrow/NFT snapshots, retained-control simulations
and a supported Clanker refresh. Actual beneficiary receipts, newer locker
qualification and owner reply acceptance remain subsequent work.

[Research 0071](../research/0071-base-fee-credits-and-balance-windows.md) records
the 2026-10-04 continuation: bounded credit decoding with a positive Clanker
receipt, named role/checkpoint regressions, capture/replay and two measured
wallet/asset balance windows. Full suites and mutation gates run on PR 210.
Research windows are not yet automated investigator reads; per-token funding
origin, backing, native/conversion delivery and the treasury ledger remain open.
The current Clanker locker is now supported as recorded in research 0069;
the unsupported-locker observation below describes the original qualification.

[Research 0072](../research/0072-clanker-transaction-balance-windows.md) records
the next continuation: the transaction reader now automatically reconciles a
single Clanker wallet/asset key over the submitted transaction's block. Two
live captures and offline reviews retain balance checkpoints and credit/claim
linkage. This supersedes the previous paragraph's non-automated status for this
one-block scope; wider windows and the other attribution/ledger gaps remain open.

[Research 0073](../research/0073-clanker-deposit-funding-provenance.md) records
the next slice: receipt-level funding evidence matches one preceding requested
Clanker deposit transfer, refusing ambiguous or incomplete evidence. A live
capture, retained provider failures, historical depositor/source/pool-key
qualification and an offline dossier preserve attribution limits. Received
credit remains independently measured, and the inspected collection concerns
a different token. Automatic historical pool attribution and deposit-specific
short reply prioritization remain the next boundary ahead of the ledger.

[Research 0074](../research/0074-clanker-historical-collection-context-and-replies.md)
implements the next bounded slice: deposit ingress and independently reconciled
credit lead replies, followed by qualified historical collection context for
one reviewed Base LP locker. Seven shared-budget calls verify runtime,
dependencies, registration, recipient and position linkage; block-end state
and unresolved fee origin remain explicit. Live deposit/claim captures,
retained rate-limit failures and offline reviews supersede the preceding
paragraph's open reply-prioritization status. Per-token revenue attribution,
event-time configuration, conversion/shared PositionManager analysis and owner
acceptance remain open before a ledger projection.

[Research 0075](../research/0075-clanker-quiet-block-reward-configuration.md)
implements a bounded quiet-block reward-configuration stability check, with
six shared-budget reads, exact anchor coverage and parent/closing rechecks.
Live qualification, positive deposit/claim captures, failed provider runs and
offline reviews retain source/provider assumptions and deadline gaps. This
narrows the preceding timing gap for this one quiet-block scope; busy-block
change histories, fee preferences, registry history, conversion/shared
PositionManager/fee-origin proof and owner acceptance remain open.

[Research 0076](../research/0076-clanker-ordered-reward-configuration-history.md)
extends this to supported recipient/admin changes in other transactions in the
same block. Replay checks old values and closing reconciliation, retains round
trips and uses the reconstructed collection tuple for recipient linkage.
Synthetic busy-block tests and a fresh quiet-block capture are retained; live
busy-block qualification, same-transaction changes, fee preferences, registry
history, conversions and per-token revenue remain open. CI distributes the
complete mutation set over sixteen shards without relaxing limits.

The credit slice is committed as `0bf05a3`. Named tests
`credit_events_preserve_requested_cumulative_and_caller_supplied_roles`,
`malformed_credit_events_and_incomplete_receipts_cannot_verify_payments` and
`retained_clanker_credit_is_an_accrual_observation_not_a_treasury_receipt` prove
the new behavior; the first also rejects a manually introduced false delta.

**Do not equate delivery with launch readiness.** Summaries are intentionally
partial `CantTell`. Complete claim resolution, historical interval coverage,
broader LP/control semantics and representative deployed Clanker/Flaunch coverage
remain qualification/development work. Current public endpoint failures and an
unsupported newer Clanker locker are recorded as gaps. The final live quality
set and owner acceptance remain open; ROADMAP milestones 1/2 are not accepted
by these builds. Treasury ledger and joint-launch gates are subsequent work.
