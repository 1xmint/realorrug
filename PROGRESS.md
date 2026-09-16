# Progress — packet 0036 (feat/0035-log-read-point)

1. [x] `ReadAt` derives `Serialize`/`Deserialize`, externally tagged, no `JsonSchema` — `crates/realorrug-types/src/chain.rs` (6c0fc63)
2. [x] `Entry` gains `pub read_at: Option<ReadAt>`, `read_at_slot` doc updated, fourth pinned-line test added — `crates/realorrug-analyst/src/log.rs` (2bd7922)
3. [x] `answer.rs` computes `read_at` once and derives `read_at_slot` from it — `crates/realorrug-analyst/src/answer.rs` (545f327)
4. [x] Every `Entry` literal updated across the tree (8d67006), including realorrug-serve/src/public.rs which the packet's grep list missed
5. [x] Tests: Robinhood round-trip keeps block number; Solana entry both fields agree; pre-`read_at` line still loads as `None` (in 2bd7922)
6. [ ] `cargo test -p realorrug-analyst -p realorrug-types`; roast/cli/onchain still build — all passed locally, re-verify after fmt
7. [ ] `clippy --all-targets -- -D warnings` on edited crates; `cargo test -p repo-conformance`
8. [ ] `cargo fmt --check` last, then push

Watch out for: crates/realorrug-roast/ is owned by another worker — do not touch it.
