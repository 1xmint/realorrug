DONE: sheet.rs push_creator_cash_flow wired into FactSheet::build (proceeds,
signed net, transfers-out count; Kind::CreatorCashFlow added in clause.rs and
mapped to Subject::Creator in fidelity.rs, no salience candidate); 7 new
tests in sheet.rs all pass individually (cargo test -p realorrug-roast --lib);
civil_from_days doc fixed (u64, not i64); design doc 0027 "Slice 5 as built"
note added. Committed a3884e5 and efb551d on branch creator-cash-flow, pushed
to origin/creator-cash-flow. PR opened against main:
https://github.com/1xmint/realorrug/pull/116 (not merged).

NEXT: nothing outstanding for this task. If CI flags anything, check
`gh pr checks` on the PR and fix as its own commit.

WATCH OUT FOR: render_quote only prints 4 decimal digits (integer div by
scale/10000), so tiny wei amounts round to "0.0000" -- tests use wei values
>= 1e14 to get a visible fraction. The net fact's label/clauses deliberately
never say "profit" (AGENTS.md rule 4 plus the packet's own instruction);
don't reintroduce the word if editing that fact's text.
