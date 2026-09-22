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

use crate::clause::Kind;
use crate::fidelity;
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
    // deterministic template passes this check caught it -- the template used
    // to end "Not financial advice", so the analyst's own safest possible
    // reply was being refused by its own rule. ADR 0005's unresolved
    // precondition 3 asked for that disclaimer; as of 2026-09-17 it lives in
    // the bio instead of every reply (see `bio.rs`'s `REALORRUG_BIO_LEAD`),
    // but the phrase stays off this list regardless -- a model-written reply
    // that happens to say "financial advice" is not asking to be refused for
    // it. The advice itself is caught by the phrases above.
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
    // "100x"/"10x" (and every other Nx multiple) used to live here as two
    // fixed literals. ADR 0033 §3: a hedged, reasoned upside/downside hint is
    // now allowed when the sheet carries a measured outcome rate for launches
    // shaped like this one, so a magnitude claim can no longer be an
    // unconditional ban -- it is [`check_hint`]'s job, which sees the sheet
    // this list does not.
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
// "real or rug" (spaced) is masked alongside "realorrug" because the reply
// template opens with "Real or Rug on <mint>:" and "rug" is an accusation
// word -- without this every published reply would refuse itself.
const OWN_NAMES: &[&str] = &["cabalhunter.org", "realorrug", "real or rug"];

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
    let mut violations: Vec<Violation> = RULES
        .iter()
        .filter(|r| lower.contains(r.phrase))
        .map(|r| Violation {
            phrase: r.phrase,
            because: r.because,
        })
        .collect();
    // These callers have no sheet, so they cannot authorise an outcome hint.
    violations.extend(hint_violations(reply, false));
    violations
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
pub type LevelRule = (&'static str, &'static str);

/// The words a reply at `level` may not contain, and why each is refused.
///
/// Public because the table has two readers and they must never disagree.
/// [`check_level`] reads it *after* generation to refuse a word; `voice.rs`
/// reads it *before*, to tell the model which words this token's verdict does
/// not license. Until 2026-09-17 only the first reader existed, so the model
/// was refused for breaking a rule nobody had shown it -- it was never told
/// the level at all -- and every such refusal cost a paid call and shipped
/// the template instead.
///
/// One function rather than a copy of the list in the prompt: a word added
/// here reaches the instruction and the check in the same edit, and a prompt
/// that quoted its own copy would drift silently, refusing replies for a word
/// it had stopped mentioning.
#[must_use]
pub fn words_refused_at(level: Level) -> &'static [LevelRule] {
    match level {
        Level::CantTell => CANTTELL_WORDS,
        Level::NothingUglyYet => NOTHINGUGLYYET_WORDS,
        Level::Sketchy => SKETCHY_WORDS,
        Level::RugMechanicsLive => RUGMECHANICSLIVE_WORDS,
        Level::Rugged => RUGGED_WORDS,
    }
}

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
    // The reassurance words. §5 folds these into `check_level`, "conditional
    // on level rather than absolute," but the fold missed this row: two or
    // more live-risk signals are firing right now, which earns none of them.
    // Without this row a reply computed as `RugMechanicsLive` could say
    // "looks safe" and nothing here refused it -- the mechanics being live
    // is exactly the case reassurance is most dangerous in.
    (
        "is safe",
        "reassurance -- live rug mechanics are not reassurance-earning",
    ),
    (
        "looks safe",
        "reassurance -- live rug mechanics are not reassurance-earning",
    ),
    (
        "totally safe",
        "reassurance -- live rug mechanics are not reassurance-earning",
    ),
    (
        "legit",
        "reassurance -- live rug mechanics are not reassurance-earning",
    ),
    (
        "trustworthy",
        "reassurance -- live rug mechanics are not reassurance-earning",
    ),
    (
        "looks clean",
        "reassurance -- live rug mechanics are not reassurance-earning",
    ),
    (
        "is clean",
        "reassurance -- live rug mechanics are not reassurance-earning",
    ),
];

/// `Rugged`'s ceiling. Not empty, and that is the fix this row exists for:
/// `Rugged` has earned every *accusing* word ("rug," "stole," and their
/// siblings sit at no ceiling here, on purpose -- see [`check_level`]'s own
/// doc comment) but it has earned none of the *reassuring* ones. Before this
/// row existed `check_level(_, Level::Rugged)` returned `&[]` for every word,
/// which meant a reply about a token the code had already judged rugged
/// could say "looks legit" and nothing in this file refused it -- the worst
/// sentence this account could publish, reachable because the level that
/// earns the most damning words was mistaken for the level that earns no
/// ceiling at all.
const RUGGED_WORDS: &[LevelRule] = &[
    (
        "is safe",
        "reassurance -- this token is Rugged, the level that earns \"rug\"/\"stole\", never \"safe\"",
    ),
    (
        "looks safe",
        "reassurance -- this token is Rugged, the level that earns \"rug\"/\"stole\", never \"safe\"",
    ),
    (
        "totally safe",
        "reassurance -- this token is Rugged, the level that earns \"rug\"/\"stole\", never \"safe\"",
    ),
    (
        "legit",
        "reassurance -- this token is Rugged, the level that earns \"rug\"/\"stole\", never \"legit\"",
    ),
    (
        "trustworthy",
        "reassurance -- this token is Rugged, the level that earns \"rug\"/\"stole\", never \"trustworthy\"",
    ),
    (
        "looks clean",
        "reassurance -- this token is Rugged, the level that earns \"rug\"/\"stole\", never \"clean\"",
    ),
    (
        "is clean",
        "reassurance -- this token is Rugged, the level that earns \"rug\"/\"stole\", never \"clean\"",
    ),
];

/// [`RULES`] phrases design 0020 §5 migrates to [`check_target`] (an
/// accusation aimed at a person) or [`check_level`] (a word above the
/// sheet's earned level) rather than an unconditional ban.
///
/// [`check_unconditional`] is *all of [`RULES`] except these*, rather than a
/// second, hand-written list that has to be kept in sync with `RULES` by
/// hand as entries are added or reworded there.
const MIGRATED_TO_TARGET_OR_LEVEL: &[&str] = &[
    // The person/project verdict words: §5's "Dropped" bullet names exactly
    // these seven. A person-directed use is caught by `check_target`
    // regardless of level; a token-directed use is caught by `check_level`
    // once it names a level that has not earned the word.
    "scam",
    "rug",
    "fraud",
    "stole",
    "stolen",
    "criminal",
    "thief",
    // The reassurance words: §5's "Kept" bullet says these "fold directly
    // into the level-check's `CantTell` row" -- conditional on level now,
    // not an unconditional ban, so they must not also be refused here.
    "is safe",
    "looks safe",
    "totally safe",
    "legit",
    "trustworthy",
    "looks clean",
    "is clean",
];

