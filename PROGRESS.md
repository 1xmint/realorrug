# PROGRESS: chore/realorrug-env-names (RADAR_* -> REALORRUG_*)

## Done (pushed to origin/chore/realorrug-env-names)
- `realorrug-types::env::env_or_legacy` helper (pre-existing on the branch).
- `realorrug-analyst`: daemon.rs (pre-existing) + bio.rs, lane2.rs, oauth.rs,
  spend.rs, telegram.rs, x.rs, answer.rs doc comment, tests/one_poll_end_to_end.rs
  doc comment. All renamed through `env_or_legacy`/`env_legacy`. `cargo check -p
  realorrug-analyst` clean.
- `realorrug-model`: `non_empty()` signature changed to `(get, new, old)` and
  reads through `env_or_legacy`; api_key.rs, catalog.rs (doc comments), codex.rs,
  lib.rs, openai.rs all updated. Codex's `INHERITED` passthrough list (PATH,
  HOME, ...) is unrelated and calls `non_empty(get, name, name)` unchanged.
  `cargo check -p realorrug-model` clean.
- `realorrug-onchain/src/rpc.rs`: `RADAR_RPC` -> `REALORRUG_RPC` via
  `env_or_legacy`. `cargo check -p realorrug-onchain` clean.
- `realorrug-cli`: `model_prices.rs` (`REALORRUG_MODEL_NAME`,
  `_PRICE_IN`, `_PRICE_OUT`, `_REASONING_EFFORT`, all via `env_or_legacy`,
  the direct-read gap is fixed), `analyst.rs` and `roast.rs` doc comments.
  `cargo check`/`clippy -p realorrug-cli` clean, `cargo fmt` run repo-wide
  (reflowed a few lines in already-committed files harmlessly).
- `realorrug-payout`: added `realorrug-types` dependency for
  `env_or_legacy`; `REALORRUG_PAYOUT_ADDRESS`, `REALORRUG_PAYOUT_FLOOR_WEI`,
  `REALORRUG_CONTEST_DIR` all fall back to their `RADAR_*` names. Added
  `the_old_payout_address_name_still_works` test (passed:
  `cargo test -p realorrug-payout the_old_payout_address_name_still_works`).
  `cargo check -p realorrug-payout` clean. NOTE: did not check whether
  `RADAR_TRUST_CLOUDFLARE` / `RADAR_CHECK_DAILY_BUDGET` exist in this crate
  or in `realorrug-serve` -- re-grep before assuming done.

Git grep for `RADAR_[A-Z_0-9]*` (real var names, not prose) now clean in:
`crates/realorrug-analyst`, `crates/realorrug-model`,
`crates/realorrug-onchain`, `crates/realorrug-cli`, `crates/realorrug-payout`
(only the intentional `old` fallback-argument literals to `env_or_legacy`
remain in payout's main.rs/lib.rs/tests.rs -- named there explicitly).

## NOT done yet -- still to rename
Grep `git grep -n "RADAR_" -- '*.rs' '*.example' '*.yml' '*.toml' '*.ts' '*.md' 'justfile'`
still hits:
- `crates/realorrug-serve/src/check.rs`, `main.rs`, `public.rs` --
  `RADAR_CHECK_CACHE_DIR`, `RADAR_CONTEST_DIR`, `RADAR_SITE_ORIGIN`,
  `RADAR_X402_PAY_TO`, `RADAR_POPULATION`, `RADAR_BASE_RATES` (verify names).
- `crates/realorrug-roast/src/sheet.rs` -- check what it reads.
- `.cargo/mutants.toml` -- one remaining mention besides `RADAR_BUILD_SHA`
  (already done); check what else it names.
- `deploy/analyst.env.example`, `deploy/payout.env.example`, `deploy/README.md`
  -- add the new `REALORRUG_*` names; per the PR body's original plan, keep a
  comment noting the old names still work via the fallback, do not delete the
  old lines outright (the box's own env files are edited by the owner, by
  hand, separately).
- `site/src/honesty.ts` -- check if this names an env var or just prose
  about "Radar"/"radar" the product; only rename if it is an actual env-var
  string used against a served API.
- `docs/adr/0013-*.md`, `0024-*.md`, `0025-*.md`, `0026-*.md`,
  `docs/design/0019-*.md`, `0020-*.md`, `0021-*.md`, `0023-*.md`, `0024-*.md`,
  `docs/research/0049-*.md` -- most of these say "RADAR_FOO" as a documented
  config key; rename to match code, but check each one is naming a variable
  and not a historical quote (e.g. an ADR recording what was decided in the
  past may deliberately keep the old name if it is quoting/dating a decision
  -- read AGENTS.md rule "citation of Radar's record says Radar ADR").
- `justfile` -- not yet grepped for `RADAR_`; check it directly, it was named
  in the task.

## How to continue
1. `git grep -n "RADAR_[A-Z_0-9]*" -- '*.rs' '*.example' '*.yml' '*.toml' '*.ts' '*.md' 'justfile'`
   to get the current exact list (some may have shifted since this was written).
2. For each `.rs` file: find the `get`/lookup pattern already in use (a
   `&impl Fn(&str) -> Option<String>` getter, or direct `std::env::var`), add
   `use realorrug_types::env::env_or_legacy;`, wrap each read as
   `env_or_legacy("REALORRUG_X", "RADAR_X", get)`, and rename the doc
   comments and tests that read the plain-path key to the new name (leave the
   `old` argument to `env_or_legacy` as the `RADAR_` string -- that is
   intentional, it is the fallback).
3. `cargo +stable-x86_64-pc-windows-gnullvm check -p <crate>` and
   `cargo +stable-x86_64-pc-windows-gnullvm clippy -p <crate>` and
   `cargo +stable-x86_64-pc-windows-gnullvm fmt` per AGENTS.md -- do not run
   `cargo test -p <crate>` widely or `cargo mutants` on this machine; CI runs
   the suites (owner's instruction, 2026-09-16). One named test locally is
   fine.
4. For `deploy/*.env.example` and docs, add the `REALORRUG_*` name; keep the
   `RADAR_*` line as a commented note that it still works via fallback, per
   the PR body.
5. `git grep -c "RADAR_" -- '*.rs' '*.example' '*.yml' '*.toml' '*.ts' '*.md' 'justfile'`
   should be empty except any intentional back-compat left named in the PR
   body (currently: none decided yet -- decide and name it there if one is
   kept, e.g. `.cargo/mutants.toml`'s comment about the old build var).
6. Commit per crate/area, push `work/realorrug-env-names-cont` to
   `origin/chore/realorrug-env-names` with
   `git push origin work/realorrug-env-names-cont:chore/realorrug-env-names`
   (this worktree's local branch is named differently from the PR branch;
   push with the explicit refspec every time), update PR #63's body to
   reflect what is now done, and remove this file once the grep above comes
   back empty.

## Watch out for
- This worktree's local branch is `work/realorrug-env-names-cont`, tracking
  `origin/chore/realorrug-env-names` -- the PR branch itself is checked out
  in a different, sibling worktree (`agent-abcde1335842e6e3b`) that this
  agent could not `cd` into (worktree isolation). Push with the explicit
  `local:remote` refspec shown above, every time.
- `realorrug-payout` is the money-moving crate (AGENTS.md rule 1): be extra
  careful there, re-read its tests before renaming, and do not let the
  legacy-fallback warning print a secret.
- Do not touch `main`, do not merge, keep the PR in draft.
