// SPDX-License-Identifier: Apache-2.0
//! The spend meter: the thing standing between a bug and an unbounded bill.

use realorrug_types::MicroUsd;

/// Hard ceilings on what Radar may spend. Deny-by-default: a request that would
/// breach any one of these is refused, and no caller can override it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Budget {
    /// The most any single call may cost. Catches a mispriced catalogue entry or
    /// a fat-fingered config before it catches the daily cap.
    pub per_call_max: MicroUsd,
    /// The most that may be spent in one accounting day.
    pub daily_max: MicroUsd,
    /// The most that may be spent in one accounting month (ADR 0039 decision
    /// 5). The widest of the three ceilings, checked last: a call that fits
    /// the per-call and daily caps can still be the one that would tip the
    /// month over its allowance.
    pub monthly_max: MicroUsd,
}

impl Budget {
    /// A budget that refuses everything. The correct default for a meter whose
    /// config has not loaded yet: spending nothing is always recoverable.
    pub const CLOSED: Self = Self {
        per_call_max: MicroUsd::ZERO,
        daily_max: MicroUsd::ZERO,
        monthly_max: MicroUsd::ZERO,
    };
}

/// Why a spend was refused.
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
pub enum Refusal {
    /// The call costs more than the per-call ceiling allows.
    #[error("call would cost {cost}, over the per-call ceiling of {ceiling}")]
    OverPerCallCeiling {
        /// What the call would cost.
        cost: MicroUsd,
        /// The configured ceiling.
        ceiling: MicroUsd,
    },
    /// The call would take the day over its cap.
    #[error("call would cost {cost}, and {spent} of {cap} is already committed today")]
    OverDailyCap {
        /// What the call would cost.
        cost: MicroUsd,
        /// Already spent or committed today.
        spent: MicroUsd,
        /// The configured daily cap.
        cap: MicroUsd,
    },
    /// The call would take the accounting month over its cap (ADR 0039
    /// decision 5) -- the monthly stop, checked after the per-call and daily
    /// ceilings both pass.
    #[error("call would cost {cost}, and {spent} of {cap} is already committed this month")]
    OverMonthlyCap {
        /// What the call would cost.
        cost: MicroUsd,
        /// Already spent or committed this accounting month.
        spent: MicroUsd,
        /// The configured monthly cap.
        cap: MicroUsd,
    },
}

/// An authorisation to spend, issued by the meter and consumed exactly once.
///
/// Carries an id so a caller cannot settle the same commitment twice and quietly
/// halve its recorded spend. Deliberately not `Clone` or `Copy`.
#[derive(PartialEq, Eq, Debug)]
pub struct Commitment {
    id: u64,
    reserved: MicroUsd,
}

impl Commitment {
    /// What the meter set aside for this call.
    #[must_use]
    pub const fn reserved(&self) -> MicroUsd {
        self.reserved
    }
}

/// Tracks spend against [`Budget`] and refuses anything that would breach it.
///
/// Has no clock. The accounting day is passed in by the caller, which is what
/// makes the meter replayable: feeding a recorded sequence of calls back through
/// a fresh meter reproduces the same refusals in the same places, and a test can
/// cross midnight without waiting.
#[derive(Debug)]
pub struct Meter {
    budget: Budget,
    day: u64,
    month: u64,
    settled_today: MicroUsd,
    settled_month: MicroUsd,
    committed: MicroUsd,
    next_id: u64,
    refusals: u64,
}

impl Meter {
    /// A meter starting on `day` with nothing spent.
    #[must_use]
    pub const fn new(budget: Budget, day: u64) -> Self {
        Self {
            budget,
            day,
            month: month_of(day),
            settled_today: MicroUsd::ZERO,
            settled_month: MicroUsd::ZERO,
            committed: MicroUsd::ZERO,
            next_id: 1,
            refusals: 0,
        }
    }

