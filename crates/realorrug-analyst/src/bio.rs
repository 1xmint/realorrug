// SPDX-License-Identifier: Apache-2.0
//! The bio as a noticeboard: who is winning, who won, who was paid.
//!
//! Design 0009 item 7, [design 0012](../../../docs/design/0012-four-things-the-owner-asked-for.md)
//! ask 1, decided in
//! [design 0014](../../../docs/design/0014-the-four-asks-answered.md). Josh
//! asked for it, the objection was put to him, and he asked for it anyway. That
//! is his call and it is recorded as his.
//!
//! Josh asked (2026-09-17) that the bio show the pool, the week's leaders and
//! the last winner **together**, not as three states that take turns hiding
//! each other. A visitor who only ever sees the account once should not have
//! to catch it on the one hour the payout line happened to be up. So the bio
//! is now a single [`State`] holding whichever of the three parts exist, and a
//! render that does not fit drops parts -- fewer leaders first, then the last
//! winner -- rather than dropping the whole thing. The pool is the one part
//! that is never *dropped for space*: it is either there, from a fresh
//! reading, or it is not there at all, and a render says nothing only when
//! none of the three parts has anything to say.
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
//! The leaderboard line is raw, unverified engagement: reposts and quotes are
//! unbounded per account (finding S16), so a mid-week leader can be one person
//! with a script. The week's actual winner is decided at close on **verified**
//! engagement, and the two can differ. Saying "leads" is the whole of the
//! difference and it is not a stylistic choice.

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

/// How many of the week's leaders a render will ever show.
///
/// A fourth name would not change anybody's decision to join and it is the
/// first thing dropped when the lead is long, so there is no reason to carry
/// more than this out of the ranking in the first place.
pub const MAX_LEADERS: usize = 3;

/// The disclaimer every reply used to end with, until 2026-09-17: the owner
/// decided it belongs on the profile once, not on every post. Fixed rather
/// than operator-configured, appended to whatever lead is set, so an
/// operator cannot configure a bio that omits it, and so it is never at the
/// mercy of `REALORRUG_BIO_LEAD`'s own length -- `from_vars` below folds it
/// into the stored lead and refuses configuration that would not leave room
/// for it, which is what "always fits" means here: checked once, at
/// configuration time, rather than hoped for at every render.
pub const DISCLAIMER: &str = "Not financial advice.";

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
        let operator_lead = env_or_legacy("REALORRUG_BIO_LEAD", "RADAR_BIO_LEAD", get)?
            .trim()
            .to_owned();
        if operator_lead.is_empty() {
            return None;
        }
        // The disclaimer is folded in here, not appended at render time: a
        // lead so long it left the disclaimer no room would otherwise be
        // accepted at configuration time and silently render without it
        // whenever a status line is also present. Refusing it here instead
        // means every configured instance either carries the disclaimer or
        // is not configured at all -- the same "deny by default" shape rule
        // 8 asks for everywhere else.
        let lead = format!("{operator_lead}{JOIN}{DISCLAIMER}");
        (lead.chars().count() < MAX).then_some(Self { lead })
    }

    /// The bio for a week, or `None` when there is nothing to say.
    ///
    /// `None` rather than a bio of only the lead: writing the lead back on its
    /// own would be a call that changes nothing, and this endpoint is metered.
    ///
    /// The status is built to fit what is left after the lead: `state.status`
    /// drops parts -- fewer leaders first, then the last winner -- until the
    /// combined text fits, and only reports `None` when even the pool line
    /// alone does not.
    #[must_use]
    pub fn render(&self, state: &State) -> Option<String> {
        let budget = MAX.checked_sub(self.lead.chars().count() + JOIN.chars().count())?;
        let status = state.status(budget)?;
        let mut out = self.lead.clone();
        out.push_str(JOIN);
        out.push_str(&status);
        // The budget arithmetic above already guarantees this, but the check
        // stays: it is the one invariant a bio truncated mid-figure would
        // violate, and it is cheap enough to hold unconditionally rather than
        // trust the arithmetic that leads to it.
        (out.chars().count() <= MAX).then_some(out)
    }
}

