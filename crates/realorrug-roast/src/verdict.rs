// SPDX-License-Identifier: Apache-2.0
//! The verdict, and the reply that ships when the model cannot be used.
//!
//! # The verdict is a rule, not a judgement
//!
//! The model decides what the headline is, what matters and how to say it. It
//! does **not** decide the verdict, for the same reason `radar-risk` is a pure
//! function: a verdict computed by a rule is replayable, and a refusal that can
//! be reproduced from a recording is one you can argue about with evidence.
//!
//! It is deliberately not a score. `GOAL.md` refuses a single safety score --
//! *"Radar has fourteen reason codes and a structural split. A green shield is
//! 'unknown rendered as safe'"* -- and a one-bit verdict word is a score with
//! one bit. So [`Verdict`] carries **reasons**, and a reply renders the reasons
//! rather than the label.
//!
//! # The template is not a fallback, it is the floor
//!
//! Every path that cannot produce a trustworthy model reply ships
//! [`template`] instead: no budget configured, no provider, the provider
//! unreachable, a fabricated number, a forbidden claim. That is rule 8 --
//! **deny by default when config is missing** -- applied to speech rather than
//! to money: an analyst that cannot verify what it is about to say falls back
//! to saying only what it measured, never to saying nothing and never to saying
//! more.

use crate::sheet::{FactSheet, Signal};
use std::fmt::Write as _;

/// The verdict ladder, ADR 0027's five names, decided by code from the sheet
/// alone.
///
/// # Not `Ord`, not `Default`, and that is deliberate
///
/// The five names read like a severity scale but are not one: `CantTell` is
/// an epistemic state (a required fact was unread), not a point between
/// `Sketchy` and `NothingUglyYet` on a badness axis. A derived `Ord` would
/// place `CantTell` at some numeric position among the others, and the first
/// thing anyone would do with that position is compare it -- `level >=
/// Level::Sketchy` reads as "at least as bad as Sketchy," and there is no
/// true answer to that question for `CantTell`, which is not on the badness
/// axis at all. A derived `Default` has the same failure in miniature: it
/// would silently pick one variant as "the" verdict for an unbuilt value, and
/// whichever variant that is, ADR 0027's own consequence 4 is that a blind
/// spot must never read as `NothingUglyYet` -- so there is no safe default to
/// pick, and the type does not offer one. Compare by matching on the variant,
/// not by ordering it.
// Serialised under its variant names (`"Sketchy"`), the same strings the
// checker route publishes, so the reply log and the site read one vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Level {
    /// It already happened, observed: liquidity removed, creator sold out,
    /// buyers cannot sell. Never on one signal alone.
    Rugged,
    /// The token is live and two or more strong signals fire together.
    RugMechanicsLive,
    /// Real red flags with innocent explanations still open: one signal, no
    /// qualifying combination.
    Sketchy,
    /// No bad signal found, and every required fact was read. The reply must
    /// carry the "yet."
    NothingUglyYet,
    /// A fact the ladder needed could not be read. Never presented as clean.
    CantTell,
}

/// The live-risk signals design 0020 §3 counts toward `RugMechanicsLive`.
///
/// Eight signals, matching §3's own list exactly (`OwnerCanStillMintOrPause`
/// included, "once it ships"; the launch-block-band member is
/// [`Signal::LaunchBlockInStrongestBand`], the one variant every chain's
/// snapshot feeds -- see its doc comment in `sheet.rs`). Named as a list
/// rather than inlined into [`level`] so the "two or more" rule and the
/// membership rule are each stated once, in one place, instead of duplicated
/// across match arms.
///
/// **No chain appears here, or anywhere in this function.** A `Signal` means
/// the same thing on every chain (`sheet.rs`'s own rule, restated by the
/// owner: one bot, one voice, a chain is data it carries); this list and
/// [`level`] read signals and `unknown`, never `venue` or anything shaped
/// like it, and must not grow a `match` on which chain produced the sheet.
const LIVE_RISK_SIGNALS: &[Signal] = &[
    Signal::LiquidityGone,
    Signal::CreatorSoldOut,
    Signal::BuyersCannotSell,
    Signal::CreatorBoughtOwnLaunch,
    Signal::LaunchBlockInStrongestBand,
    Signal::RepeatLauncher,
    Signal::HolderConcentration,
    Signal::OwnerCanStillMintOrPause,
];

/// Computes the verdict level from the sheet alone.
///
/// Pure: no chain read, no model, no I/O. Design 0020 §3's rule, in the
/// order §3 settles it:
///
/// 1. **`Rugged` first, even over a missing fact.** An observed
///    `Rugged`-qualifying pair (`LiquidityGone` + `HolderConcentration`, or
///    `CreatorSoldOut` + `BuyersCannotSell`) is checked before `unknown` is
///    consulted at all -- §3's own precedence section: both qualifying pairs
///    are built from *optional* facts, so a sheet that observed one but also
///    failed to read an unrelated required fact must still report the rug it
///    saw, not bury it under "can't tell."
/// 2. **`CantTell` whenever a required fact is unread**, once the `Rugged`
///    check above has already cleared. `sheet.unknown` is exactly the record
///    of what could not be read (`FactSheet::build`'s `unknown` list), so a
///    nonempty list here is "a required fact is unread" by construction --
///    there is no separate "optional-miss" list to consult.
/// 3. **`RugMechanicsLive`** needs two or more of [`LIVE_RISK_SIGNALS`]. A
///    single signal cannot reach it, satisfying §3 rule 4 by construction:
///    the count has to clear two before this arm returns.
/// 4. **`Sketchy`** is any signal at all, once the stronger levels above have
///    already been ruled out.
/// 5. **`NothingUglyYet`** is what is left: every required fact read (step 2
///    passed), no `Rugged` pair, fewer than two live-risk signals, and no
///    signal at all.
#[must_use]
pub fn level(sheet: &FactSheet) -> Level {
    let rugged = (sheet.signals.contains(&Signal::LiquidityGone)
        && sheet.signals.contains(&Signal::HolderConcentration))
        || (sheet.signals.contains(&Signal::CreatorSoldOut)
            && sheet.signals.contains(&Signal::BuyersCannotSell));
    if rugged {
        return Level::Rugged;
    }

    if !sheet.unknown.is_empty() {
        return Level::CantTell;
    }

    let live_risk_count = LIVE_RISK_SIGNALS
        .iter()
        .filter(|signal| sheet.signals.contains(signal))
        .count();
    if live_risk_count >= 2 {
        return Level::RugMechanicsLive;
    }

    if !sheet.signals.is_empty() {
        return Level::Sketchy;
    }

    Level::NothingUglyYet
}

/// What the rule concluded, as reasons rather than a score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// The verdict ladder's level, chosen by [`level`] from the sheet alone.
    /// The model never moves this (ADR 0027, `AGENTS.md` §3 rule 4).
    pub level: Level,
    /// Things measured about this token that a reader should know.
    ///
    /// Each is a statement of fact with its number already in the sheet. None
    /// of them is a recommendation, and the order is the order they are worth
    /// reading rather than a ranking of severity.
    pub reasons: Vec<String>,
    /// The sheet's own [`FactSheet::twins`], carried alongside the reasons so
    /// a caller that renders a verdict without the sheet in hand still has
    /// them.
    ///
    /// **A separate field, not folded into `reasons`.** A reason is a fact
    /// restated; a twin is not a fact -- it is the honest limit of a fact,
    /// and collapsing the two would let a twin get counted, quoted or
    /// checked as if it were one of the sheet's measurements.
    pub twins: Vec<String>,
}

