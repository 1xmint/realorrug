<!-- SPDX-License-Identifier: Apache-2.0 -->
# Launch day

The steps for launching the token on pump.fun, in order. Each says who does it
and how to tell it worked. The rules behind them are
[ADR 0037](../docs/adr/0037-the-token-launches-on-pump-fun-and-its-fees-pay-for-operations.md)
(pump.fun, SOL pair, fees to a disclosed treasury, no spending key here),
[ADR 0038](../docs/adr/0038-no-prizes-buybacks-or-holder-benefits.md) (nothing
for holders, no prizes), [ADR 0039](../docs/adr/0039-the-launch-gates-and-the-monthly-ceiling.md)
(the gates) and [ADR 0029](../docs/adr/0029-the-bot-holds-its-own-token-openly.md)
(the dev buy and the bot's holding, stated in public).

Nothing here is automatic. Signing, spending and launching are Josh's.

## Gates, before anything below

1. **Josh has accepted the bot's Solana replies** from the replay set, and
   the three checks pass on every accepted case (plan 0002 phase 2).
2. **Counsel has read the review packet**
   ([design 0030](../docs/design/0030-launch-review-packet.md)) and material
   objections are resolved.
3. **X has approved automated replies in writing.** Without it, launch the
   site and keep the bot private; the X launch waits.

## Before launch

4. **Choose the treasury wallet** (plain wallet or multisig) and the dev
   buy's size and wallet. Neither key lives on the server.
5. **Re-read pump.fun on the day:** the current launch terms, the
   [fee schedule](https://pump.fun/docs/fees) (the creator rate depends on
   stage and market cap), and how the creator fee recipient is set and
   changed.
6. **Build and run the pump.fun launch check** (built:
   `realorrug launch-check solana --signature <sig> --treasury <addr>
   --dev-wallet <addr> --dev-buy-lamports <n> --rpc URL`). Against the launch
   transaction it confirms: the fee recipient is the treasury, mint and
   freeze authorities are both revoked, the dev buy matches what will be
   published, and nothing else was bundled in. `<n>` is the **total** the dev
   wallet paid -- the SOL that reached the curve plus pump.fun's protocol fee
   plus the creator fee, read from the buy's own pump.fun `TradeEvent`, not
   just the amount the curve received. The endpoint is required
   (`--rpc`, or `REALORRUG_RPC` in the environment): with neither it refuses
   rather than falling back to the rate-limited public one, and it never
   prints the endpoint, since a Helius URL carries its key.

## Launch (Josh)

7. **Launch on pump.fun** paired with SOL, creator fees to the treasury, the
   dev buy in the same transaction. Read the transaction before signing.
8. **Copy the launch transaction signature and the mint address.**

## Right after launch

9. **Run the launch check** from step 6 against the signature:
   `realorrug launch-check solana --signature <sig> --treasury <treasury>
   --dev-wallet <dev-wallet> --dev-buy-lamports <n> --rpc URL`. Do not announce a
   launch it refuses.
10. **Tell the analyst which token is its own:** set `REALORRUG_SELF_MINT` in
    `/etc/realorrug/analyst.env` and restart it. Its start-up log names the
    token.
11. **Publish the wallets and numbers** on the site's token page: mint,
    launch signature, treasury, dev-buy wallet and amount, the bot's holding
    wallet. Put the mint in the bio lead (`REALORRUG_BIO_LEAD`).
12. **Ask the bot about its own token.** It must be judged like any other,
    dev buy included.

## After launch

Fees collect to the treasury. Josh pays operating costs from it by hand and
records each payment. No prize, buyback or holder payment exists to switch on.

Before recording a payment, reconcile what the treasury has actually
received and what is still sitting unclaimed in the fee vaults:

```
realorrug treasury solana --treasury <treasury address> [--mint <mint>] --rpc URL [--seconds N]
```

Read-only: it holds no key and sends nothing. It prints one line per receipt
(a transaction where the pump.fun or PumpSwap creator-fee vault's balance
fell), then totals per vault, then each vault's current unclaimed balance.
An unreadable transaction or a budget-truncated walk is listed rather than
dropped, and the totals print "at least" with a non-zero exit in that case
(AGENTS §3 rule 8: absent is not zero).
