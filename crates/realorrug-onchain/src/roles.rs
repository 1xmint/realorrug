// SPDX-License-Identifier: Apache-2.0
//! Who a holder address *is*, only when something proves it -- design 0027
//! §2.2's "Graduation, custody and powers" and §2.1's third distinction:
//! supply held, supply available to sell and executable exit depth are
//! different numbers, and a pool's balance is not "safe" just because it is a
//! known contract.
//!
//! # Proof, not guessing
//!
//! `robinhood.rs`'s existing holder walk already drops the zero address, the
//! curve and the factory from concentration -- but it can do that only
//! because those three addresses are *read off the same verified factory
//! record the rest of the dossier trusts* (`LaunchedToken::curve`,
//! `pons::FACTORY`, the zero address by definition), not because "big
//! balance in a contract" implies infrastructure. This module makes that
//! proof requirement a type: a [`RoleClaim`] must carry a [`Proof`], and
//! [`concentration`] only removes an address from the denominator when a
//! claim for it exists. Every other large holder keeps unresolved-role
//! wording -- "an address holding X%, role not established" -- never a role
//! the sheet did not establish (design 0027 §3 row 4's "done means").
//!
//! A locker is exactly this: this module never hardcodes a locker address,
//! because no locker contract has been read off deployed bytecode the way
//! `pons::curve`'s selectors were (task packet 0024's methodology). A caller
//! that *has* verified one -- by code identity, or by decoding a lock event
//! naming this token -- supplies it as a [`RoleClaim`] with the matching
//! [`Proof`]; nothing here manufactures that proof on its own.

use realorrug_robinhood::Address;

/// A role a holder address can play in a launch's own machinery.
///
/// Not "whale" or "team" -- those are ownership claims this module never
/// makes. Every variant here is infrastructure a launch mechanically needs,
/// which is why proving one only removes an address from the denominator; it
/// never promotes another address into one of these roles by elimination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// A liquidity pool holding the token against its quote asset.
    Pool,
    /// The bonding curve itself.
    Curve,
    /// The launch factory.
    Factory,
    /// A locker holding tokens under a lock contract's own rules.
    Locker,
    /// The zero address: burned or never-minted supply.
    Zero,
}

impl Role {
    /// The word the sheet may use for this role, on its own -- never
    /// "whale," "team" or "insider" for any of these, because none of them
    /// is a claim about a person.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Role::Pool => "the pool",
            Role::Curve => "the bonding curve",
            Role::Factory => "the launch factory",
            Role::Locker => "a locker",
            Role::Zero => "the zero address",
        }
    }
}

/// What established a [`RoleClaim`].
///
/// AGENTS.md §1 -- every claim is backed by something that runs -- applied to
/// roles specifically: a role is either read off an address this dossier
/// already verified by construction (the curve and factory addresses come
/// from the factory's own record; the zero address is a fixed constant), or
/// it is read off a decoded event that names this exact token and address
/// (a lock deposit, a pool-creation log). Neither is "this looks like a
/// contract" or "this address holds a lot."
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Proof {
    /// The address is one this dossier already verified by construction,
    /// named here for the reader (e.g. `"factory record: curve"`).
    VerifiedAddress(&'static str),
    /// A decoded on-chain event established the role, named here (e.g.
    /// `"Locked(token, amount) at 0x123..."`).
    DecodedEvent(String),
}

/// One holder address a dossier has proof for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleClaim {
    /// The address the claim is about.
    pub address: Address,
    /// The role it plays.
    pub role: Role,
    /// What established it.
    pub proof: Proof,
}

/// The zero, curve and factory claims every Robinhood dossier already has by
/// construction: `curve` and `factory` are read from the verified factory
/// record (`LaunchedToken`), not guessed from behaviour.
#[must_use]
pub fn verified_infrastructure(curve: Address, factory: Address) -> Vec<RoleClaim> {
    vec![
        RoleClaim {
            address: Address::ZERO,
            role: Role::Zero,
            proof: Proof::VerifiedAddress("the zero address is a fixed constant"),
        },
        RoleClaim {
            address: curve,
            role: Role::Curve,
            proof: Proof::VerifiedAddress("factory record: curve"),
        },
        RoleClaim {
            address: factory,
            role: Role::Factory,
            proof: Proof::VerifiedAddress("factory record: launch factory"),
        },
    ]
}

