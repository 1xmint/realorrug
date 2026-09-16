// SPDX-License-Identifier: Apache-2.0
//! The thread that remembers, and the matcher that reads a follow-up.
//!
//! Design 0022 (threaded follow-ups) is a whole feature; this module is
//! **half** of it, per packet 0040. It ships:
//!
//! - [`Topic`] and [`match_topic`] -- design 0022 §2's fixed matcher, code
//!   only, no model call, no chain read.
//! - [`ThreadMemory`] -- design 0022 §3's per-thread record, in memory only.
//! - [`refusal_sentence`] -- design 0022 §1's fixed "no follow-up for that"
//!   line, one per [`Level`], carrying no number.
//!
//! **Not built here, on purpose** (design 0022 §5 is the next packet, not
//! this one):
//!
//! - The per-thread and per-person caps. A person asking ten unmatched
//!   questions in one thread gets ten refusal lines today, bounded only by
//!   the account's existing daily reply budget
//!   (`crates/realorrug-analyst/src/spend.rs`'s `Cost::Reply`), not by
//!   anything in this module.
//! - The one-thing chain read for a *matched* topic (design 0022 §2's right
//!   column). [`match_topic`] returning `Some(topic)` is as far as this
//!   packet goes; nothing here reads the chain or asks a model to say what
//!   the topic's fact is. A matched follow-up today falls through to
//!   whatever the mention would have done anyway before this module existed.
//! - The level-delta check (design 0022 §4).
//!
//! # Why the phrase list is code, not a data file
//!
//! Design 0020's first-party address list is a data file because those
//! addresses are observed facts about the world, each with a capture behind
//! it. A phrase list is not a fact about the world; it is this product's own
//! vocabulary, which nobody measured and no source settles. It also must not
//! be switchable off by a missing file -- rule 7's deny-by-default would turn
//! every follow-up into a refusal the moment a path was wrong, which is the
//! wrong failure mode for "somebody asked a reasonable question."

use std::collections::{HashMap, HashSet};

use realorrug_roast::Level;

/// One of design 0020 §2's seven one-thing questions a follow-up can dig
/// into.
///
/// **Declared in this order deliberately.** [`match_topic`] breaks a tie --
/// two phrases starting at the same text index, which one phrase being a
/// prefix of another can produce -- toward whichever variant is declared
/// first here. An undocumented tie-break is a coin flip that looks like a
/// rule, so the order is: bundled, dev, current safety, dev sold, can sell,
/// holders, repeat launcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Topic {
    /// The launch-block recipient count, and `LaunchBlockBundle`.
    Bundled,
    /// The creator/fee-recipient address and creator track record.
    Dev,
    /// A fresh read of reserves -- "is it safe *now*."
    CurrentSafety,
    /// The creator's current balance, and `CreatorSoldOut`.
    DevSold,
    /// The simulated sell, and `BuyersCannotSell`.
    CanSell,
    /// Holder count and largest non-curve holder's share, and
    /// `HolderConcentration`.
    Holders,
    /// Whether this creator (or a launch-block buyer) has launched before --
    /// `RepeatLauncher`.
    RepeatLauncher,
}

/// The order [`match_topic`] breaks a tie in, and the order its own doc
/// comment states. Kept as one list so the two cannot drift apart.
const ORDER: [Topic; 7] = [
    Topic::Bundled,
    Topic::Dev,
    Topic::CurrentSafety,
    Topic::DevSold,
    Topic::CanSell,
    Topic::Holders,
    Topic::RepeatLauncher,
];

/// The phrases a topic is recognised by.
///
/// **Exhaustive, no `_` arm**, so an eighth topic cannot compile without
/// someone deciding what it sounds like. Every phrase here must already be
/// lowercase -- [`match_topic`] lowercases the reply text once and compares
/// against these directly, and a test in this module asserts the list holds
/// that invariant so an uppercase phrase does not silently become
/// unmatchable.
#[must_use]
pub fn phrases(topic: Topic) -> &'static [&'static str] {
    match topic {
        Topic::Bundled => &["bundled", "was it bundled", "snipe", "sniped"],
        Topic::Dev => &[
            "who's the dev",
            "who is the dev",
            "who made this",
            "deployer",
        ],
        Topic::CurrentSafety => &["is it safe now", "still safe", "still okay", "any update"],
        Topic::DevSold => &["did the dev sell", "creator dumped", "dev dump", "dev sell"],
        Topic::CanSell => &[
            "can i sell",
            "can buyers sell",
            "can buyers get out",
            "honeypot",
        ],
        Topic::Holders => &["how many holders", "who's holding this", "holder spread"],
        Topic::RepeatLauncher => &[
            "done this before",
            "other rugs",
            "launched before",
            "past launches",
        ],
    }
}

