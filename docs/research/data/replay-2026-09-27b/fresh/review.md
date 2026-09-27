# Replay review

3 model replies used, 0 fell back.

## creator-sale-2xhg (2XHGAAvkxKE8fS97e5mNar5wzADj6ckgoxq2ukYjpump)

- mint: 2XHGAAvkxKE8fS97e5mNar5wzADj6ckgoxq2ukYjpump
- captured at: 2026-09-27T03:39:26Z
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
- read at: slot 450878308
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
- billed: $0.000484

One of four checked early buyers had an unreadable funding source, so the launch’s buyer pattern cannot be settled. The creator shows 20.2231 SOL net decoded trade flow and 437+ transactions, but that alone does not resolve what happened here.

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
- captured at: 2026-09-27T03:39:39Z
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
- read at: slot 450878364
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
- billed: $0.000499

The creator’s decoded sells produced 0.3336 SOL net after buys, while its address has made at least 2965 transactions; that pattern leans sketchy, though the tokens may have been moved to another wallet rather than sold.

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
- captured at: 2026-09-27T03:39:53Z
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
- read at: slot 450878418
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
- billed: $0.000491

The creator’s decoded sells cover buys: 1.5459 SOL moved through sales, but net cash flow was only 0.0366 SOL, and it is absent from the largest sampled accounts.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

