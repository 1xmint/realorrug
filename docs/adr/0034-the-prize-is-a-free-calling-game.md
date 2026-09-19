<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0034 — the prize is a free calling game, and mentions earn nothing

**Date:** 2026-09-18
**Status:** recommending. Josh chose the direction in conversation on
2026-09-18; each numbered decision below is my recommendation until he rules
on it, and the "whose" column changes when he does.
**Reasoning:** [design 0028](../design/0028-the-daily-five.md).
**Facts:** [research 0051](../research/0051-the-daily-five-what-to-check-first.md).
**Reverses:** the ruling of the morning of 2026-09-18, "contest money on
engagement only" (the run ledger; design 0027 slice 11).
**Keeps:** [ADR 0023](0023-realorrug-lives-on-robinhood-chain-and-the-bot-moves-with-it.md)
decision 3 (the token is paired with ETH) and decision 5 (the legal review
runs in parallel).

## Context

That morning the prize was to be paid on engagement alone: the summons with
the most quotes, reposts, replies, likes and views won the week. By the
afternoon two facts had come up against it.

- X banned paying people for posting on 2026-01-15 (research 0051 §3). A
  prize for the most-engaged summons is exactly that.
- "@grok is this true" is the summon habit that spread, and it pays nobody
  (research 0051 §6). What spreads a summon bot is an answer worth showing.

Josh chose to move the prize off X and onto a free game on the site, where the
thing rewarded is calling launches right, not posting.

## Decision

| # | decision | whose | what it costs |
|---|---|---|---|
| 1 | realorrug **stays paired with ETH**. A pair with a tokenised stock would pay fees and prizes in tokens US persons may not receive. ADR 0023 decision 3 stands | recommending | Nothing new; the stock meta is followed by the checks S1–S5, not by the token's pair |
| 2 | **Holders get nothing from the project.** No holder reward, no holder tier, no airdrop | recommending | A weaker story for buyers; in exchange the token cannot be read as a share of the project |
| 3 | **The prize is the daily five** (design 0028): a free game of real-or-rug calls on the five largest new launches, odds set by the bot, settled by the chain, paid to the best record above the luck line. It ships with no prize; the prize switches on only after the lawyer answers the question in design 0028 §7 | recommending | Sign-in and a call log on the server (new ground); a week or more before anyone can clear the luck line; the pot may roll over for weeks |
| 4 | **Mentions stay free and earn nothing.** A summons gets a verdict, never points | recommending | No direct reason to summon beyond the answer; the bot spreads on quality alone, as grok does |
| 5 | **Suggestions are logged, never obeyed.** "@realorrug dig into X" goes to an append-only log a human reads; the bot never acts on it (AGENTS rule 3.3) | recommending | A missed good tip now and then; the log keeps it for a human |
| 6 | **Follow the meta by rule.** A reader for a new venue is added when that venue holds over 10% of launchpad revenue two weeks running | recommending | A new venue is missed for up to two weeks; in exchange nobody chases every launchpad |

## What is dropped

- The **reach multiplier**: points scaled by a summons' views.
- **Hill claims**: holding a coin's top spot until someone beat you.
- **Follow and unfollow** by the bot as a reward.
- The engagement weights proposed that morning (quotes ×10, reposts ×5,
  replies ×3, likes ×1, views ÷100) are not needed and are not asked for.

`crates/realorrug-contest/src/score.rs` is kept. It stops deciding the prize;
its age rule is reused to keep fresh accounts off the ranked board.

## Consequences

- The payout (`crates/realorrug-payout`) points at the daily-five ranking only
  when the prize switch is on, and the switch stays off until the lawyer
  answers. Nothing that pays out is built before then.
- The server gains its first write: a signed-in player's call. With no
  sign-in config it takes no calls (AGENTS rule 3.7). Deploying it to the live
  server is Josh's gate.
- Every post about the game (a settlement, the weekly result) is Josh's gate
  until he approves its rule once.
- A corrected fact from the earlier plan: Pons took about two-thirds of
  launchpad fees, not 28% (63.9% across launchpads on one day, The Defiant,
  2026-09-01; research 0051 §5). That is a one-day snapshot. Decision 6 needs
  its own two-week measure, which the project does not track yet.
- Signing a player in costs about $0.01 (one read of the player's own
  account on X's current price list; research 0051 §1).

## Not decided

- The window (fourteen days or three) waits on the replay.
- The odds table waits on the replay, which waits on design 0027 slice 8
  (PR #117, the other blocker, merged 2026-09-18).
- Which countries the rules exclude waits on the lawyer.
- Whether we may keep a player's past calls after they delete their X account
  or remove our app: X's developer policy as read does not say (research 0051
  §1). It goes to the lawyer beside the prize question, before the call log
  keeps anyone's history.
