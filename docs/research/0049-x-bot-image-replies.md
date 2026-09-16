<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0049 — Can the X bot attach an image to a reply, what it costs, and what the image should be

**Date:** 2026-09-16
**Status:** primary-source research (docs.x.com, developer.x.com, crates.io,
ai.google.dev, developers.openai.com) plus code reading of
`crates/realorrug-analyst/src/x.rs`, `oauth.rs` and `deploy/analyst.env.example`.
No account changes, no keys used, no spend. Every claim below is quoted with a
URL and a read date; every price is dated 2026-09-16 unless the source itself
carries an earlier date.

## 0. What the bot does today (ground truth from code)

`X::post` (`crates/realorrug-analyst/src/x.rs:431`) signs `POST /2/tweets`
with OAuth 1.0a user-context credentials (`oauth: Option<crate::oauth::Credentials>`,
`x.rs:237`) built from four env vars — `REALORRUG_X_API_KEY`, `REALORRUG_X_API_SECRET`,
`REALORRUG_X_ACCESS_TOKEN`, `REALORRUG_X_ACCESS_SECRET` (`deploy/analyst.env.example`).
Reads use a separate bearer (`REALORRUG_X_BEARER`) and cannot post
(`oauth.rs:1-9`, "`POST /2/tweets` **does not accept an app-only bearer
token.** It requires user context — OAuth 1.0a, or OAuth 2.0 user context with
`tweet.write`"). Two facts from the code that bound everything below:

- `oauth.rs:31-34` — the module's own documented rule: "**A JSON body is not
  signed.** OAuth 1.0a folds request-body parameters into the signature only
  when the body is `application/x-www-form-urlencoded`." `X::post` sends
  JSON and relies on this.
- `deploy/analyst.env.example` already carries a `REALORRUG_X_PRICE_REPLY` slot
  with **an unresolved uncertainty**: "whether a summoned reply carrying a
  URL is $0.010 or $0.200" — about a *link*, not media. No existing env var
  or price slot covers an image attachment; §1 below is new information the
  file does not yet have.

## 1. Can it attach an image, what auth, what limits, what it costs

**Endpoint.** `POST /2/media/upload` is the documented v2 upload endpoint
("Uploads a media file for use in posts, direct messages, or ads",
https://docs.x.com/x-api/media/upload-media, read 2026-09-16). A newer
three-step chunked flow (`/2/media/upload/initialize`, `/{id}/append`,
`/{id}/finalize`) also exists per developer-community reports, but for one
static PNG under 5 MB the single-call form is the one documented on the
reference page read above and is sufficient — chunking is for video/large
GIFs.

**Auth: OAuth 1.0a user-context works, per the endpoint's own documented
security schemes.** The reference page lists two accepted schemes: an
`OAuth2UserToken` with the `media.write` scope ("Upload media, such as
photos and videos, on your behalf."), and a `UserToken` scheme, which is the
docs' generic label for OAuth 1.0a (https://docs.x.com/x-api/media/upload-media,
read 2026-09-16) — the same credential shape the bot already holds in
`REALORRUG_X_API_KEY`/`REALORRUG_X_API_SECRET`/`REALORRUG_X_ACCESS_TOKEN`/`REALORRUG_X_ACCESS_SECRET`.
**This is contradicted in practice by developer reports, not confirmed
end-to-end**: a 2026 thread titled "Request for OAuth 1.0a Compatibility with
/2/media/upload Endpoint" and another, "Chunked upload media not working with
OAuth 1.0a" (both devcommunity.x.com, titles read via search 2026-09-16; full
thread bodies returned 403 to direct fetch and were not read) describe
friction specifically on the newer chunked `initialize/append/finalize`
endpoints. **Not established** by this pass: whether the single-call
`POST /2/media/upload` — the one the bot needs for one PNG — actually accepts
this bot's existing OAuth 1.0a credential today, only that the docs say it
should. §3 is a five-minute way to settle this without reading a devcommunity
thread further.

**Limits, for the plan to render.** Best-practices page
(https://docs.x.com/x-api/media/quickstart/best-practices, read 2026-09-16):
images up to **5 MB**, formats **"JPG, PNG, GIF, WEBP"**, and general upload
dimension bounds "between 32x32 and 1280x1024" with aspect ratio "between 1:3
and 3:1". A rendered card at, say, 1200×675 (16:9) or 1080×1080 (1:1) is well
inside every one of those numbers.

**Price.** The pay-per-use rate card (https://docs.x.com/x-api/getting-started/pricing,
read 2026-09-16) lists **"Post: Create" at $0.015 per request** and
**"Post: Create (with URL)" at $0.200 per request**, and — checked directly
today — **does not list a separate line item for `POST /2/media/upload` or
for a post carrying media without a URL**. That means:

- **Not established, and worth flagging loudly**: whether attaching an image
  (no link) bills as the plain $0.015 reply or bumps to the $0.200 "with URL"
  rate, and whether the upload call itself is billed at all. The rate card's
  own wording keys the $0.200 tier to a URL, not to media, and secondary
  aggregator pages (Blotato, Postproxy, not X's own docs, read 2026-09-16)
  repeat the $0.015 / $0.200 split the same way — none of them state a
  separate media-attachment price either. This is exactly the shape of gap
  `deploy/analyst.env.example` already warns about for the URL case, now
  showing up one line earlier: the bot must not guess, and §3's manual test
  is the only way to read the real invoiced number rather than write another
  unverified price into the env file.
- If media upload turns out to be unbilled and attaching an image does not
  count as "with URL", **the floor cost per reply is unchanged: $0.015**,
  same as today's plain-text reply — image or not.
- If it bills as a "with URL" post, cost per reply rises to **$0.200**, a
  13x jump the owner should see before turning this on, not after.

## 2. Two ways to make the image

### 2a. A rendered card (server draws a fixed template)

**Crates, checked against `deny.toml`'s allow list (Apache-2.0, ISC,
BSD-3-Clause, CC0-1.0, CDLA-Permissive-2.0, MIT, Unicode-3.0).** Read from
the crates.io registry API today (2026-09-16):

| crate | license (crates.io API) | latest version | last publish |
|---|---|---|---|
| `resvg` | `Apache-2.0 OR MIT` | 0.48.1 | 2026-08-02 |
| `tiny-skia` | `BSD-3-Clause` | 0.12.0 | 2026-02-02 |
| `fontdue` | `MIT OR Apache-2.0 OR Zlib` | 0.9.4 | 2026-07-29 |

All three licences (`Apache-2.0`, `MIT`, `BSD-3-Clause`) are already on
`deny.toml`'s allow list — nothing new to add there. All three published a
release within the last seven weeks (as of today), i.e. maintained, not
abandoned.

Two workable pairings:

1. **`resvg` + `tiny-skia`** — `resvg` renders a hand-written SVG template
   (verdict stamp, token name, a handful of fact-sheet fields laid out as
   `<text>` nodes) to a `tiny-skia::Pixmap`, saved as PNG. `tiny-skia` is
   `resvg`'s own rasterizer, so this is one dependency edge, not two
   independent ones. Text layout, wrapping and font fallback are `resvg`'s
   problem, not the bot's — the tradeoff for that is a small SVG template
   string built by code (the same "build then render" split the crate
   already favours over hand-formatted strings, cf. `reply_body` in `x.rs`).
2. **`tiny-skia` + `fontdue`** — draw rectangles/stamps directly with
   `tiny-skia`'s path API and rasterize glyphs with `fontdue`, no SVG layer.
   More code (manual text layout: line breaking, kerning are the caller's
   job) but no XML template to keep in sync with the fact sheet's field
   names, and one fewer crate than the resvg path counting `usvg` (resvg's
   SVG-parsing dependency, pulled in transitively).

**Font licensing.** Neither crate ships a font; one has to be bundled or
system-loaded. A metric font under the SIL Open Font License 1.1 (e.g. any
Google Fonts family) explicitly permits embedding and redistribution in
software — that licence is not currently in `deny.toml` because it governs a
font *asset*, not a Cargo dependency, and `cargo-deny` does not scan font
files. **Not established by this pass**: whether the repo has any existing
asset-licence policy outside `deny.toml` that a bundled font file would need
to satisfy — not found in this repository during this research.

**Cost per reply: effectively zero marginal cost** (CPU time on hardware the
service already runs on; no per-call vendor bill). **Latency:** template
rendering is local CPU work, sub-millisecond to low-millisecond for a small
raster — no network round trip, no queueing behind a rate-limited vendor API.
**AGENTS.md rule 2** ("The model may not introduce a fact... a check after
generation refuses anything else"): a rendered card is the cleanest fit for
this rule of all three options, because the numbers drawn on the image are
literally the same `FactSheet` values the text reply already draws from and
already passes `check_required` against — code fills a template with checked
fields, the model never sees or touches the image pixels, and there is no
image-specific check to build. This is a strict extension of the rule the
crate already enforces for reply text, not a new one.

### 2b. An AI-generated image per reply

**Two cheap image models, official pricing pages, read 2026-09-16:**

- **OpenAI `gpt-image-1-mini`**, low quality, 1024×1024: **$0.005 per
  image** (https://developers.openai.com/api/docs/models/gpt-image-1-mini,
  table row "1024x1024 | $0.005 | image", read 2026-09-16). Medium quality at
  the same size is $0.011, high is $0.036 — the same page, same table.
  Standalone caveat: this repo's own note in
  `deploy/analyst.env.example` about the *text* model applies in spirit here
  too — a newer/renamed model line can replace the one priced today without
  warning (`gpt-image-1-mini` sits alongside newer `gpt-image-2.5-sunburst` /
  `gpt-image-2.5-flare` models on OpenAI's current pricing page, read the
  same day, billed per-token rather than flat-per-image — a naming and
  pricing-model change already in progress on OpenAI's side).
- **Google `gemini-2.5-flash-image`**, standard tier: **$0.039 per image**
  output price (https://ai.google.dev/gemini-api/docs/pricing, read
  2026-09-16). Roughly 8x OpenAI's cheapest tier.

**Cost per reply: $0.005–$0.039**, on top of whatever `POST /2/media/upload`
and the reply post itself cost (§1) — strictly additive to the rendered-card
path's near-zero cost. **Latency:** a real network call to a second vendor,
seconds rather than milliseconds, and a second point of failure in the reply
path — the module's own `CALL_TIMEOUT` comment (`x.rs`, "one socket that
opens and never answers stops all of them") is exactly the failure mode an
image-generation call in the hot reply path would add.

**AGENTS.md rule 2 risk, stated plainly: a generated image is very likely to
put a number or word on the canvas that is not on the fact sheet.** Both
`gpt-image-1-mini` and `gemini-2.5-flash-image` are general-purpose
text-to-image models, not template fillers — asking one for "a card showing
token XYZ is 40 minutes old with 12 holders" gives it no mechanism to be
prevented from writing "3 hours" or a holder count it invented, the way the
text pipeline's post-generation `check_required` check can catch a wrong
number in prose but cannot read pixels. There is no rendered-text OCR check
in this codebase today, and building one to satisfy rule 2 for an AI image
is strictly more engineering than 2a's rendered card, which never has the
problem. This is the deciding factor against 2b for anything that displays a
fact-sheet number, independent of the per-image price.

### 2c. Hybrid — AI art background, code draws the text

Generate a small fixed set of background art once (per verdict level, e.g.
five backgrounds for `Rugged`/`RugMechanicsLive`/`Sketchy`/`NothingUglyYet`/
`CantTell`) with an image model, store the PNGs as repo assets, and at reply
time use 2a's rendering path (`resvg`/`tiny-skia`) to composite fact-sheet
text over the chosen background. **Cost per reply: same as 2a — zero
marginal**, because the AI generation cost (one-time, ~5 × $0.005–$0.039,
i.e. well under a dollar total) is paid once at build/asset time, not per
reply. **Latency: same as 2a**, local raster composite. **Rule 2: same
guarantee as 2a** — the only pixels drawn from untrusted/generated content
are decorative background art with no numbers or claims on it; every fact
on the card is still code-drawn from the checked `FactSheet`. This is
strictly better than 2b and only marginally more setup than 2a (five
one-time images to commission and store).

## 3. A minimal manual test

**Every credential below is the owner's to supply, read from the shell
environment, never printed by any command here.** All three variables the
bot's own OAuth signer needs are already in `deploy/analyst.env.example`;
this test reuses exactly those, plus one PNG.

```sh
# The owner exports these once, in a shell that is not logged/recorded.
# Same four values REALORRUG_X_API_KEY / _API_SECRET / _ACCESS_TOKEN / _ACCESS_SECRET
# already require in deploy/analyst.env.example -- "Read and write" app
# permission, set BEFORE the access token was generated (per that file's note).
export REALORRUG_X_API_KEY=...
export REALORRUG_X_API_SECRET=...
export REALORRUG_X_ACCESS_TOKEN=...
export REALORRUG_X_ACCESS_SECRET=...

# One small PNG under 5 MB, e.g. a placeholder card.
TEST_IMAGE=./test-card.png

# 1. Upload the image. This step needs OAuth 1.0a signing (the same
#    signature construction as X::post in x.rs, applied to
#    POST /2/media/upload with a multipart body instead of JSON) -- a curl
#    one-liner cannot sign OAuth 1.0a on its own, so this is the one step
#    that needs a short script (a few lines using this crate's own
#    crate::oauth::authorization, or any OAuth1.0a-capable client such as
#    `twurl`, X's own signed CLI, which the owner would need to install
#    separately) rather than raw curl.
#
#    Success looks like: HTTP 200/201 and a JSON body containing a numeric
#    "media_id" (or "media_id_string" on the v1.1-shaped response).

# 2. Post a reply carrying that media_id, to a real test post the owner
#    controls (never a stranger's), reusing X::reply_body's shape by hand:
curl -s -X POST "https://api.x.com/2/tweets" \
  -H "Authorization: <OAuth 1.0a header, signed as in step 1>" \
  -H "Content-Type: application/json" \
  -d '{"text":"test reply with image","reply":{"in_reply_to_tweet_id":"<TEST_POST_ID>"},"media":{"media_ids":["<MEDIA_ID_FROM_STEP_1>"]}}'

# Success looks like: HTTP 201, a JSON body with "data.id", and the reply
# visible on x.com under the test post, carrying the image.
```

**What settles §1's open question in the same test:** after step 2, check
the developer portal's usage/billing page (or the invoiced line item, once
one exists) for what that single reply-with-media was actually charged —
$0.015, $0.200, or something else. That single observed number is worth
more than anything a docs page states, per `AGENTS.md` §1 ("Check a number
before deciding on it... with a date and a source") and the exact gap
`deploy/analyst.env.example` already leaves open for the URL case. Record it
in that file's `REALORRUG_X_PRICE_REPLY` (or a new `REALORRUG_X_PRICE_REPLY_MEDIA`)
slot once read, not before.

## Recommendation

Build the rendered card (§2a or its hybrid variant §2c), not an AI-generated
image: it is free per reply, adds no network call to the hot reply path, and
is the only option that satisfies AGENTS.md rule 2 without new
fact-checking code, since every number on it is drawn by code straight from
the same checked `FactSheet` the text reply already uses. `resvg` + its own
`tiny-skia` rasterizer is the simplest crate pairing, both already
licence-compatible with `deny.toml` and actively maintained as of
2026-09-16. Before writing a line of rendering code, run §3's manual test
once with the owner's existing OAuth 1.0a credentials to confirm
`POST /2/media/upload` actually accepts them and to read the real invoiced
price of a media reply — the docs say OAuth 1.0a should work and describe
two possible prices ($0.015 or $0.200), but neither is confirmed against a
live charge, and this repository's own culture (per `deploy/analyst.env.example`'s
two prior pricing mistakes on 2026-09-04) is to never enter a price it has
not verified against a real invoice.

## Not established

- Whether `POST /2/media/upload` (the single-call form, not the newer
  chunked `initialize`/`append`/`finalize` endpoints) accepts this bot's
  existing OAuth 1.0a user-context credentials in practice — the endpoint's
  documented security schemes say it should (§1), but no live call was made
  this session and community reports of OAuth 1.0a friction were found only
  on the newer chunked endpoints, not on this one specifically.
- Whether a reply carrying media (no link) bills at $0.015 or at the $0.200
  "with URL" rate, and whether the upload call itself is billed separately —
  X's current pay-per-use rate card names a URL, not media, as the trigger
  for the higher tier, and states no line item for `POST /2/media/upload` at
  all (§1). This is the single most consequential unresolved number in this
  document — a 13x cost difference — and is exactly what §3's test settles.
- Any repo policy on font-asset licensing outside `deny.toml` (which scans
  Cargo dependencies, not bundled font files) — not found during this pass;
  relevant only if §2a or §2c is built and a font needs to be bundled rather
  than loaded from the host system.
- Full text of the two devcommunity.x.com threads on OAuth 1.0a media-upload
  friction — both returned HTTP 403 to direct fetch this session; only their
  titles, read via search-result snippets, are cited above.

**Confidence: CHECKED** on the crate-licence table (§2a, read directly from
the crates.io registry API today), on both image-model prices (§2b, read
directly from OpenAI's and Google's own pricing pages today), and on the
$0.015/$0.200 post-pricing distinction (§1, read directly from
docs.x.com/x-api/getting-started/pricing today). **CONDITIONAL** on the
recommendation in §"Recommendation" — conditional on §3's manual test
confirming OAuth 1.0a actually authenticates against
`POST /2/media/upload` and on reading the real per-reply-with-media price;
neither changes the crate/licence/rule-2 case for a rendered card over an AI
image, but a very high real price could change whether image replies ship
at all. **SPECULATION** only on the "not established" devcommunity friction
reports, cited by title, not by full text read.
