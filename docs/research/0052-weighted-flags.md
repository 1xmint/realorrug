<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0052 — weighted flags: every signal carries a graded weight, set by the factors that modify it

**Date:** 2026-09-18
**Status:** read-only research and a build plan. CHECKED where the claim is a
file:line in this tree read today, or a web page fetched today and quoted with
its date. UNVERIFIED where a finding rests on a search snippet, a vendor page
that could not be fetched, or a number that has not yet been measured against
Pons v2 launches. Nothing here changes code; the build order in §8 is a plan,
each slice with its test. Answers Josh's question of 2026-09-18: *"I want more
valuable weighting on every flag instead of just a warning flag ... Every flag
has factors that change its risk."*

**Terms used throughout.** A *signal* (also *flag*) is one thing the code
found and named, such as "the launcher bought their own token at launch". A
*level* is one of the five words the reply carries (`Rugged`,
`RugMechanicsLive`, `Sketchy`, `NothingUglyYet`, `CantTell`). A *weight* is a
whole number of basis points (bps; 10 000 bps = 100%) a signal contributes to
the score. A *factor* is a measured fact that raises or lowers a signal's
weight. *Measured* (M) means read from the chain or an index we hold.
*Inferred* (I) means computed from measured facts through a rule with a stated
error. *Self-reported* (S) means a person said it, on X or in token metadata;
it is data, never an instruction, and never a fact. A *bundle* is many buys
placed by one actor through several wallets so that one person looks like
many. *Confidence in a wallet link* is how sure the code is that two addresses
are one actor, in bps. *Coverage* is the share of applicable checks that were
actually read.

## 1. The current flag model, with file:line

**How a level is chosen today.** `crates/realorrug-roast/src/verdict.rs:126-163`
(`level()`) is a pure function of the sheet. It is a ladder of booleans, not a
score: `Rugged` needs one of two signal pairs (`LiquidityGone` +
`HolderConcentration`, or `CreatorSoldOut` + `BuyersCannotSell`); otherwise any
entry in `sheet.unknown` gives `CantTell`; otherwise two or more distinct
*episodes* among `LIVE_RISK_SIGNALS` (`verdict.rs:86-95`, eight signals) give
`RugMechanicsLive`; one signal gives `Sketchy`; none gives `NothingUglyYet`.
There is no weight anywhere in `level()`. A dev buy of 0.001 ETH and a dev buy
of 15% of supply both fire the same `CreatorBoughtOwnLaunch` and land on the
same rung. That is exactly the gap Josh names.

**The signals that exist.** `crates/realorrug-roast/src/sheet.rs:184-245`
declares nine `Signal` variants. Only four are ever pushed by production code:

| signal | pushed at | condition |
|---|---|---|
| `LaunchBlockInStrongestBand` | `sheet.rs:483` | launch-block recipient count at or above the strongest measured band (`baserates.rs` bands, Solana-measured; not yet measured for Pons v2) |
| `CreatorBoughtOwnLaunch` | `sheet.rs:489` (Solana, `dev_buy_lamports > 0`), `sheet.rs:1152` (Robinhood, `dev_buy_wei > 0`; `None` becomes `Kind::DevBuyUnseen`, "absent, NOT zero") | any nonzero amount |
| `CreatorNeverGraduatedOrganically` | `sheet.rs:553` | creator index `measured > 0 && organic == 0` |
| `RepeatLauncher` | `sheet.rs:582` | lifetime launches at or above `index.repeat_launcher_floor(list)` |

`LiquidityGone`, `CreatorSoldOut`, `BuyersCannotSell`,
`OwnerCanStillMintOrPause` and `HolderConcentration` are declared, have
innocent twins (`sheet.rs:260-305`, `twin_for`) and plain wording
(`sheet.rs:328-340`), and are never pushed. So the two `Rugged` pairs cannot
fire today, and `RugMechanicsLive` needs two of the four live ones.

**The score that already exists but does not decide anything.**
`crates/realorrug-roast/src/assessment.rs:177-191` holds five hand-set
weights per *episode* (`assessment.rs:65-78`): `WEIGHT_LAUNCH_BLOCK 25`,
`WEIGHT_CREATOR_HISTORY 20`, `WEIGHT_EXIT 40`, `WEIGHT_HOLDERS 25`,
`WEIGHT_AUTHORITY 20`. `Assessment::from` (`assessment.rs:233-273`) folds one
weight per distinct episode, saturating at 100, into `risk_index: u8`. It is
whole-number, deduplicated by episode (so two signals of one episode count
once), and it carries a separate `Coverage { read, applicable }`
(`assessment.rs:141-147`) that never moves the index, plus `critical_gaps =
sheet.unknown`. ADR 0032 decided this shape (score with its coverage, levels
as bands, weights hand-set then fitted); ADR 0032 decision 7 and design 0027
§3 say the bands are shadow-only until fitted. The `level` field is still
`verdict::level(sheet)`, not a band of the score. So: a score exists, it is
per-episode not per-signal, it has no factors, and nothing reads it.

**Facts the sheet carries that could be factors today** (all
`crates/realorrug-onchain`): `ChainLaunch { block, age_seconds, dev_buy_wei }`
and `Holders { count, largest_share_bps }` (`dossier.rs`); `CurveFacts {
complete, quote_reserves, quote_capacity, creator, fees }`; `Funding { buyers,
selected, coverage_bps, checked, shared, gaps }` with `Candidate {
nonce_before_launch, is_contract, funders, funding_complete }` and
`SharedFunder` (`wallets.rs`); `CreatorCashFlow { trades, transfers_out }`;
`Concentration { largest_non_infrastructure, unresolved_large }` (`roles.rs`,
a role only with a `Proof`); `MarketSnapshot { liquidity_usd, market_cap_usd
}` (`market.rs`, DexScreener or GeckoTerminal); `Unavailable { fact, why }`
for every read that failed. The Robinhood reader (`robinhood.rs:698-1009`)
fills launch, graduation, reserves, launch block/age/dev buy and holders;
funding and creator cash flow are `Unavailable` there today (research 0050
§4).

**What the rules already fix.** `forbidden.rs` rejects "scam", "rug pull" as
a verdict on a person, reassurance, and "graduation history as a good sign".
`verdict.rs:262-340` ranks facts by `Fact::kind` for the five-fact lead
(`MAX_FACTS = 5`, `verdict.rs:352`). Any weighted model has to keep all of
that; it only changes what decides the rung and adds the numbers to the
sheet.

## 2. What the outside world knows

Sources fetched or searched today (2026-09-18) unless marked otherwise.
"Fetched" means the page was read; "search snippet" means only a search
engine summary was read and the claim is UNVERIFIED until fetched.

### 2.1 How serious tools detect bundles, clusters and insiders

