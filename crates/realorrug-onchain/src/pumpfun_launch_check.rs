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
use crate::rpc::{RawInstruction, Transaction};

/// The System Program's address -- the only program the allowlist treats
/// differently depending on whether an instruction is top-level or a CPI
/// (see [`StatedTransfer`] and [`check_allowlist`]).
const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";

/// A SOL transfer the operator stated in advance -- the CLI's
/// `--allow-transfer <address>:<lamports>` -- so [`check_allowlist`] can tell
/// it from a top-level System Program transfer nobody asked for.
///
/// Matched on both fields exactly: a stated destination with the wrong
/// amount, or a stated amount to the wrong destination, is not the transfer
/// that was stated (rule 8: unknown is not safe).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatedTransfer {
    /// The exact destination.
    pub to: Address,
    /// The exact lamport amount.
    pub lamports: u64,
}

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
/// `associated_token_program`, `mpl_token_metadata` and the program itself.
/// `create_v2`'s names the same, but with Token-2022 in place of SPL Token,
/// and does **not** name `mpl_token_metadata` at all -- in its place it
/// names `mayhem_program_id`, which this list does not carry as an allowed
/// program: a `create_v2` that actually touches it is refused by
/// [`check_allowlist`], and a curve created in mayhem mode is refused
/// separately, by name, in [`check_fee_recipient`] (the mayhem fee math is
/// not one this check models). Compute Budget is not in either
/// instruction's account list -- it is its own top-level instruction,
/// universal on Solana for setting a priority fee, and carries no accounts
/// to check.
const ALLOWED_PROGRAMS: &[(&str, &str)] = &[
    (
        SYSTEM_PROGRAM,
        "System Program -- funds the new mint and curve accounts as a CPI \
         (create.system_program); a *top-level* transfer is only allowed \
         when it exactly matches a StatedTransfer the operator gave in \
         advance (research 0062 addendum, 2026-09-27)",
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
    (
        "pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ",
        "pump.fun's fee program -- buy_v2 reads its fee tier from it \
         (GetFeesWithQuoteMint), observed in launch \
         2xhvyYRjNLMiP8e5p1so7EPDAH21WwpnDeicYjVxAofhf5xBR81dDRacxkNkd25PVhXPjLr94QCdCtKNMexkYLYf",
    ),
];

/// What reading one account for checks 3 or 4 produced.
///
/// A plain `Option<&[u8]>` cannot say *why* an account came back empty, and
/// dropping that reason is exactly what the CLI's old `.ok().flatten()` did
/// -- a transport failure and "no account at that address" both collapsed
/// into the same `None`, and the operator reading the refusal had no way to
/// tell a node outage from an address that was simply wrong. Carrying the
/// slot for the same reason: a figure a check passes with is only checkable
/// on an explorer if the moment it was read at travels with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountState<'a> {
    /// The account exists; its bytes and the slot the node read them at.
    Present {
        /// The account's raw data.
        data: &'a [u8],
        /// The slot the node served it at, when the node said.
        slot: Option<Slot>,
    },
    /// No account exists at that address.
    Absent,
    /// The read itself failed. Carries why, already redacted of the RPC
    /// endpoint by the caller (the CLI never lets that string reach here
    /// unredacted).
    Failed(String),
}

impl<'a> AccountState<'a> {
    /// An account that was read successfully, with no slot recorded --
    /// convenience for callers (mostly tests) that do not have one.
    #[must_use]
    pub const fn present(data: &'a [u8]) -> Self {
        Self::Present { data, slot: None }
    }
}

