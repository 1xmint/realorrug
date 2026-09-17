<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0030 — the model fills a reply plan it cannot write around

**Date:** 2026-09-17
**Status:** **superseded by
[ADR 0031](0031-the-model-picks-the-story-and-the-evidence-licenses-the-joke.md),
and never accepted.** It was a recommendation Josh read and declined on
2026-09-17, in favour of keeping the writing with the model and binding
figures to their subject instead. Two of its arguments were wrong; ADR 0031
records which, so the reasoning here is read with them in view. Nothing in
it was built.
**Amends:** [design 0020](../design/0020-robinhood-fact-sheet-and-voice.md)
§4 "The voice", which has the model write the whole reply as free text.
`AGENTS.md` §3 rules 2 and 4 are unchanged — the point of this ADR is to
make rule 2 true, not to rewrite it.
**Consequence of:** [design 0026](../design/0026-the-trader-dossier.md) §1
and §7, and Josh's instruction of 2026-09-17 that the bot should read like a
trader who actually dug into the coin.
**Consequence lands in:** `crates/realorrug-roast/src/voice.rs`,
`fidelity.rs` and `verdict.rs`; design 0026 §10 holds slice 1 until this is
decided.

## Context

Josh asked for a bot that "says what it thinks", that feels like an expert
on the other side of the conversation rather than a data generator. The
obvious way to give him that is to hand the model more room: a richer fact
sheet, a looser prompt, better prose. That is what design 0020 §4 describes
today.

Two facts make the obvious way the wrong way.

**Rule 2 is not enforced.** `AGENTS.md` §3 rule 2 says the model may not
introduce a fact, and `fidelity::check`
(`crates/realorrug-roast/src/fidelity.rs:57`) is the check that is supposed
to hold it. It compares *numerals*. "The deployer has done this before",
"the same funder paid for eleven of these", "this looks like the wallet set
from last week" — none of those contains a digit, and all of them pass every
check the repository has. Today's fact sheet is thin enough that the model
has little to invent with. Design 0026's dossier is not: it adds funders,
wallet clusters, deployer history and timing patterns, which is precisely
the material a fluent model will connect into a claim nobody read from the
chain.

**The failure mode is not embarrassment, it is a false accusation.** An
invented number is wrong and correctable. An invented link between a wallet
and a named person is the thing `AGENTS.md` §3 rule 4 exists to prevent, and
it is the one kind of mistake that costs more than the product earns.

So the choice is not "free prose or dull prose". It is "free prose with rule
2 unenforced, or constrained prose with rule 2 actually true".

## Decision

1. **The model's output is a typed reply plan, not prose.** It chooses: the
   finding to lead with, the sheet facts that support it, a register, and at
   most one *opinion angle* — a nonfactual line such as "the exit door was
   painted on". Everything else is code.

2. **Opinion angles are a fixed set owned by code, and a level unlocks
   them.** They carry no facts, so they cannot invent one, and an angle
   available at `Rugged` is not available at `CantTell`. This is where the
   personality Josh asked for lives: the angles are written to be funny and
   blunt, and there can be many of them. What there cannot be is a new one
   the model writes at answer time.

3. **Code renders the factual clauses and appends the verdict.** Each clause
   is generated from the sheet fact it cites, so a number in a reply is a
   number that was read, by construction rather than by a check that runs
   afterwards.

4. **`fidelity::check` becomes a plan validator.** It stops grepping prose
   for digits and instead refuses a plan whose cited facts are absent from
   the sheet, whose angle the level does not unlock, or whose finding the
   evidence does not support. A check on a structure can be complete; a
   check on free text cannot.

5. **The fallback composes through the same renderer.** The deterministic
   reply is the same code path with the model's choices replaced by defaults
   — lead with the strongest finding, no angle. Falling back then produces
   a shorter answer, not a different kind of answer. Design 0026 §1 measured
   that every reply the bot has sent so far fell back, and that the one Josh
   complained about fell back because a check refused the model's draft, not
   because the provider was down. So this is the path the reader has actually
   been reading, and it is the part that fixes what he saw.

## What this costs

**It gives up the thing Josh asked for, in one specific sense.** He wants
the bot to sound like a person who thinks. Under this decision it sounds
like a person who thinks *within a vocabulary someone wrote down first*. If
the angle set is small or flat, the bot will read as a template with jokes
bolted on, which is the failure this whole effort is meant to end.

That risk is real and it is managed by writing enough angles, not by
loosening the rule. The angle set is cheap to grow — it is a list, and
adding to it needs no model change and no new data — so the fix for "it
sounds repetitive" is a pull request, not an architecture change.

**It is more code.** Free prose needs a prompt; this needs a plan type, a
renderer, an angle catalogue and a validator. That is the price of a
guarantee that holds at answer time rather than one asserted in a document.

## Alternatives rejected

**Keep free prose and strengthen the checker.** A checker that could catch
"the same funder paid for eleven of these" would have to understand the
claim, which means a second model, which means the same problem one layer
down with a bill attached. Rejected on both counts.

**Keep free prose and accept the risk, because the model is good.** This is
the honest version of the status quo and it has one answer: rule 2 is either
a rule or it is decoration, and a repository that enforces its rules with
tests should not keep one that is enforced with optimism.

**Let the model move the verdict level too.** Not considered seriously — ADR
0027 settled that, and nothing here reopens it.

## How we would know it was wrong

If, after a month of real answers, readers respond to the angles as
catchphrases rather than reactions, the constraint is too tight and the next
decision is to widen it deliberately — with a fidelity check that has been
rebuilt first, not removed.
