<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0062 — The launch check on a real pump.fun launch

**Date:** 2026-09-24.
**Status:** findings. The readback [ADR 0037](../adr/0037-the-token-launches-on-pump-fun-and-its-fees-pay-for-operations.md)
decision 6 asks for, run against a launch the network accepted.

## Why

`realorrug launch-check solana` was written from pump.fun's documented
instructions. Before it can gate our own launch (`deploy/LAUNCH.md` step 9),
it has to pass an ordinary launch someone else already made, and refuse the
same launch when it is told a wrong number. A reference proposes, a capture
disposes.

## The launch

Signature `2xhvyYRjNLMiP8e5p1so7EPDAH21WwpnDeicYjVxAofhf5xBR81dDRacxkNkd25PVhXPjLr94QCdCtKNMexkYLYf`,
slot 450167660: one `create_v2` of mint
`5AXeGnseKBZPDC9xJfZPjKy5bnAMYRcMhHFkXrKUzE8m` and one `buy_v2` by its
creator `FvHDLiUXkBFPernE9fDMcv2PKR6ZMy5z4vn9VZXqW1Hk`. The creator stands in
for both the treasury and the dev wallet, which is the shape our own launch
has when the fee recipient is the launching wallet.

## First run: refused for two reasons that were the check's

The build at main `2aff5d2` refused it:

- the allowlist named `pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ`, pump.fun's
  fee program, which `buy_v2` calls itself to read its fee tier;
- the dev buy was token-exact, so the instruction states only a SOL bound,
  and the check read that as "no SOL spend stated".

Both are fixed in [#185](https://github.com/1xmint/realorrug/pull/185): the fee
program is allowed, and the spend is read from the `TradeEvent` pump.fun
writes into the transaction for every trade (sol to the curve, fee, creator
fee). A buy with no such receipt still refuses.

## Second run: clean

Release build of main `28adce5`, sha256
`04300694fedcb9e2c2360ce20fd3eb65394a786860e35a244a9983c10d21d6ff`, run on the
VPS on 2026-09-24 with `--dev-buy-lamports 979355158`. Exit 0:

```text
CLEAN launch 2xhvyYRjNLMiP8e5p1so7EPDAH21WwpnDeicYjVxAofhf5xBR81dDRacxkNkd25PVhXPjLr94QCdCtKNMexkYLYf (slot 450167660)
  PASS    transaction: slot 450167660
  PASS    single launch: mint 5AXeGnseKBZPDC9xJfZPjKy5bnAMYRcMhHFkXrKUzE8m
  PASS    fee recipient: FvHDLiUXkBFPernE9fDMcv2PKR6ZMy5z4vn9VZXqW1Hk (slot 450215728)
  PASS    authorities: both revoked (slot 450215728)
  PASS    dev buy: 979355158 lamports (967264352 to the curve, 9189012 fee, 2901794 creator fee)
  PASS    allowlist: only allowed programs, no other buy
```

979355158 is the sum of the three parts. The creator's balance fell
988032758 lamports in that transaction; the difference is the network fee
and account rent, which are not the buy.

## Third run: a wrong number is refused

Same build, same launch, with `--dev-buy-lamports 967264352` (the curve
amount alone, the figure an operator would give if they forgot the fees).
Exit 1:

```text
NOT CLEAN: launch 2xhvyYRjNLMiP8e5p1so7EPDAH21WwpnDeicYjVxAofhf5xBR81dDRacxkNkd25PVhXPjLr94QCdCtKNMexkYLYf (slot 450167660)
  REFUSE  dev buy: the dev wallet spent 979355158 lamports, not the stated 967264352
```

(the other five lines PASS as above). `deploy/LAUNCH.md` step 6 says to
state the total paid, fees included.

## Addendum, 2026-09-27: the same launch's top-level transfer

The capture above named only the two pump.fun instructions; the same
transaction's full top-level instruction list is six entries: `0`
ComputeBudget, `1` ComputeBudget, `2` System Program `Transfer` of 1,000,000
lamports from the creator to `AStRAnpi6kFrKypragExgeRoJ1QnKH7pbSjLAKQVWUum`,
`3` pump.fun, `4` Associated Token `createIdempotent`, `5` pump.fun. Instruction
`2` is exactly the case the "Not checked" bullet below used to describe: a
top-level System Program transfer riding along with the launch, moving SOL
the check could not yet see was there.

`check_allowlist` now refuses any top-level System Program instruction that
is not `Transfer` or `TransferWithSeed`, and refuses a `Transfer` or
`TransferWithSeed` unless its destination and lamports exactly match a
`StatedTransfer` the caller gave in advance (`--allow-transfer
<address>:<lamports>`, repeatable). An inner (CPI) System Program call --
pump.fun's own `create`/`create_v2` funding the new mint and curve accounts
-- is unconditionally allowed regardless, since `RawInstruction.top_level`
now distinguishes the two rather than treating every System Program call
alike.

Run against this launch's own shape (unit tests built from this capture's
instruction layout, not a second live capture):

- `--allow-transfer AStRAnpi6kFrKypragExgeRoJ1QnKH7pbSjLAKQVWUum:1000000`
  (the tip stated exactly): passes, and the allowlist line now reads `only
  allowed programs, no other buy, 1 stated transfer (1000000 lamports to
  AStRAnpi6kFrKypragExgeRoJ1QnKH7pbSjLAKQVWUum)`.
- No `--allow-transfer` at all: refuses -- "top-level System Program
  transfer of 1000000 lamports to AStRAnpi6kFrKypragExgeRoJ1QnKH7pbSjLAKQVWUum
  (instruction 2): a launch that also pays someone is not a clean launch".
- `--allow-transfer` with the right address but the wrong lamports, or the
  right lamports but the wrong address: refuses the same way -- the stated
  value has to match exactly, not just be present.
- A second `--allow-transfer` naming a transfer this launch does not make:
  refuses -- "a stated transfer of `<n>` lamports to `<addr>` was not found
  in this transaction". Stating a transfer is a claim the check verifies,
  not a blanket exemption.

## Not checked

- One launch is one sample. A dev buy through a pump.fun instruction other
  than `buy_v2` is covered only by unit tests built from this capture's
  receipt layout, not by a second real transaction.
