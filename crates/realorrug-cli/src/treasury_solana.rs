// SPDX-License-Identifier: Apache-2.0
//! `realorrug treasury solana`: what the treasury has received on Solana, and
//! what is still unclaimed.
//!
//! `treasury solana --treasury <addr> [--mint <addr>] [--rpc URL]
//! [--seconds N]` walks the treasury address's whole signature history,
//! finds every transaction where a known fee vault's balance fell (a
//! *receipt*, per `realorrug_onchain::treasury_receipts`), and reports the
//! vaults' current unclaimed balances. This is the instrument ADR 0037's
//! Consequences names and `deploy/LAUNCH.md`'s "After launch" step calls for
//! -- run by hand, by Josh, before he records a payment. Read-only: no key,
//! nothing signed.
//!
//! `--mint` is header-only. The fee vaults are keyed by *creator*, not mint
//! (pump.fun's `creator_vault` and PumpSwap's `coin_creator_vault` both take
//! the creator address as their seed) -- and per ADR 0037 decision 2, the
//! disclosed treasury wallet *is* that on-chain creator/fee-recipient, so
//! `--treasury` doubles as the address whose history is walked and the
//! creator address both vault PDAs are derived from.
//!
//! The RPC endpoint is never printed (it carries a key): only the values the
//! report reads.

use std::time::Duration;

use realorrug_onchain::budget::{DEFAULT_MAX_CALLS, DEFAULT_MAX_PAGES};
use realorrug_onchain::treasury_receipts::{
    Receipt, SignedTransaction, Unread, VaultAddresses, VaultKind, find_receipts,
};
use realorrug_onchain::{Budget, RpcClient};
use realorrug_types::Address;

const LAMPORTS_PER_SOL: f64 = 1_000_000_000.0;

