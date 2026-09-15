// SPDX-License-Identifier: Apache-2.0
//! One week's record: entries, scores, winner, claim, payout.
//!
//! # Published, one file per week
//!
//! Design 0007 C2 writes one JSON document per week and the public site reads
//! it, never the store. This is that document's shape. Everything a reader
//! needs to check the result is in it: the entries and their metrics, the
//! scores, every exclusion with its reason, the winner, the address they
//! claimed with, and the transaction that paid them.
//!
//! # The payout policy lives here, and it is three lines
//!
//! `realorrug-payout` signs from a hot key whose blast radius is one week of
//! creator fees (ADR 0013). What keeps it that small is [`Payout::permitted`]:
//! the recipient must be the address the ledger's winner claimed with, the
//! amount may not exceed what was collected, and a week is paid at most once.
//! Pure and tested here, so the binary calls a function rather than carrying
//! its own copy of the rule -- and so that each refusal can be proved by
//! re-applying the bug without a key in the room.

use serde::{Deserialize, Serialize};

use crate::score::{Ranking, Rules};
use crate::week::{SECONDS_PER_DAY, Week};

/// How long the winner has to reply with an address before the prize rolls
/// into the next week. Design 0007 section 6.2: seven days.
pub const CLAIM_WINDOW_SECONDS: u64 = 7 * SECONDS_PER_DAY;

/// The week's winner, as decided by the rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Winner {
    /// Who, as the numeric account id a mention carries.
    pub summoner: String,
    /// The reply that won.
    pub reply_id: String,
    /// Its score.
    pub score: u64,
    /// The handle, when the close read one.
    ///
    /// `Option` because it arrives from a platform call that can fail while the
    /// rest of the close succeeds, and because every record written before
    /// 2026-09-06 has no such field.
    ///
    /// `serde(default)` is **redundant on an `Option` and kept as a statement of
    /// intent**, which is worth saying plainly because the first draft of this
    /// comment claimed the attribute was load-bearing. It is not: serde already
    /// maps a missing `Option` field to `None`, established by probe rather than
    /// recalled. What it buys is a visible marker that this field is expected to
    /// be absent in older records, and it starts mattering the day somebody
    /// changes the type to a bare `String`.
    ///
    /// The hazard it marks is real, and it is finding S11: [`records_in`] skips
    /// a file it cannot parse, without warning and without failing, so a
    /// **required** field added to a record would make old weeks vanish from the
    /// leaderboard and from the cooldown that reads them — quietly freeing a
    /// past winner to win again. The test named
    /// `the_record_production_wrote_before_these_fields_existed_still_parses` is
    /// the enforcement; this attribute is only the note.
    #[serde(default)]
    pub handle: Option<String>,
}

/// The address the winner asked to be paid at, and the reply that said so.
///
/// The claim is a public reply from the winner's own account, parsed with the
/// same strict rule as a mention (design 0007 C3), which is why it carries the
/// reply id: the link between account and address is on the platform for
/// anyone to see.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    /// The winner's Robinhood Chain (EVM) address, as text: `0x` and 40 hex
    /// digits. Parsed and checked by the caller before it is written here; this
    /// crate stores what was accepted. The payout parses it again as
    /// `realorrug_robinhood::Address` and refuses anything else, a Solana
    /// address from before the move included, before anything is signed.
    pub address: String,
    /// The reply it was read from.
    pub reply_id: String,
    /// When, as seconds since the epoch.
    pub at: u64,
}

/// A payment that was made.
///
/// The money is [`Paid`], flattened, so a record's `payout` object carries the
/// recipient and time beside whichever amount and transaction fields its chain
/// has. A week paid on Solana and a week paid on Robinhood Chain are both
/// readable, and neither is renamed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payout {
    /// To whom: the claim's address, as the claim wrote it.
    pub recipient: String,
    /// How much, and the transactions that moved it.
    #[serde(flatten)]
    pub paid: Paid,
    /// When, as seconds since the epoch.
    pub at: u64,
}

/// What a payout moved, by chain.
///
/// **Untagged, and the field names are the tag.** A record written before the
/// move to Robinhood Chain has `lamports` and `signature` and nothing else, and
/// adding a tag field would make it a record `records_in` skips (finding S11).
/// `Eth` is tried first: its three fields are all required, so a Solana payout
/// never reads as one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Paid {
    /// Robinhood Chain: the escrow claim, then the transfer to the winner.
    Eth {
        /// What the winner received, which is what the escrow said was claimed.
        wei: Wei,
        /// The claim from the Pons fee escrow, `0x` and 64 hex digits.
        claim_tx: String,
        /// The transfer to the winner.
        transfer_tx: String,
    },
    /// Solana, from pump.fun's creator vault. **Read-only legacy**: nothing
    /// writes this arm any more, and it stays so that any week paid under it
    /// still reads.
    Sol {
        /// How much.
        lamports: u64,
        /// The transaction signature.
        signature: String,
    },
}

