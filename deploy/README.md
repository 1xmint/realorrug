<!-- SPDX-License-Identifier: Apache-2.0 -->
# Deploying realorrug

Two processes, each its own unit. **Installed on the box since 2026-09-14:
`realorrug-serve`, as a user unit, and `realorrug-analyst`** (below). The old
payout is retired (below).

## What runs today

`realorrug-serve` runs as guardian's systemd **user** unit,
[`deploy/user/realorrug-serve.service`](user/realorrug-serve.service), on
`127.0.0.1:8090`, beside `radar-serve` on `8402`. A user unit because the
system unit needs sudo with a password and the box's passwordless sudo belongs
to other services; guardian lingers, so the unit starts at boot and is
supervised. Binary at `~/realorrug/bin/realorrug-serve`, with the release's
`BUILD-INFO.txt` beside it.

Installed from `release-linux` run 34793325535, commit `ae448f0`; `/health`
reported that build, and the five `/v1/public/*` documents were byte-identical
to `radar-serve`'s on install. **Both binaries were replaced on 2026-09-17 with
commit `414105c`**, checksums verified against the release's `BUILD-INFO.txt`
and the previous pair kept beside them as `*.prev-d15cf18`; `/health` reports
`414105c` and the analyst's journal reports `LIVE -- replies are being posted
publicly` on it. **Serve, the analyst and the `realorrug` command were replaced
on 2026-09-21 with commit `4ddeec9`** (`release-linux` run 35549813285),
checksums verified before and after the copy, the previous three kept as
`*.prev-a071bc7`; `/health` reports `4ddeec9` and the analyst's journal reports
`LIVE` on it. That build writes launch names, market readings and mention words
to `data/analyst/memory.sqlite3`, which the analyst created on its first poll.
Run `realorrug narratives` with `--memory ~/realorrug/data/analyst/memory.sqlite3`
(or from `~/realorrug`): run from anywhere else it finds no memory file and
says so.

**The live site reaches it** since 2026-09-14. The site calls
`https://radar.heyvera.org`; the root-owned tunnel config
`/etc/cloudflared/config.yml` holds a rule, above that hostname's catch-all,
sending `/v1/public/*` to `http://localhost:8090`. Everything else on the
hostname still goes to `radar-serve`. On the switch, the five documents fetched
through the hostname matched `8090`'s own answers apart from their
`measured_at` time, and the tunnel held a connection to `8090` and none to
`8402`.

```bash
# Which rule a URL matches (on the box)
sudo cloudflared tunnel --config /etc/cloudflared/config.yml ingress rule https://radar.heyvera.org/v1/public/stats
# Roll back to radar-serve: the config before the rule was kept beside it
sudo cp /etc/cloudflared/config.yml.bak-realorrug /etc/cloudflared/config.yml && sudo systemctl restart cloudflared
```

```bash
# Check
ssh guardian-vps-tail 'systemctl --user is-active realorrug-serve; curl -s localhost:8090/health'
# Upgrade: download the artifact, then
scp realorrug-serve guardian-vps-tail:/tmp/ && ssh guardian-vps-tail \
  'install -m 0755 /tmp/realorrug-serve ~/realorrug/bin/ && systemctl --user restart realorrug-serve'
# Remove
ssh guardian-vps-tail 'systemctl --user disable --now realorrug-serve'
```

**The analyst runs** since 2026-09-14 15:32 UTC, as the system unit
[`realorrug-analyst.service`](realorrug-analyst.service), binary
`~/bin/realorrug-analyst` from release run 34852198731 (`1706340`). It
replaced Radar's `/etc/systemd/system/radar-analyst.service`, which is
stopped and disabled; the old stopped at 15:32:13 and the new started at
15:32:14. Both started `LIVE` with two operator ids, and the new one moved the
mention cursor two minutes later with nothing in its journal but its startup
lines. The binary has no `--help`: any invocation starts the daemon. Radar's
analyst no longer exists to roll back to: Radar removed it, and its unit names
Radar's folders, which realorrug leaves in plan 0001 step 7a (below).

| unit | binary | what it does | writes |
|---|---|---|---|
| `realorrug-analyst.service` | `realorrug-analyst` | answers summoned mentions on X with measured facts | `data/analyst`, `data/contest` |
| `realorrug-serve.service` | `realorrug-serve` | the public site's five documents | nothing |

## It reads nothing from Radar

