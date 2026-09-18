// SPDX-License-Identifier: Apache-2.0
//! `realorrug bio --preview` — the exact bio text for a sample, without
//! writing it anywhere.
//!
//! Josh asked (2026-09-17) for the bio to show the pool, the leaders and the
//! last winner together; `realorrug-analyst::bio::State::status` now drops
//! parts in a fixed order when the lead is long. That order is easiest to see
//! by trying it, and the daemon writes at most hourly (`write_bio_if_changed`,
//! `daemon.rs`), so the alternative to a preview is editing `REALORRUG_BIO_LEAD`
//! on the live account and waiting to see what got dropped. This prints the
//! same [`realorrug_analyst::bio::Bio::render`] the daemon would produce, from
//! a sample the caller names on the command line, and touches nothing.

use realorrug_analyst::bio::{Bio, LastWinner, Leader, Pool, State};
use realorrug_contest::Week;
use realorrug_types::civil::date_from_days;

/// The usage line.
const USAGE: &str = "usage: realorrug bio --preview [--lead TEXT]
                              [--pool AMOUNT] [--hunters N]
                              [--leader HANDLE]... [--last-winner HANDLE]

Prints the bio text `write_bio_if_changed` would post for the sample, and
its length. Writes nothing.

--lead TEXT        the account's own line; else read from REALORRUG_BIO_LEAD
--pool AMOUNT       the pool, in ETH, e.g. 0.42
--hunters N         distinct hunters this week; defaults to the leader count
--leader HANDLE     a leader, best first; repeat for up to three
--last-winner HANDLE the most recent week's winner, shown as unclaimed";

/// The value following every occurrence of `name`, in order.
///
/// [`crate::flag`] reads the first; a leaderboard needs all of them, so this
/// is the same walk done without stopping at the first match.
fn values(args: &[String], name: &str) -> Vec<String> {
    args.iter()
        .zip(args.iter().skip(1))
        .filter(|(a, _)| a.as_str() == name)
        .map(|(_, v)| v.clone())
        .collect()
}

/// The sample [`State`] the flags describe, or `None` when none of `--pool`,
/// `--leader` or `--last-winner` was given.
///
/// `None` here mirrors `bio::choose`'s own rule: a state with nothing in it
/// is not a state, it is the absence of one, and the preview should say so
/// rather than print an empty status.
///
/// Points and the pool's hunter count are not asked for by name beyond
/// `--hunters` -- a preview illustrates the leaderboard's *shape* (best
/// first, "leads" not "wins"), not a real week's numbers, so the leaders
/// named on the command line are given synthetic, descending points and the
/// hunter count falls back to how many of them there are.
fn sample_state(args: &[String], now: u64) -> Option<State> {
    let pool_amount = crate::flag(args, "--pool");
    let leader_names = values(args, "--leader");
    let last_winner_handle = crate::flag(args, "--last-winner");
    if pool_amount.is_none() && leader_names.is_empty() && last_winner_handle.is_none() {
        return None;
    }

    let week = Week::of(now);
    let week_str = date_from_days(i64::try_from(week.opens_at() / 86_400).unwrap_or(i64::MAX));

    let pool = pool_amount.map(|amount| Pool {
        pool: amount,
        hunters: crate::flag(args, "--hunters")
            .and_then(|n| n.parse().ok())
            .unwrap_or(leader_names.len()),
    });

    let leader_count = leader_names.len();
    let leaders: Vec<Leader> = leader_names
        .into_iter()
        .enumerate()
        .map(|(i, handle)| Leader {
            handle,
            points: 10 * u64::try_from(leader_count - i).unwrap_or(1),
        })
        .collect();

    let last_winner = last_winner_handle.map(|handle| LastWinner::Won {
        week: week_str.clone(),
        handle,
        until: date_from_days(i64::try_from(week.closes_at() / 86_400).unwrap_or(i64::MAX)),
    });

    Some(State {
        week: week_str,
        pool,
        leaders,
        last_winner,
    })
}

/// The bio `--preview` would print, or the reason it has nothing to print.
///
/// Pure: takes the lead getter and the clock rather than reading the process
/// environment itself, so `run`'s printing and this construction can be
/// tested apart -- the same split [`crate::model_prices`] uses, for the same
/// reason (a mutation inside argument handling is reachable only through this
/// function's own return value, not through a process's stdout).
fn preview_text(
    args: &[String],
    now: u64,
    get: &impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    let bio = Bio::from_vars(get)
        .ok_or_else(|| "no lead: pass --lead or set REALORRUG_BIO_LEAD".to_owned())?;
    let Some(state) = sample_state(args, now) else {
        return Err("nothing to preview: pass --pool, --leader or --last-winner".to_owned());
    };
    bio.render(&state)
        .ok_or_else(|| "the lead and this sample together do not fit".to_owned())
}

