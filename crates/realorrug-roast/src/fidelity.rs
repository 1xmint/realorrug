// SPDX-License-Identifier: Apache-2.0
//! The check that the model did not introduce a number.
//!
//! # What this is, in one sentence
//!
//! Every numeric literal in the generated reply must appear in the fact sheet;
//! if one does not, the reply is discarded and the deterministic template ships
//! instead.
//!
//! # Why a check and not an instruction
//!
//! The system prompt tells the model not to invent figures, and that is worth
//! saying, but an instruction is a request and this is a public account. The
//! failure it guards against is not a malicious model — it is an ordinary one
//! being helpful: converting a fee to a dollar figure, adding two numbers,
//! rounding to something friendlier, recalling a statistic about pump.fun from
//! training. Each of those is a fabricated measurement published under a name
//! whose entire claim is that its numbers are measured.
//!
//! `radar-signer` makes the same move for the same reason. It does not trust
//! the caller's description of a transaction; it re-decodes the bytes and
//! checks them against the authorisation. **The signer re-reads the bytes it
//! signs; the roaster re-reads the numbers it posts.**
//!
//! # The tolerance rule, and why it is not zero
//!
//! A reply that may not round is a reply that must say "25.1%" where a person
//! would say "a quarter", and the result is unreadable. So a literal is accepted
//! when it matches an authorised value **at the precision the literal itself was
//! written to**: `25` matches `25.1` because `25.1` rounded to zero decimals is
//! `25`; `25.4` does not match `25.1` at one decimal.
//!
//! That rule is itself tested, because a tolerance nobody checked is a hole
//! nobody knows the size of.
//!
//! # A number belongs to the thing it was measured about
//!
//! Membership alone is not enough, and the hole it leaves is the sharpest one
//! this module has had. A sheet that measured the largest holder at 41% put 41
//! in the permitted set; a reply saying "the creator already dumped 41%" is
//! then made entirely of permitted digits and is false. Worse, it is false in
//! the direction that gets screenshotted, because the accusation lands on a
//! person.
//!
//! So an authorised value carries the [`Subject`] it was measured about, and a
//! sentence that names exactly one subject may only use numbers measured about
//! that subject. A sentence naming none, or naming two, falls back to plain
//! membership: the rule fires where it is unambiguous and stays out of the way
//! everywhere else. See [ADR 0031](../../../docs/adr/0031-the-model-picks-the-story-and-the-evidence-licenses-the-joke.md).
//!
//! # What it cannot do
//!
//! It checks *numbers*, not claims. "The creator is a scammer" contains no
//! numeral and passes here — [`crate::forbidden`] is what refuses that, and the
//! two are separate because they fail for different reasons and a reader of
//! either should not have to hold both in mind.
//!
//! The subject rule does not change that. "The biggest holder is the deployer"
//! has no digit in it, is false whenever the biggest holder is a pool, and
//! passes every check in this repository. ADR 0031 §"What the checks cannot do"
//! records why the answer to that is measurement before it is a gate.

use crate::clause::Kind;

/// What a measured number is about.
///
/// Every fact on a sheet is tagged with one of these, derived from its
/// [`Kind`]. The tag is what stops a real figure moving to the wrong actor.
///
/// Only four of these have words that name them in ordinary prose — see
/// [`named_in`] — but every fact still gets a precise tag, because the tag's
/// job is to be *refused* by a sentence naming somebody else. Tagging the
/// launch block's transaction count as "about nothing in particular" would
/// licence it inside "the creator sent 7 of them", which is the whole failure
/// this exists to stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    /// The person or address that deployed the token, and their history.
    Creator,
    /// Who holds the supply now.
    Holders,
    /// The launch itself: the block, its recipients, its transactions.
    Launch,
    /// The launchpad, its population rates and its fees. Not this token.
    Venue,
    /// The pool: what can be traded and at what cost.
    Liquidity,
    /// The token as a thing: its age, whether it graduated.
    Token,
    /// Not measured about any one of the above, and citable in any sentence.
    ///
    /// The read point, the list of what could not be read, and the innocent
    /// explanations. All three are Radar's own words about the sheet rather
    /// than about a subject in it, and a reply is meant to be able to cite
    /// them wherever they fit.
    Anywhere,
}

