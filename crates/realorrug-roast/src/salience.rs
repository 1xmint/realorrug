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
use crate::sheet::{Fact, FactSheet, Signal};
use std::collections::HashSet;
use std::fmt;

/// The fact kinds a fired [`Signal`] can be read from, across chains.
///
/// **The one place signal-to-kind is written down.** [`crate::report::signal_kind`]
/// used to keep its own single-`Kind` copy of this table for the report's
/// "Alternative explanations" column; that table said `HolderConcentration`
/// meant `Kind::LargestHolderShare` always, which is only true on Robinhood
/// Chain -- on Solana the same signal is read from `Kind::TokenOwnership`
/// (`push_token_ownership`'s own doc comment; `push_holders`'s for the
/// Robinhood side). A sheet whose only fired signal was `HolderConcentration`
/// off a Solana `TokenOwnership` share found nothing in that single-`Kind`
/// table, so [`rank`] could not tell the fact that earned the level from one
/// that never fired (2026-09-24 replay, case creator-sale-hbull). Returning
/// every kind a signal can be read from, rather than picking one, is what
/// lets [`rank`] recognise either chain's fact as the one that fired; the
/// report still renders a single column and takes this slice's first entry,
/// which stays `Kind::LargestHolderShare` for every existing report test.
///
/// One exhaustive `match`, no `_ =>` arm, the same discipline
/// [`crate::report::would_resolve_text`] holds itself to: a new [`Signal`]
/// variant that is not given a kind here fails to compile.
pub(crate) fn signal_kinds(signal: Signal) -> &'static [Kind] {
    match signal {
        Signal::LaunchBlockInStrongestBand => &[Kind::LaunchRecipients],
        Signal::CreatorNeverGraduatedOrganically => &[Kind::CreatorOrganic],
        Signal::CreatorBoughtOwnLaunch => &[Kind::DevBuy],
        // No dossier constructs `BuyersCannotSell` yet (`sheet.rs`'s own doc
        // comment); the nearest measured kind for both is the curve's own
        // liquidity, which is what a simulated sell reads against and what a
        // drain of reserves is a read of.
        Signal::LiquidityGone | Signal::BuyersCannotSell => &[Kind::CurveLiquidity],
        // The creator's balance going to zero is read from the same observed
        // cash flow `Kind::CreatorCashFlow` already names -- there is no
        // separate "creator balance" kind on the sheet.
        Signal::CreatorSoldOut => &[Kind::CreatorCashFlow],
        Signal::RepeatLauncher => &[Kind::CreatorLaunches],
        // Chain-dependent: `Kind::LargestHolderShare` on Robinhood Chain,
        // `Kind::TokenOwnership` on Solana (see this fn's own doc comment).
        Signal::HolderConcentration => &[Kind::LargestHolderShare, Kind::TokenOwnership],
        Signal::OwnerCanStillMintOrPause => &[Kind::CreatorTaxBps],
        Signal::CorrelatedSelling => &[Kind::CorrelatedSellWallets],
        Signal::CreatorFundedEarlyBuyers => &[Kind::CreatorFundedEarlyBuyers],
    }
}

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
///
/// **Only a fired signal is eligible to lead (AGENTS.md rule "absent is not
/// zero" and this module's own "matched by kind, never by label").** An
/// ordinary recipient count that never crossed the snapshot's own strongest
/// band is not a concern -- [`Signal::LaunchBlockInStrongestBand`] is the one
/// place that band's threshold is applied -- so this candidate does not
/// exist unless that signal is on `sheet.signals`. Before this gate, "3
/// token accounts were paid in the launch block" led a reply, and the
/// report's own "Strongest concern" repeated it, on a sheet whose report
/// said in the same breath that no signal had fired.
fn launch_recipients(sheet: &FactSheet) -> Option<Candidate> {
    let recipients = fact(sheet, Kind::LaunchRecipients)?;
    if !sheet.signals.contains(&Signal::LaunchBlockInStrongestBand) {
        return None;
    }
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

/// The creator-funded-early-buyers bundle: the launch's own creator address
/// materially funded one or more of the checked early buyers, at or before
/// they bought (design 0031 §2).
///
/// Ranked just above [`shared_funder`], majority or minority alike: an
/// anonymous shared address funding several buyers is the pattern an
/// exchange's hot wallet also produces, but here the sending address is
/// *known* to be the launch's own creator, which is the strongest single
/// fact this bundle can carry about coordination (research 0060: Bubblemaps'
/// deployer-funded cluster, RugCheck's "funded from the same source"). Still
/// a flow, never an identity claim (AGENTS.md rule 4): the sentence says the
/// creator's address sent money, never that the creator owns or controls the
/// wallets it reached.
fn creator_funded_early_buyers(sheet: &FactSheet) -> Option<Candidate> {
    let funded = fact(sheet, Kind::CreatorFundedEarlyBuyers)?;
    let checked = fact(sheet, Kind::FundingChecked);
    let mut id = vec![Kind::CreatorFundedEarlyBuyers];
    if checked.is_some() {
        id.push(Kind::FundingChecked);
    }
    Some(Candidate {
        id: CandidateId(id),
        priority: 96,
        sentence: format!(
            "The creator's address sent money to {} of the early buyers checked, at or before \
             they bought -- a flow between addresses, not proof of who controls them.",
            funded.rendered
        ),
    })
}

/// The exchange-paid-early-buyers bundle: a listed exchange withdrawal
/// wallet materially funded one or more checked early buyers (design 0031
/// §3).
///
/// Ranked below [`shared_funder`]'s own minority priority (80): the sheet
/// only carries this fact when [`crate::sheet`] already resolved the funder
/// to a named, dated exchange wallet rather than an anonymous shared
/// address, which is the less alarming of the two readings by design (§3:
/// "exchanges are not insiders") -- it is informational, never a signal, and
/// ranking it above an unresolved shared funder would say the opposite.
fn exchange_paid_early_buyers(sheet: &FactSheet) -> Option<Candidate> {
    let paid = fact(sheet, Kind::ExchangePaidEarlyBuyers)?;
    let checked = fact(sheet, Kind::FundingChecked);
    let mut id = vec![Kind::ExchangePaidEarlyBuyers];
    if checked.is_some() {
        id.push(Kind::FundingChecked);
    }
    Some(Candidate {
        id: CandidateId(id),
        priority: 60,
        sentence: format!(
            "A listed exchange withdrawal wallet paid out to {} of the early buyers checked, \
             at or before they bought.",
            paid.rendered
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
        .filter(|f| f.kind == Kind::Market)
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

/// The curve-liquidity bundle: what the bonding curve holds right now, when
/// a drain was actually observed (`Signal::LiquidityGone`).
///
/// Gated on the signal, not on the fact's mere presence, for the same
/// reason [`launch_recipients`] gates on `LaunchBlockInStrongestBand`: an
/// ordinary non-zero curve balance is not a concern, and printing it
/// unconditionally would rank a routine number as if it were the finding.
///
/// Ranked above [`concentration`]: this is the fact that actually earns a
/// `Rugged` verdict when paired with `HolderConcentration`
/// (`crate::verdict::rugged_pair`), and research 0059 found the
/// deterministic reply leading with the holder-concentration sentence
/// instead -- the same headline a `Sketchy` reply uses -- so a `Rugged`
/// reply never said the one thing that made it `Rugged`.
fn curve_liquidity(sheet: &FactSheet) -> Option<Candidate> {
    let liquidity = fact(sheet, Kind::CurveLiquidity)?;
    if !sheet.signals.contains(&Signal::LiquidityGone) {
        return None;
    }
    Some(Candidate {
        id: CandidateId(vec![Kind::CurveLiquidity]),
        priority: 92,
        sentence: format!(
            "The bonding curve's reserves are drained -- it now holds {}.",
            liquidity.rendered
        ),
    })
}

/// The token-ownership bundle: the largest owner among the sampled top
/// token accounts, once any bonding-curve address is proven-excluded
/// (design 0027 row 6/7 slice 6a, Solana only).
///
/// Ranked just below [`concentration`] at 85, not level with it, because the
/// two read different things and never both fire on the same dossier today
/// ([`concentration`]'s `Holders` is Robinhood-only; this kind is
/// Solana-only) -- but they are the same *shape* of finding, a share of
/// supply at an unidentified address, and a future dossier that fills both
/// should still only ever lead with one of them. Sitting one step below
/// `concentration` rather than at an unrelated priority makes that
/// non-double-leading property hold by construction: whichever fires,
/// [`concentration`] wins the tie if both ever do. Still above
/// `launch_recipients` (70): an address-level concentration reading, even a
/// capped twenty-account sample of it, is more specific than a bare
/// recipient count with no share to weigh it against.
fn token_ownership(sheet: &FactSheet) -> Option<Candidate> {
    let share = fact(sheet, Kind::TokenOwnership)?;
    Some(Candidate {
        id: CandidateId(vec![Kind::TokenOwnership]),
        priority: 85,
        sentence: format!(
            "Among the largest sampled token accounts, one unidentified wallet holds {} of the \
             total supply.",
            share.rendered
        ),
    })
}

/// The dev-buy bundle: the creator's own address bought into its own launch,
/// once that crossed [`Signal::CreatorBoughtOwnLaunch`]'s own threshold.
///
/// Gated on the signal, the same reason [`launch_recipients`] and
/// [`curve_liquidity`] are gated on theirs: `Kind::DevBuy`'s mere presence
/// (including a measured zero, `push_holders`/the launch-transaction reader's
/// own "no buy" fact) is not a concern by itself. Research replay 2026-09-24
/// (case creator-sale-catwif) found a sheet whose only fired signal was this
/// one still leading with an unrelated, unfired 4.7% holder share, because no
/// candidate here existed for `Kind::DevBuy` at all; [`rank`]'s fired-first
/// grouping is what puts it ahead of that share, not its priority.
///
/// Ranked at 84, just below [`concentration`] and [`token_ownership`]
/// (90/85), for the sheet where both signals fired: a creator's own buy is
/// usually small and is disclosed on the same footing as the bot's own
/// holding (AGENTS.md rule 6), while half the supply at one address is the
/// number that changes what a reader does next. Ranking it above them (91,
/// as first written) made the Robinhood fixture lead with a 0.05 ETH buy
/// over a 50.2% holder (the three Robinhood-fixture tests in `verdict.rs`).
///
/// The sentence reuses the fact's own `Plain` clause rather than a fresh one
/// written here: `Kind::DevBuy` is worded "in the launch block" on Solana and
/// "in the launch transaction" on Robinhood Chain (`sheet.rs`'s two writers
/// of this kind), and a candidate that hard-coded either wording would be
/// right on one chain and wrong on the other -- the same reason [`market`]
/// above reuses its fact's own clause instead of writing a new sentence.
fn dev_buy(sheet: &FactSheet) -> Option<Candidate> {
    let buy = fact(sheet, Kind::DevBuy)?;
    if !sheet.signals.contains(&Signal::CreatorBoughtOwnLaunch) {
        return None;
    }
    let sentence = buy
        .clauses
        .iter()
        .find(|c| c.voice == crate::clause::Voice::Plain)
        .map(|c| c.text.clone())?;
    Some(Candidate {
        id: CandidateId(vec![Kind::DevBuy]),
        priority: 84,
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
///
/// **A candidate backing a fired signal always outranks one that backs
/// none**, before priority is ever consulted. `priority` alone used to
/// decide this, and a sheet whose only fired signal was `CreatorBoughtOwnLaunch`
/// (a 0.5326 SOL dev buy, `Sketchy`) still led with `token_ownership`'s
/// unrelated, unfired 4.7% holder share, because 85 outranks a `DevBuy`
/// candidate that plain didn't exist -- and would still have outranked one at
/// a lower priority even once it did (2026-09-24 replay, case
/// creator-sale-catwif). [`signal_kinds`] is the one map from a fired signal
/// to the kinds it can be read from; a candidate whose id contains any of
/// them is "backed" and sorts before every candidate whose id contains none.
/// A sheet with no fired signal has an empty backed set, so every candidate
/// falls into the same "backs none" group and this ordering is exactly the
/// old priority-only one -- no signals, ranking unchanged.
#[must_use]
pub fn rank(sheet: &FactSheet) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = [
        creator_record(sheet),
        creator_funded_early_buyers(sheet),
        shared_funder(sheet),
        exchange_paid_early_buyers(sheet),
        curve_liquidity(sheet),
        concentration(sheet),
        token_ownership(sheet),
        dev_buy(sheet),
        launch_recipients(sheet),
        market(sheet),
    ]
    .into_iter()
    .flatten()
    .collect();
    let backed_kinds: HashSet<Kind> = sheet
        .signals
        .iter()
        .flat_map(|&signal| signal_kinds(signal).iter().copied())
        .collect();
    let backs_a_fired_signal =
        |candidate: &Candidate| candidate.id.0.iter().any(|k| backed_kinds.contains(k));
    // Stable: two candidates never share a (backed, priority) pair today, but
    // a tie should keep the order they were considered in rather than an
    // implementation detail of the sort.
    candidates.sort_by_key(|c| {
        (
            std::cmp::Reverse(backs_a_fired_signal(c)),
            std::cmp::Reverse(c.priority),
        )
    });
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
            skipped: Vec::new(),
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

    fn curve_liquidity_fact() -> Fact {
        Fact::exact(
            Kind::CurveLiquidity,
            "quote asset held in the bonding curve now",
            0.0,
            "0.0000 SOL",
        )
    }

    /// Research 0059: a `Rugged` sheet's reply led with the same
    /// holder-concentration headline a `Sketchy` reply uses and never said
    /// the fact that actually earned the level -- the curve's reserves read
    /// at zero. Reapplying the bug (dropping `curve_liquidity` from
    /// [`rank`], or its `Signal::LiquidityGone` gate) makes this fail: the
    /// lead reverts to the concentration bundle.
    #[test]
    fn a_drained_curve_leads_over_concentration() {
        let mut sheet = sheet_with(vec![
            curve_liquidity_fact(),
            holders_fact("addresses holding the token, not counting the bonding curve"),
            share_fact("share of circulating supply at the single largest address"),
        ]);
        sheet.signals = vec![Signal::LiquidityGone, Signal::HolderConcentration];
        let top = lead(&sheet).expect("a candidate exists");
        assert_eq!(top.id, CandidateId(vec![Kind::CurveLiquidity]));
        assert!(top.sentence.contains("0.0000 SOL"), "{}", top.sentence);
        assert!(
            top.sentence.to_lowercase().contains("drained"),
            "{}",
            top.sentence
        );
    }

    /// The curve-liquidity fact alone, with no `LiquidityGone` signal, must
    /// not lead or even rank -- an ordinary non-zero curve balance is not a
    /// finding (the same "absent is not zero" gate `launch_recipients`
    /// already holds itself to).
    #[test]
    fn an_unfired_curve_liquidity_fact_does_not_lead() {
        let sheet = sheet_with(vec![curve_liquidity_fact()]);
        assert!(lead(&sheet).is_none());
        assert!(rank(&sheet).is_empty());
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

    /// Design 0031 §3: a listed exchange withdrawal wallet ranks below an
    /// anonymous shared funder, and below concentration too -- it is the
    /// resolved, less alarming reading, informational only.
    #[test]
    fn an_exchange_paid_fact_ranks_below_a_shared_funder_and_concentration() {
        let mut sheet = funding_sheet(3, Some(4));
        sheet.facts.push(Fact::exact(
            Kind::ExchangePaidEarlyBuyers,
            "checked early buyers a listed exchange wallet funded",
            2.0,
            "2 of 4",
        ));
        let ranked = rank(&sheet);
        let exchange = ranked
            .iter()
            .find(|c| c.id.0[0] == Kind::ExchangePaidEarlyBuyers)
            .expect("still ranked");
        assert_eq!(
            exchange.id,
            CandidateId(vec![Kind::ExchangePaidEarlyBuyers, Kind::FundingChecked])
        );
        assert_eq!(exchange.priority, 60);
        let shared_index = ranked
            .iter()
            .position(|c| c.id.0[0] == Kind::SharedFunder)
            .expect("shared funder ranked");
        let holders_index = ranked
            .iter()
            .position(|c| c.id.0[0] == Kind::Holders)
            .expect("concentration ranked");
        let exchange_index = ranked
            .iter()
            .position(|c| c.id.0[0] == Kind::ExchangePaidEarlyBuyers)
            .expect("exchange ranked");
        assert!(exchange_index > shared_index);
        assert!(exchange_index > holders_index);
    }

    /// Without a checked count the exchange fact still ranks, claiming no
    /// denominator in its id -- the same shape `shared_funder` falls back to.
    #[test]
    fn an_exchange_paid_fact_without_a_checked_count_still_ranks() {
        let sheet = funding_sheet(3, None);
        let mut sheet = sheet;
        sheet.facts.push(Fact::exact(
            Kind::ExchangePaidEarlyBuyers,
            "checked early buyers a listed exchange wallet funded",
            1.0,
            "1 of 4",
        ));
        let ranked = rank(&sheet);
        let exchange = ranked
            .iter()
            .find(|c| c.id.0[0] == Kind::ExchangePaidEarlyBuyers)
            .expect("still ranked");
        assert_eq!(
            exchange.id,
            CandidateId(vec![Kind::ExchangePaidEarlyBuyers])
        );
        assert_eq!(exchange.priority, 60);
    }

    /// Design 0031 §2: the creator's own address funding checked early
    /// buyers is a stronger fact than an anonymous shared funder, so a sheet
    /// carrying both ranks the creator-funded bundle first, with the checked
    /// count still in its id.
    #[test]
    fn a_creator_funded_bundle_ranks_above_a_shared_funder() {
        let mut sheet = funding_sheet(3, Some(4));
        sheet.facts.push(Fact::exact(
            Kind::CreatorFundedEarlyBuyers,
            "checked early buyers the creator's address funded",
            2.0,
            "2 of 4",
        ));
        let top = lead(&sheet).expect("a candidate exists");
        assert_eq!(
            top.id,
            CandidateId(vec![Kind::CreatorFundedEarlyBuyers, Kind::FundingChecked])
        );
        assert!(top.priority > 95, "{}", top.priority);
        assert!(top.sentence.contains("2 of"), "{}", top.sentence);
    }

    /// An ordinary launch block -- a recipient count that never crossed the
    /// snapshot's strongest band -- must not lead. Re-applying the bug (drop
    /// the `sheet.signals.contains` gate in `launch_recipients`) makes this
    /// fail: an unfired count would rank and `lead` would return it.
    #[test]
    fn an_unfired_recipient_count_does_not_lead() {
        let sheet = sheet_with(vec![Fact::exact(
            Kind::LaunchRecipients,
            "distinct token accounts receiving the token in its own launch block",
            3.0,
            "3",
        )]);
        assert!(lead(&sheet).is_none());
        assert!(rank(&sheet).is_empty());
    }

    /// The same recipient count, once the strongest-band signal has fired,
    /// is eligible to lead -- the gate is on the signal, not on the fact
    /// being absent.
    #[test]
    fn a_fired_recipient_count_leads_when_nothing_else_outranks_it() {
        let mut sheet = sheet_with(vec![Fact::exact(
            Kind::LaunchRecipients,
            "distinct token accounts receiving the token in its own launch block",
            3.0,
            "3",
        )]);
        sheet.signals.push(Signal::LaunchBlockInStrongestBand);
        let top = lead(&sheet).expect("a fired signal is eligible to lead");
        assert_eq!(top.id, CandidateId(vec![Kind::LaunchRecipients]));
        assert!(top.sentence.contains('3'));
    }

    fn dev_buy_fact() -> Fact {
        Fact::exact(
            Kind::DevBuy,
            "SOL the creator spent buying their own token in the launch block",
            0.5326,
            "0.5326 SOL",
        )
        .saying(
            Voice::Plain,
            "The creator bought 0.5326 SOL of their own token in the launch block.",
        )
    }

    fn token_ownership_fact(share: f64, rendered: &str) -> Fact {
        Fact::exact(
            Kind::TokenOwnership,
            "share of the total token supply held by the largest owner among the sampled \
             largest accounts",
            share,
            rendered,
        )
    }

    fn creator_record_facts() -> Vec<Fact> {
        vec![
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
        ]
    }

    /// The 2026-09-24 replay's own shape (case creator-sale-catwif, mint
    /// 5pYB12...): the only fired signal is `CreatorBoughtOwnLaunch`, backed
    /// by a 0.5326 SOL dev buy, and the sheet also carries an unrelated,
    /// unfired 4.7% `TokenOwnership` share. Before [`dev_buy`] existed, `lead`
    /// returned that 4.7% share -- the wrong fact for a `Sketchy` verdict this
    /// signal alone produced. Re-applying the bug (delete the `dev_buy` call
    /// from [`rank`]'s candidate list, or drop its `sheet.signals.contains`
    /// gate so it fires unconditionally and a different priority wins) makes
    /// this fail.
    #[test]
    fn a_fired_dev_buy_leads_over_an_unrelated_unfired_token_ownership_share() {
        let mut sheet = sheet_with(vec![dev_buy_fact(), token_ownership_fact(0.047, "4.7%")]);
        sheet.signals = vec![Signal::CreatorBoughtOwnLaunch];
        let top = lead(&sheet).expect("a candidate exists");
        assert_eq!(top.id, CandidateId(vec![Kind::DevBuy]));
        assert!(top.sentence.contains("0.5326 SOL"), "{}", top.sentence);

        let report = crate::report::build(&sheet);
        let concern = report
            .strongest_concern
            .expect("a fired signal has a strongest concern");
        assert!(
            concern.evidence.contains("0.5326 SOL"),
            "{}",
            concern.evidence
        );
    }

    /// Both signals fired: the holder share leads, the creator's buy follows.
    /// Re-applying the bug (dev buy ranked above the holder shares, 91) makes
    /// this fail: the buy would lead.
    #[test]
    fn a_fired_holder_share_leads_over_a_fired_dev_buy() {
        let mut sheet = sheet_with(vec![dev_buy_fact(), token_ownership_fact(0.276, "27.6%")]);
        sheet.signals = vec![Signal::CreatorBoughtOwnLaunch, Signal::HolderConcentration];
        let ranked = rank(&sheet);
        assert_eq!(ranked[0].id, CandidateId(vec![Kind::TokenOwnership]));
        assert_eq!(ranked[1].id, CandidateId(vec![Kind::DevBuy]));
    }

    /// A `Kind::DevBuy` fact with no `CreatorBoughtOwnLaunch` signal is a
    /// measured buy (or a measured zero) that never crossed the signal's own
    /// threshold -- not a concern, the same "absent is not zero" gate
    /// [`launch_recipients`] and [`curve_liquidity`] already hold themselves
    /// to. Re-applying the bug (drop the `sheet.signals.contains` gate in
    /// [`dev_buy`]) makes this fail: an unfired buy would rank and `lead`
    /// would return it.
    #[test]
    fn a_dev_buy_fact_without_its_signal_does_not_rank() {
        let sheet = sheet_with(vec![dev_buy_fact()]);
        assert!(lead(&sheet).is_none());
        assert!(rank(&sheet).is_empty());
    }

    /// The 2026-09-24 replay's other case (creator-sale-hbull): `HolderConcentration`
    /// fires off a Solana `Kind::TokenOwnership` share, not `Kind::LargestHolderShare`
    /// -- the kind [`crate::report::signal_kind`] alone would have named. A
    /// sheet that also carries an unfired, higher-priority creator record
    /// must still lead with the 27.6% share the signal actually fired on.
    /// Re-applying the bug (drop `Kind::TokenOwnership` from
    /// [`signal_kinds`]'s `HolderConcentration` arm) makes this fail: the
    /// creator record, unfired but priority 100, would lead instead.
    #[test]
    fn a_fired_holder_concentration_share_leads_over_an_unfired_creator_record() {
        let mut sheet = sheet_with(creator_record_facts());
        sheet.facts.push(token_ownership_fact(0.276, "27.6%"));
        sheet.signals = vec![Signal::HolderConcentration];
        let top = lead(&sheet).expect("a candidate exists");
        assert_eq!(top.id, CandidateId(vec![Kind::TokenOwnership]));
        assert!(top.sentence.contains("27.6%"), "{}", top.sentence);
    }

    /// No fired signal: every candidate falls into the same "backs none"
    /// group, so the ranking is exactly the old priority-only order --
    /// creator record (100), then concentration (90), then token ownership
    /// (85). Re-applying a bug that always grouped by "backed" (rather than
    /// only when a candidate's kind is in a fired signal's set) makes this
    /// fail if it ever reordered a signal-less sheet.
    #[test]
    fn no_fired_signal_keeps_the_old_priority_only_ordering() {
        let mut sheet = sheet_with(creator_record_facts());
        sheet
            .facts
            .push(holders_fact("addresses holding the token now"));
        sheet
            .facts
            .push(share_fact("held by the single largest address"));
        sheet.facts.push(token_ownership_fact(0.047, "4.7%"));
        let ranked = rank(&sheet);
        let leads: Vec<Kind> = ranked.iter().map(|c| c.id.0[0]).collect();
        assert_eq!(
            leads,
            vec![Kind::CreatorLaunches, Kind::Holders, Kind::TokenOwnership]
        );
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
