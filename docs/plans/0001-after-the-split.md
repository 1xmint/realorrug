<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0001 — After the split

**Date:** 2026-09-13
**Status:** open. The split itself is done ([ADR 0024](../adr/0024-the-bot-stands-alone.md));
this is what follows it, in order, each with what proves it.

## Done

| item | proof |
|---|---|
| repository created, public, `1xmint/realorrug` | first commit `c13a6ec` |
| builds, tests, lint, fmt, licence headers, site, cargo-deny, MSRV in CI | the `ci` run on `c13a6ec`; see the handback below for its result |
| release binaries on push to `main` | `release-linux` workflow |

## Next, in order

1. **Own-name mask in the banned-words check.** `crates/realorrug-roast/src/forbidden.rs`
   matches by substring, so "rug" refuses every reply containing "realorrug".
   `OWN_DOMAIN` is still `cabalhunter.org`. Add the bot's own name and handle to
   the mask, with a test that fails when the mask is removed.
   *Proof:* the test, re-applied.
2. **Deploy `realorrug-serve` beside Radar's server** (`deploy/README.md`), and
   point the site's `VITE_API_BASE` at it. *Proof:* `/health`'s `build` equals
   the release artifact's `BUILD-INFO.txt` commit, and the site's five pages load.
   **Touches production: ask first.**
3. **Remove the bot from Radar.** Only after step 2, because Radar's server
   answers the live site's `/v1/public/*` today. Delete `radar-analyst`,
   `radar-roast`, `radar-contest`, `radar-payout` and the public routes; move
   `radar_roast::creator` and `BaseRates` down into a Radar crate so
   `radar-research` keeps them; make `radar-backfill`, `radar brief` and
   `radar seven-days-later` read the reply log and contest ledger as files.
   *Proof:* Radar's CI, and `radar brief` on the box.
4. **Carry Radar's two missing checks over:** `repo-conformance` (links, paths,
   a status on every numbered document) and the mutation shards on pull
   requests. *Proof:* both jobs green on a pull request.
5. **Capture a real Pons v2 launch** on Robinhood Chain: fee rate, fee currency,
   curve-only launch block ([research 0035](../research/0035-robinhood-chain-read-from-its-own-pages.md) §6).
   Launch-blocking. *Proof:* the transaction, committed as a fixture.
6. **The Robinhood Chain payout and own-token reader** (design 0019 §4.4), which
   replace `realorrug-payout`'s pump.fun path and give constraints 1 and 6 an
   instrument. *Proof:* tests over captured transactions.

## Handback

*Written at the end of each session. Start here, not in a transcript.*

- 2026-09-13: repository created and pushed. First `ci` run on `c13a6ec`:
  build, tests (968 passed), fmt, licence headers, site and MSRV green; lint
  red on one missing semicolon in `realorrug-analyst/src/daemon.rs`, which also
  stopped clippy before it reached `realorrug-cli` and `realorrug-serve`. Fixed
  in the pull request that adds this file, with the test floor raised to 968.
