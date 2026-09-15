<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0026 — realorrug reads nothing from Radar, and Radar nothing from it

**Date:** 2026-09-15
**Status:** accepted, not yet built. **Josh's decision, recorded**: "id rather
have both standalone entirely", and "yes go standalone from radar".
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

The committed base-rate snapshot (`docs/research/data/0024-base-rates.json`)
stays, read only from this repository. It describes Solana launches, so whether
its numbers belong in a reply about a Robinhood token is part of step 7.

## Consequences

- **The analyst says less about creators until its own index exists.** Radar's
  index describes a different chain, so for Robinhood tokens it already says
  nothing useful; the loss is on paper.
- **One more timer on the box**, realorrug's own index, reading Robinhood Chain
  over public RPC.
- **The contest ledger moves once, with the analyst and server stopped**, from
  `/home/guardian/radar/data/contest` to `/home/guardian/realorrug/data/contest`.
  It holds every week's record so far, and the payout must never see two
  ledgers, so the move happens before the payout timer is enabled.
- **Radar's `radar brief` loses its view of the bot.** Watching the bot becomes
  realorrug's job.
