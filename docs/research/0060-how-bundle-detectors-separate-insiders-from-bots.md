<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0060 — how bundle detectors separate insiders from bots

**Date:** 2026-09-23.
**Status:** findings. Read before [design 0031](../design/0031-busy-early-buyers.md),
which acts on them ([ADR 0040](../adr/0040-busy-early-buyers-original-funder-creator-and-exchanges.md)).

## Why this was looked at

The first recapture with full-mode funding reads (main `d152e9c`, VPS,
2026-09-23; [research 0056](0056-the-solana-replay-set.md)) left 8 of 9 real
Solana cases at `CantTell`, and in every one the only unknown was the same:
one or two of the four checked early buyers had more than 300 successful
transactions before their first purchase and no material inbound transfer
among them ("more than 3 signature pages walked"). The same wallet
(`3w2XwgAF…`) was an early buyer in two unrelated launches. These are
trading bots, and one sits among the first buyers of nearly every launch.
Josh asked how the popular detectors cut through that noise cheaply, and
whether this bot can do better.

## What the popular tools publish

Marked documented (the tool's own page), observed (a third party's
write-up) or inferred.

| tool | label | definition | confidence |
|---|---|---|---|
| Trench | bundle % | wallets buying in the same slot, from pump.fun's own data; its guide warns two-wallet bundles are often copy-trading bots | documented ([guide](https://docs.trench.bot/bundle-tools/bundle-scanner-guide)) |
| Bubblemaps | bundle vs cluster | bundle = same block or snipe window; cluster = linked by funding source or transfers, even with no same-block buy (their example: 30 wallets funded by the deployer three months before) | documented ([blog](https://blog.bubblemaps.io/whats-the-difference-between-bundle-cluster-2/)) |
| GMGN | bundler, sniper, insider, linked wallets | bundler = buys packed together; sniper = bought at open (window unpublished); linked = token transfers between wallets | inferred ([blog](https://gmgn.ai/blog/what-is-a-meme-coin-bundle/)) |
| RugCheck | insider networks | graph centrality plus "5+ wallets buying in the first transaction, all funded from the same source" | observed (third-party write-ups) |
| MELT / MemeTrans | coordinated accounts | shared Jito bundle id (from the public Jito explorer) plus shared funding source; 36.5% of supply held by coordinated accounts across 41,470 launches | documented ([arXiv 2602.13480](https://arxiv.org/html/2602.13480)) |
| Axiom, Photon, BullX, Padre, SolSniffer | sniper, insider, bundle, fresh wallet | no public definition found | gap |

The commercial tools run their own indexers, which is why they look cheap;
none publishes how far back it traces funding, and none of their public
pages mentions excluding exchange withdrawals before calling a shared
funder an insider.

## What this bot already has, and what it lacks

Same-block buying is already a measured signal here
(`LaunchBlockInStrongestBand`, `CreatorBoughtOwnLaunch`, `RepeatLauncher`,
`CorrelatedSelling`). What breaks on bots is the funding half: the walk
back from the purchase finds a wallet's *recent* funder, which a busy
wallet does not have within reach.

## Measured: a wallet's first hundred transactions

Helius `getTransactionsForAddress`, `transactionDetails: "full"`,
`sortOrder: "asc"`, `limit: 100`, `status: "succeeded"`, sent by hand on
the VPS on 2026-09-23 for three of the unresolved buyers. One call each,
0.3–0.7 s. The first row of each was a single clean inbound transfer:

| buyer | first active (UTC) | first money in |
|---|---|---|
| `3w2XwgAF…` | 2026-08-24 15:46 | 10.0 SOL from `5REmXqXp…` |
| `9U4M3hDN…` | 2026-03-04 07:52 | 0.309 SOL from `6UyudLaM…` |
| `DxhpC9c4…` | 2026-07-05 15:21 | 15.0 SOL from `5tzFkiKs…` |

So one call recovers the original funder and the wallet's age, the same
fact the funding-cluster methods above key on, and padding a wallet with
cheap transactions does not move its first one.

## Known evasions

Funding each wallet days ahead through intermediaries or exchange
withdrawals, and spreading buys across blocks, defeats both same-block and
one-hop funding checks ([dev.to write-up](https://dev.to/paulf280ui/how-to-detect-coordinated-solana-launches-your-bundle-checker-misses-2c5g),
observed). No check here catches wallets set up months ahead, each funded
from a different address.

## Sources

Linked inline; researched 2026-09-23. The table's gap row means the tool's
site and docs were searched and no methodology was found, not that none
exists.
