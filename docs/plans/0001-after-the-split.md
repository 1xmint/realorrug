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
| step 4, `repo-conformance` and mutation shards | both green on PR #3, whose first mutation run caught a survivor in the new crate |
| step 2, `realorrug-serve` answers the live site | `/health` build `ae448f0` equals release run 34793325535's; the site's pages load through the tunnel rule ([deploy guide](../../deploy/README.md)) |
| step 5, a real Pons v2 launch captured | `docs/research/data/0036-pons-v2-launch.json`, findings in [research 0036](../research/0036-pons-v2-read-from-a-real-launch.md) |

## Next, in order

1. ~~**Own-name mask in the banned-words check.**~~ Done; see the table above.
   The handle is `@realorrug` (renamed 2026-09-13), which the mask covers. The
   site is still `cabalhunter.org`, which stays in `OWN_NAMES` until the new
   domain is bought; a domain not spelled `realorrug` must be added there.
2. ~~**Deploy `realorrug-serve` beside Radar's server**~~ Done; see the table
   above. The site's `VITE_API_BASE` stayed; a tunnel rule sends its
   `/v1/public/*` calls to the new server instead (`deploy/README.md`). *Proof:* `/health`'s `build` equals
   the release artifact's `BUILD-INFO.txt` commit, and the site's five pages load.
   **Touches production: ask first.**
3. **Remove the bot from Radar.** Merged as theradar#253 on 2026-09-14; the
   box's `radar brief` proof waits for Radar's next deploy. Only after step 2, because Radar's server
   answers the live site's `/v1/public/*` today. Delete `radar-analyst`,
   `radar-roast`, `radar-contest`, `radar-payout` and the public routes; move
   `radar_roast::creator` and `BaseRates` down into a Radar crate so
   `radar-research` keeps them; make `radar-backfill`, `radar brief` and
   `radar seven-days-later` read the reply log and contest ledger as files.
   *Proof:* Radar's CI, and `radar brief` on the box.
4. ~~**Carry Radar's two missing checks over:**~~ Done; see the table above. `repo-conformance` (links, paths,
   a status on every numbered document) and the mutation shards on pull
   requests. *Proof:* both jobs green on a pull request.
5. ~~**Capture a real Pons v2 launch**~~ Done; see the table above. On Robinhood Chain: fee rate, fee currency,
   curve-only launch block ([research 0035](../research/0035-robinhood-chain-read-from-its-own-pages.md) §6).
   Launch-blocking. *Proof:* the transaction, committed as a fixture.
6. **The Robinhood Chain payout and own-token reader** (design 0019 §4.4), which
   replace `realorrug-payout`'s pump.fun path and give constraints 1 and 6 an
   instrument. *Proof:* tests over captured transactions. In three parts:
   - 6a, the reader and the launch check (`realorrug-robinhood`,
     `realorrug launch-check`). Built 2026-09-14.
   - 6b, the fee escrow decoded: `Credited`/`Claimed`, the creator's claimable
     balance, and a `claim()` captured and read back ([research 0036](../research/0036-pons-v2-read-from-a-real-launch.md) §5).
     Built 2026-09-14 (`realorrug_robinhood::escrow`).
   - 6c, the payout: claim from the escrow, pay the winner, read both back,
     under `realorrug_contest::Payout::permitted`. Signs on EVM, so it brings
     the signing dependency and the key file format; its own pull request.

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
- 2026-09-13, third session:
  - **#2, #3 and #4 merged**, each green; #4 re-ran on the new `main` first,
    so the conformance and mutation checks passed over it too.
  - **Step 5 done.** One launch, its first taxed buy and one fee sweep,
    captured as raw RPC responses pinned to blocks; research 0036 recomputes
    every number from that file. On the curve the creator gets 70 bps of
    volume plus a creator tax chosen once at launch (0 to 1,000 bps), in the
    pair asset. The mint went only to the curve, and a launch can still carry
    a buy in the same transaction, so constraint 1 is proved by the launch
    receipt holding no `CurveBuy` and no extra `SnipeTaxExempted`. Pons's published source does not match what is
    deployed; the chain is the reference.
  - **Open for Josh before launch:** the creator tax. Recommendation: zero. The
    prize is then 70 bps of curve volume in ETH, over twice pump.fun's 30, and
    after the first three seconds a trader pays only the 1% base fee.
  - **Step 2 still waits for Josh** (production). Step 3 waits on step 2.
  - *Next:* step 6. Read the capture file in Rust tests first: the launch
    receipt's recipients, the sweep split. The fee after graduation and the
    escrow's payout path are unread, and the payout reader needs both.
