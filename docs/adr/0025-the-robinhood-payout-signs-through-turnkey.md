<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0025 — The Robinhood payout signs through Turnkey

**Date:** 2026-09-14
**Status:** accepted. **Josh's decision, recorded** (2026-09-14): the payout key
lives in Turnkey, not in a file on the box. The rest is the plan he approved
for plan 0001 step 6c.
**Amends:** the payout half of [ADR 0013](0013-a-community-token-exists-and-radar-holds-none-of-it.md)'s
"What this costs", which described a hot key file whose blast radius is one
week of creator fees.

## Context

On Robinhood Chain the week's prize is the creator's balance in the Pons v2 fee
escrow `0xd3af…ac9e`. A sweep credits the escrow; only the creator fee recipient
can claim it ([research 0036](../research/0036-pons-v2-read-from-a-real-launch.md) §5).
So a payout is two transactions from that recipient's wallet: `claim(amount)`,
then a transfer to the winner.

The recipient is fixed at launch, and research 0036 never established whether
it can be changed. A key file copied off the box would therefore hold every
future week's fees, for as long as the token trades, with no way to rotate the
recipient away from the thief.

## Decision

### 1. Signing: Turnkey, with the transaction built and checked locally

`realorrug-payout` builds each unsigned EIP-1559 transaction itself and asks
Turnkey to sign it. **Turnkey is trusted to hold the key, not to sign the right
thing:** the signed bytes must decode strictly to the fields asked for and
recover to the wallet before anything is sent
([`sign_checked`](../../crates/realorrug-payout/src/lib.rs)). The encoder is
proved against mainnet by rebuilding the captured claim `0x07cab768…` from its
fields and getting its hash and sender
([test](../../crates/realorrug-payout/tests/claim_as_mainnet_signed_it.rs)).

- **What the box holds** is a Turnkey API key: a secp256k1 private key in
  `/etc/radar/turnkey.key`, mode 0400, owner `realorrug-payout`. It stamps each
  request; it cannot sign a transaction. The loader refuses a malformed key, one
  outside the curve order, one group or others can read, and one whose public
  key is not the configured one.
- **The policy** allows the `realorrug-payout` user exactly one activity,
  `ACTIVITY_TYPE_SIGN_TRANSACTION_V2` on chain 4663, for either `claim(uint256)`
  to the escrow with no value, or a transfer with empty call data. Export stays
  root's. The expression is in the [deploy guide](../../deploy/README.md).
- **No Turnkey or Ethereum library.** `k256` and `sha3`, one curve for both the
  stamp and the recovery; RLP and the stamp by hand. Turnkey's stamper crate
  would bring a second copy of the curve and digest crates.

**Why no value cap.** The winner changes weekly, so an attacker who controls the
box can already send each week's prize anywhere, up to any cap, as often as the
prize accrues. A cap limits nothing real and adds a way to withhold a prize.
What the policy buys is narrower and real: the key cannot approve a token, call
another contract, or sign on another chain.

**Unverified until the setup proof.** Turnkey's page says Robinhood Chain is at
its top EVM support level but names neither chain 4663 nor mainnet, and
matching a policy on the function is documented but untested here.
`realorrug-payout --setup-proof` settles both with three requests that move no
money: `whoami` answers; a call to the factory is denied; `claim(0)` at nonce
1,000,000 is allowed and recovers to the wallet. Its result goes in research
0037.

### 2. The Turnkey account is the creator fee recipient

Every run reads the factory and refuses unless `creator_fee_recipient` equals
`RADAR_PAYOUT_ADDRESS`, the token is known, and its fees are paid in ETH. Before
launch `REALORRUG_TOKEN` is unset and nothing runs. The wallet holds only ETH:
the gas float, and a claimed prize for the seconds before it is sent (ADR 0013
constraint 2). How the token is launched with this recipient is plan 0001 6d.

### 3. The ledger's money is wei, in a decimal string

`Payout` keeps `recipient` and `at`; its money is a flattened untagged `Paid`:
`Eth { wei, claim_tx, transfer_tx }`, or the legacy `Sol { lamports, signature }`
that nothing writes. `wei` is a string because JavaScript is inexact above 2^53
wei (under 0.01 ETH) and a `u64` stops at 18.4 ETH. The public JSON only gains
fields.

### 4. Gas, refusals, nonces, and a crash between the two transactions

- **The operator pays the gas** from a float in the wallet, never from the
  prize (constraint 3). The captured claim cost 0.0000023 ETH.
- **Nothing is signed before:** the chain id; the identity check; the escrow
  read and `Payout::permitted`; a recipient with no code (contract code, an
  EIP-7702 delegation, or an unreadable answer refuses); both transactions'
  gas estimated with `eth_estimateGas` (Robinhood is an Arbitrum chain, so
  never an assumed 21,000), a fee cap of twice the base fee, a zero tip, a
  quarter margin on the limit, and the wallet's balance covering both.
- **The transfer pays what the escrow says was claimed**, read from the claim's
  `Claimed` event, never the wallet balance. It is read back -- from the
  wallet, to the claim, that value, no call data -- before the ledger is
  written.
- **A Turnkey refusal is an answer**, not retried.
- **`<week>.pending.json`** holds each transaction's signed bytes and hash,
  written before it is sent ([Radar ADR 0017](https://github.com/1xmint/theradar/blob/main/docs/adr/0017-the-journal-records-intent-before-effect-and-replay-proves-only-the-decision.md)). A run that finds one resumes that
  week and nothing else: a claim not yet mined is sent again byte for byte
  while its nonce is free, and stops for the operator when another transaction
  took the nonce; a reverted claim clears the file; **a landed claim is never
  made again**, and the transfer pays its stored figure.
- **`payout.lock`**, created exclusively, stops a manual run beside the timer.
  A stale lock stops payouts and says so.

## What this costs

- **Turnkey is a dependency of every payout.** If it is down, the week waits a
  day. 25 signatures a month are free, then $0.10 each; a week uses two
  ([pricing](https://www.turnkey.com/pricing), checked 2026-09-14).
- **The box can still spend a week's prize** while it holds the API key. What
  changed is that stealing the key file no longer takes the key, and revoking
  the API key in the dashboard ends access without moving the recipient.
- **The operator funds gas**, a money decision at launch.
- **Radar's `radar brief` reads `lamports` and `signature`** and skips a payout it
  cannot read; plan 0001 6d fixes it in Radar before launch.
