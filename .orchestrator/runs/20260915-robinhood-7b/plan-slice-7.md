# Plan: slice 7 — assessment and judgement boundary (design 0027 §"Judgement", ADR 0032)

Base: `main` at `deaa63f` (PR #115). Slice 5 (`origin/creator-cash-flow`, 3 commits so far) is in
flight and touches `Dossier`; nothing here depends on its names. Owner has NOT approved the model
choosing a band: code alone sets the published level; the model's suggestion is logged, never used.

## The packet (one type, built once per sheet, shared by analyst and site)

`realorrug_roast::assessment::Assessment::from(&FactSheet)` — pure, deterministic, `Serialize`:

- `findings: Vec<Finding { signal: Signal, group: Group, episode: Episode }>` — one per fired signal.
  `Group` = LaunchStructure | Ownership | CreatorActivity | ExitMechanics | TradingBehaviour.
  `Episode` (the `causal_episode_id`) = LaunchBlock | CreatorHistory | Exit | Holders | Authority:
  `LaunchBlockInStrongestBand`+`CreatorBoughtOwnLaunch` -> LaunchBlock (one launch-block read);
  `RepeatLauncher`+`CreatorNeverGraduatedOrganically` -> CreatorHistory; `LiquidityGone`+
  `CreatorSoldOut`+`BuyersCannotSell` -> Exit; `HolderConcentration` -> Holders;
  `OwnerCanStillMintOrPause` -> Authority. A `const fn episode(Signal) -> Episode` match, no wildcard.
- `risk_index: u8` (0–100, "not a probability") = sum over **distinct episodes** of the episode's
  hand-set weight (doc-commented one line each, ADR 0032 d5/d6 scoping noted), saturating at 100.
  Two signals in one episode add once — this is "correlated flags count once".
- `coverage: Coverage { read: usize, applicable: usize }` = `facts.len()` over `facts.len() + gaps`,
  where gaps = `sheet.unknown` plus the non-degrading gaps `FactSheet::build` skips (capacity, fees,
  market, ownership). Never touches `risk_index` (ADR 0032 d2). Rendered as a fraction, not a percent.
- `critical_gaps: Vec<String>` = `sheet.unknown` verbatim — the gaps that force `CantTell` today.
  Reported beside coverage, never folded into it: "critical gaps survive coverage".
- `level: Level` = `verdict::level(sheet)` (code-owned, unchanged rule shape) and
  `admissible: Vec<Level>` = the set a model *would* choose from: `[level]` plus the one adjacent
  milder band for `RugMechanicsLive`/`Sketchy`, `[level]` alone for `Rugged`/`CantTell`/`NothingUglyYet`.
  Shadow only: nothing in this slice reads `admissible` to publish anything.

`verdict::level` changes in exactly one place: `live_risk_count` counts **distinct episodes** among
`LIVE_RISK_SIGNALS`, not signals. Everything else in the ladder stays. This is the one published
behaviour change in the slice and gets an independent reviewer.

## Units

### M-D-0001 — roast: `assessment.rs`, episode-counted level, verdict doc rewrite (implementer; blocks nothing; A)
Files: `crates/realorrug-roast/src/assessment.rs` (new), `lib.rs` (`pub mod assessment; pub use
assessment::Assessment`), `verdict.rs` (module doc lines 11–24 rewritten per ADR 0032 — drop the
`theradar:GOAL.md` citation; `level` counts episodes), `docs/design/0027-the-three-layers.md` (as-built
note under slice 7, same commit). Forbidden: `voice.rs`, `sheet.rs`, anything outside the roast crate
except the design note.
Done (tests in `assessment.rs` unless named):
- `episode_map_covers_every_signal`: iterate every `Signal` variant; `episode()` is total (no wildcard arm).
- `two_signals_in_one_episode_score_once`: `sheet_with(&[LaunchBlockInStrongestBand, CreatorBoughtOwnLaunch], &[])`
  -> `risk_index == weight(LaunchBlock)`, `findings.len() == 2`, distinct episodes == 1.
- `two_episodes_add`: `sheet_with(&[LaunchBlockInStrongestBand, RepeatLauncher], &[])` -> index is the sum.
- `index_saturates_at_100`: all nine signals -> `risk_index == 100` (pins the `min`).
- `verdict.rs`: `two_live_signals_in_one_episode_stay_sketchy` (LaunchBlock pair -> `Sketchy`) and the existing
  `two_live_risk_signals_reach_rug_mechanics_live` re-pointed at a cross-episode pair. Both pin the `>= 2`.
- `unread_fact_lowers_coverage_not_index`: `sheet_with(&[RepeatLauncher], &["holders"])` -> index equal to
  the no-gap sheet, `coverage.applicable == coverage.read + 1`.
- `critical_gap_survives_high_coverage`: `the_live_robinhood_sheet()` with one unknown injected -> `critical_gaps`
  names it, `level == CantTell`, `coverage.read > coverage.applicable / 2`.
- `admissible_always_contains_level` over `EVERY_LEVEL`-style table; `rugged_and_cant_tell_admit_nothing_else`.
- `packet_json_field_names_are_pinned`: serialise and assert the exact key list (the shared-packet contract).
Mutant hazards: every weight constant needs a test whose expected value would change if the weight did
(assert exact sums, not `> 0`); `saturating_add`/`min(100)` needs the nine-signal fixture; the
`admissible` match needs one test per arm. Do not add a coverage floor or score bands — with four live
signals they are unreachable (ADR 0032 d7) and would fail the mutant gate.
Verify: `REALORRUG_CARGO=... cargo +stable-x86_64-pc-windows-gnullvm clippy -p realorrug-roast --all-targets`
clean locally; PR CI green including `mutants` (all four shards). Rubric: zero missed mutants in
`assessment.rs`/`verdict.rs`; `risk_index` for `the_live_robinhood_sheet()` recorded in the test name.
Stop and ask: if the non-degrading gaps are not recoverable from `FactSheet` without changing `sheet.rs`
(then propose a `pub fn gaps(&self)` accessor and wait); if any existing verdict test other than the two
named flips; if `Level` needs `Ord`.

### M-D-0002 — voice.rs: shadow band request and capture (implementer; blocks on 0001; B, parallel with C)
Files: `crates/realorrug-roast/src/voice.rs` only. Forbidden: `verdict.rs`, `assessment.rs`, `fidelity.rs`.
Change: `verdict_brief` gains one digit-free paragraph naming the admissible band names and asking for an
optional final line `BAND: <name>`; `write` splits a trailing `BAND:` line off `text` **before**
`render::for_publication` and every check; `Reply` gains `suggested_band: Option<String>` (raw, never
parsed to `Level`, never compared to the code level for any decision).
Done: `Says("...\nBAND: Sketchy")` -> `suggested_band == Some("Sketchy")` and `text` has no `BAND:`;
`Says("...\nBAND: Rugged")` at `Sketchy` -> stored raw, `fellback` unchanged from the same text without the
line (pins "logged, not used"); `Says("text with no band")` -> `None`; `Down` and `NoProvider` -> `None`;
`Says("BAND: NothingUglyYet")` alone -> `Fellback::Empty` (the line is not content); a text that fails
`check_level` still fails with a `BAND:` line present (the band cannot rescue a reply); existing
`system_prompt_has_no_digits` / `verdict_brief_has_no_digits` still pass with the new paragraph.
Mutant hazards: the `rsplit_once('\n')`/`strip_prefix("BAND:")` pair — one test for a mid-text `BAND:`
that must NOT be stripped; `trim` on the captured value pinned by a `"BAND:  Sketchy "` fixture.
Verify: clippy scoped to roast; CI green incl. mutants. Rubric: `reply.text` identical with and without
the band line across `EVERY_LEVEL`; prompt digit count still zero.
Stop and ask: if capturing the band needs a second model call (cost); if `Reply` construction sites
outside voice.rs break (there should be none — `Reply` is built only in `write`).

### M-D-0003 — analyst log + serve cache carry the packet (implementer; blocks on 0001; C, parallel with B)
Files: `crates/realorrug-analyst/src/answer.rs`, `crates/realorrug-analyst/src/log.rs`,
`crates/realorrug-serve/src/check.rs`. Forbidden: `card.rs`, any roast file, `memory.rs`.
Change: `Entry` gains `#[serde(default)] assessment: Option<Assessment>` and
`#[serde(default)] suggested_band: Option<String>`; `build_reply` fills both from
`Assessment::from(&sheet)` and `reply.suggested_band`. `check.rs` computes the same `Assessment::from(&sheet)`
and stashes it as cache-only `_assessment` next to `_signals` via `stash_signals`'s pattern; the public
nine-field contract does not move.
Done: `assert_contract` unchanged and passing (nine fields); `assessment_is_stashed_but_never_served`:
`fresh_cached_raw` has `_assessment`, `fresh_cached` does not; `an_old_entry_without_assessment_still_loads`
(JSON literal missing both fields deserialises); `entry_carries_the_packet` on the existing restart test
with `metrics.calls == 4` unchanged (pins no extra reads); `level` in `Entry` equals `assessment.level`.
Mutant hazards: the `_assessment` insert and strip are each a line a mutant can delete — the
stash/strip test must assert presence in raw AND absence in served.
Verify: clippy scoped per crate (one cargo process at a time); CI green incl. mutants. Rubric: the JSON
of `_assessment` in serve equals `Entry.assessment` for the same sheet fixture (shared packet, proven by
one test that builds both from `sheet_with`).
Stop and ask: if `Entry`'s `signals: Option<Vec<Signal>>` would be better replaced by the packet (it would,
but not this slice); if `check.rs` needs the packet on the public surface (owner question below).

