<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0030 — the launch review packet

**Status:** draft for counsel. What the operator sends a lawyer and an
accountant before the pump.fun launch ([ADR 0039](../adr/0039-the-launch-gates-and-the-monthly-ceiling.md)
gate two). The facts are stated as they stand on 2026-09-21; every blank is
filled by Josh before sending. This is a list of questions, not legal advice
and not a finding that anything is lawful.

## 1. Who runs it

- Operated by one individual in **Tennessee**, USA. No company is formed yet.
  _Question: should a company hold the treasury, the X account, the servers
  and the API revenue before launch, and which kind?_
- The bot, website and paid API run on a single rented server.

## 2. The token

- Launched through pump.fun on Solana, paired with SOL
  ([ADR 0037](../adr/0037-the-token-launches-on-pump-fun-and-its-fees-pay-for-operations.md)).
  The whole supply goes on pump.fun's bonding curve; there is no presale,
  team allocation or vesting.
- Developer purchase at launch: **____ SOL** from wallet **____**, disclosed on
  the site with its address. The bot's own holding: **____**, address **____**
  ([ADR 0029](../adr/0029-the-bot-holds-its-own-token-openly.md)).
- Holders receive nothing from the project: no revenue, yield, buyback,
  staking benefit, airdrop, preferential verdict or contest advantage
  ([ADR 0038](../adr/0038-no-prizes-buybacks-or-holder-benefits.md)).
- _Question: given the SEC's 2026 statement that the token and the
  transaction and promises around its sale are assessed separately, does
  anything in §4's launch copy, the dev buy, or the operator's continued work
  on the analyst create an investment-contract risk? What must the copy never
  say?_

## 3. Creator fees

- pump.fun pays the creator a share of trading fees. Checked 2026-09-21 on
  [pump.fun/docs/fees](https://pump.fun/docs/fees) (page dated 20 May 2026):
  **0.300%** of trade value on the bonding curve (of a 1.25% total fee), and
  after graduation to a PumpSwap pool a rate that falls with market cap, from
  **0.300%** at 0–420 SOL down to **0.050%** at 98,240 SOL and above. These are
  re-read on launch day.
- Fees go to a disclosed treasury wallet **____** and are spent by the
  operator, by hand, on disclosed operating costs (servers, data, model calls)
  and a reserve. Nothing promises any level of fee income.
- _Questions: how is creator-fee income taxed for a Tennessee individual (or
  the company in §1)? When is it income — on accrual in the wallet or on
  claim? How are SOL-denominated receipts valued and recorded? Does paying
  operating costs from the treasury need its own records?_

## 4. Launch copy and public statements

Attached before sending: the token page, the terms, the README, the X bio and
the launch post, exactly as they will be published. They must match ADRs
0037–0039. _Question: is any sentence a promise of profit, a reward for
holding, or an implied endorsement?_ The attachment is
[research 0063](../research/0063-launch-copy-for-counsel.md), which quotes
all five texts verbatim and tables every sentence naming money against the
ADR decision it matches.

## 5. The analyst and the paid API

- The bot publishes informational assessments of tokens, from on-chain data,
  including its own token, judged by the same rules.
- A facts-only paid API priced in USDC on Base
  ([ADR 0036](../adr/0036-a-paid-facts-only-endpoint-priced-in-usdc-on-base.md))
  is built but not taking payments. Paying never buys a kinder assessment.
- _Questions: does publishing token risk assessments, or selling the facts
  behind them, carry adviser, publisher or consumer-protection obligations?
  Does holding the token while assessing it (disclosed) need more than
  disclosure? What does the API's revenue need for sales tax and records?_

## 6. X

Automated AI replies are enabled only after X's written approval
(ADR 0039 decision 4). _Question: any obligation beyond X's own rules?_

## 7. What is not in this release

No prizes, buybacks, staking benefits, holder revenue share, autonomous
trading or second-chain launch. A later prize would come back to counsel with
its own rules, funding and eligibility process, since wallet ownership is not
eligibility and terms cannot waive US sanctions obligations
([OFAC guidance](https://ofac.treasury.gov/system/files/126/virtual_currency_guidance_brochure.pdf)).
