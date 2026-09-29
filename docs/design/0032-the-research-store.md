<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0032 — the research store

**Amended 2026-09-27 after audit.** Josh asked for an Opus audit of this
design and ADR 0041, in place of his own read (2026-09-27); the audit
returned BUILD AFTER AMENDMENTS. Every amendment below is folded into the
sections it touches; §7 and §9 record what the audit added as build
requirements and open questions.

**Status:** Proposed. Recommending, until Josh reads it — plan 0002's
decision log entry of 2026-09-25 moved phase 4 (research, forecasts,
reputation) before the launch and requires "a design document and an ADR"
before any code, because phase 4 changes a recorded property of
`realorrug-serve`. The decision this design rests on is
[ADR 0041](../adr/0041-the-serve-crate-may-hold-one-store.md), marked
recommending until Josh rules on it.
**Adds to:** [design 0028](0028-the-daily-five.md) §2–§10 (the game, the
odds, the insider hole, and build steps G1–G9) and
[design 0029](0029-bot-quality-then-a-solana-launch.md) §4 ("the community,
later"), which this document turns into a store, sign-in and limits, and a
build-order mapping.
**Reverses:** nothing by itself. It changes, once the code lands, the
recorded property that `realorrug-serve` "reads published files, never a
store" (`crates/realorrug-serve/Cargo.toml`; design 0023 §0 quotes the same
line) — which is exactly why AGENTS.md §2 requires this design and an ADR
first, before that line is touched.

## 1. The problem this solves

