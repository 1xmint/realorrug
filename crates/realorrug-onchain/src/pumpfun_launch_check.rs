// SPDX-License-Identifier: Apache-2.0
//! The six checks `realorrug launch-check solana` runs against a pump.fun
//! launch transaction, per ADR 0037 decision 6 and deploy/LAUNCH.md steps 6
//! and 9.
//!
//! Pure. Takes the decoded transaction plus whatever accounts were read after
//! it, and returns six named results -- nothing here makes a network call, so
//! every refusal path is reachable with synthetic bytes built from the
//! documented layouts (`tests` below).
//!
//! # Why refuse rather than print a caveat
//!
//! This runs once, by hand, right after Josh signs the launch. There is no
//! second chance to catch a bundled snipe or a live mint authority before the
//! token is announced -- so every check that cannot be confirmed refuses
//! rather than passing quietly (AGENTS.md rule 7 and rule 8: deny by default,
//! and unknown is not safe).

use realorrug_decode::pumpfun;
use realorrug_pumpfun::curve::BondingCurve;
use realorrug_types::{Address, Slot};

use crate::mint::mint_authorities;
use crate::rpc::Transaction;

/// One check's outcome: what passed, or why it refused.
///
/// Carries the value read (or the reason) as a string rather than a
/// structured payload, because every one of these is printed and nothing
/// downstream recomputes from it -- ADR 0037 decision 6 says this instrument
/// is read by a human on launch day, not consumed by another program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    /// The check held. Carries what was read.
    Pass(String),
    /// The check did not hold, or could not be confirmed. Carries why.
    Refuse(String),
}

impl CheckOutcome {
    /// Whether this is a [`Self::Pass`].
    #[must_use]
    pub const fn ok(&self) -> bool {
        matches!(self, Self::Pass(_))
    }
}

/// The six checks, run once each, in the order the packet names them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchCheck {
    /// The slot the transaction landed in.
    pub slot: Slot,
    /// 1. The transaction exists and did not fail.
    pub transaction: CheckOutcome,
    /// 2. Exactly one pump.fun launch instruction; names the mint.
    pub single_launch: CheckOutcome,
    /// 3. The bonding curve's `creator` and the launch instruction's
    ///    `creator` both equal the treasury.
    pub fee_recipient: CheckOutcome,
    /// 4. Mint and freeze authorities are both absent.
    pub authorities: CheckOutcome,
    /// 5. The dev wallet's buy in this transaction equals the stated amount
    ///    exactly.
    pub dev_buy: CheckOutcome,
    /// 6. Every instruction's program is on the allowlist, and no buy in
    ///    this transaction belongs to anyone but the dev wallet.
    pub allowlist: CheckOutcome,
}

impl LaunchCheck {
    /// Whether every check passed.
    #[must_use]
    pub fn clean(&self) -> bool {
        [
            &self.transaction,
            &self.single_launch,
            &self.fee_recipient,
            &self.authorities,
            &self.dev_buy,
            &self.allowlist,
        ]
        .into_iter()
        .all(CheckOutcome::ok)
    }
}

/// The programs a pump.fun launch transaction may touch, and why each is
/// there.
///
/// Confirmed against pump.fun's published IDL,
/// [`idl/pump.json`](https://github.com/pump-fun/pump-public-docs/blob/81091419e4457566469d4e2a27f64ed84d42419c/idl/pump.json)
/// at commit `81091419e4457566469d4e2a27f64ed84d42419c` (read 2026-09-24):
/// `create`'s account list names `system_program`, `token_program`,
/// `associated_token_program`, `mpl_token_metadata` and the program itself;
/// `create_v2`'s names the same four with Token-2022 in place of SPL Token.
/// Compute Budget is not in either instruction's account list -- it is its
/// own top-level instruction, universal on Solana for setting a priority fee,
/// and carries no accounts to check.
const ALLOWED_PROGRAMS: &[(&str, &str)] = &[
    (
        "11111111111111111111111111111111",
        "System Program -- funds the new mint and curve accounts (create.system_program)",
    ),
    (
        "ComputeBudget111111111111111111111111111111",
        "Compute Budget -- sets a priority fee; carries no accounts of its own",
    ),
    (
        "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
        "SPL Token -- create's own token program (create.token_program)",
    ),
    (
        "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
        "Token-2022 -- create_v2's token program (create_v2.token_program)",
    ),
    (
        "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
        "Associated Token Account -- creates the curve's token account (create.associated_token_program)",
    ),
    (
        "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s",
        "Metaplex Token Metadata -- create's own metadata account (create.mpl_token_metadata)",
    ),
    (
        "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
        "pump.fun itself",
    ),
];

