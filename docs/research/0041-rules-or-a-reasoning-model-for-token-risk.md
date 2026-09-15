<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0041 — Engineered rules vs a reasoning model for token risk facts

**Date:** 2026-09-15
**Status:** questions 1, 2, 3 answered from primary/secondary sources found today;
question 4 answered narrowly (one paper name found, full text not accessible).
No sign-ups, no keys, no spend; read-only web research plus two live pricing
pages. Every quote below is dated 2026-09-15 (today's reads) unless the source
itself carries an earlier publication date, which is then noted.

## 1. How the established checkers decide

**RugCheck (rugcheck.xyz)** — documented, primary. Its own about page says:
"Collect all relevant on-chain data, attribute it to risk metrics, and deliver
a scoring system that treats every token exactly the same" and "RugCheck will
NEVER adjust a score for any single token or project. The algorithm runs the
same for everyone." (https://rugcheck.xyz/about, read 2026-09-15). That is a
description of a fixed, deterministic scoring algorithm, not an LLM or a
trained classifier — the emphasis is explicitly on *sameness* of output for
the same input, which a generative model does not guarantee run to run.
Secondary sources (Solana Tracker docs, aggregator write-ups, not RugCheck's
own text) describe "more than twenty" individual checks: mint authority,
freeze authority, top-10 holder concentration, LP lock status, and
creator/dev wallet identity (https://docs.solanatracker.io/guides/token-safety
and https://github.com/aethernet404/rugcheck, read 2026-09-15) — **inferred
signal list, not confirmed against RugCheck's own docs**, which are gated
behind a Swagger UI (api.rugcheck.xyz) without a public methodology page found
today.

**GoPlus Security** — documented, primary, most detailed of the six. Its
Token Security API response-details page
(https://docs.gopluslabs.io/reference/response-details, read 2026-09-15) lists
individually named boolean/enum fields, each a fixed rule against decoded
bytecode or contract state, e.g.:
- `is_open_source` — "1" if source is public, else risk items return null
- `is_mintable` — "Mint functions can trigger a massive sell-off"
- `owner_change_balance` — "the owner can change token holder balances"
- `hidden_owner` — "used by developers to maintain ownership ability even
  after abandoning ownership"
- `can_take_back_ownership`, `is_proxy`, `owner_address`

These are exact-match code checks (decompiled bytecode patterns, storage
slots), not a trained model output — each has a stated precondition
("will not be returned if `is_open_source` is 0") that only a rule engine, not
an ML score, would express. GoPlus also runs a **separate** "Rug-Pull
Detection API (Beta)" endpoint alongside the general Token Security API
(same docs site, endpoint list read 2026-09-15) — evidence GoPlus itself
treats rug-pull-specific inference as a distinct product from static-rule
contract security, though the Beta's own methodology page was not reached
today. **Documented**: field-level rules exist and are the bulk of the
product. **Not established**: whether the Beta rug-pull endpoint or GoPlus's
"Security Compute Layer" (mentioned in its whitepaper index,
https://whitepaper.gopluslabs.io, read 2026-09-15, full methodology page
404'd) adds ML on top.

**Token Sniffer** — documented, primary (readme.io API docs). "Analyzes both
smart contract source code and bytecode against a database of over 10,000
scam code patterns built from five years of research"
(https://tokensniffer.readme.io/reference/introduction, read 2026-09-15) —
explicit pattern-database matching, i.e. rules built from accumulated known
bad code, not a general classifier or LLM. Its "Smell Test" produces a score;
docs name no ML or LLM component.

**De.Fi scanner** — no primary methodology doc found (De.Fi's own docs were
not reachable in this session); secondary write-ups agree it is static
bytecode/ownership analysis: renounced-ownership check, admin-function
detection (`mint`, `pause`, `setFees`, `setBlacklist`), LP-lock detection,
holder concentration (https://de.fi/blog/rug-pull-checker-scanner-exploits and
https://chainaware.ai/blog/best-web3-rug-pull-detection-tools-2026/, read
2026-09-15). **Evidence label: inferred from secondary sources**, not De.Fi's
own docs.

**Bubblemaps** — documented, primary (Bubblemaps' own blog). Two named,
separately defined mechanisms, both graph/timing rules, not ML: a **bundle**
is "a set of wallets that transacted together during the token launch:
typically all buying in the same block or within the same snipe window"; a
**cluster** is "a set of wallets linked by on-chain behavior over time: shared
funding sources, coordinated transfers, or recycled addresses"
(https://blog.bubblemaps.io/whats-the-difference-between-bundle-cluster-2/,
read 2026-09-15). Bundle detection is timing-based (same block/window);
cluster detection is a funding-graph walk (shared source wallets, transfer
edges, cross-token recurrence). Neither description mentions a trained model.

**Cross-tool pattern, inferred**: every one of the five vendors with a
reachable primary doc (RugCheck, GoPlus, Token Sniffer, Bubblemaps, and
De.Fi's public description) frames its method as fixed rules over decoded
on-chain state — authority flags, holder-percentage thresholds, pattern-DB
matches, block-timing windows, funding-graph walks. None of their own
documentation claims an LLM or a general ML classifier as the primary
mechanism. An academic survey characterizes the whole commercial category the
same way from the outside: "Commercial scanners such as TokenSniffer,
RugCheck, RugDoc, and ChainAware.ai rely on heuristic rule sets with varying
degrees of proprietary ML integration, but do not disclose benchmarking
methodologies" (paraphrased from a WebSearch summary of an arXiv-adjacent
source found 2026-09-15 for the "How To Cook The Fragmented Rug Pull?"
literature review context — **this exact sentence could not be re-confirmed
against the primary PDF, arXiv 2511.15463, when fetched directly today; the
PDF's accessible text did not contain it**. Treat this one sentence as
**unverified / possibly a search-summarization artifact**, not as sourced
fact — flagged rather than deleted, per the standing "say when you were
wrong" rule.)

## 2. Published evidence for LLMs/reasoning models on this task (2024–2026)

**Peer-reviewed, on-topic, with numbers — best evidence found:**

- Cui et al. (arXiv:2501.18158v1, posted 2025-01-30, preprint, not shown to be
  peer-reviewed at a venue), "Large Language Models for Cryptocurrency
  Transaction Analysis: A Bitcoin Case Study." On raw-graph inputs GPT-4o
  reached **50.49% overall accuracy**, and on graph-feature inputs GPT-4o
  scored **39.83–46.07%**, versus decision trees/random forest/CatBoost/GNN
  which the paper found "superior to GPT-4o on feature-based tasks" while
  GPT-4o's precision "greatly exceeds that of these tree models" on some
  slices and top-3 accuracy reached 67–71%. Net: **a frontier LLM was
  roughly competitive with, and on some metrics behind, purpose-built tree
  and graph models for this classification task — not a clear win.** No
  cost or latency numbers given. (Read via WebFetch 2026-09-15.)

- TokenScout (Wu et al., ACM CCS 2024, peer-reviewed conference paper,
  https://dl.acm.org/doi/10.1145/3658644.3690234) — **not an LLM**. It is a
  temporal graph neural network over 214,084 labeled ERC-20 tokens (9.7M
  transfer events, 4 human auditors, 800 hours of manual labeling), reporting
  98.41% balanced accuracy and, deployed live March–May 2023, catching 706
  rug pulls, 174 honeypots, 90 Ponzi schemes. This is the strongest
  documented result in the space and it is a trained classifier, not an LLM
  or reasoning model — direct counter-evidence to "reasoning models win":
  the best-performing published system found today is not one.
  (**Correction note**: an earlier WebSearch result summary claimed a
  separate 2026 paper, "A Hybrid Multi-Agent System for Early Scam
  Detection," combined LLMs with rules and was "trained on the TokenScout
  Dataset" — that MDPI page (doi.org/10.3390/app16073122) returned HTTP 403
  when fetched directly today and the claim could not be verified against
  TokenScout's own paper, which describes itself as a standalone GNN system
  with no LLM component. Treat the hybrid-training claim as **unverified**.)

- RugKeeper: "A Multi-Agent LLM Framework for Rug Pull Token Detection,"
  IEEE ICASSP 2026 (Barcelona, 2026-05-04–08), paper IFS-P6.6, authors from
  Fudan University and China Mobile Shanghai ICT
  (https://www.cmsworkshops.com/ICASSP2026/view_paper.php?PaperNum=5781,
  read 2026-09-15). This is a **peer-reviewed, accepted conference paper**
  confirming an LLM multi-agent approach exists and was accepted at a
  respected signal-processing venue in 2026 — a real, dated data point that
  reasoning/LLM agents are an active research direction for this exact task.
  **The abstract, methodology, and any comparative accuracy/cost numbers were
  not reachable today** (only the title/author metadata page loaded; the full
  manuscript link was not followed). **Evidence label: documented that the
  paper exists and was accepted; not established what it actually found.**

- Related but not fetched in full: "Resisting Manipulative Bots in Meme Coin
  Copy Trading: A Multi-Agent Approach with Chain-of-Thought Reasoning"
  (arXiv:2601.08641) and "TMRugPull: A Temporally Sound Multimodal Dataset
  for Early RugPull Detection" (arXiv:2602.21529) — both surfaced by search
  as 2026 preprints touching reasoning-style agents or new labeled datasets
  for this task; neither was opened, so their claims are **not verified**,
  only noted as leads for a follow-up pass.

**Overall for Q2**: no source found today — peer-reviewed, vendor, or blog —
demonstrates a reasoning LLM *beating* a purpose-built rule engine or trained
classifier on rug-pull/scam-token detection with a clean head-to-head number.
The one head-to-head with real numbers (Cui et al.) has the LLM roughly tied
or slightly behind tree/graph models. The strongest deployed result
(TokenScout) is not an LLM. LLM-agent papers for this exact task exist and
were accepted at a 2026 venue (RugKeeper), showing the research direction is
live, but the result itself is not yet in evidence from what was reachable
today.

## 3. Open-ended judgment vs bounded code

**Documented/inferred from the sources above and from AGENTS.md's own
framing of the bot's task:**

- **Bounded code (a hop-limited graph walk, a label list) is sufficient for:**
  authority-flag checks (`is_mintable`, `hidden_owner`, freeze authority — GoPlus,
  RugCheck), holder-concentration thresholds (top-10%, top-1 holder),
  LP-lock/burn detection against known locker contract addresses, same-block
  bundle detection (Bubblemaps' bundle definition is purely a timing window),
  and funding-cluster detection **when** the funding source is a small number
  of hops from a labeled address (an exchange hot wallet, a bridge contract,
  a known deployer) — Bubblemaps' own description of clusters ("shared
  funding sources... same deployer address or bridge transaction") is exactly
  a bounded graph walk against a label list, not open-ended reasoning.

- **Open-ended judgment is where the sources hint bounded code runs out:**
  (a) a funding trail that goes many hops through unlabeled intermediate
  wallets before reaching anything recognizable — a fixed hop limit will
  silently stop and report "not found" rather than "clean," which is exactly
  the zero-vs-absent distinction AGENTS.md already calls out; (b) reading
  off-chain context — a token name or symbol that imitates a real project
  (Token Sniffer's own "impersonator tokens" category, defined by name/symbol
  similarity to "well-known tokens," https://tokensniffer.readme.io, read
  2026-09-15) requires semantic/fuzzy judgment about what a "well-known
  project" is and what counts as imitation, which a fixed string-distance
  rule can approximate but not fully cover (typosquats using homoglyphs,
  translated names, subtle narrative mimicry); (c) synthesizing several
  weak, individually-explainable-away signals into one "this smells
  coordinated" judgment — the Cui et al. paper's own finding that LLM
  "effectiveness in contextual interpretation suggests they can provide
  useful explanations" (arXiv:2501.18158, read 2026-09-15) points at
  explanation/narrative synthesis, not raw classification accuracy, as the
  place an LLM added value in their test.

- **Inferred conclusion**: the split in the evidence is not "rules vs
  reasoning" as a whole-system choice — it is that *every vendor examined
  keeps the graph walks and threshold checks in bounded code* and reserves
  anything resembling judgment (naming, narrative, weak-signal synthesis) for
  either a human reviewer or, in RugKeeper's case, an LLM agent layer that is
  additive to (not a replacement for) the on-chain rule checks.

## 4. Offline use: models proposing rules from past rugs

**Narrow finding, not established in depth.** No source reached today
directly demonstrates a workflow of "a model studied past rugs, proposed a
new detection rule, and a human then encoded it." The closest documented
adjacent facts:
- TokenScout's dataset was built by **four human auditors manually labeling
  214,084 tokens over 800 hours** (ACM CCS 2024, read via search summary
  2026-09-15) — humans generating ground truth for a model to learn from,
  the reverse direction (data → model), not model → rule.
- A cited SoK-style survey (arXiv:2403.16082, "Comprehensive Analysis of Rug
  Pull Causes, Datasets, and Detection Tools") reportedly found detection
  tools cover only "25 of 34 root causes (73.5%)" of known rug-pull
  mechanisms (from a WebSearch summary, **not independently re-verified
  against the PDF today** — flagged as such). This shows humans doing
  root-cause taxonomy work that could feed new rules, but says nothing about
  an LLM doing that taxonomy work itself.

**Not established**: whether any team runs a reasoning model over historical
rug corpora specifically to *mine candidate rules* for humans to review and
hand-encode. This is plausible as an offline batch job (cheap, no latency
pressure, one frontier-model pass per newly confirmed rug rather than per
live reply) but no vendor or paper found today documents doing it.

## What this means for us

**Recommendation**: keep the current design — code builds the fact sheet
(bounded graph walks with a hop limit, label lists for exchanges/bridges/
lockers, fixed threshold checks), a cheap model only writes the reply from
that fact sheet, and a post-generation check refuses any number not on the
sheet. Every established competitor examined (RugCheck, GoPlus, Token
Sniffer, Bubblemaps) does the substantive detection work this way, and the
one head-to-head accuracy comparison found (Cui et al., arXiv:2501.18158)
shows a frontier LLM roughly tied with — not beating — purpose-built
classifiers on the closest analogous task (Bitcoin transaction
classification). At today's Anthropic list prices (read 2026-09-15,
https://claude.com/pricing): Claude Opus 5 (the frontier reasoning-capable
tier) is **$5/MTok input, $25/MTok output**, versus Sonnet 5 at $2/$10 and
Haiku 4.5 at $1/$5 — a frontier reasoning pass is roughly 2.5–5x a Sonnet-tier
reply and 5–25x a Haiku-tier reply per token, before counting that a
reasoning model also spends many more tokens per call on hidden
chain-of-thought. Against the packet's own figures (~$0.00001/chain read,
~$0.001/cheap-model reply), a frontier reasoning pass per reply would very
plausibly land one to two orders of magnitude above the cheap-model reply
cost — for a capability gain that is not demonstrated to exist yet on this
exact task.

Where a reasoning model likely *does* earn its cost: **offline**, not per
reply — an occasional batch pass over newly confirmed rugs (cheap in
aggregate, no latency budget) to propose candidate new rules or new label-list
entries for a human to review and hand-encode, and possibly as a narrow,
budgeted escalation only for the specific open-ended sub-cases in §3
(unlabeled-wallet funding trails past the hop limit, name-imitation
judgment) rather than as a wholesale replacement for the fact-sheet pipeline.

## Not established

- RugCheck's own internal weighting/scoring formula (only third-party
  descriptions of its ~20 checks were found; RugCheck's own detailed
  methodology sits behind a Swagger API reference, not prose docs).
- Whether GoPlus's "Rug-Pull Detection API (Beta)" or its "Security Compute
  Layer" uses ML/LLM components beyond the field-level rules documented for
  the general Token Security API.
- De.Fi's own methodology documentation (not reached; relied on secondary
  write-ups).
- The RugKeeper paper's actual method, dataset, and reported numbers (only
  title/author metadata was reachable today).
- Any confirmed, working example of a model mining rug-pull rules offline
  for humans to encode (plausible, undocumented in what was reachable today).
- The one hybrid-LLM-plus-TokenScout-dataset claim from an early search
  summary, which could not be reproduced from a primary source and is
  flagged above as likely a search-tool summarization error rather than a
  documented fact.

Confidence: **CONDITIONAL** — on today's search/fetch reach (several vendor
methodology pages and two full papers, RugKeeper and the MDPI hybrid paper,
were not accessible), the recommendation (rules + memory over per-reply
reasoning) is well supported by every reachable primary vendor doc plus the
one real head-to-head accuracy paper. A second pass that actually reads
RugKeeper's full text and GoPlus's Rug-Pull Detection (Beta) methodology
could change §2 and §1's "not established" items, but is unlikely to change
the recommendation, since it would need to show an LLM *winning* a
head-to-head, which no source today showed even for the papers that do use
LLMs.
