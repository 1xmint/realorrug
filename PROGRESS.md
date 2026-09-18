# PROGRESS: the-bio-shows-the-whole-week

Done: All three units complete and committed (5197d67 Unit 1 bio.rs/daemon.rs,
88cca2d Units 2+3 realorrug-cli bio --preview + analyst.env.example doc
paragraph). Branch pushed to origin. Next step is opening the PR.

Next: Open the PR against main (do not merge), body ending with the
Claude Code attribution line. Then report PR URL + CI result (push and
check once, do not poll/wait).

Watch out for: NEVER ran cargo/tests locally per packet -- CI is the
only verification; watch its first run closely since bio.rs/daemon.rs/
realorrug-cli/bio.rs are all new code that has never compiled anywhere
yet. If CI fails, the likely spots are: bio.rs's render_parts wording
tests (exact char counts), or realorrug-cli/bio.rs's use of
realorrug_contest::Week / realorrug_types::civil::date_from_days
(confirm both are already crate deps -- they are, per Cargo.toml).
