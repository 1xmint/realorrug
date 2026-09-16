<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0028 — one bot, every chain

**Date:** 2026-09-15
**Status:** accepted. **Josh's decision, recorded**, in conversation
2026-09-15, in his own words: "all chains that the bot uses = the same bot
and voice. Not robinhood has free text and solana has menu. same bot,
different chains. Meaning if we add a chain tomorrow, it would be easy."
This is a direction decision under `AGENTS.md` §2; it is recorded here, not
reopened, and nothing below argues the alternative.
**Amends:** [design 0020](../design/0020-robinhood-fact-sheet-and-voice.md)
§4 and §6, in the same commit as this ADR — see that document's changes for
the line-level edits. Does not amend [ADR 0027](0027-the-bot-gives-verdicts-it-can-prove.md):
the verdict ladder and the model/code split it settled are chain-blind
already and this ADR only makes that explicit (point 4, below).
**Consequence lands in:** `crates/realorrug-roast/src/clause.rs`,
`crates/realorrug-roast/src/render.rs`, `crates/realorrug-roast/src/voice.rs`,
`crates/realorrug-roast/src/sheet.rs`, `crates/realorrug-roast/src/verdict.rs`
(not yet written), and a new chain-reader seam in `crates/realorrug-onchain`
and `crates/realorrug-robinhood` — none of this is built yet; design 0020 was
still a draft when this ADR landed and is amended in place rather than
reopened.

## Context

Design 0020 §4 planned two different generation mechanisms for two chains:
Robinhood would get free-text generation, written by a cheap model and
checked after the fact; Solana would keep `voice.rs`'s clause-selection path
unchanged, where the model picks whole pre-written sentences and writes no
prose. §4 called this "a recommendation, not a decision already made" and
left the choice to Josh. Josh's decision closes that question the other way
from how the two-path plan read: one bot, one voice, every chain, and Solana
moves onto the same path Robinhood was getting, not the reverse.

The code today confirms the design's description of Solana is accurate and
current: `voice.rs`'s `SYSTEM` prompt tells the model its "ENTIRE OUTPUT IS A
LIST OF CHOICES" among `crate::clause`-defined sentences, and
`crate::clause::parse`/`assemble` substitute Radar's own text for that
selection before `forbidden::check`/`fidelity::check` ever run. Nothing in
`clause.rs` or `render.rs` is chain-specific — both operate on a `FactSheet`,
already venue-agnostic per design 0020 §1 — but the *mechanism* they
implement, choose-a-sentence rather than write-one, is the thing this ADR
retires as Solana's permanent path.

## Decision

1. **One bot, one voice, every chain.** Solana's replies move onto the same
   free-text-written-then-checked path design 0020 planned for Robinhood.
   Clause selection stops being "the Solana way" and becomes, at most, the
   fallback template every chain already falls back to
   (`verdict::template`, unchanged in role by this ADR). Concretely:
   `crate::clause`'s selection mechanism — the `SELECTABLE`/`CONTEXT` split,
   the `F<number>.<voice>` pick grammar, and `voice.rs`'s `SYSTEM` prompt
   that restricts the model to choosing among pre-written sentences — is
   **retired**, not kept as a second live path beside free text. `clause.rs`
   itself is not deleted outright: its `Fact::clauses`/`Voice` types and the
   *authoring* of vetted sentences are the raw material the deterministic
   template can keep drawing on, so the module is repurposed as the
   template's sentence library rather than run through `parse`/`assemble`
   against a model's reply. `render.rs`'s cleaning pass (bidirectional
   overrides, zero-width characters, the length cap) is **kept unchanged**:
   it runs on rendered text regardless of whether that text came from a
   selection or free generation, and moving to free text makes it more
   necessary, not less, since a model-written sentence has more room to
   carry a stray character than a hand-authored clause does.
   **Recommendation: retire the selection call path in `voice.rs`
   (`clause::parse`/`assemble` against a model answer), keep `clause.rs`'s
   types as the template's fixed sentence library, keep `render.rs` exactly
   as it is.** The reason is the same one design 0020 §4 already gave: the
   checks (`forbidden::check`, `fidelity::check`) already operate on
   arbitrary rendered text, not on the selection mechanism, so nothing about
   safety requires clause selection to survive as a second generation path —
   only as source material for the one path (the template) that still needs
   fixed, reviewed sentences.
2. **A chain is data, not a code path.** The seam is a trait, named
   `ChainReader`, living beside `Dossier` in `realorrug-onchain` (the crate
   design 0020 §6 already names as the model-side boundary that owns no chain
   client). One implementation per chain — `realorrug_onchain::SolanaReader`
   for the existing Solana reads, a new `realorrug_robinhood::RobinhoodReader`
   for the reads design 0020 §1's table names — each producing the same
   `Dossier` shape `FactSheet::build` already consumes today. The owner's own
   test is "adding a chain tomorrow is easy"; concretely, for chain number
   three, someone has to write:
   - an `Address` parse function for that chain's address shape (the existing
     `realorrug_types::Address::from_str` and `realorrug_robinhood::Address::from_str`
     are the two precedents);
   - one `ChainReader` implementation supplying the reads point 5 lists;
   - one `Venue` enum arm (design 0020 §2) and the one line in `mention.rs`'s
     shape scan that recognises the new address format;
   and would **not** have to touch: `sheet.rs`'s `FactSheet`/`Fact`/`Signal`
   types (point 3), `verdict.rs`'s level function (point 4), `voice.rs`'s
   generation call, `forbidden.rs`, `fidelity.rs`, or any existing chain's
   reader. That is the whole of "easy": one new file implementing one trait,
   one new match arm at the single dispatch point design 0020 §2 already
   names, nothing shared touched.
