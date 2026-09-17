// SPDX-License-Identifier: Apache-2.0
//! The named first-party address list: addresses that launch or receive by
//! design, not by coordination.
//!
//! # Why this exists, and why it is applied before the floor
//!
//! `Signal::RepeatLauncher` (`sheet.rs`) measures how many times an address
//! has launched. The Pons v2 factory (`realorrug-robinhood/src/pons.rs`'s
//! `FACTORY`) and its fee escrow (`realorrug-robinhood/src/escrow.rs`'s
//! `ESCROW`) launch or receive on **every** token on the chain, by design, not
//! by coordination. Leave them in the population `creator.rs`'s floor is
//! measured against and the floor is a measurement about the factory, not
//! about creators -- research 0042 records the Solana version of this
//! contamination happening and being cut out: its `Infrastructure` band
//! *"excludes 13 router/fee-sink addresses covering 42% of launches"*, before
//! the bands are computed, not after.
//!
//! So this list is excluded from the population **first**, in
//! `CreatorIndex::repeat_launcher_floor`, and only then is the floor measured
//! over what is left. Computing the floor first and excluding afterwards
//! gives a floor fitted to a contaminated population, and nothing downstream
//! can tell that it happened.
//!
//! # Deny by default when the list is missing
//!
//! AGENTS.md rule 7. If the file is absent or does not parse,
//! `RepeatLauncher` does not fire at all, on any chain -- not "fires without
//! the exclusions": a prevalence measured over a population containing a
//! factory looks like a result, and a wrong result that looks like one is
//! worse than an absent one. This module never falls back on an empty list;
//! a caller that cannot load one passes `None` through to the sheet, and the
//! sheet does not compute a floor at all (see `creator.rs`).
//!
//! # Shape, following `baserates.rs`
//!
//! A `load(path)` and a `parse(text)`, a `NotLoaded` error, serde structs, no
//! HTTP -- the same shape `BaseRates` uses, for the same reason: this is a
//! measurement someone captured and published, not a live lookup.
//!
//! # One file, both chains
//!
//! ADR 0028: one bot, one voice, a chain is data the bot carries. `chain` on
//! each entry, not a second file for a second chain.
//!
//! # What is seeded, and what is not
//!
//! Exactly two entries, both already captured in this repo: the Pons v2
//! factory (research 0048, a verified-source capture) and its fee escrow
//! (research 0036's escrow claim capture). **No Solana entry is seeded.**
//! Radar's 13 router/fee-sink addresses live in another repository, and this
//! one has not captured them -- an address copied out of a link is a
//! reference, not a capture (AGENTS.md rule 1: "a reference proposes, a
//! capture disposes"). Whoever adds a Solana entry later should capture each
//! address the same way these two were: a transaction this repo has read,
//! naming the address, not a citation of Radar's list.

use serde::Deserialize;

/// Where the list lives by default.
pub const DEFAULT_PATH: &str = "docs/research/data/first-party-addresses.json";

/// Why the list could not be used.
#[derive(Debug, thiserror::Error)]
pub enum NotLoaded {
    /// The file was not there or could not be read.
    #[error("first-party list not readable at {path}: {why}")]
    Unreadable {
        /// Where it was looked for.
        path: String,
        /// The underlying reason.
        why: String,
    },
    /// The file was there but is not the shape this expects.
    #[error("first-party list malformed: {0}")]
    Malformed(String),
}

/// Which chain an entry's address is on.
///
/// One list, both chains (ADR 0028) -- this is the tag that keeps a Solana
/// address from ever being compared against a Robinhood Chain creator, or the
/// reverse; the same 20 bytes read as a Solana base58 string and a Robinhood
/// hex string would not collide, but two *different* addresses that happen to
/// render the same short prefix on two chains should never be treated as one
/// entry, and the explicit tag is what rules that out rather than relying on
/// shape alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Chain {
    /// pump.fun and the rest of the Solana surface this bot reads.
    Solana,
    /// Pons v2 and the rest of the Robinhood Chain surface this bot reads.
    Robinhood,
}

impl Chain {
    /// The chain a [`realorrug_types::ChainAddress`] is on, as this module's
    /// own tag.
    ///
    /// A `From` impl would live equally well on either type; it lives here
    /// because this crate already depends on `realorrug_types` and adding the
    /// reverse dependency to grow one conversion function would be the
    /// heavier change.
    #[must_use]
    pub fn of(address: &realorrug_types::ChainAddress) -> Self {
        match address {
            realorrug_types::ChainAddress::Solana(_) => Self::Solana,
            realorrug_types::ChainAddress::Robinhood(_) => Self::Robinhood,
        }
    }
}

