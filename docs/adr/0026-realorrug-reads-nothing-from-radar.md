<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0026 — realorrug reads nothing from Radar, and Radar nothing from it

**Date:** 2026-09-15
**Status:** accepted, not yet built. **Josh's decision, recorded**: "id rather
have both standalone entirely", and "yes go standalone from radar".
Amended 2026-09-15 (below): Robinhood answers come first.
**Amends:** [ADR 0024](0024-the-bot-stands-alone.md), whose "What stays the
same" kept a data contract with Radar of published files. That contract ends.
**Built by:** [plan 0001](../plans/0001-after-the-split.md) step 7.

## Context

ADR 0024 split the code: this repository builds with no dependency on Radar's.
The running bot still leaned on Radar in three ways, and Radar on it in one:

- **Radar's creator index.** The analyst reads `creator-index.json` and
  `population.json`, written every six hours by Radar's
  `theradar:deploy/radar-creator-index.timer`, from Radar's working directory. They index
  Solana creators; realorrug's token and its contest are on Robinhood Chain
  ([ADR 0023](0023-realorrug-lives-on-robinhood-chain-and-the-bot-moves-with-it.md)).
- **Radar's seven-days-later join.** `theradar:deploy/radar-seven-days.timer` joins this bot's
  reply log with Radar's store and writes the file the daily post reads.
- **Radar's folders.** The analyst, the server and the payout run in
  `/home/guardian/radar`, write `data/analyst` and `data/contest` there, and read
  `/etc/radar/analyst.env`.
- **Radar reads realorrug's records.** `radar brief`, `radar seven-days-later`
  and `radar-backfill` read the reply log and the contest ledger as files
  (Radar's theradar#253).

A file format is a lighter tie than a crate, but it is still a tie: Radar's
index can stop, move or change shape on Radar's schedule, and the bot's replies
change with it.

## Decision

1. **realorrug indexes its own creators, from Pons v2 launches on Robinhood
   Chain**, with the decoder it already has (`realorrug_robinhood::pons`). It
   stops reading Radar's Solana index.
2. **realorrug runs its own seven-days-later join**, of its replies against
   outcomes it reads from the chain itself.
3. **Its folders are its own:** `/home/guardian/realorrug` for the working
   directory and data, `/etc/realorrug` for configuration and the payout's key.
   The payout's key and environment file start there, because the payout is not
   yet installed; the analyst's and the server's move in step 7.
4. **Radar stops reading realorrug's records.** That is a change in Radar's
   repository, and it lands before the files move, so `radar brief` does not
   alarm on files that left on purpose.
5. **Environment variables keep their `RADAR_` prefix.** A prefix is a name,
   not a dependency, and renaming every variable would break the installed
   `analyst.env` for no behaviour gained.
   Superseded: every `RADAR_*` variable is being renamed to `REALORRUG_*`
   (same suffix) to match the project's own name, with the old name read as a
   fallback so an unrenamed env file keeps working
   (`crates/realorrug-types/src/env.rs`).

The committed base-rate snapshot (`docs/research/data/0024-base-rates.json`)
stays, read only from this repository. It describes Solana launches, so whether
its numbers belong in a reply about a Robinhood token is part of step 7.

## Consequences

- **The analyst says less about creators until its own index exists.** This
  first read "the loss is on paper". That was wrong: the analyst answers only
  about Solana coins (`crates/realorrug-analyst/src/answer.rs` and `daemon.rs`
  build the sheet with `realorrug_onchain`, and the crate has no
  `realorrug-robinhood` dependency), so dropping Radar's Solana index removed
  real creator lines from live replies. A Pons v2 index also has no reader
  until the analyst can answer about a Robinhood Chain token. See the
  amendment below.
- **One more timer on the box**, realorrug's own index, reading Robinhood Chain
  over public RPC.
- **The contest ledger moves once, with the analyst and server stopped**, from
  `/home/guardian/radar/data/contest` to `/home/guardian/realorrug/data/contest`.
  It holds every week's record so far, and the payout must never see two
  ledgers, so the move happens before the payout timer is enabled.
- **Radar's `radar brief` loses its view of the bot.** Watching the bot becomes
  realorrug's job.

## Amendment, 2026-09-15: Robinhood answers come first

**Josh's decision, recorded** ("Add Robinhood", 2026-09-15), taken after the
correction above and [research 0038](../research/0038-pons-v2-creators-and-outcomes-read-over-a-range.md)'s
launch counts.

1. **The bot learns to answer about a Robinhood Chain (Pons v2) token first**,
   before the creator index: a fact sheet for an `0x` token, chosen by the
   address's shape.
2. **Solana replies keep working meanwhile, without creator lines.**
3. **No Solana creator index is ever built** in this repository. Decision 1
   stands: the only creator index is realorrug's own, from Pons v2 launches,
   keyed on the creator fee recipient (research 0038 §2).
4. Then the Robinhood creator index, then the seven-days-later join
   (decision 2), both fed by the data source research chooses.

Built by [plan 0001](../plans/0001-after-the-split.md) step 7b, whose first
part is the Robinhood answer.

**Note, 2026-09-17: "never built" is not "never present".** Decision 3 stops a
Solana index from being *built* here. It does not remove the one already on the
production box: `docs/research/data/population.json`, a Radar-built pump.fun
index (watermark slot 447,301,081, 778,593 launches), whose summary
`realorrug-serve` was still serving on 2026-09-17. So the index type carries a
`chain` field that defaults to Solana when absent, and the fact sheet ignores
an index whose chain is not the token's. Reading decision 3 as "only one index
can ever exist, so the file needs no chain tag" would take that live pump.fun
file and relabel its launches as Pons v2's under the first Robinhood verdict
the bot wrote. Design 0020 §1's amendment of the same date carries the
mechanics.
