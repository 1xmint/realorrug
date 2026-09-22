#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Offline unit tests for stories.py. No files, no network -- every case
builds its own list of raw CMC-shaped quote dicts."""
from __future__ import annotations

import unittest

from stories import COLLAPSE_WINDOW_DAYS, MIN_QUOTES_FOR_STORY, SUSTAINED_MIN_HISTORY_DAYS, build_story


def make_quote(day: str, close: float, market_cap: float | None = None, volume: float = 100.0) -> dict:
    """One raw quote in the exact shape collect.py saves under memes/daily."""
    usd: dict = {"close": close, "volume": volume, "timestamp": f"{day}T23:59:59.999Z"}
    if market_cap is not None:
        usd["market_cap"] = market_cap
    return {"quote": {"USD": usd}}


def daily_series(start_year: int, start_month: int, start_day: int, values: list[float], **kw) -> list[dict]:
    """Build consecutive daily quotes starting at the given date."""
    from datetime import date, timedelta

    start = date(start_year, start_month, start_day)
    return [make_quote((start + timedelta(days=i)).isoformat(), v, **kw) for i, v in enumerate(values)]


class BuildStoryTests(unittest.TestCase):
    def test_no_data_on_empty_series(self) -> None:
        story = build_story("1", "AAA", "Coin A", "", "", [])
        self.assertEqual(story.label, "no_data")
        self.assertEqual(story.quote_count, 0)
        self.assertIsNone(story.peak_value)

    def test_too_short_under_minimum_quotes(self) -> None:
        values = [1.0] * (MIN_QUOTES_FOR_STORY - 1)
        quotes = daily_series(2026, 1, 1, values)
        story = build_story("2", "BBB", "Coin B", "", "", quotes)
        self.assertEqual(story.label, "too_short")
        self.assertEqual(story.quote_count, MIN_QUOTES_FOR_STORY - 1)

    def test_collapsed_within_window(self) -> None:
        # Rises to a peak of 100 on day 40, then falls under 10% of it
        # (i.e. under 10) by day 40 + 10 -- well inside the 90-day window.
        rise = [1.0 + i for i in range(40)]  # ends at 40
        rise[-1] = 100.0
        fall = [1.0] * 40  # far under the 10.0 collapse threshold
        values = rise + fall
        quotes = daily_series(2026, 1, 1, values)
        story = build_story("3", "CCC", "Coin C", "", "", quotes)
        self.assertEqual(story.label, "collapsed")
        self.assertIsNotNone(story.days_peak_to_collapse)
        self.assertLessEqual(story.days_peak_to_collapse, COLLAPSE_WINDOW_DAYS)

    def test_sustained_needs_long_history_and_held_value(self) -> None:
        # Peaks early at 100, then sits at 60 (60% of peak) for a long time.
        values = [100.0] + [60.0] * (SUSTAINED_MIN_HISTORY_DAYS + 10)
        quotes = daily_series(2025, 1, 1, values)
        story = build_story("4", "DDD", "Coin D", "", "", quotes)
        self.assertEqual(story.label, "sustained")

    def test_faded_when_history_too_short_for_sustained(self) -> None:
        # Same shape as the sustained case (held 60% of peak) but history
        # is short, so it can't be called "sustained" yet -- it's "faded".
        values = [100.0] + [60.0] * (MIN_QUOTES_FOR_STORY + 5)
        quotes = daily_series(2026, 1, 1, values)
        story = build_story("5", "EEE", "Coin E", "", "", quotes)
        self.assertEqual(story.label, "faded")

    def test_still_at_peak_with_long_history_is_sustained(self) -> None:
        values = [100.0] * (SUSTAINED_MIN_HISTORY_DAYS + 1)
        quotes = daily_series(2025, 1, 1, values)
        story = build_story("6", "FFF", "Coin F", "", "", quotes)
        self.assertEqual(story.label, "sustained")
        self.assertEqual(story.drawdown_from_peak, 0.0)

    def test_missing_market_cap_falls_back_to_price(self) -> None:
        values = [1.0 + i for i in range(MIN_QUOTES_FOR_STORY + 5)]
        quotes = daily_series(2026, 1, 1, values)  # no market_cap key at all
        story = build_story("7", "GGG", "Coin G", "", "", quotes)
        self.assertEqual(story.metric, "price")

    def test_all_zero_market_cap_falls_back_to_price(self) -> None:
        values = [1.0 + i for i in range(MIN_QUOTES_FOR_STORY + 5)]
        quotes = daily_series(2026, 1, 1, values, market_cap=0)
        story = build_story("8", "HHH", "Coin H", "", "", quotes)
        self.assertEqual(story.metric, "price")

    def test_real_market_cap_used_when_present(self) -> None:
        values = [1.0 + i for i in range(MIN_QUOTES_FOR_STORY + 5)]
        quotes = daily_series(2026, 1, 1, values, market_cap=1000.0)
        story = build_story("9", "III", "Coin I", "", "", quotes)
        self.assertEqual(story.metric, "market_cap")
        self.assertEqual(story.peak_value, 1000.0)

    def test_median_volume_over_last_30_days(self) -> None:
        # 40 days of volume 10, then last 30 days are volume 50.
        quotes = []
        from datetime import date, timedelta

        start = date(2026, 1, 1)
        for i in range(40):
            quotes.append(make_quote((start + timedelta(days=i)).isoformat(), 1.0, volume=10.0))
        for i in range(40, 70):
            quotes.append(make_quote((start + timedelta(days=i)).isoformat(), 1.0, volume=50.0))
        story = build_story("10", "JJJ", "Coin J", "", "", quotes)
        self.assertEqual(story.latest_30d_median_volume, 50.0)

    def test_unsorted_quotes_are_sorted_by_date(self) -> None:
        values = [1.0 + i for i in range(MIN_QUOTES_FOR_STORY)]
        quotes = daily_series(2026, 1, 1, values)
        story_in_order = build_story("11", "KKK", "Coin K", "", "", quotes)
        story_reversed = build_story("11", "KKK", "Coin K", "", "", list(reversed(quotes)))
        self.assertEqual(story_in_order.peak_date, story_reversed.peak_date)
        self.assertEqual(story_in_order.first_date, story_reversed.first_date)

    def test_quote_missing_usd_block_is_dropped(self) -> None:
        good = daily_series(2026, 1, 1, [1.0 + i for i in range(MIN_QUOTES_FOR_STORY)])
        bad = [{"quote": {}}]
        story = build_story("12", "LLL", "Coin L", "", "", good + bad)
        self.assertEqual(story.quote_count, MIN_QUOTES_FOR_STORY)


if __name__ == "__main__":
    unittest.main()
