<!-- SPDX-License-Identifier: Apache-2.0 -->
# CMC collector

A stdlib-only Python 3.12 script that pulls data from CoinMarketCap's Pro API
and saves it to disk on the project's Linux VPS. Nothing here talks to git,
the analyst, or the public site; it is a separate, offline data-gathering
tool. **No bulk data goes into this repository.**

Why it exists: `INTENT.md` "Settled decisions" records that the project has
an exception to CMC's no-redistribution terms for this internal use. Two
things this data is for:

- The full daily price history of every memecoin CMC tracks -- the raw
  material for "success" and "collapse" stories the analyst can draw on.
- Minute-level candles for the first 24h after a pump.fun token graduates to
  PumpSwap -- the measured outcome rate
  [ADR 0033](../../docs/adr/0033-the-analyst-talks-price-and-hints-only-on-a-measured-rate.md)
  requires before the analyst is allowed to hint at a price move.

## Files

- `collect.py` -- the collector. `memes`, `launches` and `status`
  subcommands (below).
- `test_collect.py` -- offline unit tests, no network access; every
  `urlopen` call is mocked.

## Setup on the VPS

The key is read from `REALORRUG_CMC_API_KEY`, or from `/etc/realorrug/analyst.env`
(the same file [`deploy/analyst.env.example`](../analyst.env.example)
documents for the X and model credentials -- add one more line to it):

```bash
REALORRUG_CMC_API_KEY=your-key-here
```

The key is never written to a saved file, never logged, and scrubbed out of
any exception text before it is raised (`collect._scrub`, tested in
`test_collect.py`).

Data is written under `--data` (default `~/realorrug-data/cmc`), outside the
git checkout.

## Running it

```bash
# See the plan without calling the API.
python3 deploy/cmc/collect.py --dry-run memes
python3 deploy/cmc/collect.py --dry-run launches

# Snapshot the Memes category, fetch metadata, then daily history
# (biggest market cap first). Safe to re-run: anything already on disk
# is skipped.
python3 deploy/cmc/collect.py memes

# Same, for a different CMC category id.
python3 deploy/cmc/collect.py memes --category 6051a82566fc1b42617d6dc6

# One day's price/market cap for every coin in the newest memes listing
# (reads ids from disk, never fetches its own category listing). Safe to
# re-run: today's file is skipped if it already exists.
python3 deploy/cmc/collect.py snapshot

# PumpSwap pairs, plus 1min candles for the first 24h and 1h candles for the
# first 7 days after graduation, for pairs created in the last 90 days.
python3 deploy/cmc/collect.py launches

# A narrower window.
python3 deploy/cmc/collect.py launches --window-days 14

# Counts on disk and remaining credits (GET /v1/key/info costs 0 credits).
python3 deploy/cmc/collect.py status

# Stop a run cleanly once it has spent this many credits total.
python3 deploy/cmc/collect.py --max-credits 20000 memes
```

Run it from a cron entry or a systemd timer on the VPS; it is not a
long-running daemon and does not need its own service unit.

## Keeping the meme history growing: `snapshot`

`memes` collects each coin's full daily OHLCV history once and then skips it
forever under `--resume` -- that's by design, it's a one-off backfill, not a
refresh. Re-running the full history pull daily would cost about one credit
per coin per day (~5,360 credits/day, ~160,000/month against the 450,000/
month plan) for something a much smaller call already gives us: a fresh
point on the same forward-going series.

`snapshot` reads coin ids from the newest `memes/listing-*.json.gz` already
on disk (it never calls the category-listing endpoint itself) and batches
them into `GET /v2/cryptocurrency/quotes/latest`, 100 ids per call. For the
~5,360-coin Memes category that's about 54 calls, and quotes/latest costs 1
credit per 100 ids in a call, so about 54 credits/day (~1,620/month) --
roughly a hundredth of the full-history refresh, for exactly the same
forward series the outcome stories need.

```bash
# One day's price/market cap for every coin in the newest listing.
python3 deploy/cmc/collect.py snapshot
```