[ADR 0026](../docs/adr/0026-realorrug-reads-nothing-from-radar.md). Every unit
runs in `/home/guardian/realorrug`, writes under its `data/`, and reads its
settings from `/etc/realorrug`. Radar reads nothing of realorrug's either.

Three files are read at paths relative to that working directory:

- `docs/research/data/0024-base-rates.json`, a dated snapshot, with its `chain`
  field saying which chain it was measured on (Radar's Solana/pump.fun
  measurement, today). A consumer refuses a wrong-chain snapshot the same way
  it refuses a wrong-chain creator index. It is kept and quoted, dated "as of"
  its `measured_on`, until `STALE_AFTER_DAYS` (sixty, `realorrug-roast`'s
  `baserates.rs`) -- a backstop against a rebuild job nobody noticed had
  stopped, not a routine fourteen-day expiry. A copy is committed here too.
- `docs/research/data/population.json`, the population summary behind
  `/v1/public/stats`. Written by the `creator-index` job, beside the index and
  from the same walk, so the two can never disagree. It held the last one Radar
  built until 2026-09-17, when building the Robinhood index overwrote it; it now
  states Pons v2's totals, with its `chain` field saying so. As of the run
  described below, `creator-index` measures its outcome columns from logs in
  the same pass that counts launches -- `TokenLaunched`, `Graduated` and
  `CurveBuy`, all over the same block range -- so they are no longer absent by
  default. They are still written absent, not zero, if any of those three
  walks fails to finish the range, or if `--verify` disagrees with a sampled
  `getLaunchedToken` call: a half-finished outcome pass must not overwrite a
  good file with small numbers. `/v1/public/stats` publishes JSON `null` for
  whichever columns are absent, and the page answers "not measured yet" for
  those; without the file at all it shows its own dated figures.
- `docs/research/data/creator-index.json`, who launched what. **Built on the box
  on 2026-09-17** from Robinhood Chain launches (plan 0001 step 7b): 531,581
  launches by 301,820 launchers, blocks 0 to 65,763,847, 134 requests, 33.5 MB
  on disk and about 50 MB of the analyst's memory. That run predates the
  outcome pass; a re-run now also fills `measured`/`organic`/`instant`/
  `stillborn` from the `Graduated` and `CurveBuy` walks above (the
  deployer-to-fee-recipient alias is still separate future work), so a reply
  can say this launcher has launched before, how often, and -- once re-run --
  how those went. `stillborn` here means "no `CurveBuy` in any block after the
  launch block": Robinhood's own definition, chosen because the Solana
  measurement this index mirrors used a wall-clock window and both trade
  directions, neither of which a Robinhood log answers cheaply; see
  `realorrug-cli/src/creator_index.rs`'s module doc and
  `realorrug-roast/src/creator.rs`'s field docs for the full reasoning.
  Without the file entirely, replies say nothing about who launched a token
  and the analyst says so once at startup; a successful load is silent, so the
  absence of that line is the confirmation. A copy of Radar's would be worse
  than nothing: it is a different chain, and a creator it had not seen would
  read as a first launch.

The daily "seven days later" post reads a day's file in `data/analyst/daily/`.
Radar's join wrote those; nothing does now, so the post is silent until
realorrug's own join writes a day's file from Robinhood Chain.

### Record every launch's name, hourly

`deploy/realorrug-record-launches.service` and its `.timer` run `realorrug
record-launches` once an hour against the analyst's RPC and memory, so the
narrative counts cover every Pons launch rather than only the ones someone
asked about (research 0055). New persistent config: installed only with the
owner's yes. It needs a `realorrug` CLI built from `0ce8ebc` or later.

```bash
scp deploy/realorrug-record-launches.service deploy/realorrug-record-launches.timer guardian-vps-tail:/tmp/
ssh guardian-vps-tail 'sudo install -m 0644 /tmp/realorrug-record-launches.* /etc/systemd/system/ \
  && sudo systemctl daemon-reload && sudo systemctl start realorrug-record-launches \
  && journalctl -u realorrug-record-launches -n 20 --no-pager'
# The one run above reads well? Then:
ssh guardian-vps-tail 'sudo systemctl enable --now realorrug-record-launches.timer'
```

To stop it: `sudo systemctl disable --now realorrug-record-launches.timer`.
The rows it wrote stay; nothing else reads its cursor.

### Rebuild the Robinhood creator index and base rates

Install the CI-built `realorrug` CLI at
`/home/guardian/realorrug/bin/realorrug` first. Run this on the box; systemd
loads the existing RPC setting without printing it. The command uses the first
configured endpoint so the reported RPC calls also count HTTP attempts (there
is no endpoint failover in this invocation):

