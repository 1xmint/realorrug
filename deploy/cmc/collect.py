#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""CoinMarketCap collector for realorrug (stdlib only, Python 3.12).

Saves two things to disk on the project's Linux VPS, for internal use only:

(a) `memes`: a daily snapshot of the whole CMC Memes category, its per-coin
    metadata, and the full daily price history for every coin in it -- the
    raw material for "success" and "collapse" stories.
(b) `launches`: PumpSwap pairs and, for pairs created within a recent window,
    minute-level candles for the first 24h after graduation plus an hourly
    series for the first 7 days -- the measured outcome rate ADR 0033 needs
    before the analyst is allowed to hint at a price move.

No network calls happen unless a subcommand is run; `--dry-run` prints the
plan and touches nothing. The API key is never written to disk, logged, or
allowed into an exception string (see `_scrub`).

Run `python3 collect.py --help` for the full flag list, or read
deploy/cmc/README.md for the exact command lines used on the VPS.
"""
from __future__ import annotations

import argparse
import gzip
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable, Iterator

API_BASE = "https://pro-api.coinmarketcap.com"
MEMES_CATEGORY_ID = "6051a82566fc1b42617d6dc6"
KEY_ENV_VAR = "REALORRUG_CMC_API_KEY"
KEY_FILE_PATH = "/etc/realorrug/analyst.env"
KEY_FILE_VAR_NAME = "REALORRUG_CMC_API_KEY"

DEFAULT_DATA_DIR = Path.home() / "realorrug-data" / "cmc"
DEFAULT_MAX_CREDITS = 100_000
# The docs at pro.coinmarketcap.com don't state a per-minute cap; the packet's
# 600 requests/minute plan limit is the source, kept under with margin.
MAX_REQUESTS_PER_MINUTE = 500
CATEGORY_PAGE_LIMIT = 1000
INFO_BATCH_SIZE = 100
HISTORY_COUNT = 10_000
CANDLE_LIMIT = 500  # undocumented max; kept conservative (packet: "check the docs")
DEX_PAGE_LIMIT = 100

RETRYABLE_ERROR_CODE = 1008
OUT_OF_WINDOW_ERROR_CODE = 1014
MAX_RETRIES = 5
RETRY_BASE_SECONDS = 2.0


class OutOfCreditBudget(Exception):
    """Raised to unwind cleanly once --max-credits is reached."""


class CmcApiError(Exception):
    """A non-retryable CMC error for a single item; caller logs and skips."""


def _scrub(text: str, key: str | None) -> str:
    """Remove the API key from any string before it is logged or raised.

    Called on every log line and every exception message that might carry
    request context (e.g. a URL). Never assume a call site remembered to
    scrub -- this is the one place that guarantees it.
    """
    if not key:
        return text
    return text.replace(key, "***REDACTED***")


def load_api_key() -> str:
    """Read the API key from the environment, else the VPS analyst.env file.

    Never logs the path contents or the key itself. Raises SystemExit with a
    message that names neither the key nor any fragment of it.
    """
    env_key = os.environ.get(KEY_ENV_VAR)
    if env_key:
        return env_key.strip()
    try:
        text = Path(KEY_FILE_PATH).read_text(encoding="utf-8")
    except OSError:
        raise SystemExit(
            f"No API key: set {KEY_ENV_VAR} or provide {KEY_FILE_VAR_NAME} in {KEY_FILE_PATH}"
        )
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        name, _, value = line.partition("=")
        if name.strip() == KEY_FILE_VAR_NAME:
            value = value.strip().strip('"').strip("'")
            if value:
                return value
    raise SystemExit(
        f"No API key: set {KEY_ENV_VAR} or provide {KEY_FILE_VAR_NAME} in {KEY_FILE_PATH}"
    )


def log(msg: str, *, key: str | None = None) -> None:
    print(_scrub(msg, key), file=sys.stderr, flush=True)


@dataclass
class RateLimiter:
    """Keeps request timestamps under MAX_REQUESTS_PER_MINUTE, sliding window."""

    max_per_minute: int = MAX_REQUESTS_PER_MINUTE
    _timestamps: list[float] = field(default_factory=list)

    def wait_for_slot(self, now_fn=time.time, sleep_fn=time.sleep) -> None:
        now = now_fn()
        cutoff = now - 60.0
        self._timestamps = [t for t in self._timestamps if t > cutoff]
        if len(self._timestamps) >= self.max_per_minute:
            sleep_for = self._timestamps[0] + 60.0 - now
            if sleep_for > 0:
                sleep_fn(sleep_for)
            now = now_fn()
            cutoff = now - 60.0
            self._timestamps = [t for t in self._timestamps if t > cutoff]
        self._timestamps.append(now)


@dataclass
class CreditBudget:
    max_credits: int
    used: int = 0

    def add(self, credit_count: int) -> None:
        self.used += credit_count
        if self.used >= self.max_credits:
            raise OutOfCreditBudget(
                f"credit budget reached: {self.used} >= {self.max_credits}"
            )


@dataclass
class Stats:
    items: int = 0
    credits: int = 0
    errors: int = 0

    def summary(self) -> str:
        return f"items={self.items} credits={self.credits} errors={self.errors}"


class CmcClient:
    """Thin wrapper over urllib for the CMC Pro API. No third-party deps."""

    def __init__(
        self,
        api_key: str,
        budget: CreditBudget,
        rate_limiter: RateLimiter | None = None,
        base_url: str = API_BASE,
        opener=None,
    ) -> None:
        self._api_key = api_key
        self._budget = budget
        self._rate_limiter = rate_limiter or RateLimiter()
        self._base_url = base_url
        self._urlopen = opener or urllib.request.urlopen

    def get(self, path: str, params: dict[str, Any]) -> dict[str, Any]:
        """GET path with params, retrying on 429 / error_code 1008.

        Raises CmcApiError for other error codes (caller logs and skips).
        Raises OutOfCreditBudget once the summed credit_count reaches the cap.
        Never lets the API key reach an exception message or a log line.
        """
        query = urllib.parse.urlencode(params)
        url = f"{self._base_url}{path}?{query}"
        request = urllib.request.Request(
            url, headers={"X-CMC_PRO_API_KEY": self._api_key, "Accept": "application/json"}
        )
        attempt = 0
        while True:
            attempt += 1
            self._rate_limiter.wait_for_slot()
            try:
                with self._urlopen(request, timeout=30) as response:
                    body = response.read()
            except urllib.error.HTTPError as exc:
                if exc.code == 429 and attempt <= MAX_RETRIES:
                    self._sleep_backoff(attempt)
                    continue
                raise CmcApiError(_scrub(f"HTTP {exc.code} for {path}", self._api_key)) from None
            except urllib.error.URLError as exc:
                raise CmcApiError(_scrub(f"connection error for {path}: {exc.reason}", self._api_key)) from None

            try:
                payload = json.loads(body)
            except json.JSONDecodeError as exc:
                raise CmcApiError(_scrub(f"bad JSON from {path}: {exc}", self._api_key)) from None

            status = payload.get("status", {})
            credit_count = status.get("credit_count", 0) or 0
            error_code = status.get("error_code", 0) or 0

            if error_code == RETRYABLE_ERROR_CODE and attempt <= MAX_RETRIES:
                self._sleep_backoff(attempt)
                continue

            self._budget.add(credit_count)

            if error_code == OUT_OF_WINDOW_ERROR_CODE:
                raise Cmc1014Error(status.get("error_message", "out of window"))

            if error_code:
                raise CmcApiError(
                    _scrub(f"{path}: error_code={error_code} {status.get('error_message')}", self._api_key)
                )

            return payload

    def _sleep_backoff(self, attempt: int, sleep_fn=time.sleep) -> None:
        sleep_fn(RETRY_BASE_SECONDS * (2 ** (attempt - 1)))


class Cmc1014Error(Exception):
    """Candle request fell outside the plan's historical window. Record, skip, never retry."""