/// One name on the week's raw leaderboard.
///
/// Raw, unverified engagement -- see the module's "leads, never wins" note.
/// Best first is the caller's job: `State::status` shows the first
/// [`MAX_LEADERS`] of whatever order it is given and does not re-sort.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leader {
    /// The leader's handle, without the `@`.
    pub handle: String,
    /// Their raw score.
    pub points: u64,
}

/// The most recent week that reached a winner: closed and waiting on a claim,
/// or already paid.
///
/// Two variants, not the four `State` used to have, because paid and won are
/// the only two states a *last* winner can be in: an open week with no winner
/// yet has nothing to report here, and that case is `None` at the call site,
/// not a third variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LastWinner {
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
}

/// The live pool reading, when there is a fresh one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pool {
    /// The pool, rendered in ETH and cut, never rounded up.
    pub pool: String,
    /// Distinct accounts with a published reply on a token this week.
    ///
    /// **Hunters, not replies.** One person summoning the bot ten times is one
    /// person in the running, and a count of replies would let one busy
    /// account make the week look crowded.
    pub hunters: usize,
}

/// Everything the bio has to say about the week -- whichever of the three
/// parts exist.
///
/// [`choose`] returns `None` only when all three are absent; any one of them
/// on its own is still something to say. `leaders` and `last_winner` are the
/// two parts a render may drop to make room for the lead, in that order --
/// the pool, when present, is never dropped for space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    /// The Monday of the week this render is about.
    pub week: String,
    /// The live pool reading, or `None` with no fresh one to quote.
    pub pool: Option<Pool>,
    /// The week's leaders so far, best first. May be empty; only the first
    /// [`MAX_LEADERS`] are ever shown, and fewer still if the lead is long.
    pub leaders: Vec<Leader>,
    /// The most recent week to reach a winner, if the bio has anything to say
    /// about one. Dropped before the leaders are, but after them: the pool and
    /// the leaders are both about *now*, and the last winner is history.
    pub last_winner: Option<LastWinner>,
}

impl State {
    /// The status half of the bio that fits in `budget` characters, or `None`
    /// when nothing does -- including the case where every part is absent.
    ///
    /// Tries the fullest render first and drops parts in the fixed order the
    /// module promises -- fewer leaders first, one at a time down to none,
    /// then the last winner -- stopping at the first that fits. The pool is
    /// never among the parts dropped here: it is either present, from
    /// [`choose`], or it is not, and either way every candidate keeps it.
    fn status(&self, budget: usize) -> Option<String> {
        let usable_leaders = self.leaders.len().min(MAX_LEADERS);
        let has_last = self.last_winner.is_some();
        let mut attempts: Vec<(usize, bool)> =
            (0..=usable_leaders).rev().map(|n| (n, has_last)).collect();
        if has_last {
            attempts.push((0, false));
        }
        attempts
            .into_iter()
            .map(|(leaders, last)| self.render_parts(leaders, last))
            .find(|text| !text.is_empty() && text.chars().count() <= budget)
    }