/// A holder's share once role-proven infrastructure is excluded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleHolder {
    /// The address.
    pub address: Address,
    /// Its balance.
    pub balance: u128,
    /// Its share of the denominator, in basis points.
    pub share_bps: u16,
}

/// Concentration computed with proven roles removed from both sides of the
/// ratio, and every other large holder kept unresolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Concentration {
    /// The sum of balances counted in the denominator: every holder except
    /// those a [`RoleClaim`] proved to be infrastructure.
    pub denominator: u128,
    /// The largest holder among those *not* proven to be infrastructure,
    /// with its share of [`Concentration::denominator`]. This is the
    /// role-correct concentration figure design 0027 §3 row 4 calls for --
    /// its denominator is always stated alongside it.
    pub largest_non_infrastructure: Option<RoleHolder>,
    /// Every non-infrastructure holder at or above
    /// [`UNRESOLVED_THRESHOLD_BPS`], largest first. None of these may be
    /// called a role the sheet did not establish; the wording that renders
    /// this list must say "role not established," never a guess.
    pub unresolved_large: Vec<RoleHolder>,
    /// Proven roles found among the holders passed in, for the sheet to cite
    /// (e.g. "the pool holds 41% of supply, excluded from concentration").
    pub infrastructure: Vec<(RoleHolder, Role, Proof)>,
}

/// A holder is large enough to name, unresolved or not, at 5% of the
/// denominator. Chosen because it is the threshold `robinhood.rs`'s own
/// concentration reporting already treats as worth a sentence; a smaller
/// figure is noise among ordinary buyers.
pub const UNRESOLVED_THRESHOLD_BPS: u16 = 500;

