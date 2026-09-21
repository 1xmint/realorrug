<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0037 — the token launches on pump.fun, and its fees pay for operations

**Date:** 2026-09-21
**Status:** accepted. **Josh's decisions, recorded**, from the plan he wrote
on 2026-09-21 ("bot quality → Solana launch → research community"). Nothing
is launched, bought or signed by this ADR.
**Reasoning:** [design 0029](../design/0029-bot-quality-then-a-solana-launch.md).
**Supersedes:** [ADR 0023](0023-realorrug-lives-on-robinhood-chain-and-the-bot-moves-with-it.md)
decisions 2, 3 and 5 (Robinhood Chain as the token's home, the ETH pair, and
a legal review that does not block launch);
[ADR 0025](0025-the-robinhood-payout-signs-through-turnkey.md) entirely (no
payout signer exists); [ADR 0034](0034-the-prize-is-a-free-calling-game.md)
decision 1 (the ETH pair); and the "collect the tax for giveaways" half of
[ADR 0029](0029-the-bot-holds-its-own-token-openly.md).
**Keeps:** ADR 0029's disclosed dev buy and the bot's disclosed holding;
ADR 0023 decision 6 (the repository) and the Robinhood readers, which stay as
a chain the bot answers about.

## Context

The Robinhood plan tied three things together: the token's home, a weekly
prize paid from its creator fees, and a signer that paid the winner. The prize
is gone ([ADR 0038](0038-no-prizes-buybacks-or-holder-benefits.md)), which
leaves the signer with nothing to do and the chain choice with no reason
beyond the prize's fee currency. The bot's strongest readers are Solana's:
pump.fun launches and graduated PumpSwap pools.

## Decision

| # | decision | whose |
|---|---|---|
| 1 | The **realorrug token launches through pump.fun on Solana, paired with SOL** | Josh |
| 2 | **Creator fees go to a disclosed project treasury** wallet. They pay disclosed operating costs and build a reserve. Nothing else | Josh |
| 3 | **The treasury is spent by the operator, by hand.** The analyst, the website and every model-side crate hold no key that can spend it. The Robinhood payout signer is **not** repurposed as a treasury signer | Josh |
| 4 | **The launch waits on Josh's acceptance of the bot's replies** (plan 0002 phase 2), not on statistical proof of forecasting skill | Josh |
| 5 | **Material legal objections are resolved before launch**, not answered with a disclaimer. The review packet is [design 0030](../design/0030-launch-review-packet.md) | my recommendation, in Josh's plan |
| 6 | **Before signing**, the current pump.fun launch terms, the fee recipient, the mint and freeze authorities and the transaction's actual behaviour are read from the chain, not from documentation (AGENTS §1, "a capture disposes") | Josh |
| 7 | **Every project-controlled wallet is published** with its role, and any developer purchase or compensation is disclosed with its address and amount | Josh |
| 8 | **Revenue is never promised.** No guaranteed fee income, future prize allocation or buyback is advertised. Any estimate uses the pump.fun fee rate that applies at the token's stage and market cap, dated and sourced | Josh |

## Consequences

- `crates/realorrug-payout` signs nothing. Its binary refuses to run, its
  systemd units leave `deploy/`, and it leaves the Linux release. The code
  stays so the history it wrote can be read and replayed.
- `realorrug launch-check` is a Robinhood Pons v2 check. A pump.fun
  equivalent (fee recipient, authorities, a readback of the launch
  transaction) is built before launch day; `deploy/LAUNCH.md` lists it.
- Fee accounting reads the treasury's receipts on Solana. The Robinhood
  escrow reader stays for history only.
- AGENTS §3 rule 1 changes from "the payout key" to "no key": model
  judgement never moves money, and nothing in this repository can.

## Not decided

- The treasury's form (a plain wallet or a multisig) is Josh's, before launch.
- The dev buy's size.