impl Payout {
    /// The transaction a reader checks: the transfer that paid the winner.
    #[must_use]
    pub fn transaction(&self) -> &str {
        match &self.paid {
            Paid::Eth { transfer_tx, .. } => transfer_tx,
            Paid::Sol { signature, .. } => signature,
        }
    }
}

/// An amount of wei, written as a decimal string.
///
/// **A string, not a JSON number**, for two reasons that each decide it alone.
/// JavaScript reads a number above 2^53 inexactly, and 2^53 wei is under
/// 0.01 ETH, so the site would print a prize that is not the one paid. And a
/// `u64` stops at about 18.4 ETH, which a busy week's fees can pass.
///
/// Read strictly: ASCII digits only, so `+5`, ` 5` and `5.0` are refused rather
/// than guessed at, and a JSON number is refused because whoever wrote it may
/// already have rounded it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Wei(pub u128);

impl Serialize for Wei {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Wei {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("not a decimal amount of wei: {text:?}"))
        })
    }
}

impl Wei {
    /// Decimal digits, and nothing else, that fit in `u128`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        text.parse().ok().map(Self)
    }
}

/// Why a payout is refused.
///
/// Amounts are in the chain's smallest unit: wei on Robinhood Chain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    /// Nobody won this week.
    NoWinner,
    /// The winner has not claimed, so there is no address to pay.
    Unclaimed,
    /// The recipient is not the address the winner claimed with.
    WrongRecipient,
    /// More than the week collected.
    AboveCollected {
        /// What was collected.
        collected: u128,
    },
    /// This week has already been paid.
    AlreadyPaid {
        /// The transaction that paid it: the transfer on Robinhood Chain, the
        /// signature on Solana.
        transaction: String,
    },
    /// The operator voided this week. It pays nobody and the pool rolls over.
    Voided {
        /// Their reason, as published.
        reason: String,
    },
    /// The claimed address is not a wallet.
    ///
    /// Defence in depth: `try_claim` already refuses one at claim time, where
    /// the winner can act on it. This is the second check, at the last moment
    /// before a signature, because a contract and a wallet are the same shape
    /// and what is on the other side of this is money leaving.
    NotAWallet {
        /// What is at the address instead, or `None` when it could not be read.
        owner: Option<String>,
    },
    /// The week collected less than the floor, so the pool rolls over.
    ///
    /// Design 0007 J2 and design 0009 L4 both say a floor with rollover, and
    /// the code had none until 2026-09-06. Without it a week that collected
    /// dust pays it out: the transaction fee is a meaningful share of the
    /// prize, the winner receives an amount not worth the click, and the pool
    /// that should have been building is spent.
    ///
    /// **Not an error.** The week stays unpaid and claimable-looking, the
    /// prize rolls into the next week, and the history page says so -- which
    /// is the same shape as `Unclaimed` and deliberately so.
    BelowFloor {
        /// The floor in force.
        floor: u128,
        /// What the week actually collected.
        collected: u128,
    },
}

/// A week the operator voided, and why.
///
/// # Why a lever exists at all
///
/// Design 0007 §6.2 promised that *"if the first winner is obviously bought,
/// the rule changes and the change is recorded."* Changing a rule takes a
/// deploy and an argument; the prize is claimable in seven days. Without this
/// the only options in that week are pay a farm or let the week look
/// unclaimed, and the second is a lie a reader cannot see through.
///
/// # Why it is visible rather than quiet
///
/// A voided week **says so on the page, with the operator's reason in their own
/// words**. Design 0011's whole argument is that an exclusion at payout is a
/// private correction to a public error; this is the same principle applied to
/// the one action that is unavoidably a judgement. It cannot be used to tidy a
/// week away, because using it publishes the fact that it was used.
///
/// It does not rewrite the ranking. The scores stand, the evidence stands, the
/// winner is still named — the week simply pays nobody and the pool rolls over.
/// A reader who disagrees can see exactly what the operator saw.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Voided {
    /// When the operator voided it, seconds since the epoch.
    pub at: u64,
    /// Why, in the operator's own words. Published verbatim.
    pub reason: String,
}

