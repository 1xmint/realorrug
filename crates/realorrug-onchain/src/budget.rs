// SPDX-License-Identifier: Apache-2.0
//! The bounds a dossier is built inside.
//!
//! # Why this is a type and not three arguments
//!
//! Every read this crate performs is triggered by a stranger. The public
//! analyst answers an `@`-mention, and the mint in that mention is chosen by
//! whoever sent it — including someone who picked the most expensive token on
//! the chain on purpose. A creator with four hundred launches, a token with ten
//! thousand transactions in its launch slot, and an endpoint that has begun to
//! rate-limit are all the *normal* case for a public account, not the edge one.
//!
//! So the cost of answering has to be bounded before the question is read,
//! rather than discovered while answering it. [`Budget`] is that bound, it is
//! passed by value into the read path, and it is decremented by the client
//! itself rather than by its callers — a caller that forgets is the failure this
//! shape prevents.
//!
//! # Exhaustion is a fact, not an error
//!
//! Running out of budget does **not** fail the dossier. It truncates it, and the
//! truncation is reported: a recipient count that hit the cap comes back as
//! [`Count::AtLeast`] rather than as a number. AGENTS.md rule 9 — absent is not
//! zero, and unknown is not safe — is the whole of the reasoning. "Six
//! recipients" and "at least six recipients, we stopped counting" are different
//! claims, and publishing the first when only the second was measured is exactly
//! the kind of confident wrongness the account exists not to be.

use std::time::{Duration, Instant};

/// How many RPC calls one dossier may make.
///
/// Sized from the shape of the work rather than picked round: one signature
/// page, one transaction per signature in the launch slot, the curve account,
/// the fee config, and a creator lookup. Sixty is comfortably above a normal
/// token and well below anything that could be used as an amplifier.
///
/// This is a single shared pool, not sixty calls *per step* -- research
/// 0056's 2026-09-23 addendum found real captures spending most of it before
/// `dossier::build`'s funding step (slice 6b, [`Budget::grant_calls`]'s own
/// doc) even starts, which is why that step now floors its own share of this
/// pool rather than trusting whatever the steps ahead of it left behind, the
/// same way [`PAGES_PER_WALK`] already floors pages for every named walk.
/// Raising this constant is not the fix: a starved step needs a guaranteed
/// share of the sixty, not a bigger shared pool every step (including a
/// hostile one) can draw down just the same.
pub const DEFAULT_MAX_CALLS: u32 = 60;

/// How many pages of `getSignaturesForAddress` one dossier may walk.
///
/// Each page is a thousand signatures. Three is enough to reach the launch of
/// any token that is still on the bonding curve; a token with more than three
/// thousand signatures has graduated or is being spammed, and in both cases the
/// answer is to say so rather than to keep paging.
pub const DEFAULT_MAX_PAGES: u32 = 3;

/// How many pages a single named walk inside `dossier::build` is topped up
/// with immediately before that walk starts (see [`Budget::grant_pages`]).
/// Same size as [`DEFAULT_MAX_PAGES`]: each walk is answering the same shape
/// of question -- "how far back does this address's history go" -- as the
/// mint's own first walk, so it is sized the same way.
pub const PAGES_PER_WALK: u32 = DEFAULT_MAX_PAGES;

/// How long one dossier may take.
///
/// A reply that arrives after the thread is dead is not worth its cost — this is
/// the product constraint, not a safety one, and it is the reason the whole path
/// reads the chain rather than the store.
pub const DEFAULT_DEADLINE: Duration = Duration::from_secs(20);

/// How many provider compute units one cold dossier may spend.
///
/// Design 0027 §2.4: 2,000 CU per ordinary cold dossier, retries included,
/// so that 200 dossiers a day fit the free plan's 30M CU a month with room
/// for index upkeep. Calls are counted too (`DEFAULT_MAX_CALLS`); this is a
/// second, finer ceiling for the reads whose price is not one unit each --
/// `alchemy_getAssetTransfers` costs six times an `eth_call`.
pub const DEFAULT_MAX_CU: u32 = 2_000;