    /// Total committed or settled today.
    #[must_use]
    pub const fn spent_today(&self) -> MicroUsd {
        self.settled_today.saturating_add(self.committed)
    }

    /// Total committed or settled this accounting month.
    ///
    /// Shares `committed` with [`Meter::spent_today`] on purpose: an in-flight
    /// call is real money owed against both the day's cap and the month's, and
    /// there is only one reservation to make for it, not two.
    #[must_use]
    pub const fn spent_month(&self) -> MicroUsd {
        self.settled_month.saturating_add(self.committed)
    }

    /// How many spends have been refused. A non-zero count that keeps climbing
    /// means the budget is wrong or something is looping; it belongs on the
    /// ops page.
    #[must_use]
    pub const fn refusals(&self) -> u64 {
        self.refusals
    }

    /// Reserves budget for a call.
    ///
    /// In-flight commitments count against the cap, so a burst of concurrent
    /// requests cannot collectively overshoot while each individually looks
    /// affordable.
    ///
    /// # Errors
    ///
    /// Returns [`Refusal`] if the call breaches the per-call ceiling or would
    /// take the day over its cap.
    pub fn authorize(&mut self, cost: MicroUsd, day: u64) -> Result<Commitment, Refusal> {
        self.roll_to(day);

        if cost > self.budget.per_call_max {
            self.refusals += 1;
            return Err(Refusal::OverPerCallCeiling {
                cost,
                ceiling: self.budget.per_call_max,
            });
        }
        let would_be = self.spent_today().saturating_add(cost);
        if would_be > self.budget.daily_max {
            self.refusals += 1;
            return Err(Refusal::OverDailyCap {
                cost,
                spent: self.spent_today(),
                cap: self.budget.daily_max,
            });
        }
        // Checked last: the widest of the three ceilings, so a mispriced call
        // or a busy day is caught by the tighter check first and reported as
        // what it actually is.
        let month_would_be = self.spent_month().saturating_add(cost);
        if month_would_be > self.budget.monthly_max {
            self.refusals += 1;
            return Err(Refusal::OverMonthlyCap {
                cost,
                spent: self.spent_month(),
                cap: self.budget.monthly_max,
            });
        }

        self.committed = self.committed.saturating_add(cost);
        let id = self.next_id;
        self.next_id += 1;
        Ok(Commitment { id, reserved: cost })
    }

    /// Records what a call actually cost and releases its reservation.
    ///
    /// `actual` may differ from the reservation — a vendor can price a call
    /// differently from the catalogue, and that drift is itself a signal worth
    /// watching. The reservation is released either way, so a vendor overcharging
    /// can push the day over its cap by at most the drift on calls already in
    /// flight; the next `authorize` sees the real number and refuses.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "consuming the commitment is the point: it is a linear token, and making                   it Copy as clippy suggests would allow the double-settle this prevents"
    )]
    pub fn settle(&mut self, commitment: Commitment, actual: MicroUsd) {
        debug_assert!(
            commitment.id < self.next_id,
            "commitment from another meter"
        );
        self.committed = MicroUsd(
            self.committed
                .get()
                .saturating_sub(commitment.reserved.get()),
        );
        self.settled_today = self.settled_today.saturating_add(actual);
        self.settled_month = self.settled_month.saturating_add(actual);
    }

    /// Releases a reservation for a call that never happened — a transport
    /// failure before the request was billed, or a cache hit discovered late.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "consuming the commitment is the point; see settle"
    )]
    pub fn release(&mut self, commitment: Commitment) {
        debug_assert!(
            commitment.id < self.next_id,
            "commitment from another meter"
        );
        self.committed = MicroUsd(
            self.committed
                .get()
                .saturating_sub(commitment.reserved.get()),
        );
    }

    fn roll_to(&mut self, day: u64) {
        if day > self.day {
            self.day = day;
            self.settled_today = MicroUsd::ZERO;
            // In-flight commitments deliberately survive the roll. They are real
            // money already owed; zeroing them would let a burst spanning
            // midnight spend twice its cap.

            // The month rolls independently of the day: most day rolls stay
            // inside the same month and must not touch settled_month, which
            // is exactly the case a day-boundary test below pins.
            let month = month_of(day);
            if month != self.month {
                self.month = month;
                self.settled_month = MicroUsd::ZERO;
                // Same reasoning as the day roll: an in-flight commitment is
                // real money owed and survives a month roll too.
            }
        }
    }
}

