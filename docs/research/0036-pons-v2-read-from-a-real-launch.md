<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0036 — Pons v2, read from a real launch

**Date:** 2026-09-13
**Status:** captured. Settles the Pons v2 fee rate, fee currency and launch-block
contents that [research 0035](0035-robinhood-chain-read-from-its-own-pages.md) §6
left open, on the bonding curve only. Every number in §1–§3 is recomputed from
[`docs/research/data/0036-pons-v2-launch.json`](data/0036-pons-v2-launch.json):
raw JSON-RPC responses, each pinned to a block; §4, §5 and §6 name their own
files, and §5's paragraph on the hook is read, not captured. The factory owner can change
the settings in §1 for **future** launches, so they go stale; a launched curve
keeps what it snapshotted. The split after graduation is source-only (§5).
**Feeds:** plan 0001 step 6, [design 0019](../design/0019-realorrug-on-robinhood-chain.md) §4.4,
and [ADR 0013](../adr/0013-a-community-token-exists-and-radar-holds-none-of-it.md)
constraints 1 and 2 as they apply on Robinhood Chain.

## How it was read

Robinhood Chain mainnet (chain id 4663), through the public RPC
`https://rpc.mainnet.chain.robinhood.com`. The explorer's API sits behind a bot
challenge and was not used. The factory address comes from
[Pons's v2 docs](https://docs.ponsfamily.com/v2); function and event names come
from [Pons's published source](https://github.com/ponsdotdev/ponsfamily), and
each name was confirmed by the chain answering it.

**The published source is not what is deployed.** The published factory calls
`exemptFromSnipeTax` on the curve, and the published curve has no such
function; the deployed curve emits `SnipeTaxExempted`, and an event with topic
`0x3bc39a55…` that the published source does not contain. The docs say the
snipe window is five seconds, the source says fifteen, and the chain says three.
So the source names things, and the chain says what they do.

The public RPC serves old logs but not old state: a call pinned about 57,000
blocks back failed with `metadata is not found`. A reader that needs a curve's
settings reads them from the curve, which does not change them, not from an old
factory block.

| contract | address |
|---|---|
| factory | `0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e` |
| router (the factory's `launchForwarder`) | `0xe33e9e479df8802cb0866d5d05258bec4cf62948` |
| launch captured: token | `0x22fd486d80b7cce7362ffed59bbf2fd266a148fa` |
| launch captured: curve | `0xddf3afb29e265b00c48015c3aacdedcb10088fcf` |
| launch transaction | `0x1013a302930cfbc10d55c2fec9cd9e93670f516933ea6a907081ee928cfcf8b7`, block 62357124 |

## 1. Settings, at the launch block

| setting | value | read |
|---|---|---|
| launch fee | 0.0005 ETH | factory `launchFee()` |
| anyone may launch | yes | factory `launchEnabled()` is true; in the published source that bypasses the `whitelistedLaunchers` list, and the 350 launches surveyed (§2) came from 298 different deployers |
| creator tax ceiling | 1,000 bps (10%) | factory `maxCreatorTaxBps()` |
| snipe tax | starts at 9,900 bps (99%), over 3 seconds | factory `snipeTaxStartBps()`, `snipeTaxSeconds()`; the curve holds the same |
| launch configs | one | `launchConfigCount()` |
| config 0 | supply 1,000,000,000 tokens; base fee 100 bps; graduation at 4.2 ETH; phantom reserve 1.68 ETH | `getLaunchConfig(0)` |
| protocol's share of the base fee | 3,000 bps (30%) | curve `protocolFeeShareBps()` |

## 2. The fee, and who gets it

Each trade on the curve pays two charges, both in the **pair asset**:

- a **base fee** of 100 bps, of which the protocol keeps 30% and the creator's
  side gets 70%;
- a **creator tax** the launcher chooses once, from 0 to 1,000 bps, paid to the
  creator in full.

**Measured, not read from source.** The captured fee sweep (block 62362074, curve
`0x36f815a2…`, creator tax 100 bps, pair ETH, no buyback) paid out 30 trades.
Summed from their `CurveBuy`/`CurveSell` events: fee 16,955,319,832,856,488 wei,
tax the same. `FeesSwept` paid the protocol 5,086,595,949,856,946 — exactly 30%
of the fee, rounded down — and the creator 28,824,043,715,856,030, exactly the
rest of the fee plus all of the tax. Six other sweeps read while surveying, with
creator taxes of 100 to 300 bps, split the same way to three decimals.

**So on the curve, the creator's income is 70 bps of volume plus the chosen tax,
in the pair asset.** At a creator tax of zero, that is 70 bps; pump.fun's curve
pays 30 bps ([ADR 0013](../adr/0013-a-community-token-exists-and-radar-holds-none-of-it.md)
constraint 2).

**The pair asset is chosen at launch, and ETH is not the only one.** Of the 350
`TokenLaunched` events in blocks 62338142 to 62358142, 86 name an ERC-20 pair
rather than native ETH (a survey read the same day; those logs are not in the
capture file).
A launch paired with ETH pays its creator in ETH; that is what was captured.
What those ERC-20s are was not read.

**If buyback is enabled** (`buybackBurnBps` 5,000 on these curves), part of the
creator's side of the base fee buys back tokens instead. None of the launches
captured enabled it, so its effect is source-only.

## 3. The launch block

**The mint goes only to the curve.** The launch transaction's first token
transfer is 1,000,000,000 tokens from the zero address to the curve. No other
address is minted anything.

**But a launch can carry a buy, in the same transaction.** This launch sent
0.0505 ETH: the 0.0005 ETH launch fee plus a 0.05 ETH buy. The curve then sent
the launcher 28,340,080 tokens (2.83% of supply). That buy paid the 1% base fee
and the 1% creator tax, and **no snipe tax**, because the factory exempts the
launcher and the creator fee recipient automatically. The launcher also named
three more wallets exempt (`SnipeTaxExempted` for `0x7f5cf80c…`, `0x92f4e778…`,
`0x0e750296…`); the factory allows up to 32.

No other buy landed in the launch block. The first buy by anyone else came two
seconds later, in block 62357147: 0.03798 ETH in, a creator tax of exactly 1%,
and a fee of 1% **plus** 72,156,075,574,116 wei (19 bps). The unnamed event
`0x3bc39a55…` in that transaction carries exactly that 19 bps amount. **Inference:**
it is the snipe tax, decayed from 99% at launch to 19 bps at two seconds, and
it is folded into the `fee` field. Who receives snipe tax was not measured: the
captured sweep's trades carried none.

**For ADR 0013 constraint 1 this means:** "the curve is the only recipient of the
mint" is how Pons v2 mints (one launch captured), so it cannot tell a clean
launch from one with a dev buy. What proves no dev buy
is the launch transaction's receipt holding no `CurveBuy`, and its
`SnipeTaxExempted` events naming only the deployer and the fee recipient.

## 4. A clean launch, and the check that tells them apart

*Added 2026-09-14, plan 0001 step 6.*

[`0036-pons-v2-clean-launch.json`](data/0036-pons-v2-clean-launch.json) holds
launch `0x2a43738c…` (block 62356247): sent straight to the factory with 0.0005
ETH, the launch fee and nothing else. Its receipt mints 1,000,000,000 tokens to
the curve, exempts only the deployer from the snipe tax (the deployer is also
the fee recipient), and holds no trade. Creator tax 200 bps.

`realorrug-robinhood`'s `pons::check_launch` passes it and refuses the §3
launch for exactly three extra exemptions, the curve-to-launcher transfer and
the buy; `crates/realorrug-robinhood/tests/pons_as_mainnet_wrote_it.rs` asserts both, then re-applies each
kind of dirt to the clean launch. `realorrug launch-check --tx <hash> --rpc
<url>` runs the same check against the chain, and printed the same two verdicts
against mainnet on 2026-09-14.

## 5. Where the creator's money goes, and after graduation

*Added 2026-09-14. The escrow and its claim are captured and decoded; the hook
paragraph is read from the chain and the published source only.*

**A sweep does not pay the creator; it credits the fee escrow.** The captured
sweep's receipt holds two logs from the escrow
(`0xd3afeb2a57f70ef218aa82451c51b2fb0416ac9e`, as Pons's docs name it) before
`FeesSwept`: one crediting the protocol recipient 5,086,595,949,856,946 wei, one
crediting the creator 28,824,043,715,856,030 — the `FeesSwept` amounts exactly.
Each names the curve as the source. The sweep was sent by `0x49bbf2b7…`,
neither the creator nor the protocol, so a keeper sweeps. The escrow's events
hash to these signatures, each confirmed against a log it emitted in the 3,000
blocks before block 62382501: `Credited(address,address,uint256)`,
`Claimed(address,uint256)`, `CreditedToken(address,address,address,uint256)`,
`ClaimedToken(address,address,uint256)`.

**A claim, read back to the wei.**
[`0036-escrow-claim.json`](data/0036-escrow-claim.json) holds claim
`0x07cab768…` (block 62853730) by the wallet `0x6aa025a3…`. The published
interface shows `claim()` with no argument; the deployed escrow was called as
`claim(uint256)` (selector `0x379607f5`) with 4,014,961,601,594,189,201 wei. In
the block before, the escrow's `balanceOf` for the claimer was exactly that
amount; after, zero. The escrow's ETH fell by exactly that amount, and the
claimer's rose by it less the gas (2,273,020,524,000 wei), to the wei. The
claimer holds no code, so the ETH did not pass on to someone else.

So a payout claims the creator's balance from the escrow and then pays the
winner, and the week's prize is that balance. `realorrug-robinhood`'s
`escrow` module reads it (`claimable`), encodes the claim (`claim_call`, equal
to the captured call's input) and reads a claim back (`claimed`), tested in
`crates/realorrug-robinhood/tests/escrow_as_mainnet_wrote_it.rs`. **Inference:**
the escrow also takes partial claims, since the call names an amount; no partial
claim was captured.

Credits read the same day, around the claim's block, also name the meme hook
(`0xe5e70264…`) as their source, so fees from graduated pools land in the same
escrow; those logs are not in a capture file, and how the hook splits them was
not measured.

**The fee continues after graduation, and so does the creator tax** — in the
published source. The meme hook charges `hookFeeBps`, split between protocol
and creator by `protocolFeeShareBps`, plus the launch's `creatorTaxBps` paid to
the creator in full. The deployed hook
(`0xe5e702641ea86f4ae6cc3cdaed2b886f976be044`) answers `hookFeeBps()` 100 and
`protocolFeeShareBps()` 3,000, the same as the curve, and emits more than 10,000
logs in 10,000 blocks, so graduated pools trade. No hook sweep has been decoded,
so the split after graduation is source-only.

## 6. Does a higher creator tax pay more?

*Added 2026-09-14, for the creator tax the operator chooses at launch.*

[`0036-creator-tax-survey.json`](data/0036-creator-tax-survey.json): 250
ETH-paired launches, thinned evenly from the 1,789 in blocks 62242501 to
62342501, each with its creator tax and its curve's buy and sell volume in the
36,000 blocks after launch. Creator income is estimated as volume × (70 bps +
tax), per §2.

| creator tax | launches | mean creator income, ETH | largest | graduated since |
|---|---|---|---|---|
| 0 | 62 | 0.028 | 0.366 | 1 |
| 1–100 bps | 45 | 0.016 | 0.141 | 0 |
| 101–300 bps | 131 | 0.038 | 0.876 | 2 |
| 301–999 bps | 10 | 0.007 | 0.028 | 0 |
| 1,000 bps | 2 | 0.487 | 0.974 | 0 |

**What it shows is weak.** Means are carried by single launches (the largest
in each row), launchers choose their own tax, and a launch's quality is not
measured. A first sample the same day, not kept, ranked the rows the same way
with 101–300 bps at about twice the income of zero. Neither sample shows a tax
cutting volume enough to lower the creator's income up to 300 bps; above it,
ten launches earned little and two earned a lot, which is not a pattern. Three
of 250 had graduated.

## 7. Not established

- The split after graduation, from a decoded hook sweep.
- Who receives the snipe tax, and whether the protocol takes a share of it.
- Buyback's effect on the creator's income, from a real sweep.
- A partial claim, and what the escrow does with a claim above the balance.
- What the ERC-20 pair assets are.
