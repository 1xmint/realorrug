<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0042 — Detection intelligence we left in Radar

**Date:** 2026-09-15
**Status:** measured against theradar's own docs and code, read read-only via
`gh api` / `gh search code` (no clone, per the packet's boundary). Every row
below is sourced to a specific theradar path or research note; comparison
against realorrug is sourced to specific crate files in this repo. `gh search
code` hit GitHub's rate limit partway through (holder/creator/first_fill/rug
terms) and was retried successfully a few minutes later — no term went
unanswered, but say so per the "second search that could not change the
answer" standard: this is the CHECKED tier, not exhaustive grep of every file.


**Reading the paths below:** a code span written `theradar:path/to/file` names
a file in [theradar](https://github.com/1xmint/theradar), not in this
repository. Everything without that prefix is a path here.

## The table

Radar ADR = a citation of `1xmint/theradar`'s own record, per AGENTS.md's
convention (public repo, github.com/1xmint/theradar).

| signal | where it lives (Radar ADR) | what it measures | how it decides | Solana-only or portable | do we have it |
|---|---|---|---|---|---|
| **Launch-block bundle / coordination** | `theradar:crates/radar-graph/src/lib.rs`; measured in [research 0008](https://github.com/1xmint/theradar/blob/main/docs/research/0008-the-launch-block-gives-the-bundle-away.md), re-measured in [0024](https://github.com/1xmint/theradar/blob/main/docs/research/0024-the-spike-became-a-hump-and-the-signal-moved.md) | distinct token accounts (`recipients`) that received the mint inside its own launch block/slot | a fixed threshold: exactly 6 recipients = `Coordination::Likely` (actionable), 5–7 = `Suspected` (recorded, not actionable), else `Unremarkable`. **Not a formula or a model** — three measured buckets read off a histogram in `0008`, re-measured in `0024` | **portable in concept, Solana-specific in mechanism.** Needs "what happened inside the token's own creation block/tx." On Pons v2 that is: group all `Transfer` logs from the new ERC-20 in the block (or same-tx-bundle window) containing its `TokenLaunched`/create event, count distinct non-zero-balance recipients. EVM blocks are atomic across included txs the same way a Solana slot is, so the shape of the idea ports; the *measured threshold* (six) does not — it would need re-deriving from Pons v2 launches, exactly as `0024` had to re-derive it for Solana at 5x more data | **partly.** `realorrug-onchain/src/launch.rs::recipients_in` is the ported counting primitive (same function, same tests, down to the "same recipient, two transfers, no double count" case). But `realorrug-roast/src/baserates.rs::band_for` reports the **measured base rate for the token's exact recipient count** (arguably better than radar-graph's 3-bucket enum) rather than reusing radar-graph's `Coordination`/`assess()` scoring layer at all — there is no `realorrug-graph` crate. Missing: the **calibration/drift monitor** (`Calibration::{Silent,Elevated,Consistent}` in radar-graph) that watches whether the live rate at the centre has drifted from the measured one |
| **Repeat-launcher / funding-cluster proxy** | `theradar:crates/radar-graph/src/prevalence.rs`; measured in [research 0012](https://github.com/1xmint/theradar/blob/main/docs/research/0012-recipient-sets-cannot-recur-authorities-can.md) and [0013](https://github.com/1xmint/theradar/blob/main/docs/research/0013-a-repeat-launcher-in-the-block-predicts-a-deader-token.md) | how many distinct launch blocks a given transfer-signing wallet (`authority`) appears in, over a 90-minute window | three measured bands: `Ordinary` (1–2 appearances, ~89.5%/4.7% of wallets), `Repeat` (3–99, `REPEAT_FLOOR = 3`), `Infrastructure` (≥100, `INFRASTRUCTURE_FLOOR = 100`, excludes 13 router/fee-sink addresses covering 42% of launches). Repeat predicts ~2x likelier to be dead immediately and ~half as likely to reach 10 participants ([0013](https://github.com/1xmint/theradar/blob/main/docs/research/0013-a-repeat-launcher-in-the-block-predicts-a-deader-token.md)) | **more directly portable than the bundle signal, and arguably simpler on EVM.** The Solana version needs a wallet-vs-token-account join because a `destination` is a `(owner, mint)` pair and cannot recur across mints ([0012](https://github.com/1xmint/theradar/blob/main/docs/research/0012-recipient-sets-cannot-recur-authorities-can.md)) — the whole reason it scores the *signer*, not the recipient. On an EVM ERC-20, the `Transfer` event's sender/`msg.sender` (or the router's caller, if routed through Pons v2's curve contract) already recurs as a plain address across launches, no join needed. Needs: an index of "how many distinct launch blocks/txs has this address signed a buy in, over N minutes" — the head-cutting logic (excluding infra) would need re-deriving against Robinhood Chain's own router/relayer addresses | **no.** No `realorrug-graph` equivalent, no repeat-launcher or infrastructure-floor logic anywhere in `crates/`. This is the clearest gap: the signal exists, is measured, ported cheaply, and nothing in realorrug computes it |
| **Creator track record (`creator_edge`)** | measured in [research 0007](https://github.com/1xmint/theradar/blob/main/docs/research/0007-does-creator-history-predict-anything.md); index built by `radar-research::creator_index`, read by `theradar:crates/radar-agent`/`radar-cli`'s creator lookup (the reading half is the same shape `realorrug-roast/src/creator.rs` documents porting from) | a creator's organic-graduation count *before* a pivot slot, split so past and future outcomes never touch | a measured, monotonic base rate by band (0, 1, ≥2 prior organic graduations → 0.81% / 1.60% / 1.72% later organic rate, non-overlapping 95% Wilson intervals) with a launch-frequency control run in the same note. **A measured lookup table, not a formula** | **fully portable.** Needs only: a launch event carrying the creator address, and a graduation event, both of which Pons v2 emits (`TokenLaunched`, graduation event per the packet's own description). The pivot-slot / no-lookahead discipline is venue-agnostic methodology, not Solana mechanics | **yes, largely.** `realorrug-roast/src/creator.rs` is explicitly documented as the ported *reading* half of this exact design ("The same shape... a file with the date it was measured... no rates and no verdicts, only counts" — deliberately mirrors radar's split). realorrug already has the creator-index lookup; whether the *pivot-slot, no-lookahead* base-rate study (0007's method) has been re-run against Robinhood Chain/Pons v2 data was not checked here — that's a `docs/research` question, not a `crates/` one |
| **Post-launch (rolling) bundling** | `theradar:crates/radar-graph/src/ongoing.rs` (referenced in `docs/design/0010` candidate i: "`radar_graph::ongoing` with the CryptoHouse source; `radar consider` already calls it") | the strongest bundle-shaped block seen *after* the launch block, not just at it | same `assess()` scoring, applied to a rolling window instead of one fixed slot | same portability as the launch-block signal above; needs a rolling read of post-launch blocks rather than one fixed lookup | **no.** Radar's own design doc says the detector is written but "the rolling block source is not" ([GOAL.md](https://github.com/1xmint/theradar/blob/main/GOAL.md): "bundling detected *after* launch \| partly — the detector is written, the rolling block source is not") — so this was incomplete even in Radar. realorrug has neither half |
| **Exit capacity / trades-to-depth (liquidity, sizing not spoofing)** | `theradar:crates/radar-onchain/src/reserves.rs`, `theradar:crates/radar-onchain/src/budget.rs`; measured in [research 0018](https://github.com/1xmint/theradar/blob/main/docs/research/0018-the-deep-tail-points-the-wrong-way.md), [0022](https://github.com/1xmint/theradar/blob/main/docs/research/0022-capacity-was-a-budget-not-a-ceiling.md) | how much a position could exit for before moving the curve materially (measured against reserves, not liquidity-removal detection) | a 1% price-impact budget against the curve's own reserves; **this is a sizing measurement, not a fraud detector** — it does not flag liquidity spoofing or removal, it measures how much room exists to sell | **fully portable**, needs the bonding-curve contract's own reserve-reading function, which Pons v2 has by construction | `realorrug-onchain/src/reserves.rs` exists (same filename, ported); "trades-to-depth" as a *named candidate signal* (`docs/design/0010` §7.2 row d: "liquidity velocity," described there as **not yet built even in Radar** — "build first among the new facts") was not confirmed built in either repo from what was read |
| **Holder concentration outside the pool** | never built; candidate only. `docs/design/0010` §7.2 row b: "none exists; a stdlib probe... gives one" | share of supply held outside the curve/pool by the largest non-pool holders | proposed, not implemented: two RPC calls (`getTokenLargestAccounts`, `getTokenSupply`), no formula written yet | **portable**, EVM has the direct equivalent (`balanceOf` over a holder list, or a token-holders index) and no Solana-specific quirk | **no, and neither does Radar.** Nothing to port; this is a shared gap, not something we left behind |
| **Sniper wallets / bot counts** | never built. `theradar:GOAL.md`'s own table: "sniper counts, bot counts \| **do not exist**." The closest candidate, "trader prevalence (bot-shaped share)," is unbuilt (`docs/design/0010` §7.2 row e), and its own verdict warns: label it **"addresses that traded N other launches this hour," never "bots"** — the study's bot flag is "its own heuristic," stated as an assumption not a fact | would-be signal: how many distinct addresses are trading many other tokens' launches simultaneously (a bot-shaped access pattern) | not decided; the design note explicitly refuses "bots" as a public label until the money-side link is measured | portable in the same way prevalence.rs is (a repeat-participation count), but nothing was ever measured | **no, and neither does Radar.** Not a gap we created — it never existed upstream |
| **Wash trading detector** | never built. Only appears in `theradar:docs/research/vendor/chatgpt-radar-considerations.md` and `theradar:docs/research/vendor/chatgpt-radar-considerations2.md` — an outside LLM's brainstorm/wishlist document, item "# 28. WASH-TRADING DETECTOR," explicitly labelled a vendor/considerations doc, not Radar's own measured research | n/a — no implementation, no measurement | n/a | n/a | **no, and neither does Radar.** This is exactly the owner's worry (confident verdicts on weak signals) inverted: the *idea* exists in a third-party brainstorm doc that Radar itself never built or measured. Porting "wash trading detection" from theradar would mean porting an unimplemented ChatGPT suggestion, not a measured Radar capability |
| **Liquidity spoofing / removal detector** | never built. Same status as wash trading — appears only in the vendor brainstorm docs (`docs/research/vendor/chatgpt-radar-considerations*.md`), e.g. "spoofed market information." No radar-graph, radar-onchain, or radar-risk code implements it | n/a | n/a | n/a | **no, and neither does Radar** |
| **Honeypot / sell-blocking detector** | never built as a detector. `honeypot` appears in `theradar:crates/radar-roast/src/forbidden.rs` as a **forbidden phrase** the bot must never say (a word list, not a scan for sell-blocking bytecode), and in the vendor brainstorm docs as a suggestion ("Do not rely on classic Ethereum-style 'honeypot' concepts alone... Solana threats can be economic, wallet-coordinated, authority-based... without looking like a conventional honeypot"). Mint/freeze-authority and Token-2022 extension checks (`theradar:crates/radar-sim/src/mint.rs`, candidate a in `docs/design/0010` §7.2) are the closest *built* thing — they check for the authority state that would let a creator later block or drain, not runtime sell-blocking | mint/freeze authority: one `getAccountInfo`, checked against pump.fun's venue default (both revoked at creation) | mint/freeze authority is **portable as a concept, not as code**: Pons v2 is a plain ERC-20, so the EVM-native equivalent is checking the contract for owner-only mint/pause/blacklist functions and whether ownership was renounced (this is exactly what GoPlus and De.Fi do per [research 0041](0041-rules-or-a-reasoning-model-for-token-risk.md)) — Robinhood Chain would need reading the deployed bytecode or a verified-source ABI, not a Solana account flag | **no.** realorrug has no mint/freeze/ownership-authority check crate; `realorrug-roast/src/forbidden.rs` does carry the same "honeypot" forbidden-phrase entry (the *policy*, not detection) |
| **Composite / single risk score** | **explicitly refused**, by design, in both places | — | — | — | **Radar refuses this on purpose**, and realorrug should not read its absence as a gap. `docs/design/0010` §7.2 row j lists refused facts including *"a 'bundled %' as one number (a score)"* and *"a verdict word."* `theradar:GOAL.md`: *"A single safety score. Radar has fourteen reason codes and a structural split. A green shield is 'unknown rendered as safe'."* `AGENTS.md` rule 4 (inherited into realorrug's own `AGENTS.md`) forbids the bot calling anything a rug/scam/fraud. realorrug's own `forbidden.rs` and `verdict.rs` already carry this same discipline. **Nothing to port here — the absence of a composite score in both repos is the same deliberate decision, not a difference** |

## Not established

- Whether `radar-research::creator_index`'s pivot-slot, no-lookahead **study
  methodology** (0007) has itself been re-run against Robinhood Chain/Pons v2
  data, versus only the *reading* structure (`realorrug-roast/src/creator.rs`)
  having been ported. Answering this needs reading `docs/research/data/` and
  the creator-index build code, which the packet's boundary (read theradar,
  don't audit all of realorrug's research pipeline) put out of scope for this
  pass.
- Whether "trades-to-depth" (liquidity velocity, `docs/design/0010` §7.2 row
  d) has been built in *either* repo since that design doc was written —
  it was an open candidate there, not confirmed built or refused since.
- The exact current numeric thresholds in `realorrug-roast/src/baserates.rs`
  (i.e., whether its band table has been re-derived from Robinhood
  Chain/Pons v2 launches, or is still carrying pump.fun-measured numbers) —
  reading that file's data source was out of scope for a theradar-focused
  pass and belongs to a separate check against `docs/research/data/`.
- `gh search code`'s coverage is not exhaustive proof of absence: a signal
  that exists only in an unindexed binary fixture or a very recently pushed
  file could be missed. The GOAL.md capability table and `docs/design/0010`
  §7.2's candidate table are the stronger evidence for "does not exist" claims
  above, because they are theradar's own first-person audit of itself, not an
  inference from what a keyword search returned.

## What to port first

In order, cheapest-and-most-measured first:

1. **Repeat-launcher / funding-cluster proxy** (`theradar:radar-graph/src/prevalence.rs`,
   `REPEAT_FLOOR`/`INFRASTRUCTURE_FLOOR`). Measured twice on disjoint windows
   (0012, 0013), no wallet-to-token-account join needed on EVM (actually
   *simpler* to port than on Solana, per the table above), and realorrug has
   nothing like it today. Highest ratio of "already proven, cheap to port, we
   have zero of it."
2. **The calibration/drift monitor** (`radar-graph::Calibration`). Not a new
   signal — a check on the bundle signal realorrug already partially has
   (`baserates.rs::band_for`). Worth porting *because* Radar's own production
   status caught itself drifting undetected: `docs/design/0010`'s live status
   table shows coordination running at "2,916 bps of the last 600 launch
   blocks... against 580 bps calibrated... still firing to a journal nobody
   reads." A signal with no drift monitor is exactly the "confident verdict on
   a stale threshold" failure mode the owner is worried about.
3. **Launch-block bundle scoring as a first-class, tested type**
   (`radar-graph::Coordination`/`assess()`), even though realorrug already has
   the raw `recipients_in` count and a base-rate lookup — porting the
   `Unremarkable`/`Suspected`/`Likely` verdict type (or an equivalent) gives a
   typed, `is_actionable()`-gated distinction between "worth recording" and
   "worth refusing on," which `baserates.rs` alone does not appear to encode
   as cleanly.
4. **Re-run the creator-history study (0007's pivot-slot method) against
   Pons v2 data**, if it has not been — the reading structure is already
   ported, but a lookup table built on pump.fun's regime says nothing about
   Robinhood Chain's launches until it's re-derived there, same as 0024 had to
   re-derive the bundle threshold when the population grew 7x.
5. **Post-launch (rolling) bundle detection**, last on purpose: even Radar
   never finished it ("the detector is written, the rolling block source is
   not"). Porting an unfinished half-signal is lower value than the three
   complete, measured ones above it.

**Explicitly not recommended to port:** wash trading, liquidity spoofing, and
a composite/single risk score. The first two exist only as an outside
brainstorm document's suggestions, never measured or built by Radar itself —
porting them would mean inventing a new detector under someone else's name,
which is the opposite of this packet's premise. The third is refused by
design in both repos, and should stay refused.

## The owner's worry, in Radar's own words

The owner asked specifically about confident verdicts on weak signals. Radar's
own record has three separate, explicit instances of exactly that going wrong,
worth carrying forward as a caution rather than a feature to port:

- **A published threshold turned out wrong on more data.** [Research
  0024](https://github.com/1xmint/theradar/blob/main/docs/research/0024-the-spike-became-a-hump-and-the-signal-moved.md),
  re-measuring 0008 at 7x the data: *"0008's headline does not survive. The
  enrichment at six is real and about five times smaller than reported; the
  spike-with-holes shape is gone; and the strongest band is no longer six."*
  0008 had written its own warning in advance — quoted inside 0024 — *"Six is
  a tool's default, not a law. The number will move when whoever is running
  this changes their configuration, and the detector will go quiet without
  saying so."* — and the warning came true.
- **Correlation is not causation, stated directly in the scoring code, not
  just prose.** `theradar:radar-graph/src/lib.rs`'s own doc comment on
  `Coordination::is_actionable`: *"`Suspected` fires on 13% of all launches
  and carries a four-fold enrichment; that is worth recording and too blunt to
  refuse on by itself."* And `theradar:prevalence.rs`, on why repeat-launcher
  prevalence refuses nothing despite a measured 2x effect: *"The outcome above
  is activity, and activity is not money."* — citing [research
  0011](https://github.com/1xmint/theradar/blob/main/docs/research/0011-graduation-predicts-volatility-not-profit.md),
  which found graduation itself, used for years as a profit proxy, actually
  "predicts volatility, not profit," and *"every threshold fitted against it
  inherits that."*
- **A live, uncaught drift, admitted in the system's own status report.**
  `docs/design/0010`'s box-status table: *"coordination \| **WARN**: 2,916 bps
  of the last 600 launch blocks sat at the centre, against 580 bps
  calibrated — the drift plan 0003's handback recorded, still firing to a
  journal nobody reads."* A five-fold live drift from the calibrated rate, on
  a threshold that still gates decisions, with the monitor writing to a log
  nobody was reading. This is the concrete argument for porting the
  calibration monitor (item 2 above) rather than only the signal.

## Confidence and what would change it

**CHECKED**, not PROVED: every named signal above is sourced to a specific
theradar path, research note, or design-doc line, read directly via `gh api`
(raw file contents) rather than inferred from a search snippet — with the
narrow exception of the "Not established" section, which says plainly what
was not read. What would change this: reading `docs/research/data/` and the
creator-index build pipeline in both repos to confirm whether 0007's study
has actually been re-run on Robinhood Chain data (item 4 above), and reading
realorrug's git history for anything landed after `docs/design/0010`'s
2026-09-05 snapshot that might have built "trades-to-depth" or holder
concentration since. Neither search was run in this pass; both are named
above rather than silently skipped.