Crontab line for a human to install on the VPS (just after midnight UTC, so
the day's memes listing from an earlier `memes` run is already there):

```cron
5 0 * * * cd /path/to/realorrug && /usr/bin/python3 deploy/cmc/collect.py snapshot >> /var/log/realorrug-cmc-snapshot.log 2>&1
```

## What each file on disk holds

Under `--data` (default `~/realorrug-data/cmc`):

- `memes/listing-YYYYMMDD.json.gz` -- the day's full Memes category listing
  (`data.coins[]` from `GET /v1/cryptocurrency/category`): id, symbol, name,
  rank, market cap, platform and date added, for roughly 5,360 coins.
- `memes/info/<id>.json.gz` -- one file per coin id, the metadata batch
  response from `GET /v2/cryptocurrency/info` (urls, description, date
  added, platform, contract address).
- `memes/daily/<id>.json.gz` -- one file per coin id, `{id, quotes}` from
  `GET /v2/cryptocurrency/ohlcv/historical` with `interval=daily`. An
  inactive coin with no quotes is still saved (as an empty list) so it is
  not re-fetched every run.
- `memes/snapshots/YYYYMMDD.jsonl.gz` -- one line per coin from the newest
  `memes/listing-*.json.gz` on disk, from a batched
  `GET /v2/cryptocurrency/quotes/latest` call (see "snapshot" below): id,
  symbol, price, market cap, 24h volume, 24h percent change, circulating
  supply, total supply and the quote's own `last_updated`. A field the
  response didn't carry is written as `null`, never 0 (AGENTS.md rule 8).
- `launches/pairs-YYYYMMDD.jsonl.gz` -- the day's PumpSwap pair listing from
  `GET /v4/dex/spot-pairs/latest`, one JSON object per line.
- `launches/candles/<base mint>.json.gz` -- for pairs created within
  `--window-days`: the pair record, `minute_candles_24h` (pool creation to
  +24h, 1min interval), and `hourly_candles_7d` (pool creation to +7d, 1h
  interval), each from `GET /v1/k-line/candles`. A `1014` (outside the
  plan's historical window) is recorded as `minute_out_of_window: true` /
  `hourly_out_of_window: true` rather than retried.

## Credit costs (verified on this key, 2026-09-22)

- Category listing: cheap, paged 1,000 coins at a time.
- Metadata (`/v2/cryptocurrency/info`): batched 100 ids per call.
- Daily history: BONK returned 1,362 days of daily quotes for 40 credits --
  roughly 0.03 credits/day of history. The full Memes category (~5,360
  coins) is the largest line item; `--max-credits` (default 100,000) stops a
  run cleanly once the plan's monthly 450,000-credit budget is at risk.
- Latest quotes (`/v2/cryptocurrency/quotes/latest`, used by `snapshot`): 1
  credit per 100 ids in a call, so ~54 credits for the whole Memes category
  once a day -- about 1/100th of re-running the daily-history pull.
- DEX pairs listing: standard per-page cost, paged with `scroll_id`.
- Candles (`/v1/k-line/candles`): 1 credit per call regardless of how many
  candles it returns, so a 24h/1min window (up to ~1,440 candles) is chunked
  into as few calls as the endpoint's page size allows, and a 7d/1h window
  (168 candles) fits in one call.
- `GET /v1/key/info` (used by `status`) costs 0 credits.

The plan on this key is 450,000 credits/month and 600 requests/minute; the
collector stays under 500 requests/minute and honors `--max-credits`.

## Docs vs. the packet's facts (found while building this)

The k-line candles endpoint's own docs
(https://pro.coinmarketcap.com/api/documentation/pro-api-reference/ohlcv/get-k-line-candles.md,
fetched 2026-09-22) describe `from`/`to` as "UNIX epoch, integer" with no
unit called out, and do not mention error code `1014` at all, and do not
state a maximum for `limit`. The verified facts above say `from`/`to` are
milliseconds (seconds gives a misleading "exceeds your plan's historical
data access limit" error) and that `1014` means "outside window" --
`collect.py` follows the verified facts, not the endpoint's own docs. The
pairs-listings-latest docs (same fetch) also do not state a maximum for
`limit`; `collect.py` uses page sizes of 1,000 (category), 100 (info batch)
and 100 (DEX pairs) as reasonable defaults, none of them a documented
ceiling.
