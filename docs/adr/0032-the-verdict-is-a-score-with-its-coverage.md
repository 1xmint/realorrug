<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0032 — the verdict is a score with its coverage

**Date:** 2026-09-17
**Status:** accepted. **Josh's decision, recorded**, in conversation
2026-09-17.
**Supersedes:** the no-score parts of [ADR 0027](0027-the-bot-gives-informed-verdicts-from-evidence.md)
(its five-level ladder, decision 3, as the whole of the verdict) and
[design 0026](../design/0026-the-trader-dossier.md) §4 line "Five levels
stay. A sixth would be a safety score, and this project does not publish
one." Everything else in both documents stands: the level is still code's to
set, the model still cannot move it, verdicts still never accuse a named
person, and the price rule is unchanged. A short superseded note is added at
the top of both documents in this commit.
**Consequence lands in:** `crates/realorrug-roast/src/verdict.rs`,
`sheet.rs` and `forbidden.rs`, and `docs/design/0020`. None of that code
changes in this PR; this ADR records the target and the order the missing
signals must land in first.

## Context

`crates/realorrug-roast/src/verdict.rs:11` opens: *"It is deliberately not a
score. `theradar:GOAL.md` refuses a single safety score... So `Verdict` carries
**reasons**, and a reply renders the reasons rather than the label."*
`forbidden.rs:19` repeats the same refusal (*"`theradar:GOAL.md` refuses a single
safety score for exactly this reason: 'a green shield is unknown rendered as
safe'"*), and its test at `forbidden.rs:1322` exercises it. `docs/design/0020`
line 53 and `docs/research/0042` line 36 both cite the identical `theradar:GOAL.md`
sentence to justify "no composite risk score, in either repo, on purpose."

All four citations trace to one place: `theradar`'s `theradar:GOAL.md`, Radar's own
goal document, not this repository's. There is no `theradar:GOAL.md` in `realorrug`.
The no-score rule was inherited wholesale from the sister project rather than
decided here, and nobody has re-asked whether Radar's reason for refusing a
score still holds once the analyst discloses how much of the evidence it
actually read.

Radar's reason was specific: *"a green shield is 'unknown rendered as
safe'"* — a single number invites a reader to treat "we don't know" as "it's
fine." That is a real failure mode, but it is a failure of hiding the
unknown, not a failure of using a number. A score paired with a visible
coverage figure does not hide what was unread; it names it. The fix for
"unknown rendered as safe" is showing the unknown, not refusing the number.

The current rule also loses information it already has. `verdict::level`
(`verdict.rs:119-145`) counts booleans: a `Rugged` pair, a live-risk count of
two-or-more, "any signal at all," or "nothing." Of the nine `Signal`
variants (`sheet.rs`), only four are ever pushed by production code today:
`LaunchBlockInStrongestBand` (`sheet.rs:454`), `CreatorBoughtOwnLaunch`
(`sheet.rs:460` and `sheet.rs:1092`), `CreatorNeverGraduatedOrganically`
(`sheet.rs:517`), and `RepeatLauncher` (`sheet.rs:546`). `LiquidityGone`,
`CreatorSoldOut`, `BuyersCannotSell` and `OwnerCanStillMintOrPause` are never
raised outside tests, so the `Rugged` level — which needs two of exactly
those four — is unreachable on a live token today. `HolderConcentration` has
an innocent-twin string (`sheet.rs:340`) and appears in `verdict.rs`'s
`Rugged` pair and in tests, but no code path ever computes and pushes it, so
it is printed as a concept and weighs nothing in practice. And because
`level` returns `CantTell` the moment `sheet.unknown` is nonempty — before
weighing anything else — one unread fact forces `CantTell` even beside three
strong signals that did read clean. A boolean count cannot represent "mostly
read, and what we did read is ugly" versus "barely read, one signal fired";
a score with its own coverage figure can.

Research 0046 (`docs/research/0046` line 172) is Radar's own caution against
the opposite mistake, and it does not apply here for a reason worth stating
plainly: Radar measured that combining its structural signals into a
buy/refuse *trading* decision produced no edge over holding — *"the
individual structural signals... show real statistical enrichment in
isolation, but Radar's own measurement found that combining them into a
buy/refuse decision produced no edge over holding."* That is a result about
predicting *price*, not about classifying rug-versus-real. `realorrug` does
not trade (`AGENTS.md` rule 1), so the negative result does not transfer
directly. What does transfer is the method: 0046 is first-party evidence that
weights should be *measured against outcomes*, not asserted from intuition
— which is the second half of this decision, below.

## Decision

1. **Code computes a 0–100 risk score from weighted factors** — every signal
   and every measured fact that bears on rug-versus-real, not only the
   current nine `Signal` booleans. A missing weighted factor is scored as
   absent, never as zero-risk: "absent is not zero, unknown is not safe"
   (`AGENTS.md` §3 rule 8) applies to the score exactly as it applies to a
   single signal today.
2. **A separate coverage figure travels with the score**: how much of the
   evidence that would move the score was actually read, as a fraction of
   the weighted total. A fact that was not read contributes to the
   denominator and not the numerator; it lowers coverage and never lowers
   (or raises) the score itself. This is what answers Radar's "unknown
   rendered as safe" objection without refusing the number: the reader sees
   both figures, never the score alone.
3. **The five levels become bands of score and coverage**, not a separate
   vote: `Rugged`, `RugMechanicsLive`, `Sketchy` and `NothingUglyYet` are
   score bands read only once coverage clears a floor, and `CantTell` is what
   the level reads as when coverage does not clear that floor — "coverage too
   low to say," replacing today's "any required fact unread" trigger with a
   graded one. The reply wording ADR 0027 built (blunt on `Rugged`, warm on
   `NothingUglyYet` with its "yet") survives unchanged; only what sets the
   band changes shape, from a handful of boolean rules to a threshold on two
   numbers.
4. **The model still cannot move the score, the coverage figure or the
   level.** `AGENTS.md` §3 rule 4 already says code picks the level; this ADR
   extends "the level" to mean "the score and coverage that produced it."
   Nothing about who computes it changes.
5. **Weights start hand-set and documented**, the same way today's boolean
   rule is hand-set and documented in `verdict.rs`'s doc comments — a number
   and a one-line reason for it, in code, not tuned in the dark. They are
   then fit against the creator index's measured launch outcomes: 532,226
   Robinhood launches, split organic / instant / stillborn. Research 0046's
   own caution (above) is the reason to prefer a fitted weight over a guessed
   one wherever an outcome-labelled population exists, rather than trusting
   hand-set numbers indefinitely.
6. **The caveat on the fit**: the creator index measures graduation —
   whether a launch went on to trade organically, instantly, or never traded
   at all — not "rugged." A launch can graduate and still rug later, and a
   launch can stay stillborn for reasons that have nothing to do with the
   creator's intent (no marketing, a bad ticker, launched at 4am). Fitted
   weights are therefore scoped to the factors the launch-outcome data can
   actually speak to: **launch-shape and creator-history factors only**
   (recipient-count band, dev-buy-at-launch, repeat-launcher, never-graduated-
   organically, and their kin). Factors with no analogue in the outcome data
   — liquidity removal, a simulated sell that fails, mint/freeze authority
   still live, holder concentration — keep hand-set weights until a
   comparably-sized outcome-labelled population exists for them, and that
   scoping is recorded beside the weights, not left implicit.
7. **Cost and order.** A score computed over today's four live signals is
   today's boolean count with a number stapled to it — no more informative
   than what shipped with ADR 0027, just harder to read. The missing signals
   come first, in this order, before the score is built on top of them:
   holder concentration (computed, not just declared), the LP-locked fact,
   an admin-withdraw scan (mint/freeze/pause authority still live), and
   Solana holder share (the fuller cross-chain read `HolderConcentration`
   implies today but does not yet compute). Only once those exist does a
   score drawn from them mean more than the ladder already means.

## Consequences

- **A reader gets two numbers instead of a label and a reasons list**: a
  0–100 risk score and a coverage percentage, with the five-level wording
  ADR 0027 established still carried in the reply as the band name. Nothing
  about `AGENTS.md` §3 rules 1, 2, 3, 5 or 6 changes; rule 4 is rewritten in
  the same commit as this ADR to say code computes the score and level, not
  that code "picks one of five levels."
- **`verdict.rs`'s "not a score" framing (its module doc, lines 11–24) is now
  wrong** and needs rewriting when the implementation lands — not in this
  PR, which is docs-only, but the doc comment's citation of `theradar:GOAL.md` should
  not survive past that point since it is Radar's document, not this
  project's decision.
- **The four unraised `Signal` variants and the never-computed
  `HolderConcentration` become blocking work, not background debt**: a score
  built before they exist would be presented as more informative than the
  four-signal ladder it replaces while covering the same ground, which is
  the opposite of what a coverage figure is for.
- **The fit against the creator index is scoped, not blanket.** Anyone
  extending the fit to a factor the 532,226-launch outcome set cannot speak
  to (see decision 6) is re-introducing exactly the "measure something and
  apply it somewhere it doesn't reach" mistake research 0042 and 0046 warn
  against elsewhere in this codebase.
