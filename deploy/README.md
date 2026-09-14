<!-- SPDX-License-Identifier: Apache-2.0 -->
# Deploying realorrug

Three processes, each its own unit. **Installed on the box: `realorrug-serve`
only**, since 2026-09-14, as a user unit (below). The analyst and the payout are
not installed; this is the runbook for when they are.

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
to `radar-serve`'s on install.

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

**The analyst is staged, not running.** The bot answering mentions on the box
is still Radar's `radar-analyst.service`. `realorrug-analyst` from release run
34852198731 is at `~/bin/realorrug-analyst`, with
[`realorrug-analyst.service`](realorrug-analyst.service) copied to
`~/realorrug/deploy/`; the switch installs it and stops the old unit first,
with sudo. Both units read `/etc/radar/analyst.env` and write the same
directories, so they must never run together. The binary has no `--help`:
any invocation starts the daemon.

| unit | binary | what it does | writes |
|---|---|---|---|
| `realorrug-analyst.service` | `realorrug-analyst` | answers summoned mentions on X with measured facts | `data/analyst`, `data/contest` |
| `realorrug-payout.service` + `realorrug-payout.timer` | `realorrug-payout --due` | pays a claimed, unpaid week under three refusals | `data/contest` |
| `realorrug-serve.service` | `realorrug-serve` | the public site's five documents | nothing |

## What it reads from Radar, and how

The bot does not import Radar. It reads **two files Radar publishes**, at paths
relative to its working directory:

- `docs/research/data/creator-index.json` and `population.json`, written every
  six hours by Radar's `theradar:deploy/radar-creator-index.timer`.
- `docs/research/data/0024-base-rates.json`, a dated snapshot. A copy is
  committed here, so the bot runs without Radar; the box's copy wins when the
  working directory is Radar's.

That is why the units keep `WorkingDirectory=/home/guardian/radar`. A file
format is the whole contract: if Radar stops publishing, the bot's replies say
nothing about who launched a token, and say so, rather than failing.

Radar's `radar seven-days-later` timer still joins this bot's reply log with
Radar's store and writes the file the daily post reads.

## Install

```bash
# Built by CI on a push to main; download the artifact, then:
sudo install -m 0755 realorrug-analyst realorrug-payout realorrug-serve /usr/local/bin/
sudo install -m 0644 deploy/realorrug-analyst.service deploy/realorrug-payout.service deploy/realorrug-payout.timer deploy/realorrug-serve.service /etc/systemd/system/
sudo install -m 0640 -o root -g guardian deploy/analyst.env.example /etc/radar/analyst.env
sudo systemctl daemon-reload
sudo systemctl enable --now realorrug-serve realorrug-analyst
```

The payout runs as its own user, `realorrug-payout`, which owns nothing but its
key and the contest directory. Create it before enabling the timer. The payout
code today is pump.fun's; ADR 0023 replaces it with a Robinhood Chain payout
before the token launches, so **do not enable the timer until that lands**.

The environment variables keep their `RADAR_` prefix, so an existing
`/etc/radar/analyst.env` works unchanged.
