// SPDX-License-Identifier: Apache-2.0
//! What a launch turned out to be, read from the chain rather than judged.
//!
//! This is the other half of the record `memory.rs`'s `verdicts` table
//! started: a verdict paired with an outcome is one calibration sample, and
//! research 0052 §5's replay cannot begin until there are pairs. This module
//! is the outcome half, and nothing here weighs risk or scores anything --
//! it reports which of three things a launch did, from facts a reader can
//! re-check.
//!
//! # The definition, and why it is not the industry's
//!
//! Research 0052 §5 sets three labels and explicitly refuses Solidus's
//! ("liquidity under $1,000"), which would call 98.6% of launches a rug and
//! leave every weight fitted against noise:
//!
//! - [`OutcomeLabel::Rug`] — someone took it. Observed extraction, not a
//!   price fall.
//! - [`OutcomeLabel::Failed`] — nobody took anything and it died anyway: the
//!   reserves went to zero with the buyers already sold back out.
//! - [`OutcomeLabel::Alive`] — neither, as of the moment it was read.
//!
//! "Everyone lost money" and "someone took it" are different things, and a
//! model fitted on the two folded together is fitted on the wrong question.
//!
//! # Unknown is not `Alive`
//!
//! [`label`] returns `None` far more often than it returns [`Alive`], and
//! that is deliberate (AGENTS.md §3 rule 8). A launch this reader cannot
//! settle must stay out of the sample entirely: a wrong `Alive` is worse
//! than no row at all, because it is counted as a *control* — it teaches the
//! fit that the signals which fired on that launch meant nothing.
//!
//! Two whole cases are unreadable today, and both return `None`:
//!
//! - **A graduated launch.** After graduation the curve holds nothing by
//!   design (research 0040 §3), so its empty reserves say nothing; the money
//!   is in the AMM pool, and research 0044 has not yet found that pool's
//!   address on Pons v2. Research 0052 §5's third rug rule (reserves fell
//!   ≥ 9,000 bps in one hour after graduation) is therefore not measurable
//!   here at all, and a graduated launch is left unlabelled rather than
//!   called `Alive` — which is exactly where a post-graduation rug would
//!   hide.
//! - **A launch whose curve or holders could not be read.** An absent read
//!   is not a clean reading.

use crate::dossier::Dossier;
use crate::memory::OutcomeLabel;
use crate::wallets::CreatorCashFlow;
use realorrug_robinhood::pons::Side;

/// How much of what the creator bought they must have sold back for the
/// launch to count as an extraction: half, in basis points (research 0052
/// §5).
pub const CREATOR_SOLD_RUG_BPS: u128 = 5_000;

/// One settled reading of what a launch did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Labelled {
    /// The label itself.
    pub label: OutcomeLabel,
    /// One line naming the facts that settled it, so a label someone
    /// disagrees with is re-checkable against the chain instead of trusted.
    pub evidence: String,
}

/// Reads one fresh dossier and says what the launch turned out to be, or
/// `None` when this reading cannot settle it.
///
/// The order matters: extraction is checked before death, because a creator
/// who sold out of a curve that then emptied did both, and the first is the
/// one that happened *to* somebody.
#[must_use]
pub fn label(dossier: &Dossier) -> Option<Labelled> {
    if let Some(labelled) = creator_sold_out(dossier.creator_cash_flow.as_ref()) {
        return Some(labelled);
    }

    let curve = dossier.curve.as_ref()?;
    if curve.complete {
        // Graduated: the curve is empty by design and the pool is unread.
        // See this module's doc comment -- `None`, never `Alive`.
        return None;
    }

    if curve.quote_reserves > 0 {
        return Some(Labelled {
            label: OutcomeLabel::Alive,
            evidence: format!(
                "the curve has not completed and still holds {} in quote reserves",
                curve.quote_reserves
            ),
        });
    }

    // Reserves are gone before graduation. Whether that is an extraction or
    // a death turns on whether anyone is still holding, which needs the
    // holder read; without it this is unknown, not either answer.
    let holders = dossier.holders.as_ref()?;
    if holders.count > 0 {
        Some(Labelled {
            label: OutcomeLabel::Rug,
            evidence: format!(
                "the curve emptied before graduation while {} holder(s) still held supply they cannot exit through it",
                holders.count
            ),
        })
    } else {
        Some(Labelled {
            label: OutcomeLabel::Failed,
            evidence:
                "the curve emptied before graduation with no holders left: nobody was still in it"
                    .to_owned(),
        })
    }
}