```bash
sudo systemd-run --wait --pipe --collect --uid=guardian \
  -p WorkingDirectory=/home/guardian/realorrug \
  -p EnvironmentFile=/etc/realorrug/analyst.env \
  /bin/bash -c 'set +x; rpc=${REALORRUG_ROBINHOOD_RPC:-${RADAR_ROBINHOOD_RPC:-}}; : "${rpc:?Robinhood RPC is required}"; exec /home/guardian/realorrug/bin/realorrug creator-index --rpc "${rpc%%,*}" --from 0 --verify 20 --out docs/research/data/creator-index.json --base-rates-out docs/research/data/0051-robinhood-base-rates.json'
```

`--base-rates-out` defaults to `docs/research/data/0051-robinhood-base-rates.json`
when omitted. This is an output path, not a checked-in measurement: the file
does not exist until a successful run. Leave `--to` off for `--verify`; the
head and its timestamp are pinned before walking. `--from`/`--to` can restrict
a research run, and those bounds are recorded in the snapshot, but the command
above covers the whole launch population. Every walk and verification must
finish before files are written; the base-rate file is replaced by rename.

The snapshot reuses the launch and graduation walks. Its histogram counts
distinct nonzero recipients of positive token `Transfer`s in each launch's
block, including the curve's mint and factory machinery. These are neither
owners nor buyers. The four overlapping bands match the Solana snapshot:
one to three, exactly six, five to seven, and ten to thirteen. Histogram
counts include every launch, including counts outside those bands. Graduation
means observed by the watermark; instant means within three blocks. Empty
bands, and bands with undefined instant enrichment when no launch graduated
instantly, are omitted from `launch_block.bands` rather than assigned invented
conditional rates. No round-trip costs or aftermath figures are fabricated.

`outcomes_24h` carries integer counts per band: all launches, measured launches,
incomplete windows, unpriced complete windows, and measured launches reaching
at least 2x, 5x and 10x. Its denominator is **complete, priced windows**.
The expanded trade walk reads `CurveBuy` and `CurveSell` together, in block/log
order, measuring peak `quote / tokens` relative to the first fill, from launch
through exactly 86,400 seconds later. This is each launch's quote-asset
execution-price multiple as logged, with no separate fee adjustment: not USD,
not net profit, and **not post-graduation DEX performance**. Zero-sized fills
make a launch unpriced rather than silently choosing another baseline. The
field is optional for old snapshots and is not yet used by the fact sheet or
reply rules. The analyst's existing default snapshot path remains unchanged;
this command produces the Robinhood measurement, not a loader configuration
change.

RPC estimate from the code (2026-09-17, not a measured full rebuild): let
`Qlaunch`, `Qgraduation`, `Qtrade`, and `Qtransfer` be each adaptive log walk's
requests, including result-cap retries. The walker targets 8,000 logs per
response, so each `Q` is roughly its matching log count divided by 8,000,
plus startup/range-resizing requests; this is an estimate, not a quota bound.
The trade and transfer queries match events across **all addresses**, even
though only known launches contribute. Block timestamps no longer cost one
RPC call per block: `BlockTimeModel` reads the head (already counted in the
leading `1`) plus up to `TIME_SAMPLES` (32, a fixed constant, not a function
of population size) evenly-spaced exact timestamps across `[from, to]`, then
estimates every other block's time by linear interpolation between the
nearest two samples. Robinhood Chain's block time is measured at ~0.1019
s/block and does not drift enough within one rebuild's range for the
interpolation error to move a launch across the 24-hour outcome boundary
(research 0038); a walk this cheap trades that small, stated error for
avoiding the per-block-read design below. The exact successful command total
is:

```text
1 + TIME_SAMPLES + Qlaunch + Qgraduation + Qtrade + Qtransfer + V
V = min(20, graduated launches) + min(20, non-graduated launches)
TIME_SAMPLES = 32
```

That bounds the whole rebuild's *timestamp* cost at 32 calls regardless of
launch count: a design that read one block's timestamp per launch would have
needed up to 531,581 calls at the 2026-09-15 population (research 0038,
0039), which is why that per-block design was rejected before this file
first described a rebuild command. `Qtrade` replaces the old buy-only walk
and includes the extra sell traffic; `Qtransfer` is the new recipient-count
pass. These counts, verification and the head/sample reads are all printed
separately and totaled. The actual full-history log-walk counts (`Qlaunch`,
`Qgraduation`, `Qtrade`, `Qtransfer`) await the box run; multiple endpoints
passed manually can add internal failover HTTP attempts beyond the logical
RPC counts printed by the command.

