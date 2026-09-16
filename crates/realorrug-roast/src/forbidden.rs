// SPDX-License-Identifier: Apache-2.0
//! Claims the analyst may not make, enforced rather than requested.
//!
//! # Why these are in code
//!
//! `GOAL.md` lists three constraints "to be designed in rather than
//! discovered", and the first is *"it states only what was measured. Never
//! 'this is a scam' — always the count, the history, the number."* A system
//! prompt asking for that is a request. This is the part that makes it true.
//!
//! The categories are not stylistic. Each one is a different kind of exposure,
//! and they are worth separating because they fail differently:
//!
//! - **A verdict about a person** — "scam", "rug", "fraud" — is a public,
//!   automated, at-scale statement about an identifiable project. Factual
//!   accuracy plus the absence of verdict words is the protection, and the
//!   second half is this file.
//! - **Reassurance** — "safe", "legit" — is worse than an accusation, because
//!   the reader acts on it. `GOAL.md` refuses a single safety score for exactly
//!   this reason: "a green shield is 'unknown rendered as safe'".
//! - **Advice** — "buy", "sell", "hold", a price prediction — moves measured
//!   commentary toward regulated investment advice. Measurements are not advice;
//!   recommendations are.
//! - **A cabal implied from a recipient count.** `0008` never resolved
//!   recipients to owners, and `0012` proves it cannot be done from this data:
//!   a destination is an `(owner, mint)` token account, so recipient sets cannot
//!   recur across mints. Saying "six wallets" or "six people" claims an identity
//!   the measurement does not carry.
//! - **Graduation history as a good sign.** `0011` measures organic graduations
//!   ending at a median −3,228 bps against −853 for tokens that never
//!   graduated. A creator whose tokens graduate is not a safer bet, and 0007's
//!   signal must never be published as though they were.
//!
//! # It is deliberately blunt
//!
//! This matches substrings, so it refuses a reply that says "this is not a
//! scam" as readily as one that says "this is a scam". That is the right trade:
//! the cost of a false positive is the deterministic template shipping instead,
//! and the cost of a false negative is a public accusation. A checker that tried
//! to read negation would be a checker arguing about meaning, and the point of
//! this one is that it does not argue.

use crate::verdict::Level;

/// A phrase the reply may not contain, and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rule {
    /// The phrase, lowercase.
    pub phrase: &'static str,
    /// Which line it crosses.
    pub because: &'static str,
}

