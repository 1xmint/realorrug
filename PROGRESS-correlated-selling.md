# PROGRESS: M-D-0008 correlated selling (S7)

## Done
- `crates/realorrug-onchain/src/wallets.rs`: `Sale`/`sells_from` (mirrors
  `Purchase`/`purchases_from` for `Side::Sell`, using `Trade::trader` as the
  seller); `SELL_CLUSTER_WINDOW_BLOCKS = 50`, `SELL_CLUSTER_WALLET_THRESHOLD
  = 3`, `SELL_CLUSTER_VOLUME_BPS_THRESHOLD = 1_000`,
  `SELL_CLUSTER_SPREAD_SECONDS_THRESHOLD = 3_600`,
  `SELL_CLUSTER_LINK_THRESHOLD_BPS = 4_000` (justified in a doc comment:
  same_window+sizes tier, since both_fresh needs an unbudgeted extra read);
  `largest_sell_cluster` (pure, buy-side evidence only -- never uses
  `LinkEvidence::correlated_sell` to avoid bootstrapping); `CorrelatedSelling`
  struct (`sells_read: bool` gates rule 8); `correlated_selling()` (one
  `logs_range` read, same shape as `creator_cash_flow`, degrades to
  `sells_read: false` rather than `Err`).
- `dossier.rs`: `ChainLaunch::correlated_selling: Option<wallets::CorrelatedSelling>`.
  `market.rs` (Solana), and the test literals in `sheet.rs`/`verdict.rs`/
  `robinhood.rs`, all updated to set/expect it.
- `robinhood.rs`: new step 5d in `build_with_memory`, after 5c, writing onto
  `dossier.chain_launch.as_mut()` (mutated after launch_facts, since that
  runs before the read point/window is known). Updated the
  `a_launch_reads_its_block_age_launcher_buy_and_holders` test's expected
  `ChainLaunch` to include `correlated_selling: Some(CorrelatedSelling{
  sells_read: false, .. })`, since `full_bodies`' 12 scripted answers are
  already spent by step 5 (holders) and steps 5b/5c/5d all hit a refused
  connection -- no new bodies needed, this is the same "runs past the
  script" behavior 5b/5c already had.
- Committed `83843e6` on branch `m-d-0008-correlated-selling`, pushed
  nowhere yet.

## Next (not started)
- **Not yet run**: `cargo check -p realorrug-onchain` / `-p realorrug-roast`.
  Skipped this turn because `Get-Process cargo,rustc` showed processes
  already running (PIDs 1328/16720 cargo, 16428/17176 rustc) that this
  session did not start -- likely another concurrent agent on this machine.
  **First thing the next session should do**: re-check
  `Get-Process cargo,rustc`; if clear, run `cargo check -p realorrug-onchain`
  first (it's the crate with all the new logic) and fix whatever it finds
  before touching roast. Likely issues to expect: `Address` derefs
  (`t.trader`/`log.address` types), `abs_diff` availability, unused-import
  warnings once roast-side code lands.
- `crates/realorrug-roast/src/sheet.rs`: add `Signal::CorrelatedSelling`
  variant; `Kind::CorrelatedSellWallets` / `CorrelatedSellVolumeBps` /
  `CorrelatedSellSpreadSeconds` (or similar) in `clause.rs`; `twin_for` and
  `Signal::plain` arms (compiler will point at both, they're exhaustive);
  `push_correlated_selling` (mirror `push_dev_buy_share`, read
  `launch.correlated_selling`, only push facts when `sells_read` is `true`)
  called from `push_chain_launch`; three `factors()` range-`match` blocks for
  +800 (>= 3 linked wallets), +600 (>= 1,000 bps), -300 (> 3,600 s spread),
  all Measured grade.
- `crates/realorrug-roast/src/fidelity.rs`: add the new `Kind` variants to
  `Subject::of` -- plan is `Self::Holders` ("it's about the selling wallets,
  not the creator"), but reconsider given `Subject::Launch` also exists and
  might fit better since it's window-scoped launch behavior; whichever is
  chosen, explain why in a comment per the packet.
- `crates/realorrug-roast/src/assessment.rs`: `signal_base_bps` arm
  `Signal::CorrelatedSelling => 1_000` (research 0052 §3.1); `episode` arm --
  plan is `Episode::Exit` grouped with `LiquidityGone`/`CreatorSoldOut`/
  `BuyersCannotSell` since S7 is an act, not a shape (research 0052 §9), with
  a comment saying so.
- `crates/realorrug-roast/src/verdict.rs`: append `Signal::CorrelatedSelling`
  to `LIVE_RISK_SIGNALS`. Cannot change any existing fixture's level (the
  variant does not exist in any fixture before this work), but re-check the
  diff once the variant compiles, per the packet's explicit "if it would,
  stop and report" instruction.
- Tests still needed:
  - `wallets.rs`: boundary tests for 2 vs 3 wallets (in `largest_sell_cluster`
    consumers / a small helper), 50 vs 51 blocks (`largest_sell_cluster`
    itself -- the `other.2 - anchor.2 > SELL_CLUSTER_WINDOW_BLOCKS` line).
  - `sheet.rs`: named "captured sell cluster" test built with the mock RPC
    the way `robinhood.rs` tests do (may need to live in `robinhood.rs`
    instead if it needs `full_bodies`/`serve` -- check which crate the
    packet's "the named test" refers to; re-read the task packet's wording
    if unsure); boundary tests for 999 vs 1,000 bps and exactly 1h vs just
    over, in `factors()`'s style (see
    `share_of_999_bps_lands_in_the_500_bps_band_not_the_1000_bps_band`).
- Docs: `docs/design/0020-*.md` reads table (not yet located -- grep for
  "0020" and the reads table heading) and `docs/research/0052-weighted-flags.md`
  (mark M-D-0008 done in §8, note the chosen 4,000 bps link threshold and
  the "one ranged read, not one call per window" cost interpretation) --
  same commit as the roast-crate code per AGENTS.md.
- Run `cargo clippy -p realorrug-onchain --all-targets -- -D warnings`,
  same for `-p realorrug-roast`, `cargo fmt`, and named tests, one cargo
  process at a time with `+stable-x86_64-pc-windows-gnullvm`.
- Commit the roast-crate + docs changes as a second logical commit, push,
  open the PR with `gh pr create` (body ends with the Claude Code
  attribution line; do not merge).

## Watch out for
- Never call anyone a scammer; never touch contest/payout crates,
  assessment-packet, codex/golden.
- `wallets.rs`'s `correlated_selling()` was never compiled or tested this
  turn -- treat it as a draft. In particular double-check: `Address` derives
  `Copy`? (used as `HashMap`/`BTreeMap` key via `.0`, and stored plain in
  `Vec<Address>` -- if `Address` is not `Copy` some of this needs `.clone()`).
  `LogsError` variants matched exactly as `creator_cash_flow` does. `Rpc::
  block_time` returns `Result<(u64, u64), String>` -- confirm the tuple order
  is `(number, timestamp)` as assumed (`(_, timestamp)` destructure).
- If adding `CorrelatedSelling` to `LIVE_RISK_SIGNALS` changes any existing
  fixture's published level, STOP and report instead of forcing it through.
- Kind/Signal enums are matched exhaustively in several places (`twin_for`,
  `Signal::plain`, `fidelity::Subject::of`, `assessment::episode`,
  `assessment::signal_base_bps`) -- the compiler will point at all of them.
- One cargo process at a time, `+stable-x86_64-pc-windows-gnullvm`, scoped
  `-p`. Check `Get-Process cargo,rustc` first -- a previous session left some
  running and this session never used them.
