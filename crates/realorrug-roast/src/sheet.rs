// SPDX-License-Identifier: Apache-2.0
//! The fact sheet: every number the analyst is allowed to say.
//!
//! # This type is the security boundary
//!
//! The model is given this and nothing else, and afterwards every numeric
//! literal in what it wrote is checked back against it. So the set of numbers
//! reachable from here **is** the set of numbers that can be published, and a
//! field added here is a claim authorised.
//!
//! That is the same shape as `radar-signer`'s `verify::check`, which re-decodes
//! the bytes to confirm they match the authorisation rather than trusting the
//! caller's description of them. *The signer re-reads the bytes it signs; the
//! roaster re-reads the numbers it posts.*
//!
//! # Why the numbers are enumerated rather than inferred
//!
//! [`FactSheet::authorised`] lists every value a reply may contain, in every
//! form it may take — a share appears both as its ratio and as its percentage,
//! because a model told "0.251" will reasonably write "25%". Enumerating is
//! deliberate: the alternative is a checker that tries to guess which
//! transformations of a fact are legitimate, and a checker that guesses is one
//! that can be argued into accepting a number nobody measured.

use realorrug_onchain::budget::Count;
use realorrug_onchain::dossier::{Exemption, ExemptionSource, Powers};
use realorrug_onchain::market::MarketSnapshot;
use realorrug_onchain::{ChainLaunch, Dossier, Funding, Holders, LaunchBlock, Unavailable};
#[cfg(test)]
use realorrug_types::Slot;
use realorrug_types::{ReadAt, SlotDelta};

use crate::baserates::BaseRates;
use crate::clause::{Clause, Kind, Voice};
use crate::fidelity::{Authorised, Subject};
use std::fmt::Write as _;

/// What a fact is a claim about, because one kind carries an extra
/// obligation: it must state the moment it was read at.
///
/// ADR 0033 supersedes ADR 0013 constraint 5: the analyst may state price,
/// market capitalisation and liquidity for every token, including its own
/// (ADR 0013 constraint 6, now literal). What constraint 5 asked for instead
/// -- "a price without its moment is a stale price that looks current" -- is
/// enforced through this tag: a fact built with [`About::Price`] carries the
/// block or time it was read at in its label or its rendering, so the model
/// cannot repeat the number without repeating when it was true.
///
/// [`Fact::exact`] and [`Fact::share`] tag a measurement, so an author adding
/// a price or market-cap line through them and not through a literal still
/// has to choose the tag. There is no way to make the compiler ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum About {
    /// Structure, history, depth, cost or population -- what the analyst
    /// exists to state, about any token including its own.
    Measurement,
    /// The token's price or market capitalisation, in any unit and any form.
    Price,
}

/// One publishable number, with the words that make it a claim.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fact {
    /// What kind of claim this is. Decides whether the self-mint rule drops it.
    pub about: About,
    /// A stable name for the measurement, which survives the sheet.
    ///
    /// The position of a fact in [`FactSheet::facts`] does not: the list is
    /// built conditionally, so the third fact is a different measurement on two
    /// sheets. Anything that has to refer to this fact later — a selection, a
    /// log line, a receipt — refers to this.
    pub kind: crate::clause::Kind,
    /// What it is, in the fact sheet the model reads.
    pub label: String,
    /// How it renders.
    pub rendered: String,
    /// Every numeric value this fact authorises.
    ///
    /// More than one because a single measurement has several honest
    /// renderings: 0.251, 25.1 and 25 are the same fact said three ways, and a
    /// model that picks a different one has not invented anything.
    pub values: Vec<f64>,
    /// The complete sentences this fact may be published as, one per register.
    ///
    /// **Empty means the fact is true and unpublishable.** It is shown to the
    /// model as context it may reason from and given no number it could select,
    /// so a measurement cannot reach a timeline before somebody has written the
    /// sentence that states it. See [`crate::clause`].
    pub clauses: Vec<crate::clause::Clause>,
}

impl Fact {
    /// A measured fact whose only value is the one in its rendering.
    ///
    /// A **measurement**, never a price: a price or market-cap fact is built as
    /// a literal with [`About::Price`], so that the choice is written down where
    /// the self-mint rule can read it.
    ///
    /// Built with no clauses. Add them with [`Fact::saying`]; a fact that never
    /// gets one is context and not copy.
    #[must_use]
    pub fn exact(
        kind: crate::clause::Kind,
        label: impl Into<String>,
        value: f64,
        rendered: impl Into<String>,
    ) -> Self {
        Self {
            about: About::Measurement,
            kind,
            label: label.into(),
            rendered: rendered.into(),
            values: vec![value],
            clauses: Vec::new(),
        }
    }

    /// Adds one vetted clause.
    ///
    /// The whole sentence, written here, by the code that read the measurement:
    /// subject, verb, number, unit, window and limitation. Nothing downstream
    /// completes it.
    #[must_use]
    pub fn saying(mut self, voice: crate::clause::Voice, text: impl Into<String>) -> Self {
        self.clauses.push(crate::clause::Clause::new(voice, text));
        self
    }

    /// A share, authorised as a ratio, a percentage, and the percentage rounded.
    ///
    /// The rounded form is included because a reply that says "a quarter of
    /// them" or "25%" for 25.1% is being *readable*, not inventing. The check
    /// exists to stop fabrication, and a tolerance narrow enough to forbid
    /// ordinary rounding would push every reply to the deterministic template.
    #[must_use]
    pub fn share(kind: crate::clause::Kind, label: impl Into<String>, ratio: f64) -> Self {
        let pct = ratio * 100.0;
        // Precision follows the magnitude, and this is not cosmetic. The
        // strongest finding in 0024 is that launches with one to three
        // recipients graduate instantly **0.02%** of the time; at one decimal
        // place that renders as "0.0%", which reads as *never* rather than as
        // *rare*. A reply that says a thing never happens when it happens two
        // times in twelve thousand is wrong in the direction that gets quoted
        // back at you.
        let rendered = if pct > 0.0 && pct < 0.1 {
            format!("{pct:.2}%")
        } else {
            format!("{pct:.1}%")
        };
        Self {
            about: About::Measurement,
            kind,
            label: label.into(),
            rendered,
            values: vec![
                ratio,
                pct,
                pct.round(),
                (pct * 10.0).round() / 10.0,
                (pct * 100.0).round() / 100.0,
            ],
            clauses: Vec::new(),
        }
    }
}

/// One thing on the sheet that, by the published rule, is a reason to refuse.
///
/// Design 0009 §5, M3: the hunter rank counts, per summoned reply, the refusal
/// signals the fact sheet carried at the time. These are those, as a type
/// rather than a number, so the log says *which* fired and a later rule change
/// can re-score old replies from the record.
///
/// **The model never sees these.** They are not rendered into the sheet and the
/// word "signal" appears nowhere the model reads: the bot states measured
/// facts, and "this is a reason to refuse" is a verdict the facts already
/// carry. The count exists for the leaderboard, and it travels on the reply
/// log entry beside the sheet it was counted from.
///
/// Each variant is read off a fact the sheet states, never inferred from one it
/// could not read: a truncated recipient count, an unmeasured creator and an
/// unseen dev buy are all *no signal*, not a signal of zero (rule 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    /// The launch block's recipient count lands in the snapshot's strongest
    /// band -- the one most enriched for instant graduation -- or above it.
    ///
    /// Read from the snapshot, not named: research 0024 is the record of the
    /// band moving off six, and a constant here would have fired on the wrong
    /// launches from the day it moved.
    ///
    /// **One variant for every chain, deliberately.** Design 0020 §6's table
    /// first proposed a separate `LaunchBlockBundle` for a Robinhood-shaped
    /// band, distinct from this Solana-shaped one -- the owner's later
    /// decision (one bot, one voice, a chain is data the bot carries, not a
    /// different bot) overrides that: the *meaning* ("this launch block's
    /// distinct-recipient count lands in the strongest band the current
    /// snapshot has measured") is identical on every chain, only the
    /// snapshot the band comes from differs, and the snapshot is already an
    /// argument (`rates: &BaseRates`), not part of the signal's identity. A
    /// chain-forked variant here would be a `match venue` waiting to happen
    /// one call site up.
    LaunchBlockInStrongestBand,
    /// The creator has measured launches and none of them filled over time.
    CreatorNeverGraduatedOrganically,
    /// The creator bought their own token at launch: in the launch block on
    /// Solana, in the launch transaction on Robinhood Chain.
    CreatorBoughtOwnLaunch,
    /// The curve's reserves collapsed pre-graduation while holders still hold
    /// supply they cannot exit through it.
    ///
    /// Design 0020 §3: fires only pre-graduation, because `tokenReserve == 0`
    /// post-graduation is the curve's normal end state, not a drain
    /// (research 0040 §3). Nothing constructs this yet -- the Robinhood Chain
    /// read that would (`getReserves()`, `phase`) does not exist in this crate
    /// today.
    LiquidityGone,
    /// The creator held a nonzero balance at some earlier read and now holds
    /// nothing.
    ///
    /// Design 0020 §3: needs a prior observation to mean anything (design
    /// 0021's job), so on a first-ever read this cannot fire.
    CreatorSoldOut,
    /// A simulated sell against the curve reverted.
    ///
    /// Design 0020 §3: read via `eth_call`, checking for a revert rather than
    /// sending a transaction.
    BuyersCannotSell,
    /// The creator or a launch-block buyer recurs across many launch blocks
    /// in a rolling window.
    ///
    /// Design 0020 §3, research 0042 port-order item 1. Bands not yet
    /// measured for Robinhood Chain.
    RepeatLauncher,
    /// The largest non-curve holder's share of circulating supply lands above
    /// a measured threshold.
    ///
    /// Design 0020 §3: threshold not yet measured for Pons v2.
    HolderConcentration,
    /// The owner's live powers over this launch. On Pons v2, per [ADR
    /// 0035](../../../docs/adr/0035-s13-owner-powers-live-is-the-pons-v2-reads-not-a-bytecode-scan.md):
    /// a nonzero creator tax, a pending creator-fee-recipient timelock, or a
    /// snipe-tax exemption granted to an address off both research 0047
    /// §3's first-party list and the launch's own declared list.
    ///
    /// Design 0020's original firing rule for this variant -- a deployed
    /// bytecode/ABI scan for an owner-only mint/pause/blacklist selector --
    /// stays parked behind research 0044 (ADR 0035 decision 2); it is not
    /// what fires this variant on Pons v2 today.
    OwnerCanStillMintOrPause,
    /// Wallets that bought the launch in a way that links them (research
    /// 0052 §3.2) sold within one [`realorrug_onchain::wallets::
    /// SELL_CLUSTER_WINDOW_BLOCKS`]-block window of each other.
    ///
    /// Research 0052 §3.1's S7 row: "cannot be hidden -- the sell is the
    /// point", unlike the launch-block signals above, whose gaming counters
    /// are all about *not looking linked at launch*. Pre-graduation only
    /// today: a graduated curve stops emitting `CurveSell`
    /// (`realorrug_onchain::wallets::correlated_selling`'s own doc), so this
    /// only ever fires from reads inside the bonding-curve window.
    CorrelatedSelling,
}

/// The innocent, on-chain-identical reading of a signal, from design 0020
/// §3's own "innocent twin" column.
///
/// **One `match`, exhaustive, no `_ =>` arm.** That absence is the whole
/// enforcement this function exists for: `Signal` grows a variant, this
/// function fails to compile, and the packet that lands the new signal
/// cannot ship without also writing what an innocent reading of it looks
/// like. Every sentence is phrased as what was read, not as a name for the
/// signal, because this is the only channel through which the model learns
/// there is another explanation (`FactSheet::render` never prints the
/// signal itself) -- and every sentence carries no digit, because
/// `forbidden.rs` checks every number in a reply against the sheet's facts,
/// and a number that existed only here would be a fact the model could
/// state and the check could not source.
pub(crate) fn twin_for(signal: Signal) -> &'static str {
    match signal {
        Signal::LaunchBlockInStrongestBand => {
            "a launch-block recipient count in this band can also be a launch people were \
             waiting for; no chain fact tells the two apart"
        }
        Signal::CreatorNeverGraduatedOrganically => {
            "a small number of measured launches reads the same whether none of them ever had \
             a real chance to graduate or the creator has simply not launched enough yet for \
             the record to mean much"
        }
        Signal::CreatorBoughtOwnLaunch => {
            "a creator buying into their own launch block reads the same as a creator buying a \
             token they believe in"
        }
        Signal::LiquidityGone => {
            "reserves that emptied pre-graduation read the same whether the creator drained \
             them or every buyer simply sold back to the curve on their own"
        }
        Signal::CreatorSoldOut => {
            "a creator wallet that now holds nothing reads the same whether the tokens were \
             sold or only moved to another wallet the creator still holds them in"
        }
        Signal::BuyersCannotSell => {
            "a simulated sell that fails reads the same whether the curve is broken, the \
             simulated size was too large for a curve with real but thin depth, or the launch \
             is still in its opening seconds, when the launchpad's own sell tax is near total \
             on every token"
        }
        Signal::RepeatLauncher => {
            "a creator who recurs across many launch blocks reads the same whether a person is \
             launching many tokens themselves or an unnamed relayer or bot is launching them on \
             other people's behalf, automatically and without coordination"
        }
        Signal::HolderConcentration => {
            "a wallet holding a large share of supply reads the same whether it belongs to a \
             single holder or is a vesting contract, a bridge or an exchange that nobody has \
             labelled yet"
        }
        Signal::OwnerCanStillMintOrPause => {
            "a creator tax, a pending fee-recipient change or a snipe-tax exemption reads the \
             same whether the creator plans to use that power against buyers or is simply using \
             the launchpad's own built-in mechanism the way every launch on it does"
        }
        Signal::CorrelatedSelling => {
            "wallets that look linked selling in the same short window reads the same whether \
             they are one actor cashing out or several separate early buyers who all decided, \
             on their own, that the same moment was a good time to take profit"
        }
    }
}

/// A short, digit-free phrase naming what this signal read, for a surface
/// that cannot show numbers.
///
/// **Signals, not [`crate::verdict::Verdict::reasons`], are the source for
/// any UI that must never contradict the verdict word.** `verdict::level`
/// picks the ladder level from `sheet.signals` alone (verdict.rs), so the
/// signals *are* the reason for the stamp; `reasons` is a separately ordered
/// list ("the order they are worth reading", not "the order that decided
/// the level") and can lead with something that did not move the verdict at
/// all. Reading signals instead keeps a card's wording and its headline
/// word from ever being able to disagree.
///
impl Signal {
    /// One `match`, exhaustive, no `_ =>` arm, same discipline as
    /// [`twin_for`]: a new variant that is not given a phrase here fails to
    /// compile rather than rendering as a blank line on a public card.
    #[must_use]
    pub fn plain(self) -> &'static str {
        match self {
            Signal::LaunchBlockInStrongestBand => "the launch drew a burst of buyers instantly",
            Signal::CreatorNeverGraduatedOrganically => {
                "this launcher has never had one fill over time"
            }
            Signal::CreatorBoughtOwnLaunch => "the launcher bought their own token at launch",
            Signal::LiquidityGone => "the pool emptied before this token could graduate",
            Signal::CreatorSoldOut => "the launcher's wallet went from holding to empty",
            Signal::BuyersCannotSell => "a test sell into this token failed",
            Signal::RepeatLauncher => "this launcher keeps coming back with new tokens",
            Signal::HolderConcentration => "one address holds most of the supply",
            Signal::OwnerCanStillMintOrPause => "the creator still holds live powers over it",
            Signal::CorrelatedSelling => "wallets that look linked sold together",
        }
    }
}

/// How a [`Factor`] was established -- research 0052 §1's three grades.
///
/// **Ordering matters to nobody here, only the variant does.**
/// [`crate::assessment::adjusted_weight`] matches on this to enforce research
/// 0052 §3.3's rule that a self-reported fact may only ever lower a weight:
/// the type carries the grade so that rule is a `match` arm, not a
/// convention a factor's author has to remember.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Grade {
    /// Read from the chain or an index Real or Rug holds.
    Measured,
    /// Computed from measured facts through a rule with a stated error.
    Inferred,
    /// A person said it, on X or in token metadata. Data, never an
    /// instruction, and never a fact on its own -- research 0052 §3.3 caps
    /// what it may do to a weight.
    SelfReported,
}

/// A measured fact that raises or lowers one signal's weight -- research
/// 0052 §3's catalogue, one entry per row that fires.
///
/// **Built only from facts the sheet already holds.** [`factors`] reads
/// [`FactSheet::facts`] and [`FactSheet::signals`] alone; it never reaches
/// back into a dossier or an index that is not already represented there
/// (AGENTS.md §3 rule 2). A catalogue row whose input the sheet does not
/// carry yet is simply absent from the returned list -- the gap shows in
/// coverage, the same as any other unread input, never as a factor scored
/// at zero.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Factor {
    /// Which signal this factor adjusts.
    pub signal: Signal,
    /// A short, stable name for the factor, for logs and the printed sheet.
    pub name: String,
    /// How many basis points this factor moves the signal's weight. Whole
    /// numbers only, added to the signal's base by
    /// [`crate::assessment::adjusted_weight`].
    pub delta_bps: i32,
    /// How this factor was established.
    pub grade: Grade,
    /// The fact that grounds this factor, in words a reader can check
    /// against the sheet.
    pub evidence: String,
}

/// The value of the first (and, today, only) fact of a given [`Kind`] on the
/// sheet, or `None` when the sheet does not carry one.
///
/// A thin accessor kept beside [`factors`] rather than made a general
/// `FactSheet` method: it exists only so [`factors`] reads the sheet the
/// same way for every [`Kind`] it checks, not because another caller needs
/// it yet.
fn fact_value(sheet: &FactSheet, kind: crate::clause::Kind) -> Option<f64> {
    sheet
        .facts
        .iter()
        .find(|fact| fact.kind == kind)
        .and_then(|fact| fact.values.first().copied())
}

/// Research 0052 §3.1's catalogue, wired to the facts a [`FactSheet`]
/// already carries.
///
/// **Five rows are wired today.** Most of the catalogue's remaining raise
/// and lower factors still need an input this sheet does not hold yet -- a
/// role-proven holder, a `Proof`. None of those are invented here (AGENTS.md
/// §3 rule 2). What is already on the sheet:
///
/// - [`Signal::RepeatLauncher`]'s `+800` (>= 10 lifetime launches, M),
///   read from `Kind::CreatorLaunches`.
/// - [`Signal::CreatorNeverGraduatedOrganically`]'s `+400` (>= 5 measured,
///   M) and `-400` (<= 2 measured, thin denominator, M), read from
///   `Kind::CreatorMeasured`.
/// - [`Signal::CreatorBoughtOwnLaunch`]'s share-of-supply raise and lower
///   (research 0052 §3.1's S1 row; corrected 2026-09-18 -- it does not need
///   the launch-block `eth_call` the row assumed, because `dev_buy_tokens`
///   and `supply` come from the launch receipt already read), from
///   `Kind::DevBuyShare` in bps: `+1500` if `>= 1,000`, else `+800` if
///   `>= 500`, else `-400` if `< 100`. The declared-in-calldata `-300` and
///   the announced-on-X `-200` are out of this packet's scope.
/// - [`Signal::HolderConcentration`]'s two raises (research 0052 §3.1's S5
///   row): `+1000` if `>= 2,000` bps, else `+600` if `>= 1,000` bps, read
///   from `Kind::LargestHolderShare` -- the same "share of supply outside
///   the curve" fact [`push_holders`] already renders. **Not the S5 row's
///   `largest_non_infrastructure`**: that name is `roles::Concentration`'s
///   role-proven reading, which nothing projects onto this sheet yet, so
///   these two raises read the coarser, already-published share instead.
///   That substitution is what the row's own "0 for 'may be a pool'" line
///   asks for -- an unresolved large balance keeps full weight -- so the
///   raises are the *default* reading and the row's `-600` lower (which
///   needs a `Proof` this sheet does not carry) is the only piece left out.
///   The row's third raise (`+800` for a top-10 share) is also left out:
///   nothing here computes a top-10 sum, only the single largest address.
/// - [`Signal::LaunchBlockInStrongestBand`]'s five factors (research 0052
///   §3.1's S2 row): `+1,000` if checked same-window buyers hold `>= 1,000`
///   bps together (M, raw/unweighted), `+800` if `>= 3` are fresh (M),
///   `+500` if their spends are within 10% of each other (I), `-500` if
///   every launch-window buyer is declared-exempt (M, only when the full
///   buyer list -- not a sample -- is known) and `-300` if the recipient
///   band was measured on `< 200` launches (I, thin sample). See
///   [`launch_block_band_factors`]'s own doc comment for the sampling
///   caveat this row's raises carry.
///
/// All five fire only when the signal itself already fired -- a factor
/// with no signal to adjust would have nothing to attach to on the sheet
/// the model reads. [`Signal::HolderConcentration`] is declared but not yet
/// pushed by [`FactSheet::build`] on any chain (its own threshold is not
/// measured for Pons v2, design 0020 §3), so this factor is exercised by a
/// sheet built directly in a test today, the same way
/// [`crate::assessment`]'s per-episode base weight for it already is.
#[must_use]
pub fn factors(sheet: &FactSheet) -> Vec<Factor> {
    let mut factors = Vec::new();

    if sheet.signals.contains(&Signal::RepeatLauncher)
        && let Some(launches) = fact_value(sheet, crate::clause::Kind::CreatorLaunches)
        && launches >= 10.0
    {
        factors.push(Factor {
            signal: Signal::RepeatLauncher,
            name: "lifetime launches >= 10".to_owned(),
            delta_bps: 800,
            grade: Grade::Measured,
            evidence: format!(
                "{launches:.0} tokens this creator has launched, in Real or Rug's record"
            ),
        });
    }

    if sheet
        .signals
        .contains(&Signal::CreatorNeverGraduatedOrganically)
        && let Some(measured) = fact_value(sheet, crate::clause::Kind::CreatorMeasured)
    {
        if measured >= 5.0 {
            factors.push(Factor {
                signal: Signal::CreatorNeverGraduatedOrganically,
                name: "measured launches >= 5".to_owned(),
                delta_bps: 400,
                grade: Grade::Measured,
                evidence: format!("{measured:.0} of this creator's launches have been measured"),
            });
        } else if measured <= 2.0 {
            factors.push(Factor {
                signal: Signal::CreatorNeverGraduatedOrganically,
                name: "measured launches <= 2, thin denominator".to_owned(),
                delta_bps: -400,
                grade: Grade::Measured,
                evidence: format!(
                    "only {measured:.0} of this creator's launches have been measured"
                ),
            });
        }
    }

    if sheet.signals.contains(&Signal::CreatorBoughtOwnLaunch)
        && let Some(bps_f64) = fact_value(sheet, crate::clause::Kind::DevBuyShare)
    {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "DevBuyShare's value is an integer bps in [0, 10_000+] pushed by \
                      push_dev_buy_share, which never carries a fraction"
        )]
        let bps = bps_f64.round() as i64;
        // A `match` on ranges, not a chain of `>=`/`<` comparisons: flipping
        // one boundary in a chain can leave every existing fixture passing
        // (a mutant that a test suite never notices), while a range bound
        // moving here changes which arm a boundary value lands in.
        match bps {
            1_000..=i64::MAX => factors.push(Factor {
                signal: Signal::CreatorBoughtOwnLaunch,
                name: "own-launch share >= 1,000 bps".to_owned(),
                delta_bps: 1_500,
                grade: Grade::Measured,
                evidence: format!(
                    "the launcher's launch-transaction buy is {:.2}% of total supply",
                    bps_f64 / 100.0
                ),
            }),
            500..=999 => factors.push(Factor {
                signal: Signal::CreatorBoughtOwnLaunch,
                name: "own-launch share >= 500 bps".to_owned(),
                delta_bps: 800,
                grade: Grade::Measured,
                evidence: format!(
                    "the launcher's launch-transaction buy is {:.2}% of total supply",
                    bps_f64 / 100.0
                ),
            }),
            i64::MIN..=99 => factors.push(Factor {
                signal: Signal::CreatorBoughtOwnLaunch,
                name: "own-launch share < 100 bps".to_owned(),
                delta_bps: -400,
                grade: Grade::Measured,
                evidence: format!(
                    "the launcher's launch-transaction buy is only {:.2}% of total supply",
                    bps_f64 / 100.0
                ),
            }),
            _ => {}
        }
    }

    if sheet.signals.contains(&Signal::HolderConcentration) {
        holder_concentration_factors(sheet, &mut factors);
    }

    if sheet.signals.contains(&Signal::CorrelatedSelling) {
        correlated_selling_factors(sheet, &mut factors);
    }

    if sheet.signals.contains(&Signal::OwnerCanStillMintOrPause) {
        owner_powers_factors(sheet, &mut factors);
    }

    if sheet.signals.contains(&Signal::LaunchBlockInStrongestBand) {
        launch_block_band_factors(sheet, &mut factors);
    }

    factors
}

/// S2's five raise/lower factors (research 0052 §3.1's
/// `LaunchBlockInStrongestBand` row), split out of [`factors`] itself so
/// that function stays under clippy's line count -- the same split
/// [`holder_concentration_factors`] and [`owner_powers_factors`] already
/// use.
///
/// Each of the five reads one `Kind` [`push_window_buyer_factors`] or
/// [`push_band`] may or may not have pushed; a `Kind` this sheet never got
/// (rule 8: absent is not zero) simply finds nothing here and adds no
/// factor, the same "gap shows in coverage, never a zero" discipline the
/// other `factors` sub-functions already follow.
///
/// **[`Signal::LaunchBlockInStrongestBand`] fires only on Solana today**
/// (`FactSheet::build`'s `dossier.launch` arm), while the four
/// buyer-derived `Kind`s these factors mostly read come only from
/// `dossier.funding`/`dossier.powers`/`ChainLaunch::supply`, which are
/// Robinhood-only in practice today. The two never co-occur on a real
/// dossier yet -- the same situation [`holder_concentration_factors`]'s own
/// doc comment already describes for `Signal::HolderConcentration` -- so
/// this is exercised by a sheet built directly in a test, not by a live
/// build, until one side crosses over to the other chain.
fn launch_block_band_factors(sheet: &FactSheet, factors: &mut Vec<Factor>) {
    if let Some(bps_f64) = fact_value(sheet, Kind::WindowBuyersLinkedHoldingsBps) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "WindowBuyersLinkedHoldingsBps is pushed from a u16, far inside i64's \
                      range"
        )]
        let bps = bps_f64.round() as i64;
        if let 1_000..=i64::MAX = bps {
            factors.push(Factor {
                signal: Signal::LaunchBlockInStrongestBand,
                name: "same-window buyers hold >= 1,000 bps together".to_owned(),
                delta_bps: 1_000,
                grade: Grade::Measured,
                evidence: format!(
                    "checked same-window buyers together hold {:.2}% of supply, raw (unweighted \
                     by link confidence)",
                    bps_f64 / 100.0
                ),
            });
        }
    }

    if let Some(fresh_f64) = fact_value(sheet, Kind::FreshWindowBuyers) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "FreshWindowBuyers is a count of at most MAX_CANDIDATES (4), far inside \
                      i64's range"
        )]
        let fresh = fresh_f64.round() as i64;
        if let 3..=i64::MAX = fresh {
            factors.push(Factor {
                signal: Signal::LaunchBlockInStrongestBand,
                name: ">= 3 checked same-window buyers are fresh".to_owned(),
                delta_bps: 800,
                grade: Grade::Measured,
                evidence: format!("{fresh} of the checked same-window buyers are fresh wallets"),
            });
        }
    }

    // `>= 0.5`, not `== 1.0`: the fact is only ever pushed as exactly 0.0 or
    // 1.0, but clippy's `float_cmp` forbids strict equality on floats.
    if let Some(within) = fact_value(sheet, Kind::WindowBuySizesWithinTenPercent)
        && within >= 0.5
    {
        factors.push(Factor {
            signal: Signal::LaunchBlockInStrongestBand,
            name: "checked same-window buy sizes within 10% of each other".to_owned(),
            delta_bps: 500,
            grade: Grade::Inferred,
            evidence: "the checked same-window buyers' spends are all within 10% of each other"
                .to_owned(),
        });
    }

    // `>= 0.5`, not `== 1.0`: same clippy note as above.
    if let Some(declared) = fact_value(sheet, Kind::AllWindowBuyersDeclaredExempt)
        && declared >= 0.5
    {
        factors.push(Factor {
            signal: Signal::LaunchBlockInStrongestBand,
            name: "every launch-window buyer is declared-exempt".to_owned(),
            delta_bps: -500,
            grade: Grade::Measured,
            evidence: "every buyer in the launch window is on this launch's declared \
                       snipe-tax exemption list"
                .to_owned(),
        });
    }

    if let Some(launches_f64) = fact_value(sheet, Kind::BandLaunches) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "BandLaunches is pushed from a u64 count of measured launches, far inside \
                      i64's range for any sample this project will ever measure"
        )]
        let launches = launches_f64.round() as i64;
        if let i64::MIN..=199 = launches {
            factors.push(Factor {
                signal: Signal::LaunchBlockInStrongestBand,
                name: "band measured on < 200 launches, thin sample".to_owned(),
                delta_bps: -300,
                grade: Grade::Inferred,
                evidence: format!(
                    "this recipient band has only been measured on {launches} launches"
                ),
            });
        }
    }
}

/// S13's raise/lower factors (research 0052 §3.1's row, ADR 0035), split out
/// of [`factors`] itself so that function stays under clippy's line count --
/// the same split [`holder_concentration_factors`] and
/// [`correlated_selling_factors`] already use.
///
/// Three independent inputs, each only present when [`push_powers`] pushed
/// it -- a sub-read that failed pushes nothing (rule 8), so this simply does
/// not find the `Kind` and skips that one factor, the same "gap shows in
/// coverage, never a zero" discipline [`factors`]'s own doc comment states.
fn owner_powers_factors(sheet: &FactSheet, factors: &mut Vec<Factor>) {
    if let Some(tax_f64) = fact_value(sheet, Kind::CreatorTaxBps) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "CreatorTaxBps is pushed from a u16 (0..=1,000 on Pons v2), far inside \
                      i64's range"
        )]
        let tax = tax_f64.round() as i64;
        if tax >= 500 {
            factors.push(Factor {
                signal: Signal::OwnerCanStillMintOrPause,
                name: "creator tax >= 500 bps".to_owned(),
                delta_bps: 700,
                grade: Grade::Measured,
                evidence: format!("the creator tax on this launch is {tax} bps"),
            });
        } else if tax == 0 {
            factors.push(Factor {
                signal: Signal::OwnerCanStillMintOrPause,
                name: "creator tax == 0".to_owned(),
                delta_bps: -300,
                grade: Grade::Measured,
                evidence: "the creator tax on this launch is 0 bps".to_owned(),
            });
        }
    }

    if let Some(pending) = fact_value(sheet, Kind::PendingCreatorFeeRecipientSet)
        && pending != 0.0
    {
        factors.push(Factor {
            signal: Signal::OwnerCanStillMintOrPause,
            name: "pending creator fee recipient is non-zero".to_owned(),
            delta_bps: 500,
            grade: Grade::Measured,
            evidence: "the factory's pendingCreatorFeeRecipient timelock names a non-zero \
                       address"
                .to_owned(),
        });
    }

    if let Some(count_f64) = fact_value(sheet, Kind::UndeclaredExemptions) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "UndeclaredExemptions is a count of confirmed exemptions, never near \
                      i64's range"
        )]
        let count = count_f64.round() as i64;
        if count >= 1 {
            // `+600` per address, capped at `+1,200` (research 0052 §3.1's
            // S13 row) -- two addresses already reach the cap, so a third
            // and beyond add nothing further.
            let delta = i32::try_from((count * 600).min(1_200)).unwrap_or(1_200);
            factors.push(Factor {
                signal: Signal::OwnerCanStillMintOrPause,
                name: "exempt address(es) off both lists".to_owned(),
                delta_bps: delta,
                grade: Grade::Measured,
                evidence: format!(
                    "{count} exempt address(es) on this launch are on neither the first-party \
                     list nor the launch's own declared list"
                ),
            });
        }
    }
}

