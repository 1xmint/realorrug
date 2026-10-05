<!-- SPDX-License-Identifier: Apache-2.0 -->
# RealOrRug: development roadmap

**Updated:** 2026-10-04. **Status:** active delivery direction derived from
[VISION.md](VISION.md), recorded by [ADR 0043](docs/adr/0043-the-public-library-and-compounding-intelligence.md).

This replaces [plan 0002](docs/plans/0002-bot-quality-then-a-solana-launch.md)
as the current order of work. That plan's decision log remains historical
evidence. The milestones below describe required behavior, not features
already delivered. Inspect code and captures before assigning each task;
record completion only with acceptance evidence. No launch date is promised.

## Starting point

Existing foundations: Solana and Robinhood readers, dossiers, fact sheets,
reply checks and replay tools; read memory and research SQLite stores;
spend limits; pump.fun launch checks and treasury receipt readers; a paid
facts-only endpoint; identity, forecast and outcome work. Live facilitator
interoperability is explicitly unverified in the paid route. No completed
joint launch or paid portal is established by these foundations. The new
investigator and Library are delivered behind opt-in configuration; initial
summaries remain partial `CantTell`. [Research 0066](docs/research/0066-the-investigator-and-library-qualification.md)
records reader coverage, tests and live capture limits. Milestones 1 and 2
remain open until representative quality and live coverage acceptance.

Keep React, Rust/Axum and SQLite. Use free options first and compact data.
Josh's initial contribution is $100–$200, capped at $200, with no recurring
funding promise. Existing runtime budget enforcement remains unchanged in
this implementation slice. Every implementation slice reserves real costs,
has bounded work and follows [AGENTS.md](AGENTS.md).

## Milestone 1 — useful claim-specific investigations

**Launch coverage amended 2026-10-01:** Solana, Base and Ethereum are all
required, including fee routing, controls, liquidity and historical transfers
within disclosed protocol coverage. [ADR 0044](docs/adr/0044-three-chain-investigations-and-shared-cases.md)
records the decision; [plan 0003](docs/plans/0003-three-chain-investigator-and-library.md)
tracks the ordered implementation. [ADR 0045](docs/adr/0045-base-is-the-preferred-launch-chain.md)
changes the preferred token launch chain to Base; launchpad and pairing remain
open. This does not narrow the investigator's three-chain coverage.

**Strengthens F1, F2 and F5. Required for launch.**

- Preserve the question, chain, token, wallet/transaction leads and relevant
  time window through admission and investigation. Clarify conflicting token
  identities rather than silently investigating the first address.
- Connect bounded adaptive read tools to the public investigator: test the
  user's claim, check alternatives, follow productive leads and record why a
  read or follow-up was chosen. Enforce per-request and shared cost/time caps.
- Persist findings, supporting observations, gaps, tool/rule versions and
  assessment revisions under a durable case identity. Separate submitted
  claims, measured facts and inference; retain observation times and roles.
- Reuse relevant historical cases and verified relationships through explicit
  retrieval. Re-read dynamic state when needed. A cache hit must state its age.
- Keep fidelity checks and code-controlled assessments; budget exhaustion or
  unavailable data produces an honest partial assessment, not a safe verdict.

**Acceptance:** a representative replay includes fee diversion, a legitimate
fee splitter, graduation, ambiguous holder roles, infrastructure sharing a
funder, partial reads, stale data, injected instructions and a correction.
Josh accepts useful replies; evidence-fidelity, unsupported-accusation and
unknown-data checks pass on accepted cases. Reopening a case after restart
retains its history; a later investigation demonstrates useful memory reuse.

## Milestone 2 — free Library and collaborative dossiers

**Strengthens F1, F2 and F5. Required for launch.**

- Publish searchable, shareable token dossiers keyed by chain and address,
  with coverage, depth, observation time, findings, sources and unresolved
  checks. Preserve assessment history and meaningful corrections.
- Attach X follow-ups and website leads to the same case. Use the existing
  identity foundation for authenticated contributions where needed; browse
  public dossiers freely. New claims remain unverified until checked.
- Make case evidence available to the agent's next investigation. Begin
  a compact mechanism library with legitimate lookalikes and explicit
  supporting/counterexample cases. Record which later checks use it.
- Bound retention and storage, preserve compact case history and disclose
  unavailable underlying evidence. Implement backup/restore for the shared
  state. Publish archivable checkpoints without claiming operator custody
  is independent immutability.
- Explain methodology, inference, coverage limitations and contribution
  rules. Keep existing brand art. Forecasts and reputation are preserved for
  later work and do not block this milestone.

**Acceptance:** two contributors extend the same dossier; verified additions
appear with provenance; a correction revisits a dependent finding. Search
distinguishes identical tickers on different chains. A historic view retains
its observation time, an incomplete case remains visibly incomplete, and
backup/restore recovers a case and its revisions. The agent uses a Library
case as evidence rather than treating a user allegation as fact.

