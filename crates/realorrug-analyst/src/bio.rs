// SPDX-License-Identifier: Apache-2.0
//! The bio as a noticeboard: who is winning, who won, who was paid.
//!
//! Design 0009 item 7, [design 0012](../../../docs/design/0012-four-things-the-owner-asked-for.md)
//! ask 1, decided in
//! [design 0014](../../../docs/design/0014-the-four-asks-answered.md). Josh
//! asked for it, the objection was put to him, and he asked for it anyway. That
//! is his call and it is recorded as his.
//!
//! # A bio has no version history, and everything here follows from that
//!
//! A wrong post can be deleted and a wrong reply can be corrected in the
//! thread. A bio write **overwrites the only copy**, silently, with no record
//! anywhere that the old text existed. So this module is built so that the two
//! ways it could destroy something are impossible rather than unlikely:
//!
//! - **It never invents the lead.** [`Bio::from_vars`] returns `None` unless
//!   `REALORRUG_BIO_LEAD` is set, and with no configuration nothing is written at
//!   all. That is AGENTS.md rule 8 in the place it matters most: the failure
//!   this prevents is an unconfigured instance overwriting the account's real
//!   copy with a status line.
//! - **The lead is always first and always whole.** Every branch renders
//!   `<lead> · <status>`, and `the_lead_survives_every_branch` asserts it. The
//!   lead is where the automation disclosure goes when the account does not
//!   carry X's own automated label -- and where the account's own words go when
//!   it does, which is the case here: the account carried `Automated by
//!   @1xmint_` as `@thecabalhunter`, confirmed on 2026-09-07. It was renamed
//!   `@realorrug` on 2026-09-13 and the label has not been re-read since.
//!
//! # Every number goes through the same two checks a reply does
//!
//! A bio is a public statement by the same account, so there is no reason it
//! should be held to a lower standard than a reply.
//! [`realorrug_roast::forbidden::check`] refuses the verdicts and
//! [`realorrug_roast::fidelity::check`] refuses any figure the week's record does
//! not carry. A render that fails either is **not written** -- the previous
//! bio stands, which is the safe direction, because the previous bio was also
//! true when it was written.
//!
//! # It says "leads", never "wins"
//!
//! The mid-week line is raw, unverified engagement: reposts and quotes are
//! unbounded per account (finding S16), so a mid-week leader can be one person
//! with a script. The week's actual winner is decided at close on **verified**
//! engagement, and the two can differ. Saying "leads" is the whole of the
//! difference and it is not a stylistic choice.

use std::fmt::Write as _;

use realorrug_contest::{Balance, Record, Vault, Week};
use realorrug_types::env::env_or_legacy;

/// The longest bio X accepts.
///
/// Every render is cut to this by construction rather than checked afterwards,
/// because a bio truncated by the platform mid-figure is a wrong figure with no
/// record that it was ever right.
pub const MAX: usize = 160;

/// What separates the lead from the status.
const JOIN: &str = " · ";

/// The bio writer's configuration.
///
/// Both fields are required and there is no default for either, which is what
/// makes an unconfigured instance write nothing at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bio {
    /// The fixed first part of every bio, whole and first in every branch.
    ///
    /// **The account's own copy, or the automation disclosure, or both.** This
    /// module does not choose: choosing would mean an agent deciding what a
    /// public profile says, and the one thing the operator cannot get back is
    /// the text that was there before.
    pub lead: String,
}

impl Bio {
    /// From the environment, or `None`.
    ///
    /// Takes a getter, so the rule is testable without setting process-wide
    /// variables — the shape `Prices::from_vars` uses, and for the same reason.
    ///
    /// **A blank lead is unset.** An empty string would render a bio that is
    /// only a status line, which is exactly the overwrite this is built to
    /// prevent.
    #[must_use]
    pub fn from_vars(get: &impl Fn(&str) -> Option<String>) -> Option<Self> {
        let lead = env_or_legacy("REALORRUG_BIO_LEAD", "RADAR_BIO_LEAD", get)?
            .trim()
            .to_owned();
        (!lead.is_empty() && lead.len() < MAX).then_some(Self { lead })
    }

