<!-- SPDX-License-Identifier: Apache-2.0 -->
# ADR 0040 — busy early buyers: original funder, the creator, and exchanges

**Date:** 2026-09-23
**Status:** accepted. **Josh's decisions, recorded**, from his answer of
2026-09-23 ("All three now").
**Reasoning:** [design 0031](../design/0031-busy-early-buyers.md).
**Evidence:** [research 0060](../research/0060-how-bundle-detectors-separate-insiders-from-bots.md).

## Decision

| # | decision |
|---|---|
| 1 | **A busy early buyer is read from its start.** When the walk back from its purchase ends without a funder, one more read takes the wallet's first 100 successful transactions; the first material inbound transfer is its original funder, and the first row's time is its first-active date. Original funders count toward the shared-funder finding. If that read fails or finds nothing, the buyer stays unread (rule 8) |
| 2 | **Early buyers the creator's address funded are flagged**, as a fact and a signal at the `Sketchy` footing of `CreatorBoughtOwnLaunch`. The words describe the flow of money, never who owns the wallets (rule 4) |
| 3 | **Known exchange withdrawal wallets are not shared funders.** A dated, sourced, on-chain-checked list names them; when a shared funder is on it, the sheet says the exchange paid out to those buyers and leaves it out of the shared-funder tally |
| 4 | All three are built now, in the order of design 0031, before the phase-2 review is handed over |
