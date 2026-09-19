<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0054 — a paid API over x402: current protocol state, fit with this repo's record, and the Postgres question

**Date:** 2026-09-19.
**Status:** read-only research, gathering facts for a direction question
(AGENTS.md §2). The owner is weighing whether realorrug becomes "a real tool
that people buy with x402, which also happens to be an X bot." Nothing here
decides that; it answers the questions the packet asked so the conversation
can happen with numbers in front of it. CHECKED where the claim is a
file:line in this tree read today, or a web page fetched today and quoted
with its date. UNVERIFIED where a finding rests only on a search-engine
summary that could not be fetched directly, or on a third-party page whose
authority is itself in question (flagged inline). Nothing here changes code.
**Reserved ahead of this number:** research 0052's build plan (§8, row
M-D-0006) reserves "a new research 0053" for the calibration replay; 0053 is
still unused, so this document takes 0054, the next free number after it.

## 1. x402 today

### 1.1 What it is, and who governs it

"x402 is an open, neutral standard for internet-native payments" that
integrates a payment step into the HTTP `402 Payment Required` status code
(x402.org, fetched 2026-09-19). Coinbase created and open-sourced it in May
2025; a foundation launched by Coinbase and Cloudflare in 2025 now governs
it, with members including Google, Visa, AWS, Circle, Anthropic and Vercel
(search summary of Wavect's 2026 comparison and Coinbase's own newsroom
content, both dated 2026, read 2026-09-19 — **UNVERIFIED against the
foundation's own page**, not fetched directly this session).

No page fetched this session states a single spec version number. The
GitHub repository `coinbase/x402` (fetched 2026-09-19) exposes per-scheme
spec files under `specs/schemes/exact/` (e.g.
`scheme_exact_evm.md`) rather than a top-level version string in the content
retrieved; a version claim would need a direct read of those files or of a
CHANGELOG, which this pass did not fetch. **Treat "current spec version" as
open** until that file is read.

### 1.2 Facilitators and fees

Coinbase runs a free public facilitator via its CDP for Base, Solana and
Stellar, and started charging **$0.001 per transaction for its own
facilitator from 2026-01-01, with the first 1,000 transactions per month
free** (search summary of Phemex's 2026 news item, dated 2026, read
2026-09-19 — **UNVERIFIED**, not fetched from Coinbase's own pricing page
directly). Stripe separately lists **1.5% per successful charge** for its
own x402 support on Base (same summary, same caveat). Other facilitators
exist beyond Coinbase's; the x402 Solana docs describe a facilitator role
(verify + settle) that "anyone" can run (solana.com/docs/tools/x402-facilitator,
per search summary read 2026-09-19).

### 1.3 Networks and assets

x402.org's own overview (fetched 2026-09-19): "blockchain-agnostic and
supports all EVM-compatible chains, Solana, and more. Stablecoin payments
are the primary use case." A search summary (Wavect, 2026, read 2026-09-19)
adds Stellar, Arbitrum, Polygon and Ethereum mainnet as live networks, with
Base and Solana most active. The protocol supports any ERC-20 via Permit2
and works natively with USDC and EURC via EIP-3009 for gasless transfers
(same summary).

**Is Robinhood Chain supported?** Robinhood Chain's own documentation page,
`docs.robinhood.com/chain/` (fetched 2026-09-19), **does not mention x402,
USDG, payments or facilitators anywhere in the content retrieved**; it
describes standard EVM tooling (Hardhat, Foundry, ethers.js, viem, Wagmi)
and lists Paxos (issuer of USDG) only as a stablecoin-infrastructure
ecosystem partner, not a payment rail. Everything this session found calling
itself "x402 on Robinhood Chain" is a third-party, unofficial project — repos
named `nirholas/robinhood-chain-x402`, `nirholas/loxley`,
`MadeOnSol/robinhood-chain-x402`, sites `vledx402.tech`, `hood402.com`, and
an MCP listing citing "$0.04/call" for a community-run "10-endpoint x402
rail" — none of which is Robinhood's own domain or GitHub org (all read via
search summaries 2026-09-19, **UNVERIFIED, and treated as unreliable**: this
is exactly the shape of low-authority, SEO-styled crypto tooling AGENTS.md
§1 asks to be skeptical of — a reference proposes, it does not settle
anything, and none of these are a capture the chain accepted). **Finding:
no official Robinhood Chain x402 support exists as of 2026-09-19; only
community projects claim it, unverified and low-authority.** If x402 belongs
in this product, it settles on Base or Solana today, not Robinhood Chain,
until Robinhood's own docs say otherwise.

