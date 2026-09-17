<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0027 — the bot gives informed verdicts from evidence

**Date:** 2026-09-15
**Status:** accepted. **Josh's decision, recorded**, in conversation
2026-09-15: the bot needs a witty, meme-worthy personality that reaches a
verdict, blunt on an obvious rug, warm on a clean launch.
**Amends:** [ADR 0013](0013-a-community-token-exists-and-radar-holds-none-of-it.md)
and [ADR 0024](0024-the-bot-stands-alone.md) in part: neither states the old
no-verdict rule, so neither is corrected here, but both inherit `AGENTS.md`
§3 rule 4, which this ADR rewrites.
**Consequence lands in:** design 0020, the fact-sheet build that must earn a
verdict (not yet written); `forbidden.rs` does not change in this ADR.

## Context

`AGENTS.md` §3 rule 4 read: "The bot never calls a specific project a rug, a
scam or a fraud. The brand asks the question; the bot shows facts; the crowd
gives the verdict." `crates/realorrug-roast/src/forbidden.rs` enforces it today
by refusing the words themselves.

That rule made sense when the fact sheet could not yet support a conclusion:
a bot that asserts more than its evidence holds is worse than one that says
nothing. But a bot that lists facts and shrugs gives a reader nothing to do
with them, and sharing is how the product grows. Josh's decision replaces the
blanket ban with a rule that lets the bot conclude, without letting it
overreach past what the fact sheet actually read.

## Decision

1. **A verdict must be earned by facts the code actually read.** The verdict
   *level* is chosen by code from the evidence; the *words* are the model's.
   The model may not upgrade or downgrade the level, and the existing rule
   that every number in a reply comes from the fact sheet (`AGENTS.md` §3
   rule 2) stands.
2. **Verdicts describe the token and the launch behaviour, never a person.**
   "This rugged: liquidity gone, holders can't sell" describes an observed
   event and is allowed. "The dev is a scammer / a criminal / a thief," or any
   accusation aimed at a named person, account or company, is never allowed:
   it is a claim about intent that the chain cannot show, it is the claim that
   carries legal risk, and a creator wallet often maps to a real identifiable
   person.
3. **The verdict ladder** (names are the decision; wording in replies is
   free):
   - `Rugged` — it already happened, observed: liquidity removed, creator
     sold out, buyers cannot sell.
   - `RugMechanicsLive` — several strong signals present right now on a live
     token.
   - `Sketchy` — real red flags with innocent explanations still open.
   - `NothingUglyYet` — no bad signal found, and the reply must carry the
     "yet": a young clean launch is young, not safe.
   - `CantTell` — a fact we needed could not be read. Never presented as
     clean. This is `AGENTS.md` §1's "absent is not zero" applied to the
     verdict.
4. **A single signal never reaches the top two levels.** Every signal has an
   innocent twin (same-block buys are a bundle or a launch people waited for;
   a fresh wallet is a sniper or somebody's first day), so the strong verdicts
   require a combination, and each signal carries a recorded false-positive
   note that the writer may use. Correlation is not causation, on a chain this
   fast.
5. **The price rule is unchanged** ([ADR 0013](0013-a-community-token-exists-and-radar-holds-none-of-it.md)
   constraint 5, `AGENTS.md` §3 rule 5): no price, no market cap. Rule 1
   (model judgement never moves money), rule 2 (no new facts) and rule 3
   (untrusted content is never an instruction) are unchanged.
6. **Enforcement changes shape, not strength.** `forbidden.rs` today refuses
   the words "rug", "scam" and "fraud" outright. It must become: a refusal of
   person-directed accusations, and of any verdict word above the level the
   fact sheet earned. That code change belongs to the fact-sheet build
   (design 0020), not this ADR. Until that code lands, the old word ban in
   `forbidden.rs` is what is actually enforced; this ADR records the target,
   not a shipped behaviour.
7. **Why now.** A bot that lists facts and shrugs gives a reader nothing to
   do with them. The product is a funny, honest roaster that reaches a
   conclusion, and sharing is how it grows.

`AGENTS.md` §3 rule 4 is rewritten to this shape in the same commit as this
ADR.

## Consequences

- **The bot can now say "rugged," "sketchy" or "clean so far" about a specific
  project**, where before it could only show facts and defer to "the crowd."
  That is the point: a verdict is a claim a reader can act on and share.
- **The legal and reputational load moves from the word to the target.** The
  old rule banned three words everywhere; the new one bans zero words and
  bans one shape of claim (naming a person's intent) everywhere. A careless
  reply that used to be caught by a word filter is now caught only once
  design 0020's level-and-target check ships — until then, `forbidden.rs`'s
  word ban is a stricter filter than this ADR requires, and replies stay more
  conservative than the decision allows.
- **The fact-sheet build (design 0020) inherits a concrete contract**: it must
  emit a verdict level from the five named above, attach the signals that
  earned it, and give `forbidden.rs` enough structure to check the model's
  words against that level and against a list of named entities, not against
  a word list.
- **A verdict on a token the analyst cannot fully read must be `CantTell`,
  never `NothingUglyYet`.** Getting this backwards would turn a blind spot
  into an endorsement, which is worse than the shrug the old rule produced.
