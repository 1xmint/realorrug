<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0020 — the Robinhood fact sheet, the verdict, and the voice

**Status:** **draft, not yet reviewed with Josh.** This is the plan
[ADR 0027](../adr/0027-the-bot-gives-informed-verdicts-from-evidence.md) says it is
waiting for: what a Robinhood Chain fact sheet holds, how code picks a
verdict level from it, and what the model may say. Nothing in it is built.
**Amends:** nothing. **Consequence of:** ADR 0027 (the verdict ladder and the
model/code split), [research 0041](../research/0041-rules-or-a-reasoning-model-for-token-risk.md)
(rules build the sheet, a cheap model writes, a template is the floor),
[research 0039](../research/0039-robinhood-chain-data-on-a-budget.md)
(JSON-RPC only, no indexer, no vendor risk API), and research 0042,
"detection intelligence we left in Radar" (the signal port order; wash
trading, liquidity spoofing and a composite score are refused) — **not yet
merged to `main`** as of this writing, present only on
`origin/docs/0042-radar-intelligence`, so it is cited by name and section
here rather than linked, and this document does not touch that file.
**Date:** 2026-09-15.
**Facts from:** [research 0036](../research/0036-pons-v2-read-from-a-real-launch.md),
[0038](../research/0038-pons-v2-creators-and-outcomes-read-over-a-range.md),
[0040](../research/0040-pons-v2-graduation-price-and-other-launchpads.md).
Every chain number below cites one of these three by section; a number with
no citation is not a chain fact and is written as an open question instead.
**Does not decide:** the read memory (design 0021, not yet written), threaded
follow-ups (design 0022, not yet written), the public checker page (design
0023, not yet written — note this is a different document number than
[ADR 0023](../adr/0023-realorrug-lives-on-robinhood-chain-and-the-bot-moves-with-it.md),
which already exists), or the bytecode owner/mint/pause checks (research
0044, in flight). See §7.

## Do not re-litigate

These came from the owner or from merged research, and this document writes
them up, not reopens them:

- **ADR 0027**: code picks the verdict level from evidence; the model writes
  the words and may not move the level; a verdict describes a token or a
  launch, never a named person's intent; a single signal never reaches the
  top two levels (point 4).
- **Research 0041**: no reasoning model per reply. Code builds the sheet and
  the memory, rules decide when to dig deeper, a cheap model writes only, a
  template is the fallback. A frontier model proposes rules offline, never in
  the reply path.
- **Research 0039**: JSON-RPC only, against QuickNode's Build plan ($49/month
  flat, §7 of that document). No indexer, no vendor risk API in the hot path.
