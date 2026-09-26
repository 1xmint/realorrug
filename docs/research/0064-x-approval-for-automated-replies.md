<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0064 — X approval for automated replies

**Date:** 2026-09-26.
**Status:** live-doc-read research. CHECKED where a primary X page was
fetched today and quoted; OBSERVED where an X-staff forum reply (not the
primary policy text) is the source; UNVERIFIED/secondary where only press
coverage of an X announcement was found. No sign-up, form submission or
post was made anywhere (out of this packet's boundary).

## Why

[ADR 0039](../adr/0039-the-launch-gates-and-the-monthly-ceiling.md) decision
4: "Automated AI replies on X wait on X's written approval... X's automation
rules are read on the day, not from memory." [Design
0030](../design/0030-launch-review-packet.md) §6 asks the same question
without resolving it. [Design 0024](../design/0024-off-topic-replies.md) §5
already read `help.x.com/en/rules-and-policies/x-automation` on 2026-09-16
and quoted the "prior written and explicit approval" line, marking it "not
established" whether the account holds it. [Research
0051](0051-the-daily-five-what-to-check-first.md) §2 hit a Cloudflare gate on
the same page on 2026-09-18 and had to rely on secondary quotes. This
document re-fetches the primary page today, and — because the policy text
names a process ("contact your dedicated point of contact or submit a
request through the developer portal") that prior research had no way to
test — goes one step further and reads the X developer forum for what
actually happens when developers try to use it.

## 1. Does a written-approval process exist, and where do you apply?

**The policy text itself, fetched directly today** (no Cloudflare block this
time — a plain `navigate` in the browser tool rendered it cleanly),
`https://help.x.com/en/rules-and-policies/x-automation`, page marked
"Updated April 2026," read 2026-09-26, §II.B.3 "AI-Powered Automated
Replies":

> "you may leverage artificial intelligence (AI) technologies to create
> automated reply bots that generate dynamic, context-aware responses...
> However, to safeguard user experience, prevent potential misuse, and
> ensure alignment with our rules, the deployment or operation of any AI
> reply bot requires prior written and explicit approval from X. Contact
> your dedicated point of contact or submit a request through the developer
> portal for review."

That is the whole of the written instruction. It names no form, no URL, no
turnaround time, no address. This matches design 0024 §5's 2026-09-16 quote
verbatim — the page has not changed on this point since April 2026.

**What "submit a request through the developer portal" means in practice,
from the developer forum (devcommunity.x.com), read 2026-09-26 — this is new
evidence, not in design 0024 or research 0051:**

Multiple developers have posted asking exactly this question, because the
form doesn't exist:

> "I do not have a dedicated point of contact and there is no way to open a
> request through the dev portal, what is the correct way for me to do
> this? I dont want to take the risk to go live and be banned."
> — lanaAI_bot, devcommunity.x.com, "How do I get AI bot account approval,"
> May 6 2026, read 2026-09-26
> (<https://devcommunity.x.com/t/how-do-i-get-ai-bot-account-approval/264768>)

An account tagged **"X Staff"** (`taycaldwell`) has answered several such
threads, most recently 3 days before this read (i.e. ~2026-09-23), with the
same substance each time. From the thread closed 2026-09-23,
<https://devcommunity.x.com/t/request-for-written-approval-for-an-ai-powered-automated-reply-bot/276526>,
read 2026-09-26:

> "@adabanasanka — mention-triggered only, one reply per interaction,
> in-reply-to the source post, official X API, AI/automated disclosure, no
> unsolicited mentions or trend-jacking. If it fits those rules, no extra
> written approval is needed... No extra written approval is needed if
> those constraints are actually enforced in software before you enable
> replies."

And from an earlier, functionally identical closed-and-marked-"SOLVED"
thread, <https://devcommunity.x.com/t/ai-powered-automated-reply-bot-approval-request/274245>,
Aug 27 2026, read 2026-09-26, the same staff account states it flatly:

> "There is no separate approval form in the Developer Portal. No extra
> written approval is needed if it stays inside those rules. Do not enable
> proactive/unsolicited replies."

The rules that same reply lists as the substitute for written approval:

> "The other person initiates (mention, reply, or quote). Do not
> search/stream the timeline and @mention people who did not interact with
> you. One automated reply per interaction, in-reply-to the source post,
> using the official API. Automated label on the account. No bulk
> DMs/follows/likes, no trend-jacking, no duplicate cross-posting."

**Verified vs. inferred, stated plainly:** the "requires prior written and
explicit approval" sentence is verified, primary-source text, unchanged
since April 2026. What is *not* verified at primary-source tier is that this
sentence describes a process anyone can actually complete — no dedicated
point of contact and no developer-portal form for it is stated to exist by
X, by a person carrying the "X Staff" forum tag, twice, in threads four
weeks apart (Aug 27 and ~Sep 23, 2026). This is X-staff forum guidance, not
a policy-page rewrite; the help.x.com page still says what it said in
April. Treat the staff answer as the best available reading of an
unworkable written instruction, not as a supersession of it. **The
counterexample to watch for:** a later thread, or a policy-page edit, where
X reverses this and actually opens a form or a contact channel — re-check
before relying on "no approval needed" past this date.

**What X's rules require instead, independent of whether written approval
exists (§II.B.2, same page, read 2026-09-26):**

- Reply only to users who mention/reply/quote the account first ("opted
  in") — "The reply and mention functions are intended to make
  communication between X users easier. Automating these actions to reach
  many users on an unsolicited basis is an abuse of the feature, and is not
  permitted."
- One automated reply per user interaction.
- A clear, easy opt-out, honored promptly.
- The **Automated account label**, confirmed on a separate primary page,
  `https://help.x.com/en/using-x/automated-account-labels`, read 2026-09-26:
  "Our Automation rules require these accounts to display labels and remain
  connected to a human-run account." (Note: the page's own language ("the
  invitation to our test group") suggests this label rollout is/was staged
  by invitation, not universally self-service for every account — worth
  confirming the realorrug bot account has actually been offered the label
  toggle before launch, not assuming it is.)
- No duplicate/substantially-similar posts across users (relevant to any
  fixed fallback line, per design 0024 §5, unchanged today).
- No scripting/non-API automation, no rate-limit circumvention.

This matches — and does not contradict — realorrug's own design 0024 §2
answer shape (mention-gated, one reply per interaction, disclosed,
capped).

## 2. Crypto / token-promotion / self-token rules

**Nothing crypto-specific in the X Developer Agreement or Developer
Policy.** `https://docs.x.com/developer-terms/agreement` (last updated
April 27 2026, read 2026-09-26) contains no clause on cryptocurrency,
tokens, or digital assets. `https://docs.x.com/developer-terms/policy`,
checked today for the same terms, has none either — its only
compensation-adjacent language is a generic "virtual currency" restriction
inside X Cards, unrelated to blockchain tokens (consistent with research
0051 §1's read of the same policy on 2026-09-18, which also found no such
clause).

**The one page that does name cryptocurrency explicitly scopes itself to
paid ads, not organic replies.** `https://business.x.com/en/help/ads-policies/ads-content-policies/financial-services`,
read 2026-09-26, opens: "This policy applies to monetization on X and X's
paid advertising products." It requires "prior authorization from X by
getting certified" per category (crypto vs. NFT vs. blockchain games are
separate certifications) and lists per-country licensing requirements for
"Cryptocurrency exchanges, wallets, kiosks/ATMs..." This is X's Ads
policy — it governs promoted posts and creator monetization/paid
partnerships, not an organic account replying to mentions or posting its
own launch announcement unpaid. **Inferred, not stated by X:** if
realorrug never runs a paid ad or a labeled paid partnership for the token,
this page does not appear to apply; nothing found today states that an
*organic*, unpaid post about one's own token is separately restricted by
this ads policy.

**A live, secondary-sourced risk that does apply to organic posts: X's
"first crypto post" auto-lock.** Reported by CCN.com (published 2026-04-03,
read 2026-09-26, quoting X Head of Product Nikita Bier's own posts on X
directly, so semi-primary but not independently re-verified against Bier's
original post today — that original post could not be fetched, matching
research 0051 §3's finding that x.com post fetches return HTTP 402):

> "X is 'in the process of implementing auto-locking + verification if a
> user posts about cryptocurrency for the first time in the history of
> their account.'" ... "Accounts with more than 10,000 followers that
> suddenly launch a memecoin or crypto promotion despite having no prior
> connection to the space" trigger "mandatory ownership verification."

This was announced 2026-04-02 as "in the process of implementing" — no
source read today confirms it has fully shipped, and no source names an
exemption for an account that has always been about a crypto project (which
would give it prior "connection to the space" and likely avoid the
"sudden, no history" trigger the rule targets). **NOT VERIFIED:** whether
this auto-lock is live today, 2026-09-26, or still rolling out; **CHECKED**
only that X announced the intent to build it. If the account posting the
launch announcement has no prior crypto-post history, budget for the
possibility that the very first crypto-mentioning post gets the account
auto-locked pending identity verification — a launch-day risk worth testing
with an early, low-stakes crypto-adjacent post well before the actual
launch announcement, not discovering on launch day.

## 3. Site-only launch: may Josh hand-post the launch announcement?

Nothing found today distinguishes a manual post by the account owner from
an automated one — the entire automation-rules page (§II) governs
*automated* activity; a human logging in and posting is not "automated
activity" under this policy's own framing ("this page is primarily intended
for developers... third-party application"). So: **yes, a hand-typed launch
announcement from the bot's own account is not gated by the AI-reply-bot
approval question at all** — that gate is specific to automated replies,
not to any post a human chooses to type and click "Post" on.

What *does* still apply to a manual post, verified today:

- The **Automated account label**, if the account already carries one
  (help.x.com/en/using-x/automated-account-labels), stays on the account
  regardless of whether a given post was hand-typed — the label describes
  the account, not the post.
- The **first-crypto-post auto-lock** risk above (§2) applies equally to a
  manual post — it is triggered by content, not by whether the API or a
  human posted it.
- Ordinary X Rules on financial/crypto scam content (impersonation,
  deceptive giveaways, misleading links) apply to any post, automated or
  not; nothing found today gives an organic self-promotional token
  announcement a special carve-out or a special restriction beyond those
  general rules.
- Duplicate-content and spam rules are automation-specific ("You may not
  post duplicative or substantially similar posts") and do not on their
  face constrain a single, one-time manual announcement post.

## 4. API cost to post replies at small volume

`https://docs.x.com/x-api/getting-started/pricing`, read 2026-09-26,
CHECKED, quoted directly — pay-per-usage, no subscription tiers (Free/
Basic/Pro do not appear on this page at all):

| Action | Cost |
|---|---|
| Post: Create | $0.015 per request |
| Post: Create (with URL) | $0.200 per request |
| **Post: Create (summoned)** | **$0.010 per request** |
| Posts: Read | $0.005 per resource |
| User: Read | $0.010 per resource |

These match research 0051's 2026-09-18 figures exactly for the first two
rows and Posts/User reads — **unchanged in the eight days since**. The
**"Post: Create (summoned)" row at $0.010/request is new information not
in research 0051 or design 0024**, and it is the one that matters most for
this project's shape: X's own pricing-page terminology defines a "summoned"
post as a reply sent because the account was mentioned or quoted first —
exactly realorrug's "reply only to users who mention the account" design.
It is cheaper than a plain created post ($0.010 vs $0.015), and per
secondary summaries read today (opentweet.io, vorplabs.com, both
2026-09-26) a summoned reply containing a URL is also billed at the
summoned rate rather than the $0.200 link surcharge — **this second point
is UNVERIFIED at primary tier**: the pricing page's table does not itself
split "summoned + URL" into its own row, so this is a secondary reading of
the page's intent, not a quoted number.

At $0.010/reply (summoned rate) plus a $0.005 read to fetch the mentioning
post, each reply costs roughly $0.015 all-in — **about 6,000 mention-reply
cycles for $90**, before any model or RPC cost is taken out of the same
ceiling (ADR 0039 decision 5 shares the $90 across model, RPC and X
budgets, so the real reply budget is smaller than $90 alone implies). The
pricing page also documents a **spending-limit control**
("Set a maximum amount you can spend per billing cycle to control costs.
When the limit is reached, API requests will be blocked until the next
billing cycle") — a direct, X-side mechanism to enforce the $90 ceiling
decision 5 and decision 6 already require, worth wiring into the launch
review packet's checklist under §6 rather than relying only on
`crates/realorrug-provider`'s own meter.

## Draft request message (not sent, not a form — no such form was found)

Because no submission form or dedicated-contact address was found on any X
page today, and X-staff forum answers state twice that none exists, there is
no confirmed channel to send this to. If, closer to launch, X reinstates a
real process (a form appears in the developer portal, or a dedicated point
of contact is assigned), this is the message to adapt — written only from
what was verified above, inventing no contact address:

> Subject: Prior written approval — AI-powered automated reply bot
> (mention-triggered)
>
> We operate an automated X account that replies only when a user mentions
> it. It does not search the timeline, does not reply based on keywords
> alone, and does not initiate contact with any user who has not first
> mentioned or replied to the account — each reply is a direct response to
> that mention, sent once per interaction.
>
> Every reply is generated from on-chain facts about the Solana token the
> user asked about (holder distribution, liquidity, contract checks,
> wallet-cluster analysis) read from the chain at reply time, not from
> memory or a cached opinion. The account states no price target, no buy/
> sell recommendation, and no probability of outcome — it reports what the
> chain shows and nothing else.
>
> The account carries X's "Automated" label and is linked to the
> maintaining human-run account. It honors opt-outs immediately, does not
> retry after a terminal failure, and is rate-limited by a fixed monthly
> API-spend ceiling enforced by our own spend meter in addition to X's
> account-level controls.
>
> We are requesting the prior written and explicit approval your
> AI-Powered Automated Replies policy requires before enabling automated
> replies. Please let us know the correct channel for this request, or
> confirm whether a bot meeting the above description (mention-triggered,
> one reply per interaction, disclosed, no unsolicited outreach) requires
> approval beyond compliance with the published Automation Rules.

If X's current staff-forum position (§1) still holds when this is actually
needed, the honest reading is that this message may get the same answer
seen twice today: point back to the automation rules and say no separate
approval is required if the account already meets them in software. That
does not remove ADR 0039 decision 4's gate — it changes what satisfies it:
demonstrating the mention-only, one-reply, labeled, opt-out-honoring design
is built and enforced in code (which design 0024 already targets) may be
the approval, in the absence of any other channel X currently offers.

## Not established

- Whether the "no extra written approval needed" forum guidance still holds
  on the day realorrug actually flips automated replies on — it is staff
  guidance in a forum thread, not a policy-page statement, and could be
  contradicted by a future staff answer or a policy rewrite.
- Whether the Automated-account label is available to toggle for this
  account today, or still gated behind help.x.com's stated "test group"
  invitation.
- Whether the first-crypto-post auto-lock (§2) has fully shipped as of
  2026-09-26, and whether it would catch a manual launch-announcement post
  from an account with no prior crypto history.
- Whether a summoned reply containing a URL is actually billed at $0.010
  rather than $0.200 — the pricing page's table does not spell this out
  directly; only secondary sources read today claim it.

## Sources, with URLs and dates read (all 2026-09-26 unless noted)

- <https://help.x.com/en/rules-and-policies/x-automation> — "X's automation
  development rules," updated April 2026, fetched directly via browser
  today (no Cloudflare block encountered this time).
- <https://help.x.com/en/using-x/automated-account-labels> — "About
  Automated account labels."
- <https://devcommunity.x.com/t/how-do-i-get-ai-bot-account-approval/264768> —
  posted May 6 2026.
- <https://devcommunity.x.com/t/request-for-written-approval-for-an-ai-powered-automated-reply-bot/276526> —
  posted/closed ~2026-09-23 ("4 days ago" / "3 days ago" as of today's read).
- <https://devcommunity.x.com/t/ai-powered-automated-reply-bot-approval-request/274245> —
  posted and closed Aug 27 2026.
- <https://docs.x.com/developer-terms/agreement> — last updated April 27
  2026.
- <https://docs.x.com/developer-terms/policy> — checked via fetch tool
  today for crypto/token clauses (none found), corroborating research
  0051 §1's 2026-09-18 read of the same page.
- <https://business.x.com/en/help/ads-policies/ads-content-policies/financial-services> —
  "Financial products and services" (Ads content policy).
- <https://docs.x.com/x-api/getting-started/pricing> — "X API pay-per-usage
  pricing and credits," fetched directly today.
- <https://www.ccn.com/news/crypto/post-crypto-get-locked-x-rolls-out-aggressive-anti-scam-measure/> —
  published 2026-04-03, secondary coverage of Nikita Bier's own X posts
  (Bier's original posts on x.com could not be fetched directly, HTTP 402,
  matching research 0051 §3's finding for x.com post fetches).
- Web-search snippets only, not independently fetched, used to locate the
  above and to corroborate the "summoned + URL" pricing reading: opentweet.io
  and vorplabs.com, both accessed via search 2026-09-26 (flagged UNVERIFIED
  at primary tier in §4).

**Confidence: CONDITIONAL.** The written-approval question is answered as
well as it can be from outside X (CHECKED for what the policy text says;
OBSERVED, twice, for what X staff say it actually means in practice today).
This is conditional on that staff position holding until launch — a single
forum reply is not a policy commitment, and the correct discipline stated
in ADR 0039 decision 4 stands: re-read `help.x.com/en/rules-and-policies/x-automation`
and search devcommunity.x.com for any newer staff answer on the day
automated replies are actually enabled, not from this document's date.
A second search today could not add more than this — the primary page has
not moved since April 2026, and every developer forum thread asking "where
is the form" gets the identical staff answer; that repetition, not a new
search, is what would change first.