impl Subject {
    /// What a kind of measurement is about.
    ///
    /// Exhaustive on purpose, with no wildcard arm: a measurement added later
    /// must be placed by the person adding it, and the compiler is a cheaper
    /// reviewer than a wrong reply. A wildcard defaulting to [`Self::Anywhere`]
    /// would make every future fact publishable about every actor, silently.
    #[must_use]
    pub fn of(kind: Kind) -> Self {
        match kind {
            Kind::CreatorLaunches
            | Kind::CreatorMeasured
            | Kind::CreatorOrganic
            | Kind::CreatorInstant
            | Kind::CreatorStillborn
            | Kind::CreatorTransactions
            // The creator's own spending, so a sentence about the creator may
            // cite it and a sentence about holders may not.
            | Kind::DevBuy
            | Kind::DevBuyUnseen => Self::Creator,
            Kind::Holders | Kind::LargestHolderShare => Self::Holders,
            Kind::LaunchRecipients | Kind::LaunchTransactions => Self::Launch,
            Kind::VenueUnmeasured
            | Kind::VenueMeasured
            | Kind::VenueGraduated
            | Kind::VenueOrganic
            | Kind::VenueInstant
            | Kind::VenueStillborn
            // A band is a slice of the venue's population, not a fact about
            // this token, and a reply that cites a band rate as if it were
            // this token's own number is the second-most-likely way to be
            // wrong in public.
            | Kind::BandUnavailable
            | Kind::BandNeverGraduated
            | Kind::BandOrganic
            | Kind::BandInstant
            | Kind::BandInstantProbability
            | Kind::BandTimesBaseRate
            | Kind::BaseInstant
            | Kind::BaseGraduates
            | Kind::VenueFee
            | Kind::RoundTripKernel
            | Kind::RoundTripBar
            | Kind::CostBand => Self::Venue,
            Kind::Capacity
            | Kind::CapacityNone
            | Kind::CapacityAfterGraduation
            | Kind::CurveLiquidity => Self::Liquidity,
            Kind::Graduated | Kind::Age => Self::Token,
            Kind::OutcomeRate => Self::Launch,
        }
    }
}

/// A number a reply may contain, and what it was measured about.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Authorised {
    /// What the measurement was about.
    pub subject: Subject,
    /// The value.
    pub value: f64,
}

impl Authorised {
    /// A value with no subject, citable in any sentence.
    ///
    /// For callers whose numbers all describe one thing — the weekly record,
    /// the bot's own bio — where a subject rule has nothing to separate.
    #[must_use]
    pub const fn anywhere(value: f64) -> Self {
        Self {
            subject: Subject::Anywhere,
            value,
        }
    }
}

/// Why a reply was rejected.
#[derive(Clone, Debug, PartialEq)]
pub struct Fabricated {
    /// The literal as it appeared in the reply.
    pub literal: String,
    /// Its value.
    pub value: f64,
    /// Which of the two ways it was wrong.
    pub why: Why,
}

/// The two ways a number in a reply fails.
///
/// Separate because they mean different things about the model. A number
/// nothing measured is the model inventing; a number moved to the wrong
/// subject is the model reasoning past its evidence, which is the harder
/// failure to notice and the one worth reading in a log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// No fact on the sheet carries this value at all.
    NotMeasured,
    /// The sheet carries it, about something else.
    WrongSubject {
        /// What the sheet measured it about.
        measured: Subject,
        /// What the sentence it was written in was about.
        written_about: Subject,
    },
}