/// Every forbidden phrase.
pub const RULES: &[Rule] = &[
    // A verdict about a person or a project.
    Rule {
        phrase: "scam",
        because: "a verdict about an identifiable project",
    },
    Rule {
        phrase: "rug",
        because: "a verdict about an identifiable project",
    },
    Rule {
        phrase: "fraud",
        because: "a verdict about an identifiable project",
    },
    Rule {
        phrase: "stole",
        because: "a verdict about an identifiable project",
    },
    Rule {
        phrase: "stolen",
        because: "a verdict about an identifiable project",
    },
    Rule {
        phrase: "criminal",
        because: "a verdict about an identifiable project",
    },
    Rule {
        phrase: "thief",
        because: "a verdict about an identifiable project",
    },
    // Reassurance. Worse than an accusation, because it is acted on.
    Rule {
        phrase: "is safe",
        because: "reassurance -- unknown rendered as safe",
    },
    Rule {
        phrase: "looks safe",
        because: "reassurance -- unknown rendered as safe",
    },
    Rule {
        phrase: "totally safe",
        because: "reassurance -- unknown rendered as safe",
    },
    Rule {
        phrase: "legit",
        because: "reassurance -- unknown rendered as safe",
    },
    Rule {
        phrase: "trustworthy",
        because: "reassurance -- unknown rendered as safe",
    },
    // Advice.
    Rule {
        phrase: "you should buy",
        because: "advice, not commentary",
    },
    Rule {
        phrase: "you should sell",
        because: "advice, not commentary",
    },
    Rule {
        phrase: "should buy",
        because: "advice, not commentary",
    },
    Rule {
        phrase: "should sell",
        because: "advice, not commentary",
    },
    Rule {
        phrase: "buy this",
        because: "advice, not commentary",
    },
    Rule {
        phrase: "sell this",
        because: "advice, not commentary",
    },
    Rule {
        phrase: "hold this",
        because: "advice, not commentary",
    },
    Rule {
        phrase: "ape in",
        because: "advice, not commentary",
    },
    Rule {
        phrase: "good investment",
        because: "advice, not commentary",
    },
    Rule {
        phrase: "bad investment",
        because: "advice, not commentary",
    },
    // NOT a rule: "financial advice". It was one, and the test asserting the
    // deterministic template passes this check caught it -- the template ends
    // "Not financial advice", so the analyst's own safest possible reply was
    // being refused by its own rule. The disclaimer is the thing ADR 0005's
    // unresolved precondition 3 asks for, and blocking it would have removed
    // the one sentence most worth keeping. The advice itself is caught by the
    // phrases above.
    Rule {
        phrase: "to the moon",
        because: "a price prediction",
    },
    Rule {
        phrase: "will pump",
        because: "a price prediction",
    },
    Rule {
        phrase: "will dump",
        because: "a price prediction",
    },
    Rule {
        phrase: "price target",
        because: "a price prediction",
    },
    Rule {
        phrase: "guaranteed",
        because: "a price prediction",
    },
    // A cabal identity the measurement cannot see. 0012.
    Rule {
        phrase: "wallets bought",
        because: "recipients are token accounts, not wallets (0012)",
    },
    Rule {
        phrase: "people bought",
        because: "recipients are token accounts, not people (0012)",
    },
    Rule {
        phrase: "insiders",
        because: "recipients are token accounts, not identified people (0012)",
    },
    Rule {
        phrase: "the same group",
        because: "recipient sets cannot recur across mints (0012)",
    },
    Rule {
        phrase: "cabal",
        because: "an identity the recipient count cannot carry (0012)",
    },
    Rule {
        phrase: "one person",
        because: "an identity the recipient count cannot carry (0012)",
    },
    // Added 2026-09-07, from reading the list against what a savage reply
    // actually reaches for. Every one of these is a verdict this account may
    // not deliver, and every one of them passed: the list had thirty-one
    // phrases and none of them was the word a sharp model would pick.
    //
    // **This is the weaker half of the defence and is meant to be.** A
    // substring list can only ever refuse the phrasings somebody thought of;
    // `crate::clause` is the half that works by construction, and it removes
    // the model's ability to write a word at all. These are here because they
    // are obvious once named, not because the list is now complete.
    //
    // What this list is *for* changed with that: it no longer reads the model's
    // prose, because there is none. It reads Radar's own clauses, so the author
    // it catches is a person adding a vetted sentence -- which is a slower and
    // rarer mistake, and the one that would otherwise ship unchallenged.
    Rule {
        phrase: "honeypot",
        because: "a verdict about an identifiable project",
    },
    Rule {
        phrase: "exit liquidity",
        because: "a verdict about an identifiable project",
    },
    Rule {
        phrase: "dumped on",
        because: "a verdict about an identifiable project",
    },
    Rule {
        phrase: "dumping on",
        because: "a verdict about an identifiable project",
    },
    Rule {
        // The most-quoted number-shaped claim in the market, and it is a price
        // prediction wearing a multiple.
        phrase: "100x",
        because: "a price prediction",
    },
    Rule {
        phrase: "10x",
        because: "a price prediction",
    },
    Rule {
        phrase: "bullish",
        because: "a price prediction",
    },
    Rule {
        phrase: "bearish",
        because: "a price prediction",
    },
    Rule {
        phrase: "don't buy",
        because: "advice, not commentary",
    },
    Rule {
        // The same words without the apostrophe, and without the *typographic*
        // one. A model writing prose uses U+2019 as often as U+0027, and a list
        // that only knows the ASCII form refuses one spelling of a sentence and
        // publishes the other.
        phrase: "dont buy",
        because: "advice, not commentary",
    },
    Rule {
        phrase: "don\u{2019}t buy",
        because: "advice, not commentary",
    },
    Rule {
        // A verdict, and the one word on this list that is also an ordinary
        // adjective. It is here for "the launch looks clean", which is
        // reassurance in the same sense "is safe" is — an unknown rendered as
        // an all-clear, which is the thing that gets acted on.
        phrase: "looks clean",
        because: "reassurance -- unknown rendered as safe",
    },
    Rule {
        phrase: "is clean",
        because: "reassurance -- unknown rendered as safe",
    },
];

/// The account's own names, which the rules would otherwise refuse. Lowercase.
///
/// **`cabalhunter.org`**, the site. The rule refuses "cabal" because a reply
/// must not imply an identity the recipient count cannot see (research 0012),
/// and it is right to. The side effect was that **no post could carry its own
/// address**: the weekly result ended "Rule and leaderboard: on the site" with
/// no link, and a reader who wanted to check the rule had to go and find it.
///
/// **`realorrug`**, the bot's and the token's name (design 0019 §5.4). The
/// "rug" rule matches by substring, so without this every reply that names the
/// account -- `@realorrug`, `$REALORRUG`, a `realorrug.` address -- was refused.
/// The bare name covers all three spellings; a handle spelled differently
/// (`real_or_rug`) is not covered and would need adding here.
///
/// Masked before the scan rather than added as an exception to a rule, which
/// is the difference between "this exact name is allowed" and "any 'rug' inside
/// a longer word is allowed". Every other use of the word is still refused,
/// including `cabalhunter.org.evil.example` and "realorrug is a rug" -- the
/// mask consumes the literal and the surrounding text is scanned as it stands.
const OWN_NAMES: &[&str] = &["cabalhunter.org", "realorrug"];

/// What each own name becomes for the scan.
///
/// One per byte of the name, so a violation's position in the text is
/// unchanged, and a character no rule contains.
const MASK: char = '.';

/// The reply, lowercased, with the account's own names masked out.
fn masked(reply: &str) -> String {
    OWN_NAMES.iter().fold(reply.to_lowercase(), |text, name| {
        text.replace(name, &MASK.to_string().repeat(name.len()))
    })
}