- **Research 0042**: the port order is repeat-launcher prevalence first, then
  the calibration/drift monitor, then a typed bundle verdict, then re-deriving
  creator history on Pons v2 data, then rolling post-launch bundling last.
  Wash trading and liquidity spoofing are not designed here — they were an
  outside brainstorm Radar never built or measured (0042's table, "Wash
  trading detector" and "Liquidity spoofing / removal detector" rows). No
  composite risk score, in either repo, on purpose (0042's table, "Composite
  / single risk score" row, quoted there: *"A single safety score. Radar has
  fourteen reason codes and a structural split. A green shield is 'unknown
  rendered as safe'"*).

## 1. What the sheet holds for a Robinhood token

`realorrug-roast/src/sheet.rs`'s `FactSheet` and `Fact` types (mint, read_at,
facts, untrusted, unknown, signals) are venue-agnostic already — nothing in
the type itself names Solana. Two fields carry Solana-shaped names that this
design must resolve, not restate as if they were neutral:

- **`mint: String`.** Kept as `String`, not renamed. A `FactSheet` is built
  from a resolved `Dossier`, and by the time one exists the venue is already
  known (§2) — the sheet does not need to re-discover it, only to print it
  back for the render and the log. Renaming it to something venue-neutral
  like `subject` would touch every builder function in `sheet.rs` for no
  behaviour change; the cost of *not* renaming is a field name that reads as
  Solana-specific to a new contributor, paid once, in a doc comment.
- **`read_at: Option<Slot>`.** `Slot` (`realorrug_types::Slot`) is a Solana
  slot number. Robinhood Chain has no slots; it has blocks, at 0.1019
  s/block (research 0038 §4), nine times Ethereum L1's rate. This field
  **is renamed**, not generalised: `read_at: Option<u64>` with a doc comment
  saying "a Solana slot or a Robinhood block number, whichever the dossier
  was read from" is a silent unit change with the same Rust type on both
  sides, and a reader comparing two sheets from different venues would have
  no compiler help catching the mistake. The cost is one field-rename commit
  across `sheet.rs`, `verdict.rs` and wherever `read_at` is logged; it is
  paid once, and it is the same shape of trade `LaunchedToken`'s fields
  already made (Address, not Pubkey).

**Price stays dropped the same way.** `FactSheet::build`'s `withhold_price`
runs last, after every `push_*` call, and compares `self_mint == Some(&dossier.mint)`
on the parsed address. For Robinhood this is unchanged in shape: `self_mint`
becomes `Option<&realorrug_robinhood::Address>` (or a `Venue`-typed
self-mint, §6) rather than `Option<&realorrug_types::Address>`, because
[ADR 0023](../adr/0023-realorrug-lives-on-robinhood-chain-and-the-bot-moves-with-it.md)
decision 2 put realorrug's own token on Robinhood Chain permanently. No
Robinhood fact proposed below is tagged `About::Price` — every figure is
structure, history, depth or population, same as today — so `withhold_price`
has nothing to drop for a Robinhood sheet yet, same as it has nothing to drop
for a Solana one today. The mechanism exists so the first price fact anyone
adds is caught by construction, not by a reviewer remembering AGENTS.md §3
rule 5.

### The table

Selectors: `getLaunchedToken` `0x3cf28b5a`; `getReserves`/`quoteReserve`/
`tokenReserve` `0x0902f1ac`/`0x9da771f4`/`0xcbcb3171`; `balanceOf(address)`
`0x70a08231` (research 0040 §3, confirmed against the well-known ERC-20
selector). Costs: QuickNode charges a flat 20 credits per method on
Ethereum-shaped chains, Robinhood not separately listed so the same rate is
inferred (research 0039 §2 — flagged there as inferred, repeated here as
inferred, not upgraded to fact). Alchemy: `eth_call` 26 CU, `eth_getLogs` 60
CU, `eth_getBlockByNumber` 20 CU (research 0039 §2, Alchemy's own
compute-unit-costs page).

| fact | read from | calls | QuickNode credits | Alchemy CU | required? |
|---|---|---|---|---|---|
| launch record: curve, deployer, creator fee recipient, pair, graduation threshold, creator tax, buyback | `getLaunchedToken(token)` on `FACTORY` (`0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e`), word-decoded per `pons.rs`'s `LaunchedToken::from_return` | 1 `eth_call` | 20 | 26 | **required** |
| phase (0 = not graduated, 2 = graduated; 1 never observed, 0040 §1) | word 10 of the same `getLaunchedToken` return — **not currently extracted** by `LaunchedToken::from_return` (see §6) | 0 extra (same call) | — | — | **required** |
| launch block and age | `TokenLaunched` log for this token (topic `0x8d4aad49…`, token is topic1, confirmed indexed in the signature research 0038 §1 quotes), then `eth_getBlockByNumber` on its block for the timestamp | 1 `eth_getLogs` + 1 `eth_getBlockByNumber` | 40 | 80 | **required when it can be read** — `NothingUglyYet` must account for the age (§4), and a sheet that has no age reaches that level only by saying so and naming its read point instead; §4 records why demoting it was rejected |
| graduation: whether, when, quote raised | factory log topic `0xcdb72f15…`, one indexed field (token), decoded quote-raised and token-transfer-to-factory words (research 0040 §2) — needed only once `phase == 2` | 1 `eth_getLogs`, bounded to the factory address and (launch block, now) | 20 | 60 | optional |
| curve reserves and progress toward 4.2 ETH | `getReserves()` on the curve (`quoteReserve`/`tokenReserve`, confirmed identical to the two named getters, research 0040 §3) | 1 `eth_call` | 20 | 26 | **required** |
| holder count and largest non-curve holder's share | sum every `Transfer` log for the token (topic `0xddf252ad…`), skip the zero-address mint (research 0040 §4) | 1 `eth_getLogs` (chunked if the 10,000-log cap is hit, research 0038 §4) | 20+ | 60+ | **required** |
| creator's own current balance | `balanceOf(creator_fee_recipient)` on the token | 1 `eth_call` | 20 | 26 | optional |
| creator bought in the launch block | `CurveBuy` logs from the curve, bounded to the launch block; the launch record's own launch transaction already shows this for the *deployer* (research 0036 §3: a launch can carry a buy in the same tx, exempt from snipe tax) | 1 `eth_getLogs`, one-block range, cheap | 20 | 60 | optional |
| creator has sold out | creator's own balance (above) is zero, **and** the creator bought in (or was credited by) the launch block | reuses the two reads above | — | — | optional |
| launch-block recipient count | distinct non-zero recipients of `Transfer` logs in the launch block | 1 `eth_getLogs`, one-block range | 20 | 60 | optional |
| trade counts and unique traders | `CurveBuy`/`CurveSell` logs on the curve, from launch to now (research 0036 §2's `Trade` type) | 1 `eth_getLogs`, cost scales with trade count; can hit the 10,000-log cap on a very active curve (research 0038 §4) | 20+ | 60+ | optional |
| creator track record | our own index, keyed on `creator_fee_recipient` per research 0038 §2's recommendation | 0 (local read) | — | — | optional |
| simulated sell / `BuyersCannotSell` | `eth_call` a sell against the curve at a fixed size, reverts or not (§3) | 1 `eth_call` | 20 | 26 | optional |

**Recommended required set: launch record, phase, launch block/age, reserves,
holders.** This differs from the owner's starting position (launch record,
phase, reserves, holders) by one line: **launch block and age**. The reason
is a rule already in §3's ladder, not a new preference — `NothingUglyYet`
"must carry the 'yet' and must state the token's age" is the owner's own
requirement (packet §3), and a level cannot state a fact its sheet did not
read. Making the read required rather than optional means a token whose
launch log cannot be found (state pruned past ~57,000 blocks back for
`eth_call`, research 0036; but the log itself is not pruned, research 0038
§4 — `eth_getLogs` served logs across 80% of chain history cleanly) falls to
`CantTell` rather than silently reaching `NothingUglyYet` without an age
line, which the forbidden-phrase check (§5) would then have nothing to catch
because the missing age is an omission, not a forbidden word. The cost is
two more calls (80 credits, 140 CU) on every sheet, not only on ones that
turn out `NothingUglyYet` — cheap next to the holder scan, and the read was
free reuse of the same `TokenLaunched` log research 0038 §1 already indexes
for the whole factory.

**Graduation's own detail (when, quote raised) stays optional**, on the
owner's side of the line: `phase == 2` from the required launch-record read
already answers "whether", cheaply; "when, and how much was raised" is a
second call that only a graduated token needs and that no signal in §3
depends on.

**Implementation status (2026-09-17, packet "Robinhood: required facts,
part A").** Two bugs this section's table implied are fixed, ahead of the
reads themselves: `FactSheet::build` (`realorrug-roast/src/sheet.rs`) now
skips `push_cost`'s Solana/pump.fun base-rate line and
`push_measured_population`'s Solana-watermarked population line for a
Robinhood-mint dossier (`dossier.mint`'s own `ChainAddress` tag decides it,
not an argument the caller could omit), and the `dossier.unavailable` loop
that turns a miss into a trusted `unknown` sentence now skips the three
fact names this table marks **optional** (`capacity`, `fees`, `creator
transactions`) rather than reporting every miss as if it were required —
`realorrug_onchain::robinhood::build` still records each on `Dossier` for
the operator, only the public rendering treats an optional miss as
unremarkable. **Launch block and age, holders, `LiquidityGone` and
`CreatorBoughtOwnLaunch` (parts B and C of that packet) are not built yet**:
`realorrug_onchain::robinhood::build` still has no read for any of them, so
every Robinhood token reaches `CantTell` today on the unconditional "the
launch block could not be read" line in `FactSheet::build` — this section's
required set is the target, not yet the behaviour.

## 2. Dispatch by address shape

`crates/realorrug-analyst/src/mention.rs`'s `first_address` today scans a
mention for the first base58 run of address length and returns it as
`Asked::Mint(String)` — Solana-shaped only. `realorrug_types::Address::from_str`
(`crates/realorrug-types/src/address.rs`) decodes base58 into 32 bytes;
`realorrug_robinhood::Address::from_str` (`crates/realorrug-robinhood/src/lib.rs`)
decodes `0x` plus 40 hex digits into 20 bytes. Both already exist; nothing new
needs writing to parse either shape, only to choose between them.

A `Venue` type, in `realorrug-analyst` (it is a dispatch decision, not a fact
about either chain):

```
enum Venue {
    Solana(realorrug_types::Address),
    Robinhood(realorrug_robinhood::Address),
}
```

`mention::read` generalises `first_address` to scan for the first run,
anywhere in the text, that matches **either** shape — `0x` followed by
exactly 40 hex characters, or a base58 run of 32–44 characters using none of
`0`, `O`, `I`, `l` — and returns whichever comes first in the text, parsed
into the matching `Venue` arm. This is the same principle the existing
function already applies to one shape ("first address wins"), generalised
to two: no new precedence rule is invented, the existing one is widened.

`answer.rs` and `daemon.rs`'s dispatch point (`realorrug_onchain::build`,
called unconditionally at `answer.rs:175` and `daemon.rs:885`) becomes a
match on `Venue`: `Venue::Solana(mint) => realorrug_onchain::build(...)`
unchanged, `Venue::Robinhood(token) => realorrug_robinhood::build(...)` (a
new function, §6, symmetrical to the existing one — reads `getLaunchedToken`,
the launch log, reserves and holders per §1's table and returns a
Robinhood-shaped `Dossier`). `FactSheet::build` itself does not need to match
on `Venue`: it already takes a `Dossier`, and a Robinhood dossier is a second
constructor for the same struct shape (or, if the fields genuinely diverge —
recipients-in-token-accounts vs. recipients-in-addresses, slot vs. block —
a second `Dossier` variant with its own `push_*` functions in `sheet.rs`,
decided in §6).

**Two addresses of different shape in one mention.** The generalised
`first_address` already answers this the same way today's single-shape
version answers "two Solana addresses in one mention": whichever run starts
earlier in the text wins, and the other is not looked at. No new rule is
introduced for the two-shape case; the existing "first wins" rule is simply
evaluated over a wider set of candidate runs. A mention naming both is
answered about the one named first, with no signal to the reader that a
second address was present and ignored — the same silence today's code
already has for "two Solana addresses, first wins."

**A string valid on both shapes — proof it cannot happen, in one sentence.**
A Robinhood address always begins with the two literal characters `0` then
`x`; the base58 alphabet Bitcoin/Solana addresses use excludes the digit
`0` entirely (it is the reason `0`, `O`, `I` and `l` are excluded — visual
ambiguity with `1`/`I`/`l` and `0`/`O` — and Solana's own decoder,
`bs58::decode`, rejects any string containing a literal `0` before decoding
a single byte), so no string that starts with `0x` can ever be a valid
base58 address, and the two shapes' character sets and lengths (`0x` + 40
hex vs. 32–44 base58) do not overlap in any string. The check is total
because the two patterns are checked as an exclusive `if`/`else if` over
disjoint prefixes, not as two independent regexes that could both match.

