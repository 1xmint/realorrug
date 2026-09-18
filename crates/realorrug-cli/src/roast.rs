// SPDX-License-Identifier: Apache-2.0
//! `realorrug roast <mint>` — the reply the public analyst would post.
//!
//! The caller `realorrug-roast` was built for, and the reason it could be built
//! before anything touching X exists. Everything hard about the reply pipeline
//! — the fact sheet, the verdict, the voice pass, the two checks — is proved
//! here, offline, against real mints, with no account and no credential.
//!
//! **It prints. It does not post.** Nothing in this path can publish, and the
//! X adapter is a separate binary precisely so that "write a reply" and "send a
//! reply" are different programs.
//!
//! # Reading a hundred of these is the point
//!
//! Before a public account says anything, somebody should read a large number
//! of its replies and disagree with some. `--sheet` prints the fact sheet
//! beside the reply so a disagreement can be traced to a measurement rather
//! than argued about.

use realorrug_onchain::{RpcClient, dispatch};
use realorrug_roast::{BaseRates, Fellback};

use crate::dossier::safe;
use crate::flag;

/// Runs the command.
///
/// # Errors
///
/// A message when the mint is missing or unparseable, or when the token's
/// history cannot be read at all.
pub fn run(args: &[String]) -> Result<(), String> {
    let mint_arg = mint_arg_from(args).ok_or_else(|| {
        "usage: realorrug roast <mint> [--rpc URL] [--robinhood-rpc URL] [--rates PATH] [--sheet]"
            .to_owned()
    })?;

    let client = flag(args, "--rpc").map_or_else(
        || RpcClient::from_vars(&|k| std::env::var(k).ok()),
        RpcClient::new,
    );
    // No default (rule 7), same as `launch-check --rpc`: the public Robinhood
    // endpoint is rate-limited, so an operator who does not pass this flag has
    // chosen "this chain cannot be read yet", not "read it anyway against a
    // default this command invented". A `0x…` mint with this `None` is
    // answered unreadable by the dispatcher below, never as not-an-address and
    // never against Solana.
    let robinhood = flag(args, "--robinhood-rpc").map(|url| realorrug_robinhood::Rpc::new(&url));
    // Always configured, unlike `robinhood` above: DexScreener/GeckoTerminal
    // have no "endpoint the operator must supply" step the way an RPC node
    // does, so there is no missing-config case to deny by default here --
    // only a failed read, which `dispatch::robinhood` already turns into a
    // named "market" gap rather than a zero.
    let market = realorrug_onchain::market::Http::default();
    let clients = dispatch::Clients {
        solana: &client,
        robinhood: robinhood.as_ref(),
        market: Some(&market),
    };

    // The one dispatcher every entry point that answers about a mint goes
    // through (`realorrug-onchain::dispatch`): it decides Solana or Robinhood
    // purely from the address's own shape and owns the read's budget -- the
    // crate's own default, not restated here for the same reason `answer.rs`
    // does not restate it.
    let dossier = match dispatch::read(&mint_arg, &clients) {
        Ok(d) => d,
        Err(dispatch::Error::NotAnAddress) => {
            return Err(format!("not a valid address: {}", safe(&mint_arg, 64)));
        }
        Err(dispatch::Error::Unreadable(why)) => return Err(why),
    };

    // Rule 8 twice over. A snapshot that will not load means the reply carries
    // no population context -- it never means falling back on remembered
    // numbers, which is how 0008's superseded 68% would outlive its own
    // correction. The failure is printed rather than swallowed, because an
    // analyst quietly saying less is indistinguishable from one with nothing
    // to say.
    let rates_path = flag(args, "--rates")
        .unwrap_or_else(|| realorrug_roast::baserates::DEFAULT_PATH.to_owned());
    let rates = match BaseRates::load(&rates_path) {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("no base rates ({e}); the reply will carry no population context");
            None
        }
    };
    if let Some(r) = &rates
        && r.is_stale_at(&today())
    {
        eprintln!(
            "base rates were measured on {} and are stale; re-run research 0024 before \
             trusting the population figures",
            r.measured_on
        );
    }

    // No provider is the ordinary case on a machine with no credential, and it
    // is not an error: the deterministic template ships. `REALORRUG_MODEL_CODEX` is
    // marked private-use-only, so a public analyst must go through the metered
    // API-key path -- but that choice belongs to `realorrug-model`, which reads the
    // environment, not to this file.
    let provider = realorrug_model::from_vars(&|k| std::env::var(k).ok()).ok();

    // The creator's record, and its absence is worth saying out loud: without
    // it every reply about a fresh launch says the same thing, because the cost
    // line is a constant and most launches sit in the same recipient band.
    let creators = realorrug_roast::CreatorIndex::read(realorrug_roast::creator::DEFAULT_PATH).ok();
    if creators.is_none() {
        eprintln!(
            "no creator index at {}; the reply will say nothing about who launched this; \
             realorrug builds none yet (ADR 0026).",
            realorrug_roast::creator::DEFAULT_PATH
        );
    }

    // ADR 0013 constraint 5: the analyst's own token never has its price or
    // market cap stated. Read the way the daemon reads it and refused the same
    // way -- a mint that will not parse is the rule silently switched off for
    // the real token, so the command stops rather than printing a reply that
    // looks right.
    let self_mint = realorrug_analyst::daemon::self_mint_from(&|k| std::env::var(k).ok())?;

    let (sheet, reply) = realorrug_roast::roast(
        &dossier,
        rates.as_ref(),
        creators.as_ref(),
        provider.as_deref(),
        self_mint.as_ref(),
    );

    if wants_sheet(args) {
        println!("--- fact sheet ---");
        print!("{}", sheet.render());
        for (label, value) in &sheet.untrusted {
            // Escaped on the way to a terminal for the same reason the dossier
            // escapes: these are arbitrary creator-controlled bytes.
            println!("{label} (untrusted): {}", safe(value, 64));
        }
        println!("--- reply ---");
    }

    print!("{}", reply.text);
    if needs_newline(&reply.text) {
        println!();
    }

    // The fallback reason is the single most useful line this command emits.
    // A reply that fell back because the model fabricated a figure is the only
    // early warning that the voice pass is drifting, and a silent fallback
    // would hide exactly that.
    //
    // On stderr, so a caller piping this command reads only the text that
    // would post. A refused draft is by definition text that would not.
    eprint!("{}", why_it_fell_back(&reply));
    Ok(())
}

