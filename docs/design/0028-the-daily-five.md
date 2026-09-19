<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0028 — the daily five

**Status:** Proposed. Records the direction Josh chose in conversation on
2026-09-18 and the reasoning behind it. The decisions it rests on are in
[ADR 0034](../adr/0034-the-prize-is-a-free-calling-game.md), marked
recommending until Josh rules on each. The facts it depends on are checked in
[research 0051](../research/0051-the-daily-five-what-to-check-first.md).
**Adds to:** [design 0027](0027-the-three-layers.md). It shelves one slice
of it, slice 11 (the engagement-only contest score). The code for that slice,
`crates/realorrug-contest/src/score.rs`, is kept and stops deciding the prize.
**Reverses:** the ruling of the morning of 2026-09-18, "contest money on
engagement only" (ADR 0034 decision 3 says why).

## 1. The problem this solves

The project grows by attention. The old contest paid for reach: the summons
that travelled furthest on X won the week. Two facts end that plan.

- X banned paying people for posting on 2026-01-15 (research 0051 §3). A
  prize for the most-viewed summons is a payment for posting.
- The one summon bot that went viral, "@grok is this true", pays nobody
  (research 0051 §6). Summons spread because the answer is worth showing,
  not because anyone is paid.

So a mention gets a free verdict and earns nothing, and the prize moves to
something that happens on our own site: a free game of calling launches.

## 2. The game

1. **The daily five.** Each day the site lists the five new launches that
   took in the most money in the last day, across every chain the bot reads.
   Everyone plays the same five.
2. **Sign in with X.** The X account is the player. One call per player per
   coin; the first stands and cannot be changed. An account younger than the
   age rule the old contest already uses (`min_account_age_days` in
   `score.rs`) may play but does not rank.
3. **Two buttons: real or rug.** The bot's score at listing sets the odds.
4. **Calls close** six hours after listing, and stay hidden until then. After
   close the split ("62% called rug") is shown. Hiding stops people copying
   the best callers.
5. **The chain settles every call, never price.** A price fall is not a rug,
   and a rule a chart can settle is a rule a whale can move. A "rug" call is settled the
   moment the code-computed level reaches `Rugged` (ADR 0032). A "real" call
   is settled when the window ends without that. The board says it measures
   "did it rug", not "did it make money".
6. **One pot, one rule, paid only after the lawyer answers** (§7).

## 3. The odds, and why guessing earns nothing

At listing the bot has a level for the coin. The replay of old launches
(research 0051, "Measured later, by replay") gives, for each level, the share
of coins at that level that went on to rug inside the window. Call that
share q. q is fixed at listing and stored beside the pick.

| Call | Coin rugs | Coin stands |
|---|---|---|
| rug | +(1 − q) | −q |
| real | −(1 − q) | +q |

Suppose the true chance this coin rugs is p. A "rug" call then averages
p(1 − q) − (1 − p)q = p − q. A "real" call averages q − p.

- If q is right (p = q), both calls average zero. Always rug, always real,
  a coin flip, and copying the bot all average zero. The bot sits at zero on
  the board by construction.
- A player earns only by knowing p better than the bot does: a call on the
  side where they are right about p, by the gap |p − q|.

That is the whole point: the only way up the board is to know something the
bot does not. It also means the odds must be sound. If always-rug scores above
zero at some level on replay, q at that level is too low and gets fixed before
anyone real plays.

Points are kept as whole numbers: q is in basis points (0 to 10 000), so a
call scores between −10 000 and +10 000. No floating point decides a rank.

## 4. The luck line

Most of the board is noise. A thousand people guessing produce a best score
well above zero by luck alone, so "top of the board" cannot be the prize rule.

For a player with n settled calls at odds q₁…qₙ, a zero-edge player's total
has spread σ = √(Σ qᵢ(1 − qᵢ)). Their record is z = total / σ: how many
spreads above zero they are.

With N ranked players, the luck line is z* = √(2 ln(N / 0.05)). Among N
players who only guess, the chance that any of them clears z* is under 5%.
(It is the plain bound P(Z > t) ≤ e^(−t²/2) applied to all N at once; it is
a little stricter than it needs to be, and it needs no special maths
library.) For 1 000 players z* ≈ 4.45; for 100, ≈ 3.93.

To be considered at all, a player also needs at least 20 settled calls on
coins from at least 10 different creators. The spread estimate is poor on
a handful of calls, and the creator count is one of the guards in §6.

**Ranking:** players who clear the luck line, by z, highest first. Ties go to
more settled calls, then to the earlier first call, then to the X user id.
The same calls always give the same ranking. Players below the line are shown
by points, marked "within luck".

**The pot:** the week's pot goes to the top player above the line. If nobody
clears it, the pot rolls over and the bot posts "nobody beat the bot".

## 5. Four dummy players

From day one the board carries four players scored on replayed launches:
always rug, always real, coin flip, and the bot itself. They keep the board
from being empty, they teach the rule at a glance, and they are the live test
of the odds: all four must sit near zero. One that drifts clear of zero means
q has gone stale and needs refitting.

## 6. The insider hole

