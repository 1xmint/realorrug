<!-- SPDX-License-Identifier: Apache-2.0 -->
# Plan 0001 — After the split

**Date:** 2026-09-13
**Status:** open; its Robinhood launch path is replaced by [plan 0002](0002-bot-quality-then-a-solana-launch.md) (2026-09-21). The split itself is done ([ADR 0024](../adr/0024-the-bot-stands-alone.md));
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
     under `realorrug_contest::Payout::permitted`, signed through Turnkey
     ([ADR 0025](../adr/0025-the-robinhood-payout-signs-through-turnkey.md)).
     Built 2026-09-14 on `payout/robinhood`; installed on the box, and the
     Turnkey setup proof and read-only gas capture passed 2026-09-15 (research
     0037). The timer stays off until step 7 and launch.
   - 6d, before launch: the analyst's `try_claim` accepting an EVM address; the
     site showing ETH with a Robinhood explorer link; Radar's `radar brief`
     reading the new payout shape; and how the token is launched with the
     Turnkey account as creator fee recipient (ADR 0025 §2).
7. **Standalone from Radar** ([ADR 0026](../adr/0026-realorrug-reads-nothing-from-radar.md)),
   in two parts, in the order Josh chose on 2026-09-15.
   - ~~7a~~ Done 2026-09-15; see the handback. Before the payout timer is enabled: Radar stops reading the reply log
     and ledger (a Radar pull request, installed on the box with its
     `radar-seven-days` timer disabled); then, with the analyst and server
     stopped, `data/analyst` and `data/contest` move to
     `/home/guardian/realorrug`, the base-rates snapshot is copied beside them,
     `analyst.env` moves to `/etc/realorrug`, and the three units follow
     ([deploy/README.md](../../deploy/README.md), "Moving off Radar's
     folders"). The payout's key and env file are already under
     `/etc/realorrug`. Until 7b, replies say nothing about who launched a token
     and the daily "seven days later" post stays silent, each because its file
     is absent. *Proof:* no unit, env file or code path on the box names
     `/home/guardian/radar` or `/etc/radar`; the analyst moves its mention
     cursor after the move; `/v1/public/weeks` lists the same weeks before and
     after.
   - 7b, in the order Josh chose on 2026-09-15 ("Add Robinhood", ADR 0026's
     amendment):
     1. **The bot answers about a Robinhood Chain token.** A fact sheet for an
        `0x` Pons v2 token, the chain picked by the address's shape; Solana
        replies keep working, without creator lines. Research first: which
        data source (the public RPC is not enough, research 0038 §4), then
        0038 §7's gaps (graduated phase values, the graduation event, a spot
        price read), then a design, then the build.
     2. realorrug indexes its own creators from Pons v2 launches, keyed on the
        creator fee recipient.
     3. realorrug runs its own seven-days-later join for Robinhood tokens.

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
- 2026-09-14, sixth session:
  - **Step 6c built** on `payout/robinhood`, one pull request.
    [ADR 0025](../adr/0025-the-robinhood-payout-signs-through-turnkey.md)
    records Josh's decision that the key lives in Turnkey. The ledger holds
    wei (`Paid::Eth`, with `Sol` read-only); `realorrug-payout` claims from the
    escrow, transfers exactly the `Claimed` figure, reads both back, and keeps
    a pending file so a rerun never claims twice; `realorrug contest pay
    --dry-run` and `record-payout --claim-tx --transfer-tx` run the same
    checks; serve adds `wei`, `claim_tx`, `transfer_tx`.
  - **Proved locally:** the encoder rebuilds the captured claim `0x07cab768…`
    to mainnet's hash and recovers its sender; the Turnkey stamp verifies
    under `k256`'s own DER decoder; 22 flow tests against a fake chain cover
    every refusal and every resume case. Contest, robinhood, payout, cli,
    serve, analyst and repo-conformance tests and clippy pass, one crate at a
    time. MIN_TESTS 1030 → 1070 by count.
  - **CI green on PR #12, mutants included**, after two fixes: the first
    mutation run found 24 survivors in the new payout code, each now pinned
    by a test or rewritten away; and cargo-deny failed on RUSTSEC-2026-0285
    in rustls 0.23.43, which `main` also carries, fixed by 0.23.45.
  - **Not proved:** nothing touched Turnkey or mainnet. The deploy guide's
    policy expression is untested until the setup proof.
  - *Next, needing Josh:* (1) a yes to the read-only capture for research
    0037 (a plain transfer and its receipt, the base fee, and two gas
    estimates); (2) the Turnkey organisation, wallet, user, API key and
    policy, then `realorrug-payout --setup-proof`; (3) at launch, the gas
    float, the first payout and enabling the timer. 6d before launch.