/// Checks a reply against the numbers a fact sheet authorises.
///
/// Returns every literal that is not accounted for, in the order they appear.
/// An empty result means the reply may ship.
#[must_use]
pub fn check(reply: &str, authorised: &[Authorised]) -> Vec<Fabricated> {
    let mut out = Vec::new();
    for sentence in sentences(reply) {
        // One named subject and no more. Two is not a contradiction to
        // resolve, it is a sentence like "the creator's launches all died on
        // this launchpad", where either subject could own the figure and
        // refusing would cost a true reply. Ambiguity falls back to plain
        // membership, which is exactly the rule that shipped before this.
        let named = named_in(sentence);
        let about = if named.len() == 1 {
            named.first()
        } else {
            None
        };

        for (literal, value) in literals(sentence) {
            let measured = subjects_of(value, &literal, authorised);
            if measured.is_empty() {
                out.push(Fabricated {
                    literal,
                    value,
                    why: Why::NotMeasured,
                });
            } else if let Some(&about) = about
                && !measured.contains(&about)
                && !measured.contains(&Subject::Anywhere)
            {
                out.push(Fabricated {
                    literal,
                    value,
                    // The first, because the list is in sheet order and the
                    // first fact carrying the value is the one an operator
                    // reading the log will go and look at.
                    why: Why::WrongSubject {
                        measured: measured[0],
                        written_about: about,
                    },
                });
            }
        }
    }
    out
}

/// Every subject a literal could honestly have come from.
///
/// More than one because two facts can share a value: a venue that graduates
/// 11% and a creator with 11 launches both authorise `11`, and a sentence
/// naming either of them is entitled to it.
fn subjects_of(value: f64, literal: &str, authorised: &[Authorised]) -> Vec<Subject> {
    let mut out: Vec<Subject> = Vec::new();
    for a in authorised {
        if matches_at_written_precision(a.value, value, literal) && !out.contains(&a.subject) {
            out.push(a.subject);
        }
    }
    out
}

/// Splits text into sentences for the subject rule.
///
/// A full stop between two digits is a decimal point and not a sentence end:
/// splitting "25.1" would put `25` in one sentence and `1` in another, and the
/// subject rule would then judge half a number against the wrong words.
fn sentences(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        let ends = match b {
            b'!' | b'?' | b'\n' => true,
            b'.' => {
                let before_is_digit = i > 0 && bytes[i - 1].is_ascii_digit();
                let after_is_digit = bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
                !(before_is_digit && after_is_digit)
            }
            _ => false,
        };
        if ends {
            // `i + 1` is a character boundary: every byte matched above is
            // ASCII, and an ASCII byte in UTF-8 is a whole character.
            out.push(&text[start..=i]);
            start = i + 1;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// Which subjects a sentence names out loud.
///
/// A deliberately short vocabulary of the words that can only mean one
/// subject. "Wallet" is not here, because the creator has one and so does the
/// largest holder; a word that points at two subjects would make true
/// sentences look like misattributions. Matching is on whole lowercased words,
/// so "developer" does not match inside "development" and a hashtag does not
/// hide a word from it.
fn named_in(sentence: &str) -> Vec<Subject> {
    const CREATOR: &[&str] = &[
        "creator",
        "creators",
        "deployer",
        "deployers",
        "dev",
        "devs",
        "developer",
        "developers",
        "founder",
        "founders",
        "minter",
    ];
    const HOLDERS: &[&str] = &["holder", "holders", "whale", "whales"];
    const VENUE: &[&str] = &["launchpad", "launchpads", "venue", "pons", "pump"];
    const LIQUIDITY: &[&str] = &["liquidity", "pool", "pools", "lp"];

    let mut out: Vec<Subject> = Vec::new();
    for word in sentence
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let word = word.to_ascii_lowercase();
        let subject = if CREATOR.contains(&word.as_str()) {
            Subject::Creator
        } else if HOLDERS.contains(&word.as_str()) {
            Subject::Holders
        } else if VENUE.contains(&word.as_str()) {
            Subject::Venue
        } else if LIQUIDITY.contains(&word.as_str()) {
            Subject::Liquidity
        } else {
            continue;
        };
        if !out.contains(&subject) {
            out.push(subject);
        }
    }
    out
}

/// Whether one authorised value explains one literal.
///
/// A year, a slot or an ordinal is still a number, and there is no safe
/// general exemption: "6 recipients" and "2026" are the same token to a
/// scanner. So nothing is exempt, and anything the reply is genuinely allowed
/// to say -- the slot included -- is put on the sheet instead.
fn matches_at_written_precision(authorised: f64, value: f64, literal: &str) -> bool {
    // Exact, for the common case and for integers.
    if (authorised - value).abs() < 1e-9 {
        return true;
    }
    // Or the authorised value rounded to the precision the model wrote.
    //
    // An exact comparison is correct here and the lint is wrong about it:
    // both sides have just been rounded to the SAME number of decimals, so
    // they are the same grid points or they are different ones. A tolerance
    // on top would widen the rule by an unstated amount, which is the one
    // thing a fidelity check must not have.
    let decimals = decimals_in(literal);
    #[expect(
        clippy::float_cmp,
        reason = "both sides are rounded to the same precision immediately above; a \
                  margin here would widen the tolerance rule by an unstated amount"
    )]
    let same = round_to(authorised, decimals) == round_to(value, decimals);
    same
}

