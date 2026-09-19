done: Round 2 of CI mutant fixes on `daily-five-docs`, committed on top of 279e807 (not
pushed). Killed every MISSED/TIMEOUT mutant the coordinator listed in calls.rs and
daily.rs, all with real tests except one true-equivalent case (documented with a
one-line comment, not a test, not #[mutants::skip]):
- z_hundredths: z_hundredths_zero_variance_is_none (348:17 ==/!=), z_hundredths_exact_values
  (352:69 //%//*, 354:25 delete `-`); the 354:19 </<= mutant is a true equivalent
  (magnitude is 0 at total==0 regardless) and is documented inline, not tested.
- score_calls above_line ordering (440:55/441:55 */+): extracted the inline sort
  closure into a standalone `above_line_order(a, b)` function so it's independently
  testable, and added above_line_order_uses_squares_not_sums (A: total=10,var=5 vs
  B: total=3,var=1 -- flips order between correct squaring and the `+`-mutant).
- eligibility boundaries (465:28, 471:16, 476:23): eligibility_call_count_boundary
  (19 vs 20), eligibility_creator_count_boundary (9 vs 10), eligibility_account_age_boundary
  (min-1 vs min), via a new boundary_calls() test helper.
- variance_is_an_exact_sum: pins the exact sum for 20 calls at q=2000 (320,000,000).
- DummyStrategy::AgreeWithBot (555:42/555:57): agree_with_bot_boundary at q=4999
  (Real) vs q=5000 (Rug).
- fnv1a64 (583:13, 584:14, 586:11 TIMEOUT): fnv1a64_matches_published_vectors, using
  the published "", "a", "foobar" test vectors -- the short non-empty inputs catch
  the += -> *= runaway-loop mutant fast.
- daily::pick day window (156:31 </<=): day_start_is_inclusive_day_end_is_exclusive.
clippy -p realorrug-contest --all-targets -- -D warnings: clean (`Finished` line, zero
warnings). fmt applied. All 10 new tests run individually by name (filter, not `cargo
test` unscoped): all ok.

next: nothing pending on this task; report the new commit hash, the clippy line, and
the test result lines to the coordinator. Do not push (coordinator pushes).

watch out for: this machine only runs check/clippy/fmt/a named-test filter -- never
`cargo test -p realorrug-contest` unscoped and never `cargo mutants` locally. Always
check `Get-Process cargo,rustc` before every cargo invocation. Branch is
`daily-five-docs`; do not commit to main. The above_line_order extraction is a pure
refactor (same sort order, now a named fn) -- no docs/design/0028 update was needed
since described behavior didn't change.
