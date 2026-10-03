# Case review: base:0xbc8a7388c6cba5ad74b24627f58f9bd3affd5b07

Provenance: live reader capture; no model call

Question (unverified): "Inspect Clanker configured fees, both reward pool assets, recipients and administrators. Distinguish configured allocations from paid fees and permanent liquidity."

Reply: The allegation is not settled by these bounded reads. Clanker v4 configured LP-reward share: 10000 basis points out of 10000 to 0x9d390d90c8e4346f296b76fd366a5c6ce3beacd8; reward administrator 0x9d390d90c8e4346f296b76fd366a5c6ce3beacd8. Registered LP locker 0xffa37784d619f228d8b379d287a4d7282e500762 reports 5 position(s); this does not establish permanent liquidity. Contract code was observed at 0xbc8a7388c6cba5ad74b24627f58f9bd3affd5b07. totalSupply() returned 100000000000000000000000000000 base units. Unresolved: configured LP-reward shares exclude factory/other fee bases; paid receipts, administrator changes, extensions, current hook fees and position withdrawal rights remain separate checks

Level: CantTell. Complete: false. Replay tool calls: 3 (not original live RPC cost).

Reads and refusals:

- Fees 0xbc8a7388c6cba5ad74b24627f58f9bd3affd5b07: test the allegation against fee configuration; distinguish configured shares from actual payments
- Token 0xbc8a7388c6cba5ad74b24627f58f9bd3affd5b07: validate the target and inspect available controls
- Liquidity 0xbc8a7388c6cba5ad74b24627f58f9bd3affd5b07: inspect supported liquidity without treating graduation as a drain
- planning unavailable: no captured model selection; deterministic fallback
- writer unavailable: no captured model selection; deterministic fallback

Evidence/gaps: see `base-clanker-current-route.assessment.json`.

Checkpoint: 62978b06bf5a938b25de3437da5ed25777f8acf472ede81437276201d48a51ac

Owner accepted: [ ]