/// The two wired S5 raise factors (research 0052 §3.1), split out of
/// [`factors`] itself so that function stays under clippy's line count.
///
/// Only the two raises keyed on `Kind::LargestHolderShare` are wired: the
/// table's top-10 raise and its `Proof`-gated lower both need facts
/// (`largest_non_infrastructure`/a role `Proof`) that are never projected
/// onto the sheet today, so they stay out rather than being approximated.
fn holder_concentration_factors(sheet: &FactSheet, factors: &mut Vec<Factor>) {
    let Some(ratio) = fact_value(sheet, crate::clause::Kind::LargestHolderShare) else {
        return;
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "LargestHolderShare's first value is a ratio in [0, 1] built by \
                  Fact::share from an integer bps count, so *10_000 rounds back to that \
                  same small integer"
    )]
    let bps = (ratio * 10_000.0).round() as i64;
    // A `match` on ranges, not a chain of `>=`/`<` comparisons: see the S1
    // block above for why.
    match bps {
        2_000..=i64::MAX => factors.push(Factor {
            signal: Signal::HolderConcentration,
            name: "largest holder >= 2,000 bps".to_owned(),
            delta_bps: 1_000,
            grade: Grade::Measured,
            evidence: format!(
                "the largest single address holds {:.2}% of the supply outside the curve",
                ratio * 100.0
            ),
        }),
        1_000..=1_999 => factors.push(Factor {
            signal: Signal::HolderConcentration,
            name: "largest holder >= 1,000 bps".to_owned(),
            delta_bps: 600,
            grade: Grade::Measured,
            evidence: format!(
                "the largest single address holds {:.2}% of the supply outside the curve",
                ratio * 100.0
            ),
        }),
        _ => {}
    }
}

/// The three S7 raise/lower factors (research 0052 §3.1), split out of
/// [`factors`] itself so that function stays under clippy's line count.
///
/// Range `match`/`if let`, same discipline as the S1 block above: a boundary
/// that moves has to move an arm, not a comparison a mutant can flip without
/// a test noticing.
fn correlated_selling_factors(sheet: &FactSheet, factors: &mut Vec<Factor>) {
    if let Some(wallets) = fact_value(sheet, crate::clause::Kind::CorrelatedSellWallets) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "CorrelatedSellWallets is a cluster size, pushed from a u32 that never \
                      approaches i64's range"
        )]
        let wallets = wallets.round() as i64;
        if let 3..=i64::MAX = wallets {
            factors.push(Factor {
                signal: Signal::CorrelatedSelling,
                name: "linked sellers >= 3 within the window".to_owned(),
                delta_bps: 800,
                grade: Grade::Measured,
                evidence: format!(
                    "{wallets} linked-at-buy wallets sold within the same 50-block window"
                ),
            });
        }
    }

    if let Some(bps_f64) = fact_value(sheet, crate::clause::Kind::CorrelatedSellVolumeBps) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "CorrelatedSellVolumeBps is pushed from a u16, far inside i64's range"
        )]
        let bps = bps_f64.round() as i64;
        if let 1_000..=i64::MAX = bps {
            factors.push(Factor {
                signal: Signal::CorrelatedSelling,
                name: "sold volume >= 1,000 bps of supply".to_owned(),
                delta_bps: 600,
                grade: Grade::Measured,
                evidence: format!("the cluster sold {:.2}% of total supply", bps_f64 / 100.0),
            });
        }
    }

    if let Some(seconds_f64) = fact_value(sheet, crate::clause::Kind::CorrelatedSellSpreadSeconds) {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "CorrelatedSellSpreadSeconds is pushed from a u64 spread over at most a \
                      launch's own lifetime, far inside i64's range"
        )]
        let seconds = seconds_f64.round() as i64;
        if let 3_601..=i64::MAX = seconds {
            factors.push(Factor {
                signal: Signal::CorrelatedSelling,
                name: "sells spread over > 1 hour".to_owned(),
                delta_bps: -300,
                grade: Grade::Measured,
                evidence: format!("the cluster's sells spread over {seconds} seconds"),
            });
        }
    }
}

/// Everything the analyst may assert about one token.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FactSheet {
    /// The mint, as text. Not a number, and never checked as one.
    pub mint: String,
    /// The point every figure was read at, in the reading chain's own unit.
    ///
    /// **`Option<ReadAt>`, not `Option<Slot>`.** Packet 0032 left this
    /// Solana-only because retyping it also required editing
    /// `forbidden.rs` (its `required_sheet` built a `FactSheet` by full
    /// struct literal and `check_required_age` destructured this field as a
    /// bare [`Slot`]) and that file was out of scope there. This packet owns
    /// all three files that the change touches, so the field carries
    /// [`ReadAt`] now: a Robinhood sheet's block number reaches
    /// [`FactSheet::authorised`], [`crate::verdict::template`] and
    /// `check_required_age` exactly as a Solana sheet's slot always did.
    pub read_at: Option<ReadAt>,
    /// The facts, in the order they are shown to the model.
    pub facts: Vec<Fact>,
    /// Creator-supplied strings, kept apart from the facts.
    ///
    /// **Never inlined into the fact list**, because the fact list is what the
    /// model is told is true. These are fenced separately as untrusted, and a
    /// number appearing inside one of them authorises nothing.
    pub untrusted: Vec<(String, String)>,
    /// What could not be read, so the reply can say so plainly.
    ///
    /// "Radar has no record" is a thing the analyst is expected to say, and it
    /// can only say it if the absence survives to here rather than becoming a
    /// default somewhere below.
    pub unknown: Vec<String>,
    /// The refusal signals the facts carry, in a fixed order. See [`Signal`].
    pub signals: Vec<Signal>,
    /// The innocent twin of each fired signal, one entry per entry in
    /// [`Self::signals`], in the same order. See [`twin_for`].
    ///
    /// **Never a signal's name.** Each string is a sentence about what was
    /// read, phrased so the account could say it in public -- the model must
    /// not learn `Signal` exists, only that another explanation does.
    pub twins: Vec<String>,
    /// Optional facts that could not be read, kept out of [`Self::unknown`]
    /// so they never force `CantTell` (`verdict::level`'s own rule; the
    /// exclusion list this mirrors lives at the `continue` in
    /// [`Self::build`]).
    ///
    /// **A separate list, not folded into `unknown`.** `unknown` is read as
    /// "a required fact is unread"; a `capacity`/`fees`/`creator
    /// transactions`/`market`/`token ownership` miss is not required, so
    /// mixing the two would either soften `unknown`'s meaning or silently
    /// re-introduce the severity these misses were excluded from. This is
    /// the accessor `assessment.rs`'s coverage figure reads instead of
    /// reaching into `Dossier::unavailable` (which does not survive past
    /// [`Self::build`]).
    pub skipped: Vec<String>,
}

impl FactSheet {
    /// Builds the sheet from a dossier and the published base rates.
    ///
    /// `rates` is `None` when the snapshot could not be loaded. That is not a
    /// reason to fall back on remembered numbers: without it the sheet simply
    /// carries no population context, and the reply says less. Rule 8 — a
    /// missing input is a refusal to claim, not a default.
    ///
    /// `self_mint` is the analyst's own token, from `REALORRUG_SELF_MINT`, or
    /// `None` when no token is special. ADR 0033 (superseding ADR 0013
    /// constraint 5): the analyst's own token is now judged on the same rule
    /// as any other coin, price included, per ADR 0013 constraint 6. This
    /// parameter is no longer read for that decision -- it stays on the
    /// signature rather than being torn out of every call site in this
    /// packet, in case a later one (the hint log, ADR 0033 §4) needs to know
    /// which mint is the analyst's own.
    ///
    /// `first_party` is the named-address list `Signal::RepeatLauncher` must
    /// exclude before it measures a floor (`creator.rs::repeat_launcher_floor`,
    /// packet 0037). `None` the same way `rates` and `creators` are `None`: not
    /// configured, or the file did not parse. Passing `None` here does not
    /// mean "fire `RepeatLauncher` without the exclusions" — it means the
    /// signal does not fire at all, on any chain (AGENTS.md rule 7); a
    /// prevalence measured over a population that still contains the launch
    /// factory would look like a result while being wrong.
    #[must_use]
    #[expect(
        clippy::too_many_lines,
        reason = "the Robinhood chain-gate (no base-rate cost line, no Solana population line, \
                  optional misses skipped) is a few short, well-commented branches added to an \
                  already-long assembly function; splitting it out for a line count would move \
                  the comments away from the code they explain for no behaviour change"
    )]
    #[expect(
        unused_variables,
        reason = "ADR 0033 stops self_mint driving price withholding; kept on the signature per \
                  the doc comment above rather than removed from every call site in this packet"
    )]
    pub fn build(
        dossier: &Dossier,
        rates: Option<&BaseRates>,
        creators: Option<&crate::creator::CreatorIndex>,
        self_mint: Option<&realorrug_types::Address>,
        first_party: Option<&crate::firstparty::FirstPartyList>,
    ) -> Self {
        let mut facts = Vec::new();
        let mut untrusted = Vec::new();
        let mut unknown = Vec::new();
        let mut signals = Vec::new();
        let mut skipped = Vec::new();

        // An index describes one chain's launches, and says which
        // (`creator::CreatorIndex::chain`). Dropping a wrong-chain one here,
        // once, is what keeps every reader below honest — the lookup, the two
        // signals and the population line.
        //
        // **Dropping it is not the same as ignoring it.** A wrong-chain index
        // that reached `push_creator` would miss on every address and publish
        // "this creator has no record here", which reads as a checked absence.
        // `None` publishes nothing, which is what an unchecked question is.
        let chain = crate::firstparty::Chain::of(&dossier.mint);
        let creators = creators.filter(|index| index.chain == chain);
        // A base-rate snapshot describes one chain's launches, the same as a
        // creator index (`CreatorIndex::chain`, filtered just above). Until
        // 2026-09-17 `rates` carried no chain of its own, so `push_population`
        // and `Signal::LaunchBlockInStrongestBand` ran for any chain whenever
        // *any* snapshot was loaded -- on a Robinhood Chain sheet that printed
        // Solana/pump.fun's recipient distribution as though it were measured
        // here. Filtering here, once, is what keeps both readers below honest.
        let rates = rates.filter(|r| r.chain == chain);

        if let Some(launch) = &dossier.launch {
            push_launch(&mut facts, &mut untrusted, launch);
            if let Some(rates) = rates {
                push_population(&mut facts, launch.recipients, rates);
                // In the strongest band or above it. An exact count only: a
                // truncated one was decided by Radar's call budget, and a
                // signal read off it would be a signal about the budget.
                if let (Some(exact), Some(strongest)) =
                    (launch.recipients.exact(), rates.strongest_band())
                    && exact >= strongest.lo
                {
                    signals.push(Signal::LaunchBlockInStrongestBand);
                }
            }
            // A buy that was seen. `None` is "could not see", and the sheet
            // already refuses to call that "did not buy".
            if launch.dev_buy_lamports.is_some_and(|l| l > 0) {
                signals.push(Signal::CreatorBoughtOwnLaunch);
            }
            // **The age, which is not the read point.** "Read at slot
            // 444007820" says when the camera clicked; it does not say the
            // token is six hours old. A real age exists only where both ends
            // of the subtraction are on the same clock: `launch.slot` is
            // always a Solana slot (pump.fun only), so this only fires for a
            // `ReadAt::Solana` read, never for a Robinhood block number --
            // there is no lossy slot-from-block conversion to invent one
            // with (`ReadAt::as_slot`'s own doc comment makes the same
            // refusal for the read point). A Robinhood launch arrives as
            // `chain_launch` below instead, with its age already taken from
            // the two blocks' own timestamps.
            // Only a read strictly after the launch gives an age. The read
            // slot equals the launch slot when the curve could not be read
            // (the reader falls back to the launch's own slot), and a launch
            // slot past the read slot means two nodes at different heights.
            // Either way the age is unknown and no fact is pushed; the
            // template then says it could not be read. Both used to publish
            // "0 slots (about 0 hours)" as a measurement (research 0056).
            if let Some(ReadAt::Solana(read_slot)) = dossier.read_at
                && read_slot > launch.slot
            {
                push_age(&mut facts, read_slot.saturating_since(launch.slot));
            }
        } else if let Some(launch) = &dossier.chain_launch {
            push_chain_launch(&mut facts, &mut signals, &mut untrusted, launch);
        } else {
            unknown.push("the launch block could not be read".to_owned());
        }

        // Only a reader that counts holders fills this; one that tried and
        // failed names "holders" in `unavailable`, which reaches `unknown`
        // below. Solana's reader never counts them, and its sheet is not
        // demoted for a read it never attempts.
        if let Some(holders) = &dossier.holders {
            push_holders(&mut facts, holders);
        }

        // Slice 3 (design 0027): who funded the first buyers. Only the
        // Robinhood reader fills this; a read that stopped short names its
        // gap in `unknown` beside the facts it did get.
        if let Some(funding) = &dossier.funding {
            push_funding(&mut facts, &mut unknown, funding);
            // Research 0052 §3.1's S2 row's four buyer-derived factors --
            // split out because they read `dossier.powers` and
            // `dossier.chain_launch`'s supply too, neither of which
            // `push_funding` itself touches.
            let supply = dossier.chain_launch.as_ref().and_then(|l| l.supply);
            push_window_buyer_factors(
                &mut facts,
                &mut skipped,
                funding,
                supply,
                dossier.powers.as_ref(),
            );
        }

        // **The fact that makes one reply differ from another.** The launch
        // block is about the block; three coins launched in the same minute
        // produce the same sentences from it, because the cost line is a
        // constant and most launches sit in the same recipient band. What this
        // creator did before is the part that is about *this* coin, and it is
        // the thing Radar has that nobody else does.
        //
        // **Outside the launch-block arm, and that is the whole point.** It sat
        // inside until 2026-09-06, so a coin whose launch block is past the
        // signature-page budget got no creator history at all -- and that is
        // every coin with real history, which is every coin somebody bothers to
        // ask about. The curve account carries the creator regardless of age,
        // so the launch block is preferred and the curve is the fallback.
        let creator = dossier
            .launch
            .as_ref()
            .map(|l| realorrug_types::ChainAddress::Solana(l.creator))
            .or_else(|| dossier.curve.as_ref().map(|c| c.creator));
        if let (Some(index), Some(address)) = (creators, creator) {
            let creator = address.to_string();
            push_creator(&mut facts, &mut unknown, &creator, index, chain);
            // Measured and none organic. A creator whose launches have not been
            // measured has no record to hold against them.
            if let Some(record) = index.get(&creator)
                && record.measured > 0
                && record.organic == 0
            {
                signals.push(Signal::CreatorNeverGraduatedOrganically);
            }

            // RepeatLauncher (design 0020 §3; research 0042's port-order item
            // 1). The floor is this index's own 95th percentile, measured
            // *after* the named list excludes the launch factory and its
            // escrow -- never Radar's Solana `REPEAT_FLOOR`/
            // `INFRASTRUCTURE_FLOOR` constants, which are a different
            // population (distinct launch *blocks* in a *90-minute window*),
            // a different window, and a different chain
            // (`creator::CreatorIndex::repeat_launcher_floor`'s own doc
            // comment carries the rest of that argument).
            //
            // No list, or no floor (fewer than 100 creators survive the
            // exclusion), means no signal — AGENTS.md rule 7, deny by
            // default, not "fire without the exclusion."
            //
            // **The innocent twin, per design 0020 §3: a bot that buys every
            // launch; infrastructure, not coordination.** The named list does
            // not dispose of this twin — it holds *named* addresses, and an
            // unnamed relayer nobody has captured yet reads identically, on
            // this signal alone, to a person launching forty tokens. That is
            // this signal's honest limit; putting the twin into a reply is a
            // later packet's job (design 0020 §3's own note), not this one's.
            if let Some(list) = first_party
                && let Some(record) = index.get(&creator)
                && let Some(floor) = index.repeat_launcher_floor(list)
                && record.launches >= floor
            {
                signals.push(Signal::RepeatLauncher);
            }
        }

        if let Some(curve) = &dossier.curve {
            push_curve(&mut facts, &mut unknown, curve, dossier.read_at);
        } else {
            unknown.push("the bonding curve could not be read".to_owned());
        }

        // Design 0027 §2.2's "Market and exit" row, wired up per the slice 4
        // note: `dossier.market` is `None` both when no read was configured
        // and when one was attempted and failed (the failure already named
        // `"market"` on `dossier.unavailable` by `dispatch::robinhood`, and
        // skipped from `unknown` above the same way `capacity`/`fees` are) --
        // there is nothing further to do here in either case, which is the
        // point: a missing market read is absent, never a price of zero.
        if let Some(market) = &dossier.market {
            push_market(&mut facts, market);
        }

        // Design 0027 row 6/7 slice 6a's "not done" note, closed here:
        // `dossier.token_ownership` is `None` both when no read was
        // configured (every chain but Solana today) and when one was
        // attempted and failed (named "token ownership" on
        // `dossier.unavailable`, skipped from `unknown` below the same way
        // `market` is) -- nothing further to do here in either case.
        if let Some(ownership) = &dossier.token_ownership {
            push_token_ownership(&mut facts, ownership);
        }

        // Design 0027 slice 5's creator cash-flow facts. `dossier.creator_cash_flow`
        // is `None` both when no read was configured (Solana today; see
        // `dossier::empty`'s "Solana not built" gap) and when the read ran but
        // could not complete -- `push_creator_cash_flow` itself reads
        // `CreatorCashFlow::trades_complete` through the type's own accessors
        // and publishes nothing when it is false, so there is nothing further
        // to gate here.
        if let Some(cash_flow) = &dossier.creator_cash_flow {
            push_creator_cash_flow(&mut facts, cash_flow);
        }

        // S13 "owner powers live" (research 0052 §3.1, ADR 0035).
        // `dossier.powers` is `None` when the launch transaction itself
        // could not be read (`robinhood.rs` names "powers" on
        // `dossier.unavailable` in that case) -- nothing further to do here;
        // that miss is not on the skip list below, so it still counts as a
        // coverage gap the ordinary way. Once `Some`, `push_powers` reads
        // `dossier.unavailable` itself to tell a genuine measured zero/none
        // apart from a sub-read that failed.
        if let Some(powers) = &dossier.powers {
            push_powers(&mut facts, &mut signals, powers, &dossier.unavailable);
        }

        if let Some(count) = dossier.creator_transactions {
            let rendered = format!("{count}");
            facts.push(
                Fact::exact(
                    Kind::CreatorTransactions,
                    "transactions by this creator's address (transactions, not launches)",
                    f64::from(count.lower_bound()),
                    rendered.clone(),
                )
                .saying(
                    Voice::Plain,
                    format!("Real or Rug has seen {rendered} transactions from this creator's address -- transactions, not launches."),
                )
                .saying(
                    Voice::Blunt,
                    format!("That address has {rendered} transactions on it. Transactions, not launches."),
                ),
            );
        }

        // Not inside the launch-block arm above, and deliberately: this is a
        // fact about the venue, not about the coin, so a mint whose launch block
        // could not be read still gets it. It is also what gives the creator's
        // counts a scale -- "none of 150 filled its curve" reads differently
        // once you know what share of everything does.
        //
        // **Gated on the index's chain, which the filter at the top of this
        // function has already applied.** Until 2026-09-17 this asked whether
        // the *token* was on Robinhood and refused the figures if it was, on
        // the reasoning that the only index in existence was a pump.fun one.
        // That reasoning was right about the fact and wrong about the test: it
        // suppressed the population line for the one chain a Pons v2 index
        // describes, and would have gone on suppressing it after that index
        // was built, while still printing pump.fun's totals for pump.fun
        // whatever file happened to be at the path.
        if let Some(population) = creators.and_then(|c| c.population) {
            push_measured_population(&mut facts, &population);
        }

        // **Gated on the snapshot's own chain, filtered at the top of this
        // function, and again on whether that chain has a round-trip cost
        // measurement at all.** Research 0024 measured Solana/pump.fun fresh
        // launches (`push_cost`'s "850 bps" line, "round trip Real or Rug's
        // kernel assumes"); Robinhood Chain has no such measurement yet, so
        // `BaseRates::round_trip` is `None` for it -- absent, not a
        // conservative estimate borrowed from a different chain.
        if let Some(rates) = rates
            && let Some(round_trip) = &rates.round_trip
        {
            push_cost(&mut facts, round_trip);
        }

        for miss in &dossier.unavailable {
            // **Optional facts never become an "unknown" line.** `phrase_for`
            // turns every entry here into a sentence the model may cite as a
            // reason it cannot say more, and `verdict::level` reads a nonempty
            // `unknown` as "a required fact is unread" (that function's own
            // doc comment). `capacity`, `fees` and `creator transactions` are
            // optional per design 0020 §1 -- today only Robinhood's reader
            // records them as `Unavailable` (Solana either reads them or
            // reports the whole curve/creator arm missing under a different
            // name), so skipping these three names here is the one place that
            // keeps rule 8 ("absent is not zero") without also inventing a
            // second "which facts are required" list to keep in sync with
            // `verdict::level`. The raw reason still lives on
            // `Dossier::unavailable` for the operator; only the sheet's public
            // rendering treats the miss as unremarkable.
            // `market` joins this list for the same reason `capacity` and
            // `fees` do: an off-chain aggregator that is slow or down is not
            // a required fact this analyst's verdict depends on
            // (`verdict::level`'s own doc comment), so a failed market read
            // must not degrade the level the way a missing launch block or
            // curve does. The raw reason still lands on `Dossier::unavailable`
            // named `"market"` -- see `dispatch::robinhood` -- for the
            // operator; only the public verdict severity is unaffected.
            // `token ownership` (design 0027 row 6/7 slice 6a) joins the same
            // list: Solana's dossier never sets `holders` today (see the
            // comment above `push_holders`), so no verdict currently depends
            // on a holder-concentration read for that chain, and a failed
            // sample of the largest accounts must not be the read that
            // starts requiring one.
            // `quote asset` (S1, "name the pair") joins the same list: a
            // failed `symbol()`/`decimals()` read on a Pons v2 pair token is
            // an off-chain-shaped miss the same way `market` is -- the curve
            // itself still read fine, only its unit's name did not, and a
            // launcher whose pair token answers slowly must not be scored
            // worse than one whose pair reads cleanly.
            // `correlated selling` (S7, research 0052 §3) joins it too: the
            // signal can only raise the risk score, so an unread S7 is a gap
            // in coverage, never a reason to fall to `CantTell` -- a token
            // must not score worse because its trade logs were slow to read.
            // `pending creator fee recipient`, `declared snipe-tax
            // exemptions`, `snipe tax exemption` and `snipe tax exemption
            // classification` (S13, research 0052 §3.1, ADR 0035) join for
            // the same reason as `correlated selling`: each sub-read can
            // only ever raise `OwnerCanStillMintOrPause`'s weight, so a
            // failed one is a coverage gap `push_powers` already leaves off
            // the fact sheet entirely (rule 8), never a reason for
            // `verdict::level` to fall to `CantTell`.
            if matches!(
                miss.fact,
                "capacity"
                    | "fees"
                    | "creator transactions"
                    | "market"
                    | "token ownership"
                    | "quote asset"
                    | "correlated selling"
                    | "pending creator fee recipient"
                    | "declared snipe-tax exemptions"
                    | "snipe tax exemption"
                    | "snipe tax exemption classification"
            ) {
                // Recorded here, not dropped: `assessment.rs`'s coverage
                // figure needs to know this gap exists even though
                // `verdict::level` must not. `miss.fact` (not `miss.why`) --
                // same injection reasoning as the `unknown` push below, and
                // this list is never rendered to a reader either.
                skipped.push(miss.fact.to_owned());
                continue;
            }
            // **Radar's own phrase, never the raw reason.** `miss.why` is
            // diagnostic text -- "rpc transport: http status: 429", "no account
            // at <mint>" -- and two things are wrong with publishing it.
            //
            // It is an injection surface: a reason that echoes the mint would
            // put the attacker's own base58 into the trusted block, and
            // `authorised` reads numerals out of that block, so a mint chosen to
            // contain "68" would licence 68 as a publishable figure.
            //
            // And it is bad copy. A reader asking about a coin is owed "the
            // launch block could not be read", not an HTTP status. The raw
            // reason stays on the `Dossier` for the operator, where it belongs.
            unknown.push(phrase_for(miss.fact));
        }

        // **Said once, not twice.** Every unreadable fact reaches this list by
        // two routes: the field is `None`, and `Dossier::build` also recorded a
        // reason for it in `unavailable`. Both fire for the same failure, so a
        // reply about a token whose launch block could not be read told the
        // reader so twice, and a token where nothing could be read said four
        // lines that were two.
        //
        // Found by running the thing against a real mint, which is the only
        // place it shows: every fixture in this crate's tests supplies one route
        // or the other, never both, so the duplication was invisible to all of
        // them.
        //
        // Deduplicated rather than removing one route. Keeping both is what
        // guarantees an absent fact is always reported -- if `build` ever stops
        // recording a reason, the `None` branch still speaks, and rule 9 says an
        // absence must never pass silently. Order is preserved because it is the
        // order the reader meets the facts in.
        let mut seen = std::collections::BTreeSet::new();
        unknown.retain(|miss| seen.insert(miss.clone()));

        // ADR 0033: no filter runs here any more. The analyst's own token is
        // judged, and priced, on the same rule as any other coin (ADR 0013
        // constraint 6, now literal) -- see the `self_mint` doc comment above.

        // One string per fired signal, in the same order the signal fired --
        // computed from `signals` itself so the two can never drift apart,
        // rather than pushed alongside each `signals.push` call above.
        let twins = signals
            .iter()
            .map(|&signal| twin_for(signal).to_owned())
            .collect();

        Self {
            mint: dossier.mint.to_string(),
            read_at: dossier.read_at,
            facts,
            untrusted,
            unknown,
            signals,
            twins,
            skipped,
        }
    }

    /// Every numeric value a reply may contain, and what each one is about.
    ///
    /// Three sources, and the boundary between them is the point:
    ///
    /// 1. **Each fact's declared values**, which carry the honest re-renderings
    ///    a measurement has — 0.251, 25.1 and 25 are one fact said three ways.
    /// 2. **Every numeral in the fact's own line**, label included. A label
    ///    says things like "research 0022" and "$20-$200", and those numerals
    ///    were written *by Radar* and shown to the model as true. A model citing
    ///    the band it was given has invented nothing, and a check that caught it
    ///    would reject the most careful replies while passing vaguer ones.
    /// 3. **The sheet's own remarks** — what could not be read, the innocent
    ///    explanations, and the read point. A reply citing when the sheet was
    ///    read is doing the thing this account exists to do: a slot on Solana,
    ///    a block number on Robinhood Chain, never the other chain's word for
    ///    it.
    ///
    /// **Sources 1 and 2 are attributed and source 3 is not**, and that is the
    /// fix ADR 0031 records. This function used to scan the whole rendering in
    /// one pass and return bare numbers, which meant any numeral anywhere on
    /// the sheet licensed that numeral anywhere in the reply: the largest
    /// holder's 41% could be published as "the creator already dumped 41%",
    /// a false sentence made entirely of authorised digits. Scanning per fact
    /// keeps each number tied to the measurement it came from. Source 3 is
    /// [`Subject::Anywhere`] because those lines are Radar's remarks *about*
    /// the sheet rather than measurements of anything on it.
    ///
    /// What is **not** a source is [`FactSheet::untrusted`]. That is the whole
    /// boundary: a creator who names their token "99.9% of holders profited"
    /// must not thereby licence 99.9 as a publishable figure. The untrusted
    /// strings are fenced separately and never rendered into this block.
    #[must_use]
    pub fn authorised(&self) -> Vec<Authorised> {
        let mut values: Vec<Authorised> = Vec::new();
        for fact in &self.facts {
            let subject = Subject::of(fact.kind);
            values.extend(
                fact.values
                    .iter()
                    .map(|&value| Authorised { subject, value }),
            );
            // Exactly the line `render` writes for this fact, so the two
            // cannot disagree about what the model was shown.
            let line = format!("{}: {}", fact.label, fact.rendered);
            values.extend(
                crate::fidelity::literals(&line)
                    .into_iter()
                    .map(|(_, value)| Authorised { subject, value }),
            );
        }
        for remark in self.unknown.iter().chain(self.twins.iter()) {
            values.extend(
                crate::fidelity::literals(remark)
                    .into_iter()
                    .map(|(_, value)| Authorised::anywhere(value)),
            );
        }
        if let Some(read_at) = self.read_at {
            let raw = match read_at {
                ReadAt::Solana(slot) => slot.get(),
                ReadAt::Robinhood(block) => block,
            };
            #[expect(
                clippy::cast_precision_loss,
                reason = "a slot or a block number is well inside f64's exact integer range and \
                          this is a comparison against a literal the model wrote, not arithmetic"
            )]
            values.push(Authorised::anywhere(raw as f64));
        }
        values
    }

    /// The sheet as the model sees it.
    ///
    /// Facts, what could not be read, and the read point. The mint and the
    /// untrusted strings are fenced separately by [`crate::voice`] so that
    /// nothing in this block is creator-controlled.
    ///
    /// **The read point is here because it is the only way it reaches the
    /// model at all.** `voice::write` hands the provider exactly
    /// the mint and this rendering, joined, and nothing else, so a number
    /// absent from this string is a number the model cannot write -- and
    /// from this string is a number the model cannot write -- and
    /// `forbidden::check_required_age` requires a `NothingUglyYet` reply on a
    /// sheet with no age (its launch block could not be read) to state the
    /// read point. Left out of this block,
    /// that rule is unsatisfiable by any real model and every such reply
    /// falls back to the template, which is the free-text voice going silent
    /// on a whole chain without anything saying so.
    ///
    /// Written through [`ReadAt`]'s own `Display` -- "slot 444007820",
    /// "block 100" -- the single spelling of either word, so the sheet and
    /// `verdict::template` cannot drift apart on which clock a number is in.
    /// [`FactSheet::authorised`] already permitted this number before it was
    /// rendered here; harvesting it twice is harmless, because `authorised`
    /// is a set of permitted values and not a count.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for fact in &self.facts {
            let _ = writeln!(out, "{}: {}", fact.label, fact.rendered);
        }
        for miss in &self.unknown {
            let _ = writeln!(out, "NOT KNOWN: {miss}");
        }
        // Its own heading, after the facts and after what could not be read,
        // so the model meets these as context rather than as more facts --
        // and printed only when a signal actually fired, so a clean sheet
        // gains no heading at all.
        if !self.twins.is_empty() {
            let _ = writeln!(out, "INNOCENT EXPLANATIONS -- not proof either way:");
            for twin in &self.twins {
                let _ = writeln!(out, "- {twin}");
            }
        }
        if let Some(read_at) = self.read_at {
            let _ = writeln!(out, "read at: {read_at}");
        }
        out
    }
}

/// Radar's own words for a fact it could not read.
///
/// A closed set, so nothing outside this file can put text into the trusted
/// block. An unrecognised fact name gets a generic phrase rather than its raw
/// reason -- the fallback has to be the safe one, because the case it covers is
/// a fact added later by someone who did not read this comment.
fn phrase_for(fact: &str) -> String {
    match fact {
        "launch block" => "the launch block could not be read",
        "holders" => "the holders could not be read",
        "funding" => "who funded the early buyers could not be read",
        "curve" => "the bonding curve could not be read",
        "creator history" => "the creator's history could not be read",
        // Solana records this on every read (the reader is not built there
        // yet), so the fallback put "part of this could not be read" in every
        // Solana reply (research 0056). "Not checked" is true on both chains.
        "creator cash flow" => "the creator's own buys and sells were not checked",
        _ => "part of this could not be read",
    }
    .to_owned()
}

