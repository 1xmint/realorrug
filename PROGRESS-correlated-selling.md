# PROGRESS: M-D-0008 correlated selling (S7)

## Done
- Read research 0052 (S7 row §3.1, worked example D §6, cost row §7.1, M-D-0008 row §8).
- Read existing patterns: `wallets.rs` (`Trade`/`Side`, `LinkEvidence`/`link_confidence`,
  `creator_cash_flow`'s one `logs_range` read as the pattern to copy), `robinhood.rs`
  `build_with_memory`'s numbered steps, `sheet.rs` `factors`/`push_chain_launch`,
  `assessment.rs` `signal_base_bps`/`episode`, `verdict.rs` `LIVE_RISK_SIGNALS`.
- Not yet written any code.

## Next
- `wallets.rs`: add `Sale`/`sells_from` (mirror `Purchase`/`purchases_from` for
  `Side::Sell`), `SELL_CLUSTER_WINDOW_BLOCKS = 50`, `SELL_CLUSTER_LINK_THRESHOLD_BPS`
  (justified in comment), `CorrelatedSelling` struct, `correlated_selling()` reading
  one `logs_range` over the launch->read window (same shape as `creator_cash_flow`),
  finding the largest 50-block window of mutually-linked sellers, its bps of supply,
  and its time spread (two `block_time` reads at the window's first/last sale).
- `dossier.rs`: add `ChainLaunch::correlated_selling: Option<CorrelatedSelling>`;
  `market.rs` (Solana) sets it to `None` with a comment (S7 is Robinhood-only,
  `CurveSell` pre-graduation, research 0044 blocks it post-graduation).
- `robinhood.rs`: call `correlated_selling` as a new numbered step in
  `build_with_memory`, after 5c (creator cash flow); update `ChainLaunch` literal
  at line ~204 (if needed) and any mock-RPC tests' pinned call counts (`HOLDERS`
  pattern) if `build`'s call count changes.
- `sheet.rs`: `Signal::CorrelatedSelling`; `Kind::CorrelatedSellWallets` /
  `CorrelatedSellVolumeBps` / `CorrelatedSellSpreadSeconds`; `push_correlated_selling`
  called from `push_chain_launch`; `factors()` arms for +800/+600/-300 with
  range `match`; `twin_for`/`Signal::plain` arms (exhaustive matches will fail to
  compile otherwise); `fidelity.rs` `Subject::of` arm (Holders subject, it's about
  the selling wallets, not the creator).
- `assessment.rs`: `signal_base_bps` arm (1,000); `episode` arm -- likely a new
  or `Episode::Exit` grouping (S7 is an act like S10/11/12); check doc comment.
- `verdict.rs`: add to `LIVE_RISK_SIGNALS` (acts can't be sat under, research
  0052 §9) -- confirm no existing fixture already sets this signal (it can't,
  since the signal doesn't exist yet) before adding.
- Tests: named captured-sell-cluster test with mock RPC in `robinhood.rs` style;
  boundary tests (2 vs 3 wallets, 50 vs 51 blocks, 999 vs 1000 bps, 1h vs just
  over) in `wallets.rs` and `sheet.rs`.
- Docs: design 0020 reads table; research 0052 (mark M-D-0008 done, note the
  chosen link threshold and window logic).
- `cargo check -p realorrug-onchain`, `-p realorrug-roast`; `cargo clippy` both
  `--all-targets -- -D warnings`; `cargo fmt`; named tests.
- Commit, push, open PR.

## Watch out for
- Never call anyone a scammer; never touch contest/payout crates,
  assessment-packet, codex/golden.
- Keep RPC cost bounded: one `logs_range` call for S7, matching
  `creator_cash_flow`'s pattern, not a call per 50-block window.
- If adding `CorrelatedSelling` to `LIVE_RISK_SIGNALS` changes any existing
  fixture's published level, STOP and report instead of forcing it through.
- Kind/Signal enums are matched exhaustively in several places (`twin_for`,
  `Signal::plain`, `fidelity::Subject::of`, `assessment::episode`,
  `assessment::signal_base_bps`) -- the compiler will point at all of them.
- One cargo process at a time, `+stable-x86_64-pc-windows-gnullvm`, scoped `-p`.