| tool | what it measures | linking evidence | how it states confidence | source |
|---|---|---|---|---|
| Bubblemaps | *bundle* = "a set of wallets that transacted together during the token launch: typically all buying in the same block or within the same snipe window"; *cluster* = "a set of wallets linked by on-chain behavior over time: shared funding sources, coordinated transfers, or recycled addresses" | same-block buys (bundle); shared funding source, internal transfers, cross-token recurrence (cluster) | one named threshold: "a bundle holding 10%+ of total supply is a major red flag"; hedged wording, clusters "often indicate" hidden concentration; no numeric confidence | blog.bubblemaps.io, "What's the difference between bundle and cluster", dated 2026-05-06, fetched |
| pump.fun bundle checkers (solbundler and similar) | wallets that bought in slot 0 (the launch slot), their share of supply, whether they have sold | same-slot buys, found by binary search over block timestamps to the exact slot | a count and a share, no confidence; a bundler vendor states outright that bundlers "execute buy with delay between all wallets to ensure they bypass every type of bundle detection" | solbundler.app bundle checker and "how to bundle" pages, search snippet, undated |
| GMGN | four live metrics: insider ("rat") wallet ratio, bundle-buy ratio, dev holdings %, top-10 concentration; blue-chip owner % | a cluster of 3+ related wallets tags all as bundlers; a wallet matching several signals is an insider | percentages of supply, no confidence interval | search snippet, undated; UNVERIFIED |
| RugCheck | composite 0-100 score, higher is riskier, from mint/freeze authority, LP lock, holder concentration, mutable metadata | none for wallet links; it grades the token's powers and its top holders | one number, fixed rules; research 0041 §1 read it as deterministic | search snippet; research 0041 |
| Chaos Labs + Nansen (LayerZero Sybil work) | industrial Sybil clusters | source of funding: a CEX/DEX/bridge funding many addresses in one window; clusters kept only with silhouette score > 0.7; "maximising precision over recall" | a list explicitly "not ... definitive"; about 14.5% of users classified Sybil; 803,093 addresses | x.com/omeragoldberg and x.com/chaoslabs, 2024-05-17/18, search snippet |
| Elliptic | automatic rug-pull detection on memecoins (method not read) | not read | not read | elliptic.co, search result, UNVERIFIED |
| Arkham, Trench | not fetched today | -- | -- | UNVERIFIED; nothing in this document rests on them |

The common shape: **every tool separates *when* (same block, launch window)
from *who* (funding, transfers, recurrence)**, and the one that publishes a
full method (Chaos Labs) chooses precision over recall and says the list is
not final. None publishes a probability that a flagged wallet is really
linked. The honest state of the art is a count, a share of supply, and a
hedge.

### 2.2 Linking evidence, and what we can see on Robinhood Chain

| evidence | what it means | leaks even when the dev is careful? | visible with our data today |
|---|---|---|---|
| same-block buys at launch | several wallets bought in the launch block | no; a careful dev spreads buys over blocks (bundler vendors sell exactly this) | yes: `CurveBuy` and `Transfer` logs in the launch block (`sheet.rs:483`); 0.1 s blocks make "same block" narrower than on Solana, so also count "first 30 blocks" (3 s, the `snipeTaxSeconds` window) |
| near-identical buy sizes | scripted wallets | partly; scripts randomise sizes, usually within a narrow band | yes, from `Purchase.quote` |
| fresh wallets (`nonce_before_launch == 0`) | wallets created for this launch | weakly; pre-ageing wallets costs time and gas, and "this buy was the wallet's first transaction" is still a fact | yes: `Candidate.nonce_before_launch` (`wallets.rs`); research 0050 §7.2 found no innocent twin for many fresh wallets buying one launch in one block |
| shared funding source, one hop | the buyers' ETH came from one address | this is what a careful dev hides first: fund via a CEX, or several hops | **no** for native ETH: funding transfers emit no logs, and no trace API is on the public RPC or Alchemy's Robinhood method table (research 0045 §1, 0050 §5). `Funding` is `Unavailable` on Robinhood today |
| CEX withdrawal timing | many wallets funded from one exchange hot wallet within minutes | leaks the exchange, not the person; Chaos Labs used exactly this window | no (same trace gap); and no label list of exchange hot wallets for chain 4663 exists |
| deployer links | a buyer was funded by, or funds, the deployer or `creatorFeeRecipient` | leaks unless the dev uses a fresh funding path | no for ETH; **yes for ERC-20 transfers** (`Transfer` logs) if the dev moves tokens between wallets |
| transfer graph between holders | tokens moved wallet to wallet after launch | leaks; moving tokens is a logged event | yes, `Transfer` logs, one `eth_getLogs` per token (60 CU, research 0047 §7) |
| correlated sells | several wallets sell within a few blocks of each other | leaks; selling is the purpose, and the sell is logged | yes, `CurveSell` logs pre-graduation; post-graduation needs the v4 pool address, which research 0044 has not found |
| snipe-tax exemption | `snipeTaxExempt(addr) == true` for an address not on the first-party list and not in the launch calldata | leaks: it is a contract read | yes, one `eth_call` each (research 0047 §7.1, 0048 §6) |
| gas-price clustering | same gas settings across buyers | weak on Nitro, where most gas prices sit at the base fee (research 0050 §7.2) | measurable, low value |

**What "we could not tell" means on this chain.** The funding-source link,
the single strongest evidence every tool above relies on, is not readable
here without a trace-capable node (QuickNode Build, $49/month, search
snippet dated 2026; `debug_traceTransaction` is quoted at 200-1,000+ CU per
call, and it is transaction-scoped, so finding one wallet's funding means
walking the block range). So on Robinhood the linked-wallet signal is, today,
a *timing and shape* signal, not a *who* signal, and §3 weights it as such.

### 2.3 Evasions, and what still shows

- **Delay between wallet buys** defeats same-block. Still shows: the buys
  land in the first seconds (blocks 0-30) before any human could have seen
  the launch; the wallets are fresh; sizes cluster.
- **Pre-aged wallets** defeat the nonce test. Still shows: if they were aged
  by buying other launches, the cross-token recurrence Bubblemaps names, which
  our creator index can hold as a *buyer* index (not built; §8).
- **CEX funding** defeats one-hop funding links. Still shows: correlated
  sells, and (if traces were bought) the exchange withdrawal window.
- **Declared exemptions.** Pons v2 lets a launcher name exempt addresses in
  `launchToken` calldata (research 0048 §3). A dev who declares their bundle
  wallets is *open* about it; an exempt address not in that list is not.
  This is the one place the chain itself records "announced" versus hidden,
  and it is the mechanism §3 uses so that hiding never scores better than
  being open.

### 2.4 Published prediction rates: evidence versus folklore

| claim | number | evidence grade |
|---|---|---|
| Solidus Labs, 2025 Rug Pull Report | 98.6% of pump.fun tokens (Jan 2024 to Mar 2025, 7M+ tokens with 5+ trades) "collapsed into worthless pump-and-dump schemes"; 93% of Raydium pools showed soft-rug traits | CoinDesk 2025-05-07 and the Solidus X post; the definition is "liquidity under $1,000", which counts every failed honest launch as a rug. A base rate, not a prediction |
| "Catching the Rug" (arXiv 2608.20271, submitted 2026-08-20) | 6.4M Solana tokens over 7 months; XGBoost "achieves robust performance ... using only the first 5 minutes of trading data"; "a vast majority ... exhibit rug pull characteristics within one hour" | abstract fetched; precision and recall are not in the abstract; features not enumerated. The five-minute finding supports weighting launch-shape signals heavily |
| MELT (arXiv 2602.13480, revised 2026-05-21) | 41k+ Solana launches, 200M+ transactions, 122 behavioural features, risk labels; "on average, 36.5% of token supply is held by coordinated accounts" | abstract fetched. The 36.5% is the best published number for how much supply bundles typically hold, and it is Solana |
| ScienceDirect, "Detecting rug pulls in decentralized exchanges: the rise of meme coins" (2025); TON study (arXiv 2509.01168) | machine-learned rug detection on Uniswap-style and TON pools | search results only; UNVERIFIED |
| Radar research 0008/0024, 0012/0013, 0007, 0011 | launch-block recipient band, repeat launcher, creator edge; graduation history is *not* a good sign (organic graduations median -3,228 bps) | our own measurements on Solana pump.fun, in the Radar tree |
| "50%+ sniper supply means a coordinated dump", "10%+ bundle is a major red flag" | vendor thresholds | folklore until measured on Pons v2; §5 replays them |

