# Case review: ethereum:0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48

Provenance: live reader capture; no model call

Question (unverified): "Inspect token controls, liquidity and bounded transfers. Treat missing data as unknown."

Reply: The allegation is not settled by these bounded reads. Contract code was observed at 0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48. totalSupply() returned 49249773962700412 base units. A bounded token-transfer log read covered blocks 26096951 through 26098951; it is not full wallet history. Verified Uniswap v2 pair 0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc reports reserve0 10498194939073 and reserve1 3896564802660525590293 in base units. Factory feeTo() returns 0xf38521f130fccf29db1961597bc5d2b60f995f85; this is configuration, not a payout receipt. Unresolved: getters and standard slots do not prove absence of mint, pause, blacklist or nonstandard upgrade powers; verify implementation semantics

Level: CantTell. Complete: false. Replay tool calls: 3 (not original live RPC cost).

Reads and refusals:

- Token 0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48: validate the target and inspect available controls
- History 0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48: inspect supplied wallet leads within bounded history
- Liquidity 0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48: inspect supported liquidity without treating graduation as a drain
- planning unavailable: no captured model selection; deterministic fallback
- writer unavailable: no captured model selection; deterministic fallback

Evidence/gaps: see `ethereum-usdc-read.assessment.json`.

Checkpoint: 149c53e06221afd707276ae6d8b9266c7e2932814eeea22552c2ff9c80c7a895

Owner accepted: [ ]