## Milestone 3 — transparent finances and joint launch readiness

**Strengthens F3, F4 and F5. Required for launch.**

- Publish a financial ledger for founder funding, creator receipts, expenses,
  balances, unclaimed fees and reserve coverage. Label allocated, pending,
  executed and verified states, failures and reconciliation gaps. Distinguish
  on-chain receipts from operator-reported off-chain expenses.
- Resolve treasury form, wallets, dev buy, startup cash allocation, reserve
  purpose/cap/excess rules and operating budgets within available funding.
  The initial $200 maximum is not a monthly allowance. Existing runtime
  ceilings remain until explicit implementation changes them.
- Compare Base/Flaunch and Base/Clanker using deployed fee/control traces
  and launch/operating costs within funded resources; select the platform,
  pairing, custody and allocation through a specific decision. Research 0067
  recommends Flaunch-first qualification, not an adopted provider or split.
- Implement the selected Base route's launch readback and receipt/claim
  reconciliation before relying on it. Publish supply/issuance and upgrade
  powers, fee and liquidity rights, revenue custody, holdings and compensation.
  Existing Solana launch tooling is historical coverage, not a Base verifier.
  [Research 0069](docs/research/0069-clanker-current-locker-and-wallet-receipts.md)
  adds current Clanker locker coverage and an actual wallet-level claim trace.
  [Research 0070](docs/research/0070-bounded-base-fee-delivery-matching.md)
  delivers bounded direct ERC-20 claim/receipt matching with duplicate and
  ambiguity refusal. [Research 0071](docs/research/0071-base-fee-credits-and-balance-windows.md)
  adds bounded credit-event observations and two reconciled research windows;
  requested amounts, cumulative balances and untrusted pool labels retain their
  separate roles. [Research 0072](docs/research/0072-clanker-transaction-balance-windows.md)
  adds automated one-block wallet/asset reconciliation inside the transaction
  reader, with retained live credit and claim captures. Wider historical windows,
  native/conversion delivery, backing and per-token attribution remain open.
  These readers do not establish a completed ledger or select the launchpad.
- Use operator-managed operations/reserve spending as the launch default.
  The 25% burn / 25% liquidity idea remains a proposal. Do not advertise a
  split, automated execution or immutable controls that are not adopted,
  funded and verified.
- Bring website copy, disclosures and the operational launch checklist into
  agreement with the approved launch behavior. Keep quality, launch review,
  X approval and transaction-signing gates from the current accepted policy.

**Acceptance:** reconcile fee claims and transfers without double counting;
refuse a wrong recipient or unstated launch transfer; read back launch
authorities and dev buy. The ledger distinguishes an earmark from a verified
action and reports a missing observation as unknown. Every wallet and
remaining control has a public explanation. Library and investigator meet
milestones 1–2; forecasts and paid investigations are not prerequisites.

**Owner launch gates:** Josh accepts representative replies; the existing
launch-copy/legal-and-tax review process ([ADR 0042](docs/adr/0042-claudes-review-replaces-counsel-at-gate-two.md))
has every finding fixed or ruled on; X approval is obtained before automated
X replies; terms and deployed configuration are rechecked; Josh reviews and
signs the launch transaction. Without X approval, website work and private
evaluation continue; do not call that a completed public X investigator launch.
No deployment, signing, posting or spending is performed by this roadmap.

## Milestone 4 — one paid investigation option

**Strengthens F2, F3 and F5. After the joint launch.**

- Define one useful bounded scope and measure its model, read, storage and
  payment costs. Choose price, supported payment rail, margin, maximum work,
  capacity and fulfillment terms from captures. Existing facts-route pricing
  is not automatically an investigation price.
- Verify the chosen x402 client/facilitator and actual network/asset support.
  Existing Base USDC code and a candidate Solana USDC portal are separate
  options to decide in the implementation design. Native SOL acceptance or
  conversion requires a separately verified flow.
- Build the portal: token and question → existing coverage → available scope
  and price → wallet authorization → progress → durable dossier/result.
  Keep payer identity out of public badges by default. Verified results enrich
  the public Library; payment never changes evidentiary standards.
- Admit only work that can be funded and fulfilled. Add explicit funded-work
  accounting before expanding beyond existing runtime limits. Distinguish
  owner contribution, settled receipts, fulfillment cost and surplus.
- Persist purchase/job identity, settlement evidence and result retrieval.
  Prevent duplicate charges and test interrupted settlement, worker restart,
  failed reads and incomplete fulfillment. Implement the promised recovery
  or refund process before charging customers; reconciliation cannot rely on
  a best-effort bookkeeping write.

**Acceptance:** a real end-to-end payment capture maps one purchase to a
retrievable result and financial receipt. Invalid/expired payments, duplicate
retries, lost HTTP responses, timeouts and restarts cannot silently charge
twice or lose charged work. Capacity exhaustion refuses purchases before
money is taken. Failed or partial work follows its disclosed policy. A paid
investigation visibly adds verified coverage to the free Library.