## Moving off Radar's folders

Plan 0001 step 7a, done once. Before it, the analyst ran in
`/home/guardian/radar` and read `/etc/radar/analyst.env`. First install the
Radar build that stops reading realorrug's files and disable its
`theradar:deploy/radar-seven-days.timer`, so Radar does not report the files as missing. Then,
on the box, with a build of this repository's units in `~/realorrug/deploy`:

```bash
# 1. What the site lists now, to compare after
# (its measured_at is the time of asking, so it is left out of the comparison)
curl -s localhost:8090/v1/public/weeks | sed 's/"measured_at":"[^"]*"//g' > ~/weeks-before.json
# 2. Stop both (about five minutes of no replies)
sudo systemctl stop realorrug-analyst && systemctl --user stop realorrug-serve
# 3. Copy, keeping the originals until the proof passes
cp -a ~/radar/data/analyst/. ~/realorrug/data/analyst/
cp -a ~/radar/data/contest/. ~/realorrug/data/contest/
mkdir -p ~/realorrug/docs/research/data
cp -a ~/radar/docs/research/data/0024-base-rates.json ~/radar/docs/research/data/population.json ~/realorrug/docs/research/data/
sudo install -m 0640 -o root -g guardian /etc/radar/analyst.env /etc/realorrug/analyst.env
# (the second substitution is for the file's header comments, which name /etc/radar)
sudo sed -i -e 's#/home/guardian/radar/#/home/guardian/realorrug/#g' -e 's#/etc/radar/#/etc/realorrug/#g' /etc/realorrug/analyst.env
# 4. The units, keeping the old ones beside them
cp /etc/systemd/system/realorrug-analyst.service ~/.config/systemd/user/realorrug-serve.service ~/realorrug/
sudo install -m 0644 ~/realorrug/deploy/realorrug-analyst.service /etc/systemd/system/
install -m 0644 ~/realorrug/deploy/user/realorrug-serve.service ~/.config/systemd/user/
sudo systemctl daemon-reload && systemctl --user daemon-reload
# 5. Start, and check
sudo systemctl start realorrug-analyst && systemctl --user start realorrug-serve
curl -s localhost:8090/v1/public/weeks | sed 's/"measured_at":"[^"]*"//g' | cmp - ~/weeks-before.json && echo same weeks
sudo grep -n 'radar/\|/etc/radar' /etc/systemd/system/realorrug-*.service ~/.config/systemd/user/realorrug-serve.service /etc/realorrug/analyst.env; [ $? -eq 1 ] && echo no Radar paths
```

The move is done when the weeks match, no Radar path is left, and the
analyst's cursor file changes after the start (`ls -l
~/realorrug/data/analyst/cursor`). Then the copies under `~/radar/data` can go.
To go back before that: stop both, install the two kept units
(`~/realorrug/realorrug-*.service`) where they came from, reload, start.

## Install

```bash
# Built by CI on a push to main; download the artifact, then:
sudo install -m 0755 realorrug-analyst realorrug-serve /usr/local/bin/
sudo install -m 0644 deploy/realorrug-analyst.service deploy/realorrug-serve.service /etc/systemd/system/
sudo install -m 0640 -o root -g guardian deploy/analyst.env.example /etc/realorrug/analyst.env
sudo systemctl daemon-reload
sudo systemctl enable --now realorrug-serve realorrug-analyst
```

### The payout is retired

The weekly prize is off
([ADR 0038](../docs/adr/0038-no-prizes-buybacks-or-holder-benefits.md)) and
the token's fees are spent by the operator, by hand
([ADR 0037](../docs/adr/0037-the-token-launches-on-pump-fun-and-its-fees-pay-for-operations.md)).
`realorrug-payout` is no longer built or installed, and if run it refuses
before reading any key. Its units, env example and key script are gone from
this directory. Past weeks' records stay under `data/contest` and
`realorrug contest` still reads them.

The environment variables are being renamed from their `RADAR_` prefix to
`REALORRUG_` (same suffix); each name falls back to its old `RADAR_` form
while the fallback lives, so an existing `analyst.env` keeps working
unchanged once it is under `/etc/realorrug` with its directory paths moved.
Rename the box's env files to the `REALORRUG_` names when convenient -- see
`crates/realorrug-types/src/env.rs`.
