# PROGRESS: finish PR #100 (robinhood-counts-its-own-population)

Done:
- Fixed the `needless_pass_by_value` clippy failure in `crates/realorrug-robinhood/src/lib.rs` (`logs_range_filter` takes `&serde_json::Value`).
- Replaced the O(launch-count) per-block RPC timestamp design with `BlockTimeModel` in `crates/realorrug-cli/src/creator_index.rs`: reads the head plus up to `TIME_SAMPLES` (32) evenly-spaced exact timestamps, then interpolates/extrapolates linearly. Rebuilt `run()`'s `--to`/watermark ordering so the backwards-range check still fires before any RPC call (regression caught and fixed against `a_range_that_runs_backwards_is_refused_before_anything_is_written`).
- Extracted and unit-tested pure predicates to close CI's mutation-testing gaps: `repeats_a_token_or_curve`, `invalid_graduation`, `is_curve_buy`, `trade_precedes_launch`; added a `run()` test for the `base_out` collision check; added `base_rates_json` tests for the both-empty guard, an isolated `reached_5x` boundary, and the `measured_on` value.
- Rewrote `deploy/README.md`'s RPC-estimate section for the new bounded (`1 + TIME_SAMPLES + ...`) design, replacing the old up-to-531,581-launch-block-reads warning.
- Confirmed `crates/realorrug-roast/src/baserates.rs`'s `outcomes_24h: Option<OutcomeRates>` is `#[serde(default)]`, so old snapshots (no `outcomes_24h` key) still parse.

Next:
- Push this branch, then `gh pr checks 100 --watch --interval 60` (never run cargo/tests locally per the packet). Fix anything CI still reports.
- If green, `gh pr ready 100` (never merge).

Watch out for:
- Do not push again while a CI run is in flight.
- No RPC mock exists in `creator_index.rs`'s test module; walk functions (`walk_token_launched`, `walk_graduated`, `walk_curve_trades`) are still only exercised indirectly through their extracted pure predicates, not directly.
- `crates/realorrug-cli/src/creator_index.rs` has never compiled locally (cargo is disallowed here); the first CI `build`/`tests` run is the first real compile check.
