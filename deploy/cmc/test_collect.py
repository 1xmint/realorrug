#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Offline unit tests for collect.py. No network access; urlopen is mocked.

Run with:
    python -m unittest deploy/cmc/test_collect.py
or from inside deploy/cmc:
    python -m unittest test_collect
"""
from __future__ import annotations

import gzip
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

# Allow running as `python -m unittest deploy/cmc/test_collect.py` from the
# repo root (this file's own directory isn't on sys.path in that form) as
# well as from inside deploy/cmc.
sys.path.insert(0, str(Path(__file__).resolve().parent))

import collect


class FakeResponse:
    def __init__(self, payload: dict):
        self._body = json.dumps(payload).encode("utf-8")

    def read(self) -> bytes:
        return self._body

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


def ok_payload(data, credit_count=1, extra_status=None):
    status = {"credit_count": credit_count, "error_code": 0, "error_message": None}
    if extra_status:
        status.update(extra_status)
    return {"status": status, "data": data}


def error_payload(error_code, error_message="error", credit_count=0):
    return {
        "status": {
            "credit_count": credit_count,
            "error_code": error_code,
            "error_message": error_message,
        },
        "data": None,
    }


def make_client(responses, budget=None):
    """responses: list of dicts to return in order, or a callable(url)->dict."""
    calls = []

    def opener(request, timeout=30):
        calls.append(request.full_url)
        if callable(responses):
            payload = responses(request.full_url)
        else:
            payload = responses[len(calls) - 1]
        return FakeResponse(payload)

    budget = budget or collect.CreditBudget(max_credits=collect.DEFAULT_MAX_CREDITS)
    rate_limiter = collect.RateLimiter()
    client = collect.CmcClient("test-secret-key", budget, rate_limiter, opener=opener)
    return client, calls, budget


class TestKeyScrubbing(unittest.TestCase):
    def test_scrub_removes_key_from_text(self):
        key = "sk-super-secret-123"
        text = f"request failed for https://pro-api.coinmarketcap.com/x?key={key}"
        scrubbed = collect._scrub(text, key)
        self.assertNotIn(key, scrubbed)
        self.assertIn("REDACTED", scrubbed)

    def test_scrub_noop_without_key(self):
        text = "no secrets here"
        self.assertEqual(collect._scrub(text, None), text)

    def test_http_error_message_never_contains_key(self):
        import urllib.error

        key = "sk-super-secret-456"

        def opener(request, timeout=30):
            raise urllib.error.HTTPError(request.full_url, 500, "boom", {}, None)

        budget = collect.CreditBudget(max_credits=1000)
        client = collect.CmcClient(key, budget, collect.RateLimiter(), opener=opener)
        with self.assertRaises(collect.CmcApiError) as ctx:
            client.get("/v1/test", {"key_leak_attempt": key})
        self.assertNotIn(key, str(ctx.exception))

    def test_load_api_key_error_never_contains_path_secret(self):
        # No env var, no file present at the real path in this sandbox: the
        # SystemExit message should name neither a key nor a stray value.
        with mock.patch.dict("os.environ", {}, clear=True):
            with mock.patch("collect.Path.read_text", side_effect=OSError("nope")):
                with self.assertRaises(SystemExit) as ctx:
                    collect.load_api_key()
                self.assertNotIn("REALORRUG_CMC_API_KEY=", str(ctx.exception).split(":")[-1])


class TestCategoryPaging(unittest.TestCase):
    def test_pages_until_short_page(self):
        page1 = {"coins": [{"id": i} for i in range(1, 4)]}
        page2 = {"coins": [{"id": 4}]}
        responses = [ok_payload(page1), ok_payload(page2)]
        client, calls, _ = make_client(responses)
        coins = list(collect.iter_category_coins(client, "cat1", page_limit=3))
        self.assertEqual([c["id"] for c in coins], [1, 2, 3, 4])
        self.assertEqual(len(calls), 2)
        self.assertIn("start=1", calls[0])
        self.assertIn("start=4", calls[1])

    def test_empty_first_page_stops(self):
        client, calls, _ = make_client([ok_payload({"coins": []})])
        coins = list(collect.iter_category_coins(client, "cat1", page_limit=3))
        self.assertEqual(coins, [])
        self.assertEqual(len(calls), 1)


class TestHistoryShapes(unittest.TestCase):
    def test_flat_shape_with_quotes_key(self):
        data = {"id": 1, "name": "X", "symbol": "X", "quotes": [{"time_open": "t"}]}
        self.assertEqual(collect._extract_quotes(data, "1"), [{"time_open": "t"}])

    def test_keyed_by_id_shape(self):
        data = {"1": {"id": 1, "quotes": [{"time_open": "a"}, {"time_open": "b"}]}}
        self.assertEqual(len(collect._extract_quotes(data, "1")), 2)

    def test_inactive_coin_empty_quotes(self):
        data = {"id": 1, "quotes": []}
        self.assertEqual(collect._extract_quotes(data, "1"), [])

    def test_none_data(self):
        self.assertEqual(collect._extract_quotes(None, "1"), [])


class TestResumeSkipsExisting(unittest.TestCase):
    def test_daily_history_skips_file_already_on_disk(self):
        with tempfile.TemporaryDirectory() as tmp:
            data_dir = Path(tmp)
            daily_dir = data_dir / "memes" / "daily"
            info_dir = data_dir / "memes" / "info"
            daily_dir.mkdir(parents=True)
            info_dir.mkdir(parents=True)
            collect.write_json_gz(daily_dir / "1.json.gz", {"id": "1", "quotes": []})
            collect.write_json_gz(info_dir / "1.json.gz", {"id": 1})

            listing = [{"id": 1, "quote": {"USD": {"market_cap": 100}}}]
            listing_path = data_dir / "memes" / "listing-20260922.json.gz"
            collect.write_json_gz(listing_path, listing)

            def opener(request, timeout=30):
                self.fail(f"should not call the API for id=1, but called {request.full_url}")

            budget = collect.CreditBudget(max_credits=1000)
            client = collect.CmcClient("k", budget, collect.RateLimiter(), opener=opener)

            args = _namespace(
                data=str(data_dir), category=collect.MEMES_CATEGORY_ID, dry_run=False, resume=True
            )
            # Force the resume path to reuse the listing we already wrote,
            # and cause 2026-09-22's listing filename to match by patching time.
            with mock.patch("collect.time.strftime", return_value="20260922"):
                stats = collect.cmd_memes(args, client, "k")
            self.assertEqual(stats.errors, 0)


class TestCreditCapStops(unittest.TestCase):
    def test_budget_raises_once_over_cap(self):
        budget = collect.CreditBudget(max_credits=50)
        budget.add(30)
        self.assertEqual(budget.used, 30)
        with self.assertRaises(collect.OutOfCreditBudget):
            budget.add(30)
        self.assertEqual(budget.used, 60)

    def test_client_stops_run_when_cap_reached(self):
        pages = [ok_payload({"coins": [{"id": 1}]}, credit_count=40)] * 5
        budget = collect.CreditBudget(max_credits=50)
        client, calls, _ = make_client(pages, budget=budget)
        with self.assertRaises(collect.OutOfCreditBudget):
            list(collect.iter_category_coins(client, "cat1", page_limit=1))
        # Only two calls should have happened before the cap tripped (40, then 80 >= 50).
        self.assertEqual(len(calls), 2)


class Test1014NotRetried(unittest.TestCase):
    def test_1014_recorded_and_not_retried(self):
        payloads = [error_payload(collect.OUT_OF_WINDOW_ERROR_CODE, "exceeds plan's historical data access limit")]
        client, calls, _ = make_client(payloads)
        with self.assertRaises(collect.Cmc1014Error):
            client.get("/v1/k-line/candles", {"address": "mint", "from": 0, "to": 1})
        self.assertEqual(len(calls), 1)  # not retried

    def test_string_status_codes_are_read_as_numbers(self):
        # The DEX and k-line endpoints send "0" and "1014" as strings (seen
        # live 2026-09-22); a string "0" once read as an error.
        ok = {"status": {"error_code": "0", "credit_count": "1"}, "data": []}
        client, _, _ = make_client([ok])
        self.assertEqual(client.get("/v4/dex/spot-pairs/latest", {}), ok)
        late = {"status": {"error_code": "1014", "error_message": "exceeds", "credit_count": 0}}
        client, calls, _ = make_client([late])
        with self.assertRaises(collect.Cmc1014Error):
            client.get("/v1/k-line/candles", {"address": "mint", "from": 0, "to": 1})
        self.assertEqual(len(calls), 1)

    def test_fetch_candles_records_out_of_window_and_returns_partial(self):
        # First chunk succeeds, second chunk hits 1014.
        seq = [[[1, 2, 3, 4, 5, 1000, 1]]]

        def responder(url):
            if not seq:
                return error_payload(collect.OUT_OF_WINDOW_ERROR_CODE)
            return ok_payload(seq.pop(0), credit_count=1)

        client, calls, _ = make_client(responder)
        candles, out_of_window = collect.fetch_candles(
            client, "mint", 0, collect._INTERVAL_MS["1min"] * collect.CANDLE_LIMIT * 3, "1min"
        )
        self.assertTrue(out_of_window)
        self.assertEqual(len(candles), 1)


class TestKeyNeverInLogsOrErrors(unittest.TestCase):
    def test_generic_cmc_api_error_scrubbed(self):
        payloads = [error_payload(9999, "some odd error")]
        client, calls, _ = make_client(payloads)
        with self.assertRaises(collect.CmcApiError) as ctx:
            client.get("/v1/test", {})
        self.assertNotIn("test-secret-key", str(ctx.exception))


class TestCandleChunkPaging(unittest.TestCase):
    def test_pages_across_ms_window(self):
        # limit=2 candles per chunk, 1min interval => span 120000ms per chunk.
        collect_limit = 2
        from_ms = 0
        to_ms = collect._INTERVAL_MS["1min"] * collect_limit * 3  # 3 chunks worth

        seen_ranges = []

        def responder(url):
            qs = url.split("?", 1)[1]
            params = dict(p.split("=") for p in qs.split("&"))
            seen_ranges.append((int(params["from"]), int(params["to"])))
            return ok_payload([[1, 2, 3, 4, 5, int(params["from"]), 1]])

        client, calls, _ = make_client(responder)
        candles, out_of_window = collect.fetch_candles(
            client, "mint", from_ms, to_ms, "1min", limit=collect_limit
        )
        self.assertFalse(out_of_window)
        self.assertEqual(len(seen_ranges), 3)
        self.assertEqual(seen_ranges[0][0], 0)
        self.assertEqual(seen_ranges[-1][1], to_ms)
        # Each chunk's span should be interval_ms * limit apart.
        span = collect._INTERVAL_MS["1min"] * collect_limit
        for i in range(len(seen_ranges) - 1):
            self.assertEqual(seen_ranges[i][1], seen_ranges[i][0] + span)


class TestAtomicWrite(unittest.TestCase):
    def test_write_then_read_roundtrip(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sub" / "x.json.gz"
            collect.write_json_gz(path, {"a": 1})
            self.assertTrue(path.exists())
            self.assertFalse(path.with_suffix(path.suffix + ".tmp").exists())
            data = json.loads(gzip.decompress(path.read_bytes()))
            self.assertEqual(data, {"a": 1})


def _namespace(**kwargs):
    class NS:
        pass

    ns = NS()
    for k, v in kwargs.items():
        setattr(ns, k, v)
    return ns


if __name__ == "__main__":
    unittest.main()