/// Matches a reply's text against design 0022 §2's fixed phrase list.
///
/// **Earliest match wins.** When a reply names two topics -- "did it get
/// bundled and did the dev sell" -- the one whose phrase appears earliest in
/// the text is the answer, because it is the first thing the asker typed,
/// which is explainable to a reader without reading any code, and it is
/// deterministic. A tie (two phrases starting at the same index) breaks by
/// [`Topic`]'s declared order, per its own doc comment.
///
/// Pure. No call, no chain read, no model. It is the fence design 0022 §6
/// describes: the untrusted reply text is consumed here and never travels
/// further.
#[must_use]
pub fn match_topic(text: &str) -> Option<Topic> {
    let lower = text.to_lowercase();
    // `&str`, not `&String`: a `Copy` type, so both closures below can take
    // it by value without moving `lower` itself -- which a `String` cannot
    // survive being moved into on every one of the seven calls `flat_map`
    // makes.
    let lower: &str = &lower;
    let candidates = ORDER.iter().flat_map(move |&topic| {
        phrases(topic)
            .iter()
            .filter_map(move |phrase| lower.find(phrase).map(|at| (at, topic)))
    });
    earliest(candidates)
}

/// The earliest-wins, declared-order-breaks-ties rule, factored out of
/// [`match_topic`] so it can be exercised directly against a synthetic set
/// of candidates -- the real phrase table has no two phrases that start at
/// the same index in any one text, so a test against `match_topic` alone
/// could never observe a genuine tie.
///
/// `min_by_key` returns the *first* minimum on equal keys, which is why the
/// order `candidates` is produced in (here, [`ORDER`]) is what decides ties,
/// not insertion order within one topic's own phrase list.
fn earliest(candidates: impl Iterator<Item = (usize, Topic)>) -> Option<Topic> {
    candidates.min_by_key(|&(at, _)| at).map(|(_, topic)| topic)
}

/// One thread's standing state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadRecord {
    /// The token this thread has been reading. Carried because a follow-up's
    /// own text ordinarily names no address (`mention.rs`'s module doc:
    /// only a base58 mint or a `$TICKER` is ever extracted, and a follow-up
    /// usually has neither), so a caller answering one has nothing else to
    /// log the reply against.
    pub token: String,
    /// The verdict level currently standing for this thread.
    ///
    /// **Not the whole `Verdict`.** Design 0022 §3 as written says the
    /// record holds the standing `Verdict`; packet 0040 changes that to the
    /// level alone, and this document says so in §3. A level is what design
    /// 0022 §4 and [`refusal_sentence`] actually use, the reply log
    /// (`crate::log::Entry`) already keeps the sheet and the reply text for
    /// audit, and storing a second copy of the evidence here is a second
    /// thing that can drift from the first.
    pub level: Level,
    /// Which of [`Topic`]'s seven questions this thread has already been
    /// answered on.
    ///
    /// Recorded but not yet *read* by anything in this packet -- design
    /// 0022 §5's "a second 'is it bundled' in the same thread is answered
    /// from what is already stored" is the next packet's behaviour.
    /// Recording it now costs nothing and avoids a second migration of this
    /// type when that packet lands.
    pub answered: HashSet<Topic>,
}

/// Every thread this process has answered in, since it started.
///
/// **In memory, for the process's lifetime, not on disk.**
/// `admission::Gate` holds its own dedupe and counters in plain `HashMap`s
/// with no path behind them; this matches it, for the same reason: a
/// restart loses thread memory, and the worst that costs is a follow-up
/// treated as a first mention -- a read the bot would have made anyway
/// before this feature existed, already bounded by the mint dedupe
/// `admission.rs` already applies. Do not invent a state file, and do not
/// add a config key.
#[derive(Default, Debug)]
pub struct ThreadMemory {
    /// Design 0022 §3's own key: a thread's memory is keyed on
    /// `(conversation_id, token)`, not on the conversation alone (so a
    /// second token asked in the same conversation gets its own record) and
    /// not on the token alone across different conversations (so one
    /// thread's reads cannot answer an unrelated thread's questions).
    by_key: HashMap<(String, String), ThreadRecord>,
}