/// What a bounded read ran out of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Exhausted {
    /// The call allowance was spent.
    Calls,
    /// The page allowance was spent.
    Pages,
    /// The wall clock ran out.
    Deadline,
    /// The compute-unit allowance was spent.
    ComputeUnits,
}

/// The bounds one dossier is built inside.
///
/// Not `Clone`, deliberately. A cloned budget is two budgets, which is a
/// doubled bill and an amplifier a stranger controls.
#[derive(Debug)]
pub struct Budget {
    calls_left: u32,
    pages_left: u32,
    started: Instant,
    deadline: Duration,
    calls_made: u32,
    cu_left: u32,
    cu_spent: u32,
    retries: u32,
    paused: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CALLS, DEFAULT_MAX_PAGES, DEFAULT_DEADLINE)
    }
}

impl Budget {
    /// A budget with the given allowances and [`DEFAULT_MAX_CU`] compute
    /// units.
    #[must_use]
    pub fn new(calls: u32, pages: u32, deadline: Duration) -> Self {
        Self::with_compute_units(calls, pages, deadline, DEFAULT_MAX_CU)
    }

    /// A budget with the given allowances, including its compute units.
    #[must_use]
    pub fn with_compute_units(calls: u32, pages: u32, deadline: Duration, cu: u32) -> Self {
        Self {
            calls_left: calls,
            pages_left: pages,
            started: Instant::now(),
            deadline,
            calls_made: 0,
            cu_left: cu,
            cu_spent: 0,
            retries: 0,
            paused: Duration::ZERO,
        }
    }

    /// Takes `cost` compute units, or refuses without taking any.
    ///
    /// All-or-nothing: a read that costs 120 CU against 100 left is not
    /// made at all, because the provider bills the whole call, not the part
    /// the budget could afford. Separate from [`Budget::take_call`] because
    /// the two ceilings measure different things -- sixty cheap calls fit
    /// the CU allowance easily, and eight expensive ones do not.
    ///
    /// # Errors
    ///
    /// [`Exhausted::ComputeUnits`] when fewer than `cost` units remain.
    pub fn take_cu(&mut self, cost: u32) -> Result<(), Exhausted> {
        if self.cu_left < cost {
            return Err(Exhausted::ComputeUnits);
        }
        self.cu_left -= cost;
        self.cu_spent += cost;
        Ok(())
    }

    /// How many compute units remain, for a read that sizes its own inner
    /// cap from what is actually left (see [`Budget::calls_left`]).
    #[must_use]
    pub const fn cu_left(&self) -> u32 {
        self.cu_left
    }

    /// How many compute units have been spent.
    #[must_use]
    pub const fn cu_spent(&self) -> u32 {
        self.cu_spent
    }

    /// Takes one call, or says why it cannot.
    ///
    /// The deadline is checked first: a budget with calls left but no time left
    /// should report the reason it actually stopped, because "we ran out of
    /// time" and "we ran out of allowance" call for different fixes.
    ///
    /// # Errors
    ///
    /// [`Exhausted`] when the deadline has passed or no call allowance remains.
    pub fn take_call(&mut self) -> Result<(), Exhausted> {
        if self.started.elapsed() >= self.deadline {
            return Err(Exhausted::Deadline);
        }
        if self.calls_left == 0 {
            return Err(Exhausted::Calls);
        }
        self.calls_left -= 1;
        self.calls_made += 1;
        Ok(())
    }

    /// Takes one page.
    ///
    /// Separate from [`Budget::take_call`] because a page is also a call, and
    /// the caller takes both: paging is the one operation whose cost is
    /// unbounded in the *number* of calls rather than in their size, so it
    /// carries its own ceiling as well as drawing on the shared one.
    ///
    /// # Errors
    ///
    /// [`Exhausted::Pages`] when the page allowance is spent.
    pub fn take_page(&mut self) -> Result<(), Exhausted> {
        if self.pages_left == 0 {
            return Err(Exhausted::Pages);
        }
        self.pages_left -= 1;
        Ok(())
    }

