# Case review: solana:JB2rSPb4W4bnnr5HwQ17JPTi7gMbvdhjgJUE2oQbpump

Provenance: live reader capture; no model call

Question (unverified): "Inspect fee routing, authorities, liquidity and gaps. A configured recipient does not prove a payment or common control."

Reply: The allegation is not settled by these bounded reads. Pump curve creator field is 7zTUAxGL36kWFDyDkzeLQAeiKRNRPKQFLuMY2craHpdM; derived creator fee vault is 3g8qDGZn4p8AgKhU4MKjry5uHcaYCejK2ZpUEuVJaYho. This is a configured route, not beneficiary identity. Creator fee vault native balance is 672428 lamports at slot 452355189; this includes any account reserve. Account JB2rSPb4W4bnnr5HwQ17JPTi7gMbvdhjgJUE2oQbpump is owned by TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb. Mint authority: revoked. Freeze authority: revoked. A bounded recent signature page was read; it is not the wallet's complete history. Unresolved: creator vaults aggregate fees across tokens; balances/decreases are not per-token recipient receipts or downstream ownership proof

Level: CantTell. Complete: false. Replay tool calls: 4 (not original live RPC cost).

Reads and refusals:

- Fees JB2rSPb4W4bnnr5HwQ17JPTi7gMbvdhjgJUE2oQbpump: test the allegation against fee configuration; distinguish configured shares from actual payments
- Token JB2rSPb4W4bnnr5HwQ17JPTi7gMbvdhjgJUE2oQbpump: validate the target and inspect available controls
- History JB2rSPb4W4bnnr5HwQ17JPTi7gMbvdhjgJUE2oQbpump: inspect supplied wallet leads within bounded history
- Liquidity JB2rSPb4W4bnnr5HwQ17JPTi7gMbvdhjgJUE2oQbpump: inspect supported liquidity without treating graduation as a drain
- planning unavailable: no captured model selection; deterministic fallback
- writer unavailable: no captured model selection; deterministic fallback

Evidence/gaps: see `solana-pump-current.assessment.json`.

Checkpoint: b94369cd9f95a9ca048ba751b17d01f8968ae34971f0abdf1a95c3c4c5aa4421

Owner accepted: [ ]
