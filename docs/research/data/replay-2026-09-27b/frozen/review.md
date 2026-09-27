# Replay review

11 model replies used, 0 fell back.

## ordinary-launch (24RwgHxwu8icT1tcDtgH4RwyaDWao86xfacUo2xHpump)

- mint: 24RwgHxwu8icT1tcDtgH4RwyaDWao86xfacUo2xHpump
- captured at: 2026-09-27T02:57:01Z
- rules version: 2026-09-21
- level: NothingUglyYet

### Report

**Strongest concern**
- no signal fired

**Alternative explanations**
- no signal fired, so there is no alternative reading to state

**Context**
- Among the largest sampled token accounts, one unidentified wallet holds 0.08% of the total supply.
  (evidence: TokenOwnership)

**Missing checks**
- read at: slot 450868899
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 13/16 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000460

About 199.8 hours old, this launch has no graduation yet, but the largest sampled unidentified wallet holds only 0.08% of supply. The read is still early: creator buying is not found, and only 4 of 22 early buyers had funding checked.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

## incomplete-read-funding (2XHGAAvkxKE8fS97e5mNar5wzADj6ckgoxq2ukYjpump)

- mint: 2XHGAAvkxKE8fS97e5mNar5wzADj6ckgoxq2ukYjpump
- captured at: 2026-09-27T02:57:17Z
- rules version: 2026-09-21
- level: CantTell

### Report

**Strongest concern**
- The creator's decoded sells cover its decoded buys, and it is not among the largest sampled token accounts.
  (evidence: CreatorCashFlow)

**Alternative explanations**
| fact kind | alternative reading |
|---|---|
| CreatorCashFlow | a creator wallet that now holds nothing reads the same whether the tokens were sold or only moved to another wallet the creator still holds them in |

**Missing checks**
- read at: slot 450868952
- not known -- where 1 of the 4 checked early buyers got their money could not be read
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
| fact kind | level at read | what would resolve it |
|---|---|---|
| CreatorCashFlow | CantTell | whether the creator's tokens moved to a wallet still under the same control, which a further chain read could still uncover |