Design 0028 §2.4 requires forecasts to stay hidden until the six-hour window
closes ("Calls close six hours after listing, and stay hidden until then...
Hiding stops people copying the best callers"). A published file is public
from the moment it is written; there is no way to publish a file that only
becomes readable later without a process that holds it back and decides when
to reveal it. Design 0029 §4 adds sign-in, researcher profiles, evidence
submissions and discussion — all per-user, all written by visitors, none of
which a batch job can pre-compute onto a file the way the leaderboard and the
weekly pages are pre-computed today (design 0023 §0: `realorrug-serve` "shows
what another process already computed"). None of this can live in published
files. It needs a place a live request can write to and read back before the
next file refresh, which is what "never a store" ruled out.

**What "never a store" protected.** Design 0023 §0 describes the property
being protected precisely: `realorrug-serve` "has never needed a live chain
read" and runs neither the analyst nor the payout. **Correction:** it is not
true that this crate "depends on none of the chain-reading crates" —
`crates/realorrug-serve/Cargo.toml` lists `realorrug-onchain` and
`realorrug-robinhood` as dependencies, for design 0023's checker route
(`check.rs`) alone, a live, budget-metered read for an address nobody has
asked about before. Neither dependency reaches `realorrug-payout`
(`repo-conformance` holds that), and neither gives this crate a write path a
visitor controls, which is the property actually at stake here. The promise
was never "no SQLite file touches this
crate" — `crates/realorrug-serve/src/record.rs` already opens the analyst
daemon's own SQLite memory to write down every verdict it serves, for later
calibration (design 0023 §10, research 0052 §5), with the same deny-by-default
shape this design reuses in §3. The promise was narrower: no user identity,
no user-submitted data, no write path a visitor controls, and no dependency
that could put a spending key or the model's own judgement anywhere near this
process (AGENTS §3 rule 1: "nothing here holds a spending key"). Phase 4
needs exactly the thing that promise ruled out: a write path a signed-in
visitor controls.

**What still protects it.** `realorrug-contest` stays pure — "no clock, no
network, no key" (its Cargo description, and design 0028 §10's G2 and G3
build notes) — and never becomes the store; it only ever scores
rows the store hands it. The store holds records of what was said and read,
never judgement: it cannot compute a verdict, hold a spending key, or attach
value to a call or a reputation figure (AGENTS §3 rule 1; design 0029 §4,
"reputation has no transferable or redeemable value"). And the six existing
public routes (`/v1/public/stats`, `/leaderboard`, `/pool`, `/weeks`,
`/hunters`, `/recent`) keep reading published files exactly as before — only
the new forecast, evidence and reputation surface touches the store.

## 2. The records

Design 0029 §4 and plan 0002's phase 4 name five kinds of record. Every row
of every kind carries: the **chain** the token is on, the **token address**,
**timestamps** (submitted, window close, horizon, settled — a row states
whichever apply to its kind and leaves the rest absent, never zero, per
AGENTS §3 rule 8), **evidence references** (what the row rests on, not a
copy of it), the **rule version** that scored or will score it, and the
**id of the row it chains to** — the same hash-chaining
`realorrug-journal` uses (`crates/realorrug-journal/src/file.rs`,
`Journal::record` and `Journal::verify`; §3 below).

| Record | What it is | Key facts (source) | What it can never do |
|---|---|---|---|
| Forecast | A player's real-or-rug call on one of the daily five | Immutable once submitted (design 0029 §4, "immutable forecasts"); one per player per coin, first stands (design 0028 §2); six-hour entry window, fourteen-day horizon (design 0028 §2.4, plan 0002 phase 4); hidden until the window closes (design 0028 §2.4) | Be edited or withdrawn after submission; be shown, even to its own author, before window close |
| Outcome | What the chain settled the coin to, at horizon | Settled by the same readers the bot already has, i.e. the observation jobs design 0028 G5 runs (design 0028 §2.5: "the chain settles every call, never price"); `Rugged` when the code-computed level reaches `Rugged` inside the window; `Stood` only when the window closes with a successful settlement read in every 24-hour span of the horizon (a gap in the read history is not "no rug seen," it is no read); **`Unresolved`** is a third, explicit outcome for every other case (ADR 0038 decision 5; design 0029 §4; plan 0002 phase 4). Published as "rug observed within the window" / "no qualifying rug observed" / "unresolved" (ADR 0038 decision 5); `Stood` and `Rugged` are internal names only, and neither is ever shown or read as a "real" certification | Ever become a win or a loss for a forecast scored against it — see below for when `Unresolved` is assigned |
| Evidence submission | A link or a sheet reference plus the submitter's note | Never a fact the bot itself used or introduced (AGENTS §3 rule 2, "the model may not introduce a fact"); it is the submitter's claim, not the bot's | Be read into the bot's fact sheet or verdict; settle an outcome or a discussion (design 0029 §4, "[votes] never settle a chain fact, an outcome, or the bot's verdict" — the same rule applies to evidence, which is a citation, not a chain read) |
| Reputation record | Two separate figures per player: contribution and forecasting | Kept apart, no shared score (design 0029 §4, "separate reputation for contributions and for forecasting"); every figure shown with the sample size behind it (plan 0002 phase 4, "records are shown with sample sizes") | Carry a prize, a holder benefit, or any transferable or redeemable value (design 0029 §4; ADR 0038); be read as a statistical guarantee about a future call (design 0028 §4's luck-line caveat — see §5) |
| Discussion / vote | A comment or a vote on a token's page, surfacing research | Never settles a chain fact, an outcome, or the bot's verdict (design 0029 §4, verbatim); never moves the bot's score or level (AGENTS §3 rule 4; design 0028 §9, "It never moves the bot's score or level") | Settle anything; be weighted into the verdict; be attributed to a player who has not signed in |

**When `Unresolved` is assigned — proposal.** Design 0028 §2.5 only defines
two outcomes because the daily five's observation job was built and tested
against clean reads. Plan 0002's decision log entry of 2026-09-25 records
that 6 of 10 replayed cases came back `CantTell` not because the token gave
no evidence but because the live read deadline
(`crates/realorrug-onchain/src/budget.rs:69`) cut the chain read short — a
live-path fragility, not a property of the coin. A forecast whose horizon
closes while the settlement job cannot produce a definitive `Rugged`/`Stood`
reading (the read is incomplete, rate-limited, or otherwise cut short) is
proposed to settle `Unresolved` rather than being forced into `Stood` by
default — forcing it into `Stood` would let a slow endpoint quietly turn an
unread coin into a "the coin was fine" data point, which is exactly the
"missing data reads as safety" failure AGENTS §3 rule 8 forbids. **Correction:**
a third outcome is not new here — ADR 0038 decision 5 already names it
("a missing observation settles as unresolved, which cannot score a survival
call as correct"), and design 0029 §4 ("an explicit unresolved outcome") and
plan 0002 phase 4 name it too; what this design adds is the trigger
(the settlement job cannot produce a definitive `Rugged`/`Stood` reading) and
the exclusion from scoring. `Unresolved` forecasts are excluded from a player's luck-line
variance and z-score (design 0028 §4) the same way they are excluded from
points — an unread coin proves nothing about who read it correctly.

## 3. The store

One SQLite file, on the one host (plan 0002 phase 4: "SQLite on the one
host"). The path comes from configuration; with it unset, the store denies
by default — the shape already used for realorrug-serve's own verdict write
(`crates/realorrug-serve/src/record.rs`: `let Some(path) = memory_path else
{ return; };`, then `let Ok(memory) = Memory::open(path) else { return; };`,
so a request that cannot write still answers exactly as before). The file is
opened the way `realorrug-onchain::memory::Memory` opens its own: one
`Connection`, no pool, no async runtime, because SQLite serialises writes on
one file regardless (`crates/realorrug-onchain/src/memory.rs`, `Memory`'s
doc comment, "No connection pool, no async runtime").

**Append-only and hash-chained**, the way `realorrug-journal` chains: each
row carries the hash of the row before it (`crates/realorrug-journal/src/
file.rs`, `Journal::record`), so an alteration in the middle of the chain is
visible from every row after it, relative to a trusted checkpoint — the same
limit `realorrug-journal`'s own doc states plainly ("It does not resist a
host attacker who rewrites the whole chain"). A row is never updated or
deleted; a correction is a new row that references the one it corrects. A
**verify routine** walks the chain and reports the first break, the same
shape as `Journal::verify` (`crates/realorrug-journal/src/file.rs`).

**Who writes:** the `realorrug-serve` process only — the same process that
takes the sign-in and the call, because a second writer against one SQLite
file is the shape `realorrug-onchain::memory::Memory`'s own doc comment
rules out ("SQLite serialises writes on one file anyway"; ADR 0041 decision
1 makes this explicit for the new store). **Who reads:** `realorrug-serve`'s
own public reads (forecasts after close, outcomes, reputation figures,
discussion), and the calibration job of plan 0002 phase 5, which reads the
same rows — "keep community claims apart from verified labels" (plan 0002
phase 5) means phase 5 reads what phase 4 wrote, never the other way round.

**One writer, even for settlement (G5).** The settlement job (design 0028
G5, the observation jobs) does not open the store and does not become a
second writer against it. It publishes an outcome file, the same
published-file shape design 0023 §0 already uses for the leaderboard and the
weekly pages; `realorrug-serve` reads that file and ingests it as outcome
rows through its own single write path. `realorrug-serve` stays the store's
only writer regardless of how many separate jobs produce the facts that end
up in it.

## 4. Sign-in and limits

**Sign-in with X only, identity kept off the chain rows.** Chain rows (§2,
§3) name a player only by a random player key — a forecast, an evidence
submission, a vote never carries the handle or the X id directly. The X id,
handle, account creation date and session hash live in a separate, mutable
`identity` table outside the append-only store. Deleting an account removes
its identity row; the chain rows stay, but with nothing left to link them to
a person — they read as anonymous history, satisfying the append-only chain
(§3) and design 0028 §7's "nothing it cannot delete on request" at once,
which resolves the contradiction between them and answers ADR 0034's open
question ("whether we may keep a player's past calls after they delete
their X account"): we may, once the identity row is gone. Sessions expire
after 30 days. The X access token is thrown away once `/2/users/me`
returns — nothing longer-lived than the session itself is kept. The site
publishes a privacy notice listing these fields (handle, X id, account
creation date, session hash, player key) and how each is deleted or expires.
Research 0051 §1's citation of X's deletion-sync rule ("delete or modify any
content you have if it is deleted or modified on X") is why the identity
table, not the chain, is where deletion has to land.

**Cost, metered.** Each sign-in reads the player's own account once
(`/2/users/me`), at X's published per-resource rate of $0.010 (research
0051 §1, "each sign-in that calls it to learn handle+id costs about $0.01").
`realorrug-serve` holds its own `realorrug-provider` Meter for this spend,
capped by `REALORRUG_SERVE_MONTHLY_USD`, which must also fit inside
`REALORRUG_MONTHLY_USD` less `REALORRUG_FIXED_MONTHLY_USD`; when any of the
three is unset or invalid, or the slice is larger than what is left,
sign-in and the research assistant (below) both refuse rather than spend
unmetered (AGENTS §3 rule 7, "no budget refuses spending"). The analyst's
own fixed deduction against ADR 0039 decision 5's $90-a-month ceiling
already accounts for this line, so the meter enforces the mechanism that
line assumed rather than adding a second ceiling: fixed costs are taken off
first, and "model, RPC and X budgets share what is left" (ADR 0039 decision
5), so sign-in reads draw from the X share of that remainder, the same pool
automated replies and mention reads already draw from — this design's
earlier text said sign-in reads "count against" decision 5 with no
mechanism; the meter is that mechanism.

**Per-user limits — named numbers, one proposal each:**

| Limit | Number | Reason |
|---|---|---|
| Forecasts per round | 1 per player per coin, first stands | Cited: design 0028 §2, "One call per player per coin; the first stands and cannot be changed" |
| Evidence submissions per day | 20 per player — **proposal** | Bounds one account's contribution to the store's growth before any reputation gate has judged the account's submissions, without blocking a legitimate day of active research |
| Sign-ins per hour | 10 attempts per account or IP — **proposal** | Each attempt costs about $0.01 (above); an unbounded retry loop turns a client bug or a probe into an uncapped draw against ADR 0039 decision 5's ceiling before the monthly stop notices it |
| Research-assistant calls per day | N per player, a config value — **proposal** | Metered under the sign-in Meter above; bounds one account's draw on the assistant's own spend, the same shape as the sign-in and evidence limits above |

**The assistant never picks a side.** It drafts the research the same way it
drafts anything else on the fact sheet, per AGENTS §3 rule 2: facts from the
sheet, open questions, cited evidence. The user, not the assistant, picks
real or rug — the assistant's output is a draft the user reads and decides
from, never a submitted forecast. User evidence, discussion and forecasts
enter the assistant's prompt only as quoted data, the same untrusted-content
shape AGENTS §3 rule 3 already requires for mentions and post text — never
in a system-prompt position. The reason a side is never picked: a model
choosing real or rug is a hint at a future move with no measured outcome
rate behind it (AGENTS §3 rules 4 and 5), which this design's earlier text
missed when it said the assistant "proposes a forecast the user confirms"
(design 0029 §4) — it proposes research, and the user is the one who
forecasts.

**Cache ages on public reads** (ADR 0039 decision 6: "When a read is served
from cache, the reply or page says how old it is; stale data is never shown
as current"). Every public read from the store — the round's forecasts once
revealed, settled outcomes, reputation figures — states the age of what it
is showing, the same discipline `crates/realorrug-serve/src/check.rs`
already applies to its own cached reads.

**No CORS header without a configured origin**, the same deny-by-default
shape `crates/realorrug-serve/src/public.rs` already uses: an unconfigured
origin means no `Access-Control-Allow-Origin` header at all, not a wildcard
(AGENTS §3 rule 7, "no origin sends no CORS header").

## 5. Determinism and replay

A round replayed from its own rows must settle to the same outcome every
time: the outcome record is written once, from the rule version and the
evidence references pinned to it, and a verify pass that re-derives an
outcome from those same inputs must reach the same value or the chain is
broken (§3's verify routine). This mirrors design 0028's own discipline for
scoring — no floating point decides a rank (design 0028 §3, §10 "G2 as
built"), so a rule version pins exactly which integer rule scored a round,
and a later rule change cannot silently reach back and rescore a settled
row.

The luck line (design 0028 §4) drops its statistical promise once forecasts
run beyond the closed cohort of the daily five's original five-coins-a-day,
twenty-calls-minimum design: a community player's record may be thin, and a
player's line is only as meaningful as the sample behind it. Every
reputation and luck-line figure the store publishes therefore carries its
sample size next to it (plan 0002 phase 4, "records are shown with sample
sizes") rather than a bare score, so a record built on five calls reads
differently from one built on five hundred without needing a second number
to say so.

## 6. Crate placement

**Recommendation: a new `realorrug-store` crate**, not a module inside
`realorrug-serve`. The one tradeoff that decides it: plan 0002 phase 5's
calibration job reads "the same rows" phase 4 writes (§3 above), and that
job is not `realorrug-serve` — it is a separate batch process, the same
shape as the calibration work `realorrug-onchain::memory::Memory` already
serves to the analyst daemon and to `realorrug-serve`'s own verdict writes
alike. Putting the store's schema, chaining and verify routine inside
`realorrug-serve` would mean phase 5's calibration job either duplicates
that code or depends on the whole web-server crate (its router, axum, and
every route) just to read forecast rows — the same kind of layering AGENTS
§4 already rules out for the model-facing side ("no path from a model-side
crate to the payout"). A standalone `realorrug-store` crate, read by both
`realorrug-serve` and the phase 5 calibration job the way both
`realorrug-serve` and `realorrug-analyst` already read
`realorrug-onchain::memory::Memory`, avoids that duplication. `ADR 0041`
decision 1 ("`realorrug-serve` may hold one SQLite store") is satisfied
either way — `realorrug-serve` depends on `realorrug-store` and calls it,
which is holding the store, not owning its schema.

`realorrug-contest` stays pure regardless of this choice: it never opens a
connection, never reads a clock, and never holds a key (its Cargo description). It is handed rows by whichever crate reads the store and returns
scores and rankings, exactly as design 0028's G2 and G3 already work.

## 7. What waits on Josh

Everything below is a Gate in `INTENT.md` ("deploying to the live server, X
credentials in production, posting from the X account, and any spend") or
named directly in plan 0002's 2026-09-25 decision log entry:

- **Live deploy.** Phase 4 may be built and run "on a private box with test
  config" (plan 0002 decision log, 2026-09-25); deploying it to the live
  server is Josh's gate, as it already is for the daily five itself (design
  0028 §10, G4: "Live deploy is Josh's gate").
- **A production X app.** Sign-in needs a registered X application; a
  production one, as opposed to test config, is a spend and credential
  decision.
- **Posting.** Any settlement or weekly post the store's records feed stays
  Josh's gate per post type, exactly as design 0028 §8 already requires.
- **Any spend.** Sign-in costs money per read (§4); spending against ADR
  0039's ceiling is not authorized by this design.
- **PR #141's install**, for settlement (G5). PR #141 ("deploy: hourly
  record-launches timer (not installed)") adds the unit files and install
  steps for the hourly job that records every launch; "nothing is installed
  by this PR; installing waits for the owner's yes" (PR #141's own
  description). Design 0028's G5 (settling calls on the observation jobs)
  depended on that job actually running. As of 2026-09-29 (section 12) it does
  not: settlement reads the chain itself and is ordered after nothing, so this
  install is no longer a dependency of it. Installing either job stays Josh's
  gate.
- **The legal packet, extended.** Before launch, design 0030 gains §8, "the
  forecasting game and personal data": free entry and no prize; retention of the personal data
  §4's identity table holds. The legal and tax review (row 9-26-0022,
  research 0066) answers those questions the same way it answers design
  0030's other sections (ADR 0042).

### The audit's owner questions

- **Q1 — decided (Josh, 2026-09-27).** While G1 (the odds-by-level replay,
  design 0028 §10) is unmeasured, the board shows hit/miss counts with
  sample sizes only. It moves to odds scoring once G1's replay lands.
- **Q2 — decided (Josh, 2026-09-28): no crowd signal.** The audit asked
  whether the crowd signal (design 0028 §9, "62% of players called this a
  rug") should be weighted by record. Josh dropped it instead: an aggregate
  of players' calls is noise, fake accounts can tilt it, and shown beside a
  live, tradeable token it is the part of phase 4 most likely to read as
  investment advice. Step 4-7 is removed (§8); design 0028's G8 is dropped.
- **Q3 — decided (Josh, 2026-09-27).** $REALORRUG is playable in the daily
  five exactly like any other token, settlement posts included. The audit
  recommended withholding settlement posts about it; Josh chose no exception,
  which keeps AGENTS §3 rule 5 ("the project's own token is treated exactly
  like any other") literal. The legal and tax review (row 9-26-0022) reads
  whether a project posting its own token's outcome reads as promotion.

## 8. Build order

**The ledger's order**, replacing the unit-numbered table this section
carried before the audit:

1. **4-2 store:** forecast records + store, pure.
2. **4-3 serve:** sign-in, authenticated submission, public reads.
3. **4-4 settlement.**
4. **4-5 site pages.**
5. **4-6 research assistant.**

There is no 4-7: the crowd signal was dropped (Josh, 2026-09-28; Q2 in §7).

**Correction.** The table this replaced said "design 0028's daily five
already existing (G1–G3, done)". G1, the table of odds by level, is not
built — research 0051 marks its replay as blocked ("the replay itself is
blocked"), and design 0028 §10 lists G1 as waiting on a slice merged
2026-09-18, not as done. Only G2 and G3 have "as built" sections (design
0028 §10); G1 does not. The board (Q1 above) ranks by hit/miss counts with
sample sizes until G1's replay lands.

## 9. Build requirements from the audit

These are requirements for the build, not proposals — 4-2 and 4-3 (§8) must
satisfy every one of them.

**Enforcing "no value" (AGENTS §3 rule 1; ADR 0038).**

- Add `realorrug-store` to `repo-conformance`'s `MODEL_SIDE` list
  (`crates/repo-conformance/src/lib.rs`), which forbids a path from any
  model-side crate to `realorrug-payout`.
- No route serialises `contest::calls::winner()` — a reputation or board
  endpoint may show a rank or a hit/miss count, never a computed winner.
- Add "reward", "win", "winner", "redeem" and "airdrop" to
  `FORBIDDEN_CLAIMS` in `site/src/honesty.ts`.
- No wallet address is ever asked for or stored — sign-in is X-only (§4);
  nothing here is a payout surface.

**Late forecasts and duplicates.** A forecast after the window closes is
refused by the server clock, not the client's. A unique `(round, coin,
player)` index means the first call stands (§2, "Forecast"); a second call
against the same index gets `409 Conflict`.

**Authors see their own call.** An author may read their own hidden call
through an authenticated read before window close. It leaks nothing to
other players — the route checks the caller's own player key against the
row's, and answers nothing for anyone else's key. This relaxes §2's earlier
"not even its author" line; say so here rather than leaving the two in
contradiction.

**Sample size.** §5's "every record carries its sample size" means
reputation figures, which are aggregates over many rows. A
single forecast has no n of its own — the sample-size discipline applies to
what is computed from forecasts, not to the forecast row itself.

**Sessions.** HttpOnly, Secure cookie, stored server-side as a SHA-256 hash
— never the raw session token. An exact-origin CORS allowlist with
credentials, matching §4's existing "no CORS header without a configured
origin" (AGENTS §3 rule 7). OAuth 2 with PKCE plus `state`. A CSRF token on
every POST. Prefer an `api.` subdomain of the site's own domain over a
separate domain, so cookies scope correctly.

**Account age.** The account-age eligibility rule (design 0028 §1)
needs `user.fields=created_at` on the `/2/users/me` read, stored in the
identity table (§4) alongside the handle and X id.

**`Unresolved` outcome, in code.** `contest::calls::Outcome` gains
`Unresolved`, excluded from points, variance and n — the same exclusion §2
already states in prose, named here as the type-level change that build
step must make.

**Settlement cadence and cost.** Settle once a day, plus once at the
horizon — not hourly. Hourly reads across roughly 70 coins run to about
100M compute units a month, against the free plan's 30M
(**estimate**; section 12 records the cadence built, the arithmetic at that cadence, and the method for the measured figure, which is not yet taken).

## 10. As built (4-2, `realorrug-store`)

A new crate, `crates/realorrug-store`, holding one `rusqlite::Connection`
(`bundled`), no pool, no async runtime, no clock and no key — matching
`realorrug-onchain::memory::Memory`'s own shape. `Store::open` is the only
way in; a caller with no configured path never calls it, the same
deny-by-default shape `realorrug-serve/src/record.rs` uses for its verdict
write.

**One table, four kinds.** Rather than one table per record kind, every
forecast, outcome, evidence and discussion row lands in a single `rows`
table (`kind`, `round`, `chain`, `token`, `player_key`, `at`, a
kind-specific JSON `payload`, `previous_hash`, `hash`), chained in
submission order regardless of kind. §3 names the chain as one property of
the store, not one per record kind, so one chain across all four kinds is
what "each row carries the previous row's digest" means literally; splitting
it into four independent chains would have been a design choice this
section does not ask for. Only the hash function, blake3, is shared with
`realorrug-journal`; the field encoding is this crate's own (each field
length-prefixed so a byte cannot shift across a field boundary and hash the
same, and a player key hashed with a presence marker, `0` for none and `1`
followed by the key, so a NULL and an empty string cannot hash alike).
`Store::verify` walks the chain and reports the first broken `seq`; it has no
`Torn` case the way `realorrug_journal::Verified` does, because a write is
one transaction that lands or does not — there is no "complete row with no
terminator" shape to distinguish from a broken one.

**Departures from §2 and §3, on purpose.** A forecast row carries no rule
version, no horizon and no evidence references of its own (the close time is
in its payload; the rule version and the evidence reference belong to the
outcome row, which is where they are known). And a correction is not a new
row that references the one it corrects: the store picks the authoritative
outcome as the latest outcome row for the round and coin (`Store::outcome`),
because a reference from the correction to the corrected row would be one
more field for a caller to get wrong for no reader that needs it yet. §2 and
§3 are the aim; this is what 4-2 built.

**Append is one locked unit.** Every write reads the tail and inserts inside
one `BEGIN IMMEDIATE` … `COMMIT` (rolled back on error), so a second
connection on the same file waits or gets `SQLITE_BUSY` instead of reading
the same tail and chaining a second row to it. Two autocommit statements
would not do it, however many connections there are. A unique index on
`previous_hash` (`ux_chain_link`) is the structural backstop: the first row
chains from the fixed genesis constant and only one row can hold it, so two
rows can never share a predecessor. Append-only is also enforced in the
schema: `BEFORE UPDATE` and `BEFORE DELETE` triggers on `rows` abort with
"rows are append-only". They are not tamper-proofing — a host attacker with
the file drops them — but a raw statement on an open connection is refused.

**What `verify` proves and does not.** `Verified::Intact` carries `rows` and
`head`, the last row's hash. A deleted middle row, a swapped `seq` and an
edited field each break the chain at a named row. A truncated tail does not:
the shorter chain is valid. It is detected only against a head recorded
earlier where the writer cannot reach it, which is why `head` is returned.

**The unique-once-stands rule** is a partial SQLite index,
`ux_forecast_once` on `(round, chain, token, player_key) WHERE kind =
'forecast'`, so it costs outcome/evidence/discussion rows nothing.
`Store::submit_forecast` checks the rule itself under the write lock and
returns `StoreError::Duplicate`; `realorrug-serve` maps that to `409` in 4-3.
Any other constraint failure is `StoreError::Constraint`, never `Duplicate`,
so a `409` always means "you already called this." A stored row whose kind
and payload disagree is `StoreError::Corrupt`, not a panic.

**Visibility (§9's "Authors see their own call").** `Store::forecast` takes
both the row's own player and the requesting player and answers `None` for
anyone but the author before the row's own `window_close` — never a
separate "is this mine" flag a caller could get wrong, since the row itself
carries the close time it was submitted against.

**The close boundary.** `submit_forecast` refuses at `now >= window_close`,
and `forecast` reveals at the same `now >= window_close`, so no instant
exists at which entry is open and a call can already be read. This is design
0028 §2.4: at close, entry shuts and the reveal begins. A test pins
`now == window_close` to `WindowClosed`. This holds for forecasts that
share one `window_close`: the store takes the close per call and trusts it,
so 4-3 must source it from the round, never the client (follow-up 13).

**The Q1 board's count.** `realorrug_contest::calls::hit_miss` returns
`HitMiss { hits, misses, n }`: a hit is a call whose side matched the
outcome, `Unresolved` is excluded from all three counts, and `n = hits +
misses`. It is what 4-3's board serves, not `score_calls`: a count carries
no value, where `score_calls` produces points and a winner (§9).

**The player key.** `PlayerKey` is a newtype only the store can build:
`Store::new_player_key` (128 bits from SQLite's `randomblob`, 32 lower-case
hex characters) or a read of an identity row. Every write method takes
`&PlayerKey`, so an X id or a counter cannot be passed by mistake. `x_id` is
`UNIQUE` on `identity` (a second key for the same X account is
`StoreError::XIdTaken`), and `Store::player_for_x_id` returns the existing key
so a returning user keeps one record. `calls::SettledCall::player` and
`PlayerRecord::player` are this key, never the X id.

**Identity (§4).** A second table, `identity`, keyed by `player_key`, holds
the X id, handle, `account_created_at` and session hash. No chain row has a
column for any of them — not "redacted on delete," never present to begin
with — so `Store::delete_identity` is one `DELETE` against one table and the
chain needs no migration to stay valid and unlinked. `init` sets `PRAGMA
secure_delete = ON`, so the deleted row's bytes are overwritten in the file
and not left in a free page; a file-backed test scans the database and any
journal for the X id after the delete.

**Reused, not copied (§6).** `realorrug_contest::calls::{Side, Odds,
Outcome}` are the store's own forecast-side and outcome types; `Outcome`
gained `Unresolved` there (§9), excluded from `points`, a player's
`variance` and `settled` (`n`) by filtering it out before `record_for` and
`eligibility` sum anything, rather than scoring it; `points` returns 0 for it only to keep the match exhaustive. The
store's own `Store::public_wording` is the §2/A4 mapping
(`Rugged`→"rug observed within the window", `Stood`→"no qualifying rug
observed", `Unresolved`→"unresolved"). That `Store::public_wording` is the
only place either internal name reaches a reply is a convention 4-3 must
keep, not something the types enforce: `Outcome` derives `Serialize`, so a
handler that serialises one directly would print `Rugged` or `Stood`.

`realorrug-store` is added to `repo-conformance`'s `MODEL_SIDE` list
(§9's requirement) in the same commit.

**Not done in 4-2, deferred to 4-3+ per the packet's own scope:** HTTP,
sign-in, and the `FORBIDDEN_CLAIMS` site edit.

## 11. As built (4-3, `realorrug-serve`)

Two files in `crates/realorrug-serve/src`: `auth.rs` (sign-in, sessions,
CSRF, the meter, the rate limit, CORS) and `forecast.rs` (the forecast and
read routes). `session_routes` in `lib.rs` merges both under one
`Arc<AuthState>` and layers CORS over them, so the tests drive the layering
the server runs.

**Routes.** `GET /auth/x/start`, `GET /auth/x/callback`, `GET /auth/me`,
`POST /auth/logout`, `POST /account/delete`; `POST /forecast` (session and
CSRF; body `{round, chain, token, side}` and nothing else), `GET
/forecast/mine?round=` (the caller's own calls, `no-store`); public reads
`GET /v1/rounds/{round}`, `/v1/rounds/{round}/forecasts`,
`/v1/rounds/{round}/outcomes`, `GET /v1/board`, `GET /v1/privacy`. Nothing
here signs, spends or posts, and nothing asks for a wallet address.

**Where a round's close comes from (chosen, not measured).** A JSON file named
by `REALORRUG_ROUNDS_FILE`: `{"rounds":[{"id","close","coins":[{"chain",
"token","q_basis_points"}]}]}`, re-read on each request so the operator can
open the next round without a restart. Unknown fields are refused; a missing,
unparseable or duplicate-id file is no file, and every round route answers
503 (rule 7). An unknown round is 404; a coin outside the round is 400. The
client never supplies the close or the odds; the close that reaches
`Store::submit_forecast` is the file's, and the store refuses at `now >=
window_close` on the server's clock. There is no operator route to edit the
file and no job that writes outcomes yet; both are the next step, and
until then settled outcomes come only from whoever writes them to the store.
The alternative, a rounds table in the store, would have made the close a
chain row the operator cannot correct; the file keeps the round definition
where the operator can fix a typo before anyone has called.

**Odds.** Required for every coin in the rounds file, as a `u16` in basis
points. A coin without `q_basis_points` makes the whole file invalid, so every
round route answers 503 and no row is written. There is no default: the odds
are written into the append-only forecast chain, and a stand-in such as an
even 5000 would sit there as though the bot had measured it (AGENTS §3 rule 8,
"absent is not zero"). The operator states each coin's odds when listing it.
The client never names it.

**Status codes.** `WindowClosed` 403, `Duplicate` 409, `WindowMoved` 503
(the rounds file's close for a round differs from the close its first saved
forecast carried, so the operator edited a live round and nothing is saved;
a 503 rather than a 409 because it is the server's inconsistency, not the
player repeating themselves), unknown field or bad side 400, too many scanning
public reads from one client (30 a minute, an IPv6 client counted by its /64)
429, oversized body 413, a busy store 503 (a SQLite `BUSY` means the
write did not happen, so it is safe to retry), no session or no CSRF 401/403.

**What a stranger can read, and when.** Before a round's close the round
routes answer `closed:false` with a note and no count, list or aggregate, so
nothing derived from a hidden call is observable. The route checks the close
from the round record, and the store filters each row by its own payload
`window_close` as well (`Store::closed_forecasts`, `settled_forecasts`), so
one wrong check does not leak. After close: forecasts carry chain, token,
side, odds and submission time and no player identifier at all (no key, no
handle); outcomes are the `Store::public_wording` phrases in a field named
`reading`, never the enum; the board is plain hit and miss counts with the
sample size `n` from `calls::hit_miss`, one line per opaque `board_id` (the
first sixteen hex characters of a domain-separated SHA-256 of the player key,
so two boards can be joined to each other but not to a key or a handle),
ordered by that id and not ranked, with lines of `n == 0` omitted. An
unresolved call is not in `n` (rule 8). No route serialises `winner()`,
`score_calls`, `DailyFiveRanking` or `PlayerRecord.total`. Every public
response states `read_at` (the server's clock) and `newest_at` (the newest
row it draws on, or null), with `Cache-Control: public, max-age=30`;
errors and `/forecast/mine` are `no-store`.

**Head, not row count.** `Verified::Intact{rows, head}` is checked at start-up
and the head is logged to stderr (journald) as `store head <hash>`, where the
writer of the database file cannot reach it; a broken chain is logged loudly.
The row count is never served, since before close it would reveal a hidden
call. This is a weaker record than a signed or published head; publishing one
is left open.

**Sessions and identity.** One live session per player: a new sign-in
replaces the old one, and the privacy notice says so. The session cookie's
hash is stored in `identity.session_hash`, and the session resolves to a
`PlayerKey` through a store method; no X-id map exists outside `identity`.
The forecast route uses the session's key as the requester and never one from
the request. The X access token is discarded after the one profile read.
Identities are never logged. Deletion is `POST /account/delete` (§9).

**CORS.** `Access-Control-Allow-Origin` is set only when the request's
`Origin` is exactly one entry of `REALORRUG_APP_ORIGINS`, with credentials and
`Vary: Origin`; never `*`, and no configured origin sends no CORS header
(rule 7). A preflight answers 204 for GET, POST, OPTIONS and the headers
`content-type` and `x-csrf-token`.

**Serve-owned meter.** PR #205's `serve_monthly_allowance_from` had not
merged when this was built, so `auth::serve_monthly_allowance_from` applies
the same $90-ceiling rule itself and is the one function to swap. The
allowance is `REALORRUG_SERVE_MONTHLY_USD` and is closed when
`REALORRUG_MONTHLY_USD` is unset or invalid, when `REALORRUG_FIXED_MONTHLY_USD`
is unset or invalid, when the fixed part is at or above the ceiling, when the
slice is unset, invalid or zero, or when it is larger than the ceiling less
the fixed part. Serve's slice is therefore checked against the same ceiling as
the other processes' slices, so the three cannot add up past $90.

**Serve the API from the site's own registrable domain.** The session cookie
is `SameSite=Lax`, so a browser sends it on the site's `fetch` calls only when
the API is same-site with the page. The API must be served from a subdomain of
the site's own registrable domain (for `cabalhunter.org`, `api.cabalhunter.org`).
A different site such as `radar.heyvera.org` is cross-site: the browser never
attaches the cookie to the site's fetches, so every signed-in call would read
as signed out. The cookie stays `Lax`, not `SameSite=None`: `None` would send
it on any cross-site request and widen the CSRF surface the token exists to
narrow. `REALORRUG_APP_ORIGINS` and `REALORRUG_X_REDIRECT_URI` name that same
API host.

**Site.** `FORBIDDEN_CLAIMS` gained "reward", "winnings", "you win", "redeem"
and "airdrop". "winner" and "win" wait: `History.tsx` still renders the
retired weekly prize's winner, and "win" is a substring of "window".

**Env** names for the box are in `deploy/README.md`.

## 12. As built (4-4, settlement)

*Reworked 2026-09-29.* The first build (PR #209 before this rework) read the
analyst memory's `verdict_outcomes` label and wrote the store itself. Both were
wrong against this document: one overwritten, untimed label per token cannot say
whether a rug fell inside the horizon or whether each 24-hour span was read
(the outcome row in §2 says the settlement job's own dated reads are the
evidence), and a second writer against the store contradicts "one writer, even
for settlement" (§3, G5). The coordinator's packet pointed at both; the code
followed the packet. This section describes the corrected build.

`realorrug settle` decides each round coin's one outcome from dated chain reads
it made itself. No model decides, nothing is posted or signed, and the command
never opens the store. The pure rule is `realorrug-contest/src/settle.rs`; the
command is `realorrug-cli/src/settle.rs`; the ingest is
`realorrug-serve/src/outcomes.rs`. The units `deploy/realorrug-settle.service`
and its timer are written and **not installed**; installing them is Josh's gate
(`deploy/README.md`, "Settle closed rounds").

**Three steps, three owners.**

1. `settle observe` (reads the chain, appends to a file). For every round coin
   whose horizon has not ended and that is owed a read, it reads the coin with
   the analyst's own readers under the normal per-read budget
   (`Budget::default()`, plus the compute-unit meter on Robinhood reads), builds
   the fact sheet, and appends one observation: read time, the level the code
   computes (`realorrug_roast::level`), whether the read was complete (any level
   but `CantTell`), the rule version, `sheet-blake3:<hash>` of the rendered sheet
   as the evidence reference, and the number of calls the read used. **No model
   call.** With no RPC configured (`--rpc` or `REALORRUG_RPC`; there is no
   default and the public endpoint is not used) it reads nothing, creates
   nothing and says so. The RPC URL is never printed, and a failed read is
   recorded as an incomplete `CantTell` observation with the error text
   dropped, since an RPC error can carry the URL. Reads per run are capped at 150.
2. `settle publish` (derives, writes a file). It derives each coin's outcome from
   the observations and appends the new rows to the outcomes file, in the
   published-file shape of design 0023 §0 (the whole file is written to a
   temporary name and renamed, so a reader never sees half of it). It writes
   nothing when there is nothing new. `--dry-run` prints and writes nothing.
   `settle verify` re-derives every published row from its own evidence.
3. `realorrug-serve` ingests (the only writer). See below.

**The rule (`settle-1`), against §2.** The round's `close` in the rounds file is
the end of the entry window (confirmed, not assumed: forecasts are refused at
`>= close`, and the reveal begins there), and the horizon is `close` plus
fourteen days. This was checked against the first build's guard and needed no
change.

- `Rugged` as soon as an observation inside the horizon has level `Rugged`,
  complete or not: a rug seen is a rug, whatever else the read missed.
- `Stood` only once the horizon has ended and every one of the fourteen 24-hour
  spans holds a complete read that was not `Rugged`.
- `Unresolved` only once the horizon has ended without either.
- **Nothing is written** for a coin whose horizon is open and which has not been
  seen rugged. There is no premature `Unresolved`; the earlier build's
  permanent-`Unresolved` problem cannot occur.
- An incomplete read is recorded but never enters the derivation as evidence, so
  a gap is a gap: absent is not zero, and unknown is not safe.
- The evidence reference of an outcome is `close=<n>;reads=<at>:<R|N>,...`
  (canonical, in-window complete reads), so a row is checkable from itself:
  `rederive` returns the outcome from the rule version, the evidence and
  `settled_at`, and nothing when they disagree, when the rule version is
  another's, or when any read is later than `settled_at`.
- `due` decides whether a coin is read now: only inside its window, not once it
  has been seen rugged, and not when its current 24-hour span already holds a
  complete read. That keeps a coin to one complete read per span.

**How serve ingests, and when.** `REALORRUG_OUTCOMES_FILE` names the published
file. At start, and then every ten minutes, a task in `realorrug-serve` reads it
and, for each row, writes through `Store::record_outcome_once`, the same single
write path the forecast routes use. A poll, not a file watch: the file changes
at most twice a day, a poll has nothing platform-specific to go wrong, and a
missed tick costs ten minutes, not a row. **Deny by default:** with the variable
unset no task runs and nothing is ingested; a file not published yet, unreadable,
or not the published shape ingests nothing; an unreadable rounds file ingests
nothing, since the close a row is checked against comes from it. A row is
written only when its round and coin are in the rounds file, the close in its
own evidence equals the rounds file's close, and it re-derives to its own
outcome from its own evidence. A row that fails is counted and skipped, and the
rest are still ingested. **Ingesting the same file twice writes nothing new:**
`record_outcome_once` checks for an existing outcome and appends inside one
write transaction, so a re-read, a grown file and a restart are all no-ops for
rows already held. The store's triggers refuse an UPDATE or DELETE, and a
written `Unresolved` is permanent: correcting one needs a new rule version and a
decision of Josh's, not a re-run.

**Cadence.** The timer is not hourly. It fires twice a day (04:15 and 16:15 UTC,
each with a randomised delay of up to ten minutes; `Persistent=true` catches a
missed one). Twice, not once, because `Stood` needs a complete read in every
24-hour span: with one run a day, a single missed or jittered run leaves a span
empty and turns a coin that stood into `Unresolved`, permanently. With `due`,
the second slot reads only the coins whose span is still empty, so the cost
stays at about one complete read per coin per day. The publish step after each
observe is the horizon-end pass. **The ordering dependency on record-launches is
dropped**: nothing here reads what that job writes.

**Cost, and what is not yet measured.** Design 0027 §2.4 gives 2,000 compute
units as the ceiling for one cold dossier (the Alchemy table it cites, checked
2026-09-18, 30M free CU a month), and the earlier estimate in §9 (about 100M CU
a month) was hourly reads of 70 coins: 70 x 24 x 30 x 2,000. At the cadence
built, the ceiling is:

> 5 coins a day x 14-day horizon = **70 coins in flight**, one complete read
> each per day: 70 x 30 x 2,000 CU = **4.2M CU a month at the ceiling**, about
> 14% of the 30M free plan, and at most 70 x 60 = **4,200 calls a day** (the
> read's 60-call budget). Reading every coin twice a day would be 8.4M, which is
> what a missing `due` would cost.

Two limits on that arithmetic. It is a ceiling, not a measurement: the figure
per read is the budget's cap, and the read's real use is lower. And only
Robinhood reads draw compute units (`take_cu`); a Solana read is metered by its
calls alone. **The measured number is not in this document.** The dossier
records its calls but not its compute units, no capture stores either, and
measuring it needs a live read against a paid endpoint, which this build did not
make. The method for whoever runs it: run `realorrug settle observe` once
against the configured RPC for one open round, then read the `calls` field of
each line in the observations file (and, for Robinhood coins, the provider's own
usage dashboard for the same interval, since the dossier does not count compute
units). Multiply the mean by 70 coins and 30 days, and replace the ceiling above
with it, dated and sourced.

**Files.** The rounds file (`REALORRUG_ROUNDS_FILE`), the observations file
(`REALORRUG_SETTLE_OBSERVATIONS`, one JSON object a line, append-only, each line
strict about unknown fields) and the outcomes file (`REALORRUG_OUTCOMES_FILE`).
The job holds no store path. A file that is not the expected shape is refused
whole, and the job never overwrites an existing outcomes file it cannot read.

**What is true today, and not.**

- `Stood` is now reachable from real data: fourteen days of complete reads, one
  per span, are exactly what the job produces. Nothing has run it yet (the units
  are not installed), so there are no outcomes to show and no board figure; the
  first `Stood` cannot exist before the first horizon ends, fourteen days after
  the first close.
- **The deviation from ADR 0041 decision 6 is gone.** The first build wrote the
  store directly; this one publishes a file and serve ingests it, as the ADR
  says.
- An observation is a code-computed level from a fact sheet at a moment. It is
  not a claim about a person, and no model wrote any of it (AGENTS.md section 3
  rules 1 and 4).
