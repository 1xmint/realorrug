<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0029 — bot quality, then a Solana launch, then a research community

**Status:** accepted. Records the reasoning behind the plan Josh wrote on
2026-09-21. The decisions are [ADR 0037](../adr/0037-the-token-launches-on-pump-fun-and-its-fees-pay-for-operations.md),
[ADR 0038](../adr/0038-no-prizes-buybacks-or-holder-benefits.md) and
[ADR 0039](../adr/0039-the-launch-gates-and-the-monthly-ceiling.md); the order
of work is [plan 0002](../plans/0002-bot-quality-then-a-solana-launch.md).

## 1. The model

realorrug is an evidence-backed memecoin analyst with a Solana community
token launched through pump.fun. Creator fees support disclosed operating
costs and a reserve. There are no buybacks, no holder benefits and no
valuable prizes.

The growth loop is:

> useful verdicts → people share and investigate → more timestamped evidence
> and outcomes → better analysis → repeat users and paid API demand.

The token's price is not part of that loop, and nothing the project says may
suggest it is.

## 2. Why this shape

- **The prize was the risky part.** A prize paid from token fees ties the
  token to an expectation of reward, and X bans paying people for posts
  (research 0051 §3). Removing it removes the payout key, the eligibility
  problem (wallet ownership is not eligibility, and terms cannot waive US
  sanctions obligations) and the winner-selection code path.
- **Fees for operations is the plainest story that is true.** Servers, data
  and model calls cost money every month; the fees pay them, in public.
- **Solana is where the bot is strongest.** Its pump.fun and PumpSwap readers
  are the most mature, and pump.fun is where the launches it judges happen.
- **Quality before launch.** A launch amplifies whatever the bot already says.
  The gate is Josh reading real replies and accepting them, not a statistical
  proof of forecasting skill: the product is informational, and calibration
  can keep improving after launch as long as no probability is published
  before it is earned.

This is a lower-risk structure to put in front of counsel. **It is not a
determination of legality.** The SEC distinguishes the token itself from the
transaction and the promises made around its sale
([SEC, 2026](https://www.sec.gov/newsroom/press-releases/2026-30-sec-clarifies-application-federal-securities-laws-crypto-assets)),
which is why the launch copy and every public statement matter as much as the
token's mechanics. The questions for counsel are in
[design 0030](0030-launch-review-packet.md).

## 3. What a report must say

A reply is at most 280 characters, so it is a headline. Beside it, every
answer carries a full report, built by code from the fact sheet:

1. the strongest observed concern and the facts that support it;
2. plausible alternative explanations;
3. the checks that are missing, and when the chain was read;
4. what further evidence would change the assessment.

Deterministic evidence checks and verdict rules stay separate from the
model-written words (AGENTS §3 rule 4). A strong verdict is kept when the
evidence supports it. Missing data never reads as safety (rule 8), and the
weighted risk index is never presented as a probability.

The model integration stays as it is. Evidence and reply construction improve
first; a different model is compared only when documented failures justify it.

## 4. The community, later

After launch: sign-in, researcher profiles, evidence submissions and
source-linked discussion; an assistant that helps structure research and
proposes a forecast the user confirms; timestamped, immutable forecasts with a
named event and deadline; separate reputation for contributions and for
forecasting. It starts from the daily five (design 0028): up to five eligible
launches, a six-hour entry window, a fourteen-day horizon, forecasts shown
only after entries close, and an explicit unresolved outcome.

Votes surface useful research. They never settle a chain fact, an outcome, or
the bot's verdict. Reputation has no transferable or redeemable value.

## 5. Money

Before demand, everything together costs at most $90 a month
(ADR 0039 decision 5). The facts-only paid API and its pricing
([ADR 0036](../adr/0036-a-paid-facts-only-endpoint-priced-in-usdc-on-base.md))
stay; before it takes real payments it needs a checked facilitator, replay
protection, settlement recovery, and a way to re-fetch a paid answer whose
delivery was interrupted. Paying never buys a kinder assessment.

## 6. Deferred

No valuable prizes, buybacks, staking benefits, holder revenue share,
autonomous trading, or second-chain token launch in this release. A later
prize needs its own funded rules and an enforceable eligibility process.
