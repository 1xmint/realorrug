# Progress — M-D-0001 (assessment.rs, episode-counted level)

DONE: Set up worktree/branch `assessment-packet` off `origin/main`. Read the
plan (plan-slice-7 packet M-D-0001), ADR 0032, and design 0027's Judgement
section. Read `verdict.rs` (full ladder, `Level`, `LIVE_RISK_SIGNALS`,
`level()`, existing `sheet_with`/`the_live_robinhood_sheet` test fixtures)
and `sheet.rs`'s `FactSheet` struct and `FactSheet::build` (the
capacity/fees/creator-transactions/market/token-ownership skip list at
sheet.rs:679-684). No code changes made yet — verified the stop condition
below before writing anything, per the packet's own instruction not to work
around it.

NEXT: Blocked — see the question below. Whoever picks this up next should
get an owner answer on the accessor shape, then: add the agreed accessor to
`sheet.rs` (out of scope for M-D-0001 itself; needs its own unit or an
amendment), then write `assessment.rs` per the plan (Finding/Group/Episode,
`episode()` total match, `risk_index` summed over distinct episodes,
`Coverage`, `critical_gaps`, `level`/`admissible`), re-point the two named
`verdict.rs` tests at the episode-counted `level`, and rewrite the
`verdict.rs` module doc (lines 11-24, drop the `theradar:GOAL.md` citation).

WATCH OUT: `FactSheet` (sheet.rs:347-385) has no field that survives the
capacity/fees/creator-transactions/market/token-ownership skip -- those
misses are matched and `continue`d inside `FactSheet::build` (sheet.rs:679-
684) before ever reaching `unknown`, `facts`, or any other field, and
`Dossier::unavailable` itself is not carried on `FactSheet`. So
`coverage.applicable`'s "non-degrading gaps" term cannot be computed from a
`&FactSheet` alone today -- this is the plan's own named stop condition
("if the non-degrading gaps are not recoverable from FactSheet without
touching sheet.rs") and its own "Not verified" item. `sheet.rs` is forbidden
for M-D-0001. Do not add a workaround inside the roast crate that
re-derives these from something else; get the owner's answer on the
`pub fn gaps(&self)`-shaped accessor first.