## Milestone 5 — depth tiers, batches and evaluated intelligence

**Strengthens F1, F2 and F3. Add when demand and results justify it.**

- Introduce light/focused/deep options only when their coverage and cost
  differences are measured and useful. Show what each scope does and cannot
  establish; paying more does not promise certainty.
- Add bounded batches over explicit token lists, with cost/capacity admission,
  resumable work, progress and completed/failed/unresolved manifests. Merge
  useful observations into existing cases without inflating counts through
  duplicate scans. State universe, chains, depth and timestamps.
- Evaluate reusable patterns on later periods and separate related creators
  across evaluation groups. Include ordinary cases, counterexamples, false
  positives, unresolved outcomes and sample limitations. Publish probabilities
  only where measured evidence supports them.
- Extend historical views, alerts and machine-readable data products where
  demand funds their marginal costs. Measure whether reuse improves quality
  or lowers comparable investigation cost.
- Integrate preserved forecasts and contribution reputation when useful;
  reward accurate contributions and corrections, preserve unresolved outcomes,
  and keep reputation non-transferable with no financial promise.

**Acceptance:** a batch can restart without duplicate billing or fictitious
coverage. Later cases demonstrate useful pattern retrieval, and corrections
propagate to dependent statistics. Evaluation reports include denominators,
coverage and baselines, not selected successes alone. Upgrades fit explicit
funded budgets and do not create an implicit recurring founder commitment.

## Separate gate — financial automation

**Potentially strengthens F3–F5; not required for the initial launch.**

Before adopting buybacks, burns, additional liquidity or automated treasury
spend, write a specific decision backed by mechanism research, cost and
deployed-control review. Resolve allocation denominator, runway protection,
recipients, both liquidity legs, venue availability, execution bounds,
slippage/manipulation safeguards, authority/upgrade/pause powers and recovery.
Use separately constrained execution; model judgment remains without keys or
money-movement authority, and the retired payout signer is not repurposed.
Protocol-native automation passes this gate too; choosing a launchpad does
not silently adopt its defaults or prove that its controls are immutable.

**Acceptance:** captures prove receipt → allocation → execution → effect for
every leg, including supply burn and LP ownership separately. Duplicate
invocations, missing venue, low runway, failed execution and control changes
produce the documented behavior. Public source, powers and receipts match
what is advertised. If costs cannot fit funded resources, keep the mechanism
proposed rather than marketing it as automatic or trustless.

## Verification and planning discipline

Work in small slices with a named caller and acceptance scenario. Update the
vision only for a changed product decision, the roadmap for delivery/status,
ADRs for decisions and research notes for measured findings. Each completed
slice names its capture, review or test evidence. Full suites run in CI;
local checks follow AGENTS and avoid heavy workspace builds.

The claim-preserving investigator and Library foundation in plan 0003 is
delivered for review. Next: qualify useful fee-routing investigations and
candidate Base launch deployments, then improve code-authorized claim
resolution where bounded observations cannot yet settle a claim. Use
[research 0067](docs/research/0067-base-launch-platform-comparison.md) to compare
Flaunch and Clanker before selecting the treasury/readback implementation.
Live qualification and owner reply acceptance remain open; the financial
ledger follows measured routing, rather than an assumed platform configuration.

[Research 0068](docs/research/0068-base-fee-routing-and-retained-controls.md)
now qualifies a current Flaunch fee/escrow/NFT snapshot and demonstrates retained
allocation/disable powers through read-only simulations, alongside a supported
Clanker refresh. Research 0069–0071 subsequently add the current Clanker locker,
direct claim delivery, credit observations and measured wallet/asset windows.
[Research 0072](docs/research/0072-clanker-transaction-balance-windows.md) adds
one-block Clanker reconciliation within the submitted transaction read, with
live credit/claim captures and canonical balance checkpoints.
[Research 0073](docs/research/0073-clanker-deposit-funding-provenance.md) now adds
unique preceding Clanker deposit-transfer matches and a historical upstream
qualification showing why this live deposit cannot be assigned to the case
token. [Research 0074](docs/research/0074-clanker-historical-collection-context-and-replies.md)
now adds bounded automatic historical collection context for one reviewed LP
locker and makes deposit-specific proof, independently measured credit and the
different-token context lead public replies. Context uses block-end state and
does not settle per-token revenue or upgrade financial status. Live captures,
provider failures and partial offline reviews are retained. Next: qualify
event-time configuration, fee origin, conversion/shared PositionManager limits
and representative owner-reviewed replies ahead of the ledger; qualify backing,
native delivery and remaining custody powers, and measure launch costs. Wider
windows and Flaunch balance/funding reconciliation remain open.
No provider, pairing or financial allocation is selected.
