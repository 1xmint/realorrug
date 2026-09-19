# M-D-0001 progress (branch assessment-packet / assessment-finish)

## DONE
- Coordinator's ruling applied: `the_template_states_a_twin_at_rug_mechanics_live_and_none_at_rugged`
  re-pointed to a genuine cross-episode pair (`CreatorBoughtOwnLaunch` +
  `HolderConcentration`); twin assertions unchanged.
- Added `two_live_signals_in_one_episode_stay_sketchy` (LaunchBlock pair ->
  Sketchy); re-pointed `two_live_risk_signals_reach_rug_mechanics_live` at a
  cross-episode pair (`CreatorBoughtOwnLaunch` + `HolderConcentration`).
- Audited every other verdict.rs test named in the prior progress note --
  none of them flip; only the one test above and the two new/re-pointed
  ones changed expected values.
- Fixed 8 missing-docs clippy warnings assessment.rs introduced (enum
  variant docs on `Group`, struct field docs on `Finding`) and 2
  cast-possible-truncation warnings in its tests (`u8::try_from(...).unwrap()`).
- `cargo check -p realorrug-roast`, `clippy -p realorrug-roast --tests -- -D
  warnings`, and `fmt -p realorrug-roast` all clean.
- Ran every named test individually (not whole-crate): the 9 verdict.rs
  tests audited above, all `assessment::` tests (9, via one filter) -- all
  pass.
- risk_index of `the_live_robinhood_sheet()` fixture = 25 (only
  `CreatorBoughtOwnLaunch` fires on it -> one `LaunchBlock` episode ->
  `WEIGHT_LAUNCH_BLOCK`), computed with a throwaway test then reverted (not
  committed).
- Added the slice 7 unit 1 as-built note to
  `docs/design/0027-the-three-layers.md` (states the one published
  behaviour change and the moved test, plainly).
- Committed as `b5c2493` on `assessment-finish` (tracks `assessment-packet`
  upstream). Pushed to `origin/assessment-packet`.
- Opened PR (see command output / return) against `main`, titled "Count
  each risky moment once, and keep gaps visible beside coverage", marked
  held for owner approval, not merged.

## NEXT
- None for M-D-0001 -- unit complete. Follow-on units (M-D-0002 voice.rs
  shadow band, M-D-0003 analyst/serve packet carry, M-D-0004 independent
  review) are separate plan units, not part of this task.

## WATCH OUT
- Never whole-crate `cargo test -p`; only single named tests or one
  `assessment::`/`verdict::`-style filter, as done here.
- PR is intentionally NOT merged -- it changes one published behaviour
  (two live signals from the same launch-block episode now stay Sketchy
  instead of RugMechanicsLive) and needs the owner's sign-off first.
