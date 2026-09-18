// SPDX-License-Identifier: Apache-2.0
//! One typed selection service: what leads, ranked by what it is, never by
//! what it is called.
//!
//! # The bug this replaces
//!
//! A live Robinhood reply shipped the launch age and the block number while
//! 529 holders and a 50.2% top-holder share sat measured on the fact sheet
//! and unused. The cause was not a missing fact -- it was *how* a fact
//! reached the reply: [`crate::verdict::template`], [`crate::verdict::headline`]
//! and [`crate::voice::request_for`] each matched a fragment of [`Fact::label`]
//! against their own hand-written list, and the label list that mattered was
//! Solana-only. Renaming a label, or writing a new one for a fact that
//! already existed, silently dropped it from every one of those lists at
//! once, and nothing would fail: `find` returns `None` for a fragment that no
//! longer matches, and `None` reads exactly like "this sheet has nothing to
//! say here."
//!
//! [`Fact::kind`] already exists and already survives a label rewrite --
//! [`crate::clause::Kind`] is the stable name a selection, a log line or a
//! receipt refers to. This module is the one place that ranks by it, so a
//! reworded label can change what a reader sees a fact *called* and never
//! change whether it is said at all.
//!
//! # Bundles, not single facts
//!
//! A concentration share means nothing without knowing whose share it is and
//! what it is a share *of*. [`Candidate`] groups the kinds that make a
//! finding legible -- [`Kind::Holders`] travels with [`Kind::LargestHolderShare`]
//! -- so a selection never surfaces "50.2%" without the denominator and the
//! (un)identified role beside it.
//!
//! # Materiality is not the same question as attribution
//!
//! A large balance at an address whose role was never established is still a
//! material fact -- concentration risk exists whether or not anyone knows who
//! holds the balance -- but it is not evidence of a person able to act on it.
//! [`concentration`] therefore always writes the unresolved-role sentence
//! ("the biggest balance is still unidentified") rather than "a whale" or "one
//! wallet can dump": nothing on the sheet resolves an address to a role, so
//! nothing here is entitled to claim one. A future slice that adds a resolved
//! `TradeActor`/`Holding` role (design doc §2.1) is what would let this
//! module say more.

use crate::clause::Kind;
use crate::sheet::{Fact, FactSheet};
use std::fmt;

/// A stable name for a ranked candidate: the [`Kind`]s whose facts it bundles.
///
/// Two candidates are the same candidate if they bundle the same kinds, no
/// matter what either fact's `label` says this week. This is what makes the
/// "renaming a label cannot change selection" property checkable: a test
/// renames `Fact::label` and asserts the [`CandidateId`] the fact produces,
/// and therefore its rank, is unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CandidateId(pub Vec<Kind>);

impl fmt::Display for CandidateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<String> = self.0.iter().map(|k| format!("{k:?}")).collect();
        write!(f, "{}", names.join("+"))
    }
}

/// One ranked, renderable candidate: a bundle of facts and the sentence built
/// from them.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// The kinds this candidate bundles, for logging and for re-identifying
    /// the same candidate across the headline, the template and the model
    /// request.
    pub id: CandidateId,
    /// Rank order: higher goes first. Not a probability, not a score shown to
    /// a reader -- an ordering, the same purpose [`crate::verdict::Level`]
    /// refuses to be one for severity.
    pub priority: i64,
    /// The sentence this candidate contributes, already honest about role and
    /// denominator. Never empty -- a candidate with nothing to say is not
    /// produced.
    pub sentence: String,
}

/// Finds the fact of a given kind, ignoring its label entirely.
///
/// The one lookup every caller in this module goes through, so "matched by
/// kind, not label" is true by construction rather than by every call site
/// remembering to do it right.
fn fact(sheet: &FactSheet, kind: Kind) -> Option<&Fact> {
    sheet
        .facts
        .iter()
        .find(|f| f.kind == kind && !f.rendered.is_empty())
}

/// The concentration bundle: how many addresses hold the token, and what the
/// largest one's share is when that is known.
///
/// Ranked high by design: a share of the float sitting at one address is
/// exactly the kind of fact that changes what a reader does next, which is
/// what refutation 6 (design doc §1) calls the sharpest number the sheet
/// carries. It never outranks a proven creator record, because a repeated
/// launcher with a measured failure rate is the one fact this analyst has
/// that nobody else can compute; but it beats every other Robinhood-only
/// fact, including the age and the block the sheet was read at, which are
/// printed unconditionally and separately from this ranking (see
/// [`crate::verdict::template`]).
fn concentration(sheet: &FactSheet) -> Option<Candidate> {
    let holders = fact(sheet, Kind::Holders)?;
    let share = fact(sheet, Kind::LargestHolderShare);
    let sentence = match share {
        // Deliberately never "a whale" or "one wallet can dump": nothing here
        // resolves the address to a role, so the sentence says exactly that --
        // an unresolved concentration fact, not a person able to act on it.
        Some(share) => format!(
            "{} addresses hold it, but the biggest balance -- {} of it -- is still unidentified.",
            holders.rendered, share.rendered
        ),
        None => format!(
            "{} addresses hold it, not counting the bonding curve.",
            holders.rendered
        ),
    };
    let mut id = vec![Kind::Holders];
    if share.is_some() {
        id.push(Kind::LargestHolderShare);
    }
    Some(Candidate {
        id: CandidateId(id),
        priority: 90,
        sentence,
    })
}