/// Runs the six checks.
///
/// `curve_account` and `mint_account` are the raw bytes read from chain
/// *after* the transaction, at whatever slot the caller read them -- passing
/// `None` for either is what an unreadable account looks like, and both
/// checks that need one refuse rather than guess (rule 8).
#[must_use]
pub fn check_launch(
    tx: &Transaction,
    curve_account: Option<&[u8]>,
    mint_account: Option<&[u8]>,
    treasury: &Address,
    dev_wallet: &Address,
    dev_buy_lamports: u64,
) -> LaunchCheck {
    let transaction = if tx.failed {
        CheckOutcome::Refuse("the transaction failed".to_owned())
    } else {
        CheckOutcome::Pass(format!("slot {}", tx.slot.0))
    };

    let launches = find_launches(tx);
    let single_launch = match launches.as_slice() {
        _ if tx.failed => CheckOutcome::Refuse("the transaction failed".to_owned()),
        [] => CheckOutcome::Refuse("no pump.fun create or create_v2 instruction".to_owned()),
        [one] => CheckOutcome::Pass(format!("mint {}", one.mint)),
        many => CheckOutcome::Refuse(format!(
            "{} launch instructions in one transaction",
            many.len()
        )),
    };

    let fee_recipient = if tx.failed {
        CheckOutcome::Refuse("the transaction failed".to_owned())
    } else {
        match launches.as_slice() {
            [one] => check_fee_recipient(one, curve_account, treasury),
            _ => {
                CheckOutcome::Refuse("no single launch to check a fee recipient against".to_owned())
            }
        }
    };

    let authorities = check_authorities(mint_account);

    let dev_buy = if tx.failed {
        CheckOutcome::Refuse("the transaction failed".to_owned())
    } else {
        check_dev_buy(tx, dev_wallet, dev_buy_lamports)
    };

    let allowlist = if tx.failed {
        CheckOutcome::Refuse("the transaction failed".to_owned())
    } else {
        check_allowlist(tx, dev_wallet)
    };

    LaunchCheck {
        slot: tx.slot,
        transaction,
        single_launch,
        fee_recipient,
        authorities,
        dev_buy,
        allowlist,
    }
}

/// The mint of the first pump.fun launch instruction in the transaction, if
/// any -- so the RPC-glue caller can derive the bonding-curve address and
/// fetch the mint account *before* running [`check_launch`], which needs
/// both already read.
///
/// Deliberately does not enforce "exactly one launch" the way check 2 does:
/// this exists only to name an account to fetch, and [`check_launch`] still
/// runs its own full scan and refuses on more than one, so a second launch
/// instruction is never silently ignored -- it is just not this function's
/// job to catch it.
#[must_use]
pub fn candidate_mint(tx: &Transaction) -> Option<Address> {
    find_launches(tx).first().map(|f| f.mint)
}

/// A launch instruction found in the transaction, with what it recorded.
struct Found {
    mint: Address,
    creator: Address,
}

/// Every pump.fun `create`/`create_v2` instruction in the transaction.
///
/// Scans every instruction the transaction carries, top-level and inner
/// (CPI) alike -- [`Transaction::instructions`] is already flattened that
/// way, on purpose (see `crate::launch`'s module doc), and a launch instruction
/// hidden inside a CPI is exactly the kind of bundling check 6 exists to
/// catch, not a reason to stop looking.
fn find_launches(tx: &Transaction) -> Vec<Found> {
    let program = pumpfun::PROGRAM_ID.to_string();
    tx.instructions
        .iter()
        .filter(|ix| ix.program == program)
        .filter_map(|ix| {
            let instruction =
                realorrug_decode::decode(realorrug_decode::Program::PumpFun, &ix.data)
                    .known()
                    .copied()
                    .and_then(realorrug_decode::Instruction::pumpfun)?;
            if !instruction.is_launch() {
                return None;
            }
            // The mint is the launch instruction's first account on both
            // paths (IDL `create.accounts[0]` and `create_v2.accounts[0]`,
            // same commit cited on `ALLOWED_PROGRAMS`).
            let mint: Address = ix.accounts.first()?.parse().ok()?;
            let args = pumpfun::launch_args(instruction, &ix.data)?.ok()?;
            Some(Found {
                mint,
                creator: args.creator,
            })
        })
        .collect()
}