fn push_launch(facts: &mut Vec<Fact>, untrusted: &mut Vec<(String, String)>, launch: &LaunchBlock) {
    let recipients = format!("{}", launch.recipients);
    facts.push(
        Fact::exact(
            Kind::LaunchRecipients,
            "distinct token accounts receiving the token in its own launch block \
             (token accounts, NOT owners, NOT people)",
            f64::from(launch.recipients.lower_bound()),
            recipients.clone(),
        )
        // Rule 3 of the old prompt, now unbreakable: the model asked for this
        // clause gets "token accounts" whether or not it remembered the rule,
        // because the noun is not its to choose.
        .saying(
            Voice::Plain,
            format!(
                "The launch block put it into {recipients} token accounts -- accounts, not people."
            ),
        )
        .saying(
            Voice::Blunt,
            format!("It reached {recipients} token accounts at birth. Accounts, not owners."),
        ),
    );
    let transactions = format!("{}", launch.transactions);
    facts.push(
        Fact::exact(
            Kind::LaunchTransactions,
            "transactions in the launch block",
            f64::from(launch.transactions.lower_bound()),
            transactions.clone(),
        )
        .saying(
            Voice::Plain,
            format!("The launch block carried {transactions} transactions."),
        )
        .saying(
            Voice::Blunt,
            format!("One block, {transactions} transactions."),
        ),
    );
    match launch.dev_buy_lamports {
        Some(l) => {
            // `LaunchBlock` is Solana-shaped only (`realorrug-onchain`'s doc
            // comment on the type), so this figure is always lamports -- SOL,
            // 9 decimals, is not an assumption here the way it was for the
            // curve's quote amount, it is simply what this field is. What
            // would stop this being safe: the day `LaunchBlock` (or whatever
            // reads a Robinhood launch block) grows a non-Solana variant, this
            // stops being "simply what the field is" and becomes exactly the
            // guess `quote_asset` above exists to avoid -- at that point this
            // arm needs its own `quote_asset`-shaped unit, not a second
            // hard-coded string.
            let sol = format!("{} SOL", render_quote(u128::from(l), 9));
            facts.push(
                Fact::exact(
                    Kind::DevBuy,
                    "SOL the creator spent buying their own token in the launch block",
                    quote_as_f64(u128::from(l), 9),
                    sol.clone(),
                )
                .saying(
                    Voice::Plain,
                    format!("The creator bought {sol} of their own token in the launch block."),
                )
                .saying(Voice::Blunt, format!("The creator's own bid: {sol}.")),
            );
        }
        // Rule 9, and this one is a statement about a person: "did not buy" and
        // "we could not see a buy" are different accusations. The clause says
        // the second, and the model cannot reach for the first, because the only
        // sentence on offer is this one.
        None => facts.push(
            Fact {
                about: About::Measurement,
                kind: Kind::DevBuyUnseen,
                label: "creator's own buy in the launch block".to_owned(),
                rendered: "not found -- absent, NOT zero. Do not say the creator bought nothing."
                    .to_owned(),
                values: Vec::new(),
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                "No buy by the creator was found in the launch block, which is not the same as none.",
            )
            .saying(
                Voice::Blunt,
                "Real or Rug found no creator buy. Found, not happened.",
            ),
        ),
    }
    untrusted.push(("token name".to_owned(), launch.metadata.name.clone()));
    untrusted.push(("token symbol".to_owned(), launch.metadata.symbol.clone()));
}

/// The age, as a fact of its own -- never a stand-in read off the read point.
///
/// Design 0020 §4: "`NothingUglyYet` must state the age -- 'six hours old,'
/// not just 'clean so far.'" Two numbers, both authorised, and the checkable
/// one is never dropped in favour of the felt one:
///
/// - the slot count itself, exact and reproducible from the two reads it was
///   subtracted from;
/// - an approximate wall clock, always hedged ("about"/"roughly") because
///   Solana's slot time drifts and Alpenglow changes the relationship again
///   ([`SlotDelta::approx_duration`]'s own 400ms-target doc comment) -- an
///   exact-sounding hour count nobody could reproduce would be a fabricated
///   fact under rule 1.
///
/// Solana only. A Robinhood age needs no slot-time estimate: its blocks carry
/// timestamps, and [`push_chain_launch`] states the difference of two of them.
fn push_age(facts: &mut Vec<Fact>, delta: SlotDelta) {
    let slots = delta.get();
    // Rounded to one decimal, the same precision `Fact::share` uses for a
    // percentage, so the rendered string and the authorised literal are the
    // same digits -- a model citing "7.1 hours" is citing exactly the value
    // this fact declared, not a re-rounding of it.
    let hours = (delta.approx_duration().as_secs_f64() / 3600.0 * 10.0).round() / 10.0;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a slot delta is well inside f64's exact integer range"
    )]
    let slots_value = slots as f64;
    let rendered = format!("{slots} slots (about {hours} hours) since its launch block");
    facts.push(Fact {
        about: About::Measurement,
        kind: Kind::Age,
        label: "how long ago this token's launch block was, on the chain's own clock".to_owned(),
        rendered: rendered.clone(),
        values: vec![slots_value, hours],
        clauses: vec![
            Clause::new(
                Voice::Plain,
                format!("It launched about {hours} hours ago -- {slots} slots, by the read point."),
            ),
            Clause::new(
                Voice::Blunt,
                format!("{slots} slots old. Roughly {hours} hours."),
            ),
        ],
    });
}

/// A Robinhood launch: its age, the launcher's own buy in the launch
/// transaction, and what the token calls itself.
///
/// **The name and symbol are untrusted, and the parameter that carries them
/// is the whole fix.** Until 2026-09-17 this function had no `untrusted`
/// parameter at all, so the Robinhood path could not push a name even once
/// the reader had one: the share card a link unfurls to on X drew the verdict
/// over a blank where the token's name belongs. The Solana path
/// ([`push_launch`]) pushed both strings from the day it was written, and the
/// two paths now differ only in where the strings were read from.
///
/// **The age is exact, and still rounded.** Both ends are block timestamps
/// on the same chain, so nothing here is an estimate from a block time; the
/// hour figure is rounded to one decimal so the rendered string and the
/// authorised literal are the same digits. Past two days the days figure is
/// the one a reader wants, and both are authorised. The block number itself
/// is deliberately not a value: the age check refuses a reply that cites a
/// block where an age belongs, and a block number is a block.
fn push_chain_launch(
    facts: &mut Vec<Fact>,
    signals: &mut Vec<Signal>,
    untrusted: &mut Vec<(String, String)>,
    launch: &ChainLaunch,
) {
    // Each pushed on its own, because one reading and the other not is a real
    // outcome of two separate calls and the card already draws a name without
    // a symbol. A pair that is only pushed when both read would throw away a
    // name that did read (AGENTS.md rule 8: absent is not zero).
    for (label, value) in [
        ("token name", launch.name.as_ref()),
        ("token symbol", launch.symbol.as_ref()),
    ] {
        if let Some(value) = value {
            untrusted.push((label.to_owned(), value.clone()));
        }
    }
    if let Some(seconds) = launch.age_seconds {
        #[expect(
            clippy::cast_precision_loss,
            reason = "an age in seconds is well inside f64's exact integer range"
        )]
        let seconds = seconds as f64;
        let hours = (seconds / 3600.0 * 10.0).round() / 10.0;
        let days = (seconds / 86_400.0 * 10.0).round() / 10.0;
        let (rendered, values, plain, blunt) = if hours >= 48.0 {
            (
                format!("about {days} days ago"),
                vec![days, hours],
                format!("It launched about {days} days ago, by the chain's own clock."),
                format!("{days} days old."),
            )
        } else {
            (
                format!("about {hours} hours ago"),
                vec![hours],
                format!("It launched about {hours} hours ago, by the chain's own clock."),
                format!("{hours} hours old."),
            )
        };
        facts.push(Fact {
            about: About::Measurement,
            kind: Kind::Age,
            label: "how long ago this token launched, from the timestamps of its launch block \
                    and the read block"
                .to_owned(),
            rendered,
            values,
            clauses: vec![
                Clause::new(Voice::Plain, plain),
                Clause::new(Voice::Blunt, blunt),
            ],
        });
    }

    match launch.dev_buy_wei {
        Some(wei) if wei > 0 => {
            let eth = format!("{} ETH", render_quote(wei, 18));
            facts.push(
                Fact::exact(
                    Kind::DevBuy,
                    "ETH the launcher spent buying their own token in the launch transaction",
                    quote_as_f64(wei, 18),
                    eth.clone(),
                )
                .saying(
                    Voice::Plain,
                    format!(
                        "The launcher bought {eth} of their own token in the launch transaction."
                    ),
                )
                .saying(Voice::Blunt, format!("The launcher's own bid: {eth}.")),
            );
            signals.push(Signal::CreatorBoughtOwnLaunch);
        }
        // Read, and there was none. A measured zero, so it may be said -- but
        // only about the launch transaction, which is all that was read.
        Some(_) => facts.push(
            Fact::exact(
                Kind::DevBuy,
                "ETH the launcher spent buying their own token in the launch transaction",
                0.0,
                "0 ETH",
            )
            .saying(
                Voice::Plain,
                "The launch transaction carried no buy by the launcher.",
            )
            .saying(Voice::Blunt, "No launcher bid in the launch transaction."),
        ),
        // Rule 9, as for Solana: "did not buy" and "could not see" are
        // different statements about a person.
        None => facts.push(
            Fact {
                about: About::Measurement,
                kind: Kind::DevBuyUnseen,
                label: "launcher's own buy in the launch transaction".to_owned(),
                rendered: "not read -- absent, NOT zero. Do not say the launcher bought nothing."
                    .to_owned(),
                values: Vec::new(),
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                "Whether the launcher bought in the launch transaction could not be read.",
            )
            .saying(Voice::Blunt, "The launcher's own bid: unread."),
        ),
    }

    push_dev_buy_share(facts, launch);
    push_correlated_selling(facts, signals, launch);
}

/// S7 "correlated selling" (research 0052 §3.1): the largest cluster of
/// linked-at-buy wallets that sold inside one window, its share of supply
/// and how long its sells spread over.
///
/// **Only pushes anything when `sells_read` is `true`.** Rule 8 (absent is
/// not zero): a read that never happened must never be published as "no
/// correlated selling", so a `false` reads as silence here, the same as an
/// unset `dev_buy_wei` does above. The signal itself fires on `>= 2` linked
/// sellers -- one seller has nothing to be correlated *with* -- and the
/// three raise/lower factors past that are [`factors`]'s job, not this
/// function's, the same split `push_dev_buy_share`/[`factors`] already use
/// for S1.
fn push_correlated_selling(facts: &mut Vec<Fact>, signals: &mut Vec<Signal>, launch: &ChainLaunch) {
    let Some(cs) = launch.correlated_selling.as_ref() else {
        return;
    };
    // Unread is unknown (rule 8), and one seller is not correlated with
    // anything: nothing is said below a linked pair, so a fact about "1
    // linked seller" never reaches a reply.
    match (cs.sells_read, cs.linked_sellers) {
        (true, 2..) => {}
        _ => return,
    }

    facts.push(
        Fact::exact(
            Kind::CorrelatedSellWallets,
            "wallets in the largest cluster of linked-at-buy sellers whose sells landed inside \
             one 50-block window",
            f64::from(cs.linked_sellers),
            cs.linked_sellers.to_string(),
        )
        .saying(
            Voice::Plain,
            format!(
                "{} wallets that look linked sold within a 50-block window of each other.",
                cs.linked_sellers
            ),
        )
        .saying(
            Voice::Blunt,
            format!("Linked sellers: {}.", cs.linked_sellers),
        ),
    );

    if let Some(bps) = cs.sold_bps_of_supply {
        let bps_f64 = f64::from(bps);
        let rendered = format!("{:.2}%", bps_f64 / 100.0);
        facts.push(
            Fact::exact(
                Kind::CorrelatedSellVolumeBps,
                "that cluster's tokens sold as a share of the token's total supply",
                bps_f64,
                rendered.clone(),
            )
            .saying(
                Voice::Plain,
                format!("Together they sold {rendered} of supply."),
            )
            .saying(Voice::Blunt, format!("Cluster sold: {rendered}.")),
        );
    }

    if let Some(seconds) = cs.spread_seconds {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a spread in seconds is well inside f64's exact integer range"
        )]
        let seconds_f64 = seconds as f64;
        facts.push(
            Fact::exact(
                Kind::CorrelatedSellSpreadSeconds,
                "seconds between that cluster's earliest and latest sell",
                seconds_f64,
                format!("{seconds} s"),
            )
            .saying(
                Voice::Plain,
                format!("Their sells spread over {seconds} seconds."),
            )
            .saying(Voice::Blunt, format!("Sell spread: {seconds} s.")),
        );
    }

    signals.push(Signal::CorrelatedSelling);
}

/// S13 "owner powers live" (research 0052 §3.1, ADR 0035): creator tax, the
/// pending creator-fee-recipient timelock, and undeclared snipe-tax
/// exemptions, projected from `dossier.powers` onto the fact sheet.
///
/// Called only when `dossier.powers` is `Some` ([`FactSheet::build`]'s job),
/// so `creator_tax_bps` -- free on the same `getLaunchedToken` call every
/// dossier already pays for -- is always pushed and the signal always
/// fires: Pons v2 gives every launch a creator-tax capability, a
/// fee-recipient timelock and an exemption mechanism, so the signal is
/// about what a specific launch does with those built-in powers, not
/// whether it has them at all (ADR 0035 decision 1).
///
/// The other two facts are pushed only when `unavailable` carries no entry
/// for the sub-read they depend on -- `Powers` has no separate
/// success/failure flag for either, so this is the only place that can
/// still tell "the read succeeded and found nothing" apart from "the read
/// never completed" (rule 8).
fn push_powers(
    facts: &mut Vec<Fact>,
    signals: &mut Vec<Signal>,
    powers: &Powers,
    unavailable: &[Unavailable],
) {
    let missing = |name: &str| unavailable.iter().any(|miss| miss.fact == name);

    let tax_bps = f64::from(powers.creator_tax_bps);
    facts.push(
        Fact::exact(
            Kind::CreatorTaxBps,
            "the creator's cut of every trade on this launch, in basis points",
            tax_bps,
            format!("{} bps", powers.creator_tax_bps),
        )
        .saying(
            Voice::Plain,
            format!(
                "The creator takes {} bps of every trade on this launch.",
                powers.creator_tax_bps
            ),
        )
        .saying(
            Voice::Blunt,
            format!("Creator tax: {} bps.", powers.creator_tax_bps),
        ),
    );

    if !missing("pending creator fee recipient") {
        let pending = powers.pending_creator_fee_recipient.is_some();
        let rendered = if pending { "pending" } else { "none pending" };
        facts.push(
            Fact::exact(
                Kind::PendingCreatorFeeRecipientSet,
                "whether the factory's pendingCreatorFeeRecipient timelock names a non-zero \
                 address",
                if pending { 1.0 } else { 0.0 },
                rendered.to_owned(),
            )
            .saying(
                Voice::Plain,
                if pending {
                    "A change to who receives creator fees is pending on this launch.".to_owned()
                } else {
                    "No change to who receives creator fees is pending on this launch.".to_owned()
                },
            )
            .saying(
                Voice::Blunt,
                format!("Pending fee-recipient change: {rendered}."),
            ),
        );
    }

    let exemptions_unread = missing("declared snipe-tax exemptions")
        || missing("snipe tax exemption")
        || missing("snipe tax exemption classification");
    if !exemptions_unread {
        let undeclared = powers
            .exemptions
            .iter()
            .filter(|exemption: &&Exemption| exemption.source == ExemptionSource::Undeclared)
            .count();
        #[expect(
            clippy::cast_precision_loss,
            reason = "a count of exemptions on one launch is far inside f64's exact integer \
                      range"
        )]
        let undeclared_f64 = undeclared as f64;
        facts.push(
            Fact::exact(
                Kind::UndeclaredExemptions,
                "addresses exempt from this launch's snipe tax that are on neither research \
                 0047 §3's first-party list nor the launch's own declared list",
                undeclared_f64,
                undeclared.to_string(),
            )
            .saying(
                Voice::Plain,
                format!(
                    "{undeclared} address(es) are exempt from this launch's snipe tax without \
                     being on the first-party list or the launch's own declared list."
                ),
            )
            .saying(
                Voice::Blunt,
                format!("Undeclared exemptions: {undeclared}."),
            ),
        );
    }

    // Fires unconditionally once `Some` -- see this function's own doc
    // comment.
    signals.push(Signal::OwnerCanStillMintOrPause);
}

/// The launcher's own launch-block buy as a share of the token's total
/// supply, from `dev_buy_tokens` and `supply` alone -- both already on the
/// receipt `launch_facts` reads, so this needs no `eth_call` for a
/// launch-block price the way research 0052 §3.1's S1 row assumed.
///
/// `None` on either side pushes nothing: a share computed from a supply of
/// `None` would be inventing the denominator (AGENTS.md §3 rule 2), and a
/// share is not a fact until both halves are read. Integer bps, checked,
/// because a share this small in a wrong direction is exactly the number
/// [`crate::sheet::factors`] grades a boundary on.
fn push_dev_buy_share(facts: &mut Vec<Fact>, launch: &ChainLaunch) {
    let (Some(tokens), Some(supply)) = (launch.dev_buy_tokens, launch.supply) else {
        return;
    };
    if supply == 0 {
        return;
    }
    let Some(bps) = tokens
        .checked_mul(10_000)
        .and_then(|n| n.checked_div(supply))
    else {
        return;
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "a bps share is at most a few million even for a wildly lopsided supply, far \
                  inside f64's exact integer range"
    )]
    let bps_f64 = bps as f64;
    // Always two places: a bps share is exact to 0.01%, so two places
    // neither rounds a small buy to zero nor adds a digit that was not read.
    let rendered = format!("{:.2}%", bps_f64 / 100.0);
    facts.push(
        Fact::exact(
            Kind::DevBuyShare,
            "the launcher's own launch-transaction buy as a share of the token's total supply, \
             from the receipt's CurveBuy tokensOut and mint Transfer alone",
            bps_f64,
            rendered.clone(),
        )
        .saying(
            Voice::Plain,
            format!("The launcher bought {rendered} of supply in the launch transaction."),
        )
        .saying(
            Voice::Blunt,
            format!("Launcher's launch-tx share: {rendered}."),
        ),
    );
}

/// Who holds a Robinhood token, and how much the largest single address has.
///
/// **An address, never a person.** After graduation the largest holder is
/// usually the trading pool, and any holder may be a contract, so the share is
/// labelled as an address's and the sentence says it may not be a person.
/// Calling it a whale or a wallet would be a claim about who owns it, which
/// nothing here read.
fn push_holders(facts: &mut Vec<Fact>, holders: &Holders) {
    facts.push(
        Fact::exact(
            Kind::Holders,
            "addresses holding the token now, not counting its curve, the factory or the zero \
             address",
            f64::from(holders.count),
            holders.count.to_string(),
        )
        .saying(
            Voice::Plain,
            format!(
                "{} addresses hold it, not counting its bonding curve.",
                holders.count
            ),
        )
        .saying(Voice::Blunt, format!("{} holders.", holders.count)),
    );
    if let Some(bps) = holders.largest_share_bps {
        let share = Fact::share(
            Kind::LargestHolderShare,
            "share of the supply outside the curve held by the single largest address, which \
             may be a pool or a contract rather than a person",
            f64::from(bps) / 10_000.0,
        );
        let pct = share.rendered.clone();
        facts.push(
            share
                .saying(
                    Voice::Plain,
                    format!(
                        "The largest single address holds {pct} of the supply outside the curve; \
                         it may be a pool, not a person."
                    ),
                )
                .saying(Voice::Blunt, format!("Top address: {pct} of what's out.")),
        );
    }
}

/// Who funded the early buyers that were checked.
///
/// # A count with its denominator, never an owner
///
/// "The same address funded 3 of the 4 early buyers checked" is a chain
/// fact: three transfers, one sender, before three purchases. An exchange's
/// hot wallet produces exactly that pattern for three strangers who withdrew
/// to fresh wallets, so the sentence stops at the flow. It never says "one
/// person", "insiders", "the same owner" or "controlled": those are claims
/// about identity the chain cannot settle, and `forbidden.rs` refuses the
/// first two outright. The denominator is always the checked count, never
/// the buyer count, and "checked" is in the words so a reader cannot take
/// four wallets for all of them.
///
/// Dust never reaches here: `wallets.rs` marks a funder material only
/// against the purchase, and `Funding::shared` counts material funders only.
fn push_funding(facts: &mut Vec<Fact>, unknown: &mut Vec<String>, funding: &Funding) {
    let checked = u32::try_from(funding.checked.len()).unwrap_or(u32::MAX);
    if checked == 0 {
        // No candidate checked is not "nobody funded anybody"; the gap
        // below says why, and a sheet with no funding fact says nothing.
        if !funding.gaps.is_empty() {
            unknown.push("who funded the early buyers could not be read".to_owned());
        }
        return;
    }
    // Solana has no quote amount to weigh a share against; `coverage_bps` is
    // `None` there, and the clause that would name a share is dropped
    // entirely rather than rendering a false "0%" (AGENTS.md rule 8: absent
    // is not zero).
    let coverage = funding
        .coverage_bps
        .map(|bps| format!("{:.0}%", f64::from(bps) / 100.0));
    let plain_words = coverage.as_ref().map_or_else(
        || {
            format!(
                "Where {checked} of the {} early buyers got their money was checked.",
                funding.buyers
            )
        },
        |pct| {
            format!(
                "Where {checked} of the {} early buyers got their money was checked; they bought \
                 {pct} of what the launch window bought.",
                funding.buyers
            )
        },
    );
    let blunt_words = coverage.as_ref().map_or_else(
        || {
            format!(
                "Funding checked for {checked} of {} early buyers.",
                funding.buyers
            )
        },
        |pct| {
            format!(
                "Funding checked for {checked} of {} early buyers ({pct} of the window's buys).",
                funding.buyers
            )
        },
    );
    facts.push(
        Fact::exact(
            Kind::FundingChecked,
            "early buyers whose funding before their first purchase was checked, of the buyers \
             in the launch window",
            f64::from(checked),
            format!("{checked} of {}", funding.buyers),
        )
        .saying(Voice::Plain, plain_words)
        .saying(Voice::Blunt, blunt_words),
    );
    if let Some(top) = funding.shared.first() {
        facts.push(
            Fact::exact(
                Kind::SharedFunder,
                "checked early buyers that one address sent material value to before their first \
                 purchase; a flow between addresses, which an exchange also produces, not \
                 ownership",
                f64::from(top.funded),
                format!("{} of {checked}", top.funded),
            )
            .saying(
                Voice::Plain,
                format!(
                    "The same address funded {} of the {checked} early buyers checked before they \
                     bought. That is a flow on chain; an exchange paying out withdrawals looks \
                     the same.",
                    top.funded
                ),
            )
            .saying(
                Voice::Blunt,
                format!(
                    "One address funded {} of the {checked} early buyers checked.",
                    top.funded
                ),
            ),
        );
    }
    if !funding.gaps.is_empty() {
        unknown.push(format!(
            "the funding check did not finish: {checked} of {} chosen early buyers were read",
            funding.selected
        ));
    }
}

/// S2's four buyer-derived raise/lower factors (research 0052 §3.1's
/// `LaunchBlockInStrongestBand` row), split out of [`FactSheet::build`]
/// itself.
///
/// **`funding.checked` is a cost-limited sample**, at most
/// `wallets::MAX_CANDIDATES` (4) candidates, never every buyer in the
/// launch window -- every fact this function pushes says so in its
/// rendering or label, and the two counting facts (fresh buyers, linked
/// holdings) sum only what was checked, never claiming the full window.
///
/// Each of the four inputs is independent, per rule 8 (absent is not
/// zero): a read this function cannot complete registers a `skipped`
/// coverage gap and leaves that one `Kind` off the sheet, without
/// stopping the other three.
fn push_window_buyer_factors(
    facts: &mut Vec<Fact>,
    skipped: &mut Vec<String>,
    funding: &Funding,
    supply: Option<u128>,
    powers: Option<&Powers>,
) {
    push_window_holdings_and_freshness(facts, skipped, funding, supply);
    push_window_sizes_and_exemption(facts, skipped, funding, powers);
}

/// The +1,000 linked-holdings and +800 fresh-buyer factors of research
/// 0052 §3.1's S2 row -- split from the sizes/exemption pair below only to
/// stay under clippy's line-count cap, not because the four facts differ in
/// kind.
fn push_window_holdings_and_freshness(
    facts: &mut Vec<Fact>,
    skipped: &mut Vec<String>,
    funding: &Funding,
    supply: Option<u128>,
) {
    // +1,000: same-window buyers hold >= 1,000 bps together (M), read from
    // the checked candidates' raw (unweighted) shares of total supply --
    // `confidence_bps = 10,000` for every one, which is what makes this a
    // raw sum rather than S1's confidence-discounted "effective" sum.
    match supply {
        None | Some(0) => skipped
            .push("same-window buyers' linked holdings needs the launch's total supply".to_owned()),
        Some(supply) => {
            let mut excluded = 0u32;
            let mut holdings = Vec::new();
            for candidate in &funding.checked {
                // A candidate with no token amount read is unknown, not a
                // zero share -- excluded from the sum, not counted against
                // it (AGENTS.md §3 rule 8).
                let Some(tokens) = candidate.bought_tokens else {
                    excluded += 1;
                    continue;
                };
                let Ok(address) = candidate.address.parse() else {
                    excluded += 1;
                    continue;
                };
                let share_bps =
                    u16::try_from(tokens.saturating_mul(10_000) / supply).unwrap_or(u16::MAX);
                holdings.push((address, share_bps, 10_000u16));
            }
            if holdings.is_empty() {
                skipped.push(
                    "same-window buyers' linked holdings needs a checked candidate's token \
                     amount, and none were read"
                        .to_owned(),
                );
            } else {
                let bps = realorrug_onchain::wallets::linked_holdings_bps(&holdings);
                let bps_f64 = f64::from(bps);
                let note = if excluded > 0 {
                    format!(
                        ", {excluded} of {} checked candidates excluded (no token amount or address read)",
                        funding.checked.len()
                    )
                } else {
                    String::new()
                };
                facts.push(Fact::exact(
                    Kind::WindowBuyersLinkedHoldingsBps,
                    "same-window buyers' combined holding, in basis points, from the checked \
                     candidates alone (a cost-limited sample, never every buyer in the launch \
                     window)",
                    bps_f64,
                    format!("{:.2}%{note}", bps_f64 / 100.0),
                ));
            }
        }
    }

    // +800: >= 3 of the checked buyers are fresh (M), i.e.
    // `nonce_before_launch == Some(0)`. Any unread nonce makes the whole
    // count an undercount that could wrongly miss the raise, so the fact
    // stays absent rather than being taken over the readable subset.
    if !funding.checked.is_empty() {
        if funding
            .checked
            .iter()
            .any(|candidate| candidate.nonce_before_launch.is_none())
        {
            skipped.push(
                "how many same-window buyers are fresh needs every checked candidate's \
                 pre-launch transaction count, and at least one was not read"
                    .to_owned(),
            );
        } else {
            let fresh = funding
                .checked
                .iter()
                .filter(|candidate| candidate.nonce_before_launch == Some(0))
                .count();
            #[expect(
                clippy::cast_precision_loss,
                reason = "a count of checked candidates is at most MAX_CANDIDATES (4), far \
                          inside f64's exact integer range"
            )]
            let fresh_f64 = fresh as f64;
            facts.push(Fact::exact(
                Kind::FreshWindowBuyers,
                "checked same-window buyers with no transactions before the launch block (a \
                 cost-limited sample, never every buyer in the launch window)",
                fresh_f64,
                format!("{fresh} of {}", funding.checked.len()),
            ));
        }
    }
}

/// The +500 size-spread and -500 exemption factors of research 0052 §3.1's
/// S2 row -- see `push_window_holdings_and_freshness`'s doc comment for why
/// this is split out.
fn push_window_sizes_and_exemption(
    facts: &mut Vec<Fact>,
    skipped: &mut Vec<String>,
    funding: &Funding,
    powers: Option<&Powers>,
) {
    // +500: checked buy sizes are within 10% of each other (I), compared by
    // spend (`bought_wei`, always read) rather than token amount (which a
    // candidate may lack, see the holdings block above).
    if funding.checked.len() < 2 {
        skipped.push(
            // No digits: the report repeats this reason, and the number check
            // refuses a figure the sheet did not measure.
            "whether same-window buy sizes are close to each other needs at least two \
             checked candidates"
                .to_owned(),
        );
    } else {
        let smallest = funding
            .checked
            .iter()
            .map(|candidate| candidate.bought_wei)
            .min()
            .unwrap_or(0);
        let largest = funding
            .checked
            .iter()
            .map(|candidate| candidate.bought_wei)
            .max()
            .unwrap_or(0);
        let within = realorrug_onchain::wallets::sizes_within_ten_percent(smallest, largest);
        facts.push(Fact::exact(
            Kind::WindowBuySizesWithinTenPercent,
            "whether the checked same-window buyers' spends are all within 10% of each other \
             (a cost-limited sample, never every buyer in the launch window)",
            if within { 1.0 } else { 0.0 },
            if within {
                "within 10%".to_owned()
            } else {
                "not within 10%".to_owned()
            },
        ));
    }

    // -500: every launch-window buyer is on the declared exemption list
    // (M). Only when `funding.checked` is the *full* buyer list, never a
    // sample -- see `Kind::AllWindowBuyersDeclaredExempt`'s doc comment.
    let checked_count = u32::try_from(funding.checked.len()).unwrap_or(u32::MAX);
    if checked_count == funding.buyers && !funding.checked.is_empty() {
        if let Some(powers) = powers {
            let all_declared = funding.checked.iter().all(|candidate| {
                let Ok(address) = candidate.address.parse::<realorrug_types::ChainAddress>() else {
                    return false;
                };
                powers.exemptions.iter().any(|exemption| {
                    exemption.address == address && exemption.source == ExemptionSource::Declared
                })
            });
            facts.push(Fact::exact(
                Kind::AllWindowBuyersDeclaredExempt,
                "whether every launch-window buyer is on this launch's declared snipe-tax \
                 exemption list",
                if all_declared { 1.0 } else { 0.0 },
                if all_declared {
                    "all declared-exempt".to_owned()
                } else {
                    "not all declared-exempt".to_owned()
                },
            ));
        } else {
            skipped.push(
                "whether every launch-window buyer is declared-exempt needs this launch's \
                 exemption list"
                    .to_owned(),
            );
        }
    }
    // Else: `funding.checked` is a sample, not the full window -- not a
    // read failure, so not a coverage gap (the sampling limit itself is
    // documented on `Kind::AllWindowBuyersDeclaredExempt`, not repeated as
    // a gap on every sheet that hits it).
}

