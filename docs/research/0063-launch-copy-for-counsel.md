<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0063 — Launch copy for counsel

**Date:** 2026-09-26.
**Status:** draft for counsel; nothing here is published. Assembled from
`main` at commit `9fe1e55ba6c8350f12cbe7c7bce649b420eab4ea`.

## Note to counsel

The reader of this section is now Claude's review, per [ADR
0042](../adr/0042-claudes-review-replaces-counsel-at-gate-two.md)
(2026-09-26); no outside counsel is retained, and the "counsel" heading and
copy below are otherwise unchanged.

[Design 0030](../design/0030-launch-review-packet.md) §4 asks for five texts
attached to the review packet "exactly as they will be published," so the
question there — "is any sentence a promise of profit, a reward for holding,
or an implied endorsement?" — can be answered from one document. This is that
document. Three terms recur below and are defined once, here, rather than in
each quoted text: **creator fees** are a percentage pump.fun charges on every
trade of the token and pays to whoever launched it (the "creator"); **the
treasury** is the project's own wallet that receives those fees and from
which the operator pays disclosed costs by hand; a **bonding curve** is
pump.fun's automatic pricing mechanism — the token trades against a formula
tied to how much of the supply has been bought, with no order book and no
counterparty, until the curve "graduates" to an open market. **The token does
not exist yet.** Nothing below describes something live; it describes what
will be published, and only after the gates in
[ADR 0039](../adr/0039-the-launch-gates-and-the-monthly-ceiling.md) are met.