/// Check 3: the bonding curve's own `creator` field, and the creator the
/// launch instruction recorded, both equal `treasury`.
///
/// The curve's `creator` field is what pump.fun actually pays creator fees
/// to (via the creator vault derived from it) -- the instruction's own
/// `creator` argument is corroborating, not authoritative on its own, since
/// nothing stops a caller from asking for one thing and pump.fun could in
/// principle be upgraded to record another. Layout confirmed against
/// `idl/pump.json`'s `BondingCurve` account at the same commit
/// [`ALLOWED_PROGRAMS`] cites: five `u64`s, a `bool`, then `creator: pubkey`
/// -- exactly [`BondingCurve::parse`]'s layout.
fn check_fee_recipient(
    found: &Found,
    curve_account: Option<&[u8]>,
    treasury: &Address,
) -> CheckOutcome {
    if found.creator != *treasury {
        return CheckOutcome::Refuse(format!(
            "the launch instruction recorded creator {}, not the treasury",
            found.creator
        ));
    }
    let Some(data) = curve_account else {
        return CheckOutcome::Refuse("the bonding-curve account could not be read".to_owned());
    };
    match BondingCurve::parse(data) {
        Ok(curve) if curve.creator == *treasury => CheckOutcome::Pass(format!("{treasury}")),
        Ok(curve) => CheckOutcome::Refuse(format!(
            "the bonding curve's creator is {}, not the treasury",
            curve.creator
        )),
        Err(e) => CheckOutcome::Refuse(format!("the bonding-curve account did not parse: {e:?}")),
    }
}

/// Check 4: mint and freeze authorities are both absent, read from the mint
/// account now (not from the launch transaction, which cannot see a later
/// revocation).
fn check_authorities(mint_account: Option<&[u8]>) -> CheckOutcome {
    let Some(data) = mint_account else {
        return CheckOutcome::Refuse("the mint account could not be read".to_owned());
    };
    match mint_authorities(data) {
        Ok((Some(mint_authority), _)) => {
            CheckOutcome::Refuse(format!("mint authority is still {mint_authority}"))
        }
        Ok((_, Some(freeze_authority))) => {
            CheckOutcome::Refuse(format!("freeze authority is still {freeze_authority}"))
        }
        Ok((None, None)) => CheckOutcome::Pass("both revoked".to_owned()),
        Err(e) => CheckOutcome::Refuse(format!("the mint account did not parse: {e}")),
    }
}

/// Check 5: the dev wallet's buy in this transaction, exactly.
///
/// A buy instruction is attributed to the dev wallet when its own account
/// list names it (the `user` account every buy variant carries) -- never the
/// fee payer, which lets anyone else's buy in the same transaction be
/// attributed to the dev wallet by paying for it.
fn check_dev_buy(tx: &Transaction, dev_wallet: &Address, expected: u64) -> CheckOutcome {
    let dev_wallet_key = dev_wallet.to_string();
    let program = pumpfun::PROGRAM_ID.to_string();
    let mut total: Option<u64> = None;

    for ix in tx.instructions.iter().filter(|i| i.program == program) {
        if !ix.accounts.iter().any(|a| a == &dev_wallet_key) {
            continue;
        }
        let Some(instruction) =
            realorrug_decode::decode(realorrug_decode::Program::PumpFun, &ix.data)
                .known()
                .copied()
                .and_then(realorrug_decode::Instruction::pumpfun)
        else {
            continue;
        };
        if !instruction.is_buy() {
            continue;
        }
        let Some(Ok(trade)) = pumpfun::trade_args(instruction, &ix.data) else {
            return CheckOutcome::Refuse("a buy by the dev wallet did not decode".to_owned());
        };
        let Some(lamports) = trade.exact_lamports() else {
            // A token-exact buy (`buy`/`buy_v2`) states a SOL *bound*, not an
            // outcome -- rule 9, publishing the bound as the spend would be
            // stating a number the chain does not carry.
            return CheckOutcome::Refuse(
                "the dev wallet's buy names a token amount, not a SOL spend, and the exact spend cannot be read".to_owned(),
            );
        };
        total = Some(total.unwrap_or(0).saturating_add(lamports));
    }

    match total {
        Some(found) if found == expected => CheckOutcome::Pass(format!("{found} lamports")),
        Some(found) => CheckOutcome::Refuse(format!(
            "the dev wallet spent {found} lamports, not the stated {expected}"
        )),
        None if expected == 0 => CheckOutcome::Pass("no dev buy, as stated".to_owned()),
        None => CheckOutcome::Refuse(format!(
            "no buy by the dev wallet was found, expected {expected} lamports"
        )),
    }
}

