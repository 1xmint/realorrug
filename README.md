<!-- SPDX-License-Identifier: Apache-2.0 -->
# realorrug

An X account you summon about a memecoin. It answers with what the chain
shows — who launched it, what was bought in the launch block, what the curve
holds — and a check after generation refuses any number that is not on the fact
sheet. **Real or rug? It shows the facts. You decide.**

A community token, realorrug, will launch through pump.fun on Solana, paired
with SOL. Its creator fees go to a disclosed treasury and pay the project's
disclosed operating costs and a reserve. Holders get nothing from the project:
no revenue share, yield, buyback, airdrop, prize or better verdicts. The bot
holds some of the token in public and trades none of it.
[ADR 0037](docs/adr/0037-the-token-launches-on-pump-fun-and-its-fees-pay-for-operations.md)
has the launch, [ADR 0038](docs/adr/0038-no-prizes-buybacks-or-holder-benefits.md)
what holders do not get, and [ADR 0039](docs/adr/0039-the-launch-gates-and-the-monthly-ceiling.md)
what must happen first. The order of work is
[plan 0002](docs/plans/0002-bot-quality-then-a-solana-launch.md).

**Nothing is launched.** The token does not exist yet. It launches after the
bot's Solana replies pass the owner's review and counsel has read
[the review packet](docs/design/0030-launch-review-packet.md).

## What is here

| path | what |
|---|---|
| `crates/realorrug-analyst` | the summoned-reply loop: mention parser, admission gate, reply log, X client |
| `crates/realorrug-roast` | the reply: fact sheet, model, fidelity check, banned verdict words |
| `crates/realorrug-onchain` | the dossier, read from the chain on demand |
| `crates/realorrug-contest` | the retired weekly contest, kept so its records replay; the daily five's scoring |
| `crates/realorrug-payout` | retired (ADR 0037): the old Robinhood prize payout, kept for history; its binary refuses to run |
| `crates/realorrug-robinhood` | Robinhood Chain, read (the bot still answers about tokens there): receipts, and Pons v2 launches, trades and fee sweeps; the launch check behind `realorrug launch-check`; the fee escrow's credits, claims and claimable balance |
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