impl ThreadMemory {
    /// An empty thread memory, for the daemon to hold for its own lifetime.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the level a thread's verdict stands at.
    ///
    /// Called where the verdict is computed for the published reply -- not
    /// by reading it back out of the log, which would make the log
    /// load-bearing for behaviour instead of for audit (design 0022 §3, as
    /// this packet changes it). Overwrites whatever level stood before for
    /// this `(conversation, token)`: the newest verdict is the standing one.
    pub fn record(&mut self, conversation: &str, token: &str, level: Level) {
        let record = self
            .by_key
            .entry((conversation.to_owned(), token.to_owned()))
            .or_insert_with(|| ThreadRecord {
                token: token.to_owned(),
                level,
                answered: HashSet::new(),
            });
        record.level = level;
    }

    /// The standing record for a conversation, if this thread has answered
    /// in it before.
    ///
    /// **Looked up by conversation id alone**, even though the map is keyed
    /// on the pair. A follow-up's own text ordinarily names no address (see
    /// [`ThreadRecord::token`]'s doc comment), so the caller checking
    /// whether a mention is a follow-up has no token to look the pair up by
    /// yet -- and every thread this packet's caller can build has exactly
    /// one token in it, because [`record`](Self::record) is only ever
    /// called with the one mint a thread's first answer resolved. A second,
    /// *different* token asked inside the same conversation is design 0022
    /// §5's territory (a second, independent key), not something this
    /// lookup needs to disambiguate yet.
    #[must_use]
    pub fn standing(&self, conversation: &str) -> Option<&ThreadRecord> {
        self.by_key
            .iter()
            .find(|((c, _), _)| c == conversation)
            .map(|(_, record)| record)
    }
}