/// Check 6: every instruction's program is on the allowlist, and every buy
/// in the transaction belongs to the dev wallet.
fn check_allowlist(tx: &Transaction, dev_wallet: &Address) -> CheckOutcome {
    let dev_wallet_key = dev_wallet.to_string();
    let pumpfun_program = pumpfun::PROGRAM_ID.to_string();

    for ix in &tx.instructions {
        if !ALLOWED_PROGRAMS.iter().any(|(p, _)| *p == ix.program) {
            return CheckOutcome::Refuse(format!(
                "an instruction from {} is not allowed",
                ix.program
            ));
        }
        if ix.program != pumpfun_program {
            continue;
        }
        let Some(instruction) =
            realorrug_decode::decode(realorrug_decode::Program::PumpFun, &ix.data)
                .known()
                .copied()
                .and_then(realorrug_decode::Instruction::pumpfun)
        else {
            continue;
        };
        if instruction.is_sell() {
            return CheckOutcome::Refuse(
                "a sell instruction is bundled into the launch".to_owned(),
            );
        }
        if instruction.is_buy() && !ix.accounts.iter().any(|a| a == &dev_wallet_key) {
            return CheckOutcome::Refuse(
                "a buy in this transaction is not the dev wallet's".to_owned(),
            );
        }
    }
    CheckOutcome::Pass("only allowed programs, no other buy".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::RawInstruction;

    fn tx(accounts: &[&str], failed: bool) -> Transaction {
        Transaction {
            slot: Slot(1),
            accounts: accounts.iter().map(|a| (*a).to_owned()).collect(),
            instructions: Vec::new(),
            pre_token_balances: Vec::new(),
            post_token_balances: Vec::new(),
            pre_balances: Vec::new(),
            post_balances: Vec::new(),
            failed,
        }
    }

    fn addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    fn create_ix(mint: Address, creator: Address) -> RawInstruction {
        let mut data = pumpfun::Instruction::Create
            .discriminator()
            .as_bytes()
            .to_vec();
        for s in ["Name", "SYM", "uri"] {
            data.extend_from_slice(&u32::try_from(s.len()).expect("short").to_le_bytes());
            data.extend_from_slice(s.as_bytes());
        }
        data.extend_from_slice(creator.as_bytes());
        RawInstruction {
            program: pumpfun::PROGRAM_ID.to_string(),
            data,
            accounts: vec![mint.to_string()],
        }
    }

    fn buy_ix(user: Address, lamports: u64) -> RawInstruction {
        let mut data = pumpfun::Instruction::BuyExactSolIn
            .discriminator()
            .as_bytes()
            .to_vec();
        data.extend_from_slice(&lamports.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());
        RawInstruction {
            program: pumpfun::PROGRAM_ID.to_string(),
            data,
            accounts: vec![user.to_string()],
        }
    }

    fn curve_bytes(creator: Address) -> Vec<u8> {
        let mut data = vec![0u8; 8 + 5 * 8 + 1 + 32];
        data[..8].copy_from_slice(&realorrug_pumpfun::curve::DISCRIMINATOR);
        // Non-zero reserves so a real curve would price; irrelevant here.
        data[8..16].copy_from_slice(&1u64.to_le_bytes());
        data[16..24].copy_from_slice(&1u64.to_le_bytes());
        data[49..81].copy_from_slice(creator.as_bytes());
        data
    }

    fn mint_bytes(mint_authority: Option<Address>, freeze_authority: Option<Address>) -> Vec<u8> {
        let mut data = vec![0u8; 82];
        if let Some(a) = mint_authority {
            data[0..4].copy_from_slice(&1u32.to_le_bytes());
            data[4..36].copy_from_slice(a.as_bytes());
        }
        if let Some(a) = freeze_authority {
            data[46..50].copy_from_slice(&1u32.to_le_bytes());
            data[50..82].copy_from_slice(a.as_bytes());
        }
        data
    }

    fn passing_tx() -> (Transaction, Address, Address, Address) {
        let treasury = addr(1);
        let dev_wallet = addr(2);
        let mint = addr(3);
        let mut t = tx(&[dev_wallet.to_string().leak()], false);
        t.instructions = vec![create_ix(mint, treasury), buy_ix(dev_wallet, 1_000_000)];
        (t, treasury, dev_wallet, mint)
    }

    #[test]
    fn a_clean_launch_passes_every_check() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            Some(&curve),
            Some(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
        );
        assert!(result.clean(), "{result:?}");
        assert_eq!(result.slot, Slot(1));
    }

    #[test]
    fn a_failed_transaction_refuses_every_check() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.failed = true;
        let result = check_launch(&t, None, None, &treasury, &dev_wallet, 1_000_000);
        assert!(!result.clean());
        assert_eq!(
            result.transaction,
            CheckOutcome::Refuse("the transaction failed".to_owned())
        );
    }

    #[test]
    fn no_launch_instruction_refuses_and_two_launches_refuse() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let mut none = t.clone();
        none.instructions
            .retain(|ix| ix.accounts != vec![addr(3).to_string()]);
        let result = check_launch(&none, None, None, &treasury, &dev_wallet, 1_000_000);
        assert!(!result.single_launch.ok());

        let mut two = t.clone();
        two.instructions.push(create_ix(addr(4), treasury));
        let result = check_launch(&two, None, None, &treasury, &dev_wallet, 1_000_000);
        assert!(!result.single_launch.ok(), "{:?}", result.single_launch);
        assert!(matches!(&result.single_launch, CheckOutcome::Refuse(r) if r.contains("2 launch")));
    }

    #[test]
    fn a_fee_recipient_that_is_not_the_treasury_refuses() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let curve = curve_bytes(addr(9));
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            Some(&curve),
            Some(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
        );
        assert!(!result.fee_recipient.ok(), "{:?}", result.fee_recipient);
    }

    #[test]
    fn an_unreadable_curve_account_refuses_rather_than_passing() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            None,
            Some(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
        );
        assert!(!result.fee_recipient.ok());
    }

    #[test]
    fn a_present_mint_authority_refuses_and_names_it() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(Some(addr(7)), None);
        let result = check_launch(
            &t,
            Some(&curve),
            Some(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
        );
        assert!(!result.authorities.ok());
        assert!(
            matches!(&result.authorities, CheckOutcome::Refuse(r) if r.contains(&addr(7).to_string()))
        );
    }

    #[test]
    fn a_present_freeze_authority_refuses() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, Some(addr(8)));
        let result = check_launch(
            &t,
            Some(&curve),
            Some(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
        );
        assert!(!result.authorities.ok());
    }

    #[test]
    fn an_unreadable_mint_account_refuses() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let result = check_launch(&t, None, None, &treasury, &dev_wallet, 1_000_000);
        assert!(!result.authorities.ok());
    }

    #[test]
    fn a_dev_buy_that_does_not_match_refuses() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            Some(&curve),
            Some(&mint_account),
            &treasury,
            &dev_wallet,
            2_000_000,
        );
        assert!(!result.dev_buy.ok(), "{:?}", result.dev_buy);
    }

    #[test]
    fn no_dev_buy_found_with_a_nonzero_expectation_refuses_rather_than_reading_zero() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.instructions
            .retain(|ix| ix.accounts != vec![dev_wallet.to_string()]);
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            Some(&curve),
            Some(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
        );
        assert!(!result.dev_buy.ok());
    }

    #[test]
    fn no_dev_buy_found_when_none_was_stated_passes() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.instructions
            .retain(|ix| ix.accounts != vec![dev_wallet.to_string()]);
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            Some(&curve),
            Some(&mint_account),
            &treasury,
            &dev_wallet,
            0,
        );
        assert!(result.dev_buy.ok(), "{:?}", result.dev_buy);
    }

    #[test]
    fn a_buy_by_someone_else_refuses_the_allowlist_check() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.instructions.push(buy_ix(addr(5), 500_000));
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            Some(&curve),
            Some(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
        );
        assert!(!result.allowlist.ok(), "{:?}", result.allowlist);
    }

    #[test]
    fn a_bundled_unknown_program_refuses_the_allowlist_check_and_names_it() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.instructions.push(RawInstruction {
            program: "SomeOtherProgram".to_owned(),
            data: vec![0; 8],
            accounts: Vec::new(),
        });
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            Some(&curve),
            Some(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
        );
        assert!(
            matches!(&result.allowlist, CheckOutcome::Refuse(r) if r.contains("SomeOtherProgram"))
        );
    }
}
