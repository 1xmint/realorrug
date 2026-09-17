<!-- SPDX-License-Identifier: Apache-2.0 -->
# Launch day

The steps for launching the token, in order. Each one says who does it and how
to tell it worked. The rules behind them are
[ADR 0029](../docs/adr/0029-the-bot-holds-its-own-token-openly.md) (the dev buy
and the bot's holding, stated in public) and
[ADR 0025](../docs/adr/0025-the-robinhood-payout-signs-through-turnkey.md) (the
payout signs through Turnkey).

Nothing here is automatic. Signing, spending and launching are Josh's; the
checks after each step can be run by anyone with the box.

## Before launch

1. **Decide the dev buy.** Its size in ETH, and which wallet receives the
   tokens: the deployer (Josh's launching wallet) or the creator fee recipient
   (the bot's Turnkey wallet). `realorrug launch-check` accepts either and
   refuses any other recipient.
2. **Have the Turnkey wallet's address ready.** It becomes the creator fee
   recipient, and the payout refuses to run if the factory names any other
   (`REALORRUG_PAYOUT_ADDRESS` in `deploy/payout.env.example`).
3. **Rehearse the payout:** a tiny real payment from the Turnkey wallet to
   Josh's own wallet, read back from the chain, so the key is known to sign
   before any fees depend on it (ADR 0025).

## Launch (Josh)

4. **Launch on Pons v2** with the creator fee recipient set to the Turnkey
   wallet, and the dev buy in the same transaction. No other wallet exempted
   from the snipe tax, no other buy bundled in.
5. **Copy the launch transaction hash.**

## Right after launch

6. **Check the launch on the box:**

   ```bash
   ~/realorrug/bin/realorrug launch-check --tx <hash> --rpc <endpoint>
   ```

   It must print `CLEAN launch`, one `dev buy` line with the ETH, tokens and
   recipient, and `fees to` the Turnkey wallet. `NOT CLEAN` lists every reason;
   do not announce a launch it refuses.
7. **Tell the analyst which token is its own:** set `REALORRUG_SELF_MINT` in
   `/etc/realorrug/analyst.env` to the token address, then restart the analyst.
   Its start-up log names the token; before this it says "no token is the
   analyst's own". This is what stops it ever stating its own token's price.
8. **Tell the payout which token pays the prize:** set `REALORRUG_TOKEN` in
   `/etc/realorrug/payout.env`. Unset, the payout does nothing.
9. **Publish the numbers.** On the tokenomics page (`site/src/Token.tsx`,
   the `Status` section), replace the two "not yet" notes with the token
   address, the launch transaction, and the `dev buy` line from step 6, as
   printed. Put the token address in the bio lead (`REALORRUG_BIO_LEAD`) too:
   the page promises the address appears there and nowhere else.
10. **Ask the bot about its own token** on X. The reply must mention the dev
    buy, like it would for anyone else's launch.

## The first week

The contest needs no switch. The week closes on the first tick after Monday
00:00 UTC, scores that week's replies, and posts the winner. The creator tax
collects in the Pons fee escrow; the payout claims it and pays the claimed
winner.
