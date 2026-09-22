<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0057 — what CoinMarketCap's startup key gives us, checked live

**Date:** 2026-09-22.
**Status:** findings from live calls, recorded for the collector in
[`deploy/cmc/`](../../deploy/cmc/README.md). Every fact below came from a
call made on 2026-09-22 from the project VPS with the startup-plan key; none
is from CMC's documentation alone (AGENTS.md §1: a reference proposes, a
capture disposes).

## Why

The owner wants the history of the biggest memecoins, and saved data from
which success and collapse stories can be drawn. The same data can later
supply the measured outcome rate
[ADR 0033](../adr/0033-the-analyst-talks-price-and-hints-only-on-a-measured-rate.md)
requires before the analyst may hint at a price move.

**Terms.** CMC's standard terms forbid publishing anything derived from its
data. The owner has an exception for this project (recorded in `INTENT.md`
"Settled decisions", 2026-09-22), so collection is not limited by those terms.

## The plan's limits

| fact | value | how checked |
|---|---|---|
| plan | startup, 450,000 credits a month | `/v1/key/info` (costs 0 credits) |
| rate | 600 requests a minute | same |
| credits reset | 2026-10-01 | same |

The key lives only on the VPS, in `/etc/realorrug/analyst.env` as
`REALORRUG_CMC_API_KEY`, and the collector reads it there.

## Memecoin history

- CMC's "Memes" category is id `6051a82566fc1b42617d6dc6` and held 5,360
  coins on the day.
- `/v2/cryptocurrency/ohlcv/historical` with a daily interval returns a
  coin's whole history in one call: BONK (id 23095) returned 1,362 days for
  40 credits.
- A coin CMC marks inactive returns no quotes at all. An empty history
  means "CMC has none", not "the coin never traded" (AGENTS.md rule 8).

## Launch candles

- `/v1/k-line/candles` takes `from` and `to` in **milliseconds**. Passed in
  seconds, it answers "exceeds plan's historical limit", which looks like a
  plan limit and is not one.
- One-minute candles reach back at least 90 days; asking for 365 days
  returns error `1014`.
- Each row is `[open, high, low, close, volume, timestamp_ms, traders]`,
  and one call costs 1 credit.
- `/v4/dex/pairs/ohlcv/historical` is deprecated; the k-line endpoint
  replaces it.

## DEX pair listings

- The DEX and k-line endpoints send `error_code` and `credit_count` as
  strings (`"0"`, `"1014"`). Read as numbers without conversion, `"0"`
  looked like an error and stopped the first run.
- `/v4/dex/spot-pairs/latest` puts its paging cursor, `scroll_id`, on
  **every row**, not in `status`. The next page takes the last row's.
  Reading it from `status` stopped the listing after 100 pairs.
- `pool_created` and `created_at` are always null, both there and in
  `/v4/dex/pairs/quotes/latest`. There is no pool creation time anywhere.
- PumpSwap is dex id 12062 (slug `pumpswap`); the pump.fun bonding curve is
  slug `pumpfun` (id 10979).

## What the collector does about it

With no creation time, a token's start is read from its own candles: the
first daily candle in the window, then the first hourly candle inside that
day (2 credits). A token whose first daily candle falls on the window's
opening day may have traded earlier, so it is marked `older_than_window`
rather than given a wrong start. The saved field is still named
`pool_created_ms` for the files already written, but it holds the first
traded hour, not the pool's creation.

**Not checked:** whether k-line candles for a mint include bonding-curve
trades or only PumpSwap trades. Until that is checked against a token whose
graduation slot is known, "first candle" is the first trade CMC saw, not a
proven graduation or launch moment.

## First runs

A 40-credit trial on 2026-09-22 listed 200 PumpSwap pairs and saved 1,471
one-minute candles for the first new token. The full runs started the same
day on the VPS: memecoin history capped at 200,000 credits, then launches
over 90 days capped at 150,000, logs in `~/realorrug-data/cmc/logs/`.

## Sources

- Live calls from the VPS, 2026-09-22 (probe scripts not kept; the key never
  left the VPS).
- [`deploy/cmc/collect.py`](../../deploy/cmc/collect.py) and its tests.
