<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0065 — cached-read age on every served surface

**Date:** 2026-09-27.
**Status:** CHECKED. Every path below was read on this branch today; the one
gap found was fixed and pinned with a test in the same commit, re-applying
the fix's own absence to see the new tests fail before the fix went back in.

## Why

[ADR 0039](../adr/0039-the-launch-gates-and-the-monthly-ceiling.md) decision
6: "When a read is served from cache, the reply or page says how old it is;
stale data is never shown as current." That line was written for the
monthly-ceiling cache generally; this note walks every surface that can
serve a stored or cached read and records, per surface, whether the age
actually reaches the viewer.

## Inventory

| Surface | Verdict | Where |
|---|---|---|
| `crates/realorrug-serve/src/check.rs` `fresh_cached`/`fresh_cached_raw` | **Holds** | `verdict_doc` fixes `measured_at` at the moment of the chain read, before the doc is written to disk (`check()`); the cache hit path never recomputes it. Pinned by `a_served_cache_hit_still_carries_its_original_measured_at`. |
| `crates/realorrug-serve/src/card.rs` (share-card PNG, `Cache-Control: public, max-age=600`) | **Was a gap — fixed** | The rendered card carried no digits at all (by design, "no price, ever"), so a viewer or an X-unfurl cache showing it minutes later saw no read-age either — an omission that reads as current for an image whose whole point is to be reshared. Fixed: `handle` now reads the cache entry's own `_written_at` (already present, written once by `check.rs`), computes `now.saturating_sub(written_at)`, and `build_svg` draws it via the new `format_age`. |
| `crates/realorrug-serve/src/public.rs` (`recent_in`, `stats_in`, `leaderboard_in`, `week_doc`) | **Holds** | Every `measured_at`/`closed_at`/`built_at` field is either a fresh disk read at request time or the event's own recorded timestamp; nothing here re-stamps "now" over an old value. |
| `crates/realorrug-serve/src/record.rs` | **Not a cache** | Write-only; nothing reads it back yet. |
| `crates/realorrug-serve/src/facts.rs` | **Not a cache** | Paid x402 endpoint; always reads the chain fresh, no caching layer. |
| `site/src/Check.tsx` | **Holds** | Renders `` `Read from the chain at ${result.measured_at}.` `` verbatim from the server's own field; never substitutes the browser's clock. |
| `site/src/honesty.ts` `measuredAgo` | **Holds** | Computes elapsed time from a measured ISO timestamp against a passed-in `now: Date`, never treats "now" as the read moment. Used correctly by `History.tsx` and `Home.tsx`. |
| `site/src/Token.tsx` | **Not a cache** | Static page, no served read. |
| `crates/realorrug-analyst/src/answer.rs` / `admission.rs` (`Gate`'s dedupe-window cache reuse) | **Holds** | A `FactSheet` reused inside the dedupe window keeps its original `dossier.read_at` (a chain block/slot); `realorrug-roast/src/sheet.rs`'s `push_curve` labels every price fact with that block/slot, so reuse never relabels a price as newly read. |
| `crates/realorrug-analyst/src/bio.rs` (`POOL_FRESH_SECONDS`, `fresh_pool`) | **Holds, by omission** | A bio field can't carry an age string, so once a pool figure passes `POOL_FRESH_SECONDS` (6h) it is dropped from the reply rather than shown stale-as-current — a valid alternative to disclosing an age, since ADR 0039 decision 6's actual requirement is "never shown as current," not "always disclose an age." |
| `crates/realorrug-analyst/src/daemon.rs`, `telegram.rs`, `x.rs` | **Holds (reviewed, no gap found)** | `daemon.rs` has its own `is_stale_at` guard for base rates; `telegram.rs`'s cache reuse goes through the same `dedupe_seconds`/`admission.rs` mechanism already covered above. Not read as exhaustively line-by-line as `check.rs`/`card.rs`; flagged below as the one area a future pass could look at harder. |

## What changed

- `crates/realorrug-serve/src/card.rs`: added `format_age` (words seconds
  into "read just now" / "read N min ago" / "read N hr ago"), threaded the
  cache entry's `_written_at` through `handle` into a new `age: Option<&str>`
  parameter on `build_svg`, and drew it under the chain label. Three new
  tests: `format_age_words_the_three_bands`,
  `an_age_is_drawn_only_when_one_is_given`,
  `the_age_reflects_the_caches_written_at_not_the_serve_time`.
- `crates/realorrug-serve/src/check.rs`: one new test,
  `a_served_cache_hit_still_carries_its_original_measured_at`, pinning the
  already-correct behaviour so a future change to `fresh_cached` that
  recomputed `measured_at` at serve time would fail it.
- `docs/design/0025-the-website.md` §7: added the read-age requirement the
  card now meets, so the document matches the shipped behaviour in the same
  commit (AGENTS.md §4).

## Left as a question, not built here

- `realorrug-analyst`'s "as of {moment}" phrasing sometimes sits next to
  "right now" in the same reply — a pre-existing wording quirk, not a case
  of a stale read being shown as current (the moment itself is always the
  real read time). Out of this packet's line budget; worth a follow-up pass
  if it reads as confusing in practice.
- `daemon.rs`/`telegram.rs`/`x.rs` were reviewed by targeted grep and by
  following the one dedupe/cache mechanism they share with `answer.rs`, not
  read end-to-end. No gap surfaced in that pass.