    /// One candidate render: the pool sentence when there is one, the first
    /// `leaders` entries, and the last winner when `last` is set -- joined in
    /// that order, each part a full sentence.
    ///
    /// Compact by design: Josh asked (2026-09-17) for the pool, the leaders
    /// and the last winner to fit *together* rather than take turns, and a
    /// live lead already spends over half of `MAX`. Each part is `Pool <n>
    /// ETH`, `Leads @a @b @c` (handles only -- see the module's "leads, never
    /// wins" note for why points do not belong in a public bio: an
    /// unverified score published as a fact is a claim this account cannot
    /// back), or `Last won @handle` / `Last paid <n> <unit>`, joined with the
    /// same middle dot that separates the lead from the status.
    fn render_parts(&self, leaders: usize, last: bool) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(pool) = &self.pool {
            parts.push(format!("Pool {} ETH", pool.pool));
        }
        if leaders > 0 {
            let names = self
                .leaders
                .iter()
                .take(leaders)
                .map(|l| format!("@{}", l.handle))
                .collect::<Vec<_>>()
                .join(" ");
            parts.push(format!("Leads {names}"));
        }
        if last {
            match &self.last_winner {
                Some(LastWinner::Won { handle, .. }) => {
                    parts.push(format!("Last won @{handle}"));
                }
                Some(LastWinner::Paid { amount, unit }) => {
                    parts.push(format!("Last paid {amount} {unit}"));
                }
                None => {}
            }
        }
        parts.join(JOIN)
    }

    /// Every figure this status could state, for the fidelity check.
    ///
    /// A superset of whatever a particular render kept after dropping parts:
    /// `check` only refuses a numeral that appears in the text and is not
    /// here, so an unused figure being present costs nothing, and computing
    /// it once for the whole `State` is simpler than threading through which
    /// parts a given render happened to keep.
    ///
    /// The dates are split into their parts the way `weekly::authorise_date`
    /// does, because a checker reading numerals out of the text sees `2026`,
    /// `09` and `07` rather than one date.
    #[must_use]
    pub fn authorised(&self) -> Vec<f64> {
        let mut out = Vec::new();
        date_parts(&mut out, &self.week);
        if let Some(pool) = &self.pool {
            amount_parts(&mut out, &pool.pool);
            #[expect(clippy::cast_precision_loss, reason = "a head count, far below 2^53")]
            out.push(pool.hunters as f64);
        }
        for leader in &self.leaders {
            #[expect(clippy::cast_precision_loss, reason = "a score, far below 2^53")]
            out.push(leader.points as f64);
        }
        match &self.last_winner {
            Some(LastWinner::Won { week, until, .. }) => {
                date_parts(&mut out, week);
                date_parts(&mut out, until);
            }
            Some(LastWinner::Paid { amount, .. }) => amount_parts(&mut out, amount),
            None => {}
        }
        out
    }
}

