<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0032 — the research store

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
read," runs neither the analyst nor the payout, and depends on none of the
chain-reading crates. The promise was never "no SQLite file touches this
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
"reputation has no transferable or redeemable value"). And the five existing
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
| Outcome | What the chain settled the coin to, at horizon | Settled by the same readers the bot already has, i.e. the observation jobs design 0028 G5 runs (design 0028 §2.5: "the chain settles every call, never price"); `Rugged` when the code-computed level reaches `Rugged` inside the window, `Stood` when the window closes without that (design 0028 §2.5); **`Unresolved`** is a third, explicit outcome | Ever become a win or a loss for a forecast scored against it — see below for when it is assigned |
| Evidence submission | A link or a sheet reference plus the submitter's note | Never a fact the bot itself used or introduced (AGENTS §1 rule 2, "the model may not introduce a fact"); it is the submitter's claim, not the bot's | Be read into the bot's fact sheet or verdict; settle an outcome or a discussion (design 0029 §4, "[votes] never settle a chain fact, an outcome, or the bot's verdict" — the same rule applies to evidence, which is a citation, not a chain read) |
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
"missing data reads as safety" failure AGENTS §3 rule 8 forbids. This is a
proposal, not a cited rule: no document names a third outcome or its
trigger. `Unresolved` forecasts are excluded from a player's luck-line
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

## 4. Sign-in and limits

**Sign-in with X only.** What is stored: the handle, the X user id, and a
session token — nothing else. No password, no email, no other X data.

**Cost.** Each sign-in reads the player's own account once
(`/2/users/me`), at X's published per-resource rate of $0.010 (research
0051 §1, "each sign-in that calls it to learn handle+id costs about $0.01").
That cost counts against ADR 0039 decision 5's $90-a-month ceiling — fixed
costs are taken off first, and "model, RPC and X budgets share what is
left" (ADR 0039 decision 5), so sign-in reads draw from the X share of that
remainder, the same pool automated replies and mention reads already draw
from.

**Per-user limits — named numbers, one proposal each:**

| Limit | Number | Reason |
|---|---|---|
| Forecasts per round | 1 per player per coin, first stands | Cited: design 0028 §2, "One call per player per coin; the first stands and cannot be changed" |
| Evidence submissions per day | 20 per player — **proposal** | Bounds one account's contribution to the store's growth before any reputation gate has judged the account's submissions, without blocking a legitimate day of active research |
| Sign-ins per hour | 10 attempts per account or IP — **proposal** | Each attempt costs about $0.01 (above); an unbounded retry loop turns a client bug or a probe into an uncapped draw against ADR 0039 decision 5's ceiling before the monthly stop notices it |

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
  depends on that job actually running, so its live install is Josh's gate
  the same way the timer's own PR already says.

## 8. Build order

Plan 0002's phase 4 units (4-2 through 4-7, in the order its text lists
them) mapped onto design 0028's build steps G4–G8:

| Plan 0002 unit | Design 0028 step | What it waits on |
|---|---|---|
| 4-2: sign-in, profiles | G4 (X sign-in, take a call, append-only call log, rate limits, deny by default) | This design and ADR 0041 being read (AGENTS §2); a production X app (§7) for anything beyond test config |
| 4-3: evidence submissions, discussion | G4 (the same append-only write surface, extended to evidence and discussion rows) | 4-2's sign-in, since a submission needs a signed-in player |
| 4-4: a research assistant that proposes forecasts the user confirms | Not in design 0028's G-steps — a new surface on top of G4's write path | 4-2, 4-3; the assistant proposes, never submits — the user confirms every forecast it drafts (AGENTS §3 rule 2, "the model may not introduce a fact"; design 0029 §4, "proposes a forecast the user confirms") |
| 4-5: immutable forecasts on the daily five (five launches, six-hour window, fourteen-day horizon, hidden until close, explicit unresolved outcome) | G4 (the call itself) and G5 (settling it on the observation jobs) | design 0028's daily five already existing (G1–G3, done — design 0028 §10 "as built"); PR #141's install, for G5 (§7) |
| 4-6: separate contribution and forecasting reputation, no value | G6 (board endpoints beside `/v1/public/hunters`) | 4-5's settled forecasts, since a reputation figure needs settled rows to compute a sample size from |
| 4-7: server-side authenticated submission and public round/outcome/profile reads, every record carrying chain, token address, timestamps, evidence references and rule versions | G4 (writes) and G6 (public reads) together | 4-2 through 4-6, since this unit is the store's shape (§2, §3) applied across every record kind above |

`docs/design/README.md` and `docs/adr/README.md` do not exist in this
repository today, so no index entry was added for either document.
