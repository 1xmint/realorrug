# Replay review

10 model replies used, 0 fell back.

## ordinary-launch (24RwgHxwu8icT1tcDtgH4RwyaDWao86xfacUo2xHpump)

- mint: 24RwgHxwu8icT1tcDtgH4RwyaDWao86xfacUo2xHpump
- captured at: 2026-09-24T21:17:39Z
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
- read at: slot 450147861
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 13/16 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000468

About 119.7 hours old, this launch has no graduation and only 0.0624 SOL in its bonding curve, so liquidity is thin. The largest sampled owner holds just 0.08%; early-funder checks cover only 4 of 22 buyers, leaving much of the launch unread.

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
- captured at: 2026-09-24T21:23:20Z
- rules version: 2026-09-21
- level: CantTell

### Report

**Strongest concern**
- no signal fired

**Alternative explanations**
- no signal fired, so there is no alternative reading to state

**Context**
- Among the largest sampled token accounts, one unidentified wallet holds 0.1% of the total supply.
  (evidence: TokenOwnership)

**Missing checks**
- read at: slot 450149123
- not known -- where 1 of the 4 checked early buyers got their money could not be read
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 14/18 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000482

One of 4 checked early buyers has unreadable funding, so the launch cannot be classified: that missing source could have settled whether early buying was independently funded.

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
- captured at: 2026-09-24T21:20:05Z
- rules version: 2026-09-21
- level: CantTell

### Report

**Strongest concern**
- The creator bought 0.5326 SOL of their own token in the launch block.
  (evidence: DevBuy)

**Alternative explanations**
| fact kind | alternative reading |
|---|---|
| DevBuy | a creator buying into their own launch block reads the same as a creator buying a token they believe in |

**Missing checks**
- read at: slot 450148409
- not known -- where 3 of the 3 checked early buyers got their money could not be read
- not known -- the creator's own buys and sells were not checked
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read
- not read (optional) -- whether every launch-window buyer is declared-exempt needs this launch's exemption list

**What would change it**
| fact kind | level at read | what would resolve it |
|---|---|---|
| DevBuy | CantTell | a public statement of intent from the creator, which is off-chain and unverifiable |

(risk index 25/100, coverage 9/15 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000454

Where the 3 checked early buyers got their money could not be read; that would have settled whether the launch had opaque funding links.

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
- captured at: 2026-09-24T21:19:43Z
- rules version: 2026-09-21
- level: CantTell

### Report

**Strongest concern**
- Among the largest sampled token accounts, one unidentified wallet holds 27.4% of the total supply.
  (evidence: TokenOwnership)

**Alternative explanations**
| fact kind | alternative reading |
|---|---|
| LargestHolderShare | a wallet holding a large share of supply reads the same whether it belongs to a single holder or is a vesting contract, a bridge or an exchange that nobody has labelled yet |

**Missing checks**
- read at: slot 450148314
- not known -- where 4 of the 4 checked early buyers got their money could not be read
- not known -- the creator's own buys and sells were not checked
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
| fact kind | level at read | what would resolve it |
|---|---|---|
| LargestHolderShare | CantTell | a label for the large address -- a vesting contract, a bridge or an exchange -- which no chain read settles by itself |

(risk index 25/100, coverage 9/14 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000448

The creator’s own buys and sells were not checked, so this cannot establish what happened to the creator’s position. One unidentified address holds 27.4% of supply; that could be concentrated ownership or an unlabeled custody address, and the sheet cannot distinguish them.

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
- captured at: 2026-09-24T21:24:04Z
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
- read at: slot 450149309
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
| fact kind | level at read | what would resolve it |
|---|---|---|
| DevBuy | Sketchy | a public statement of intent from the creator, which is off-chain and unverifiable |

(risk index 25/100, coverage 16/19 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000483

The creator bought 1.0000 SOL of their own token in the launch block. That can mean conviction rather than coordination, but with no graduation yet and only 0.3246 SOL of 1% impact budget, the launch leans sketchy.

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
- captured at: 2026-09-24T21:18:19Z
- rules version: 2026-09-21
- level: CantTell

### Report

**Strongest concern**
- no signal fired

**Alternative explanations**
- no signal fired, so there is no alternative reading to state

**Context**
- Among the largest sampled token accounts, one unidentified wallet holds 2.9% of the total supply.
  (evidence: TokenOwnership)

**Missing checks**
- read at: slot 450147992
- not known -- where 1 of the 4 checked early buyers got their money could not be read
- not known -- the creator's own buys and sells were not checked
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 11/16 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000446

The creator’s own buys and sells were not checked, so creator activity could not settle this. One unidentified wallet holds 2.9%; the token graduated to an AMM, but the evidence leaves the launch’s ownership story unresolved.

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
- captured at: 2026-09-24T21:18:35Z
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
- read at: slot 450148058
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 15/18 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000484

At about 119.3 hours old, this launch has not graduated and only 0.0429 SOL remains in its bonding curve, leaving little room to trade before price impact. The sampled largest holder has just 0.1%; the main unknown is what happens next.

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
- captured at: 2026-09-24T21:17:59Z
- rules version: 2026-09-21
- level: CantTell

### Report

**Strongest concern**
- no signal fired

**Alternative explanations**
- no signal fired, so there is no alternative reading to state

**Context**
- Among the largest sampled token accounts, one unidentified wallet holds 2.8% of the total supply.
  (evidence: TokenOwnership)

**Missing checks**
- read at: slot 450147938
- not known -- where 4 of the 4 checked early buyers got their money could not be read
- not known -- the creator's own buys and sells were not checked
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 9/14 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000444

Funding for all 4 checked early buyers could not be read, so their source of funds—and whether early buying was organic or coordinated—remains unresolved.

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
- captured at: 2026-09-24T21:19:20Z
- rules version: 2026-09-21
- level: CantTell

### Report

**Strongest concern**
- no signal fired

**Alternative explanations**
- no signal fired, so there is no alternative reading to state

**Context**
- Among the largest sampled token accounts, one unidentified wallet holds 3.0% of the total supply.
  (evidence: TokenOwnership)

**Missing checks**
- read at: slot 450148229
- not known -- the creator's own buys and sells were not checked
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 10/14 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000446

The creator’s own buys and sells were not checked, so creator-linked selling—and what it would settle—remains unread. One unidentified wallet holds 3.0% of supply; the token graduated to an AMM, but that pool is not priced here, so this remains unresolved.

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
- captured at: 2026-09-24T21:18:54Z
- rules version: 2026-09-21
- level: NothingUglyYet

### Report

**Strongest concern**
- no signal fired

**Alternative explanations**
- no signal fired, so there is no alternative reading to state

**Context**
- Among the largest sampled token accounts, one unidentified wallet holds 0.2% of the total supply.
  (evidence: TokenOwnership)

**Missing checks**
- read at: slot 450148128
- not read (optional) -- same-window buyers' linked holdings needs the launch's total supply
- not read (optional) -- how many same-window buyers are fresh needs every checked candidate's pre-launch transaction count, and at least one was not read
- not read (optional) -- whether same-window buy sizes are close to each other needs every checked buyer's spend, and at least one was not read

**What would change it**
- no fired signal to resolve either way

(risk index 0/100, coverage 14/17 facts read -- a count of what fired, not the published level and not a forecast)

### Reply

- source: model reply used
- billed: $0.000475

This launch is about 42.7 hours old, so the small largest-holder share (0.2%) says little yet. Three of four checked early buyers were already active on chain, while the token remains on the bonding curve with 0.2674 SOL.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