### 1.4 Settlement latency

Per-network, from search summaries read 2026-09-19 (not independently
verified against a benchmark run):
- **Solana:** confirmation in "~400ms," full verify+settle round trip "under
  2 seconds" (Solana Compass, PayAI blog summaries).
- **Base:** "sub-second" settlement (~750ms p50) is achievable with
  Flashblocks per PayAI's own blog, versus ~1.7s without it; PayAI's own
  post also names a real failure mode — "a sequencer restart, a failover, a
  reorg — a transaction that was preconfirmed never makes it into the sealed
  block."
This matches the reasoning already in this tree:
`crates/realorrug-onchain/src/rpc.rs:11` states "AGENTS.md rule 7 forbids
the x402 lane on the execution path because on-chain settlement adds
400–800ms" (CHECKED, file:line, read 2026-09-19) — the range this document's
external search returned (400ms–1.7s depending on network and whether a
speed-up layer is used) is consistent with that number, not a correction to
it.

### 1.5 Minimum practical price per call, and refunds

x402 payments observed in the wild average **≈$0.20** across roughly 131,000
daily transactions and ≈$28,000 daily volume (CoinDesk, 2026-03-11, fetched
2026-09-19, quoting Artemis analysis: "The x402 'agent payments' boom is
still mostly a mirage" and "roughly half of observed x402 transactions
reflect artificial activity"). That $0.20 average is itself distorted by
wash/test traffic per that same article, so it is a weak floor, not a
target. **Refunds/failed-call handling:** x402 is designed as non-reversible
— "no chargeback path, no dispute window, no merchant pull-back" once a
facilitator confirms settlement (search summary of a PayAI/dev.to
discussion, read 2026-09-19). The protocol distinguishes rejected, expired,
pending, confirmed and failed settlement states, and practitioners report a
real failure mode: "if the balance decreases but the facilitator reports no
successful settlement, you have orphaned payments" (dev.to, "X402 Battle
Scars," read 2026-09-19, **UNVERIFIED** — a blog post, not the spec). **This
means a paid endpoint here would need to handle the case where a caller was
charged and the server failed to answer (RPC read failed, model call
failed, budget exhausted) by building its own refund path — x402 gives none
for free.**

### 1.6 Rust server-side support

This repo's HTTP stack is confirmed as **axum 0.8 + tokio**, in
`crates/realorrug-serve/Cargo.toml` (CHECKED, file read 2026-09-19):
`axum = { version = "0.8", ... features = ["http1", "json", "query",
"tokio"] }`. `realorrug-cli` and `realorrug-payout` use `ureq` for outbound
calls; `realorrug-payout` never depends on an HTTP server crate at all — it
is a CLI binary. Rust x402 crates exist and were found on crates.io/docs.rs
2026-09-19: `x402-rs` ("foundational data structures, protocol types, and a
reference facilitator implementation," with a runnable facilitator binary
and an `x402-axum-example`), `x402-sdk-solana-rust`,
`solana-foundation/x402-sdk` (Solana Foundation's own GitHub org — the one
name here with plausible first-party authority), `x402-kit`, `x402-sdk`,
and smaller community crates `r402`, `nginx-x402` (all via search summary,
**UNVERIFIED against each crate's actual README** beyond the docs.rs/crates.io
titles returned). Given this repo already runs axum, `x402-rs`'s axum
example is the natural fit to prototype against if a paid route is ever
built — worth a direct read of that crate's source before depending on it,
not done in this pass.

### 1.7 Who actually pays via x402 today

CoinDesk (2026-03-11, fetched 2026-09-19): "the merchants that x402 is
designed to serve are still rare," much of the traffic "reflect[s]
infrastructure testing and experimental use," and an Artemis analyst
concludes demand is "mostly a mirage" relative to a "$7 billion ecosystem
valuation." **The intended payer is an AI agent or autonomous script, not a
human clicking a link** — but as of that March 2026 read, genuine
agent-driven commerce volume is small and roughly half of observed activity
looks self-dealt or synthetic. This is the single most load-bearing
external fact for this direction question: **x402 today is a
protocol looking for real usage, not a proven revenue channel.** It is six
months old at time of this research (2026-09-19 vs. this article's
2026-03-11); no more recent usage-volume figure was fetched this session,
and a follow-up search for a September 2026 number would be the next thing
to check before betting the direction on it.

## 2. Fit with this repo's record

### 2.1 Existing x402 scaffolding already in this tree

x402 is not a new idea for this codebase — it is inherited language from
Radar, present today as naming convention and as a documented *non-goal* on
the execution path, not as a built paid-API surface:

- `crates/realorrug-onchain/src/rpc.rs:9-19` (CHECKED): "Direct RPC, never
  the x402 lane... This path exists to answer while a thread is still
  alive; a settlement round trip per call... would put the reply minutes
  late. Latency is the product here." This is about *outbound* RPC reads
  the bot itself makes, not about a paid API the bot could serve — it
  already settles that x402 must never sit on the reply-generation path.
- `crates/realorrug-model/src/lib.rs:23-30` and
  `crates/realorrug-model/src/codex.rs:56-61` (CHECKED): "`radar-serve`'s own
  environment holds an x402 payout address, a facilitator URL and — on the
  other path — a model key, and none of that is any business of a
  subprocess whose input is partly written by whoever named a token."
  `codex.rs`'s test uses the literal env var name `REALORRUG_X402_PAY_TO`
  (`codex.rs:471`) as a stand-in secret to prove the subprocess never
  inherits it. **This confirms the naming convention already treats an x402
  address as a receive-only "pay to" address, structurally separate from
  any signing key** — consistent with, not a violation of, AGENTS.md §3
  rule 1.
- `crates/realorrug-agent/src/lib.rs:76-78` (CHECKED): "the x402 surface
  returns 404 rather than serving free. This exists for the case where they
  [the routes] are [mounted]" — deny-by-default language (§3 rule 7)
  already anticipates an x402-gated route existing someday, unbuilt today.

**None of this is a built paid endpoint.** It is three places where a
future x402 surface was named and fenced off, written while this codebase
was still Radar. Building one now means filling in code behind an existing
naming convention, not inventing one.

### 2.2 Rules a paid API would touch

| rule | source | how a paid API touches it |
|---|---|---|
| Model judgement never moves money (§3 rule 1) | AGENTS.md | An x402 "pay to" address only *receives*; confirmed above it is already named and coded as receive-only, structurally apart from `realorrug-payout`'s signing key (`crates/realorrug-payout` depends on `realorrug-contest` and `realorrug-robinhood`, never touched by `realorrug-serve`'s or `realorrug-model`'s code — CHECKED via each crate's `Cargo.toml`, read 2026-09-19). **No path from x402 receipt to the payout key exists in the dependency graph read today.** Building the paid route inside `realorrug-serve` (already dependency-clean of `realorrug-payout`) keeps that true; building it inside `realorrug-cli` or anywhere `realorrug-payout` is a workspace dependency would need the same review this table is doing, again. |
| The model may not introduce a fact (§3 rule 2) | AGENTS.md | A paid reply is still built from `FactSheet` (`crates/realorrug-roast/src/sheet.rs`) and checked back against it after generation (sheet.rs's own doc comment, CHECKED: "the set of numbers reachable from here **is** the set of numbers that can be published"). Charging for the answer does not relax this: a paid reply that hallucinated a number would be the same violation, now sold. |
| A verdict is earned by facts the sheet holds, never accuses a person (§3 rule 4) | AGENTS.md, ADR 0032 | Selling a verdict does not change who computes it — `level()` in `crates/realorrug-roast/src/verdict.rs:126-163` (CHECKED, cited already by research 0052 §1) is still a pure function of the sheet, still the code's rung, not the model's. |
| Price is stated with its moment; a hint needs a measured rate (§3 rule 5, ADR 0033) | AGENTS.md | A sold reply about the project's own token, or any token, still needs its price stamped with a block/time and any 10x-style hint gated on a measured outcome rate — ADR 0033 §5 is explicit the project's own token gets no special handling. |
| Holdings are public, nothing trades (§3 rule 6, ADR 0029) | AGENTS.md | Selling API access is a new revenue line, distinct from the token holding and the creator-tax prize funding ADR 0029 already describes. It does not, by itself, create a new path to a trade — but if payment ever arrived *in* the project's own token rather than a stablecoin, that would need its own review against "nothing trades automatically." |
| Deny by default when config is missing (§3 rule 7) | AGENTS.md | Matches the existing pattern exactly: `realorrug-agent`'s 404-when-unmounted x402 comment (§2.1 above) is this rule, already written for this exact feature. |
| The published level stays on the flag rules until calibration (research 0052, ADR M-D-0006 hold) | research 0052 §8 | **This is the sharpest conflict.** Research 0052 §1 (CHECKED) describes `level()` as "a ladder of booleans, not a score" today, and its own score (`assessment.rs:177-191`) "does not decide anything" yet — the calibration replay (0052's row M-D-0006, reserving research "0053") is an *unbuilt* read-only measurement that would tell whether the hand-set weights beat a boolean ladder, gated on "≥200 per arm... bands monotone... Brier beats the boolean ladder... on both splits," and falls back to "the weight stays hand-set and the doc says so" if it fails. **Selling a per-call verdict today means selling an uncalibrated score** — the ladder's own author (research 0052) has not yet claimed the number means what a paid customer would assume it means. |

### 2.3 The legal/regulatory angle — a question for the owner's lawyer, not advice

Not answered here; flagged only, per the packet's instruction. This tree
already has a running thread of exactly this shape: ADR 0023 decision 5
keeps "the legal review runs in parallel" (CHECKED,
`docs/adr/0023-...md:61`, "Until a lawyer answers, the exposure is Josh's
personally. Stated once."), and ADR 0034 gates the daily-five prize payout
entirely on "the lawyer answers the question in design 0028 §7" before any
money moves (CHECKED, `docs/adr/0034-...md`). A paid x402 API is a new
instance of the same open question, not a new one: selling access to a
verdict about a token, in a stablecoin, from an account that also
(per ADR 0029) may hold a small amount of the project's own token and
comments on its price (per ADR 0033), sits in the same regulatory shape ADR
0013's "legal precondition" already named before the first post. Whether
receiving x402 payments changes the entity's obligations (money transmission,
securities-adjacent commentary, tax treatment of stablecoin receipts) is the
lawyer's question, not this document's.

## 3. What a paid response would contain

### 3.1 Measured facts already on the fact sheet (sellable as-is)

`crates/realorrug-roast/src/sheet.rs` and the onchain dossier types it reads
from (CHECKED, file read 2026-09-19) already carry, per token:

- `ChainLaunch`, `Holders`, `Funding`, `MarketSnapshot` (`realorrug-onchain`
  dossier fields imported at `sheet.rs:26-27`) — launch record, launch
  block, holder counts/shares, funding edges (who funded an early buyer
  before their purchase), price/market-cap/liquidity with its `ReadAt`
  moment (`About::Price` tag, `sheet.rs:40-58`, CHECKED).
- `Signal`s actually pushed today (research 0052 §1, CHECKED):
  `LaunchBlockInStrongestBand`, `CreatorBoughtOwnLaunch`,
  `CreatorNeverGraduatedOrganically`, `RepeatLauncher` — each a measured (M)
  fact with a file:line source, not an inference.
- The creator track record (design 0021 §2, "daily" shelf life) — prior
  launches, prior graduations, shared across every token that creator has
  touched.

Every one of these is `About::Measurement` or a block-stamped
`About::Price` fact: a number the sheet actually holds, checked back
against the reply after generation (sheet.rs's stated security boundary,
§2.2 above). **This is what is honestly sellable today: the raw measured
facts, each with its read point, exactly as a free reply already states
them.**

### 3.2 What is not yet sellable as a graded verdict

The five-word level (`Rugged`, `RugMechanicsLive`, `Sketchy`,
`NothingUglyYet`, `CantTell`) and the weighted score research 0052 is
building are **not yet calibrated** (§2.2 table, row 6). Selling "this
token is `Sketchy`, 62% confidence" today would be selling a number research
0052 itself says is unproven against outcomes — the paid product's honest
framing, until the calibration replay lands, is "here are the measured
facts a rug-check should look at," not "here is the calibrated risk of a
rug," even though the free reply already states the level.

### 3.3 Free X reply versus paid response

The packet asks what the free reply should keep versus hold back. Nothing
in this tree's rules supports holding back a *fact* on the free lane while
selling it on the paid one — AGENTS.md §3 rule 2 and rule 4 apply to every
reply, free or paid, and ADR 0033 §5 states the project's own token (by
extension, no token) gets special handling. The distinction that fits the
existing rules is **depth and reach, not truth-withholding**: the free X
reply already gives the level and the headline facts (design 0027's
salience ranking decides what leads); a paid response could reasonably give
the *entire* fact sheet — every measured fact, not just the ones salience
picked to lead with — plus the raw creator-history and funding-edge detail
that a 280-character reply has no room for. That is a product decision
(more data, not gated data), not a rule this research settles.

## 4. Postgres versus the current SQLite memory

### 4.1 When Postgres would actually be needed

`crates/realorrug-onchain/src/memory.rs` (CHECKED, file read 2026-09-19) is
SQLite, one file on disk, chosen in design 0021 §4 for a specific reason
that still holds: "this runs as a daemon on one small box and must survive
a restart... SQLite... needs no server process, no network port and no
second thing to keep running on a two-core box." That reasoning is about
**one process, one writer.** A paid x402 API changes the shape only if it
becomes **a second, separately-hosted process reading or writing the same
memory concurrently** — e.g., a public API host answering paid requests
while the X-bot daemon keeps polling and writing on its own box. SQLite's
single-writer file model (design 0021 §1, "atomic single-writer commits")
is the thing that stops holding once there are two writers on two hosts, or
even one writer and enough concurrent readers that file locking becomes the
bottleneck. **If the paid API stays inside the same daemon/process that
already owns `memory.sqlite3` (one box, one writer, the API answers reads
from the same in-process connection), Postgres is not needed yet — SQLite's
existing design already serves reads fine and design 0021's whole cost
argument (§5) was built assuming a single memory.** Postgres becomes the
right call the moment there is a **separate API host** from the daemon's
own box, because that is two processes wanting the same state without a
shared filesystem.

### 4.2 Managed-hosting prices, current as read 2026-09-19

| provider | free tier | cheapest paid tier | source, date |
|---|---|---|---|
| **Neon** | 100 CU-hours + 0.5 GB storage per project, up to 60,000 MAU, auto-suspend after 5 min idle | "Launch": pay-as-you-go, no monthly minimum — $0.106/CU-hour compute, $0.35/GB-month storage | neon.com/pricing, fetched 2026-09-19 |
| **Supabase** | 500 MB database, 50,000 MAU, 2 active projects, projects pause after 1 week idle | "Pro": $25/month flat, 8 GB storage included ($0.125/GB after), includes $10/month compute credit covering one Micro instance | supabase.com/pricing, fetched 2026-09-19 |
| **Railway** | none stated in what was fetched | "Hobby": $5/month, includes $5 of usage; a small always-on Postgres typically runs $10–$20/month total once compute + $0.15/GB-month volume storage are added, because a database never idles to zero | search summary of docs.railway.com/pricing and third-party trackers, read 2026-09-19, **UNVERIFIED against Railway's own docs page directly** — the $10–$20/month "always-on" estimate is a secondary source's arithmetic, not Railway's own stated price |
| **VPS (generic, not vendor-quoted this session)** | — | a small VPS (1 vCPU/1-2 GB) running self-managed Postgres typically runs $5–$12/month at common providers, but adds the operator's own backup, patching and uptime burden that a managed plan absorbs | not independently priced this session — flagged as the cheapest raw-dollar option but the highest operational-cost option, consistent with AGENTS.md's own preference for the smallest change that fully solves the problem, since self-managing a database is new maintenance surface this daemon does not have today |

### 4.3 Cheapest path that holds for the first months, and a recommendation

**Recommendation: do not add Postgres yet.** Nothing in this tree's design
docs describes a second, separately-hosted process reading the memory
concurrently — design 0021 §7 explicitly defers even a same-process
same-crate question (whether the checker page shares the daemon's read
budget) as "not established." Adding Postgres now would be solving a
concurrency problem that does not exist in the architecture as documented
today, at a real recurring cost (Supabase Pro $25/month, or Neon's
metered compute, versus SQLite's $0). **If and when a paid API is built as
a genuinely separate host** (its own box, its own uptime, so it cannot share
the daemon's SQLite file), **Neon's pay-as-you-go "Launch" tier is the
cheapest managed option that holds for the first months** at this project's
likely early volume (a handful of paid calls a day, well under 100
CU-hours), because it has no monthly minimum and its free tier alone may
cover the first weeks of testing before any real payment volume exists;
Supabase's flat $25/month only earns its cost once usage is steady enough
that its bundled auth/storage/edge-function features are actually used,
none of which this product needs yet. Railway sits between the two on
price and is the most awkward of the three to reason about here because its
own pricing page was not fetched directly this session — read that page
before choosing it.

## 5. Per-call cost floor

**Chain-read cost.** Research 0052 §7.1 (CHECKED, table read 2026-09-19)
measures roughly **500 CU of new reads per token on top of today's existing
dossier read**, all Alchemy compute units. Design 0021 §5 (CHECKED)
separately states the existing per-question read averages ("4 of a reply's
reads" refreshed at ~26 CU/`eth_call` and ~60 CU/`eth_getLogs`, inside the
existing 60-call-per-token ceiling `memory.rs`'s doc comment names, "the
existing sixty-call budget"). Worst case — 60 calls at the most expensive
60 CU rate — is 3,600 CU; at Alchemy's pay-as-you-go **$0.525 per 1,000,000
CU** (research 0039, re-confirmed by research 0052 §2, both CHECKED against
Alchemy's pricing page as of 2026-09-15), that is **≈$0.0019 per token
check, worst case** — a fraction of a cent. This is not the binding cost.

**Model-inference cost.** `crates/realorrug-model/src/codex.rs:50` (CHECKED)
names `NOMINAL_CALL: MicroUsd = MicroUsd(10_000)` — **$0.01** as the nominal
per-call charge on the subscription (Codex CLI) path, explicitly "not a
price" but a rate-limiting unit; the API-key path "computes a real cost from
real token counts" instead, not sized in this pass (no per-model $/token
rate was pulled from `realorrug-model/src/catalog.rs` this session).

**Facilitator fee.** Coinbase's own facilitator: $0.001/transaction after
the first 1,000/month free (search summary of Phemex, 2026, **UNVERIFIED**,
§1.2 above); Stripe's: 1.5% of the charge. A third-party (unofficial,
low-authority) Robinhood-Chain x402 rail quoted **$0.04/call** flat as its
whole price, not a fee on top of a smaller charge (§1.3, **UNVERIFIED and
of doubtful authority** — not used as a real cost input here, only noted
because the packet asked).

**Floor, stacked, worst case on Alchemy + Coinbase's paid facilitator:**
$0.0019 (chain reads) + $0.01 (nominal model call) + $0.001 (facilitator) ≈
**$0.013 per call.** A price with real margin over that floor — say
$0.05–$0.25, in line with the $0.20 average x402 payment size CoinDesk
reported market-wide (§1.5) — clears it several times over. **The real cost
floor is the model call, not the chain reads**, which is why the deciding
number for pricing is which model/provider actually answers a paid request
and its real $/token rate — not sized in this document, and the next thing
to check before setting a price.

## Recommendation and questions for the owner

**Recommendation:** the plumbing to receive x402 payments already exists as
a naming convention in this tree (`REALORRUG_X402_PAY_TO`, the 404-when-
unmounted pattern) and costs almost nothing to clear per call (§5), and the
dependency graph already keeps a receive-only address away from the payout
signing key (§2.2) — so building a paid route is cheap and structurally
safe. But two things argue for sequencing it *after*, not instead of, work
already in flight: the level/score a paid customer would be buying is
explicitly uncalibrated today (research 0052's own M-D-0006 hold, §2.2),
and the market this would sell into shows more infrastructure-testing
traffic than real agent commerce as of the most recent figure found
(CoinDesk, 2026-03-11, six months stale — worth re-checking before betting
on it). The cheapest true first product is **selling the measured fact
sheet, not a graded verdict**, over the existing SQLite-backed daemon with
no new database, at a price in the tens of cents that clears the ≈$0.013
worst-case floor several times over — and only reaching for Postgres the
day a paid API host is actually a second, separately-deployed process from
the X-bot daemon.

**Questions only the owner can answer:**
1. Does the lawyer's still-open review (ADR 0023 decision 5, ADR 0034's
   payout gate) need to clear *before* receiving x402 payments, or only
   before the daily-five prize pays out — i.e., is receiving stablecoin
   payment for an API answer a different regulatory question than paying a
   contest winner, in the owner's read of what he's already asked the
   lawyer?
2. Is the product being sold the *fact sheet* (measured, sellable today per
   §3.1) or the *verdict* (`Rugged`/`Sketchy`/etc., not yet calibrated per
   §2.2/§3.2) — because those are different products with different
   honesty obligations, and the packet's framing ("a real tool people buy")
   reads more like the latter than the former.

## Confidence and what would change it

**CHECKED** for everything sourced from this repository's own files
(Cargo.toml dependency graphs, sheet.rs, memory.rs, rpc.rs, codex.rs,
AGENTS.md, the ADRs and design docs cited, research 0039/0052's own tables)
and for the two pages fetched directly this session
(x402.org, docs.robinhood.com/chain/, neon.com/pricing, supabase.com/pricing,
github.com/coinbase/x402, coindesk.com's 2026-03-11 article). **UNVERIFIED**
for everything that came only as a search-engine summary and was not
independently fetched: the x402 spec version number, Coinbase's exact
current facilitator fee page, Railway's own pricing page, every claim about
an "official" Robinhood Chain x402 rail (actively doubted, §1.3), and any
model $/token rate for the actual inference cost floor in §5. **A second
search could not change §1.3's conclusion** (no official Robinhood Chain
x402 support was found, and the candidates found are low-authority) without
Robinhood publishing something new — that question is settled for now.
**What would change the recommendation:** a September-2026-dated x402 usage
figure showing real (not wash) agent commerce growing past the March 2026
figure; a direct read of `x402-rs`'s source to confirm it fits axum 0.8
cleanly; and the model $/token rate actually chosen for a paid route,
which sets the true cost floor in §5 more precisely than the $0.01 nominal
unit does.