/// What this creator's other tokens did.
///
/// # Counts, never a rate
///
/// "Nine of forty-one" and "22%" say the same thing to an arithmetician and
/// different things to a reader: the share hides the denominator, and the
/// denominator is the part that decides whether the number means anything.
/// `creator_track_record` computes rates with a minimum sample and a note
/// explaining itself; this publishes what was counted and lets the reader do
/// the division.
///
/// # Absent is not innocent
///
/// A creator the index has never seen launched before Radar was watching. That
/// is said plainly, because a reply that omitted the line would read as a clean
/// record — rule 9 in the direction that flatters, which is the one that gets
/// somebody hurt.
///
/// # Never presented as a good sign
///
/// Research 0011: graduation predicts **volatility, not profit**. Organic
/// graduations end at a median −3,228 bps against −853 for tokens that never
/// graduate. So the graduation count is published as a measurement and the
/// label never suggests it is encouraging.
/// What "filled its curve immediately" is counted in, on a given chain.
///
/// Not decoration. Three Solana slots is about 1.2 seconds and three Robinhood
/// blocks is about six, so the word is part of what the figure means -- and a
/// reply that said "slots" about a Pons v2 launch would be quoting a
/// measurement in a unit nobody measured it in.
const fn immediate_fill_unit(chain: crate::firstparty::Chain) -> &'static str {
    match chain {
        crate::firstparty::Chain::Solana => "slots",
        crate::firstparty::Chain::Robinhood => "blocks",
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "five near-identical fact pushes in a row, each with the comment explaining why its               denominator travels with it; carrying the chain's unit for the instant-fill line               put it one line over, and splitting a straight list of facts in two to satisfy a               line count would separate those comments from the pushes they explain"
)]
fn push_creator(
    facts: &mut Vec<Fact>,
    unknown: &mut Vec<String>,
    creator: &str,
    index: &crate::creator::CreatorIndex,
    chain: crate::firstparty::Chain,
) {
    let unit = immediate_fill_unit(chain);
    let Some(record) = index.get(creator) else {
        // One line, no continuation. A `\` continuation in a Rust string keeps
        // the *leading* whitespace of the next line, so this rendered with a
        // run of fourteen spaces in the middle of a published sentence -- which
        // is the sort of thing that looks like a broken bot rather than a
        // careful one.
        unknown.push(
            "this creator has no record here: Real or Rug has been watching since August, so they launched before that, or have not launched again"
                .to_owned(),
        );
        return;
    };

    let launches = record.launches.to_string();
    // The caveat rides on the sentence rather than in a separate list, when
    // there is one to make. An index that has counted launches but measured no
    // outcomes is the normal state of a new index -- the launch walk is tens of
    // calls, the outcome pass is one per token -- and the count it does hold is
    // the whole of what `Signal::RepeatLauncher` needs.
    //
    // **Why not `unknown`.** That list is read twice: the reply says every line
    // in it, and `verdict::level` returns `CantTell` the moment it is non-empty.
    // Putting "no outcome measured" there would mean a sheet that can see this
    // launcher has forty-one launches to their name reports that it cannot tell
    // you anything -- the evidence read as its own absence. Attached here it is
    // still said, in the same breath as the number it qualifies, where a model
    // writing freely cannot quote the count and drop the limit.
    let (label, unmeasured) = if record.measured == 0 {
        (
            "tokens this creator has launched, in Real or Rug's record -- none with an outcome measured yet",
            " None of them has had an outcome measured yet.",
        )
    } else {
        (
            "tokens this creator has launched, in Real or Rug's record",
            "",
        )
    };
    facts.push(
        Fact::exact(
            Kind::CreatorLaunches,
            label,
            f64::from(record.launches),
            launches.clone(),
        )
        // "in Radar's record" is in the sentence and not in the model's memory.
        // The window is the part a reader needs to weigh the count, and it is
        // the part a free-writing model drops first.
        .saying(
            Voice::Plain,
            format!(
                "This creator has launched {launches} tokens in Real or Rug's record.{unmeasured}"
            ),
        )
        .saying(
            Voice::Blunt,
            format!("{launches} launches on this creator, in Real or Rug's record.{unmeasured}"),
        ),
    );

    // The denominator, always beside the numerator. A gap between launches and
    // measured means the outcome pass has not caught up -- not that those
    // tokens did nothing -- and a share quoted without it would be a share of
    // an unstated population.
    if record.measured == 0 {
        return;
    }
    // Every clause below carries its denominator, because that is the number
    // that decides whether the numerator means anything -- and the denominator
    // is what a sentence written for effect leaves out.
    let measured = record.measured.to_string();
    facts.push(
        Fact::exact(
            Kind::CreatorMeasured,
            "of those, how many have been measured",
            f64::from(record.measured),
            measured.clone(),
        )
        .saying(
            Voice::Plain,
            format!("Of those, {measured} have had an outcome measured."),
        )
        .saying(
            Voice::Blunt,
            format!("{measured} of them have been measured."),
        ),
    );
    let organic = record.organic.to_string();
    facts.push(
        Fact::exact(
            Kind::CreatorOrganic,
            "of the measured, how many reached an AMM by filling over time",
            f64::from(record.organic),
            organic.clone(),
        )
        .saying(
            Voice::Plain,
            format!("{organic} of those {measured} filled a curve over time."),
        )
        .saying(
            Voice::Blunt,
            format!("{organic} of {measured} filled a curve the slow way."),
        ),
    );
    let instant = record.instant.to_string();
    facts.push(
        Fact::exact(
            Kind::CreatorInstant,
            format!(
                "of the measured, how many filled their curve within three {unit} (capital committed before the token existed, not demand)"
            ),
            f64::from(record.instant),
            instant.clone(),
        )
        .saying(
            Voice::Plain,
            format!(
                "{instant} of those {measured} filled inside three {unit}, which is capital arranged before the token existed."
            ),
        )
        .saying(
            Voice::Blunt,
            format!("{instant} of {measured} filled inside three {unit}. That is arrangement, not demand."),
        ),
    );
    let stillborn = record.stillborn.to_string();
    facts.push(
        Fact::exact(
            Kind::CreatorStillborn,
            "of the measured, how many showed almost no activity at all",
            f64::from(record.stillborn),
            stillborn.clone(),
        )
        .saying(
            Voice::Plain,
            format!("{stillborn} of those {measured} showed almost no activity at all."),
        )
        .saying(
            Voice::Blunt,
            format!("{stillborn} of {measured} never moved."),
        ),
    );
}

/// The population as **Radar itself measured it**, from the store.
///
/// # Why this is beside the snapshot rather than instead of it
///
/// `push_population` places one coin's recipient count in a distribution that
/// came from outside: a public RPC walking 45 slots, and a SQL endpoint that
/// truncates at a thousand rows. That distribution is the only one available,
/// because the store did not record a launch-block recipient count until
/// ADR 0012 and only rows written after 2026-09-03 carry one.
///
/// The graduation rates are different. Radar has every succeeded launch it ever
/// recorded and every outcome it ever measured, so it can count them rather than
/// sample them — and the creator-index timer already does, every six hours, in
/// the same pass. On these figures the store is the better instrument, and using
/// the sampled ones when the counted ones are on disk would be a choice to be
/// less accurate.
///
/// # The denominator is stated, always
///
/// Every share here is over `measured`, and `measured` is printed beside them.
/// The gap between what was launched and what was measured is Radar's own
/// backlog, and a share quoted without its denominator invites the reader to
/// treat a lag as a finding.
fn push_measured_population(facts: &mut Vec<Fact>, population: &crate::creator::Population) {
    // Rule 9 in one branch: nothing measured is not a population of zeroes. Say
    // that the figure is missing, in Radar's own words, rather than publishing
    // "0% of launches graduate" off an empty denominator.
    let Some(graduated) = population.graduated_share() else {
        facts.push(Fact {
            about: About::Measurement,
            kind: Kind::VenueUnmeasured,
            label: "how the venue as a whole turns out".to_owned(),
            rendered: "NOT AVAILABLE -- no outcome has been measured yet".to_owned(),
            values: Vec::new(),
            // No clause: an absence is context for choosing what to say, and
            // there is nothing here to publish. A sentence about it would be a
            // sentence about Radar's backlog.
            clauses: Vec::new(),
        });
        return;
    };
    let measured = population.measured.to_string();
    facts.push(Fact::exact(
        Kind::VenueMeasured,
        "launches Real or Rug has recorded and measured, which every share below is out of",
        // Lossless below 2^53; these are counts of launches.
        #[expect(
            clippy::cast_precision_loss,
            reason = "counts of launches; 2^53 is six orders of magnitude away"
        )]
        {
            population.measured as f64
        },
        measured.clone(),
    ));
    // The denominator travels inside every share's sentence rather than beside
    // it, so a clause published alone still says what it is a share of.
    let share = Fact::share(
        Kind::VenueGraduated,
        "of every measured launch, how many graduated at all",
        graduated,
    );
    let rendered = share.rendered.clone();
    facts.push(
        share
            .saying(
                Voice::Plain,
                format!("Across the {measured} launches Real or Rug has measured, {rendered} graduated at all."),
            )
            .saying(
                Voice::Blunt,
                format!("{rendered} of {measured} measured launches ever graduated."),
            ),
    );
    if let Some(organic) = population.organic_share() {
        let f = Fact::share(
            Kind::VenueOrganic,
            "of every measured launch, how many filled their curve over time",
            organic,
        );
        let r = f.rendered.clone();
        facts.push(
            f.saying(
                Voice::Plain,
                format!("Of those {measured} measured launches, {r} filled a curve over time."),
            )
            .saying(
                Voice::Blunt,
                format!("{r} of {measured} filled a curve over time."),
            ),
        );
    }
    if let Some(instant) = population.instant_share() {
        let f = Fact::share(
            Kind::VenueInstant,
            "of every measured launch, how many filled inside their own launch block",
            instant,
        );
        let r = f.rendered.clone();
        facts.push(
            f.saying(
                Voice::Plain,
                format!("Of those {measured} measured launches, {r} filled inside their own launch block."),
            )
            .saying(
                Voice::Blunt,
                format!("{r} of {measured} filled inside their own launch block."),
            ),
        );
    }
    if let Some(stillborn) = population.stillborn_share() {
        let f = Fact::share(
            Kind::VenueStillborn,
            "of every measured launch, how many showed almost no activity at all",
            stillborn,
        );
        let r = f.rendered.clone();
        facts.push(
            f.saying(
                Voice::Plain,
                format!(
                    "Of those {measured} measured launches, {r} showed almost no activity at all."
                ),
            )
            .saying(Voice::Blunt, format!("{r} of {measured} never moved.")),
        );
    }
}

fn push_population(facts: &mut Vec<Fact>, recipients: Count, rates: &BaseRates) {
    // A truncated count must not be looked up in a distribution: the band it
    // lands in would be decided by Radar's call budget rather than by the chain.
    let Some(exact) = recipients.exact() else {
        facts.push(Fact {
            about: About::Measurement,
            kind: Kind::BandUnavailable,
            label: "population context for the recipient count".to_owned(),
            rendered: "NOT AVAILABLE -- the count was cut short, so it cannot be \
                       placed in a distribution"
                .to_owned(),
            values: Vec::new(),
            clauses: Vec::new(),
        });
        return;
    };
    let Some(band) = rates.band_for(exact) else {
        return;
    };
    push_band(facts, exact, band, rates.launches);
    push_base_rates(facts, rates);
}

/// This launch's own band, as a distribution it sits inside.
fn push_band(
    facts: &mut Vec<Fact>,
    exact: u32,
    band: &crate::baserates::Band,
    total_launches: u64,
) {
    // `band.name` is Radar's own label for a range and it contains digits --
    // "10-13 recipients". Those digits are on the sheet because the band's own
    // facts authorise them, and they are inside the clause for the same reason
    // the denominator is: a share of an unnamed population is not a
    // measurement, it is a mood.
    let name = &band.name;
    let f = Fact::share(
        Kind::BandNeverGraduated,
        format!(
            "share of launches that NEVER graduated whose block had {exact} recipients ({name})"
        ),
        band.never_graduated,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!("Among launches in the {name} band, {r} never graduated at all."),
        )
        .saying(
            Voice::Blunt,
            format!("{r} of the {name} band never graduated."),
        ),
    );
    let f = Fact::share(
        Kind::BandOrganic,
        format!("share of ORGANIC graduations in that band ({name})"),
        band.organic,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!("Among launches in the {name} band, {r} filled a curve over time."),
        )
        .saying(
            Voice::Blunt,
            format!("{r} of the {name} band filled a curve over time."),
        ),
    );
    let f = Fact::share(
        Kind::BandInstant,
        format!("share of INSTANT graduations in that band ({name})"),
        band.instant,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!("Among launches in the {name} band, {r} graduated instantly."),
        )
        .saying(
            Voice::Blunt,
            format!("{r} of the {name} band graduated instantly."),
        ),
    );
    let f = Fact::share(
        Kind::BandInstantProbability,
        format!("probability a launch in that band ({name}) graduates instantly"),
        band.p_instant,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!("A launch in the {name} band graduates instantly {r} of the time."),
        )
        .saying(
            Voice::Blunt,
            format!("Instant graduation in the {name} band: {r}."),
        ),
    );
    // Research 0024 is the record of this multiple moving off six recipients. A
    // clause states it as a dated comparison against the population rate rather
    // than as a property of the band, because it is the ratio of two measured
    // shares and the denominator is the one that moves.
    let times = format!("{:.1}x", band.x_base_instant);
    facts.push(
        Fact::exact(
            Kind::BandTimesBaseRate,
            format!("how many times the base rate that is ({name} band)"),
            band.x_base_instant,
            times.clone(),
        )
        .saying(
            Voice::Plain,
            format!("That is {times} the rate across every launch Real or Rug has measured."),
        )
        .saying(Voice::Blunt, format!("{times} the rate of the field.")),
    );
    let (launches_f64, launches) = band_launches(band, total_launches);
    facts.push(Fact::exact(
        Kind::BandLaunches,
        format!("launches this project has measured with {exact} recipients ({name})"),
        launches_f64,
        launches.to_string(),
    ));
}

/// The band's own sample size (research 0052 §3.1's S2 row's thin-sample
/// lower). `Band` itself carries no raw count -- only `fires_on`, its share of
/// *all* launches -- so this derives the count from that share and the
/// snapshot's own total, both already measured; it is a computation over two
/// read numbers, not an invented one.
fn band_launches(band: &crate::baserates::Band, total_launches: u64) -> (f64, u64) {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count of launches across the whole snapshot is far inside f64's exact \
                  integer range for any sample this project will ever measure"
    )]
    let total_f64 = total_launches as f64;
    let launches_f64 = (band.fires_on * total_f64).round();
    #[expect(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        reason = "fires_on is a share in [0, 1] and total_launches is non-negative, so the \
                  product rounds to a non-negative count far inside u64's range"
    )]
    let launches = launches_f64 as u64;
    (launches_f64, launches)
}

/// The whole field, which is what makes a band figure mean anything.
///
/// Carries `measured_on` into the Plain-voice line ("as of `<date>`") rather
/// than only into a log line the reader never sees. This snapshot is kept and
/// quoted up to `STALE_AFTER_DAYS` (`baserates.rs`) rather than dropped after a
/// short fixed window, so the date travelling with the fact -- not a silent
/// cutoff -- is what tells a reader how current the figure is.
fn push_base_rates(facts: &mut Vec<Fact>, rates: &BaseRates) {
    let as_of = &rates.measured_on;
    let f = Fact::share(
        Kind::BaseInstant,
        format!("population rate (as of {as_of}): share of all launches that graduate instantly"),
        rates.base_rate_instant,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!(
                "Across every launch in the snapshot (as of {as_of}), {r} graduated instantly."
            ),
        )
        .saying(
            Voice::Blunt,
            format!("The whole field graduates instantly {r} of the time."),
        ),
    );
    let f = Fact::share(
        Kind::BaseGraduates,
        format!("population rate (as of {as_of}): share of all launches that graduate at all"),
        rates.base_rate_graduates,
    );
    let r = f.rendered.clone();
    facts.push(
        f.saying(
            Voice::Plain,
            format!("Across every launch in the snapshot, {r} graduated at all."),
        )
        .saying(
            Voice::Blunt,
            format!("{r} of the whole field ever graduates."),
        ),
    );
}

/// Pushes the "has this token graduated" fact and, when it has, the AMM
/// exit-capacity note, returning `true` so `push_curve` skips the curve-only
/// facts that follow.
///
/// Split out of `push_curve` to keep that function under the line-count
/// lint, not for reuse. A graduated coin has an empty curve *because it
/// left*, and the two remaining curve facts are both false about it: the
/// capacity is not zero, it is elsewhere, and the curve's fee schedule is not
/// the fee the coin pays. "cannot size into this at all" about a coin trading
/// on an AMM is rule 9 read backwards -- absent taken for zero -- and it is
/// the worst line the sheet could carry, because a graduated coin is exactly
/// the kind of coin people ask the bot about.
fn push_graduation(facts: &mut Vec<Fact>, curve: &realorrug_onchain::CurveFacts) -> bool {
    facts.push(
        Fact {
            about: About::Measurement,
            kind: Kind::Graduated,
            label: "has the token graduated off the bonding curve".to_owned(),
            rendered: if curve.complete { "yes" } else { "no" }.to_owned(),
            values: Vec::new(),
            clauses: Vec::new(),
        }
        .saying(
            Voice::Plain,
            if curve.complete {
                "It has left the bonding curve and trades on an AMM."
            } else {
                "It is still on its bonding curve."
            },
        )
        .saying(
            Voice::Blunt,
            if curve.complete {
                "Off the curve, on an AMM."
            } else {
                "Still on the curve."
            },
        ),
    );
    if curve.complete {
        facts.push(
            Fact {
                about: About::Measurement,
                kind: Kind::CapacityAfterGraduation,
                label: "exit capacity".to_owned(),
                rendered:
                    "graduated off the curve; it trades on the AMM, which Real or Rug does not price. \
                     NOT zero, and NOT 'cannot size into this'."
                        .to_owned(),
                values: Vec::new(),
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                "Real or Rug does not price the AMM it moved to, so it has no exit size for this one.",
            )
            .saying(Voice::Blunt, "Real or Rug cannot size the AMM it moved to."),
        );
        return true;
    }
    false
}

fn push_curve(
    facts: &mut Vec<Fact>,
    unknown: &mut Vec<String>,
    curve: &realorrug_onchain::CurveFacts,
    read_at: Option<ReadAt>,
) {
    if push_graduation(facts, curve) {
        return;
    }

    // ADR 0033: liquidity is one of the figures the analyst may now state for
    // every token, and it carries the moment it was read at -- a reserve
    // figure with no read point is a stale figure that looks current. Read
    // from `curve.quote_reserves`, already fetched by `curve_facts` for the
    // capacity calculation below; no new RPC call.
    if let Some(asset) = &curve.quote_asset {
        let amount = format!(
            "{} {}",
            render_quote(curve.quote_reserves, asset.decimals),
            asset.symbol
        );
        let moment = read_at.map_or_else(|| "an unread point".to_owned(), |r| r.to_string());
        facts.push(
            Fact {
                about: About::Price,
                kind: Kind::CurveLiquidity,
                label: format!("quote asset held in the bonding curve now, read at {moment}"),
                rendered: amount.clone(),
                values: vec![quote_as_f64(curve.quote_reserves, asset.decimals)],
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                format!("The curve holds {amount} right now, as of {moment}."),
            )
            .saying(
                Voice::Blunt,
                format!("{amount} in the curve, as of {moment}."),
            ),
        );

        // S1, "name the pair": when this curve's quote asset is an ERC-20
        // token rather than native ETH, state which one by its symbol AND
        // its address, never the symbol alone -- `asset.symbol` is the
        // launcher-chosen text `sanitised_symbol` cleared, and the address
        // is what lets a reader check that name rather than take it on
        // faith (AGENTS.md §3 rule 3: untrusted metadata is data, and data
        // a reader can verify is safer data than data they cannot).
        if let Some(address) = &asset.address {
            let pair = format!("{} ({address})", asset.symbol);
            facts.push(
                Fact {
                    about: About::Measurement,
                    kind: Kind::QuotePair,
                    label: "the ERC-20 token this curve is paired with, not ETH".to_owned(),
                    rendered: pair.clone(),
                    values: Vec::new(),
                    clauses: Vec::new(),
                }
                .saying(
                    Voice::Plain,
                    format!("This curve is paired with {pair}, not ETH."),
                )
                .saying(Voice::Blunt, format!("Paired with {pair}. Not ETH.")),
            );
        }
    }

    match (curve.quote_capacity, &curve.quote_asset) {
        (Some(l), Some(asset)) => {
            let amount = format!("{} {}", render_quote(l, asset.decimals), asset.symbol);
            facts.push(
                Fact::exact(
                    Kind::Capacity,
                    "quote asset that can be bought before price moves 1% -- this is REAL OR RUG.S \
                     OWN impact budget, NOT a ceiling the venue imposes (research 0022)",
                    quote_as_f64(l, asset.decimals),
                    amount.clone(),
                )
                // Research 0022 reversed the capacity claim: this is Radar's own
                // impact budget, not a ceiling the venue imposes. The clause
                // says whose budget it is, in the sentence, because the earlier
                // wording was read as the venue's limit and that reading was
                // wrong for a year.
                .saying(
                    Voice::Plain,
                    format!("{amount} can be bought before the price moves one percent, on Real or Rug's own impact budget."),
                )
                .saying(
                    Voice::Blunt,
                    format!("{amount} before the price moves one percent, by Real or Rug's budget."),
                ),
            );
        }
        // A capacity figure exists but this reader could not name the asset
        // it is denominated in -- rule 8, a number and its unit travel
        // together or not at all, so no amount is rendered here. This is the
        // "unidentified quote asset" case (never guess ETH, never guess SOL).
        (Some(_), None) => unknown.push(
            "the impact budget could not be priced: the quote asset for this curve is not one \
             Real or Rug can identify"
                .to_owned(),
        ),
        (None, _) => facts.push(
            Fact {
                about: About::Measurement,
                kind: Kind::CapacityNone,
                label: "exit capacity".to_owned(),
                rendered: "none -- cannot size into this at all. NOT 'no limit found'.".to_owned(),
                values: Vec::new(),
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                "No size at all clears Real or Rug's impact budget here.",
            )
            .saying(
                Voice::Blunt,
                "Nothing fits inside Real or Rug's impact budget here.",
            ),
        ),
    }
    push_fee(facts, curve);
}

/// The dated market snapshot (design 0027 §2.2, ADR 0033): a USD price and,
/// when the aggregator reported one, a market cap with its basis.
///
/// **The moment is `snapshot.observed_at`, never `dossier.read_at`.** Those
/// are two different clocks -- `market.rs`'s own doc comment explains why:
/// this is what DexScreener or GeckoTerminal said at the wall-clock instant
/// the HTTP call returned, not what a block on this dossier's own chain
/// held. Rendering the chain's read point here would be publishing a false
/// precision the aggregator never gave.
///
/// **Never `curve.quote_capacity`.** `market.rs`'s own regression test
/// (`liquidity_dollars_never_reach_capacity`) guards the write side of that
/// boundary; this function is the read side, and it never reaches into
/// `snapshot.liquidity_usd` for anything but its own sentence -- a dollar
/// figure the aggregator computed from reserves, not an amount Real or Rug
/// can size a trade into.
fn push_market(facts: &mut Vec<Fact>, snapshot: &MarketSnapshot) {
    let moment = render_observed_at(snapshot.observed_at);
    if let Some(price) = snapshot.price_usd {
        let rendered = format!("${}", render_usd(price));
        facts.push(
            Fact {
                about: About::Price,
                kind: Kind::Market,
                label: format!("USD price, read from an off-chain aggregator at {moment}"),
                rendered: rendered.clone(),
                values: vec![price],
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                format!("An aggregator priced it at {rendered} as of {moment}."),
            )
            .saying(Voice::Blunt, format!("{rendered}, as of {moment}.")),
        );
    }
    if let Some(cap) = snapshot.market_cap_usd {
        // `cap_basis` is `Some` whenever `market_cap_usd` is, per
        // `MarketSnapshot`'s own doc comment -- never guessed, so this
        // fact never states a basis the aggregator did not itself give.
        let basis = snapshot.cap_basis.unwrap_or("basis not reported");
        let rendered = format!("${}", render_usd(cap));
        facts.push(
            Fact {
                about: About::Price,
                kind: Kind::Market,
                label: format!(
                    "USD market cap ({basis}), read from an off-chain aggregator at {moment}"
                ),
                rendered: rendered.clone(),
                values: vec![cap],
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                format!("An aggregator put the market cap at {rendered} ({basis}) as of {moment}."),
            )
            .saying(
                Voice::Blunt,
                format!("{rendered} market cap ({basis}), as of {moment}."),
            ),
        );
    }
}

/// Formats a market snapshot's wall-clock read point as a UTC calendar
/// moment -- `MarketSnapshot::observed_at`'s own clock, not a chain's block
/// or slot, so this is the one place that turns it into words rather than
/// reusing `ReadAt`'s `Display` (which only knows how to say a slot or a
/// block).
///
/// Renders `"2025-09-16 05:20 UTC"`. `FactSheet::authorised` scans every
/// fact's label with `fidelity::literals` and authorises what it finds under
/// the fact's `Subject` (`Subject::Token` here, shared with `Kind::Age`). A
/// scanner that split the moment into 2025, 09, 16, 05 and 20 would let the
/// day-of-month back "it launched 16 hours ago"; `fidelity::literals`
/// instead reads this exact shape as the single value `20250916.0520`, so
/// only the same moment written again can match it.
fn render_observed_at(observed_at: std::time::SystemTime) -> String {
    observed_at
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(
            |_| "an unread point".to_owned(),
            |d| {
                let secs = d.as_secs();
                let days = secs / 86_400;
                let time_of_day = secs % 86_400;
                let (year, month, day) = civil_from_days(days);
                let hour = time_of_day / 3_600;
                let minute = (time_of_day % 3_600) / 60;
                format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
            },
        )
}

/// Days since the Unix epoch (1970-01-01) to a proleptic Gregorian calendar
/// date. Howard Hinnant's `civil_from_days`
/// (<http://howardhinnant.github.io/date_algorithms.html>, public domain),
/// ported to Rust -- correct for any `u64` day count, including every leap
/// day the Gregorian rule recognises, without pulling in a date-and-time
/// crate for one read-only conversion.
fn civil_from_days(days_since_epoch: u64) -> (u64, u32, u32) {
    // Unsigned on purpose: `render_observed_at` only ever has a moment after
    // the epoch, so the algorithm's branch for eras before year 0 would be
    // code no input can reach.
    let z = days_since_epoch + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "doy - (153*mp+2)/5 + 1 is always in [1, 31] by the algorithm's own invariant"
    )]
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "mp is always in [0, 11] by the algorithm's own invariant, so month is in [1, 12]"
    )]
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}

/// Renders a USD figure with two decimal places from a dollar up, and in full
/// below it, so a token priced at a fraction of a cent does not round to
/// "$0.00" and read as free, and a cheap one keeps the digits that matter.
fn render_usd(value: f64) -> String {
    if value.abs() < 1.0 && value != 0.0 {
        format!("{value}")
    } else {
        format!("{value:.2}")
    }
}

/// The largest owner among the sampled top token accounts (design 0027 row
/// 6/7 slice 6a: `realorrug_onchain::TokenOwnership`, Solana only).
///
/// **Excludes any owner proven to be the token's own bonding curve**, the
/// one exclusion `dossier.rs`'s `token_ownership` reader can make without
/// guessing (recomputing the pump.fun bonding curve's program-derived
/// address and matching it). Every other owner stays
/// `OwnerRole::Unresolved` regardless of its balance's size or shape, so
/// this always writes the unresolved-role sentence -- "one unidentified
/// wallet" -- and never a role the sheet did not establish (AGENTS.md §4's
/// last bullet). If every sampled account belongs to the curve, or the
/// sample is empty, there is no non-curve owner to report and this writes
/// nothing rather than a fact about zero owners.
fn push_token_ownership(facts: &mut Vec<Fact>, ownership: &realorrug_onchain::TokenOwnership) {
    let Some(largest) = ownership
        .owners
        .iter()
        .find(|o| o.role != realorrug_onchain::OwnerRole::BondingCurve)
    else {
        return;
    };
    // `share_bps` is `None` only when `getTokenSupply` reported zero (rule 9:
    // never a share against nothing), which makes this owner's stake
    // unmeasurable rather than zero -- so the fact is skipped entirely
    // rather than printed as "0%".
    let Some(bps) = largest.share_bps else {
        return;
    };
    let share = Fact::share(
        Kind::TokenOwnership,
        "share of the total token supply held by the largest owner among the sampled \
         largest accounts, excluding any address proven to be the bonding curve",
        f64::from(bps) / 10_000.0,
    );
    let pct = share.rendered.clone();
    facts.push(
        share
            .saying(
                Voice::Plain,
                format!(
                    "Among the largest sampled token accounts, one unidentified wallet holds \
                     {pct} of the total supply."
                ),
            )
            .saying(
                Voice::Blunt,
                format!("One unidentified wallet: {pct} of supply."),
            ),
    );
}

/// The creator's observed on-chain cash flow on Pons v2 (design 0027 slice
/// 5: `realorrug_onchain::wallets::CreatorCashFlow`, Robinhood only).
///
/// Reads `trades_complete` through the type's own accessors rather than the
/// raw fields: [`realorrug_onchain::wallets::CreatorCashFlow::proceeds_wei`]
/// and `net_wei` already return `None` when the trade history is partial
/// (rule 9, absent is not zero), and gating this whole function on
/// `proceeds_wei` being `Some` extends the same rule to
/// [`realorrug_onchain::wallets::CreatorCashFlow::transfers_out`], whose raw
/// `u32` has no `None` case of its own to fall back to. When the read is
/// incomplete this writes no fact at all, and the gap text on
/// `CreatorCashFlow::gaps` is left unpublished the same way a failed
/// `capacity`/`fees`/`token ownership` read is in `FactSheet::build` --
/// an optional read this analyst's verdict does not depend on, never turned
/// into an `unknown` line that would force `CantTell`.
///
/// Never renders the word "profit": the net figure is proceeds from decoded
/// sales minus quote spent on decoded buys, nothing else -- it excludes gas,
/// fees and any token still held but not sold.
fn push_creator_cash_flow(
    facts: &mut Vec<Fact>,
    cash_flow: &realorrug_onchain::wallets::CreatorCashFlow,
) {
    let Some(proceeds) = cash_flow.proceeds_wei() else {
        return;
    };
    let eth = format!("{} ETH", render_quote(proceeds, 18));
    facts.push(
        Fact::exact(
            Kind::CreatorCashFlow,
            "ETH the creator received in sales -- summed across every decoded sale by the \
             deployer or fee recipient on Pons v2",
            quote_as_f64(proceeds, 18),
            eth.clone(),
        )
        .saying(
            Voice::Plain,
            format!(
                "The creator has received {eth} in sales of this token on Pons v2, across \
                 every decoded sale by the deployer or fee recipient."
            ),
        )
        .saying(Voice::Blunt, format!("Creator sale proceeds: {eth}.")),
    );

    // `net_wei` cannot be `None` here: it is `None` only when either half of
    // the subtraction is, and `proceeds_wei` above already proved
    // `trades_complete`, which is the only thing gating either half.
    if let Some(net) = cash_flow.net_wei() {
        let magnitude = net.unsigned_abs();
        let sign = if net < 0 { "-" } else { "" };
        let rendered = format!("{sign}{} ETH", render_quote(magnitude, 18));
        let value = if net < 0 {
            -quote_as_f64(magnitude, 18)
        } else {
            quote_as_f64(magnitude, 18)
        };
        facts.push(
            Fact::exact(
                Kind::CreatorCashFlow,
                "observed net cash flow on Pons v2 -- sale proceeds minus quote spent buying \
                 in, across every decoded trade by the deployer or fee recipient; excludes \
                 gas, fees and anything still held but not sold",
                value,
                rendered.clone(),
            )
            .saying(
                Voice::Plain,
                format!(
                    "The creator's observed net cash flow on Pons v2 is {rendered} -- sale \
                     proceeds minus what they spent buying in, nothing else."
                ),
            )
            .saying(Voice::Blunt, format!("Observed net cash flow: {rendered}.")),
        );
    }

    // A transfer is never a sale (rule (b), `count_transfers_out`'s own doc
    // comment): it is only counted here, never priced, and only said when
    // there is at least one to say.
    if cash_flow.transfers_out > 0 {
        let count = cash_flow.transfers_out;
        let rendered = format!("{count}");
        facts.push(
            Fact::exact(
                Kind::CreatorCashFlow,
                "outgoing token transfers from the creator's deployer or fee-recipient \
                 address that were not a decoded sale on the curve -- a count of transfers, \
                 never a count of sales",
                f64::from(count),
                rendered.clone(),
            )
            .saying(
                Voice::Plain,
                format!(
                    "The creator's address also sent {rendered} outgoing token transfers that \
                     were not decoded sales on the curve."
                ),
            )
            .saying(
                Voice::Blunt,
                format!("Plus {rendered} outgoing transfers -- transfers, not sales."),
            ),
        );
    }
}

/// The venue's own fee, which is not the cost of trading and says so.
fn push_fee(facts: &mut Vec<Fact>, curve: &realorrug_onchain::CurveFacts) {
    if let Some(fees) = &curve.fees {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a basis-point figure is a small integer; this is a comparison value"
        )]
        let rt = fees.round_trip_bps() as f64;
        facts.push(
            Fact {
                about: About::Measurement,
                kind: Kind::VenueFee,
                label: "venue fee, round trip, read from the on-chain schedule".to_owned(),
                // The rendered line is shown verbatim by the template, so it
                // carries the qualifier and nothing else: an instruction to the
                // model here was printed in public replies, and the 850 bps it
                // quoted is `push_cost`'s fresh-launch figure, which the reply
                // already states beside a different measured round trip.
                rendered: format!(
                    "{rt} bps, the venue's own fee and not the whole cost of a round trip"
                ),
                values: vec![rt, rt / 100.0],
                clauses: Vec::new(),
            }
            // "the venue's fee" and "the cost of trading" are different claims
            // and the gap between them is most of the cost. The qualifier is in
            // the sentence rather than in a rule the model is asked to hold.
            .saying(
                Voice::Plain,
                format!("The venue's own round-trip fee, from its on-chain schedule, is {rt} bps, which is not the cost of trading it."),
            )
            .saying(
                Voice::Blunt,
                format!("Venue fee alone: {rt} bps round trip. That is the floor, not the bill."),
            ),
        );
    }
}