/// Runs the six checks.
///
/// `curve_account` and `mint_account` describe whatever the caller managed to
/// read *after* the transaction landed -- [`AccountState::Absent`] or
/// [`AccountState::Failed`] is what an unreadable account looks like, and
/// both checks that need one refuse rather than guess (rule 8).
#[must_use]
pub fn check_launch(
    tx: &Transaction,
    curve_account: AccountState<'_>,
    mint_account: AccountState<'_>,
    treasury: &Address,
    dev_wallet: &Address,
    dev_buy_lamports: u64,
    stated_transfers: &[StatedTransfer],
) -> LaunchCheck {
    // `tx.failed` reads `meta.err`, which is `None` both when the node said
    // the transaction succeeded and when the node's response never carried a
    // `meta` object at all -- `!tx.meta_present` catches the second case,
    // which `failed` alone cannot (rule 8: absent is not zero).
    let unreadable = !tx.meta_present;
    let refused_outcome = || {
        CheckOutcome::Refuse(if unreadable {
            "the transaction's outcome could not be read".to_owned()
        } else {
            "the transaction failed".to_owned()
        })
    };

    let transaction = if tx.failed || unreadable {
        refused_outcome()
    } else {
        CheckOutcome::Pass(format!("slot {}", tx.slot.0))
    };

    let launches = find_launches(tx);
    let single_launch = match launches.as_slice() {
        _ if tx.failed || unreadable => refused_outcome(),
        [] => CheckOutcome::Refuse("no pump.fun create or create_v2 instruction".to_owned()),
        [Ok(one)] => CheckOutcome::Pass(format!("mint {}", one.mint)),
        [Err(reason)] => CheckOutcome::Refuse(reason.clone()),
        many => CheckOutcome::Refuse(format!(
            "{} launch instructions in one transaction",
            many.len()
        )),
    };

    let fee_recipient = if tx.failed || unreadable {
        refused_outcome()
    } else {
        match launches.as_slice() {
            [Ok(one)] => check_fee_recipient(one, curve_account, treasury),
            _ => {
                CheckOutcome::Refuse("no single launch to check a fee recipient against".to_owned())
            }
        }
    };

    let authorities = check_authorities(mint_account);

    let dev_buy = if tx.failed || unreadable {
        refused_outcome()
    } else {
        check_dev_buy(tx, dev_wallet, dev_buy_lamports)
    };

    let allowlist = if tx.failed || unreadable {
        refused_outcome()
    } else {
        check_allowlist(tx, dev_wallet, stated_transfers)
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
    find_launches(tx).first()?.as_ref().ok().map(|f| f.mint)
}

/// A launch instruction found in the transaction, with what it recorded.
struct Found {
    mint: Address,
    creator: Address,
}

/// Every pump.fun `create`/`create_v2` instruction in the transaction, and
/// whether it actually decoded.
///
/// Scans every instruction the transaction carries, top-level and inner
/// (CPI) alike -- [`Transaction::instructions`] is already flattened that
/// way, on purpose (see `crate::launch`'s module doc), and a launch instruction
/// hidden inside a CPI is exactly the kind of bundling check 6 exists to
/// catch, not a reason to stop looking.
///
/// `Err` rather than dropped when the discriminator says "launch" but the
/// mint account or the argument payload does not decode -- a transaction
/// carrying two launch instructions where the second is merely malformed
/// still has two launch instructions in it, and check 2 has to see that
/// count, not a count that quietly went back to one.
fn find_launches(tx: &Transaction) -> Vec<Result<Found, String>> {
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
            let mint = ix.accounts.first().and_then(|a| a.parse::<Address>().ok());
            let args = pumpfun::launch_args(instruction, &ix.data).and_then(Result::ok);
            Some(match (mint, args) {
                (Some(mint), Some(args)) => Ok(Found {
                    mint,
                    creator: args.creator,
                }),
                _ => Err(format!(
                    "a pump.fun {} instruction did not decode",
                    instruction.anchor_name()
                )),
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
    curve_account: AccountState<'_>,
    treasury: &Address,
) -> CheckOutcome {
    if found.creator != *treasury {
        return CheckOutcome::Refuse(format!(
            "the launch instruction recorded creator {}, not the treasury",
            found.creator
        ));
    }
    let (data, slot) = match curve_account {
        AccountState::Present { data, slot } => (data, slot),
        AccountState::Absent => {
            return CheckOutcome::Refuse("the bonding-curve account could not be read".to_owned());
        }
        AccountState::Failed(reason) => {
            return CheckOutcome::Refuse(format!(
                "the bonding-curve account could not be read: {reason}"
            ));
        }
    };
    match BondingCurve::parse(data) {
        Ok(curve) if curve.creator == *treasury => match BondingCurve::is_mayhem_mode(data) {
            Ok(true) => CheckOutcome::Refuse(
                "the bonding curve is in mayhem mode, which changes the fee math this check does not model"
                    .to_owned(),
            ),
            Ok(false) => CheckOutcome::Pass(slot_suffix(format!("{treasury}"), slot)),
            Err(e) => CheckOutcome::Refuse(format!(
                "the bonding curve's is_mayhem_mode flag did not read: {e:?}"
            )),
        },
        Ok(curve) => CheckOutcome::Refuse(format!(
            "the bonding curve's creator is {}, not the treasury",
            curve.creator
        )),
        Err(e) => CheckOutcome::Refuse(format!("the bonding-curve account did not parse: {e:?}")),
    }
}

/// Appends "at slot N" to a passing check's text when the read carried a
/// slot -- so the value is checkable on an explorer, not just asserted.
fn slot_suffix(text: String, slot: Option<Slot>) -> String {
    match slot {
        Some(s) => format!("{text} (slot {})", s.0),
        None => text,
    }
}

/// Check 4: mint and freeze authorities are both absent, read from the mint
/// account now (not from the launch transaction, which cannot see a later
/// revocation).
fn check_authorities(mint_account: AccountState<'_>) -> CheckOutcome {
    let (data, slot) = match mint_account {
        AccountState::Present { data, slot } => (data, slot),
        AccountState::Absent => {
            return CheckOutcome::Refuse("the mint account could not be read".to_owned());
        }
        AccountState::Failed(reason) => {
            return CheckOutcome::Refuse(format!("the mint account could not be read: {reason}"));
        }
    };
    match mint_authorities(data) {
        Ok((Some(mint_authority), _)) => {
            CheckOutcome::Refuse(format!("mint authority is still {mint_authority}"))
        }
        Ok((_, Some(freeze_authority))) => {
            CheckOutcome::Refuse(format!("freeze authority is still {freeze_authority}"))
        }
        Ok((None, None)) => CheckOutcome::Pass(slot_suffix("both revoked".to_owned(), slot)),
        Err(e) => CheckOutcome::Refuse(format!("the mint account did not parse: {e}")),
    }
}

/// Check 5: the dev wallet's buy in this transaction, exactly.
///
/// A buy instruction is attributed to the dev wallet when its own account
/// list names it (the `user` account every buy variant carries) -- never the
/// fee payer, which lets anyone else's buy in the same transaction be
/// attributed to the dev wallet by paying for it.
///
/// The spend itself comes from pump.fun's own `TradeEvent`s, summed for every
/// event whose `is_buy` and `user` name the dev wallet, never from the buy
/// instruction's own arguments: `buy`/`buy_v2` (the common launch-day
/// variants) state a *bound* on the SOL side, not an outcome, and the total
/// paid -- curve amount, protocol fee and creator fee together -- is only
/// recorded in the event (see `realorrug_decode::pumpfun::trade_event`).
fn check_dev_buy(tx: &Transaction, dev_wallet: &Address, expected: u64) -> CheckOutcome {
    let dev_wallet_key = dev_wallet.to_string();
    let program = pumpfun::PROGRAM_ID.to_string();

    // A buy instruction naming the dev wallet with no matching receipt would
    // let a buy silently spend nothing -- count both so a shortfall is
    // caught rather than read as "no dev buy" (rule 8).
    let dev_buy_instructions = tx
        .instructions
        .iter()
        .filter(|ix| ix.program == program)
        .filter(|ix| ix.accounts.iter().any(|a| a == &dev_wallet_key))
        .filter_map(|ix| {
            realorrug_decode::decode(realorrug_decode::Program::PumpFun, &ix.data)
                .known()
                .copied()
                .and_then(realorrug_decode::Instruction::pumpfun)
        })
        .filter(|instruction| instruction.is_buy())
        .count();

    let mut sol_sum: u64 = 0;
    let mut fee_sum: u64 = 0;
    let mut creator_fee_sum: u64 = 0;
    let mut dev_buy_events = 0usize;

    for ix in tx.instructions.iter().filter(|i| i.program == program) {
        let Some(decoded) = pumpfun::trade_event(&ix.data) else {
            continue;
        };
        let Ok(event) = decoded else {
            return CheckOutcome::Refuse(
                "a pump.fun trade event in this transaction did not decode".to_owned(),
            );
        };
        if !event.is_buy || event.user != *dev_wallet {
            continue;
        }
        dev_buy_events += 1;
        sol_sum = sol_sum.saturating_add(event.sol_amount);
        fee_sum = fee_sum.saturating_add(event.fee);
        creator_fee_sum = creator_fee_sum.saturating_add(event.creator_fee);
    }

    if dev_buy_events < dev_buy_instructions {
        return CheckOutcome::Refuse(format!(
            "{dev_buy_instructions} buy instruction(s) name the dev wallet but only \
             {dev_buy_events} carried a trade-event receipt"
        ));
    }

    let total = (dev_buy_events > 0).then(|| {
        sol_sum
            .saturating_add(fee_sum)
            .saturating_add(creator_fee_sum)
    });

    match total {
        Some(found) if found == expected => CheckOutcome::Pass(format!(
            "{found} lamports ({sol_sum} to the curve, {fee_sum} fee, {creator_fee_sum} creator fee)"
        )),
        Some(found) => CheckOutcome::Refuse(format!(
            "the dev wallet spent {found} lamports, not the stated {expected}"
        )),
        None if expected == 0 => CheckOutcome::Pass("no dev buy, as stated".to_owned()),
        None => CheckOutcome::Refuse(format!(
            "no buy by the dev wallet was found, expected {expected} lamports"
        )),
    }
}

/// The System Program's own instruction discriminant (a plain 4-byte
/// little-endian `u32`, not an Anchor 8-byte sighash -- the System Program
/// predates Anchor). Confirmed against Solana's `SystemInstruction` enum
/// (`solana-program`/agave `system_instruction.rs`, cited by the packet this
/// check implements): `Transfer` is variant 2, `TransferWithSeed` is variant
/// 11 -- the only two variants that move lamports to an account named in the
/// instruction, which is why they are the only two this check has to tell
/// from every other top-level System Program instruction.
const SYSTEM_TRANSFER: u32 = 2;
const SYSTEM_TRANSFER_WITH_SEED: u32 = 11;

/// What a top-level System Program instruction turned out to be, for
/// [`check_allowlist`].
enum TopLevelSystem {
    /// A `Transfer` or `TransferWithSeed` that decoded: its destination and
    /// lamports.
    Transfer { to: Address, lamports: u64 },
    /// A System Program instruction other than a transfer (`CreateAccount`,
    /// `Assign`, ...). Research 0062's real launch has none of these at the
    /// top level, so this check cannot confirm one is harmless (rule 8) and
    /// refuses it outright.
    NotATransfer,
    /// The discriminant, or the transfer's own payload or accounts, did not
    /// decode.
    Undecodable,
}

/// Reads a top-level System Program instruction as a [`TopLevelSystem`].
///
/// The destination is the second account for `Transfer` (`[from, to]`) and
/// the third for `TransferWithSeed` (`[from, base, to]`, `base` being the
/// signer) -- both lay lamports out as the first field after the
/// discriminant, so the amount is read the same way either way.
fn classify_top_level_system(ix: &RawInstruction) -> TopLevelSystem {
    let Some(kind) = ix
        .data
        .get(0..4)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map(u32::from_le_bytes)
    else {
        return TopLevelSystem::Undecodable;
    };
    let to_index = match kind {
        SYSTEM_TRANSFER => 1,
        SYSTEM_TRANSFER_WITH_SEED => 2,
        _ => return TopLevelSystem::NotATransfer,
    };
    let Some(lamports) = ix
        .data
        .get(4..12)
        .and_then(|b| <[u8; 8]>::try_from(b).ok())
        .map(u64::from_le_bytes)
    else {
        return TopLevelSystem::Undecodable;
    };
    let Some(to) = ix
        .accounts
        .get(to_index)
        .and_then(|a| a.parse::<Address>().ok())
    else {
        return TopLevelSystem::Undecodable;
    };
    TopLevelSystem::Transfer { to, lamports }
}

/// Handles one top-level System Program instruction for [`check_allowlist`]:
/// `Some(_)` is the refusal to return immediately, `None` means it matched a
/// stated transfer (and `matched` now records that) and the scan continues.
///
/// Split out of `check_allowlist` itself only to keep that function's line
/// count under clippy's limit -- the behaviour (match a stated transfer
/// exactly, or refuse) is unchanged from research 0062's addendum.
fn stated_or_refuse(
    ix: &RawInstruction,
    index: usize,
    stated_transfers: &[StatedTransfer],
    matched: &mut [bool],
) -> Option<CheckOutcome> {
    match classify_top_level_system(ix) {
        TopLevelSystem::Transfer { to, lamports } => {
            let found = stated_transfers
                .iter()
                .enumerate()
                .find(|(j, s)| !matched[*j] && s.to == to && s.lamports == lamports);
            match found {
                Some((j, _)) => {
                    matched[j] = true;
                    None
                }
                None => Some(CheckOutcome::Refuse(format!(
                    "top-level System Program transfer of {lamports} lamports to {to} \
                     (instruction {index}): a launch that also pays someone is not a clean launch"
                ))),
            }
        }
        TopLevelSystem::NotATransfer | TopLevelSystem::Undecodable => {
            Some(CheckOutcome::Refuse(format!(
                "a top-level System Program instruction other than a stated transfer is not \
                 allowed (instruction {index})"
            )))
        }
    }
}

/// Check 6: every instruction's program is on the allowlist, every top-level
/// System Program transfer matches a `StatedTransfer` exactly, and every buy
/// in the transaction belongs to the dev wallet.
///
/// A top-level System Program transfer moves SOL, not the token, so it
/// cannot be a hidden buy -- but a launch that also pays someone (a
/// transaction-landing tip, a platform fee) is not a clean launch unless the
/// operator said it would, with the exact destination and amount, before the
/// check ran (research 0062 addendum, 2026-09-27; Josh's approved plan,
/// 2026-09-26 step A7). Inner (CPI) System Program calls are unconditionally
/// allowed -- pump.fun's own `create` funds the new mint and curve accounts
/// that way on every ordinary launch.
fn check_allowlist(
    tx: &Transaction,
    dev_wallet: &Address,
    stated_transfers: &[StatedTransfer],
) -> CheckOutcome {
    let dev_wallet_key = dev_wallet.to_string();
    let pumpfun_program = pumpfun::PROGRAM_ID.to_string();
    let mut matched = vec![false; stated_transfers.len()];
    // Counts only top-level instructions, so the index in a refusal matches
    // the position an explorer or the operator's own launch tool would show
    // (the lead's finding numbers the real launch's top-level instructions
    // 0 through 5, inner CPIs are not numbered at all).
    let mut top_level_index = 0usize;

    for ix in &tx.instructions {
        if !ALLOWED_PROGRAMS.iter().any(|(p, _)| *p == ix.program) {
            return CheckOutcome::Refuse(format!(
                "an instruction from {} is not allowed",
                ix.program
            ));
        }

        if ix.program == SYSTEM_PROGRAM {
            if !ix.top_level {
                // pump.fun's own CPI into the System Program to fund the new
                // mint and curve accounts -- expected on every launch, and
                // not something the operator states in advance.
                continue;
            }
            let index = top_level_index;
            top_level_index += 1;
            if let Some(refusal) = stated_or_refuse(ix, index, stated_transfers, &mut matched) {
                return refusal;
            }
            continue;
        }
        if ix.top_level {
            top_level_index += 1;
        }
        if ix.program != pumpfun_program {
            continue;
        }
        // The event-CPI self-invocation carries no instruction of its own
        // (it is pump.fun logging via a self-CPI so an indexer can read
        // events from instruction data); it is not one of the pumpfun
        // `Instruction` variants and would otherwise be refused as an
        // undecodable instruction below.
        if realorrug_decode::Discriminator::from_data(&ix.data) == Some(pumpfun::ANCHOR_EVENT_CPI) {
            continue;
        }
        let decoded = realorrug_decode::decode(realorrug_decode::Program::PumpFun, &ix.data);
        let Some(instruction) = decoded
            .known()
            .copied()
            .and_then(realorrug_decode::Instruction::pumpfun)
        else {
            // Rule 8: an instruction this check cannot name is not one it can
            // vouch for. Silently skipping it (the old behaviour) let an
            // unrecognised pump.fun instruction ride along unchecked.
            return CheckOutcome::Refuse(format!(
                "a pump.fun instruction did not decode ({decoded:?})"
            ));
        };
        if matches!(
            instruction,
            pumpfun::Instruction::Create
                | pumpfun::Instruction::CreateV2
                | pumpfun::Instruction::ExtendAccount
                | pumpfun::Instruction::InitUserVolumeAccumulator
        ) {
            continue;
        }
        if instruction.is_sell() {
            return CheckOutcome::Refuse(
                "a sell instruction is bundled into the launch".to_owned(),
            );
        }
        if instruction.is_buy() {
            if !ix.accounts.iter().any(|a| a == &dev_wallet_key) {
                return CheckOutcome::Refuse(
                    "a buy in this transaction is not the dev wallet's".to_owned(),
                );
            }
            continue;
        }
        return CheckOutcome::Refuse(format!(
            "a pump.fun {} instruction is not allowed in a launch",
            instruction.anchor_name()
        ));
    }

    // The operator said each of these would be there -- one that never
    // matched an instruction in the transaction is not a stated transfer,
    // it is a claim the launch did not keep (rule 8).
    if let Some((unmatched, _)) = stated_transfers
        .iter()
        .zip(&matched)
        .find(|(_, found)| !**found)
    {
        return CheckOutcome::Refuse(format!(
            "a stated transfer of {} lamports to {} was not found in this transaction",
            unmatched.lamports, unmatched.to
        ));
    }

    if stated_transfers.is_empty() {
        CheckOutcome::Pass("only allowed programs, no other buy".to_owned())
    } else {
        let list = stated_transfers
            .iter()
            .map(|s| format!("{} lamports to {}", s.lamports, s.to))
            .collect::<Vec<_>>()
            .join(", ");
        CheckOutcome::Pass(format!(
            "only allowed programs, no other buy, {} stated transfer{} ({list})",
            stated_transfers.len(),
            if stated_transfers.len() == 1 { "" } else { "s" }
        ))
    }
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
            meta_present: true,
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
            top_level: true,
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
            top_level: true,
        }
    }

    /// A pump.fun event-CPI instruction carrying a `TradeEvent` for `user`,
    /// with the given lamport breakdown. `token_amount` and the reserve
    /// fields do not matter to this check, so they are zeroed.
    fn event_ix(user: Address, sol: u64, fee: u64, creator_fee: u64) -> RawInstruction {
        let mut data = pumpfun::ANCHOR_EVENT_CPI.as_bytes().to_vec();
        data.extend_from_slice(pumpfun::TRADE_EVENT.as_bytes());
        data.extend_from_slice(addr(3).as_bytes()); // mint
        data.extend_from_slice(&sol.to_le_bytes()); // sol_amount
        data.extend_from_slice(&0u64.to_le_bytes()); // token_amount
        data.push(1); // is_buy
        data.extend_from_slice(user.as_bytes());
        data.extend_from_slice(&0i64.to_le_bytes()); // timestamp
        data.extend_from_slice(&0u64.to_le_bytes()); // virtual_sol_reserves
        data.extend_from_slice(&0u64.to_le_bytes()); // virtual_token_reserves
        data.extend_from_slice(&0u64.to_le_bytes()); // real_sol_reserves
        data.extend_from_slice(&0u64.to_le_bytes()); // real_token_reserves
        data.extend_from_slice(addr(6).as_bytes()); // fee_recipient
        data.extend_from_slice(&0u64.to_le_bytes()); // fee_basis_points
        data.extend_from_slice(&fee.to_le_bytes()); // fee
        data.extend_from_slice(user.as_bytes()); // creator
        data.extend_from_slice(&0u64.to_le_bytes()); // creator_fee_basis_points
        data.extend_from_slice(&creator_fee.to_le_bytes()); // creator_fee
        RawInstruction {
            program: pumpfun::PROGRAM_ID.to_string(),
            data,
            accounts: Vec::new(),
            top_level: false,
        }
    }

    fn curve_bytes(creator: Address) -> Vec<u8> {
        // Through `is_mayhem_mode` (byte 81, left 0: off); a curve that ends
        // before it refuses, as `a_curve_too_short_for_the_mayhem_flag_refuses` holds.
        let mut data = vec![0u8; 8 + 5 * 8 + 1 + 32 + 1];
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
        t.instructions = vec![
            create_ix(mint, treasury),
            buy_ix(dev_wallet, 1_000_000),
            event_ix(dev_wallet, 1_000_000, 0, 0),
        ];
        (t, treasury, dev_wallet, mint)
    }

    #[test]
    fn a_clean_launch_passes_every_check() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(result.clean(), "{result:?}");
        assert_eq!(result.slot, Slot(1));
    }

    #[test]
    fn a_failed_transaction_refuses_every_check() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.failed = true;
        // Accounts that would pass, and a transaction whose outcome *was*
        // read: every refusal below can only come from the failure itself,
        // not from "no launch", an unreadable account or an unread outcome.
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(!result.clean());
        let failed = CheckOutcome::Refuse("the transaction failed".to_owned());
        assert_eq!(result.transaction, failed);
        assert_eq!(result.single_launch, failed);
        assert_eq!(result.fee_recipient, failed);
        assert_eq!(result.dev_buy, failed);
        assert_eq!(result.allowlist, failed);
    }

    #[test]
    fn a_single_launch_that_does_not_decode_refuses_with_its_reason() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        // A create discriminator with no accounts and no args: one launch,
        // and it names no mint.
        t.instructions[0].accounts.clear();
        t.instructions[0].data.truncate(8);
        let result = check_launch(
            &t,
            AccountState::Absent,
            AccountState::Absent,
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert_eq!(
            result.single_launch,
            CheckOutcome::Refuse("a pump.fun create instruction did not decode".to_owned())
        );
    }

    #[test]
    fn no_launch_instruction_refuses_and_two_launches_refuse() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let mut none = t.clone();
        none.instructions
            .retain(|ix| ix.accounts != vec![addr(3).to_string()]);
        let result = check_launch(
            &none,
            AccountState::Absent,
            AccountState::Absent,
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(!result.single_launch.ok());
        assert_eq!(
            result.single_launch,
            CheckOutcome::Refuse("no pump.fun create or create_v2 instruction".to_owned())
        );

        let mut two = t.clone();
        two.instructions.push(create_ix(addr(4), treasury));
        let result = check_launch(
            &two,
            AccountState::Absent,
            AccountState::Absent,
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
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
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(!result.fee_recipient.ok(), "{:?}", result.fee_recipient);
    }

    #[test]
    fn an_unreadable_curve_account_refuses_rather_than_passing() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::Absent,
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
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
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
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
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(!result.authorities.ok());
    }

    #[test]
    fn an_unreadable_mint_account_refuses() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let result = check_launch(
            &t,
            AccountState::Absent,
            AccountState::Absent,
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(!result.authorities.ok());
    }

    #[test]
    fn a_dev_buy_that_does_not_match_refuses() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            2_000_000,
            &[],
        );
        assert!(!result.dev_buy.ok(), "{:?}", result.dev_buy);
    }

    #[test]
    fn no_dev_buy_found_with_a_nonzero_expectation_refuses_rather_than_reading_zero() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.instructions
            .retain(|ix| ix.accounts != vec![dev_wallet.to_string()]);
        // Also drop the matching receipt -- this test is about no evidence
        // of a dev buy existing at all, not just the instruction.
        t.instructions
            .retain(|ix| pumpfun::trade_event(&ix.data).is_none());
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(!result.dev_buy.ok());
    }

    #[test]
    fn no_dev_buy_found_when_none_was_stated_passes() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.instructions
            .retain(|ix| ix.accounts != vec![dev_wallet.to_string()]);
        t.instructions
            .retain(|ix| pumpfun::trade_event(&ix.data).is_none());
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            0,
            &[],
        );
        assert!(result.dev_buy.ok(), "{:?}", result.dev_buy);
    }

    /// A token-exact `buy_v2` (the common launch-day variant) states only a
    /// SOL *bound* -- the real spend, fees included, has to come from the
    /// event-CPI receipt, not the instruction's own arguments.
    fn buy_v2_ix(user: Address, token_bound: u64, sol_bound: u64) -> RawInstruction {
        let mut data = pumpfun::Instruction::BuyV2
            .discriminator()
            .as_bytes()
            .to_vec();
        data.extend_from_slice(&token_bound.to_le_bytes());
        data.extend_from_slice(&sol_bound.to_le_bytes());
        RawInstruction {
            program: pumpfun::PROGRAM_ID.to_string(),
            data,
            accounts: vec![user.to_string()],
            top_level: true,
        }
    }

    #[test]
    fn a_token_exact_dev_buy_passes_with_its_receipt_total() {
        let treasury = addr(1);
        let dev_wallet = addr(2);
        let mint = addr(3);
        let mut t = tx(&[dev_wallet.to_string().leak()], false);
        t.instructions = vec![
            create_ix(mint, treasury),
            buy_v2_ix(dev_wallet, 33_515_219_091_058, u64::MAX),
            event_ix(dev_wallet, 967_264_352, 9_189_012, 2_901_794),
        ];
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            979_355_158,
            &[],
        );
        assert_eq!(
            result.dev_buy,
            CheckOutcome::Pass(
                "979355158 lamports (967264352 to the curve, 9189012 fee, 2901794 creator fee)"
                    .to_owned()
            )
        );
    }

    #[test]
    fn a_buy_naming_the_dev_wallet_with_no_receipt_refuses() {
        // The instruction is there but its event either never landed in this
        // slice or was dropped -- a buy with no receipt must not silently
        // read as "no dev buy" (rule 8), which a zero-decoded default would.
        let treasury = addr(1);
        let dev_wallet = addr(2);
        let mint = addr(3);
        let mut t = tx(&[dev_wallet.to_string().leak()], false);
        t.instructions = vec![
            create_ix(mint, treasury),
            buy_v2_ix(dev_wallet, 1, u64::MAX),
        ];
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1,
            &[],
        );
        assert!(
            matches!(&result.dev_buy, CheckOutcome::Refuse(r) if r.contains('1') && r.contains("receipt")),
            "{:?}",
            result.dev_buy
        );
    }

    #[test]
    fn an_undecodable_trade_event_refuses() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        let mut bad = pumpfun::ANCHOR_EVENT_CPI.as_bytes().to_vec();
        bad.extend_from_slice(pumpfun::TRADE_EVENT.as_bytes());
        // No payload after the tags -- truncated.
        t.instructions.push(RawInstruction {
            program: pumpfun::PROGRAM_ID.to_string(),
            data: bad,
            accounts: Vec::new(),
            top_level: true,
        });
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(
            matches!(&result.dev_buy, CheckOutcome::Refuse(r) if r.contains("did not decode")),
            "{:?}",
            result.dev_buy
        );
    }

    #[test]
    fn another_wallets_receipt_is_not_counted() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.instructions.push(event_ix(addr(5), 500_000, 0, 0));
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        // Unchanged from `passing_tx`'s own dev-wallet receipt: the extra
        // event for someone else must not be folded into the total.
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
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(!result.allowlist.ok(), "{:?}", result.allowlist);
    }

    #[test]
    fn the_fee_program_is_allowed() {
        // pump.fun's own `buy_v2` invokes this program (GetFeesWithQuoteMint)
        // to read its fee tier -- the allowlist has to permit it or every
        // ordinary token-exact launch refuses.
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.instructions.push(RawInstruction {
            program: "pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ".to_owned(),
            data: vec![0; 8],
            accounts: Vec::new(),
            top_level: true,
        });
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(result.allowlist.ok(), "{:?}", result.allowlist);
    }

    #[test]
    fn a_bundled_unknown_program_refuses_the_allowlist_check_and_names_it() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.instructions.push(RawInstruction {
            program: "SomeOtherProgram".to_owned(),
            data: vec![0; 8],
            accounts: Vec::new(),
            top_level: true,
        });
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(
            matches!(&result.allowlist, CheckOutcome::Refuse(r) if r.contains("SomeOtherProgram"))
        );
    }

    #[test]
    fn clean_is_false_when_exactly_one_check_refuses() {
        // Five of the six checks pass; only `authorities` refuses (a present
        // mint authority). `clean()` folds all six with `all`, so this is
        // the case a fold that started `true` and only ORed in failures
        // would get wrong -- it has to catch the single refusal, not just
        // the all-refuse or all-pass extremes the other tests cover.
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(Some(addr(7)), None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(result.transaction.ok());
        assert!(result.single_launch.ok());
        assert!(result.fee_recipient.ok());
        assert!(!result.authorities.ok(), "{:?}", result.authorities);
        assert!(result.dev_buy.ok());
        assert!(result.allowlist.ok());
        assert!(!result.clean(), "one refusal must fail clean()");
    }

    #[test]
    fn a_launch_instruction_whose_recorded_creator_differs_from_the_treasury_refuses() {
        // Distinct from `a_fee_recipient_that_is_not_the_treasury_refuses`:
        // there the *curve's* creator field disagrees with the treasury.
        // Here the curve agrees, but the `create` instruction's own args
        // name a different creator -- `check_fee_recipient` must catch this
        // before it ever reads the curve, or a launch could record one
        // creator on-instruction and land a curve stamped with another.
        let treasury = addr(1);
        let dev_wallet = addr(2);
        let mint = addr(3);
        let mut t = tx(&[dev_wallet.to_string().leak()], false);
        t.instructions = vec![create_ix(mint, addr(9)), buy_ix(dev_wallet, 1_000_000)];
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(!result.fee_recipient.ok(), "{:?}", result.fee_recipient);
        assert!(
            matches!(&result.fee_recipient, CheckOutcome::Refuse(r) if r.contains("the launch instruction recorded creator")),
            "{:?}",
            result.fee_recipient
        );
    }

    /// Runs the clean launch with its curve bytes swapped for `curve`.
    fn with_curve(curve: &[u8]) -> LaunchCheck {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let mint_account = mint_bytes(None, None);
        check_launch(
            &t,
            AccountState::present(curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        )
    }

    #[test]
    fn a_curve_in_mayhem_mode_refuses_by_name() {
        let mut curve = curve_bytes(addr(1));
        curve[81] = 1;
        let result = with_curve(&curve);
        assert!(
            matches!(&result.fee_recipient, CheckOutcome::Refuse(r) if r.contains("mayhem mode")),
            "{:?}",
            result.fee_recipient
        );
    }

    #[test]
    fn a_curve_too_short_for_the_mayhem_flag_refuses() {
        let mut curve = curve_bytes(addr(1));
        curve.truncate(81);
        let result = with_curve(&curve);
        assert!(
            matches!(&result.fee_recipient, CheckOutcome::Refuse(r) if r.contains("is_mayhem_mode")),
            "{:?}",
            result.fee_recipient
        );
    }

    /// Check 3 and 4 carry the slot a read was served at, and a failed read
    /// its reason, so the operator can tell a node outage from a wrong address.
    #[test]
    fn a_read_carries_its_slot_and_a_failed_read_its_reason() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let curve = curve_bytes(treasury);
        let result = check_launch(
            &t,
            AccountState::Present {
                data: &curve,
                slot: Some(Slot(77)),
            },
            AccountState::Failed("rpc transport: timed out".to_owned()),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert_eq!(
            result.fee_recipient,
            CheckOutcome::Pass(format!("{treasury} (slot 77)"))
        );
        assert_eq!(
            result.authorities,
            CheckOutcome::Refuse(
                "the mint account could not be read: rpc transport: timed out".to_owned()
            )
        );
    }

    fn allowlist_with(extra: RawInstruction) -> CheckOutcome {
        allowlist_with_stated(extra, &[])
    }

    fn allowlist_with_stated(extra: RawInstruction, stated: &[StatedTransfer]) -> CheckOutcome {
        let (mut t, _treasury, dev_wallet, _mint) = passing_tx();
        t.instructions.push(extra);
        check_allowlist(&t, &dev_wallet, stated)
    }

    #[test]
    fn a_pump_fun_instruction_that_does_not_decode_refuses() {
        let outcome = allowlist_with(RawInstruction {
            program: pumpfun::PROGRAM_ID.to_string(),
            data: vec![0xEE; 8],
            accounts: Vec::new(),
            top_level: true,
        });
        assert!(
            matches!(&outcome, CheckOutcome::Refuse(r) if r.contains("did not decode")),
            "{outcome:?}"
        );
    }

    #[test]
    fn a_known_pump_fun_instruction_outside_a_launch_refuses_by_name() {
        let outcome = allowlist_with(RawInstruction {
            program: pumpfun::PROGRAM_ID.to_string(),
            data: pumpfun::Instruction::CollectCreatorFee
                .discriminator()
                .as_bytes()
                .to_vec(),
            accounts: Vec::new(),
            top_level: true,
        });
        assert!(
            matches!(&outcome, CheckOutcome::Refuse(r) if r.contains("is not allowed in a launch")),
            "{outcome:?}"
        );
    }

    #[test]
    fn an_instruction_whose_program_did_not_resolve_refuses() {
        let outcome = allowlist_with(RawInstruction {
            program: crate::rpc::UNRESOLVED_PROGRAM.to_owned(),
            data: Vec::new(),
            accounts: Vec::new(),
            top_level: true,
        });
        assert!(!outcome.ok(), "{outcome:?}");
    }

    /// A response with no `meta` leaves `failed` false; that must not read as
    /// a transaction that succeeded.
    #[test]
    fn a_transaction_whose_outcome_was_not_read_refuses() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.meta_present = false;
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        let unread = CheckOutcome::Refuse("the transaction's outcome could not be read".to_owned());
        assert_eq!(result.transaction, unread);
        assert_eq!(result.single_launch, unread);
        assert_eq!(result.allowlist, unread);
    }

    /// A second launch whose payload does not decode still counts as a
    /// second launch.
    #[test]
    fn a_malformed_second_launch_still_counts() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.instructions.push(RawInstruction {
            program: pumpfun::PROGRAM_ID.to_string(),
            data: pumpfun::Instruction::Create
                .discriminator()
                .as_bytes()
                .to_vec(),
            accounts: Vec::new(),
            top_level: true,
        });
        let result = check_launch(
            &t,
            AccountState::Absent,
            AccountState::Absent,
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(
            matches!(&result.single_launch, CheckOutcome::Refuse(r) if r.contains("2 launch")),
            "{:?}",
            result.single_launch
        );
    }

    // -- top-level System Program transfers (research 0062 addendum) --

    /// A `Transfer` (discriminant 2): `[from, to]`, lamports first after the
    /// discriminant.
    fn system_transfer_ix(to: Address, lamports: u64, top_level: bool) -> RawInstruction {
        let mut data = SYSTEM_TRANSFER.to_le_bytes().to_vec();
        data.extend_from_slice(&lamports.to_le_bytes());
        RawInstruction {
            program: SYSTEM_PROGRAM.to_owned(),
            data,
            accounts: vec![addr(9).to_string(), to.to_string()],
            top_level,
        }
    }

    /// A `TransferWithSeed` (discriminant 11): `[from, base, to]` -- the
    /// destination is the *third* account, not the second.
    fn system_transfer_with_seed_ix(to: Address, lamports: u64) -> RawInstruction {
        let mut data = SYSTEM_TRANSFER_WITH_SEED.to_le_bytes().to_vec();
        data.extend_from_slice(&lamports.to_le_bytes());
        RawInstruction {
            program: SYSTEM_PROGRAM.to_owned(),
            data,
            accounts: vec![addr(9).to_string(), addr(10).to_string(), to.to_string()],
            top_level: true,
        }
    }

    /// `CreateAccount` (discriminant 0) -- a top-level System Program
    /// instruction that is not a transfer at all.
    fn system_create_account_ix(top_level: bool) -> RawInstruction {
        RawInstruction {
            program: SYSTEM_PROGRAM.to_owned(),
            data: 0u32.to_le_bytes().to_vec(),
            accounts: vec![addr(9).to_string(), addr(10).to_string()],
            top_level,
        }
    }

    #[test]
    fn an_unstated_top_level_transfer_refuses_the_launch() {
        let (mut t, treasury, dev_wallet, _mint) = passing_tx();
        t.instructions
            .push(system_transfer_ix(addr(7), 1_000_000, true));
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(!result.clean(), "{result:?}");
        assert!(
            matches!(
                &result.allowlist,
                CheckOutcome::Refuse(r)
                    if r.contains("1000000") && r.contains(&addr(7).to_string())
            ),
            "{:?}",
            result.allowlist
        );
    }

    #[test]
    fn the_same_launch_with_no_transfer_at_all_passes() {
        let (t, treasury, dev_wallet, _mint) = passing_tx();
        let curve = curve_bytes(treasury);
        let mint_account = mint_bytes(None, None);
        let result = check_launch(
            &t,
            AccountState::present(&curve),
            AccountState::present(&mint_account),
            &treasury,
            &dev_wallet,
            1_000_000,
            &[],
        );
        assert!(result.clean(), "{result:?}");
    }

    #[test]
    fn a_stated_transfer_that_matches_exactly_passes_and_is_reported() {
        let to = addr(7);
        let outcome = allowlist_with_stated(
            system_transfer_ix(to, 1_000_000, true),
            &[StatedTransfer {
                to,
                lamports: 1_000_000,
            }],
        );
        assert!(
            matches!(
                &outcome,
                CheckOutcome::Pass(r) if r.contains("1000000") && r.contains(&to.to_string())
            ),
            "{outcome:?}"
        );
    }

    #[test]
    fn a_stated_transfer_with_the_wrong_lamports_refuses() {
        let to = addr(7);
        let outcome = allowlist_with_stated(
            system_transfer_ix(to, 999_999, true),
            &[StatedTransfer {
                to,
                lamports: 1_000_000,
            }],
        );
        assert!(!outcome.ok(), "{outcome:?}");
    }

    #[test]
    fn a_stated_transfer_to_the_wrong_destination_refuses() {
        let outcome = allowlist_with_stated(
            system_transfer_ix(addr(8), 1_000_000, true),
            &[StatedTransfer {
                to: addr(7),
                lamports: 1_000_000,
            }],
        );
        assert!(!outcome.ok(), "{outcome:?}");
    }

    #[test]
    fn a_stated_transfer_that_never_appears_refuses() {
        let (t, _treasury, dev_wallet, _mint) = passing_tx();
        let outcome = check_allowlist(
            &t,
            &dev_wallet,
            &[StatedTransfer {
                to: addr(7),
                lamports: 1_000_000,
            }],
        );
        assert!(
            matches!(&outcome, CheckOutcome::Refuse(r) if r.contains("was not found")),
            "{outcome:?}"
        );
    }

    #[test]
    fn an_unstated_transfer_with_seed_refuses() {
        let outcome = allowlist_with(system_transfer_with_seed_ix(addr(7), 500));
        assert!(!outcome.ok(), "{outcome:?}");
    }

    #[test]
    fn a_stated_transfer_with_seed_that_matches_passes() {
        let to = addr(7);
        let outcome = allowlist_with_stated(
            system_transfer_with_seed_ix(to, 500),
            &[StatedTransfer { to, lamports: 500 }],
        );
        assert!(outcome.ok(), "{outcome:?}");
    }

    #[test]
    fn a_top_level_create_account_refuses_even_though_the_program_is_allowed() {
        let outcome = allowlist_with(system_create_account_ix(true));
        assert!(
            matches!(
                &outcome,
                CheckOutcome::Refuse(r) if r.contains("other than a stated transfer")
            ),
            "{outcome:?}"
        );
    }

    #[test]
    fn an_inner_system_transfer_is_allowed_unstated() {
        // pump.fun's own `create` CPIs into the System Program to fund the
        // new mint and curve accounts -- expected on every ordinary launch,
        // and never something the operator states in advance.
        let outcome = allowlist_with(system_transfer_ix(addr(7), 1_000_000, false));
        assert!(outcome.ok(), "{outcome:?}");
    }

    #[test]
    fn an_inner_create_account_is_also_allowed_unstated() {
        let outcome = allowlist_with(system_create_account_ix(false));
        assert!(outcome.ok(), "{outcome:?}");
    }

    // -- the real launch's own top-level shape (research 0062 addendum,
    // 2xhvyYRjNLMiP8e5p1so7EPDAH21WwpnDeicYjVxAofhf5xBR81dDRacxkNkd25PVhXPjLr94QCdCtKNMexkYLYf):
    // 0/1 ComputeBudget, 2 a top-level System transfer, 3 pump.fun create,
    // 4 Associated Token createIdempotent, 5 pump.fun. --

    fn compute_budget_ix() -> RawInstruction {
        RawInstruction {
            program: "ComputeBudget111111111111111111111111111111".to_owned(),
            data: vec![0; 5],
            accounts: Vec::new(),
            top_level: true,
        }
    }

    fn ata_create_idempotent_ix() -> RawInstruction {
        RawInstruction {
            program: "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL".to_owned(),
            data: vec![1],
            accounts: Vec::new(),
            top_level: true,
        }
    }

    fn real_launch_shape(tip_to: Address, tip_lamports: u64) -> (Transaction, Address) {
        let dev_wallet = addr(2);
        let mint = addr(3);
        let treasury = addr(1);
        let mut t = tx(&[dev_wallet.to_string().leak()], false);
        t.instructions = vec![
            compute_budget_ix(),
            compute_budget_ix(),
            system_transfer_ix(tip_to, tip_lamports, true),
            create_ix(mint, treasury),
            ata_create_idempotent_ix(),
            buy_ix(dev_wallet, 1_000_000),
        ];
        (t, dev_wallet)
    }

    #[test]
    fn the_real_launchs_shape_passes_with_the_transfer_stated() {
        let to = addr(9);
        let (t, dev_wallet) = real_launch_shape(to, 1_000_000);
        let outcome = check_allowlist(
            &t,
            &dev_wallet,
            &[StatedTransfer {
                to,
                lamports: 1_000_000,
            }],
        );
        assert!(outcome.ok(), "{outcome:?}");
    }

    /// The index in a refusal is what the operator looks up in an explorer,
    /// so it counts top-level instructions of every program (the stated tip
    /// and the non-System ones alike) and skips inner CPIs. A miscount would
    /// point the operator at the wrong instruction while still refusing, so
    /// only the printed number catches it.
    #[test]
    fn a_refusal_names_the_top_level_index_an_explorer_shows() {
        let to = addr(9);
        let (mut t, dev_wallet) = real_launch_shape(to, 1_000_000);
        t.instructions
            .insert(4, system_transfer_ix(addr(12), 5, false));
        t.instructions
            .push(system_transfer_ix(addr(7), 2_000, true));
        let outcome = check_allowlist(
            &t,
            &dev_wallet,
            &[StatedTransfer {
                to,
                lamports: 1_000_000,
            }],
        );
        let CheckOutcome::Refuse(why) = &outcome else {
            panic!("expected a refusal, got {outcome:?}");
        };
        assert!(why.contains("(instruction 6)"), "{why}");
    }

    #[test]
    fn the_real_launchs_shape_refuses_without_the_transfer_stated() {
        let to = addr(9);
        let (t, dev_wallet) = real_launch_shape(to, 1_000_000);
        let outcome = check_allowlist(&t, &dev_wallet, &[]);
        assert!(!outcome.ok(), "{outcome:?}");
    }

    #[test]
    fn the_real_launchs_shape_refuses_with_the_wrong_lamports_stated() {
        let to = addr(9);
        let (t, dev_wallet) = real_launch_shape(to, 1_000_000);
        let outcome = check_allowlist(&t, &dev_wallet, &[StatedTransfer { to, lamports: 999 }]);
        assert!(!outcome.ok(), "{outcome:?}");
    }

    #[test]
    fn the_real_launchs_shape_refuses_with_the_wrong_destination_stated() {
        let to = addr(9);
        let (t, dev_wallet) = real_launch_shape(to, 1_000_000);
        let outcome = check_allowlist(
            &t,
            &dev_wallet,
            &[StatedTransfer {
                to: addr(10),
                lamports: 1_000_000,
            }],
        );
        assert!(!outcome.ok(), "{outcome:?}");
    }

    #[test]
    fn the_real_launchs_shape_refuses_a_second_stated_transfer_that_is_absent() {
        let to = addr(9);
        let (t, dev_wallet) = real_launch_shape(to, 1_000_000);
        let outcome = check_allowlist(
            &t,
            &dev_wallet,
            &[
                StatedTransfer {
                    to,
                    lamports: 1_000_000,
                },
                StatedTransfer {
                    to: addr(11),
                    lamports: 42,
                },
            ],
        );
        assert!(
            matches!(
                &outcome,
                CheckOutcome::Refuse(r) if r.contains("was not found") && r.contains("42")
            ),
            "{outcome:?}"
        );
    }
}
