<!-- SPDX-License-Identifier: Apache-2.0 -->
# Research 0067 — Base launch platform comparison

**Date:** 2026-10-02. **Status:** documentation/source comparison; deployment qualification open.
**Direction:** [VISION](../../VISION.md). **Decision:** [ADR 0045](../adr/0045-base-is-the-preferred-launch-chain.md).

Josh prefers Base and explicitly leaves the launchpad open for comparison.
The recommendation below is ours; it is not a selected platform, launch quote,
custody audit or authorization to spend. Three-chain investigation coverage,
the joint token/investigator launch and startup funding cap remain unchanged.

## Practical comparison

| Route | Useful connection | Added work and dependency | Position |
|---|---|---|---|
| Flaunch on Base | F4: a native fee-funded bid wall; F3: creator revenue; F5: inspectable routing and revenue rights. | Qualify the actual hook, escrow, pairing and revenue-NFT custodian; read fee waterfalls, disable/management powers, claim receipts and execution. | First candidate to qualify if native token support can coexist with funded operations. |
| Clanker on Base | F3: creator LP rewards can fund our treasury; a stable paired asset could simplify runway accounting. | Qualify current locker/hook, reward recipients and administrators, both reward assets, claim receipts and actual deployment costs. | Alternative if it gives a simpler, better-qualified funding path. |
| Virtuals on Base | Agent distribution and commerce may become useful to paid services. | Its documented tokenization adds VIRTUAL pairing, graduation and liquidity requirements; replacing our runtime is unnecessary. | Revisit for demonstrated distribution/commerce value. |
| Direct Base deployment | Control over token and routing design. | More contract, launch, liquidity and review work for a solo builder. | No custom launch contracts in this slice. |
| Solana/pump.fun | Existing launch/readback integration remains useful. | Separate launch/payment chains and an integration advantage that no longer determines Josh's preference. | Preserved investigation support and historical launch tooling. |

Base can place the project token, treasury receipts and a future Base-USDC
service on one chain. This is an operational inference, not proof that demand
will increase or that existing payment code is production-ready.

## Primary-source findings, checked 2026-10-02

- [Flaunch creator revenue](https://docs.flaunch.gg/features/creator-revenue)
  describes a launch-time fixed creator/community allocation. Its
  [buyback documentation](https://docs.flaunch.gg/features/auto-buybacks)
  describes accumulated-fee thresholds and owner disable powers. Allocation,
  pending funds, an executed purchase and a supply burn are separate events;
  no prompt purchase or burn is established by a configured allocation.
- [Flaunch revenue-NFT rights](https://docs.flaunch.gg/features/royalty-nft)
  include transferable income and management rights.
  [AddressFeeSplitManager](https://docs.flaunch.gg/managers/addressfeesplitmanager)
  describes immutable initial recipients but also recipient-share transfer,
  manager ownership, NFT rescue and creator changes. An immutable parameter
  is not proof that the complete revenue route cannot change. Resolve these
  powers in the exact deployed implementation before a trust claim.
- [Flaunch launch configuration](https://docs.flaunch.gg/getting-started/flaunch-a-coin)
  and [manager types](https://docs.flaunch.gg/managers/manager-types) offer
  standard routing paths. Prefer a verified existing path; a custom manager
  adds contract review and execution obligations. Platform defaults are not
  our chosen allocation or an affordable launch quote.
- [Clanker creator rewards](https://clanker.gitbook.io/documentation/general/creator-rewards-and-fees)
  concern the initial pool, not all pools trading the token. The
  [Droid funding documentation](https://clanker.gitbook.io/documentation/droids/funding)
  describes a reward carve-out, Base USDC compute runway and contract routing.
  This is a useful reference; its hosted runtime and funding defaults are not
  requirements for our existing investigator. Distinguish protocol fees,
  reward shares, claims and actual treasury receipts.
- [Virtuals' whitepaper](https://whitepaper.virtuals.io/) describes tokenization
  paired with VIRTUAL and additional graduation/liquidity requirements.
  Agent branding alone does not prove suitable costs or custody for this project.

Pinned source references: [Flaunch contracts at 77d7d23](https://github.com/flayerlabs/flaunchgg-contracts/tree/77d7d23cd7c8c947e7f63d2c2da95cc766c093ea)
and [Clanker SDK at 4f4d2bb](https://github.com/clanker-devco/clanker-sdk/tree/4f4d2bbf41c7f10543559dc043c85f443a6d452e).
Source state is not deployed-bytecode equivalence or an independent audit.

## Repository fit and qualification gate

The bounded readers recognize two normal Flaunch hooks, including the current
Base hook listed in the pinned source. Recognition is not complete fee/control
or liquidity-rights coverage. [Research 0066](0066-the-investigator-and-library-qualification.md)
has no positive Flaunch live qualification and records an unsupported newer
Clanker locker. Generic Base token reads cannot close either gap. The existing
Base-USDC facts endpoint records unverified live facilitator interoperability.
The Solana launch checker is not a Base launch checker.

Before choosing a route, capture one representative deployed case per viable
candidate, with chain/block/time, version/address/code identity, token and
pairing, fee denominator/waterfall, recipients, rights and control owners.
Keep configured shares, claimable balances, claims, transfers and execution
effects separate. Include low-fee/threshold behavior and unavailable reads.
Compare launch/gas/dev-buy/operating costs within the $200 startup maximum;
no recurring founder funding or new paid infrastructure is assumed.

Then select the launchpad, pairing, custody and concrete allocation through a
new decision. Extend readback and the public ledger for that exact route.
Protocol-native buybacks need the same economics/control review as custom
automation. The 25% burn / 25% permanent-liquidity proposal remains open;
neither a bid wall nor initial platform liquidity implements that proposal.
