<!-- SPDX-License-Identifier: Apache-2.0 -->
# 0061 — Solana exchange withdrawal wallets

**Date:** 2026-09-23.
**Status:** findings. The list [design 0031](../design/0031-busy-early-buyers.md)
§3 asks for ([ADR 0040](../adr/0040-busy-early-buyers-original-funder-creator-and-exchanges.md) #3).

## Why

A wallet that an exchange pays withdrawals from funds thousands of
strangers. When it shows up as the shared funder of a launch's early buyers,
"the same address funded 3 of the 4" is true and misleading. The first
original-funder probe ([research 0060](0060-how-bundle-detectors-separate-insiders-from-bots.md))
already hit one: `DxhpC9c4…` was first paid 15 SOL by `5tzFkiKs…`, labelled
Binance below.

## The list

Labels read on Solscan's account pages on 2026-09-23, in a browser (a plain
fetch gets HTTP 403). "Solscan funding out" is the count Solscan shows on
the same page. Every row is **observed**, from one labelling service: no
exchange publishes these, and Arkham's entity pages, the obvious second
source, sat behind a Cloudflare challenge that day.

Measured by hand on the VPS the same day at about 23:14 UTC: the wallet's
latest 100 successful transactions (Helius `getTransactionsForAddress`,
full, newest first), how many sent SOL out, and how many distinct accounts
gained SOL in those sends.

| exchange | address | Solscan label | Solscan funding out | latest 100: out / distinct recipients / span |
|---|---|---|---|---|
| Binance | `5tzFkiKscXHK5ZXCGbXZxdw7gTjjD1mBwuoFbhUvuAi9` | Binance 2 | more than 1,000,000 | 81 / 88 / 4 min |
| Coinbase | `GJRs4FwHtemZ5ZE9x3FNvJ8TMwitKTh21yxdRPqn7npE` | Coinbase Hot Wallet 2 | more than 1,000,000 | 100 / 53 / 11 min |
| OKX | `C68a6RCGLiPskbPYtAcsCjhG8tfTWYcoB4JjCrXFdqyo` | OKX Hot Wallet (C68a6) | 453,537 | 55 / 10 / 31 min |
| Bybit | `AC5RDfQFmDS1deWZos921JfqscXdByf8BKHs5ACWjtW2` | Bybit Hot Wallet (AC5RD) | more than 1,000,000 | 33 / 7 / 3.6 h |
| Kraken | `FWznbcNXWQuHTawe9RxvQ2LdCENssh12dsznf4RiouN5` | Kraken Hot Wallet (FWznb) | 694,232 | 0 / 0 / 8.3 days |
| Gate | `u6PJ8DtQuPFnfmwHbGFULQ4u4EgjDiyYKjVEsynXq2w` | Gate | 616,916 | 53 / 53 / 21 min |
| HTX | `BY4StcU9Y2BpgH8quZzorg31EGE4L1rjomN8FNsCBEcx` | HTX Hot Wallet (BY4St) | 209,727 | 97 / 33 / 3.0 days |
| Crypto.com | `AobVSwdW9BbpMdJvTqeCN4hPAmh4rHm7vwLnQ5ATSyrS` | Crypto.com Hot Wallet 2 | more than 1,000,000 | 100 / 83 / 15 min |

Reading it:

- Binance, Coinbase, Gate and Crypto.com pay dozens of distinct wallets
  within minutes: withdrawal wallets by any reading.
- OKX, Bybit and HTX pay out, to fewer distinct wallets in the window.
  Their Solscan totals (over 200,000 recipients each) are what put them on
  the list; the window is a spot check, not the test.
- Kraken's wallet only received in its latest 100 transactions (inbound
  rows over 8 days, likely deposits or dust). It stays on the list on its
  Solscan label and history. A buyer it funded was paid by it at the time,
  which is what the sentence says.

## Not found

KuCoin and MEXC: no labelled Solana withdrawal wallet found. Bitget: only a
cold wallet label was found, which is not what this list is for. Absence
from the list is not a claim that an address is not an exchange (design
0031 §3).

No maintained open dataset with real Solana exchange addresses was found:
`tracebrief/tracebrief-solana` ships a template only, and
`ImMike/crypto-wallet-address-labels` showed no Solana exchange rows in its
README and one commit. How RugCheck and Bubblemaps mark exchange wallets was
not found in their own documentation.

## Refreshing it

Exchanges rotate hot wallets. The list carries this date; a refresh reads
the labels again and repeats the fan-out probe, and adds a dated addendum
here rather than editing the table in place.
