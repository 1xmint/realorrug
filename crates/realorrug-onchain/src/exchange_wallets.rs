// SPDX-License-Identifier: Apache-2.0
//! A dated, sourced list of known Solana exchange withdrawal wallets
//! (design 0031 §3, [research 0061](../../../docs/research/0061-solana-exchange-withdrawal-wallets.md)).
//!
//! Withdrawal wallets pay out to thousands of strangers. When one of them
//! shows up as a "shared funder" of a launch's early buyers, "the same
//! address funded 3 of the 4" is true and misleading -- the wallet did not
//! choose those buyers, it pays whoever asked to withdraw. This table lets
//! [`crate::wallets::shared_funders`] leave a listed address out of that
//! tally and say what it actually is instead.
//!
//! Exchanges rotate hot wallets, so the list is dated rather than assumed
//! current forever; research 0061's own "Refreshing it" section is how it
//! gets updated. **Absence from this list is not a claim that an address is
//! not an exchange** -- KuCoin and MEXC, for two, had no labelled Solana
//! withdrawal wallet to find (AGENTS.md rule 8: absent is not zero).

/// One entry in the table: an exchange's withdrawal wallet, as labelled by
/// one source on one date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExchangeWallet {
    /// The exchange's name, as the sheet should say it.
    pub exchange: &'static str,
    /// The address, in Solana's base58 text form.
    pub address: &'static str,
    /// The labelling service the address and name came from.
    pub source: &'static str,
    /// The date the label was read, `YYYY-MM-DD`.
    pub checked: &'static str,
}

/// The eight addresses research 0061 measured on 2026-09-23, reading
/// Solscan's account pages in a browser (a plain fetch gets HTTP 403).
const EXCHANGE_WALLETS: &[ExchangeWallet] = &[
    ExchangeWallet {
        exchange: "Binance",
        address: "5tzFkiKscXHK5ZXCGbXZxdw7gTjjD1mBwuoFbhUvuAi9",
        source: "Solscan",
        checked: "2026-09-23",
    },
    ExchangeWallet {
        exchange: "Coinbase",
        address: "GJRs4FwHtemZ5ZE9x3FNvJ8TMwitKTh21yxdRPqn7npE",
        source: "Solscan",
        checked: "2026-09-23",
    },
    ExchangeWallet {
        exchange: "OKX",
        address: "C68a6RCGLiPskbPYtAcsCjhG8tfTWYcoB4JjCrXFdqyo",
        source: "Solscan",
        checked: "2026-09-23",
    },
    ExchangeWallet {
        exchange: "Bybit",
        address: "AC5RDfQFmDS1deWZos921JfqscXdByf8BKHs5ACWjtW2",
        source: "Solscan",
        checked: "2026-09-23",
    },
    ExchangeWallet {
        exchange: "Kraken",
        address: "FWznbcNXWQuHTawe9RxvQ2LdCENssh12dsznf4RiouN5",
        source: "Solscan",
        checked: "2026-09-23",
    },
    ExchangeWallet {
        exchange: "Gate",
        address: "u6PJ8DtQuPFnfmwHbGFULQ4u4EgjDiyYKjVEsynXq2w",
        source: "Solscan",
        checked: "2026-09-23",
    },
    ExchangeWallet {
        exchange: "HTX",
        address: "BY4StcU9Y2BpgH8quZzorg31EGE4L1rjomN8FNsCBEcx",
        source: "Solscan",
        checked: "2026-09-23",
    },
    ExchangeWallet {
        exchange: "Crypto.com",
        address: "AobVSwdW9BbpMdJvTqeCN4hPAmh4rHm7vwLnQ5ATSyrS",
        source: "Solscan",
        checked: "2026-09-23",
    },
];

/// Looks `address` up in [`EXCHANGE_WALLETS`], an exact text match (Solana
/// base58 is case-sensitive, so no case-folding).
///
/// Returns `None` for an address not on the list -- which is not a claim
/// that the address is not an exchange, only that this dated read did not
/// find a label for it (AGENTS.md rule 8).
#[must_use]
pub fn exchange_withdrawal_wallet(address: &str) -> Option<&'static ExchangeWallet> {
    EXCHANGE_WALLETS.iter().find(|w| w.address == address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_finds_binance() {
        let found = exchange_withdrawal_wallet("5tzFkiKscXHK5ZXCGbXZxdw7gTjjD1mBwuoFbhUvuAi9")
            .expect("Binance's wallet is on the list");
        assert_eq!(found.exchange, "Binance");
        assert_eq!(found.source, "Solscan");
        assert_eq!(found.checked, "2026-09-23");
    }

    #[test]
    fn miss_is_none() {
        // A real Solana address (a well-known burn address), never labelled
        // an exchange in research 0061 -- absence, not a false hit.
        assert!(
            exchange_withdrawal_wallet("1nc1nerator11111111111111111111111111111111").is_none()
        );
    }

    #[test]
    fn lookup_is_case_sensitive() {
        // Solana base58 is case-sensitive; a lower-cased address is a
        // different, non-existent address, not a fuzzy match on the same one.
        let lowered = "5tzfkikscxhk5zxcgbxzxdw7gtjjd1mbwuofbhuvui9";
        assert!(exchange_withdrawal_wallet(lowered).is_none());
    }

    #[test]
    fn every_entry_round_trips() {
        for entry in EXCHANGE_WALLETS {
            let found = exchange_withdrawal_wallet(entry.address)
                .expect("every table address must be found by its own lookup");
            assert_eq!(found, entry);
        }
    }
}