/// Why the reply this command printed is the template, and what was thrown
/// away to get there.
///
/// Built as a string rather than printed in place so a test can read it. The
/// printing version had no test that could fail, which for the one diagnostic
/// an operator actually reads is the wrong trade.
///
/// # Why it prints the draft and not only the reason
///
/// Design 0026 §1, measured on the box 2026-09-17: every substantive reply the
/// bot had ever sent was a fallback, and nobody could read a single one of the
/// drafts behind them. So every statement about how the voice reads -- that it
/// is too flat, too wordy, too eager -- was a guess about text no human had
/// seen. The daemon keeps it now (`realorrug_analyst` writes `refused` into
/// the reply log); this is the same evidence for the operator at a terminal,
/// who is the person actually deciding whether the voice is any good.
fn why_it_fell_back(reply: &realorrug_roast::Reply) -> String {
    // Writing into the string rather than formatting and appending: clippy
    // refuses the second, and a write into a `String` cannot fail, so the
    // `Result` is discarded rather than carried up a function that has none.
    use std::fmt::Write as _;
    let mut out = match &reply.fellback {
        None => "(model reply, both checks passed)\n".to_owned(),
        Some(Fellback::NoProvider) => {
            "(deterministic template: no model provider configured)\n".to_owned()
        }
        Some(Fellback::Unreachable(why)) => {
            format!("(deterministic template: provider unreachable -- {why})\n")
        }
        Some(Fellback::Empty) => "(deterministic template: provider said nothing)\n".to_owned(),
        Some(Fellback::Forbidden(v)) => {
            let mut s =
                "(deterministic template: the model wrote a claim it may not publish)\n".to_owned();
            for violation in v {
                let _ = writeln!(s, "    {:?} -- {}", violation.phrase, violation.because);
            }
            s
        }
        Some(Fellback::Fabricated(f)) => {
            let mut s =
                "(deterministic template: the model wrote a number the sheet does not license)\n"
                    .to_owned();
            for fab in f {
                // The two failures read differently and an operator acts on
                // them differently: a number nothing measured means the model
                // invented, and a number moved to another subject means it
                // reasoned past its evidence -- which looks like a correct
                // reply unless the line says so.
                match fab.why {
                    realorrug_roast::fidelity::Why::NotMeasured => {
                        let _ = writeln!(s, "    {} is not on the fact sheet", fab.literal);
                    }
                    realorrug_roast::fidelity::Why::WrongSubject {
                        measured,
                        written_about,
                    } => {
                        let _ = writeln!(
                            s,
                            "    {} was measured about {measured:?}, written about \
                             {written_about:?}",
                            fab.literal
                        );
                    }
                }
            }
            s
        }
    };
    if let Some(draft) = &reply.refused {
        out.push_str("--- the draft that was refused ---\n");
        // Escaped, unlike `reply.text` above, and the difference is not an
        // inconsistency. Published text has been through
        // `render::for_publication`, which removes every control character; a
        // refused draft is deliberately the model's raw bytes, and the model
        // has seen the token's own name and symbol. Printing those raw to a
        // terminal is the one place this command could be made to emit an
        // escape sequence a token's creator chose.
        //
        // It costs legibility -- an em dash prints as `\u{2014}` -- and buys
        // the thing the draft is kept for: a zero-width space the model padded
        // with is visible here and nowhere else.
        out.push_str(&safe(draft, 600));
        out.push('\n');
    }
    out
}