/// Runs the command.
///
/// # Errors
///
/// A message when `--treasury` is missing or unparseable (rule 7: deny by
/// default when config is missing).
pub fn run(args: &[String]) -> Result<(), String> {
    let treasury: Address = crate::flag(args, "--treasury")
        .ok_or("--treasury <address> is required")?
        .parse()
        .map_err(|e| format!("--treasury: {e}"))?;
    // Header-only: read and validated so a typo is caught early, but the
    // vault derivations below never touch it.
    let mint: Option<Address> = crate::flag(args, "--mint")
        .map(|m| m.parse().map_err(|e| format!("--mint: {e}")))
        .transpose()?;

    // No default endpoint (rule 7): the public one is rate-limited, and
    // picking it silently would be picking for the operator.
    let client = crate::flag(args, "--rpc").map_or_else(
        || RpcClient::from_vars(&|k| std::env::var(k).ok()),
        RpcClient::new,
    );
    let seconds = crate::flag(args, "--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);
    let mut budget = Budget::new(
        DEFAULT_MAX_CALLS,
        DEFAULT_MAX_PAGES,
        Duration::from_secs(seconds),
    );

    let pumpfun_vault = realorrug_pumpfun::pda::creator_vault(&treasury)
        .ok_or("could not derive the pump.fun creator vault")?;
    let pumpswap_vault = realorrug_pumpfun::pda::pumpswap_coin_creator_vault_ata(&treasury)
        .ok_or("could not derive the PumpSwap coin-creator vault")?;
    let vaults = VaultAddresses {
        pumpfun: pumpfun_vault,
        pumpswap: pumpswap_vault,
    };

    let (signatures, truncated) = client
        .signatures_back_to_oldest(&mut budget, &treasury)
        .map_err(|e| e.to_string())?;

    let mut transactions = Vec::with_capacity(signatures.len());
    for sig in &signatures {
        if sig.err.is_some() {
            // A failed transaction moved nothing (`SignatureInfo::err`'s own
            // doc comment: 0006 found counting these overstated a label by
            // more than a third). Not a receipt and not unread either --
            // there is nothing here that could have been one.
            continue;
        }
        let transaction = client
            .transaction(&mut budget, &sig.signature)
            .unwrap_or(None);
        transactions.push(SignedTransaction {
            signature: sig.signature.clone(),
            transaction,
        });
    }

    let (mut receipts, mut unread) = find_receipts(&vaults, &transactions);
    receipts.sort_by_key(|r| r.slot.0);
    if truncated {
        unread.push(Unread {
            signature: String::new(),
            why: "the signature walk was truncated by the budget before reaching the oldest \
                  signature"
                .to_owned(),
        });
    }

    let pumpfun_unclaimed = unclaimed_pumpfun(&client, &mut budget, &pumpfun_vault);
    let pumpswap_unclaimed = unclaimed_pumpswap(&client, &mut budget, &pumpswap_vault);

    print!(
        "{}",
        report(
            &treasury,
            mint.as_ref(),
            &receipts,
            &unread,
            pumpfun_unclaimed,
            pumpswap_unclaimed,
        )
    );

    if unread.is_empty() {
        Ok(())
    } else {
        // Non-zero exit: rule 8, an incomplete read must not look the same
        // as a complete one to a script checking the exit status.
        Err(String::new())
    }
}

/// The pump.fun vault's unclaimed lamports: its balance minus the
/// rent-exempt minimum it must keep, with the slot it was read at. `None`
/// when the account could not be read (rule 8: absent, not zero).
fn unclaimed_pumpfun(
    client: &RpcClient,
    budget: &mut Budget,
    vault: &Address,
) -> Option<(u64, u64)> {
    let account = client.account(budget, vault).ok().flatten()?;
    let slot = account.slot.map(|s| s.0).unwrap_or_default();
    let rent_exempt = client
        .minimum_balance_for_rent_exemption(budget, account.data.len())
        .ok()?;
    Some((account.lamports.saturating_sub(rent_exempt), slot))
}

/// The PumpSwap vault's unclaimed WSOL, with the slot it was read at. `None`
/// when the account could not be read or parsed as an SPL token account.
fn unclaimed_pumpswap(
    client: &RpcClient,
    budget: &mut Budget,
    vault: &Address,
) -> Option<(u64, u64)> {
    let account = client.account(budget, vault).ok().flatten()?;
    let slot = account.slot.map(|s| s.0).unwrap_or_default();
    let parsed =
        realorrug_pumpfun::token::TokenAccount::parse(&account.data, &realorrug_pumpfun::token::SPL_TOKEN_PROGRAM)
            .ok()?;
    Some((parsed.amount, slot))
}

fn sol(lamports: u64) -> String {
    format!("{:.9}", lamports as f64 / LAMPORTS_PER_SOL)
}

/// What the operator reads: one line per receipt, then totals per vault
/// kind, then unclaimed, then unread items.
fn report(
    treasury: &Address,
    mint: Option<&Address>,
    receipts: &[Receipt],
    unread: &[Unread],
    pumpfun_unclaimed: Option<(u64, u64)>,
    pumpswap_unclaimed: Option<(u64, u64)>,
) -> String {
    let mut lines = vec![format!("treasury {treasury}")];
    if let Some(m) = mint {
        lines.push(format!("mint {m} (header only -- vaults key on creator)"));
    }

    lines.push(String::new());
    lines.push("receipts:".to_owned());
    if receipts.is_empty() {
        lines.push("  (none read)".to_owned());
    }
    for r in receipts {
        lines.push(format!(
            "  {} slot {} {} {} SOL{}",
            r.signature,
            r.slot.0,
            r.vault.label(),
            sol(r.amount),
            if r.collect_instruction_seen {
                ""
            } else {
                " (no collect instruction seen -- corroboration only, still counted)"
            }
        ));
    }

    lines.push(String::new());
    let prefix = if unread.is_empty() { "" } else { "at least " };
    for kind in [VaultKind::PumpFun, VaultKind::PumpSwap] {
        let total: u64 = receipts
            .iter()
            .filter(|r| r.vault == kind)
            .map(|r| r.amount)
            .sum();
        lines.push(format!(
            "{prefix}{} SOL received into the {}",
            sol(total),
            kind.label()
        ));
    }

    lines.push(String::new());
    match pumpfun_unclaimed {
        Some((amount, slot)) => lines.push(format!(
            "{} SOL unclaimed in the pump.fun creator vault (slot {slot})",
            sol(amount)
        )),
        None => lines.push("pump.fun creator vault: could not be read".to_owned()),
    }
    match pumpswap_unclaimed {
        Some((amount, slot)) => lines.push(format!(
            "{} SOL unclaimed in the PumpSwap coin-creator vault (slot {slot})",
            sol(amount)
        )),
        None => lines.push("PumpSwap coin-creator vault: could not be read".to_owned()),
    }

    if !unread.is_empty() {
        lines.push(String::new());
        lines.push("unread (totals above are at least, not exact):".to_owned());
        for u in unread {
            if u.signature.is_empty() {
                lines.push(format!("  {}", u.why));
            } else {
                lines.push(format!("  {}: {}", u.signature, u.why));
            }
        }
    }

    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use realorrug_types::Slot;

    fn addr(byte: u8) -> Address {
        Address::new([byte; 32])
    }

    #[test]
    fn missing_treasury_is_refused() {
        let err = run(&["treasury".to_owned(), "solana".to_owned()]).unwrap_err();
        assert!(err.contains("--treasury"), "{err}");
    }

    #[test]
    fn a_report_with_no_receipts_still_names_both_vaults() {
        let text = report(&addr(1), None, &[], &[], None, None);
        assert!(text.contains("(none read)"), "{text}");
        assert!(text.contains("pump.fun creator vault: could not be read"), "{text}");
        assert!(
            text.contains("PumpSwap coin-creator vault: could not be read"),
            "{text}"
        );
    }

    #[test]
    fn unread_items_prefix_totals_with_at_least() {
        let receipt = Receipt {
            signature: "sig1".to_owned(),
            slot: Slot(10),
            vault: VaultKind::PumpFun,
            amount: 1_000_000_000,
            collect_instruction_seen: true,
        };
        let unread = Unread {
            signature: "sig2".to_owned(),
            why: "transaction could not be read".to_owned(),
        };
        let text = report(&addr(1), None, &[receipt], &[unread], None, None);
        assert!(text.contains("at least 1.000000000 SOL"), "{text}");
        assert!(text.contains("unread (totals above are at least"), "{text}");
    }

    #[test]
    fn a_mint_header_says_it_is_header_only() {
        let text = report(&addr(1), Some(&addr(2)), &[], &[], None, None);
        assert!(text.contains("header only"), "{text}");
    }
}