/// Rounds to a number of decimal places.
fn round_to(v: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(i32::try_from(decimals).unwrap_or(0));
    (v * factor).round() / factor
}

/// How many decimal places a literal was written to.
fn decimals_in(literal: &str) -> u32 {
    literal
        .split_once('.')
        .map_or(0, |(_, frac)| u32::try_from(frac.len()).unwrap_or(0))
}

/// The shortest run of base58 characters treated as an address rather than a
/// number.
///
/// A Solana address is 32–44 base58 characters. No unit suffix, currency symbol
/// or ordinary word comes anywhere near this length, so the rule cannot swallow
/// a real figure — "4200bps" is nine characters and is still scanned.
const ADDRESS_MIN_LEN: usize = 32;

/// Blanks out address-shaped tokens before scanning.
///
/// A digit inside an identifier is not a claim. Without this, the mint in
/// "Radar on 82U9hMTJP9Wz…" contributes half a dozen numbers that no fact sheet
/// authorises, and every reply naming the token it is about would be rejected.
///
/// It is a **security** rule and not only a cosmetic one. pump.fun addresses are
/// ground so that they end in `pump`, which is public proof that grinding an
/// address to contain a chosen substring is cheap. If digits inside an address
/// were authorised, an attacker could mint a token whose address contains the
/// figure they want published and then have it quoted back as measured.
fn blank_addresses(text: &str) -> String {
    const B58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0;
    // Bounded by the input length rather than by trusting the cursor -- see the
    // note in `literals`, which had the same shape and cost two runners five
    // minutes each before it was changed.
    for _ in 0..=chars.len() {
        if i >= chars.len() {
            break;
        }
        let start = i;
        for _ in 0..=chars.len() {
            if i >= chars.len() || !B58.contains(chars[i]) {
                break;
            }
            i += 1;
        }
        let run = i - start;
        if run >= ADDRESS_MIN_LEN {
            out.extend(std::iter::repeat_n(' ', run));
        } else {
            out.extend_from_slice(&chars[start..i]);
        }
        if i < chars.len() {
            out.push(chars[i]);
            i += 1;
        }
    }
    out.into_iter().collect()
}

