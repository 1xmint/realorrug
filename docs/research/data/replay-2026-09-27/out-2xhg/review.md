# Replay review

1 model reply used, 0 fell back.

## creator-sale-2xhg (2XHGAAvkxKE8fS97e5mNar5wzADj6ckgoxq2ukYjpump)

- mint: 2XHGAAvkxKE8fS97e5mNar5wzADj6ckgoxq2ukYjpump
- captured at: 2026-09-27T02:59:41Z
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
- read at: slot 450869483
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
- billed: $0.000491

One of the 4 checked early buyers had unreadable funding; that leaves the key source-of-funds question unsettled, so the launch can't be classified. The creator's sales show 20.2231 SOL net cash flow, but that alone does not establish where the tokens went.

### Checks

- PASS -- evidence fidelity (reply) -- every number is on the fact sheet
- PASS -- unsupported accusation (reply) -- no forbidden phrase found
- PASS -- unknown-data (reply) -- no gap called safe, and the risk index called no probability
- PASS -- evidence fidelity (report) -- every number is on the fact sheet
- PASS -- unsupported accusation (report) -- no forbidden phrase found
- PASS -- unknown-data (report) -- no gap called safe, and the risk index called no probability

accept? (yes/no, note): 

