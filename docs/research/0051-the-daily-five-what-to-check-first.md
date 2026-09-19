<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0051 — the daily five: what to check first

**Date:** 2026-09-18
**Status:** live-doc-read research. CHECKED where a primary X/xAI/Defiant page
was fetched today and quoted; UNVERIFIED where the primary page blocked the
fetch (Cloudflare challenge or paywall) and the finding rests on a secondary
source quoting or summarizing that primary page. No paid API key was used and
no sign-up was performed (out of this packet's boundary); nothing below was
exercised live against the X API.

## 1. X sign-in: cost, scopes, storage and deletion

**Sign-in cost.** The X API pay-per-use price list
(<https://docs.x.com/x-api/getting-started/pricing>, read 2026-09-18) prices
reads and writes per resource; it does not list a price for the OAuth 2.0
Authorization Code + PKCE handshake itself. CHECKED that reads/writes are
priced; UNVERIFIED-by-omission that sign-in has no separate line item (the
page simply has no such line — absence is not proof, per AGENTS rule 3.8, so
budget for at least one `/2/users/me` read per sign-in, below).

**Minimum scopes.** X's own OAuth 2.0 PKCE guide
(<https://docs.x.com/fundamentals/authentication/oauth-2-0/authorization-code>,
read 2026-09-18 via search cache; direct fetch not attempted separately from
the pricing/scopes pages already in evidence) shows an authorization request
built with `scope=tweet.read users.read`. `/2/users/me` needs user context
(OAuth 2.0 PKCE, not app-only bearer) — confirmed by X developer-community
discussion of the endpoint requiring user-context auth (devcommunity.x.com
thread "Accessing 2/users/me through oauth 2.0 returns 403", read 2026-09-18,
secondary/community source, UNVERIFIED as policy but consistent with X's own
docs). So `users.read` + `tweet.read` is the minimum pair in practice; the
game only needs the handle and id, so `tweet.read` is arguably unnecessary
scope creep unless X requires it to issue a token that resolves `/2/users/me`
— CHECKED that both scopes appear together in X's example; NOT VERIFIED
today whether `users.read` alone suffices, because that requires an
exercised token this packet's boundary excludes.

**Per-call cost of reading the user.** X's pricing page states user reads at
**"$0.010 per resource"** (docs.x.com/x-api/getting-started/pricing, read
2026-09-18). `/2/users/me` returns one user resource, so each sign-in that
calls it to learn handle+id costs about $0.01, distinct from and in addition
to whatever the OAuth handshake itself costs (nothing, per the page's
silence on it). CHECKED for the $0.01/user-read rate; CONDITIONAL that
`/2/users/me` is billed at the generic "user reads" rate rather than a
separate free "identity" tier, since the page does not name that endpoint
specifically.

**Post/read rates for context (same page, CHECKED 2026-09-18):**
- Post read: "$0.005 per resource"
- Post created: "$0.015 per request"
- Post with a URL: "$0.20 per request" (see §4 below — this is the figure
  the plan needs, not the "20x" ratio computed from it)

**Storage and deletion (X Developer Agreement and Policy,
<https://docs.x.com/developer-terms/policy>, read 2026-09-18, CHECKED for
the quotes below).** The policy's clearest deletion duty is content-sync,
not account-deletion-triggered erasure: *"delete or modify any content you
have if it is deleted or modified on X. This must be done as soon as
reasonably possible, or within 24 hours after receiving a request."* That
24-hour clock is framed around X-side content changes (a deleted post, a
suspended account whose content X removes), not explicitly around a player
unlinking or revoking our app. The policy also caps bulk redistribution of
identifiers: developers may *"only distribute Post IDs, Direct Message IDs,
and/or User IDs"* to third parties, capped at "1,500,000 Post IDs per entity
per 30 days" — not directly about local storage, but it confirms user IDs
are the kind of thing the policy treats as sensitive-but-storable content.

I could not find, on this pass, a clause in the fetched policy text that
explicitly says "you must delete a user's stored ID/handle/history when that
user deletes their X account or revokes your app's access." **This is a
gap, not a clean answer** — mark it NOT VERIFIED rather than assume either
"we may keep everything" or "we must purge everything." The safe, defensible
reading consistent with the 24-hour content-sync clause and the general
spirit of the policy (data flows from X, and stops flowing when X-side
consent stops) is: **the past *calls* (settled facts of who guessed what,
already resolved against the chain) are the site's own contest record, not
"X Content," and are likely fine to keep for scoring history; the *handle
display* tied to a revoked/deleted account should stop being shown/linked,
and any live authorization token must be discarded immediately (revocation
invalidates it anyway).** This is a reading, not a citation — flag it for
the lawyer already reviewing the project (ADR 0023 decision 5) before
building the "keep past calls after revoke" behavior into G4/G5.

## 2. Automated mentions of opted-in users

Direct fetch of X's automation-rules page
(<https://help.x.com/en/rules-and-policies/x-automation>) returned a
Cloudflare interactive challenge today (2026-09-18) and could not be read
directly; likewise the devcommunity policy-clarification thread returned
403. The quotes below come from a web search whose result snippets quote
that same help.x.com page's text verbatim (matches wording widely mirrored
across secondary automation-rules summaries, e.g. socialrails.com,
opentweet.io, read 2026-09-18) — **UNVERIFIED at primary-source tier**,
CHECKED only in the sense that multiple independent secondary sources agree
word-for-word:

> "Automating reply and mention actions to reach many users on an
> unsolicited basis is an abuse of the feature and is not permitted."

> "You may send automated replies or mentions to X users so long as in
> advance of sending the automated reply, the recipient or mentioned
> user(s) have requested or have clearly indicated an intent on X to be
> contacted."

Best practices quoted alongside: keep automated mentions "low-volume and
high-value," never mention the same user twice for one opt-in, explain what
the user is opting into, and honor opt-outs immediately.

**Reading:** a player who ticks "tag me" on our own site has "clearly
indicated an intent... to be contacted," which the rule treats as the
condition that makes an automated mention permitted. The design already in
the plan (tag only if a player opted in; otherwise counts only, §"The
posts") matches this rule's shape closely. This is a reading of
secondary-quoted text, not a primary-source confirmation — re-fetch
help.x.com directly (past the Cloudflare gate, e.g. from a browser session)
before shipping G7, since a policy page rewrite between now and launch would
not be caught by this pass.

## 3. X's ban on paying for posting ("InfoFi")

Primary-ish source: X's head of product, Nikita Bier, posting the policy
change on X itself
(<https://x.com/nikitabier/status/2011825522817270230>) — direct fetch
returned HTTP 402 today (2026-09-18) because x.com posts are gated behind
sign-in for fetch tools; the quote below is reproduced by multiple
2026-01-16-dated secondary reports (Unchained, TechCrunch-adjacent crypto
press, UEEx, Tekedia, Bitget, all read 2026-09-18) that all cite the same
post text, so this is **UNVERIFIED at primary tier, well-corroborated at
secondary tier**:

> "We are revising our developer API policies: We will no longer allow apps
> that reward users for posting on X (aka 'infofi'). This has led to a
> tremendous amount of AI slop & reply spam on the platform. We have
> revoked API access from these apps..."

Reported date: 2026-01-16 (the announcement post), with coverage often
rounding to "2026-01-15" — the plan's date is a one-day rounding of the
same event, not a conflict worth treating as a disagreement.

**Reading, marked as a reading:** the rule targets apps that "reward users
for posting" — payment or points contingent on the act of posting/engaging.
The daily-five game's prize is contingent on prediction *accuracy*, settled
by the chain, and specifically **requires no post at all** to earn or lose
points; posting about a result is optional and unrewarded (per the plan's
own rule 2, "Mentions stay free and earn nothing" — decision 4 in ADR 0034).
On its face this falls outside "reward users for posting." The residual
risk is optics/enforcement discretion: a crypto-adjacent prize tied to an X
sign-in, publicized via X posts that "callers reshare," could still draw
scrutiny even though the reward is not conditioned on the post. This is my
reading, not X's — nothing found today states an exception for
non-posting-conditioned prediction games.

## 4. Post-with-link vs. without: pricing

X's pay-per-use pricing page (docs.x.com/x-api/getting-started/pricing, read
2026-09-18, CHECKED) prices a plain created post at **"$0.015 per request"**
and a post containing a URL at **"$0.20 per request."** $0.20 / $0.015 ≈
**13.3x**, not "about twenty times." **This corrects the plan's figure**:
the ratio is roughly 13x on today's published rates, not ~20x. (If the plan's
"~20x" was computed against an older or rounded base rate — e.g. treating
plain posts at $0.01 rather than $0.015 — that would produce 20x; today's
page shows $0.015, so the 13x figure is what today's numbers support.) The
underlying operational conclusion the plan draws from this — no links in
posts or replies, keep the link in the bio — holds regardless of whether the
multiplier is 13x or 20x; only the stated multiplier needs correcting.

## 5. Pons v2 share of launchpad revenue

Primary source: The Defiant,
<https://thedefiant.io/news/blockchains/robinhood-chain-dex-volume-hits-usd1-49-billion-as-pons-takes-two-thirds-of-launchpad-fees>,
dated 2026-09-01, read 2026-09-18. CHECKED, quoted directly:

> "Pons collected $4.89 million in fees on Aug. 31, the most recent full
> day, against $1.72 million for pump.fun. That gave Pons 63.9% of the
> $7.65 million paid to launchpads across crypto that day."

This is **63.9% (~two-thirds) of fees paid to launchpads across all of
crypto on one specific day (2026-08-31), not a multi-week average and not
scoped to "across chains" the way ADR 0034 decision 6 phrases its 10%
two-weeks-running trigger.** It is a single-day cross-sectional snapshot,
not the rolling measure decision 6 needs. The article's companion piece
("Pons Is Out-Earning Pump.fun Almost Three To One," same outlet, same
read date) suggests the lead has held for multiple consecutive days ("every
day since Aug. 29") but that is a narrower streak (three days as of the
article) than "two weeks running." **Correction to record:** the plan's
"about two-thirds... citing The Defiant" is directionally right (63.9% ≈
two-thirds) but the measurement is a one-day fee snapshot across all
launchpads in crypto, not an "across chains" launchpad-revenue share over
the two-week window decision 6 actually requires — decision 6's trigger
still needs its own rolling measurement built, not a citation of this
snapshot. This also confirms the earlier plan's 28% figure is stale/wrong,
per today's number.

## 6. "@grok is this true" pays nobody

No primary xAI/X policy page found today stating a specific "no reward for
Grok summons" rule, because none is needed: Grok-on-X is a product feature
(ask Grok a question by tagging it in a reply), not a paid-participation
program. Secondary sources read 2026-09-18 confirm the shape: xAI's
developer-facing reward program is a *spend* rebate for X API credit
purchases ("earn up to 20% back in xAI credits based on cumulative spend,"
per X API pricing summaries, distinct from summon activity), and no source
found today describes any payment, credit, or reward triggered by a person
typing "@grok is this true." This is **OBSERVED-by-absence**: I looked for
a reward program tied to Grok summons and found none, which is the claim
the plan needs, but absence of a found program is not the same as a
document stating "there is no reward" (AGENTS rule 3.8: absent is not
zero). Confidence here rests on the negative search, not a quoted denial.

## Measured later, by replay

This section names what the replay must produce, not numbers — the replay
itself is blocked (see below).

- **q per bot level.** For each level the bot's scoring code can output
  (`Rugged`, `RugMechanicsLive`, `Sketchy`, `NothingUglyYet`, `CantTell`),
  the replay must compute the measured share of past launches at that level,
  using only chain facts that were visible at the moment of listing — never
  a fact learned later. This is what turns a level into the q the odds
  formula (1-q / q payouts) needs.
- **How fast rugs land.** The replay must produce the distribution of
  time-to-rug across past launches that did rug (share landing inside 1
  day, inside 3 days, inside 7 days, inside 14 days). This sets the calling
  window: if most rugs resolve inside a short window, the window should be
  that short, not the longer default — a game that answers fast holds
  players.
- **The four dummy players.** Always-call-rug, always-call-real, coin-flip,
  and agree-with-the-bot must each be scored against the replayed odds
  table. The rule the replay must satisfy: none of the four should average
  above zero by more than noise. Any fixed strategy that scores above zero
  on replay is proof the odds table is too rough (too generous to one side)
  and must be fixed before a real player plays.
- **Blocked on:** design 0027 slice 8 (outcome calibration — the code that
  decides when a launch's outcome is settled enough to grade against) and
  PR #117 (held for Josh's yes on "launch-block pair counts once"; the odds
  table and this replay both wait on it, per the housekeeping note in the
  plan).

## Summary of corrections and gaps against the plan

1. Sign-in itself is not separately priced by X, but `/2/users/me` costs
   about $0.01/read on today's page — a real, small, recurring cost per new
   player, not the zero the plan implicitly assumes.
2. Deletion-on-revoke/deletion-on-account-delete is **not explicitly
   answered** in the fetched X Developer Policy text — a gap, not a yes or
   no, and it should go to the lawyer before G4 stores any call history.
3. The post-with-link-vs-without ratio is about **13x on today's numbers**
   ($0.20 vs $0.015), not "about twenty times" — the plan's number should be
   corrected, though the conclusion (no links in posts) is unaffected.
4. The Pons "two-thirds" figure is confirmed (63.9%, The Defiant,
   2026-09-01) but it is a **one-day snapshot across all crypto
   launchpads**, not the rolling two-week "launchpad revenue" measure ADR
   0034 decision 6 needs — decision 6 still needs its own tracked metric.
5. Automated-mention and InfoFi-ban findings are directionally
   confirmatory of the plan (opt-in tagging looks compliant; a
   non-posting-conditioned prize looks outside the ban) but both rest on
   secondary sources today because the primary X pages returned a
   Cloudflare challenge / paywall — re-check both directly before G7 ships.
