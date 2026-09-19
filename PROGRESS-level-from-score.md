Done: `level_from_score` added to `verdict.rs` (shares `level`'s gates via a
new `rugged_pair` helper, adds the coverage-below-6000-bps gate), wired into
`Assessment` as a new `score_level` field (shadow only, published `level`
untouched), printed on `realorrug roast --sheet` as a "provisional" line, all
named/boundary tests passing, `cargo check`/`clippy -D warnings`/`fmt` clean
for `realorrug-roast` and `realorrug-cli`, docs/research/0052 updated with an
implementation note and a flagged discrepancy (S1+S2 share an episode in the
shipped code so never actually noisy-OR to 2,520 as the doc's own example
assumes -- test uses the two base weights directly instead, see the doc note
above §4.2's "M-D-0005 built, shadow only").

Next: push branch `m-d-0005-level-from-score`, open the PR, then stop --
nothing else in scope is outstanding.

Watch out for: the S1+S2 episode-collision noted above needs Josh's call (fix
the episode map, or accept the doc's example was never reachable); admissible
was deliberately left unchanged (still built from published `level`, not
`score_level`) since M-D-0005's scope says "shadow first" and rebuilding it
would let a model reach for a score-chosen band the ladder didn't choose.