- 2026-09-14, fourth session:
  - **Step 5 merged**, PR #5.
  - **Step 6a built**: `realorrug-robinhood` reads receipts and decodes Pons
    v2 launches, trades and sweeps; `pons::check_launch` is constraint 1;
    `realorrug launch-check` runs it against the chain. Tested on the dirty
    launch, a clean one captured this session, and each kind of dirt re-applied;
    the RPC client and the command are tested against a loopback server. Against
    mainnet it printed CLEAN for `0x2a43738c…` and NOT CLEAN, five reasons,
    for `0x1013a302…`.
  - **Found, not yet coded:** sweeps credit a shared fee escrow and the creator
    claims from it; the hook keeps charging after graduation, and in source the
    creator tax continues there (research 0036 §5).
  - **Creator tax:** Josh asked whether a higher tax fuels the flywheel. Two
    samples of 250 launches (research 0036 §6): 101–300 bps earned the most
    and above 300 bps showed no gain but two outliers. Recommendation moved
    from zero to **200 bps**, all of it to the prize. Josh decides at launch.
  - **Step 2 approved by Josh** on 2026-09-14: deploy once the step 6 pull
    request is merged and `main` is green.
  - **6a merged** (PR #6, `ae448f0`), `main` green.
  - **Step 2, first half done:** `realorrug-serve` from release run 34793325535
    runs on the box as a user unit on `127.0.0.1:8090`; `/health` says
    `ae448f0`, and the five `/v1/public/*` documents are byte-identical to
    `radar-serve`'s ([deploy guide](../../deploy/README.md)). **Second half
    waits for Josh:** the live site reaches `radar-serve` through the root-owned
    tunnel config, and the rule sending `/v1/public/*` to `8090` needs his sudo.
    Then the site's five pages are step 2's proof, and step 3 can start.
  - *Next:* Josh's tunnel rule, then step 3; 6b meanwhile.
  - **Step 2 done.** Josh added the tunnel rule; `cloudflared ingress rule`
    matched `/v1/public/stats` to `localhost:8090`. Through
    `radar.heyvera.org` all five documents answer 200 with the site's CORS
    header and match `8090`'s own answers apart from `measured_at`; the tunnel
    held one connection to `8090` and none to `8402`. On `cabalhunter.org`,
    home, leaderboard, pool, history and token rendered, their calls to
    `weeks`, `stats`, `leaderboard` and `pool` each 200. Rollback is in the
    [deploy guide](../../deploy/README.md). Step 3 is unblocked.
- 2026-09-14, fifth session:
  - **Step 6b done**, PR #9, every check green including mutants.
    `realorrug_robinhood::escrow` reads credits, claims and the claimable
    balance, and reads a claim back. Captured claim `0x07cab768…`: the
    escrow's record fell from 4,014,961,601,594,189,201 wei to zero, its ETH
    by the same, and the claimer's ETH rose by that less the gas, to the wei.
    The deployed call is `claim(uint256)`, not the published `claim()`.
  - **Step 3 done in Radar**, theradar#253 (squash `c8fca0e`), every check
    green. Four bot crates, the public routes and the bot's commands and
    units deleted; `creator` and `BaseRates` moved into `radar-research`;
    `radar brief`, `seven-days-later` and `radar-backfill` read the reply log
    and ledger as files. Radar's test floor 2089 → 1979. The first mutation
    run caught six survivors in the new file readers, each now pinned. A
    helper agent did most of it and stopped without committing; the work was
    recovered from its worktree. Radar #249 merged too (squash `26e1bd0`), and
    the `radar-realorrug` worktree is removed. Radar allows only squash merges.
  - **Not yet deployed:** Radar's new build is not on the box, so `radar brief`
    on the box, step 3's second proof, still runs the old binary.
  - **Radar's contest alarm was false.** `radar brief` said `data/contest`
    could not be written, but week 2958 closed at 00:03 UTC and the analyst's
    journal holds no read-only error since 2026-09-11. The installed
    `/etc/systemd/system/radar-brief.service` lacked the contest grant Radar's repo copy has.
  - **The analyst swap is staged, waiting on Josh's sudo:** `realorrug-analyst`
    from release run 34852198731 (`1706340`, sha256 `928f6835…`) at
    `~/bin/realorrug-analyst`, and both unit files in `~/realorrug/deploy/`.
    Running it with `--help` to check it started it as a daemon (it has no
    such flag); it had no credential, read and posted nothing, wrote nothing,
    and was stopped within three minutes. `radar brief`'s process list does
    not name `realorrug-analyst`, so a stale binary there is not yet caught.
  - **Analyst swapped**, Josh's sudo, 15:32 UTC: `radar-analyst` stopped and
    disabled, `realorrug-analyst` enabled and `LIVE` as the old one was, two
    operator ids each, no overlap; it moved the mention cursor two minutes
    later with no error. `radar brief` after the swap: contest `[ok]` (week
    2958, 3 records); its one failure is Radar's `radar-backfill` running an
    old build, which predates this session.
  - *Next:* step 6c, the payout. Radar's own deploy (its new build, and
    `radar brief` on the box as step 3's second proof) is Radar's to run.