/// The creator-record bundle: launches by this address, and how many ever
/// filled a curve over time.
///
/// Ranked above concentration: it is specific to this creator rather than to
/// the token's current holder set, checkable against Radar's own record, and
/// -- per the design doc's account of three identical replies on 2026-09-04 --
/// the one fact that reliably differs from one coin to the next.
fn creator_record(sheet: &FactSheet) -> Option<Candidate> {
    let launched = fact(sheet, Kind::CreatorLaunches)?;
    let organic = fact(sheet, Kind::CreatorOrganic)?;
    Some(Candidate {
        id: CandidateId(vec![Kind::CreatorLaunches, Kind::CreatorOrganic]),
        priority: 100,
        sentence: format!(
            "{} launches by this creator. {} ever filled a curve.",
            launched.rendered, organic.rendered
        ),
    })
}

/// The launch-block recipient bundle: how many distinct accounts were paid in
/// the block that created the token.
///
/// Below the creator record and concentration: a count with no per-creator or
/// per-holder denominator to weigh it against, useful mainly when neither of
/// the stronger bundles has anything to say.
fn launch_recipients(sheet: &FactSheet) -> Option<Candidate> {
    let recipients = fact(sheet, Kind::LaunchRecipients)?;
    Some(Candidate {
        id: CandidateId(vec![Kind::LaunchRecipients]),
        priority: 70,
        sentence: format!(
            "{} token accounts were paid in the launch block.",
            recipients.rendered
        ),
    })
}

/// The shared-funder bundle: one address sent value to several of the early
/// buyers that were checked, before their first purchase (design 0027 slice 3).
///
/// When that address funded a majority of the checked buyers it ranks just
/// below a creator record and above concentration: a coordinated-looking
/// launch is what a reader most wants to know and cannot see on a chart. A
/// minority stays below concentration, because two of nine is a pattern an
/// exchange's withdrawals produce every day. Without the checked count there
/// is no majority to establish, so it ranks as a minority -- absent is not
/// "all of them" (rule 8). The sentence says flow, never who is behind it: an
/// exchange hot wallet produces exactly this shape.
fn shared_funder(sheet: &FactSheet) -> Option<Candidate> {
    let shared = fact(sheet, Kind::SharedFunder)?;
    let checked = fact(sheet, Kind::FundingChecked);
    let majority = match (
        shared.values.first(),
        checked.and_then(|c| c.values.first()),
    ) {
        (Some(funded), Some(checked)) => funded * 2.0 > *checked,
        _ => false,
    };
    let mut id = vec![Kind::SharedFunder];
    if checked.is_some() {
        id.push(Kind::FundingChecked);
    }
    Some(Candidate {
        id: CandidateId(id),
        priority: if majority { 95 } else { 80 },
        sentence: format!(
            "One address sent value to {} of the early buyers checked, before their first \
             buy -- a flow between addresses, which an exchange also produces, not proof \
             of who is behind them.",
            shared.rendered
        ),
    })
}

/// The market bundle: a dated USD price and/or market cap (design 0027
/// §2.2, ADR 0033).
///
/// Ranked below every risk-bearing bundle above -- `launch_recipients` at 70
/// is the lowest of those, and this sits under it at 40 -- because a price
/// is a fact about the market's current opinion, not about anything this
/// analyst measured for itself the way a creator record or a concentration
/// share is. AGENTS.md §3 rule 5: price is stated with its moment, never
/// leads a reply, and never a hint to buy, sell or hold.
fn market(sheet: &FactSheet) -> Option<Candidate> {
    let facts: Vec<&Fact> = sheet
        .facts
        .iter()
        .filter(|f| f.kind == Kind::Market && !f.rendered.is_empty())
        .collect();
    if facts.is_empty() {
        return None;
    }
    let sentence = facts
        .iter()
        .filter_map(|f| {
            f.clauses
                .iter()
                .find(|c| c.voice == crate::clause::Voice::Plain)
                .map(|c| c.text.clone())
        })
        .collect::<Vec<_>>()
        .join(" ");
    if sentence.is_empty() {
        return None;
    }
    Some(Candidate {
        id: CandidateId(vec![Kind::Market]),
        priority: 40,
        sentence,
    })
}