**Anything else** — too short, too long, the wrong alphabet, no `0x` and not
base58-clean — is neither, and `Asked::Nothing` (or a new `Asked::NeitherShape`
if the reply should say *why* rather than just that nothing was found) means
the reply says the address shape was not recognised, never that the token
was silently treated as Solana or Robinhood by default. Rule 8 (deny by
default when config, or here identification, is missing).

## 3. The verdict ladder, decided by code

### The signal set for Robinhood

| signal | measures | read | threshold, and where from | innocent twin |
|---|---|---|---|---|
| `LiquidityGone` | reserves no longer support an exit | `getReserves()`'s `tokenReserve` at or near zero while holders still hold supply | `tokenReserve == 0` post-graduation is the curve's *normal* end state (research 0040 §3 — a graduated curve reads `tokenReserve = 0` by design, not by draining), so this signal fires only **pre-graduation**, when `tokenReserve` collapses while `phase == 0` | every buyer sold back to the curve — research 0040 §4 observed exactly this on a real token, 71 `Transfer` logs summing to zero external holders |
| `CreatorSoldOut` | the creator held and now holds nothing | `balanceOf(creator_fee_recipient)` reads 0, and the creator was seen holding a nonzero balance at some earlier read (from this bot's own memory, design 0021, or from the launch-block buy) | zero, compared against a prior nonzero read; **needs a prior observation to mean anything**, which is design 0021's job (§7) — on a first-ever read this signal cannot fire, only "creator currently holds nothing," which is a different, weaker claim | moved to another wallet the creator still holds them in ("never held in the first place" is not this signal's twin: it only fires after a nonzero read, so that case never reaches it) |
| `BuyersCannotSell` | a simulated sell reverts | `eth_call` a sell against the curve (`CurveSell`-shaped calldata) at a fixed size, pinned to the current block, checking for a revert rather than sending a transaction | fixed simulated size TBD — no chain fact sizes this yet (open question, §8); a revert on **any** size above dust is the strong form, a revert only above some size is the weak form and needs a sweep, not one call | our simulated size or slippage tolerance was wrong, not the curve's mechanics — a curve with real but thin depth reverts a large sell the same way a broken one does; or the launch is still in its opening seconds, when Pons v2's own snipe tax makes every token's sell fail (research 0044) |
| `CreatorBoughtOwnLaunch` | creator among launch-block recipients | `CurveBuy` logs in the launch block naming the creator/deployer address, or the launch record's own launch-transaction buy (research 0036 §3) | any nonzero buy by the creator in the launch block | a creator buying a token they believe in — research 0036 §3's own captured launch was exactly this, a dev buy that paid full fees and no snipe tax because the factory exempts the launcher automatically |
| `LaunchBlockBundle` | launch-block distinct recipient count lands in the strongest measured band | distinct non-zero-balance recipients of `Transfer` logs in the launch block, looked up in a measured snapshot's bands (the same `baserates.rs::band_for` shape, re-derived for Pons v2 — **not yet measured**, §8) | **read from the snapshot, never a constant** — research 0042's first lesson, quoted there: *"Six is a tool's default, not a law. The number will move when whoever is running this changes their configuration, and the detector will go quiet without saying so"* (0008, quoted in 0042), which came true in 0024's re-measurement | a launch people were waiting for — no chain fact distinguishes a bundle bought by insiders from a bundle bought by fans; only the *rate* at which each recipient count precedes an outcome, measured, does |
| `RepeatLauncher` | this creator has launched many more tokens than the creators around them | the creator's **lifetime** launch count, as the creator index already records it — research 0042's port-order item 1, and *simpler on EVM than on Solana* per that document's table, because an EVM sender address recurs directly, no wallet-to-token-account join needed. Not a rolling window: §7's original plan to ask the read memory for launches-in-the-last-N-minutes was dropped, because the index holds the lifetime count already and a window would be a second, differently-shaped count of the same thing | **the index's own 95th percentile**, nearest-rank (`((n - 1) * 95) / 100` on the ascending sort), computed **after** the named first-party list is excluded, floored at 2, and refused outright under 100 remaining creators — `creator::CreatorIndex::repeat_launcher_floor`, whose doc comment carries the argument for each of those three numbers. **Never Radar's `REPEAT_FLOOR`/`INFRASTRUCTURE_FLOOR` (3, 100)**: those count distinct launch *blocks* in a *90-minute Solana window*, a different population on a different chain, and importing them would look like a measurement while being one | a bot that buys every launch; infrastructure, not coordination — 0042's own table excludes 13 known router/fee-sink addresses covering 42% of Solana launches from this count. The named list (`docs/research/data/first-party-addresses.json`) is the Robinhood equivalent and now exists, but it holds only **named** addresses: an unnamed relayer nobody has captured yet still reads, on this signal alone, exactly like a person launching forty tokens. That is the signal's honest limit |
| `CreatorNeverGraduatedOrganically` | already a `Signal` variant (`sheet.rs`) | measured launches with none organic, from the creator index | reuses the existing Solana logic unchanged in shape: `record.measured > 0 && record.organic == 0` | small sample — a creator with two measured launches and zero organic has a different confidence than one with fifty, and the sheet states the denominator (already true today, `push_creator`) so the model can read it |
| `HolderConcentration` | largest non-curve holder's share | `Transfer`-log sum (research 0040 §4), largest balance divided by circulating (total minus curve) | threshold **not yet measured** for Pons v2 — no distribution of real holder-concentration outcomes has been gathered (open question, §8) | a vesting contract, a bridge, an exchange — none of which this design can currently tell apart from a whale, because no label list for Robinhood-chain contracts exists yet (the EVM equivalent of research 0042's "exchange hot wallet, bridge contract, known deployer" label-list idea, itself flagged there as needed and not yet built for Solana either) |
| `OwnerCanStillMintOrPause` | owner-only mint/pause/blacklist selectors present and ownership not renounced | deployed bytecode scan, or a verified-source ABI read | **blocked on research 0044**; this design names the slot the signal fills (a `Signal` variant, a fact pair "owner address" + "selector present") without designing the bytecode check itself | a stock template with an owner nobody uses — most ERC-20 templates ship an `Ownable` ancestor whether or not the deployer ever calls it |

### The levels, as a rule over that set

- **`Rugged`** — an observed, completed event, never on one reading alone:
  `LiquidityGone` confirmed by a second fact (holders still hold supply they
  cannot exit — i.e. `HolderConcentration`'s numerator is nonzero while
  `LiquidityGone` fired), **or** `CreatorSoldOut` together with
  `BuyersCannotSell`.
- **`RugMechanicsLive`** — the token is live (`phase == 0`, still on the
  curve) and **two or more** of the live-risk signals (`LiquidityGone`,
  `CreatorSoldOut`, `BuyersCannotSell`, `CreatorBoughtOwnLaunch`,
  `LaunchBlockBundle`, `RepeatLauncher`, `HolderConcentration`,
  `OwnerCanStillMintOrPause` once it ships) fire together.
- **`Sketchy`** — at least one signal, no qualifying combination.
- **`NothingUglyYet`** — every required fact read (§1's five), no signal
  fired. The reply must carry the "yet" and must state the token's age
  (§1's launch-block/age read, now required for exactly this reason).
- **`CantTell`** — any required fact unread.

### Precedence: `Rugged` versus `CantTell`

**When a required fact is unread but a `Rugged`-qualifying pair was
observed, `Rugged` wins.** Agreeing with the owner's starting position, for
the owner's stated reason and one more from the chain facts: an observed
completed rug (`LiquidityGone` + `HolderConcentration`, or `CreatorSoldOut` +
`BuyersCannotSell`) is built entirely from *optional* facts in §1's table —
holders, the creator's balance, a simulated sell — none of which overlaps
the *required* set (launch record, phase, launch-block/age, reserves). So the
only way this precedence question is ever actually reached is a sheet that
read enough to observe a completed rug but failed to read, say, the launch
log for age (a pruned-log edge case, or an RPC timeout mid-read, research
0038 §4's own `"log query timed out"`) — and downgrading that sheet to
`CantTell` would mean a reader asking about a token that has already,
verifiably rugged gets told "can't tell" because of an unrelated missing
timestamp. That is a worse failure than the one this ordering risks: a
`Rugged` sheet that also happens to be missing its age line still names the
observed facts that earned `Rugged` (§5: every reply must name at least one
fact that earned the verdict), so nothing false is published — only that
one *unrelated* absence (age) goes unstated in that reply, which is a
smaller loss than presenting an observed rug as merely uncertain.

### Worked examples, one per level, against real tokens from 0040

- **`Rugged`.** Token `0x8d55168977bf2f28eb20b3a61d6fc277084ff8d6` (research
  0040 §4): 71 `Transfer` logs summing to **zero external holders**, curve
  the only nonzero balance. If this token is still pre-graduation
  (`phase == 0`) when read, `LiquidityGone` fires (reserves cannot support an
  exit for anyone, because there is no one left to exit) — but 0040 §4's own
  framing of this exact token as "the innocent twin of liquidity gone" (its
  language, echoed in the packet) means the design must not fire `Rugged`
  from `LiquidityGone` alone here: it needs the second confirming fact,
  `HolderConcentration`'s numerator nonzero — which this token does **not**
  have, since there are zero external holders to concentrate. **So this
  worked example is deliberately the twin, not the rug**: with zero external
  holders and reserves collapsed, the sheet has no second holder to point
  to, `LiquidityGone` fires alone, and the ladder correctly stops it at
  `Sketchy`, not `Rugged` — the token everyone sold back to the curve on
  their own, which is what "no combination required" is for. A genuine
  `Rugged` worked example needs a token where the sheet can show reserves
  gone **and** a holder still stuck — no such token is captured in 0036,
  0038 or 0040 today (open question, §8: this design cannot cite a real
  Robinhood `Rugged` example from the research on file, and says so rather
  than inventing one).
- **`RugMechanicsLive`.** Token `0x63ae1e0f1a756ec4947f3f06efb09c588984ed88`
  (research 0040 §1, §2, §4): graduated (`phase == 2`, confirmed live), five
  holders summed exactly against `balanceOf` (0040 §4), largest holding
  861,769,343,136,162,414,596,698,110 raw units against a 1,000,000,000-token
  (18-decimal) supply — roughly 86% of a ~1.0 supply once decimals are
  accounted for, which would fire `HolderConcentration` at any reasonable
  threshold once one is measured (§8, threshold not yet set). If this token
  additionally shows `CreatorBoughtOwnLaunch` (not checked in 0040), two live
  signals would qualify it for `RugMechanicsLive`; on the facts 0040 actually
  captured (one strong concentration reading alone), it would sit at
  `Sketchy` until a second signal is confirmed — stated here as what the
  rule requires, not as a claim this document is making about that real
  token, which was not fully read for this purpose.
- **`Sketchy`.** The zero-external-holders token above,
  `0x8d55168977bf2f28eb20b3a61d6fc277084ff8d6` (0040 §4): one signal
  (`LiquidityGone`, uncombined), the twin openly acknowledged, no second
  fact to confirm it — exactly the shape `Sketchy` exists for.
- **`NothingUglyYet`.** The clean launch research 0036 §4 captured,
  `0x2a43738c…` (block 62,356,247): sent straight to the factory with only
  the 0.0005 ETH launch fee, mints 1,000,000,000 tokens to the curve and
  nothing else, exempts only the deployer (who is also the fee recipient)
  from the snipe tax, holds no trade. If reserves and holders read cleanly
  and no signal fires, the reply states `NothingUglyYet` with the token's
  age from its launch block's timestamp (§1) — carrying the "yet." Where
  the age is not readable, §4's tier two carries the "yet" instead, by
  naming the read point and saying the age is unknown.
- **`CantTell`.** Any token whose `getLaunchedToken` call lands on a pruned
  block boundary or times out — research 0036 measured `eth_call` state
  pruned past ~57,000 blocks back (`"metadata is not found"`), and research
  0038 §4 measured `eth_getLogs` itself timing out on a wide range
  (`"log query timed out"`) independent of pruning. Either failure, on any
  of the five required facts, is `CantTell` by the rule above — not
  presented as clean, per ADR 0027 point 3 and consequence 4.

## 4. The voice

**Corrected by [ADR 0028](../adr/0028-one-bot-every-chain.md), 2026-09-15**:
this section's recommendation below is now the decision for every chain, not
only Robinhood — see that ADR for what changes and why, not restated here.

**Architectural note, stated once here rather than re-argued in every
rule below.** Today's Solana voice pass (`voice.rs`) does not let the model
write free text: it asks the model to *select* among Radar's own
pre-written clauses (`F3.blunt`, `F1.plain`, …, `crate::clause`), and
`forbidden::check`/`fidelity::check` run on the *assembled* selection. That
is the safest possible implementation of "the model writes, code checks,"
because a clause the model did not write cannot contain a forbidden phrase
or a fabricated number by construction — but it also means every Solana
reply is built from sentences a person wrote in advance, which cannot be
witty about *this specific token* the way the owner's target voice needs to
be (packet §4: "witty, natural, meme-worthy... a verdict every time"). This
design recommends the Robinhood voice pass move to genuine free-text
generation — the cheap model writes its own sentences from the rendered
sheet, exactly as `voice.rs`'s own header table already describes the
intended split ("what the headline is, what matters, the framing, the tone
| **the model**") — because `fidelity::check` and `forbidden::check` already
operate on arbitrary rendered text (`fidelity::literals(text)`,
`forbidden::check(reply: &str)`), not on the clause-selection mechanism
specifically: nothing in either check requires the text to have come from a
fixed clause list. The cost is real and is named plainly: free text is
harder to keep witty *and* safe than a vetted clause is, which is exactly
why §5 exists as a harder check than a word list. **This was a
recommendation when this document was drafted; [ADR 0028](../adr/0028-one-bot-every-chain.md)
now settles it, for both chains, as free text everywhere** — Solana's
clause-selection mechanism is retired as a live generation path, not kept
beside free text, and `clause.rs`'s sentences become the fallback template's
source material instead. The signal set and verdict ladder in §3 are
unaffected either way, because they feed the sheet the model reads, not the
generation mechanism itself.

### Length, and why

**Under 280 characters, one to three sentences.** It posts to X (the
platform, formerly Twitter — realorrug's own account posts there per design
0019). A reply that does not fit gets truncated by the platform, mid-fact,
which is worse than a shorter reply chosen on purpose.

### Register per level

- **`Rugged`** — blunt, funny, short. The joke can land hard because the
  facts already did the work.
  > "Liquidity's gone and the eleven people still holding this can't get out
  > — the curve's empty and there's nobody left to sell to. Rugged. 4.2 ETH
  > came in, zero is going back out."

  > "Every single buyer sold back to the curve except the ones who can't:
  > reserves are dry, holders are stuck. That's not a dip, that's the exit
  > being welded shut. Rugged."
- **`RugMechanicsLive`** — still funny, but with an edge of "watch this
  closely," not "this already happened."
  > "One wallet's sitting on 86% of supply and the creator bought their own
  > launch in the same block. Neither one alone means much. Together, on a
  > token that's still live? That's two red flags doing yoga together.
  > RugMechanicsLive."

  > "Live token, two signals firing at once: the creator's still holding a
  > chunk they bought at launch, and one non-curve wallet owns most of what's
  > left. Could be nothing. Could be everything. Watching this one."
- **`Sketchy`** — dry, a little wry, but honest that the innocent read is on
  the table.
  > "Every buyer on this one sold back to the curve — could be a coordinated
  > bail, could just be a token nobody wanted twice. Reserves are thin either
  > way. Sketchy, not sunk."

  > "Zero holders outside the curve. That's either a graveyard or a crowd
  > that changed its mind together. Can't tell you which from here. Sketchy."
- **`NothingUglyYet`** — warm, genuinely, with the "yet" doing real work and
  the age accounted for, either stated or named as unreadable (§4).
  > "Six hours old, reserves intact, nobody's dumped, nobody's stuck. Clean
  > so far — emphasis on *so far*, it's had six hours to be clean in."

  > "Launched this morning, curve's healthy, creator hasn't touched a thing.
  > Nothing ugly yet. Ask me again in a week."
- **`CantTell`** — plainly uncertain, names the gap, no verdict word
  stronger than the honest shrug.
  > "Couldn't read this one's holder list — the RPC timed out mid-scan. Not
  > saying it's fine, not saying it's not. Can't tell yet, try me again in a
  > bit."

  > "Launch record's there but the reserve read failed twice. That's a gap
  > in what I can see, not a clean bill of health. Can't tell."

### Every reply names at least one fact that earned the verdict

A verdict with no evidence line is refused (§5's fidelity/level checks
enforce this structurally: a reply that names no fact has, by construction,
no number the sheet authorised beyond the slot/block, which is thin enough
to be worth its own check — named as an open question in §8, since today's
`fidelity::check` does not yet enforce "at least one fact cited," only "no
uncited number").

### `CantTell` and `NothingUglyYet`'s required lines

`CantTell` must say *what* could not be read — "the holder list," "the
reserve read," not a bare "can't tell." That is enforced the same way §1
makes a miss a required line: the sheet carries the `unknown` list before
the model can be asked to say it.

**What counts as saying it, measured 2026-09-17.** The check first asked for
the sheet's phrase minus " could not be read" as one literal run of
characters, so "the holders" matched and "holders couldn't be read" did not.
That refused the only Robinhood reply the bot had written. It now asks for
every content word of the phrase to appear somewhere in the reply, dropping
"the", "a", "an", "of" and a trailing "'s" -- so a reply may reorder the
words and drop the article, but a reply that names half the topic
("the creator", when the sheet missed the creator's history) is still
refused. It deliberately does not stem: matching "holder" to "holders" would
also match "holding", and a check that lets the wrong sentence through is
worse here than one that makes the model say the plain noun.

`NothingUglyYet` must account for how old the token is, and **there are two
ways to do that, because the chain gives us two different things.** Both are
enforced by `forbidden::check_required_age`, which reads the level and the
sheet together.

**Tier one, the age is on the sheet.** The launch-block read produced a
`Kind::Age` fact — "63954 slots (about 7.1 hours) since its launch block."
The reply must use an age word (*ago*, *old*, *hour*) **and** state a number
the age fact itself carries. The read point's own number is deliberately not
in the set of numbers that satisfy this: a slot number is not an age, and
counting it would let "read at slot 444007820" pass as "six hours old."
Those are different claims and only one of them is true.

**Tier two, the age could not be read.** A Robinhood sheet carries an age
when its reader found the token's `TokenLaunched` log: the Robinhood reader
fills `Dossier::chain_launch` with the difference between the launch
block's timestamp and the read block's, and `sheet.rs`'s
`push_chain_launch` states it in hours (days past two). Before that reader
existed every Robinhood sheet was ageless, which is why this tier was
written with a whole chain in mind; it now covers the sheets whose launch
log or block timestamp could not be read. The reply
must then do three things, all of them: say in words that the age is not
known ("how old this token is could not be read"), name the read point in
words (*slot*, *block*, *read at*), and state the read point's own number.
Saying "clean so far" and stopping is refused; so is quietly leaving the age
out and hoping nobody notices what is missing.

Tier two is only satisfiable because `FactSheet::render()` prints the read
point, and `render()` is the model's entire world — `voice.rs` hands it
`format!("Token: {mint}\n\n{}", sheet.render())` and nothing else. A rule
that requires the reply to state a number the model was never shown refuses
every reply that could ever be written, which is not a strict rule but a
broken one: it would have taken the free-text voice silently off a whole
chain, falling back to the template on every Robinhood `NothingUglyYet`
with nothing anywhere saying so. The sheet must carry a line before the
check may require it. That is the same discipline as `CantTell`'s
`unknown` list, and it is why the two tests that pin the read point into
`render()` sit next to the tests for this rule.

**A sheet with no read point at all fails outright**, at either tier: with
no clock to anchor to, a "nothing ugly yet" is a claim about a moment we
cannot name (AGENTS.md rule 8 — absent is not zero, unknown is not safe).

**The alternative that was considered and rejected: demote every ageless
token to `CantTell`.** It is the tidier rule — the age is a required fact,
the fact is missing, so the honest verdict is "can't tell." It was rejected
because, when it was decided, on Robinhood the age was missing *by default*, not by accident. Every
Robinhood token would land on `CantTell` no matter how much the reader did
see: the reserves, the holder spread, the creator's history, the launch
block itself, all read successfully and all thrown away over one timestamp
the chain does not publish. That trades a whole chain's worth of real
evidence for a uniformity the reader never had, and it would make the
account's most common Robinhood reply a shrug about tokens it understood
perfectly well. The two-tier rule keeps the verdict the evidence earned and
makes the reply say plainly which clock it is reading against.

### Never a person

The line, with examples on both sides:

- **Allowed** — an observed action, described: "the creator wallet sold
  everything in block 3." "The curve's reserves went to zero and stayed
  there." "One wallet holds 86% of supply." Each names a measured fact about
  the chain, not a claim about what anyone intended.
- **Refused** — an accusation of intent, aimed at a person, handle, account
  or company: "the dev scammed you." "This creator is a thief." "@handle
  rugged their own token." Each asserts something the chain cannot show
  (what someone meant to do) and aims it at an identifiable party.
- **The hard cases, both ways**: "the creator wallet sold everything in
  block 3" is allowed — it names an address and an observed transfer, and
  says nothing about why. "The dev scammed you" is refused for the same
  facts — same underlying event, different claim, because it asserts intent
  and character rather than describing a transaction. "This wallet has
  never sold a token it launched" is allowed (an observed pattern, stated as
  a pattern). "This creator has a pattern of scamming" is refused (the same
  pattern, relabelled as an accusation).

### What the model may not do

Move the level (§3 decides it, the model never sees `Signal` values or the
level name in its input — it sees only rendered facts, same discipline
`sheet.rs`'s doc comment already states: "the word 'signal' appears nowhere
the model reads"), add a number not on the sheet (`fidelity::check`), state
a price or market cap (§1, `About::Price`, no Robinhood fact is tagged this
way yet so none is available to state), or take an instruction from the
mention (AGENTS.md §3 rule 3 — the mention's text never reaches the model in
a system-prompt position; only the fact sheet and fenced untrusted strings
do, same mechanism `voice.rs`'s header already documents).

### The template fallback

Same floor as today (`verdict::template`): no provider configured, an
unreachable provider, a fabricated number, a forbidden claim, or an unusable
answer all ship the deterministic template instead — restating the sheet's
facts in a fixed order, never a model sentence. Whatever the generation
mechanism (clause selection or free text, above), the fallback path and its
triggers are unchanged from today's `voice::write`. The step-5 replay (an
existing mechanism this design does not change) shows the model's words and
the template side by side, so a reviewer can see what would have shipped
either way.

## 5. `forbidden.rs`: from a word ban to a level-and-target check

### The target check

A refusal of accusation aimed at a person, account or company. A
person-reference is recognised by shape, not by a name list: an `@handle`
pattern, the bare words "dev," "team," "founder," "creator" when followed
within a short window by an accusation-shaped word, a named company
(harder — no closed list exists; starts as "a capitalised multi-word run
immediately preceding an accusation word," accepting some misses), or a bare
`0x`-address used as the grammatical subject of a sentence ("`0x1234…` is a
scammer" vs. "`0x1234…` sold everything in block 3" — the second names the
same address as the subject of an *observed* verb, which is allowed; the
distinction is the verb, not the address). An accusation word near one of
these — "scam," "scammer," "thief," "stole," "fraud," "criminal," the
existing `RULES` entries that name a person's *character* rather than an
*action* — is refused when it co-occurs with a person-reference inside a
short window (a sentence, roughly). **False-refusal cost**: a sentence like
"the team's tokenomics are a scam" about the *design*, not a person, would
be refused under this rule as written, because "team" precedes "scam" — an
honest limitation, and the conservative direction (refuse and fall back to
template) is the one AGENTS.md rule 7 already prefers for missing
certainty. Estimating the exact false-refusal rate needs a corpus of real
generated replies to test against, which does not exist before this ships
(open question, §8).

### The level check

Each verdict level carries a ceiling; a word above it is refused regardless
of target. Two that matter most, exactly as the packet names them:

- **Nothing may call a `CantTell` token safe, clean, fine or legit.** These
  words (already in `RULES` today — "is safe," "looks safe," "legit," and a
  Robinhood-specific addition of "fine," "clean" outside the `NothingUglyYet`
  arm) are refused whenever the sheet's computed level is `CantTell`,
  regardless of what the model wrote — this is ADR 0027's consequence,
  quoted directly: *"A verdict on a token the analyst cannot fully read must
  be `CantTell`, never `NothingUglyYet`. Getting this backwards would turn a
  blind spot into an endorsement."*
- **"Rug"/"rugged"/"stole" sit at `Rugged` only.** A reply computed as
  `Sketchy` or below that contains "rugged" is refused, even though "rug" is
  also the account's own name-adjacent word (`OWN_NAMES` masking, unchanged,
  still needed) — the level gate is checked *after* the own-name mask, so
  "realorrug" is still never caught, but "rugged" used as a verdict on any
  level below `Rugged` is.

A full per-level lexicon (illustrative, not exhaustive — the closed set is
an implementation decision for the code, not fully enumerable in prose):

| level | words refused if present |
|---|---|
| `CantTell` | safe, clean, fine, legit, trustworthy, nothing ugly, healthy |
| `NothingUglyYet` | rug, rugged, stole, stolen, safe (unqualified — "yet" must qualify it), guaranteed |
| `Sketchy` | rug, rugged, stole, stolen, safe, clean, fine, legit |
| `RugMechanicsLive` | rug, rugged, stole, stolen |
| `Rugged` | (no level-word restriction above `Rugged`'s own vocabulary — this is the level where "rugged" is earned) |

### Kept and dropped, name by name

Today's `forbidden.rs` (`crates/realorrug-roast/src/forbidden.rs`) has three
categories relevant here:

- **Kept, unchanged in purpose**: the reassurance rules (`is safe`, `looks
  safe`, `legit`, `trustworthy`) fold directly into the level-check's
  `CantTell` row above — same words, now conditional on level rather than
  absolute. The advice rules (`should buy`, `ape in`, `100x`, `bullish`,
  price-prediction words) are kept unconditionally; ADR 0027 says nothing
  about advice, and AGENTS.md's advice rule is untouched by it. The
  `honeypot` entry — a **forbidden phrase today**, per 0042's table: *"a
  word list, not a scan for sell-blocking bytecode"* — is **kept as a
  forbidden phrase** until research 0044 ships an actual sell-blocking
  check; saying "honeypot" about a token this design cannot yet verify is
  exactly the unearned-verdict problem ADR 0027 exists to prevent.
- **Dropped**: the blanket, level-independent ban on "rug," "scam," "fraud,"
  "stole," "stolen," "criminal," "thief" — these become the target check
  (aimed at a person: still refused, always) and the level check (aimed at a
  token, below the level that earns them: refused; at or above it: allowed).
  The word is no longer refused *everywhere*; the claim it makes, checked
  against who it is aimed at and what level was earned, is.
- **`OWN_NAMES` masking** (`cabalhunter.org`, `realorrug`) is kept unchanged
  — it is orthogonal to both the old and new rule shapes, and Robinhood adds
  no new own-name.

### The transition

Until this code ships, the old word ban (`RULES` as written today) is what
is actually enforced, and the bot stays more conservative than ADR 0027
allows — ADR 0027 says exactly this, consequence section, and it is
unchanged by this design. **How the switch is made safely**: `forbidden::check`
gains the level/target logic as new functions (`check_target`,
`check_level`) beside the existing `check`, both tested independently
against the existing `RULES`-derived word set; the caller in `voice.rs`
(or its Robinhood-side new module) switches from `forbidden::check(text)` to
`forbidden::check_target(text) + forbidden::check_level(text, level)` in one
commit, with the old `check` kept and still exercised by its existing tests
so a regression in the new logic does not silently widen what ships. **What
proves the switch is safe**: a test that feeds the same corpus of past
replies (or a synthetic one built from `sheet.rs`'s test fixtures) through
both the old `check` and the new `check_target`/`check_level`, asserting the
new pair refuses everything the old one did that was aimed at a person or
above its level, and that the new pair's *additional* refusals (level
violations the old word ban never caught, because it had no concept of
level) are the ones ADR 0027 exists to add — i.e. a mutation-style
re-application of the old bug (temporarily disabling `check_level`) must
make a previously-refused-only-by-level case pass, proving the new check,
not the old one, is what is catching it.

### The twin's floor is the template, not a `forbidden.rs` check on the model

**Decided, packet 0038.** Every signal in §3's table gains a `twins` entry on
`FactSheet` (`sheet.rs`), one sentence per fired signal from an exhaustive
`match` on `Signal`, printed under its own heading in `render()` so it is
part of the model's own prompt. `verdict::template` — the floor every path
that cannot trust the model's reply falls back to — states at least one twin
at `Sketchy` and `RugMechanicsLive` (the two levels that are adverse and not
conclusive), states none at `Rugged` (an observed completed event, where
hedging would be false balance in the other direction), and has none to
state at `NothingUglyYet` or `CantTell` (no signal fired).

**Rejected: requiring the model's own free-text reply to name a twin,
enforced in `forbidden.rs`.** The alternative considered and set aside was a
`check_twin`-shaped function beside `check_target`/`check_level`, refusing
any `Sketchy`-or-`RugMechanicsLive` reply that did not mention an innocent
explanation. It fails for the same reason `forbidden.rs`'s existing checks
are all shape-based, not meaning-based: the check can confirm a *string*
appears, not that the model's paraphrase of it is honest. A reply that
quoted a twin's sentence back verbatim would pass; a reply that wrote a
better, more specific innocent explanation in its own words — the entire
point of §4's free-text voice — would look, to a string check, exactly like
a reply that invented one from nothing. Enforcing it would either accept
verbatim quoting (pushing every reply back toward the templated sameness §4
exists to avoid) or refuse honest paraphrase alongside dishonest omission,
with no way to tell the two apart from the string alone. The template
guarantees the floor unconditionally, because it is built from the sheet
directly rather than generated and then checked; the model, reading the
twins on its own sheet, is free to do better than the floor without a check
standing in the way of trying.

## 6. What changes, file by file

| file | what changes | why | shape |
|---|---|---|---|
| `crates/realorrug-robinhood/src/pons.rs` | `LaunchedToken::from_return` extracts word 10 (`phase`) — currently unread | §1's phase fact is required and this is where it lives today, unread | changed function body, no signature change |
| `crates/realorrug-robinhood/src/pons.rs` or a new module in the same crate | reserve reads (`getReserves`/`quoteReserve`/`tokenReserve`), holder-sum from `Transfer` logs, `balanceOf`, graduation-log decode, launch-block `Transfer`/`CurveBuy` scans | §1's table; none of these reads exist in this crate today, only the launch-record and trade/sweep decoding do | new functions, additive |
| `crates/realorrug-onchain/src/robinhood.rs`, new | a `build` function and a `RobinhoodReader` (`ChainReader` impl), symmetrical to `realorrug_onchain::build`/`SolanaReader`, assembling `realorrug-robinhood`'s reads into a `Dossier`-shaped value for a Robinhood token — **not** a `realorrug-robinhood`-side function; see "the crate boundary question" below, which this row no longer agrees with itself on | §2's dispatch needs a second `build` to call | new module, new function, new type |
| `crates/realorrug-roast/src/sheet.rs` | `read_at: Option<Slot>` renamed to a block-number-shaped field (§1); new `push_*` functions for the Robinhood-only facts (reserves, holders, graduation); `self_mint` becomes Robinhood-shaped or `Venue`-shaped | §1, and ADR 0023 decision 2 (the token lives on Robinhood now) | changed field type, new functions, changed constructor parameter |
| `crates/realorrug-roast/src/sheet.rs` | new `Signal` variants: `LiquidityGone`, `CreatorSoldOut`, `BuyersCannotSell`, `CreatorBoughtOwnLaunch`, `LaunchBlockBundle`, `RepeatLauncher`, `HolderConcentration` — one variant each, shared with Solana where the same name already exists, per [ADR 0028](../adr/0028-one-bot-every-chain.md) point 3 (a signal means the same thing on every chain; the read differing is the chain reader's problem, not a reason to fork the variant) | §3's signal set | new enum variants |
| `crates/realorrug-roast/src/verdict.rs` | `Verdict` gains a `level: Level` field (`Rugged`/`RugMechanicsLive`/`Sketchy`/`NothingUglyYet`/`CantTell`), computed by a new pure function implementing §3's rule over `sheet.signals` and `sheet.unknown`; `Verdict::from` keeps producing `reasons` (the restated facts) alongside the new level | **this is the type ADR 0027 names as the one that changes** | new field, new type (`Level`), new pure function; `reasons` unchanged |
| `crates/realorrug-roast/src/forbidden.rs` | new `check_target`, `check_level` functions per §5; existing `check`/`RULES` kept during the transition | §5, ADR 0027 point 6 | new functions, additive during transition |
| `crates/realorrug-roast/src/voice.rs` | one free-text generation path, shared by every chain, that sends the rendered sheet to a cheap-tier model and asks for prose, gated by `fidelity::check` and the new `forbidden` functions before publication; per [ADR 0028](../adr/0028-one-bot-every-chain.md) point 1, Solana's clause-selection call (`clause::parse`/`assemble` against a model answer) is retired rather than kept beside it | §4, ADR 0028 | changed function, not additive; existing `write`/`request_for` change shape |
| `crates/realorrug-roast/src/fidelity.rs` | unchanged | `literals`/`check` already operate on arbitrary text | none |
| `crates/realorrug-roast/src/baserates.rs` | a Robinhood-shaped analogue of `Band`/`BaseRates`, populated once a Pons v2 recipient-count distribution is measured (research 0042 port-order item 3) — **not part of this design's day-one scope**; §3's `LaunchBlockBundle` signal reads `None` (no signal fires) until this exists. `RepeatLauncher` no longer waits on it: its floor is measured from the creator index's own distribution at the moment it is used, so there is no snapshot to publish and nothing to keep in step | §3, §7 | new type, additive, not built here |
| `crates/realorrug-roast/src/creator.rs` | reused as-is; keyed on `creator_fee_recipient` per research 0038 §2, same `CreatorIndex`/`Record`/`Population` shapes | §1's creator-track-record row | none, or a build-pipeline change outside this crate's scope |
| `crates/realorrug-analyst/src/mention.rs` | `first_address`/`read` generalised to the two-shape scan (§2) | §2 | changed function body; `Asked::Mint` keeps its `String` shape unchanged — the text alone carries which shape it is (`0x…` versus base58), so nothing downstream needs a second, disagreeable tag for the same fact |
| `crates/realorrug-onchain/src/dispatch.rs`, new | a single `read(mint_text, clients)` function: parses `mint_text` into a `ChainAddress` and matches once on its shape, calling `SolanaReader` or `RobinhoodReader` through `ChainReader` — corrected from this row's original plan of a `match` on a `Venue` at each caller, which would have been the same "which chain is this" decision written twice, the exact defect §2 exists to remove | §2 | new module, new function, new `Clients`/`Error` types |
| `crates/realorrug-analyst/src/answer.rs:175`, `crates/realorrug-analyst/src/daemon.rs:885`, `crates/realorrug-cli/src/roast.rs`, `crates/realorrug-cli/src/analyst.rs` | the unconditional `realorrug_onchain::build` call becomes a call to `realorrug_onchain::dispatch::read`; `Answering` gains a `robinhood: Option<&realorrug_robinhood::Rpc>` field, and every caller that builds one takes a `--robinhood-rpc`/`REALORRUG_ROBINHOOD_RPC` flag with no default (AGENTS.md §3 rule 7) | §2 | changed call sites and one struct field, same function signatures otherwise |

### The crate boundary question

**Corrected by task packet 0030, which built this (9-15-0026): the
`Dossier`-assembling `build`/`ChainReader` for Robinhood lives in
`crates/realorrug-onchain/src/robinhood.rs`, beside the Solana one, not in
`realorrug-robinhood`.** The raw reads (§1's `getLaunchedToken`, reserves,
holder-sum, graduation-log decode) still live in `realorrug-robinhood`,
exactly as first recommended — only the *assembler that turns those reads
into a `Dossier`* moves. The reason is a dependency direction this section
did not check: `realorrug-types` depends on `realorrug-robinhood` (for
`ChainAddress::Robinhood`), which makes `realorrug-robinhood` the leaf of the
workspace. `ChainReader` is defined in `realorrug-onchain`, above that leaf;
a trait defined above a crate cannot be implemented inside it without that
crate depending back on the trait's crate, which is a dependency cycle
(`realorrug-onchain` already depends on `realorrug-robinhood` for the reads,
so the reverse edge cannot also exist). Putting `build` in
`realorrug-onchain` instead has no such problem: `realorrug-onchain` already
sits above `realorrug-robinhood` in the graph, the same place it already sits
above `realorrug-pumpfun` for Solana's own reads.

**The rest of this section's argument is still right about what matters, and
is not being restated to argue the other way.** The sheet *shape*
(`FactSheet`, `Fact`, `Signal`, `Verdict`, `forbidden`, `fidelity`) still
stays in `realorrug-roast`, unchanged in crate ownership from today, and this
is still what makes [ADR 0028](../adr/0028-one-bot-every-chain.md) point 2's
seam possible: one reader per chain, in its own chain-side module, against
one shared sheet, verdict and voice — `realorrug-roast` gains no chain client
and no path to the payout under this design either way, because the reads
themselves (not the assembler that calls them) are what would have put a
client dependency there, and they were never proposed to move.  What this
correction protects is narrower than "keep the reads out of
`realorrug-roast`" — it is "keep the reads out of `realorrug-roast`, **and**
keep the trait that ties them to a `Dossier` out of the one crate that cannot
depend on it." The cost of the original recommendation — putting `build`
inside `realorrug-robinhood` — would not have been a model-facing crate
gaining a chain client it does not need (it already has one); it would have
been a manifest that cannot compile, which is the shape
AGENTS.md §4's "enforce a property at the cheapest level that holds it" rule
argues for catching before it is written down as a plan: `realorrug-roast` must stay a crate that only
*reads* a `Dossier` and *writes* text, never one that holds an RPC client or
a signing key. **`realorrug-roast` gains no path to the payout under this
design**: nothing proposed above adds a payout-adjacent dependency to that
crate, and the new Robinhood reads live in `realorrug-robinhood`, which
today has no payout code either (the payout key lives in `realorrug-payout`,
untouched by this document).

### A figure travels with its unit, and the read point is the chain's own

Two narrower corrections, from building §1's sheet rather than designing it.

**A figure and its unit travel together, never apart.** `CurveFacts` used to
assume SOL, 9 decimals, for every curve — correct for a Solana curve and
silently wrong for anything else. It now carries `quote_asset:
Option<QuoteAsset>` (a symbol and a decimal count) beside the reserve and
capacity amounts, set by each chain's own reader; `sheet.rs` renders an amount
only when the asset was identified, and says the impact budget "could not be
priced" rather than guess a unit when it was not (rule 8). Both chains render
through the same integer-arithmetic function, so a Robinhood curve's ETH and
a Solana curve's SOL differ only in the `decimals` and `symbol` passed in, not
in a second code path that could drift from the first. The reserve and
capacity amounts are `u128`, not `u64`, because a `u64` of wei tops out
around 18.4 ETH — an ordinary Robinhood curve, not an edge case.

**The read point is the chain's own, not a bare number.** `ReadAt` (already
in `realorrug-types`) is a `Solana(Slot)` / `Robinhood(u64)` enum rather than
one shared integer, specifically so a slot and a block number — both,
underneath, a `u64` counting up roughly once per chain tick — cannot be
compared or displayed as if they were the same clock. **This document records
a gap that building it found, rather than one it solved**: `FactSheet::read_at`
is still `Option<Slot>`, Solana-only, because retyping it (or adding a second
field beside it) breaks `forbidden.rs`'s full struct-literal test
construction and its direct field access, and `forbidden.rs` is owned by a
later task. Today a Robinhood sheet's `read_at` is `None` — not wrong, since
`ReadAt::as_slot` returns `None` for a Robinhood block by design, but the
consequence is a Robinhood `NothingUglyYet` reply currently has no
chronological fact on the sheet to cite at all. Fixing `FactSheet::read_at`
is a later task; `crates/realorrug-analyst/src/log.rs`'s matching gap — a
stored record that could not say what the bot knew when it spoke about a
Robinhood token — was task 9-15-0032: `Entry` now carries a chain-tagged
`read_at: Option<realorrug_types::ReadAt>` beside the still-present,
Solana-only `read_at_slot: Option<u64>`, so a Robinhood reply's log line
keeps its block number instead of losing it to `ReadAt::as_slot`'s honest
`None`.

## 7. What this design does not decide

- **Design 0021 (the read memory — freshness, caching, credits), not yet
  written.** This document names one interface it needs from 0021:
  `CreatorSoldOut` (§3) requires knowing the creator's balance *before* the
  current read to say "sold out" rather than "currently holds nothing." That
  is a memory read, not a fresh chain read, and 0020 assumes 0021 supplies it
  as an interface — `has_prior_balance(creator, token) -> Option<u128>`
  shaped — without designing how that memory is stored, refreshed or
  budgeted. Until 0021 is wired in, `CreatorSoldOut` does not fire (no
  signal, per rule 9 — absent is not zero). **`RepeatLauncher` was listed
  here too and no longer belongs**: it was going to ask the memory for
  `launches_in_window(address, minutes)`, and what shipped reads the lifetime
  count the creator index already holds, so it needs nothing from 0021, and §3's ladder degrades gracefully: fewer signals
  available means more sheets land at `Sketchy` or below rather than
  reaching `RugMechanicsLive`, never the reverse.
- **Design 0022 (threaded follow-ups), not yet written.** Out of scope here
  entirely; this document assumes one mention, one sheet, one reply, and
  says nothing about a conversation.
- **Design 0023 (the public checker page), not yet written.** Out of scope;
  this document's sheet and verdict are consumed by the reply pipeline only,
  and a page reading the same `FactSheet`/`Verdict` types is 0023's problem,
  not this one's, beyond the types themselves being public enough to reuse.
- **Research 0044 (bytecode checks — owner-only mint/pause/blacklist,
  ownership renouncement), in flight.** `OwnerCanStillMintOrPause` (§3)
  names the slot this fills — a `Signal` variant and a fact pair — without
  designing the bytecode scan itself. 0044 owes this document: the actual
  selectors to check for, whether a verified-source ABI read is available or
  a raw bytecode scan is required, and a false-positive rate against
  ordinary `Ownable`-templated tokens that never use the power they carry.

## 8. Not established

- No real Robinhood token from research 0036, 0038 or 0040 demonstrates a
  genuine `Rugged` case (reserves gone *and* a stuck holder together) — the
  worked example in §3 had to show why the closest candidate is actually the
  `LiquidityGone` twin, not a rug, rather than cite a real `Rugged` token.
- `LaunchBlockBundle`'s band is not measured for Pons v2/Robinhood Chain at
  all — research 0042 names the port order but the actual distribution (the
  Robinhood-chain equivalent of research 0008/0012/0013's Solana
  measurements) has not been gathered, so that signal is designed and cannot
  fire. `RepeatLauncher` is no longer in that position: it takes its floor
  from whatever creator index it is handed, so it needs no published
  snapshot. **What is still unmeasured about it** is whether the 95th
  percentile is the right cut for this population, and how many unnamed
  relayers sit above it — both answerable only from a real Robinhood creator
  index, which does not exist yet.
- `HolderConcentration`'s threshold is not measured — no distribution of
  real holder-concentration-vs-outcome exists for Pons v2 tokens.
- `BuyersCannotSell`'s simulated sell size and slippage tolerance are not
  chosen — no chain fact sizes them, and getting this wrong in either
  direction (too small: never fires; too large: fires on ordinary curves)
  was named in §3's own twin as the likely failure mode.
- The exact false-refusal rate of §5's target check (a person-reference near
  an accusation word) is not measured — no corpus of real generated replies
  exists yet to test it against.
- Whether `fidelity::check` should also enforce "at least one cited fact"
  (§4) is named as an open question, not designed here — today's check only
  catches uncited *numbers*, not an evidence-free verdict that cites none.
- Whether the free-text voice recommendation (§4) or the existing
  clause-selection mechanism is what Josh wants for Robinhood is not
  decided — this document recommends free text and states the trade-off,
  but the choice is his to make, same as ADR 0027 itself was.
- The graduation event's real name/signature (research 0040 §2: only the
  topic hash and two decoded data words are confirmed; no verified source
  exists to name it against) — this design cites the topic and fields, never
  a function/event name that was not itself observed.
- What `phase == 1` means, if it is ever used (research 0040 §1) — this
  design's rule already treats any phase other than 0 or 2 as `CantTell`
  (§1, restating research 0040 §1's own recommendation), so this gap does
  not block the design, only names it as still open.