/// Splits `balances` into proven infrastructure and everyone else, then
/// computes concentration over everyone else only.
///
/// `balances` should already exclude the machinery `robinhood.rs`'s own
/// holder walk always drops (curve, factory, zero) -- but this function does
/// not assume that: any address in `balances` that also has a [`RoleClaim`]
/// in `proven` is excluded here too, so passing the same claims twice is
/// harmless, and a caller that forgot to drop the curve address upstream
/// still gets a correct denominator rather than a silently wrong one.
#[must_use]
pub fn concentration(balances: &[(Address, u128)], proven: &[RoleClaim]) -> Concentration {
    let proof_for = |a: &Address| proven.iter().find(|c| c.address == *a);

    let mut infrastructure = Vec::new();
    let mut rest: Vec<(Address, u128)> = Vec::new();
    for &(address, balance) in balances {
        if let Some(claim) = proof_for(&address) {
            infrastructure.push((address, balance, claim.role, claim.proof.clone()));
        } else {
            rest.push((address, balance));
        }
    }

    let denominator: u128 = rest.iter().fold(0u128, |s, (_, b)| s.saturating_add(*b));
    let share_bps = |balance: u128| -> u16 {
        balance
            .saturating_mul(10_000)
            .checked_div(denominator)
            .and_then(|bps| u16::try_from(bps).ok())
            .unwrap_or(if denominator == 0 { 0 } else { u16::MAX })
    };

    let mut ranked: Vec<RoleHolder> = rest
        .iter()
        .map(|&(address, balance)| RoleHolder {
            address,
            balance,
            share_bps: share_bps(balance),
        })
        .collect();
    ranked.sort_by(|a, b| b.balance.cmp(&a.balance).then(a.address.0.cmp(&b.address.0)));

    let largest_non_infrastructure = ranked.first().cloned();
    let unresolved_large = ranked
        .into_iter()
        .filter(|h| h.share_bps >= UNRESOLVED_THRESHOLD_BPS)
        .collect();

    let infra_holders = infrastructure
        .into_iter()
        .map(|(address, balance, role, proof)| {
            (
                RoleHolder {
                    address,
                    balance,
                    // An infrastructure holder's own "share" is reported
                    // against the total the sheet already prints elsewhere
                    // (total supply held), not this denominator -- callers
                    // that want that number compute it themselves from
                    // `balance`; putting it here would silently mix two
                    // denominators in one struct.
                    share_bps: 0,
                },
                role,
                proof,
            )
        })
        .collect();

    Concentration {
        denominator,
        largest_non_infrastructure,
        unresolved_large,
        infrastructure: infra_holders,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(b: u8) -> Address {
        Address([b; 20])
    }

    #[test]
    fn a_proven_pool_is_excluded_from_both_sides() {
        let pool = addr(0xaa);
        let whale = addr(0x01);
        let small = addr(0x02);
        let balances = vec![(pool, 900), (whale, 60), (small, 40)];
        let proven = vec![RoleClaim {
            address: pool,
            role: Role::Pool,
            proof: Proof::DecodedEvent("PairCreated(token, pool) at 0xdead".to_owned()),
        }];
        let c = concentration(&balances, &proven);
        assert_eq!(c.denominator, 100, "the pool's 900 must not count");
        let largest = c.largest_non_infrastructure.expect("a non-infra holder");
        assert_eq!(largest.address, whale);
        assert_eq!(largest.share_bps, 6_000, "60/100");
        assert_eq!(c.infrastructure.len(), 1);
        assert_eq!(c.infrastructure[0].1, Role::Pool);
    }

    #[test]
    fn an_unproven_large_holder_stays_unresolved_not_a_role() {
        let big = addr(0xbb);
        let small = addr(0x01);
        // `small` is below the 5% unresolved threshold, so only `big` (80%)
        // should be named as unresolved.
        let balances = vec![(big, 990), (small, 10)];
        // No proof supplied at all -- `big` looks exactly like a pool would,
        // but nothing proves it, so it must not be excluded and must not be
        // assigned any `Role`.
        let c = concentration(&balances, &[]);
        assert_eq!(c.denominator, 1_000);
        assert!(c.infrastructure.is_empty());
        assert_eq!(c.unresolved_large.len(), 1);
        assert_eq!(c.unresolved_large[0].address, big);
        assert_eq!(c.unresolved_large[0].share_bps, 9_900);
        let largest = c.largest_non_infrastructure.expect("largest still reported");
        assert_eq!(largest.address, big, "an unproven whale is still the largest holder");
    }

    #[test]
    fn a_claim_for_an_address_not_present_changes_nothing() {
        let balances = vec![(addr(0x01), 100)];
        let phantom = RoleClaim {
            address: addr(0x99),
            role: Role::Locker,
            proof: Proof::VerifiedAddress("unrelated"),
        };
        let c = concentration(&balances, &[phantom]);
        assert_eq!(c.denominator, 100);
        assert!(c.infrastructure.is_empty());
    }

    #[test]
    fn verified_infrastructure_names_the_zero_curve_and_factory() {
        let curve = addr(0x11);
        let factory = addr(0x22);
        let claims = verified_infrastructure(curve, factory);
        assert_eq!(claims.len(), 3);
        assert!(claims.iter().any(|c| c.address == Address::ZERO && c.role == Role::Zero));
        assert!(claims.iter().any(|c| c.address == curve && c.role == Role::Curve));
        assert!(claims.iter().any(|c| c.address == factory && c.role == Role::Factory));
        for c in &claims {
            assert!(matches!(c.proof, Proof::VerifiedAddress(_)));
        }
    }

    #[test]
    fn an_empty_denominator_reports_no_largest_holder() {
        let pool = addr(0xaa);
        let proven = vec![RoleClaim {
            address: pool,
            role: Role::Pool,
            proof: Proof::VerifiedAddress("test"),
        }];
        let c = concentration(&[(pool, 1_000)], &proven);
        assert_eq!(c.denominator, 0);
        assert!(c.largest_non_infrastructure.is_none());
        assert!(c.unresolved_large.is_empty());
    }
}
