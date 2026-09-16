<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0045 — Wallet analysis and address clustering

**Date:** 2026-09-15
**Status:** desk research, CHECKED tier. No new JSON-RPC calls were made this
session — costs below are derived from Alchemy's/QuickNode's per-method rates
and the 10,000-result/getLogs paging behaviour already measured in research
0038/0039 (both cited by path, not re-run), applied to the method shapes
described here. Vendor pages and search-engine summaries are dated where
fetched; where a source was read only as a search-result summary rather than
the primary page itself, that is said plainly and graded lower. No live
Robinhood Chain data (funder distributions, gas-price histograms, AA usage)
was pulled — that gap is named in §6, not silently assumed.

**Reading the paths below:** a code span written `theradar:path/to/file` names
a file in [theradar](https://github.com/1xmint/theradar), not in this
repository. Everything without that prefix is a path here.

## 1. Method table

| method | published source | peer-reviewed / vendor / other | data it needs |
|---|---|---|---|
| **Common-funder graph** (cluster wallets that received their first funds from the same upstream address) | Used by Chaos Labs + Nansen for the LayerZero Sybil bounty (2024): "primarily rel[ies] on source of funding analysis heuristics," marks "the first native token transfer to each account as the funding transaction," and for "every anomalous funding event from a CEX, DEX, or bridge... identifies... the three most significant clusters" (paraphrased from Omer Goldberg/Chaos Labs' X thread, 2024-05-18, read here only via a search-engine summary of that thread, not the primary post — **secondary, not primary**). Silhouette-score-gated cluster validity (>0.7) is part of the same methodology. | **VENDOR** (Chaos Labs / Nansen, commissioned by LayerZero Labs Foundation) | Per-address "first funding transaction" — needs a full address-indexed history or a trace API, not a bare `eth_getLogs` scan (see §2) |
| **Funding-source clustering (general form)** | "wallets sharing a common funder are likely controlled by the same entity" — general description of the technique, from search-aggregated sources, not one paper | **secondary summary**, not a primary citation | same as above |
| **Compression-based behavioural similarity (no funding link required)** | "Compression-Based Behavioral Similarity for Open-World Sybil Discovery on Ethereum," arXiv:2607.27370 (fetched 2026-09-15). Abstract framing, quoted: can it "differentiate Sybil bots, organic users, and arbitrage bot wallets **without direct financial links**?" — explicitly positioned as *not* needing a funding graph, using gzip-based Normalized Compression Distance over EVM trace "Transaction Grammar" instead | **preprint** (arXiv, not confirmed peer-reviewed as of 2026-09-15) | Full EVM execution traces per wallet — heavier than log-based methods, not costed here (out of budget for this document; flagged as a direction, not evaluated) |
| **Subgraph-based feature propagation for Sybil labels** | "Detecting Sybil Addresses in Blockchain Airdrops: A Subgraph-based Feature Propagation and Fusion Approach," arXiv:2505.09313 — title/abstract only read via search result, not fetched | **preprint**, not fetched in full — **named, not verified in depth** | graph features over an address's transaction subgraph |
| **TrustScan (Trusta Labs)** | Public GitHub repo `TrustaLabs/Airdrop-Sybil-Identification`; "a 2-phase approach first uses graph mining algorithms to detect coordinated communities, then refines results with user behavior analysis to reduce false positives" (paraphrased from search summary of the repo/Medium post, not the code itself read line by line) | **VENDOR**, open-sourced methodology (unusual for a vendor — code is public) but not independently re-derived here | Graph-mining pass over a full transaction graph, then a behavioural-feature refinement pass |
| **Gitcoin Passport / Human Passport sybil resistance** | gitcoin.co and human.tech blog posts (search-summarized, not fetched); "on-chain activities that cost gas do not scale well for Sybils... may well already exist for many... users" — a cost-asymmetry argument for identity/attestation-based resistance, not primarily a funding-graph technique | **VENDOR** (Gitcoin/Human.tech's own description of their own product) | Off-chain identity attestations plus on-chain activity signals — **not the same shape of method** as common-funder clustering; included because the packet named it, but it answers a different question (identity proof, not address linkage) |
| **Timing correlation (same-block / near-block buys)** | Radar's own launch-block bundle signal, `theradar:crates/radar-graph/src/lib.rs`, measured in theradar research 0008 and re-measured in 0024 (cited via research 0042 §"the table", read 2026-09-15) — a *recipient-count* version of timing correlation, not funder-based | **measured, first-party (Radar)**, portable in shape per 0042 | Distinct token-transfer recipients inside one launch block — no funding lookup at all |
| **Gas-price / nonce fingerprints** | No measured, published study located this session (search covered general ERC-4337/wallet-tooling docs, not a fingerprinting study). ADR 0027 itself names "a fresh wallet" (i.e., low nonce) as a signal with a stated innocent twin ("a fresh wallet is a sniper or somebody's first day") — that is a design decision, not a measurement | **never measured anywhere found this session** — say so in those words, per the packet's instruction | Transaction `gasPrice`/`maxFeePerGas` and sender `nonce`, both already present on any tx object already being pulled for other reasons |
| **Shared approval targets** | No measured, published study located this session. Mechanically well understood (ERC-20 `Approval` events name a spender) but not found cited as a *sybil-detection* signal in any source read today | **never measured anywhere found this session** | `Approval` event logs, same shape as `Transfer` logs |
| **Round-number funding** | No measured, published study located this session | **never measured anywhere found this session** | The funding transaction's `value` field |
| **"Funded from the same CEX withdrawal" case** | Same LayerZero/Chaos-Labs-Nansen methodology as row 1: "over 50% of funders" were CEX/DEX/bridges, and clusters were built explicitly around CEX-funded cohorts (e.g. "a cluster of 100+ LayerZero users... funded by FTX" in a 24-hour window) — same secondary-source caveat as row 1 | **VENDOR** | A labelled list of CEX/bridge hot-wallet addresses (third-party attribution data) plus the funding-transaction lookup |
| **Common Input Ownership Heuristic (UTXO-style clustering)** | Arkham Intelligence's own guide pages describe their entity clustering as using "address clustering techniques like the Common Input Ownership Heuristic" (search-summarized from Arkham's own how-to-use guide, not the primary page fetched directly) | **VENDOR** (Arkham) | This heuristic is UTXO-native (it identifies co-signed inputs in one Bitcoin-style transaction). **It does not transfer to EVM as written** — an EVM transaction has exactly one sender, no multi-input co-signing to observe — Arkham's EVM-side clustering necessarily uses a different, undocumented technique layered with off-chain OSINT ("social media, forum posts, public court records"); the published heuristic name is real but the mechanism it names is the wrong shape for this chain |

## 2. Per-method cost for us

All costs below use the already-measured Alchemy compute-unit table and
QuickNode credit rate from research 0039 §2 (`eth_call` 26 CU / ~20 credits,
`eth_getLogs` 60 CU / ~20 credits, `eth_getTransactionReceipt`/
`eth_getBlockByNumber` 20 CU / ~20 credits, both read 2026-09-15), applied
here to method shapes, not re-measured against live Robinhood Chain traffic.

| method | calls per token/address | credits (QuickNode, ~20/call flat rate per 0039) | CU (Alchemy) | can we afford it in the hot path |
|---|---|---|---|---|
| Timing correlation (same-launch-block recipients) | **0 extra calls.** Already read from the `Transfer` logs pulled for req #1 (per-launch index, 0039) | 0 marginal | 0 marginal | **yes — free, reuses existing index** |
| Gas-price / nonce fingerprint | **0 extra calls**, if we already pull the full tx object for the launch/buy tx (we do, per 0039's per-launch index); +1 `eth_getTransactionByHash`-equivalent (≈20 CU / ~20 credits) per address if we don't | 0–20/address | 0–20 CU/address | **yes, effectively free as a byproduct** |
| Shared approval targets | 1 `eth_getLogs` per token contract for its `Approval` topic, same shape and cap as the `Transfer`-log read already budgeted in 0039 | ~20/token | 60 CU/token | **yes, same order of magnitude as req #1's existing per-launch log read; additive, not a new category of cost** |
| Common-funder graph, **restricted to ERC-20-token-sent funding** | 1 `eth_getLogs` per candidate address, filtered on the `Transfer` topic's recipient-topic slot with no contract-address filter, paged from genesis to tip. Feasible in principle (topic filters don't require a contract address) but the block-range paging cost scales with chain age exactly like the from-genesis backfill 0038/0039 already flagged as expensive (171,000-launch backfill, "already true that free-tier polite calling can't sustain it") | tens to low hundreds of credits per address, dominated by page count, not measured this session | same shape, unmeasured page count | **conditional — affordable per address, not affordable as a population-wide scan; only usable narrowly (e.g. on the creator wallet and the top N buyers of one flagged token), never as a background job over every wallet** |
| Common-funder graph, **native-ETH funding (the dominant funding path per §3)** | **Not obtainable from `eth_getLogs` at all** — a plain ETH transfer emits no log. Needs `debug_traceTransaction`/`trace_filter`-class calls scanning by address, or a full address-indexed explorer API (Blockscout-shaped). Neither is priced: Blockscout was blocked in 0039 §3 (Cloudflare 403, keyless), and no vendor's trace-API per-call price was read this session (a real gap, named in §6) | **not established** | **not established** | **no — this is the "first funding needs a full-chain index or a trace API" case the hard constraint told us to say plainly. We cannot afford, and in one case (Blockscout) cannot even reach, the data this needs today.** |
| "Same CEX withdrawal" clustering | Needs the common-funder lookup above (native-ETH case, mostly) **plus** a labelled CEX/bridge address list — the label list itself is third-party risk-API-shaped data, explicitly excluded from the hot path by the packet's hard constraint (no third-party risk API in the hot path) | inherits the "not established" cost above, plus an excluded dependency | inherits above | **no, twice over: blocked by both the funding-lookup cost above and the hard constraint against third-party risk data in the hot path** |
| Round-number funding | Same lookup as common-funder graph (needs the funding tx's `value`); no incremental cost once that tx is found, but inherits that tx's cost above | inherits above | inherits above | **inherits the funding-lookup limitation; free once you already have the funding tx, otherwise blocked the same way** |
| Compression-based behavioural similarity (arXiv:2607.27370) | Needs full EVM execution traces per wallet (`debug_traceTransaction`-class calls, one or more per tx analysed) | **not costed** — out of scope depth for this pass; flagged, not evaluated | **not costed** | **not evaluated — a trace-heavy method on a budget that has not priced trace calls at all is not assessable here** |

**The one clean line this section draws:** everything that reads from
`Transfer`/`Approval` logs or from tx objects we already pull for the
per-launch index (timing, gas/nonce, approvals, token-funded-only
common-funder) is affordable, because it rides on infrastructure research
0039 already recommends building. Everything that needs "who sent this
wallet its very first ETH" from cold — the literal common-funder/CEX-
withdrawal case in its general form — is **not** affordable today: it needs
either a trace API (unpriced, unmeasured) or an address-indexed explorer
(Blockscout, blocked). This is the same "first funding on an EVM chain
normally needs a full-chain index or a trace API" statement the packet asked
for, stated plainly rather than glossed over.

## 3. The custodial-funding wrinkle, answered

**What is documented, read 2026-09-15:** Robinhood's own support article
(`robinhood.com/us/en/support/articles/robinhood-chain-mainnet/`) states
users "bridge ETH and supported ERC-20 tokens to Robinhood Chain using the
canonical Arbitrum bridge or partner bridging routes," that "Robinhood Wallet
natively supports Robinhood Chain with no manual setup required," and — the
load-bearing sentence for this section — that "Robinhood Chain operates
independently of the main Robinhood app and doesn't affect your investments,
crypto balances, or portfolio." Separately (search-summarized vendor/press
copy, not the primary Robinhood page — **secondary**), "Robinhood is covering
gas for eligible Robinhood Wallet users during the first 90 days after
launch, including bridging in."

Two things follow, and they pull in different directions:

1. **The packet's premise as literally stated — brokerage custody itself
   funding wallets — is not confirmed by what was read today.** The support
   page explicitly disclaims a link between the brokerage's custodied crypto
   balances and Robinhood Chain activity. This document did not find, and
   did not have budget to chase further, the actual contract address(es) the
   canonical Arbitrum bridge or Robinhood's gas-sponsorship arrangement use
   on Robinhood Chain mainnet (chain id 4663). That is named in §6, not
   assumed either way.
2. **A functionally identical shared-rail problem is documented regardless
   of the word "custody."** A canonical bridge contract, a partner-bridge
   router, and (if RH's 90-day gas sponsorship works the way sponsored-gas
   products usually do) a paymaster/relayer address are each, by
   construction, a single upstream address that funds very large numbers of
   otherwise-unrelated new wallets. This is the same shape as a custodian
   even if it is not literally Robinhood's brokerage ledger, and it is the
   shape that breaks a naive common-funder signal.

**Verdict on the question asked: an infrastructure floor in the shape of
Radar's `INFRASTRUCTURE_FLOOR`/`REPEAT_FLOOR` (research 0042, theradar
research 0012/0013) is the right *shape* of answer, but a numeric floor alone
is not sufficient here — it needs to be paired with a named-address
exclusion list, exactly as Radar itself did.** Radar's own measured system
did not rely on the floor by itself: it excluded "13 router/fee-sink
addresses covering 42% of launches" *by name*, in addition to the ≥100
numeric floor (0042, quoting `theradar:radar-graph/src/prevalence.rs`). The reasoning
ports directly: on a young chain where a small number of first-party
addresses (the canonical bridge, a partner-bridge router, a gas-sponsorship
relayer) plausibly fund a large share of all new wallets — the LayerZero
precedent found "over 50% of funders" were CEX/DEX/bridge addresses on an
*established* multi-year ecosystem (§1, row "same CEX withdrawal case") — a
purely numeric floor computed from our own observed data would need to climb
into the tens of thousands of distinct recipients before it separated "the
bridge" from "a merely popular router," by which point the floor has already
silently absorbed months of real signal as noise. Naming Robinhood's own
bridge/relayer addresses up front (first-party, documented, not a
third-party risk API, so it does not conflict with the hard constraint) and
excluding them **before** the numeric floor runs is cheaper and more honest
than waiting for our own data to discover them the way Radar's floor implies
it originally had to.

**Where custodial/bridge funding breaks the signal outright, not just
dents it:** for the *native-ETH, first-hop* form of common-funder clustering
specifically (§2's "not obtainable from `eth_getLogs`" row), the practical
effect of a dominant shared-funding rail is that even if we could afford the
trace-API lookup, the answer for a large share of wallets would resolve to
"the bridge" or "the relayer" and stop there — one hop is not enough
information on this chain. The LayerZero methodology's own response to this
exact problem was to go **one hop further**: cluster on who funded the
*bridge transaction*, or restrict the signal to wallets whose first ETH did
**not** come from a known bridge/CEX/relayer contract at all (shrinking the
addressable population, unmeasured by how much here). Either fix costs more
calls than the floor-plus-exclusion-list approach above. **Conclusion:
infrastructure-floor-plus-named-exclusion is the right shape for the signals
we can actually afford (log-based, ERC-20-funded, same-launch-block); the
literal first-hop common-funder/CEX-withdrawal signal is broken outright by
custodial/bridge rails on this specific chain, not merely noisy, until we can
afford a second hop or a trace API — neither of which is priced today.**

## 4. False positives, per method, each with its innocent twin

- **Common-funder graph (ERC-20-funded subset, the only affordable slice).**
  *Innocent twin:* two genuinely unrelated retail users who both funded their
  wallet from the same on-ramp, bridge router, or (per §3) Robinhood's own
  gas-sponsorship relayer — on a retail-first, young-chain product this could
  be a majority of all wallets, not an edge case (documented direction, not a
  measured Robinhood Chain fraction — see §6). *Expected false-positive
  behaviour:* without the named-address exclusion in §3, the single largest
  "cluster" in a naive run is not a coordinated actor at all, it is
  infrastructure.
- **"Same CEX withdrawal" clustering.** *Innocent twin:* the general
  version of the case above, specifically two strangers who both withdrew
  from the same exchange (including Robinhood's own brokerage, if that link
  is ever confirmed) — extremely common, and per §2 this method additionally
  needs third-party label data the hard constraint excludes from the hot
  path, so it is doubly unaffordable, not just noisy.
- **Timing correlation (same-block / near-block buys).** *Innocent twin,*
  stated in ADR 0027 itself: "same-block buys are a bundle or a launch
  people waited for." A fair, free, widely-anticipated launch produces the
  same shape (many independent buyers racing to be first) as a coordinated
  bundle. Radar's own scoring code states the general form directly
  (quoted in 0042): "`Suspected` fires on 13% of all launches and carries a
  four-fold enrichment; that is worth recording and too blunt to refuse on
  by itself."
- **Gas-price / nonce fingerprints.** *Innocent twin:* wallet-software
  defaults. Most wallets suggest an RPC-derived gas price by default, so a
  cluster of identical gas prices is frequently a shared wallet app or a
  shared RPC endpoint's fee suggestion, not shared control. A low nonce is,
  per ADR 0027's own example, "a sniper or somebody's first day" — brand-new
  wallets are the overwhelming majority of participants on any young launch
  venue, so low-nonce alone has almost no discriminating power. **This
  method was never measured anywhere found this session** — stated per the
  packet's instruction, not softened.
- **Shared approval targets.** *Innocent twin:* everyone who trades on Pons
  v2 approves the same curve/router contract, because that is the only way
  to trade on it — approval-target overlap is close to universal among active
  traders and must itself be treated the way Radar treats router addresses
  (excluded, not scored), or the signal simply measures "did this wallet
  trade here," which is not evidence of common ownership. **Never measured
  anywhere found this session.**
- **Round-number funding.** *Innocent twin:* a human manually typing "0.1
  ETH" or "$50" into a bridge or on-ramp UI — round numbers are the natural
  default for a retail user, not a farm signature; if anything, automated
  farms have more reason than genuine users to avoid round numbers, to evade
  exactly this heuristic. **Never measured anywhere found this session**,
  and the innocent-twin rate plausibly exceeds the true-positive rate on a
  retail-first product — a caution, not a finding, since neither rate was
  measured.
- **Exchange hot wallets (as funders, not as the entity being clustered).**
  *Innocent twin:* an exchange hot wallet routinely sends thousands of
  small, similarly-shaped payouts to unrelated customers; any funding-graph
  signal that does not special-case known hot-wallet addresses will report
  every pair of that exchange's customers as "linked," which is exactly the
  §3 infra-floor problem in a different guise.
- **Routers / relayers / paymasters / ERC-4337 bundlers.** *Innocent twin:*
  by design under EIP-4337 (`eips.ethereum.org/EIPS/eip-4337`, the published
  spec itself, not a vendor page), a single global `EntryPoint` contract and
  a small number of paymasters sponsor and relay UserOperations for very
  large numbers of unrelated smart-contract accounts — this is the intended,
  documented behaviour of account abstraction, not an artifact. Any
  funding-graph or gas-payer signal that does not special-case `EntryPoint`
  and known paymaster addresses will cluster thousands of strangers who
  share nothing but a wallet-app choice. **Whether Robinhood Chain/Pons v2
  traffic actually uses ERC-4337 account abstraction at all was not checked
  this session** — named in §6, not assumed either way.
- **Airdrop farmers who are not the creator.** This is the one row where the
  "false positive" runs the other direction: funding-cluster and
  repeat-launcher signals are *designed* to catch exactly this population,
  so the risk is not a false positive on the farm — it is a false positive
  on a **genuine, prolific, legitimate repeat creator or power-trader**, who
  produces the identical funding/repeat-participation shape as a farm.
  Radar's own creator-track-record work (theradar research 0007, cited in
  0042) exists specifically because funding/repeat-pattern signals alone
  cannot separate the two — only outcome history (did this creator's past
  launches organically graduate) can, and that is a different signal from
  everything in this document.

**Policy note, tied to ADR 0027 rule 2 and AGENTS.md §3 rule 4.** Every
method above, by construction, tends toward identifying a *specific actor
behind a cluster of addresses* — that is the point of clustering. ADR 0027
is explicit that a verdict may describe "the token and the launch
behaviour," never a named person, account or company as a scammer or thief,
and AGENTS.md §3 rule 4 carries the same restriction forward. A correctly
computed cluster is still only evidence that a set of addresses behaves as
one actor; it is never itself permission to name who that actor is or assert
intent. This is a policy constraint on top of the technical one, and it
applies to every row in §4, not only the ones with weak signal.

## 5. Recommendation

**Build repeat-address prevalence — the same signer wallet appearing across
N distinct launches within a rolling window, with a first-party named-address
exclusion list (Robinhood's own bridge/relayer/gas-sponsorship contracts)
applied before a numeric infrastructure floor modelled on Radar's measured
`REPEAT_FLOOR`/`INFRASTRUCTURE_FLOOR` (theradar research 0012/0013, via
0042) — first, and not literal common-funder or CEX-withdrawal clustering,
because it needs zero RPC calls beyond the per-launch index research 0039
already recommends building, has a twice-measured precedent on a comparable
venue, and is the one candidate in this document that is not broken outright
by Robinhood Chain's shared bridge/gas-sponsorship funding rails (§3), unlike
every funder-lookup-based method in §2's "not affordable"/"broken outright"
rows.**

## 6. Not established

- Whether Robinhood's brokerage-custodied crypto balances are themselves a
  direct on-chain funding source for Robinhood Chain wallets, as opposed to
  the canonical Arbitrum bridge and the separate self-custody Robinhood
  Wallet app — the one primary page read today (robinhood.com support
  article, 2026-09-15) states the two systems "operate independently,"
  which cuts against the packet's premise as literally worded; this was not
  chased further (would need Robinhood Chain's own bridge/relayer contract
  addresses, not found this session).
- The actual contract address(es) for Robinhood Chain's canonical bridge,
  any partner-bridge routers, and the 90-day gas-sponsorship relayer/
  paymaster — needed to build the named-exclusion list §3 recommends, not
  looked up this session.
- What fraction of Robinhood Chain wallets are actually funded via a
  shared bridge/relayer path versus organically — no on-chain measurement
  was run; §3 and §4's "plausibly a majority" language is reasoning by
  analogy to LayerZero's "over 50% of funders" finding on a different,
  older chain, not a Robinhood Chain measurement.
- Whether Robinhood Chain or Pons v2 traffic uses ERC-4337 account
  abstraction (bundlers/paymasters/`EntryPoint`) at all — not checked.
- Any vendor's per-call price for trace-API methods (`debug_traceTransaction`,
  `trace_filter`) on Robinhood Chain — not priced in research 0039, not
  priced here; this is the direct blocker on the native-ETH common-funder
  signal (§2).
- The LayerZero/Chaos-Labs/Nansen methodology (§1, §3, §4) was read via a
  search-engine summary of Omer Goldberg's 2024-05-18 X thread, not the
  primary post itself — graded secondary throughout this document for that
  reason, not upgraded to a direct quote.
- Two named preprints (arXiv:2607.27370, arXiv:2505.09313) were read only at
  abstract/title depth; neither was evaluated for the accuracy of its
  results, only for what method shape and data requirement it claims.
- Arkham's and Nansen's full entity-clustering pipelines beyond the specific
  claims quoted in §1 — both are commercial black boxes; what is described
  here is only what each vendor states publicly about its own method, not an
  independent verification of it.
- No live Robinhood Chain gas-price, nonce, approval-target, or funding-value
  distribution was pulled this session — every false-positive rate discussed
  in §4 is a mechanism-level argument (why the innocent twin exists), not a
  measured base rate on this chain. Per AGENTS.md §1, zero measurement here
  is a statement about this document's instrument (no RPC calls made), not a
  claim that these effects are absent or small.

## Confidence and what would change it

**CHECKED for the method landscape and the cost/afford­ability analysis**
(sourced to research 0038/0039/0042's already-measured facts and to dated,
quoted vendor/spec pages); **CONDITIONAL** for the custodial-funding verdict
in §3, conditional on the not-yet-confirmed link between Robinhood's
brokerage custody and Robinhood Chain funding addresses. What would change
it: (1) finding and reading Robinhood Chain's actual bridge/relayer contract
addresses and their observed recipient counts, which would turn §3 from
reasoning-by-analogy into a measured number; (2) pricing a trace-API call on
one of the named vendors, which would move the native-ETH common-funder row
in §2 from "not established" to a real affordability verdict; (3) fetching
the primary Chaos Labs/Nansen LayerZero writeup directly rather than via a
search summary, which would let §1/§3/§4's vendor citations be quoted rather
than paraphrased.
