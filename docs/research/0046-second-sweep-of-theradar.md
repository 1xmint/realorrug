<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0046 — Second sweep of theradar

**Date:** 2026-09-15
**Status:** measured against theradar's own code and docs, read read-only via
`gh api` (no clone), and against this repository's own files, read directly.
`gh api rate_limit` was checked before starting (`core: 5000/5000 remaining`,
`search: 30/30 remaining`, read 2026-09-15) and no rate limit was hit this
pass, so no retry was needed. Radar ADR = a citation of `1xmint/theradar`'s own
record (AGENTS.md §4), linking github.com/1xmint/theradar. Nothing from
theradar is copied into this repository; every row below re-derives the idea
and cites where a number would need re-measuring on Pons v2, per ADR 0026.

**Reading the paths below:** a code span written `theradar:path/to/file` names
a file in [theradar](https://github.com/1xmint/theradar), not in this
repository. Everything without that prefix is a path here.

## 1. Closing 0042's three open items

### (a) Has the 0007 creator-history pivot-slot study been re-run on Robinhood Chain / Pons v2 data?

**No — not established, and this repo's own document (0038) says so plainly.**

- `docs/research/0038-pons-v2-creators-and-outcomes-read-over-a-range.md` is
  **not** the re-run. It is the *precursor* — it establishes which fields
  exist and how to read them, and its own "Not established" section (§7)
  lists exactly what 0007's method needs and does not have:
  *"A real distribution of launch-to-graduation gaps, trade counts, or any
  other quantity needed to set defensible `instant`/`organic`/`stillborn`
  thresholds for Pons v2 — this document proposes an analogue of pump.fun's
  shape... without calibrating it"* and *"A current graduation rate (only
  0036's day-old 1.2% sample is on file)"* (0038 §7, read 2026-09-15).
  0007's method needs a population large enough to split into non-overlapping
  Wilson intervals by prior-graduation band, with a pivot-slot no-lookahead
  join; 0038 has neither a calibrated instant/organic split nor a
  graduation-rate distribution — it has a single 1.2%, one-day-old sample
  carried over from research 0036, explicitly flagged as likely an undercount.
- `crates/realorrug-roast/src/creator.rs`, the code that would consume such a
  study, is unchanged since 0042: its `Record`/`CreatorIndex` types still key
  on `"Creator address, base58"` (line: `pub fn get(&self, creator: &str)`,
  doc comment "base58") and a `watermark_slot: u64` — Solana vocabulary, not
  Pons v2's `0x`-address/block-number shape. Its module doc still cites
  "117,390 creators, watched since August" as the population it reads, which
  is Radar's Solana index, not a Robinhood Chain one. No Pons v2 creator-index
  file or build path exists in `crates/` (confirmed by `grep`: no `creator`
  hits outside `realorrug-roast/src/creator.rs` and the pump.fun-shaped code
  it was ported from).
- **Verdict: still open, not answered — but for a stated reason, not silently.**
  0038 is the groundwork the study would need (creator/fee-recipient
  identification, phase-word decoding), not the study itself. ADR 0026's
  amendment ("no Solana creator index is ever built... the only creator index
  is realorrug's own, from Pons v2 launches") means when this does get built it
  replaces `creator.rs` rather than re-running 0007 against the old Solana
  index — so the honest framing is: **0007's method has not been re-run on any
  chain this repository trades on, and the index it would feed does not exist
  yet either.**

### (b) Was "trades-to-depth" / liquidity velocity ever built in either repo?

**Built in realorrug, but only for Solana/pump.fun — not for Pons v2. Not
built for either chain in theradar as a *named* "trades-to-depth" instrument,
but theradar independently has the same underlying capacity measurement under
a different name, and realorrug already ported it.**

- 0042 missed this because it only read `realorrug-onchain/src/reserves.rs`.
  The actual capacity math lives elsewhere: `crates/realorrug-pumpfun/src/curve.rs::buy_within_impact`
  (`pub fn buy_within_impact(&self, max_bps: u32, ceiling_lamports: u64) -> Option<u64>`,
  line 260) computes "the largest buy whose price impact stays within
  `max_bps`" directly off the bonding curve's own `x·y=k` reserves, and
  `crates/realorrug-onchain/src/dossier.rs` calls it at
  `CAPACITY_IMPACT_BPS: u32 = 100` (1%, line 40) to fill
  `Dossier::capacity_lamports`. This **is** the "1% price-impact budget
  against the curve's own reserves" signal 0042's table described for
  Radar's `theradar:radar-onchain/src/reserves.rs`/`budget.rs` — it is already ported,
  under a different filename split (`curve.rs` + `dossier.rs`, not
  `reserves.rs`), which is why the first sweep missed it.
- `crates/realorrug-pumpfun/src/curve.rs`'s own doc comment cites Radar's
  research directly: *"[0022](https://github.com/hey-vera/radar/blob/main/docs/research/0022-capacity-was-a-budget-not-a-ceiling.md)
  says so where it uses these numbers"* (line 24) — confirming this module
  is the intentional port of that research. (Note: this citation names
  `hey-vera/radar`, not `1xmint/theradar`; the two names refer to the same
  project at different points — flagged under "Not established" below since
  which is canonical was not resolved this pass.)
- **This is Solana/pump.fun-only.** `crates/realorrug-robinhood/src/pons.rs`
  (Pons v2, the EVM side) has no `impact`, `capacity`, `reserve` or `depth`
  logic at all (`grep -n -i "impact\|capacity\|reserve\|depth"` returns only
  unrelated `Vec::with_capacity` and the graduation-threshold field name). So
  the signal exists in this repo, cheaply portable in concept (Pons v2's
  bonding curve is also constant-product-shaped per research 0036), but has
  not been re-derived or re-implemented for Robinhood Chain.
- **A genuinely new gap, found this pass, adjacent to this question:** Radar's
  own [research 0034](https://github.com/1xmint/theradar/blob/main/docs/research/0034-effective-quote-reserves-are-real-and-the-raw-balance-is-half-the-answer.md)
  (dated 2026-09-10, five days before this sweep) established that a
  **post-graduation** PumpSwap pool's true tradeable depth is
  `quote_vault_balance + virtual_quote_reserves`, not the raw vault balance —
  *"Pricing off the raw vault balance alone is wrong by a factor of 2.1 on
  the pool tested"* — confirmed to one part in thirty million against two
  real mainnet trades. `realorrug-onchain/src/reserves.rs`'s own doc comment
  still says: *"research 0033 read the second term without establishing its
  meaning"* and lists under "What is not here": *"no effective reserve"*
  (lines 41–45) — i.e. **realorrug's post-graduation reserve reader is stale
  relative to Radar's own most recent correction**, and any exit-capacity
  number computed off it today for a graduated pump.fun token would
  undercount by roughly half. This affects the AMM side only; the pre-
  graduation bonding-curve capacity (`buy_within_impact`) is unaffected,
  since `BondingCurve` already carries its own `virtual_sol_reserves` field.

### (c) Are `baserates.rs`'s numbers pump.fun-measured or re-derived for Pons v2?

**pump.fun-measured (Solana), not re-derived. Confirmed from the data file's
own header, not inferred from the filename.**

- `crates/realorrug-roast/src/baserates.rs::DEFAULT_PATH` points at
  `"docs/research/data/0024-base-rates.json"` (line 29).
- That file's own `_comment` array (read directly, 2026-09-15) says:
  *"Base rates the public analyst reads instead of recomputing. Emitted by
  research 0024; see docs/research/0024-the-spike-became-a-hump-and-the-
  signal-moved.md for method and caveats, and docs/research/queries/0024-
  launch-block-recipients.sql for the query."* and *"EVERY FIGURE HERE HAS A
  DATE ON IT... 0008 measured the same quantities on 2026-08-25 and its
  headline was wrong by 2.7x nine days later."*
- The body: `"measured_on": "2026-09-03"`, `"supersedes": "0008"`,
  `"store": {"launches": 483629, ..., "watermark_slot": 444006292}`. A
  `watermark_slot` is Solana vocabulary (Robinhood Chain uses block numbers,
  confirmed in research 0038's own reads, e.g. block 27,027,321). 483,629
  launches at a Solana slot watermark is Radar's own pump.fun population, the
  same one research 0024 measured and 0042 already identified as a Solana
  study.
- **So: confirmed pump.fun/Solana-measured, unchanged since 0042, and not
  re-derived for Pons v2.** `baserates.rs`'s own module doc already warns
  against exactly this failure mode ("A consumer that hard-codes any of these
  numbers is repeating that [mistake]"), which is a warning this repository
  has not yet acted on for the chain it actually operates the bot on.

## 2. New from the sweep (theradar, crate-by-crate and doc-by-doc)

theradar's `crates/` (all 23 listed and walked by filename;
detection-shaped ones opened): `radar-agent`, `radar-asof`, `radar-backfill`,
`radar-cli`, `radar-customer`, `radar-decode`, `radar-exec`,
`radar-graph`(0042), `radar-instruments`, `radar-journal`, `radar-model`,
`radar-onchain`(0042), `radar-provider`, `radar-pumpfun`, `radar-research`,
`radar-risk`, `radar-serve`, `radar-signer`, `radar-sim`, `radar-store`,
`radar-strategy`, `radar-types`, `repo-conformance`. The ones 0042 never
opened at all — `radar-instruments`, `radar-risk`, `radar-strategy`,
`radar-sim` — held everything new below. The pure-infrastructure crates
(`radar-agent`, `radar-asof`, `radar-backfill`, `radar-customer`,
`radar-decode`, `radar-exec`, `radar-journal`, `radar-model`, `radar-provider`,
`radar-serve`, `radar-signer`, `radar-store`, `radar-types`) were confirmed by
file name only (auth, HTTP serving, signing, event journal, cost tracking,
type primitives, decoders) to hold no detection/scoring/calibration/refusal
logic — see "Not established" for the limit of that check.

| signal | where it lives (Radar ADR) | what it measures | how it decides | Solana-only or portable | do we have it |
|---|---|---|---|---|---|
| **Creator launch-cadence threshold** | `theradar:crates/radar-strategy/src/creator_edge.rs::Thresholds::max_launches_per_day`; measured in [research 0007](https://github.com/1xmint/theradar/blob/main/docs/research/0007-does-creator-history-predict-anything.md) | how many launches per day a creator submits, as a separate axis from their organic-graduation rate | measured, not assumed: *"`docs/research/0007` split 638 creators at candidate cuts of 10, 15, 20, 30, 50 and 100 prior launches; the first four separated the quieter and busier populations at 95% and the last two did not."* Ten launches/day is the chosen cut, inside the separating range but not one of the tested points on purpose ("insensitivity is the argument for the number, not the fit") | **fully portable** — needs only a creator address and a launch timestamp per launch, both of which `TokenLaunched` on Pons v2 already carries. No Solana-specific mechanic | **no.** `realorrug-roast/src/creator.rs::Record` has no per-day rate or timestamp span field at all — this is a second, independent finding from the same 0007 study that 0042's table did not mention (it only covered the organic-graduation-band half) |
| **`creator_history` instrument** (raw activity, no outcome) | `theradar:crates/radar-instruments/src/creator_history.rs` | per creator: `distinct_symbols`, `duplicate_metadata_launches` (relaunching the same name/symbol/URI), `max_launches_in_one_slot` (parallel submission), `failed_launches`, `launches_per_hour` | not a threshold — a structured report, deliberately: *"It reports; it does not judge. There is no 'spam' flag here, because whether a launch rate predicts anything is a question for the research store to answer against outcomes, not for this instrument to assume."* `duplicate_metadata_launches` corresponds to design 0010 §7.2 candidate **h ("repeated metadata")**, which 0042's table did not cover and which is already *built* here, not just proposed | **portable.** Creator identity, name/symbol/URI and per-block launch counts are all available from Pons v2's `TokenLaunched` event and the ERC-20's own constructor args, no Solana mechanic needed | **no.** None of `duplicate_metadata_launches`, `max_launches_in_one_slot`, `failed_launches` or `launches_per_hour` exist anywhere in `crates/` (grepped for the field names directly; zero hits) |
| **Launch-block contiguity** (candidate c, distinct from the recipient-count bundle signal) | design 0010 §7.2 row c: *"the block's signatures in order; the mint's launch-slot signatures mapped to positions; the longest run of consecutive positions."* Status there: **"build, fixture first"** — proposed, not confirmed built in the crates read this pass | whether the token's own launch-slot buys sit in one unbroken run of block positions, as a second, independent shape-test for an atomic (Jito) bundle, alongside the recipient-count signal 0042 already covered | a longest-consecutive-run length, calibrated against the store the same way 0008/0024 calibrated the recipient count (not yet done per the design doc's own "build" verdict) | **portable in concept, needs re-deriving on EVM.** A Jito bundle is sequential and atomic inside one Solana slot; an EVM block's transaction order is also deterministic per-block, so "longest run of consecutive tx positions touching this token" ports as an idea, but there is no Jito-equivalent "bundle" primitive to validate it against on Robinhood Chain — this needs its own calibration study before it means anything there | **no, and neither does theradar** (design doc marks it unbuilt) — a shared gap, not something left behind, but worth naming since it is a *different* measurement from the one signal 0042 already covered |
| **`simulate_exit` / `can_be_stopped`, `can_be_diluted`** (live instrument, superset of the honeypot row) | `theradar:crates/radar-instruments/src/simulate_exit.rs`; its structural half is `theradar:crates/radar-sim/src/mint.rs`'s Token-2022 `Extension` enum (`TransferFeeConfig`, `MintCloseAuthority`, `DefaultAccountState`, `NonTransferable`, `PermanentDelegate`, `TransferHook`, `Pausable`, plus `Unknown{id}` for anything not recognised) | `exitable: bool`, `can_be_stopped: bool` (a third party can freeze, hook, tax or seize), `can_be_diluted: bool` (issuer can still mint), `structural_threats: Vec<String>`, plus a real quote-ladder capacity (`no_route_at`, largest exit per impact budget) from a live router (Jupiter), not curve arithmetic | `can_be_stopped`/`can_be_diluted` are **decisive on their own** in `theradar:radar-strategy/src/avoidance.rs::PassReason` — *"A good quote on an exit that a third party can cancel is not a good price, it is a story about one."* This is a structured decision type, not a bare authority-flag read | **portable as a concept, not as code** — same conclusion 0042 already reached for the mint/freeze-authority half, but this instrument is the *fuller*, wired-up version: a typed `can_be_stopped`/`can_be_diluted` pair plus a `structural_threats` list, versus realorrug having neither the authority read nor a typed decision around it. EVM equivalent: contract bytecode/ABI check for owner-only pause/blacklist/mint functions (0041 already names GoPlus/De.Fi doing this) | **no.** `crates/realorrug-onchain` has no mint/freeze-authority reader at all (confirmed again this pass), so it is missing both the raw fact 0042 named and the typed decision wrapper this sweep found |
| **Effective quote reserves (PumpSwap)** | [research 0034](https://github.com/1xmint/theradar/blob/main/docs/research/0034-effective-quote-reserves-are-real-and-the-raw-balance-is-half-the-answer.md) | true tradeable depth of a graduated pump.fun/PumpSwap pool: `quote_vault_balance + virtual_quote_reserves`, not the raw balance | measured against two real mainnet trades to 1 part in 30,000,000; raw balance alone is wrong by a factor of 2.1x on the pool tested | **Solana/PumpSwap-specific formula**, but the *idea* — "a curve's stated reserve is not necessarily its full tradeable depth; check for a virtual/hidden component before quoting capacity" — is portable, and whether Pons v2's curve has an analogous virtual-reserve term was not checked in either research 0036 or 0038 this session or last | **no, and this is a regression relative to Radar's own most recent state**: `realorrug-onchain/src/reserves.rs`'s doc comment still describes the pre-0034 state of knowledge ("0033 read the second term without establishing its meaning") five days after Radar itself resolved it. This is the clearest concrete "we ported the code before the last correction landed" gap found this pass |
| **The trading-strategy negative results** (0014, 0017, 0020) | `theradar:docs/research/0014-the-control-was-entirely-tokens-nobody-could-sell.md`, `theradar:docs/research/0017-a-control-that-could-have-been-traded.md`, `theradar:docs/research/0020-the-exit-rule-question-cannot-be-answered-here.md` | whether Radar's own selection strategy (bundle + creator-edge + exit-capacity, combined into `radar-strategy`'s buy/refuse decision) beats holding or a random control | **measured, and negative, repeatedly.** 0017 (re-measured): *"the edges are now +22, −337, 0 and 0 bps across the four strata — a median edge of 0 bps, and still no edge."* 0020: *"No rule beats the baseline, on the pessimistic bound or the optimistic one."* 0014's corrected reading: *"the selection's gross median is negative."* | n/a — this is a negative result about Radar's own *trading* strategy, not a per-token detector | **realorrug does not trade at all** (AGENTS.md rule 1: "Model judgement never moves money"), so this does not port as code. It ports as a **caution**: the individual structural signals (bundle, repeat-launcher, creator history) show real statistical enrichment in isolation, but Radar's own measurement found that combining them into a *buy/refuse* decision produced no edge over holding. That is direct, first-party evidence for ADR 0027 rule 4's "a single signal never reaches the top two [verdict] levels" and for treating any future "verdict from a weighted combination of these signals" proposal with the same suspicion this repository already applies to a composite score |

**Innocent twins, per ADR 0027 rule 4, for the new signals proposed above as portable:**

- **Launch-cadence threshold** — innocent twin: a legitimate multi-project
  studio or a memecoin factory account that launches many *good* tokens
  quickly; the signal is a measured correlation with lower per-launch
  graduation rate (0007), not evidence any specific fast launch is bad.
  Expected false positives: any high-volume, low-malice launcher (an
  exchange's or influencer's token-generator bot) reads the same as a spam
  wallet on this signal alone.
- **`duplicate_metadata_launches`** — innocent twin: a creator relaunching
  after a failed transaction, a rebrand, or a deliberate "v2" of their own
  earlier token; the instrument itself refuses to call this "spam" for this
  reason (its own doc comment, quoted above).
- **Launch-block contiguity** — innocent twin: a busy launch slot where an
  unrelated token's buys happen to land in adjacent positions by chance, not
  by bundle; design 0010 itself requires a fixture-first capture before this
  is trusted for exactly this reason.
- **`can_be_stopped`/`can_be_diluted`** — innocent twin: many legitimate
  tokens ship a pausable or mintable contract for genuine operational reasons
  (a bug-fix pause switch, a vesting mint schedule) with no intent to rug;
  the instrument's own framing treats the *capability* as decisive regardless
  of intent, which is defensible for an exit-capacity question ("can you get
  out") but would be an over-claim if stated as "this creator intends to rug."
- **Effective-reserves correction** — not a detection signal, a pricing
  correction; no innocent twin applies.

## 3. Updated port order

**Changed from 0042's list.** 0042's five items stand, but two things move:

1. **Repeat-launcher / funding-cluster proxy** — unchanged, still first.
   Cheapest, twice-measured, nothing built here (0042).
2. **NEW: the `creator_history` raw-activity fields** (`duplicate_metadata_launches`,
   `max_launches_in_one_slot`, `failed_launches`, `launches_per_hour`),
   inserted here because they are the same shape of win as item 1 —
   structurally verifiable on-chain, zero-cost (a store/log read, no paid
   call), and this repo has none of them. They are a natural pair with
   `creator.rs`'s existing outcome counts: one reports what a creator *did*,
   the other what happened *next*, and `creator_history` is the half
   realorrug is missing entirely.
3. **The calibration/drift monitor** — unchanged from 0042, item 2 there.
4. **Launch-block bundle scoring as a first-class type** — unchanged from
   0042, item 3 there.
5. **NEW, ahead of the creator-history re-run: fix `reserves.rs`'s stale
   effective-reserves formula** (research 0034), inserted above the 0007
   re-run because it is a *correction to a signal realorrug already has
   partially built* (exit capacity for graduated pump.fun tokens), it is
   fully specified and measured by Radar already, and it is currently wrong
   by a factor of ~2x on every graduated-pool capacity number the bot could
   quote — a correctness bug hiding in a shipped module outranks a new study.
6. **Re-run the creator-history study (0007's pivot-slot method) against
   Pons v2 data** — unchanged from 0042, item 4 there, now item 6.
7. **Post-launch (rolling) bundle detection** — unchanged from 0042, last on
   purpose (even Radar never finished it).

**Explicitly still not recommended to port**, extending 0042's list: **the
`radar-risk`/`radar-strategy` trading-authorization kernel and position-sizing
logic** (`theradar:radar-risk/src/kernel.rs`, `theradar:radar-strategy/src/avoidance.rs`,
`theradar:crates/radar-strategy/src/creator_edge.rs`'s sizing thresholds). This is Radar's own autonomous-trader
architecture, gated by design 0017/0018's frozen experiment, and its own
research (0014, 0017, 0020, table above) found no trading edge from it. It is
also categorically out of scope: realorrug never trades (AGENTS.md rule 1),
so there is no "proposal" for a risk kernel to authorise. The *ideas* worth
keeping from it — a typed distinction between structural and evidentiary
refusal reasons (`PassReason::is_structural()`), and separate staleness
budgets for a token reading versus a creator record — are worth remembering
for design 0020's fact-sheet build (ADR 0027's `CantTell` verdict needs
exactly this kind of "why can't we say" typing), but that is a design
borrowing, not a code port, and is not added as a numbered port-order item
here since it names no file to copy from.

## 4. Not established

- **`hey-vera/radar` vs `1xmint/theradar`**: `curve.rs`'s own doc comment
  cites `github.com/hey-vera/radar` for research 0016 and 0022, not
  `1xmint/theradar`. Whether these are the same repository under a prior
  organisation name, a fork, or a genuinely different project was not
  resolved this pass — AGENTS.md §4 says cite theradar as `1xmint/theradar`,
  and this document has done so throughout, but the discrepancy in
  realorrug's own existing code comment is flagged rather than silently
  normalised.
- **The pure-infrastructure crates** (`radar-agent`, `radar-asof`,
  `radar-backfill`, `radar-customer`, `radar-decode`, `radar-exec`,
  `radar-journal`, `radar-model`, `radar-provider`, `radar-serve`,
  `radar-signer`, `radar-store`, `radar-types`) were walked by directory
  listing (file names only) and judged to hold no detection logic from their
  names and module purpose (auth/session, HTTP serving, signing, event
  journal, cost/pricing plumbing, type primitives, instruction decoders,
  agent tool-calling). None were opened file-by-file. A signal hiding inside,
  say, `theradar:radar-backfill/src/outcomes.rs` or `theradar:radar-decode/src/pumpswap.rs`
  under an unexpected name would not have been caught by this pass.
- **`docs/research/` 0001–0006, 0009–0010, 0015, 0019, 0021, 0023, 0025–0033,
  0035** (theradar's own numbering) were not opened, only their titles read
  from the index. 0009 ("what a token actually does to your money") and 0016
  ("the entry was a bid and the exit was a mid") were referenced only via
  quotations already embedded in other documents read this pass, not read
  directly in full. A detector-shaped finding inside any of these titles
  that does not announce itself as one would not have been caught.
- **`docs/design/` 0001–0009, 0011–0017, 0019** were not opened beyond 0010
  and 0018 (both read for this pass) — titles only.
- **Whether Pons v2's bonding curve has an analogous "virtual reserve" term**
  the way pump.fun's does (surfaced by the research-0034 finding above): not
  checked against research 0036 or 0038's captured data this session; would
  need a targeted re-read of `pons.rs`'s decoded curve fields against a real
  trade, the same method 0034 used.
- **`gh search code`** was not used this pass (directory-by-directory `gh api`
  listing was used instead, per the packet's instruction that search coverage
  is not proof of absence) — so a signal that exists as a *reference* inside
  a file whose containing directory was listed but not opened (the
  infrastructure crates above) would not surface either way.

## Confidence and what would change it

**CHECKED**, not PROVED, for the same reason 0042 was CHECKED: every claim
above is sourced to a specific theradar path, research note, or design-doc
line read directly via `gh api` (raw content), or to a specific line in this
repository's own files, with the "Not established" section above naming what
was not opened. Items (a) and (c) are as settled as a single-session read can
make them — a file header and a struct's field types are primary evidence,
not inference. Item (b) is settled for "is trades-to-depth built" (yes, for
Solana; no, for Pons v2) but the research-0034 finding attached to it is new
information this pass produced rather than answered a question 0042 asked;
what would change it is checking whether Pons v2's curve needs the same
effective-reserve correction, which needs a live trade capture against
`pons.rs`, not a documents-only read. What would change the sweep table: the
unopened `docs/research/`/`docs/design/` titles and the file-name-only
infrastructure crates above, opened in full.