/// Every numeric literal in a piece of text, with its value.
///
/// Address-shaped tokens are blanked first — see [`blank_addresses`]. Commas
/// inside a number are group separators and are dropped, because a model writing
/// `17,497` means the same as `17497` and a scanner that split them would report
/// `17` and `497`: two fabricated numbers where there were none, which would
/// send every reply to the template.
///
/// A `%` or a `$` around the number is punctuation and is not part of it.
#[must_use]
pub fn literals(text: &str) -> Vec<(String, f64)> {
    let text = blank_addresses(text);
    let bytes: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;

    // Both scans are bounded by the length of the input rather than by trusting
    // the cursor to advance. Every iteration of either moves `i` forward by at
    // least one or stops, so `bytes.len() + 1` rounds is always enough.
    //
    // Written this way because CI reported the `while` forms as timeouts: an
    // `i += 1` mutated to `i *= 1` leaves the cursor where it is and the scan
    // never ends, costing a runner five minutes and reporting nothing useful.
    // Bounded, the same mutation ends the scan with a wrong answer, which a test
    // catches in milliseconds. A hang is a poor way to detect a bug.
    for _ in 0..=bytes.len() {
        if i >= bytes.len() {
            break;
        }
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        let mut seen_dot = false;
        for _ in 0..=bytes.len() {
            if i >= bytes.len() {
                break;
            }
            let c = bytes[i];
            if c.is_ascii_digit() {
                i += 1;
            } else if c == ',' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                // A separator only when digits continue on the other side, so
                // "6, 7 and 8" is three numbers rather than one.
                i += 1;
            } else if c == '.' && !seen_dot && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit()
            {
                // A decimal point only when a digit follows: a number ending a
                // sentence must not swallow the full stop.
                seen_dot = true;
                i += 1;
            } else {
                break;
            }
        }
        let literal: String = bytes[start..i].iter().collect();
        let cleaned: String = literal.chars().filter(|c| *c != ',').collect();
        if let Ok(value) = cleaned.parse::<f64>() {
            out.push((cleaned, value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The membership rule on its own, with no subject attached to anything.
    ///
    /// Shadows [`super::check`] for the tests below it, which predate ADR 0031
    /// and are all about whether a value is on the sheet at all. Written as a
    /// shim rather than by rewriting fourteen call sites because those tests
    /// are the record of the tolerance rule, and a diff that touches every one
    /// of them hides which ones actually changed. The subject rule's own tests
    /// call `super::check` directly.
    fn check(reply: &str, authorised: &[f64]) -> Vec<Fabricated> {
        let authorised: Vec<Authorised> = authorised
            .iter()
            .copied()
            .map(Authorised::anywhere)
            .collect();
        super::check(reply, &authorised)
    }

    #[test]
    fn a_fabricated_figure_is_caught() {
        // The verification standard: inject a number nobody measured and
        // confirm the check catches it.
        let caught = check("The round trip here is 4200 bps.", &[850.0, 456.0]);
        assert_eq!(caught.len(), 1);
        assert_eq!(caught[0].literal, "4200");
    }

    #[test]
    fn an_authorised_figure_passes() {
        assert!(check("The round trip is 850 bps.", &[850.0, 456.0]).is_empty());
    }

    #[test]
    fn ordinary_rounding_is_allowed_and_misstatement_is_not() {
        // The tolerance rule, stated as a test because a tolerance nobody
        // checked is a hole nobody knows the size of.
        assert!(
            check("about 25%", &[25.1]).is_empty(),
            "25 rounds from 25.1"
        );
        assert!(check("25.1%", &[25.1]).is_empty());
        assert!(
            !check("25.4%", &[25.1]).is_empty(),
            "not a rounding of 25.1"
        );
        assert!(!check("68%", &[25.1]).is_empty(), "0008's dead number");
        // Rounding does not licence a different order of magnitude.
        assert!(!check("250", &[25.1]).is_empty());
    }

    #[test]
    fn a_number_at_the_very_end_of_the_text_does_not_read_past_it() {
        // The decimal-point branch guards with `i + 1 < bytes.len()` before
        // looking at the next byte. Mutated to `i - 1`, the guard is true at the
        // last index and the read runs off the end. The existing full-stop test
        // did not catch it because its full stop was not the final byte.
        //
        // Both of these end exactly at the character after the digits.
        assert!(check("the figure is 25.", &[25.0]).is_empty());
        assert_eq!(literals("ends on 6").len(), 1);

        // And the *text* of the literal, not only how many there are. The guard
        // has a second `i + 1`, inside `bytes[i + 1]`, and mutating that one to
        // `i - 1` looks at the digit already consumed instead of the character
        // ahead -- so the full stop is swallowed into the number. "6." and "6"
        // parse to the same f64, which is why a count cannot tell them apart;
        // what changes is the precision the literal is taken to be written to,
        // and that is what the rounding tolerance is derived from.
        // The full stop must not be the final character, or the guard before it
        // (`i + 1 < bytes.len()`) short-circuits and the byte in question is
        // never read -- which is how the first version of this test missed the
        // mutation entirely while looking straight at it.
        let ends = literals("ends on 6. Next");
        assert_eq!(ends.len(), 1);
        assert_eq!(
            ends[0].0, "6",
            "the full stop is punctuation, not a decimal point"
        );

        // The same character *is* a decimal point when a digit follows it.
        let inner = literals("it is 6.5 here");
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].0, "6.5");

        // A comma at the very end of the text, which is the group-separator
        // branch's version of the same edge. Its guard is `i + 1 <
        // bytes.len()`, and every way of getting that guard wrong -- `<=`,
        // `i - 1`, `i * 1` -- makes it true at the last index and reads
        // `bytes[i + 1]` off the end.
        assert_eq!(literals("counted 6,").len(), 1);
        assert_eq!(literals("counted 6,")[0].0, "6");

        // And the separator still works where it should, so the guard is not
        // simply refusing everything.
        assert_eq!(literals("counted 17,497 launches")[0].0, "17497");

        // The guard has to read *forward*. Reading the character behind instead
        // consumes the comma, and the decimal-point branch then takes the `.`,
        // so "1,.5" becomes a single 1.5 where there were two numbers.
        //
        // That is not cosmetic. Both 1 and 5 can be authorised figures, and
        // their merge is a third number that appears in the reply and in no
        // fact sheet -- the exact fabrication this module exists to refuse,
        // arriving out of two values that were each allowed.
        //
        // Found by checking the mutation exhaustively over short strings rather
        // than by reasoning about it: the argument that it could not matter was
        // wrong for 2,232 of 97,655 cases.
        assert_eq!(
            literals("1,.5").len(),
            2,
            "a comma before a decimal point is not a group separator"
        );
    }

    #[test]
    fn the_exact_match_is_a_difference_and_not_a_sum_or_a_ratio() {
        // Two values that agree far past any precision a model would write, but
        // round to different grid points at the precision it *did* write. Only
        // the `(a - value).abs()` branch can pass this, so it is the branch
        // being tested rather than the rounding one underneath it.
        //
        // Every mutation of that expression breaks it: a sum is ~1.0, a ratio is
        // ~1.0, and neither is under the epsilon -- and with the rounding branch
        // disagreeing, the literal is reported as fabricated.
        assert!(
            check("0.5000000002", &[0.500_000_000_1]).is_empty(),
            "a difference of 1e-10 is the same number to any reader"
        );
    }

    #[test]
    fn the_epsilon_is_exclusive_and_the_boundary_is_where_it_says() {
        // A difference of exactly the epsilon is *not* a match. `<` and `<=` are
        // one character apart and disagree only here, and a tolerance rule that
        // silently widened by one representable step is the thing this whole
        // module exists to prevent.
        //
        // 1e-9 minus zero is exactly 1e-9 in binary floating point, so this is a
        // real boundary rather than an approximation of one.
        assert!(
            !check("0.000000001", &[0.0]).is_empty(),
            "exactly the epsilon is outside it"
        );
        // And a step under it is inside.
        assert!(check("0.000000001", &[0.000_000_000_1]).is_empty());
    }

    #[test]
    fn group_separators_do_not_split_a_number_into_two_inventions() {
        // Without this, every reply quoting the population would be sent to the
        // template -- and a check that fires on everything gets switched off.
        assert_eq!(
            literals("17,497 launches"),
            vec![("17497".to_owned(), 17_497.0)]
        );
        assert!(check("17,497 launches", &[17_497.0]).is_empty());
    }

    #[test]
    fn a_comma_between_numbers_is_still_a_list() {
        // The other direction: "6, 7 and 8" must not become 678.
        let found = literals("6, 7 and 8");
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].0, "6");
        assert_eq!(found[2].0, "8");
    }

    #[test]
    fn a_number_ending_a_sentence_does_not_swallow_the_full_stop() {
        assert_eq!(literals("It is 6."), vec![("6".to_owned(), 6.0)]);
        assert_eq!(literals("It is 6.5."), vec![("6.5".to_owned(), 6.5)]);
    }

    #[test]
    fn currency_and_percent_signs_are_punctuation() {
        assert_eq!(
            literals("$50 and 30%"),
            vec![("50".to_owned(), 50.0), ("30".to_owned(), 30.0),]
        );
    }

    #[test]
    fn digits_inside_an_address_are_not_claims() {
        // Without this, every reply naming the token it is about is rejected,
        // because a base58 address is full of digits. And because pump.fun
        // addresses are ground to end in `pump`, an attacker could otherwise
        // grind one containing the figure they wanted published.
        let mint = "82U9hMTJP9WzBAG5852mRoQ4Qbwa48nWudPyGEpHpump";
        assert!(literals(mint).is_empty(), "{:?}", literals(mint));
        assert!(check(&format!("Real or Rug on {mint}: 6 recipients."), &[6.0]).is_empty());
    }

    #[test]
    fn a_short_token_with_letters_is_still_scanned() {
        // The rule must not become a way to smuggle a number past the check by
        // gluing a unit to it. "4200bps" is nowhere near address length.
        let found = literals("the round trip is 4200bps");
        assert_eq!(found, vec![("4200".to_owned(), 4200.0)]);
        assert!(!check("4200bps", &[850.0]).is_empty());
    }

    #[test]
    fn a_reply_with_no_numbers_passes_trivially() {
        assert!(check("Real or Rug has no record of this token.", &[]).is_empty());
    }

    #[test]
    fn every_fabricated_literal_is_reported_not_just_the_first() {
        // The log is how a public mistake becomes a correction rather than an
        // argument, so it has to say everything that was wrong.
        let caught = check("11 recipients, 99% of them, 4200 bps", &[11.0]);
        assert_eq!(caught.len(), 2);
        assert_eq!(caught[0].literal, "99");
        assert_eq!(caught[1].literal, "4200");
    }

    #[test]
    fn a_model_doing_arithmetic_on_real_facts_is_still_caught() {
        // The realistic failure. Both inputs are authorised; the sum is not a
        // measurement, and it is exactly the kind of helpfulness that would
        // otherwise put an unmeasured figure under a name that promises
        // measurement.
        assert!(!check("850 plus 456 is 1306 bps", &[850.0, 456.0]).is_empty());
    }

    /// A sheet that measured one thing at 41%.
    fn holders_at_41() -> Vec<Authorised> {
        vec![Authorised {
            subject: Subject::Holders,
            value: 41.0,
        }]
    }

    #[test]
    fn a_figure_measured_about_the_holders_cannot_be_published_about_the_creator() {
        // The sentence this whole change exists for. Every digit in it is on
        // the sheet, and it accuses a person of something nobody measured.
        let caught = super::check("The creator already dumped 41%.", &holders_at_41());
        assert_eq!(caught.len(), 1, "{caught:?}");
        assert_eq!(caught[0].literal, "41");
        assert_eq!(
            caught[0].why,
            Why::WrongSubject {
                measured: Subject::Holders,
                written_about: Subject::Creator,
            }
        );

        // The true version of the same figure ships.
        assert!(
            super::check("The top holder has 41%.", &holders_at_41()).is_empty(),
            "the sentence the sheet actually supports must still pass"
        );
    }

    #[test]
    fn the_subject_rule_stays_out_of_an_ambiguous_sentence() {
        // Two subjects named: either could own the figure, and refusing would
        // cost a true reply. Falls back to plain membership, which passes.
        assert!(
            super::check(
                "The creator is also the largest holder at 41%.",
                &holders_at_41()
            )
            .is_empty()
        );
        // And none named at all.
        assert!(super::check("41% sits in one place.", &holders_at_41()).is_empty());
    }

    #[test]
    fn one_wrong_sentence_does_not_condemn_the_right_one_beside_it() {
        // Per sentence, not per reply: the subject is read from the words
        // around the number, so the sentence boundary is what the rule is
        // measured against.
        let caught = super::check(
            "The top holder has 41%. The creator already dumped 41%.",
            &holders_at_41(),
        );
        assert_eq!(caught.len(), 1, "{caught:?}");
        assert!(matches!(caught[0].why, Why::WrongSubject { .. }));
    }

    #[test]
    fn a_remark_about_the_sheet_is_citable_in_any_sentence() {
        // The read point and the not-known lines are Radar's words about the
        // sheet rather than measurements of a subject on it, so a sentence
        // naming an actor may still cite them.
        let authorised = vec![Authorised::anywhere(444_007_820.0)];
        assert!(
            super::check(
                "The creator's side of this was read at slot 444007820.",
                &authorised
            )
            .is_empty()
        );
    }

    #[test]
    fn a_decimal_point_does_not_end_a_sentence() {
        // If it did, "25.1" would be judged as `25` in one sentence and `1` in
        // the next, and the subject rule would rule on half a number.
        assert_eq!(
            sentences("the top holder has 25.1% of it"),
            vec!["the top holder has 25.1% of it"]
        );
        let authorised = vec![Authorised {
            subject: Subject::Holders,
            value: 25.1,
        }];
        assert!(super::check("The top holder has 25.1%.", &authorised).is_empty());
    }

    #[test]
    fn only_a_digit_on_both_sides_makes_a_full_stop_a_decimal_point() {
        // One side is not enough, in either direction. "1. the creator" is a
        // numbered list and ends a sentence; "v.2" is a name and ends one too.
        // Only a digit on both sides is a number that must be kept whole.
        assert_eq!(sentences("v.2"), vec!["v.", "2"]);
        assert_eq!(sentences("41. the creator"), vec!["41.", " the creator"]);
    }

    #[test]
    fn a_sentence_keeps_its_own_ending_and_gives_the_next_one_none() {
        // The terminator belongs to the sentence it ends. If it led the next
        // one instead, every sentence after the first would start with a stray
        // mark, and a trailing terminator would add an empty sentence after it.
        assert_eq!(sentences("gone! next"), vec!["gone!", " next"]);
        assert_eq!(sentences("gone!"), vec!["gone!"]);
    }

    #[test]
    fn a_number_nothing_measured_is_still_caught_and_says_so() {
        let caught = super::check("The creator already dumped 77%.", &holders_at_41());
        assert_eq!(caught.len(), 1, "{caught:?}");
        assert_eq!(caught[0].why, Why::NotMeasured);
    }

    #[test]
    fn a_word_is_matched_whole_and_not_inside_another() {
        // "development" is not the creator, and a rule that thought it was
        // would refuse true sentences about a token's roadmap.
        assert_eq!(named_in("development is 41% done"), Vec::new());
        // Case and punctuation do not hide a word from it, and each subject
        // is named once however many of its words appear.
        assert_eq!(
            named_in("the DEV, the dev's holder"),
            vec![Subject::Creator, Subject::Holders]
        );
    }
}