    /// Raises the page allowance to at least `floor`, for a named walk about
    /// to start.
    ///
    /// # Why a floor, not a page pool per walk
    ///
    /// `dossier::build` performs several independent signature walks against
    /// one shared [`Budget`]: the mint's own history, the creator's, the
    /// funding candidates', and the creator's cash-flow account. Giving
    /// `Budget` its own notion of distinct walks would mean threading a walk
    /// identifier through every [`Budget::take_page`] call site --
    /// `robinhood.rs` and `market.rs` call it too, for their own single walk
    /// each, and would gain nothing from a multi-walk API they never use.
    /// Simpler: the allowance stays one counter, and the caller that actually
    /// knows when a new named walk begins (`dossier::build`) raises it to a
    /// floor right before that walk runs, so a walk that already burned the
    /// shared pool (a busy mint's three-thousand-signature history, say)
    /// cannot starve the next one down to zero. `max`, not addition: a walk
    /// that starts with pages still unspent from before keeps them rather
    /// than stacking a second full allowance on top, so the guarantee stays
    /// "at least `floor`" and does not compound across steps. The overall
    /// call cap ([`DEFAULT_MAX_CALLS`]) is untouched by this and stays the
    /// one global limit.
    pub fn grant_pages(&mut self, floor: u32) {
        self.pages_left = self.pages_left.max(floor);
    }

    /// Raises the call allowance to at least `floor`, for a named read about
    /// to start whose own inner caps (not `Budget`'s) already bound what it
    /// can spend.
    ///
    /// # Why funding needed this and the other named walks did not
    ///
    /// `Budget::grant_pages`'s doc explains why the *page* allowance is
    /// floored per named walk: `dossier::build`'s steps 1, 3, 5 and 6 are
    /// each one `getSignaturesForAddress` walk of a different address
    /// against the shared pool, and a busy walk ahead of another must not
    /// leave it with zero pages of its own. The *call* allowance was never
    /// given the same treatment, and step 5 (`investigate_solana`, slice 6b)
    /// is the one place that gap matters: unlike the other named walks,
    /// which spend only on paging plus one or two follow-up reads, step 5
    /// pages up to [`crate::wallets::MAX_CANDIDATES`] separate candidates
    /// and fetches up to [`crate::wallets::MAX_FUNDING_TRANSACTIONS`]
    /// transactions for each one -- a call shape no other step comes close
    /// to. On a real capture, steps 1 through 4 routinely leave `calls_left`
    /// well under what step 5 needs even though its own *pages* were
    /// floored, so it starts several candidates and then runs out of calls
    /// partway through -- a "read stopped: Calls" gap on the candidates it
    /// never got to, not a genuine measured absence of a funder (AGENTS.md
    /// rule 8). `Budget::grant_calls`, called once by `dossier::build`
    /// immediately before step 5, exists to close exactly that gap the same
    /// way `grant_pages` already closes it for pages.
    ///
    /// `max`, not addition, for the identical reason `grant_pages` uses
    /// `max`: a budget that already has more than `floor` left keeps it
    /// rather than stacking a second allowance on top, so raising this floor
    /// can only ever help a starved step, never grow the overall
    /// [`DEFAULT_MAX_CALLS`] ceiling a healthy one already respects. Step
    /// 5's own inner caps (`MAX_CANDIDATES`, `MAX_FUNDING_SIGNATURE_PAGES`,
    /// `MAX_FUNDING_TRANSACTIONS`) are what keep this floor itself bounded --
    /// see the constant `dossier::build` sizes it from.
    pub fn grant_calls(&mut self, floor: u32) {
        self.calls_left = self.calls_left.max(floor);
    }

    /// How many calls have been made.
    ///
    /// Reported on the dossier so the cost of an answer is visible next to the
    /// answer. A figure nobody can see is a figure nobody notices doubling.
    #[must_use]
    pub const fn calls_made(&self) -> u32 {
        self.calls_made
    }

    /// How many calls remain, right now, for whoever asks.
    ///
    /// Lets a read that can burn an unbounded number of calls on its own --
    /// the Robinhood holder-log paging in `realorrug-onchain::robinhood` is
    /// the first of these -- size its own inner cap from what today's budget
    /// actually has left, rather than from a constant that ignores how much
    /// the calls before it already spent.
    #[must_use]
    pub const fn calls_left(&self) -> u32 {
        self.calls_left
    }