/// `realorrug bio --preview ...`
pub fn run(args: &[String]) -> Result<(), String> {
    if !args.iter().any(|a| a == "--preview") {
        return Err(USAGE.to_owned());
    }

    // The same rule `Bio::from_vars` applies to the daemon's own read, so a
    // preview run with no `--lead` shows exactly what an unconfigured bot
    // would (or, since `from_vars` refuses a blank lead, what it would not).
    let cli_lead = crate::flag(args, "--lead");
    let get = |key: &str| {
        if key == "REALORRUG_BIO_LEAD" {
            cli_lead.clone().or_else(|| std::env::var(key).ok())
        } else {
            std::env::var(key).ok()
        }
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let text = preview_text(args, now, &get)?;
    println!("{text}");
    println!("{} chars", text.chars().count());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn values_collects_every_occurrence_in_order() {
        let a = args(&["bio", "--leader", "alice", "--leader", "bob"]);
        assert_eq!(values(&a, "--leader"), vec!["alice", "bob"]);
        assert_eq!(values(&a, "--last-winner"), Vec::<String>::new());
    }

    #[test]
    fn no_sample_flags_means_no_state() {
        let a = args(&["bio", "--preview"]);
        assert_eq!(sample_state(&a, 1_800_000_000), None);
    }

    #[test]
    fn a_pool_alone_is_a_state_with_no_leaders_and_no_last_winner() {
        let a = args(&["bio", "--preview", "--pool", "0.42"]);
        let state = sample_state(&a, 1_800_000_000).expect("a state");
        assert_eq!(state.pool.expect("a pool").pool, "0.42");
        assert!(state.leaders.is_empty());
        assert_eq!(state.last_winner, None);
    }

    #[test]
    fn leaders_come_out_best_named_first_with_descending_points() {
        let a = args(&["bio", "--preview", "--leader", "alice", "--leader", "bob"]);
        let state = sample_state(&a, 1_800_000_000).expect("a state");
        assert_eq!(state.leaders[0].handle, "alice");
        assert_eq!(state.leaders[1].handle, "bob");
        assert!(state.leaders[0].points > state.leaders[1].points);
    }

    #[test]
    fn hunters_defaults_to_the_leader_count_but_a_flag_overrides_it() {
        let a = args(&[
            "bio",
            "--preview",
            "--pool",
            "0.1",
            "--leader",
            "alice",
            "--leader",
            "bob",
        ]);
        let state = sample_state(&a, 1_800_000_000).expect("a state");
        assert_eq!(state.pool.expect("a pool").hunters, 2);

        let with_flag = args(&[
            "bio",
            "--preview",
            "--pool",
            "0.1",
            "--leader",
            "alice",
            "--hunters",
            "9",
        ]);
        let state = sample_state(&with_flag, 1_800_000_000).expect("a state");
        assert_eq!(state.pool.expect("a pool").hunters, 9);
    }

    #[test]
    fn a_last_winner_flag_names_an_unclaimed_win() {
        let a = args(&["bio", "--preview", "--last-winner", "carol"]);
        let state = sample_state(&a, 1_800_000_000).expect("a state");
        match state.last_winner.expect("a last winner") {
            LastWinner::Won { handle, .. } => assert_eq!(handle, "carol"),
            LastWinner::Paid { .. } => panic!("expected an unclaimed win"),
        }
    }

    #[test]
    fn without_preview_the_command_refuses() {
        assert!(run(&args(&["bio", "--pool", "0.1"])).is_err());
    }

    #[test]
    fn the_preview_text_is_exactly_what_bio_render_produces() {
        let lead = "Automated. Reads the chain, states what it measured.";
        let a = args(&[
            "bio",
            "--preview",
            "--lead",
            lead,
            "--pool",
            "0.1",
            "--leader",
            "alice",
            "--leader",
            "bob",
            "--last-winner",
            "carol",
        ]);
        let now = 1_800_000_000;
        let cli_lead = crate::flag(&a, "--lead");
        let get = |key: &str| {
            if key == "REALORRUG_BIO_LEAD" {
                cli_lead.clone()
            } else {
                None
            }
        };

        let got = preview_text(&a, now, &get).expect("fits");

        // Built independently, through the same public path `preview_text`
        // uses internally -- `Bio::from_vars` then `render` -- so this pins
        // the command's output to `Bio::render` rather than to
        // `preview_text`'s own arithmetic being self-consistent.
        let state = sample_state(&a, now).expect("a state");
        let want = Bio::from_vars(&get)
            .expect("a lead")
            .render(&state)
            .expect("fits");
        assert_eq!(got, want);
    }

    #[test]
    fn preview_text_refuses_with_no_sample() {
        let a = args(&["bio", "--preview", "--lead", "hi"]);
        assert!(preview_text(&a, 1_800_000_000, &|_| None).is_err());
    }
}