fn push_cost(facts: &mut Vec<Fact>, round_trip: &crate::baserates::RoundTrip) {
    let kernel = format!("{} bps", round_trip.kernel);
    facts.push(
        Fact::exact(
            Kind::RoundTripKernel,
            "measured all-in round trip Real or Rug's kernel assumes, on fresh launches",
            round_trip.kernel,
            kernel.clone(),
        )
        .saying(
            Voice::Plain,
            format!("A round trip on a fresh launch costs {kernel} all in, as Real or Rug's kernel measures it."),
        )
        .saying(Voice::Blunt, format!("{kernel} to get in and out, all in.")),
    );
    let bar = format!("{} bps", round_trip.bar);
    facts.push(
        Fact::exact(
            Kind::RoundTripBar,
            "expected edge a strategy must clear before one trade is worth making",
            round_trip.bar,
            bar.clone(),
        )
        .saying(
            Voice::Plain,
            format!(
                "A strategy has to clear {bar} of expected edge before one trade is worth making."
            ),
        )
        .saying(
            Voice::Blunt,
            format!("Clear {bar} of edge or do not trade."),
        ),
    );
    for band in &round_trip.cost_bands {
        let size = &band.band;
        let rendered = format!("{} bps ({:.1}%)", band.round_trip, band.round_trip / 100.0);
        facts.push(
            Fact {
                about: About::Measurement,
                kind: Kind::CostBand,
                label: format!("round trip for a position of {size}"),
                rendered: rendered.clone(),
                values: vec![band.round_trip, band.round_trip / 100.0],
                clauses: Vec::new(),
            }
            .saying(
                Voice::Plain,
                format!("A position of {size} pays {rendered} to go round."),
            )
            .saying(Voice::Blunt, format!("{size} costs {rendered} round trip.")),
        );
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "quote units are converted only for a comparison against a literal the model \
              wrote; the rendered figure comes from integer arithmetic in `render_quote`"
)]
fn quote_as_f64(units: u128, decimals: u8) -> f64 {
    let scale = 10u128.pow(u32::from(decimals));
    units as f64 / scale as f64
}

