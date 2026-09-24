# Replay 2026-09-24b: the set for the owner's review

Ten real Solana captures, read on 2026-09-24 and replayed with the model
on build 3eca025 (`realorrug replay cases --model`).
`review.md` has every case's verdict, report, reply and the three checks,
with an `accept?` line for each. All ten replies are the model's own (none
fell back to the template) and all 60 checks pass. Only the owner fills those lines, and only a
case marked yes may move to `crates/realorrug-roast/tests/replay/accepted/`.

| Plan 0002 kind | Case | Level |
|---|---|---|
| suspicious launch | suspicious-launch-creator-buy | Sketchy |
| ordinary launch | ordinary-launch, clean-read-pay, clean-read-versioned-tx | NothingUglyYet |
| graduation | graduated-pumpswap | CantTell |
| incomplete read | incomplete-read-funding, graduated-pumpswap | CantTell |
| creator sale | creator-sale-jimothy, creator-sale-hbull, creator-sale-catwif | CantTell |
| misleading concentration | misleading-concentration-pool | CantTell |

What each case can and cannot show:

- **Suspicious launch** was found by capturing fresh pump.fun launches from
  DexScreener's latest lists. It is the one launch that the rules flagged: the
  creator bought 1 SOL of their own token in the launch block.
- **Two cases were relabelled.** `clean-read-versioned-tx` was an incomplete
  read until the versioned-transaction fix (#177). `clean-read-pay` was first
  picked as suspicious, but the rules now find nothing on it. Both read fully
  and clean.
- **A creator sale shows only as a plain fact, never as a verdict.** The
  "creator sold out" signal exists but nothing on Solana raises it yet. On
  graduated tokens, the creator's trades are also not checked. So these three
  cases are CantTell, and their replies say the creator's buys and sells were
  not checked.
- **Earlier runs of this set refused good drafts**, and each refusal was
  traced before it was fixed: a no-age refusal was the length cut dropping
  the age sentence (#180's prompt change), "funding" read as off-topic for
  a funding gap (#180), and the "2" in a shortened mint ("JB2rSP") read as
  a creator figure (#181). A draft the checks refuse still ships the
  template, and `review.md` shows the refused draft beside it.
- **Several CantTell cases were cut short by the read's time limit** (the
  sheet's gaps say "read stopped"). They are real incomplete reads, not
  fixtures.
