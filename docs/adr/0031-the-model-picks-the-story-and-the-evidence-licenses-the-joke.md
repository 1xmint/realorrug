<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0031 — the model picks the story, and the evidence licenses the joke

**Date:** 2026-09-17
**Status:** **accepted.** Josh's decision, 2026-09-17: *"go with your
recommendation. over all the bot should be twitter viral worthy."*
**Supersedes:** [ADR 0030](0030-the-model-fills-a-reply-plan-it-cannot-write-around.md),
which proposed a fixed plan the model fills in. It was never accepted, and
two of its arguments were wrong. Both are recorded below, because the next
person to feel the same pull should find the reason it was resisted.
**Amends:** [design 0020](../design/0020-robinhood-fact-sheet-and-voice.md)
§4 "The voice". `AGENTS.md` §3 rules 2 and 4 are unchanged; this ADR exists
to make rule 2 true rather than to rewrite it.
**Consequence of:** [design 0026](../design/0026-the-trader-dossier.md) §1
and §7, Josh's instruction of 2026-09-17 that the bot should read like a
trader who dug into the coin, and his ruling the same day that virality is
the product's first priority.
**Consequence lands in:** `crates/realorrug-roast/src/fidelity.rs`,
`sheet.rs` and `voice.rs`. Design 0026 §10 slice 1 is unblocked by this.

## The decision

The model keeps the writing. It reads every flag on the sheet, chooses what
leads, chooses what supports it, sets the order and sets the register. What
constrains it is not a shape but a licence: **a figure may only be published
about the thing it was measured about, and a joke is licensed by the
specific evidence it is about, never by the verdict grade.**

Three consequences, each of which is a piece of work:

1. **An authorised number carries its subject.** `FactSheet::authorised`
   returns values paired with the measurement they came from, and
   `fidelity::check` refuses a number used about a different subject. The
   largest holder's 41% may not become "the creator already dumped 41%".
2. **Personality lives in the system prompt, not in a catalogue.** There is
   no fixed menu of jokes keyed to verdict levels. The prompt describes a
   voice; the model writes in it.
3. **Verdict level grants nothing.** No phrase is unlocked by reaching
   `Rugged`. A metaphor that asserts a fact — that buyers were trapped,
   that the pool was pulled — is permitted by the specific fact being on
   the sheet, and by nothing else.

## Why not the fixed plan

ADR 0030 proposed that code write the factual sentence and the model fill
decorative slots around it. That is safer in a narrow sense and wrong in
the sense that matters here. Josh's ruling is that the bot has to be worth
sharing, and a reply assembled from slots is recognisable as assembled.

The measurement, from reading a run of comparable replies: a reader notices
a fixed pattern somewhere around the fifth to tenth reply of the same kind.
But repetition on its own is not what kills it. A bot with a recurring
fixation reads as a personality. **A joke that fires whatever the evidence
says reads as a machine**, and that is the property a verdict-keyed
catalogue has by construction. This is the line the design has to hold: the
same joke twice is fine, the same joke where it isn't true is not.

## The two things ADR 0030 got wrong

Recorded because they were persuasive.

**Verdict level is not evidence for what a metaphor means.** ADR 0030
unlocked stronger angles at stronger grades, which treats "the code allowed
this grade" as "the thing the joke asserts is true". It is not.
`Rugged` is reachable through `LiquidityGone` plus `HolderConcentration`
with no `BuyersCannotSell` at all (`crates/realorrug-roast/src/verdict.rs:119`),
so "the exit door was painted on" could fire on a token where nobody was
ever stuck. The grade is a summary of several facts; a joke is a claim
about one of them.

**"Every reply fell back" was not evidence the model needed constraining.**
ADR 0030 leaned on it. Design 0026 §1's own diagnosis attributes those
fallbacks to a broken gate in `forbidden.rs`, since fixed. Using a
symptom of a bug as proof that a design is unsafe is the error, and it is
mine.

## What the checks cannot do, stated plainly

`fidelity.rs` checks numbers. It does not check claims, and this decision
does not pretend otherwise. "The biggest holder is the deployer" contains
no digit, is false when the biggest holder is a pool, and passes every
check the repository has. "Love how the sell button only works for the
house" names nobody and counts nothing, and asserts that ordinary holders
cannot sell.

Two answers, and they are different in kind:

- **The subject rule closes the demonstrable hole.** Misattributing a
  measured figure to the wrong actor is a concrete false sentence with a
  concrete fix, and it is the failure most likely to get the bot dunked on,
  because the accusatory direction — creator, deployer, dev — is the one
  that gets screenshotted.
- **The claim problem is measured before it is gated.** Per codex's
  suggestion, claim extraction runs against drafts as a shadow evaluation
  that records what the model asserted and whether the sheet supports it.
  It does not block publication until we know its false-refusal rate. A
  publication gate whose behaviour nobody has measured is how a bot ends up
  posting nothing.

## Cost

Replies get slower to develop, because the voice is now a prompt-and-measure
loop rather than a table of strings. The subject rule will refuse some true
sentences — a reply that names the creator and cites a venue-wide rate in
the same breath is the shape most at risk — and the shadow evaluation is
where that rate becomes visible before it becomes a fallback in production.

## What would change this

A measured false-claim rate high enough that numbers are not the problem.
If the shadow evaluation shows the model asserting unsupported
non-numeric claims at a rate readers would notice, the honest response is
to narrow the voice, and this ADR is the thing that gets superseded.
