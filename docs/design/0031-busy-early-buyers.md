<!-- SPDX-License-Identifier: Apache-2.0 -->
# Design 0031 — busy early buyers: original funder, the creator, and exchanges

**Status:** accepted by Josh on 2026-09-23 ([ADR 0040](../adr/0040-busy-early-buyers-original-funder-creator-and-exchanges.md)).
**Evidence:** [research 0060](../research/0060-how-bundle-detectors-separate-insiders-from-bots.md).

## The problem

`full_mode_funding_search` (`realorrug-onchain/src/wallets.rs`) looks for a checked early buyer's most recent material
inbound transfer at or before its first purchase, walking back at most
`MAX_FUNDING_SIGNATURE_PAGES` pages. A trading bot has hundreds or
thousands of transactions in that window and no inbound transfer among
them, so the walk ends unresolved, the sheet records "where N of the 4
checked early buyers got their money could not be read", and any unknown
forces `CantTell` (`verdict::level`). On 2026-09-23 that alone put 8 of 9
real Solana cases at `CantTell`. Reading deeper does not help: a bot's
history runs to tens of thousands of transactions.

## Three changes

### 1. Read a busy buyer from its start

When the walk back hits its page cap without a funder, make one more
full-mode read of the same wallet: `sortOrder: "asc"`, `limit: 100`,
`status: "succeeded"`, `slot.lte` the purchase slot. Run every row through
the same `funder_of` test the walk uses, in order, and take the **first**
material inbound transfer as the wallet's *original funder*. Record the
first row's time as the wallet's *first active* moment.

- Found: the candidate is resolved, with its funder marked as original,
  not recent. It counts toward the shared-funder tally exactly as a recent
  funder does: "the same address sent material value to N of the checked
  early buyers at or before they bought" stays literally true.
- Not found in those 100 rows, or the read fails, or the node lacks full
  mode: the candidate stays unread, exactly as today (AGENTS.md rule 8). No
  second page: a wallet whose first hundred transactions hold no material
  inbound transfer is not one this check can explain cheaply.
- Cost: one call per unresolved candidate, at most `MAX_CANDIDATES`, inside
  the existing funding call floor (`FUNDING_CALL_FLOOR`; full mode never
  spends the `getTransaction`-per-signature fetches that floor was sized
  for) and a raised page floor (`FUNDING_PAGE_FLOOR`, now `MAX_CANDIDATES *
  (MAX_FUNDING_SIGNATURE_PAGES + 1)` to grant the one extra page this read
  costs each candidate).

The sheet gains one fact when any candidate was resolved this way: how
many of the checked early buyers were already active before the launch,
with their first-active dates ("2 of the 4 early buyers checked were
active before this launch, first seen 2026-03-04 and 2026-08-24"). This is
a dated measurement, not a label: nothing calls them bots.

Nothing today reads a Solana launch's wall-clock time -- `LaunchBlock`
(`realorrug-onchain/src/launch.rs`) carries a slot, not a timestamp -- so
in production the fact falls back to design's own contingency: it counts
every checked candidate with a first-active reading and drops "before
this launch" from the words ("2 of the 4 early buyers checked already had
a transaction on chain, first seen ..."). The "before this launch"
wording above is what the fact says once a launch timestamp exists to
compare against; the comparison itself is implemented and tested today,
only unfed.

### 2. The creator funding early buyers

Compare each checked candidate's funder (recent or original) with the
launch's creator address. When one or more match, the sheet carries a fact
"the creator's address sent material value to N of the checked early
buyers at or before they bought" and a new signal,
`CreatorFundedEarlyBuyers`. It is the strongest pattern in the research
(Bubblemaps' deployer-funded cluster; RugCheck's "funded from the same
source"). It costs no extra reads.

The signal is a flow, not an identity claim, so the words stay at the flow
(rule 4): the creator's address sent money to these wallets; nothing says
the creator owns them. Alone it earns `Sketchy`, exactly as
`CreatorBoughtOwnLaunch` does alone, and its weight in the score is that
signal's weight: 1,200. Unlike `CreatorBoughtOwnLaunch`, it is not in
`verdict::LIVE_RISK_SIGNALS`, so it never counts toward
`RugMechanicsLive` -- it is not a live-risk signal. Moving it up the
ladder waits on a measured rate, like every other level change (ADR
0032).

### 3. Exchanges are not insiders

A dated, sourced list of known exchange withdrawal wallets on Solana lives
in the repository. Each entry names the exchange, the address, the source
that labels it, and the date it was checked. On-chain, the address must
show the fan-out an exchange has (many distinct recipients in a short
window), measured and dated in the list's own research note.

When a shared funder is on the list, the sheet says so in place of the
shared-funder sentence: "N of the checked early buyers were paid out by
<exchange>'s withdrawal wallet (labelled by <source>, checked <date>)".
That funder is left out of the shared-funder tally, because thousands of
strangers withdraw from the same exchange wallet. An exchange-funded buyer
still counts as resolved: where its money came from was read.

An address not on the list is treated as today. Absence from the list is
not a claim that the address is not an exchange.

## What this does not catch

Wallets set up months ahead, each funded from a different address, and
bought with across several blocks. No check here sees that (research 0060,
known evasions); the reply's denominators ("of the 4 checked") keep it
from implying otherwise.

## Order of work

1 and 2 together (one read path, one comparison), then 3 (a sourced list
first, then the sheet wording). Recapture the replay set on the VPS after
each.
