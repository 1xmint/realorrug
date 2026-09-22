#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Turn one coin's saved CMC daily history into a labelled outcome story.

Reads the files `deploy/cmc/collect.py` already wrote under
`--data/memes/{daily,info}/<id>.json.gz` (stdlib only, Python 3, no network
calls -- this script never talks to CoinMarketCap). A label describes the
shape of the price (or market cap) history only: it never says "scam",
"rug", or anything that implies intent or names a person (AGENTS.md rule 4).

Run `python3 stories.py <data_dir> --out <file.csv>`.
"""
from __future__ import annotations

import argparse
import csv
import gzip
import json
import statistics
import sys
from dataclasses import dataclass, fields
from datetime import date, datetime
from pathlib import Path
from typing import Any, Iterable

# --- Labelling thresholds -----------------------------------------------
# Every threshold here is a judgement call about how to name a price shape,
# not a fact from CMC. Named and commented so the next reader can see (and
# change) the reasoning without re-deriving it from the code.

MIN_QUOTES_FOR_STORY = 30
"""Fewer daily quotes than this and there isn't enough history to call any
shape at all -- too easy for one good or bad week to look like a trend."""

SUSTAINED_MIN_HISTORY_DAYS = 180
"""A coin needs at least this many days on record before "held its value"
means anything more than "hasn't had time to fall yet"."""

SUSTAINED_LATEST_RATIO = 0.5
"""latest / peak at or above this, with enough history, reads as "held up"
rather than "still standing by chance"."""

COLLAPSE_RATIO = 0.10
"""Falling below this fraction of the peak is treated as the value having
left, not just a large ordinary drawdown."""

COLLAPSE_WINDOW_DAYS = 90
"""A drop below COLLAPSE_RATIO counts as "collapsed" only if it happened
within this many days of the peak -- a slow multi-year decline is "faded",
not the same shape as a fast collapse."""

VOLUME_WINDOW_DAYS = 30
"""Window used for the reported "latest N-day median volume" figure."""

LABELS = ("sustained", "faded", "collapsed", "too_short", "no_data")


@dataclass(frozen=True)
class DailyPoint:
    """One day's data, already reduced to what the labelling needs."""

    when: date
    price: float
    market_cap: float | None
    volume: float


@dataclass(frozen=True)
class Story:
    """One coin's outcome record. Field order is the CSV column order."""

    id: str
    symbol: str
    name: str
    platform: str
    contract_address: str
    label: str
    metric: str  # "market_cap" or "price": which one peak/latest/drawdown use
    quote_count: int
    first_date: str
    peak_value: float | None
    peak_date: str
    latest_value: float | None
    latest_date: str
    drawdown_from_peak: float | None
    days_first_to_peak: int | None
    days_peak_to_collapse: int | None
    latest_30d_median_volume: float | None


def _parse_quotes(raw_quotes: Iterable[dict[str, Any]]) -> list[DailyPoint]:
    """Reduce the raw CMC quote objects to `DailyPoint`s, oldest first.

    A quote missing its USD block or timestamp is dropped rather than
    guessed at (AGENTS.md rule 8: absent is not zero).
    """
    points: list[DailyPoint] = []
    for raw in raw_quotes:
        usd = raw.get("quote", {}).get("USD")
        if not usd:
            continue
        timestamp = usd.get("timestamp")
        close = usd.get("close")
        if timestamp is None or close is None:
            continue
        when = datetime.fromisoformat(timestamp.replace("Z", "+00:00")).date()
        market_cap = usd.get("market_cap")
        volume = usd.get("volume") or 0.0
        points.append(DailyPoint(when=when, price=float(close), market_cap=market_cap, volume=float(volume)))
    points.sort(key=lambda p: p.when)
    return points


def _choose_metric(points: list[DailyPoint]) -> str:
    """Decide whether market cap or price measures this coin's history.

    CMC records `market_cap: 0` for long stretches of some coins' early
    history rather than omitting the field (verified by inspection: BONK's
    2022 quotes carry `market_cap: 0` while its 2026 quotes carry a real
    figure). Zero is a measurement about CMC's supply data here, not a fact
    about the coin (AGENTS.md rule 8), so a coin only counts as having
    market-cap data if at least one quote's market cap is a positive number.
    Otherwise every quote's price stands in for it, and the record says so.
    """
    if any(p.market_cap is not None and p.market_cap > 0 for p in points):
        return "market_cap"
    return "price"


def _value(point: DailyPoint, metric: str) -> float:
    if metric == "market_cap":
        return point.market_cap or 0.0
    return point.price