def atomic_write_bytes(path: Path, data: bytes) -> None:
    """Write via a temp file then rename, so a crash never leaves a partial file."""
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_bytes(data)
    os.replace(tmp, path)


def write_json_gz(path: Path, obj: Any) -> None:
    data = gzip.compress(json.dumps(obj).encode("utf-8"))
    atomic_write_bytes(path, data)


def append_jsonl_gz_line(existing: bytes | None, obj: Any) -> bytes:
    """Return existing (decompressed) jsonl bytes with one more record appended."""
    line = (json.dumps(obj) + "\n").encode("utf-8")
    return (existing or b"") + line


def read_gz_bytes(path: Path) -> bytes | None:
    if not path.exists():
        return None
    with gzip.open(path, "rb") as f:
        return f.read()


# --------------------------------------------------------------------------
# memes subcommand
# --------------------------------------------------------------------------


def iter_category_coins(
    client: CmcClient, category_id: str, page_limit: int = CATEGORY_PAGE_LIMIT
) -> Iterator[dict[str, Any]]:
    """Yield every coin in the category, paging with start/limit."""
    start = 1
    while True:
        payload = client.get(
            "/v1/cryptocurrency/category",
            {"id": category_id, "start": start, "limit": page_limit},
        )
        coins = payload.get("data", {}).get("coins", [])
        if not coins:
            return
        yield from coins
        if len(coins) < page_limit:
            return
        start += page_limit