/// What is left for metered spend this month, once the month's fixed
/// services are paid for -- ADR 0039 decision 5's monthly stop.
///
/// `REALORRUG_MONTHLY_USD` is the whole ceiling (the VPS, any flat RPC plan,
/// **and** every metered call any process reads this into); `REALORRUG_FIXED_
/// MONTHLY_USD` is the fixed part alone. What is left for metered spend is
/// the difference, which is why fixed costs come off first rather than being
/// tracked separately: a metered call is refused the moment the two together
/// would exceed the whole ceiling, and fixed costs are the part of that total
/// no process here meters call-by-call.
///
/// Lives in this crate, not in `realorrug-analyst`, because both the
/// analyst's own `Spend` and `realorrug-model`'s `budget_from_vars` need the
/// same monthly stop, and a policy two callers share belongs where neither
/// has to reach into the other to find it.
///
/// `None` -- and so a closed budget at the call site -- when either variable
/// is unset or will not parse, or when the fixed figure is not strictly less
/// than the whole ceiling: a fixed cost at or above the ceiling leaves
/// nothing for a metered call to spend, ever, and that is indistinguishable
/// from "not configured" rather than "configured to spend nothing", so it is
/// refused the same way (AGENTS.md rule 7 and rule 8: a zero-or-negative
/// allowance is not a fact this function invents a spending decision to work
/// around).
///
/// No legacy fallback name: these two variables are new with this stop, not a
/// rename of anything that existed before it.
#[must_use]
pub fn monthly_allowance_from(get: &impl Fn(&str) -> Option<String>) -> Option<MicroUsd> {
    let monthly = get("REALORRUG_MONTHLY_USD")?
        .trim()
        .parse::<f64>()
        .ok()
        .map(MicroUsd::from_dollars)?;
    let fixed = get("REALORRUG_FIXED_MONTHLY_USD")?
        .trim()
        .parse::<f64>()
        .ok()
        .map(MicroUsd::from_dollars)?;
    if fixed >= monthly {
        return None;
    }
    Some(MicroUsd(monthly.get() - fixed.get()))
}

/// Days since the Unix epoch (UTC, midnight) to a month index that increases
/// by exactly one every calendar month: `year * 12 + (month - 1)`.
///
/// Pure and clockless -- the caller (outside this crate) decides what "today"
/// is; this only turns that day number into the accounting month it falls in,
/// the same division of labour [`Meter`]'s own day-scoping already uses.
///
/// Howard Hinnant's `civil_from_days`: integer-only, no floating point, valid
/// for every day on or after the epoch, which is all a `u64` day number can
/// represent.
#[must_use]
#[expect(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "civil_from_days is Hinnant's algorithm verbatim; day as i64 only \
              wraps past ~292 billion years since the epoch, and the result is \
              always non-negative for any day on or after it, which is every \
              value a caller-supplied u64 day can represent"
)]
pub const fn month_of(day: u64) -> u64 {
    let z = day as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], counting March as month 0
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12], January/February roll into the next civil year
    let y = if m <= 2 { y + 1 } else { y };
    (y * 12 + (m - 1)) as u64
}