/// Where the prize waits, as last read from the chain.
///
/// Written by the payout run, which reads it anyway, and read by the public
/// pool page. Absent means **no token yet**, which the page renders as a
/// sentence and never as a balance of zero.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vault {
    /// The contract holding it: the Pons fee escrow, or on Solana the creator
    /// vault.
    pub address: String,
    /// Its balance when read.
    #[serde(flatten)]
    pub balance: Balance,
    /// When it was read, seconds since the epoch.
    pub measured_at: u64,
}

/// A vault's balance, by chain. Untagged for the reason [`Paid`] is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Balance {
    /// What the Pons escrow holds for the payout wallet.
    Eth {
        /// The address the escrow holds it for: the creator fee recipient,
        /// which is the payout wallet.
        holder: String,
        /// How much.
        wei: Wei,
    },
    /// Solana's creator vault. Read-only legacy.
    Sol {
        /// How much.
        lamports: u64,
    },
}

impl Vault {
    /// The JSON the site reads.
    ///
    /// # Errors
    ///
    /// Only if a value cannot be serialised, which no value here can fail to be.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// A reading read back.
    ///
    /// # Errors
    ///
    /// When the text is not a vault reading.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

/// One week's record, as published.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// Which week.
    pub week: Week,
    /// When it opened, seconds since the epoch. Derived from `week`; written
    /// out so a reader of the JSON does not have to know the arithmetic.
    pub opened_at: u64,
    /// When it closed.
    pub closed_at: u64,
    /// The ranking, entries and exclusions both.
    pub ranking: Ranking,
    /// The winner, if anything counted.
    pub winner: Option<Winner>,
    /// The post the winner must reply to in order to claim.
    ///
    /// Written back after the week closes, once the account has posted it under
    /// its own winning reply (design 0007 §6.2). `None` means the prompt has not
    /// been posted — in a dry run it never is, and no claim can be made, which
    /// is correct, because no winning reply was published for anyone to see
    /// either.
    ///
    /// **This field is what stops a coin's mint address being paid the prize.**
    /// Before it existed, `try_claim` accepted any mint-shaped string in any
    /// mention by the winner inside the claim window, and a mint is such a
    /// string — so a winner who summoned the bot about a coin during their own
    /// claim week had that coin's mint recorded as their payout address, and
    /// `Payout::permitted` would have approved paying it. See `try_claim`.
    #[serde(default)]
    pub claim_prompt: Option<String>,
    /// The winner's claim, once made.
    pub claim: Option<Claim>,
    /// The operator voided this week, and why.
    ///
    /// `None` is the ordinary case. `Some` means the week pays nobody whatever
    /// the ranking says, and the reason is published beside the evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voided: Option<Voided>,
    /// The rule this week was scored under.
    ///
    /// **Written into the record rather than looked up**, because the rule
    /// changes and a closed week has to stay checkable against the rule that
    /// actually decided it. A reader asking why an entry placed where it did
    /// gets an answer from this file alone.
    ///
    /// `None` on a week closed before 2026-09-06, when nothing recorded it.
    /// That is unknown rather than "the current rule", and the history page
    /// says so: rule 9, and the difference matters to somebody disputing a
    /// placing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<Rules>,
    /// The payment, once made.
    pub payout: Option<Payout>,
}

impl Record {
    /// A closed week's record from its ranking, unclaimed and unpaid.
    #[must_use]
    pub fn close(week: Week, ranking: Ranking, rule: &Rules) -> Self {
        let winner = ranking.winner().map(|r| Winner {
            summoner: r.entry.summoner.clone(),
            reply_id: r.entry.reply_id.clone(),
            score: r.score,
            handle: r.entry.handle.clone(),
        });
        Self {
            week,
            opened_at: week.opens_at(),
            closed_at: week.closes_at(),
            ranking,
            winner,
            claim_prompt: None,
            claim: None,
            voided: None,
            rule: Some(rule.clone()),
            payout: None,
        }
    }

    /// The last moment a claim is accepted.
    ///
    /// After this the prize rolls into the next week (design 0007 J2 and
    /// section 6.2). Exclusive, like a week's close.
    #[must_use]
    pub const fn claim_window_closes_at(&self) -> u64 {
        self.closed_at + CLAIM_WINDOW_SECONDS
    }

    /// Whether a claim made at `now` is in time.
    #[must_use]
    pub const fn accepts_claim_at(&self, now: u64) -> bool {
        now >= self.closed_at && now < self.claim_window_closes_at()
    }

