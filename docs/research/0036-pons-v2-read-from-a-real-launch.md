<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0036 — Pons v2, read from a real launch

**Date:** 2026-09-13
**Status:** captured. Settles the Pons v2 fee rate, fee currency and launch-block
contents that [research 0035](0035-robinhood-chain-read-from-its-own-pages.md) §6
left open, on the bonding curve only. Every number below is recomputed from
[`docs/research/data/0036-pons-v2-launch.json`](data/0036-pons-v2-launch.json):
raw JSON-RPC responses, each pinned to a block. The factory owner can change
the settings in §1 for **future** launches, so they go stale; a launched curve
keeps what it snapshotted. The fee after graduation is not read.
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

## 4. Not established

- The fee after graduation, in the Uniswap v4 pool and its hook.
- Who receives the snipe tax, and whether the protocol takes a share of it.
- Buyback's effect on the creator's income, from a real sweep.
- How the creator's amount leaves the fee escrow, which a payout reader needs.
- What the ERC-20 pair assets are.

These go to plan 0001 step 6, which reads this capture in tests.
