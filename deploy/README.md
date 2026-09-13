<!-- SPDX-License-Identifier: Apache-2.0 -->
# Deploying realorrug

Three processes, each its own unit. **None of them is installed on the box as of
2026-09-13**; this is the runbook for when they are.

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
