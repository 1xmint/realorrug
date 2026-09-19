<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0035 — S13 "owner powers live" is the Pons v2 reads, not a bytecode scan

**Date:** 2026-09-19
**Status:** accepted. Josh's decision, recorded here, not a first reaction
(2026-09-19).
**Reasoning:** [research 0052](../research/0052-weighted-flags.md)
§3.1's S13 row (`OwnerCanStillMintOrPause`), redefined for Pons v2 there.
**Reconciles:** the conflict [design 0020](../design/0020-robinhood-fact-sheet-and-voice.md)
§3 names in its S13 row and its §8 research-0044 note -- design 0020 gated
`OwnerCanStillMintOrPause` on a bytecode/ABI owner-selector scan (blocked on
research 0044); research 0052 §3 fires the same `Signal` variant off creator
tax, `pendingCreatorFeeRecipient` and snipe-tax exemptions instead, and the
two documents had not been reconciled (research 0052 §3.1's own S13 row and
design 0020's own §8 note both say so explicitly).
**Consequence lands in:** `crates/realorrug-roast/src/sheet.rs` (`factors`,
`FactSheet::build`), `crates/realorrug-roast/src/fidelity.rs` (`Subject::of`).

## Context

`Signal::OwnerCanStillMintOrPause` is one enum variant with two different
firing rules written for it in two documents. Design 0020 named the variant
for a bytecode scan that finds an owner-only mint/pause/blacklist selector on
the deployed contract -- a check this repository cannot build until research
0044 tells it how to tell a stock `Ownable` ancestor nobody calls apart from
one somebody uses (the "stock-Ownable false-positive twin", design 0020's own
name for it). Research 0052 §3, written after Pons v2's real admin surface
was read from verified source (research 0047), redefines the same slot around
what Pons v2 actually gives an owner: a creator tax fixed at launch (0-1,000
bps), a 3-day timelock that can redirect creator fees
(`pendingCreatorFeeRecipient`), and a launch-time snipe-tax exemption list. No
packet had picked one, so the reads research 0052 asks for
(`crates/realorrug-onchain/src/robinhood.rs`'s `powers_facts`, landed by
packet M-D-0004) sat on `Dossier.powers` unprojected onto the sheet the model
reads -- the variant declared, its base weight in `assessment.rs`, and its
factor table all waiting on a direction call this ADR makes.

## Decision

1. **On Pons v2, `Signal::OwnerCanStillMintOrPause` fires off the chain's
   real powers**, per research 0052 §3.1's S13 row: creator tax, the pending
   creator-fee recipient, and classified snipe-tax exemptions. It is not
   conditioned the way `CreatorBoughtOwnLaunch` is on a nonzero buy -- Pons
   v2's own design gives every launched token a creator-tax capability, a
   fee-recipient timelock and an exemption mechanism, so the signal fires
   whenever the powers read at all (creator tax is free, from the same
   `getLaunchedToken` call every dossier already pays for), and the factor
   table (below) is what moves its weight for a specific launch.
2. **Design 0020's bytecode-scan version of this row stays parked, unbuilt,
   until research 0044 answers the stock-Ownable false-positive twin.**
   Nothing here claims research 0044 is answered or that a bytecode scan
   should never be added later for a chain without Pons v2's admin-surface
   research; it is simply not what this variant fires on for Pons v2 today.
   Design 0020 §3's S13 row and §8's research-0044 note are updated to point
   here instead of describing an unreconciled conflict.
3. **The factor table is research 0052 §3.1's own S13 numbers**, wired to
   the facts `powers_facts` already reads:
   - `+700` if creator tax `>= 500` bps (M).
   - `+500` if `pendingCreatorFeeRecipient` is non-zero (M).
   - `+600` per exempt address classified `Undeclared` (on neither research
     0047 §3's first-party list nor the launch's own declared list), capped
     at `+1,200` total (M).
   - `-300` if creator tax is exactly `0` (M).
   - The existing 25%-of-base floor (`assessment.rs::MIN_SHARE_OF_BASE_PERCENT`)
     applies unchanged; nothing here adds a second floor.
4. **A power sub-read that failed is a coverage gap, never a zero or a clean
   reading.** `pending creator fee recipient`, `declared snipe-tax
   exemptions`, `snipe tax exemption` and `snipe tax exemption
   classification` join `sheet.rs`'s existing `skipped` list (the same list
   `"correlated selling"` is already on) -- a read that could only ever raise
   the signal must not degrade the verdict level, or the analyst's own
   uncertainty, into "nothing to see here."

## Consequences

- The conflict between design 0020 and research 0052 that both documents
  named is resolved in research 0052's favour for Pons v2, because it is the
  one built from Pons v2's actual verified source (research 0047) rather than
  from a template assumption research 0044 has not yet checked.
- A chain whose admin surface is not Pons v2's -- or a future Pons v2
  successor that changes the admin surface -- needs its own reconciliation,
  not an assumption that this ADR's factor table travels with it.
- Design 0020 and research 0052 are both updated in the same commit as the
  code, per AGENTS.md's rule that a decision recorded only in a chat log did
  not happen.