/// Design 0022 §1's fixed line for a follow-up that matches no topic:
/// plainly no follow-up for that question, and the standing verdict restated
/// -- one fixed sentence per level, behind an exhaustive `match`.
///
/// **Carries no number.** A number would have to come from a fact sheet this
/// path never builds, and rule 2 says every number in a reply is on the
/// sheet. A sentence per level is the only thing this path is entitled to
/// say.
#[must_use]
pub const fn refusal_sentence(level: Level) -> &'static str {
    match level {
        Level::Rugged => {
            "I don't have a follow-up for that one -- what I've already told this thread \
             stands: this one rugged."
        }
        Level::RugMechanicsLive => {
            "I don't have a follow-up for that one -- what I've already told this thread \
             stands: the mechanics for a rug are live on this one."
        }
        Level::Sketchy => {
            "I don't have a follow-up for that one -- what I've already told this thread \
             stands: this one is sketchy."
        }
        Level::NothingUglyYet => {
            "I don't have a follow-up for that one -- what I've already told this thread \
             stands: nothing ugly yet."
        }
        Level::CantTell => {
            "I don't have a follow-up for that one -- what I've already told this thread \
             stands: I can't tell on this one."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_phrase_in_the_table_is_already_lowercase() {
        // The lowercasing in `match_topic` silently makes any uppercase
        // phrase here unmatchable -- nothing in that text would ever equal
        // it, and the failure would look like "the matcher doesn't work" for
        // one topic only.
        for topic in ORDER {
            for phrase in phrases(topic) {
                assert_eq!(
                    *phrase,
                    phrase.to_lowercase(),
                    "{topic:?} carries a phrase that is not lowercase: {phrase:?}"
                );
            }
        }
    }

    #[test]
    fn each_topic_is_matched_by_its_own_phrases_and_no_others() {
        for topic in ORDER {
            for phrase in phrases(topic) {
                assert_eq!(
                    match_topic(phrase),
                    Some(topic),
                    "{phrase:?} should match {topic:?}"
                );
                for other in ORDER {
                    if other == topic {
                        continue;
                    }
                    for other_phrase in phrases(other) {
                        assert_ne!(
                            phrase, other_phrase,
                            "{topic:?} and {other:?} share a phrase: {phrase:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_text_matching_nothing_returns_none() {
        assert_eq!(match_topic("why did you say that"), None);
        assert_eq!(match_topic("prove it"), None);
        assert_eq!(match_topic(""), None);
    }

    #[test]
    fn the_earliest_match_wins_in_either_order() {
        // Both directions matter: a test where the phrases are in the same
        // order every time cannot tell "the earliest index wins" from "this
        // variant always wins." `bundled` and `did the dev sell` are chosen
        // so the indexes differ by more than one character.
        let bundled_first = "did it get bundled, and did the dev sell";
        assert_eq!(match_topic(bundled_first), Some(Topic::Bundled));

        let dev_sold_first = "did the dev sell, and was it bundled";
        assert_eq!(match_topic(dev_sold_first), Some(Topic::DevSold));
    }

    #[test]
    fn a_genuine_index_tie_breaks_toward_the_earlier_declared_variant() {
        // No two phrases in the real table start at the same index in any
        // one text, so the tie rule is exercised directly against
        // `earliest`, the combinator `match_topic` is built on. `earliest`
        // returns the first minimum on a tie, and `match_topic` produces
        // candidates walking `ORDER` -- so feeding candidates in `ORDER`'s
        // own sequence, as `match_topic` does, is what makes "first in
        // iteration" mean "declared first."
        assert_eq!(
            earliest([(3, Topic::Bundled), (3, Topic::Holders)].into_iter()),
            Some(Topic::Bundled),
            "Bundled is declared before Holders, and was iterated first"
        );
        assert_eq!(
            earliest([(5, Topic::Dev), (5, Topic::RepeatLauncher)].into_iter()),
            Some(Topic::Dev),
            "Dev is declared before RepeatLauncher, and was iterated first"
        );
        // The iteration order, not the numeric value of the variant, is
        // what decides it: feeding the later-declared topic first flips the
        // winner, which is exactly what ties `earliest` to `ORDER` rather
        // than to some other implicit ranking.
        assert_eq!(
            earliest([(5, Topic::RepeatLauncher), (5, Topic::Dev)].into_iter()),
            Some(Topic::RepeatLauncher)
        );
    }

    #[test]
    fn thread_memory_starts_empty() {
        let memory = ThreadMemory::new();
        assert_eq!(memory.standing("conv-1"), None);
    }

    #[test]
    fn a_recorded_thread_is_found_by_conversation_alone() {
        let mut memory = ThreadMemory::new();
        memory.record("conv-1", "mint-a", Level::Sketchy);
        let record = memory.standing("conv-1").expect("recorded");
        assert_eq!(record.token, "mint-a");
        assert_eq!(record.level, Level::Sketchy);
        assert!(record.answered.is_empty());
    }

    #[test]
    fn recording_again_overwrites_the_level() {
        let mut memory = ThreadMemory::new();
        memory.record("conv-1", "mint-a", Level::NothingUglyYet);
        memory.record("conv-1", "mint-a", Level::RugMechanicsLive);
        assert_eq!(
            memory.standing("conv-1").expect("recorded").level,
            Level::RugMechanicsLive
        );
    }

    #[test]
    fn an_unrecorded_conversation_is_not_a_thread() {
        let mut memory = ThreadMemory::new();
        memory.record("conv-1", "mint-a", Level::Sketchy);
        assert_eq!(memory.standing("conv-2"), None);
    }

    #[test]
    fn every_level_has_a_pinned_sentence_with_no_digit() {
        for level in [
            Level::Rugged,
            Level::RugMechanicsLive,
            Level::Sketchy,
            Level::NothingUglyYet,
            Level::CantTell,
        ] {
            let sentence = refusal_sentence(level);
            assert!(
                !sentence.chars().any(|c| c.is_ascii_digit()),
                "{level:?} carries a digit: {sentence}"
            );
            assert!(
                sentence.contains("I don't have a follow-up"),
                "{level:?}: {sentence}"
            );
        }
        // Pinned per level, not merely "some sentence came back": a mutant
        // that swaps two arms' bodies must fail this.
        assert!(refusal_sentence(Level::Rugged).contains("rugged"));
        assert!(refusal_sentence(Level::RugMechanicsLive).contains("mechanics for a rug"));
        assert!(refusal_sentence(Level::Sketchy).contains("sketchy"));
        assert!(refusal_sentence(Level::NothingUglyYet).contains("nothing ugly yet"));
        assert!(refusal_sentence(Level::CantTell).contains("can't tell"));
    }
}
