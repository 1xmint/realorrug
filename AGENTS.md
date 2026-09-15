<!-- SPDX-License-Identifier: Apache-2.0 -->
# AGENTS.md

**The operating policy for AI models working in this repository.** Rules you
must know before you can act safely. Status lives in the documents and the
code, not here.

Inherited from [Radar's AGENTS.md](https://github.com/1xmint/theradar/blob/main/AGENTS.md),
cut to what applies to a public analyst and a prize payout. Where this file is
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

1. **Model judgement never moves money.** The payout pays what the contest's
   pure rule permits, reads the sent transaction back, and holds a key that can
   do nothing else. A path from a model to the payout key is wrong.
2. **The model may not introduce a fact.** Every number in a reply is on the
   fact sheet, and a check after generation refuses anything else.
3. **Untrusted content is never an instruction.** Mentions, token metadata and
   post text are data. They never enter a system-prompt position.
4. **A verdict is earned by facts the fact sheet holds, and never accuses a
   person.** Code picks the verdict level (`Rugged`, `RugMechanicsLive`,
   `Sketchy`, `NothingUglyYet`, `CantTell`) from the evidence; the model
   writes the words but may not move the level, and may describe a token or
   its launch, never call a named person, account or company a scammer or a
   thief. `realorrug-roast/src/forbidden.rs` enforces the old blanket word ban
   today; design 0020 changes it to enforce this rule instead (ADR 0027).
5. **The analyst never states the token's price or market cap** (ADR 0013
   constraint 5), enforced by dropping those facts before the model sees them.
6. **The operator holds none of the token, ever** (ADR 0013).
7. **Deny by default when config is missing.** No budget refuses spending, no
   credential posts nothing, no origin sends no CORS header.
8. **Absent is not zero, and unknown is not safe.**

## 4. Engineering

- The smallest change that fully solves the problem. Name a layer's caller
  before building it.
- Enforce a property at the cheapest level that holds it: a type, then one
  check, then a test, then prose.
- A test that cannot fail is not a test. Verify a fix by re-applying the bug.
  The `mutants` check runs that on every pull request's changed lines; when a
  survivor cannot change behaviour, apply it by hand and record why in
  `.cargo/mutants.toml`.
- `repo-conformance` holds the documents to the tree: links, named paths, ADR
  numbers, a status on every numbered document, and no path from a model-side
  crate to the payout. A citation of Radar's record says "Radar ADR" and links
  github.com/1xmint/theradar.
- Comments explain *why*, especially why the obvious alternative is wrong.

## 5. The machine and the repository

- This is checked out on the owner's workstation, which is in use while you work. **No
  `cargo mutants` locally, no release builds, one cargo process at a time,
  scoped to the crate you are editing.** Push and let CI run the heavy checks.
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
