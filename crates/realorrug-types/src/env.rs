// SPDX-License-Identifier: Apache-2.0
//! Reading an environment variable that was renamed.
//!
//! The project was renamed from Radar to realorrug, and every `RADAR_*`
//! variable this workspace reads is being renamed to `REALORRUG_*` (same
//! suffix) to match. The production box's env files still use the old names
//! -- only the owner edits them, by hand, on a machine this code does not
//! reach -- so a binary that only understood the new name would go dark on
//! its next deploy for no reason a log line explains.
//!
//! [`env_or_legacy`] is the one place that trade-off lives: read the new
//! name first, and if it is absent, read the old one and say so once. This
//! function -- and every call site that uses it -- exists **only** until the
//! box's env files are renamed; delete both when that happens.

/// Reads `new`, falling back to `old` when `new` is absent.
///
/// `lookup` takes a key rather than this function taking the process
/// environment directly so tests can supply a map instead of mutating
/// `std::env` (which is `unsafe` in edition 2024 and racy against other
/// tests running in parallel threads).
///
/// Neither set is `None`, exactly as it was before either name existed --
/// AGENTS.md rule 7, deny by default, is a property of the caller and this
/// function does not weaken it. When only `old` is set, a single warning
/// names `new` (never the value: rule 8, a secret does not belong in a log)
/// so an operator has one line telling them what to rename.
pub fn env_or_legacy(
    new: &str,
    old: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    if let Some(v) = lookup(new) {
        return Some(v);
    }
    lookup(old).inspect(|_| {
        eprintln!(
            "realorrug: {old} is set but {new} is not -- reading the old name for now. \
             Rename it in the box's env file; this fallback is deleted once every box is."
        );
    })
}

#[cfg(test)]
mod tests {
    use super::env_or_legacy;
    use std::collections::HashMap;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |k: &str| map.get(k).cloned()
    }

    #[test]
    fn the_new_name_wins_when_both_are_set() {
        let get = vars(&[("NEW", "new-value"), ("OLD", "old-value")]);
        assert_eq!(
            env_or_legacy("NEW", "OLD", &get),
            Some("new-value".to_owned())
        );
    }

    #[test]
    fn the_old_name_alone_still_works() {
        let get = vars(&[("OLD", "old-value")]);
        assert_eq!(
            env_or_legacy("NEW", "OLD", &get),
            Some("old-value".to_owned())
        );
    }

    #[test]
    fn neither_set_is_none() {
        let get = vars(&[]);
        assert_eq!(env_or_legacy("NEW", "OLD", &get), None);
    }
}