/// Refuses every [`RULES`] phrase design 0020 §5 keeps as an unconditional
/// ban -- advice, a price prediction, the `honeypot`/`exit liquidity`/
/// `dumped on`/`dumping on` phrases, and the cabal-identity words from
/// research 0012 -- regardless of the reply's target or the sheet's earned
/// level.
///
/// §5, "Kept, unchanged in purpose": *"The advice rules ... are kept
/// unconditionally; ADR 0027 says nothing about advice, and AGENTS.md's
/// advice rule is untouched by it,"* and the `honeypot` entry is *"kept as a
/// forbidden phrase until research 0044 ships an actual sell-blocking
/// check."* The cabal-identity words (research 0012: a destination is an
/// `(owner, mint)` token account, so a recipient count never resolves to a
/// person or a recurring group) are not named in §5's kept/dropped list at
/// all, which places them with everything else `RULES` still bans outright
/// rather than with the seven words §5 explicitly moves elsewhere.
///
/// This is [`check`]'s own scan (same masking, same lowercasing, same
/// substring bluntness -- §5 does not ask for word-boundary precision on
/// these, only `check_target`/`check_level` need that) restricted to the
/// phrases [`MIGRATED_TO_TARGET_OR_LEVEL`] does not name, so a caller using
/// `check_target` + `check_level` + this function refuses exactly what
/// `check` refused, split three ways rather than narrowed.
#[must_use]
pub fn check_unconditional(reply: &str) -> Vec<Violation> {
    let lower = masked(reply);
    RULES
        .iter()
        .filter(|r| !MIGRATED_TO_TARGET_OR_LEVEL.contains(&r.phrase))
        .filter(|r| lower.contains(r.phrase))
        .map(|r| Violation {
            phrase: r.phrase,
            because: r.because,
        })
        .collect()
}

/// ADR 0033: a future-move hint needs a measured outcome rate, a hedge and
/// reasoning in the same sentence. Production sheets do not yet carry the
/// rate; only the later outcome-measurement pass may populate that kind.
/// Advice and unconditional predictions remain forbidden even with a rate.
#[must_use]
pub fn check_hint(reply: &str, sheet: &crate::sheet::FactSheet) -> Vec<Violation> {
    let has_rate = sheet.facts.iter().any(|fact| {
        fact.kind == Kind::OutcomeRate
            && !fact.rendered.trim().is_empty()
            && !fact.values.is_empty()
            && fact.values.iter().all(|value| value.is_finite())
    });
    hint_violations(reply, has_rate)
}

fn hint_violations(reply: &str, has_rate: bool) -> Vec<Violation> {
    let lower = masked(reply).replace('\u{2019}', "'");
    let mut violations = Vec::new();
    for sentence in sentences(&lower) {
        let words: Vec<&str> = sentence
            .split(|c: char| !c.is_alphanumeric() && c != '\'')
            .filter(|word| !word.is_empty())
            .collect();
        // Imperatives are advice; measured selling/buying and "holders can't
        // sell" must remain sayable. Also catch advice after a conjunction.
        let advice = words.iter().enumerate().any(|(at, word)| {
            ["buy", "sell", "hold"].contains(word)
                && (at == 0
                    || ["and", "then", "please", "should", "must", "just", "to"]
                        .contains(&words[at - 1])
                    || words
                        .get(at + 1)
                        .is_some_and(|next| ["now", "this", "it", "your", "until"].contains(next)))
        });
        if advice {
            violations.push(Violation {
                phrase: "buy/sell/hold advice",
                because: "advice, not commentary",
            });
        }
        // Match any numeric multiple, including "10x'd" and "100X", not
        // only the two magnitudes the old phrase list happened to name.
        let multiple = words.iter().any(|word| {
            word.split_once('x').is_some_and(|(number, suffix)| {
                !number.is_empty()
                    && number.bytes().all(|byte| byte.is_ascii_digit())
                    && (suffix.is_empty() || suffix == "'d")
            })
        });
        let movement = words.iter().any(|word| {
            [
                "pump", "dump", "moon", "moonshot", "rise", "fall", "rally", "crash", "recover",
                "soar", "double", "triple", "upside", "downside",
            ]
            .contains(word)
        });
        if !multiple && !movement {
            continue;
        }
        let hedged = sentence.contains("wouldn't be surprised if")
            || word_occurs(sentence, "could")
            || sentence.contains("if the trend holds");
        let reasoned = ["because", "given", "since"]
            .iter()
            .any(|word| word_occurs(sentence, word));
        let certain = word_occurs(sentence, "will")
            || sentence.contains("'ll ")
            || sentence.contains("going to")
            || word_occurs(sentence, "moon")
            || word_occurs(sentence, "moonshot");
        if !has_rate || !hedged || !reasoned || certain {
            violations.push(Violation {
                phrase: "outcome hint",
                because: "a hint needs a measured outcome rate, a hedge and reasoning; never certainty",
            });
        }
    }
    violations
}

