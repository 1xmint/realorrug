// SPDX-License-Identifier: Apache-2.0
//! Answering one mention, from a mint to a log entry.
//!
//! # Why this is here rather than in the command that had it
//!
//! `radar analyst --mentions <file>` had this pipeline inline, which was right
//! while it was the only caller. The daemon is a second one, and two copies of
//! *what the account says* is the arrangement where the version somebody reads
//! two hundred replies from is not the version that posts them.
//!
//! So the pipeline lives here and the two callers differ only in what they do
//! with the result: the command prints it, the daemon publishes it. Neither
//! decides what it says.
//!
//! # It returns rather than prints
//!
//! [`Answered`] carries every outcome, including the ones that are not a reply.
//! A mention naming a symbol, a mention naming nothing, and a mention refused by
//! the gate are all *results* — the command renders them for a person and the
//! daemon counts them, and neither has to infer what happened from an empty
//! return.

use realorrug_onchain::{RpcClient, dispatch};
use realorrug_roast::BaseRates;
use realorrug_types::Address;

use crate::admission::{Admitted, Gate, Refused};
use crate::log::Entry;
use crate::mention::Asked;
use crate::x::Mention;
use realorrug_roast::Billed;

/// Everything answering a mention needs that does not change between mentions.
pub struct Answering<'a> {
    /// The chain, read on demand.
    pub client: &'a RpcClient,
    /// Robinhood Chain's endpoint, or `None` when it is not configured.
    ///
    /// AGENTS.md §3 rule 7: there is no default (`realorrug-cli`'s
    /// `launch-check --rpc` is the pattern -- the public endpoint is
    /// rate-limited, so picking it silently would be picking for the
    /// operator). A Robinhood address that arrives with this `None` is
    /// answered [`Answered::Unreadable`], never dispatched to Solana and
    /// never [`Answered::NotAnAddress`].
    pub robinhood: Option<&'a realorrug_robinhood::Rpc>,
    /// The published base rates, or `None` when the snapshot is missing.
    ///
    /// `None` makes a reply say less rather than say more: a recipient count
    /// with no population to quote it against is a number without a meaning,
    /// and rule 9 says a missing rate means the claim cannot be made.
    pub rates: Option<&'a BaseRates>,
    /// Every creator's record, or `None` when the index is missing.
    ///
    /// The fact that makes one reply differ from another. Without it three
    /// coins launched in the same minute produce the same sentences, because
    /// the cost line is a constant and most launches sit in the same recipient
    /// band. `None` makes the reply say so rather than say nothing.
    pub creators: Option<&'a realorrug_roast::CreatorIndex>,
    /// The model, or `None` for the deterministic template.
    pub provider: Option<&'a dyn realorrug_model::Provider>,
    /// The analyst's own token, or `None` when no token is special.
    ///
    /// ADR 0013 constraint 5: a price or market-cap fact about this mint is
    /// dropped from the sheet before the model sees it. Read once from
    /// `RADAR_SELF_MINT` by the caller, which **stops** on a value that will not
    /// parse rather than passing `None` -- because `None` here means the rule
    /// is off, and a misspelt mint must not switch it off for the real token.
    pub self_mint: Option<&'a Address>,
    /// Seconds since the epoch, supplied rather than read.
    ///
    /// The gate's windows are computed from it, so a caller can drive a day of
    /// admissions through this without waiting one.
    pub now: u64,
}