/// Every candidate this sheet supports, ranked highest priority first.
///
/// **The one ranking every caller shares.** [`crate::verdict::headline`],
/// [`crate::verdict::template`]'s lead facts and [`crate::voice::request_for`]'s
/// suggested lead all read this list rather than keeping their own; adding a
/// new bundle here is what makes it available to all three at once, and
/// removing one removes it from all three, instead of three edits that can
/// drift apart the way the old per-file `LEAD` lists did.
#[must_use]
pub fn rank(sheet: &FactSheet) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = [
        creator_record(sheet),
        shared_funder(sheet),
        concentration(sheet),
        launch_recipients(sheet),
        market(sheet),
    ]
    .into_iter()
    .flatten()
    .collect();
    // Stable: two candidates never share a priority today, but a tie should
    // keep the order they were considered in rather than an implementation
    // detail of the sort.
    candidates.sort_by_key(|c| std::cmp::Reverse(c.priority));
    candidates
}

/// The single best candidate, if the sheet supports any.
#[must_use]
pub fn lead(sheet: &FactSheet) -> Option<Candidate> {
    rank(sheet).into_iter().next()
}

/// Every candidate's id, ranked best first, for the log: the selected lead is
/// the first entry and every entry after it was measured, ranked and not
/// used. Recorded regardless of what a model reply actually opens with, so a
/// reviewer can compare the two without re-deriving this ranking from the raw
/// sheet.
#[must_use]
pub fn candidate_log(sheet: &FactSheet) -> Vec<String> {
    rank(sheet).into_iter().map(|c| c.id.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clause::Voice;
    use crate::sheet::About;

    fn holders_fact(label: &str) -> Fact {
        Fact::exact(Kind::Holders, label, 529.0, "529")
    }

    fn share_fact(label: &str) -> Fact {
        Fact::exact(Kind::LargestHolderShare, label, 0.502, "50.2%")
    }

    fn sheet_with(facts: Vec<Fact>) -> FactSheet {
        FactSheet {
            mint: "0xTest".to_owned(),
            read_at: None,
            facts,
            untrusted: Vec::new(),
            unknown: Vec::new(),
            signals: Vec::new(),
            twins: Vec::new(),
        }
    }

    /// Renaming a fact's display label must not change whether -- or how --
    /// it is selected. This is the property the whole module exists for: the
    /// old defect was a label list that only matched one chain's wording.
    #[test]
    fn renaming_the_label_does_not_change_selection() {
        let original = sheet_with(vec![
            holders_fact("addresses holding the token now"),
            share_fact("held by the single largest address"),
        ]);
        let renamed = sheet_with(vec![
            holders_fact("a totally different way of saying the same thing"),
            share_fact("also renamed, still the same measurement"),
        ]);

        let original_ids: Vec<CandidateId> = rank(&original).into_iter().map(|c| c.id).collect();
        let renamed_ids: Vec<CandidateId> = rank(&renamed).into_iter().map(|c| c.id).collect();
        assert_eq!(original_ids, renamed_ids);
        assert!(!original_ids.is_empty());

        // The rendered sentence is also unaffected: it is built from `kind`
        // and `rendered`, never from `label`.
        let original_lead = lead(&original).expect("concentration bundle present");
        let renamed_lead = lead(&renamed).expect("concentration bundle present");
        assert_eq!(original_lead.sentence, renamed_lead.sentence);
    }

    /// The Robinhood fixture from the design doc: 529 holders, a 50.2% top
    /// share whose role is unknown. Both surface, bundled, with
    /// unresolved-role wording -- never "whale," never "one wallet can dump."
    #[test]
    fn robinhood_concentration_surfaces_with_unresolved_role_wording() {
        let sheet = sheet_with(vec![
            holders_fact("addresses holding the token now"),
            share_fact("held by the single largest address"),
        ]);
        let top = lead(&sheet).expect("a candidate exists");
        assert_eq!(
            top.id,
            CandidateId(vec![Kind::Holders, Kind::LargestHolderShare])
        );
        assert!(top.sentence.contains("529"));
        assert!(top.sentence.contains("50.2%"));
        assert!(top.sentence.contains("unidentified"));
        assert!(!top.sentence.to_lowercase().contains("whale"));
        assert!(!top.sentence.to_lowercase().contains("dump"));
    }

    /// A holder count with no resolved top share still ranks and still reads
    /// honestly -- absent is not "no address holds a large share," it is
    /// unmeasured, and the sentence never claims otherwise.
    #[test]
    fn holders_alone_still_ranks_without_inventing_a_share() {
        let sheet = sheet_with(vec![holders_fact("addresses holding the token now")]);
        let top = lead(&sheet).expect("a candidate exists");
        assert_eq!(top.id, CandidateId(vec![Kind::Holders]));
        assert!(top.sentence.contains("529"));
        assert!(!top.sentence.contains('%'));
    }

    /// A contract or pool balance must not lead merely because it is large: a
    /// proven, denominated creator record still outranks concentration, and
    /// concentration in turn is ranked as an unresolved fact, never promoted
    /// past a genuinely stronger, attributed finding.
    #[test]
    fn creator_record_outranks_concentration() {
        let sheet = sheet_with(vec![
            Fact::exact(
                Kind::CreatorLaunches,
                "tokens this creator has launched",
                12.0,
                "12",
            ),
            Fact::exact(
                Kind::CreatorOrganic,
                "how many reached an AMM by filling over time",
                0.0,
                "0",
            ),
            holders_fact("addresses holding the token now"),
            share_fact("held by the single largest address"),
        ]);
        let ranked = rank(&sheet);
        assert_eq!(
            ranked[0].id,
            CandidateId(vec![Kind::CreatorLaunches, Kind::CreatorOrganic])
        );
        assert_eq!(
            ranked[1].id,
            CandidateId(vec![Kind::Holders, Kind::LargestHolderShare])
        );
    }

    fn funding_sheet(funded: u32, checked: Option<u32>) -> FactSheet {
        let mut facts = vec![
            holders_fact("addresses holding the token now"),
            share_fact("held by the single largest address"),
            Fact::exact(
                Kind::SharedFunder,
                "checked early buyers one address funded",
                f64::from(funded),
                format!("{funded} of {}", checked.unwrap_or(0)),
            ),
        ];
        if let Some(checked) = checked {
            facts.push(Fact::exact(
                Kind::FundingChecked,
                "early buyers whose funding was checked",
                f64::from(checked),
                format!("{checked} of {checked}"),
            ));
        }
        sheet_with(facts)
    }

    /// The fact slice 3 measured and nothing surfaced: one address funding
    /// most checked buyers now leads over concentration, bundled with the
    /// checked count, in flow wording that never names who is behind it.
    #[test]
    fn a_majority_shared_funder_leads_over_concentration() {
        let top = lead(&funding_sheet(3, Some(4))).expect("a candidate exists");
        assert_eq!(
            top.id,
            CandidateId(vec![Kind::SharedFunder, Kind::FundingChecked])
        );
        assert_eq!(top.priority, 95);
        assert!(top.sentence.contains("3 of 4"), "{}", top.sentence);
        assert!(top.sentence.contains("exchange"), "{}", top.sentence);
        for word in ["one person", "insider", "team", "controlled", "sybil"] {
            assert!(!top.sentence.to_lowercase().contains(word), "{word}");
        }
    }

    /// Two of four (exactly half) and four of nine are not majorities and
    /// stay below concentration; five of nine is one and leads.
    #[test]
    fn a_minority_shared_funder_ranks_below_concentration() {
        for (funded, checked) in [(2, 4), (4, 9)] {
            let ranked = rank(&funding_sheet(funded, Some(checked)));
            assert_eq!(ranked[0].id.0[0], Kind::Holders, "{funded} of {checked}");
            assert_eq!(ranked[1].id.0[0], Kind::SharedFunder);
            assert_eq!(ranked[1].priority, 80);
        }
        let ranked = rank(&funding_sheet(5, Some(9)));
        assert_eq!(ranked[0].priority, 95, "five of nine is a majority");
    }

    /// Without the checked count there is no majority to establish: the
    /// shared funder still ranks, as a minority, and claims no denominator
    /// in its id.
    #[test]
    fn a_shared_funder_without_a_checked_count_is_not_a_majority() {
        let ranked = rank(&funding_sheet(3, None));
        let shared = ranked
            .iter()
            .find(|c| c.id.0[0] == Kind::SharedFunder)
            .expect("still ranked");
        assert_eq!(shared.id, CandidateId(vec![Kind::SharedFunder]));
        assert_eq!(shared.priority, 80);
    }

    /// Silences an unused-import warning for `About`/`Voice` if a future edit
    /// removes the only use above; kept imported because most fixtures in
    /// this crate build facts with `.saying(...)`, and a contributor copying
    /// one of the tests above should not have to hunt for these imports.
    #[test]
    fn about_and_voice_are_reachable_from_this_module() {
        let _ = About::Measurement;
        let _ = Voice::Plain;
    }
}
