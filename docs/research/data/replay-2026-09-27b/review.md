# Replay 2026-09-27b: the lead fix (PR #198), read by the lead

- **Build**: `main` at `21efc31` (PR #198), release-linux run 36291729170, `realorrug`
  sha256 `f92f6592…` checked against `BUILD-INFO.txt` on the workstation and on the VPS.
- **Host**: the VPS, in a fresh directory under `~`. Nothing live changed. Nothing posted,
  signed or spent from a wallet.
- **Model cost**: $0.005083 (frozen) + $0.001474 (fresh). All 14 replies are the model's own;
  none fell back to the template. All 14 pass the three checks (fidelity, forbidden, unknown)
  on both the reply and the report.
- **Josh makes the final call on every reply.** Below is my read, not an acceptance.

## What was run, and why two ways

`realorrug replay` never rebuilds a sheet: it reads the frozen `*.sheet.json` as captured. So:

- **`frozen/`**: the 11 sheets from `../replay-2026-09-27/out/`, replayed with the model. This
  tests the new lead on its own. These sheets still carry the old labels ("per sale, summed",
  "on Pons v2"), because they were captured before PR #198.
- **`fresh/`**: a new capture of the three mints where `CreatorSoldOut` fired (2XHG,
  EYPSU1oh, JB2r), at the live read budget, then replayed with the model. This tests all three
  fixes: the lead, the SOL-total wording, and no "Pons v2" venue. Captures took 13–15 s.
  2XHG is still `CantTell` from its one funding-walk gap; the other two have no gaps.

## The two replies that were wrong on 2026-09-27

In every case where it fired, the report's strongest concern is now the `CreatorSoldOut`
sentence ("The creator's decoded sells cover its decoded buys, and it is not among the
largest sampled token accounts").

| case | 2026-09-27 | frozen (new lead, old labels) | fresh (all fixes) |
|---|---|---|---|
| clean-read-versioned-tx | problem: "0.9431 SOL per sale", "on Pons v2", "creator extraction" | **acceptable**: gives the 0.3336 SOL net, the creator is not among the sampled holders, and it offers the moved-not-sold reading | **acceptable**: 0.3336 SOL net, at least 2965 transactions, "leans sketchy", and it offers the moved-not-sold reading |
| clean-read-pay | problem: invented a "recurring coordinated-launch shape"; the creator was not mentioned | **problem, new**: "it holds just 0.1% of sampled supply". That 0.1% belongs to the largest sampled owner, which the sheet does not identify (`token_ownership`), not to the creator. The signal itself says the creator is *not* among those accounts | **acceptable**: 1.5459 SOL total across sales, 0.0366 SOL net, and the creator is absent from the largest sampled accounts. Each number matches the fact it came from |
| creator-sale-2xhg / incomplete-read-funding | acceptable | acceptable: it says why this is a can't-tell, and gives the 20.2231 SOL net | acceptable: the same, plus at least 437 transactions |

The frozen pay problem gives a role to an unidentified holder, which AGENTS.md §4 forbids. No
check catches it, because every number in it is on the sheet. It happened in 1 draw of 6
across the three cases. The fresh draw of the same mint did not do it. It is worth watching,
not yet worth a code change: the sheet's `token_ownership` label already says "largest owner
among the sampled largest accounts", and the other replies in this run all call that owner
"unidentified". If it recurs, the fix is to have the lead sentence carry the unresolved-role
wording itself.

## The other nine frozen replies (new model draws)

The facts and leads are the same as 2026-09-27; the words are a new draw. My read of each is
**acceptable**: every number matches its fact, no person is accused, and each unknown is called
unknown, not safe. Two phrasings are Josh's to judge:

- creator-sale-catwif and suspicious-launch-creator-buy: a launch-block creator buy "could also
  mean genuine conviction". This guesses at motive, but it is hedged and names an innocent
  reading.
- creator-sale-hbull: an unidentified 24.2% holder "could be a vesting contract, bridge, or
  exchange". This lists possible holders without assigning one, which is the unresolved-role
  wording.

## Files

- `frozen/review.md`: the replay's own output for the 11 frozen sheets.
- `fresh/*.sheet.json`, `fresh/review.md`: the three new captures and their replay.
- `BUILD-INFO.txt`: the build these ran on.

Every file was searched for `api-key` and for email-address patterns before and after it was
copied off the VPS. None were found.
