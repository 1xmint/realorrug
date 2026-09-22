<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0058 — meme coin outcome stories

**Date:** 2026-09-22.
**Status:** findings from a run over the saved data, recorded for later
research and for the measured outcome rate
[ADR 0033](../adr/0033-the-analyst-talks-price-and-hints-only-on-a-measured-rate.md)
requires. The label counts and top-20 tables below came from
`deploy/cmc/stories.py` run on the project VPS against the daily history
[`docs/research/0057`](0057-what-coinmarketcap-gives-us.md) describes; the
CSV itself stays on the VPS (`~/realorrug-data/cmc/stories.csv`), not in this
repository (`deploy/cmc/README.md`: "no bulk data goes into this
repository").

## What the data is

Every meme coin CMC's "Memes" category ever listed (5,360 ids), each with:

- `memes/info/<id>.json.gz` -- symbol, name, platform and contract address.
- `memes/daily/<id>.json.gz` -- the coin's whole daily OHLCV history from
  `GET /v2/cryptocurrency/ohlcv/historical`, or an empty list if CMC has no
  quotes for it at all.

`stories.py` reduces each coin's daily series to one row: a label plus the
peak, its date, the latest value, drawdown from peak, days from first quote
to peak, days from peak to a collapse (if any), and the latest 30-day median
volume.

## The labels and thresholds

| label | meaning | rule |
|---|---|---|
| `no_data` | CMC has no quotes for this id | zero daily quotes |
| `too_short` | not enough history to call any shape | fewer than 30 daily quotes |
| `collapsed` | fell hard, fast, near the peak | fell below 10% of peak within 90 days of the peak |
| `sustained` | held its value over a long stretch | latest is at or above 50% of peak, and history spans at least 180 days |
| `faded` | everything else -- lost value, but not on the collapse's terms, or hasn't sustained one long enough to say | between the two |

The numbers (30 quotes, 10% of peak, 90 days, 50%, 180 days) are named
constants in `stories.py`, each with a one-line comment on the judgement
behind it. They are a starting cut, not a finding -- moving any of them
would move coins between `faded` and its neighbours, and someone should feel
free to.

**Labels describe the shape of a price or market-cap curve only.** None of
them says or implies a scam, a rug, or intent by any person (AGENTS.md rule
4) -- a coin can "collapse" in this sense from a founder's exit, from a
market-wide crash, from a listing CMC mislabelled, or from thin liquidity
amplifying an ordinary sell. The label does not distinguish those.

## Which value the shape is measured on

CMC's `market_cap` field is `0` for long stretches of some coins' real
history, not merely absent -- verified by inspection: BONK's 2022 quotes on
disk all carry `market_cap: 0` while its 2026 quotes carry a real figure
(`market_cap: 295,769,007.77` on 2026-09-21). Treating a recorded `0` as a
real zero market cap for a coin that plainly had one would be exactly the
"zero is a measurement about your instrument" mistake AGENTS.md rule 8 warns
against. So a coin's whole series uses market cap only if at least one of
its quotes ever carries a market cap above zero; otherwise every quote's
close price stands in for it, and the row's `metric` column says which was
used.

**This matters more than expected.** Of 5,360 coins, 4,220 (79%) never once
carry a positive market cap -- their peak and latest values are close
prices, not market caps, and are not comparable across coins with different
supplies. The two top-20 tables below are restricted to `metric ==
"market_cap"` rows so "peak" means the same thing in every row; the
price-only coins are a separate, larger population this note does not rank.

## Counts (2026-09-22 run, 5,360 coins)

| label | count |
|---|---|
| `sustained` | 78 |
| `faded` | 1,904 |
| `collapsed` | 2,919 |
| `too_short` | 224 |
| `no_data` | 235 |

## Top 20 by peak market cap -- `sustained`

| symbol | peak market cap | peak date | drawdown from peak | days peak to collapse |
|---|---:|---|---:|---|
| M | $5,852,167,531 | 2026-04-23 | 0.438 | -- |
| 币安人生 | $817,357,797 | 2026-06-07 | 0.374 | -- |
| TIBBIR | $422,153,940 | 2025-10-27 | 0.382 | -- |
| USELESS | $416,181,054 | 2025-10-14 | 0.330 | 113 |
| APEPE | $336,067,483 | 2026-05-09 | 0.227 | -- |
| 龙虾 | $238,799,614 | 2026-09-20 | 0.013 | -- |
| BULLA | $114,825,578 | 2026-09-16 | 0.078 | -- |
| PURR | $90,735,045 | 2026-08-23 | 0.177 | -- |
| DOGE | $75,982,503 | 2026-01-05 | 0.286 | -- |
| NEET | $46,164,769 | 2026-08-27 | 0.361 | -- |
| DRB | $24,699,112 | 2026-09-08 | 0.212 | -- |
| BUTTCOIN | $24,561,961 | 2026-06-10 | 0.422 | -- |
| ARARA | $20,868,175 | 2025-09-14 | 0.230 | -- |
| MAX | $9,276,249 | 2026-09-19 | 0.227 | -- |
| 一 | $7,099,940 | 2026-01-22 | 0.465 | -- |
| KORI | $4,249,488 | 2026-05-25 | 0.240 | -- |
| SCAM | $4,180,746 | 2026-09-21 | 0.000 | -- |
| DOPU | $2,265,004 | 2025-10-20 | 0.394 | -- |
| AKITA | $518,525 | 2026-06-08 | 0.303 | -- |
| SLT | $408,818 | 2026-07-05 | 0.076 | -- |

## Top 20 by peak market cap -- `collapsed`

| symbol | peak market cap | peak date | drawdown from peak | days peak to collapse |
|---|---:|---|---:|---|
| BOBO | $5,737,928,964 | 2026-05-24 | 1.000 | 1 |
| AI16Z | $2,593,752,079 | 2025-01-02 | 1.000 | 64 |
| PNUT | $1,798,579,864 | 2024-11-17 | 0.969 | 81 |
| SIREN | $1,715,882,021 | 2026-03-23 | 0.989 | 10 |
| GOAT | $1,218,108,947 | 2024-11-16 | 0.985 | 82 |
| SBBTC | $1,080,951,823 | 2025-02-03 | 0.989 | 8 |
| ERC20 | $1,025,825,758 | 2018-07-05 | 1.000 | 1 |
| $AKUMA | $854,685,020 | 2025-01-10 | 1.000 | 27 |
| PIPPIN | $811,717,923 | 2026-02-24 | 0.977 | 27 |
| AIXBT | $745,437,191 | 2025-01-15 | 0.970 | 77 |
| SKYAI | $728,216,326 | 2026-05-05 | 0.931 | 60 |
| ZEREBRO | $650,212,369 | 2025-01-02 | 0.948 | 30 |
| MOODENG | $614,176,659 | 2024-11-15 | 0.922 | 83 |
| LIGHT | $604,353,363 | 2025-09-29 | 0.807 | 6 |
| SAMO | $595,644,787 | 2021-10-28 | 0.997 | 82 |
| PUPS | $591,219,696 | 2024-04-13 | 0.990 | 68 |
| CHILLGUY | $561,419,815 | 2024-11-27 | 0.973 | 62 |
| EMC2 | $552,781,486 | 2017-12-18 | 1.000 | 79 |
| SHIRO | $425,234,378 | 2024-12-14 | 0.999 | 37 |
| DOGE | $413,994,927 | 2024-11-13 | 0.998 | 81 |

## What was surprising

- **Most coins CMC lists have no usable market cap, ever.** 79% (4,220 of
  5,360) never carry a positive `market_cap` in their whole recorded
  history, only price. That's a bigger share than expected for a listing
  category CMC actively tracks.
- **A symbol on this list is not a unique coin.** `DOGE` appears in both
  top-20 tables above -- two different CMC ids, since the real Dogecoin
  would not show a 99.8% drawdown from a $414M peak. CMC's "Memes" category
  holds many small tokens that reuse a famous name or ticker (`DOGE`, and
  separately `SCAM` as a literal ticker); a symbol alone never identifies
  which coin a row is about, only the `id` column in the CSV does.
- **`too_short` and `no_data` together are 459 coins (8.6%)** -- CMC lists
  them, but there isn't enough (or any) history to say anything about their
  outcome at all.

## What these labels cannot tell us

- **Survivorship.** This is only the coins CMC's Memes category chose to
  list; coins that never got listed, or were delisted before this snapshot,
  are not here. A `collapsed` count this large is a lower bound on how many
  meme coins actually failed, not the true rate.
- **A `collapsed` label is a price shape, never proof of intent.** Nothing
  in this data distinguishes a founder walking away, a liquidity pool
  drained, an exchange delisting, or a broad market crash that took every
  coin down together. AGENTS.md rule 4 is why the label stops at "the price
  did this," not "someone did this."
- **No bonding-curve-stage data.** This is CMC's post-listing daily history
  only; nothing here says whether or when a token graduated from a
  bonding-curve launchpad, which the `launches/` side of the same collector
  ([0057](0057-what-coinmarketcap-gives-us.md)) captures separately and this
  note does not join to it.
- **A `sustained` label describes today, not tomorrow.** It says a coin held
  at least half its peak for at least 180 days as of 2026-09-22; it is not a
  prediction, and AGENTS.md rule 5 (ADR 0033) still requires a measured rate
  before any of this backs a hint about a future price move.

## Sources

- `deploy/cmc/stories.py` and `deploy/cmc/test_stories.py`, run against
  `~/realorrug-data/cmc/memes/{daily,info}` on the VPS, 2026-09-22.
- [0057 -- what CoinMarketCap's startup key gives us](0057-what-coinmarketcap-gives-us.md),
  for how the underlying data was collected.