/// A phrase that must not be published, found in a reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    /// The phrase found.
    pub phrase: &'static str,
    /// Why it is refused.
    pub because: &'static str,
}

/// Every forbidden phrase in a reply.
///
/// Case-insensitive, because a capitalised accusation is the same accusation.
#[must_use]
pub fn check(reply: &str) -> Vec<Violation> {
    let lower = masked(reply);
    RULES
        .iter()
        .filter(|r| lower.contains(r.phrase))
        .map(|r| Violation {
            phrase: r.phrase,
            because: r.because,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Design 0020 §5: the target check and the level check.
//
// `check` above and `RULES` above it are unchanged and stay exercised by
// their own tests through the transition ADR 0027 point 6 describes. Nothing
// below switches a caller -- the caller in `voice.rs` moves to this pair in a
// later commit, once the free-text voice path it belongs with exists.
//
// **Chain-blind, like `verdict::level`.** Neither function below takes a
// venue or reads anything chain-shaped: a person-reference and a verdict
// word mean the same thing on every chain (ADR 0028 point 1, one bot, one
// voice, every chain), so the rule that catches them must not grow a branch
// on which chain produced the reply.
// ---------------------------------------------------------------------------

/// A bare word that, alone in a sentence, marks the sentence as being about a
/// person rather than a token: "the **dev** rugged us," "ask the **team**."
///
/// Deliberately excludes "buyer," "holder," "wallet" and similar -- those
/// name a chain-observable role the sheet can back with a fact ("holders
/// can't sell"), not a person whose intent is being characterised.
const PERSON_WORDS: &[&str] = &["dev", "team", "founder", "creator"];

/// A word that characterises a person's intent or identity rather than an
/// observed event, used by [`check_target`].
///
/// §5: "scam," "scammer," "thief," "stole," "fraud," "criminal" by name, plus
/// "rug"/"rugged" and "stolen" -- the same words the level check gates by
/// level when aimed at a *token*, but never gated at all when aimed at a
/// *person*: a person-directed accusation is refused regardless of what the
/// sheet earned, because no fact sheet can prove intent.
const ACCUSATION_WORDS: &[&str] = &[
    "scam", "scammer", "thief", "fraud", "criminal", "stole", "stolen", "rug", "rugged",
];

/// Is `c` a character that cannot appear inside a bare word this file
/// matches -- i.e. a word boundary.
fn boundary(c: char) -> bool {
    !c.is_alphanumeric()
}

/// Does `haystack` (already lowercase) contain `word` as a whole word, not as
/// a substring of a longer one -- "rug" must not match inside "rugged," even
/// though both are separately in [`ACCUSATION_WORDS`].
///
/// Unlike [`check`]'s deliberately blunt substring match, the target and
/// level checks match whole words: a substring match here would make "rug"
/// fire on "rugged" too, double-counting one word as two violations and
/// making the per-level table above impossible to state precisely (the table
/// lists "rug" and "rugged" as separate, differently-earned words).
fn word_occurs(haystack: &str, word: &str) -> bool {
    // A range rather than a hand-rolled cursor. The cursor this replaced was
    // one mutation away from never advancing -- `start = at + 1` with the `+`
    // changed -- and `just mutants` reported exactly that, twice, as a timeout
    // rather than as a survivor. The justfile's own rule for a timeout is to
    // bound the loop, not to raise the budget. A range cannot be made not to
    // terminate by changing one operator, so the whole class is gone.
    //
    // O(haystack * word) rather than O(haystack), on strings that are one
    // social-media reply long. `starts_with` on a non-boundary index is false
    // rather than a panic, but the guard is kept because slicing `haystack`
    // by that index below is not.
    (0..=haystack.len()).any(|at| {
        haystack.is_char_boundary(at)
            && haystack[at..].starts_with(word)
            && haystack[..at].chars().next_back().is_none_or(boundary)
            && haystack[at + word.len()..]
                .chars()
                .next()
                .is_none_or(boundary)
    })
}

/// [`masked`], but case-preserving: the account's own names become [`MASK`]
/// wherever they occur (case-insensitively), and every other character keeps
/// its original case.
///
/// `check_target` needs this and `check`/`check_level` do not: a capitalised
/// multi-word run is one of §5's person-reference shapes, so lowercasing the
/// whole reply first (what [`masked`] does) would destroy the very signal
/// this function looks for. The own-name mask still has to run first, the
/// same reason it does in `masked`: without it, `RealOrRug` -- the account's
/// own capitalised handle -- would itself read as a two-word capitalised run.
///
/// Safe on non-ASCII text because [`OWN_NAMES`] are ASCII: a case-insensitive
/// byte match against an ASCII pattern can only ever match ASCII bytes, so it
/// can never land inside a multi-byte UTF-8 sequence and split a character.
fn mask_case_preserving(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut masked_byte = vec![false; bytes.len()];
    for name in OWN_NAMES {
        let needle = name.as_bytes();
        if needle.is_empty() {
            continue;
        }
        // `windows` rather than a cursor, for the reason given in
        // `word_occurs`: three mutations of this loop's `i +=` made it run
        // forever, which CI reports as a timeout. `windows` yields nothing
        // when the needle is longer than the text, which is the same answer
        // the cursor gave.
        //
        // It differs from the cursor in one way: it marks *overlapping*
        // occurrences, where the cursor skipped past each match. For these
        // needles that is the same set of bytes, because neither `realorrug`
        // nor `cabalhunter.org` has a prefix that is also one of its
        // suffixes, so no occurrence of either can overlap another.
        for (i, window) in bytes.windows(needle.len()).enumerate() {
            if window.eq_ignore_ascii_case(needle) {
                masked_byte[i..i + needle.len()].fill(true);
            }
        }
    }
    text.char_indices()
        .map(|(i, c)| if masked_byte[i] { MASK } else { c })
        .collect()
}

/// Splits text into roughly-sentence windows, the unit §5 checks a
/// person-reference and an accusation word for co-occurrence within.
///
/// Not a real sentence parser -- it splits on `.`, `!`, `?` and newline and
/// nothing more. That is enough for the shapes this file matches (none of
/// them span a full stop), and a real parser would be a second thing to get
/// wrong beside the check itself.
fn sentences(text: &str) -> impl Iterator<Item = &str> {
    text.split(['.', '!', '?', '\n'])
}

/// Does this (already own-name-masked) sentence contain an `@handle`?
fn has_handle(sentence: &str) -> bool {
    sentence.char_indices().any(|(i, c)| {
        c == '@'
            && sentence[i + c.len_utf8()..]
                .chars()
                .next()
                .is_some_and(|n| n.is_alphanumeric() || n == '_')
    })
}

/// Does this sentence contain two capitalised words back to back -- the
/// closed-list-free shape §5 uses for a named company ("Big Token is a
/// scam"), accepting the misses a real named-entity list would catch and the
/// occasional false hit a sentence-initial proper noun produces.
fn has_capitalized_run(sentence: &str) -> bool {
    let mut consecutive = 0;
    for word in sentence.split_whitespace() {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric());
        let starts_upper = word.chars().next().is_some_and(char::is_uppercase);
        if starts_upper {
            consecutive += 1;
            if consecutive >= 2 {
                return true;
            }
        } else {
            consecutive = 0;
        }
    }
    false
}

/// Does this (lowercase) sentence use a bare `0x` address as the *subject* of
/// a copula ("`0x1234… is a scammer`"), rather than the subject of an
/// *observed* verb ("`0x1234… sold everything in block 3`")?
///
/// §5's own worked example is exactly this pair, and it says the distinction
/// is the verb, not the address: this checks specifically for "is"/"was"
/// immediately after the address (skipping punctuation such as an ellipsis),
/// so an address followed by any other verb -- "sold," "bought," "moved" --
/// is never treated as a person-reference by this shape alone.
fn has_address_subject(sentence_lower: &str) -> bool {
    // Two std iterators rather than two cursors, for the reason given in
    // `word_occurs`: three mutations between them made this function run
    // forever. `match_indices` is non-overlapping, which is what the cursor's
    // `start = at + 2` already did, and `0x` cannot overlap itself anyway.
    sentence_lower.match_indices("0x").any(|(at, _)| {
        let after = &sentence_lower[at + 2..];
        let hex = after.bytes().take_while(u8::is_ascii_hexdigit).count();
        // `0x` with nothing after it is two characters of prose, not an
        // address. The hex run is what makes it one.
        if hex == 0 {
            return false;
        }
        let rest = after[hex..].trim_start_matches(|c: char| !c.is_alphanumeric());
        let next_word: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
        next_word == "is" || next_word == "was"
    })
}

/// Refuses an accusation aimed at a person, account or company -- ADR 0027
/// point 2, design 0020 §5's target check.
///
/// A violation needs both a person-reference (an `@handle`, a bare
/// "dev"/"team"/"founder"/"creator," a capitalised multi-word run, or a bare
/// `0x` address used as a copula's subject) and an [`ACCUSATION_WORDS`] entry
/// in the same roughly-a-sentence window. Runs after the own-name mask, same
/// as `check`, so `realorrug` is never itself read as a person-reference.
///
/// **False-refusal cost, accepted in §5**: "the team's tokenomics are a
/// scam" is about the *design*, not a person, and is refused anyway, because
/// "team" precedes "scam" in the same sentence. The conservative direction
/// (refuse, fall back to the template) is the one AGENTS.md rule 7 already
/// prefers for missing certainty.
#[must_use]
pub fn check_target(text: &str) -> Vec<Violation> {
    let masked_text = mask_case_preserving(text);
    let mut violations = Vec::new();
    for sentence in sentences(&masked_text) {
        let lower = sentence.to_lowercase();
        let has_person = has_handle(sentence)
            || PERSON_WORDS.iter().any(|w| word_occurs(&lower, w))
            || has_capitalized_run(sentence)
            || has_address_subject(&lower);
        if !has_person {
            continue;
        }
        for word in ACCUSATION_WORDS {
            if word_occurs(&lower, word) {
                violations.push(Violation {
                    phrase: word,
                    because: "an accusation aimed at a person, account or company",
                });
            }
        }
    }
    violations
}

/// One row of §5's per-level word table: a word this level's sheet has not
/// earned, and why.
type LevelRule = (&'static str, &'static str);

/// `CantTell`'s ceiling: a fact the sheet needed could not be read, so
/// nothing on this list may be said regardless of what else is true --
/// exactly ADR 0027's own consequence, quoted in §5: *"A verdict on a token
/// the analyst cannot fully read must be `CantTell`, never `NothingUglyYet`.
/// Getting this backwards would turn a blind spot into an endorsement."*
const CANTTELL_WORDS: &[LevelRule] = &[
    ("safe", "a blind spot presented as an all-clear (ADR 0027)"),
    ("clean", "a blind spot presented as an all-clear (ADR 0027)"),
    ("fine", "a blind spot presented as an all-clear (ADR 0027)"),
    ("legit", "a blind spot presented as an all-clear (ADR 0027)"),
    (
        "trustworthy",
        "a blind spot presented as an all-clear (ADR 0027)",
    ),
    (
        "healthy",
        "a blind spot presented as an all-clear (ADR 0027)",
    ),
    (
        "nothing ugly",
        "a blind spot presented as an all-clear (ADR 0027)",
    ),
];

/// `NothingUglyYet`'s ceiling: no bad signal was found, but nothing here has
/// *earned* a rug word -- that word sits at `Rugged` only, per §5's second
/// headline row -- and "guaranteed" is a price-prediction-shaped promise no
/// young, thin-evidence launch has earned either.
const NOTHINGUGLYYET_WORDS: &[LevelRule] = &[
    (
        "rug",
        "\"rug\"/\"rugged\" describe an event only Rugged has earned",
    ),
    (
        "rugged",
        "\"rug\"/\"rugged\" describe an event only Rugged has earned",
    ),
    (
        "stole",
        "\"stole\"/\"stolen\" describe an event only Rugged has earned",
    ),
    (
        "stolen",
        "\"stole\"/\"stolen\" describe an event only Rugged has earned",
    ),
    (
        "safe",
        "young and clean is young, not safe -- the reply must carry the \"yet\"",
    ),
    (
        "guaranteed",
        "a promise no launch this thin on history has earned",
    ),
];

/// `Sketchy`'s ceiling: real red flags with an innocent explanation still
/// open earns neither reassurance (still flags open) nor a rug word (nothing
/// observed rose to a live-mechanics or rugged level).
const SKETCHY_WORDS: &[LevelRule] = &[
    (
        "rug",
        "\"rug\"/\"rugged\" describe an event only Rugged has earned",
    ),
    (
        "rugged",
        "\"rug\"/\"rugged\" describe an event only Rugged has earned",
    ),
    (
        "stole",
        "\"stole\"/\"stolen\" describe an event only Rugged has earned",
    ),
    (
        "stolen",
        "\"stole\"/\"stolen\" describe an event only Rugged has earned",
    ),
    (
        "safe",
        "a red flag is still open -- not reassurance-earning",
    ),
    (
        "clean",
        "a red flag is still open -- not reassurance-earning",
    ),
    (
        "fine",
        "a red flag is still open -- not reassurance-earning",
    ),
    (
        "legit",
        "a red flag is still open -- not reassurance-earning",
    ),
];

/// `RugMechanicsLive`'s ceiling: several strong signals are live right now,
/// which is damning enough without borrowing `Rugged`'s past-tense word --
/// "rugged" claims the removal already happened, and `RugMechanicsLive`
/// means the mechanics to do it are present, not that they were used.
const RUGMECHANICSLIVE_WORDS: &[LevelRule] = &[
    (
        "rug",
        "the mechanics are live, not yet used -- \"rugged\" is past tense Rugged alone has earned",
    ),
    (
        "rugged",
        "the mechanics are live, not yet used -- \"rugged\" is past tense Rugged alone has earned",
    ),
    (
        "stole",
        "the mechanics are live, not yet used -- \"stole\" is past tense Rugged alone has earned",
    ),
    (
        "stolen",
        "the mechanics are live, not yet used -- \"stole\" is past tense Rugged alone has earned",
    ),
];

/// Refuses a word above the ceiling the sheet's computed level earned,
/// regardless of who or what it is aimed at -- design 0020 §5's level check.
///
/// `Rugged` has no ceiling of its own: it is the level "rugged" and its
/// siblings are earned *at*, so nothing in this file's vocabulary is refused
/// there. Runs on the own-name-masked, lowercased text, same as `check`, so
/// `realorrug` is never caught by "rug."
///
/// Word, not substring: unlike `check`'s deliberate substring bluntness,
/// "rug" must not fire on "rugged" (they are different, separately-earned
/// rows of the table above), so this matches whole words only
/// ([`word_occurs`]).
#[must_use]
pub fn check_level(text: &str, level: Level) -> Vec<Violation> {
    let lower = masked(text);
    let rules: &[LevelRule] = match level {
        Level::CantTell => CANTTELL_WORDS,
        Level::NothingUglyYet => NOTHINGUGLYYET_WORDS,
        Level::Sketchy => SKETCHY_WORDS,
        Level::RugMechanicsLive => RUGMECHANICSLIVE_WORDS,
        Level::Rugged => &[],
    };
    rules
        .iter()
        .filter(|(word, _)| word_occurs(&lower, word))
        .map(|(word, because)| Violation {
            phrase: word,
            because,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_can_name_its_own_site_and_nothing_else_that_says_cabal() {
        // The rule refuses "cabal" for a good reason (0012) and the side
        // effect was that no post could carry its own address -- the weekly
        // result ended "Rule and leaderboard: on the site" with no link.
        //
        // Re-apply by deleting "cabalhunter.org" from `OWN_NAMES`: the first
        // two assertions fail. The rest are what stops the mask being a hole.
        assert!(check("Rule and leaderboard: cabalhunter.org/history").is_empty());
        assert!(check("CabalHunter.org/leaderboard").is_empty());

        // Every other cabal is still refused, including one dressed as a
        // hostname. The mask consumes the literal and the rest is scanned as
        // it stands, so the leading "a cabal" here is still found.
        assert!(!check("a cabal ran it").is_empty());
        assert!(!check("the cabal is cabalhunter.org").is_empty());
        assert!(!check("cabalhunters.org").is_empty());
        assert_eq!(check("a cabal ran it")[0].phrase, "cabal");
    }

    #[test]
    fn the_account_can_say_its_own_name_and_every_other_rug_is_still_refused() {
        // "rug" matches by substring, so before the mask every reply naming
        // the account was refused and the template shipped instead. Design
        // 0019 §5.4.
        //
        // Re-apply by deleting "realorrug" from `OWN_NAMES`: the first four
        // assertions fail. The rest are what stops the mask being a hole.
        assert!(check("Summoned by @realorrug").is_empty());
        assert!(check("$REALORRUG has no price on this sheet").is_empty());
        assert!(check("RealOrRug.com/history").is_empty());
        assert!(check("Rule and leaderboard: cabalhunter.org -- ask @realorrug").is_empty());

        // The mask consumes only the name, so a verdict beside it is found.
        assert_eq!(check("realorrug says: rug")[0].phrase, "rug");
        assert_eq!(check("@realorrug calls it a RUG")[0].phrase, "rug");
        assert!(!check("real or rug? a rug.").is_empty());
        assert!(!check("realorrug: scam").is_empty());
    }

    #[test]
    fn a_verdict_about_a_project_is_refused() {
        assert_eq!(check("This is a scam.")[0].phrase, "scam");
        assert_eq!(check("Classic RUG.")[0].phrase, "rug");
    }

    #[test]
    fn reassurance_is_refused_as_readily_as_accusation() {
        // GOAL.md refuses a single safety score because a green shield is
        // "unknown rendered as safe". This is the sentence form of one.
        assert!(!check("This one is safe.").is_empty());
        assert!(!check("Looks legit to me.").is_empty());
    }

    #[test]
    fn advice_and_price_predictions_are_refused() {
        assert!(!check("You should buy this.").is_empty());
        assert!(!check("This is going to the moon.").is_empty());
        assert!(!check("Guaranteed returns.").is_empty());
    }

    #[test]
    fn an_identity_the_measurement_cannot_see_is_refused() {
        // 0012: a destination is an (owner, mint) token account, so recipient
        // sets cannot recur across mints and "six wallets" claims something the
        // data does not hold.
        assert!(!check("Six wallets bought it in the launch block.").is_empty());
        assert!(!check("All one person.").is_empty());
        assert!(!check("The same group as last time.").is_empty());
    }

    #[test]
    fn a_denial_is_refused_too_and_that_is_deliberate() {
        // Blunt on purpose. A checker that read negation would be a checker
        // arguing about meaning; the cost of this false positive is the
        // deterministic template, and the cost of the false negative is a
        // public accusation.
        assert!(!check("This is not a scam.").is_empty());
    }

    #[test]
    fn an_ordinary_measured_reply_passes() {
        let reply = "Eleven recipients in the launch block. 0.5% of launches that never \
                     graduated look like that. The round trip at $50 is about 4.6%. \
                     Radar has no record of this creator.";
        assert!(check(reply).is_empty(), "{:?}", check(reply));
    }

    #[test]
    fn every_rule_is_lowercase_or_it_can_never_match() {
        // The check lowercases the reply, so an uppercase rule would be dead
        // and would look like it was working.
        for rule in RULES {
            assert_eq!(rule.phrase, rule.phrase.to_lowercase(), "{}", rule.phrase);
            assert!(!rule.because.is_empty());
        }
    }

    #[test]
    fn the_phrases_added_on_the_seventh_are_actually_refused() {
        // Each of these passed the thirty-one-phrase list, and each is a verdict
        // this account may not deliver. Asserted one at a time rather than as a
        // loop over `RULES`, which would only prove the list contains what the
        // list contains.
        for said in [
            "classic honeypot",
            "you are the exit liquidity here",
            "the creator dumped on buyers",
            "this is a 100x",
            "looks bullish to me",
            "bearish, obviously",
            "don\u{2019}t buy this one",
            "dont buy it",
            "the launch looks clean",
        ] {
            assert!(!check(said).is_empty(), "{said:?} must be refused");
        }
    }

    #[test]
    fn the_new_phrases_do_not_refuse_ordinary_measured_sentences() {
        // The rule AGENTS.md sets: a check that fires on a change a reasonable
        // person would make is worse than no check, because it spends the
        // credibility of every other check. These are sentences the account
        // should be able to publish, and each sits next to one of the new
        // phrases.
        for said in [
            "nothing here was measured",
            "the creator has launched three and none filled a curve",
            "cleanly decoded, and the block says nothing else",
            "buyers paid the round trip twice",
        ] {
            assert!(
                check(said).is_empty(),
                "{said:?} must survive: {:?}",
                check(said)
            );
        }
    }

    // -----------------------------------------------------------------
    // check_target
    // -----------------------------------------------------------------

    #[test]
    fn a_bare_person_word_beside_an_accusation_is_refused() {
        for said in [
            "the dev is a scam",
            "the team rugged us",
            "ask the founder, he's a thief",
            "the creator committed fraud",
        ] {
            assert!(!check_target(said).is_empty(), "{said:?} must be refused");
        }
    }

    #[test]
    fn a_handle_beside_an_accusation_is_refused() {
        assert!(!check_target("@some_dev is a scammer").is_empty());
    }

    #[test]
    fn a_capitalised_multiword_run_beside_an_accusation_is_refused() {
        assert!(!check_target("Big Token Labs is a scam").is_empty());
    }

    #[test]
    fn an_0x_address_as_the_subject_of_an_accusation_is_refused_but_an_observed_verb_is_not() {
        // design 0020 §5's own worked example: the distinction is the verb,
        // not the address. Re-apply the bug by deleting `has_address_subject`
        // from `check_target`'s `has_person` computation: the first assertion
        // starts passing when it must fail.
        assert!(!check_target("0x1234abcd is a scammer").is_empty());
        assert!(check_target("0x1234abcd sold everything in block 3").is_empty());
        // The worked example's own spelling, with the address trailing an
        // ellipsis rather than more hex digits.
        assert!(!check_target("0x1234\u{2026} is a scammer").is_empty());
        assert!(check_target("0x1234\u{2026} sold everything in block 3").is_empty());
    }

    #[test]
    fn an_accusation_word_with_no_person_reference_is_not_a_target_violation() {
        // "this token looks rugged" is about the token, not a person -- the
        // level check, not the target check, is what has an opinion on it.
        assert!(check_target("this token looks rugged so far").is_empty());
        assert!(check_target("classic scam mechanics on this contract").is_empty());
    }

    #[test]
    fn a_person_reference_with_no_accusation_word_is_not_a_target_violation() {
        assert!(check_target("the creator has launched three tokens before").is_empty());
        assert!(check_target("@some_dev posted the contract").is_empty());
    }

    #[test]
    fn own_names_are_never_read_as_a_person_reference() {
        // `RealOrRug` is the account's own capitalised handle; without the
        // case-preserving mask it would read as a two-word capitalised run
        // sitting next to whatever accusation word follows.
        assert!(check_target("RealOrRug says: rugged").is_empty());
    }

    #[test]
    fn the_own_name_mask_reaches_a_name_late_in_a_long_reply() {
        // Re-apply the bug by stopping `mask_case_preserving`'s scan early --
        // any of the three parts of `i + needle.len() <= bytes.len()` will do
        // it. The name then stays unmasked, `RealOrRug Says` reads as a
        // two-word capitalised run, and this sentence's "scam" is refused
        // when it must not be. The existing own-name test cannot catch that:
        // its second word is lowercase, so there is no run either way.
        assert!(check_target("no evidence of a scam here, RealOrRug Says").is_empty());
    }

    #[test]
    fn an_at_sign_is_only_a_handle_when_a_name_follows_it() {
        // The character *after* the `@` decides it, so both halves of
        // `is_alphanumeric() || == '_'` have to hold: a name makes a handle,
        // a bare `@` in prose does not.
        assert!(!check_target("@alice ran a scam").is_empty());
        assert!(check_target("reply to @ if you think this is a scam").is_empty());
    }

    #[test]
    fn one_capitalised_word_is_not_a_named_company() {
        // Two capitalised words back to back is the shape. One alone is not,
        // or every sentence would begin with a person.
        assert!(check_target("Scam tokens exist on every chain").is_empty());
    }

    #[test]
    fn an_observed_verb_after_an_address_passes_even_beside_an_accusation_word() {
        // The worked-example test above has no accusation word in its allowed
        // half, so it cannot tell whether the verb check fires at all. This
        // one can: "rug" is in the same sentence, and it must still pass.
        assert!(
            check_target("0xdeadbeef sold everything in the last block before the rug").is_empty()
        );
    }

    #[test]
    fn a_bare_0x_with_no_hex_digits_is_not_an_address() {
        // `0x` alone is two characters of prose, not a subject -- the hex run
        // after it is what makes it an address.
        assert!(check_target("0x is a scam").is_empty());
    }

    #[test]
    fn an_address_running_to_the_end_of_the_text_is_not_read_past() {
        // The hex scan's bound is what stops it walking off the end of the
        // string, and getting that wrong panics rather than answering wrongly.
        assert!(check_target("nothing here looks like a scam at 0xdeadbeef").is_empty());
    }

    // -----------------------------------------------------------------
    // check_level
    // -----------------------------------------------------------------

    #[test]
    fn nothing_may_call_a_canttell_token_safe_clean_fine_or_legit() {
        // ADR 0027's own consequence, quoted in the design: a blind spot
        // presented as clean is worse than the shrug the old rule produced.
        for said in [
            "this one looks safe",
            "clean launch",
            "seems fine",
            "totally legit",
        ] {
            assert!(
                !check_level(said, Level::CantTell).is_empty(),
                "{said:?} must be refused at CantTell"
            );
        }
        // An ordinary CantTell-appropriate sentence survives.
        assert!(check_level("a required fact could not be read", Level::CantTell).is_empty());
    }

    #[test]
    fn rug_rugged_stole_and_stolen_sit_at_rugged_only() {
        for level in [
            Level::NothingUglyYet,
            Level::Sketchy,
            Level::RugMechanicsLive,
        ] {
            assert!(
                !check_level("this token got rugged", level).is_empty(),
                "{level:?} must not earn \"rugged\""
            );
            assert!(
                !check_level("the buyers were stolen from", level).is_empty(),
                "{level:?} must not earn \"stolen\""
            );
        }
        // Rugged is where the word is earned: no ceiling on it here.
        assert!(check_level("this token got rugged", Level::Rugged).is_empty());
    }

    #[test]
    fn word_occurs_matches_whole_words_only_so_rug_does_not_fire_on_rugged() {
        // Re-apply by swapping `word_occurs` for a plain `.contains`: this
        // sentence contains only "rugged", never bare "rug", and a substring
        // match would double it into two violations sharing one occurrence.
        let violations = check_level("this token got rugged", Level::Sketchy);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].phrase, "rugged");
    }

    // -----------------------------------------------------------------
    // The equivalence with the old ban, and the mutation §5 names.
    // -----------------------------------------------------------------

    #[test]
    fn a_person_directed_word_the_old_ban_caught_is_still_caught_by_the_new_pair() {
        // The old blanket ban refused this because it contains "scam"
        // anywhere. The new pair must still refuse it -- here, on the target
        // check alone, because it is aimed at "the dev" and needs no level
        // fact at all.
        let text = "the dev is a scam";
        assert!(!check(text).is_empty(), "the old ban must still catch this");
        assert!(!check_target(text).is_empty());
    }

    #[test]
    fn a_reassurance_word_the_old_ban_caught_at_canttell_is_still_caught_by_the_new_pair() {
        let text = "looks safe to me";
        assert!(!check(text).is_empty(), "the old ban must still catch this");
        assert!(!check_level(text, Level::CantTell).is_empty());
    }

    #[test]
    fn only_check_level_catches_a_token_rugged_call_below_its_earned_level() {
        // This is the case the packet names: the old blanket ban refused
        // "rugged" unconditionally, with no concept of level or target. The
        // new pair must still refuse it, and it must be `check_level` doing
        // the refusing -- `check_target` has no opinion, because nothing here
        // names a person.
        let text = "this token looks rugged so far, nothing confirmed";
        assert!(!check(text).is_empty(), "the old ban must still catch this");
        assert!(
            check_target(text).is_empty(),
            "no person-reference here -- check_target must stay silent"
        );
        assert!(!check_level(text, Level::Sketchy).is_empty());

        // The mutation: disabling `check_level` (returning an empty vec, as
        // if the function always passed) would let this previously-refused
        // case publish. Encoded directly, the way the packet asks: a case
        // that would pass if `check_level` returned `Vec::new()`
        // unconditionally.
        let disabled_check_level: Vec<Violation> = Vec::new();
        assert!(
            disabled_check_level.is_empty() && !check_level(text, Level::Sketchy).is_empty(),
            "a stub check_level would wrongly let {text:?} through"
        );
    }

    #[test]
    fn check_level_catches_a_bare_word_the_old_ban_never_had_an_opinion_on() {
        // The old RULES list only banned the phrases "is safe"/"looks
        // safe"/"totally safe"/"legit"/"trustworthy" -- never the bare word
        // "fine" or "clean" alone. This is the level check's additional
        // refusal ADR 0027 point 6 asks for: a claim the old word ban had no
        // concept of, because it had no concept of level.
        let text = "the launch is fine so far";
        assert!(
            check(text).is_empty(),
            "the old ban has no opinion on \"fine\""
        );
        assert!(!check_level(text, Level::CantTell).is_empty());
    }
}
