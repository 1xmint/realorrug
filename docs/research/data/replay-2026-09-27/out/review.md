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
- billed: $0.000470

At about 199.8 hours old, this token has no graduation yet, but the sampled largest wallet holds only 0.08% of supply. The early-funding read is thin: just 4 of 22 buyers checked, with 2 linked to exchange withdrawals, so there is little ugly evidence yet.

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
- Among the largest sampled token accounts, one unidentified wallet holds 0.0% of the total supply.
  (evidence: TokenOwnership)

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

One of 4 checked early buyers has unreadable funding, so the buyer-history check cannot settle whether the launch had recycled or pre-funded participation. The creator’s sales show 20.2231 SOL net cash flow, while graduation is still absent and the bonding curve holds 0.0965 SOL.

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
- billed: $0.000434

The creator bought 0.5326 SOL of their own token in the launch block—possibly conviction, but it makes the launch sketchy without proving intent. Only 4 of 19 early buyers had funding checked, so the broader buyer picture remains incomplete.

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
- billed: $0.000465

At about 122.8 hours old, this launch has no decisive damage in the measured data: the largest sampled unidentified wallet holds 0.1% of supply, and the curve still holds 0.1289 SOL. It has not graduated, so the story is still early rather than settled.

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
- billed: $0.000452

An unidentified wallet holds 24.2% of supply, the main risk here: it could be a vesting contract, bridge, or exchange, but that isn’t labelled, so the concentration leans this launch sketchy. The token did graduate to an AMM, though its exit capacity isn’t sized here.

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
- billed: $0.000460

The creator bought 1.0000 SOL of their own token in the launch block, which is a real red flag but can also mean they believed in the launch. I lean sketchy because graduation has not happened and only 0.3134 SOL is available before a 1% move.

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
- billed: $0.000465

About 225.7 hours old, BINGUS has graduated to the AMM and its largest sampled unidentified wallet holds 2.9% of supply. The early picture is mixed: 2 of 4 checked buyers received exchange-withdrawal funding, while 2 were already active on chain; no creator buy was found.

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
- Among the largest sampled token accounts, one unidentified wallet holds 0.1% of the total supply.
  (evidence: TokenOwnership)

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
- billed: $0.000488

The creator’s decoded sales netted 0.3336 SOL on Pons v2, while their balance changed 0.9431 SOL per sale; that is the red flag. An empty creator wallet could reflect transfers, not sales, but the cash flow makes me lean toward creator extraction.

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
- billed: $0.000438

About 264.8 hours old, Jean Phil has graduated to the AMM and its largest sampled unidentified wallet holds 2.6% of supply. Early funding was checked for only 4 of 13 buyers, so the launch history is still partly unread.

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
- billed: $0.000438

About 1962.8 hours old, this token has graduated to an AMM and its largest sampled unidentified wallet holds 3.0% of supply. Only 4 of 14 early buyers had funding checked, so the launch history is still partly unread.

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
- Among the largest sampled token accounts, one unidentified wallet holds 0.1% of the total supply.
  (evidence: TokenOwnership)

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
- billed: $0.000509

Three of four checked early buyers were already active on-chain, first seen on 2025-01-31, 2026-08-06 and 2026-08-24—a recurring coordinated-launch shape.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