/// The creator selling back most of what they bought, when the trade history
/// is provably whole.
///
/// The denominator is what the creator **bought**, not what they held. A
/// free allocation minted to them at launch is not in `trades` at all, so a
/// creator who bought nothing and dumped an allocation is invisible to this
/// rule and falls through to the curve reading below — under-calling, never
/// over-calling. That is the safe direction: a missed `Rug` costs one
/// sample, a fabricated one poisons the fit.
///
/// `trades_complete` gates the whole rule. A partial trade list can only
/// understate sales, and a ratio computed from one would be a number about
/// our read rather than about the creator (AGENTS.md §1).
fn creator_sold_out(cash_flow: Option<&CreatorCashFlow>) -> Option<Labelled> {
    let cash_flow = cash_flow?;
    if !cash_flow.trades_complete {
        return None;
    }
    let bought = cash_flow
        .trades
        .iter()
        .filter(|t| t.side == Side::Buy)
        .fold(0u128, |sum, t| sum.saturating_add(t.tokens));
    if bought == 0 {
        return None;
    }
    let sold = cash_flow
        .trades
        .iter()
        .filter(|t| t.side == Side::Sell)
        .fold(0u128, |sum, t| sum.saturating_add(t.tokens));
    let sold_bps = sold.saturating_mul(10_000) / bought;
    if sold_bps < CREATOR_SOLD_RUG_BPS {
        return None;
    }
    Some(Labelled {
        label: OutcomeLabel::Rug,
        evidence: format!(
            "the creator sold {sold_bps} bps of the tokens they bought back into the curve, across a trade history read whole"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dossier::{CurveFacts, Holders};
    use crate::wallets::CreatorTrade;
    use realorrug_robinhood::Hash32;
    use realorrug_robinhood::pons::CreatorRole;
    use realorrug_types::ChainAddress;

    fn empty() -> Dossier {
        Dossier {
            mint: mint(),
            read_at: None,
            launch: None,
            curve: None,
            creator_transactions: None,
            chain_launch: None,
            holders: None,
            funding: None,
            market: None,
            token_ownership: None,
            creator_cash_flow: None,
            powers: None,
            unavailable: Vec::new(),
            calls: 0,
            elapsed_ms: 0,
        }
    }

    fn mint() -> ChainAddress {
        "0x00000000000000000000000000000000000000aa"
            .parse()
            .expect("an address")
    }

    fn curve(complete: bool, quote_reserves: u128) -> CurveFacts {
        CurveFacts {
            complete,
            quote_reserves,
            quote_capacity: None,
            quote_asset: None,
            creator: mint(),
            fees: None,
        }
    }

    fn trade(side: Side, tokens: u128) -> CreatorTrade {
        CreatorTrade {
            role: CreatorRole::Deployer,
            side,
            quote: 1,
            tokens,
            block: 10,
            transaction: Hash32([0; 32]).to_string(),
            unique_id: format!("{side:?}-{tokens}"),
        }
    }

    fn cash_flow(complete: bool, trades: Vec<CreatorTrade>) -> CreatorCashFlow {
        CreatorCashFlow {
            trades,
            transfers_out: 0,
            trades_complete: complete,
            gaps: Vec::new(),
            quote_asset: crate::dossier::QuoteAsset::eth(),
        }
    }

    /// A curve still holding money before graduation is the ordinary living
    /// launch, and the only thing that earns `Alive`.
    #[test]
    fn a_curve_that_still_holds_money_is_alive() {
        let mut dossier = empty();
        dossier.curve = Some(curve(false, 4_000_000_000_000_000_000));
        let labelled = label(&dossier).expect("a reading");
        assert_eq!(labelled.label, OutcomeLabel::Alive);
        assert!(labelled.evidence.contains("4000000000000000000"));
    }

    /// The whole reason `None` exists: after graduation the curve is empty
    /// by design and the pool is unread, so an empty curve proves nothing.
    /// Calling this `Alive` is exactly where a post-graduation rug would
    /// hide, and calling it `Rug` would label every graduated launch.
    #[test]
    fn a_graduated_launch_is_not_labelled_either_way() {
        let mut dossier = empty();
        dossier.curve = Some(curve(true, 0));
        dossier.holders = Some(Holders {
            count: 40,
            largest_share_bps: Some(1_200),
        });
        assert_eq!(label(&dossier), None);
    }

    /// Reserves gone before graduation with people still holding is the
    /// extraction case.
    #[test]
    fn an_emptied_curve_with_holders_left_is_a_rug() {
        let mut dossier = empty();
        dossier.curve = Some(curve(false, 0));
        dossier.holders = Some(Holders {
            count: 31,
            largest_share_bps: Some(900),
        });
        let labelled = label(&dossier).expect("a reading");
        assert_eq!(labelled.label, OutcomeLabel::Rug);
        assert!(labelled.evidence.contains("31 holder"));
    }

    /// The same empty curve with nobody left in it is a death, not a theft,
    /// and research 0052 §5 keeps the two apart on purpose.
    #[test]
    fn an_emptied_curve_with_nobody_left_is_a_failure_not_a_rug() {
        let mut dossier = empty();
        dossier.curve = Some(curve(false, 0));
        dossier.holders = Some(Holders {
            count: 0,
            largest_share_bps: None,
        });
        assert_eq!(
            label(&dossier).expect("a reading").label,
            OutcomeLabel::Failed
        );
    }

    /// An empty curve with no holder read cannot tell extraction from death,
    /// so it settles nothing. Unknown is not `Alive` and not `Failed`.
    #[test]
    fn an_emptied_curve_with_no_holder_read_settles_nothing() {
        let mut dossier = empty();
        dossier.curve = Some(curve(false, 0));
        assert_eq!(label(&dossier), None);
    }

    /// No curve read at all is no reading at all.
    #[test]
    fn a_dossier_with_no_curve_settles_nothing() {
        assert_eq!(label(&empty()), None);
    }

    /// The creator selling back most of what they bought is an extraction
    /// wherever the curve stands -- checked first, because a creator who
    /// sold out of a curve that later emptied did both, and this is the one
    /// that happened to somebody.
    #[test]
    fn a_creator_selling_back_most_of_what_they_bought_is_a_rug() {
        let mut dossier = empty();
        // Still a healthy-looking live curve: without this rule it would
        // read as `Alive`.
        dossier.curve = Some(curve(false, 9_000_000_000_000_000_000));
        dossier.creator_cash_flow = Some(cash_flow(
            true,
            vec![trade(Side::Buy, 1_000), trade(Side::Sell, 500)],
        ));
        let labelled = label(&dossier).expect("a reading");
        assert_eq!(labelled.label, OutcomeLabel::Rug);
        assert!(labelled.evidence.contains("5000 bps"));
    }

    /// Exactly at the threshold counts; one token under it does not. The
    /// boundary is the rule, so it is the thing worth pinning.
    #[test]
    fn the_creator_sale_threshold_is_at_half_not_near_it() {
        let at = cash_flow(true, vec![trade(Side::Buy, 1_000), trade(Side::Sell, 500)]);
        assert!(creator_sold_out(Some(&at)).is_some(), "half is enough");
        let under = cash_flow(true, vec![trade(Side::Buy, 1_000), trade(Side::Sell, 499)]);
        assert_eq!(
            creator_sold_out(Some(&under)),
            None,
            "one token under half is not an extraction"
        );
    }

    /// A trade list that is not provably whole can only understate sales, so
    /// a ratio from one is a number about our read, not about the creator.
    #[test]
    fn a_partial_trade_history_never_calls_a_rug() {
        let partial = cash_flow(
            false,
            vec![trade(Side::Buy, 1_000), trade(Side::Sell, 1_000)],
        );
        assert_eq!(creator_sold_out(Some(&partial)), None);
        assert_eq!(creator_sold_out(None), None);
    }

    /// A creator who bought nothing has no denominator. They may have dumped
    /// a free allocation, which this rule cannot see -- under-calling on
    /// purpose, because a fabricated `Rug` poisons the fit while a missed one
    /// costs a single sample.
    #[test]
    fn a_creator_who_bought_nothing_is_not_judged_by_this_rule() {
        let sold_only = cash_flow(true, vec![trade(Side::Sell, 10_000)]);
        assert_eq!(creator_sold_out(Some(&sold_only)), None);
    }
}