- 2026-09-15, seventh session:
  - **Research 0037 captured** (Josh's yes): a transfer is 21,000 gas with no
    L1 part, a claim estimates at 42,581, and `claim(0)` reverts `NoBalance()`.
    A 0.001 ETH float covers about 95 weeks. The key loader now reads the file
    Turnkey's CLI writes.
  - **Turnkey, read only:** one root user, no wallet, no policy. Josh's first
    API key was on the root user and P-256, so it cannot be the payout's.
  - **Josh chose standalone from Radar**, recorded as ADR 0026 and step 7. The
    payout's key and env file moved to `/etc/realorrug` in PR #12.
  - **`deploy/make-payout-key.sh`** makes the key on the box with OpenSSL
    (Turnkey's CLI is not installed there). Tested against a scratch folder:
    it refused a second run, and the public key it printed was recomputed from
    the file it wrote. Running it on the box is Josh's: the agent was refused
    writing a secret there.
  - **Josh ran the script and created the `realorrug-payout` service user**
    with its public key. The dashboard filed that secp256k1 key as P-256, with
    no choice of curve, so it could never stamp a request. The payout now
    stamps with P-256 (ADR 0025 amended): `p256` beside `k256`, the scheme
    `SIGNATURE_SCHEME_TK_API_P256`, and the script makes a P-256 key, removing
    the earlier secp256k1 one. The over-HTTP test fails with the old scheme
    restored; the script was rerun against a scratch folder three ways (old key
    present, good key present, fresh), with each printed public key recomputed
    on P-256 from the file.
  - **Turnkey is set up** (Josh, on the dashboard): the key script rerun on the
    box; the service user recreated with the P-256 public key (Turnkey refused
    to delete a user's only API key: "user missing valid credential"); the
    wallet; and the policy, entered as JSON, which Turnkey accepted. The policy
    matches empty call data as `''` or `'0x'`, and the setup proof gained a
    fourth request, a 1 wei transfer, so that guess is tested before launch.
  - **The first setup proof failed at signing** (2026-09-15, PR #12 merged and
    installed on the box). `whoami` answered as `realorrug-payout`, so the key,
    organisation and stamp work; the three signing requests each came back
    HTTP 404, "Could not find any resource to sign with. Addresses are case
    sensitive." The payout sent the wallet address in lowercase, and Turnkey
    matches it only in EIP-55's mixed case. It now sends that form
    (`tx::checksummed`, tested against EIP-55's own examples and the wallet's
    address; re-applying the lowercase form fails the HTTP test). The "denied"
    on the factory call that run was this error, not the policy, so it proves
    nothing; the rerun must show the claim and transfer signed.
  - **The setup proof passed** on the rerun (PR #13's binary): `whoami`
    answered; the factory call was denied by the policy engine (HTTP 403, "No
    policies evaluated to outcome: Allow"); `claim(0)` and a 1 wei transfer
    were both signed by the wallet and read back. The four lines are in
    research 0037 §4. The policy's empty-data guess holds.
  - **Step 7 split** (Josh, 2026-09-15): 7a, the Radar cut and the folder move,
    first, because it unblocks the payout timer; 7b, realorrug's own creator
    index and seven-days-later join, after. Josh approved stopping the analyst
    and server for about five minutes for the move.
  - **Step 7a done.** Radar stopped reading the bot's files in theradar#254
    (merged as `56b18a4`); its `radar` and `radar-backfill` and, by Josh's
    `radar-deploy`, `radar-serve` are that build on the box (`/health` build
    `56b18a45`). The `radar-seven-days` timer is disabled and its unit files
    removed. realorrug's units and env moved in PR #15, and PR #16 made the
    move's last check able to pass. Josh ran the move on the box; its script
    printed all three proofs: the weeks matched before and after, no unit or env
    file names `/home/guardian/radar` or `/etc/radar`, and the analyst's cursor
    changed after the start (19:33:27 UTC), logging
    `dir=/home/guardian/realorrug/data/analyst`. The copies under
    `~/radar/data` are kept for a day.
  - **Found on the box:** Radar's brief had failed since 2026-09-12 on
    "replaced but not restarted: radar-backfill". The stale process was
    `radar-market-tape`, not `radar-follow`; Josh restarted it.
  - *Next:* 7b, research first (Pons v2 launches and outcomes read over a
    range); then the payout timer and the launch items in 6d.