impl Verdict {
    /// Computes the verdict from the sheet.
    ///
    /// Pure, and a function of the sheet alone. Deliberately: given the same
    /// facts this returns the same reasons on any machine at any time, which is
    /// what makes a published reply reproducible from its recorded fact sheet.
    #[must_use]
    pub fn from(sheet: &FactSheet) -> Self {
        let mut reasons = Vec::new();
        for fact in &sheet.facts {
            // The reasons are the facts, restated. This is not a simplification
            // waiting to be replaced by scoring: the product is "here is what
            // was measured", and a rule that weighted these into a conclusion
            // would be the single safety score GOAL.md refuses.
            reasons.push(public_reason(fact));
        }
        for miss in &sheet.unknown {
            reasons.push(format!("not known -- {miss}"));
        }
        Self {
            level: level(sheet),
            reasons,
            twins: sheet.twins.clone(),
        }
    }
}

/// One fact as a stranger on the website should read it.
///
/// **The plain clause, not `label: rendered`.** A fact carries both, and they
/// are written for different readers. `rendered` and `label` are written for
/// the model and for the fidelity check that reads its output: they carry the
/// argument that keeps a later engineer from getting the fact backwards, in
/// the engineer's own shouting -- "NOT zero, and NOT 'cannot size into this'",
/// "this is REAL OR RUG.S OWN impact budget, NOT a ceiling the venue imposes
/// (research 0022)". Every word of that is true and none of it is for a
/// reader. `/v1/check/` published it verbatim, so the public page argued with
/// itself in capitals about a document nobody outside this repository can
/// read.
///
/// The plain clause is the same measurement written as one sentence by the
/// code that read it, which is what [`crate::sheet::Fact::saying`] exists for.
/// A fact with no clause falls back to the old form rather than vanishing:
/// unreadable beats absent, because a missing reason is a fact the reader
/// never learns was measured (AGENTS.md rule 8).
fn public_reason(fact: &crate::sheet::Fact) -> String {
    fact.clauses
        .iter()
        .find(|clause| clause.voice == crate::clause::Voice::Plain)
        .map_or_else(
            || format!("{}: {}", fact.label, fact.rendered),
            |clause| clause.text.clone(),
        )
}

/// Facts the template leads with, in order, matched by [`crate::clause::Kind`]
/// -- never by a fragment of [`crate::sheet::Fact::label`].
///
/// **Order is the product decision, not a formatting one.** The cost line
/// leads: "six recipients" is insidery, while "a $50 position pays 4.6% to get
/// in and out" is comprehensible to anyone, is measured, and is said nowhere
/// else. The bundle line is second.
///
/// **Matched by kind, not by label, since 2026-09-18.** This list used to be a
/// list of label fragments, and the label list that mattered for Robinhood was
/// solely responsible for the 529-holder/50.2%-top-holder failure the design
/// doc's §1 refutation 8 records: renaming a label, or writing a new one for a
/// fact that already existed, silently dropped it from this list, and `find`
/// returning `None` for a fragment that no longer matches reads exactly like
/// "this sheet has nothing to say here." `Kind` is the stable name a fact
/// keeps across any rewording of its `label` -- it is what a selection, a log
/// line and a receipt already refer to (`crate::clause::Kind`'s own doc
/// comment) -- so matching on it here closes the same hole for the template
/// that [`crate::salience`] closes for the headline and the model request.
const LEAD: &[crate::clause::Kind] = &[
    // The creator's record first. Running the command against three real
    // launches on 2026-09-04 produced three **identical** replies: the cost
    // line is a constant and most launches sit in the same recipient band, so
    // nothing above or below this was about the coin being asked about.
    //
    // This is. "One hundred and fifty launches, none of which reached an AMM by
    // filling over time" is specific, checkable, and the thing Radar has that
    // nobody else does.
    crate::clause::Kind::CreatorLaunches,
    crate::clause::Kind::CreatorOrganic,
    // **Immediately after it, and this is the point of the ordering.** A count
    // with no denominator is a number the reader cannot weigh: "none of 150"
    // sounds damning to somebody who assumes half of them should have, and
    // unremarkable to somebody who assumes none ever do. Neither reader is
    // informed. The population says which.
    //
    // Measured 2026-09-04 over 506,991 outcomes: 2.81%.
    // **Before the population lines, and this cost a revision to get right.**
    // Those lines are constants too -- 2.8% and 23.0% in every reply -- so
    // leading with them just swaps one repeated opener for another. This is the
    // one fact about *this coin* that survives when the creator is unknown,
    // which for a fresh launch is the common case.
    //
    // The honest shape of the limitation: for a brand-new coin by a creator
    // Radar has never seen, the launch block is nearly all it has. That is a
    // fact about the product, not about the wording, and the template should
    // show it rather than pad around it.
    crate::clause::Kind::LaunchRecipients,
    crate::clause::Kind::VenueGraduated,
    // The most quotable figure in the set, and one the published snapshot does
    // not carry at all: 23.0% of measured launches show almost no life.
    crate::clause::Kind::VenueStillborn,
    crate::clause::Kind::BandInstant,
    crate::clause::Kind::BandNeverGraduated,
    crate::clause::Kind::Capacity,
    crate::clause::Kind::DevBuy,
    // Everything above is a Solana label and everything below is a Robinhood
    // one, and **no label is on both lists**, so one array serves both chains:
    // a Solana sheet matches only the entries above, a Robinhood sheet only
    // the entries below, and neither chain's reply is changed by the other's
    // entries existing. (The one shared label is the graduation line, which
    // `push_curve` writes for both; it sits below the Solana entries so a
    // Solana sheet with a creator record still fills its five slots from them
    // first and only a sheet too sparse to fill them reaches it.)
    //
    // This block is why the fix exists. Until 2026-09-17 the list was Solana's
    // alone, so for a Robinhood token nothing matched, the loop printed zero
    // facts, and the reply was the launch age and the block it was read at --
    // with the 529 holders and the 50.2% top address measured, on the sheet,
    // and silently dropped.
    //
    // Ordered the way somebody deciding whether to buy would ask. The top
    // address's share is first because it is the one number that can make the
    // rest irrelevant: whatever else is true, one address that can sell half
    // the float decides what happens next.
    crate::clause::Kind::LargestHolderShare,
    crate::clause::Kind::Holders,
    crate::clause::Kind::Graduated,
    // Robinhood's own dev-buy fact shares `Kind::DevBuy` with Solana's "SOL the
    // creator spent" above -- one sheet only ever carries one of the two, so
    // this entry is never reached when the Solana one already filled a slot,
    // and it is what fills that slot on a Robinhood sheet instead.
    //
    // The venue's own fee, read from its on-chain schedule -- and until
    // 2026-09-17 present on every sheet and printed on none, because this
    // array is the only gate a fact has to clear to reach a reader and this
    // one was never on it. It is the sharpest comparison the sheet carries:
    // the venue publishes a fee a fraction of the measured all-in cost, and
    // the gap between the two is most of what a trader pays.
    crate::clause::Kind::VenueFee,
    // **The round trip is deliberately NOT here.** It led every reply until
    // 2026-09-05, and it is the same 456 bps every time, so every reply opened
    // with the same sentence -- an account that reads as a bot repeating itself
    // rather than as something that looked at the coin.
    //
    // It is too useful to drop, so it is printed as a closing line instead: last,
    // always, and out of competition for the five slots above.
];