(risk index 40/100, coverage 14/18 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000494

One of four checked early buyers had unreadable funding, so the path into that first purchase remains unresolved; that is why this is a can't-tell, not evidence either way. The creator's decoded sales netted 20.2231 SOL, but the creator is not among the largest sampled accounts.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

## creator-sale-catwif (5pYB12kEhfhSFXJjZ7JtyqDpt6uUqhsF6iu6Ee9spump)

- mint: 5pYB12kEhfhSFXJjZ7JtyqDpt6uUqhsF6iu6Ee9spump
- captured at: 2026-09-27T02:57:27Z
- rules version: 2026-09-21
- level: Sketchy

### Report

**Strongest concern**
- The creator bought 0.5326 SOL of their own token in the launch block.
  (evidence: DevBuy)

**Alternative explanations**
| fact kind | alternative reading |
|---|---|
| DevBuy | a creator buying into their own launch block reads the same as a creator buying a token they believe in |

**Missing checks**
- read at: slot 450869011
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
| fact kind | level at read | what would resolve it |
|---|---|---|
| DevBuy | Sketchy | a public statement of intent from the creator, which is off-chain and unverifiable |

(risk index 25/100, coverage 10/13 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000443

The creator spent 0.5326 SOL buying their own token in the launch block—a red flag, though it could also mean genuine conviction. With only 4 of 19 early buyers’ funding checked and 1 linked to an exchange withdrawal wallet, I lean sketchy rather than conclusive.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

## suspicious-launch-snappad (5t5keof7mNJAMq4vKMBZA5WDk6Aa5cBgUsxyRVYspump)

- mint: 5t5keof7mNJAMq4vKMBZA5WDk6Aa5cBgUsxyRVYspump
- captured at: 2026-09-27T02:59:24Z
- rules version: 2026-09-21
- level: NothingUglyYet

### Report

**Strongest concern**
- no signal fired

**Alternative explanations**
- no signal fired, so there is no alternative reading to state

**Context**
- Among the largest sampled token accounts, one unidentified wallet holds 0.1% of the total supply.
  (evidence: TokenOwnership)

**Missing checks**
- read at: slot 450869433
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 13/16 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000471

At about 122.8 hours old, this launch has no major concentration signal: the largest sampled unidentified wallet holds 0.1% of supply. It has not graduated, with 0.1289 SOL in the bonding curve; early funding checks covered only 4 of 19 buyers, so the picture remains limited.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

## creator-sale-hbull (7V6Sk63y8Rr1MvcN5mYNp61wgFhy4EeQg5gUASk9pump)

- mint: 7V6Sk63y8Rr1MvcN5mYNp61wgFhy4EeQg5gUASk9pump
- captured at: 2026-09-27T02:57:41Z
- rules version: 2026-09-21
- level: Sketchy

### Report

**Strongest concern**
- Among the largest sampled token accounts, one unidentified wallet holds 24.2% of the total supply.
  (evidence: TokenOwnership)

**Alternative explanations**
| fact kind | alternative reading |
|---|---|
| LargestHolderShare | a wallet holding a large share of supply reads the same whether it belongs to a single holder or is a vesting contract, a bridge or an exchange that nobody has labelled yet |

**Missing checks**
- read at: slot 450869057
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
| fact kind | level at read | what would resolve it |
|---|---|---|
| LargestHolderShare | Sketchy | a label for the large address -- a vesting contract, a bridge or an exchange -- which no chain read settles by itself |

(risk index 25/100, coverage 10/13 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000448

One unidentified wallet holds 24.2% of supply, the main red flag: that could be a vesting contract, bridge, or exchange, but without identification the concentration keeps this launch sketchy. It has graduated to an AMM, so exits exist, but that does not explain the wallet.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

## suspicious-launch-creator-buy (Axo9EE6yU5B3HJpyT6i32fGgV9P13z4eSUzEy9gGpump)

- mint: Axo9EE6yU5B3HJpyT6i32fGgV9P13z4eSUzEy9gGpump
- captured at: 2026-09-27T02:57:54Z
- rules version: 2026-09-21
- level: Sketchy

### Report

**Strongest concern**
- The creator bought 1.0000 SOL of their own token in the launch block.
  (evidence: DevBuy)

**Alternative explanations**
| fact kind | alternative reading |
|---|---|
| DevBuy | a creator buying into their own launch block reads the same as a creator buying a token they believe in |

**Missing checks**
- read at: slot 450869100
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
| fact kind | level at read | what would resolve it |
|---|---|---|
| DevBuy | Sketchy | a public statement of intent from the creator, which is off-chain and unverifiable |

(risk index 25/100, coverage 13/16 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000458

The creator bought 1.0000 SOL of their own token in the launch block, which is a real flag but can also mean genuine conviction. I lean sketchy because the token still has not graduated and only 0.3134 SOL sits within a 1% impact budget.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

## misleading-concentration-pool (DsjPNCjFrQDXGZ96UzohaMm9PQJJUxWFQ6Gy6do9CSLT)

- mint: DsjPNCjFrQDXGZ96UzohaMm9PQJJUxWFQ6Gy6do9CSLT
- captured at: 2026-09-27T02:58:08Z
- rules version: 2026-09-21
- level: NothingUglyYet

### Report

**Strongest concern**
- no signal fired

**Alternative explanations**
- no signal fired, so there is no alternative reading to state

**Context**
- Among the largest sampled token accounts, one unidentified wallet holds 2.9% of the total supply.
  (evidence: TokenOwnership)

**Missing checks**
- read at: slot 450869150
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 11/14 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000457

About 225.7 hours old, BINGUS has graduated to an AMM and the largest sampled unidentified wallet holds 2.9% of supply. Two of four checked early buyers had exchange-withdrawal funding, so that link is suggestive, not proof; no creator buy was found.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

## clean-read-versioned-tx (EYPSU1oha6ELaZ4wN1crMcdnXDb21S6LWkJXohs7pump)

- mint: EYPSU1oha6ELaZ4wN1crMcdnXDb21S6LWkJXohs7pump
- captured at: 2026-09-27T02:58:22Z
- rules version: 2026-09-21
- level: Sketchy

### Report

**Strongest concern**
- The creator's decoded sells cover its decoded buys, and it is not among the largest sampled token accounts.
  (evidence: CreatorCashFlow)

**Alternative explanations**
| fact kind | alternative reading |
|---|---|
| CreatorCashFlow | a creator wallet that now holds nothing reads the same whether the tokens were sold or only moved to another wallet the creator still holds them in |

**Missing checks**
- read at: slot 450869196
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
| fact kind | level at read | what would resolve it |
|---|---|---|
| CreatorCashFlow | Sketchy | whether the creator's tokens moved to a wallet still under the same control, which a further chain read could still uncover |

(risk index 40/100, coverage 15/18 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000487

The creator’s decoded sales net 0.3336 SOL after buys, while the creator address is not among the largest sampled holders—consistent with selling, though transfers to another wallet could look identical.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

## graduated-pumpswap (GTBxUiw6wJdmmkCGZgRHLyYxqu1vG4KtRpeox6yDpump)

- mint: GTBxUiw6wJdmmkCGZgRHLyYxqu1vG4KtRpeox6yDpump
- captured at: 2026-09-27T02:58:37Z
- rules version: 2026-09-21
- level: NothingUglyYet

### Report

**Strongest concern**
- no signal fired

**Alternative explanations**
- no signal fired, so there is no alternative reading to state

**Context**
- Among the largest sampled token accounts, one unidentified wallet holds 2.6% of the total supply.
  (evidence: TokenOwnership)

**Missing checks**
- read at: slot 450869253
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 10/13 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000443

At about 264.8 hours old, this token has graduated to an AMM and no sampled largest owner exceeds 2.6%; one unidentified wallet holds that share. The launch block had 2 recipient token accounts in 1 transaction, while creator buying there is not known.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

## creator-sale-jimothy (Ge87EtsjwRQbHaqQmKRno69RFTwh9bfSsm99XNxTpump)

- mint: Ge87EtsjwRQbHaqQmKRno69RFTwh9bfSsm99XNxTpump
- captured at: 2026-09-27T02:58:53Z
- rules version: 2026-09-21
- level: NothingUglyYet

### Report

**Strongest concern**
- no signal fired

**Alternative explanations**
- no signal fired, so there is no alternative reading to state

**Context**
- Among the largest sampled token accounts, one unidentified wallet holds 3.0% of the total supply.
  (evidence: TokenOwnership)

**Missing checks**
- read at: slot 450869313
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 10/13 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000446

About 1962.8 hours old, this launch has graduated to an AMM and no sampled large holder exceeds 3.0%; the main unknown is the creator’s launch-block buy, which was not found. Four of 14 early buyers had funding checked, so the history is partial.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

## clean-read-pay (JB2rSPb4W4bnnr5HwQ17JPTi7gMbvdhjgJUE2oQbpump)

- mint: JB2rSPb4W4bnnr5HwQ17JPTi7gMbvdhjgJUE2oQbpump
- captured at: 2026-09-27T02:59:09Z
- rules version: 2026-09-21
- level: Sketchy

### Report

**Strongest concern**
- The creator's decoded sells cover its decoded buys, and it is not among the largest sampled token accounts.
  (evidence: CreatorCashFlow)

**Alternative explanations**
| fact kind | alternative reading |
|---|---|
| CreatorCashFlow | a creator wallet that now holds nothing reads the same whether the tokens were sold or only moved to another wallet the creator still holds them in |

**Missing checks**
- read at: slot 450869373
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
| fact kind | level at read | what would resolve it |
|---|---|---|
| CreatorCashFlow | Sketchy | whether the creator's tokens moved to a wallet still under the same control, which a further chain read could still uncover |

(risk index 40/100, coverage 14/17 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000476

The creator’s decoded sells cover its buys, while it holds just 0.1% of sampled supply—consistent with moving tokens to another wallet, but the 1.5459 SOL net change across sales makes that explanation less convincing. Their sale trail is the red flag.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