/// Renders a quote-asset amount by integer arithmetic, to four decimal places.
///
/// `realorrug-types` keeps money integral on purpose, and a printed figure that has
/// silently rounded through a float is exactly what this account must not
/// publish. This is the one path both chains render through: Solana's SOL (9
/// decimals) and Robinhood's ETH (18 decimals) differ only in `decimals`, not
/// in the arithmetic.
fn render_quote(units: u128, decimals: u8) -> String {
    let scale = 10u128.pow(u32::from(decimals));
    format!(
        "{}.{:04}",
        units / scale,
        (units % scale) / (scale / 10_000)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_precision_switch_is_exactly_where_0024_needs_it() {
        // The rule is `> 0.0 && < 0.1` renders two decimals, everything else
        // one. Both comparisons are one character from being wrong and every
        // mutation of them survived, because nothing tested either edge.
        //
        // What is at stake is 0024's strongest finding: one to three recipients
        // graduate instantly 0.02% of the time. At one decimal that is "0.0%",
        // which reads as *never* rather than *rare* -- wrong in the direction
        // that gets quoted back at you.
        assert_eq!(
            Fact::share(Kind::LaunchRecipients, "x", 0.000_2).rendered,
            "0.02%"
        );
        assert_eq!(
            Fact::share(Kind::LaunchRecipients, "x", 0.000_5).rendered,
            "0.05%"
        );

        // Zero is not "0.00%". It is genuinely zero, and the two-decimal form is
        // for small-but-real, so the lower bound is exclusive.
        assert_eq!(
            Fact::share(Kind::LaunchRecipients, "x", 0.0).rendered,
            "0.0%"
        );

        // And a tenth of a percent is the upper bound, also exclusive: 0.1% has
        // no hidden precision to show.
        assert_eq!(
            Fact::share(Kind::LaunchRecipients, "x", 0.001).rendered,
            "0.1%"
        );

        // Ordinary magnitudes are unaffected.
        assert_eq!(
            Fact::share(Kind::LaunchRecipients, "x", 0.251).rendered,
            "25.1%"
        );
    }

    #[test]
    fn every_unreadable_fact_names_which_fact_it_was() {
        // Each arm is a different sentence in a published reply. Deleting any of
        // them falls through to the generic phrase, which is safe but says less,
        // and nothing noticed -- three arms, three survivors.
        assert_eq!(
            phrase_for("launch block"),
            "the launch block could not be read"
        );
        assert_eq!(phrase_for("curve"), "the bonding curve could not be read");
        assert_eq!(
            phrase_for("creator history"),
            "the creator's history could not be read"
        );
        // The fallback is deliberately the safe one, for a fact added later by
        // someone who did not read the comment above it.
        assert_eq!(
            phrase_for("creator cash flow"),
            "the creator's own buys and sells were not checked"
        );
        assert_eq!(
            phrase_for("something new"),
            "part of this could not be read"
        );
    }

    #[test]
    fn quote_units_convert_at_the_documented_rate() {
        // Used only to compare against a figure a model wrote, which is why it
        // is a float at all -- but a wrong conversion there authorises a wrong
        // number in a public reply. Covers both chains' decimals so the
        // exponent itself (`10u128.pow(decimals)`) is under test, not just the
        // division.
        assert!((quote_as_f64(1_000_000_000, 9) - 1.0).abs() < 1e-9);
        assert!((quote_as_f64(500_000_000, 9) - 0.5).abs() < 1e-9);
        assert!((quote_as_f64(3_000_000_000, 9) - 3.0).abs() < 1e-9);
        assert!((quote_as_f64(0, 9) - 0.0).abs() < 1e-9);
        assert!((quote_as_f64(1_000_000_000_000_000_000, 18) - 1.0).abs() < 1e-9);
        assert!((quote_as_f64(2_500_000_000_000_000_000, 18) - 2.5).abs() < 1e-9);
    }

    #[test]
    fn a_share_authorises_its_ordinary_roundings_and_nothing_else() {
        let f = Fact::share(Kind::LaunchRecipients, "x", 0.251);
        assert!(f.values.iter().any(|v| (*v - 0.251).abs() < 1e-9));
        assert!(f.values.iter().any(|v| (*v - 25.1).abs() < 1e-9));
        assert!(f.values.iter().any(|v| (*v - 25.0).abs() < 1e-9));
        // Not a number a reader would call a rounding of 25.1%.
        assert!(!f.values.iter().any(|v| (*v - 68.0).abs() < 1e-9));
        assert_eq!(f.rendered, "25.1%");
    }

    /// A minimal sheet with exactly one fact, one miss and a read point, so
    /// `render`'s whole output can be pinned without a fixture that grows.
    fn pinnable(read_at: realorrug_types::ReadAt) -> FactSheet {
        FactSheet {
            mint: "MintOne".to_owned(),
            read_at: Some(read_at),
            facts: vec![Fact::exact(
                Kind::LaunchRecipients,
                "distinct token accounts receiving the token in its own launch block",
                11.0,
                "11",
            )],
            untrusted: Vec::new(),
            unknown: vec!["the bonding curve could not be read".to_owned()],
            signals: Vec::new(),
            twins: Vec::new(),
            skipped: Vec::new(),
        }
    }

    /// The hours are arithmetic, and arithmetic is where a mutation hides.
    ///
    /// 63954 slots x 400ms = 25581.6s; over 3600 that is 7.106 hours, which
    /// rounds to one decimal as 7.1. Every operator in that line has a
    /// mutant, and each one lands on a different number -- 381.6, 1.7, 0.1 --
    /// so pinning the rendered string and the two authorised values catches
    /// all of them at once. It also pins the contract the reply depends on:
    /// the digits a model may cite are the digits this fact declared.
    #[test]
    fn the_age_fact_pins_the_hours_it_declares() {
        let mut facts = Vec::new();
        push_age(&mut facts, SlotDelta(63_954));
        assert_eq!(facts.len(), 1);
        assert_eq!(
            facts[0].rendered,
            "63954 slots (about 7.1 hours) since its launch block"
        );
        assert_eq!(facts[0].values, vec![63_954.0, 7.1]);
    }

    #[test]
    fn a_solana_sheet_renders_byte_for_byte() {
        // The whole string, not a `contains`. `voice::write` hands the model
        // the mint and this rendering and nothing else, so every byte here is
        // the model's entire world -- a line silently added or dropped
        // changes what the account is able to say, and no other test in this
        // crate would notice. Pinned whole for that reason; when this
        // assertion fails, read the diff and decide whether the new line
        // belongs, rather than weakening it to a `contains`.
        assert_eq!(
            pinnable(realorrug_types::ReadAt::Solana(realorrug_types::Slot(
                444_007_820
            )))
            .render(),
            [
                "distinct token accounts receiving the token in its own launch block: 11",
                "NOT KNOWN: the bonding curve could not be read",
                "read at: slot 444007820",
                "",
            ]
            .join(
                "
"
            )
        );
    }

    #[test]
    fn a_robinhood_sheet_carries_its_block_into_the_render_the_template_and_authorised() {
        // All three surfaces, because the read point reaches the reader by
        // three different routes and the Robinhood arm of each was the gap:
        // `render` is the only channel to the model, `template` is what ships
        // when the model's reply is refused, and `authorised` is what lets a
        // reply state the number at all. A block number must never wear a
        // slot's label on any of them -- `ReadAt`'s `Display` is the single
        // spelling, which is why "block 100" appears here and nowhere else.
        let sheet = pinnable(realorrug_types::ReadAt::Robinhood(100));

        assert!(
            sheet.render().contains("read at: block 100"),
            "{}",
            sheet.render()
        );
        assert!(
            !sheet.render().contains("slot"),
            "a block number must not be labelled a slot: {}",
            sheet.render()
        );

        let template = crate::verdict::template(&sheet);
        assert!(template.contains("Read at block 100."), "{template}");

        assert!(
            sheet
                .authorised()
                .iter()
                .any(|a| (a.value - 100.0).abs() < 1e-9)
        );
    }

    #[test]
    fn quote_renders_by_integer_arithmetic() {
        assert_eq!(render_quote(1_000_000_000, 9), "1.0000");
        assert_eq!(render_quote(303_000_000, 9), "0.3030");
        assert_eq!(render_quote(0, 9), "0.0000");
        // A different exponent must produce a different answer, so a mutant
        // that flips `decimals` or the `.pow` call is caught here rather than
        // only by 9-decimals cases where a nearby wrong exponent could
        // coincidentally still read plausible.
        assert_eq!(render_quote(1_000_000_000_000_000_000, 18), "1.0000");
        assert_eq!(render_quote(2_500_000_000_000_000_000, 18), "2.5000");
    }

    #[test]
    fn an_untrusted_name_is_never_a_fact() {
        // The separation this type exists for: a number inside a token's *name*
        // must authorise nothing, or a creator could licence their own figures
        // by putting them in the name.
        let sheet = FactSheet {
            mint: "M".to_owned(),
            read_at: None,
            facts: Vec::new(),
            untrusted: vec![("token name".to_owned(), "99999 percent safe".to_owned())],
            unknown: Vec::new(),
            signals: Vec::new(),
            twins: Vec::new(),
            skipped: Vec::new(),
        };
        assert!(sheet.authorised().is_empty());
        assert!(!sheet.render().contains("99999"));
    }

    /// A market-cap fact, naming the moment it was read at as ADR 0033
    /// requires.
    fn a_price_fact() -> Fact {
        Fact {
            about: About::Price,
            kind: Kind::CurveLiquidity,
            clauses: Vec::new(),
            label: "market capitalisation, read at slot 1".to_owned(),
            rendered: "69000 USD".to_owned(),
            values: vec![69_000.0, 69.0],
        }
    }

    #[test]
    fn every_authorised_number_carries_the_measurement_it_came_from() {
        // The hole ADR 0031 closes. `authorised` used to scan the whole
        // rendering in one pass and return bare values, so the largest
        // holder's 41 was licensed anywhere in the reply -- including inside
        // "the creator already dumped 41%".
        //
        // Re-apply the bug by scanning `self.render()` instead of each fact's
        // own line: 41 gains an `Anywhere` entry and the second assertion
        // fails, because `Anywhere` is citable in a sentence about anybody.
        let sheet = FactSheet {
            mint: "M".to_owned(),
            read_at: None,
            facts: vec![
                Fact::exact(Kind::LargestHolderShare, "largest holder", 41.0, "41%"),
                Fact::exact(Kind::CreatorLaunches, "launches by this creator", 3.0, "3"),
            ],
            untrusted: Vec::new(),
            unknown: Vec::new(),
            signals: Vec::new(),
            twins: Vec::new(),
            skipped: Vec::new(),
        };

        let authorised = sheet.authorised();
        let for_41: Vec<Subject> = authorised
            .iter()
            .filter(|a| (a.value - 41.0).abs() < 1e-9)
            .map(|a| a.subject)
            .collect();
        assert!(!for_41.is_empty(), "41 must still be publishable at all");
        assert!(
            for_41.iter().all(|s| *s == Subject::Holders),
            "41 was measured about the holders and nothing else: {for_41:?}"
        );

        // And the check built on it refuses the sentence that moved it.
        let fabricated = crate::fidelity::check("The creator already dumped 41%.", &authorised);
        assert_eq!(fabricated.len(), 1, "{fabricated:?}");
    }

    #[test]
    fn a_price_fact_is_kept_and_stays_authorised() {
        // ADR 0033 supersedes ADR 0013 constraint 5: `FactSheet::build` no
        // longer drops `About::Price` facts for the self mint. Re-apply the
        // withholding this replaces by filtering `facts` on `About::Price`
        // before the sheet is built: the 69000 leaves the authorised set and
        // the second assertion fails.
        let facts = vec![
            Fact::exact(Kind::LaunchRecipients, "recipients", 6.0, "6"),
            a_price_fact(),
        ];
        let sheet = FactSheet {
            mint: "M".to_owned(),
            read_at: None,
            facts,
            untrusted: Vec::new(),
            unknown: Vec::new(),
            signals: Vec::new(),
            twins: Vec::new(),
            skipped: Vec::new(),
        };

        assert!(
            sheet.facts.iter().any(|f| f.about == About::Price),
            "the price fact was dropped: {:?}",
            sheet.facts
        );
        let authorised = sheet.authorised();
        assert!(
            authorised.iter().any(|a| (a.value - 69_000.0).abs() < 1e-9),
            "the market cap did not stay authorised: {authorised:?}"
        );
        assert!(
            authorised.iter().any(|a| (a.value - 6.0).abs() < 1e-9),
            "an unrelated measured fact was lost: {authorised:?}"
        );
    }

    #[test]
    fn a_price_facts_label_names_its_read_point() {
        // ADR 0033: "every price or market cap carries the block or time it
        // was read at" -- a price without its moment is a stale price that
        // looks current. Checked on the fact's own label rather than the
        // sheet-wide read line, because a reply may select and quote one
        // fact without the rest of the sheet.
        let fact = a_price_fact();
        assert_eq!(fact.about, About::Price);
        assert!(
            fact.label.contains("slot") || fact.rendered.contains("slot"),
            "a price fact must name the block or time it was read at: {fact:?}"
        );
    }

    #[test]
    fn a_graduated_coin_still_gets_its_creators_history_and_says_where_it_trades() {
        // The shape of every coin worth asking about: enough signatures that
        // `oldest_launch` refuses to guess a launch block, and a curve that has
        // graduated. Until 2026-09-06 this produced the worst sheet in the
        // system -- no creator history at all, because the lookup sat inside
        // the launch-block arm, and "exit capacity: none, cannot size into this
        // at all" about a coin trading perfectly well on an AMM.
        //
        // Re-apply either half to see this fail: move the `creator` lookup back
        // inside `if let Some(launch)`, or delete `push_curve`'s early return.
        let mut d = dossier_for([7u8; 32]);
        d.launch = None;
        d.curve = Some(realorrug_onchain::CurveFacts {
            creator: realorrug_types::ChainAddress::Solana(realorrug_types::Address::new(
                [9u8; 32],
            )),
            complete: true,
            quote_reserves: 0,
            quote_capacity: None,
            quote_asset: None,
            fees: None,
        });
        d.unavailable.push(realorrug_onchain::dossier::Unavailable {
            fact: "launch block",
            why: "this token has more history than the page budget allows".to_owned(),
        });

        let sheet = FactSheet::build(&d, None, Some(&index_with(record(150, 0))), None, None);
        let rendered = sheet.render();

        // The creator's record survived the missing launch block.
        assert!(rendered.contains("150"), "no creator history: {rendered}");
        assert!(
            sheet
                .signals
                .contains(&Signal::CreatorNeverGraduatedOrganically),
            "the signal was lost with the launch block: {:?}",
            sheet.signals
        );
        // And the curve says where the coin went rather than that it is stuck.
        assert!(rendered.contains("trades on the AMM"), "{rendered}");
        // The claim, not the word: the replacement line names the old phrasing
        // in order to forbid it, so a bare substring would match itself.
        assert!(
            !rendered.contains("none -- cannot size into this at all"),
            "{rendered}"
        );
        assert!(!rendered.contains("venue fee"), "{rendered}");
    }

    /// A dossier about one mint and nothing else, so what the sheet says is
    /// decided by the mint alone.
    fn dossier_for(mint: [u8; 32]) -> Dossier {
        Dossier {
            mint: realorrug_types::ChainAddress::Solana(realorrug_types::Address::new(mint)),
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

    /// A launch block with the three things the signals read.
    fn launch(
        recipients: realorrug_onchain::budget::Count,
        dev_buy_lamports: Option<u64>,
    ) -> realorrug_onchain::launch::LaunchBlock {
        realorrug_onchain::launch::LaunchBlock {
            slot: Slot(444_007_820),
            creator: realorrug_types::Address::new([9u8; 32]),
            recipients,
            transactions: realorrug_onchain::budget::Count::Exactly(4),
            dev_buy_lamports,
            metadata: realorrug_onchain::launch::Metadata {
                name: "x".to_owned(),
                symbol: "X".to_owned(),
                uri: String::new(),
            },
        }
    }

    /// A snapshot whose strongest band is `lo..=hi`, beside a weaker one at
    /// one to three so that "strongest" is a comparison and not the only row.
    fn rates_strongest(lo: u32, hi: u32) -> BaseRates {
        let band = |name: &str, lo, hi, x| crate::baserates::Band {
            name: name.to_owned(),
            lo,
            hi,
            fires_on: 0.0,
            never_graduated: 0.0,
            organic: 0.0,
            instant: 0.0,
            p_instant: 0.0,
            x_base_instant: x,
        };
        BaseRates {
            chain: crate::firstparty::Chain::Solana,
            measured_on: "2026-09-03".to_owned(),
            aftermath: None,
            outcomes_24h: None,
            launches: 1,
            base_rate_graduates: 0.0,
            base_rate_instant: 0.0,
            bands: vec![
                band("one to three", 1, 3, 0.0),
                band("strong", lo, hi, 10.1),
            ],
            round_trip: None,
        }
    }

    fn index_with(record: crate::creator::Record) -> crate::creator::CreatorIndex {
        let mut creators = std::collections::BTreeMap::new();
        creators.insert(realorrug_types::Address::new([9u8; 32]).to_string(), record);
        crate::creator::CreatorIndex {
            chain: crate::firstparty::Chain::Solana,
            watermark_slot: 444_343_109,
            built_at: 1_788_000_000,
            population: None,
            creators,
        }
    }

    fn record(measured: u32, organic: u32) -> crate::creator::Record {
        crate::creator::Record {
            launches: measured + 1,
            measured,
            organic,
            instant: measured.saturating_sub(organic),
            stillborn: 0,
        }
    }

    #[test]
    fn a_launch_count_with_no_outcomes_yet_is_stated_and_does_not_blank_the_verdict() {
        // The normal state of a new index. Walking the factory's launch logs
        // costs tens of calls; measuring each launch's outcome costs one call
        // per token, about 171,000 of them (research 0038 4). So an index that
        // knows launch counts and no outcomes is what exists first, and for a
        // while.
        //
        // The count is the whole of what `Signal::RepeatLauncher` needs. Before
        // this, the missing outcomes went into `unknown`, which `verdict::level`
        // reads as "a required fact is unread" and answers `CantTell` -- so
        // seeing that a launcher had forty-one launches to their name made the
        // bot less able to speak, not more.
        let mut dossier = dossier_for([9u8; 32]);
        dossier.launch = Some(launch(
            realorrug_onchain::budget::Count::Exactly(12),
            Some(30_000_000),
        ));
        let index = index_with(crate::creator::Record {
            launches: 41,
            measured: 0,
            organic: 0,
            instant: 0,
            stillborn: 0,
        });
        let sheet = FactSheet::build(&dossier, None, Some(&index), None, None);

        assert!(
            !sheet
                .unknown
                .iter()
                .any(|u| u.contains("outcome") || u.contains("measured")),
            "an unmeasured outcome must not reach the list that answers CantTell: {:?}",
            sheet.unknown
        );
        // Still said, though -- in the same breath as the number it qualifies,
        // which is the one place a model quoting the count cannot drop it.
        let rendered = sheet.render();
        assert!(
            rendered.contains("41"),
            "the launch count itself must survive: {rendered}"
        );
        assert!(
            rendered.contains("none with an outcome measured yet"),
            "the limit must be stated beside the count, in the text the model is shown: {rendered}"
        );
    }

    #[test]
    fn the_three_signals_fire_together_in_a_fixed_order_and_the_model_sees_none_of_them() {
        // Design 0009 M3. Twelve recipients in a snapshot whose strongest band
        // is ten to thirteen; a dev buy that was seen; a creator with three
        // measured launches and no organic graduation.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(
            realorrug_onchain::budget::Count::Exactly(12),
            Some(30_000_000),
        ));
        let rates = rates_strongest(10, 13);
        let index = index_with(record(3, 0));
        let sheet = FactSheet::build(&dossier, Some(&rates), Some(&index), None, None);
        assert_eq!(
            sheet.signals,
            [
                Signal::LaunchBlockInStrongestBand,
                Signal::CreatorBoughtOwnLaunch,
                Signal::CreatorNeverGraduatedOrganically,
            ]
        );
        // Not rendered. The model is shown facts, and the word would be a
        // verdict handed to it.
        let rendered = sheet.render();
        assert!(!rendered.to_lowercase().contains("signal"), "{rendered}");
        // The twins line up with the signals, same order, one each --
        // packet 0038.
        assert_eq!(
            sheet.twins,
            [
                twin_for(Signal::LaunchBlockInStrongestBand).to_owned(),
                twin_for(Signal::CreatorBoughtOwnLaunch).to_owned(),
                twin_for(Signal::CreatorNeverGraduatedOrganically).to_owned(),
            ]
        );
    }

    #[test]
    fn measured_launches_above_five_raises_creator_never_graduated_factor() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(1), None));
        let index = index_with(record(6, 0));
        let sheet = FactSheet::build(&dossier, None, Some(&index), None, None);
        assert!(
            sheet
                .signals
                .contains(&Signal::CreatorNeverGraduatedOrganically)
        );
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::CreatorNeverGraduatedOrganically,
                name: "measured launches >= 5".to_owned(),
                delta_bps: 400,
                grade: Grade::Measured,
                evidence: "6 of this creator's launches have been measured".to_owned(),
            }]
        );
    }

    #[test]
    fn measured_launches_at_or_below_two_lowers_creator_never_graduated_factor() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(1), None));
        let index = index_with(record(2, 0));
        let sheet = FactSheet::build(&dossier, None, Some(&index), None, None);
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::CreatorNeverGraduatedOrganically,
                name: "measured launches <= 2, thin denominator".to_owned(),
                delta_bps: -400,
                grade: Grade::Measured,
                evidence: "only 2 of this creator's launches have been measured".to_owned(),
            }]
        );
    }

    #[test]
    fn between_the_two_measured_thresholds_no_factor_fires() {
        // Three measured launches: past the thin-sample lower (<= 2) and
        // short of the raise (>= 5). Research 0052 §3.1 names both edges and
        // nothing in between, so the middle stays base-weight.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(1), None));
        let index = index_with(record(3, 0));
        let sheet = FactSheet::build(&dossier, None, Some(&index), None, None);
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    /// research 0052 §6 case B, replayed through `factors` alone (the
    /// noisy-OR score is `assessment.rs`'s job): 800 bps, one wallet, said
    /// nothing. `1,200 + 800 = 2,000` is checked there; this pins the `+800`
    /// half of that arithmetic to the exact fact this packet wires.
    #[test]
    fn share_of_500_bps_raises_creator_bought_own_launch_factor_by_800() {
        let dossier =
            robinhood_launch_with_share(Some(3_600), Some(1), Some(500), Some(10_000), None, None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(sheet.signals.contains(&Signal::CreatorBoughtOwnLaunch));
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::CreatorBoughtOwnLaunch,
                name: "own-launch share >= 500 bps".to_owned(),
                delta_bps: 800,
                grade: Grade::Measured,
                evidence: "the launcher's launch-transaction buy is 5.00% of total supply"
                    .to_owned(),
            }]
        );
    }

    /// One bps short of the 500 boundary: 499/1,000 = 499 bps must not
    /// raise the factor. Paired with the test above so a mutant that moves
    /// the `500` bound either up or down is caught on one side or the other.
    #[test]
    fn share_of_499_bps_does_not_raise_the_500_bps_factor() {
        let dossier =
            robinhood_launch_with_share(Some(3_600), Some(1), Some(499), Some(10_000), None, None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    #[test]
    fn share_of_1000_bps_raises_creator_bought_own_launch_factor_by_1500() {
        let dossier = robinhood_launch_with_share(
            Some(3_600),
            Some(1),
            Some(1_000),
            Some(10_000),
            None,
            None,
        );
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::CreatorBoughtOwnLaunch,
                name: "own-launch share >= 1,000 bps".to_owned(),
                delta_bps: 1_500,
                grade: Grade::Measured,
                evidence: "the launcher's launch-transaction buy is 10.00% of total supply"
                    .to_owned(),
            }]
        );
    }

    /// One bps short of the 1,000 boundary: it must land in the `>= 500`
    /// arm (`+800`), not the `>= 1,000` arm (`+1,500`) and not fall through
    /// to no factor -- pinning both edges of the middle band at once.
    #[test]
    fn share_of_999_bps_lands_in_the_500_bps_band_not_the_1000_bps_band() {
        let dossier =
            robinhood_launch_with_share(Some(3_600), Some(1), Some(999), Some(10_000), None, None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::CreatorBoughtOwnLaunch,
                name: "own-launch share >= 500 bps".to_owned(),
                delta_bps: 800,
                grade: Grade::Measured,
                evidence: "the launcher's launch-transaction buy is 9.99% of total supply"
                    .to_owned(),
            }]
        );
    }

    /// research 0052 §6 case A's S1 half, replayed through `factors` alone:
    /// 50 bps share lowers the factor by 400 (`1,200 - 400 = 800` before the
    /// case's own self-reported `-200`, which this packet does not build).
    #[test]
    fn share_of_50_bps_lowers_creator_bought_own_launch_factor_by_400() {
        let dossier =
            robinhood_launch_with_share(Some(3_600), Some(1), Some(50), Some(10_000), None, None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::CreatorBoughtOwnLaunch,
                name: "own-launch share < 100 bps".to_owned(),
                delta_bps: -400,
                grade: Grade::Measured,
                evidence: "the launcher's launch-transaction buy is only 0.50% of total supply"
                    .to_owned(),
            }]
        );
    }

    /// One bps short of the 100-bps lower boundary from the other side: 99
    /// of 10,000 = 99 bps must still lower the factor. Paired with the test
    /// below so a mutant moving the `100` bound is caught either way.
    #[test]
    fn share_of_99_bps_still_lowers_the_factor() {
        let dossier =
            robinhood_launch_with_share(Some(3_600), Some(1), Some(99), Some(10_000), None, None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let found = factors(&sheet);
        assert_eq!(found[0].delta_bps, -400, "{found:?}");
    }

    /// Exactly 100 bps: past the `< 100` lower and short of the `>= 500`
    /// raise, so research 0052 §3.1's S1 row fires no factor at all here.
    #[test]
    fn share_of_exactly_100_bps_fires_no_factor() {
        let dossier =
            robinhood_launch_with_share(Some(3_600), Some(1), Some(100), Some(10_000), None, None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    /// A missing supply is absent, not zero (rule 8): no share fact is
    /// pushed, so no factor can fire even though `CreatorBoughtOwnLaunch`
    /// itself still does (a nonzero `dev_buy_wei` was seen).
    #[test]
    fn missing_supply_fires_no_share_factor() {
        let dossier =
            robinhood_launch_with_share(Some(3_600), Some(1), Some(500), None, None, None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(sheet.signals.contains(&Signal::CreatorBoughtOwnLaunch));
        assert!(fact_of(&sheet, Kind::DevBuyShare).is_none());
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    /// A supply of zero would divide by zero; guarded, not a panic and not
    /// a share of infinity.
    #[test]
    fn zero_supply_fires_no_share_factor() {
        let dossier =
            robinhood_launch_with_share(Some(3_600), Some(1), Some(500), Some(0), None, None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::DevBuyShare).is_none());
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    /// A sheet built directly with [`Signal::HolderConcentration`] already
    /// fired, the way a test has to today: [`FactSheet::build`] never pushes
    /// it yet (its own threshold is not measured for Pons v2, design 0020
    /// §3), but `factors` must still grade a [`Kind::LargestHolderShare`]
    /// fact once the signal is on the sheet.
    fn sheet_with_largest_holder_share(bps: u32) -> FactSheet {
        FactSheet {
            mint: "MintOne".to_owned(),
            read_at: None,
            facts: vec![Fact::share(
                Kind::LargestHolderShare,
                "share of the supply outside the curve held by the single largest address",
                f64::from(bps) / 10_000.0,
            )],
            untrusted: Vec::new(),
            unknown: Vec::new(),
            signals: vec![Signal::HolderConcentration],
            twins: vec![String::new()],
            skipped: Vec::new(),
        }
    }

    /// research 0052 §3.1's S5 row, top raise: a largest holder at or above
    /// 2,000 bps raises `HolderConcentration` by 1,000.
    #[test]
    fn largest_holder_share_of_2000_bps_raises_holder_concentration_factor_by_1000() {
        let sheet = sheet_with_largest_holder_share(2_000);
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::HolderConcentration,
                name: "largest holder >= 2,000 bps".to_owned(),
                delta_bps: 1_000,
                grade: Grade::Measured,
                evidence: "the largest single address holds 20.00% of the supply outside the \
                           curve"
                    .to_owned(),
            }]
        );
    }

    /// One bps short of the 2,000-bps boundary: it must land in the
    /// `>= 1,000` arm (`+600`), not the `>= 2,000` arm (`+1,000`), pinning
    /// the boundary from the raise side.
    #[test]
    fn largest_holder_share_of_1999_bps_lands_in_the_1000_bps_band_not_the_2000_bps_band() {
        let sheet = sheet_with_largest_holder_share(1_999);
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::HolderConcentration,
                name: "largest holder >= 1,000 bps".to_owned(),
                delta_bps: 600,
                grade: Grade::Measured,
                evidence: "the largest single address holds 19.99% of the supply outside the \
                           curve"
                    .to_owned(),
            }]
        );
    }

    /// research 0052 §3.1's S5 row, lower raise: a largest holder at or
    /// above 1,000 bps raises `HolderConcentration` by 600.
    #[test]
    fn largest_holder_share_of_1000_bps_raises_holder_concentration_factor_by_600() {
        let sheet = sheet_with_largest_holder_share(1_000);
        let found = factors(&sheet);
        assert_eq!(found[0].delta_bps, 600, "{found:?}");
    }

    /// One bps short of the 1,000-bps boundary: no factor fires at all, so
    /// `HolderConcentration` stays at base weight. Paired with the test
    /// above so a mutant moving the `1,000` bound is caught either way.
    #[test]
    fn largest_holder_share_of_999_bps_fires_no_factor() {
        let sheet = sheet_with_largest_holder_share(999);
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    #[test]
    fn one_linked_seller_says_nothing_and_fires_nothing() {
        let dossier = robinhood_launch_with_correlated_selling(1, Some(5_000), Some(0));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(!sheet.signals.contains(&Signal::CorrelatedSelling));
        assert!(fact_of(&sheet, Kind::CorrelatedSellWallets).is_none());
        assert!(fact_of(&sheet, Kind::CorrelatedSellVolumeBps).is_none());
    }

    #[test]
    fn unread_sells_say_nothing_and_fire_nothing() {
        let mut dossier = robinhood_launch_with_correlated_selling(3, Some(5_000), Some(0));
        if let Some(cs) = dossier
            .chain_launch
            .as_mut()
            .and_then(|launch| launch.correlated_selling.as_mut())
        {
            cs.sells_read = false;
        }
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(!sheet.signals.contains(&Signal::CorrelatedSelling));
        assert!(fact_of(&sheet, Kind::CorrelatedSellWallets).is_none());
    }

    /// research 0052 §3.1's S7 row: 2 linked sellers is not yet a cluster
    /// worth raising over (the signal itself needs `>= 2` to fire at all,
    /// but the `+800` factor's own threshold is `>= 3`) -- no factor fires.
    #[test]
    fn two_linked_sellers_does_not_raise_the_wallet_count_factor() {
        let dossier = robinhood_launch_with_correlated_selling(2, None, None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(sheet.signals.contains(&Signal::CorrelatedSelling));
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    /// Exactly 3 linked sellers clears research 0052 §3.1's S7 wallet-count
    /// boundary. Paired with the test above so a mutant moving the `3` bound
    /// either way is caught on one side or the other.
    #[test]
    fn three_linked_sellers_raises_the_wallet_count_factor_by_800() {
        let dossier = robinhood_launch_with_correlated_selling(3, None, None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::CorrelatedSelling,
                name: "linked sellers >= 3 within the window".to_owned(),
                delta_bps: 800,
                grade: Grade::Measured,
                evidence: "3 linked-at-buy wallets sold within the same 50-block window".to_owned(),
            }]
        );
    }

    /// 999 of 10,000 bps (9.99% of supply) is one bps short of research
    /// 0052 §3.1's S7 volume boundary: no volume factor fires.
    #[test]
    fn sold_volume_of_999_bps_does_not_raise_the_volume_factor() {
        let dossier = robinhood_launch_with_correlated_selling(2, Some(999), None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    /// Exactly 1,000 bps of supply sold clears the S7 volume boundary.
    /// Paired with the test above so a mutant moving the `1_000` bound
    /// either way is caught on one side or the other.
    #[test]
    fn sold_volume_of_1000_bps_raises_the_volume_factor_by_600() {
        let dossier = robinhood_launch_with_correlated_selling(2, Some(1_000), None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::CorrelatedSelling,
                name: "sold volume >= 1,000 bps of supply".to_owned(),
                delta_bps: 600,
                grade: Grade::Measured,
                evidence: "the cluster sold 10.00% of total supply".to_owned(),
            }]
        );
    }

    /// Exactly 1 hour (3,600 seconds) is still within research 0052 §3.1's
    /// S7 "spread over > 1 hour" wording: the lower factor must not fire yet.
    #[test]
    fn a_spread_of_exactly_one_hour_does_not_lower_the_spread_factor() {
        let dossier = robinhood_launch_with_correlated_selling(2, None, Some(3_600));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    /// One second past 1 hour clears the S7 spread boundary and lowers the
    /// factor. Paired with the test above so a mutant moving the `3_600`
    /// bound either way is caught on one side or the other.
    #[test]
    fn a_spread_of_one_hour_and_one_second_lowers_the_spread_factor_by_300() {
        let dossier = robinhood_launch_with_correlated_selling(2, None, Some(3_601));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::CorrelatedSelling,
                name: "sells spread over > 1 hour".to_owned(),
                delta_bps: -300,
                grade: Grade::Measured,
                evidence: "the cluster's sells spread over 3601 seconds".to_owned(),
            }]
        );
    }

    /// A reverted launch transaction reads `dev_buy_wei` and `dev_buy_tokens`
    /// as `Some(0)`, the same as `robinhood.rs`'s own reader does for a
    /// reverted receipt -- and 0 of any nonzero supply is 0 bps, so the
    /// factor lowers the same way an honestly-tiny buy would.
    #[test]
    fn a_reverted_launch_transactions_zero_dev_buy_does_not_raise_creator_bought_own_launch() {
        let dossier =
            robinhood_launch_with_share(Some(3_600), Some(0), Some(0), Some(10_000), None, None);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        // dev_buy_wei of 0 never sets the signal in the first place (a
        // buy that reads as zero is "did not buy", not "bought own
        // launch") -- so there is nothing for a share factor to attach to.
        assert!(
            !sheet.signals.contains(&Signal::CreatorBoughtOwnLaunch),
            "{:?}",
            sheet.signals
        );
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    #[test]
    fn ten_or_more_lifetime_launches_raises_repeat_launcher_factor() {
        let creator_address = realorrug_types::Address::new([9u8; 32]).to_string();
        let list = empty_first_party_list();
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(1), None));
        let index = index_with_floor_and_target(creator_address, 10, 5, 100);
        let sheet = FactSheet::build(&dossier, None, Some(&index), None, Some(&list));
        assert!(sheet.signals.contains(&Signal::RepeatLauncher));
        let found = factors(&sheet);
        assert_eq!(
            found,
            vec![Factor {
                signal: Signal::RepeatLauncher,
                name: "lifetime launches >= 10".to_owned(),
                delta_bps: 800,
                grade: Grade::Measured,
                evidence: "10 tokens this creator has launched, in Real or Rug's record".to_owned(),
            }]
        );
    }

    #[test]
    fn nine_lifetime_launches_does_not_raise_the_repeat_launcher_factor() {
        // The boundary's other side: one below ten fires the signal (the
        // floor here is five) but not the factor.
        let creator_address = realorrug_types::Address::new([9u8; 32]).to_string();
        let list = empty_first_party_list();
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(1), None));
        let index = index_with_floor_and_target(creator_address, 9, 5, 100);
        let sheet = FactSheet::build(&dossier, None, Some(&index), None, Some(&list));
        assert!(sheet.signals.contains(&Signal::RepeatLauncher));
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    /// M-D-0002's own fixture rubric: `the_live_robinhood_sheet` prints its
    /// factors with grades. It has none today. The only signal this fixture
    /// fires is `CreatorBoughtOwnLaunch` (a dev buy was seen in the launch
    /// transaction), but its `ChainLaunch` fixture carries `dev_buy_wei`
    /// alone -- `dev_buy_tokens` and `supply` are `None` -- so the share
    /// this factor now reads (dev-share, corrected 2026-09-18: from the
    /// launch receipt, no `eth_call`) has nothing to compute from. The
    /// fixture also passes no creator index, so the two factors this packet
    /// wires elsewhere (`RepeatLauncher`, `CreatorNeverGraduatedOrganically`)
    /// have no signal to attach to here either. An empty list is the honest
    /// answer, not a bug to paper over with an invented number.
    #[test]
    fn the_live_robinhood_sheet_prints_its_factors_with_grades() {
        let sheet = crate::verdict::tests::the_live_robinhood_sheet();
        let found = factors(&sheet);
        for factor in &found {
            println!(
                "{:?} {} {:+} bps ({:?}): {}",
                factor.signal, factor.name, factor.delta_bps, factor.grade, factor.evidence
            );
        }
        assert!(
            found.is_empty(),
            "no factor is wireable for this fixture yet -- see this test's doc comment: {found:?}"
        );
    }

    #[test]
    fn every_signal_variant_has_a_nonempty_digit_free_twin() {
        // The whole point of the exhaustive `match` with no `_ =>` arm: a
        // tenth signal added later has to gain a twin here before the crate
        // compiles again. This test does not catch that on its own (the
        // compiler does) -- it catches an empty string or a smuggled digit,
        // which the match's exhaustiveness cannot.
        for signal in [
            Signal::LaunchBlockInStrongestBand,
            Signal::CreatorNeverGraduatedOrganically,
            Signal::CreatorBoughtOwnLaunch,
            Signal::LiquidityGone,
            Signal::CreatorSoldOut,
            Signal::BuyersCannotSell,
            Signal::RepeatLauncher,
            Signal::HolderConcentration,
            Signal::OwnerCanStillMintOrPause,
            Signal::CorrelatedSelling,
        ] {
            let twin = twin_for(signal);
            assert!(!twin.is_empty(), "{signal:?} has an empty twin");
            assert!(
                !twin.chars().any(|c| c.is_ascii_digit()),
                "{signal:?}'s twin carries a digit, which `forbidden.rs` cannot source: {twin}"
            );
        }
    }

    /// Every variant, listed once for the two tests below. A new variant
    /// will not fail to compile against this array, but it will fail
    /// `Signal::plain`'s own exhaustive match, which is the cheaper guard.
    const EVERY_SIGNAL: [Signal; 10] = [
        Signal::LaunchBlockInStrongestBand,
        Signal::CreatorNeverGraduatedOrganically,
        Signal::CreatorBoughtOwnLaunch,
        Signal::LiquidityGone,
        Signal::CreatorSoldOut,
        Signal::BuyersCannotSell,
        Signal::RepeatLauncher,
        Signal::HolderConcentration,
        Signal::OwnerCanStillMintOrPause,
        Signal::CorrelatedSelling,
    ];

    #[test]
    fn every_signal_has_a_plain_phrase_no_other_signal_shares() {
        // The checks below pin what a phrase must look like; this pins that
        // there are ten of them. One body returning a single string for
        // every variant passes "non-empty, short, digit-free" perfectly and
        // draws a card whose three lines all say the same thing.
        let phrases: Vec<&str> = EVERY_SIGNAL.iter().map(|s| s.plain()).collect();
        let mut unique = phrases.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            EVERY_SIGNAL.len(),
            "two signals share a plain phrase, so the card would say the same \
             thing twice: {phrases:?}"
        );
    }

    #[test]
    fn every_signal_variant_has_a_short_nonempty_digit_free_plain_phrase() {
        // Same discipline as the twin test above, for `Signal::plain`: the
        // exhaustive match with no `_ =>` arm means a new variant fails to
        // compile without a phrase here; this test catches an empty, too
        // long, or digit-smuggling one, which exhaustiveness alone cannot.
        for signal in EVERY_SIGNAL {
            let plain = signal.plain();
            assert!(!plain.is_empty(), "{signal:?} has an empty plain phrase");
            assert!(
                plain.len() < 60,
                "{signal:?}'s plain phrase is {} chars, over the card's budget: {plain}",
                plain.len()
            );
            assert!(
                !plain.chars().any(|c| c.is_ascii_digit()),
                "{signal:?}'s plain phrase carries a digit, which the card must never draw: \
                 {plain}"
            );
        }
    }

    #[test]
    fn a_fired_signal_renders_its_twin_and_an_unfired_sheet_renders_none() {
        // Design 0020 §1: the twin belongs to the fact, phrased as a
        // sentence about what was read, and the model's whole world is
        // `render()` -- so this is the only way to prove the twin actually
        // reaches the prompt.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(12), None));
        let rates = rates_strongest(10, 13);
        let fired = FactSheet::build(&dossier, Some(&rates), None, None, None);
        assert_eq!(fired.signals, [Signal::LaunchBlockInStrongestBand]);
        let rendered = fired.render();
        assert!(
            rendered.contains(twin_for(Signal::LaunchBlockInStrongestBand)),
            "{rendered}"
        );

        // Same launch, but a snapshot whose strongest band this recipient
        // count misses: no signal, so no twin heading at all.
        let quiet = FactSheet::build(&dossier, Some(&rates_strongest(50, 60)), None, None, None);
        assert!(quiet.signals.is_empty());
        let rendered = quiet.render();
        assert!(!rendered.contains("INNOCENT EXPLANATIONS"), "{rendered}");
        assert!(quiet.twins.is_empty());
    }

    /// An index with `filler_count` creators at exactly `floor_value`
    /// launches each, plus one creator at `target_address` carrying
    /// `target_launches`.
    ///
    /// `filler_count` is chosen (100) so the filler block alone clears the
    /// refusal floor and dominates the 95th-percentile rank regardless of
    /// where the single target creator's own count sorts -- so this always
    /// measures a floor of exactly `floor_value`, and the test using it is
    /// only about whether `target_launches >= floor_value` fires the signal,
    /// not about the percentile arithmetic (that is `creator.rs`'s job).
    fn index_with_floor_and_target(
        target_address: String,
        target_launches: u32,
        floor_value: u32,
        filler_count: usize,
    ) -> crate::creator::CreatorIndex {
        let mut creators = std::collections::BTreeMap::new();
        for i in 0..filler_count {
            creators.insert(
                format!("filler{i}"),
                crate::creator::Record {
                    launches: floor_value,
                    ..crate::creator::Record::default()
                },
            );
        }
        creators.insert(
            target_address,
            crate::creator::Record {
                launches: target_launches,
                ..crate::creator::Record::default()
            },
        );
        crate::creator::CreatorIndex {
            chain: crate::firstparty::Chain::Solana,
            watermark_slot: 444_343_109,
            built_at: 1_788_000_000,
            population: None,
            creators,
        }
    }

    fn empty_first_party_list() -> crate::firstparty::FirstPartyList {
        crate::firstparty::FirstPartyList::parse(r#"{"entries": []}"#).expect("parses")
    }

    #[test]
    fn a_creator_at_or_above_the_floor_fires_repeat_launcher_and_one_below_does_not() {
        // 100 fillers at launches == 5 pin the floor at 5 regardless of the
        // target's own count (see `index_with_floor_and_target`). Four
        // values against that one floor: strictly below, exactly at it, and
        // above -- `>=` is what the sheet uses, so the boundary is asserted
        // both ways rather than only in the direction that flatters `>`.
        let creator_address = realorrug_types::Address::new([9u8; 32]).to_string();
        let list = empty_first_party_list();
        for (target_launches, fires) in [(4u32, false), (5u32, true), (6u32, true)] {
            let mut dossier = dossier_for([3u8; 32]);
            dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(1), None));
            let index =
                index_with_floor_and_target(creator_address.clone(), target_launches, 5, 100);
            let sheet = FactSheet::build(&dossier, None, Some(&index), None, Some(&list));
            assert_eq!(
                sheet.signals.contains(&Signal::RepeatLauncher),
                fires,
                "{target_launches} launches against a floor of 5"
            );
        }
    }

    #[test]
    fn no_named_list_fires_no_repeat_launcher_signal_even_far_above_any_floor() {
        // Deny by default (AGENTS.md rule 7): `first_party: None` is the same
        // "not configured, or did not parse" state whether the file was
        // absent or malformed -- both reach `FactSheet::build` as `None`, so
        // this one case covers both halves of "no list, or an unparseable
        // list, fires no signal."
        let creator_address = realorrug_types::Address::new([9u8; 32]).to_string();
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(1), None));
        // Far above any plausible floor, so a survivor that fires without a
        // list cannot hide behind "the count just happened to be too low."
        let index = index_with_floor_and_target(creator_address, 9_999, 5, 100);
        let sheet = FactSheet::build(&dossier, None, Some(&index), None, None);
        assert!(
            !sheet.signals.contains(&Signal::RepeatLauncher),
            "{:?}",
            sheet.signals
        );
    }

    #[test]
    fn under_a_hundred_remaining_creators_fires_no_repeat_launcher_signal() {
        // 40 creators is a real index with a real record, just too small for
        // the floor to mean anything (`creator.rs::repeat_launcher_floor`'s
        // own refusal) -- distinct from the "no list" case above, and from
        // `CreatorNeverGraduatedOrganically`, which this dossier does not
        // qualify for (`record.measured == 0` here).
        let creator_address = realorrug_types::Address::new([9u8; 32]).to_string();
        let list = empty_first_party_list();
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(1), None));
        let index = index_with_floor_and_target(creator_address, 9_999, 5, 40);
        let sheet = FactSheet::build(&dossier, None, Some(&index), None, Some(&list));
        assert!(
            !sheet.signals.contains(&Signal::RepeatLauncher),
            "{:?}",
            sheet.signals
        );
    }

    #[test]
    fn above_the_strongest_band_counts_and_below_it_does_not() {
        // "Ten to thirteen or above": 14 is above and fires; 9 is below and
        // does not; 13 and 10 are the edges. Re-applied `>=` as `>`: 10 stops
        // firing and this fails.
        let rates = rates_strongest(10, 13);
        for (recipients, fires) in [(9, false), (10, true), (13, true), (14, true), (40, true)] {
            let mut dossier = dossier_for([3u8; 32]);
            dossier.launch = Some(launch(
                realorrug_onchain::budget::Count::Exactly(recipients),
                None,
            ));
            let sheet = FactSheet::build(&dossier, Some(&rates), None, None, None);
            assert_eq!(
                sheet.signals.contains(&Signal::LaunchBlockInStrongestBand),
                fires,
                "{recipients} recipients"
            );
        }
    }

    #[test]
    fn the_strongest_band_is_read_from_the_snapshot_and_moves_with_it() {
        // Research 0024's lesson as a test: the same twelve recipients fire
        // when the snapshot's strongest band is ten to thirteen and do not
        // when it is exactly six -- and six then does.
        let mut twelve = dossier_for([3u8; 32]);
        twelve.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(12), None));
        let mut six = dossier_for([4u8; 32]);
        six.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(6), None));

        let at_six = rates_strongest(6, 6);
        assert!(
            FactSheet::build(&twelve, Some(&at_six), None, None, None)
                .signals
                .contains(&Signal::LaunchBlockInStrongestBand),
            "twelve is above six and fires: 'or above' is the rule"
        );
        assert!(
            FactSheet::build(&six, Some(&at_six), None, None, None)
                .signals
                .contains(&Signal::LaunchBlockInStrongestBand)
        );
        let at_ten = rates_strongest(10, 13);
        assert!(
            !FactSheet::build(&six, Some(&at_ten), None, None, None)
                .signals
                .contains(&Signal::LaunchBlockInStrongestBand),
            "six is below ten and does not fire once the band has moved"
        );
    }

    #[test]
    fn what_could_not_be_read_is_no_signal_rather_than_a_signal_of_zero() {
        // Rule 9, three ways. A truncated recipient count was decided by the
        // call budget; a creator with nothing measured has no record to hold
        // against them; a dev buy that was not seen is not a dev buy of zero.
        // And no snapshot means no band to be strongest.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::AtLeast(40), None));
        let rates = rates_strongest(10, 13);
        let unmeasured = index_with(record(0, 0));
        let sheet = FactSheet::build(&dossier, Some(&rates), Some(&unmeasured), None, None);
        assert_eq!(sheet.signals, []);

        // A dev buy of exactly zero lamports, if a chain ever reported one, is
        // a buy that was seen and was nothing -- also no signal.
        let mut zero = dossier_for([3u8; 32]);
        zero.launch = Some(launch(
            realorrug_onchain::budget::Count::Exactly(12),
            Some(0),
        ));
        assert_eq!(
            FactSheet::build(&zero, None, None, None, None).signals,
            [],
            "no snapshot, no band; a zero buy is not a buy"
        );

        // A creator with a measured organic graduation is not "never".
        let mut organic = dossier_for([3u8; 32]);
        organic.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(2), None));
        let graduated = index_with(record(3, 1));
        assert_eq!(
            FactSheet::build(&organic, Some(&rates), Some(&graduated), None, None).signals,
            []
        );

        // No launch block at all: nothing to count.
        assert_eq!(
            FactSheet::build(
                &dossier_for([5u8; 32]),
                Some(&rates),
                Some(&graduated),
                None,
                None
            )
            .signals,
            []
        );
    }

    #[test]
    fn the_curve_facts_reach_the_sheet() {
        // `push_curve` replaced with nothing survived mutation testing on
        // 2026-09-05: no test asserted that a curve the dossier read appears on
        // the sheet. A sheet that silently says less is LEARNINGS 5 in a
        // published reply -- an absence that reads as fine.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.curve = Some(realorrug_onchain::CurveFacts {
            creator: realorrug_types::ChainAddress::Solana(realorrug_types::Address::new(
                [9u8; 32],
            )),
            complete: false,
            quote_reserves: 6_186_150_833,
            quote_capacity: Some(303_000_000),
            quote_asset: Some(realorrug_onchain::QuoteAsset::sol()),
            fees: None,
        });
        let rendered = FactSheet::build(&dossier, None, None, None, None).render();
        assert!(
            rendered.contains("has the token graduated off the bonding curve: no"),
            "{rendered}"
        );
        assert!(rendered.contains("0.3030 SOL"), "{rendered}");

        // Graduated, and no depth at all: both are statements, not blanks.
        let mut done = dossier_for([3u8; 32]);
        done.curve = Some(realorrug_onchain::CurveFacts {
            creator: realorrug_types::ChainAddress::Solana(realorrug_types::Address::new(
                [9u8; 32],
            )),
            complete: true,
            quote_reserves: 0,
            quote_capacity: None,
            quote_asset: None,
            fees: None,
        });
        let rendered = FactSheet::build(&done, None, None, None, None).render();
        assert!(
            rendered.contains("has the token graduated off the bonding curve: yes"),
            "{rendered}"
        );
        assert!(rendered.contains("cannot size into this"), "{rendered}");
    }

    #[test]
    fn a_market_snapshot_produces_a_fact_carrying_its_read_time() {
        // ADR 0033: a price carries the moment it was read. `observed_at` is
        // `MarketSnapshot`'s own wall clock, not `dossier.read_at` -- so the
        // rendered sentence must carry the snapshot's own UTC moment, and
        // this pins that against a future edit that reaches for the wrong
        // clock. 1_758_000_000 is 2025-09-16 05:20:00 UTC.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.market = Some(realorrug_onchain::market::MarketSnapshot {
            price_usd: Some(0.0421),
            market_cap_usd: Some(420_000.0),
            cap_basis: Some("circulating, as DexScreener reports it"),
            liquidity_usd: Some(15_000.0),
            volume_24h_usd: Some(8_200.0),
            pair_address: Some("0xabc".to_owned()),
            source: realorrug_onchain::market::Source::DexScreener,
            observed_at: std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_758_000_000),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let rendered = sheet.render();
        assert!(
            rendered.contains("2025-09-16 05:20 UTC"),
            "the rendered sheet must carry the snapshot's own read time: {rendered}"
        );
        assert!(rendered.contains("$0.0421"), "{rendered}");
        assert!(rendered.contains("$420000.00"), "{rendered}");
        assert!(
            rendered.contains("circulating, as DexScreener reports it"),
            "{rendered}"
        );
        // Every literal the market fact renders must be authorised, the same
        // check any other numeric fact must pass.
        let authorised: Vec<f64> = sheet.authorised().into_iter().map(|a| a.value).collect();
        assert!(authorised.contains(&0.0421));
        assert!(authorised.contains(&420_000.0));
    }

    #[test]
    fn no_market_snapshot_means_no_market_fact() {
        // `Dossier::market: None` -- no client configured, or a failed read
        // already turned into a named gap by `dispatch::robinhood` -- must
        // never produce a market fact, and must never surface as an
        // "unknown" line either (`market` is optional, like `capacity` and
        // `fees`).
        let dossier = dossier_for([3u8; 32]);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(!sheet.facts.iter().any(|f| f.kind == Kind::Market));
        assert!(
            !sheet.unknown.iter().any(|u| u.contains("market")),
            "{:?}",
            sheet.unknown
        );
    }

    #[test]
    fn a_failed_market_read_is_an_optional_gap_not_an_unknown_line() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.unavailable.push(realorrug_onchain::Unavailable {
            fact: "market",
            why: "dexscreener: timed out; fallback also failed: geckoterminal: timed out"
                .to_owned(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(
            !sheet.unknown.iter().any(|u| u.contains("market")),
            "a failed market read must not degrade verdict severity: {:?}",
            sheet.unknown
        );
    }

    #[test]
    fn a_skipped_optional_fact_lands_in_skipped_and_not_in_unknown() {
        // `assessment.rs`'s coverage figure (slice 7, ADR 0032) needs to see
        // this gap even though `verdict::level` must not: the same failed
        // market read as the test above, checked from the other side. If the
        // `skipped.push` at the `continue` in `build` were ever deleted, this
        // fails while the test above still passes -- proving the two lists
        // are genuinely independent, not one gap counted twice.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.unavailable.push(realorrug_onchain::Unavailable {
            fact: "market",
            why: "dexscreener: timed out; fallback also failed: geckoterminal: timed out"
                .to_owned(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(sheet.skipped, vec!["market".to_owned()]);
        assert!(!sheet.unknown.iter().any(|u| u.contains("market")));
    }

    #[test]
    fn a_failed_quote_asset_read_is_an_optional_gap_not_an_unknown_line() {
        // S1, "name the pair": a Pons v2 pair token that failed to name
        // itself is off-chain-shaped the same way a failed `market` read is
        // (the curve itself read fine; only its unit's name did not), so it
        // must not degrade `verdict::level` any more than `market` does.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.unavailable.push(realorrug_onchain::Unavailable {
            fact: "quote asset",
            why: "symbol(): the pair's name is not one Real or Rug will print as a ticker"
                .to_owned(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(
            !sheet.unknown.iter().any(|u| u.contains("quote asset")),
            "a failed pair read must not degrade verdict severity: {:?}",
            sheet.unknown
        );
    }

    #[test]
    fn a_skipped_quote_asset_lands_in_skipped_and_not_in_unknown() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.unavailable.push(realorrug_onchain::Unavailable {
            fact: "quote asset",
            why: "symbol(): the pair's name is not one Real or Rug will print as a ticker"
                .to_owned(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(sheet.skipped, vec!["quote asset".to_owned()]);
        assert!(!sheet.unknown.iter().any(|u| u.contains("quote asset")));
    }

    #[test]
    fn an_unread_correlated_selling_is_a_coverage_gap_not_an_unknown_line() {
        // S7 can only raise the score, so logs that could not be read must
        // show as a gap in coverage and leave `unknown` (which can push the
        // level to `CantTell`) exactly as it was.
        let dossier = dossier_for([3u8; 32]);
        let baseline = FactSheet::build(&dossier, None, None, None, None);
        let mut missed = dossier.clone();
        missed.unavailable.push(realorrug_onchain::Unavailable {
            fact: "correlated selling",
            why: "the curve's trade logs could not be read".to_owned(),
        });
        let sheet = FactSheet::build(&missed, None, None, None, None);
        assert_eq!(sheet.skipped, vec!["correlated selling".to_owned()]);
        assert_eq!(sheet.unknown, baseline.unknown);
    }

    /// Builds an address distinguishable by its last byte, so several
    /// exemptions in one test are visibly different addresses.
    fn robinhood_address(last_byte: u8) -> realorrug_types::ChainAddress {
        let mut bytes = [0u8; 20];
        bytes[19] = last_byte;
        realorrug_types::ChainAddress::Robinhood(realorrug_robinhood::Address(bytes))
    }

    fn powers_with(
        creator_tax_bps: u16,
        pending_creator_fee_recipient: Option<realorrug_types::ChainAddress>,
        exemptions: Vec<Exemption>,
    ) -> Powers {
        Powers {
            creator_tax_bps,
            pending_creator_fee_recipient,
            exemptions,
        }
    }

    fn factor_delta(sheet: &FactSheet, name: &str) -> Option<i32> {
        factors(sheet)
            .into_iter()
            .find(|f| f.signal == Signal::OwnerCanStillMintOrPause && f.name == name)
            .map(|f| f.delta_bps)
    }

    #[test]
    fn creator_tax_499_bps_raises_nothing() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(499, None, Vec::new()));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(factor_delta(&sheet, "creator tax >= 500 bps"), None);
    }

    #[test]
    fn creator_tax_500_bps_raises_700() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(500, None, Vec::new()));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(factor_delta(&sheet, "creator tax >= 500 bps"), Some(700));
    }

    #[test]
    fn creator_tax_501_bps_raises_700() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(501, None, Vec::new()));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(factor_delta(&sheet, "creator tax >= 500 bps"), Some(700));
    }

    #[test]
    fn creator_tax_zero_lowers_300() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(0, None, Vec::new()));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(factor_delta(&sheet, "creator tax == 0"), Some(-300));
    }

    #[test]
    fn creator_tax_one_bps_moves_neither_factor() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(1, None, Vec::new()));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(factor_delta(&sheet, "creator tax == 0"), None);
        assert_eq!(factor_delta(&sheet, "creator tax >= 500 bps"), None);
    }

    #[test]
    fn no_pending_fee_recipient_raises_nothing() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(0, None, Vec::new()));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(
            factor_delta(&sheet, "pending creator fee recipient is non-zero"),
            None
        );
    }

    #[test]
    fn a_pending_fee_recipient_raises_500() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(0, Some(robinhood_address(7)), Vec::new()));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(
            factor_delta(&sheet, "pending creator fee recipient is non-zero"),
            Some(500)
        );
    }

    #[test]
    fn zero_undeclared_exemptions_raises_nothing() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(0, None, Vec::new()));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(
            factor_delta(&sheet, "exempt address(es) off both lists"),
            None
        );
    }

    #[test]
    fn one_undeclared_exemption_raises_600() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(
            0,
            None,
            vec![Exemption {
                address: robinhood_address(1),
                source: ExemptionSource::Undeclared,
            }],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(
            factor_delta(&sheet, "exempt address(es) off both lists"),
            Some(600)
        );
    }

    #[test]
    fn two_undeclared_exemptions_raise_1200_and_cap_there() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(
            0,
            None,
            vec![
                Exemption {
                    address: robinhood_address(1),
                    source: ExemptionSource::Undeclared,
                },
                Exemption {
                    address: robinhood_address(2),
                    source: ExemptionSource::Undeclared,
                },
            ],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(
            factor_delta(&sheet, "exempt address(es) off both lists"),
            Some(1_200)
        );
    }

    #[test]
    fn three_undeclared_exemptions_stay_capped_at_1200() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(
            0,
            None,
            vec![
                Exemption {
                    address: robinhood_address(1),
                    source: ExemptionSource::Undeclared,
                },
                Exemption {
                    address: robinhood_address(2),
                    source: ExemptionSource::Undeclared,
                },
                Exemption {
                    address: robinhood_address(3),
                    source: ExemptionSource::Undeclared,
                },
            ],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(
            factor_delta(&sheet, "exempt address(es) off both lists"),
            Some(1_200)
        );
    }

    #[test]
    fn a_declared_exemption_is_not_counted_as_undeclared() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(
            0,
            None,
            vec![Exemption {
                address: robinhood_address(1),
                source: ExemptionSource::Declared,
            }],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(
            factor_delta(&sheet, "exempt address(es) off both lists"),
            None
        );
    }

    #[test]
    fn a_first_party_exemption_is_not_counted_as_undeclared() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(
            0,
            None,
            vec![Exemption {
                address: robinhood_address(1),
                source: ExemptionSource::FirstParty,
            }],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(
            factor_delta(&sheet, "exempt address(es) off both lists"),
            None
        );
    }

    #[test]
    fn an_unread_owner_power_sub_read_is_a_coverage_gap_not_an_unknown_line() {
        // Same discipline as `an_unread_correlated_selling_is_a_coverage_gap...`
        // above: each of S13's three sub-reads can only ever raise the
        // score, so a failed one must show up as a coverage gap and leave
        // `unknown` untouched, never push `verdict::level` toward `CantTell`.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(0, None, Vec::new()));
        let baseline = FactSheet::build(&dossier, None, None, None, None);

        for fact_name in [
            "pending creator fee recipient",
            "declared snipe-tax exemptions",
            "snipe tax exemption",
            "snipe tax exemption classification",
        ] {
            let mut missed = dossier.clone();
            missed.unavailable.push(realorrug_onchain::Unavailable {
                fact: fact_name,
                why: "the read did not complete".to_owned(),
            });
            let sheet = FactSheet::build(&missed, None, None, None, None);
            assert_eq!(
                sheet.skipped,
                vec![fact_name.to_owned()],
                "expected {fact_name} to land in skipped"
            );
            assert_eq!(
                sheet.unknown, baseline.unknown,
                "{fact_name} must not degrade verdict severity"
            );
        }

        // And the gated fact itself must be absent from the sheet -- never a
        // zero/clean value standing in for "unread" (rule 8).
        let mut missed_pending = dossier.clone();
        missed_pending
            .unavailable
            .push(realorrug_onchain::Unavailable {
                fact: "pending creator fee recipient",
                why: "the read did not complete".to_owned(),
            });
        let sheet = FactSheet::build(&missed_pending, None, None, None, None);
        assert!(
            !sheet
                .facts
                .iter()
                .any(|f| f.kind == Kind::PendingCreatorFeeRecipientSet),
            "an unread pending-recipient sub-read must not publish a fact at all"
        );
    }

    #[test]
    fn any_one_unread_exemption_read_withholds_the_undeclared_count() {
        // The count needs all three exemption reads; any single one failing
        // leaves it off the sheet, so an undeclared exemption the dossier
        // does hold is never published on a half-read basis (rule 8).
        let mut dossier = dossier_for([3u8; 32]);
        dossier.powers = Some(powers_with(
            0,
            None,
            vec![Exemption {
                address: robinhood_address(1),
                source: ExemptionSource::Undeclared,
            }],
        ));
        for fact_name in [
            "declared snipe-tax exemptions",
            "snipe tax exemption",
            "snipe tax exemption classification",
        ] {
            let mut missed = dossier.clone();
            missed.unavailable.push(realorrug_onchain::Unavailable {
                fact: fact_name,
                why: "the read did not complete".to_owned(),
            });
            let sheet = FactSheet::build(&missed, None, None, None, None);
            assert!(
                !sheet
                    .facts
                    .iter()
                    .any(|f| f.kind == Kind::UndeclaredExemptions),
                "{fact_name} unread must withhold the undeclared count"
            );
        }
    }

    #[test]
    fn a_market_candidate_never_outranks_concentration() {
        // AGENTS.md §3 rule 5: price never leads a reply. `salience::rank`
        // is the one ranking every reader shares, so pinning the order here
        // is pinning it everywhere `lead`, `template` and `request_for` read
        // from.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.holders = Some(realorrug_onchain::Holders {
            count: 529,
            largest_share_bps: Some(5020),
        });
        dossier.market = Some(realorrug_onchain::market::MarketSnapshot {
            price_usd: Some(1.0),
            market_cap_usd: None,
            cap_basis: None,
            liquidity_usd: None,
            volume_24h_usd: None,
            pair_address: None,
            source: realorrug_onchain::market::Source::GeckoTerminal,
            observed_at: std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_758_000_000),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let ranked = crate::salience::rank(&sheet);
        let market_rank = ranked
            .iter()
            .position(|c| c.id == crate::salience::CandidateId(vec![Kind::Market]))
            .expect("market candidate present");
        let concentration_rank = ranked
            .iter()
            .position(|c| c.id.0.contains(&Kind::Holders))
            .expect("concentration candidate present");
        assert!(
            market_rank > concentration_rank,
            "market must rank below concentration: {ranked:?}"
        );
        // The market candidate carries only the market facts' plain clause:
        // not another fact's words, and not the blunt voice.
        assert_eq!(
            ranked[market_rank].sentence,
            "An aggregator priced it at $1.00 as of 2025-09-16 05:20 UTC."
        );
    }

    #[test]
    fn a_price_under_a_dollar_keeps_its_digits_and_one_from_a_dollar_up_has_two() {
        assert_eq!(render_usd(0.0421), "0.0421");
        assert_eq!(render_usd(0.5), "0.5");
        assert_eq!(render_usd(1.0), "1.00");
        assert_eq!(render_usd(0.0), "0.00");
        assert_eq!(render_usd(420_000.0), "420000.00");
    }

    /// `civil_from_days` against dates independently computed with `date -u
    /// -d <date> +%s`, divided by 86400: the epoch itself, a leap day both
    /// on and off a century boundary (2024 and 2000 are leap; 2100 is not,
    /// despite also dividing by 4, so its 28 February is followed by
    /// 1 March), the first March after the epoch, and a date well past the
    /// range any real market snapshot will ever carry.
    #[test]
    fn civil_from_days_matches_known_calendar_dates_including_leap_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(20_347), (2025, 9, 16));
        assert_eq!(civil_from_days(59), (1970, 3, 1));
        assert_eq!(civil_from_days(47_541), (2100, 3, 1));
    }

    #[test]
    fn render_observed_at_reads_as_a_date_that_scans_as_one_number() {
        // 1_758_000_000 is 2025-09-16 05:20:00 UTC. The exact string is
        // pinned because `fidelity::literals` reads only this shape as one
        // number; see `render_observed_at` for why five would be a hole.
        let observed_at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_758_000_000);
        let rendered = render_observed_at(observed_at);
        assert_eq!(rendered, "2025-09-16 05:20 UTC");
        let literals = crate::fidelity::literals(&rendered);
        assert_eq!(
            literals.len(),
            1,
            "the moment must scan as one literal, not several small ones: {literals:?}"
        );
    }

    /// Builds a `TokenOwner` for the token-ownership tests below.
    fn token_owner(
        owner: [u8; 32],
        amount: u128,
        share_bps: Option<u16>,
        role: realorrug_onchain::OwnerRole,
    ) -> realorrug_onchain::TokenOwner {
        realorrug_onchain::TokenOwner {
            owner: realorrug_types::Address::new(owner),
            accounts: 1,
            amount,
            share_bps,
            role,
        }
    }

    #[test]
    fn an_unresolved_owner_gets_unidentified_wording_never_a_role() {
        // The only proof this reader can make is "this address is the
        // bonding curve"; every other owner, however large, must render as
        // "unidentified" -- never a role the sheet did not establish
        // (AGENTS.md §4's last bullet).
        let mut dossier = dossier_for([3u8; 32]);
        dossier.token_ownership = Some(realorrug_onchain::TokenOwnership {
            owners: vec![token_owner(
                [40u8; 32],
                6_000,
                Some(6_000),
                realorrug_onchain::OwnerRole::Unresolved,
            )],
            supply: 10_000,
            decimals: 6,
            mint_authority: None,
            freeze_authority: None,
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let fact = fact_of(&sheet, Kind::TokenOwnership).expect("a token-ownership fact");
        assert_eq!(fact.rendered, "60.0%");
        let plain = fact
            .clauses
            .iter()
            .find(|c| c.voice == crate::clause::Voice::Plain)
            .expect("a plain clause");
        assert!(
            plain.text.contains("one unidentified wallet holds 60.0%"),
            "{}",
            plain.text
        );
        let authorised: Vec<f64> = sheet.authorised().into_iter().map(|a| a.value).collect();
        assert!(authorised.contains(&0.6));
    }

    #[test]
    fn a_proven_curve_owner_is_skipped_for_the_next_largest_unresolved_one() {
        // Requirement from design 0027 slice 6a: the bonding curve is the
        // one owner this reader can exclude by proof. Excluding it must fall
        // through to the next owner in the (already amount-sorted) list, not
        // suppress the fact entirely.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.token_ownership = Some(realorrug_onchain::TokenOwnership {
            owners: vec![
                token_owner(
                    [41u8; 32],
                    7_000,
                    Some(7_000),
                    realorrug_onchain::OwnerRole::BondingCurve,
                ),
                token_owner(
                    [42u8; 32],
                    2_000,
                    Some(2_000),
                    realorrug_onchain::OwnerRole::Unresolved,
                ),
            ],
            supply: 10_000,
            decimals: 6,
            mint_authority: None,
            freeze_authority: None,
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let fact = fact_of(&sheet, Kind::TokenOwnership).expect("a token-ownership fact");
        assert_eq!(fact.rendered, "20.0%");
    }

    #[test]
    fn every_sampled_owner_being_the_curve_writes_no_fact() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.token_ownership = Some(realorrug_onchain::TokenOwnership {
            owners: vec![token_owner(
                [43u8; 32],
                10_000,
                Some(10_000),
                realorrug_onchain::OwnerRole::BondingCurve,
            )],
            supply: 10_000,
            decimals: 6,
            mint_authority: None,
            freeze_authority: None,
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(!sheet.facts.iter().any(|f| f.kind == Kind::TokenOwnership));
    }

    #[test]
    fn no_token_ownership_read_means_no_fact_and_no_unknown_line() {
        let dossier = dossier_for([3u8; 32]);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(!sheet.facts.iter().any(|f| f.kind == Kind::TokenOwnership));
        assert!(
            !sheet.unknown.iter().any(|u| u.contains("ownership")),
            "{:?}",
            sheet.unknown
        );
    }

    #[test]
    fn a_failed_token_ownership_read_is_an_optional_gap_not_an_unknown_line() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.unavailable.push(realorrug_onchain::Unavailable {
            fact: "token ownership",
            why: "rpc transport: http status: 429".to_owned(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(
            !sheet.unknown.iter().any(|u| u.contains("ownership")),
            "a failed token-ownership read must not degrade verdict severity: {:?}",
            sheet.unknown
        );
    }

    #[test]
    fn a_token_ownership_candidate_never_outranks_concentration() {
        // `concentration`'s `Holders` is Robinhood-only and `TokenOwnership`
        // is Solana-only, so the two never fire on the same dossier today --
        // but the priorities are still pinned so that if a future dossier
        // ever carries both, `concentration` -- the fuller, non-sampled
        // count -- wins the tie rather than whichever was considered first.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.holders = Some(realorrug_onchain::Holders {
            count: 529,
            largest_share_bps: Some(5020),
        });
        dossier.token_ownership = Some(realorrug_onchain::TokenOwnership {
            owners: vec![token_owner(
                [44u8; 32],
                6_000,
                Some(6_000),
                realorrug_onchain::OwnerRole::Unresolved,
            )],
            supply: 10_000,
            decimals: 6,
            mint_authority: None,
            freeze_authority: None,
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let ranked = crate::salience::rank(&sheet);
        let ownership_rank = ranked
            .iter()
            .position(|c| c.id == crate::salience::CandidateId(vec![Kind::TokenOwnership]))
            .expect("token-ownership candidate present");
        let concentration_rank = ranked
            .iter()
            .position(|c| c.id.0.contains(&Kind::Holders))
            .expect("concentration candidate present");
        assert!(
            ownership_rank > concentration_rank,
            "token ownership must rank below concentration: {ranked:?}"
        );
    }

    fn creator_trade(
        role: realorrug_robinhood::pons::CreatorRole,
        side: realorrug_robinhood::pons::Side,
        quote: u128,
    ) -> realorrug_onchain::wallets::CreatorTrade {
        realorrug_onchain::wallets::CreatorTrade {
            role,
            side,
            quote,
            tokens: 1,
            block: 1,
            transaction: realorrug_robinhood::Hash32([1; 32]),
            unique_id: format!("0x01-0-{quote}"),
        }
    }

    /// Design 0027 slice 5's own done criterion (a): a sale by either role --
    /// the deployer or the fee recipient, not only one -- must be counted
    /// toward the same published proceeds figure.
    #[test]
    fn proceeds_count_trades_from_both_the_deployer_and_the_fee_recipient() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.creator_cash_flow = Some(realorrug_onchain::wallets::CreatorCashFlow {
            trades: vec![
                creator_trade(
                    realorrug_robinhood::pons::CreatorRole::Deployer,
                    realorrug_robinhood::pons::Side::Sell,
                    200_000_000_000_000,
                ),
                creator_trade(
                    realorrug_robinhood::pons::CreatorRole::FeeRecipient,
                    realorrug_robinhood::pons::Side::Sell,
                    300_000_000_000_000,
                ),
            ],
            transfers_out: 0,
            trades_complete: true,
            gaps: Vec::new(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let fact = fact_of(&sheet, Kind::CreatorCashFlow).expect("a proceeds fact");
        assert_eq!(fact.rendered, "0.0005 ETH");
    }

    /// Done criterion (b), read the other direction from `wallets.rs`'s own
    /// test of the same rule: a transfer out is never folded into the
    /// published proceeds figure, even when it is the only thing on the
    /// sheet.
    #[test]
    fn a_transfer_out_never_adds_to_proceeds() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.creator_cash_flow = Some(realorrug_onchain::wallets::CreatorCashFlow {
            trades: vec![creator_trade(
                realorrug_robinhood::pons::CreatorRole::Deployer,
                realorrug_robinhood::pons::Side::Sell,
                100_000_000_000_000,
            )],
            transfers_out: 3,
            trades_complete: true,
            gaps: Vec::new(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let proceeds = fact_of(&sheet, Kind::CreatorCashFlow).expect("a proceeds fact");
        assert_eq!(
            proceeds.rendered, "0.0001 ETH",
            "the 3 outgoing transfers must not inflate the sale"
        );
        let transfer_fact = sheet
            .facts
            .iter()
            .find(|f| f.kind == Kind::CreatorCashFlow && f.rendered == "3")
            .expect("a transfers-out fact");
        // Worded as a transfer that was *not* a sale ("were not decoded
        // sales on the curve") is the required disclaimer; the fact must
        // never be worded as "received in sales" the way the proceeds fact
        // is.
        assert!(
            transfer_fact
                .clauses
                .iter()
                .all(|c| !c.text.contains("received")),
            "a transfer must never be worded as if it were received in a sale: {:?}",
            transfer_fact.clauses
        );
        assert!(
            transfer_fact
                .clauses
                .iter()
                .any(|c| c.text.contains("not decoded sales")),
            "{:?}",
            transfer_fact.clauses
        );
    }

    /// Done criterion (c), the sheet-level half of `wallets.rs`'s own test:
    /// an incomplete trade history publishes no ETH number at all.
    #[test]
    fn incomplete_trades_publish_no_eth_number() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.creator_cash_flow = Some(realorrug_onchain::wallets::CreatorCashFlow {
            trades: vec![creator_trade(
                realorrug_robinhood::pons::CreatorRole::Deployer,
                realorrug_robinhood::pons::Side::Sell,
                100_000_000_000_000,
            )],
            transfers_out: 1,
            trades_complete: false,
            gaps: vec!["creator trade history: too many results".to_owned()],
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(
            !sheet.facts.iter().any(|f| f.kind == Kind::CreatorCashFlow),
            "an incomplete read must publish nothing, not a partial number: {:?}",
            sheet.facts
        );
        // The gap is an optional miss, same treatment as a failed
        // `capacity`/`fees`/`token ownership` read: never forced into
        // `unknown`, which would degrade the verdict to `CantTell`.
        assert!(
            !sheet.unknown.iter().any(|u| u.contains("cash flow")),
            "{:?}",
            sheet.unknown
        );
    }

    #[test]
    fn a_negative_net_renders_with_its_sign() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.creator_cash_flow = Some(realorrug_onchain::wallets::CreatorCashFlow {
            trades: vec![
                creator_trade(
                    realorrug_robinhood::pons::CreatorRole::Deployer,
                    realorrug_robinhood::pons::Side::Buy,
                    500_000_000_000_000_000,
                ),
                creator_trade(
                    realorrug_robinhood::pons::CreatorRole::Deployer,
                    realorrug_robinhood::pons::Side::Sell,
                    100_000_000_000_000_000,
                ),
            ],
            transfers_out: 0,
            trades_complete: true,
            gaps: Vec::new(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let facts: Vec<_> = sheet
            .facts
            .iter()
            .filter(|f| f.kind == Kind::CreatorCashFlow)
            .collect();
        let net = facts
            .iter()
            .find(|f| f.rendered.starts_with('-'))
            .expect("a negative net fact");
        assert_eq!(net.rendered, "-0.4000 ETH");
        assert!(
            !net.label.to_lowercase().contains("profit"),
            "{}",
            net.label
        );
        assert!(
            !net.clauses
                .iter()
                .any(|c| c.text.to_lowercase().contains("profit")),
            "{:?}",
            net.clauses
        );
        let authorised: Vec<f64> = sheet.authorised().into_iter().map(|a| a.value).collect();
        assert!(
            authorised.contains(&-0.4),
            "the negative net's value must be authorised: {authorised:?}"
        );
    }

    /// Breaking even is not a loss: a net of exactly zero carries no minus
    /// sign, in its words or in its number.
    #[test]
    fn a_zero_net_renders_without_a_sign() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.creator_cash_flow = Some(realorrug_onchain::wallets::CreatorCashFlow {
            trades: vec![
                creator_trade(
                    realorrug_robinhood::pons::CreatorRole::Deployer,
                    realorrug_robinhood::pons::Side::Buy,
                    100_000_000_000_000_000,
                ),
                creator_trade(
                    realorrug_robinhood::pons::CreatorRole::Deployer,
                    realorrug_robinhood::pons::Side::Sell,
                    100_000_000_000_000_000,
                ),
            ],
            transfers_out: 0,
            trades_complete: true,
            gaps: Vec::new(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let net = sheet
            .facts
            .iter()
            .find(|f| f.kind == Kind::CreatorCashFlow && f.label.starts_with("observed net"))
            .expect("a net fact");
        assert_eq!(net.rendered, "0.0000 ETH");
        assert!(
            net.values.iter().all(|v| !v.is_sign_negative()),
            "{:?}",
            net.values
        );
    }

    #[test]
    fn a_zero_transfer_count_writes_no_transfer_fact() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.creator_cash_flow = Some(realorrug_onchain::wallets::CreatorCashFlow {
            trades: vec![creator_trade(
                realorrug_robinhood::pons::CreatorRole::Deployer,
                realorrug_robinhood::pons::Side::Sell,
                100,
            )],
            transfers_out: 0,
            trades_complete: true,
            gaps: Vec::new(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(
            !sheet
                .facts
                .iter()
                .any(|f| f.kind == Kind::CreatorCashFlow && f.rendered == "0"),
            "a zero transfer count must not be published: {:?}",
            sheet.facts
        );
    }

    #[test]
    fn every_creator_cash_flow_number_is_authorised() {
        let mut dossier = dossier_for([3u8; 32]);
        dossier.creator_cash_flow = Some(realorrug_onchain::wallets::CreatorCashFlow {
            trades: vec![creator_trade(
                realorrug_robinhood::pons::CreatorRole::Deployer,
                realorrug_robinhood::pons::Side::Sell,
                1_000_000_000_000_000_000,
            )],
            transfers_out: 2,
            trades_complete: true,
            gaps: Vec::new(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let authorised: Vec<f64> = sheet.authorised().into_iter().map(|a| a.value).collect();
        assert!(authorised.contains(&1.0), "{authorised:?}");
        assert!(authorised.contains(&2.0), "{authorised:?}");
    }

    #[test]
    fn a_robinhood_curve_renders_in_its_own_asset_and_a_solana_curve_is_unchanged() {
        // No `match chain` in this file: the two reads through the same
        // `push_curve` path, and the only difference in the output is the
        // decimals and symbol carried on `quote_asset`.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.curve = Some(realorrug_onchain::CurveFacts {
            creator: realorrug_types::ChainAddress::Solana(realorrug_types::Address::new(
                [9u8; 32],
            )),
            complete: false,
            quote_reserves: 20_000_000_000_000_000_000,
            quote_capacity: Some(2_500_000_000_000_000_000),
            quote_asset: Some(realorrug_onchain::QuoteAsset::eth()),
            fees: None,
        });
        let rendered = FactSheet::build(&dossier, None, None, None, None).render();
        assert!(rendered.contains("2.5000 ETH"), "{rendered}");
        assert!(!rendered.contains("SOL"), "{rendered}");

        // Pinned byte-for-byte: the Solana path renders exactly what it did
        // before this file learned about a second chain.
        let mut sol = dossier_for([3u8; 32]);
        sol.curve = Some(realorrug_onchain::CurveFacts {
            creator: realorrug_types::ChainAddress::Solana(realorrug_types::Address::new(
                [9u8; 32],
            )),
            complete: false,
            quote_reserves: 6_186_150_833,
            quote_capacity: Some(303_000_000),
            quote_asset: Some(realorrug_onchain::QuoteAsset::sol()),
            fees: None,
        });
        let rendered = FactSheet::build(&sol, None, None, None, None).render();
        assert!(rendered.contains("0.3030 SOL"), "{rendered}");
    }

    #[test]
    fn a_solana_curve_renders_byte_for_byte() {
        // Both chains now render through one function (`push_curve` /
        // `render_quote`), and `.contains` checks elsewhere would stay green
        // through a regression that only shifted or reworded the Solana
        // output. This pins the exact two lines the shared path produces for
        // a Solana curve, so any change to that path -- intentional or not --
        // has to touch this assertion.
        let mut facts = Vec::new();
        let mut unknown = Vec::new();
        let curve = realorrug_onchain::CurveFacts {
            creator: realorrug_types::ChainAddress::Solana(realorrug_types::Address::new(
                [9u8; 32],
            )),
            complete: false,
            quote_reserves: 6_186_150_833,
            quote_capacity: Some(303_000_000),
            quote_asset: Some(realorrug_onchain::QuoteAsset::sol()),
            fees: None,
        };
        push_curve(
            &mut facts,
            &mut unknown,
            &curve,
            Some(ReadAt::Solana(Slot(444_007_820))),
        );
        let mut rendered = String::new();
        for fact in &facts {
            let _ = writeln!(rendered, "{}: {}", fact.label, fact.rendered);
        }
        assert_eq!(
            rendered,
            "has the token graduated off the bonding curve: no\n\
             quote asset held in the bonding curve now, read at slot 444007820: 6.1861 SOL\n\
             quote asset that can be bought before price moves 1% -- this is REAL OR RUG.S OWN \
             impact budget, NOT a ceiling the venue imposes (research 0022): 0.3030 SOL\n"
        );
        assert!(unknown.is_empty(), "{unknown:?}");
    }

    #[test]
    fn a_curve_with_no_identified_quote_asset_renders_no_amount() {
        // Rule 8: absent is not zero, and a number Radar cannot name the unit
        // of is not a number Radar publishes. The capacity figure exists but
        // is withheld entirely, and the reason goes to `unknown` instead.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.curve = Some(realorrug_onchain::CurveFacts {
            creator: realorrug_types::ChainAddress::Solana(realorrug_types::Address::new(
                [9u8; 32],
            )),
            complete: false,
            quote_reserves: 5_000_000_000_000_000_000,
            quote_capacity: Some(1_000_000_000_000_000_000),
            quote_asset: None,
            fees: None,
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let rendered = sheet.render();
        assert!(!rendered.contains("exit capacity"), "{rendered}");
        assert!(
            !rendered.contains("1.0000"),
            "an amount was rendered with no identified unit: {rendered}"
        );
        assert!(
            sheet
                .unknown
                .iter()
                .any(|u| u.contains("quote asset") || u.contains("could not be priced")),
            "{:?}",
            sheet.unknown
        );
    }

    #[test]
    fn a_robinhood_token_pair_is_named_with_its_symbol_and_address() {
        // S1, "name the pair": a curve paired with an ERC-20 token rather
        // than native ETH states which one, by symbol AND address -- the
        // symbol alone is untrusted launcher-chosen text (AGENTS.md §3 rule
        // 3), and the address is what lets a reader check it.
        let pair = realorrug_robinhood::Address([0x72; 20]);
        let asset = realorrug_onchain::QuoteAsset::token(
            realorrug_types::ChainAddress::Robinhood(pair),
            "HIMS".to_owned(),
            18,
        );
        let mut facts = Vec::new();
        let mut unknown = Vec::new();
        let curve = realorrug_onchain::CurveFacts {
            creator: realorrug_types::ChainAddress::Robinhood(realorrug_robinhood::Address(
                [0x71; 20],
            )),
            complete: false,
            quote_reserves: 5_000_000_000_000_000_000,
            quote_capacity: Some(1_000_000_000_000_000_000),
            quote_asset: Some(asset),
            fees: None,
        };
        push_curve(&mut facts, &mut unknown, &curve, None);
        let rendered: Vec<String> = facts.iter().map(|f| f.rendered.clone()).collect();
        let expected = format!("HIMS ({pair})");
        assert!(
            rendered.contains(&expected),
            "expected {expected:?} among {rendered:?}"
        );
        assert!(
            facts
                .iter()
                .any(|f| f.kind == Kind::QuotePair && f.rendered == expected),
            "{facts:?}"
        );
        assert!(unknown.is_empty(), "{unknown:?}");
    }

    #[test]
    fn a_native_eth_pair_renders_no_quote_pair_fact() {
        // ETH has no contract to cite (`QuoteAsset::address` is `None`), so
        // the "paired with" fact must not appear at all -- unchanged from
        // before S1.
        let mut facts = Vec::new();
        let mut unknown = Vec::new();
        let curve = realorrug_onchain::CurveFacts {
            creator: realorrug_types::ChainAddress::Robinhood(realorrug_robinhood::Address(
                [0x71; 20],
            )),
            complete: false,
            quote_reserves: 5_000_000_000_000_000_000,
            quote_capacity: Some(1_000_000_000_000_000_000),
            quote_asset: Some(realorrug_onchain::QuoteAsset::eth()),
            fees: None,
        };
        push_curve(&mut facts, &mut unknown, &curve, None);
        assert!(
            !facts.iter().any(|f| f.kind == Kind::QuotePair),
            "{facts:?}"
        );
    }

    const SNAPSHOT: &str = include_str!("../../../docs/research/data/0024-base-rates.json");

    #[test]
    fn the_cost_facts_reach_the_sheet() {
        // `push_cost` replaced with nothing survived the same run. The cost line
        // is the fact GOAL.md says leads every reply, and nothing pinned that it
        // was there at all.
        let rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        let sheet = FactSheet::build(&dossier_for([3u8; 32]), Some(&rates), None, None, None);
        let rendered = sheet.render();
        assert!(
            rendered.contains(
                "expected edge a strategy must clear before one trade is worth making: 456 bps"
            ),
            "{rendered}"
        );
        assert!(
            rendered
                .contains("round trip Real or Rug's kernel assumes, on fresh launches: 850 bps"),
            "{rendered}"
        );
        assert!(
            rendered.contains("round trip for a position of $20-$200: 456 bps (4.6%)"),
            "{rendered}"
        );
        // Authorised in both renderings, so a reply quoting 4.6% is not refused
        // as a fabrication of a figure the sheet stated.
        let authorised = sheet.authorised();
        assert!(authorised.iter().any(|a| (a.value - 456.0).abs() < 1e-9));
        assert!(authorised.iter().any(|a| (a.value - 4.56).abs() < 1e-9));
    }

    #[test]
    fn the_measured_population_reaches_the_sheet() {
        // `push_measured_population` replaced with nothing survived the same
        // run. This is the denominator every creator count is read against;
        // without it "none of 150 filled its curve" has no scale.
        let index = crate::creator::CreatorIndex {
            chain: crate::firstparty::Chain::Solana,
            watermark_slot: 444_374_676,
            built_at: 1_788_000_000,
            population: Some(crate::creator::Population {
                launches: 508_814,
                measured: 506_991,
                organic: 9_060,
                instant: 5_222,
                stillborn: 116_608,
            }),
            creators: std::collections::BTreeMap::new(),
        };
        let rendered =
            FactSheet::build(&dossier_for([3u8; 32]), None, Some(&index), None, None).render();
        assert!(
            rendered.contains(
                "launches Real or Rug has recorded and measured, which every share below is out of: 506991"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains(
                "of every measured launch, how many showed almost no activity at all: 23.0%"
            ),
            "{rendered}"
        );
        assert!(
            rendered.contains("of every measured launch, how many graduated at all: "),
            "{rendered}"
        );

        // Nothing measured is not a population of zeroes -- rule 9. The figure
        // is said to be missing, in Radar's words, rather than published as 0%.
        let empty = crate::creator::CreatorIndex {
            population: Some(crate::creator::Population::default()),
            ..index
        };
        let rendered =
            FactSheet::build(&dossier_for([3u8; 32]), None, Some(&empty), None, None).render();
        assert!(
            rendered.contains("NOT AVAILABLE -- no outcome has been measured yet"),
            "{rendered}"
        );
        assert!(!rendered.contains("0.0%"), "{rendered}");
    }

    /// A dossier about one Robinhood token and nothing else, mirroring
    /// `dossier_for`'s Solana counterpart.
    fn robinhood_dossier_for(token: [u8; 20]) -> Dossier {
        Dossier {
            mint: realorrug_types::ChainAddress::Robinhood(realorrug_robinhood::Address(token)),
            read_at: Some(realorrug_types::ReadAt::Robinhood(100)),
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

    fn robinhood_launched(age_seconds: Option<u64>, dev_buy_wei: Option<u128>) -> Dossier {
        named_robinhood_launch(age_seconds, dev_buy_wei, None, None)
    }

    fn named_robinhood_launch(
        age_seconds: Option<u64>,
        dev_buy_wei: Option<u128>,
        name: Option<&str>,
        symbol: Option<&str>,
    ) -> Dossier {
        robinhood_launch_with_share(age_seconds, dev_buy_wei, None, None, name, symbol)
    }

    /// [`named_robinhood_launch`], plus the two fields
    /// [`Signal::CreatorBoughtOwnLaunch`]'s share factors read:
    /// `dev_buy_tokens` and `supply`.
    fn robinhood_launch_with_share(
        age_seconds: Option<u64>,
        dev_buy_wei: Option<u128>,
        dev_buy_tokens: Option<u128>,
        supply: Option<u128>,
        name: Option<&str>,
        symbol: Option<&str>,
    ) -> Dossier {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.chain_launch = Some(realorrug_onchain::ChainLaunch {
            block: 64,
            age_seconds,
            dev_buy_wei,
            dev_buy_tokens,
            supply,
            name: name.map(str::to_owned),
            symbol: symbol.map(str::to_owned),
            correlated_selling: None,
        });
        dossier
    }

    /// Builds a launch whose only fact of interest is S7's cluster, so each
    /// boundary test below exercises exactly one of `correlated_selling_factors`'s
    /// three range checks without the `CreatorBoughtOwnLaunch` share above also
    /// firing and cluttering the assertion.
    fn robinhood_launch_with_correlated_selling(
        linked_sellers: u32,
        sold_bps_of_supply: Option<u16>,
        spread_seconds: Option<u64>,
    ) -> Dossier {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.chain_launch = Some(realorrug_onchain::ChainLaunch {
            block: 64,
            age_seconds: Some(3_600),
            dev_buy_wei: None,
            dev_buy_tokens: None,
            supply: None,
            name: None,
            symbol: None,
            correlated_selling: Some(realorrug_onchain::wallets::CorrelatedSelling {
                linked_sellers,
                sold_bps_of_supply,
                spread_seconds,
                sells_read: true,
            }),
        });
        dossier
    }

    fn fact_of(sheet: &FactSheet, kind: Kind) -> Option<&Fact> {
        sheet.facts.iter().find(|f| f.kind == kind)
    }

    #[test]
    fn a_read_no_later_than_the_launch_gives_no_age() {
        // Two reads from nodes at different heights can put the launch after
        // the read, and a dossier with no curve read carries the launch's own
        // slot as its read point. Both floored to "0 slots (about 0 hours)",
        // published as if measured; the age has to be absent instead.
        let mut dossier = dossier_for([3u8; 32]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(1), None));
        dossier.read_at = Some(ReadAt::Solana(Slot(444_007_819)));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::Age).is_none());

        dossier.read_at = Some(ReadAt::Solana(Slot(444_007_820)));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::Age).is_none());

        // One slot later is a real age.
        dossier.read_at = Some(ReadAt::Solana(Slot(444_007_821)));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert_eq!(fact_of(&sheet, Kind::Age).map(|f| f.values[0]), Some(1.0));
    }

    #[test]
    fn a_robinhood_launch_gives_an_age_a_dev_buy_and_its_signal() {
        let sheet = FactSheet::build(
            &robinhood_launched(Some(5_400), Some(500_000_000_000_000_000)),
            None,
            None,
            None,
            None,
        );
        let age = fact_of(&sheet, Kind::Age).expect("an age fact");
        assert_eq!(age.rendered, "about 1.5 hours ago");
        assert_eq!(age.values, [1.5]);
        let buy = fact_of(&sheet, Kind::DevBuy).expect("a dev buy fact");
        assert_eq!(buy.values, [0.5]);
        assert!(buy.rendered.ends_with(" ETH"), "{}", buy.rendered);
        assert_eq!(sheet.signals, [Signal::CreatorBoughtOwnLaunch]);
        assert!(
            !sheet.unknown.iter().any(|u| u.contains("launch block")),
            "{:?}",
            sheet.unknown
        );
    }

    /// The share card's whole source for the token's name.
    ///
    /// `check.rs` stashes `FactSheet::untrusted`'s "token name" and "token
    /// symbol" entries for `card.rs` to draw, so a Robinhood token whose name
    /// never reaches this list is a card with a blank where the name goes --
    /// which is what shipped until 2026-09-17, because `push_chain_launch`
    /// had no `untrusted` parameter at all. Re-apply the bug by dropping the
    /// pushes and this fails on the first assertion.
    #[test]
    fn a_robinhood_token_name_reaches_the_untrusted_list_and_never_the_facts() {
        let sheet = FactSheet::build(
            &named_robinhood_launch(Some(5_400), Some(1), Some("Pepe Token"), Some("PEPE")),
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            sheet.untrusted,
            [
                ("token name".to_owned(), "Pepe Token".to_owned()),
                ("token symbol".to_owned(), "PEPE".to_owned()),
            ]
        );
        assert!(
            !sheet
                .facts
                .iter()
                .any(|f| f.rendered.contains("Pepe") || f.label.contains("Pepe")),
            "a launcher's chosen string became a fact the model may assert: {:?}",
            sheet.facts
        );
    }

    /// One call reading and the other not is two calls, not one.
    #[test]
    fn a_name_that_read_survives_a_symbol_that_did_not() {
        let sheet = FactSheet::build(
            &named_robinhood_launch(Some(5_400), Some(1), Some("Pepe Token"), None),
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            sheet.untrusted,
            [("token name".to_owned(), "Pepe Token".to_owned())]
        );
        let blank = FactSheet::build(
            &named_robinhood_launch(Some(5_400), Some(1), None, None),
            None,
            None,
            None,
            None,
        );
        assert!(blank.untrusted.is_empty(), "{:?}", blank.untrusted);
    }

    #[test]
    fn a_robinhood_launch_older_than_two_days_is_stated_in_days() {
        // 47.96h rounds to 48.0h, the boundary itself: days from there on.
        let sheet = FactSheet::build(
            &robinhood_launched(Some(172_656), Some(0)),
            None,
            None,
            None,
            None,
        );
        let age = fact_of(&sheet, Kind::Age).expect("an age fact");
        assert_eq!(age.rendered, "about 2 days ago");
        assert_eq!(age.values, [2.0, 48.0]);
        let under = FactSheet::build(
            &robinhood_launched(Some(172_440), Some(0)),
            None,
            None,
            None,
            None,
        );
        assert_eq!(
            fact_of(&under, Kind::Age).expect("an age fact").rendered,
            "about 47.9 hours ago"
        );
    }

    #[test]
    fn a_robinhood_launch_with_no_buy_says_zero_and_raises_no_signal() {
        let sheet = FactSheet::build(&robinhood_launched(None, Some(0)), None, None, None, None);
        assert!(fact_of(&sheet, Kind::Age).is_none());
        let buy = fact_of(&sheet, Kind::DevBuy).expect("a measured zero");
        assert_eq!(buy.values, [0.0]);
        assert!(sheet.signals.is_empty(), "{:?}", sheet.signals);
    }

    #[test]
    fn an_unread_robinhood_launch_buy_is_unseen_not_zero() {
        let sheet = FactSheet::build(&robinhood_launched(Some(60), None), None, None, None, None);
        assert!(fact_of(&sheet, Kind::DevBuy).is_none());
        assert!(fact_of(&sheet, Kind::DevBuyUnseen).is_some());
        assert!(sheet.signals.is_empty(), "{:?}", sheet.signals);
    }

    #[test]
    fn a_robinhood_sheet_without_a_launch_still_names_the_launch_block_unknown() {
        let sheet = FactSheet::build(&robinhood_dossier_for([1u8; 20]), None, None, None, None);
        assert!(fact_of(&sheet, Kind::Age).is_none());
        assert!(
            sheet
                .unknown
                .iter()
                .any(|u| u == "the launch block could not be read"),
            "{:?}",
            sheet.unknown
        );
    }

    #[test]
    fn robinhood_holders_are_counted_and_the_top_share_is_an_address() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.holders = Some(realorrug_onchain::Holders {
            count: 42,
            largest_share_bps: Some(1_250),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let count = fact_of(&sheet, Kind::Holders).expect("a holder count");
        assert_eq!(count.values, [42.0]);
        assert_eq!(count.rendered, "42");
        let top = fact_of(&sheet, Kind::LargestHolderShare).expect("a top share");
        assert_eq!(top.rendered, "12.5%");
        let words: String = top.clauses.iter().map(|c| c.text.as_str()).collect();
        assert!(
            !words.contains("wallet") && !words.contains("whale"),
            "{words}"
        );

        dossier.holders = Some(realorrug_onchain::Holders {
            count: 0,
            largest_share_bps: None,
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::Holders).is_some());
        assert!(fact_of(&sheet, Kind::LargestHolderShare).is_none());
    }

    /// A funding result: `checked` candidates, of `buyers`, with `shared`
    /// addresses funding `funded` of them each.
    fn funding_of(buyers: u32, checked: u32, shared: &[u32], gaps: &[&str]) -> Funding {
        let candidate = |i: u32| realorrug_onchain::Candidate {
            address: realorrug_robinhood::Address([u8::try_from(i).unwrap_or(0); 20]).to_string(),
            bought_wei: 1,
            bought_tokens: Some(1),
            first_purchase_block: 64,
            is_contract: Some(false),
            nonce_before_launch: Some(0),
            funders: Vec::new(),
            funding_complete: true,
        };
        Funding {
            buyers,
            selected: checked,
            coverage_bps: Some(7_500),
            rule: "test",
            checked: (0..checked).map(candidate).collect(),
            shared: shared
                .iter()
                .enumerate()
                .map(|(i, funded)| realorrug_onchain::SharedFunder {
                    address: realorrug_robinhood::Address(
                        [0xf0 + u8::try_from(i).unwrap_or(0); 20],
                    )
                    .to_string(),
                    funded: *funded,
                })
                .collect(),
            gaps: gaps.iter().map(|g| (*g).to_owned()).collect(),
            cu_spent: 0,
        }
    }

    #[test]
    fn a_solana_funding_with_no_coverage_never_renders_a_percentage() {
        // Solana's `coverage_bps` is `None` (no quote amount to weigh a
        // share against); the checked-count sentence must still say who was
        // checked without inventing a share figure.
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        let mut funding = funding_of(4, 4, &[], &[]);
        funding.coverage_bps = None;
        dossier.funding = Some(funding);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let checked = fact_of(&sheet, Kind::FundingChecked).expect("a checked count");
        assert_eq!(checked.rendered, "4 of 4");
        let words = clause_words(checked);
        assert!(
            !words.contains('%'),
            "a None coverage rendered a percentage: {words}"
        );
    }

    /// Words a funding sentence may never use: each names an owner behind
    /// the addresses, which the chain cannot show.
    const OWNERSHIP_WORDS: [&str; 7] = [
        "one person",
        "insiders",
        "same owner",
        "common control",
        "controlled",
        "same group",
        "sybil",
    ];

    fn clause_words(fact: &Fact) -> String {
        fact.clauses
            .iter()
            .map(|c| c.text.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn a_shared_funder_is_a_count_of_the_checked_never_an_owner() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_of(4, 4, &[3], &[]));
        let sheet = FactSheet::build(&dossier, None, None, None, None);

        let checked = fact_of(&sheet, Kind::FundingChecked).expect("a checked count");
        assert_eq!(checked.values, [4.0]);
        assert_eq!(checked.rendered, "4 of 4");
        assert!(
            clause_words(checked).contains("75%"),
            "{}",
            clause_words(checked)
        );

        let shared = fact_of(&sheet, Kind::SharedFunder).expect("a shared funder");
        assert_eq!(shared.values, [3.0]);
        assert_eq!(shared.rendered, "3 of 4");
        let words = clause_words(shared);
        assert!(
            words.contains("3 of the 4 early buyers checked"),
            "the denominator and 'checked' are in the words: {words}"
        );
        for word in OWNERSHIP_WORDS {
            assert!(!words.contains(word), "{word:?} in {words}");
        }
        assert!(
            !sheet.unknown.iter().any(|u| u.contains("funding")),
            "a finished check named a gap: {:?}",
            sheet.unknown
        );
    }

    #[test]
    fn an_exchange_like_hub_funding_every_buyer_is_still_only_a_flow() {
        // Four fresh wallets each withdrew from the same hot wallet before
        // buying: the strongest-looking pattern, and still only a flow.
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_of(9, 4, &[4], &[]));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let shared = fact_of(&sheet, Kind::SharedFunder).expect("a shared funder");
        assert_eq!(shared.rendered, "4 of 4");
        let words = clause_words(shared);
        assert!(words.contains("4 of the 4 early buyers checked"), "{words}");
        assert!(
            words.contains("exchange"),
            "the innocent reading is in the words: {words}"
        );
        for word in OWNERSHIP_WORDS {
            assert!(!words.contains(word), "{word:?} in {words}");
        }
        let checked = fact_of(&sheet, Kind::FundingChecked).expect("a checked count");
        assert_eq!(checked.rendered, "4 of 9", "four checked is not nine");
    }

    #[test]
    fn no_shared_funder_means_no_shared_funder_fact() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_of(4, 4, &[], &[]));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::FundingChecked).is_some());
        assert!(fact_of(&sheet, Kind::SharedFunder).is_none());
    }

    #[test]
    fn a_funding_check_that_stopped_short_names_its_gap() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        let mut funding = funding_of(
            4,
            2,
            &[2],
            &["compute-unit cap of 330 CU reached: 2 of 4 candidates checked"],
        );
        funding.selected = 4;
        dossier.funding = Some(funding);
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let shared = fact_of(&sheet, Kind::SharedFunder).expect("a shared funder");
        assert_eq!(
            shared.rendered, "2 of 2",
            "the denominator is the checked, not the chosen"
        );
        assert!(
            sheet.unknown.iter().any(|u| {
                u == "the funding check did not finish: 2 of 4 chosen early buyers were read"
            }),
            "{:?}",
            sheet.unknown
        );

        // Nothing checked at all: no funding fact, and the gap is named.
        dossier.funding = Some(funding_of(4, 0, &[], &["budget exhausted"]));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::FundingChecked).is_none());
        assert!(
            sheet
                .unknown
                .iter()
                .any(|u| u == "who funded the early buyers could not be read"),
            "{:?}",
            sheet.unknown
        );

        // Nothing checked and no gap: an empty launch window, nothing to say.
        dossier.funding = Some(funding_of(0, 0, &[], &[]));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::FundingChecked).is_none());
        assert!(!sheet.unknown.iter().any(|u| u.contains("funded")));
    }

    #[test]
    fn unreadable_funding_is_named_in_plain_words() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.unavailable.push(realorrug_onchain::Unavailable {
            fact: "funding",
            why: "budget exhausted".to_owned(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(
            sheet
                .unknown
                .iter()
                .any(|u| u == "who funded the early buyers could not be read"),
            "{:?}",
            sheet.unknown
        );
    }

    #[test]
    fn unreadable_holders_are_named_in_plain_words() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.unavailable.push(realorrug_onchain::Unavailable {
            fact: "holders",
            why: "rate limited".to_owned(),
        });
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(
            sheet
                .unknown
                .iter()
                .any(|u| u == "the holders could not be read"),
            "{:?}",
            sheet.unknown
        );
    }

    #[test]
    fn a_robinhood_sheet_carries_no_base_rate_cost_line() {
        // `push_cost` measures Solana/pump.fun fresh launches -- the "850 bps"
        // line names Radar's kernel, not Robinhood Chain. Printing it on a
        // Robinhood sheet states a base rate for the wrong venue, which is a
        // fabricated fact by AGENTS.md §3 rule 2 even though every digit in
        // it is real -- real for a different chain.
        let rates = BaseRates::parse(SNAPSHOT).expect("the published snapshot");
        let sheet = FactSheet::build(
            &robinhood_dossier_for([1u8; 20]),
            Some(&rates),
            None,
            None,
            None,
        );
        let rendered = sheet.render();
        assert!(
            !rendered.contains("round trip Real or Rug's kernel assumes"),
            "a Solana base-rate cost line leaked onto a Robinhood sheet: {rendered}"
        );
        assert!(
            !rendered.contains("expected edge a strategy must clear"),
            "{rendered}"
        );
        assert!(!rendered.contains("bps"), "{rendered}");

        // A matching Solana dossier still carries the line -- Solana behaviour
        // is unchanged by the Robinhood gate.
        let solana_rendered =
            FactSheet::build(&dossier_for([3u8; 32]), Some(&rates), None, None, None).render();
        assert!(
            solana_rendered.contains("round trip Real or Rug's kernel assumes"),
            "{solana_rendered}"
        );
    }

    #[test]
    fn a_robinhood_base_rates_snapshot_produces_the_band_line_on_a_robinhood_sheet() {
        // The mirror of `a_robinhood_sheet_carries_no_base_rate_cost_line`: a
        // Robinhood-chain snapshot's population figures belong on a Robinhood
        // sheet, the same way a Pons v2 creator index's totals do
        // (`a_population_line_is_printed_by_the_index_chain_not_the_token_chain`).
        // `LaunchBlock` itself carries no chain tag -- `Chain::of` reads it off
        // `dossier.mint` -- so this constructs a Robinhood dossier with a
        // launch block directly, which is what `push_population`'s chain gate
        // has to get right regardless of how that block was read.
        let band = |name: &str, lo, hi, x| crate::baserates::Band {
            name: name.to_owned(),
            lo,
            hi,
            fires_on: 0.0,
            never_graduated: 0.0,
            organic: 0.0,
            instant: 0.0,
            p_instant: 0.0,
            x_base_instant: x,
        };
        let robinhood_rates = BaseRates {
            chain: crate::firstparty::Chain::Robinhood,
            measured_on: "2026-09-16".to_owned(),
            aftermath: None,
            outcomes_24h: None,
            launches: 1,
            base_rate_graduates: 0.0,
            base_rate_instant: 0.0,
            bands: vec![band("ten to thirteen", 10, 13, 10.1)],
            round_trip: None,
        };
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(11), None));

        let sheet = FactSheet::build(&dossier, Some(&robinhood_rates), None, None, None);
        let rendered = sheet.render();
        assert!(
            rendered.contains("ten to thirteen"),
            "a Robinhood snapshot's own band was withheld from a Robinhood sheet: {rendered}"
        );
        assert!(
            rendered.contains("as of 2026-09-16"),
            "the band line must carry the snapshot's measurement date: {rendered}"
        );
        assert!(
            sheet.signals.contains(&Signal::LaunchBlockInStrongestBand),
            "{:?}",
            sheet.signals
        );

        // And the mirror: a Solana snapshot says nothing on this same
        // Robinhood dossier -- the chain gate runs on the snapshot, not on
        // whether a snapshot was supplied at all.
        let solana_rates = BaseRates {
            chain: crate::firstparty::Chain::Solana,
            ..robinhood_rates
        };
        let wrong_chain =
            FactSheet::build(&dossier, Some(&solana_rates), None, None, None).render();
        assert!(
            !wrong_chain.contains("ten to thirteen"),
            "a Solana snapshot's band leaked onto a Robinhood sheet: {wrong_chain}"
        );
    }

    #[test]
    fn a_population_line_is_printed_by_the_index_chain_not_the_token_chain() {
        // Until 2026-09-17 this asked whether the *token* was on Robinhood and
        // dropped the population figures if it was. That was right about the
        // only file then in existence (a pump.fun index) and wrong about the
        // test, because it would have gone on hiding a Pons v2 index's totals
        // from the one chain those totals describe.
        //
        // Both halves are asserted, because one alone passes on a `build` that
        // ignores the chain entirely in either direction.
        let population = crate::creator::Population {
            launches: 508_814,
            measured: 506_991,
            organic: 9_060,
            instant: 5_222,
            stillborn: 116_608,
        };
        let solana_index = crate::creator::CreatorIndex {
            chain: crate::firstparty::Chain::Solana,
            watermark_slot: 444_374_676,
            built_at: 1_788_000_000,
            population: Some(population),
            creators: std::collections::BTreeMap::new(),
        };
        let line = "launches Real or Rug has recorded and measured";

        let leaked = FactSheet::build(
            &robinhood_dossier_for([2u8; 20]),
            None,
            Some(&solana_index),
            None,
            None,
        )
        .render();
        assert!(
            !leaked.contains(line),
            "a pump.fun population line leaked onto a Pons v2 sheet: {leaked}"
        );

        let robinhood_index = crate::creator::CreatorIndex {
            chain: crate::firstparty::Chain::Robinhood,
            ..solana_index.clone()
        };
        let printed = FactSheet::build(
            &robinhood_dossier_for([2u8; 20]),
            None,
            Some(&robinhood_index),
            None,
            None,
        )
        .render();
        assert!(
            printed.contains(line),
            "a Pons v2 index's own totals were withheld from a Pons v2 sheet: {printed}"
        );

        // And the mirror, so neither chain is special-cased: a Pons v2 index
        // says nothing on a pump.fun sheet.
        let wrong_way = FactSheet::build(
            &dossier_for([3u8; 32]),
            None,
            Some(&robinhood_index),
            None,
            None,
        )
        .render();
        assert!(
            !wrong_way.contains(line),
            "a Pons v2 population line leaked onto a pump.fun sheet: {wrong_way}"
        );
    }
    #[test]
    fn an_optional_miss_alone_does_not_reach_cant_tell() {
        // `fees`, `capacity` and `creator transactions` are optional per
        // design 0020 §1: a token that read everything required but could not
        // supply one of these three must still reach a real verdict, not
        // `CantTell`. Re-apply the bug -- deleting the `matches!` skip in the
        // `dossier.unavailable` loop above -- and this fails, because *any*
        // nonempty `unknown` forces `CantTell` (`verdict::level`'s own rule
        // 2).
        let mut dossier = dossier_for([5u8; 32]);
        dossier.launch = Some(launch(realorrug_onchain::budget::Count::Exactly(11), None));
        dossier.curve = Some(realorrug_onchain::CurveFacts {
            creator: realorrug_types::ChainAddress::Solana(realorrug_types::Address::new(
                [9u8; 32],
            )),
            complete: false,
            quote_reserves: 6_186_150_833,
            quote_capacity: None,
            quote_asset: None,
            fees: None,
        });
        dossier.unavailable = vec![
            realorrug_onchain::dossier::Unavailable {
                fact: "fees",
                why: "no fee schedule read".to_owned(),
            },
            realorrug_onchain::dossier::Unavailable {
                fact: "capacity",
                why: "curve arithmetic not modelled".to_owned(),
            },
            realorrug_onchain::dossier::Unavailable {
                fact: "creator transactions",
                why: "not counted".to_owned(),
            },
        ];

        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(
            sheet.unknown.is_empty(),
            "an optional miss reached the trusted unknown list: {:?}",
            sheet.unknown
        );
        assert_ne!(
            crate::verdict::level(&sheet),
            crate::verdict::Level::CantTell,
            "an optional-only miss forced CantTell"
        );
    }

    #[test]
    fn the_self_mint_gets_no_special_treatment() {
        // ADR 0033 rule 5 supersedes ADR 0013 constraint 5, which this test
        // used to pin: the analyst's own token is now treated exactly like
        // any other, hints included -- no special handling, no disclosure
        // line. `self_mint` used to make `build` drop every `About::Price`
        // fact and append a "never stated" note for that one address; that
        // filter is gone (`sheet.rs` commit 4d3d1ff). Passing `self_mint`
        // must not change what the sheet says about a dossier at all.
        let own = realorrug_types::Address::new([3u8; 32]);
        let other = realorrug_types::Address::new([4u8; 32]);
        let dossier = dossier_for([3u8; 32]);

        let own_token = FactSheet::build(&dossier, None, None, Some(&own), None).render();
        let stranger = FactSheet::build(&dossier, None, None, Some(&other), None).render();
        let unconfigured = FactSheet::build(&dossier, None, None, None, None).render();

        assert_eq!(
            own_token, stranger,
            "the analyst's own token must be judged on the same rule as any other coin"
        );
        assert_eq!(
            own_token, unconfigured,
            "the analyst's own token must be judged on the same rule as any other coin"
        );
        assert!(
            !own_token.contains("never stated"),
            "a disclosure line leaked back onto the analyst's own token: {own_token}"
        );
    }

    // --- Research 0052 §3.1's S2 row (`Signal::LaunchBlockInStrongestBand`) ---

    /// A sheet built directly with one of S2's five facts already on it and
    /// the signal already fired -- the same "built directly" pattern
    /// [`sheet_with_largest_holder_share`] uses for S5, since
    /// [`FactSheet::build`] never fires this signal on a Robinhood dossier
    /// (no `dossier.launch` recipient band there) or this fact on a Solana
    /// one (no `dossier.funding` there).
    fn sheet_with_s2_fact(kind: Kind, value: f64) -> FactSheet {
        FactSheet {
            mint: "MintOne".to_owned(),
            read_at: None,
            facts: vec![Fact::exact(kind, "x", value, value.to_string())],
            untrusted: Vec::new(),
            unknown: Vec::new(),
            signals: vec![Signal::LaunchBlockInStrongestBand],
            twins: vec![String::new()],
            skipped: Vec::new(),
        }
    }

    #[test]
    fn linked_holdings_of_999_bps_fires_no_factor() {
        let sheet = sheet_with_s2_fact(Kind::WindowBuyersLinkedHoldingsBps, 999.0);
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    #[test]
    fn linked_holdings_of_1000_bps_raises_by_1000() {
        let sheet = sheet_with_s2_fact(Kind::WindowBuyersLinkedHoldingsBps, 1_000.0);
        let found = factors(&sheet);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].delta_bps, 1_000);
        assert_eq!(found[0].grade, Grade::Measured);
    }

    #[test]
    fn linked_holdings_of_1001_bps_also_raises_by_1000() {
        let sheet = sheet_with_s2_fact(Kind::WindowBuyersLinkedHoldingsBps, 1_001.0);
        assert_eq!(factors(&sheet)[0].delta_bps, 1_000);
    }

    #[test]
    fn two_fresh_buyers_fires_no_factor() {
        let sheet = sheet_with_s2_fact(Kind::FreshWindowBuyers, 2.0);
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    #[test]
    fn three_fresh_buyers_raises_by_800() {
        let sheet = sheet_with_s2_fact(Kind::FreshWindowBuyers, 3.0);
        let found = factors(&sheet);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].delta_bps, 800);
        assert_eq!(found[0].grade, Grade::Measured);
    }

    #[test]
    fn sizes_within_ten_percent_raises_by_500() {
        let sheet = sheet_with_s2_fact(Kind::WindowBuySizesWithinTenPercent, 1.0);
        let found = factors(&sheet);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].delta_bps, 500);
        assert_eq!(found[0].grade, Grade::Inferred);
    }

    #[test]
    fn sizes_not_within_ten_percent_fires_no_factor() {
        let sheet = sheet_with_s2_fact(Kind::WindowBuySizesWithinTenPercent, 0.0);
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    #[test]
    fn all_declared_exempt_lowers_by_500() {
        let sheet = sheet_with_s2_fact(Kind::AllWindowBuyersDeclaredExempt, 1.0);
        let found = factors(&sheet);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].delta_bps, -500);
        assert_eq!(found[0].grade, Grade::Measured);
    }

    #[test]
    fn not_all_declared_exempt_fires_no_factor() {
        let sheet = sheet_with_s2_fact(Kind::AllWindowBuyersDeclaredExempt, 0.0);
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    #[test]
    fn band_measured_on_199_launches_lowers_by_300() {
        let sheet = sheet_with_s2_fact(Kind::BandLaunches, 199.0);
        let found = factors(&sheet);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].delta_bps, -300);
        assert_eq!(found[0].grade, Grade::Inferred);
    }

    #[test]
    fn band_measured_on_200_launches_fires_no_factor() {
        let sheet = sheet_with_s2_fact(Kind::BandLaunches, 200.0);
        assert!(factors(&sheet).is_empty(), "{:?}", factors(&sheet));
    }

    #[test]
    fn band_launches_derives_from_fires_on_times_total() {
        let band = crate::baserates::Band {
            name: "1-3".to_owned(),
            lo: 1,
            hi: 3,
            fires_on: 0.2,
            never_graduated: 0.0,
            organic: 0.0,
            instant: 0.0,
            p_instant: 0.0,
            x_base_instant: 0.0,
        };
        assert_eq!(band_launches(&band, 1_000), (200.0, 200));
        assert_eq!(band_launches(&band, 995), (199.0, 199));
    }

    /// A candidate whose fields the S2 buyer-derived facts read: an address,
    /// how much it spent (always read) and bought in tokens (may be
    /// unread), and whether it was fresh (may be unread).
    fn s2_candidate(
        i: u8,
        bought_wei: u128,
        bought_tokens: Option<u128>,
        nonce_before_launch: Option<u64>,
    ) -> realorrug_onchain::Candidate {
        realorrug_onchain::Candidate {
            address: realorrug_robinhood::Address([i; 20]).to_string(),
            bought_wei,
            bought_tokens,
            first_purchase_block: 64,
            is_contract: Some(false),
            nonce_before_launch,
            funders: Vec::new(),
            funding_complete: true,
        }
    }

    fn funding_with(buyers: u32, checked: Vec<realorrug_onchain::Candidate>) -> Funding {
        Funding {
            buyers,
            selected: u32::try_from(checked.len()).unwrap_or(0),
            coverage_bps: Some(7_500),
            rule: "test",
            checked,
            shared: Vec::new(),
            gaps: Vec::new(),
            cu_spent: 0,
        }
    }

    #[test]
    fn missing_supply_is_a_coverage_gap_and_leaves_the_holdings_fact_absent() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_with(1, vec![s2_candidate(1, 1, Some(1), Some(0))]));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::WindowBuyersLinkedHoldingsBps).is_none());
        assert!(
            sheet
                .skipped
                .iter()
                .any(|s| s.contains("linked holdings needs the launch's total supply")),
            "{:?}",
            sheet.skipped
        );
    }

    #[test]
    fn a_candidate_with_no_token_amount_is_excluded_from_the_holdings_sum() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.chain_launch = Some(realorrug_onchain::ChainLaunch {
            block: 64,
            age_seconds: None,
            dev_buy_wei: None,
            dev_buy_tokens: None,
            supply: Some(10_000),
            name: None,
            symbol: None,
            correlated_selling: None,
        });
        dossier.funding = Some(funding_with(
            2,
            vec![
                s2_candidate(1, 1, Some(1_000), Some(0)),
                s2_candidate(2, 1, None, Some(0)),
            ],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let bps = fact_of(&sheet, Kind::WindowBuyersLinkedHoldingsBps)
            .expect("one readable candidate is enough to publish a sum");
        assert_eq!(
            bps.values,
            [1_000.0],
            "candidate 2's unread amount counted as zero"
        );
        assert!(bps.rendered.contains("1 of 2 checked candidates excluded"));
    }

    #[test]
    fn no_readable_token_amount_at_all_is_a_coverage_gap() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.chain_launch = Some(realorrug_onchain::ChainLaunch {
            block: 64,
            age_seconds: None,
            dev_buy_wei: None,
            dev_buy_tokens: None,
            supply: Some(10_000),
            name: None,
            symbol: None,
            correlated_selling: None,
        });
        dossier.funding = Some(funding_with(1, vec![s2_candidate(1, 1, None, Some(0))]));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::WindowBuyersLinkedHoldingsBps).is_none());
        assert!(
            sheet.skipped.iter().any(|s| s.contains("none were read")),
            "{:?}",
            sheet.skipped
        );
    }

    #[test]
    fn an_unread_nonce_is_a_coverage_gap_for_the_fresh_count() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_with(
            2,
            vec![
                s2_candidate(1, 1, Some(1), Some(0)),
                s2_candidate(2, 1, Some(1), None),
            ],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::FreshWindowBuyers).is_none());
        assert!(
            sheet
                .skipped
                .iter()
                .any(|s| s.contains("at least one was not read")),
            "{:?}",
            sheet.skipped
        );
    }

    #[test]
    fn buy_sizes_exactly_ten_percent_apart_are_within() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        // 90 vs 100: smallest >= largest - largest/10, closed at exactly 10%
        // (mirrors `sizes_within_ten_percent`'s own boundary test in
        // `realorrug-onchain/src/wallets.rs`).
        dossier.funding = Some(funding_with(
            2,
            vec![
                s2_candidate(1, 90, Some(1), Some(0)),
                s2_candidate(2, 100, Some(1), Some(0)),
            ],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let f = fact_of(&sheet, Kind::WindowBuySizesWithinTenPercent).expect("two candidates");
        assert_eq!(f.rendered, "within 10%");
    }

    #[test]
    fn buy_sizes_just_over_ten_percent_apart_are_not_within() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_with(
            2,
            vec![
                s2_candidate(1, 89, Some(1), Some(0)),
                s2_candidate(2, 100, Some(1), Some(0)),
            ],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let f = fact_of(&sheet, Kind::WindowBuySizesWithinTenPercent).expect("two candidates");
        assert_eq!(f.rendered, "not within 10%");
    }

    /// An exemption for the same address [`s2_candidate`] builds -- all 20
    /// bytes set to `i`, not [`robinhood_address`]'s "zeros but the last
    /// byte" scheme, so a test pairing the two actually names the same
    /// address.
    fn s2_exemption(i: u8, source: ExemptionSource) -> Exemption {
        Exemption {
            address: realorrug_types::ChainAddress::Robinhood(realorrug_robinhood::Address(
                [i; 20],
            )),
            source,
        }
    }

    #[test]
    fn every_full_window_buyer_declared_exempt_publishes_the_fact() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_with(
            2,
            vec![
                s2_candidate(1, 1, Some(1), Some(0)),
                s2_candidate(2, 1, Some(1), Some(0)),
            ],
        ));
        dossier.powers = Some(powers_with(
            0,
            None,
            vec![
                s2_exemption(1, ExemptionSource::Declared),
                s2_exemption(2, ExemptionSource::Declared),
            ],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let f = fact_of(&sheet, Kind::AllWindowBuyersDeclaredExempt).expect("full window known");
        assert_eq!(f.rendered, "all declared-exempt");
    }

    #[test]
    fn one_undeclared_buyer_means_not_all_declared_exempt() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_with(
            2,
            vec![
                s2_candidate(1, 1, Some(1), Some(0)),
                s2_candidate(2, 1, Some(1), Some(0)),
            ],
        ));
        dossier.powers = Some(powers_with(
            0,
            None,
            vec![s2_exemption(1, ExemptionSource::Declared)],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let f = fact_of(&sheet, Kind::AllWindowBuyersDeclaredExempt).expect("full window known");
        assert_eq!(f.rendered, "not all declared-exempt");
    }

    #[test]
    fn a_declared_exemption_for_another_address_does_not_cover_the_buyer() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_with(1, vec![s2_candidate(1, 1, Some(1), Some(0))]));
        dossier.powers = Some(powers_with(
            0,
            None,
            vec![s2_exemption(9, ExemptionSource::Declared)],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let f = fact_of(&sheet, Kind::AllWindowBuyersDeclaredExempt).expect("full window known");
        assert_eq!(f.rendered, "not all declared-exempt");
    }

    #[test]
    fn fresh_window_buyers_counts_only_zero_nonce_candidates() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_with(
            3,
            vec![
                s2_candidate(1, 1, Some(1), Some(0)),
                s2_candidate(2, 1, Some(1), Some(0)),
                s2_candidate(3, 1, Some(1), Some(5)),
            ],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let f = fact_of(&sheet, Kind::FreshWindowBuyers).expect("every nonce read");
        assert_eq!(f.values, [2.0]);
    }

    #[test]
    fn a_candidate_with_an_unreadable_address_is_excluded_from_the_holdings_sum() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.chain_launch = Some(realorrug_onchain::ChainLaunch {
            block: 64,
            age_seconds: None,
            dev_buy_wei: None,
            dev_buy_tokens: None,
            supply: Some(10_000),
            name: None,
            symbol: None,
            correlated_selling: None,
        });
        let mut unreadable = s2_candidate(2, 1, Some(1_000), Some(0));
        unreadable.address = "not an address".to_owned();
        dossier.funding = Some(funding_with(
            2,
            vec![s2_candidate(1, 1, Some(1_000), Some(0)), unreadable],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        let bps = fact_of(&sheet, Kind::WindowBuyersLinkedHoldingsBps).expect("one readable");
        assert_eq!(bps.values, [1_000.0]);
        assert!(
            bps.rendered.contains("1 of 2 checked candidates excluded"),
            "{}",
            bps.rendered
        );
    }

    #[test]
    fn missing_exemption_list_is_a_coverage_gap_when_the_full_window_is_known() {
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_with(1, vec![s2_candidate(1, 1, Some(1), Some(0))]));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::AllWindowBuyersDeclaredExempt).is_none());
        assert!(
            sheet
                .skipped
                .iter()
                .any(|s| s.contains("needs this launch's exemption list")),
            "{:?}",
            sheet.skipped
        );
    }

    #[test]
    fn a_sample_short_of_the_full_window_never_fires_the_exemption_fact_or_a_gap() {
        // Two buyers total, one checked (a sample, not the full window): the
        // exemption fact must not fire even with an exemption list present,
        // and this is not a read failure, so it is not a coverage gap
        // either -- see `Kind::AllWindowBuyersDeclaredExempt`'s doc comment.
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_with(2, vec![s2_candidate(1, 1, Some(1), Some(0))]));
        dossier.powers = Some(powers_with(
            0,
            None,
            vec![s2_exemption(1, ExemptionSource::Declared)],
        ));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::AllWindowBuyersDeclaredExempt).is_none());
        assert!(
            !sheet.skipped.iter().any(|s| s.contains("declared-exempt")),
            "a sample, not a read failure, must not be logged as a gap: {:?}",
            sheet.skipped
        );
    }

    #[test]
    fn an_empty_checked_list_fires_none_of_the_four_buyer_facts_or_gaps() {
        // `funding.buyers == 0 == checked.len()` satisfies the exemption
        // gate's equality on its own; the `!is_empty()` operand is what
        // keeps an empty window from publishing a vacuous "all declared".
        let mut dossier = robinhood_dossier_for([1u8; 20]);
        dossier.funding = Some(funding_with(0, Vec::new()));
        dossier.powers = Some(powers_with(0, None, Vec::new()));
        let sheet = FactSheet::build(&dossier, None, None, None, None);
        assert!(fact_of(&sheet, Kind::AllWindowBuyersDeclaredExempt).is_none());
        assert!(fact_of(&sheet, Kind::FreshWindowBuyers).is_none());
        assert!(fact_of(&sheet, Kind::WindowBuySizesWithinTenPercent).is_none());
    }
}