def chunked(items: list[Any], size: int) -> Iterator[list[Any]]:
    for i in range(0, len(items), size):
        yield items[i : i + size]


def cmd_memes(args: argparse.Namespace, client: CmcClient | None, api_key: str) -> Stats:
    stats = Stats()
    data_dir = Path(args.data)
    date_stamp = time.strftime("%Y%m%d", time.gmtime())
    listing_path = data_dir / "memes" / f"listing-{date_stamp}.json.gz"
    info_dir = data_dir / "memes" / "info"
    daily_dir = data_dir / "memes" / "daily"

    if args.dry_run:
        log(
            f"[dry-run] memes: category={args.category} -> {listing_path}, "
            f"info -> {info_dir}/<id>.json.gz, daily history -> {daily_dir}/<id>.json.gz"
        )
        return stats

    assert client is not None

    coins: list[dict[str, Any]]
    if listing_path.exists() and args.resume:
        log(f"resume: listing already at {listing_path}")
        coins = json.loads(gzip.decompress(listing_path.read_bytes()))
    else:
        try:
            coins = list(iter_category_coins(client, args.category))
        except OutOfCreditBudget:
            log("credit budget reached during category listing")
            return stats
        write_json_gz(listing_path, coins)
        log(f"wrote listing: {len(coins)} coins -> {listing_path}")

    stats.items += len(coins)

    ids = [str(c["id"]) for c in coins if "id" in c]

    # Metadata, batched.
    for batch in chunked(ids, INFO_BATCH_SIZE):
        missing = [i for i in batch if not (info_dir / f"{i}.json.gz").exists()]
        if not missing:
            continue
        try:
            payload = client.get("/v2/cryptocurrency/info", {"id": ",".join(missing)})
        except OutOfCreditBudget:
            log("credit budget reached during info fetch")
            return stats
        except CmcApiError as exc:
            log(f"info batch failed, skipping: {exc}")
            stats.errors += 1
            continue
        data = payload.get("data", {})
        for coin_id, info in data.items():
            write_json_gz(info_dir / f"{coin_id}.json.gz", info)
        log(f"info: wrote {len(data)} of {len(missing)} requested")

    # Daily history, biggest market cap first.
    def market_cap(coin: dict[str, Any]) -> float:
        try:
            return float(coin.get("quote", {}).get("USD", {}).get("market_cap") or 0)
        except (TypeError, ValueError):
            return 0.0

    ordered = sorted(coins, key=market_cap, reverse=True)
    for coin in ordered:
        coin_id = str(coin.get("id"))
        if not coin_id or coin_id == "None":
            continue
        out_path = daily_dir / f"{coin_id}.json.gz"
        if out_path.exists() and args.resume:
            continue
        try:
            payload = client.get(
                "/v2/cryptocurrency/ohlcv/historical",
                {
                    "id": coin_id,
                    "time_start": "2013-01-01",
                    "interval": "daily",
                    "count": HISTORY_COUNT,
                },
            )
        except OutOfCreditBudget:
            log("credit budget reached during daily history fetch")
            return stats
        except CmcApiError as exc:
            log(f"daily history for {coin_id} failed, skipping: {exc}")
            stats.errors += 1
            continue
        quotes = _extract_quotes(payload.get("data"), coin_id)
        # Save even an empty result (inactive coin) so it is not re-fetched.
        write_json_gz(out_path, {"id": coin_id, "quotes": quotes})
        log(f"daily history {coin_id}: {len(quotes)} quotes")

    return stats


def _extract_quotes(data: Any, coin_id: str) -> list[dict[str, Any]]:
    """Handle both response shapes: a single object, or keyed by id."""
    if data is None:
        return []
    if isinstance(data, dict) and "quotes" in data:
        return data.get("quotes") or []
    if isinstance(data, dict):
        entry = data.get(coin_id) or data.get(int(coin_id)) if coin_id.isdigit() else None
        if entry is None:
            # keyed by id but our id's type didn't match a lookup above
            for key, value in data.items():
                if str(key) == coin_id:
                    entry = value
                    break
        if isinstance(entry, dict):
            return entry.get("quotes") or []
        if isinstance(entry, list):
            return entry
    return []