Nothing published is measured on Robinhood Chain. Every threshold in §3 is
therefore a starting weight to be replayed (§5), never a fact.

## 3. The signal catalogue

**How to read the table.** *Base* is the weight in bps of the score when the
signal fires with no factor applied. *Raise* and *lower* are factors, each
tagged M (measured), I (inferred), S (self-reported). A factor moves the
weight by a stated whole number; factors add, then the result is clamped to
[floor, cap] (§3.3). *Link* is how confidence in a wallet link scales the
weight (§3.2). *Gaming / counter* names what a dev does to dodge it and what
still shows. *Today* is whether the sheet can measure it now. Weights are
starting values (§2.4, last row) chosen so that two no-factor launch-shape
signals land near today's `RugMechanicsLive` rung (§4.3), keeping existing
fixtures roughly stable until the replay in §5 moves them.

### 3.1 The table

| # | signal | base bps | raise | lower | gaming / counter | today |
|---|---|---|---|---|---|---|
| S1 | **creator bought own launch** (`CreatorBoughtOwnLaunch`) | 1,200 | +1,500 if the creator's own share (M: `dev_buy_wei` against the curve price at the launch block) or the linked-wallet effective sum (I, §3.2) is >= 1,000 bps; +800 if >= 500 bps; +600 if the creator sold any within 24 h (M, `CreatorCashFlow`) | -400 if share < 100 bps (M); -300 if the buy wallets are declared in the launch calldata exemption list (M, research 0048 §3); at most -200 if announced on X before the launch block (S) | split across wallets / §3.2 linking; hide by CEX funding / correlated-sell factor still fires | yes; share needs one `eth_call` for the launch-block price [^s1-correction] |
| S2 | **launch-block recipient band** (`LaunchBlockInStrongestBand`) | 1,500 | +1,000 if same-window buyers hold >= 1,000 bps together (M; Bubblemaps' own 10% threshold); +800 if >= 3 buyers are fresh (M); +500 if sizes are within 10% of each other (I) | -500 if every launch-window buyer is on the declared exemption list (M); -300 if the band was measured on < 200 launches (I, thin sample) | spread buys over 30 blocks / count the same shape over the first 30 blocks (3 s, `snipeTaxSeconds`) | **factors wired** (M-D-weights-s2-s5-s13, `sheet.rs::launch_block_band_factors`, `push_window_buyer_factors`, `push_band`): the three raises read `funding.checked` (a cost-limited sample of at most 4 candidates, never every buyer in the launch window) -- the `+1,000` sum is raw/unweighted (`confidence_bps = 10,000` per candidate) via `wallets::linked_holdings_bps`, a candidate missing `bought_tokens` is excluded from that sum rather than counted as zero, and the fresh count needs every checked candidate's nonce read or it stays absent; the `-500` exemption lower fires only when `funding.checked.len() == funding.buyers`, i.e. the full window is known, never against the sample; the `-300` thin-sample lower reads `Band.launches`, which `baserates.rs`'s `Band` still does not store as a field -- it is derived in `push_band` as `band.fires_on * BaseRates::launches` (both already-measured numbers), not invented. **Still dormant on a real Pons v2 dossier**: this signal fires only on a Solana `dossier.launch`, while the four buyer-derived facts above come only from `dossier.funding`/`dossier.powers`/`ChainLaunch::supply`, which are Robinhood-only today. The two never co-occur on real data yet, the same situation S5's `HolderConcentration` factors are already in (§3.1 below); these are exercised by a sheet built directly in a test until one side of the chain gate moves |
| S3 | **repeat launcher** (`RepeatLauncher`) | 1,000 | +800 if >= 10 lifetime launches (M); +500 if any prior launch hit `LiquidityGone` before graduation (M, needs an outcome index); +400 if a duplicate name/symbol relaunch (M, Radar `creator_history`) | -500 if any prior launch graduated organically **and** held liquidity 7 days (M; research 0011: graduation alone is not a good sign, hence the 7-day hold); -200 if the floor was measured on < 200 creators (I) | fresh deployer per launch / `creatorFeeRecipient` recurrence and `pendingCreatorFeeRecipient` moves (research 0047 §7.2) | yes; keyed on `deployer()` per 0047 §6 |
| S4 | **creator never graduated organically** (`CreatorNeverGraduatedOrganically`) | 600 | +400 if measured >= 5 (M) | -400 if measured <= 2 (M, thin denominator) | none; it is history | yes |
| S5 | **holder concentration** (`HolderConcentration`) | 1,200 | +1,000 if largest non-infrastructure >= 2,000 bps (M); +600 if >= 1,000 bps (M); +800 if top-10 >= 5,000 bps (M) | 0 for "may be a pool": an unresolved large balance keeps full weight with unresolved-role wording; -600 only with a `Proof` (`roles.rs`) that the balance is curve, pool, factory or locker | split across wallets / linked-wallet sum (§3.2) | **partly wired** (M-D-weights-s2-s5-s13): the two raises fire off `Kind::LargestHolderShare` (`sheet.rs` `holder_concentration_factors`), standing in for "largest non-infrastructure" per this row's own "unresolved large balance keeps full weight" rule, since `roles::Concentration`'s role-proven reading is never projected onto the sheet. The top-10 raise and the `Proof`-gated lower stay out: no top-10 fact and no `Proof` reach the sheet; `HolderConcentration` itself is still not pushed by `FactSheet::build` on any chain, so these factors are exercised only by directly-constructed test sheets today; holder-read paging bug (research 0050 §5) unresolved |
| S6 | **linked-wallet holdings** (new) | 0 alone | feeds S1, S2, S5 as the sum over linked wallets scaled by link confidence (§3.2) | -- | see §2.3 | link kinds: same-window + fresh + size (yes); ERC-20 transfer graph (yes); ETH funding (no); **the sum itself is wired** (M-D-S6-token-amounts: `wallets::linked_holdings_bps`, dedupes by wallet at its strongest effective reading, integer bps only), but no caller feeds it yet -- S1/S2's factor wiring is the next slice |
| S7 | **correlated selling** (new) | 1,000 | +800 if >= 3 linked wallets sold within 50 blocks (M); +600 if the sold volume >= 1,000 bps of supply (M) | -300 if the sells spread over > 1 hour (M) | cannot be hidden: the sell is the point | **wired, pre-graduation only** (`wallets.rs` `correlated_selling`; fires at 2 linked sellers, link confidence ≥ 4,000 bps from buy-side evidence); post-graduation blocked on the pool address |
| S8 | **fresh-wallet share** (new) | 800 | +600 if >= 5,000 bps of launch-window buy volume came from wallets with `nonce_before_launch == 0` (M) | -400 if < 2,000 bps (M) | pre-age wallets / cross-token recurrence (needs a buyer index, §8) | yes for the candidates checked; `coverage_bps` states how much volume was checked |
| S9 | **funding from a known rug-linked wallet** (new) | 1,500 | +1,000 if the funder deployed a token that hit `LiquidityGone` (M) | -- | CEX hop / nothing today | **no**: needs traces or an ERC-20 path; declared but not pushed, like today's five |
| S10 | **liquidity gone pre-graduation** (`LiquidityGone`) | 4,000 | +2,000 if holders still hold >= 1,000 bps (M) | -1,500 if every buyer sold back (the innocent twin, M via `CurveSell` sum) | none | not pushed today; `quote_reserves` reads; the twin needs the sell sum |
| S11 | **creator sold out** (`CreatorSoldOut`) | 2,500 | +1,000 if within 24 h of launch (M) | -800 if sold < 5,000 bps of their position (M) | move to a linked wallet first / transfer graph | needs a prior balance (memory `has_prior_balance`) |
| S12 | **buyers cannot sell** (`BuyersCannotSell`) | 4,000 | +1,500 if a dust-size sell reverts (M) | -1,000 if only sizes above a threshold revert (M, weak form) | none | needs an `eth_call` sell simulation; size not chosen |
| S13 | **owner powers live** (`OwnerCanStillMintOrPause`; on Pons v2 the real powers are creator tax 0-1,000 bps, `pendingCreatorFeeRecipient`, snipe-tax exemptions — [ADR 0035](../adr/0035-s13-owner-powers-live-is-the-pons-v2-reads-not-a-bytecode-scan.md) settled this row's conflict with design 0020's bytecode-scan version in this row's favour for Pons v2) | 800 | +700 if creator tax >= 500 bps (M); +500 if `pendingCreatorFeeRecipient` is non-zero (M); +600 per exempt address off both the first-party list and the launch calldata, cap +1,200 (M) | -300 if tax == 0 (M) | none; contract reads | **wired** (M-D-S13-owner-powers, ADR 0035): the `eth_call` reads (`robinhood.rs` `powers_facts`, M-D-0004) land on `Dossier.powers`, and `sheet.rs`'s `push_powers` now projects creator tax, the pending-recipient flag and the `Undeclared`-classified exemption count onto the sheet as `Fact`s; `factors` fires this row's full raise/lower table off them. A sub-read that failed (`pending creator fee recipient`, `declared snipe-tax exemptions`, `snipe tax exemption`, `snipe tax exemption classification`) is a coverage gap in `sheet.rs`'s `skipped` list, never a zero or clean reading for that piece |
| S14 | **LP ownership / lock / burn** | not applicable pre-graduation (a category error: reserves sit in the curve, research 0044); post-graduation 1,500 if pool reserves fall >= 5,000 bps in 1 h (M) | -- | -- | -- | blocked on the pool address; `PonsV2LaunchLocker.sol` unread |
| S15 | **tax changed after launch** | Pons v2 sets the tax once (research 0047); if a change is ever observed, 2,000 | -- | -- | -- | drift alarm only (0047 §7.3) |
| S16 | **social signals** (X posts, metadata claims) | 0 | none; a post never raises a weight | at most -200 on S1 for a pre-launch announcement (S), never below the floor | trivially gamed | data only (§7.3) |
| S17 | **a required fact unread** | not a weight: `CantTell` stays a gate (§4.3) | -- | -- | -- | -- |

[^s1-correction]: **Correction (dev-share wiring):** this table row assumed
the creator's share of supply needed the curve's launch-block price via an
`eth_call`. It does not: the launch transaction's own receipt (already
fetched for `dev_buy_wei`) carries the buyer's `CurveBuy` `tokensOut` and the
ERC-20 mint `Transfer` from the zero address, so `dev_buy_tokens * 10,000 /
supply` is read straight off facts already on the sheet, with no added RPC
call. See `ChainLaunch::dev_buy_tokens`/`ChainLaunch::supply`
(`realorrug-onchain/src/dossier.rs`) and `crate::sheet::factors` in
`realorrug-roast/src/sheet.rs`.

### 3.2 How wallet-link confidence scales a weight

Link confidence `c` is a whole number in bps, taken from the **strongest**
evidence present, never added across kinds (correlated evidence must not
multiply, design 0027 §"Judgement"):

| evidence | c (bps) | grade |
|---|---|---|
| ERC-20 transfer between the two wallets, or from the deployer | 9,000 | M |
| declared together in `launchToken` exemption calldata | 9,000 | M, and this is the *open* case |
| same launch block + both fresh + sizes within 10% | 7,000 | I |
| same launch block + both fresh | 5,000 | I |
| same 30-block window + sizes within 10% | 4,000 | I |
| same 30-block window only | 2,000 | I |
| sold within 50 blocks of each other | 3,000 (confirms; never the only link) | I |
| ETH funding, one hop (not readable today) | 8,000 | M when available |

A linked-wallet sum uses `share_bps * c / 10_000` per wallet, whole-number
division. So 15% held across wallets linked at 7,000 bps counts as 1,050 bps
of supply, not 1,500; the sheet shows both numbers and the link kind.

**Hiding never scores better than being open.** Take the same true 15%
across three wallets. Declared in calldata (c 9,000, effective 1,350 bps):
S1 = 1,200 + 1,500 - 300 = 2,400; S2 fires (same block) with its -500
because every buyer is declared: 1,500 + 1,000 - 500 = 2,000. Total 4,400.
Hidden, same block, fresh, similar sizes (c 7,000, effective 1,050 bps):
S1 = 1,200 + 1,500 = 2,700; S2 = 1,500 + 1,000 + 800 + 500 = 3,800; S8 fires
too (800 + 600). Total 7,900. Every lower is tied to openness (declared list,
small share) and every raise to a concealment trace (fresh wallets, matched
sizes), so at equal true holdings the hidden case is never below the open
case. When the link evidence is too weak to sum at all, the shape signals
(S2, S8) still fire on their own, and the reply says "we could not tell who
these wallets are" in unresolved-role wording.

### 3.3 Floors and the unread rule

No factor set may take a signal below 25% of its base, so a measured fact
always leaves a mark. Self-reported factors together may lower a signal by at
most 200 bps and may never raise one. A signal whose measurement failed
(`Unavailable`) is scored as **unread**, not zero: it goes to `critical_gaps`
and to the coverage fraction, never to the score. That is how "we could not
tell" is weighted: it does not lower the score, it lowers coverage, and
coverage below the floor is `CantTell` regardless of score. The reply says
which check was unread, in the words `sheet.unknown` already carries.

## 4. How weights combine into one score

### 4.1 Three rules, compared on one sheet

The sheet is Josh's third case (§6, case C): four signals after factors,
S2 = 3,800, S1 = 2,700, S5 = 1,800, S8 = 1,400 bps.

| rule | formula (whole numbers, bps) | this sheet | what it gets wrong |
|---|---|---|---|
| capped sum | `min(10_000, sum(w))` | 9,700 | four mild signals outrun one hard fact; correlated signals (a bundle shows up as S1, S2, S5 and S8 at once) count four times; today's `risk_index` is this rule per episode |
| strongest flag dominates | `max(w)` | 3,800 | a bundle plus a repeat launcher plus concentration reads the same as the bundle alone; adding evidence never moves the number |
| noisy-OR | `rest = 10_000; for w in sorted_desc(w): rest = rest * (10_000 - w) / 10_000; score = 10_000 - rest` | 6,809 raw, 6,289 after episode dedup (§4.1 below) | needs a deterministic order (floor division makes the fold order-sensitive by a few bps), and it still double-counts correlated signals unless episodes are deduplicated first |

**Pick: noisy-OR over deduplicated episodes.** "Noisy-OR" is the rule for
combining independent chances that any one of several things is present:
the chance that none is present is the product of each one's absence. It
gives the properties wanted here: the strongest signal sets the floor, each
further signal adds with diminishing returns, the score never exceeds 10,000,
and adding a signal never lowers it. Correlation is handled the same way
`assessment.rs:233-273` already handles it: signals in one `Episode` are
folded to that episode's **maximum** weight first (not its sum), so a bundle
seen four ways counts once at its strongest reading. The fold runs in `u32`
on weights sorted descending then by signal id, so two sheets with the same
signals always produce the same number. No floating point anywhere
(`AGENTS.md` §3.1 and the daily-five rule in design 0028).

Worked, raw: 3,800 then 2,700 then 1,800 then 1,400 → rest 10,000 → 6,200 →
4,526 → 3,711 → 3,191; score 6,809. After dedup (S8 folds into S2's
episode): 3,800, 2,700, 1,800 → rest 3,711; score 6,289.

### 4.2 Score to level

`Rugged` and `CantTell` stay gates, never bands. `Rugged` needs an observed
completed event (today's two pairs, `verdict.rs:127-133`); no score reaches
it. `CantTell` fires on any required fact unread (today's `sheet.unknown`
gate, `verdict.rs:135`) **or** coverage below 6,000 bps of applicable checks;
"applicable" excludes checks the venue cannot offer at all (ETH funding on
Robinhood today), which are named in the reply instead. Between the gates:

| level | rule |
|---|---|
| `RugMechanicsLive` | score >= 2,500 bps |
| `Sketchy` | at least one signal fired and score < 2,500 |
| `NothingUglyYet` | no signal fired, every required fact read; the reply carries the age and the "yet" |

Two no-factor launch-shape signals in different episodes (S2
`LaunchBlockInStrongestBand` 1,500 + S5 `HolderConcentration` 1,200) give
2,520 → `RugMechanicsLive`, matching today's "two episodes" rung. S1 + S3
with no factors gives 2,080 → `Sketchy`, which today reads
`RugMechanicsLive`; that is intended (a 0.001 ETH dev buy plus a second
launch is not two live rug mechanics). It is one of several pairs this flag
moves from `RugMechanicsLive` to `Sketchy` once it publishes -- every pair
below is a no-factor noisy-OR that clears today's 2,500 line but not this
one (verified against `assessment.rs`'s `noisy_or`, §8, M-D-0005
stop-and-ask):

- S1 `CreatorBoughtOwnLaunch` 1,200 + S3 `RepeatLauncher` 1,000 → 2,080
- S1 1,200 + S5 `HolderConcentration` 1,200 → 2,256 (the sheet in
  `the_template_states_a_twin_at_rug_mechanics_live_and_none_at_rugged`)
- S3 1,000 + S5 1,200 → 2,080
- S2 `LaunchBlockInStrongestBand` 1,500 + S3 1,000 → 2,350

**M-D-0005 built, shadow only (2026-09-18).**
`crates/realorrug-roast/src/verdict.rs::level_from_score` computes this
table's rule beside `level`, sharing its `Rugged` and `CantTell` gates
exactly (a private `rugged_pair` helper the two now call, so they cannot
drift apart) plus one this document adds: coverage under 6,000 bps of
`crate::assessment::Coverage` also forces `CantTell`. It is exposed on
`Assessment::score_level` (a new JSON key, `score_bps`'s neighbour) and on
`realorrug roast --sheet`'s printed "provisional" line; **`level` itself is
still what publishes**, per §9's "the level should still come from
`verdict::level`" -- nothing reads `score_level` to decide anything yet.
`Assessment::admissible` was left alone rather than rebuilt from bands: the
band set it offers today already brackets `level`, and rebuilding it from
`score_level` while `level` stays the published one would let a model reach
for a band the shadow score chose over the one the ladder chose, which is a
behaviour change this slice does not need to make.

S1 (`CreatorBoughtOwnLaunch`) and S2 (`LaunchBlockInStrongestBand`) are one
episode per ADR 0032 (`assessment.rs`'s `episode` function) and read
`Sketchy` on both the `level` and `level_from_score` paths -- not the S2+S5
pair used above.

### 4.3 Score to the daily-five odds q

Design 0028's zero-average proof needs only that `q` for a class of launch
equals the measured rug rate of that class, where the class is what the
player is shown. Today the class is the level. This document adds a finer
class, the **score band** (0-999, 1,000-2,499, 2,500-4,999, 5,000-7,499,
7,500-10,000 bps), and `q` per band comes from the same replay (§5), as a
whole number of bps. The proof holds unchanged because a player with no
knowledge still averages `p - q = 0` in every band, and the luck line
`z*` in Q32 does not change. Three rules keep it honest: the score shown at
pick time is the score `q` keys on (snapshot at pick, never recomputed);
a band with fewer than 200 replayed launches falls back to its level's `q`;
and `q` is read from the replay table, never computed from the score by a
formula. Nothing here touches the payout key; the contest crate reads a
band the roast crate wrote, the same direction as today.

## 5. Calibration

**Label.** A rug is an observed extraction, not a failed launch (design 0027
§"Judgement"). Label a replayed launch `rug` if within the window it hit
S10 (`LiquidityGone` pre-graduation with holders still holding >= 1,000 bps)
or S11 (creator sold >= 5,000 bps of their position) or, post-graduation, pool
reserves fell >= 9,000 bps in one hour. Label `failed` if reserves went to
zero with every buyer sold back (the S10 twin). Label `alive` otherwise. The
Solidus definition ("liquidity under $1,000", §2.4) would label 98.6% `rug`
and make every weight meaningless; it is not used.

**Sample.** Available now: 350 launches with 298 deployers in the creator
index (research 0036 §1), 8,262 graduation logs (0040), and ADR 0032's
532,226 launch outcomes for the creator-history factors. Minimum for a
weight: **200 launches where the signal fired and 200 where it did not** in
each split. That is the two-arm size that detects a 1,000 bps difference in
rug rate at the usual 80% power when the base rate is near 5,000 bps, and
more than enough when it is near 9,000. A factor gets a non-zero delta only
if it moves the rug rate by >= 500 bps in the same direction on **both**
held-out splits (time: last 30% of launch dates; wallet family: creators
never seen in the fit, design 0027 §5). Weights are whole hundreds of bps,
so the fit has few knobs to overfit with.

**Detecting a wrong weight.** Three checks, all whole-number:

1. **Monotone bands.** The observed rug rate per score band must rise with
   the band. A band whose rate is below the band under it flags the weights
   that put launches there.
2. **Brier in bps.** `sum((score_bps - outcome * 10_000)^2) / n / 10_000`
   on the held-out split, compared against the boolean ladder's score
   (fired = 10,000, not = 0) and against a constant equal to the base rate.
   A weighted model that does not beat both is not shipped.
3. **The four dummy players.** Design 0028's random, always-rug, always-real
   and copy-the-bot dummies must sit near zero on the band-keyed `q` over the
   window. A dummy that drifts is a miscalibrated band.

## 6. Five worked cases

Supply and shares are in bps of total supply; the launch-block price is one
`eth_call`. Each case lists the signals that fire, the factors with their
grade, the noisy-OR score, the level, and the sentence the sheet can carry.

**A. Dev bought on the first block, transparent, small** (Josh's second
case). Dev bought 50 bps in the launch transaction, only launch-block buyer,
tax 0, first launch, announced on X two minutes before.
S1: 1,200 - 400 (share < 100 bps, M) - 200 (announced, S) = 600. Nothing
else fires. Score 600. `Sketchy` (a signal fired). Sheet: "the launcher
bought 0.5% of supply in the launch block, in one wallet, and said so
beforehand; that is a small buy and it is disclosed." The -200 is the only
self-reported number on the sheet and is tagged S.

**B. Dev bought on the first block, 800 bps, one wallet, said nothing**
(Josh's first case). S1: 1,200 + 800 (>= 500 bps, M) = 2,000. Score 2,000.
`Sketchy`, 500 bps under the `RugMechanicsLive` line. Sheet: "the launcher
holds 8% from a launch-block buy; no other launch-block buyer."

**C. Dev obviously bundled, 1,500 bps across wallets** (Josh's third case).
Dev wallet 400 bps; four more wallets in the launch block, all fresh
(`nonce_before_launch == 0`), sizes within 10%, together 1,100 bps. Link c =
7,000 (§3.2); effective linked holding 1,050 bps.
S1: 1,200 + 1,500 (linked effective sum >= 1,000 bps, I) = 2,700.
S2: 1,500 + 1,000 (window buyers hold >= 1,000 bps raw, M) + 800 (>= 3
fresh, M) + 500 (sizes, I) = 3,800.
S5: 1,200 + 600 (linked sum >= 1,000 bps) = 1,800.
S8: 800 + 600 (>= 5,000 bps of window volume from fresh wallets, M) = 1,400.
Episodes: S2 and S8 share `LaunchBlock` (max 3,800); S1 is `CreatorHistory`
(2,700); S5 `Holders` (1,800). Noisy-OR: 10,000 - 6,200 × 0.73 × 0.82 =
**6,289**. `RugMechanicsLive`. Sheet: "five wallets bought in the launch
block and hold 15% together; four had never sent a transaction before;
their buys are within 10% of each other in size. Whether they are one
actor is inferred from that shape, at 70% link confidence, not read."

**D. A skilled hidden bundle.** Dev bought 100 bps, declared, alone in the
launch block. Six wallets aged for a month, funded through an exchange, buy
between 4 s and 20 s after launch (past the 3 s snipe-tax window), sizes
spread ±30%, together 1,400 bps. Link c = 0 (outside the 30-block window,
no fresh, no size match, no transfer between them).
S1: 1,200 (share 100 bps: not under 100, not over 500) - 300 (declared, M) =
900. No S2 (one launch-block recipient), no S8 (no fresh wallets), no S5
(largest 250 bps). Score 900. `Sketchy`. Sheet: "the launcher bought 1% in
the launch block and declared the wallet; six wallets bought within 20 s of
launch and hold 14% together; who funded them could not be read on this
chain." That last clause is the "could not tell" display: an unresolved
role, counted in coverage as not applicable, weighted zero, and said out
loud. When those six sell within 50 blocks of each other, S7 fires: 1,000 +
800 + 600 = 2,400, and with S1 the score is 10,000 - 7,600 × 0.91 = 3,084 →
`RugMechanicsLive`, after the fact. **This is the honest limit of the data:
a skilled bundle on this chain is caught when it acts, not before, unless
traces are bought (§7).** Note the Pons v2 mechanic that shapes this: the
snipe tax starting at 9,900 bps for 3 s (research 0047 §7) makes undeclared
launch-block bundling expensive, so a careful dev either declares the
wallets (visible) or waits past 3 s (invisible to S2, visible to the window
fact and to S7).

**E. A clean launch.** `dev_buy_wei` read as 0 (a zero read, not `None`),
40 distinct buyers in the first minute, largest non-infrastructure holder
300 bps, tax 100 bps, first launch by this deployer, all required facts
read. No signal fires. Score 0, coverage 6 of 6 applicable. `NothingUglyYet`.
Sheet: "12 minutes old; no launch-block buy by the launcher; 40 buyers so
far; largest wallet holds 3%; nothing ugly yet."

Cross-check of the rule that hiding never wins: C (open shape, 6,289) scores
above D (hidden, 900) only because D hid well enough to leave no measured
trace; the moment D leaves one (a shared fresh wallet, a transfer, a joint
sell) it climbs, and nothing D can *declare* lowers it below C's declared
equivalent, because the declared lowers (-300, -500) are smaller than the
concealment raises (+800, +500, +1,000).

## 7. Data

### 7.1 Measurable today, per signal (Alchemy CU from research 0039 §2)

| signal | reads | cost per token |
|---|---|---|
| S1 amount | launch record | already read |
| S1 share | curve price at launch block, one `eth_call` | 26 CU |
| S2 recipients and window buyers | `eth_getLogs` launch block (already); widen to 30 blocks | 60 CU |
| S8 fresh wallets | `eth_getTransactionCount` per candidate at launch block − 1 | ~20 CU each, 4 candidates (`wallets.rs` selection) |
| S3, S4 | creator index | already held |
| S5 | holder walk (`Transfer` sum; paging bug, research 0050 §5) | 60 CU + fix |
| S7 | `CurveBuy`/`CurveSell` logs, launch to read point, one ranged read; then the first and last sell block's timestamps | 60 CU + 80 CU × 2 (as built: one read covers every window) |
| S13 | `snipeTaxExempt` × (creator + top 5), `pendingCreatorFeeRecipient`, tax | 26 CU × 7 |
| link c | derived from the above | 0 |

Roughly 500 CU per token on top of today's read, well inside research 0039's
monthly baseline.

### 7.2 New data and its cost (prices dated)

| data | unlocks | source and price | status |
|---|---|---|---|
| transaction traces (`debug_traceTransaction`, `trace_block`) | ETH funding links (S6 at c 8,000, S9, CEX windows) | QuickNode Build, $49/month, trace calls 200-1,000+ CU each (search snippets dated 2026; costbench, chainstack) | UNVERIFIED that chain 4663 is offered with trace on that tier; research 0045 §1 says the method docs exist, tier gating unread. Josh's sign-up |
| a Dune or Blockscout query layer | address-scoped funding walks without our own indexer | Dune $65+/month (research 0045, unmeasured); Blockscout API key (price not found, keyless blocked by Cloudflare, 0045) | UNVERIFIED |
| exchange hot-wallet labels for chain 4663 | CEX-window clustering | none published found today | not available |
| buyer index (which wallets bought which launches) | cross-token recurrence, the pre-aged-wallet counter | our own memory (`memory.rs`), one `Forever` row per buyer per launch | build, §8 |
| post-graduation pool address | S7 and S14 after graduation | research 0044 has not found it; `PonsV2LaunchLocker.sol` unread | research task |

### 7.3 Are X posts worth reading as a data-only signal?

For weighting: **no, not now.** The only weight a post can move is S1's
-200 self-reported lower, capped and tagged. Everything a dev can honestly
declare that matters (the bundle wallets) is declared on-chain in the
`launchToken` calldata (research 0048 §3), which is measured, free and not
fakeable. A post costs about $0.01 per account read (research 0051 §1), can
be back-dated only by lying, and is trivially gamed. Josh's note that
player-post logging may no longer be needed (rewards live on the site, ADR
0034 decision 3) removes the other reason to keep an X reader running. Keep
posts where they already are: mentions arrive as data, suggestions are
logged never obeyed (ADR 0034 decision 5), and token metadata is on-chain.
Revisit if the catalog (design 0027 §5) shows a post pattern that separates
rugs from survivors on both held-out splits.

## 8. Build order

Ids follow the run's `M-D-NNNN` sequence. Verification runs with
`REALORRUG_CARGO="cargo +stable-x86_64-pc-windows-gnullvm"` on this
machine, one crate at a time, single named tests only; suites run in CI
(`gh pr checks <n> --watch --interval 60`). Every task: no edits to `main`,
stage by path, the document that describes the behaviour changes in the same
commit. "Independent" tasks may run at the same time.

| id | owner | blocks on | allowed files | forbidden | verification | rubric (measured) | stop and ask if |
|---|---|---|---|---|---|---|---|
| **M-D-0001** noisy-OR fold and `Weight` type | implementer | -- (independent) | `crates/realorrug-roast/src/assessment.rs`, its tests; `docs/adr/0032-the-verdict-is-a-score-with-its-coverage.md` (one paragraph) | `verdict.rs`, `sheet.rs`, any contest or payout crate | `cargo check -p realorrug-roast`; `cargo clippy -p realorrug-roast -- -D warnings`; `cargo test -p realorrug-roast noisy_or -- --exact` for the named test | `noisy_or(&[3800,2700,1800]) == 6289` (after episode dedup) and `noisy_or(&[3800,2700,1800,1400]) == 6809` raw; permutation of input gives the same output; adding any weight never lowers the output; `u32` only, no `f32`/`f64` in the file (grep count 0); JSON gains `score_bps` and keeps `risk_index` | any existing pinned JSON key changes value; or the fold needs >u32 |
| **M-D-0002** factors on the sheet | implementer | 0001 | `crates/realorrug-roast/src/sheet.rs` (new `Factor { signal, name, delta_bps: i32, grade, evidence }`), `assessment.rs` (read factors), `docs/design/0020-robinhood-fact-sheet-and-voice.md` §3 table | `verdict.rs` level logic; onchain crate | `cargo check -p realorrug-roast`; named test `factors_never_raise_from_self_reported` | every `Factor` with grade `SelfReported` has `delta_bps <= 0` and the sum of S deltas per signal >= -200 (test); floor 25% of base enforced (test); `the_live_robinhood_sheet` fixture prints its factors with grades | the fixture's dev-buy share cannot be computed because the launch-block price read is missing → ask before adding an RPC call |
| **M-D-0003** link confidence and 30-block window | implementer | -- (independent of 0001/0002) | `crates/realorrug-onchain/src/wallets.rs` (`link_confidence(a, b) -> u16`, window constant), `robinhood.rs` (window logs) | roast crate; `memory.rs` schema | `cargo check -p realorrug-onchain`; named test `link_confidence_takes_strongest_not_sum` | table in §3.2 reproduced exactly; two wallets with transfer + same-block give 9,000 not 16,000; window is a named constant with the `snipeTaxSeconds` citation | the launch-block log read cannot be widened without paging (60 CU → more) |
| **M-D-0004** S13 power reads | implementer | -- (independent) | `crates/realorrug-onchain/src/robinhood.rs`, `pons.rs` (selectors), `dossier.rs` (new `Powers` struct) | roast crate | `cargo check -p realorrug-onchain`; named test against the captured fixture | `snipeTaxExempt`, `pendingCreatorFeeRecipient`, creator tax read for the fixture token; first-party list from research 0047 §3 applied; an exempt address off both lists is reported, one on the calldata list is reported as declared | the `launchToken` calldata decode does not match research 0048 §3's argument shape |
| **M-D-0005** level from score, shadow first | implementer, **independent reviewer required** (changes the level contract, a public surface) | 0001, 0002 | `crates/realorrug-roast/src/verdict.rs` (`level_from_score`, called beside `level`), `assessment.rs` (`admissible` from bands), templates | contest and payout crates | `cargo check -p realorrug-roast`; named test `two_no_factor_launch_signals_reach_rug_mechanics_live` | §4.2 bands exact; `Rugged` and `CantTell` unreachable from score (test); shadow mode: published level still `verdict::level` until a flag flips | the fixture that today reads `RugMechanicsLive` on S1+S3 must change expectation → confirm with Josh before flipping |
| **M-D-0006** calibration replay | researcher | 0001, 0002, 0003 | a new research 0053; a read-only replay script under `scripts/` | any crate source | the document lists per-band rug rate, Brier in bps, both held-out splits | >= 200 per arm for every weight kept; bands monotone; Brier beats the boolean ladder and the constant on both splits | bands are not monotone after the fit, or any arm is under 200 → the weight stays hand-set and the doc says so |
| **M-D-0007** band-keyed `q` | implementer, **independent reviewer required** (payment-adjacent) | 0005, 0006 | `crates/realorrug-contest/src/daily.rs`, `calls.rs`; design 0028 §odds | payout crate; any model-side crate | `cargo check -p realorrug-contest`; named test `dummy_players_sit_near_zero_on_band_q` | four dummies within the luck line over the replay window; `q` read from a table, no arithmetic on score; snapshot at pick time (test: recomputing the score after pick does not change settlement) | fewer than 200 replayed launches in any band → fall back to level `q` |
| **M-D-0008** S7 correlated selling | implementer | 0003 | `crates/realorrug-onchain/src/wallets.rs`, `robinhood.rs`; `sheet.rs` push | contest crate | `cargo check -p realorrug-onchain`; named test on a captured sell cluster | >= 3 linked wallets within 50 blocks fires; spread over > 1 h lowers by 300 | the post-graduation pool address is needed for the fixture → pre-graduation only |
| **M-D-0009** buyer index | implementer | -- (independent) | `crates/realorrug-onchain/src/memory.rs` (new table, `Forever`), `robinhood.rs` | roast crate | `cargo check -p realorrug-onchain`; named migration test | a wallet seen buying two launches is retrievable by address; schema change documented in design 0021 | the SQLite migration would touch existing `(what, subject, block)` rows |
| **M-D-0010** review | reviewer | 0001, 0005, 0007 | none (read-only) | all | reads the diffs and this document | every rule in the packet's HARD RULES has a test or a type that holds it; no float; no path from roast to payout | any rule holds only by prose |

Tracer bullet: **0001 → 0002 → 0005 (shadow)** is the thinnest slice that
scores a real sheet end to end and prints the number beside today's level.
0003, 0004, 0009 run alongside. 0006 then 0007 widen it into the daily five.

**Published level stays on the flag rules until calibration (owner, 2026-09-19).** The score runs in shadow; the published level is not switched to the score level until M-D-0006 has enough labelled launches to check the weights. Switching would move four pairs from `RugMechanicsLive` to `Sketchy` (S1+S3 2,080; S1+S5 2,256; S3+S5 2,080; S2+S3 2,350).

**M-D-0009 built (2026-09-19).** `crates/realorrug-onchain/src/memory.rs`
gained the `buyer_index` table (`Forever` in spirit, keyed `(chain, buyer,
token)`, `INSERT OR IGNORE` idempotent), `Memory::record_buy` and
`Memory::launches_bought_by`, documented in design 0021's new "Buyer index"
subsection. `robinhood.rs` writes into it from `dossier.funding.checked` --
the launch-window buyers `wallets::investigate` already reads -- once a
`funding` read succeeds and a memory is present; no new RPC read, and a
write failure is dropped rather than failing the sheet. The recorded amount
is `Candidate::bought_wei` (the quote in wei `wallets::investigate` already
computed), not a re-derived ERC-20 token count -- `wallets.rs` is outside
this task's allowed files, so no new decode of the launched token's own
amount was added; a later slice that wants that number reads it from
`wallets.rs` and passes it through the same `record_buy` call this task
added.

**M-D-S6-token-amounts built (2026-09-19), closing the gap above.**
`Trade::tokens` was already decoded per purchase in `realorrug-robinhood`'s
`pons.rs` and simply dropped by `wallets::purchases_from`; it now survives
onto `Purchase::tokens`, sums onto `Buyer::tokens` the way `quote` already
does, and reaches `Candidate::bought_tokens` (`Option<u128>` -- `None` on
Solana, whose candidates never had a decoded `Trade` to read tokens from,
never a fabricated 0). `robinhood.rs`'s `record_buyer_index` now passes
`Candidate::bought_tokens` through to `Memory::record_buy`, which stores it
in a new `buyer_index.token_amount` column added by a migration that does
not rewrite existing rows (design 0021's "Buyer index" section, updated in
the same commit) -- a buy recorded before this shipped reads back
`token_amount = None`, not 0. This task also added
`wallets::linked_holdings_bps`, the S6 primitive itself (§3.1's row below,
§3.2's formula): given a wallet's own share of supply and the strongest
[`link_confidence`] tying it to the wallet under investigation, it sums
`share_bps * confidence_bps / 10_000` across distinct wallets, counting a
wallet linked more than once at its strongest reading, never a sum of
several. No caller reads it yet -- S1's and S2's linked-wallet raises
(§3.1 rows S1, S2) are the intended callers, wired in a follow-up slice
that also carries the `Powers.exemptions`/`Funding.checked` reads those
factors need.

**The M-D-0006 sample is now being collected (2026-09-20).** §5's replay
needs labelled launches, and §4 of research 0055 establishes that this one
input cannot be reconstructed later: recomputing "what would we have said
then" from today's chain state is a different claim from "what we actually
said then". So the record starts ahead of the thing that reads it.
`crates/realorrug-onchain/src/memory.rs` gained two tables -- `verdicts`
(one row per verdict as published: chain, token, the block it was read at,
the level, the score in bps, the signals that fired, which surface served
it, and when) and `verdict_outcomes` (one row per token: `rug`, `failed` or
`alive` by §5's definition, the block it was observed at, and one line of
evidence so a disputed label can be re-checked against the chain rather than
trusted). `Memory::labelled_verdicts` joins them, inner join only: a verdict
with no outcome yet is not half a pair, and counting it as one would hand
the fit an unlabelled launch as whatever the missing label defaulted to
(AGENTS.md rule 8). An outcome is `INSERT OR REPLACE` keyed on the token, so
a launch called `alive` in May and `rug` in June is one launch that rugged,
and every verdict already recorded re-pairs against the newer label.

Both serving surfaces write a record: the free checker route
(`crates/realorrug-serve/src/check.rs`) and the paid facts endpoint
(`crates/realorrug-serve/src/facts.rs`), tagged `"check"` and `"facts"`, so
a later fit can hold one surface out -- the two see different tokens for
different reasons. The record keeps the **score** as well as the level even
though ADR 0036 decision 1 publishes neither: the whole question M-D-0006
settles is whether score bands beat the boolean ladder, and a level-only
record could never test it. No published surface changed -- the level stays
on the flag rules, per the owner's 2026-09-19 hold above, and nothing reads
the pairs yet.

**The labelling side (2026-09-20).** `realorrug label-outcomes` closes the
pair. It takes the tokens that were judged at least `--days` ago (7 by
default) and have no outcome yet, oldest first and capped by `--max`,
re-reads each through the ordinary dispatcher, and writes what
`crates/realorrug-onchain/src/outcome.rs` makes of it. That module is pure
and holds §5's definitions:

- **`rug`** when the creator sold back at least 5,000 bps of the tokens they
  bought across a trade history read whole, or when the curve emptied before
  graduation while holders were still in it.
- **`failed`** when the curve emptied before graduation with nobody left.
- **`alive`** when the curve has not completed and still holds quote reserves.

Two whole cases return **no label at all** rather than a guess, and the
command leaves them in the queue for a later run. A **graduated** launch:
after graduation the curve holds nothing by design, so its empty reserves
say nothing, and the AMM pool that does hold the money has no address we can
find on Pons v2 yet (research 0044) -- which also makes §5's third rug rule,
reserves falling ≥ 9,000 bps in the hour after graduation, not measurable
here at all. And a launch whose **curve or holders could not be read**: an
absent read is not a clean reading. The asymmetry is deliberate (rule 8): a
wrong `alive` is worse than no row, because the fit counts it as a control
and learns that the signals which fired on it meant nothing.

The creator rule under-calls on purpose. Its denominator is what the creator
**bought**, so a free allocation dumped without any purchase is invisible to
it, and `trades_complete` gates the whole rule, so a partial history never
calls a rug. A missed rug costs one sample; a fabricated one poisons the
fit.

§5's 200-per-arm minimum is still a count of pairs that do not exist today:
the queue only fills as the serving surfaces record verdicts, and the first
of those can be labelled seven days after it was served.

## 9. Where this is weak

- **False precision.** Every base and delta in §3 is hand-set today; the
  bps look exact and are not. Until §5 runs, the sheet should print the
  score with the word "provisional" and the level should still come from
  `verdict::level` (0005 shadow mode). An alternative that avoids the
  problem: three ordinal sizes per signal (small, medium, large) with the
  same noisy-OR over three fixed weights. Cheaper, less gameable, coarser.
- **Gaming.** The thresholds are public in this repository. A dev will sit
  at 499 bps, at 31 blocks, at 11% size spread. Counter: the replay can move
  thresholds without a code change if they live in a measured snapshot
  (`baserates.rs` already does this for bands); and the strongest signals
  (S7, S10, S11, S12) are about acts, not shapes, and cannot be sat under.
- **Thin data.** 350 launches, 1.2% graduation (3 of 250), 298 deployers.
  Most weights will not reach 200 per arm for months. The plan tolerates
  this by keeping weights hand-set where the sample is thin and saying so on
  the sheet (§3.1's "thin sample" lowers).
- **The sniper twin.** Wallets buying in the first seconds may be third-party
  bots, not the dev. On Pons v2 the 9,900 bps snipe tax makes that costly and
  rare, but "rare" is an inference until the replay counts exempt versus
  taxed early buys.
- **Solana thresholds ported.** Bubblemaps' 10%, MELT's 36.5%, GMGN's
  clusters of 3 are all Solana pump.fun numbers. Nothing in §2 was measured
  on an Orbit chain with 0.1 s blocks and a snipe tax.
- **Nothing here catches case D before it acts.** Only traces would, at
  $49/month plus the walk cost, and even then a CEX hop hides the person.
  The document says so rather than pretending a shape signal is a link.

Alternatives considered and set aside: a learned model publishing the band
(design 0027 §5 keeps it in shadow; ADR 0032 decision 3); keeping the
boolean ladder and only adding magnitude facts to the reply (cheapest, and
it answers none of Josh's three cases differently); likelihood-ratio
multiplication (design 0027 §"Judgement" rejected it for manufacturing
confidence from correlated observations, which is also why §4 deduplicates
episodes before the fold).

## Confidence and what would change it

CHECKED: §1 file:lines; the noisy-OR arithmetic in §4 and §6 (done by hand
in whole numbers, reproducible by the 0001 test); Bubblemaps' definitions
and 10% threshold (fetched, 2026-05-06); the MELT and "Catching the Rug"
abstracts (fetched, 2026-05-21 and 2026-08-20); Pons v2 mechanics as cited
from research 0044/0047/0048.

UNVERIFIED: every base weight and delta in §3 (hand-set; §5 is the test);
GMGN and RugCheck method details (search snippets); QuickNode's tier and
chain support for traces on 4663 (search snippets dated 2026, not the
vendor page); the Chaos Labs figures (X posts via snippet); Arkham and
Trench (not read); whether a 30-block window has an innocent twin on Pons
v2 beyond the sniper case.

What would change the recommendation: a replay showing score bands are not
monotone in rug rate (then the weights are wrong, not the rule); a trace
source under $50/month with address-scoped funding walks on chain 4663
(then S6 and S9 become measured and case D changes); a measured count of
early taxed buys showing third-party snipers are common on Pons v2 (then S2
and S8 lose weight); Josh deciding that ordinal sizes are enough (then §4
still applies, over three weights per signal).

QUESTIONS: none reopen a packet decision. One note: ADR 0032's "532,226
launch outcomes" is a Solana-era count; the Robinhood replay in §5 starts
from 350, and this document plans for that.