    /// The bio for a week, or `None` when there is nothing to say.
    ///
    /// `None` rather than a bio of only the lead: writing the lead back on its
    /// own would be a call that changes nothing, and this endpoint is metered.
    #[must_use]
    pub fn render(&self, state: &State) -> Option<String> {
        let status = state.status()?;
        let mut out = self.lead.clone();
        out.push_str(JOIN);
        out.push_str(&status);
        (out.chars().count() <= MAX).then_some(out)
    }
}

/// What the bio has to say, in the order the week goes through it.
///
/// An enum rather than three booleans, because the states are exclusive and a
/// caller holding two of them is a caller that can render a bio saying somebody
/// both leads and was paid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// The week is open and somebody is ahead on raw engagement.
    ///
    /// **Raw, and the wording says so.** Reposts and quotes are unbounded per
    /// account (S16), so this is the number a script can move and the week's
    /// actual winner is decided on verified engagement at close.
    Leads {
        /// The Monday the week opened, `YYYY-MM-DD`.
        week: String,
        /// The leader's handle, without the `@`.
        handle: String,
        /// Their raw score.
        points: u64,
    },
    /// The week closed with a winner who has not claimed.
    Won {
        /// The Monday the week opened.
        week: String,
        /// The winner's handle, without the `@`.
        handle: String,
        /// The last date a claim is accepted, `YYYY-MM-DD`.
        until: String,
    },
    /// The prize was paid.
    Paid {
        /// The amount, already rendered in `unit`.
        amount: String,
        /// `ETH`, or `SOL` for a week paid before the move to Robinhood Chain.
        unit: &'static str,
    },
    /// The week is open: what the pool holds and how many people are in.
    ///
    /// **Hunters, not replies.** One person summoning the bot ten times is one
    /// person in the running, and a count of replies would let one busy
    /// account make the week look crowded.
    Open {
        /// The Monday the week opened.
        week: String,
        /// The pool, rendered in ETH and cut, never rounded up.
        pool: String,
        /// Distinct accounts with a published reply on a token this week.
        hunters: usize,
    },
}