/// What happened to one mention.
#[derive(Debug)]
pub enum Answered {
    /// A reply was built. The entry is not yet published.
    Reply {
        /// The reply and its evidence, ready to publish.
        entry: Box<Entry>,
        /// What the voice pass owes the meter that reserved it.
        billed: Billed,
    },
    /// The mention named a symbol, which identifies nothing.
    ///
    /// Carries the reply that says so — the honest answer, and the best content
    /// available: guessing which token a symbol meant is how measurements get
    /// published about the wrong project.
    Ticker {
        /// The key the gate admitted this on, so the caller records against the
        /// same one. Derived here rather than by the caller: two derivations of
        /// one key is how a dedupe map ends up with entries nothing looks up.
        key: String,
        /// What to say.
        text: String,
    },
    /// A reply inside a thread this bot has already answered in, whose text
    /// matches none of `followup::Topic`'s phrases.
    ///
    /// Design 0022 §1's fixed refusal: no follow-up for that question, and
    /// the standing verdict restated with no number in it
    /// (`followup::refusal_sentence`). No model call, no chain read — a
    /// sibling of [`Self::Ticker`], not a reuse of it, because `Ticker`'s own
    /// doc comment is specifically about a symbol naming nothing, and this is
    /// a different reason to say the same *shape* of thing (a plain-text
    /// reply, built with no model call, for a mention understood but not
    /// answerable the way the asker hoped).
    Followup {
        /// The key the gate admitted this on, matching [`Self::Ticker`]'s own
        /// reasoning for carrying it here rather than re-deriving it.
        key: String,
        /// What to say.
        text: String,
    },
    /// Design 0024's lane 2: a mention naming no address and no ticker,
    /// outside any thread this bot already stands a verdict in.
    ///
    /// A short, in-character reply plus the fixed nudge
    /// (`crate::lane2::NUDGE`), or `crate::lane2::FALLBACK` when a guardrail
    /// refused the model's answer or refused the mention outright before a
    /// call. No chain read, no `FactSheet` -- a sibling of [`Self::Ticker`]
    /// and [`Self::Followup`], not a reuse of either: those two are plain
    /// text built with zero model calls, and this one sometimes is and
    /// sometimes is not, which is exactly why its own guardrails
    /// (`crate::lane2`, `realorrug_roast::forbidden`) exist.
    Lane2 {
        /// The key lane 2's own gate admitted this on.
        key: String,
        /// What to say.
        text: String,
        /// What the model call (if any was made) owes the meter, same
        /// reasoning as [`Self::Reply`]'s own field.
        billed: Billed,
    },
    /// Nothing usable was found in the mention.
    Nothing,
    /// The gate refused it.
    Refused(Refused),
    /// The mint parsed as base58 but is not an address.
    NotAnAddress,
    /// The chain could not be read within the call budget.
    Unreadable(String),
}

impl Answered {
    /// What the meter should do with the model reservation made before this.
    ///
    /// Every outcome but a reply stopped at the parser, the gate or an
    /// unreadable chain and never reached the provider, so the reservation is
    /// given back. Answered here rather than at each call site because the X
    /// loop and the Telegram lane must not answer it differently, and matched
    /// exhaustively so a new outcome has to say which side it is on.
    #[must_use]
    pub const fn billed(&self) -> Billed {
        match self {
            Self::Reply { billed, .. } | Self::Lane2 { billed, .. } => *billed,
            Self::Ticker { .. }
            | Self::Followup { .. }
            | Self::Nothing
            | Self::Refused(_)
            | Self::NotAnAddress
            | Self::Unreadable(_) => Billed::NoCall,
        }
    }
}

