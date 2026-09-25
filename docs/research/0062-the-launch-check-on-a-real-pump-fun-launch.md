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

## Not checked

- A top-level System Program transfer in the launch transaction is allowed
  by the allowlist today. It moves SOL, not the token, so it cannot be a
  hidden buy, but a launch that also pays someone would pass. Whether that
  should refuse is open.
- One launch is one sample. A dev buy through a pump.fun instruction other
  than `buy_v2` is covered only by unit tests built from this capture's
  receipt layout, not by a second real transaction.
