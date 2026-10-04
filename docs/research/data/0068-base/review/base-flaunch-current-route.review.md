# Case review: base:0xb28ad96e19b0cdbfe5705b5948223c74ac1f8149

Provenance: live reader capture; no model call

Question (unverified): "Inspect configured Flaunch fees, revenue ownership, escrow and bid-wall controls. Distinguish quoted allocation from beneficiary receipts, buybacks, supply burns and permanent liquidity."

Reply: The allegation is not settled by these bounded reads. Flaunch feeSplit() on a 10000-unit post-referral fee input quotes 0 bid-wall units, 9000 creator units and 1000 protocol units; this is configuration, not receipts. Flaunch creator() returns 0x8aa48dfe58f84d85d41706543432e5d8511e90da; hook owner() returns 0xb8a70b4d1547bf6193bd67a73f4f98ea9fd0a973; fee calculator is 0xdbc2f399bbac8cd766f20c9b917a9a6ecad5bc4b. Contract code was observed at 0xb28ad96e19b0cdbfe5705b5948223c74ac1f8149. totalSupply() returned 100000000000000000000000000000 base units. Unresolved: feeSplit excludes referral waterfall and is not total trade fees; distribution events can accrue to escrow rather than reach a beneficiary; custom managers, dynamic fees, bid-wall liquidity, administrative changes and full historical receipts remain unresolved

Level: CantTell. Complete: false. Replay tool calls: 3 (not original live RPC cost).

Reads and refusals:

- Fees 0xb28ad96e19b0cdbfe5705b5948223c74ac1f8149: test the allegation against fee configuration; distinguish configured shares from actual payments
- Token 0xb28ad96e19b0cdbfe5705b5948223c74ac1f8149: validate the target and inspect available controls
- Liquidity 0xb28ad96e19b0cdbfe5705b5948223c74ac1f8149: inspect supported liquidity without treating graduation as a drain
- planning unavailable: no captured model selection; deterministic fallback
- writer unavailable: no captured model selection; deterministic fallback

Evidence/gaps: see `base-flaunch-current-route.assessment.json`.

Checkpoint: 6e394ce742c68c4c9dd3dfe6f5b4fe8961326ed0981a5924619a101b9946fe3f

Owner accepted: [ ]