    /// How long the read has been running.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Records one HTTP 429 retry and the time slept waiting for it, so a
    /// retry that succeeds is no longer invisible on the dossier it paid for.
    pub fn note_retry(&mut self, paused: Duration) {
        self.retries += 1;
        self.paused += paused;
    }

    /// How many HTTP 429 retries this budget's reads have needed.
    #[must_use]
    pub const fn retries(&self) -> u32 {
        self.retries
    }

    /// How long this budget's reads have spent paused on 429 retries.
    #[must_use]
    pub const fn paused(&self) -> Duration {
        self.paused
    }
}

/// A count that may have been cut short.
///
/// The reason this is not a `u32` is AGENTS.md rule 9. A count that stopped at
/// its cap and a count that finished are different facts, and the type is what
/// stops the first being published as the second. `radar-graph` refuses on a
/// recipient count; a truncated count fed to a threshold is a refusal decided by
/// a budget rather than by the chain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Count {
    /// Everything was counted.
    Exactly(u32),
    /// Counting stopped at the bound. The real number is this or larger.
    AtLeast(u32),
}

impl Count {
    /// The number, whether or not it is complete.
    ///
    /// Named to make a caller say what it is doing. Reaching for the number
    /// while discarding whether it was complete is the mistake this enum
    /// exists to make visible, so it is available but never implicit.
    #[must_use]
    pub const fn lower_bound(self) -> u32 {
        match self {
            Self::Exactly(n) | Self::AtLeast(n) => n,
        }
    }

    /// The number, only if it is known to be the whole of it.
    #[must_use]
    pub const fn exact(self) -> Option<u32> {
        match self {
            Self::Exactly(n) => Some(n),
            Self::AtLeast(_) => None,
        }
    }

    /// Whether counting was cut short.
    #[must_use]
    pub const fn is_truncated(self) -> bool {
        matches!(self, Self::AtLeast(_))
    }
}

impl std::fmt::Display for Count {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exactly(n) => write!(f, "{n}"),
            Self::AtLeast(n) => write!(f, "at least {n}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_budget_stops_at_its_call_allowance() {
        let mut budget = Budget::new(2, 1, Duration::from_secs(60));
        assert!(budget.take_call().is_ok());
        assert!(budget.take_call().is_ok());
        assert_eq!(budget.take_call(), Err(Exhausted::Calls));
        // And it stays refused rather than recovering on the next attempt.
        assert_eq!(budget.take_call(), Err(Exhausted::Calls));
        assert_eq!(budget.calls_made(), 2);
    }

    #[test]
    fn a_budget_stops_at_its_page_allowance() {
        let mut budget = Budget::new(60, 2, Duration::from_secs(60));
        assert!(budget.take_page().is_ok());
        assert!(budget.take_page().is_ok());
        assert_eq!(budget.take_page(), Err(Exhausted::Pages));
    }

    #[test]
    fn granting_pages_raises_a_starved_walk_to_the_floor() {
        // A walk that already spent the shared pool would otherwise see
        // every later walk fail immediately with `Exhausted::Pages` -- the
        // page-budget-starvation bug this method exists to prevent.
        let mut budget = Budget::new(60, 1, Duration::from_secs(60));
        assert!(budget.take_page().is_ok());
        assert_eq!(budget.take_page(), Err(Exhausted::Pages));
        budget.grant_pages(2);
        assert!(budget.take_page().is_ok());
        assert!(budget.take_page().is_ok());
        assert_eq!(budget.take_page(), Err(Exhausted::Pages));
    }

    #[test]
    fn granting_pages_never_stacks_on_top_of_what_is_left() {
        // `max`, not addition: a walk with pages still unspent from before it
        // ran must not turn one floor into two full allowances added
        // together, or a later walk's guarantee stops meaning "at least
        // this many" and starts meaning "however much piled up".
        let mut budget = Budget::new(60, 5, Duration::from_secs(60));
        budget.grant_pages(2);
        assert!(budget.take_page().is_ok());
        assert!(budget.take_page().is_ok());
        assert!(budget.take_page().is_ok());
        assert!(budget.take_page().is_ok());
        assert!(budget.take_page().is_ok());
        assert_eq!(budget.take_page(), Err(Exhausted::Pages));
    }

