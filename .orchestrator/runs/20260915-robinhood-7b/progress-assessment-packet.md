# M-D-0001 progress (branch assessment-packet)

## DONE
- Coordinator's `skipped: Vec<String>` amendment fully wired: field on
  `FactSheet` (sheet.rs), filled in `build()` at the non-degrading-miss
  `continue`, `Self` literal updated, pinning test
  `a_skipped_optional_fact_lands_in_skipped_and_not_in_unknown` added, and
  every other `FactSheet { .. }` literal the compiler would flag fixed
  (clause.rs, forbidden.rs x2, salience.rs, verdict.rs x3, voice.rs base
  `sheet()`).
- New `crates/realorrug-roast/src/assessment.rs`: `Group`, `Episode`,
  `episode()`, `group()`, `Finding`, `Coverage`, `Assessment` (+ `from`,
  weights, `milder`/`admissible`), and all 9 named tests from the plan.
  Wired into `lib.rs` (`pub mod assessment; pub use assessment::Assessment;`).
- `verdict.rs` `level()` rewritten to count distinct episodes (not raw
  signal count) for the `RugMechanicsLive` `>= 2` threshold; module doc
  rewritten per ADR 0032 (drops the old theradar:GOAL.md citation).
  `mod tests` and `the_live_robinhood_sheet()` made `pub(crate)` so
  assessment.rs's tests can reuse the fixture.
- All of the above staged (not yet committed) on `assessment-packet`.

## NEXT (blocked, stop-and-ask triggered)
Found that `verdict.rs`'s existing test
`the_template_states_a_twin_at_rug_mechanics_live_and_none_at_rugged` builds
its `RugMechanicsLive` fixture from `CreatorBoughtOwnLaunch` +
`LaunchBlockInStrongestBand` -- both map to `Episode::LaunchBlock` under the
new `episode()` function, i.e. the exact same pair the plan names as the
positive example of the new behaviour (one episode -> `Sketchy`, not
`RugMechanicsLive`). This test is not one of the plan's two named tests
(`two_live_signals_in_one_episode_stay_sketchy`,
`two_live_risk_signals_reach_rug_mechanics_live`), so its flip trips the
plan's own stop condition ("if any existing verdict test other than the two
named flips"). Reported to the coordinator; awaiting an answer on whether to
re-point this test's fixture to a genuine cross-episode `RugMechanicsLive`
pair (e.g. `CreatorBoughtOwnLaunch` + `HolderConcentration`) as part of this
same unit, or something else.

Once answered: add the two named verdict.rs tests, resolve this third test,
audit the remaining verdict.rs tests (`a_single_signal_never_reaches...`,
`two_live_risk_signals_reach_rug_mechanics_live` -- already cross-episode as
written: `CreatorBoughtOwnLaunch`+`LaunchBlockInStrongestBand`+
`HolderConcentration` = 2 distinct episodes {LaunchBlock, Holders}, likely
fine as-is or trivially re-pointed, `a_missing_required_fact_forces_cant_tell...`,
`an_observed_rugged_pair_beats_an_unrelated_missing_fact`,
`nothing_ugly_yet_is_unreachable_when_anything_is_unknown`,
`no_signal_and_nothing_unknown_is_nothing_ugly_yet`) for other flips, add the
design-0027 as-built note, run cargo check/clippy/fmt and the named tests
individually, commit, push, open the PR.

## WATCH OUT
- Do not silently re-point the flipping test without an answer -- that is
  exactly the behaviour the plan's stop condition exists to catch.
- sheet.rs diff must stay minimal; another worker (push_creator_cash_flow)
  is also editing sheet.rs on another branch.
- Never whole-crate `cargo test`; only single named tests.