    /// The JSON the site reads.
    ///
    /// # Errors
    ///
    /// Only if a value cannot be serialised, which no value here can fail to be.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// A record read back.
    ///
    /// # Errors
    ///
    /// When the text is not a record. A torn or hand-edited file is refused
    /// rather than partly read: the ledger is the evidence, and half of it is
    /// not evidence.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

/// Every week record in a directory, in no particular order.
///
/// A record is a file named `<week>.json` where `<week>` is the week number;
/// both halves of the name are required, so a backup copy, a notes file or a
/// numbered file with no extension is not a record. A file that does not parse
/// is skipped rather than failing the read: a torn write is not evidence, and
/// the weeks either side of it still are.
///
/// The one place this crate touches a disk, and only to read what it wrote.
#[must_use]
pub fn records_in(dir: &std::path::Path) -> Vec<Record> {
    let Ok(listing) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    listing
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".json") && name.trim_end_matches(".json").parse::<u64>().is_ok()
        })
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .filter_map(|text| Record::from_json(&text).ok())
        .collect()
}

impl Payout {
    /// Whether paying `amount` to `recipient` for this week is permitted.
    ///
    /// The three lines that bound the hot key's blast radius, in the order
    /// they are cheapest to check. `collected` is what the week's creator fee
    /// amounted to, read from the chain by the caller. Every amount is in wei.
    ///
    /// # Errors
    ///
    /// The refusal, which the caller records and never argues with.
    pub fn permitted(
        record: &Record,
        recipient: &str,
        amount: u128,
        collected: u128,
        floor: u128,
    ) -> Result<(), Refusal> {
        if let Some(paid) = &record.payout {
            return Err(Refusal::AlreadyPaid {
                transaction: paid.transaction().to_owned(),
            });
        }
        // Checked before the winner, the claim and the amount, because a
        // voided week is not a week with a problem to work around -- it is a
        // week the operator has already decided pays nobody. Reporting
        // `Unclaimed` for it would send somebody looking for a claim.
        if let Some(voided) = &record.voided {
            return Err(Refusal::Voided {
                reason: voided.reason.clone(),
            });
        }
        if record.winner.is_none() {
            return Err(Refusal::NoWinner);
        }
        let Some(claim) = &record.claim else {
            return Err(Refusal::Unclaimed);
        };
        if claim.address != recipient {
            return Err(Refusal::WrongRecipient);
        }
        if amount > collected {
            return Err(Refusal::AboveCollected { collected });
        }
        // Last, and deliberately: every refusal above is about whether this
        // week may be paid at all, and this one is only about whether it is
        // worth paying yet. Checking it earlier would report "below the floor"
        // for a week that is voided, unpaid twice, or claimed by the wrong
        // address -- three answers that are more urgent and more true.
        if collected < floor {
            return Err(Refusal::BelowFloor { floor, collected });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::{Entry, Metrics, Ranked};

    /// No floor: what these tests are about is the other five refusals, and a
    /// floor would make every one of them depend on an amount that is not the
    /// thing under test.
    const NO_FLOOR: u128 = 0;

    const WEEK: Week = Week(2958);

    #[test]
    fn the_floor_is_below_and_not_at_or_below_and_not_equal_to() {
        // Three operators produce three different rules and CI proved two of
        // them survive: `<=` withholds a prize from a week that reached the
        // bar exactly, and `==` refuses only the single amount equal to the
        // floor and pays everything under it -- the opposite of a floor.
        //
        // The three amounts either side are what tell them apart, and the
        // realorrug-payout tests could not: they are in another crate, and
        // `cargo mutants` runs each crate's own suite.
        let record = claimed();
        let floor = 1_000u128;
        let at = |collected: u128| Payout::permitted(&record, "ADDR", collected, collected, floor);

        assert!(
            matches!(at(floor - 1), Err(Refusal::BelowFloor { .. })),
            "a lamport short is below the floor"
        );
        // At the floor pays. `<=` fails here.
        assert_eq!(at(floor), Ok(()), "exactly at the floor is not below it");
        // And above it pays. `==` fails here only in company with the first
        // assertion, which is why all three amounts are asserted.
        assert_eq!(at(floor + 1), Ok(()));
        // Zero collected is below any floor, and this is the case an operator
        // meets every Monday morning right after a payout.
        assert!(matches!(
            at(0),
            Err(Refusal::BelowFloor { collected: 0, .. })
        ));
    }

    #[test]
    fn a_voided_week_pays_nobody_and_says_so_before_anything_else() {
        // Checked ahead of the winner, the claim and the amount. A voided week
        // is not a week with a problem to work around -- the operator has
        // already decided it pays nobody -- and reporting `Unclaimed` for one
        // would send somebody looking for a claim that is not the point.
        //
        // Re-apply by moving the check below the claim: an unclaimed voided
        // week reports `Unclaimed` and the reason never reaches anybody.
        let mut record = Record::close(WEEK, ranking_with_winner(), &Rules::published(["op"]));
        record.voided = Some(Voided {
            at: 1_788_000_000,
            reason: "every point came from six accounts made that morning".to_owned(),
        });

        // No claim at all: still Voided, not Unclaimed.
        match Payout::permitted(&record, "somebody", 1, 1_000, NO_FLOOR) {
            Err(Refusal::Voided { reason }) => {
                assert!(reason.contains("six accounts"), "{reason}");
            }
            other => panic!("expected Voided, got {other:?}"),
        }

        // And with a perfectly good claim, it still pays nobody.
        record.claim = Some(Claim {
            address: "somebody".to_owned(),
            at: 1_788_000_100,
            reply_id: "r9".to_owned(),
        });
        assert!(matches!(
            Payout::permitted(&record, "somebody", 1, 1_000, NO_FLOOR),
            Err(Refusal::Voided { .. })
        ));
    }

    #[test]
    fn a_week_that_was_already_paid_reports_that_rather_than_the_void() {
        // Order between the two. Money that already left is the more useful
        // fact, and it is the one that cannot be undone by editing a file.
        let mut record = Record::close(WEEK, ranking_with_winner(), &Rules::published(["op"]));
        record.payout = Some(Payout {
            recipient: "somebody".to_owned(),
            paid: eth(10, "0xclaim", "0xsig"),
            at: 1_788_000_000,
        });
        record.voided = Some(Voided {
            at: 1_788_000_100,
            reason: "too late".to_owned(),
        });
        assert!(matches!(
            Payout::permitted(&record, "somebody", 1, 1_000, NO_FLOOR),
            Err(Refusal::AlreadyPaid { .. })
        ));
    }

    #[test]
    fn a_record_written_before_the_veto_existed_is_not_voided() {
        // Rule 9's direction here is the cheap one: absent means the operator
        // never voided it, which is true of every week closed before today.
        let old = Record::close(WEEK, Ranking::default(), &Rules::published(["op"]));
        let json = old.to_json().expect("json");
        assert!(
            !json.contains("voided"),
            "an unvoided week does not carry an empty field: {json}"
        );
        let back: Record = serde_json::from_str(&json).expect("reads");
        assert_eq!(back.voided, None);
    }

    #[test]
    fn only_a_numbered_json_file_in_the_directory_is_a_record() {
        // Both halves of the name rule. CI's mutants turned the `&&` into `||`
        // and a numbered file with no extension, holding a valid record,
        // became a week; the leaderboard would have shown it.
        let dir = std::env::temp_dir().join(format!("radar-ledger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let write = |name: &str, week: Week| {
            std::fs::write(
                dir.join(name),
                Record::close(week, Ranking::default(), &Rules::published(["op"]))
                    .to_json()
                    .expect("json"),
            )
            .expect("write");
        };
        write("2957.json", Week(2957));
        write("2958", Week(2958));
        write("2959.json.bak", Week(2959));
        std::fs::write(dir.join("notes.json"), "{}").expect("write");
        let weeks: Vec<u64> = records_in(&dir).into_iter().map(|r| r.week.0).collect();
        assert_eq!(weeks, [2957]);
        assert!(
            records_in(&dir.join("nowhere")).is_empty(),
            "a missing directory is no records"
        );
    }

    fn ranking_with_winner() -> Ranking {
        Ranking {
            ranked: vec![Ranked {
                entry: Entry {
                    reply_id: "r1".to_owned(),
                    summoner: "alice".to_owned(),
                    mention_id: None,
                    handle: Some("alice_h".to_owned()),
                    mint: "M".to_owned(),
                    at: WEEK.opens_at() + 10,
                    metrics: Metrics {
                        likes: 4,
                        ..Metrics::default()
                    },
                },
                score: 4,
            }],
            excluded: Vec::new(),
        }
    }

    fn claimed() -> Record {
        let mut record = Record::close(WEEK, ranking_with_winner(), &Rules::published(["op"]));
        record.claim = Some(Claim {
            address: "ADDR".to_owned(),
            reply_id: "r2".to_owned(),
            at: WEEK.closes_at() + 60,
        });
        record
    }

    #[test]
    fn closing_a_week_names_the_winner_and_leaves_it_unclaimed_and_unpaid() {
        let record = Record::close(WEEK, ranking_with_winner(), &Rules::published(["op"]));
        assert_eq!(
            record.winner,
            Some(Winner {
                summoner: "alice".to_owned(),
                reply_id: "r1".to_owned(),
                score: 4,
                // Carried from the entry, so the leaderboard can print a name
                // rather than a numeric id.
                handle: Some("alice_h".to_owned()),
            })
        );
        assert_eq!(record.opened_at, WEEK.opens_at());
        assert_eq!(record.closed_at, WEEK.closes_at());
        assert!(record.claim.is_none());
        assert!(record.payout.is_none());

        // And a week where nothing counted has no winner, not a default one.
        let empty = Record::close(WEEK, Ranking::default(), &Rules::published(["op"]));
        assert!(empty.winner.is_none());
    }

    #[test]
    fn the_claim_window_is_seven_days_from_the_close_and_not_a_second_more() {
        let record = Record::close(WEEK, ranking_with_winner(), &Rules::published(["op"]));
        assert!(
            !record.accepts_claim_at(record.closed_at - 1),
            "the week is still open"
        );
        assert!(record.accepts_claim_at(record.closed_at));
        assert!(record.accepts_claim_at(record.claim_window_closes_at() - 1));
        assert!(
            !record.accepts_claim_at(record.claim_window_closes_at()),
            "rolled over"
        );
        assert_eq!(
            record.claim_window_closes_at() - record.closed_at,
            7 * SECONDS_PER_DAY
        );
    }

    #[test]
    fn the_three_refusals_each_fire_and_a_correct_payout_is_permitted() {
        // Each refusal re-applied as a bug fails exactly one assertion here:
        // drop the recipient comparison and `WrongRecipient` is not returned;
        // change `>` to `>=` on the amount and paying exactly what was
        // collected is refused; drop the paid check and a week pays twice.
        let record = claimed();
        assert_eq!(
            Payout::permitted(&record, "ADDR", 1_000, 1_000, NO_FLOOR),
            Ok(())
        );
        assert_eq!(
            Payout::permitted(&record, "ADDR", 999, 1_000, NO_FLOOR),
            Ok(())
        );
        assert_eq!(
            Payout::permitted(&record, "MALLORY", 1_000, 1_000, NO_FLOOR),
            Err(Refusal::WrongRecipient)
        );
        assert_eq!(
            Payout::permitted(&record, "ADDR", 1_001, 1_000, NO_FLOOR),
            Err(Refusal::AboveCollected { collected: 1_000 })
        );

        let mut paid = claimed();
        paid.payout = Some(Payout {
            recipient: "ADDR".to_owned(),
            paid: eth(1_000, "CLAIM", "SIG"),
            at: 1,
        });
        // The transfer is named, not the claim: it is the transaction that
        // paid the winner, and the one a reader looks up.
        assert_eq!(
            Payout::permitted(&paid, "ADDR", 1_000, 1_000, NO_FLOOR),
            Err(Refusal::AlreadyPaid {
                transaction: "SIG".to_owned()
            })
        );
    }

    fn eth(wei: u128, claim_tx: &str, transfer_tx: &str) -> Paid {
        Paid::Eth {
            wei: Wei(wei),
            claim_tx: claim_tx.to_owned(),
            transfer_tx: transfer_tx.to_owned(),
        }
    }

    #[test]
    fn the_floor_and_the_cap_hold_at_amounts_no_u64_can_carry() {
        // A week's fees can pass 18.4 ETH, which is where `u64` wei stops.
        // Re-apply by narrowing `permitted` to `u64` with `as` casts: the
        // amounts either side of 2^64 wrap and both assertions below flip.
        let record = claimed();
        let big = u128::from(u64::MAX) + 5;
        assert_eq!(Payout::permitted(&record, "ADDR", big, big, big), Ok(()));
        assert_eq!(
            Payout::permitted(&record, "ADDR", big + 1, big, 0),
            Err(Refusal::AboveCollected { collected: big })
        );
        assert_eq!(
            Payout::permitted(&record, "ADDR", big - 1, big - 1, big),
            Err(Refusal::BelowFloor {
                floor: big,
                collected: big - 1
            })
        );
    }

    #[test]
    fn wei_is_a_decimal_string_read_strictly() {
        // JSON numbers above 2^53 are inexact in the browser, so wei is text.
        // Re-apply by deriving `Serialize` on `Wei`: the first assertion sees a
        // bare number.
        let at_scale = Wei(4_014_961_601_594_189_201);
        assert_eq!(
            serde_json::to_string(&at_scale).expect("json"),
            "\"4014961601594189201\""
        );
        assert_eq!(
            serde_json::from_str::<Wei>("\"4014961601594189201\"").expect("reads"),
            at_scale
        );
        assert_eq!(Wei::parse(&u128::MAX.to_string()), Some(Wei(u128::MAX)));
        for bad in [
            "",
            "+5",
            " 5",
            "5.0",
            "-1",
            "0x10",
            "340282366920938463463374607431768211456",
        ] {
            assert_eq!(Wei::parse(bad), None, "{bad:?}");
        }
        assert!(
            serde_json::from_str::<Wei>("5").is_err(),
            "a number may already have been rounded"
        );
    }

    #[test]
    fn a_week_paid_on_either_chain_reads_back_as_the_chain_that_paid_it() {
        // Both shapes, as bytes. A Solana payout from before the move carries
        // `lamports` and `signature`; a Robinhood payout carries `wei`,
        // `claim_tx` and `transfer_tx`. Each must read, and read as its own arm.
        //
        // Re-apply by making `wei` a `u128` number: the Eth record below does
        // not parse and `records_in` would skip the week (S11). Re-apply by
        // tagging the enum: the Solana record stops parsing.
        let sol = r#"{"week":2956,"opened_at":1787529600,"closed_at":1788134400,
            "ranking":{"ranked":[],"excluded":[]},"winner":null,"claim":null,
            "payout":{"recipient":"So11111111111111111111111111111111111111112",
                      "lamports":3000000000,"signature":"5sig","at":1788134520}}"#;
        let record = Record::from_json(sol).expect("a Solana payout still reads");
        let paid = record.payout.expect("paid");
        assert_eq!(
            paid.paid,
            Paid::Sol {
                lamports: 3_000_000_000,
                signature: "5sig".to_owned()
            }
        );
        assert_eq!(paid.transaction(), "5sig");

        let eth_json = r#"{"week":2960,"opened_at":1789948800,"closed_at":1790553600,
            "ranking":{"ranked":[],"excluded":[]},"winner":null,"claim":null,
            "payout":{"recipient":"0x6aa025a3292c4ab6a55af3b6a7f7cbf62a5c4d06",
                      "wei":"4014961601594189201",
                      "claim_tx":"0x07cab768bdf8dcf67edc9b0bc74d9d2d70cdd4f88b4cbe8ada2b6ec85f44fa7b",
                      "transfer_tx":"0x11","at":1790553700}}"#;
        let record = Record::from_json(eth_json).expect("an Eth payout reads");
        let paid = record.payout.clone().expect("paid");
        assert_eq!(
            paid.paid,
            eth(
                4_014_961_601_594_189_201,
                "0x07cab768bdf8dcf67edc9b0bc74d9d2d70cdd4f88b4cbe8ada2b6ec85f44fa7b",
                "0x11"
            )
        );
        assert_eq!(paid.transaction(), "0x11");
        // And it writes back flat, with the amount as a string.
        let json = record.to_json().expect("json");
        assert!(json.contains("\"wei\": \"4014961601594189201\""), "{json}");
        assert!(!json.contains("lamports"), "{json}");
        assert_eq!(Record::from_json(&json).expect("round-trips"), record);
    }

    #[test]
    fn nothing_is_paid_without_a_winner_or_without_a_claim() {
        let unclaimed = Record::close(WEEK, ranking_with_winner(), &Rules::published(["op"]));
        assert_eq!(
            Payout::permitted(&unclaimed, "ADDR", 1, 1, NO_FLOOR),
            Err(Refusal::Unclaimed)
        );

        let nobody = Record::close(WEEK, Ranking::default(), &Rules::published(["op"]));
        assert_eq!(
            Payout::permitted(&nobody, "ADDR", 1, 1, NO_FLOOR),
            Err(Refusal::NoWinner)
        );
    }

    #[test]
    fn the_paid_check_comes_first_so_a_paid_week_is_never_reported_as_anything_else() {
        // A second payout attempt with a wrong recipient must say "already
        // paid", not "wrong recipient": the first is the fact that stops an
        // operator retrying with a corrected address.
        let mut paid = claimed();
        paid.payout = Some(Payout {
            recipient: "ADDR".to_owned(),
            paid: Paid::Sol {
                lamports: 1,
                signature: "SIG".to_owned(),
            },
            at: 1,
        });
        assert!(matches!(
            Payout::permitted(&paid, "MALLORY", 1, 1, NO_FLOOR),
            Err(Refusal::AlreadyPaid { .. })
        ));
    }

    #[test]
    fn the_vault_reading_round_trips_and_half_of_it_is_refused() {
        let vault = Vault {
            address: "0xd3afeb2a57f70ef218aa82451c51b2fb0416ac9e".to_owned(),
            balance: Balance::Eth {
                holder: "0x6aa025a3292c4ab6a55af3b6a7f7cbf62a5c4d06".to_owned(),
                wei: Wei(4_014_961_601_594_189_201),
            },
            measured_at: 1_788_000_000,
        };
        let json = vault.to_json().expect("serialises");
        assert!(json.contains("\"wei\": \"4014961601594189201\""), "{json}");
        assert_eq!(Vault::from_json(&json).expect("round-trips"), vault);
        assert!(Vault::from_json(&json[..json.len() / 2]).is_err());

        // A Solana reading from before the move still reads as one.
        let old = Vault::from_json(r#"{"address":"VAULT","lamports":5000,"measured_at":1}"#)
            .expect("the legacy shape reads");
        assert_eq!(old.balance, Balance::Sol { lamports: 5_000 });
    }

    #[test]
    fn the_record_round_trips_through_the_json_the_site_reads() {
        let mut record = claimed();
        record.ranking.excluded.push((
            Entry {
                reply_id: "r9".to_owned(),
                summoner: "radar".to_owned(),
                mention_id: None,
                handle: None,
                mint: "M".to_owned(),
                at: WEEK.opens_at() + 5,
                metrics: Metrics::default(),
            },
            crate::score::Excluded::Operator,
        ));
        let json = record.to_json().expect("serialises");
        assert!(json.contains("\"week\": 2958"), "{json}");
        assert!(
            json.contains("Operator"),
            "the exclusion and its reason are published: {json}"
        );
        let back = Record::from_json(&json).expect("round-trips");
        assert_eq!(back, record);

        // Half a file is not evidence.
        assert!(Record::from_json(&json[..json.len() / 2]).is_err());
    }
    #[test]
    fn the_record_production_wrote_before_these_fields_existed_still_parses() {
        // The exact bytes of `~/radar/data/contest/2956.json`, read off the
        // production box on 2026-09-06. It was written before `claim_prompt`
        // and `handle` existed, and it is the only closed week there is.
        //
        // This is the S11 regression and the failure it guards against is
        // silent. `records_in` SKIPS a file it cannot parse -- it does not warn
        // and it does not fail -- so a *required* field added to this struct
        // would make old weeks disappear from the leaderboard and, worse, from
        // the cooldown that reads every earlier record to decide who is still
        // serving one. A past winner would quietly become eligible again.
        //
        // Re-applied and confirmed: adding a required `probe_required: u64` to
        // `Record` fails exactly this test with
        // `missing field `probe_required``, 28 passed and 1 failed, while
        // nothing else in the crate notices.
        //
        // Deleting the `#[serde(default)]` above does NOT fail it, and finding
        // that out beat assuming it: serde already maps a missing `Option`
        // field to `None`. The attribute is a note about intent. This test is
        // the enforcement.
        const AS_PRODUCTION_WROTE_IT: &str = r#"{
  "week": 2956,
  "opened_at": 1787529600,
  "closed_at": 1788134400,
  "ranking": {
    "ranked": [],
    "excluded": []
  },
  "winner": null,
  "claim": null,
  "payout": null
}"#;
        let record = Record::from_json(AS_PRODUCTION_WROTE_IT)
            .expect("a record written before the new fields existed still parses");
        assert_eq!(record.week, Week(2956));
        assert_eq!(record.opened_at, 1_787_529_600);
        assert_eq!(record.closed_at, 1_788_134_400);
        assert!(record.winner.is_none());
        // Absent, and absent is the value that lets no claim through.
        assert!(record.claim_prompt.is_none());
    }

    #[test]
    fn an_entry_written_before_handles_existed_still_parses() {
        // The same guarantee one level down. An entry is nested inside a
        // record, so a required field added here takes the whole week with it.
        let entry: crate::score::Entry = serde_json::from_str(
            r#"{"reply_id":"r1","summoner":"123","mint":"M","at":10,
                "metrics":{"reposts":0,"quotes":0,"likes":0,"replies":0}}"#,
        )
        .expect("an entry without a handle parses");
        assert_eq!(entry.handle, None);
    }
}