    #[test]
    fn compute_units_are_taken_whole_or_not_at_all() {
        let mut budget = Budget::with_compute_units(60, 3, Duration::from_secs(60), 150);
        assert_eq!(budget.take_cu(120), Ok(()));
        assert_eq!(budget.cu_left(), 30);
        assert_eq!(budget.cu_spent(), 120);
        // 30 left, 120 asked: refused, and nothing is taken for the refusal.
        assert_eq!(budget.take_cu(120), Err(Exhausted::ComputeUnits));
        assert_eq!(budget.cu_left(), 30);
        assert_eq!(budget.cu_spent(), 120);
        // Exactly what is left is still affordable (`<`, not `<=`).
        assert_eq!(budget.take_cu(30), Ok(()));
        assert_eq!(budget.cu_left(), 0);
        // Calls are a separate ceiling: none was taken above.
        assert_eq!(budget.calls_made(), 0);
    }

    #[test]
    fn a_default_budget_carries_the_dossier_cu_allowance() {
        assert_eq!(Budget::default().cu_left(), DEFAULT_MAX_CU);
        assert_eq!(DEFAULT_MAX_CU, 2_000);
    }

    #[test]
    fn a_spent_deadline_refuses_before_the_call_allowance_is_consulted() {
        // The order matters for the report, not just the refusal: a budget with
        // fifty calls left that stopped on time must not say it ran out of
        // calls, or the fix applied will be the wrong one.
        let mut budget = Budget::new(60, 3, Duration::ZERO);
        assert_eq!(budget.take_call(), Err(Exhausted::Deadline));
        assert_eq!(budget.calls_made(), 0);
    }

    #[test]
    fn retries_reports_the_real_count_not_a_fixed_one() {
        // `Budget::retries` -> 1 survived: a budget that never retried must
        // read 0, and one that retried three times must read exactly 3, or a
        // dossier's "retries" figure would lie whenever it was not exactly
        // one.
        let mut budget = Budget::new(60, 3, Duration::from_secs(60));
        assert_eq!(budget.retries(), 0);
        budget.note_retry(Duration::from_millis(1));
        budget.note_retry(Duration::from_millis(1));
        budget.note_retry(Duration::from_millis(1));
        assert_eq!(budget.retries(), 3);
    }

    #[test]
    fn elapsed_reports_real_time_rather_than_zero() {
        // `Budget::elapsed` -> Default::default() survived: nothing asserted the
        // value, and it is reported on every dossier as the cost of an answer.
        // A figure nobody checks is a figure nobody notices doubling.
        let budget = Budget::new(10, 3, Duration::from_secs(60));
        std::thread::sleep(Duration::from_millis(5));
        assert!(
            budget.elapsed() >= Duration::from_millis(5),
            "elapsed must measure something: {:?}",
            budget.elapsed()
        );
    }

    #[test]
    fn lower_bound_returns_the_count_it_was_given() {
        // Replacing this with 0 or 1 survived. It is what a threshold reads, so
        // a constant here would make every refusal a refusal about nothing.
        assert_eq!(Count::Exactly(6).lower_bound(), 6);
        assert_eq!(Count::AtLeast(11).lower_bound(), 11);
        assert_eq!(Count::Exactly(0).lower_bound(), 0);
        assert_eq!(Count::AtLeast(1).lower_bound(), 1);
    }

    #[test]
    fn a_truncated_count_never_presents_itself_as_exact() {
        // The whole reason `Count` exists. `lower_bound` is equal for both, so
        // a consumer reading only that cannot tell them apart -- which is why
        // `exact` is the one a threshold has to go through.
        let cut = Count::AtLeast(6);
        let whole = Count::Exactly(6);
        assert_eq!(cut.lower_bound(), whole.lower_bound());
        assert_eq!(cut.exact(), None);
        assert_eq!(whole.exact(), Some(6));
        assert!(cut.is_truncated());
        assert!(!whole.is_truncated());
        assert_eq!(cut.to_string(), "at least 6");
        assert_eq!(whole.to_string(), "6");
    }
}
