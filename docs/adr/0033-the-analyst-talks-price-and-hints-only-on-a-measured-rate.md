<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0033 — the analyst talks price, and hints only on a measured rate

**Date:** 2026-09-17
**Status:** accepted. **Josh's decision, recorded**, in conversation
2026-09-17: "talking about token including price is important for people to
understand the tokenomics", and "if the bot reasons that the 10x chance is
high it says so ... hinting instead of saying it will ... explaining its
reasoning".
**Supersedes:** [ADR 0013](0013-a-community-token-exists-and-radar-holds-none-of-it.md)
constraint 5 ("the analyst never states the token's price or market
capitalisation"), and the restatement of it in
[ADR 0029](0029-the-bot-holds-its-own-token-openly.md). Constraint 6 of ADR
0013, the token is judged like any other, stands and is now literal.
**Rewrites:** `AGENTS.md` §3 rule 5.
**Consequence lands in:** `crates/realorrug-roast/src/sheet.rs`
(`withhold_price` stops dropping price facts), `forbidden.rs`, `voice.rs`,
and `crates/realorrug-serve/src/{card,check}.rs`.

## Context

The analyst answers "is this coin real or a rug". A reader cannot judge a
token's shape without its price, market cap and liquidity: a dev buy of 0.05
ETH means one thing at a 3 ETH market cap and another at 300. Dropping those
numbers made the answer less useful, not safer.

The bot is also meant to be worth quoting. Josh wants it to say so when the
evidence points somewhere, the way a careful trader would: "I wouldn't be
surprised if this 10x'd tonight", with its reasons. Accounts that post
unfounded calls are common. One whose calls rest on a measured rate, and that
publishes its misses, is not.

## Decision

1. **Price talk is allowed for every token.** Replies, the card and the check
   page may state price, market cap, liquidity and the measured round-trip
   cost.
2. **Every price or market cap carries the block or time it was read at.** A
   price without its moment is a stale price that looks current.
3. **Hints are allowed only when a measured rate backs them.** An upside or
   downside hint is allowed only when the fact sheet carries an outcome rate
   for launches shaped like this one, for example "of 400 launches like this,
   30 reached 10x within 24 hours". Such a hint is:
   - hedged ("wouldn't be surprised", "could", "if the trend holds"), never
     "will";
   - never an instruction to buy, sell or hold;
   - always given with its reasoning.
   Without that fact, the check after generation refuses a prediction.
4. **Every hint is logged, and a hit-and-miss scorecard is published.** The
   bot's reputation should be something a reader can check, not a claim.
5. **The project's own token is treated exactly like any other.** Price,
   hints, verdict: same rules, same code path, no special handling.
6. **Disclosures live in the X bio and on the website, never in replies.**
   The bio always ends "Not financial advice." (`bio.rs` `DISCLAIMER`). The
   site states the dev buy, with its size, wallet and transaction.

## What this costs, stated plainly

- **The touting objection, if the bot ever holds.** Today the bot's wallet
  holds none of the token: the prize pool is the vault contract, and nothing
  trades (Josh, 2026-09-17). ADR 0029 still permits a small disclosed holding.
  If that happens, the bot will be stating the price of, and hinting about, a
  token it holds. ADR 0029 limited that with "the bot never states the
  price", and that limit is gone. What would remain: the holding is public,
  nothing trades automatically, and a hint needs a measured rate.
- **A hint is only as good as its rate.** Until the outcome rate is measured
  (a later pass over the creator index, which will read each launch's peak
  multiple within 24 hours), no hint can pass the check. The code for the
  check lands first and is inert until then.
- **ADR 0013's legal precondition is unchanged and more pressing.** An
  automated account that comments on the price of a token its operator holds
  (the dev buy)
  is exactly the case that note asked to have read before the first post.

## Order of work

1. This ADR and the `AGENTS.md` rule.
2. Stop dropping price facts; stamp each with its block or time; let price
   words through the forbidden check; refuse bare predictions; add the
   optional outcome-rate fact (empty) and the hedged-hint path that needs it;
   tell the model the same rules in `voice.rs`.
3. Measure the outcome rate in the creator-index rebuild.
4. The hint log and the published scorecard.