/// The mint a `realorrug roast` invocation names, if any.
///
/// Positional first, then `--mint`. The positional slot is guarded against
/// swallowing a flag: without that guard `realorrug roast --rpc URL` takes `--rpc`
/// as the mint and reports it as an invalid address, which is a confusing way
/// of saying "you named no mint at all".
fn mint_arg_from(args: &[String]) -> Option<String> {
    args.get(1)
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .or_else(|| flag(args, "--mint"))
}

/// Whether the fact sheet was asked for.
///
/// Its own function so the comparison can be tested. Inline, `==` could become
/// `!=` and every run would print the sheet except the ones that asked for it.
fn wants_sheet(args: &[String]) -> bool {
    args.iter().any(|a| a == "--sheet")
}

/// Whether a reply needs a newline before the shell prompt returns.
///
/// One line, and extracted anyway, for the reason above: a mutation of the `!`
/// is invisible in a `print!` and visible here.
fn needs_newline(text: &str) -> bool {
    !text.ends_with('\n')
}

/// Today, as `YYYY-MM-DD`.
///
/// Derived from the system clock, which is fine for "are these base rates a
/// fortnight old" and is deliberately nowhere near the decision path -- the
/// risk kernel is pure and has no clock, and nothing here feeds it.
///
/// The clock is the only thing this does. All the arithmetic is in
/// [`from_days`], which is pure and therefore checkable at a fixed day.
fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    from_days(i64::try_from(secs / 86_400).unwrap_or(0))
}

