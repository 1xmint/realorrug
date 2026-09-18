# PROGRESS: the-bio-shows-the-whole-week

Done: All three units committed and pushed (5197d67, 88cca2d, 71be6a0).
PR opened: https://github.com/1xmint/realorrug/pull/103. CI just started
(all checks "pending" as of this write) -- not polled further per the
"never sit and wait on async checks" rule.

Next: Check `gh pr checks 103` for the run result. If green, done --
report and stop (packet says do not merge). If red, read the failing
job's log, fix on this same branch, commit, push -- do not rerun cargo
locally beyond what AGENTS.md/the packet allow (this packet forbids ALL
local cargo, even a single test).

Watch out for: this is new code across bio.rs (large rewrite),
daemon.rs (signature change to bio_to_write/choose), and a brand new
crates/realorrug-cli/src/bio.rs -- none of it has ever compiled anywhere
until CI runs it. Likely first-failure spots: exact-char-count test
fixtures in bio.rs's drop-order tests, or an unused-import/clippy lint
in the new CLI file. mutants-shards may also flag weak-but-passing
tests in the new drop-order logic.