/// Answers one mention.
///
/// Reads the chain, builds the fact sheet, writes the reply, and returns the
/// log entry — **without publishing it**. Publishing is
/// [`publish`](crate::publish::publish), which records before it says anything,
/// and keeping the two apart is what lets a caller build two hundred replies and
/// post none of them.
///
/// The gate is consulted here because a refusal must happen **before** the chain
/// is read: the read is the expensive part, and admitting first would mean a
/// refused mention still cost a dossier.
///
/// `threads` is packet 0040's addition: the process-lifetime memory of which
/// conversations this bot has already answered in, and at what verdict level
/// (`crate::followup::ThreadMemory`). Checked first, before the mint/ticker
/// parse below, because a follow-up in a remembered thread that matches no
/// topic never gets that far — it is answered from the standing level alone,
/// with no chain read.
pub fn answer(
    mention: &Mention,
    gate: &mut Gate,
    threads: &mut crate::followup::ThreadMemory,
    lane2: &mut crate::lane2::Gate,
    ctx: &Answering<'_>,
) -> Answered {
    // **A follow-up that names nothing design 0020 §2 lists** gets design
    // 0022 §1's fixed refusal, before the mint/ticker parse below ever runs.
    // Only mentions inside a thread this process has already answered in are
    // eligible — `ThreadMemory::standing` returns `None` for every ordinary
    // first mention, which falls straight through to today's path
    // unchanged. A *matched* topic also falls through unchanged: reading the
    // one fact a matched topic names is design 0022 §2's right column, the
    // next packet's job, not this one's.
    if let Some(conversation) = &mention.conversation
        && let Some(record) = threads.standing(conversation)
        && crate::followup::match_topic(&mention.text).is_none()
    {
        // Gated like a ticker reply, on a key this reply's dedupe can be
        // recorded against, so a person cannot spend an unbounded number of
        // free follow-up refusals past the account's ordinary per-summoner
        // and global caps -- the same reasoning `Asked::Ticker` below already
        // states for gating a reply that costs no chain read.
        let key = format!("followup:{conversation}");
        if let Admitted::No(why) = gate.admit(&mention.author, &key, ctx.now) {
            return Answered::Refused(why);
        }
        return Answered::Followup {
            text: crate::followup::refusal_sentence(record.level).to_owned(),
            key,
        };
    }

    let mint_text = match crate::mention::read(&mention.text) {
        Asked::Mint(m) => m,
        Asked::Ticker(t) => {
            // **Gated, like a mint.** This returned before the gate until
            // 2026-09-07, which was harmless only because nothing published a
            // ticker reply. Answering them without a gate would put one reply
            // shape outside every cap in this module: `$A`, `$B`, `$C` for
            // ever, one post each, from one account.
            //
            // Keyed on the symbol, which is what makes the dedupe meaningful:
            // the answer to `$DOGE` is the same sentence for everyone who asks
            // inside the window.
            let key = format!("${t}");
            if let Admitted::No(why) = gate.admit(&mention.author, &key, ctx.now) {
                return Answered::Refused(why);
            }
            return Answered::Ticker {
                text: crate::ticker_reply(&t),
                key,
            };
        }
        // Design 0024: lane 2 owns every mention naming no address and no
        // ticker, now that the thread check above has already ruled out
        // "this is a follow-up in a thread we already stand a verdict in."
        // `lane2::reply` reads no chain and admits itself on its own,
        // smaller gate (design 0024 §3) -- `answer.rs` only routes to it.
        Asked::Nothing => return crate::lane2::reply(mention, lane2, ctx.provider, ctx.now),
    };

    if let Admitted::No(why) = gate.admit(&mention.author, &mint_text, ctx.now) {
        return Answered::Refused(why);
    }

    // Dispatched on the address's own shape -- `0x` is always Robinhood's,
    // anything else is tried as Solana's base58 -- never on configuration and
    // never guessed. The one function this whole task exists to introduce:
    // both analyst entry points and the CLI call it rather than each writing
    // its own "which chain is this" match. It also owns the call budget
    // (`realorrug-onchain`'s own default, sixty calls, three pages, twenty
    // seconds) for the same reason `answer.rs` used to build it by hand and
    // no longer does: a stranger chooses when this runs and how many run at
    // once, and a second copy of the ceiling is a second thing to forget to
    // change.
    let clients = dispatch::Clients {
        solana: ctx.client,
        robinhood: ctx.robinhood,
    };
    let dossier = match dispatch::read(&mint_text, &clients) {
        Ok(d) => d,
        Err(dispatch::Error::NotAnAddress) => return Answered::NotAnAddress,
        Err(dispatch::Error::Unreadable(why)) => return Answered::Unreadable(why),
    };

    let (sheet, reply) = realorrug_roast::roast(
        &dossier,
        ctx.rates,
        ctx.creators,
        ctx.provider,
        ctx.self_mint,
    );

    // Computed once. `read_at_slot` below is derived from this same value
    // rather than re-read from `dossier`, so the two fields can never
    // disagree with each other.
    let read_at = dossier.read_at;

    // Recorded here, where the verdict is computed for the published reply
    // -- not by reading it back out of the log afterwards, which would make
    // the log load-bearing for behaviour instead of for audit. A mention
    // with no conversation id (an ordinary DM-shaped call, a fixture with
    // the field omitted, or a platform response that dropped it) records
    // nothing: there is no thread to remember this reply against.
    if let Some(conversation) = &mention.conversation {
        threads.record(conversation, &mint_text, realorrug_roast::level(&sheet));
    }

    Answered::Reply {
        // Read before `reply.text` is moved below. `Billed` is `Copy`, so this
        // is not a borrow that has to outlive anything.
        billed: reply.billed,
        entry: Box::new(Entry {
            at: ctx.now,
            mention_id: mention.id.clone(),
            summoner: mention.author.clone(),
            mint: Some(mint_text),
            read_at,
            read_at_slot: read_at
                .and_then(realorrug_types::ReadAt::as_slot)
                .map(|s| s.0),
            // The evidence, not only the words. A log of replies without fact
            // sheets records what Radar said and not whether it was entitled to say
            // it, and the second is the half that settles an argument.
            fact_sheet: sheet.render(),
            reply: reply.text,
            fellback: reply.fellback.as_ref().map(|f| format!("{f:?}")),
            reply_id: None,
            // Counted where the sheet was built, carried here so the week-close
            // job scores from the record and never re-reads the chain.
            signals: Some(sheet.signals),
            pointed_at: None,
        }),
    }
}