/// How many facts the template will print.
///
/// A reply is read at a glance and screenshotted, or it is not read. The full
/// sheet is what `radar roast --sheet` is for; this is what gets posted, and a
/// twenty-line dump would be posted by nobody.
///
/// Five rather than four since 2026-09-04, and the extra one is the creator's
/// record. Four was enough while every fact was about the block; it is not
/// enough now that two of the lines are the only ones that differ between one
/// coin and the next.
const MAX_FACTS: usize = 5;

/// The one sentence Radar prints if the model says nothing useful.
///
/// Built from the sheet's own facts and nothing else, so it passes
/// [`crate::fidelity::check`] against `sheet.authorised()` by construction --
/// which is what lets it be handed to the model as a starting line rather than
/// as a suggestion the checks would later refuse.
///
/// It exists because three real launches on 2026-09-04 produced three
/// **identical** replies. The cost line is a constant and most launches sit in
/// the same recipient band, so the model had no anchor that was about the coin
/// in front of it. This is that anchor, and it is chosen the same way [`LEAD`]
/// orders facts: the creator's record if there is one, otherwise the launch
/// block.
///
/// `None` when the sheet has neither. **An unknown creator is not a creator
/// with zero launches** -- rule 9 -- so there is nothing to lead with and the
/// template prints no headline rather than a misleading one.
///
/// **This is the template's own first line now, not an offer to the model.** It
/// was handed to the model as a tag until 2026-09-08, because a model writing
/// prose needed an anchor about this coin. A model that selects clauses has
/// one by construction -- the sheet leads with this coin's own sentences -- so
/// the offer was removed rather than translated into a clause nobody would
/// pick over the sentence it was built from.
///
/// Under a hundred characters, because the first sentence is what gets
/// screenshotted without the rest.
#[must_use]
pub fn headline(sheet: &FactSheet) -> Option<String> {
    // Drawn from [`crate::salience`], the one typed selection service --
    // ranked by `Fact::kind`, never by a fragment of `Fact::label`. Until
    // 2026-09-18 this matched label substrings directly, one branch per
    // bundle, in a fixed order this function alone decided; a Solana-only
    // label list is exactly how the 529-holder/50.2%-top-holder failure
    // happened (verdict.rs's own [`LEAD`] doc comment), and headline
    // selection had the same defect independently, one file over. Now the
    // headline, [`template`]'s lead facts and [`crate::voice::request_for`]'s
    // suggested lead all rank the same candidates, so a bundle promoted or
    // demoted here moves for all three at once.
    crate::salience::lead(sheet)
        .map(|c| c.sentence)
        .filter(|line| line.chars().count() <= 100)
}

/// The reply that ships when a model reply cannot be trusted or cannot be had.
///
/// Contains only figures from the sheet, so it passes
/// [`crate::fidelity::check`] against its own source by construction — and a
/// test asserts that rather than assuming it, because if the floor were itself
/// unpublishable there would be nothing left to fall back to.
///
/// **No name-and-address header.** Until 2026-09-17 every reply opened "Real
/// or Rug on <mint>:" -- a line that named the account and repeated the
/// mint, when the reply is already threaded under the mention that named
/// both. It was also the one line every reply shared regardless of the coin,
/// which is the same defect the LEAD ordering below exists to avoid one line
/// later. The reply now leads on [`headline`], the sentence that is actually
/// about this coin.
#[must_use]
pub fn template(sheet: &FactSheet) -> String {
    let mut out = String::new();
    // The headline, when there is one, so the floor leads on the fact that is
    // about this coin rather than on whichever fact happened to sort first.
    if let Some(headline) = headline(sheet) {
        let _ = writeln!(out, "{headline}");
    }

    let mut shown = 0;
    // Kinds already printed, so a fact that legitimately shares its `Kind`
    // with an earlier `LEAD` entry -- Robinhood's ETH dev buy and Solana's
    // SOL dev buy both carry `Kind::DevBuy` -- is never printed twice from
    // one sheet that happens to carry it once.
    let mut printed = std::collections::HashSet::new();
    for wanted in LEAD {
        if shown >= MAX_FACTS {
            break;
        }
        if !printed.insert(*wanted) {
            continue;
        }
        let Some(fact) = sheet
            .facts
            .iter()
            .find(|f| f.kind == *wanted && !f.rendered.is_empty())
        else {
            continue;
        };
        // The caveats live in the label and are restated in short form here
        // rather than dropped: "token accounts, not people" and "Radar's budget,
        // not a trading limit" are the parts that keep the numbers honest,
        // and a reply that sheds them is a reply that says something else.
        let _ = writeln!(out, "- {}: {}", short(&fact.label), fact.rendered);
        shown += 1;
    }

    for miss in &sheet.unknown {
        // Said plainly, never rendered as reassurance and never by omission: a
        // reader not told something is missing assumes it was checked.
        let _ = writeln!(out, "- not known: {miss}");
    }
    // The cost, always, and always last. It is the same number in every reply --
    // that is why it is not competing for a slot above -- but it is also the one
    // figure that applies to the reader no matter what the rest of the reply
    // said, so dropping it to make room would be dropping the only line that is
    // about them rather than about the coin.
    //
    // Drawn from the sheet rather than written as a constant here, so
    // `fidelity::check` still holds: a number in the reply that is not on the
    // sheet is exactly what that check refuses, and hard-coding 456 would make
    // this function the one place allowed to invent one.
    //
    // **The band qualifier is load-bearing.** The sheet carries one of these per
    // notional band and the cheapest one is first, so matching on "round trip
    // for a position of" alone finds `$0.20-$2` -- 3042 bps -- and publishes a
    // cost 6.7x the real one on every reply. Written without it, run against
    // three live coins, and caught by reading the output.
    if let Some(cost) = sheet.facts.iter().find(|f| {
        f.label.contains("round trip for a position of $20-$200") && !f.rendered.is_empty()
    }) {
        let _ = writeln!(
            out,
            "Entering and leaving a $20-$200 position: {}.",
            cost.rendered
        );
    }
    // The age, before the read point, and stated even when there is none to
    // give a number for -- design 0020 §4: a `NothingUglyYet` reply about a
    // token whose age is unknown must say so, not stay silent about the
    // limit on how far its "clean so far" reaches (rule 8, unknown is not
    // safe). Only a sheet with something chronological at all reaches either
    // line, same as before this task.
    if let Some(age) = sheet
        .facts
        .iter()
        .find(|f| f.kind == crate::clause::Kind::Age)
    {
        let _ = writeln!(out, "Launched {}.", age.rendered);
    } else if sheet.read_at.is_some() {
        let _ = writeln!(out, "How old this token is could not be read.");
    }
    // One sentence, true on both chains: `ReadAt`'s own `Display` writes
    // "slot 444007820" on Solana and "block 100" on Robinhood, and this is
    // the only place that spells either word -- see `ReadAt`'s doc comment.
    if let Some(read_at) = sheet.read_at {
        let _ = writeln!(out, "Read at {read_at}.");
    }
    // The floor is what the account publishes when the model's own reply is
    // refused, so it is the floor on the whole account's honesty too. At
    // `Sketchy` and `RugMechanicsLive` something fired and it is not proof
    // (design 0020 §5): stating one twin here is what keeps a template-only
    // reply from reading as an accusation the sheet never earned. `Rugged`
    // states none -- a confirmed pair is an observed completed event, and
    // hedging it would be false balance in the other direction -- and
    // `NothingUglyYet`/`CantTell` have no signal at all to twin.
    //
    // Matched one level per arm, not folded into a single `matches!` guard:
    // a mutation that flipped which levels qualify would still pass a test
    // that only checked "a twin appears somewhere," so each level is
    // asserted against on its own in the tests below.
    match level(sheet) {
        Level::Sketchy | Level::RugMechanicsLive => {
            if let Some(twin) = sheet.twins.first() {
                let _ = writeln!(out, "An innocent explanation: {twin}.");
            }
        }
        Level::Rugged | Level::NothingUglyYet | Level::CantTell => {}
    }
    out
}