/// What a meter has to remember across a restart.
///
/// **A budget that forgets is not a budget.** The daily ceiling is the only
/// thing standing between a bug in a paid call and an unbounded bill, and
/// `radar-serve` runs under `Restart=always` — a process that crashes and comes
/// back with a fresh allowance can spend the day's budget as many times as it
/// can crash.
///
/// Serialisable rather than self-persisting, because this crate is pure policy:
/// no clock, no network, no filesystem. The caller decides where it lives, the
/// same way it decides what "today" means.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Ledger {
    /// The accounting day this covers.
    pub day: u64,
    /// Micro-USD committed or settled on that day.
    pub spent: u64,
    /// How many calls were refused for want of budget.
    pub refusals: u64,
    /// The accounting month this covers -- absent from a ledger written before
    /// the monthly stop existed, in which case it deserialises as `0` and
    /// [`Meter::restore`] treats that the same as any other month mismatch: a
    /// fresh month, not a claim on this one's allowance.
    #[serde(default)]
    pub month: u64,
    /// Micro-USD committed or settled this accounting month.
    #[serde(default)]
    pub spent_month: u64,
}

impl Meter {
    /// What this meter would need to be rebuilt.
    ///
    /// Records `spent_today`, which includes **in-flight commitments** as well
    /// as settled spend. That is deliberate and it is the conservative
    /// direction: a process that dies mid-call cannot know whether the call
    /// happened, and assuming it did risks under-spending while assuming it did
    /// not risks paying twice. Spending nothing is always recoverable.
    #[must_use]
    pub const fn ledger(&self) -> Ledger {
        Ledger {
            day: self.day,
            spent: self.spent_today().get(),
            refusals: self.refusals,
            month: self.month,
            spent_month: self.spent_month().get(),
        }
    }

