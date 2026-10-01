<!-- SPDX-License-Identifier: Apache-2.0 -->
# realorrug

A public forensic investigator for tokens, summoned on X and supported by a
website. The direction is claim-specific research and a free Library of
living token dossiers that people can extend with evidence. Verified
observations, relationships, patterns, outcomes and corrections should make
subsequent investigations better. **Real or rug? It shows the facts. You decide.**

Start with [VISION.md](VISION.md), the product source of truth, then
[ROADMAP.md](ROADMAP.md), the current delivery order and acceptance criteria.
[AGENTS.md](AGENTS.md) governs engineering and evidence;
[ADR 0043](docs/adr/0043-the-public-library-and-compounding-intelligence.md)
records the product direction; [ADR 0044](docs/adr/0044-three-chain-investigations-and-shared-cases.md)
and [plan 0003](docs/plans/0003-three-chain-investigator-and-library.md) track
three-chain investigation delivery. Earlier plans remain history.

The investigator and community token will launch together with the free
Library. The selected token route is pump.fun on Solana, paired with SOL.
The paid investigation portal follows launch. Treasury spending remains
operator-managed for disclosed operations and a bounded reserve; 25%
buy-and-burn plus 25% permanent liquidity is a proposal, not an adopted split.
Holding or paying never buys a favorable verdict. Model judgment has no
spending authority, and no prize, yield or airdrop is adopted.

**The token is not launched.** Existing readers, reply checks, memory, research
storage, launch/treasury checks and paid facts code are foundations. The new
bounded investigator, shared cases and Library are implemented behind opt-in
configuration. Initial summaries remain partial `CantTell` assessments; live
protocol qualification and reply-quality acceptance remain open. Launch waits
for representative quality acceptance, Library readiness and the current
review/X/configuration/signing gates in ROADMAP. Forecasts and reputation are
preserved outside the critical launch path.

Initial founder funding is $100–$200, capped at $200 total, using free options
first, with no recurring commitment. Existing runtime spending limits remain
unchanged until a reviewed implementation changes them. This documentation
work does not deploy, launch, sign or post, and no paid model call is used for
the initial capture/review evidence.

## Investigator and Library

The `/library` website browses chain-qualified cases without sign-in. Existing
X authentication and CSRF protect public contributions. Submitted allegations
remain unverified; observations retain their read point and corrections mark
dependent findings for review. Base and Ethereum addresses require a network.
The legacy Robinhood checker remains available with an explicit network choice.

The worker shares the existing spending ledger and request limits. Enable only
after reviewing [coverage and capture evidence](docs/research/0066-the-investigator-and-library-qualification.md)
and [the environment example](deploy/analyst.env.example). Missing storage,
budget, prices or a live worker refuses new website work. Interrupted jobs and
uncertain publication attempts never retry silently. Production delivery has
an additional opt-in gate. No paid portal or financial automation is added.

Operator commands: `investigation-capture` freezes bounded live observations
without a model call; `case-review` produces offline owner-review reports;
`case-store` verifies/backs up history or appends a correction. Run the binary
without arguments for their syntax. Captures are reader output, not complete
raw RPC recordings or proof of full protocol coverage.

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
| `crates/realorrug-store` | the research store (design 0032): hash-chained forecast rows and a separate identity table |
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
