<!-- SPDX-License-Identifier: Apache-2.0 -->
# realorrug

An X account you summon about a memecoin. It answers with what the chain
shows — who launched it, what was bought in the launch block, what the curve
holds — and a check after generation refuses any number that is not on the fact
sheet. **Real or rug? It shows the facts. You decide.**

A community token, realorrug, funds a weekly prize for the people whose
summons travelled furthest. The bot holds some of it, in public, and trades
none of it. [ADR 0013](docs/adr/0013-a-community-token-exists-and-radar-holds-none-of-it.md)
has the original constraints, [ADR 0029](docs/adr/0029-the-bot-holds-its-own-token-openly.md)
the two it replaced, and [ADR 0023](docs/adr/0023-realorrug-lives-on-robinhood-chain-and-the-bot-moves-with-it.md)
where the token lives.

**Nothing is launched.** The token does not exist yet.

## What is here

| path | what |
|---|---|
| `crates/realorrug-analyst` | the summoned-reply loop: mention parser, admission gate, reply log, X client |
| `crates/realorrug-roast` | the reply: fact sheet, model, fidelity check, banned verdict words |
| `crates/realorrug-onchain` | the dossier, read from the chain on demand |
| `crates/realorrug-contest` | the weekly prize as a rule that replays |
| `crates/realorrug-payout` | the week's payout on Robinhood Chain: claims from the escrow and pays the winner, signed through Turnkey |
| `crates/realorrug-robinhood` | Robinhood Chain, read: receipts, and Pons v2 launches, trades and fee sweeps; the launch check behind `realorrug launch-check`; the fee escrow's credits, claims and claimable balance |
| `crates/realorrug-serve` | the public site's documents |
| `crates/realorrug-cli` | `realorrug dossier`, `roast`, `analyst`, `contest`, `launch-check`, `label-outcomes`, `narratives`, `audit`, `model-prices` |
| `crates/realorrug-agent`, `crates/realorrug-model`, `crates/realorrug-provider` | the boundary a model sits behind, the model client, the spend meter |
| `crates/realorrug-types`, `crates/realorrug-decode`, `crates/realorrug-pumpfun`, `crates/realorrug-journal` | shared vocabulary, Solana decoding, pump.fun, the hash-chained journal |
| `site/` | the public site |
| `deploy/` | systemd units, the runbook, and the launch-day checklist (`deploy/LAUNCH.md`) |

It began as part of [Radar](https://github.com/1xmint/theradar), a Solana
research system, and was split out on 2026-09-13.
[ADR 0024](docs/adr/0024-the-bot-stands-alone.md) says what was copied and why.

## Build and test

```bash
just check   # build, tests, lint, fmt
just ci      # everything CI runs, including the site and cargo-deny
just site    # the public site alone
```

On Windows under Git Bash, export a toolchain whose linker works:

```bash
export REALORRUG_CARGO="cargo +stable-x86_64-pc-windows-gnullvm"
```

## Licence

Apache-2.0.
