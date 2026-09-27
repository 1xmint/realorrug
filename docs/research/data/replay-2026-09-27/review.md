# Replay review — 2026-09-27 recapture, build 321da56

Josh makes the final accept/reject call on every reply below (the `accept? (yes/no, note):`
lines carried over from each case's own generated review file). This file's job is to lay
out the evidence and a first pass of pre-read judgment against AGENTS.md §3, not to accept
anything on its own.

- **Build**: `main` at commit `321da56` (PR #196, "wire Signal::CreatorSoldOut from a single
  capture"), `release-linux` workflow run `36289629533`. Binary sha256
  `7e9d934aaa1139ea6c1cf002763e5324fa3bef8180e61d93bbcee7bb24f7aa57`, verified against
  `BUILD-INFO.txt` immediately after download and again on the VPS after transfer.
- **When**: captures ran 2026-09-27T02:57:01Z–02:59:41Z (one pass). Replay (`--model`) ran
  immediately after.
- **Host**: `guardian-vps-tail`, fresh directory `~/replay-2026-09-24/m321da56/` under the
  operator's home directory (not a deploy; `realorrug-serve` and `/etc` were not touched;
  `REALORRUG_X_PUBLISH` stayed off; nothing posted, signed, or traded).
- **Budget**: live `Budget::default()` (20 s read deadline), no override flag — there is no
  deadline/budget flag on `capture` per `--help`.
- **Summary**: 12 captures, all `rc=0`. 2 of 12 are `CantTell`, both the *same mint*
  (`2XHGAAvkxKE8fS97e5mNar5wzADj6ckgoxq2ukYjpump`) captured under two labels
  (`incomplete-read-funding` in the 11-case set, `creator-sale-2xhg` as the standalone
  creator-sale check) — both `CantTell` for the identical reason, a **funding-search gap**
  (one of four checked early buyers' money could not be traced back), not a clock
  ("Deadline") gap, so per the packet's own rule a second pass 8 minutes later was not run.
  `Signal::CreatorSoldOut` fired on 4 of 12 cases, across **3 distinct mints**:
  `incomplete-read-funding` / `creator-sale-2xhg` (same mint, both fired),
  `clean-read-versioned-tx`, and `clean-read-pay`. None of the 12 reached `Rugged`, because
  `Rugged` needs `CreatorSoldOut` *and* `BuyersCannotSell` together (`verdict.rs`) and no
  case had both. All 12 cases passed all 6 automated checks (evidence fidelity, unsupported
  accusation, unknown-data — for both reply and report); 12/12 replies used the model, 0
  fell back to the template.

## Lead's second read (2026-09-26, overrides the pre-read below where they differ)

The pre-read further down marks all twelve "looks acceptable". Read against the
generated text in `out/review.md`, two are not, and a third report is wrong in the
same way:

- **clean-read-versioned-tx — problem.** "their balance changed 0.9431 SOL *per
  sale*" misreads the fact: 0.9431 SOL is the *sum* of the creator's per-sale balance
  changes (the fact's label says "per sale, summed across every decoded sale", and the
  model kept the first half). Every number passes the check, but the sentence states
  something the sheet does not. "lean toward creator extraction" also names what a
  person did rather than what the launch shows; under AGENTS.md §3 rule 4 it is at
  the edge, and a reply that says "the creator's wallet sold everything it bought and
  now holds none" says the same thing on the sheet's own terms.
- **clean-read-pay — problem.** The level is `Sketchy` because `CreatorSoldOut`
  fired, and the reply never mentions the creator. It leads with early buyers'
  wallet ages and calls them "a recurring coordinated-launch shape": no fact on the
  sheet establishes coordination or recurrence, and wallets that were already active
  are, if anything, the opposite of the fresh-wallet pattern. The model introduced a
  finding the sheet does not hold.
- **The shared cause is in the report, not the model.** In all three sheets where
  `CreatorSoldOut` fired (`clean-read-versioned-tx`, `clean-read-pay`,
  `incomplete-read-funding`/`creator-sale-2xhg`), the report's "Strongest concern" is
  a 0.1% (or 0.0%) unidentified holder, not the creator's sale. In
  `realorrug-roast/src/salience.rs`, `signal_kinds` maps `CreatorSoldOut` to
  `Kind::CreatorCashFlow`, but `rank` has no candidate built from that kind, so the
  fired signal backs nothing and the lead falls back to priority order among the
  unbacked candidates. The model is handed a weak lead and either
  reaches for another fact (pay) or reads the raw figures unguided (versioned-tx).
  Fix before these two are replayed: when `CreatorSoldOut` fires, the creator's
  cash flow and empty wallet lead; and the "per sale, summed" label is reworded so
  the sum cannot be read as a per-sale figure.

The other ten read acceptably to me on the same checks. Josh's call on all twelve;
the two above I would reject and regenerate after the fix rather than accept.

## Per-case table

| label | mint (short) | wall secs | gaps | level | signals | CreatorSoldOut | checks | billed |
|---|---|---|---|---|---|---|---|---|
| ordinary-launch | 24RwgHxwu8ic… | 15.02 | 0 | NothingUglyYet | none | no | 6/6 PASS | $0.000470 |
| incomplete-read-funding | 2XHGAAvkxKE8… | 15.76 | 1 (funding) | CantTell | creator_sold_out | **yes** | 6/6 PASS | $0.000494 |
| creator-sale-catwif | 5pYB12kEhfhS… | 9.67 | 0 | Sketchy | creator_bought_own_launch | no | 6/6 PASS | $0.000434 |
| suspicious-launch-snappad | 5t5keof7mNJA… | 14.60 | 0 | NothingUglyYet | none | no | 6/6 PASS | $0.000465 |
| creator-sale-hbull | 7V6Sk63y8Rr1… | 13.93 | 0 | Sketchy | holder_concentration | no | 6/6 PASS | $0.000452 |
| suspicious-launch-creator-buy | Axo9EE6yU5B3… | 13.43 | 0 | Sketchy | creator_bought_own_launch | no | 6/6 PASS | $0.000460 |
| misleading-concentration-pool | DsjPNCjFrQDX… | 13.13 | 0 | NothingUglyYet | none | no | 6/6 PASS | $0.000465 |
| clean-read-versioned-tx | EYPSU1oha6EL… | 13.84 | 0 | Sketchy | creator_sold_out | **yes** | 6/6 PASS | $0.000488 |
| graduated-pumpswap | GTBxUiw6wJdm… | 15.87 | 0 | NothingUglyYet | none | no | 6/6 PASS | $0.000438 |
| creator-sale-jimothy | Ge87EtsjwRQb… | 15.84 | 0 | NothingUglyYet | none | no | 6/6 PASS | $0.000438 |
| clean-read-pay | JB2rSPb4W4bn… | 16.00 | 0 | Sketchy | creator_sold_out | **yes** | 6/6 PASS | $0.000509 |
| creator-sale-2xhg | 2XHGAAvkxKE8… | 17.25 | 1 (funding) | CantTell | creator_sold_out | **yes** | 6/6 PASS | $0.000491 |

Total billed across all 12 replies: **$0.005604**. "gaps" = length of the sheet's `unknown`
array (facts a required read could not settle); the two funding gaps are the identical
"where 1 of 4 checked early buyers got their money could not be read" gap, not a clock gap.

## creator-sale-2xhg: why CreatorSoldOut fired here, and why the two fallback mints also fired it

The packet asked, if `creator-sale-2xhg` did not produce `CreatorSoldOut`, to also capture
`JB2rSPb4W4bnnr5HwQ17JPTi7gMbvdhjgJUE2oQbpump` and
`EYPSU1oha6ELaZ4wN1crMcdnXDb21S6LWkJXohs7pump` and explain why each did or did not fire. In
fact **`creator-sale-2xhg` did fire `CreatorSoldOut`** (my first pass at reading the sheets
misread the JSON nesting — the `signals` array lives under `sheet.sheet.signals`, not the
top level — and wrongly concluded nothing had fired; re-reading with the correct path
showed 4 of 12 cases firing it). Both fallback mints were already in the 11-case set anyway
(as `clean-read-versioned-tx` and `clean-read-pay`), so no extra captures were needed —
they too fired `CreatorSoldOut` independently. Per PR #196's rule, the signal fires when all
three hold: (1) `creator_cash_flow.trades_complete` is true with no gaps in that read, (2)
at least one decoded buy with `tokens_sold() >= tokens_bought()` and bought > 0, (3) the
creator's address is absent from `token_ownership.owners`. For all three firing cases the
sheet shows a creator wallet with completed, gap-free sale history and zero remaining
token-account presence — `creator-sale-2xhg`/`incomplete-read-funding` show 20.2231 SOL net
creator cash flow, `clean-read-versioned-tx` shows 0.3336 SOL net creator cash flow via a
versioned transaction on a second-generation AMM ("Pons v2"), and `clean-read-pay` shows the
creator wallet emptied with no gap in that specific read (the case's own `CantTell`-adjacent
gap, where present, is the unrelated early-buyer funding-search gap, not a gap in the
creator-cash-flow read itself). None of the three reached `Rugged` because none also had
`BuyersCannotSell` fire — the sell side of the pair was never established in any of these
sheets.

## Pre-read: reply-by-reply judgment against AGENTS.md §3

Judged against: every number traces to the fact sheet; no named person is called a
scammer/thief; a price/market cap carries its moment; a forward-looking hint is hedged and
never an instruction to buy/sell/hold; the lead matches the most important ranked fact. None
of the 12 replies below state a bare price or market-cap figure, so the "price with its
moment" criterion does not apply to any of them (n/a rather than pass or fail) — all dollar
figures present are the replay's own model-billing costs, not token prices, and are not part
of the reply text itself.

### ordinary-launch — looks acceptable
"At about 199.8 hours old… 0.08% of supply… just 4 of 22 buyers checked." No signal fired;
the lead correctly matches the ranked context fact (largest unidentified holder share) since
there is nothing stronger to lead with. No accusation, no forecast, all numbers traced by
the automated check.

### incomplete-read-funding — looks acceptable
Leads with the funding-search gap ("unreadable funding… cannot settle") ahead of the
20.2231 SOL creator cash-flow figure, which matches the case's own level (`CantTell`
overrides a fired `CreatorSoldOut` here because the gap is unresolved) — the lead following
the unresolved gap rather than the fired signal is consistent with the CantTell branch of the
level logic, not a misordering. Hedges correctly ("cannot settle" rather than a verdict); no
accusation.

### creator-sale-catwif — looks acceptable
Leads with the fired signal (creator dev-buy of 0.5326 SOL) and explicitly states the
alternative reading ("possibly conviction") before leaning sketchy — this is the hedge
AGENTS.md §3.5 asks for. No named-person accusation; describes the launch, not a person.

### suspicious-launch-snappad — looks acceptable
No signal fired; leads with age and the largest sampled wallet share, consistent with
ranking by fact kind when nothing stronger exists. No forecast language ("has not graduated,
so the story is still early" is a status read, not a hint about the future).

### creator-sale-hbull — looks acceptable
Leads with the fired signal (24.2% unidentified holder) and states the alternative reading
(vesting contract, bridge, exchange) in the same breath, matching the report's own
alternative-explanations table. No accusation of a named party; the address is described,
never called a person or given a name.

### suspicious-launch-creator-buy — looks acceptable
Leads with the fired signal (1.0000 SOL dev buy) and hedges it ("real red flag but can also
mean they believed in the launch") before giving its own lean, distinguishing the model's
inference from a sheet fact. No accusation.

### misleading-concentration-pool — looks acceptable
No signal fired; leads with age/graduation and the largest wallet share, then the
funding-check status of early buyers. All figures traceable; no forecast or accusation. The
label ("misleading-concentration-pool") is the operator's own case label, not part of the
generated reply.

### clean-read-versioned-tx — looks acceptable
Leads with the fired `CreatorSoldOut` signal (creator's decoded sales, 0.3336 SOL net) ahead
of the alternative reading (transfer vs. sale), which matches this case's report ranking
`CreatorCashFlow` as the strongest concern. Correctly hedges its own lean ("makes me lean
toward creator extraction") rather than asserting it as settled fact. No named-person
accusation — describes "the creator" generically, never a name or account label as a
scammer.

### graduated-pumpswap — looks acceptable
No signal fired; leads with graduation status and largest holder share, in line with ranking
when no signal is present. No forecast, no accusation, numbers check out.

### creator-sale-jimothy — looks acceptable
No signal fired (this mint's own creator-cash-flow read did not establish the same three
conditions PR #196 requires); leads with age/graduation and largest-holder share, and states
the funding-check coverage (4 of 14) plainly. No accusation, no forecast.

### clean-read-pay — looks acceptable
Leads with the funding-history pattern (3 of 4 early buyers already active on-chain,
first-seen dates given) rather than the fired `CreatorSoldOut` signal itself; the report's
own "Strongest concern" for this case is also the token-ownership/creator-cash-flow context
rather than a labelled `CreatorSoldOut` sentence, so the reply's lead is consistent with the
report it is paired with. Calls the pattern "a recurring coordinated-launch shape" — this
describes buyer behavior, not a named person, and is not a forecast or trade instruction, so
it does not cross into accusation despite the strong wording; still worth Josh's own read
given how close "coordinated-launch shape" sits to an accusatory framing.

### creator-sale-2xhg — looks acceptable
Same mint and same gap as `incomplete-read-funding` under a different label; leads with the
unresolved funding gap ahead of the 20.2231 SOL creator cash-flow figure, and explicitly
declines to draw a conclusion from the cash-flow number alone ("that alone does not
establish where the tokens went"). Correct hedge for a `CantTell` case; no accusation.

## What was not done

- No second capture pass 8 minutes later: neither `CantTell` case had a clock ("Deadline")
  gap — both were the funding-search gap — so the packet's own rule for when a second pass
  is required did not apply.
- No separate captures of `JB2rSPb4W4bnnr5HwQ17JPTi7gMbvdhjgJUE2oQbpump` or
  `EYPSU1oha6ELaZ4wN1crMcdnXDb21S6LWkJXohs7pump` beyond the ones already in the 11-case set:
  `creator-sale-2xhg` fired `CreatorSoldOut` on the first read (once the JSON-nesting misread
  above was corrected), so the packet's fallback condition was never triggered, and both
  named fallback mints were already covered as `clean-read-versioned-tx` / `clean-read-pay`.
- No code changes, no commit, no push, no PR — all output here is left uncommitted for Josh
  to review and commit.
- No read, print, or copy of `/etc/realorrug/*.env` or any key-bearing file; the Helius RPC
  URL was never printed. Both the VPS-side and workstation-side copies were grepped for
  `api-key`/`api_key`/`Bearer `/base64-looking key fragments before and after transfer, with
  no matches.
- `realorrug-serve` and `/etc` were not touched; nothing under the systemd unit changed.