/// One named address: what it is, where it was captured, and when.
///
/// Every field is required -- there is no default for "what is this address"
/// or "where did this come from," and a partially-filled entry would be a
/// captured address with an uncaptured justification.
#[derive(Clone, Debug, Deserialize)]
pub struct Entry {
    /// The address, as the chain writes it. Robinhood Chain's hex is
    /// case-insensitive and this repo always renders it lowercase
    /// (`realorrug_robinhood::Address`'s `Display`); Solana's base58 is
    /// case-**sensitive**, so a Solana entry is captured in its exact case,
    /// not lowercased -- "lowercase, as the chain writes it" is not a
    /// contradiction for a chain whose own writing is already lowercase, and
    /// this repo has not yet captured a Solana entry to test the other case
    /// against (see the module doc).
    pub address: String,
    /// Which chain it is on.
    pub chain: Chain,
    /// What it is, in plain words: `"the Pons v2 factory"`.
    pub role: String,
    /// Where the address was captured: a repo path or a document number. A
    /// reference proposes, a capture disposes (AGENTS.md rule 1).
    pub source: String,
    /// The date it was added, `YYYY-MM-DD`.
    pub added: String,
}

/// The whole list, as loaded.
#[derive(Clone, Debug, Deserialize)]
pub struct FirstPartyList {
    entries: Vec<Entry>,
}

impl FirstPartyList {
    /// Reads the list from a path.
    ///
    /// # Errors
    ///
    /// [`NotLoaded`] when the file cannot be read or is not the expected
    /// shape.
    pub fn load(path: &str) -> Result<Self, NotLoaded> {
        let text = std::fs::read_to_string(path).map_err(|e| NotLoaded::Unreadable {
            path: path.to_owned(),
            why: e.to_string(),
        })?;
        Self::parse(&text)
    }

    /// Parses the list.
    ///
    /// # Errors
    ///
    /// [`NotLoaded::Malformed`] when the JSON is not the expected shape.
    /// Unknown top-level keys are ignored deliberately: the file's own header
    /// note (see the module doc) lives beside `entries` in the same object,
    /// and a schema that refused an explanatory key would be an incentive to
    /// leave the explanation out.
    pub fn parse(text: &str) -> Result<Self, NotLoaded> {
        serde_json::from_str(text).map_err(|e| NotLoaded::Malformed(e.to_string()))
    }

    /// Whether this address, on this chain, is on the named list.
    ///
    /// Exact string match against the captured form -- see [`Entry::address`]
    /// on why this deliberately does not lowercase its input before
    /// comparing.
    #[must_use]
    pub fn contains(&self, chain: Chain, address: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.chain == chain && e.address == address)
    }

    /// How many entries the list holds, for a caller that wants to say so.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> &'static str {
        r#"{
            "note": "a header comment, since JSON has none",
            "entries": [
                {
                    "address": "0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e",
                    "chain": "robinhood",
                    "role": "the Pons v2 factory",
                    "source": "crates/realorrug-robinhood/src/pons.rs",
                    "added": "2026-09-15"
                }
            ]
        }"#
    }

    #[test]
    fn a_listed_address_is_found_on_its_own_chain_and_no_other() {
        let list = FirstPartyList::parse(sample()).expect("parses");
        assert!(list.contains(
            Chain::Robinhood,
            "0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e"
        ));
        // Same text, wrong chain: re-applying the bug that drops the chain
        // check out of `contains` would let a Solana lookup on this string
        // match a Robinhood-only entry.
        assert!(!list.contains(Chain::Solana, "0x7ed598bcef8bd9edd8c97a195c6d13f40801ec7e"));
        assert!(!list.contains(Chain::Robinhood, "0xnotlisted"));
        // The empty-list test alone cannot tell a real count from one
        // hard-wired to zero or a hard-wired `true`.
        assert_eq!(list.len(), 1);
        assert!(!list.is_empty());
    }

    #[test]
    fn an_unreadable_path_refuses_rather_than_falling_back() {
        let err = FirstPartyList::load("docs/research/data/does-not-exist.json")
            .expect_err("no such file");
        assert!(matches!(err, NotLoaded::Unreadable { .. }));
    }

    #[test]
    fn a_missing_required_field_is_malformed_not_defaulted() {
        // `source` is required per the module doc: a captured address with no
        // capture is worse than no entry at all, so this must refuse, not
        // silently fill in an empty string.
        let text = r#"{"entries": [{"address": "0xabc", "chain": "robinhood", "role": "x", "added": "2026-09-15"}]}"#;
        let err = FirstPartyList::parse(text).expect_err("source is required");
        assert!(matches!(err, NotLoaded::Malformed(_)));
    }

    #[test]
    fn an_empty_entry_list_still_parses_and_excludes_nothing() {
        let list = FirstPartyList::parse(r#"{"entries": []}"#).expect("parses");
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
        assert!(!list.contains(Chain::Robinhood, "0xanything"));
    }
}