# --------------------------------------------------------------------------
# launches subcommand
# --------------------------------------------------------------------------


def iter_dex_pairs(
    client: CmcClient,
    network_slug: str = "solana",
    dex_slug: str = "pumpswap",
    limit: int = DEX_PAGE_LIMIT,
) -> Iterator[dict[str, Any]]:
    """Page PumpSwap pairs via scroll_id cursor pagination."""
    scroll_id = None
    while True:
        params: dict[str, Any] = {
            "network_slug": network_slug,
            "dex_slug": dex_slug,
            "limit": limit,
            "aux": "pool_created",
        }
        if scroll_id:
            params["scroll_id"] = scroll_id
        payload = client.get("/v4/dex/spot-pairs/latest", params)
        data = payload.get("data", [])
        if not data:
            return
        yield from data
        next_scroll = payload.get("status", {}).get("scroll_id") or payload.get("scroll_id")
        if not next_scroll or next_scroll == scroll_id:
            return
        scroll_id = next_scroll


def _ms_chunks(start_ms: int, end_ms: int, span_ms: int) -> Iterator[tuple[int, int]]:
    cur = start_ms
    while cur < end_ms:
        nxt = min(cur + span_ms, end_ms)
        yield cur, nxt
        cur = nxt


def fetch_candles(
    client: CmcClient,
    address: str,
    from_ms: int,
    to_ms: int,
    interval: str,
    platform: str = "solana",
    limit: int = CANDLE_LIMIT,
) -> tuple[list[list[Any]], bool]:
    """Fetch candles across the [from_ms, to_ms) window, paging in chunks.

    Returns (candles, out_of_window). A 1014 on any chunk is recorded by
    setting out_of_window True and stopping further chunks for this address;
    already-collected candles are still returned.
    """
    interval_ms = _INTERVAL_MS.get(interval, 60_000)
    span_ms = interval_ms * limit
    candles: list[list[Any]] = []
    for chunk_start, chunk_end in _ms_chunks(from_ms, to_ms, span_ms):
        try:
            payload = client.get(
                "/v1/k-line/candles",
                {
                    "platform": platform,
                    "address": address,
                    "interval": interval,
                    "from": chunk_start,
                    "to": chunk_end,
                    "limit": limit,
                },
            )
        except Cmc1014Error:
            return candles, True
        chunk = payload.get("data", [])
        if isinstance(chunk, list):
            candles.extend(chunk)
    return candles, False


_INTERVAL_MS = {
    "1min": 60_000,
    "1h": 3_600_000,
}

HOUR_MS = 3_600_000
DAY_MS = 24 * HOUR_MS


def cmd_launches(args: argparse.Namespace, client: CmcClient | None, api_key: str) -> Stats:
    stats = Stats()
    data_dir = Path(args.data)
    date_stamp = time.strftime("%Y%m%d", time.gmtime())
    pairs_path = data_dir / "launches" / f"pairs-{date_stamp}.jsonl.gz"
    candles_dir = data_dir / "launches" / "candles"

    if args.dry_run:
        log(
            f"[dry-run] launches: window={args.window_days}d -> {pairs_path}, "
            f"candles -> {candles_dir}/<base mint>.json.gz"
        )
        return stats

    assert client is not None

    now_ms = int(time.time() * 1000)
    window_ms = args.window_days * DAY_MS

    pairs: list[dict[str, Any]] = []
    try:
        for pair in iter_dex_pairs(client):
            pairs.append(pair)
            stats.items += 1
    except OutOfCreditBudget:
        log("credit budget reached during pair listing")
    if pairs:
        payload_bytes = b"".join((json.dumps(p) + "\n").encode("utf-8") for p in pairs)
        atomic_write_bytes(pairs_path, gzip.compress(payload_bytes))
        log(f"wrote {len(pairs)} pairs -> {pairs_path}")

    for pair in pairs:
        base_mint = pair.get("base_asset_contract_address")
        pool_created = _pool_created_ms(pair)
        if not base_mint or pool_created is None:
            continue
        if now_ms - pool_created > window_ms:
            continue
        out_path = candles_dir / f"{base_mint}.json.gz"
        if out_path.exists() and args.resume:
            continue
        try:
            minute_candles, min_oow = fetch_candles(
                client, base_mint, pool_created, pool_created + DAY_MS, "1min"
            )
            hourly_candles, hour_oow = fetch_candles(
                client, base_mint, pool_created, pool_created + 7 * DAY_MS, "1h"
            )
        except OutOfCreditBudget:
            log("credit budget reached during candle fetch")
            return stats
        except CmcApiError as exc:
            log(f"candles for {base_mint} failed, skipping: {exc}")
            stats.errors += 1
            continue
        write_json_gz(
            out_path,
            {
                "pair": pair,
                "pool_created_ms": pool_created,
                "minute_candles_24h": minute_candles,
                "minute_out_of_window": min_oow,
                "hourly_candles_7d": hourly_candles,
                "hourly_out_of_window": hour_oow,
            },
        )
        log(
            f"candles {base_mint}: {len(minute_candles)} 1min, {len(hourly_candles)} 1h"
            + (" (1min out of window)" if min_oow else "")
        )

    return stats


