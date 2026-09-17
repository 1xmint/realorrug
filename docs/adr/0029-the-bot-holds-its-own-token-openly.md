<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0029 — the bot holds its own token, openly

**Date:** 2026-09-17
**Status:** accepted. **Josh's decision, recorded**, in conversation
2026-09-17: "As dev I will put in a dev buy, small one. The bot will hold the
token and collect the tax for giveaways, one day it may trade on its own but
not anytime soon."
**Supersedes:** [ADR 0013](0013-a-community-token-exists-and-radar-holds-none-of-it.md)
constraints 1 ("no dev buy, no allocation") and 2 ("the operator holds zero
tokens"). Constraints 3 to 6 stand unchanged: the fee becomes the prize, entry
is free, the bot never states the price, and the token is judged like any other.
**Rewrites:** `AGENTS.md` §3 rule 6.

## Context

ADR 0013 answered two objections to a bot that judges memecoins launching its
own. The allocation objection: a dev buy is a recipient in the launch block,
the very shape the bot points at. The touting objection: an operator who holds
the token gains when the bot's reach moves its price. ADR 0013 answered both by
holding nothing.

Josh has chosen differently. A small dev buy is normal on Pons v2, and a bot
that holds its own token and pays giveaways from the tax is a stronger story
for the community than a bot with no stake. The objections do not go away, so
this ADR answers them with disclosure instead of absence.

## Decision

1. **One small dev buy, in the launch block, stated in public.** Its size, the
   wallet that made it and the transaction are on the tokenomics page from
   launch day. `realorrug launch-check` passes a buy whose tokens go to the
   deployer or the creator fee recipient, prints it, and still refuses any
   other trade, transfer or snipe-tax exemption in the launch transaction.
2. **The bot's wallet may hold the token.** Its address is published. Every
   token it holds is visible on chain to anyone.
3. **The creator tax goes to the bot's wallet and funds the weekly prize**, as
   ADR 0013 constraint 3 and [ADR 0015](0015-the-prize-is-an-evidence-relay-and-a-winner-is-always-selected.md)
   describe. The prize payout still pays only what the contest's pure rule
   permits, through the key [ADR 0025](0025-the-robinhood-payout-signs-through-turnkey.md)
   limits.
4. **No trading.** Nothing sells, buys or swaps the token automatically. Any
   future trading needs its own ADR first, and model judgement never moves
   money (`AGENTS.md` §3 rule 1) either way.
5. **The bot judges its own token exactly like any other.** Its fact sheet
   will show the dev buy in the launch block, and the reply says so. That is
   the point: the bot does not get a kinder reading of its own launch.

## What this costs, stated plainly

- **The launch block has a recipient besides the curve.** Anyone checking the
  token with the bot's own tool will see a dev buy. The answer is that it is
  small, public and said out loud, not that it is absent.
- **The touting objection is now real, not reduced.** The bot's wallet gains if
  the price rises. What limits it: the bot never states the price or market cap
  (ADR 0013 constraint 5, enforced in code), the holding is public, and nothing
  trades.
- **Site copy promising "no dev buy" and "holds zero" becomes false** and is
  rewritten in the same stretch of work, before launch.

## Not decided here

- Whether the dev buy sits in the bot's wallet or Josh's own. The tokenomics
  page states whichever it is.
- The dev buy's size.
