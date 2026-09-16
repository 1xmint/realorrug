<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0025 — the website

**Status:** recording, not yet built. Nothing in this document has been
built, bought or launched.
**Date:** 2026-09-16.
**Depends on:** design 0023 (the public checker page — token lookup, its
three states, its cache and its share text) and design 0020/ADR 0027 (the
fact sheet and the verdict ladder the checker's words come from). This
document does not restate any of 0023's decisions; where it touches the
checker page it cites 0023 by section and defers to it. It also does not
re-decide anything ADR 0013 already settled (rule 6, the operator holds none
of the token) or AGENTS.md §3 (rule 4, rule 5) — it states what those rules
mean for a page layout and moves on.
**Scope:** the whole public site — page list, URLs, per-page layout and data
source, art direction, the per-token share image (as a requirement, not a
renderer), performance budget, what must never appear, the art assets to
produce, and a build order. Not the checker's own behaviour (0023 owns that),
not the renderer for the share image (a separate research thread is running
on that; §7 states the requirement only), not `crates/realorrug-serve`'s
internal code shape beyond the routes a page needs to exist.

## 0. Why this document exists

The owner's words: today's site "looks like a low effort html page" and he
wants it "planned properly, super professional, memecoin-grade art." That is
a direction question (AGENTS.md §2) — stop, have the conversation, write it
down — not a ticket to reskin `site/src/index.css` on sight.

**What is actually wrong, read from the code itself, not assumed:**

- The site is still branded for a different product. `site/src/App.tsx`
  hardcodes the wordmark `Cabal<span>Hunter</span>` in the sticky header.
  `site/src/Home.tsx:110` reads `Solana · pump.fun · measured since August`
  as the hero's dateline; `site/src/Home.tsx:132` and `:263`, and
  `site/src/About.tsx:28,39,66,112,132`, all say "Cabal Hunter", "pump.fun"
  or "Solana" in prose a stranger reads as the current description of the
  product. `site/src/Token.tsx:108` explains tokenomics as "pump.fun charges
  a fee on the trade." None of this is Robinhood Chain, the chain design
  0019 already moved the analyst to, and none of it is the "realorrug"
  identity the domain and this design's own filename assume. This is not a
  cosmetic typo; a stranger who lands here and then checks the bot's actual
  X profile sees two different products.
- Two pages never resolve. `site/src/Leaderboard.tsx:320` and
  `site/src/Pool.tsx:191` each render a bare `Reading…` and nothing else on
  the branch where their data has not arrived — no timeout fallback to
  `site/src/fixtures/stats.json` the way `site/src/api.ts`'s own doc
  ("older but true, never blank and never invented") promises for `stats()`.
  Reading `site/src/api.ts`'s `leaderboard()` and `pool()` functions: both
  already return an honest empty shape (`{ entries: [], ... }` /
  `{ vault: null, ... }`) when the fetch fails, so the page-level `Reading…`
  branch is reachable only during the request itself — but a request against
  an endpoint route that exists (`/v1/public/leaderboard`, `/v1/public/pool`
  in `crates/realorrug-serve/src/lib.rs`) yet returns a shape the component
  does not expect, or hangs past `TIMEOUT_MS` without the catch firing the
  way the component assumes, reads on screen as stuck forever — the same
  failure mode this repository has already named once, `Analyst.tsx` sitting
  on "reading…" because an empty error string is falsy
  (`site/src/api.ts`'s own module doc, §"The fallback is a real answer").
  This document does not fix that bug (it is a `site/` code change, out of
  this document's DOCS ONLY scope) — it is named here because a rebuild that
  reuses the current `Leaderboard`/`Pool` fetch pattern without noticing
  would ship the noir redesign with the same stuck spinner underneath it,
  and because the build order in §9 puts the fix ahead of the reskin for
  that reason. Full root cause is a `site/` investigation, not decided here
  (§10).
- Mobile nav: `site/src/App.tsx`'s header renders six links
  (`Leaderboard`, `Pool`, `History`, `Token`, `About`, plus `Home` excluded
  by the `r.path !== "/"` filter — five visible) in a single-row flex
  container with no wrap and no overflow handling
  (`flex items-center gap-0.5 ... sm:gap-1`). `routes.ts`'s own comment
  already documents the 375px width as "the width most of this traffic
  arrives at" and names the same six-item budget as the reason `Pool`
  carries a `short` label instead of "Prize pool" — so the row was already
  tuned once for width and is still tight. `About` is last in the row and
  has no `short` override, so it is the first candidate to run out of room
  or wrap onto a second line the sticky header was not sized for; this
  document treats "About" clipping as credible given the code, not as an
  independently re-measured fact (§10).

None of the above is disputed by this document; all of it is read from the
files named. What follows is the plan: not "make it prettier" but a specific
page list, the data each page actually has (from `realorrug-serve` and the
contest/payout crates, §1–§2), an art direction that uses the owner's
existing X brand assets rather than inventing a new one, and an order to
build it in that fixes the two named bugs before it reskins over them.

## 1. What `realorrug-serve` can answer today

From `crates/realorrug-serve/src/lib.rs`'s router, verbatim:

| Route | Handler | What it reads |
|---|---|---|
| `/health` | `health` | version, build SHA |
| `/v1/public/stats` | `public::stats` | population stats (design 0023 §0 quotes the crate doc: "population stats") |
| `/v1/public/leaderboard` | `public::leaderboard` | the open week's entries |
| `/v1/public/pool` | `public::pool` | the prize pool balance and past winners |
| `/v1/public/weeks` | `public::weeks` | every closed week, with claim and payout state |
| `/v1/public/hunters` | `public::hunters` | (routed; not read by any current page — see §10) |

Every handler "reads a published file, never the store"
(`crates/realorrug-serve/src/public.rs`'s own doc, quoted in design 0023
§0) — this is a read-only mirror of documents another process already
computed, with no live chain read and no dependency on
`realorrug-onchain`/`realorrug-provider`/`realorrug-robinhood`/
`realorrug-pumpfun` (design 0023 §0, same citations, still true — this
document adds no route and takes no dependency this crate does not already
have). The one thing this API set **cannot** answer is "what does this
specific pasted token look like" — that is exactly the gap design 0023's new
route closes, and it is not re-litigated here.

**What the site's own types already model**, from `site/src/api.ts` (types
this document reuses rather than redefining):

- `Leaderboard.entries: Entry[]` — the open week's ranked participants, each
  with `raw` (platform-reported engagement) and `verified` (what survived
  the scan — distinct accounts, age-floored). `Entry.score` is `null` before
  the scan reaches that entry, never `0` standing in for it.
- `Pool.winners: Winner[]` — past winners with `lamports` and a
  `signature` — a winner is not "paid" here, only recorded as having won.
- `Weeks.weeks: Week[]` — every closed week, each with its own `claim`
  (`claimed` / `open` / `rolled_over` / `no_winner`) and `payout`
  (`paid` / `owed` / `unclaimed` / `awaiting_claim` / `voided` / `no_winner`)
  state, and `payout.signature` when paid.

## 2. What the contest and payout crates say "participants", "rewards
   ranking" and "payout history" actually mean

Stated precisely, from the code that defines them, because the owner's
phrase and the product's actual shape are not guaranteed to be the same
thing and this document must not guess:

- **"Participants"** are not "everyone who holds the token" or "everyone who
  follows the account." `crates/realorrug-contest/src/lib.rs`'s module doc:
  "every summoned reply is an entry" — a participant is someone who
  `@`-mentioned the bot asking about a token, got a reply, and that reply is
  what the week scores. The site's `Entry` type (`site/src/api.ts`) is this
  same thing, one per summoner. There is no notion of a participant who
  never summoned the bot.
- **"Rewards ranking"** is not a token-holder leaderboard or a trading
  leaderboard. It is `realorrug-contest`'s scoring rule "over the bot's own
  replies' public metrics" (same module doc), producing the
  `ledger::Record` the site's `Leaderboard`/`Entry` types mirror — ranked by
  `verified` engagement (distinct, age-floored accounts), with `raw`
  (platform-reported, farmable) shown as evidence beside it, never as the
  rank itself (`Entry.raw`'s own doc: "the number one account can farm to
  the top for nothing"). Design 0007 §6.2, cited in the crate doc, is
  explicit that gaming is made *visible* by this shape, not made
  impossible — a rewards-ranking page must carry that same honesty, not
  present the rank as an unimpeachable score.
- **"Payout history"** is `crates/realorrug-payout`'s ledger of what
  actually moved: per its module doc, a week's payout is **two on-chain
  transactions** from the payout wallet — `claim(amount)` against the Pons
  v2 fee escrow (because on Robinhood Chain a sweep credits the escrow, it
  does not pay the creator directly), then a plain ETH transfer to the
  claimed address — and the ledger is written only after both are read back
  from the chain (module doc, "read both back, and only then write the
  ledger"). The site's `Week.payout` and `Payout.signature` fields
  (`site/src/api.ts`) are this ledger's public face: one signature per paid
  week, and `Payout.state` covering the four ways a week can be *not* paid
  (`owed`, `unclaimed`, `awaiting_claim`, `voided`) — each a different fact,
  never merged. A payout-history page listing "every payout linked to its
  chain transaction" (the lead's proposal, evaluated in §4) is this
  `Payout.signature` field per closed week, nothing invented beyond it.

## 3. Owner's brand art, and what it commits this site to

**These two files are now in the worktree** — `site/public/brand/logo.png`
(1254×1254) and `site/public/brand/banner.png` (2048×768), viewed directly
for this document. **The owner's own framing, stated 2026-09-16: they are
AI-generated and not binding** — "we can make the website however we want,
as long as its beautiful." So the rest of this section, and §5, treat them
as a strong starting point to react to, not a spec to trace. Recommendation,
not recording (AGENTS.md §2): the two are close enough to "beautiful and
on-brand for a viral memecoin analyst" that redoing them from scratch would
cost real time for a marginal gain — §3a below is the recommended direction
(use, adapted) and §3b is one genuinely different alternative (replace) for
the owner to react to, per instruction.

Described in prose, at the resolutions above, not linked as repo paths this
conformance check would need to resolve:

- **Profile picture**: a round badge, gold rim, the background split
  green (left) / red (right); a glossy green coin with a rising arrow on
  the left, a cracked dark-red coin with a falling arrow on the right;
  huge 3D graffiti type — "REAL" in neon green, "OR" in yellow brush,
  "RUG" in torn red.
- **Banner**: a dark noir detective evidence board — pinned sepia photos
  and notes, red string converging on a gold coin, a desk lamp, a
  magnifying glass.

Image files land in `site/public/brand/logo.png` and
`site/public/brand/banner.png`, supplied by the owner later; this document
references them by that path in prose only, and does not add either path to
any conformance-checked link list (`repo-conformance` checks named paths
that exist in the tree at check time — a not-yet-supplied asset referenced
only in prose, the way this paragraph does it, is not a claim the checker
can or should verify).

**What that commits the site to, read literally rather than loosely:**

- Two colour stories are already fixed by the badge, not invented here:
  **green = good / real, red = bad / rug**, plus **gold** as the badge's
  rim and the banner's coin — a verdict-colour vocabulary already exists
  before this document names hex values in §5.
  The current site's single-accent amber
  (`site/src/index.css`, `--color-signal`) was chosen specifically *because*
  "everything this site says is a warning" and green reads as "go" on every
  other crypto page (the CSS file's own comment) — that reasoning was sound
  for a page with one signal colour and no brand badge to match. It stops
  being the constraint once the badge itself uses green for "real" and red
  for "rug" as its central graphic device; matching the badge is worth more
  than avoiding a colour crypto sites overuse elsewhere, and a five-level
  verdict ladder (§5's palette) needs more than one hue to keep `Rugged` and
  `Sketchy` from reading as the same colour at different opacities. The
  amber is not dropped; it becomes the ladder's true-middle colour
  (`CantTell`), where "warning, undecided" is exactly the right word for it.
- The banner's world — noir detective, evidence board, pinned photos, red
  string, a magnifying glass — is the site's **atmosphere and texture**,
  not its every page's foreground. AGENTS.md §3 rule 4 is exact: a verdict
  "never accuses a person" and the model "may describe a token or its
  launch, never call a named person, account or company a scammer or a
  thief." The banner's own photos are crossed-out portraits — that is fine
  as marketing art board decoration where no specific token or account is
  named, but it must never appear **beside** a real token's verdict, because
  a crossed-out photo next to a live verdict card reads as "this is the
  person," a claim the product is constitutionally forbidden from making.
  §5 and §6 draw this line by page: atmosphere (torn paper, string,
  corkboard texture, an empty evidence-tag frame) is allowed everywhere; a
  human photograph, even a stock or illustrated one, is allowed nowhere near
  a verdict.

  Looking at the actual banner file confirms this is not a hypothetical:
  it already contains two sepia portrait photos with a hand-drawn red X
  over each face, mixed among other pinned material (screenshots, a car, a
  house, icons for a bank/globe/database). Those two crossed-out faces are
  exactly what must never sit beside a real token's verdict (§6) — they are
  usable as background texture cropped away from any live verdict card, and
  unusable as a foreground element on `/check/:address` itself.

### 3a. Recommended direction — use the badge, adapt the banner (not replace)

The badge (logo.png) is the stronger of the two assets and needs no
adaptation: its green/gold/red is already a coherent verdict vocabulary
(§5), its 3D graffiti wordmark is distinctive enough to be recognized at
favicon size, and it reads as "memecoin" without reading as "cheap" — glossy
coin renders and bevelled metallic type are the actual current visual
language of high-production memecoin brands (compare any top-20 memecoin's
X profile picture), not a dated style to move away from. **Use it directly**
as the header wordmark and favicon source (§9 asset list), unmodified.

The banner is the one that needs adapting rather than using whole: at full
size, on a page whose job is a stranger deciding in four seconds whether to
paste an address, a dense evidence board with 20+ pinned items competes with
the paste box for attention and takes real bytes to load (§ Performance
budget). **Adapt it**: crop and extract its *texture* — the corkboard
grain, the warm desk-lamp lighting gradient, the red-string line quality —
as the tileable background and the one-use hero accent §5 already specifies,
with every pinned photo (including the two crossed-out faces) cropped out
entirely rather than merely small. The full banner remains useful as-is for
things that are not the live product surface: an X header/banner image
(where it already lives), a loading-state or 404-page illustration, or a
`/how-it-works` page hero where "case file" is the whole point and no real
token's verdict is anywhere on the page.

Rejected alternative to this recommendation: **replace both assets
entirely** with new AI-generated or commissioned art. Rejected as the
primary recommendation because the owner explicitly said he "keeps" this
brand art in the same message that set this document's scope, and because
the badge in particular already does its job well — replacing a working
recognizable mark the account has presumably already posted under has a
real cost (a stranger who knows the account by its current PFP would see a
different mark) that this document has no evidence is worth paying. It is
offered instead as the deliberately different alternative below, since the
owner asked for one to react to.

### 3b. Alternative direction — replace with a flatter, chart-native brand

A genuinely different direction: drop the photoreal coin/graffiti-type badge
and the photoreal evidence-board banner in favor of a **flat, geometric
mark** — a single bold wordmark in the display face (§5's Space Grotesk,
not a bespoke graffiti render) inside a simple two-tone circle (green
left / red right, no coin illustration, no gold bevel), and a banner that is
an abstracted candlestick/line-chart motif turning from green into red
across the width, no photos, no desk-lamp scene at all. This direction
trades "movie poster, illustrated, detective-noir" for "trading-terminal,
flat, data-native" — closer to how a serious analytics product (a Dune
dashboard, a DeFi risk tool) brands itself than how a meme coin's own token
page brands itself. It is a legitimate reading of "super professional" that
leans away from "memecoin-grade art" rather than toward it, which is exactly
why it is offered as the contrasting option rather than the recommendation:
the owner's own brief asked for *both* professional and memecoin-grade, and
§3a's photoreal/noir direction is judged the better fit for holding both at
once, but this flatter direction is the one to pick if "professional" should
win when the two pull apart. Under this alternative, §5's palette and
verdict-colour logic (green/red/gold/amber) is unchanged — only the
illustration style and the texture/motion treatment (no corkboard grain, no
red-string SVG, no noir atmosphere) would need to be redrawn to match.

## 4. Page list and URLs — decided, with the rejected alternative

**Decision: six pages, all top-level, replacing today's seven** (`/`,
`/leaderboard`, `/pool`, `/history`, `/token`, `/about`, plus the three
footer-only trust pages unchanged):

| Path | Replaces | Purpose |
|---|---|---|
| `/` | `/` | Lookup + live feed + contest teaser (§4a) |
| `/check/:address` | *(new — design 0023 §7 names this route, path decided here)* | Per-token verdict + share card |
| `/contest` | `/leaderboard` | Participants + rewards ranking, this week |
| `/payouts` | `/pool` + `/history` merged | Every closed week, its payout, its transaction |
| `/how-it-works` | *(new)* | Verdict ladder, what the bot never does |
| `/tokenomics` | `/token` | Unchanged path (owner said "maybe tokenomics" — kept as a page, not folded into home; see rejection below) |

Footer stays: `/privacy`, `/terms`, `/contact`, unchanged.

**Evaluating the lead's proposal against this, point by point:**

- *Home = the lookup, with hero art, big paste box, live feed of latest
  verdicts, copyable contract address, contest teaser* — **accepted**, this
  is `/` in §4a below. Rejected alternative: keep `/` as a static "about the
  bot" landing page and put the checker at its own `/check` path with no
  entry on the front page. Rejected because design 0023 §8 explicitly left
  "how a visitor is told about the page at all... whether it is the site's
  front page" undecided and deferred it to this document — a checker a
  stranger cannot find from the link in a reply is a feature nobody uses;
  the whole product's distribution model (per `App.tsx`'s footer: "the
  account posts... the reply is a link") is a link landing cold on a
  stranger who has four seconds, so the thing they came for must be the
  first thing on the page.
- *A shareable per-token page whose X link preview shows a verdict-stamp
  image* — **accepted**, this is `/check/:address`. This document states the
  requirement (§7) and leaves the renderer to the separate research thread
  the packet names, per instruction.
- *Contest page* — **accepted**, as `/contest`, but renamed from the lead's
  implied "Leaderboard." Rejected alternative: keep the URL and label
  `/leaderboard`. Rejected because §2 already establishes "leaderboard" as a
  ranking word and this page is showing **participants** (who summoned the
  bot) ranked by a **rewards rule**, which is a contest, not a scoreboard of
  token performance — a visitor arriving expecting a coin leaderboard (the
  common crypto-site meaning) would be misled by the label alone, and the
  owner's own phrase "current participants + rewards ranking" is the more
  accurate name for what the data actually is (§2).
- *Payouts page with every payout linked to its chain transaction* —
  **accepted**, as `/payouts`, merging today's separate `/pool` (current
  balance + past winners) and `/history` (closed weeks with claim/payout
  detail). Rejected alternative: keep them as two pages, matching today's
  site. Rejected because both pages read the same underlying fact — money
  that moved or is owed — from adjacent parts of the same `Week`/`Pool`
  shape (§1), and a visitor asking "did the bot actually pay out" (the
  owner's own framing, "payout history") has to check two URLs today to get
  one answer; one page, current pool balance at the top and the full paid
  history below it, is the smaller number of places the same fact lives.
- *How it works (verdict levels, what the bot never does)* — **accepted**,
  as `/how-it-works`, and given its own page rather than folded into
  `/about`. Rejected alternative: extend today's `/about` page in place.
  Rejected because `/about`'s current content
  (`site/src/About.tsx`) is about the *operator* — who runs it, what the fee
  split is — and "verdict levels, what the bot never does" is about the
  *product's rule*, ADR 0027's ladder and AGENTS.md §3's refusals; a
  stranger deciding whether to trust a verdict on `/check/:address` needs
  the rule page, not the operator's biography, and a single link from the
  checker to "how we decide this" reads better than a link to "who we are."
  `/about` keeps the operator content, unchanged in scope.
- *Tokenomics as a home section, not a page* (the lead's proposal) —
  **rejected**, kept as its own page at `/tokenomics`. Rejected the lead's
  proposal because the owner's own list of "pages he imagines" names
  "maybe tokenomics" as a page, not a section, and because home's real
  estate is already committed in §4a to the lookup, the live feed and the
  contest teaser — folding a fourth concern (ADR 0013's "operator holds
  none of the token" disclosure, which is `/token`'s central content today,
  per `site/src/Token.tsx`) onto the busiest page dilutes the page whose
  entire job is converting a first-four-seconds visitor into a token paste.
  A one-line link to `/tokenomics` from the home footer area is enough; the
  full page keeps its own room to make ADR 0013's disclosure properly, the
  way `site/src/Token.tsx` does today.

## 4a. `/` — Home

**Data:** `stats()` (population figures), the checker's own cold/warm/error
states (design 0023 §1), `leaderboard()` for the teaser strip (top 3
entries only), no new endpoint.

**Layout, top to bottom:**

1. Header (existing `App.tsx` shell, see §8 for the nav-clip fix).
2. Hero: wordmark badge (owner's PFP, small, top-left of the hero block),
   headline in the display face naming the product's one job in one
   sentence, the noir texture as a background treatment (not the banner
   image itself at full size — a repeating subtle paper/board texture
   derived from it, see §5), and the dateline updated to
   `Robinhood Chain · measured since <date>` replacing `Solana · pump.fun`
   (`Home.tsx:110`, §0).
3. **The paste box** — the single largest interactive element on the page:
   an input plus a "Check it" button, wired to design 0023's checker (its
   three states render inline below the box on submit, not on a separate
   page load — `/check/:address` in §4b is the *shareable, permanent* URL
   for a result, reached either by pasting here or by a shared link).
4. **Copyable contract address** — the product's own token address (ADR
   0013's token, the one the operator holds none of), a monospace string
   with a copy button, directly under the paste box because it is the
   second most common reason a visitor is on this page at all.
5. **Live feed of latest verdicts** — a short list (5–10) of the most
   recent checked tokens and their verdict level, each linking to its
   `/check/:address`. **Data gap:** no current endpoint returns "recently
   checked tokens" — `/v1/public/stats`, `/leaderboard`, `/pool`, `/weeks`,
   `/hunters` (§1) hold none of this. This needs a new
   `realorrug-serve` route reading the checker's own cache (design 0023
   §3's per-key verdict cache, keyed `(chain, address)`) as a recency-sorted
   list — the same store, a different read pattern, not a new source of
   truth. Named here as a requirement on `realorrug-serve`; the route
   itself is not designed in this document (§10).
6. **Contest teaser** — this week's top 3 from `leaderboard()`, a "See full
   contest →" link to `/contest`.
7. Footer (existing).

## 4b. `/check/:address` — Token page

**Data:** design 0023's verdict document in full (§1 of that document) —
ladder level, evidence bullets, read timestamp, share text — served by the
route that document's §7 names but does not path yet; this document names
the path (`/check/:address`) as the URL `site/src/routes.ts` should use.

**Layout, top to bottom:**

1. Header.
2. Verdict stamp — the headline ladder level in its verdict colour (§5),
   large, the way a case file stamps "CLOSED" or "OPEN" — this is the one
   place the noir metaphor is allowed to be literal rather than ambient,
   because it is decoration on the product's own words, not a claim about a
   person.
3. Token identity — name and symbol as inert text (design 0023 §6 — never
   markup, never a URL fragment built from the string), the address in
   monospace with a copy button.
4. Evidence list — design 0023 §1's bullets, verbatim from the fact sheet,
   unchanged from that document's shape.
5. Freshness line — "Read N minutes ago from Robinhood Chain" (design 0023
   §1), or the cold/error state from the same section when this URL is
   loaded directly for an address not yet cached — `/check/:address` must
   handle a fresh cold visit the same way the home paste box's inline
   result does, since a shared link is exactly a visitor who was never on
   `/` at all.
6. Share button — design 0023 §5's server-composed text, unchanged.
7. Footer.

**Mobile-first note for both `/` and `/check/:address`:** the paste box and
the verdict stamp are the first interactive/legible elements after the
header on a 375px viewport, full-width, no multi-column layout above
roughly 640px — this matches `routes.ts`'s own stated assumption that most
traffic arrives at 375px from a link in an X reply, unchanged from today's
site, not a new decision.

## 4c. `/contest` — Participants + rewards ranking

**Data:** `leaderboard()` unchanged (§1, §2) — no new endpoint.

**Layout, top to bottom:** week dateline and close countdown (existing
`Leaderboard.tsx` pattern, kept), ranked entry list (rank, summoner handle
or id, `verified` engagement as the displayed number, `raw` shown as a
secondary/muted figure per §2's "evidence, not the rank" rule, unchanged
from today), a short explainer line linking to `/how-it-works`'s scoring
section rather than re-explaining the rule on this page.

**This page must ship with the `Reading…` fix from §0**, not as a follow-up
— see §9.

## 4d. `/payouts` — Payout history

**Data:** `pool()` for the current balance and vault address, `weeks()` for
every closed week's `claim`/`payout` state and signature (§1, §2) — both
existing endpoints, merged onto one page, no new endpoint.

**Layout, top to bottom:** current pool balance and vault address (today's
`Pool.tsx` header content), then every closed week newest-first, each row
showing the week, the winner, the amount, and — **the lead's explicit
ask** — the payout's transaction signature as a link to a Robinhood Chain
explorer whenever `payout.state === "paid"`; the four not-yet-paid states
(`owed`, `unclaimed`, `awaiting_claim`, `voided`) render their own honest
sentence per `site/src/api.ts`'s `Payout.state` doc, never a blank row.

**This page must ship with the `Reading…` fix from §0** as well — see §9.

## 4e. `/how-it-works`

**Data:** none from `realorrug-serve` — this page is prose plus a static
rendering of ADR 0027's five-level ladder (names and one sentence each,
quoting AGENTS.md §3 rule 4's own list: `Rugged`, `RugMechanicsLive`,
`Sketchy`, `NothingUglyYet`, `CantTell`) and a short "what this account
never does" list drawn directly from AGENTS.md §3 (never states price or
market cap — rule 5; never names a person — rule 4; model judgement never
moves money — rule 1). This page is the trust anchor the checker's evidence
bullets link back to; it carries no live data and no new endpoint.

## 4f. `/tokenomics`

Unchanged in scope from today's `/token` (`site/src/Token.tsx`) — ADR
0013's disclosure that the operator holds none of the token, and the fee
mechanics. URL renamed from `/token` to `/tokenomics` to match the owner's
own word for it and to read unambiguously in the nav (`/token` next to
`/check/:address` was one letter from confusing).

## 5. Art direction

**Palette** — extending, not replacing, `site/src/index.css`'s existing
OKLCH system (same colour space, same measured-contrast discipline the
file's own comment already commits to):

| Role | Value | Source / reasoning |
|---|---|---|
| `--color-ink` (ground) | `oklch(0.15 0.012 265)` unchanged | Keep — already near-black, already reads as noir without changing it. |
| `--color-surface` / `--color-raised` | unchanged | Same. |
| `--color-line` / `--color-edge` | unchanged | Same. |
| `--color-text` / `--color-dim` / `--color-faint` | unchanged | Same. |
| **`--color-good` (verdict: NothingUglyYet)** | `oklch(0.72 0.16 148)` / hex `#3ecf6e` | The badge's glossy left-side green, slightly desaturated for text-on-dark legibility. Replaces today's `--color-good` at `oklch(0.76 0.13 158)` — close enough in lightness to keep existing contrast ratios, shifted in hue to match the badge's actual green rather than a generic mint. |
| **`--color-warn` (verdict: Sketchy)** | `oklch(0.80 0.15 78)` / hex `#e2a63f` (today's amber, kept) | Today's single `--color-signal` becomes the ladder's *middle* colour, not its only one — see §3. |
| **`--color-danger` (verdict: Rugged / RugMechanicsLive)** | `oklch(0.58 0.19 27)` / hex `#c23b2c` | The badge's cracked dark-red coin. `RugMechanicsLive` and `Rugged` share this hue; `RugMechanicsLive` at full value, `Rugged` at a darker/heavier weight (font-weight, not colour drift) so the two are told apart by emphasis, not by two reds a colour-blind reader can't separate. |
| **`--color-unknown` (verdict: CantTell)** | `oklch(0.56 0.014 265)` (today's `--color-faint`, reused) | Deliberately the least visually confident colour on the ladder — `CantTell` is "we don't know," not a fourth warning colour competing for attention (design 0023 §1: "deliberately the least reassuring"). |
| **`--color-gold` (accent, badge rim, evidence-board framing)** | `oklch(0.78 0.14 85)` / hex `#d4a93a` | The badge's gold rim and the banner's gold coin. Used for borders, the verdict-stamp frame, and small accent marks — never as a text colour at body size (contrast against `--color-ink` is borderline at small sizes; reserve it for ≥18px elements and 1–2px borders, checked before ship, not guessed here). |

Rejected alternative: drop the amber entirely and run the whole ladder on
green/red only. Rejected because a two-colour ladder collapses
`RugMechanicsLive`/`Sketchy`/`CantTell` into "not fully green, not fully
red" with no way to tell three different degrees of "not sure" apart at a
glance — the entire product's honesty argument (ADR 0027, "never accuses,"
"CantTell is not the same as clean") depends on those three reading as
visibly different states, not shades of the same warning.

**Type** — extending, not replacing, the existing "display face for
headings only, system stack for numbers" split
(`site/src/index.css`'s own stated reasoning, kept because it is sound: a
page whose argument is a column of numbers should not risk a display face's
tabular-numeral support). Recommended pairing, both free/open-licence:

- **Display (headings, the verdict stamp, the wordmark)**: **Space Grotesk**
  (SIL Open Font License 1.1) — a geometric grotesk with enough personality
  for a "REAL OR RUG" stamp treatment without going full graffiti-script,
  which would fight the badge's own hand-done wordmark rather than
  complementing it. Already common enough in crypto-adjacent products that
  it reads as "confident fintech," which is the "professional" half of the
  owner's brief.
- **Body and numerals**: keep the existing system stack
  (`site/src/index.css`'s current choice, unchanged) — the file's own
  reasoning ("the difference is small" for body text, "the identity needed
  it" only for headings) still holds; nothing in this brief changes body
  text's job.

Rejected alternative: a hand-lettered/graffiti display face everywhere, to
match the badge's "REAL/OR/RUG" 3D graffiti type directly. Rejected because
that type treatment belongs to the badge as a piece of finished art, not as
a live web font rendering arbitrary headline strings — a graffiti face
rendering "This token has not been checked yet" reads as a joke, not a
warning; Space Grotesk carries the *energy* of the badge (bold, confident,
geometric) without asking a system font-rendering path to reproduce hand
art.

**Texture / background treatment**: a subtle, low-contrast corkboard/paper
grain as a repeating background on `--color-ink`, generated once as a small
tileable PNG or CSS noise pattern (not the banner image scaled up — see
§3's atmosphere-not-foreground rule), plus a single occurrence of the
banner's "red string" motif as an SVG line-drawing accent connecting the
hero's paste box to the live feed section on `/` only (never repeated as
wallpaper — one deliberate use reads as brand; ambient use everywhere reads
as slow page and busy layout, exactly the "low effort" complaint in reverse
direction).

**Motion**, all respecting `prefers-reduced-motion: reduce` (a hard gate —
every animation below has a static equivalent, not a shorter version of
itself):

- Verdict stamp on `/check/:address`: a brief scale-and-settle (120ms) on
  first render, as if a stamp just hit the page. Reduced-motion: appears at
  final state, no animation.
- Live feed rows on `/`: new entries fade+slide in from the top on poll
  refresh (200ms), not a jarring reflow. Reduced-motion: list simply
  updates.
- Paste box focus state: the gold-rim accent (§5) brightens on focus,
  CSS `transition` only, no JS-driven animation — this one is not gated by
  `prefers-reduced-motion` since a colour transition under 200ms is not
  vestibular-motion territory (WCAG's own distinction), consistent with
  vestibular-motion territory (WCAG's own distinction), consistent with
  how `App.tsx`'s nav already does `transition-colors` unconditionally
  today.

## 6. What must never appear

Restated precisely because a design document is exactly where these get
loosened by accident:

- **No price, no market cap**, anywhere on the site, for any token,
  including the product's own token on `/tokenomics` (AGENTS.md §3 rule 5;
  design 0023 §5 already states the same for the share text). This applies
  to the home live feed (§4a item 5) and the `/check/:address` page (§4b)
  as much as it applies to the bot's own reply — a figure the fact sheet
  already drops before the model sees it (design 0023 §5, citing
  `realorrug-roast/src/sheet.rs`) must not be reintroduced by the site
  fetching it from a different, unfiltered source (a DEX API, for instance)
  just because the site is a different codebase from the bot. If a future
  page wants a price for context, that is a new direction question, not an
  implementation detail.
- **No accusing a person.** AGENTS.md §3 rule 4, restated for layout: the
  verdict describes a token or its launch, never names a person, account or
  company as a scammer or thief. Concretely for this site's art (§3): the
  banner's crossed-out sepia portraits are atmosphere-only art-board
  decoration and must never be placed adjacent to, or used as an icon for,
  a real token's verdict card — no "wanted poster" treatment of a creator
  wallet, no portrait-style avatar next to a `Rugged` stamp, even
  illustrated or generic. A creator wallet address, shown as inert
  monospace text exactly the way `Summoner` already renders one
  (design 0023 §6), is the only "who" this site ever names, and it is
  never paired with imagery that implies a face.
- **No token the operator holds.** ADR 0013 / AGENTS.md §3 rule 6. The
  `/tokenomics` page's whole job is stating this, unchanged from today's
  `/token` page's content — this document does not add anything that could
  contradict it (no "buy the token here" CTA anywhere on the site, home
  included).
- **No stale figure presented as fresh.** Every timestamp shown (verdict
  read time, pool `measured_at`, week `closed_at`) renders literally, the
  same "older but true, never blank and never invented" rule `site/src/api.ts`
  already states for `stats()` — this document extends that same discipline
  to the new home live-feed and `/check/:address`, it does not carve an
  exception for them.

## 7. Per-token share image — requirement only

A separate research thread is running on rendered verdict cards; this
document states what `/check/:address` needs from it and leaves the
renderer choice there, per the packet's instruction:

- One image per `(chain, address, verdict-level)` — not per raw fact sheet,
  since two reads of the same token that land on the same verdict level
  should not need two images (cache key mirrors design 0023 §3's own
  `(chain, address)` cache key, with the verdict level as the piece that
  actually changes the rendered art).
- Must render, at minimum: the verdict level in its verdict colour (§5),
  the token name/symbol as inert text (never as anything the renderer
  interprets as markup — same untrusted-input rule as design 0023 §6, now
  applied to a second surface), and a small wordmark/badge corner mark so a
  screenshot of the image alone is still attributable to the site.
  Must **not** render a price or market cap figure (§6) even if the
  renderer has access to one from a source outside the fact sheet.
- Must be servable as the `og:image` for `/check/:address` specifically —
  the X unfurl card is the entire point of a "shareable" page; a generic
  site-wide `og:image` on this route defeats §4b's reason for existing.
- Sizing: standard X/Twitter card minimum, 1200×630, under roughly 300KB so
  an unfurl is not itself a slow load on the "four seconds on a phone" case
  the rest of this document is designed around.

Renderer choice, caching strategy for the rendered image itself, and
whether it is generated at verdict-compute time (alongside design 0023 §5's
share text) or lazily on first request: **not decided here** — the running
research thread's scope.

## 8. Fixing the mobile nav clip (§0)

Not a new design, a correction to the existing one: `routes.ts`'s own
comment already states the six-item, 375px budget and gives `Pool` a
`short` label for exactly this reason (`"Prize pool" wrapped onto two
lines... and made the sticky header eat a third of the screen"`). With this
document's page rename (§4), the nav becomes: Home *(hidden, per today's
`r.path !== "/"` filter)*, Check, Contest, Payouts, How it works,
Tokenomics, About — seven visible items where today's is five. Every one of
these needs a `short` label under the same discipline `routes.ts` already
applies (`Check`, `Contest`, `Payouts`, `How it works` is already short
enough, `Tokenomics` → `Token`, `About` unchanged) — if seven short labels
still do not fit 375px, the nav needs a genuine overflow pattern (a "More"
menu, or moving `How it works` and `Tokenomics` to the footer list the way
the three trust pages already live there) rather than letting the row wrap
uncontrolled. Which of those two — shrink to fit, or move items to the
footer — is a call to make against the actual rendered widths once the
labels exist, not one this document can settle by reading source; named
here as the fix's shape, not its final measurement (§10).

## 9. Build order — 4 to 6 shippable slices

Each slice ships and is checkable on its own; later slices do not block on
future slices existing.

1. **Fix the stuck `Reading…` states and the brand text, no new pages.**
   **Shipped 2026-09-16.** Files: `site/src/Leaderboard.tsx`, `site/src/Pool.tsx`
   (the timeout/error fallback bug, §0), `site/src/App.tsx` (wordmark),
   `site/src/Home.tsx`, `site/src/About.tsx`, `site/src/Token.tsx` (every
   "Cabal Hunter" / "Solana" / "pump.fun" string replaced with the realorrug /
   Robinhood Chain equivalent). No new routes, no new art. This ships first
   because every later slice reuses these files' data-fetching and copy
   patterns — building the noir reskin on top of the stuck-spinner bug ships
   the same bug in nicer clothes.

   The fetch fix: `site/src/api.ts`'s `get()` cleared its 4s abort timer as
   soon as the response headers arrived, before reading the body. A server
   that sent headers and then stalled was never aborted, `pool()` and
   `leaderboard()` never settled, and the page read `Reading…` forever. The
   timer now runs until the body is read (cleared in a `finally`), so every
   call settles within `TIMEOUT_MS` and falls back as it always meant to.
   `site/src/api.test.ts` stalls the body behind a fake `fetch` that honours
   the abort signal, and fails if the timer is cleared early again.

   Left unfixed, out of this slice's scope: `site/src/Leaderboard.tsx` and
   `site/src/Pool.tsx` still carry unrelated Solana/pump.fun-specific mechanic
   copy (`SOL` units, `solscan.io` links, the pump.fun fee schedule, "a Solana
   wallet address"), as does `site/src/Contact.tsx`, `site/src/Privacy.tsx`,
   `site/src/Terms.tsx`, `site/src/History.tsx` and `site/src/title.ts`'s
   `SITE` constant — none of these were named in this document's §0, and
   replacing them correctly needs the actual Robinhood Chain payout mechanic
   (design 0025 §2 cites `realorrug-payout`'s two-transaction, Pons v2 escrow
   shape), not a guessed Solana-shaped substitute. A later slice or a
   dedicated pass should carry that fix.
2. **Palette and type system.** File: `site/src/index.css` (§5's colour
   tokens added alongside, not replacing, the existing ones where kept;
   `--color-good` updated; Space Grotesk added as the display face,
   self-hosted or a font-display: swap Google Fonts link, decided at build
   time). No layout changes yet — this slice is checkable by diffing
   rendered contrast ratios against WCAG, the same way the existing file's
   comment says the console's palette was checked.
3. **Nav fix and page renames.** Files: `site/src/routes.ts` (new `short`
   labels, `/token` → `/tokenomics`, `/leaderboard` → `/contest`, `/pool` +
   `/history` → `/payouts`), `site/src/App.tsx` (route table, §8's overflow
   fix). Existing page components move/rename, content unchanged from
   slice 1's fixed copy — this slice is the URL and nav restructuring from
   §4, without new page content yet.
4. **`/how-it-works`, new page.** Files: `site/src/HowItWorks.tsx` (new),
   wired into `App.tsx` and `routes.ts`. Static content only (§4e), no new
   API — ships independently because nothing else depends on it existing
   yet, and it is the page every later share-text and evidence-list link
   (§4b) points back to, so having it live before `/check/:address` ships
   means that link is never pointing at a 404.
5. **`/check/:address` and the home paste box**, once design 0023's route
   exists in `crates/realorrug-serve`. Files: `site/src/Check.tsx` (new,
   §4b), `site/src/Home.tsx` (hero + paste box, §4a items 2–4), new fetch
   helper in `site/src/api.ts` matching design 0023 §3's cache shape. This
   is the slice this document cannot ship ahead of `realorrug-serve`'s new
   route — named as a dependency, not a blocker on this document's own
   work, since the route's existence is design 0023's deliverable.
6. **Home live feed + art pass.** Files: `site/src/Home.tsx` (live feed
   section, §4a item 5, depending on the new recency-list route named in
   that section), texture/motion assets (§5) applied across all pages,
   `/check/:address`'s share-image `og:image` wiring once §7's research
   lands a renderer. This is deliberately last: it depends on the most
   not-yet-built pieces (a new serve route, a separate renderer), and every
   page already reads correctly (slices 1–5) without it — the live feed and
   the image are polish on a working, honestly-branded site, not
   prerequisites for one.

## 10. Not established

- The exact root cause of the `Reading…` stuck state (§0) — read from the
  component and `api.ts` source as a plausible mechanism (a response shape
  mismatch, or a timeout path that does not fire the catch), not confirmed
  by running the site or reading server logs. Slice 1 (§9) must diagnose
  this for real before writing the fix, not assume this document's guess.
- Whether "About" is the specific nav item that clips at 375px, versus some
  other item, versus the row simply feeling tight without visibly
  breaking — read from `routes.ts`'s own stated budget and `App.tsx`'s
  unwrapped flex row as a credible mechanism, not confirmed against a
  rendered screenshot at 375px.
- The original first-pass site review this packet cited by path (an
  `.orchestrator` run-folder document, 20260915-robinhood-7b) does not exist
  in this worktree or anywhere findable in the repository at the time of
  writing — every claim this document makes about the site's current state
  is instead re-derived directly from `site/src/*` and
  `crates/realorrug-serve/src/*`, cited by path and line above, and should
  be read as this document's own findings rather than a confirmation of
  that other review's.
- Whether the new home-page "recently checked" route (§4a item 5) belongs
  in `crates/realorrug-serve/src/public.rs` or a sibling module — design
  0023 §7 already draws that same line for the checker's own route (not
  `public.rs`, because that file's handlers only ever read a published file
  and never the store) and this new route has the same shape (reads the
  checker's cache, which is not one of the five pre-computed public
  documents) — but the module is not created by this document.
- Exact font-loading mechanism for Space Grotesk (self-hosted static asset
  vs. a Google Fonts link) — a build-tooling choice against the performance
  budget below, not decided here.
- Whether Robinhood Chain and Solana checking both exist on
  `/check/:address` at launch, or Robinhood Chain ships first — design 0023
  §8 already left this open and this document does not resolve it either;
  the page layout in §4b works identically either way.

## Performance budget

- First contentful paint on the home paste box: under 1.5s on a mid-tier
  phone over 4G — the box is the page's entire job (§4a), and it must be
  visible before any texture, live feed, or webfont has necessarily
  finished loading. Space Grotesk loads with `font-display: swap`
  specifically so the headline text is never blocked on it.
- Texture/background assets (§5): each under 40KB, tileable, cached
  aggressively (`Cache-Control: public, max-age` matching the existing
  `public.rs` pattern `crates/realorrug-serve/src/public.rs` already uses
  for its own responses).
- Share image (§7): under 300KB, per §7's own sizing note.
- No page ships a render-blocking script beyond what Vite's production
  build already produces for the existing site — this document adds pages
  and assets, not a new client-side framework or state-management
  dependency (AGENTS.md §4, "smallest change that fully solves the
  problem").

## Art assets to produce

| Asset | Purpose | Size(s) |
|---|---|---|
| `site/public/brand/logo.png` | Header wordmark / badge, favicon source | Owner-supplied; site consumes at ≤64px header height, plus a derived favicon set (32×32, 16×16, apple-touch 180×180) generated from it |
| `site/public/brand/banner.png` | Reference only for texture extraction (§5) — not displayed full-size on any page | Owner-supplied |
| Corkboard/paper texture tile | `/` and `/check/:address` background | One tileable PNG or SVG pattern, ≤40KB, roughly 256×256 source tile |
| Red-string SVG accent | `/` hero-to-feed connector (§5), one use only | Vector, no raster size constraint |
| Verdict-stamp treatment (5, one per ladder level) | `/check/:address` headline stamp | CSS/SVG combination using §5's palette, not five separate raster images |
| Favicon / touch-icon set | Browser chrome | Derived from `logo.png`, standard sizes above |
| Per-token share image template | `/check/:address` `og:image` (§7) | 1200×630, template only — renderer and per-token instances are the separate research thread's output |

## Not established

(See §10 for the full list. Summary: the `Reading…` root cause, whether
"About" specifically is the nav item that clips, the missing first-pass
site review named in the packet, the new recency-feed route's module home,
font-loading mechanism, and Robinhood-vs-Solana launch order on the
checker — none of
these are re-decided by this document and each is named at the point it
matters above.)

## ASCII wireframes

### Home — mobile (375px)

```
┌─────────────────────────┐
│ [≡] realorrug     [nav] │  <- sticky header, §8 fix
├─────────────────────────┤
│  (subtle board texture) │
│  [badge]                │
│  REAL OR RUG — checked, │
│  not predicted.          │
│  Robinhood Chain ·       │
│  measured since Sep 2026 │
│                          │
│ ┌──────────────────────┐│
│ │ Paste a token addr…  ││  <- paste box, largest element
│ └──────────────────────┘│
│      [ Check it ]        │
│                          │
│ Contract: 0x22fd…48fa 📋 │  <- copyable CA
├─────────────────────────┤
│ Latest verdicts          │
│ • 0xAB12…  Sketchy       │
│ • 0xCD34…  NothingUgly…  │
│ • 0xEF56…  Rugged        │
├─────────────────────────┤
│ This week's contest      │
│ 1. @alice   142 pts      │
│ 2. @bob      98 pts      │
│ 3. @carol    71 pts      │
│      [ See full contest →]│
├─────────────────────────┤
│ footer (trust links,     │
│ close time, @handle)     │
└─────────────────────────┘
```

### Home — desktop (≥1024px)

```
┌───────────────────────────────────────────────────────────────┐
│ realorrug        Check  Contest  Payouts  How it works  ...   │
├───────────────────────────────────────────────────────────────┤
│   [badge]   REAL OR RUG — checked, not predicted.               │
│             Robinhood Chain · measured since Sep 2026          │
│                                                                 │
│   ┌───────────────────────────────────┐  Contract:             │
│   │ Paste a token address…   [Check]  │  0x22fd…48fa  📋       │
│   └───────────────────────────────────┘                        │
├───────────────────────────────┬─────────────────────────────────┤
│ Latest verdicts                │ This week's contest              │
│ • 0xAB12…      Sketchy         │ 1. @alice     142 pts            │
│ • 0xCD34…      NothingUglyYet  │ 2. @bob        98 pts            │
│ • 0xEF56…      Rugged          │ 3. @carol      71 pts            │
│ • 0x9988…      CantTell        │      [ See full contest → ]      │
├───────────────────────────────┴─────────────────────────────────┤
│ footer                                                          │
└───────────────────────────────────────────────────────────────┘
```

### `/check/:address` — mobile (375px)

```
┌─────────────────────────┐
│ [≡] realorrug     [nav] │
├─────────────────────────┤
│   ┌───────────────────┐ │
│   │     SKETCHY        │ │  <- verdict stamp, amber
│   └───────────────────┘ │
│  TOKENNAME (TICK)        │
│  0x22fd…48fa  📋         │
├─────────────────────────┤
│ • Creator sold 91% of    │
│   holding in first 3 min │
│   (that alone isn't      │
│   enough on its own)     │
│ • Liquidity still in pool│
│ • No other large holder  │
│   has sold                │
├─────────────────────────┤
│ Read 4 minutes ago from  │
│ Robinhood Chain           │
│                            │
│ [ Share this verdict → ]  │
├─────────────────────────┤
│ footer                    │
└─────────────────────────┘
```

### `/check/:address` — desktop (≥1024px)

```
┌───────────────────────────────────────────────────────────────┐
│ realorrug        Check  Contest  Payouts  How it works  ...   │
├───────────────────────────────────────────────────────────────┤
│  ┌───────────┐   TOKENNAME (TICK)                              │
│  │  SKETCHY   │   0x22fd…48fa  📋                               │
│  └───────────┘   Read 4 minutes ago from Robinhood Chain        │
│                                                                 │
│  • Creator wallet sold 91% of its holding in the first three   │
│    minutes after launch. That alone can be a bundle, a         │
│    sniper, or a rug — it's not enough on its own.               │
│  • Liquidity is still in the pool right now.                    │
│  • No large holder besides the creator has sold.                │
│                                                                 │
│  [ Share this verdict on X → ]                                  │
├───────────────────────────────────────────────────────────────┤
│ footer                                                          │
└───────────────────────────────────────────────────────────────┘
```