The five texts follow, in design 0030 §4's order: the token page, the terms,
the README, the X bio, and the launch post. Each is quoted verbatim in a
block quote, and below it a table lists every sentence that mentions money,
fees, the treasury, holders, the bot's own holding, or a verdict, next to the
[ADR 0037](../adr/0037-the-token-launches-on-pump-fun-and-its-fees-pay-for-operations.md)/[ADR 0038](../adr/0038-no-prizes-buybacks-or-holder-benefits.md)/[ADR 0039](../adr/0039-the-launch-gates-and-the-monthly-ceiling.md)
decision it matches, by number. A sentence with no matching decision among
those three ADRs says **"no rule covers this"** — most of those are governed
by a different ADR (0033's price-and-moment rule, 0013's per-reply
disclaimer, 0029's "nothing trades automatically"), named in the row, or are
general non-advice language with no single decision behind it. **No sentence
below was found to contradict ADRs 0037–0039**; none is marked "does not
match."

## 1. The token page

Source: `site/src/Token.tsx`, route `/tokenomics`
([`site/src/App.tsx`](../../site/src/App.tsx)). No figure on this page is
filled at runtime from data — the page states rules, not readings — so there
is no placeholder to mark.

> # A badge, not an investment.
>
> There is no token yet. When there is one, these are the rules it is
> launched under — written down first, so they can be held against it
> afterwards.
>
> ## Right now
>
> **No token exists.** Nothing has been minted, no contract address has been
> published, and any address claiming to be this token is not. When one
> exists it will be published here and in the account's own bio, and nowhere
> else.
>
> **Project-controlled wallets have no addresses yet.** Any developer
> purchase or compensation, and the treasury wallet that receives creator
> fees, are published here on launch day with their size, address and
> transaction — not before, and not as a guess in the meantime.
>
> ## The six rules — What it is launched under
>
> A badge, not an investment. It is not a share, it does not grant a vote,
> and it buys no feature — every answer the bot gives is free to everyone,
> with or without it. Nothing here buys, sells or swaps the token
> automatically; any future trading needs its own decision, in public, first.
>
> 01. **It launches through pump.fun, on Solana, paired with SOL.** Not
>     Robinhood Chain, not any other chain, and not launched yet. There is no
>     address to check until it is.
>
> 02. **Project-controlled wallets are published with their addresses.** Any
>     developer purchase or compensation is disclosed the same way — with its
>     size, its wallet and its transaction, from launch day, not before.
>
> 03. **Creator fees pay disclosed operating costs and a reserve. Nothing
>     else.** Servers, data and model usage, published as categories. No
>     prize, no buyback, no yield, no revenue share to anybody who holds the
>     token.
>
> 04. **Holding the token buys nothing in the product.** Every answer the bot
>     gives is free to everyone, with or without it. It is not a share, it
>     grants no vote, and it changes no verdict.
>
> 05. **A price is stated with the moment it was read, and never as a
>     promise.** The bot gives a price or market cap only with the time it
>     read it, never says what the token will do, and never tells anyone to
>     buy, sell or hold.
>
> 06. **The token is judged like any other.** Same rule, same fact sheet,
>     same refusals — including about its own launch and its own wallets.
>     Ask it.
>
> ## Where the money goes — A fee on trading pays disclosed costs, and
> nothing else
>
> 1. Somebody trades the token on pump.fun. A creator fee is charged on the
>    trade, in SOL.
> 2. The fee is credited to the project's own treasury wallet, address
>    published at launch.
> 3. The treasury pays disclosed operating costs — servers, data, model
>    usage — and holds a reserve.
> 4. Nothing is paid to holders. There is no prize, no buyback and no yield
>    to distribute.
>
> ## The fee — Set by pump.fun, not by this project
>
> pump.fun's fee rate depends on the launch stage and the token's market
> capitalisation, and both change as the token trades. Rather than quote a
> number here that would be wrong by the time somebody reads it, this page
> links the source instead.
>
> [pump.fun's fee documentation → [https://pump.fun/docs/fees]]
>
> ## Risks — What this page will not soften
>
> - **It can go to zero.** A memecoin with no revenue, no product entitlement
>   and no promise behind it can lose all of its value, and most do.
> - **This is not an investment.** Nothing here is an offer, a solicitation,
>   or advice to buy, sell or hold anything.
> - **There is no promise of fee income.** Fees exist only if the token
>   trades, and the project makes no commitment about how much that will ever
>   be.
> - **The fee rate is not fixed.** pump.fun sets it by launch stage and
>   market cap, and can change how it sets it. See the link above rather than
>   a number frozen here.
>
> ## Before any of this happens — What has to be true first
>
> - **The bot's replies are good enough.** The operator reviews its answers
>   on real Solana launches and accepts them one by one; accepted answers are
>   re-checked on every change.
> - **A legal and tax review.** A precondition, not a follow-up, and the
>   answer may be no.
> - **X approves automated replies in writing.** Without it the site runs and
>   the bot stays private.
> - **The launch is checked and signed by the operator.** The fee recipient
>   and the token's settings are read back before anything is announced.
>   Nothing on the server holds a key.

| Sentence | Decision |
|---|---|
| "Project-controlled wallets have no addresses yet... the treasury wallet that receives creator fees, are published here on launch day with their size, address and transaction." | ADR 0037 #2, #7 |
| "Any developer purchase or compensation is disclosed the same way — with its size, its wallet and its transaction, from launch day, not before." (rule 02) | ADR 0037 #7 |
| "It is not a share, it does not grant a vote, and it buys no feature." (intro) | ADR 0038 #3 |
| "Nothing here buys, sells or swaps the token automatically; any future trading needs its own decision, in public, first." | no rule covers this (ADR 0029) |
| "Creator fees pay disclosed operating costs and a reserve. Nothing else." (rule 03) | ADR 0037 #2 |
| "No prize, no buyback, no yield, no revenue share to anybody who holds the token." (rule 03 plain) | ADR 0038 #1, #2 |
| "Holding the token buys nothing in the product." (rule 04) | ADR 0038 #3 |
| "It is not a share, it grants no vote, and it changes no verdict." (rule 04 plain) | ADR 0038 #3 |
| "A price is stated with the moment it was read, and never as a promise." (rule 05) | no rule covers this (ADR 0033) |
| "The bot gives a price or market cap only with the time it read it, never says what the token will do, and never tells anyone to buy, sell or hold." (rule 05 plain) | no rule covers this (ADR 0033) |
| "The token is judged like any other." (rule 06) | no rule covers this (ADR 0033's own-token rule) |
| "Same rule, same fact sheet, same refusals — including about its own launch and its own wallets." (rule 06 plain) | no rule covers this |
| "A creator fee is charged on the trade, in SOL." (step 1) | ADR 0037 #2 |
| "The fee is credited to the project's own treasury wallet, address published at launch." (step 2) | ADR 0037 #2, #7 |
| "The treasury pays disclosed operating costs — servers, data, model usage — and holds a reserve." (step 3) | ADR 0037 #2 |
| "Nothing is paid to holders. There is no prize, no buyback and no yield to distribute." (step 4) | ADR 0038 #1, #2 |
| "pump.fun's fee rate depends on the launch stage and the token's market capitalisation, and both change as the token trades." | ADR 0037 #8 |
| "A memecoin with no revenue, no product entitlement and no promise behind it can lose all of its value, and most do." | no rule covers this |
| "Nothing here is an offer, a solicitation, or advice to buy, sell or hold anything." | no rule covers this |
| "There is no promise of fee income. Fees exist only if the token trades, and the project makes no commitment about how much that will ever be." | ADR 0037 #8 |
| "The fee rate is not fixed. pump.fun sets it by launch stage and market cap, and can change how it sets it." | ADR 0037 #8 |
| "A legal and tax review. A precondition, not a follow-up, and the answer may be no." | ADR 0037 #5; ADR 0039 #2 |
| "X approves automated replies in writing. Without it the site runs and the bot stays private." | ADR 0039 #4 |
| "The launch is checked and signed by the operator... Nothing on the server holds a key." | ADR 0037 #3 |
| "The bot's replies are good enough. The operator reviews its answers on real Solana launches and accepts them one by one; accepted answers are re-checked on every change." | ADR 0039 #1 |

## 2. The terms

Source: `site/src/Terms.tsx`, route `/terms`. No runtime-filled figure
appears on this page.

> # Terms of use — What this is, and what it is not
>
> **Real or Rug is measurement and information, not financial advice.** It is
> operated by Josh Fair. Nothing here is an offer to trade for you, to hold
> your money, or to buy or sell anything. Using this site means accepting the
> terms below.
>
> ## Who operates this
>
> Real or Rug is operated by Josh Fair. The account on X is automated, and it
> says so on the account and on [the about page [/about]]. The software
> behind it is [published in full [SOURCE — the repository, `site/src/honesty.ts`'s `SOURCE` constant]]
> under the Apache License 2.0, so the rules described here can be checked
> against the code that implements them rather than taken on trust.
>
> ## What the service does
>
> It reads public chain data, on Robinhood Chain and on Solana — token
> launches, the accounts paid in a launch block, bonding curves, what a
> creator has launched before — and reports what it measured, with the
> moment it measured it. Every figure is published with a date because every
> figure is expected to move.
>
> It does not predict prices. It does not rank coins as investments, it does
> not tell you a coin will rise or fall, and it does not tell you to buy or
> sell. That is a limit of the measurements, not modesty: nothing in this
> data supports a claim about where a price is going.
>
> ## Not financial advice
>
> Nothing on this site, and nothing in a reply the account posts, is
> financial, investment, legal, tax or accounting advice, a personal
> recommendation, or a suggestion that any trade is suitable for you. It is
> general information about public data.
>
> Decisions you make with it are yours. Trading tokens of this kind loses
> money for most people who try it, and this site publishes the figures that
> say so rather than the ones that would sell it.
>
> ## No offer, no custody, no management
>
> Nothing here is an offer or a solicitation to buy or sell any token or
> security. The operator does not accept deposits, does not manage money for
> anybody, does not trade on your behalf, and never takes custody of anything
> of yours. There is nothing to connect and nothing to sign.
>
> **Nobody running this will ever ask you for a private key or a seed
> phrase, and nobody will message you first asking for one.** Anyone who does
> is not us, whatever name they are using.
>
> ## No guarantee of accuracy
>
> The figures are measurements taken at a moment, from data that changes,
> using instruments that have been wrong before. One earlier measurement of
> these same quantities was out by a factor of 2.7 nine days later, which is
> why nothing here is presented as a constant and everything carries its
> date.
>
> When something is found to be wrong, the correction is published in the
> same place as the original and the account posts it. Corrections are not
> quietly edited in. But no accuracy is guaranteed, and you should check
> anything that matters to you against the chain itself — which is why the
> links to do so are on the page.
>
> ## No guarantee of availability
>
> The site, the account and the data it reads may be unavailable, delayed,
> incomplete or discontinued at any time and without notice. The account may
> not answer. A figure may fall back to an older committed snapshot, in which
> case the page says so.
>
> Everything is provided as it is, without warranty of any kind, to the
> extent the law allows.
>
> ## No prize, and no benefit from holding the token
>
> The weekly prize has ended. There is no live contest, no pool, no claim
> window and no eligibility rule to state here — a historical record of the
> weeks that ran while it was live is kept on [the history page [/payouts]],
> and nothing new is added to it.
>
> The token — launching through pump.fun on Solana, paired with SOL, not yet
> launched — confers no rights, revenue or benefits of any kind. It is not a
> share of anything, it grants no vote, it buys no feature in this product,
> and holding it does not change a verdict, entitle you to a payment, or earn
> a return. Creator fees pay disclosed operating costs and a reserve; they do
> not pay you. Full detail, including the risks, is on [the tokenomics page
> [/tokenomics]]. Where that page and this one differ, that page is the
> specific statement and this is the summary.
>
> ## Reports are informational, and partly AI-generated
>
> Every reply is a report on public chain data, not financial advice, and
> not a personal recommendation. The checks behind a verdict are
> deterministic — the same facts always produce the same score and the same
> level — but the sentence explaining them is written by a language model.
> The model may describe what happened; it does not decide the score, the
> level, or move any money.
>
> ## Using the site
>
> Read it, share it, quote it. Do not attack it, do not try to interfere with
> its availability for other people, and do not present yourself as this
> account or as its operator.
>
> ## Links to other sites
>
> Links to X, to a chain explorer and to the repository lead to services this
> operator does not run and cannot vouch for. Their terms govern what happens
> once you arrive.
>
> ## What these terms deliberately do not say
>
> They name no governing law, no jurisdiction, no arbitration procedure, no
> liability cap and no company. None of that has been established for this
> project, and writing down a clause nobody decided would be exactly the kind
> of confident, unchecked claim this whole site exists to argue against.
>
> If a lawyer adds that language later it will appear here, dated, like
> everything else. Until then this page tells you what is true and stops.
>
> ## Changes, and how to raise one
>
> This page is built from a public repository, so every change to it is a
> commit anybody can read and compare. If something here is wrong, unclear,
> or contradicts another page, say so on [the repository's issues [ISSUES —
> `site/src/honesty.ts`'s `ISSUES` constant]] or through [the contact page
> [/contact]].
>
> Last reviewed 2026-09-08. Measured, not predicted. Not financial advice,
> not a recommendation, and not a solicitation to buy or sell anything.

| Sentence | Decision |
|---|---|
| "Nothing here is an offer to trade for you, to hold your money, or to buy or sell anything." | no rule covers this |
| "It does not predict prices... it does not tell you to buy or sell." | no rule covers this (ADR 0033) |
| "Nothing on this site... is financial, investment, legal, tax or accounting advice." | no rule covers this |
| "Trading tokens of this kind loses money for most people who try it." | no rule covers this |
| "Nothing here is an offer or a solicitation to buy or sell any token or security... does not manage money for anybody, does not trade on your behalf." | no rule covers this |
| "The weekly prize has ended. There is no live contest, no pool, no claim window and no eligibility rule to state here." | ADR 0038 #1, #4 |
| "The token... confers no rights, revenue or benefits of any kind." | ADR 0037 #1; ADR 0038 #2 |
| "It is not a share of anything, it grants no vote, it buys no feature in this product, and holding it does not change a verdict, entitle you to a payment, or earn a return." | ADR 0038 #2, #3 |
| "Creator fees pay disclosed operating costs and a reserve; they do not pay you." | ADR 0037 #2 |
| "The checks behind a verdict are deterministic... it does not decide the score, the level, or move any money." | ADR 0037 #3 |
| "Not financial advice, not a recommendation, and not a solicitation to buy or sell anything." (footer) | no rule covers this |

## 3. The README

Source: `README.md`, root of the repository, transcribed verbatim (the whole
file is short enough to quote in full; the table below).

> # realorrug
>
> An X account you summon about a memecoin. It answers with what the chain
> shows — who launched it, what was bought in the launch block, what the
> curve holds — and a check after generation refuses any number that is not
> on the fact sheet. **Real or rug? It shows the facts. You decide.**
>
> A community token, realorrug, will launch through pump.fun on Solana,
> paired with SOL. Its creator fees go to a disclosed treasury and pay the
> project's disclosed operating costs and a reserve. Holders get nothing from
> the project: no revenue share, yield, buyback, airdrop, prize or better
> verdicts. The bot holds some of the token in public and trades none of it.
> [ADR 0037 [docs/adr/0037-the-token-launches-on-pump-fun-and-its-fees-pay-for-operations.md]]
> has the launch, [ADR 0038 [docs/adr/0038-no-prizes-buybacks-or-holder-benefits.md]]
> what holders do not get, and [ADR 0039 [docs/adr/0039-the-launch-gates-and-the-monthly-ceiling.md]]
> what must happen first. The order of work is
> [plan 0002 [docs/plans/0002-bot-quality-then-a-solana-launch.md]].
>
> **Nothing is launched.** The token does not exist yet. It launches after
> the bot's Solana replies pass the owner's review and counsel has read [the
> review packet [docs/design/0030-launch-review-packet.md]].
>
> ## What is here
>
> | path | what |
> |---|---|
> | `crates/realorrug-analyst` | the summoned-reply loop: mention parser,
> admission gate, reply log, X client |
> | `crates/realorrug-roast` | the reply: fact sheet, model, fidelity check,
> banned verdict words |
> | `crates/realorrug-onchain` | the dossier, read from the chain on demand |
> | `crates/realorrug-contest` | the retired weekly contest, kept so its
> records replay; the daily five's scoring |
> | `crates/realorrug-payout` | retired (ADR 0037): the old Robinhood prize
> payout, kept for history; its binary refuses to run |
> | `crates/realorrug-robinhood` | Robinhood Chain, read (the bot still
> answers about tokens there): receipts, and Pons v2 launches, trades and fee
> sweeps; the launch check behind `realorrug launch-check`; the fee escrow's
> credits, claims and claimable balance |
> | `crates/realorrug-serve` | the public site's documents |
> | `crates/realorrug-cli` | `realorrug dossier`, `roast`, `analyst`,
> `contest`, `launch-check`, `label-outcomes`, `narratives`, `audit`,
> `model-prices` |
> | `crates/realorrug-agent`, `crates/realorrug-model`,
> `crates/realorrug-provider` | the boundary a model sits behind, the model
> client, the spend meter |
> | `crates/realorrug-types`, `crates/realorrug-decode`,
> `crates/realorrug-pumpfun`, `crates/realorrug-journal` | shared vocabulary,
> Solana decoding, pump.fun, the hash-chained journal |
> | `site/` | the public site |
> | `deploy/` | systemd units, the runbook, and the launch-day checklist
> (`deploy/LAUNCH.md`) |
>
> It began as part of [Radar [https://github.com/1xmint/theradar]], a Solana
> research system, and was split out on 2026-09-13.
> [ADR 0024 [docs/adr/0024-the-bot-stands-alone.md]] says what was copied and
> why.
>
> ## Build and test
>
> ```bash
> just check   # build, tests, lint, fmt
> just ci      # everything CI runs, including the site and cargo-deny
> just site    # the public site alone
> ```
>
> On Windows under Git Bash, export a toolchain whose linker works:
>
> ```bash
> export REALORRUG_CARGO="cargo +stable-x86_64-pc-windows-gnullvm"
> ```
>
> ## Licence
>
> Apache-2.0.

| Sentence | Decision |
|---|---|
| "A community token, realorrug, will launch through pump.fun on Solana, paired with SOL." | ADR 0037 #1 |
| "Its creator fees go to a disclosed treasury and pay the project's disclosed operating costs and a reserve." | ADR 0037 #2 |
| "Holders get nothing from the project: no revenue share, yield, buyback, airdrop, prize or better verdicts." | ADR 0038 #1, #2, #3 |
| "The bot holds some of the token in public and trades none of it." | ADR 0037 #7 |
| "Nothing is launched... It launches after the bot's Solana replies pass the owner's review and counsel has read the review packet." | ADR 0039 #1, #2 |
| "`crates/realorrug-payout` \| retired (ADR 0037): the old Robinhood prize payout, kept for history; its binary refuses to run" | ADR 0038 #4 |
| "`crates/realorrug-contest` \| the retired weekly contest, kept so its records replay; the daily five's scoring" | ADR 0038 #5 |
| "`crates/realorrug-robinhood` \| ...the fee escrow's credits, claims and claimable balance" (a Robinhood Chain legacy fee escrow, not the pump.fun creator fee) | no rule covers this |

## 4. The X bio

Source: `crates/realorrug-analyst/src/bio.rs` (`Bio::from_vars`, `Bio::render`),
configured by `deploy/analyst.env.example` — never `/etc/realorrug/*.env`,
which this task does not read. A rendered bio is `<lead> · <status> · Not
financial advice.`, where `<lead>` is the operator's fixed configuration from
`REALORRUG_BIO_LEAD` and `<status>` is generated at render time from the
account's live contest/pool state (not from any environment variable, so
there is nothing in the example file to fill it with — the placeholder for it
is the shape shown in the code's own doc comment, quoted below). `bio::MAX`
is 160 characters and the disclaimer is `bio::DISCLAIMER`, `"Not financial
advice."`.

**`deploy/analyst.env.example` leaves `REALORRUG_BIO_LEAD` commented out and
unset** (line 364, `# REALORRUG_BIO_LEAD=`) — it is the only bio variable the
file names. Per `Bio::from_vars`, an unset lead returns `None`, and per the
module's own doc comment this is deliberate: "unset means the bio is never
written at all," so the account's existing bio text is left standing,
unchanged, indefinitely, by an instance run from this example file as-is.
There is therefore no lead text to quote — that part is absent — and no
status can be computed either, since the status depends on live data this
static file cannot supply. The one fixed, always-present part is the
disclaimer, which is not configurable and cannot be turned off
(`from_vars` refuses any lead that would leave it no room).

> `Not financial advice.`

The shape a configured instance's bio would take, from the code's own
example in `deploy/analyst.env.example` (lines 333–336, quoted, not this
task's own construction):

> `<REALORRUG_BIO_LEAD> - Week of 2026-09-07 leads: @somebody, 12 pts`
> `<REALORRUG_BIO_LEAD> - Won the week of 2026-09-07: @somebody. Claim: reply to the prompt under your post by 2026-09-21`
> `<REALORRUG_BIO_LEAD> - Paid 0.1234 SOL to the week's winner`

(Note for engineering, not counsel: the example file's own comment joins
with `" - "`; `bio.rs`'s actual `JOIN` constant is `" · "` — a drift between
the comment and the code this task does not fix, since it owns neither file.)

| Sentence | Decision |
|---|---|
| `Not financial advice.` (the only text this deployment ever writes, per the example file as it stands) | no rule covers this (ADR 0033 decision 6, the bio's disclaimer rule) |

## 5. The launch post

**No text for a launch post exists anywhere in the repository** — a search of
`docs/` and every crate under `crates/` found none. What follows is drafted
for this packet, states only what ADRs 0037–0039 and the README already say,
and is **proposed, not published; Josh writes or approves the final text**.
It is not to be posted by anyone, and `REALORRUG_X_PUBLISH` stays off
regardless.

> A community token, realorrug, launches through pump.fun on Solana, paired
> with SOL. Creator fees go to a disclosed treasury for disclosed operating
> costs. Holders get nothing from the project — no revenue share, yield,
> buyback, prize or better verdicts. Not financial advice.

275 characters, under the 280-character limit.

| Sentence | Decision |
|---|---|
| "A community token, realorrug, launches through pump.fun on Solana, paired with SOL." | ADR 0037 #1 |
| "Creator fees go to a disclosed treasury for disclosed operating costs." | ADR 0037 #2 |
| "Holders get nothing from the project — no revenue share, yield, buyback, prize or better verdicts." | ADR 0038 #1, #2, #3 |
| "Not financial advice." | no rule covers this (ADR 0033 decision 6) |

## What engineering checked

- The honesty tests that cover this site copy: `site/src/honesty.test.ts`,
  16 tests (`measuredAgo`, `count`, `links`, `eth`, and the `FORBIDDEN_CLAIMS`
  block naming "prize", "payout", "buyback", "holders earn" and "yield").
  Run with `npm test -- honesty` inside `site/` (the package's `test` script
  is `vitest run`). Result on this branch, from `main` at
  `9fe1e55ba6c8350f12cbe7c7bce649b420eab4ea`: **16 passed, 0 failed.**
- The bio length: `bio::MAX` is 160 characters; with `REALORRUG_BIO_LEAD`
  unset in the example file, no bio is ever rendered by this deployment (see
  §4), so there is no length to check against a live figure. The disclaimer
  alone, `"Not financial advice."`, is 22 characters.
- The launch post length: 275 characters, under the platform's 280-character
  limit (§5).
- **The one open question for Josh:** approve the drafted launch post in §5
  as written, or rewrite it. Nothing posts it until you say which.

None of the five texts read against ADRs 0037–0039 was found to contradict
them; see the tables above for every sentence checked and which decision it
matches.