/// The most recent closed week's winner, in the shape the bio's last-winner
/// part wants, or `None`.
///
/// Reads the record and nothing else. Pure, same as the rest of this module:
/// the caller that reads the vault and the reply log is the one with a
/// filesystem to pay for.
#[must_use]
pub fn last_winner_of(record: &Record, now: u64) -> Option<LastWinner> {
    if let Some(payout) = &record.payout {
        return match payout.paid {
            realorrug_contest::Paid::Sol { lamports, .. } => Some(LastWinner::Paid {
                amount: render_sol(lamports),
                unit: "SOL",
            }),
            realorrug_contest::Paid::Eth { wei, .. } => Some(LastWinner::Paid {
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
    Some(LastWinner::Won {
        week: monday_of(record.week),
        handle: handle.clone(),
        until: day_of(record.claim_window_closes_at()),
    })
}

/// How old a pool reading may be and still be quoted as the pool now.
///
/// The pool line carries no time, so a reader takes the figure as current. A
/// reading older than six hours means the job that takes it has stopped, and
/// the pool is then left out of the status rather than quote a figure that
/// may have moved a long way since -- the leaders and the last winner, when
/// either exists, still get said.
pub const POOL_FRESH_SECONDS: u64 = 6 * 3_600;

/// The live pool reading, or `None` with no fresh one to quote.
fn fresh_pool(vault: Option<&Vault>, hunters: usize, now: u64) -> Option<Pool> {
    let vault = vault?;
    // A Solana vault says nothing here: nothing writes one any more, and a
    // SOL figure in an ETH sentence would be a wrong one.
    let Balance::Eth { wei, .. } = &vault.balance else {
        return None;
    };
    // A reading from the future is a clock fault, not a fresh reading.
    if vault.measured_at > now || now - vault.measured_at > POOL_FRESH_SECONDS {
        return None;
    }
    Some(Pool {
        pool: wei.to_eth(3),
        hunters,
    })
}

/// What the bio says now, combining whichever of the pool, the leaders and
/// the last winner exist, or `None` when none of the three does.
///
/// **No part invents another.** A stale or missing pool reading does not hide
/// a claimable winner, and a quiet leaderboard does not hide a fresh pool --
/// each of the three is decided on its own evidence, and the only case with
/// nothing to say is all three being silent at once.
#[must_use]
pub fn choose(
    record: Option<&Record>,
    vault: Option<&Vault>,
    hunters: usize,
    leaders: &[Leader],
    now: u64,
) -> Option<State> {
    let pool = fresh_pool(vault, hunters, now);
    let last_winner = record.and_then(|r| last_winner_of(r, now));
    let leaders: Vec<Leader> = leaders.iter().take(MAX_LEADERS).cloned().collect();
    if pool.is_none() && leaders.is_empty() && last_winner.is_none() {
        return None;
    }
    Some(State {
        week: monday_of(Week::of(now)),
        pool,
        leaders,
        last_winner,
    })
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
    // No subject: a bio's figures all describe the week's record, which is one
    // thing. The subject rule has nothing to separate here and is given
    // nothing to separate.
    let authorised: Vec<realorrug_roast::fidelity::Authorised> = authorised
        .iter()
        .copied()
        .map(realorrug_roast::fidelity::Authorised::anywhere)
        .collect();
    match realorrug_roast::fidelity::check(text, &authorised).first() {
        Some(f) => Err(format!("a figure the record does not carry: {}", f.literal)),
        None => Ok(()),
    }
}

/// A date's numerals, split the way `weekly::authorise_date` does: a checker
/// reading numerals out of the text sees `2026`, `09` and `07`, not one date.
fn date_parts(out: &mut Vec<f64>, d: &str) {
    out.extend(d.split('-').filter_map(|p| p.parse::<f64>().ok()));
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

#[cfg(test)]
mod tests {
    use super::*;

    const WEEK: Week = Week(2958);

    fn bio() -> Bio {
        Bio {
            lead: "Automated. Reads the chain, states what it measured.".to_owned(),
        }
    }

    fn last_won() -> LastWinner {
        LastWinner::Won {
            week: "2026-09-07".to_owned(),
            handle: "somebody".to_owned(),
            until: "2026-09-21".to_owned(),
        }
    }

    fn leaders() -> Vec<Leader> {
        vec![
            Leader {
                handle: "alice".to_owned(),
                points: 40,
            },
            Leader {
                handle: "bob".to_owned(),
                points: 31,
            },
            Leader {
                handle: "carol".to_owned(),
                points: 12,
            },
        ]
    }

    fn state() -> State {
        State {
            week: "2026-09-14".to_owned(),
            pool: Some(Pool {
                pool: "0.129".to_owned(),
                hunters: 17,
            }),
            leaders: leaders(),
            last_winner: Some(last_won()),
        }
    }

    #[test]
    fn the_lead_survives_every_branch_and_is_always_first() {
        // **The assertion this module exists for.** A bio write overwrites the
        // only copy, with no version history anywhere, so the one thing that
        // must be impossible is a render that drops the lead -- which is where
        // the automation disclosure goes on an account without X's own label,
        // and where the account's own words go on one with it.
        let b = bio();
        for s in [
            state(),
            State {
                leaders: Vec::new(),
                last_winner: None,
                ..state()
            },
        ] {
            let text = b.render(&s).expect("a bio");
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

        // The disclaimer is folded into the stored lead, fixed and always
        // present -- an operator cannot configure a bio that omits it.
        let set = Bio::from_vars(&|k| (k == "REALORRUG_BIO_LEAD").then(|| "  hello  ".to_owned()));
        assert_eq!(set.expect("set").lead, format!("hello{JOIN}{DISCLAIMER}"));
    }

    #[test]
    fn the_disclaimer_is_fixed_and_a_lead_leaving_it_no_room_is_refused() {
        // An operator cannot configure the disclaimer away: it is not part of
        // `REALORRUG_BIO_LEAD`, it is appended to whatever that holds.
        let set = Bio::from_vars(&|k| (k == "REALORRUG_BIO_LEAD").then(|| "Automated.".to_owned()));
        assert!(set.expect("set").lead.ends_with(DISCLAIMER));

        // A lead that leaves the disclaimer no room is refused at
        // configuration time, same as a lead alone too long for `MAX` was
        // refused before this task -- "always fits" is checked once, here,
        // rather than hoped for at every render.
        // Counted in characters, not bytes: `MAX` and the length check in
        // `from_vars` are both character counts, and `JOIN` holds a middle dot
        // that is two bytes and one character. Byte arithmetic here would name
        // a lead one character short of the boundary and assert the wrong side
        // of it.
        let room = MAX - JOIN.chars().count() - DISCLAIMER.chars().count();
        let too_long = "x".repeat(room);
        assert_eq!(
            Bio::from_vars(&|_| Some(too_long.clone())),
            None,
            "{too_long}"
        );
        // One character shorter leaves exactly enough room.
        let fits = "x".repeat(room - 1);
        assert!(Bio::from_vars(&|_| Some(fits.clone())).is_some());
    }

    #[test]
    fn a_render_that_would_be_cut_off_is_not_written_at_all() {
        // A bio truncated by the platform mid-figure is a wrong figure, in the
        // one place with no record that it was ever right. `None` leaves the
        // previous bio standing, which was also true when it was written.
        let long = Bio {
            lead: "x".repeat(MAX - 10),
        };
        assert_eq!(long.render(&state()), None);
    }

    #[test]
    fn the_leaderboard_line_says_leads_and_never_wins() {
        // Raw engagement: reposts and quotes are unbounded per account (S16),
        // so a mid-week leader can be one person with a script, and the week's
        // actual winner is decided on verified engagement at close. The two
        // can differ, and the wording is the whole of the difference.
        let text = bio().render(&state()).expect("a bio");
        assert!(text.contains("Leads"), "{text}");
        assert!(!text.to_lowercase().contains("wins"), "{text}");
    }

    #[test]
    fn all_three_parts_fit_together_when_there_is_room() {
        // Small enough figures that the pool, both leaders and the last
        // winner all survive in the same render -- the case this task is
        // for: a visitor sees all three, not whichever one happened to be up.
        let s = State {
            week: "2026-09-14".to_owned(),
            pool: Some(Pool {
                pool: "0.1".to_owned(),
                hunters: 5,
            }),
            leaders: vec![
                Leader {
                    handle: "ab".to_owned(),
                    points: 9,
                },
                Leader {
                    handle: "cd".to_owned(),
                    points: 7,
                },
            ],
            last_winner: Some(LastWinner::Paid {
                amount: "0.5".to_owned(),
                unit: "ETH",
            }),
        };
        let text = bio().render(&s).expect("a bio");
        assert!(text.contains("Pool 0.1 ETH"), "{text}");
        assert!(text.contains("@ab"), "{text}");
        assert!(text.contains("@cd"), "{text}");
        assert!(text.contains("Last paid 0.5 ETH"), "{text}");
        assert!(text.chars().count() <= MAX);
        assert_eq!(check(&text, &s.authorised()), Ok(()), "{text}");
    }

    #[test]
    fn parts_drop_in_order_when_the_lead_is_long() {
        // Fewer leaders first, one at a time down to none, and only then the
        // last winner -- never the pool, which anchors the line. Each budget
        // below is sized to exactly the next candidate down the fixed
        // sequence, so a wrong order (say, dropping the last winner before
        // the leaders are exhausted) would show up as the wrong text.
        let s = state();
        let lead_for = |chars: usize| Bio {
            lead: "x".repeat(MAX - JOIN.chars().count() - chars),
        };

        // Room for the pool, all three leaders and the last winner: nothing
        // dropped.
        let full = s.render_parts(MAX_LEADERS, true).chars().count();
        let text = lead_for(full).render(&s).expect("a bio");
        assert!(text.contains("@alice") && text.contains("@carol"), "{text}");
        assert!(text.contains("Last won"), "{text}");

        // One character less: the third leader goes, the last winner stays.
        let two_leaders = s.render_parts(2, true).chars().count();
        let text = lead_for(two_leaders).render(&s).expect("a bio");
        assert!(text.contains("@bob"), "{text}");
        assert!(!text.contains("@carol"), "{text}");
        assert!(text.contains("Last won"), "{text}");

        // Down to no leaders: the last winner still stands alone with the
        // pool.
        let no_leaders = s.render_parts(0, true).chars().count();
        let text = lead_for(no_leaders).render(&s).expect("a bio");
        assert!(text.contains("Last won"), "{text}");
        assert!(!text.contains("Leads "), "{text}");

        // Tighter again: the last winner goes too, only the pool is left.
        let pool_only = s.render_parts(0, false).chars().count();
        let text = lead_for(pool_only).render(&s).expect("a bio");
        assert!(text.contains("Pool 0.129 ETH"), "{text}");
        assert!(!text.contains('@'), "{text}");
        assert!(!text.contains("Last won"), "{text}");

        // And a lead that leaves no room even for the pool alone: nothing.
        assert_eq!(lead_for(pool_only - 1).render(&s), None);
    }

    #[test]
    fn the_live_lead_keeps_the_pool_two_leaders_and_the_last_winner() {
        // The case this task was opened for: Josh's actual lead ("Hunting
        // cabals. Exposing tokens. No mercy. Truth for you.", 57 chars) with
        // realistic ten-character handles, not the short ones the other
        // fixtures use. The verbose wording this replaced dropped the
        // leaders and the last winner here and left only the pool; the
        // compact format keeps at least the pool, two leaders and the last
        // winner together, worked out by hand in the PR body.
        let b = Bio {
            lead: "Hunting cabals. Exposing tokens. No mercy. Truth for you.".to_owned(),
        };
        let s = State {
            week: "2026-09-14".to_owned(),
            pool: Some(Pool {
                pool: "0.42".to_owned(),
                hunters: 23,
            }),
            leaders: vec![
                Leader {
                    handle: "aaaaaaaaaa".to_owned(),
                    points: 40,
                },
                Leader {
                    handle: "bbbbbbbbbb".to_owned(),
                    points: 31,
                },
                Leader {
                    handle: "cccccccccc".to_owned(),
                    points: 12,
                },
            ],
            last_winner: Some(LastWinner::Won {
                week: "2026-09-07".to_owned(),
                handle: "dddddddddd".to_owned(),
                until: "2026-09-21".to_owned(),
            }),
        };
        let text = b.render(&s).expect("a bio");
        assert!(
            text.chars().count() <= MAX,
            "{} chars: {text}",
            text.chars().count()
        );
        assert!(text.contains("Pool 0.42 ETH"), "{text}");
        assert!(text.contains("@aaaaaaaaaa"), "{text}");
        assert!(text.contains("@bbbbbbbbbb"), "{text}");
        assert!(text.contains("Last won @dddddddddd"), "{text}");
        // The third leader is a bonus, not a requirement -- the packet asks
        // for pool + 2 leaders + last winner at minimum.
        assert!(text.contains("@cccccccccc"), "{text}");
        assert_eq!(check(&text, &s.authorised()), Ok(()), "{text}");
    }

    #[test]
    fn every_render_passes_the_two_checks_a_reply_passes() {
        // A bio is a public statement by the same account and is held to the
        // same standard. The fidelity check is the one that bites: it reads
        // every numeral in the text and refuses any the record does not carry.
        let b = bio();
        for s in [
            state(),
            State {
                leaders: Vec::new(),
                ..state()
            },
            State {
                last_winner: None,
                ..state()
            },
        ] {
            let text = b.render(&s).expect("a bio");
            assert_eq!(check(&text, &s.authorised()), Ok(()), "{text}");
        }
    }

    #[test]
    fn a_figure_the_record_does_not_carry_is_refused() {
        let text = bio().render(&state()).expect("a bio");
        assert!(check(&text, &[]).is_err(), "{text}");
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
    fn the_record_decides_the_last_winner_and_a_voided_week_says_nothing() {
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
            last_winner_of(&record, closed + 60),
            Some(LastWinner::Won {
                week: "2026-09-07".to_owned(),
                handle: "somebody".to_owned(),
                until: "2026-09-21".to_owned(),
            })
        );

        // Paid: the money moved and that is the fact.
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
            last_winner_of(&paid, closed + 200),
            Some(LastWinner::Paid {
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
        assert_eq!(last_winner_of(&voided, closed + 200), None);

        // Past the claim window there is nothing for anybody to do.
        assert_eq!(
            last_winner_of(&record_with_winner(), record.claim_window_closes_at()),
            None
        );

        // A winner with no handle read is not named as a number.
        let mut nameless = record_with_winner();
        nameless.winner.as_mut().expect("winner").handle = None;
        assert_eq!(last_winner_of(&nameless, closed + 60), None);
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
    fn choose_combines_the_pool_the_leaders_and_the_last_winner() {
        // Week 2959 opens Monday 2026-09-14.
        let now = Week(2959).opens_at() + 3_600;
        let fresh = vault(129_999_999_999_999_999, now - 60);
        let record = record_with_winner();
        let s = choose(Some(&record), Some(&fresh), 17, &leaders(), now).expect("a state");
        assert_eq!(s.week, "2026-09-14");
        assert_eq!(
            s.pool,
            Some(Pool {
                pool: "0.129".to_owned(),
                hunters: 17,
            })
        );
        assert_eq!(s.leaders, leaders());
        assert!(s.last_winner.is_some());

        // A claimable winner does not need a fresh pool reading to be shown --
        // the pool and the last winner are each decided on their own evidence.
        let no_pool = choose(Some(&record), None, 17, &leaders(), now).expect("a state");
        assert_eq!(no_pool.pool, None);
        assert!(no_pool.last_winner.is_some());

        // No pool reading means no status at all only when nothing else has
        // anything to say either -- unchanged from before this task.
        assert_eq!(choose(None, None, 0, &[], now), None);

        // More than `MAX_LEADERS` leaders offered: only the first are kept.
        let many: Vec<Leader> = (0..10)
            .map(|i| Leader {
                handle: format!("h{i}"),
                points: 100 - i,
            })
            .collect();
        let s = choose(None, Some(&fresh), 0, &many, now).expect("a state");
        assert_eq!(s.leaders.len(), MAX_LEADERS);
        assert_eq!(s.leaders, many[..MAX_LEADERS]);

        // Exactly six hours old is still quoted; a second more is not, because
        // the line carries no time and a reader takes it as now.
        assert_eq!(POOL_FRESH_SECONDS, 21_600);
        assert!(choose(None, Some(&vault(1, now - POOL_FRESH_SECONDS)), 0, &[], now).is_some());
        assert_eq!(
            choose(
                None,
                Some(&vault(1, now - POOL_FRESH_SECONDS - 1)),
                0,
                &[],
                now
            ),
            None
        );
        // A reading from the future is a clock fault, not a fresh reading.
        assert_eq!(choose(None, Some(&vault(1, now + 1)), 0, &[], now), None);
        // No reading, or a Solana one, says nothing.
        assert_eq!(choose(None, None, 3, &[], now), None);
        let sol = Vault {
            balance: Balance::Sol { lamports: 5 },
            ..fresh
        };
        assert_eq!(choose(None, Some(&sol), 3, &[], now), None);
    }
}