/// A refusal, in words worth telling somebody.
#[must_use]
pub fn describe(why: &Refused) -> String {
    match why {
        Refused::Unconfigured => "no limits configured, so nothing is answered".to_owned(),
        Refused::SummonerDaily { cap } => format!("this account has had its {cap} replies today"),
        Refused::GlobalDaily { cap } => format!("the daily cap of {cap} replies is spent"),
        Refused::GlobalRate { per_hour } => {
            format!("answering as fast as it is allowed to — {per_hour} an hour; try again shortly")
        }
        Refused::AlreadyAnswered { reply_id } => {
            format!("already answered for this mint, see {reply_id}")
        }
        Refused::SelfOrIgnored => "Radar does not answer itself".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::Limits;

    fn mention(text: &str) -> Mention {
        Mention {
            id: "m1".to_owned(),
            author: "a1".to_owned(),
            text: text.to_owned(),
            parent: None,
            conversation: None,
        }
    }

    fn threads() -> crate::followup::ThreadMemory {
        crate::followup::ThreadMemory::new()
    }

    fn lane2_gate() -> crate::lane2::Gate {
        crate::lane2::Gate::unconfigured()
    }

    fn gate() -> Gate {
        Gate::new(
            Limits {
                per_summoner_daily: 3,
                global_daily: 50,
                dedupe_seconds: 3600,
            },
            vec!["radar".to_owned()],
        )
    }

    fn ctx(client: &RpcClient) -> Answering<'_> {
        Answering {
            client,
            robinhood: None,
            rates: None,
            creators: None,
            provider: None,
            self_mint: None,
            now: 1_788_000_000,
        }
    }

    /// A client pointed at an address nothing answers on.
    ///
    /// Every test below returns before the client is used. It exists so the
    /// context can be built, and a test that accidentally reached the network
    /// would fail rather than quietly depend on it.
    fn unreachable_client() -> RpcClient {
        RpcClient::new("http://127.0.0.1:1".to_owned())
    }

    #[test]
    fn a_symbol_is_answered_by_asking_for_the_address() {
        // Guessing which token a symbol meant is how measurements get published
        // about the wrong project.
        let client = unreachable_client();
        let out = answer(
            &mention("@radar what about $ABC"),
            &mut gate(),
            &mut threads(),
            &mut lane2_gate(),
            &ctx(&client),
        );
        match out {
            Answered::Ticker { text: reply, .. } => {
                assert!(reply.contains("$ABC"), "{reply}");
                assert!(reply.contains("contract address"), "{reply}");
            }
            other => panic!("a symbol must not be resolved: {other:?}"),
        }
    }

    #[test]
    fn a_mention_naming_nothing_is_not_an_error() {
        let client = unreachable_client();
        let out = answer(
            &mention("@radar hello"),
            &mut gate(),
            &mut threads(),
            &mut lane2_gate(),
            &ctx(&client),
        );
        assert!(matches!(out, Answered::Nothing), "{out:?}");
    }

    #[test]
    fn the_gate_refuses_before_the_chain_is_read() {
        // The ordering that matters for the bill: the read is the expensive
        // part, so a refusal must come first. With no limits configured the
        // gate refuses everything, and this returns without touching the
        // unreachable client -- which it would hang on for twenty seconds if
        // the order were wrong.
        let mut closed = Gate::new(
            Limits {
                per_summoner_daily: 0,
                global_daily: 0,
                dedupe_seconds: 0,
            },
            Vec::new(),
        );
        let client = unreachable_client();
        let out = answer(
            &mention("@radar So11111111111111111111111111111111111111112"),
            &mut closed,
            &mut threads(),
            &mut lane2_gate(),
            &ctx(&client),
        );
        assert!(matches!(out, Answered::Refused(_)), "{out:?}");
    }

    #[test]
    fn radar_does_not_answer_itself() {
        let client = unreachable_client();
        let mut g = gate();
        let mut m = mention("@radar So11111111111111111111111111111111111111112");
        m.author = "radar".to_owned();
        let out = answer(&m, &mut g, &mut threads(), &mut lane2_gate(), &ctx(&client));
        assert!(
            matches!(out, Answered::Refused(Refused::SelfOrIgnored)),
            "{out:?}"
        );
    }

    #[test]
    fn a_robinhood_address_is_not_answered_not_an_address() {
        // The exact bug this task fixes: a real `0x…` address must not come
        // back looking like it was never a token at all. With no Robinhood
        // endpoint configured the chain still cannot be read, but that is a
        // different, distinguishable answer.
        let client = unreachable_client();
        let out = answer(
            &mention("@radar 0x1111111111111111111111111111111111111111"),
            &mut gate(),
            &mut threads(),
            &mut lane2_gate(),
            &ctx(&client),
        );
        assert!(matches!(out, Answered::Unreadable(_)), "{out:?}");
    }

    #[test]
    fn a_robinhood_address_with_no_endpoint_never_attempts_a_solana_read() {
        // The Solana client here points at an address that would error (or
        // hang) if it were ever asked -- so a dispatcher that mistakenly fell
        // through to a Solana read would surface a different failure than
        // this one.
        let client = unreachable_client();
        let out = answer(
            &mention("@radar 0x1111111111111111111111111111111111111111"),
            &mut gate(),
            &mut threads(),
            &mut lane2_gate(),
            &ctx(&client),
        );
        let Answered::Unreadable(why) = out else {
            panic!("expected Unreadable, got {out:?}");
        };
        assert!(why.contains("Robinhood"), "{why}");
    }

    #[test]
    fn a_run_that_is_address_shaped_but_not_a_real_address_is_not_an_address() {
        // Base58-shaped and the right length by mention.rs's own rules, but
        // not a string that decodes to a 32-byte key -- neither chain's
        // parser accepts it, so this must still be `NotAnAddress`, not
        // `Unreadable`: nothing was configured wrong, the text simply never
        // named a token.
        let client = unreachable_client();
        let out = answer(
            &mention(&format!("@radar {}", "a".repeat(32))),
            &mut gate(),
            &mut threads(),
            &mut lane2_gate(),
            &ctx(&client),
        );
        assert!(matches!(out, Answered::NotAnAddress), "{out:?}");
    }

    #[test]
    fn every_refusal_is_described_in_words_worth_telling_somebody() {
        // The strings go in front of an operator reading a run. A refusal
        // rendered as a debug enum is a refusal nobody acts on.
        for why in [
            Refused::Unconfigured,
            Refused::SummonerDaily { cap: 3 },
            Refused::GlobalDaily { cap: 50 },
            Refused::AlreadyAnswered {
                reply_id: "r1".to_owned(),
            },
            Refused::SelfOrIgnored,
            Refused::GlobalRate { per_hour: 2 },
        ] {
            let text = describe(&why);
            // Each says something only it could say. Asserting "not empty and
            // starts with a letter" passed for every refusal rendered as the
            // same string, which mutation testing found by replacing the whole
            // function with one word.
            let distinctive = match why {
                Refused::Unconfigured => "no limits",
                Refused::SummonerDaily { .. } => "this account",
                Refused::GlobalDaily { .. } => "daily cap",
                Refused::AlreadyAnswered { .. } => "already answered",
                Refused::SelfOrIgnored => "itself",
                Refused::GlobalRate { .. } => "an hour",
            };
            assert!(
                text.contains(distinctive),
                "{text:?} should name why it refused, not merely refuse"
            );
        }
    }

    #[test]
    fn the_call_budget_is_bounded_on_every_axis() {
        // A stranger chooses when this runs and how many run at once, so an
        // unbounded axis is an unbounded read. Asserted against
        // `realorrug-onchain`'s own constants rather than numbers written here: a
        // test restating them would pass while the two drifted apart.
        let mut budget = realorrug_onchain::Budget::default();
        for i in 0..realorrug_onchain::budget::DEFAULT_MAX_CALLS {
            assert!(budget.take_call().is_ok(), "call {i} is within budget");
        }
        assert!(
            budget.take_call().is_err(),
            "the call after the ceiling must be refused"
        );

        let mut pages = realorrug_onchain::Budget::default();
        for i in 0..realorrug_onchain::budget::DEFAULT_MAX_PAGES {
            assert!(pages.take_page().is_ok(), "page {i} is within budget");
        }
        assert!(
            pages.take_page().is_err(),
            "the page after the ceiling must be refused"
        );
    }
}
