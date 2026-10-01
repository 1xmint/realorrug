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

**Do not equate delivery with launch readiness.** Summaries are intentionally
partial `CantTell`. Complete claim resolution, historical interval coverage,
broader LP/control semantics and representative deployed Clanker/Flaunch cases
remain qualification/development work. Current public endpoint failures and an
unsupported newer Clanker locker are recorded as gaps. The final live quality
set and owner acceptance remain open; ROADMAP milestones 1/2 are not accepted
by these builds. Treasury ledger and joint-launch gates are subsequent work.
