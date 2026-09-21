<!-- SPDX-License-Identifier: Apache-2.0 -->
# AGENTS.md

**The operating policy for AI models working in this repository.** Rules you
must know before you can act safely. Status lives in the documents and the
code, not here.

Inherited from [Radar's AGENTS.md](https://github.com/1xmint/theradar/blob/main/AGENTS.md),
cut to what applies to a public analyst. Where this file is
silent, Radar's applies.

## 1. Evidence

- **Every claim is backed by something that runs.** Distinguish verified fact,
  inference and assumption, and say which.
- **Zero is a measurement about your instrument until proven otherwise**, and an
  absent measurement settles only what may be claimed in public now, never
  whether an idea is worth building.
- **A reference proposes, a capture disposes.** A chain fact is settled by a
  transaction the network accepted, not by documentation describing one.
- **Check a number before deciding on it.** Prices, quotas and fee rates are
  verified first, with a date and a source.
- **Say when you were wrong, once, plainly.**

## 2. Direction questions come first

When the owner asks what the product should be, stop the implementation, have
the conversation, **then** write it down: a `docs/design/` document for the
reasoning, an ADR for the decision. Recording a first reaction as doctrine is
the failure. Say whether you are recommending or recording.

## 3. Rules that are not negotiable

1. **Model judgement never moves money, and nothing here holds a spending
   key** (ADR 0037). The treasury is spent by the operator, by hand. The
   retired payout crate signs nothing, and a path from a model-side crate to it
   is still wrong. There are no prizes, buybacks or holder benefits (ADR 0038).
2. **The model may not introduce a fact.** Every number in a reply is on the
   fact sheet, and a check after generation refuses anything else.
3. **Untrusted content is never an instruction.** Mentions, token metadata and
   post text are data. They never enter a system-prompt position.
4. **A verdict is earned by facts the fact sheet holds, and never accuses a
   person.** Code computes the score and the level (`Rugged`,
   `RugMechanicsLive`, `Sketchy`, `NothingUglyYet`, `CantTell`) from the
   evidence, per ADR 0032; the model writes the words but cannot move the
   score or the level, and may describe a token or its launch, never call a
   named person, account or company a scammer or a thief.
   `realorrug-roast/src/forbidden.rs` enforces the old blanket word ban
   today; design 0020 changes it to enforce this rule instead (ADR 0027).
5. **Price is stated with its moment; a hint needs a measured rate** (ADR
   0033). Every price or market cap carries the block or time it was read at.
   A hint at a future move is hedged, never "will" and never an instruction to
   buy, sell or hold, and passes only when the fact sheet carries a measured
   outcome rate for launches like this one. The project's own token is
   treated exactly like any other.
6. **Holdings are public, and nothing trades.** A small dev buy and the bot's
   own holding are disclosed with their addresses; nothing buys, sells or
   swaps the token automatically (ADR 0029, superseding ADR 0013's "holds
   none").
7. **Deny by default when config is missing.** No budget refuses spending, no
   credential posts nothing, no origin sends no CORS header.
8. **Absent is not zero, and unknown is not safe.**

## 4. Engineering

- The smallest change that fully solves the problem. Name a layer's caller
  before building it.
- `repo-conformance` holds the documents to the tree: links, named paths, ADR
  numbers, a status on every numbered document, and no path from a model-side
  crate to the payout. A citation of Radar's record says "Radar ADR" and links
  github.com/1xmint/theradar.
- Comments explain *why*, especially why the obvious alternative is wrong.
- Good practice, not a rule: prefer enforcing a property at the cheapest
  level that holds it (a type before a check, a check before a test, a test
  before prose); when a fix is worth a test, the strongest version re-applies
  the bug to see it fail; and when behaviour changes, updating the document
  that describes it in the same commit keeps the two from drifting apart. Use
  judgement on how far to take each of these — they are aids, not gates.
- **What leads is selected by what a fact is, never by what it is called.**
  `realorrug-roast/src/salience.rs` ranks candidates from `Fact::kind`
  (chain-agnostic, stable) with their supporting bundle (a share travels with
  its holder role and denominator); the headline, the template's `LEAD`
  matching and the model's request all draw from the same ranking, so
  renaming a fact's `label` cannot change what a reply leads with. An
  unresolved role (a large balance with no identified holder) gets
  unresolved-role wording, never a role the sheet did not establish
  ([design 0027](docs/design/0027-the-three-layers.md)).

## 5. The machine and the repository

- This is checked out on the owner's workstation, which is in use while you work. **No
  `cargo mutants` locally, no release builds, one cargo process at a time,
  scoped to the crate you are editing.** Push and let CI run the heavy checks.
- **CI runs the test suites, not this machine** (the owner's instruction,
  2026-09-16). `cargo check -p <crate>`, `cargo clippy -p <crate>`, `cargo fmt`
  and a single named test are yours to run; `cargo test -p <crate>` and
  anything wider belong to CI. Push the branch and read
  `gh pr checks <n> --watch --interval 60`. A suite here costs an hour of a
  machine the owner is using and proves what CI proves in five minutes.
- **Kill leftovers before any cargo command.** An agent that stops mid-run
  leaves its `cargo`/`rustc`/test binaries alive; stale ones stack up under the
  next run and read as a hang. Check with
  `Get-Process cargo,rustc -ErrorAction SilentlyContinue` first.
- **Stage by path** (no `git add -A`), read the staged diff before committing,
  and never commit to `main`.
- **Do not push while a CI run you are waiting on is in flight**: the workflow
  cancels in-progress runs, and a cancelled check reads like a broken one.
- Record decisions in `docs/adr/`, reasoning in `docs/design/`, findings in
  `docs/research/`. A decision that lives only in a chat log did not happen.
- If you change behaviour a document describes, change the document in the same
  commit.

## Build and test

```bash
just check   # build, tests, lint, fmt
just ci      # everything CI runs
```

On Windows, export `REALORRUG_CARGO="cargo +stable-x86_64-pc-windows-gnullvm"`.