def _pool_created_ms(pair: dict[str, Any]) -> int | None:
    """Pull pool creation time (ms) from the aux `pool_created` field or created_at."""
    aux = pair.get("pool_created") or pair.get("aux", {}).get("pool_created")
    value = aux if aux is not None else pair.get("created_at")
    if value is None:
        return None
    try:
        value = int(value)
    except (TypeError, ValueError):
        return None
    # created_at/pool_created may be seconds; CMC's own timestamps are seconds
    # elsewhere in the API. Treat anything under 10^12 as seconds.
    if value < 1_000_000_000_000:
        value *= 1000
    return value


# --------------------------------------------------------------------------
# status subcommand
# --------------------------------------------------------------------------


def cmd_status(args: argparse.Namespace, client: CmcClient | None, api_key: str) -> Stats:
    stats = Stats()
    data_dir = Path(args.data)
    memes_daily = data_dir / "memes" / "daily"
    memes_info = data_dir / "memes" / "info"
    launches_candles = data_dir / "launches" / "candles"

    def count(d: Path) -> int:
        return len(list(d.glob("*.json.gz"))) if d.exists() else 0

    log(f"memes/daily: {count(memes_daily)} files")
    log(f"memes/info: {count(memes_info)} files")
    log(f"launches/candles: {count(launches_candles)} files")

    if args.dry_run:
        log("[dry-run] status: would call GET /v1/key/info (0 credits)")
        return stats

    assert client is not None
    try:
        payload = client.get("/v1/key/info", {})
    except CmcApiError as exc:
        log(f"key/info failed: {exc}")
        stats.errors += 1
        return stats
    usage = payload.get("data", {}).get("usage", {})
    log(f"key/info: {json.dumps(usage)}")
    return stats


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", default=str(DEFAULT_DATA_DIR), help="data directory root")
    parser.add_argument(
        "--max-credits", type=int, default=DEFAULT_MAX_CREDITS, help="stop once summed credit_count reaches this"
    )
    parser.add_argument("--dry-run", action="store_true", help="print the plan, call nothing")
    parser.add_argument("--no-resume", dest="resume", action="store_false", default=True, help="refetch even if already on disk")

    sub = parser.add_subparsers(dest="command", required=True)

    p_memes = sub.add_parser("memes", help="snapshot the Memes category, metadata and daily history")
    p_memes.add_argument("--category", default=MEMES_CATEGORY_ID, help="CMC category id")
    p_memes.set_defaults(func=cmd_memes)

    p_launches = sub.add_parser("launches", help="PumpSwap pairs and post-graduation candles")
    p_launches.add_argument("--window-days", type=int, default=90, help="only fetch candles for pairs created within this many days")
    p_launches.set_defaults(func=cmd_launches)

    p_status = sub.add_parser("status", help="counts on disk and remaining credits")
    p_status.set_defaults(func=cmd_status)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    api_key = None if args.dry_run else load_api_key()

    client = None
    budget = CreditBudget(max_credits=args.max_credits)
    if not args.dry_run:
        client = CmcClient(api_key, budget)

    start = time.time()
    try:
        stats = args.func(args, client, api_key or "")
    except OutOfCreditBudget as exc:
        log(f"stopped: {exc}", key=api_key)
        stats = Stats(credits=budget.used)
    stats.credits = budget.used
    elapsed = time.time() - start
    log(f"summary: {stats.summary()} elapsed={elapsed:.1f}s", key=api_key)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