impl State {
    /// The status half of the bio, or `None`.
    fn status(&self) -> Option<String> {
        let mut out = String::new();
        match self {
            Self::Leads {
                week,
                handle,
                points,
            } => {
                let _ = write!(out, "Week of {week} leads: @{handle}, {points} pts");
            }
            Self::Won {
                week,
                handle,
                until,
            } => {
                let _ = write!(
                    out,
                    "Won the week of {week}: @{handle}. Claim: reply to the prompt under your post by {until}"
                );
            }
            Self::Paid { amount, unit } => {
                let _ = write!(out, "Paid {amount} {unit} to the week's winner");
            }
            Self::Open {
                week,
                pool,
                hunters,
            } => {
                let noun = if *hunters == 1 { "hunter" } else { "hunters" };
                let _ = write!(
                    out,
                    "Week of {week}: {pool} ETH prize pool, {hunters} {noun} in"
                );
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// Every figure this status states, for the fidelity check.
    ///
    /// The dates are split into their parts the way `weekly::authorise_date`
    /// does, because a checker reading numerals out of the text sees `2026`,
    /// `09` and `07` rather than one date.
    #[must_use]
    pub fn authorised(&self) -> Vec<f64> {
        let mut out = Vec::new();
        let mut date = |d: &str| {
            out.extend(d.split('-').filter_map(|p| p.parse::<f64>().ok()));
        };
        match self {
            Self::Leads {
                week,
                points,
                handle: _,
            } => {
                date(week);
                #[expect(clippy::cast_precision_loss, reason = "a score, far below 2^53")]
                out.push(*points as f64);
            }
            Self::Won {
                week,
                until,
                handle: _,
            } => {
                date(week);
                date(until);
            }
            Self::Paid { amount, unit: _ } => amount_parts(&mut out, amount),
            Self::Open {
                week,
                pool,
                hunters,
            } => {
                date(week);
                amount_parts(&mut out, pool);
                #[expect(clippy::cast_precision_loss, reason = "a head count, far below 2^53")]
                out.push(*hunters as f64);
            }
        }
        out
    }
}

/// The state a closed week's record puts the bio in, or `None`.
///
/// Reads the record and nothing else. The mid-week [`State::Leads`] is **not**
/// produced here, because it needs a platform read the record cannot supply and
/// this function must stay pure — the caller that pays for that read is the one
/// that builds it.
#[must_use]
pub fn state_of(record: &Record, now: u64) -> Option<State> {
    if let Some(payout) = &record.payout {
        return match payout.paid {
            realorrug_contest::Paid::Sol { lamports, .. } => Some(State::Paid {
                amount: render_sol(lamports),
                unit: "SOL",
            }),
            realorrug_contest::Paid::Eth { wei, .. } => Some(State::Paid {
                amount: wei.to_eth(4),
                unit: "ETH",
            }),
        };
    }
    // A voided week says nothing. The void is published on the site with the
    // operator's reason, and a bio has no room for a reason -- a bare "the week
    // was voided" in the one place with no version history is the private
    // correction design 0011 refuses, wearing a public sentence.
    if record.voided.is_some() {
        return None;
    }
    let winner = record.winner.as_ref()?;
    let handle = winner.handle.as_ref()?;
    // Past the window there is nothing for anybody to do, so the bio stops
    // asking. It does not announce the rollover: that is on the history page,
    // where it can say which week and why.
    if !record.accepts_claim_at(now) {
        return None;
    }
    Some(State::Won {
        week: monday_of(record.week),
        handle: handle.clone(),
        until: day_of(record.claim_window_closes_at()),
    })
}

/// How old a pool reading may be and still be quoted as the pool now.
///
/// The open-week line carries no time, so a reader takes the figure as
/// current. A reading older than six hours means the job that takes it has
/// stopped, and the bio then falls back to what the week's record says rather
/// than quote a figure that may have moved a long way since.
pub const POOL_FRESH_SECONDS: u64 = 6 * 3_600;

/// The open-week state, or `None` when there is no fresh ETH pool reading.
///
/// A Solana vault says nothing here: nothing writes one any more, and a SOL
/// figure in an ETH sentence would be a wrong one.
#[must_use]
pub fn open_state(vault: Option<&Vault>, hunters: usize, now: u64) -> Option<State> {
    let vault = vault?;
    let Balance::Eth { wei, .. } = &vault.balance else {
        return None;
    };
    // A reading from the future is a clock fault, not a fresh reading.
    if vault.measured_at > now || now - vault.measured_at > POOL_FRESH_SECONDS {
        return None;
    }
    Some(State::Open {
        week: monday_of(Week::of(now)),
        pool: wei.to_eth(3),
        hunters,
    })
}

/// What the bio says now, from everything it may say.
///
/// **A winner who can still claim comes first.** That line is the only one
/// somebody has to act on, and the claim window is the week after the close,
/// so without this order the open-week line would hide it for all seven days.
/// After that the live pool, because a pool that grows while you watch is the
/// reason to join; and only with no fresh pool reading does a closed week's
/// payout stand in.
#[must_use]
pub fn choose(
    record: Option<&Record>,
    vault: Option<&Vault>,
    hunters: usize,
    now: u64,
) -> Option<State> {
    let closed = record.and_then(|r| state_of(r, now));
    if let Some(won @ State::Won { .. }) = closed {
        return Some(won);
    }
    open_state(vault, hunters, now).or(closed)
}

/// The numerals a rendered amount puts in the text: each side of the point,
/// and the whole value.
fn amount_parts(out: &mut Vec<f64>, amount: &str) {
    out.extend(amount.split('.').filter_map(|p| p.parse::<f64>().ok()));
    if let Ok(v) = amount.parse::<f64>() {
        out.push(v);
    }
}

/// Lamports as SOL, at the precision a prize is worth quoting to.
fn render_sol(lamports: u64) -> String {
    format!(
        "{}.{:04}",
        lamports / 1_000_000_000,
        (lamports % 1_000_000_000) / 100_000
    )
}

/// The Monday a week opens on.
fn monday_of(week: Week) -> String {
    day_of(week.opens_at())
}

/// A timestamp as `YYYY-MM-DD`.
fn day_of(secs: u64) -> String {
    realorrug_types::civil::date_from_days(i64::try_from(secs / 86_400).unwrap_or(i64::MAX))
}

/// Whether a rendered bio may be published.
///
/// The same two checks a reply passes, and for the same reason: a bio is a
/// public statement by the same account. `authorised` is the set the week's
/// record carries, so a figure the record does not hold cannot appear.
///
/// # Errors
///
/// The forbidden phrase, or the number that is not on the record.
pub fn check(text: &str, authorised: &[f64]) -> Result<(), String> {
    if let Some(v) = realorrug_roast::forbidden::check(text).first() {
        return Err(format!("forbidden phrase {:?}: {}", v.phrase, v.because));
    }
    match realorrug_roast::fidelity::check(text, authorised).first() {
        Some(f) => Err(format!("a figure the record does not carry: {}", f.literal)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEEK: Week = Week(2958);

    fn bio() -> Bio {
        Bio {
            lead: "Automated. Reads the chain, states what it measured.".to_owned(),
        }
    }

    fn won() -> State {
        State::Won {
            week: "2026-09-07".to_owned(),
            handle: "somebody".to_owned(),
            until: "2026-09-21".to_owned(),
        }
    }

    fn open() -> State {
        State::Open {
            week: "2026-09-14".to_owned(),
            pool: "0.129".to_owned(),
            hunters: 17,
        }
    }

    #[test]
    fn the_lead_survives_every_branch_and_is_always_first() {
        // **The assertion this module exists for.** A bio write overwrites the
        // only copy, with no version history anywhere, so the one thing that
        // must be impossible is a render that drops the lead -- which is where
        // the automation disclosure goes on an account without X's own label,
        // and where the account's own words go on one with it.
        //
        // Re-apply by rendering the status alone in any branch: this fails.
        let b = bio();
        for state in [
            State::Leads {
                week: "2026-09-07".to_owned(),
                handle: "a".to_owned(),
                points: 12,
            },
            won(),
            State::Paid {
                amount: "0.1234".to_owned(),
                unit: "ETH",
            },
            open(),
        ] {
            let text = b.render(&state).expect("a bio");
            assert!(text.starts_with(&b.lead), "lead not first: {text}");
            assert!(text.contains(&b.lead), "lead not whole: {text}");
            assert!(
                text.chars().count() <= MAX,
                "{} chars: {text}",
                text.chars().count()
            );
        }
    }

    #[test]
    fn an_unconfigured_instance_writes_nothing_rather_than_inventing_a_lead() {
        // Rule 8, in the place it matters most: the failure this prevents is
        // an instance nobody configured overwriting the account's real copy
        // with a status line, silently and unrecoverably.
        assert_eq!(Bio::from_vars(&|_| None), None);
        assert_eq!(Bio::from_vars(&|_| Some("   ".to_owned())), None);
        // And a lead that leaves no room for a status is refused at
        // configuration time rather than producing a bio that is only a lead.
        assert_eq!(Bio::from_vars(&|_| Some("x".repeat(MAX))), None);

        let set = Bio::from_vars(&|k| (k == "REALORRUG_BIO_LEAD").then(|| "  hello  ".to_owned()));
        assert_eq!(set.expect("set").lead, "hello");
    }

    #[test]
    fn a_render_that_would_be_cut_off_is_not_written_at_all() {
        // A bio truncated by the platform mid-figure is a wrong figure, in the
        // one place with no record that it was ever right. `None` leaves the
        // previous bio standing, which was also true when it was written.
        let long = Bio {
            lead: "x".repeat(MAX - 10),
        };
        assert_eq!(long.render(&won()), None);
    }

    #[test]
    fn the_mid_week_line_says_leads_and_never_wins() {
        // Raw engagement: reposts and quotes are unbounded per account (S16),
        // so a mid-week leader can be one person with a script, and the week's
        // actual winner is decided on verified engagement at close. The two
        // can differ, and the wording is the whole of the difference.
        let text = bio()
            .render(&State::Leads {
                week: "2026-09-07".to_owned(),
                handle: "somebody".to_owned(),
                points: 12,
            })
            .expect("a bio");
        assert!(text.contains("leads"), "{text}");
        assert!(!text.to_lowercase().contains("wins"), "{text}");
        assert!(!text.to_lowercase().contains("won"), "{text}");
    }

    #[test]
    fn every_branch_passes_the_two_checks_a_reply_passes() {
        // A bio is a public statement by the same account and is held to the
        // same standard. The fidelity check is the one that bites: it reads
        // every numeral in the text and refuses any the record does not carry.
        let b = bio();
        for state in [
            State::Leads {
                week: "2026-09-07".to_owned(),
                handle: "somebody".to_owned(),
                points: 12,
            },
            won(),
            State::Paid {
                amount: "0.1234".to_owned(),
                unit: "ETH",
            },
            open(),
        ] {
            let text = b.render(&state).expect("a bio");
            assert_eq!(
                check(&text, &state.authorised()),
                Ok(()),
                "{text} :: {:?}",
                state.authorised()
            );
        }
    }

    #[test]
    fn a_figure_the_record_does_not_carry_is_refused() {
        // Re-apply by having `authorised` return an empty set for `Paid`: the
        // amount becomes a number nothing measured, and this catches it.
        let text = bio()
            .render(&State::Paid {
                amount: "0.1234".to_owned(),
                unit: "ETH",
            })
            .expect("a bio");
        assert!(check(&text, &[]).is_err(), "{text}");
        // And the number in the text has to be the number on the record.
        assert!(
            check(
                &text,
                &State::Paid {
                    amount: "9.9999".to_owned(),
                    unit: "ETH",
                }
                .authorised()
            )
            .is_err()
        );
    }

    /// A closed week with a winner who has a handle.
    fn record_with_winner() -> Record {
        let entry = realorrug_contest::Entry {
            reply_id: "r1".to_owned(),
            summoner: "9001".to_owned(),
            mention_id: Some("m1".to_owned()),
            handle: Some("somebody".to_owned()),
            mint: "M".to_owned(),
            at: WEEK.opens_at() + 10,
            metrics: realorrug_contest::Metrics::default(),
        };
        let ranking = realorrug_contest::Ranking {
            ranked: vec![realorrug_contest::Ranked { entry, score: 12 }],
            excluded: Vec::new(),
        };
        Record::close(WEEK, ranking, &realorrug_contest::Rules::published(["op"]))
    }

    #[test]
    fn the_record_decides_the_state_and_a_voided_week_says_nothing() {
        let closed = WEEK.closes_at();

        // A winner inside the window: the claim instruction.
        let record = record_with_winner();
        // The dates, not just the shape. CI turned `secs / 86_400` in `day_of`
        // into `%` and `*` and nothing failed, because nothing read what the
        // dates actually were -- so the bio could have invited the winner to
        // claim by 1970-01-01.
        //
        // Week 2958 opens Monday 2026-09-07 and closes 2026-09-14; the claim
        // window is seven days from the close.
        assert_eq!(
            state_of(&record, closed + 60),
            Some(State::Won {
                week: "2026-09-07".to_owned(),
                handle: "somebody".to_owned(),
                until: "2026-09-21".to_owned(),
            })
        );

        // Paid wins over everything: the money moved and that is the fact.
        let mut paid = record_with_winner();
        paid.payout = Some(realorrug_contest::ledger::Payout {
            recipient: "R".to_owned(),
            paid: realorrug_contest::Paid::Sol {
                lamports: 123_400_000,
                signature: "sig".to_owned(),
            },
            at: closed + 120,
        });
        assert_eq!(
            state_of(&paid, closed + 200),
            Some(State::Paid {
                amount: "0.1234".to_owned(),
                unit: "SOL",
            })
        );

        // A voided week says nothing at all. The reason is published on the
        // site; a bare "the week was voided" in the one place with no version
        // history is a correction nobody can check.
        let mut voided = record_with_winner();
        voided.voided = Some(realorrug_contest::ledger::Voided {
            at: closed + 60,
            reason: "every point came from six accounts made that morning".to_owned(),
        });
        assert_eq!(state_of(&voided, closed + 200), None);

        // Past the claim window there is nothing for anybody to do.
        assert_eq!(
            state_of(&record_with_winner(), record.claim_window_closes_at()),
            None
        );

        // A winner with no handle read is not named as a number.
        let mut nameless = record_with_winner();
        nameless.winner.as_mut().expect("winner").handle = None;
        assert_eq!(state_of(&nameless, closed + 60), None);
    }

    fn vault(wei: u128, measured_at: u64) -> Vault {
        Vault {
            address: "0xescrow".to_owned(),
            balance: Balance::Eth {
                holder: "0xbot".to_owned(),
                wei: realorrug_contest::Wei(wei),
            },
            measured_at,
        }
    }

    #[test]
    fn the_open_week_line_quotes_a_fresh_eth_pool_and_the_hunters() {
        // Week 2959 opens Monday 2026-09-14.
        let now = Week(2959).opens_at() + 3_600;
        let fresh = vault(129_999_999_999_999_999, now - 60);
        let state = open_state(Some(&fresh), 17, now).expect("a fresh pool");
        assert_eq!(state, open());
        let text = bio().render(&state).expect("a bio");
        assert!(
            text.ends_with("Week of 2026-09-14: 0.129 ETH prize pool, 17 hunters in"),
            "{text}"
        );
        assert_eq!(check(&text, &state.authorised()), Ok(()), "{text}");
        let one = State::Open {
            week: "2026-09-14".to_owned(),
            pool: "0.129".to_owned(),
            hunters: 1,
        };
        assert!(bio().render(&one).expect("a bio").ends_with("1 hunter in"));

        // Exactly six hours old is still quoted; a second more is not, because
        // the line carries no time and a reader takes it as now.
        assert_eq!(POOL_FRESH_SECONDS, 21_600);
        assert!(open_state(Some(&vault(1, now - POOL_FRESH_SECONDS)), 0, now).is_some());
        assert_eq!(
            open_state(Some(&vault(1, now - POOL_FRESH_SECONDS - 1)), 0, now),
            None
        );
        // A reading from the future is a clock fault, not a fresh reading.
        assert_eq!(open_state(Some(&vault(1, now + 1)), 0, now), None);
        // No reading, or a Solana one, says nothing.
        assert_eq!(open_state(None, 3, now), None);
        let sol = Vault {
            balance: Balance::Sol { lamports: 5 },
            ..fresh
        };
        assert_eq!(open_state(Some(&sol), 3, now), None);
    }

    #[test]
    fn a_claimable_win_outranks_the_pool_and_the_pool_outranks_an_old_payout() {
        let record = record_with_winner();
        let closed = WEEK.closes_at();
        let now = closed + 60;
        let fresh = vault(2_000_000_000_000_000_000, now);

        // The winner still has to act, so their line stands all week.
        assert!(matches!(
            choose(Some(&record), Some(&fresh), 4, now),
            Some(State::Won { .. })
        ));

        // Paid in ETH: the pool line replaces it while the pool is fresh...
        let mut paid = record_with_winner();
        paid.payout = Some(realorrug_contest::ledger::Payout {
            recipient: "R".to_owned(),
            paid: realorrug_contest::Paid::Eth {
                wei: realorrug_contest::Wei(123_456_789_000_000_000),
                claim_tx: "c".to_owned(),
                transfer_tx: "t".to_owned(),
            },
            at: closed + 30,
        });
        assert!(matches!(
            choose(Some(&paid), Some(&fresh), 4, now),
            Some(State::Open { hunters: 4, .. })
        ));
        // ...and the payout, in ETH to four places, stands in without one.
        assert_eq!(
            choose(Some(&paid), None, 4, now),
            Some(State::Paid {
                amount: "0.1234".to_owned(),
                unit: "ETH",
            })
        );

        // No record at all (the first week) still shows the pool.
        assert!(matches!(
            choose(None, Some(&fresh), 0, now),
            Some(State::Open { .. })
        ));
        assert_eq!(choose(None, None, 0, now), None);
    }
}