3. **One signal set, not one per chain.** A `Signal` means the same thing
   wherever it is read; how it is read is the chain reader's problem, not the
   signal's. Design 0020 §6's file-by-file table hedged on this — it listed
   Robinhood's `CreatorBoughtOwnLaunch` as "Robinhood-shaped, distinct from
   the existing Solana equivalent if the read differs" — and this ADR
   corrects that, rather than restates it: the read differing is exactly why
   the reader is per-chain and the signal is not. `CreatorBoughtOwnLaunch`
   is one `Signal` variant in `sheet.rs`; a Solana reader fills it from a
   token-account transfer in the launch slot, a Robinhood reader fills it
   from a `CurveBuy` log in the launch block, and `verdict.rs`'s level
   function reads one variant either way. A chain-forked `Signal` enum (one
   `Signal::CreatorBoughtOwnLaunchRobinhood` beside
   `Signal::CreatorBoughtOwnLaunch`) is exactly what this point forbids: it
   would mean `verdict.rs`'s ladder has to know which chain it is on to
   read its own inputs, which point 4 rules out directly.
4. **The verdict ladder is chain-blind.** The function that picks a level
   (`verdict.rs`'s level function, ADR 0027 point 1's "the verdict *level*
   is chosen by code from the evidence") takes `&[Signal]` and the sheet's
   `unknown` list, and nothing else — no `Venue`, no chain tag, no mint or
   token address. Per `AGENTS.md` §4, "enforce at the cheapest level that
   holds it": the cheapest level here is the function's own signature. A
   level function typed `fn level(signals: &[Signal], unknown: &[Unknown]) ->
   Level` cannot read which chain it is on because the type it accepts does
   not carry one — this is stronger than a check or a test, because there is
   no chain-shaped value in scope to branch on by mistake. `FactSheet` itself
   may carry `mint`/`read_at` for logging and display, but the level
   function is given only the signal slice and the unknown slice, never the
   sheet.
5. **What a chain reader owes.** Every `ChainReader` implementation must be
   able to produce, for a given address: the facts design 0020 §1 marks
   **required** for that chain's ladder (Solana's existing required set, or
   Robinhood's five: launch record, phase, launch-block/age, reserves,
   holders), each signal in §3's set that its chain can support, and — this
   is the load-bearing half — an explicit `unknown` entry, not a default or a
   zero, for any fact or signal it could not read. `AGENTS.md` §3 rule 8:
   absent is not zero. A reader that cannot read holder concentration on a
   given call returns "holder concentration: unknown," and `sheet.rs`'s
   existing `unknown` list is where that lands; it does not return `0`, an
   empty `Vec`, or silently omit the fact. This is what lets `verdict.rs`'s
   ladder reach `CantTell` correctly (ADR 0027 point 3) regardless of which
   chain's reader produced the gap — the ladder does not need to know why a
   fact is missing, only that it is.
6. **What this costs.** This is more work now than two forks would have
   been: Solana's clause-selection path, live and tested today, has to move
   onto free-text generation and lose the guarantee that a published
   sentence was pre-written by a person, which was clause selection's whole
   safety argument. The forbidden/fidelity checks already operate on
   arbitrary text (design 0020 §4), so the mechanism to catch a bad free-text
   reply exists — but a shared voice trained on one chain's facts reading
   badly on another's is a real risk, not a hypothetical one: the two chains'
   fact sheets differ in shape (slots vs. blocks, token accounts vs.
   addresses, a bundle-band measured on Solana's own history vs. one not yet
   measured for Robinhood per design 0020 §8), and a prompt or a fallback
   template tuned against one chain's numbers can produce a technically-true
   but tonally-wrong sentence on the other's. What catches that: design
   0020's step-5 replay of 50 real mentions, which today only exercises
   Robinhood, must be run against both chains before this ships — 50 Solana
   mentions through the new free-text path, 50 Robinhood mentions through
   it, reviewed side by side against what clause selection or the template
   would have said for the same sheet, so a chain-specific tonal miss is
   caught by a person reading replies before it is caught by a user
   complaining about one.

## Consequences

- **Solana loses clause selection as its live generation path.** The bot
  that shipped disciplined, hand-authored sentences on Solana now writes
  free text there too, gated by the same checks Robinhood was always going
  to need. This is a real behaviour change to an existing, working path, not
  only a plan for a new one.
- **`clause.rs`'s sentence library is not wasted**, it is repurposed as the
  fallback template's source material — the template still needs fixed,
  reviewed sentences for the no-provider and no-signal cases point 1
  describes, and `clause.rs`'s existing `Fact`/`Voice` types already hold
  exactly that.
- **A third chain is now a bounded, named amount of work** (point 2's list),
  which is the test the owner's sentence sets and the one this ADR is
  written to pass.
- **The replay step (point 6) gains a second chain to cover** before this
  ships, which is new work design 0020 did not scope, because it assumed
  Robinhood's voice pass was the only one being newly built.
