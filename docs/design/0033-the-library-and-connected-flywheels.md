<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0033 — the Library and connected flywheels

**Date:** 2026-09-30
**Status:** accepted reasoning for [ADR 0043](../adr/0043-the-public-library-and-compounding-intelligence.md).
Product direction lives in [VISION.md](../../VISION.md); delivery in
[ROADMAP.md](../../ROADMAP.md). This note explains the choices without becoming
a second vision or a detailed implementation schema.

## A public investigation should leave a useful asset

Josh wants people to bring specific token allegations and wallet/transaction
leads, then build on each other's findings. A generic reply can attract
attention, but the accumulated asset requires a shared, revisable case and
an agent that retrieves it during later work. A Library makes that asset
accessible beyond an X thread and allows evidence, corrections and unknowns
to survive restarts and changes in social distribution.

The user's fee-diversion example requires an explicit fee definition, period,
denominator and route. Storing the allegation as verified knowledge would
compound error. Preserve claims, observations and inference separately.
Historical facts remain about their observation time; corrections update
dependent assessments without erasing the earlier record.

The five flywheels separate acquisition, accumulated intelligence, funding,
token participation and trust. F2 is named **Compounding Intelligence** because
the asset includes relationships, mechanism patterns, legitimate explanations,
outcomes, playbooks and evaluation cases. Each needs a consumer in the next
investigation. More storage or repeated allegations alone does not improve
the investigator. User-selected suspicious tokens also cannot establish
market-wide outcome rates; ordinary and unresolved cases matter.

## Library first, paid work later

Josh explicitly selected a public Library at the joint investigator/token
launch, with the paid portal later. This gives the free product a useful case
history and lets us measure costs before selling different investigation
depths. Existing facts-only payment code is a foundation, not a tested paid
investigation product.

The later buyer pays for useful fresh work and access, while completed
findings enrich the public Library. A paid investigation has greater scope
where priced for it, not different standards or a guaranteed conclusion.
Show existing coverage so a buyer can choose meaningful work. Immediate
progress acknowledges a request; a completed dossier acknowledges fulfillment.
Paid capacity, settlement recovery, duplicate-charge prevention and durable
retrieval must precede sales. Broad batches require a defined universe and
coverage manifest, not a marketing claim of all historical tokens.

## A small launch should preserve options without making promises

Solana/pump.fun with SOL pairing remains selected because the repo already
has relevant launch and treasury readers. Base/Flaunch is a meaningful
alternative for native token-support mechanisms; Clanker exposes a relevant
compute/runway model; Virtuals offers agent commerce. Their custody and
administrative dependencies must be compared alongside convenience. Harmonic
is useful for public decisions and named knowledge consumers, not proof that
an agent wallet lacks service or owner control.

Josh changed the funding constraint from an initial $90 monthly planning
ceiling to a $100–$200 startup contribution, capped at $200 and free-first.
This does not edit the existing runtime meter or imply unlimited model/API
access. Future revenue-funded work needs explicit cost and admission controls;
startup funding is not an ongoing subsidy.

Reserve, burn and permanent liquidity have distinct purposes. Runway protects
service continuity; a token burn reduces supply; paired liquidity supports
trading and relinquishing LP ownership constrains withdrawals. Liquidity assets
remain tradable and change composition. Allocation percentages require a
denominator and funded obligations. A split can otherwise spend more than
receipts: $100 collected, $90 operating cost, $25 burn and $25 liquidity leaves
a $40 deficit. This arithmetic is illustrative, not a revenue forecast.

The proposed 25% burn / 25% liquidity split is therefore saved as a proposal.
It is not a launch requirement or a promise. The launch default remains
disclosed operations and a bounded reserve; financial automation can be
considered once separately constrained execution and its controls are verified.
The model can research and report on finances, but cannot move money.

## What remains implementation work

Use the existing React/Rust/SQLite foundation. Design the smallest bounded
claim-preserving investigation and durable-case slice first, then let the
Library and follow-up surfaces consume it. Reuse useful storage/check work
without assuming library methods have public callers or that historical
designs are implemented. Preserve the forecast/reputation work for later;
its completion is not needed to establish a useful forensic Library.

The vision's references are documentation-based research. Actual fee routing,
final shares, custody, burn/LP effects and payment behavior require captures
before public claims or a financial implementation decision.
