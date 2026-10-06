# Case review: base:0x1bbf51f3742c13d67fa89c56d09a979d40c891f1

Provenance: live reader capture; no model call

Question (unverified): "Inspect configured Clanker fees, LP reward recipients and administrators. Distinguish allocation configuration from realized payments and permanent liquidity."

Reply: The allegation is not settled by these bounded reads. Clanker v4 configured LP-reward share: 10000 basis points out of 10000 to 0x2a293c59e0c2bfdd5cc0312c7094be9cf6f9e164; reward administrator 0x2a293c59e0c2bfdd5cc0312c7094be9cf6f9e164. Registered LP locker 0x63d2dfea64b3433f4071a98665bcd7ca14d93496 reports 1 position(s); this does not establish permanent liquidity. Contract code was observed at 0x1bbf51f3742c13d67fa89c56d09a979d40c891f1. totalSupply() returned 100000000000000000000000000000 base units. Unresolved: configured LP-reward shares exclude factory/other fee bases; paid receipts, administrator changes, extensions, current hook fees and position withdrawal rights remain separate checks

Level: CantTell. Complete: false. Replay tool calls: 3 (not original live RPC cost).

Reads and refusals:

- Fees 0x1bbf51f3742c13d67fa89c56d09a979d40c891f1: test the allegation against fee configuration; distinguish configured shares from actual payments
- Token 0x1bbf51f3742c13d67fa89c56d09a979d40c891f1: validate the target and inspect available controls
- Liquidity 0x1bbf51f3742c13d67fa89c56d09a979d40c891f1: inspect supported liquidity without treating graduation as a drain
- planning unavailable: no captured model selection; deterministic fallback
- writer unavailable: no captured model selection; deterministic fallback

Evidence/gaps: see `base-clanker-fees-refresh.assessment.json`.

Checkpoint: 90d99f6f4e01ddabed6d69fc8fbed2af3e3ad77f26a07d254161f12454d61d81

Owner accepted: [ ]
