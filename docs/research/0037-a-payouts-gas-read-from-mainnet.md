<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0037 — A payout's gas, read from mainnet

**Date:** 2026-09-15
**Status:** captured. §1–§3 read mainnet; §4, the Turnkey setup proof, passed
on the box on 2026-09-15. Every number in §1–§3 is recomputed from
[`docs/research/data/0037-transfer-and-gas.json`](data/0037-transfer-and-gas.json):
raw JSON-RPC responses, read only, nothing sent.
**Feeds:** [ADR 0025](../adr/0025-the-robinhood-payout-signs-through-turnkey.md)
§4 and the gas float funded at launch (plan 0001).

## How it was read

Robinhood Chain mainnet (chain id 4663), through the public RPC
`https://rpc.mainnet.chain.robinhood.com`, at block 63,762,731. The transfer is
the most recent type-2 transaction found walking back from there with a value,
empty call data, and a recipient with no code. The estimates are made from the
creator whose escrow claim [research 0036](0036-pons-v2-read-from-a-real-launch.md)
§5 captured, `0x6aa0…4d06`, because that account has something in the escrow to
claim. The revert data `0xc2caa2a6` was named `NoBalance()` by the Sourcify
4byte signature database, the only name it holds for that selector.

## 1. A plain transfer costs 21,000 gas and nothing for L1

| read | value |
|---|---|
| transfer `0xbbf7981b…1480`, gas limit | 21,000 |
| its receipt, `gasUsed` | 21,000 |
| its receipt, `gasUsedForL1` | 0 |
| its effective gas price | 65,482,000 wei |
| `eth_estimateGas`, 1 wei from the creator | 21,000 |
| base fee at block 63,762,731 | 65,642,000 wei |

Robinhood Chain is an Arbitrum chain, and on some Arbitrum chains every
transaction carries an extra L1 charge folded into its gas. Here, today, a plain
transfer does not: the receipt's L1 part is zero and the estimate is Ethereum's
21,000. The payout still estimates rather than assumes, so a charge added later
shows up in the estimate and the balance check before anything is signed.

## 2. A claim estimates at 42,581 gas and uses about 32,700

The creator held 5,164,929,159,408,132 wei in the escrow at capture.
`eth_estimateGas` for `claim` of exactly that is **42,581**. Research 0036's
captured claim set a limit of 42,593 and used 32,679, so the estimate carries
about a third over what a claim uses, and a claim sent with it lands.

What the payout sets from these, at this base fee:

| | estimate | limit (estimate + a quarter) | worst case at the fee cap (2 × base fee) |
|---|---|---|---|
| claim | 42,581 | 53,227 | 6,987,853,468,000 wei |
| transfer | 21,000 | 26,250 | 3,446,205,000,000 wei |
| **both** | | | **10,434,058,468,000 wei ≈ 0.0000104 ETH** |

The worst case is what the wallet must hold before the claim is signed. What a
week actually costs is closer to the gas used at the base fee, about
(32,679 + 21,000) × 65,642,000 ≈ 0.0000035 ETH. **A gas float of 0.001 ETH
covers about 95 weeks of the worst-case check at this base fee.** The base fee
moves, and the check reads it on every run.

## 3. The escrow refuses a claim of zero

`claim(0)` from the same creator, who held a balance, reverts with
`NoBalance()`. So the escrow refuses a zero amount, not only an empty balance.

Two things follow:

- **The payout never asks for it.** A week with nothing collected stops at
  `NothingCollected` before any gas is estimated.
- **The setup proof's `claim(0)` could not take money even if it were sent.** The
  proof only asks Turnkey to sign it, at nonce 1,000,000, which the wallet will
  not reach; and the escrow would refuse it anyway.

## 4. The Turnkey setup proof

Passed, 2026-09-15: `realorrug-payout --setup-proof` on the box, run as the
`realorrug-payout` system user with `/etc/realorrug/payout.env` and the P-256
API key, against the organisation, wallet, user and policy the
[deploy guide](../../deploy/README.md) describes. Its four lines, as printed
(the organisation id is not recorded; no key is printed):

1. `whoami answered, as user realorrug-payout`
2. `a call to the factory was denied: ... HTTP 403: {"code":7, "message":"You
   don't have sufficient permissions to take this action. ..." ... "No policies
   evaluated to outcome: Allow", "policyEvaluations":[{... "outcome":"OUTCOME_DENY_IMPLICIT"}]}`
3. `claim(0) at nonce 1000000 was signed by 0xd66578ac5fd7e36f8427c61edb7a2cbfa4c75793,
   hash 0xc026e14d065803c39e1f073978e4f4baae58438a40e7bbc586a74693681f0765; not
   sent, and it cannot land`
4. `a 1 wei transfer at nonce 1000000 was signed by 0xd66578ac5fd7e36f8427c61edb7a2cbfa4c75793,
   hash 0x710e5745c496934e62e1c4cc8cb0c2e218f32d1ff6fbf9ba631db5d3e480430d; not
   sent, and it cannot land`

What it settles:

- **The stamp, key and organisation work**: Turnkey named the user.
- **The policy denies what it should**: the factory call failed in the policy
  engine itself, with the one policy evaluated and none allowing it. This is a
  403 from policy, not the 404 of the first run below.
- **The policy allows the claim**, and the signed bytes recover to the wallet:
  the payout decodes them, compares every field with what it asked for, and
  recovers the sender before printing line 3.
- **`eth.tx.data == '' || eth.tx.data == '0x'` matches a plain transfer**, the
  one part of the policy that was a guess.
- Nothing was sent. Neither transaction was broadcast, and at nonce 1,000,000
  neither could be mined before the wallet had sent a million others.

A first run on 2026-09-15 answered `whoami` and failed every signing request
with HTTP 404, "Could not find any resource to sign with. Addresses are case
sensitive.": Turnkey's `signWith` takes the address only in EIP-55 mixed case.
Its factory "denial" was that error, so it is not a result.
