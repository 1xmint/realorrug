# Progress — packet 0036 (feat/0035-log-read-point)

1. [x] `ReadAt` derives `Serialize`/`Deserialize`, externally tagged, no `JsonSchema` — `crates/realorrug-types/src/chain.rs` (6c0fc63)
2. [x] `Entry` gains `pub read_at: Option<ReadAt>`, `read_at_slot` doc updated, fourth pinned-line test added — `crates/realorrug-analyst/src/log.rs` (2bd7922)
3. [x] `answer.rs` computes `read_at` once and derives `read_at_slot` from it — `crates/realorrug-analyst/src/answer.rs` (545f327)
4. [x] Every `Entry` literal updated across the tree (8d67006), including realorrug-serve/src/public.rs which the packet's grep list missed
5. [x] Tests: Robinhood round-trip keeps block number; Solana entry both fields agree; pre-`read_at` line still loads as `None` (in 2bd7922)
6. [x] `cargo test -p realorrug-analyst -p realorrug-types` pass; roast/cli/onchain/serve still build
7. [x] clippy clean on realorrug-types/realorrug-analyst/realorrug-serve; `cargo test -p repo-conformance` passes
8. [x] `cargo fmt --check` clean as the last command before push (caught one line-wrap in answer.rs, fixed in bc4b1a6)

All done. Pushed to feat/0035-log-read-point. Nothing left undone from the packet.

Watch out for: crates/realorrug-roast/ is owned by another worker — do not touch it.
