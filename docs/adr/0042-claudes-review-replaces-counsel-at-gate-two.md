<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0042 — Claude's review replaces counsel at gate two

**Date:** 2026-09-26
**Status:** accepted. **Josh's decision, recorded**, from 2026-09-26.
**Amends:** [ADR 0039](0039-the-launch-gates-and-the-monthly-ceiling.md)
gate two ("the review packet goes to counsel first; material objections are
resolved before launch").
**Reasoning:** [design 0030](../design/0030-launch-review-packet.md), the
review packet; [research 0063](../research/0063-launch-copy-for-counsel.md),
the launch copy checked against it.
**Order of work:** [plan 0002](../plans/0002-bot-quality-then-a-solana-launch.md).

## Decision

No outside counsel or accountant is retained for this launch. Josh chose
this; the choice is recorded here, not recommended, and no later document may
reintroduce hiring counsel as a recommendation without a new decision from
Josh.

| # | decision |
|---|---|
| 1 | ADR 0039 gate two is amended: instead of counsel reading design 0030 and resolving material objections, Claude performs the legal and tax review, checking design 0030's review packet against research 0063's launch copy. |
| 2 | The review is written down as a numbered research document (research 0065, not yet written) that answers every question design 0030 asks, each with a dated source, the same evidentiary bar as AGENTS.md §1. |
| 3 | The review states plainly, in its own text, that it is not from a licensed lawyer or accountant (AGENTS.md §1: say what kind of evidence a document is). |
| 4 | Every finding the review raises is either fixed (in code, copy or a further ADR) or ruled on by Josh, one by one; gate two is met once every finding has one of those two outcomes, not on the review's existence alone. |

## Consequences

- [`deploy/LAUNCH.md`](../../deploy/LAUNCH.md) gate two now names the review
  and this ADR instead of counsel.
- [Design 0030](../design/0030-launch-review-packet.md) and
  [research 0063](../research/0063-launch-copy-for-counsel.md) each gain a
  one-line pointer to this ADR; neither document's questions or quoted copy
  change otherwise.
- ADR 0039's gate-two row carries a note that it is amended by this ADR;
  gates one, three, four, five and six are untouched.
- Nothing here changes who signs or spends: ADR 0037's rule that model
  judgement never moves money, and Josh's exclusive signing of the launch
  transaction (ADR 0039 gate three), stand exactly as they are.
