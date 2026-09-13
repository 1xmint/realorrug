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
| step 1, own-name mask: `realorrug` masked beside `cabalhunter.org` | `the_account_can_say_its_own_name_and_every_other_rug_is_still_refused` in `forbidden.rs`, which failed at its first assertion with the name removed from `OWN_NAMES` |

## Next, in order

1. ~~**Own-name mask in the banned-words check.**~~ Done; see the table above.
   The handle is `@realorrug` (renamed 2026-09-13), which the mask covers. The
   site is still `cabalhunter.org`, which stays in `OWN_NAMES` until the new
   domain is bought; a domain not spelled `realorrug` must be added there.
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
- 2026-09-13, second session:
  - **Step 1 done**, PR #2, CI green. The new test fails at its first
    assertion with `realorrug` removed from `OWN_NAMES` (run locally).
  - **Step 4 built**, PR #3. `repo-conformance` (19 tests) found 17 links, 3
    ADR numbers and 3 deploy-guide paths pointing at Radar files that were
    never copied; all fixed. It adds a check that no model-side crate reaches
    `realorrug-payout`. **`realorrug-cli` is the named exception**: the
    operator's binary holds both `roast` and the payout fallback. The first
    mutation run caught one survivor in the new crate, fixed and re-pushed;
    read PR #3's `mutants` result before merging.
  - **Handle renamed to `@realorrug`**, PR #4 (comments and fixtures; no code
    holds the handle). **Josh to change `VITE_X_HANDLE` in Cloudflare Pages.**
    The "Automated by" label was confirmed as `@thecabalhunter` and has not
    been re-read since the rename.
  - **Step 2 not started**: it deploys to production and waits for Josh.
    Steps 3, 5 and 6 not started.
  - *Next:* merge #2, #3, #4 once green. Then step 2 on Josh's yes; if it is
    not given, step 5 (the Pons v2 launch capture) does not depend on it.