A dev can call "rug" on their own coin from a second account and then rug it.
That is real edge, bought with a rug. The design narrows the hole; it does not
close it, and the rules page says so.

- **The size bar.** Only the five largest launches of the day are playable.
  A dev must first take in enough money to reach the top five.
- **Many creators.** A rank needs settled calls on coins from at least ten
  different creators. One dev can only fake knowledge of their own coins.
- **The luck line.** A handful of insider wins among many ordinary calls does
  not clear it.
- **The age rule.** Fresh accounts do not rank.

## 7. What keeps it legal and honest

- **Free to play, always.** No entry fee, no paid tier, no staking a call.
- **The token never helps you play.** Holding or buying the token gives no
  extra calls, better odds, or early access. "Every trade grows the pot" is
  the only link between them.
- **Written rules on the site:** no purchase needed, who may enter, how the
  winner is picked, void where prohibited, excluded countries.
- **The game ships with no prize.** The board, calls, records and posts run
  without one. The prize switches on only after the lawyer already reviewing
  the project (ADR 0023 decision 5) answers one question: *"Is a free-entry
  weekly prize for prediction accuracy acceptable when the pot is funded by
  token trading fees and rolls over?"* If the rollover is the problem, the
  rollover goes first.
- **Model judgement never moves money** (AGENTS rule 3.1). Scoring, the luck
  line and the winner are pure code. The payout pays what that code permits.

## 8. The posts

Each post type is Josh's gate until he approves its rule once.

- **Rugged:** "Rugged. N called it. The bot did not." The callers reshare.
- **Still standing:** "N% called rug. Still standing." The dev and holders
  reshare. That is the dev's whole reward: no cash, no follow, no seat.
- **Weekly:** the winner, or "nobody beat the bot; the pot is now X".
- No link in any post or reply; the link lives in the bio (research 0051 §4).
- A handle is tagged only when that player ticked "tag me"; otherwise counts
  only (research 0051 §2).
- A post describes a coin and a count of calls. It never calls a person a
  scammer (AGENTS rule 3.4) and never states a price.

## 9. The crowd signal (later)

Once records exist, the site shows beside the bot's verdict what proven
callers said, weighted by record so a new account counts for nothing and a
dev cannot vote a coin clean with fake accounts. It is its own line. It never
moves the bot's score or level.

## 10. Build order

| Step | What | Waits on |
|---|---|---|
| G1 | Replay old launches: q per level, the window, dummy scores | 0027 slice 8, PR #117 |
| G2 | Score calls: settled calls and odds in; points, z, luck line, ranking out. Pure, in `realorrug-contest` | Nothing |
| G3 | Pick the daily five from launches the readers already see; store level and q at listing. Pure | Nothing |
| G4 | X sign-in, take a call, append-only call log, rate limits, deny by default with no config. In `realorrug-serve` | Research 0051 §1; live deploy is Josh's gate |
| G5 | Settle calls on the observation jobs | 0027 slice 9 |
| G6 | Board endpoints beside `/v1/public/hunters` and site pages | G2, G4 |
| G7 | Settlement and weekly posts; a result card players post themselves, from `crates/realorrug-serve/src/card.rs` | Josh's gate |
| G8 | The crowd signal | Settled records |
| G9 | Point the payout at the G2 ranking | The lawyer's answer; Josh's gate |

Until G1 lands, G2 and G3 are built and tested against an odds table given in
the test. The window defaults to fourteen days and becomes three if the replay
shows most rugs land inside three.

### G2 as built

`crates/realorrug-contest/src/calls.rs`, exported from the crate root.

- The luck line is compared without a `sqrt` or a division: `total^2 >=
  z*^2 * variance` is equivalent to `z >= z*` whenever `total > 0` and
  `variance > 0`, and needs one `f64` (`z*^2 = 2 ln(N / 0.05)`) computed once
  per ranking rather than a `sqrt` computed once per player. `z` itself is
  still reported on each player record (§4 says "ordered by z desc"), but only
  to order players who already cleared the line by the integer test; a
  boundary case close enough to be sensitive to that ordering is, by
  construction, statistically indistinguishable from noise. See the doc
  comment on `clears_luck_line` for the precision bound this rests on.
- A player whose every call was made at `q = 0` or `q = 10 000` has
  `variance = 0` and is placed "within luck" rather than treated as clearing
  the line automatically (dividing by zero variance is not "infinitely
  good"). §4 does not name this edge case; it cannot arise from the
  published odds table (§3, "the replay ... gives ... the share"), which
  never actually reaches the two ends of the scale.
- The coin-flip dummy strategy (§5) is a stable hash (FNV-1a) of the coin id,
  not `splitmix64`: the design note only rules out an RNG dependency, and
  FNV-1a needs no seed and no crate, just the coin id.
- `min_account_age_days` is threaded through as a caller-supplied parameter
  rather than reusing `score::Rules` directly: the daily five's eligibility
  gate (20 calls, 10 creators, age) is its own rule with its own thresholds,
  not the weekly contest's operator list and cooldown, so only the age floor
  is shared, by convention rather than by sharing the type.
