<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0024 — The bot stands alone

**Date:** 2026-09-13
**Status:** accepted. **Josh's decision, recorded**: "keep going until the bot is
completely standalone in the new repo", and "create the repo public".
**Amends:** [design 0019](../design/0019-realorrug-on-robinhood-chain.md) §5.3,
which recommended consuming Radar's shared crates at a tagged git dependency.
This repository copies them instead.
**Numbering:** continues Radar's ADR sequence, so the documents carried over
from Radar (0013, 0015, 0023) keep their numbers and every citation of them
still means one thing. They are copies; the originals live at
[1xmint/theradar](https://github.com/1xmint/theradar), and links inside them
that point at Radar documents not copied here resolve there.

## Context

[ADR 0023](0023-realorrug-lives-on-robinhood-chain-and-the-bot-moves-with-it.md)
moved the bot into its own repository before the token launches. Design 0019
§5.2 counted what that cuts: Radar's website, research and backfill all imported
the bot's crates, and the bot imported eight crates Radar's trading side also
uses.

## Decision

**This repository builds, tests and runs the bot with no dependency on Radar's
repository.** Copied from `1xmint/theradar` at commit `517f372`:

| here | from Radar | why it is here |
|---|---|---|
| `realorrug-analyst`, `-roast`, `-contest`, `-payout` | same names, `radar-` prefix | the bot |
| `realorrug-onchain`, `-journal` | same | the dossier reader and the operations journal the bot writes |
| `realorrug-types`, `-decode`, `-pumpfun`, `-model`, `-agent`, `-provider` | same | shared with Radar's trading side; **forked** |
| `realorrug-cli` | six commands of `radar-cli`: `dossier`, `roast`, `analyst`, `contest`, `audit`, `model-prices` | the operator's tools |
| `realorrug-serve` | `radar-serve`'s `public.rs` and the five `/v1/public/*` routes | the public site's documents |
| `site/` | `site/` | the public site |
| `deploy/` | the analyst and payout units and env examples | the runbook |

**Git history is not rewritten into this repository.** Moving it would need
`git filter-repo`, a download this session did not make. The history of every
copied file is in Radar at the commit above; this repository's first commit
names it.

### What was deliberately not copied

- `realorrug-pumpfun`'s test that Radar's signer reads what the crate builds,
  and with it the only dependency on `radar-risk` and `radar-signer`. The bot
  never signs a trade; that cross-check belongs to the signer it checks.
- `radar-serve`'s operator route `/v1/analyst/replies`. It serves full fact
  sheets, which are operator material, and this server has no operator
  authentication. The log is on the box.
- `repo-conformance` and the mutation-testing jobs. Both are Radar's standards,
  and both are worth having here. Neither ran on the first commit.

### What stays the same

- **Environment variables keep their `RADAR_` prefix**, so an existing
  `/etc/radar/analyst.env` works unchanged. New ones, like `REALORRUG_BIND`, do not.
- **The data contract with Radar is two published files**: the creator index
  with its population summary, and the base-rate snapshot (`deploy/README.md`).

## Consequences

- **The six shared crates are now two copies that will drift.** That is the
  price of "completely standalone" over a git dependency. A fix to a decoder in
  one repository has to be carried to the other by hand. Accepted, because the
  bot is about to change these crates for Robinhood Chain anyway, and a shared
  crate that must serve an EVM bot and a Solana trader would be worse.
- **Radar still contains its copy of the bot** until a separate change removes
  it and replaces its imports with the file contract. Until then there are two
  copies of the bot, and only this one is to be changed.
- **A reviewer reading a crate's comments will find references to Radar
  documents** (design 0007, research 0024 and others). They are citations to
  the other repository, not missing files.