/// Refuses a word above the ceiling the sheet's computed level earned,
/// regardless of who or what it is aimed at -- design 0020 §5's level check.
///
/// `Rugged` has no ceiling on the *accusing* words: it is the level "rug,"
/// "rugged" and their siblings are earned *at*, so none of them is refused
/// there. It does have a ceiling on the *reassuring* ones ([`RUGGED_WORDS`]):
/// nothing about earning "rugged" also earns "looks legit." Runs on the
/// own-name-masked, lowercased text, same as `check`, so `realorrug` is never
/// caught by "rug."
///
/// Word, not substring: unlike `check`'s deliberate substring bluntness,
/// "rug" must not fire on "rugged" (they are different, separately-earned
/// rows of the table above), so this matches whole words only
/// ([`word_occurs`]).
#[must_use]
pub fn check_level(text: &str, level: Level) -> Vec<Violation> {
    let lower = masked(text);
    words_refused_at(level)
        .iter()
        .filter(|(word, _)| word_occurs(&lower, word))
        .map(|(word, because)| Violation {
            phrase: word,
            because,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Design 0020 §4: the two required lines.
//
// The opposite shape from everything above. `check`, `check_target`,
// `check_level` and `check_unconditional` all refuse a phrase the reply
// *has*; this refuses an *absence* -- a `CantTell` reply that never says
// what could not be read, or a `NothingUglyYet` reply that never states the
// age. Kept as its own function rather than folded into `check_level`
// because the two shapes take different questions ("is this word present"
// vs. "is this content present") and different inputs (`check_level` takes
// only the level; this needs the sheet itself) -- a check that answered both
// would be a check nobody could read.
// ---------------------------------------------------------------------------

/// Content design 0020 §4 requires a reply to carry, and it did not.
///
/// Reuses [`Violation`] rather than a new type: both are "the reply may not
/// publish, and here is why," and a caller gating on `Fellback::Forbidden`
/// (`voice.rs`) does not need a second variant to also handle this case.
#[must_use]
pub fn check_required(text: &str, level: Level, sheet: &crate::sheet::FactSheet) -> Vec<Violation> {
    match level {
        Level::CantTell => check_required_canttell(text, sheet),
        Level::NothingUglyYet => check_required_age(text, sheet),
        // §4 names a requirement at exactly these two levels. Nothing else on
        // the ladder has one -- adding a third would be a rule design 0020
        // §4 does not state, which the packet's scope boundary refuses.
        Level::Sketchy | Level::RugMechanicsLive | Level::Rugged => Vec::new(),
    }
}

/// The suffix `sheet.rs::phrase_for` writes on every reason it can name
/// ("the launch block **could not be read**"). Stripped so the comparison is
/// against the topic itself -- "the launch block" -- and a reply naming the
/// topic in different words around it ("the launch-block read came back
/// empty") still counts as naming the same thing `sheet.unknown` does.
const UNKNOWN_SUFFIX: &str = " could not be read";

/// The topic named by one of `sheet.unknown`'s phrases.
fn topic(phrase: &str) -> &str {
    phrase.strip_suffix(UNKNOWN_SUFFIX).unwrap_or(phrase)
}

/// The words a reply must mention to have named `phrase`'s topic.
///
/// Every phrase `sheet::phrase_for` builds opens with "the" -- "the holders",
/// "the bonding curve", "the creator's history" -- and until 2026-09-17 the
/// check below asked whether the reply *contained that whole string*. So a
/// reply had to write "the holders", and "holders can't be read" failed a
/// rule it plainly satisfies. Measured on the box that day: the one Robinhood
/// reply the bot had sent fell back for exactly this violation, on exactly
/// this sheet entry.
///
/// Matching word by word, with the article dropped and a possessive reduced
/// to its noun, asks what the rule actually means: did the reply name the
/// thing that could not be read. It is still a real gate -- every content
/// word must appear -- and it no longer turns on grammar the model was never
/// told to copy.
///
/// Until 2026-09-17 this required an exact substring for each content word,
/// which meant "holder" did not satisfy "holders" and a reply calling
/// something "unreadable" did not satisfy "could not be read". Measured on
/// the box the same day: a real model draft naming the holders in different
/// words fell back on this check even though it plainly named the topic.
///
/// A word now satisfies the topic word if it shares a simple stem (plural
/// `-s`/`-es`, so "holder" and "holders" match each other) or appears in
/// [`synonyms`], a short hand-written list for word forms stemming does not
/// reach ("unread", "readable" for "read"). It still requires every content
/// word's topic to be named, and a phrase with no content words still cannot
/// pass -- a bare "can't tell" that names nothing still fails.
fn topic_words(phrase: &str) -> Vec<String> {
    topic(phrase)
        .split_whitespace()
        .filter(|w| !matches!(w.to_lowercase().as_str(), "the" | "a" | "an" | "of"))
        .map(|w| {
            let w = w.to_lowercase();
            w.strip_suffix("'s").map_or(w.clone(), str::to_owned)
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// A simple plural/singular stem: strips a trailing `-es` or `-s` when the
/// remainder is still long enough to be a real word on its own ("holders" ->
/// "holder", but not "as" -> "a"). Good enough for the noun phrases
/// `sheet::phrase_for` builds; it is not a general English stemmer.
fn stem(word: &str) -> &str {
    word.strip_suffix("es")
        .or_else(|| word.strip_suffix('s'))
        .filter(|s| s.len() >= 3)
        .unwrap_or(word)
}

/// Word forms that count as naming `word`'s topic but do not share its stem
/// -- a prefix like "un-" or a suffix like "-able" changes the stem outright.
/// Hand-written and short on purpose: the phrases `sheet::phrase_for` builds
/// name four topics today, so a complete per-word list costs less and risks
/// less than a general synonym lookup would.
fn synonyms(word: &str) -> &'static [&'static str] {
    match word {
        "read" => &["unread", "readable", "unreadable", "reading"],
        _ => &[],
    }
}

/// Whether `text` (already lowercased) names `topic_word` by its stem or by
/// one of its listed synonyms.
fn names_topic_word(text: &str, topic_word: &str) -> bool {
    let topic_stem = stem(topic_word);
    let by_stem = text
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| !w.is_empty() && stem(w) == topic_stem);
    let by_synonym = synonyms(topic_word).iter().any(|syn| text.contains(syn));
    by_stem || by_synonym
}

fn check_required_canttell(text: &str, sheet: &crate::sheet::FactSheet) -> Vec<Violation> {
    if sheet.unknown.is_empty() {
        // Dead in production: `verdict::level` returns `CantTell` exactly
        // when `sheet.unknown` is non-empty (`verdict.rs`'s own `level`), so
        // a real `CantTell` sheet always has something to name. Kept as a
        // real branch rather than `unreachable!` because this function is
        // also callable directly against a hand-built sheet (this file's own
        // tests, or any future caller), and a panic there would be a worse
        // failure than "nothing was required."
        return Vec::new();
    }
    let lower = text.to_lowercase();
    let named = sheet.unknown.iter().any(|miss| {
        let words = topic_words(miss);
        // An empty word list would make every reply pass, which is the one
        // answer this check may never give by accident. No phrase
        // `sheet::phrase_for` builds reduces to nothing, so this is a guard
        // against a future phrase, not a case seen today.
        !words.is_empty() && words.iter().all(|w| names_topic_word(&lower, w))
    });
    if named {
        Vec::new()
    } else {
        vec![Violation {
            phrase: "canttell reply names nothing that could not be read",
            because: "design 0020 §4: a CantTell reply must say what could not be read, not a \
                      bare \"can't tell\"",
        }]
    }
}

/// Words that mark a numeral in the reply as a statement of *when* the sheet
/// was read -- the read point, never the age (they are different facts, see
/// [`check_required_age`]).
const READ_POINT_WORDS: &[&str] = &["slot", "block", "read at"];

/// Words that mark a numeral in the reply as a statement of the token's age,
/// rather than of anything else the sheet authorised.
const AGE_WORDS: &[&str] = &["ago", "old", "hour"];

/// Whether `text` states one of `values` as a literal `fidelity::check` would
/// not otherwise flag as fabricated.
///
/// Reuses `fidelity::check`'s own notion of "the sheet authorised this
/// number" rather than a second one: a literal counts as stated when checking
/// the text against *only* this narrower set does not report it as
/// fabricated -- i.e. it is left out of the fabricated list, which is the
/// only public API this module needs to ask.
fn states_one_of(text: &str, values: &[f64]) -> bool {
    // Subject-free, deliberately. The question here is whether a required
    // figure is *present*, and the narrow set passed in is a single required
    // fact: there is no second subject for it to be confused with, so the
    // subject rule has nothing to decide and would only subtract from the
    // count for the wrong reason.
    let values: Vec<fidelity::Authorised> = values
        .iter()
        .copied()
        .map(fidelity::Authorised::anywhere)
        .collect();
    fidelity::literals(text).len() > fidelity::check(text, &values).len()
}

/// Content design 0020 §4 requires a `NothingUglyYet` reply to carry: the age,
/// which is not the read point.
///
/// "Read at slot 444007820" says when the camera clicked; it does not say the
/// token is six hours old, and design 0020 §4 asks for the second: the
/// reassurance is only bounded if the reader learns the thing is new. Three
/// shapes, matching [`crate::sheet::FactSheet::build`]'s own three cases:
///
/// - **A real age exists** ([`crate::clause::Kind::Age`] is on the sheet,
///   Solana today): the reply must state *that* fact's value, not just the
///   read point -- a reply citing only `sheet.read_at`'s number is exactly
///   the defect this function exists to catch.
/// - **No age, but a read point** (a sheet whose launch block or its
///   timestamp could not be read): the
///   reply must say how old the token is could not be read, and must still
///   state the read point. Saying "nothing ugly yet" about a token whose age
///   is unknown, without saying so, is reassurance with its limit removed --
///   rule 8, unknown is not safe.
/// - **Neither exists**: nothing chronological is on this sheet at all --
///   rule 7, deny by default when the input the requirement needs is
///   missing, same as before this task.
fn check_required_age(text: &str, sheet: &crate::sheet::FactSheet) -> Vec<Violation> {
    let refused = vec![Violation {
        phrase: "nothinguglyyet reply states no age",
        because: "design 0020 §4: a NothingUglyYet reply must state the age, not just \"clean \
                  so far\"",
    }];
    let Some(read_at) = sheet.read_at else {
        return refused;
    };

    if let Some(age) = sheet
        .facts
        .iter()
        .find(|f| f.kind == crate::clause::Kind::Age)
    {
        // A real age exists. The read point alone must not pass: its
        // number is deliberately excluded from the values checked here.
        let lower = text.to_lowercase();
        let mentions_age_word = AGE_WORDS.iter().any(|w| lower.contains(w));
        if mentions_age_word && states_one_of(text, &age.values) {
            Vec::new()
        } else {
            refused
        }
    } else {
        // Ageless. The reply must say the age is unknown, and must still
        // cite the read point -- both, not either.
        let lower = text.to_lowercase();
        let says_age_unknown = (lower.contains("age") || lower.contains("old"))
            && (lower.contains("could not be read") || lower.contains("unknown"));
        let mentions_read_point_word = READ_POINT_WORDS.iter().any(|w| lower.contains(w));
        let read_point_value = match read_at {
            realorrug_types::ReadAt::Solana(slot) => slot.get(),
            realorrug_types::ReadAt::Robinhood(block) => block,
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "a slot or a block number is well inside f64's exact integer range -- \
                      the same cast sheet.rs::authorised already makes for the same value"
        )]
        let read_point_value = read_point_value as f64;
        if says_age_unknown && mentions_read_point_word && states_one_of(text, &[read_point_value])
        {
            Vec::new()
        } else {
            refused
        }
    }
}

// ---------------------------------------------------------------------------
// Design 0024 §2.1: lane 2's own checks.
//
// Lane 2 (`crates/realorrug-analyst/src/lane2.rs`) has no `FactSheet` and no
// computed `Level` -- there is nothing here a number or a claim could be
// checked against, which is the guardrail design point itself: the only
// safe number in a lane with no facts is no number (AGENTS.md rule 2, rule
// 8). These three checks are new; `check_unconditional` above is reused as
// it stands, and `check_any_person_reference` below reuses `check_target`'s
// own person-reference detection with one widened trigger rather than
// inventing a second one.
// ---------------------------------------------------------------------------

/// Refuses any ASCII digit anywhere in the reply -- design 0024 §2.1.
///
/// Blunter than [`check_required`]'s "does this number match the sheet":
/// lane 2 has no sheet a digit could be checked against, so every digit is
/// refused, not only a wrong one.
#[must_use]
pub fn check_numerals(text: &str) -> Vec<Violation> {
    if text.chars().any(|c| c.is_ascii_digit()) {
        vec![Violation {
            phrase: "0-9",
            because: "lane 2 has no fact sheet a digit could ever be checked against",
        }]
    } else {
        Vec::new()
    }
}

/// The words a lane-2 reply may not pair with a claim that one has been
/// identified, per design 0024 §2.1.
///
/// Deliberately does not ban the bare words: lane 2's own fixed nudge line
/// (appended by its caller, never run through this check) says "drop a
/// contract address" as a request, and the distinction is the same one
/// [`check_target`] already draws between a fact and an accusation -- it is
/// the *claim*, not the word, that is refused.
const IDENTIFICATION_WORDS: &[&str] = &["token", "coin", "contract", "mint", "address"];

/// Does this (already lowercase) sentence contain a `$`-cashtag -- `$` then
/// an alphanumeric character?
fn has_cashtag(sentence: &str) -> bool {
    sentence.char_indices().any(|(i, c)| {
        c == '$'
            && sentence[i + c.len_utf8()..]
                .chars()
                .next()
                .is_some_and(char::is_alphanumeric)
    })
}

/// Refuses a cashtag, or one of [`IDENTIFICATION_WORDS`] used alongside a
/// copula in the same sentence ("this **token is** great," "it**'s** a
/// **legit coin**") -- design 0024 §2.1's `check_no_identification`.
///
/// A positive-claim shape, not a word ban: "drop a contract address" pairs
/// no copula with the word, and is never refused by this. The false-refusal
/// cost of a copula appearing near the word for an unrelated reason is
/// accepted, the same conservative direction §5 (rule 7) already takes for
/// `check_target`.
#[must_use]
pub fn check_no_identification(text: &str) -> Vec<Violation> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    for sentence in sentences(&lower) {
        if has_cashtag(sentence) {
            out.push(Violation {
                phrase: "$",
                because: "a cashtag treats a symbol as identified, and lane 2 has no sheet to \
                          check one against",
            });
        }
        let names_one = IDENTIFICATION_WORDS
            .iter()
            .any(|w| word_occurs(sentence, w));
        let has_copula =
            word_occurs(sentence, "is") || word_occurs(sentence, "are") || sentence.contains("'s ");
        if names_one && has_copula {
            out.push(Violation {
                phrase: "token/coin/contract/mint/address claim",
                because: "a positive identification claim lane 2 has no fact sheet to back",
            });
        }
    }
    out
}

/// Design 0024 §2.1's widened `check_target` trigger for lane 2: a
/// violation needs **only** a person-reference, no [`ACCUSATION_WORDS`]
/// co-occurrence -- lane 2 has no legitimate register in which naming a
/// real person, complimentary or not, is the bot's job.
///
/// Shares every person-reference shape `check_target` uses
/// ([`has_handle`], [`PERSON_WORDS`], [`has_capitalized_run`],
/// [`has_address_subject`]) and the same own-name mask, so `realorrug` is
/// never itself read as a person-reference. This is a superset of what
/// `check_target` alone would catch for the same text; lane 2's caller uses
/// this function alone rather than also calling `check_target`, per this
/// module's own doc note that the trigger is widened, not duplicated.
#[must_use]
pub fn check_any_person_reference(text: &str) -> Vec<Violation> {
    let masked_text = mask_case_preserving(text);
    let mut violations = Vec::new();
    for sentence in sentences(&masked_text) {
        let lower = sentence.to_lowercase();
        let has_person = has_handle(sentence)
            || PERSON_WORDS.iter().any(|w| word_occurs(&lower, w))
            || has_capitalized_run(sentence)
            || has_address_subject(&lower);
        if has_person {
            violations.push(Violation {
                phrase: "person-reference",
                because: "lane 2 has no legitimate reason to name a real person at all",
            });
        }
    }
    violations
}

/// A fixed keyword scan of a **mention's** text (not a reply) for a
/// tragedy, death, violence, war, or a political figure or party -- design
/// 0024 §2.1's `check_sensitive_topic`. A hit routes straight to lane 2's
/// fixed fallback line with no model call at all.
///
/// A keyword scan, not a classifier, for the reason design 0022 §2 gives
/// its own fixed phrase list: the set of subjects that must never get a
/// joke is closed enough to enumerate, and a false refusal costs one plain
/// nudge instead of one joke -- the conservative direction rule 7 already
/// prefers.
const SENSITIVE_WORDS: &[&str] = &[
    "died",
    "death",
    "dead",
    "killed",
    "kill",
    "suicide",
    "shooting",
    "shooter",
    "massacre",
    "terrorist",
    "terrorism",
    "bombing",
    "bomb",
    "genocide",
    "war",
    "attack",
    "murder",
    "rape",
    "assault",
    "tragedy",
    "disaster",
    "earthquake",
    "president",
    "election",
    "senator",
    "congress",
    "republican",
    "democrat",
    "politician",
    "prime minister",
];

/// Scans a mention's text for [`SENSITIVE_WORDS`]; a hit means lane 2 must
/// never attempt a joke about it (design 0024 §2.1).
#[must_use]
pub fn check_sensitive_topic(mention: &str) -> Vec<Violation> {
    let lower = mention.to_lowercase();
    SENSITIVE_WORDS
        .iter()
        .filter(|w| word_occurs(&lower, w))
        .map(|w| Violation {
            phrase: w,
            because: "a tragedy or political subject gets the fixed nudge, never a joke",
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
                     Real or Rug has no record of this creator.";
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
    fn reassurance_is_refused_at_rugmechanicslive_even_though_it_is_not_earned() {
        // The gap this task closes: two live-risk signals are firing right
        // now, which earns none of the reassurance vocabulary. Delete the
        // reassurance rows from `RUGMECHANICSLIVE_WORDS` and this fails.
        for said in ["this one looks safe", "totally legit", "looks clean so far"] {
            assert!(
                !check_level(said, Level::RugMechanicsLive).is_empty(),
                "{said:?} must be refused at RugMechanicsLive"
            );
        }
    }

    #[test]
    fn reassurance_is_refused_at_rugged_but_rug_and_stole_are_still_earned() {
        // The worst sentence this account could publish, until `RUGGED_WORDS`
        // existed: a token already judged Rugged, called "legit" and nothing
        // refusing it. Delete `RUGGED_WORDS` (swap it back for `&[]`) and the
        // first loop's assertions fail.
        for said in ["looks safe to me", "totally legit", "is clean now"] {
            assert!(
                !check_level(said, Level::Rugged).is_empty(),
                "{said:?} must be refused at Rugged"
            );
        }
        // Rugged is still where "rug"/"stole" are earned -- this row must not
        // turn into a second accusing-word ceiling by accident.
        assert!(check_level("this token got rugged", Level::Rugged).is_empty());
        assert!(check_level("the buyers were stolen from", Level::Rugged).is_empty());
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

    // -----------------------------------------------------------------
    // check_unconditional
    // -----------------------------------------------------------------

    #[test]
    fn advice_and_price_prediction_are_refused_at_every_level() {
        // Task 9-15-0025: `voice.rs`'s live gate used to be `check_target` +
        // `check_level` alone, which has no opinion on advice or a price
        // prediction at all -- neither function's vocabulary includes them.
        // Re-apply the gap by deleting this function's call in `voice.rs`
        // (or gutting `MIGRATED_TO_TARGET_OR_LEVEL` to exclude nothing): a
        // reply saying "should buy" would then publish. "100x" moved to
        // `check_hint`, which has its own coverage below.
        for said in ["will pump", "you should buy this one"] {
            assert!(
                !check_unconditional(said).is_empty(),
                "{said:?} must be refused"
            );
        }
    }

    #[test]
    fn honeypot_is_still_refused_unconditionally() {
        assert!(!check_unconditional("classic honeypot mechanics").is_empty());
    }

    #[test]
    fn exit_liquidity_and_dumped_on_are_still_refused_unconditionally() {
        assert!(!check_unconditional("you are the exit liquidity here").is_empty());
        assert!(!check_unconditional("the creator dumped on buyers").is_empty());
    }

    #[test]
    fn a_cabal_identity_the_measurement_cannot_see_is_still_refused() {
        // Research 0012: a destination is an (owner, mint) token account, so
        // "people bought" or "cabal" claims an identity the recipient count
        // does not carry.
        for said in [
            "six people bought it in the launch block",
            "classic cabal behaviour",
            "six wallets bought it",
        ] {
            assert!(
                !check_unconditional(said).is_empty(),
                "{said:?} must be refused"
            );
        }
    }

    #[test]
    fn check_unconditional_does_not_refuse_the_words_target_and_level_now_own() {
        // The whole point of splitting `check` into three functions: a word
        // `check_target`/`check_level` now judges by target or level must
        // not also be an unconditional ban here, or a legitimate `Rugged`
        // verdict ("this one got rugged") would be refused regardless of the
        // level it earned.
        for said in ["this one got rugged", "looks safe so far", "totally clean"] {
            assert!(
                check_unconditional(said).is_empty(),
                "{said:?} must survive check_unconditional: {:?}",
                check_unconditional(said)
            );
        }
    }

    #[test]
    fn check_unconditional_lets_an_ordinary_measured_reply_through() {
        let reply = "Eleven recipients in the launch block. 0.5% of launches that never \
                     graduated look like that. The round trip at $50 is about 4.6%.";
        assert!(
            check_unconditional(reply).is_empty(),
            "{:?}",
            check_unconditional(reply)
        );
    }

    #[test]
    fn every_migrated_phrase_is_actually_in_rules() {
        // If a word in `MIGRATED_TO_TARGET_OR_LEVEL` were ever misspelled or
        // renamed relative to `RULES`, the filter would silently do nothing
        // for it and the phrase would stay banned unconditionally here --
        // not wrong, but a sign the list has drifted from what it claims to
        // name. This pins the two lists together.
        for phrase in MIGRATED_TO_TARGET_OR_LEVEL {
            assert!(
                RULES.iter().any(|r| r.phrase == *phrase),
                "{phrase:?} is not a RULES phrase"
            );
        }
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

    // -----------------------------------------------------------------
    // check_required
    // -----------------------------------------------------------------

    use crate::sheet::{Fact, FactSheet};

    /// A sheet naming one unread fact and read at a fixed Robinhood block, so
    /// `check_required_canttell` (against `unknown`) has something to check.
    ///
    /// **Ageless, deliberately.** A Robinhood sheet whose launch block could
    /// not be read carries no [`crate::clause::Kind::Age`] fact -- this
    /// fixture is that shape, and stands in for it in every test that is not
    /// itself about the age rule.
    fn required_sheet(unknown: Vec<String>) -> FactSheet {
        FactSheet {
            mint: "MintOne".to_owned(),
            read_at: Some(realorrug_types::ReadAt::Robinhood(100)),
            facts: vec![Fact::exact(
                crate::clause::Kind::LaunchRecipients,
                "distinct token accounts receiving the token in its own launch block",
                11.0,
                "11",
            )],
            untrusted: Vec::new(),
            unknown,
            signals: Vec::new(),
            twins: Vec::new(),
            skipped: Vec::new(),
        }
    }

    /// A Solana sheet read at a fixed slot, carrying a real age fact -- the
    /// other tier `check_required_age` has to tell apart from the read point.
    fn required_sheet_with_age(unknown: Vec<String>) -> FactSheet {
        let mut sheet = FactSheet {
            mint: "MintOne".to_owned(),
            read_at: Some(realorrug_types::ReadAt::Solana(realorrug_types::Slot(
                444_007_820,
            ))),
            facts: vec![Fact::exact(
                crate::clause::Kind::LaunchRecipients,
                "distinct token accounts receiving the token in its own launch block",
                11.0,
                "11",
            )],
            untrusted: Vec::new(),
            unknown,
            signals: Vec::new(),
            twins: Vec::new(),
            skipped: Vec::new(),
        };
        sheet.facts.push(Fact {
            about: crate::sheet::About::Measurement,
            kind: crate::clause::Kind::Age,
            label: "how long ago this token's launch block was, on the chain's own clock"
                .to_owned(),
            rendered: "about 7.1 hours old at the read (63954 slots after its launch block)"
                .to_owned(),
            values: vec![63954.0, 7.1],
            clauses: Vec::new(),
        });
        sheet
    }

    #[test]
    fn a_canttell_reply_naming_nothing_is_refused() {
        // The packet's own wording: "the holder list," "the reserve read" --
        // not a bare "can't tell." Re-apply by deleting the call inside
        // `check_required_canttell` that scans `sheet.unknown`: this starts
        // passing when it must fail.
        let sheet = required_sheet(vec!["the launch block could not be read".to_owned()]);
        assert!(
            !check_required(
                "Can't tell on this one, nothing to go on.",
                Level::CantTell,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn a_canttell_reply_naming_a_thing_on_the_unknown_list_passes() {
        let sheet = required_sheet(vec!["the launch block could not be read".to_owned()]);
        assert!(
            check_required(
                "The launch block couldn't be pulled, so this one's a shrug.",
                Level::CantTell,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn a_plural_suffix_is_stripped_only_when_three_bytes_remain() {
        // Pin the length boundary: "box" has three bytes, "ax" only two.
        assert_eq!(stem("boxes"), "box");
        assert_eq!(stem("axes"), "axes");
    }

    #[test]
    fn unread_names_the_read_topic_without_sharing_its_stem() {
        // Pin the synonym-only boundary: "unread" cannot match "read" by stem.
        assert!(names_topic_word("unread", "read"));
    }

    /// A sheet carrying a measured `Kind::OutcomeRate` fact -- the shape
    /// `check_hint` requires before it authorises a hedged hint (ADR 0033
    /// §3). `required_sheet` is the ageless fixture every other test in this
    /// file reuses; this adds the one fact that is new here.
    fn sheet_with_outcome_rate() -> FactSheet {
        let mut sheet = required_sheet(Vec::new());
        sheet.facts.push(Fact::exact(
            crate::clause::Kind::OutcomeRate,
            "of 40 launches shaped like this one, 6 hit 10x within 24h",
            15.0,
            "15%",
        ));
        sheet
    }

    #[test]
    fn price_and_market_cap_words_pass() {
        // ADR 0033 rule 1: price, market cap and liquidity may now be stated.
        // None of these sentences carries advice, a bare prediction or an
        // outcome hint, so `check` must find nothing wrong with any of them.
        assert!(check("The price is 0.00042 SOL, read at slot 444007820.").is_empty());
        assert!(check("Market cap: 69000 USD, read at slot 444007820.").is_empty());
        assert!(check(
            "The quote asset held in the bonding curve now, read at slot 444007820, is 6.1861 SOL."
        )
        .is_empty());
    }

    #[test]
    fn bare_predictions_and_advice_always_fail_even_with_a_rate() {
        // ADR 0033 rule 3: a hedge and a rate unlock a hint, never certainty
        // and never an instruction. `check` (no sheet, phrase list included)
        // always denies every one of these.
        for reply in [
            "This will pump tonight.",
            "It's going to 10x by tomorrow.",
            "My price target is $1.",
            "Buy now before it moons.",
            "You should sell this coin.",
            "Just hold this and you'll be fine.",
            "This is going to the moon.",
        ] {
            assert!(!check(reply).is_empty(), "{reply}");
        }

        // The subset that names a magnitude or a movement word also fails
        // `check_hint` on its own -- certainty ("will", "going to", "moon")
        // or an un-hedged claim -- even once a measured rate exists.
        let with_rate = sheet_with_outcome_rate();
        for reply in [
            "This will pump tonight.",
            "It's going to 10x by tomorrow.",
            "This is going to the moon.",
        ] {
            assert!(!check_hint(reply, &with_rate).is_empty(), "{reply}");
        }
    }

    #[test]
    fn a_hedged_hint_is_refused_without_the_outcome_rate_fact_and_allowed_with_it() {
        // The hook ADR 0033 §3 asks for: the sheet holds no `OutcomeRate`
        // fact in production yet, so a hedged, reasoned, non-certain hint
        // must still be refused -- and once the fact exists (a later
        // creator-index pass), the identical sentence is allowed.
        let hint = "I wouldn't be surprised if this 10x'd tonight, because launches like this \
                    one graduate fast.";

        let without_rate = required_sheet(Vec::new());
        assert!(
            !check_hint(hint, &without_rate).is_empty(),
            "a hedged hint passed with no outcome-rate fact on the sheet"
        );

        let with_rate = sheet_with_outcome_rate();
        assert!(
            check_hint(hint, &with_rate).is_empty(),
            "a hedged, reasoned, non-certain hint with a measured rate was still refused"
        );
    }

    // -----------------------------------------------------------------
    // `hint_violations` internals `cargo mutants` found untested: the
    // advice rule's conjunction/index arithmetic, the "Nx" suffix check,
    // and the certainty/hedge disjunction chain. Each test below is
    // shaped so exactly one survivor's mutation flips its answer -- the
    // fastest re-application is calling `hint_violations` with the
    // mutated operator swapped in by hand, which is what each comment
    // below does.
    // -----------------------------------------------------------------

    #[test]
    fn advice_fires_on_the_preceding_word_alone_when_the_next_word_does_not_qualify() {
        // "just" (before "buy") is on the preceding-word list; "toast"
        // (after "buy") is not on the following-word list. Only the
        // preceding-word branch can be true here, so this sentence pins
        // both `words[at - 1]` (mutated to `at + 1` or `at / 1`, either of
        // which reads the wrong or a non-offsetting index and misses) and
        // the `||` joining the two branches (mutated to `&&`, which needs
        // both true and also misses).
        assert!(!hint_violations("just buy toast.", false).is_empty());
    }

    #[test]
    fn advice_fires_on_the_following_word_alone_when_the_preceding_word_does_not_qualify() {
        // The mirror case: "consider" (before "sell") is not on the
        // preceding-word list, "now" (after "sell") is on the
        // following-word list. Pins `words[at + 1]` against `at - 1` or
        // `at * 1`, either of which misses.
        assert!(!hint_violations("consider sell now.", false).is_empty());
    }

    #[test]
    fn a_word_that_merely_ends_in_x_is_not_an_nx_multiple() {
        // "complex" splits on 'x' into ("comple", ""): the digits check
        // fails, so this must not count as an "Nx" multiple, and with
        // no movement word either, the sentence must clear `hint_violations`
        // outright regardless of the outcome-rate flag. Pins the `&&`
        // between the digits check and the suffix check -- mutated to
        // `||`, the empty suffix alone would wrongly make a non-numeric
        // word "complex" read as a multiple.
        assert!(hint_violations("the trend looks complex.", false).is_empty());
    }

    #[test]
    fn the_trend_holds_phrase_hedges_on_its_own() {
        // Hedged only through "if the trend holds" -- not "wouldn't be
        // surprised if", not "could" -- reasoned, rated and never certain,
        // so this hint is authorised outright. Pins the `||` joining this
        // phrase into `hedged`: mutated to `&&`, `hedged` requires all
        // three hedge phrases at once and is never true, so the hint is
        // wrongly refused.
        assert!(
            hint_violations(
                "it might double if the trend holds, because launches like this one continue.",
                true
            )
            .is_empty()
        );
    }

    #[test]
    fn each_certainty_word_alone_still_refuses_a_hedged_rated_hint() {
        // Four sentences, each certain only through one specific word in
        // the `will || 'll || going to || moon || moonshot` chain, with
        // every other certainty word absent. Each pins the `||` before
        // that word: mutated to `&&`, the chain needs every earlier
        // disjunct true simultaneously and this one alone can never make
        // `certain` true, so the sentence is wrongly authorised.
        for reply in [
            "it could double, and it'll happen too, because launches like this one continue.",
            "it could double, and it is going to happen, because launches like this one continue.",
            "it could double, heading to the moon, because launches like this one continue.",
            "it could double, analysts call it a moonshot, because launches like this continue.",
        ] {
            assert!(!hint_violations(reply, true).is_empty(), "{reply}");
        }
    }

    #[test]
    fn a_hedge_alone_without_reasoning_is_still_refused() {
        // Hedged and rated, but not reasoned and not certain -- refused
        // for the missing reasoning. Pins both `||`s on the outer
        // `!has_rate || !hedged || !reasoned || certain` gate: mutated to
        // `&&` at either position, the missing-reasoning branch stops
        // being enough on its own and the hint is wrongly authorised.
        assert!(!hint_violations("it could double this week.", true).is_empty());
    }

    #[test]
    fn a_canttell_reply_naming_the_topic_without_the_article_passes() {
        // The live failure of 2026-09-17. `sheet::phrase_for` writes every
        // unknown as "the X could not be read", and the check used to ask
        // whether the reply contained "the holders" as one string. A reply
        // that names the holders in ordinary English does not carry the
        // article, so the rule refused a reply that satisfied it, the
        // template shipped, and the reader got the fact dump Josh complained
        // about.
        let sheet = required_sheet(vec!["the holders could not be read".to_owned()]);
        for reply in [
            "Holders couldn't be read, so this is a shrug for now.",
            "No read on holders here.",
            "holders: unreadable at this block.",
        ] {
            assert!(
                check_required(reply, Level::CantTell, &sheet).is_empty(),
                "{reply}"
            );
        }
    }

    #[test]
    fn a_canttell_reply_naming_a_possessive_topic_without_its_apostrophe_passes() {
        // "the creator's history" needs both words, not the punctuation. A
        // reply writing "creator history" names the same thing.
        let sheet = required_sheet(vec!["the creator's history could not be read".to_owned()]);
        assert!(
            check_required(
                "Couldn't pull the creator history, so nothing to say there.",
                Level::CantTell,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn a_canttell_reply_naming_only_half_a_topic_is_still_refused() {
        // Dropping the article must not drop the gate. "the creator's
        // history" takes both words: a reply that says "creator" and never
        // says what about the creator could not be read has not named the
        // thing, and this is the loosening's own boundary.
        let sheet = required_sheet(vec!["the creator's history could not be read".to_owned()]);
        assert_eq!(
            check_required(
                "The creator is around somewhere, but this is a shrug.",
                Level::CantTell,
                &sheet
            )
            .len(),
            1
        );
    }

    #[test]
    fn a_canttell_reply_naming_a_different_unknown_thing_still_passes() {
        // Any one of `unknown`'s topics is enough -- the packet says "at
        // least one," not "all."
        let sheet = required_sheet(vec![
            "the launch block could not be read".to_owned(),
            "the bonding curve could not be read".to_owned(),
        ]);
        assert!(
            check_required(
                "The bonding curve came back empty, so no read here.",
                Level::CantTell,
                &sheet
            )
            .is_empty()
        );
    }

    /// Both halves of "says the age is unknown", one at a time.
    ///
    /// The ageless branch asks for an age word AND a phrase admitting the age
    /// was not read. Flip that `&&` to `||` and either half alone would do,
    /// which is how "old or not" becomes an age statement. These two replies
    /// each satisfy exactly one half, so each fails under the flip and passes
    /// today.
    #[test]
    fn an_ageless_reply_that_never_says_the_age_is_unknown_is_refused() {
        let sheet = required_sheet(vec!["the launch block could not be read".to_owned()]);
        assert!(
            !check_required(
                "Eleven at birth, old or not, read at block 100. Nothing ugly yet.",
                Level::NothingUglyYet,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn an_ageless_reply_that_admits_a_gap_without_naming_the_age_is_refused() {
        // "unknown" with no age word: the reply admits *something* was not
        // read without saying it was how long this token has existed.
        let sheet = required_sheet(vec!["the launch block could not be read".to_owned()]);
        assert!(
            !check_required(
                "How far back this one goes is unknown, read at block 100.",
                Level::NothingUglyYet,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn a_nothinguglyyet_reply_with_no_age_is_refused() {
        // Re-apply by deleting the `states_one_of` half of the `Some(age)`
        // arm's condition (or hard-coding it `true`): this starts passing
        // when it must fail.
        let sheet = required_sheet_with_age(Vec::new());
        assert!(
            !check_required(
                "Nothing ugly here yet, clean so far.",
                Level::NothingUglyYet,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn a_nothinguglyyet_reply_with_the_sheets_age_passes() {
        let sheet = required_sheet_with_age(Vec::new());
        assert!(
            check_required(
                "Nothing ugly yet. Launched about 7.1 hours ago -- 63954 slots.",
                Level::NothingUglyYet,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn a_nothinguglyyet_reply_stating_only_the_read_point_is_refused_on_a_sheet_with_an_age() {
        // The defect this task fixes: the read point is not the age, and a
        // sheet with a real age fact must not be satisfied by a reply that
        // cites only when the sheet was read. Re-apply by making the read
        // point's own number a match for the age check (e.g. reverting to
        // checking `sheet.read_at` instead of the age fact's values): this
        // starts passing when it must fail.
        let sheet = required_sheet_with_age(Vec::new());
        assert!(
            !check_required(
                "Read at slot 444007820, nothing ugly yet.",
                Level::NothingUglyYet,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn a_nothinguglyyet_reply_with_an_unrelated_number_is_still_refused() {
        // The number alone is not enough -- it has to be attached to an age
        // word, or "11 accounts, nothing ugly yet" (an authorised figure
        // that has nothing to do with when the sheet was read) would pass.
        let sheet = required_sheet_with_age(Vec::new());
        assert!(
            !check_required(
                "11 token accounts, nothing ugly yet.",
                Level::NothingUglyYet,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn a_nothinguglyyet_reply_with_an_age_word_but_a_fabricated_number_is_refused() {
        // The age word alone is not enough either -- the number beside it has
        // to be the one the sheet actually authorised, reusing
        // `fidelity::check`'s own notion of "authorised" rather than a
        // second, looser one.
        let sheet = required_sheet_with_age(Vec::new());
        assert!(
            !check_required(
                "1 hour ago, nothing ugly yet.",
                Level::NothingUglyYet,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn a_nothinguglyyet_reply_on_an_ageless_sheet_saying_nothing_about_the_age_is_refused() {
        // Robinhood-shaped: no age fact. Citing only the read point, the way
        // the pre-packet stand-in check accepted, is not enough -- rule 8,
        // unknown is not safe, so the reply must say the age itself could not
        // be read.
        let sheet = required_sheet(Vec::new());
        assert!(
            !check_required(
                "Read at block 100, nothing ugly yet.",
                Level::NothingUglyYet,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn a_nothinguglyyet_reply_on_an_ageless_sheet_must_still_state_the_read_point() {
        // Saying the age is unknown is not enough either -- design 0020 §4
        // keeps the read point required too, on every sheet that has one.
        let sheet = required_sheet(Vec::new());
        assert!(
            !check_required(
                "How old this token is could not be read. Nothing ugly yet.",
                Level::NothingUglyYet,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn a_nothinguglyyet_reply_on_an_ageless_sheet_stating_both_passes() {
        let sheet = required_sheet(Vec::new());
        assert!(
            check_required(
                "How old this token is could not be read. Read at block 100, nothing ugly yet.",
                Level::NothingUglyYet,
                &sheet
            )
            .is_empty()
        );
    }

    #[test]
    fn check_required_has_no_opinion_at_the_other_three_levels() {
        let sheet = required_sheet(Vec::new());
        for level in [Level::Sketchy, Level::RugMechanicsLive, Level::Rugged] {
            assert!(
                check_required("anything at all", level, &sheet).is_empty(),
                "{level:?} must carry no requirement from this function"
            );
        }
    }

    #[test]
    fn the_template_carries_the_required_line_at_every_level_on_both_chains() {
        // The packet's expensive-to-miss case: `voice::write` publishes
        // `verdict::template` on every refusal, so if the template itself
        // cannot pass this check, a refusal ships the very thing it exists
        // to prevent. Tested at all five levels, on a Solana sheet (a real
        // age) and a Robinhood one (ageless) -- three levels carry no
        // requirement and pass trivially, but they are asserted anyway so a
        // future third-level requirement cannot silently start failing here
        // unnoticed.
        let unknown_solana =
            required_sheet_with_age(vec!["the launch block could not be read".to_owned()]);
        let clean_solana = required_sheet_with_age(Vec::new());
        let unknown_robinhood =
            required_sheet(vec!["the launch block could not be read".to_owned()]);
        let clean_robinhood = required_sheet(Vec::new());
        for (level, sheet) in [
            (Level::CantTell, &unknown_solana),
            (Level::NothingUglyYet, &clean_solana),
            (Level::Sketchy, &clean_solana),
            (Level::RugMechanicsLive, &clean_solana),
            (Level::Rugged, &clean_solana),
            (Level::CantTell, &unknown_robinhood),
            (Level::NothingUglyYet, &clean_robinhood),
            (Level::Sketchy, &clean_robinhood),
            (Level::RugMechanicsLive, &clean_robinhood),
            (Level::Rugged, &clean_robinhood),
        ] {
            let text = crate::verdict::template(sheet);
            assert!(
                check_required(&text, level, sheet).is_empty(),
                "the template must pass check_required at {level:?}: {text:?}"
            );
        }
    }

    // -----------------------------------------------------------------
    // Design 0024's lane-2 guardrails (`has_cashtag`, `check_no_identification`,
    // `check_any_person_reference`). Each test below isolates one branch of
    // an `||` (or the top-level `&&`) so a mutant that widens or narrows it
    // fails exactly one of these, not by accident.
    // -----------------------------------------------------------------

    #[test]
    fn has_cashtag_needs_the_dollar_immediately_before_an_alphanumeric() {
        assert!(!has_cashtag("ab"), "no dollar sign at all is not a cashtag");
        assert!(
            !has_cashtag("$"),
            "a dollar with nothing after it is not a cashtag"
        );
        assert!(
            has_cashtag("$5"),
            "a dollar directly before an alphanumeric is a cashtag"
        );
    }

    #[test]
    fn check_no_identification_needs_both_a_named_thing_and_a_copula() {
        // `names_one && has_copula` -- exactly one true must not be enough.
        assert!(
            check_no_identification("it is great").is_empty(),
            "a copula with nothing identified is not a claim"
        );
        assert!(
            check_no_identification("drop a contract address").is_empty(),
            "the nudge line itself: identified, but no copula, so it must \
             pass -- design 0024 §2.1's own worked example"
        );
        // Each `||` inside `has_copula` triggered alone, with the other two
        // false, so a mutant narrowing it to `&&` loses exactly this case.
        assert!(
            !check_no_identification("the token is great").is_empty(),
            "\"is\" alone must trigger the copula"
        );
        assert!(
            !check_no_identification("this coin are great").is_empty(),
            "\"are\" alone must trigger the copula"
        );
        assert!(
            !check_no_identification("this coin's great").is_empty(),
            "\"'s \" alone must trigger the copula"
        );
    }

    #[test]
    fn check_any_person_reference_fires_on_any_one_shape_alone() {
        // Each `||` term triggered alone, with the other three false, so a
        // mutant narrowing any one `||` to `&&` loses exactly that case.
        assert!(
            !check_any_person_reference("the dev thinks so").is_empty(),
            "a PERSON_WORDS hit alone must fire"
        );
        assert!(
            !check_any_person_reference("Totally Legit here").is_empty(),
            "a capitalized run alone must fire"
        );
        assert!(
            !check_any_person_reference("0x1234 is a fraud").is_empty(),
            "an address-as-subject alone must fire"
        );
        assert!(
            check_any_person_reference("nothing notable happening here").is_empty(),
            "none of the shapes present must not fire"
        );
    }
}