/// A `YYYY-MM-DD` date from a count of days since the 1970 epoch.
///
/// **This was inside `today()` and copied into the test module.** The tests
/// then exercised the copy, so every mutation of the real arithmetic survived --
/// twenty-four of them, reported by CI on 2026-09-03. It is LEARNINGS 18's shape
/// applied to a test: two instruments compared as if they were one, except here
/// only one of them was ever run.
///
/// On 2026-09-05 the definition moved to `realorrug_types::civil`, because the
/// public endpoints needed the same function for a contest week's Monday and a
/// second copy would have been the same mistake one crate over. This is the
/// one caller's name for it, and the tests below still pin every boundary --
/// now of the shared definition.
fn from_days(days: i64) -> String {
    realorrug_types::civil::date_from_days(days)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_is_a_date_the_staleness_check_can_read() {
        // The staleness rule treats an unparseable date as stale, so a broken
        // clock here would make every snapshot look old rather than making an
        // old one look fresh. Worth asserting the shape anyway: the silent
        // failure would be a warning that fires on every run and gets ignored.
        let t = today();
        assert_eq!(t.len(), 10, "{t}");
        assert_eq!(t.as_bytes()[4], b'-');
        assert_eq!(t.as_bytes()[7], b'-');
        let year: i32 = t[..4].parse().expect("a year");
        assert!((2024..2100).contains(&year), "{t}");
        let month: u32 = t[5..7].parse().expect("a month");
        assert!((1..=12).contains(&month), "{t}");
        let day: u32 = t[8..].parse().expect("a day");
        assert!((1..=31).contains(&day), "{t}");
    }

    #[test]
    fn the_mint_argument_is_read_positionally_and_never_swallows_a_flag() {
        let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();

        // Positional.
        assert_eq!(
            mint_arg_from(&args(&["roast", "SoMeMiNt"])),
            Some("SoMeMiNt".to_owned())
        );
        // Named.
        assert_eq!(
            mint_arg_from(&args(&["roast", "--mint", "SoMeMiNt"])),
            Some("SoMeMiNt".to_owned())
        );
        // A flag in the positional slot is not a mint. Without the guard this
        // returns Some("--rpc"), and the command then reports the flag as an
        // invalid address rather than saying no mint was given.
        assert_eq!(mint_arg_from(&args(&["roast", "--rpc", "http://x"])), None);
        // ...and the named form still wins from behind a flag.
        assert_eq!(
            mint_arg_from(&args(&["roast", "--sheet", "--mint", "SoMeMiNt"])),
            Some("SoMeMiNt".to_owned())
        );
        // Nothing at all.
        assert_eq!(mint_arg_from(&args(&["roast"])), None);
    }

    #[test]
    fn the_sheet_is_printed_only_when_it_is_asked_for() {
        let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(wants_sheet(&args(&["roast", "M", "--sheet"])));
        assert!(!wants_sheet(&args(&["roast", "M"])));
        // A different flag is not this flag.
        assert!(!wants_sheet(&args(&["roast", "M", "--sheets"])));
    }

    #[test]
    fn a_reply_gets_a_newline_only_when_it_lacks_one() {
        assert!(needs_newline("no trailing newline"));
        assert!(!needs_newline("has one\n"));
        // Empty output still wants the prompt on its own line.
        assert!(needs_newline(""));
    }

    #[test]
    fn run_refuses_before_it_reaches_the_network() {
        // Both refusals happen during argument handling, so this needs no RPC
        // and no fixture -- and it is what stops the whole body being
        // replaceable with `Ok(())`. A `realorrug roast` that printed nothing and
        // exited zero would look exactly like success, which is LEARNINGS 5.
        let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();

        let usage = run(&args(&["roast"])).expect_err("no mint is a usage error");
        assert!(usage.starts_with("usage: realorrug roast"), "{usage}");

        let bad = run(&args(&["roast", "not-an-address"])).expect_err("a bad mint is an error");
        assert!(bad.starts_with("not a valid address:"), "{bad}");
    }

    #[test]
    fn the_calendar_is_right_at_every_boundary_the_algorithm_has() {
        // Two known values used to be the whole of this, against a copy of the
        // algorithm that lived in this module -- so every arithmetic mutation of
        // the real one survived. These are chosen so that each constant and each
        // branch changes an answer if it moves.

        // The epoch, and the era offset that centres the algorithm on March.
        assert_eq!(from_days(0), "1970-01-01");
        assert_eq!(from_days(1), "1970-01-02");
        assert_eq!(from_days(31), "1970-02-01");

        // The March pivot itself: `mp < 10` and the `y + 1` correction either
        // side of it. 1970-02-28 and 1970-03-01 are consecutive days that take
        // opposite branches.
        assert_eq!(from_days(58), "1970-02-28");
        assert_eq!(from_days(59), "1970-03-01");

        // Leap day in an ordinary leap year, and the day either side of it.
        assert_eq!(from_days(19_781), "2024-02-28");
        assert_eq!(from_days(19_782), "2024-02-29");
        assert_eq!(from_days(19_783), "2024-03-01");

        // 2000 is a leap year because it is divisible by 400 -- the case the
        // `doe / 36_524` and `doe / 146_096` terms exist for, and the one a
        // naive rule gets wrong.
        assert_eq!(from_days(11_015), "2000-02-28");
        assert_eq!(from_days(11_016), "2000-02-29");
        assert_eq!(from_days(11_017), "2000-03-01");

        // 2100 is *not* a leap year, because it is divisible by 100 and not by
        // 400. Without this the century rule is unchecked in the direction that
        // matters.
        assert_eq!(from_days(47_540), "2100-02-28");
        assert_eq!(from_days(47_541), "2100-03-01");

        // Year and month ends, where `doy`, `mp` and the `+ 1` on the day meet.
        assert_eq!(from_days(19_721), "2023-12-30");
        assert_eq!(from_days(19_722), "2023-12-31");
        assert_eq!(from_days(19_723), "2024-01-01");

        // A date from before the epoch, so `div_euclid` and `rem_euclid` are
        // exercised on a negative `z` -- the reason they are there rather than
        // `/` and `%`.
        assert_eq!(from_days(-1), "1969-12-31");
        assert_eq!(from_days(-365), "1969-01-01");

        // The day this was written, cross-checked against a calendar.
        assert_eq!(from_days(20_699), "2026-09-03");
    }

    #[test]
    fn a_refused_draft_is_printed_under_the_reason_it_was_refused() {
        use realorrug_roast::forbidden::Violation;
        use realorrug_roast::voice::Billed;

        let refused = realorrug_roast::Reply {
            text: "the template that shipped instead".to_owned(),
            fellback: Some(Fellback::Forbidden(vec![Violation {
                phrase: "canttell reply names nothing that could not be read",
                because: "design 0020 §4",
            }])),
            billed: Billed::Unreported,
            refused: Some(
                "holder data is unavailable, so I can't call this one\u{200b}".to_owned(),
            ),
        };
        let out = super::why_it_fell_back(&refused);
        // The reason, unchanged: this must not become a diagnostic that says
        // what the model wrote and no longer says why it was thrown away.
        assert!(out.contains("a claim it may not publish"), "{out}");
        assert!(out.contains("design 0020 §4"), "{out}");
        // And the draft itself, which is the whole point of the function.
        assert!(out.contains("--- the draft that was refused ---"), "{out}");
        assert!(out.contains("holder data is unavailable"), "{out}");
        // Escaped on the way out, because a raw draft is model output and this
        // is a terminal. It is also the only way the character worth seeing is
        // visible at all: a zero-width space printed raw prints as nothing.
        assert!(out.contains(r"\u{200b}"), "{out}");
        assert!(out.ends_with('\n'), "{out:?}");

        // A reply nothing refused prints no draft header. Without this the
        // arm could print the header unconditionally and every passing reply
        // would announce a draft that does not exist.
        let published = realorrug_roast::Reply {
            text: "a reply that passed".to_owned(),
            fellback: None,
            billed: Billed::Unreported,
            refused: None,
        };
        let out = super::why_it_fell_back(&published);
        assert_eq!(out, "(model reply, both checks passed)\n");
    }

    #[test]
    fn today_uses_the_same_arithmetic_the_tests_check() {
        // The defect this file had was a second copy of the algorithm. This is
        // the assertion that there is only one: whatever day the clock is on,
        // `today()` must equal `from_days` of that day.
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let days = i64::try_from(secs / 86_400).expect("days since the epoch");
        assert_eq!(today(), from_days(days));
    }
}