/// A label short enough to post, with its caveat intact.
fn short(label: &str) -> &str {
    match label {
        l if l.contains("distinct token accounts receiving") => {
            "token accounts in the launch block (accounts, not people)"
        }
        l if l.contains("share of INSTANT graduations") => {
            "share of instantly-graduating launches in that band"
        }
        l if l.contains("share of launches that NEVER graduated") => {
            "share of never-graduated launches in that band"
        }
        // Matched on the words both the old "SOL that can be bought" label and
        // today's "quote asset that can be bought" one share: after the rename
        // this arm stopped firing, and the long label -- "(research 0022)" and
        // "the venue" included -- reached the floor, where `fidelity` read 0022
        // as a number and the capacity as a claim about the venue. No venue
        // word here for the same reason.
        l if l.contains("that can be bought before price moves 1%") => {
            "how much can be bought before 1% price impact (Real or Rug's own sizing budget, not a trading limit)"
        }
        l if l.contains("SOL the creator spent") => "the creator's own buy",
        l if l.contains("round trip for a position of") => "round trip on a $20-$200 position",
        // The population lines. Written as a comparison rather than as a
        // statistic, because the reader is holding the creator's count two lines
        // above and the sentence has to connect the two for them.
        l if l.contains("how many graduated at all") => {
            "across every launch Real or Rug has measured, how many graduated at all"
        }
        l if l.contains("how many showed almost no activity at all") => {
            "and how many showed almost no activity at all"
        }
        l if l.contains("how many filled their curve over time") => {
            "across every launch Real or Rug has measured, how many filled over time"
        }
        // The two lines that make one reply differ from the next, so they are
        // the two whose wording matters most. The sheet's labels are written to
        // be unambiguous to a model reading twenty of them; these are written to
        // be read once, by somebody deciding whether to buy.
        l if l.contains("tokens this creator has launched") => "tokens this creator has launched",
        l if l.contains("how many reached an AMM by filling over time") => {
            "of those, how many ever filled their curve over time"
        }
        // The Robinhood lines. The caveats the sheet's labels carry are kept,
        // not trimmed for length: "may be a pool, not a person" is what stops
        // the top-address line reading as an accusation about somebody, and
        // "not counting the curve" is what stops the holder count reading as
        // higher than it is. A short line that sheds either says something the
        // sheet did not measure.
        l if l.contains("held by the single largest address") => {
            "the largest single address's share of the supply outside the curve (may be a pool, \
             not a person)"
        }
        l if l.contains("addresses holding the token now") => {
            "addresses holding it, not counting the curve, the factory or the zero address"
        }
        l if l.contains("has the token graduated off the bonding curve") => {
            "has it graduated off its bonding curve"
        }
        l if l.contains("ETH the launcher spent") => {
            "the launcher's own buy in the launch transaction"
        }
        l if l.contains("venue fee, round trip, read from the on-chain schedule") => {
            "the venue's own fee, not the cost of trading"
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clause::Kind;
    use crate::sheet::{About, Fact};

    /// A sheet shaped like the real ones the box produced on 2026-09-04: a
    /// creator with a long record, the population beside it, the launch block,
    /// and the cost.
    fn a_real_shaped_sheet() -> FactSheet {
        FactSheet {
            mint: "ECQdbWN1jBAQ9GXGFxX9gqvoa6NT3weWe4SCpAaapump".to_owned(),
            read_at: Some(realorrug_types::ReadAt::Solana(realorrug_types::Slot(
                444_388_986,
            ))),
            facts: vec![
                // **Every band, in the order the snapshot lists them.** A
                // fixture carrying only the wanted one cannot catch a lookup
                // that finds the wrong band -- and that is exactly what
                // happened: written with one band, it passed, and against a
                // real sheet it published $0.20-$2's 3042 bps.
                Fact {
                    about: About::Measurement,
                    kind: Kind::CostBand,
                    clauses: Vec::new(),
                    label: "round trip for a position of $0.20-$2".to_owned(),
                    rendered: "3042 bps (30.4%)".to_owned(),
                    values: vec![3042.0, 30.4],
                },
                Fact {
                    about: About::Measurement,
                    kind: Kind::CostBand,
                    clauses: Vec::new(),
                    label: "round trip for a position of $2-$20".to_owned(),
                    rendered: "250 bps (2.5%)".to_owned(),
                    values: vec![250.0, 2.5],
                },
                Fact {
                    about: About::Measurement,
                    kind: Kind::CostBand,
                    clauses: Vec::new(),
                    label: "round trip for a position of $20-$200".to_owned(),
                    rendered: "456 bps (4.6%)".to_owned(),
                    values: vec![456.0, 4.6],
                },
                Fact::exact(
                    Kind::CreatorLaunches,
                    "tokens this creator has launched",
                    150.0,
                    "150",
                ),
                Fact::exact(
                    Kind::CreatorOrganic,
                    "how many reached an AMM by filling over time",
                    0.0,
                    "0",
                ),
                Fact::share(
                    Kind::VenueGraduated,
                    "of every measured launch, how many graduated at all",
                    0.0281,
                ),
                Fact::exact(
                    Kind::LaunchRecipients,
                    "distinct token accounts receiving the token in its own launch block",
                    4.0,
                    "4",
                ),
                Fact::share(
                    Kind::VenueStillborn,
                    "of every measured launch, how many showed almost no activity at all",
                    0.230,
                ),
            ],
            untrusted: vec![("token name".to_owned(), "GOAT".to_owned())],
            unknown: Vec::new(),
            signals: Vec::new(),
            twins: Vec::new(),
        }
    }

    #[test]
    fn the_headline_is_publishable_by_construction() {
        // It is handed to the model as a starting line and printed by the
        // floor, so it has to pass the same two checks every reply passes --
        // and it passes them by being built only out of the sheet, not by
        // being careful.
        let sheet = a_real_shaped_sheet();
        let headline = headline(&sheet).expect("a sheet with a creator record has one");
        assert!(
            crate::fidelity::check(&headline, &sheet.authorised()).is_empty(),
            "{:?}",
            crate::fidelity::check(&headline, &sheet.authorised())
        );
        assert!(
            crate::forbidden::check(&headline).is_empty(),
            "{:?}",
            crate::forbidden::check(&headline)
        );
        assert!(
            headline.chars().count() <= 100,
            "{} chars: {headline}",
            headline.chars().count()
        );
    }

    #[test]
    fn an_unknown_creator_gets_no_headline_rather_than_a_zero() {
        // Rule 9, and the direction that matters. A creator Radar has never
        // seen has NO record, not a record of zero launches -- and "0 launches
        // by this creator" would be a damning sentence invented out of an
        // absence. With no creator record and no launch block there is nothing
        // about this coin to lead with, so nothing is offered.
        let mut sheet = a_real_shaped_sheet();
        sheet.facts.retain(|f| {
            !f.label.contains("tokens this creator has launched")
                && !f
                    .label
                    .contains("how many reached an AMM by filling over time")
                && !f
                    .label
                    .contains("receiving the token in its own launch block")
        });
        assert_eq!(headline(&sheet), None);
        // And the floor still renders, without an empty line where the
        // headline would have been.
        let out = template(&sheet);
        assert!(
            !out.contains(
                "

"
            ),
            "a blank line was left behind: {out:?}"
        );
    }

    #[test]
    fn the_headline_falls_back_to_the_launch_block_when_the_creator_is_new() {
        let mut sheet = a_real_shaped_sheet();
        sheet.facts.retain(|f| {
            !f.label.contains("tokens this creator has launched")
                && !f
                    .label
                    .contains("how many reached an AMM by filling over time")
        });
        let headline = headline(&sheet).expect("the launch block is still about this coin");
        assert!(headline.contains("launch block"), "{headline}");
        assert!(crate::fidelity::check(&headline, &sheet.authorised()).is_empty());
    }

    #[test]
    fn the_reply_does_not_open_with_the_line_that_is_the_same_every_time() {
        // The defect this ordering fixes. Until 2026-09-05 the round trip led
        // every reply, and it is 456 bps in all of them -- so three different
        // coins opened with the same sentence, which reads as a bot repeating
        // itself rather than as something that looked at the coin. As of
        // 2026-09-17 there is no name-and-address header either (`template`'s
        // own doc comment), so line 0 is the headline itself.
        let out = template(&a_real_shaped_sheet());
        let first = out.lines().next().expect("a first line");
        assert!(
            first.contains("launches by this creator"),
            "the first line must be about this coin, got: {first}"
        );
        assert!(
            !first.contains("round trip"),
            "the constant must not lead: {first}"
        );
    }

    #[test]
    fn the_creators_record_is_followed_by_what_it_should_be_weighed_against() {
        // A count with no denominator is a number the reader cannot use. "None
        // of 150" sounds damning to somebody who assumes half should have, and
        // unremarkable to somebody who assumes none ever do -- neither of them
        // is informed, and the population is what decides.
        //
        // This was measured and then not published: the figures landed in the
        // sheet on 2026-09-04 and never reached a reply, because LEAD is a fixed
        // whitelist and they were not on it.
        let out = template(&a_real_shaped_sheet());
        let launched = out.find("creator has launched").expect("the count");
        let population = out
            .find("across every launch Real or Rug has measured")
            .expect("the denominator must be published");
        assert!(
            population > launched,
            "the population must come after the count it explains:
{out}"
        );
        // And in the same short list, not pushed off the end of it by the five
        // slot cap -- a denominator the reader never sees is one that was not
        // published.
        assert!(
            out.lines()
                .take(6)
                .any(|l| l.contains("across every launch")),
            "it must survive the cap:
{out}"
        );
    }

    #[test]
    fn the_cost_is_always_said_and_always_last() {
        // Out of competition for the five slots, because it is the same in every
        // reply -- but never dropped, because it is the only line that is about
        // the reader rather than about the coin.
        let out = template(&a_real_shaped_sheet());
        let cost = out
            .find("Entering and leaving")
            .expect("the cost line must survive being demoted");
        let last_fact = out.rfind("- ").expect("fact lines");
        assert!(
            cost > last_fact,
            "it must come after the facts:
{out}"
        );
        assert!(
            out.contains("456 bps"),
            "with its figure:
{out}"
        );
        assert!(
            !out.contains("3042"),
            "the cheapest band is listed first and must not be the one found:
{out}"
        );
    }

    #[test]
    fn a_sheet_with_no_cost_simply_has_no_cost_line() {
        // Rule 8. The snapshot may be absent, and inventing 456 here would make
        // this function the one place in the reply path allowed to publish a
        // number that is not on the sheet -- which is precisely what
        // `fidelity::check` refuses everywhere else.
        let mut sheet = a_real_shaped_sheet();
        sheet.facts.retain(|f| !f.label.contains("round trip"));
        let out = template(&sheet);
        assert!(!out.contains("Entering and leaving"), "{out}");
        assert!(!out.contains("456"), "{out}");
    }

    #[test]
    fn the_token_name_never_reaches_the_reply() {
        // It is the most obvious "improvement" to make -- "Radar on GOAT:" reads
        // far better than a base58 string -- and it is attacker-controlled text
        // (rule 4). A coin named "Radar says BUY" would have the bot publish
        // that. The mint cannot be spoofed; the name can, so it stays fenced in
        // `untrusted` and out of everything that gets posted.
        let out = template(&a_real_shaped_sheet());
        assert!(!out.contains("GOAT"), "{out}");
    }

    fn sheet() -> FactSheet {
        FactSheet {
            mint: "MintOne".to_owned(),
            read_at: Some(realorrug_types::ReadAt::Solana(realorrug_types::Slot(
                444_007_820,
            ))),
            facts: vec![
                Fact::exact(Kind::LaunchRecipients, "recipients", 11.0, "11"),
                Fact::share(
                    Kind::BandNeverGraduated,
                    "share of never-graduated in that band",
                    0.005,
                ),
            ],
            untrusted: vec![("token name".to_owned(), "Gay Pepe".to_owned())],
            unknown: vec!["the creator's launch count".to_owned()],
            signals: Vec::new(),
            twins: Vec::new(),
        }
    }

    #[test]
    fn a_fact_with_nothing_rendered_is_not_quoted() {
        // The selector is `label matches AND something was rendered`. With OR, a
        // fact whose value could not be read is printed with an empty value --
        // "- recipients: " -- which reads as a measurement of nothing rather
        // than as an absence. That is LEARNINGS 5's shape in a published reply.
        // The label has to be one `LEAD` looks for, or the selector never
        // reaches the second operand and the mutation is untested -- which is
        // how the first version of this passed while the mutant lived.
        let wanted = LEAD[0];
        let mut s = sheet();
        s.facts = vec![Fact {
            about: About::Measurement,
            kind: wanted,
            clauses: Vec::new(),
            label: format!("a label for {wanted:?}"),
            rendered: String::new(),
            values: vec![],
        }];
        s.unknown.clear();
        let out = template(&s);
        // Counted, not searched for by label: the template prints
        // `short(&fact.label)`, so looking for the full LEAD string in the
        // output cannot find the line even when it is there. The first version
        // of this test searched for it and passed while the mutant lived.
        let quoted = out.lines().filter(|l| l.starts_with("- ")).count();
        assert_eq!(quoted, 0, "an unrendered fact was quoted anyway:\n{out}");
    }

    #[test]
    fn the_reply_stops_at_the_fact_ceiling() {
        // `shown += 1` counts toward MAX_FACTS. Mutated to `*=` it stays at zero
        // for ever and the ceiling never binds, so a reply grows without limit
        // -- and the one place that shows up is a post that will not send.
        // The labels have to be ones `LEAD` actually looks for, or the loop
        // matches nothing and prints nothing -- which is how the first version
        // of this test passed while the mutant lived. LEAD carries six patterns
        // against a ceiling of four, so a sheet answering all six is the only
        // case where the ceiling is what stops a six-line post.
        assert!(
            LEAD.len() > MAX_FACTS,
            "the ceiling is unreachable if LEAD is no longer than it, and this test would prove nothing"
        );
        let mut s = sheet();
        s.facts = LEAD
            .iter()
            .map(|wanted| Fact::exact(*wanted, format!("a label for {wanted:?}"), 11.0, "11"))
            .collect();
        // The unknowns are printed as `- ` lines too, and they are not what the
        // ceiling governs. Counting them made the first run of this read five
        // against four and look like a defect in the ceiling rather than in the
        // count.
        s.unknown.clear();
        let out = template(&s);
        let quoted = out.lines().filter(|l| l.starts_with("- ")).count();
        assert_eq!(
            quoted, MAX_FACTS,
            "{quoted} facts quoted against a ceiling of {MAX_FACTS}:\n{out}"
        );
    }

    #[test]
    fn the_template_passes_its_own_fidelity_check() {
        // The property that makes it a safe floor. If the template could
        // contain a number the sheet does not authorise, then the thing that
        // ships when a reply is rejected would itself be unpublishable -- and
        // there would be nothing left to fall back to.
        let sheet = sheet();
        let text = template(&sheet);
        let caught = crate::fidelity::check(&text, &sheet.authorised());
        assert!(caught.is_empty(), "{caught:?}");
    }

    #[test]
    fn the_template_passes_the_forbidden_check() {
        let text = template(&sheet());
        assert!(crate::forbidden::check(&text).is_empty());
    }

    #[test]
    fn the_template_says_what_is_unknown_rather_than_omitting_it() {
        // "Radar has no record" said plainly, never as reassurance and never by
        // silence -- a reader who is not told something is missing assumes it
        // was checked.
        let text = template(&sheet());
        assert!(text.contains("not known: the creator's launch count"));
    }

    #[test]
    fn the_template_carries_the_slot_every_figure_was_read_at() {
        assert!(template(&sheet()).contains("444007820"));
    }

    #[test]
    fn an_untrusted_name_never_reaches_the_template() {
        // The template is the trusted floor; a creator-controlled string in it
        // would be a creator writing part of Radar's reply.
        assert!(!template(&sheet()).contains("Gay Pepe"));
    }

    #[test]
    fn the_verdict_is_a_function_of_the_sheet_alone() {
        // Replayability: the same facts give the same reasons, which is what
        // lets a published reply be reproduced from its recorded fact sheet.
        assert_eq!(Verdict::from(&sheet()), Verdict::from(&sheet()));
        assert!(
            Verdict::from(&sheet())
                .reasons
                .iter()
                .any(|r| r.contains("11"))
        );
    }

    #[test]
    fn an_empty_sheet_still_produces_a_publishable_reply() {
        // The case a stranger can force: a mint nothing could be read about.
        // It must produce a reply that says so, not an empty string and not a
        // reply implying everything was fine.
        let empty = FactSheet {
            mint: "MintTwo".to_owned(),
            read_at: None,
            facts: Vec::new(),
            untrusted: Vec::new(),
            unknown: vec!["the launch block could not be read".to_owned()],
            signals: Vec::new(),
            twins: Vec::new(),
        };
        let text = template(&empty);
        assert!(text.contains("not known"));
        assert!(crate::forbidden::check(&text).is_empty());
        assert!(crate::fidelity::check(&text, &empty.authorised()).is_empty());
    }

    /// A sheet carrying only signals and unknowns, for the level-function
    /// tests below -- the facts themselves are irrelevant to [`level`].
    ///
    /// Twins are derived from `signals` the same way [`FactSheet::build`]
    /// derives them, so the `template` tests below (which need a real twin
    /// on the sheet to assert against) can reuse this fixture rather than
    /// building a third one.
    fn sheet_with(signals: Vec<Signal>, unknown: Vec<String>) -> FactSheet {
        let twins = signals
            .iter()
            .map(|&signal| crate::sheet::twin_for(signal).to_owned())
            .collect();
        FactSheet {
            mint: "MintLevel".to_owned(),
            read_at: None,
            facts: Vec::new(),
            untrusted: Vec::new(),
            unknown,
            signals,
            twins,
        }
    }

    #[test]
    fn a_single_signal_never_reaches_rug_mechanics_live() {
        // Design 0020 §3 rule 4: a single signal never reaches the top two
        // levels. Each of the eight live-risk signals, alone, must land at
        // `Sketchy` -- if the `>= 2` in `level` were mutated to `>= 1`, every
        // one of these would report `RugMechanicsLive` instead.
        for signal in LIVE_RISK_SIGNALS {
            let sheet = sheet_with(vec![*signal], Vec::new());
            assert_eq!(
                level(&sheet),
                Level::Sketchy,
                "{signal:?} alone reached a top-two level"
            );
        }
    }

    #[test]
    fn two_live_risk_signals_reach_rug_mechanics_live() {
        // The positive case beside the negative one above: two signals that
        // are not a `Rugged`-qualifying pair still clear the "two or more"
        // bar. Catches a `>=` mutated to `>` (which would need three) as well
        // as one mutated to `==` (which would stop counting past two).
        let sheet = sheet_with(
            vec![
                Signal::CreatorBoughtOwnLaunch,
                Signal::LaunchBlockInStrongestBand,
                Signal::HolderConcentration,
            ],
            Vec::new(),
        );
        assert_eq!(level(&sheet), Level::RugMechanicsLive);
    }

    #[test]
    fn the_template_states_a_twin_at_sketchy() {
        // Packet 0038's defect: a single signal earns `Sketchy`, and the
        // template shipped no hedge at all -- an honest fact with a boring
        // explanation the bot knew about and never said. Asserted at this
        // level alone (not folded into a loop over every level) so a
        // mutation that swapped which levels qualify still fails here.
        let sheet = sheet_with(vec![Signal::CreatorBoughtOwnLaunch], Vec::new());
        assert_eq!(level(&sheet), Level::Sketchy);
        let text = template(&sheet);
        assert!(
            text.contains(crate::sheet::twin_for(Signal::CreatorBoughtOwnLaunch)),
            "{text}"
        );
    }

    #[test]
    fn the_template_states_a_twin_at_rug_mechanics_live_and_none_at_rugged() {
        let live = sheet_with(
            vec![
                Signal::CreatorBoughtOwnLaunch,
                Signal::LaunchBlockInStrongestBand,
            ],
            Vec::new(),
        );
        assert_eq!(level(&live), Level::RugMechanicsLive);
        let text = template(&live);
        assert!(
            text.contains(crate::sheet::twin_for(Signal::CreatorBoughtOwnLaunch)),
            "{text}"
        );

        // `Rugged` is an observed completed event, never on one reading
        // alone -- hedging it with a twin would be false balance in the
        // other direction, so the template must state none. Both twins the
        // qualifying pair carries are checked absent, not just the first
        // one, so a mutation that only strips the leading twin cannot hide
        // behind this assertion.
        let rugged = sheet_with(
            vec![Signal::CreatorSoldOut, Signal::BuyersCannotSell],
            Vec::new(),
        );
        assert_eq!(level(&rugged), Level::Rugged);
        let text = template(&rugged);
        assert!(
            !text.contains(crate::sheet::twin_for(Signal::CreatorSoldOut)),
            "{text}"
        );
        assert!(
            !text.contains(crate::sheet::twin_for(Signal::BuyersCannotSell)),
            "{text}"
        );
    }

    #[test]
    fn the_template_states_no_twin_when_nothing_fired() {
        // `NothingUglyYet` and `CantTell` have no signal at all, so there is
        // nothing to twin -- checked at both levels, not just one, since
        // they reach the same "no twin" branch by two different routes.
        let clean = sheet_with(Vec::new(), Vec::new());
        assert_eq!(level(&clean), Level::NothingUglyYet);
        assert!(!template(&clean).contains("innocent explanation"));

        let cant_tell = sheet_with(
            Vec::new(),
            vec!["the launch block could not be read".to_owned()],
        );
        assert_eq!(level(&cant_tell), Level::CantTell);
        assert!(!template(&cant_tell).contains("innocent explanation"));
    }

    #[test]
    fn a_missing_required_fact_forces_cant_tell_even_with_several_signals() {
        // Design 0020 §3's `CantTell` rule: any required fact unread wins over
        // `Sketchy` and over `RugMechanicsLive`, as long as no `Rugged`-qualifying
        // pair was also observed (the precedence case is its own test below).
        // If the `!sheet.unknown.is_empty()` check were dropped, this would
        // report `RugMechanicsLive` from the three signals instead.
        let sheet = sheet_with(
            vec![
                Signal::CreatorBoughtOwnLaunch,
                Signal::LaunchBlockInStrongestBand,
                Signal::RepeatLauncher,
            ],
            vec!["the bonding curve could not be read".to_owned()],
        );
        assert_eq!(level(&sheet), Level::CantTell);
    }

    #[test]
    fn an_observed_rugged_pair_beats_an_unrelated_missing_fact() {
        // Design 0020 §3's precedence section, named explicitly: an observed
        // `Rugged`-qualifying pair is built entirely from optional facts, so
        // a sheet that saw the rug but also failed to read something
        // unrelated must still report `Rugged`, not `CantTell` -- downgrading
        // an observed rug to a shrug because of an unrelated missing
        // timestamp is the worse failure. If the `Rugged` check ran after the
        // `unknown` check instead of before it, this would report `CantTell`.
        let sheet = sheet_with(
            vec![Signal::CreatorSoldOut, Signal::BuyersCannotSell],
            vec!["the launch block could not be read".to_owned()],
        );
        assert_eq!(level(&sheet), Level::Rugged);
    }

    #[test]
    fn each_rugged_pair_requires_both_members_not_either_alone() {
        // §3: "or `CreatorSoldOut` together with `BuyersCannotSell`" -- one of
        // the pair, alone, must not reach `Rugged`. Catches `&&` mutated to
        // `||` in either half of the `rugged` check.
        for lone in [Signal::LiquidityGone, Signal::HolderConcentration] {
            assert_ne!(level(&sheet_with(vec![lone], Vec::new())), Level::Rugged);
        }
        for lone in [Signal::CreatorSoldOut, Signal::BuyersCannotSell] {
            assert_ne!(level(&sheet_with(vec![lone], Vec::new())), Level::Rugged);
        }
        assert_eq!(
            level(&sheet_with(
                vec![Signal::LiquidityGone, Signal::HolderConcentration],
                Vec::new()
            )),
            Level::Rugged
        );
    }

    #[test]
    fn nothing_ugly_yet_is_unreachable_when_anything_is_unknown() {
        // ADR 0027 consequence 4: a blind spot must never read as an
        // endorsement. Sweeping every unknown-nonempty case here rather than
        // asserting one: if the `unknown` check were ever skipped for some
        // shape of sheet, this is where it would show up first.
        for unknown in [
            vec!["the launch block could not be read".to_owned()],
            vec!["the bonding curve could not be read".to_owned()],
            vec![
                "a".to_owned(),
                "b".to_owned(),
                "c".to_owned(),
                "d".to_owned(),
            ],
        ] {
            assert_ne!(
                level(&sheet_with(Vec::new(), unknown)),
                Level::NothingUglyYet
            );
        }
    }

    #[test]
    fn no_signal_and_nothing_unknown_is_nothing_ugly_yet() {
        // The floor of the ladder, and the only way to reach it: every
        // required fact read, no signal fired.
        assert_eq!(
            level(&sheet_with(Vec::new(), Vec::new())),
            Level::NothingUglyYet
        );
    }

    #[test]
    fn cant_tell_is_not_equal_to_nothing_ugly_yet() {
        // The comment on `Level` names the risk; this is the check that would
        // catch a future edit that gave the two variants the same
        // discriminant or merged them by accident -- `PartialEq` must treat
        // them as different, always.
        assert_ne!(Level::CantTell, Level::NothingUglyYet);
    }

    /// The sheet the box actually produced for
    /// `0x13e6cdB0470B10AfCB96177Ae8702ace2ac72cD6` on 2026-09-17, after the
    /// holder read was fixed: 529 addresses, the largest holding 50.2%, still
    /// on its curve, and a launcher who bought 0.05 ETH of their own token.
    ///
    /// **Built through `FactSheet::build` rather than written out as facts.**
    /// A hand-written fixture would carry my copy of each label, so a reword
    /// in `sheet.rs` would break [`LEAD`]'s matching in production and leave
    /// this test green -- which is the exact failure being fixed here, one
    /// layer up. Going through the real builder means the labels in the test
    /// are the labels the bot sees.
    fn the_live_robinhood_sheet() -> FactSheet {
        let dossier = realorrug_onchain::Dossier {
            mint: realorrug_types::ChainAddress::Robinhood(realorrug_robinhood::Address(
                [0x13u8; 20],
            )),
            read_at: Some(realorrug_types::ReadAt::Robinhood(3_012_345)),
            launch: None,
            curve: Some(realorrug_onchain::CurveFacts {
                creator: realorrug_types::ChainAddress::Robinhood(realorrug_robinhood::Address(
                    [9u8; 20],
                )),
                complete: false,
                quote_reserves: 6_186_150_833,
                quote_capacity: None,
                quote_asset: Some(realorrug_onchain::QuoteAsset::eth()),
                fees: None,
            }),
            creator_transactions: None,
            chain_launch: Some(realorrug_onchain::ChainLaunch {
                block: 2_998_000,
                age_seconds: Some(86_400),
                dev_buy_wei: Some(50_000_000_000_000_000),
                name: None,
                symbol: None,
            }),
            holders: Some(realorrug_onchain::Holders {
                count: 529,
                largest_share_bps: Some(5_022),
            }),
            funding: None,
            market: None,
            unavailable: Vec::new(),
            calls: 10,
            elapsed_ms: 7_577,
        };
        FactSheet::build(&dossier, None, None, None, None)
    }

    /// Renaming a fact's label must not change what the headline or the
    /// template selects. This is the property the old `LEAD: &[&str]` did not
    /// have: matching on `Kind` instead means a label rewrite changes only
    /// what a reader sees a fact *called*, never whether it is said.
    #[test]
    fn renaming_a_facts_label_does_not_change_what_leads() {
        let original = the_live_robinhood_sheet();
        let mut renamed = original.clone();
        for f in &mut renamed.facts {
            f.label = format!("a completely rewritten label for {:?}", f.kind);
        }

        assert_eq!(headline(&original), headline(&renamed));

        let original_reply = template(&original);
        let renamed_reply = template(&renamed);
        assert!(original_reply.contains("529") && original_reply.contains("50.2%"));
        assert!(renamed_reply.contains("529") && renamed_reply.contains("50.2%"));
    }

    #[test]
    fn a_robinhood_reply_prints_the_facts_the_sheet_measured() {
        // The bug this test exists for: every entry in `LEAD` was a Solana
        // label, so on a Robinhood sheet the loop matched nothing, printed no
        // facts at all, and shipped a reply that was the launch age and the
        // block number -- with the 529 holders and the 50.2% top address
        // measured, on the sheet, and dropped in silence.
        //
        // Re-applying the bug is deleting the four Robinhood entries from
        // `LEAD`; every assertion below then fails.
        let reply = template(&the_live_robinhood_sheet());

        assert!(
            reply.contains("529"),
            "the holder count is missing: {reply}"
        );
        assert!(
            reply.contains("50.2%"),
            "the top address's share is missing: {reply}"
        );
        // The caveats travel with the numbers or the numbers say something
        // that was not measured.
        assert!(
            reply.contains("may be a pool, not a person"),
            "the top-address caveat is missing: {reply}"
        );
        assert!(
            reply.contains("not counting the curve"),
            "the holder-count caveat is missing: {reply}"
        );
        assert!(
            reply.contains("has it graduated off its bonding curve: no"),
            "the graduation line is missing: {reply}"
        );
        assert!(
            reply.contains("0.0500 ETH"),
            "the launcher's own buy is missing: {reply}"
        );
    }

    #[test]
    fn a_robinhood_reply_leads_with_the_concentration_not_the_block_number() {
        // The first line is what gets screenshotted, so it carries the count
        // and the number that decides how to read the count. A reply led by
        // "Read at block 3012345" is the one the owner objected to on
        // 2026-09-17: true, and about the instrument rather than the coin.
        let sheet = the_live_robinhood_sheet();
        assert_eq!(
            headline(&sheet).as_deref(),
            Some(
                "529 addresses hold it, but the biggest balance -- 50.2% of it -- is still \
                 unidentified."
            )
        );
        let reply = template(&sheet);
        let first = reply.lines().next().unwrap_or_default();
        assert!(
            first.contains("529") && first.contains("50.2%"),
            "the headline is not the first line: {reply}"
        );
    }

    #[test]
    fn a_robinhood_reply_without_a_top_share_still_counts_the_holders() {
        // `largest_share_bps` is `None` when nobody holds any -- the sheet
        // says so -- and the headline must not go silent because one of its
        // two numbers was absent. Absent is not zero (rule 8), so the line
        // drops the share rather than printing 0%.
        let mut dossier_sheet = the_live_robinhood_sheet();
        dossier_sheet
            .facts
            .retain(|f| !f.label.contains("held by the single largest address"));
        assert_eq!(
            headline(&dossier_sheet).as_deref(),
            Some("529 addresses hold it, not counting the bonding curve.")
        );
    }

    /// The Robinhood fixture from design doc §1 refutation 8 and §4: 529
    /// holders and a 50.2% top-holder share whose owner is unknown must both
    /// surface, bundled, ahead of the launch age and the block the sheet was
    /// read at -- and never as "a whale" or "one wallet can dump."
    #[test]
    fn the_529_holder_fixture_surfaces_ahead_of_age_and_block() {
        let sheet = the_live_robinhood_sheet();
        let reply = template(&sheet);
        let holders_line = reply
            .lines()
            .position(|l| l.contains("529") && l.contains("50.2%"))
            .expect("the concentration bundle is somewhere in the reply");
        let age_or_block_line = reply
            .lines()
            .position(|l| l.starts_with("Launched") || l.starts_with("Read at"))
            .expect("the age or read-at line is somewhere in the reply");
        assert!(
            holders_line < age_or_block_line,
            "concentration did not lead the age/block line: {reply}"
        );
        assert!(!reply.to_lowercase().contains("whale"));
        assert!(!reply.to_lowercase().contains("one wallet can dump"));
    }

    #[test]
    fn a_solana_reply_is_unchanged_by_the_robinhood_entries() {
        // The two label sets are disjoint, which is what lets one `LEAD`
        // serve both chains. If a future Robinhood entry were worded so that
        // a Solana label matched it, a Solana reply would start printing a
        // fact out of order or twice; this pins the Solana output so that
        // shows up here rather than on the account.
        let reply = template(&a_real_shaped_sheet());
        assert!(
            !reply.contains("addresses hold it"),
            "a Robinhood line reached a Solana reply: {reply}"
        );
        assert!(
            reply.contains("tokens this creator has launched"),
            "the Solana lead was displaced: {reply}"
        );
    }

    /// What a stranger on the website reads, and what they must never read.
    ///
    /// `rendered` and `label` are written for the model and for the fidelity
    /// check: they argue with a future engineer, in capitals, about documents
    /// nobody outside this repository can open. `/v1/check/` published them
    /// verbatim. Re-apply the bug by putting `format!("{}: {}", fact.label,
    /// fact.rendered)` back into `Verdict::from` and both the first and the
    /// last assertion fail.
    #[test]
    fn a_public_reason_is_the_plain_sentence_not_the_engineers_argument() {
        // The real graduated-capacity fact, copied from `sheet.rs` because
        // that is the one that reached a reader's screen.
        let graduated = Fact::exact(
            Kind::CapacityAfterGraduation,
            "quote asset that can be bought before price moves 1% -- this is REAL OR RUG.S OWN \
             impact budget, NOT a ceiling the venue imposes (research 0022)",
            0.0,
            "graduated off the curve; it trades on the AMM, which Real or Rug does not price. \
             NOT zero, and NOT 'cannot size into this'.",
        )
        .saying(
            crate::clause::Voice::Plain,
            "Real or Rug does not price the AMM it moved to, so it has no exit size for this one.",
        )
        .saying(
            crate::clause::Voice::Blunt,
            "Real or Rug cannot size the AMM it moved to.",
        );
        assert_eq!(
            public_reason(&graduated),
            "Real or Rug does not price the AMM it moved to, so it has no exit size for this one."
        );

        // A fact nobody wrote a sentence for is still published, in whatever
        // form there is: unreadable beats absent, because a reason that
        // vanishes is a measurement the reader never learns was taken
        // (AGENTS.md rule 8). Re-apply that half by making the fallback
        // return an empty string and this fails.
        let unwritten = Fact::exact(
            Kind::CreatorLaunches,
            "tokens this creator has launched",
            150.0,
            "150",
        );
        assert_eq!(
            public_reason(&unwritten),
            "tokens this creator has launched: 150"
        );

        // Through the real construction, which is the path `/v1/check/`
        // actually takes.
        let mut sheet = a_real_shaped_sheet();
        sheet.facts = vec![graduated];
        let reasons = Verdict::from(&sheet).reasons;
        assert!(
            !reasons
                .iter()
                .any(|reason| reason.contains("NOT") || reason.contains("research 0022")),
            "the engineer's own argument reached the website: {reasons:?}"
        );
    }
}