def build_story(
    coin_id: str,
    symbol: str,
    name: str,
    platform: str,
    contract_address: str,
    raw_quotes: Iterable[dict[str, Any]],
) -> Story:
    """Pure function: one coin's raw daily quotes in, one labelled Story out.

    Takes no clock, no filesystem, no network -- everything it needs is in
    `raw_quotes`, so it is exercised directly in `test_stories.py`.
    """
    points = _parse_quotes(raw_quotes)

    if not points:
        return Story(
            id=coin_id,
            symbol=symbol,
            name=name,
            platform=platform,
            contract_address=contract_address,
            label="no_data",
            metric="none",
            quote_count=0,
            first_date="",
            peak_value=None,
            peak_date="",
            latest_value=None,
            latest_date="",
            drawdown_from_peak=None,
            days_first_to_peak=None,
            days_peak_to_collapse=None,
            latest_30d_median_volume=None,
        )

    metric = _choose_metric(points)
    values = [_value(p, metric) for p in points]

    first = points[0]
    latest = points[-1]
    peak_index = max(range(len(points)), key=lambda i: values[i])
    peak = points[peak_index]
    peak_value = values[peak_index]
    latest_value = values[-1]

    recent_volumes = [p.volume for p in points[-VOLUME_WINDOW_DAYS:]]
    median_volume = statistics.median(recent_volumes) if recent_volumes else None

    if len(points) < MIN_QUOTES_FOR_STORY:
        return Story(
            id=coin_id,
            symbol=symbol,
            name=name,
            platform=platform,
            contract_address=contract_address,
            label="too_short",
            metric=metric,
            quote_count=len(points),
            first_date=first.when.isoformat(),
            peak_value=peak_value,
            peak_date=peak.when.isoformat(),
            latest_value=latest_value,
            latest_date=latest.when.isoformat(),
            drawdown_from_peak=None,
            days_first_to_peak=(peak.when - first.when).days,
            days_peak_to_collapse=None,
            latest_30d_median_volume=median_volume,
        )

    drawdown = None if peak_value <= 0 else 1.0 - (latest_value / peak_value)

    days_peak_to_collapse: int | None = None
    if peak_value > 0:
        collapse_threshold = COLLAPSE_RATIO * peak_value
        for p, v in zip(points[peak_index:], values[peak_index:]):
            if v < collapse_threshold:
                days_peak_to_collapse = (p.when - peak.when).days
                break

    total_history_days = (latest.when - first.when).days
    latest_ratio = None if peak_value <= 0 else latest_value / peak_value

    if days_peak_to_collapse is not None and days_peak_to_collapse <= COLLAPSE_WINDOW_DAYS:
        label = "collapsed"
    elif (
        latest_ratio is not None
        and latest_ratio >= SUSTAINED_LATEST_RATIO
        and total_history_days >= SUSTAINED_MIN_HISTORY_DAYS
    ):
        label = "sustained"
    else:
        label = "faded"

    return Story(
        id=coin_id,
        symbol=symbol,
        name=name,
        platform=platform,
        contract_address=contract_address,
        label=label,
        metric=metric,
        quote_count=len(points),
        first_date=first.when.isoformat(),
        peak_value=peak_value,
        peak_date=peak.when.isoformat(),
        latest_value=latest_value,
        latest_date=latest.when.isoformat(),
        drawdown_from_peak=drawdown,
        days_first_to_peak=(peak.when - first.when).days,
        days_peak_to_collapse=days_peak_to_collapse,
        latest_30d_median_volume=median_volume,
    )


# --- File I/O and CLI -----------------------------------------------------


def _read_json_gz(path: Path) -> Any:
    with gzip.open(path, "rt", encoding="utf-8") as fh:
        return json.load(fh)


def _info_fields(info: dict[str, Any]) -> tuple[str, str, str, str]:
    """Pull symbol/name/platform/contract out of one info file.

    `platform` is CMC's single "issued on" chain for the coin (None for a
    base-layer coin); `contract_address` is that platform's token address.
    Coins with the same token deployed on several chains also carry a
    `contract_address` list under other platforms, but the collector's own
    README documents only this per-coin `platform` field, so this is the one
    address recorded here.
    """
    symbol = info.get("symbol") or ""
    name = info.get("name") or ""
    platform = info.get("platform") or {}
    platform_name = platform.get("name") or ""
    contract_address = platform.get("token_address") or ""
    return symbol, name, platform_name, contract_address


def iter_stories(data_dir: Path) -> Iterable[Story]:
    daily_dir = data_dir / "memes" / "daily"
    info_dir = data_dir / "memes" / "info"
    for daily_path in sorted(daily_dir.glob("*.json.gz")):
        coin_id = daily_path.name[: -len(".json.gz")]
        info_path = info_dir / f"{coin_id}.json.gz"
        symbol = name = platform_name = contract_address = ""
        if info_path.exists():
            try:
                info = _read_json_gz(info_path)
                symbol, name, platform_name, contract_address = _info_fields(info)
            except (OSError, json.JSONDecodeError):
                pass
        try:
            daily = _read_json_gz(daily_path)
            raw_quotes = daily.get("quotes", [])
        except (OSError, json.JSONDecodeError):
            raw_quotes = []
        yield build_story(coin_id, symbol, name, platform_name, contract_address, raw_quotes)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("data_dir", type=Path, help="directory holding memes/daily and memes/info")
    parser.add_argument("--out", type=Path, required=True, help="CSV file to write, one row per coin")
    args = parser.parse_args(argv)

    counts = {label: 0 for label in LABELS}
    column_names = [f.name for f in fields(Story)]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("w", newline="", encoding="utf-8") as fh:
        writer = csv.writer(fh)
        writer.writerow(column_names)
        for story in iter_stories(args.data_dir):
            counts[story.label] = counts.get(story.label, 0) + 1
            writer.writerow([getattr(story, c) for c in column_names])

    print(f"wrote {args.out}", file=sys.stderr)
    for label in LABELS:
        print(f"{label}: {counts[label]}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