### M-D-0004 — independent review of 0001 and 0003 (reviewer; blocks on 0001, 0003)
Reads only. Checks: the episode map against design 0027 §"Judgement" groups; that no code path reads
`admissible` or `suggested_band` to publish; that `risk_index` never enters `authorised()` numbers or
prompt text (rule 2: a digit in the prompt is a fabrication hazard); that the 0027 as-built note and the
verdict.rs doc changed with the behaviour; that `Entry`'s new fields are `serde(default)`.
Rubric: a written yes/no per check with file:line. Stop: any "no" reopens the owning unit, not the plan.

## Order and parallelism
0001 first (alone). Then 0002 and 0003 together (disjoint files). 0004 after 0003 merges. Max two
implementers at once is satisfied. Each unit is one PR; rebase on `main` before opening if
`creator-cash-flow` has merged — the `Dossier` literal in `verdict.rs` tests and `metrics.calls == 4` in
`answer.rs` are the two known merge points.

## Not verified
- Whether the skip-list gaps in `FactSheet::build` are reachable from the sheet (0001 stop condition).
- Whether `Dossier` has `Default` (would soften the slice-5 merge point in `verdict.rs` tests).
- Exact weight values: proposed LaunchBlock 25, CreatorHistory 20, Exit 40, Holders 25, Authority 20
  (sum > 100 so saturation is reachable); wrong weights cost nothing published — the index is logged only.

## QUESTIONS (owner only)
1. May `/v1/check` JSON gain an `assessment` field (public surface, site change) or stay cache-only for now?
   Planned: cache-only.
2. Should the `BAND:` request go in the same model call (prompt grows by one paragraph, no extra spend) —
   planned — or wait until approval so the prompt does not change at all?
3. Is the LaunchBlock episode (band + dev buy count once) the correlation you meant? It is the only pair
   of live signals production can raise today, so it is the one that moves a published level.