    /// Rebuilds a meter from a saved ledger.
    ///
    /// A ledger from an earlier day is *not* carried forward: the budget is
    /// daily, so yesterday's spend has no claim on today's allowance. Restoring
    /// it as though it were today's would refuse everything until midnight,
    /// which is a different bug and a more visible one.
    ///
    /// The restored spend is treated as settled. There is nothing in flight
    /// after a restart, and a commitment that outlived its process cannot be
    /// released by anyone.
    ///
    /// The day and the month are rolled independently: a restart on a new day
    /// inside the same month resets `settled_today` but must carry
    /// `spent_month` forward -- the month has no less claim on the ledger's
    /// figure just because the process happened to restart. Only a genuine
    /// month mismatch (or a ledger written before the monthly stop existed,
    /// which deserialises with `month: 0`) resets the monthly figure.
    #[must_use]
    pub const fn restore(budget: Budget, ledger: &Ledger, day: u64) -> Self {
        let month = month_of(day);
        let (settled_today, refusals) = if ledger.day == day {
            (MicroUsd(ledger.spent), ledger.refusals)
        } else {
            (MicroUsd::ZERO, 0)
        };
        let settled_month = if ledger.month == month {
            MicroUsd(ledger.spent_month)
        } else {
            MicroUsd::ZERO
        };
        Self {
            budget,
            day,
            month,
            settled_today,
            settled_month,
            committed: MicroUsd::ZERO,
            next_id: 1,
            refusals,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn spend_survives_a_restart() {
        // The failure this exists to stop. radar-serve runs under
        // Restart=always: a process that crashes and comes back with a fresh
        // allowance can spend the day's budget as many times as it can crash,
        // and nothing about that looks wrong from inside any single process.
        let budget = Budget {
            per_call_max: MicroUsd::from_dollars(1.00),
            daily_max: MicroUsd::from_dollars(5.00),
            monthly_max: MicroUsd::from_dollars(1_000.0),
        };
        let mut before = Meter::new(budget, 42);
        // Five calls of ninety cents, each inside the per-call ceiling: real
        // spend arrives in increments, not in one lump.
        for _ in 0..5 {
            let c = before
                .authorize(MicroUsd::from_dollars(0.90), 42)
                .expect("inside both ceilings");
            before.settle(c, MicroUsd::from_dollars(0.90));
        }

        let mut after = Meter::restore(budget, &before.ledger(), 42);
        assert_eq!(after.spent_today(), MicroUsd::from_dollars(4.50));

        // The remaining allowance is what is left, not the whole budget.
        assert!(
            after.authorize(MicroUsd::from_dollars(0.60), 42).is_err(),
            "a restart must not hand back a spent allowance"
        );
        assert!(after.authorize(MicroUsd::from_dollars(0.40), 42).is_ok());
    }

    #[test]
    fn an_in_flight_commitment_is_restored_as_spent() {
        // A process that dies mid-call cannot know whether the call happened.
        // Assuming it did risks under-spending by that amount; assuming it did
        // not risks paying for it twice. Spending nothing is recoverable.
        let budget = Budget {
            per_call_max: MicroUsd::from_dollars(1.00),
            daily_max: MicroUsd::from_dollars(5.00),
            monthly_max: MicroUsd::from_dollars(1_000.0),
        };
        let mut before = Meter::new(budget, 7);
        let _never_settled = before
            .authorize(MicroUsd::from_dollars(0.75), 7)
            .expect("fits");

        let after = Meter::restore(budget, &before.ledger(), 7);
        assert_eq!(
            after.spent_today(),
            MicroUsd::from_dollars(0.75),
            "the commitment outlived the process and nobody can release it"
        );
    }

    #[test]
    fn yesterdays_ledger_does_not_consume_todays_budget() {
        // The budget is daily. Carrying yesterday's spend forward would refuse
        // everything until midnight, which is a different bug -- louder, but
        // still a bug.
        let budget = Budget {
            per_call_max: MicroUsd::from_dollars(1.00),
            daily_max: MicroUsd::from_dollars(5.00),
            monthly_max: MicroUsd::from_dollars(1_000.0),
        };
        let mut yesterday = Meter::new(budget, 100);
        for _ in 0..5 {
            let c = yesterday
                .authorize(MicroUsd::from_dollars(1.00), 100)
                .expect("inside both ceilings");
            yesterday.settle(c, MicroUsd::from_dollars(1.00));
        }
        assert_eq!(yesterday.spent_today(), MicroUsd::from_dollars(5.00));

        let today = Meter::restore(budget, &yesterday.ledger(), 101);
        assert_eq!(
            today.spent_today(),
            MicroUsd::ZERO,
            "a new day, a new budget"
        );
    }

    #[test]
    fn a_ledger_round_trips_through_json() {
        // It has to survive whatever the caller writes it to, and the caller is
        // outside this crate by design -- no filesystem here.
        let budget = Budget {
            per_call_max: MicroUsd::from_dollars(1.00),
            daily_max: MicroUsd::from_dollars(5.00),
            monthly_max: MicroUsd::from_dollars(1_000.0),
        };
        let mut meter = Meter::new(budget, 9);
        let c = meter
            .authorize(MicroUsd::from_dollars(0.25), 9)
            .expect("fits");
        meter.settle(c, MicroUsd::from_dollars(0.25));
        // Refuse one, so the count is not zero and a dropped field would show.
        let _ = meter.authorize(MicroUsd::from_dollars(99.0), 9);

        let ledger = meter.ledger();
        let json = serde_json::to_string(&ledger).expect("serialises");
        let back: Ledger = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back, ledger);
        assert_eq!(back.day, 9);
        assert_eq!(back.spent, MicroUsd::from_dollars(0.25).get());
        assert!(back.refusals > 0, "refusals are part of the record");
    }

    use super::*;

    fn budget() -> Budget {
        Budget {
            per_call_max: MicroUsd::from_dollars(0.10),
            daily_max: MicroUsd::from_dollars(5.0),
            monthly_max: MicroUsd::from_dollars(1_000.0),
        }
    }

    #[test]
    fn a_default_meter_refuses_everything() {
        // Config not loaded must mean "spend nothing", never "spend freely".
        let mut m = Meter::new(Budget::CLOSED, 0);
        assert!(m.authorize(MicroUsd(1), 0).is_err());
    }

    #[test]
    fn an_affordable_call_is_authorised_and_settled() {
        let mut m = Meter::new(budget(), 0);
        let c = m
            .authorize(MicroUsd::from_dollars(0.001), 0)
            .expect("affordable");
        assert_eq!(m.spent_today(), MicroUsd::from_dollars(0.001));
        m.settle(c, MicroUsd::from_dollars(0.001));
        assert_eq!(m.spent_today(), MicroUsd::from_dollars(0.001));
    }

    #[test]
    fn an_overpriced_single_call_is_refused_before_the_daily_cap_is_touched() {
        // A mispriced catalogue entry should be caught by the per-call ceiling,
        // not by burning through the day's budget one call at a time.
        let mut m = Meter::new(budget(), 0);
        let err = m
            .authorize(MicroUsd::from_dollars(1.0), 0)
            .expect_err("over ceiling");
        assert!(matches!(err, Refusal::OverPerCallCeiling { .. }));
        assert_eq!(m.spent_today(), MicroUsd::ZERO);
    }

    #[test]
    fn concurrent_calls_cannot_collectively_overshoot() {
        // Each of these is individually affordable. If in-flight commitments did
        // not count, all fifty would be authorised and the day would land at
        // double its cap.
        let mut m = Meter::new(budget(), 0);
        let mut held = Vec::new();
        for _ in 0..50 {
            if let Ok(c) = m.authorize(MicroUsd::from_dollars(0.10), 0) {
                held.push(c);
            }
        }
        assert_eq!(held.len(), 50, "5.00 cap / 0.10 per call");
        assert!(m.authorize(MicroUsd::from_dollars(0.10), 0).is_err());
        assert_eq!(m.spent_today(), MicroUsd::from_dollars(5.0));
    }

    #[test]
    fn releasing_a_failed_call_returns_its_budget() {
        let mut m = Meter::new(budget(), 0);
        let c = m
            .authorize(MicroUsd::from_dollars(0.05), 0)
            .expect("affordable");
        m.release(c);
        assert_eq!(m.spent_today(), MicroUsd::ZERO);
    }

    #[test]
    fn a_new_day_resets_settled_spend_but_not_money_already_owed() {
        let mut m = Meter::new(budget(), 0);
        let inflight = m
            .authorize(MicroUsd::from_dollars(0.10), 0)
            .expect("affordable");
        let done = m
            .authorize(MicroUsd::from_dollars(0.10), 0)
            .expect("affordable");
        m.settle(done, MicroUsd::from_dollars(0.10));

        // Rolling the day clears what was settled, but the in-flight call is real
        // money already owed and must keep counting.
        let c = m
            .authorize(MicroUsd::from_dollars(0.01), 1)
            .expect("new day");
        assert_eq!(m.spent_today(), MicroUsd::from_dollars(0.11));
        m.settle(c, MicroUsd::from_dollars(0.01));
        m.settle(inflight, MicroUsd::from_dollars(0.10));
    }

    #[test]
    fn a_vendor_overcharging_is_recorded_and_then_refused() {
        // The drift is absorbed on the call already in flight, and the next
        // authorisation sees the real total.
        let mut m = Meter::new(
            Budget {
                per_call_max: MicroUsd::from_dollars(1.0),
                daily_max: MicroUsd::from_dollars(1.0),
                monthly_max: MicroUsd::from_dollars(1_000.0),
            },
            0,
        );
        let c = m
            .authorize(MicroUsd::from_dollars(0.01), 0)
            .expect("cheap as catalogued");
        m.settle(c, MicroUsd::from_dollars(0.99));
        assert_eq!(m.spent_today(), MicroUsd::from_dollars(0.99));
        assert!(m.authorize(MicroUsd::from_dollars(0.02), 0).is_err());
    }

    #[test]
    fn refusals_are_counted_for_the_ops_page() {
        let mut m = Meter::new(Budget::CLOSED, 0);
        for _ in 0..3 {
            let _ = m.authorize(MicroUsd(1), 0);
        }
        assert_eq!(m.refusals(), 3);
    }

    // -- ADR 0039 decision 5: the monthly stop ------------------------------

    fn monthly_budget() -> Budget {
        Budget {
            per_call_max: MicroUsd::from_dollars(100.0),
            daily_max: MicroUsd::from_dollars(100.0),
            monthly_max: MicroUsd::from_dollars(30.0),
        }
    }

    #[test]
    fn a_call_that_lands_exactly_on_the_monthly_cap_is_authorised() {
        // cargo-mutants needs the boundary itself pinned, not just "over
        // refuses": a `>` that should be `>=` shows up only here.
        let mut m = Meter::new(monthly_budget(), 0);
        let c = m
            .authorize(MicroUsd::from_dollars(30.0), 0)
            .expect("exactly the cap is still affordable");
        m.settle(c, MicroUsd::from_dollars(30.0));
        assert_eq!(m.spent_month(), MicroUsd::from_dollars(30.0));
    }

    #[test]
    fn one_micro_usd_over_the_monthly_cap_is_refused() {
        let mut m = Meter::new(monthly_budget(), 0);
        // One micro-USD under the $30 cap: 29_999_999 micro-USD.
        let c = m
            .authorize(MicroUsd(29_999_999), 0)
            .expect("one micro-USD under the cap must be affordable");
        // The reservation already in flight leaves exactly one micro-USD of
        // headroom; asking for two must hit the monthly ceiling, not pass.
        let err = m
            .authorize(MicroUsd(2), 0)
            .expect_err("one micro-USD over the cap");
        assert!(
            matches!(err, Refusal::OverMonthlyCap { .. }),
            "the monthly ceiling, not the daily or per-call one, is what stops this"
        );
        m.release(c);
    }

    #[test]
    fn fixed_costs_leave_only_what_remains_for_metered_spend() {
        // This crate does not know about "fixed" vs "metered" -- that split is
        // the caller's arithmetic (daemon::monthly_allowance_from). What this
        // crate must get right is that whatever allowance the caller computes
        // is the whole of `monthly_max`, with no separate fixed-cost tracking
        // inside the meter itself.
        let budget = Budget {
            per_call_max: MicroUsd::from_dollars(100.0),
            daily_max: MicroUsd::from_dollars(100.0),
            // $90 whole ceiling, $60 fixed, as the packet's own example: the
            // caller hands this crate only the $30 that is left.
            monthly_max: MicroUsd::from_dollars(30.0),
        };
        let mut m = Meter::new(budget, 0);
        let c = m
            .authorize(MicroUsd::from_dollars(30.0), 0)
            .expect("the metered allowance, not the whole ceiling");
        m.settle(c, MicroUsd::from_dollars(30.0));
        assert!(m.authorize(MicroUsd::from_dollars(0.01), 0).is_err());
    }

    #[test]
    fn a_month_boundary_resets_settled_month_but_a_day_boundary_inside_it_does_not() {
        let mut m = Meter::new(monthly_budget(), 20_727); // 2026-10-01
        let c = m
            .authorize(MicroUsd::from_dollars(10.0), 20_727)
            .expect("affordable");
        m.settle(c, MicroUsd::from_dollars(10.0));

        // A day roll still inside October must not touch the month's spend.
        let _ = m.authorize(MicroUsd::from_dollars(0.0), 20_728);
        assert_eq!(
            m.spent_month(),
            MicroUsd::from_dollars(10.0),
            "2026-10-02 is still October"
        );

        // 2026-11-01 is a new month.
        let _ = m.authorize(MicroUsd::from_dollars(0.0), 20_758);
        assert_eq!(
            m.spent_month(),
            MicroUsd::ZERO,
            "November has no claim on October's spend"
        );
    }

    #[test]
    fn a_monthly_ledger_survives_a_restart_and_resets_only_at_the_month_boundary() {
        let budget = monthly_budget();
        let mut before = Meter::new(budget, 20_727); // 2026-10-01
        let c = before
            .authorize(MicroUsd::from_dollars(12.0), 20_727)
            .expect("affordable");
        before.settle(c, MicroUsd::from_dollars(12.0));

        // Restart on a later day, same month: the monthly figure carries over.
        let after_same_month = Meter::restore(budget, &before.ledger(), 20_735);
        assert_eq!(after_same_month.spent_month(), MicroUsd::from_dollars(12.0));

        // Restart in the next month: the monthly figure resets.
        let after_next_month = Meter::restore(budget, &before.ledger(), 20_758);
        assert_eq!(after_next_month.spent_month(), MicroUsd::ZERO);
    }

    #[test]
    fn a_ledger_written_before_the_monthly_stop_still_loads() {
        // The exact shape `serde_json` produced before `month`/`spent_month`
        // existed. `#[serde(default)]` must fill both with zero rather than
        // refuse to deserialise -- an old ledger.json on the live server must
        // not crash the process on the first restart after this ships.
        let old_json = r#"{"day":20727,"spent":4500000,"refusals":2}"#;
        let ledger: Ledger = serde_json::from_str(old_json).expect("old shape still loads");
        assert_eq!(ledger.day, 20_727);
        assert_eq!(ledger.spent, 4_500_000);
        assert_eq!(ledger.month, 0, "absent, defaults to zero");
        assert_eq!(ledger.spent_month, 0, "absent, defaults to zero");

        // Restoring from it must not hand the new month a claim it never
        // earned: month 0 is 0000-01, so any real day mismatches it and the
        // monthly figure starts clean rather than erroring.
        let restored = Meter::restore(monthly_budget(), &ledger, 20_727);
        assert_eq!(restored.spent_month(), MicroUsd::ZERO);
    }

    #[test]
    fn two_lanes_share_one_monthly_total() {
        // realorrug-analyst's X lane and Telegram lane are not two Meters that
        // need reconciling -- they are the same `Spend` passed by `&mut`
        // reference into both, so this is the whole of what "sharing" needs
        // to prove: one lane's settle leaves less for the other to authorize
        // against, with no separate bookkeeping anywhere.
        let mut shared = Meter::new(monthly_budget(), 0);

        let lane1 = shared
            .authorize(MicroUsd::from_dollars(20.0), 0)
            .expect("lane 1 spends first");
        shared.settle(lane1, MicroUsd::from_dollars(20.0));

        // Lane 2 only has what lane 1 left: 30 - 20 = 10.
        let lane2 = shared
            .authorize(MicroUsd::from_dollars(10.0), 0)
            .expect("lane 2 draws on what lane 1 left");
        shared.settle(lane2, MicroUsd::from_dollars(10.0));

        let err = shared
            .authorize(MicroUsd::from_dollars(0.01), 0)
            .expect_err("both lanes together already spent the month's cap");
        assert!(matches!(err, Refusal::OverMonthlyCap { .. }));
    }

    #[test]
    fn month_of_matches_known_calendar_dates() {
        // Days since the Unix epoch for each date, cross-checked by hand
        // against the proleptic Gregorian calendar, including the leap day
        // this algorithm must not mishandle.
        assert_eq!(month_of(10_957), 2_000 * 12); // 2000-01-01
        assert_eq!(month_of(11_016), 2_000 * 12 + 1); // 2000-02-29: the era's own leap day, doe == 146_096
        assert_eq!(month_of(20_454), 2_026 * 12); // 2026-01-01
        assert_eq!(month_of(20_726), 2_026 * 12 + 8); // 2026-09-30
        assert_eq!(month_of(20_727), 2_026 * 12 + 9); // 2026-10-01: a new month
        assert_eq!(month_of(21_184), 2_028 * 12); // 2028-01-01
        assert_eq!(month_of(21_243), 2_028 * 12 + 1); // 2028-02-29: leap day
        assert_eq!(month_of(21_244), 2_028 * 12 + 2); // 2028-03-01: still rolls over on time
    }
}
