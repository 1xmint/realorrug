// SPDX-License-Identifier: Apache-2.0
//! Every creator's record, precomputed once so a reply can look one up.
//!
//! # Why this exists
//!
//! The public analyst answers about a coin while the thread it was asked in is
//! still alive. It reads the chain on demand for the things the chain can
//! answer — the launch block, the curve — and that works because those are one
//! block and one account.
//!
//! *"How did this creator's other tokens turn out"* is not answerable that way.
//! It is a question about 529,000 recorded launches joined to 1.4 million
//! outcome measurements, and `creator_track_record` answers it by decoding both
//! tables in full: ten seconds and climbing, on a shared two-core box, for one
//! mention.
//!
//! So it is precomputed. The same shape `docs/research/data/0024-base-rates.json`
//! already uses: a file with the date it was measured, read in microseconds,
//! refused rather than guessed at when it is absent.
//!
//! # What it is for
//!
//! Without it, every reply is the same reply. Three different coins measured on
//! 2026-09-04 produced identical text — the cost line, the recipient count, the
//! band — because nothing in the fact sheet was about *that* coin. A creator's
//! record is the fact that differs, and it is the one Radar has that nobody else
//! does: 117,390 creators, watched since August.
//!
//! # Where the halves live
//!
//! This is the **reading** half: the type, the lookup, and the file. Building it
//! needs the store, which the analyst deliberately does not have on its path, so
//! `radar_research::creator_index` owns that and writes this shape.
//!
//! The same split `BaseRates` uses, for the same reason: the consumer owns the
//! type it depends on, and the producer is free to be as heavy as it needs.
//!
//! # What it deliberately does not carry
//!
//! No rates and no verdicts, only counts. A rate computed here would be a rate
//! computed twice — `creator_track_record` already has one, with a minimum
//! sample and a `sample_note` explaining itself — and two of them would drift.
//! The consumer decides what a count means, and refuses to say anything when the
//! sample is too small.
//!
//! And **graduation is split**. A curve bought out within three slots of launch
//! was bought by capital committed before the token existed, so it is evidence
//! of coordination rather than demand. A creator ranked on the undifferentiated
//! count is ranked partly on how well they bundle. Robinhood Chain has blocks,
//! not slots; `realorrug-cli/src/creator_index.rs` reads "three slots" as three
//! *blocks* of the factory's `Graduated` event landing after `TokenLaunched`,
//! because the underlying claim — capital already in place beats capital that
//! has to arrive — does not depend on which chain is doing the ordering.
//!
//! **Stillborn is Robinhood's own definition, not Solana's.** The Solana
//! measurement this module was written against never got a calibrated rule (see
//! `docs/research/0038-pons-v2-creators-and-outcomes-read-over-a-range.md` §4);
//! what it sketched — few or zero buys *and* sells in a wall-clock window — does
//! not map onto an Ethereum-style log, which carries no timestamp and where a
//! sell presupposes a buy that already happened. Robinhood's outcome pass
//! instead calls a curve stillborn when it has **no `CurveBuy` in any block
//! after its launch block**: cheap to answer from logs alone (one pass over
//! every curve's buys, keeping only the address, not the log), and it still
//! means the thing "stillborn" is supposed to mean — nobody, ever, chose to buy
//! this token once the moment of launch had passed.

use std::collections::BTreeMap;

/// Where the index lives by default.
///
/// Beside the base rates, and read the same way: a published measurement with
/// the slot it was taken at, refused rather than guessed at when absent.
pub const DEFAULT_PATH: &str = "docs/research/data/creator-index.json";

/// One creator's record, as counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Record {
    /// Launches recorded at or before the watermark.
    pub launches: u32,
    /// Launches for which an outcome has been measured.
    ///
    /// At most `launches`. A gap means the outcome pass has not caught up, not
    /// that those tokens did nothing — which is why both numbers are kept and a
    /// consumer must quote the second when it quotes a share.
    pub measured: u32,
    /// Measured tokens whose curve filled over time rather than in a block.
    pub organic: u32,
    /// Measured tokens whose curve completed within three blocks of launch --
    /// see the module doc's "graduation is split" section.
    pub instant: u32,
    /// Measured tokens with no `CurveBuy` in any block after the launch
    /// block -- Robinhood's own definition; see the module doc's "stillborn"
    /// section.
    pub stillborn: u32,
}

/// The whole population, at the same watermark as the records.
///
/// # Why this rides along rather than being measured separately
///
/// It is the sum of the records, and computing it in the same pass is what makes
/// it *consistent with* them: a population measured by a second scan could
/// disagree with the parts it is supposed to be the total of, and the reply
/// would quote both.
///
/// It also costs nothing. The pass that builds the index has already visited
/// every launch and every outcome; these are five additions per row.
///
/// # Why it is worth having at all
///
/// `docs/research/data/0024-base-rates.json` carries two figures of the same
/// shape, and they came from **outside**: a public RPC walking 45 slots and a
/// SQL endpoint that truncates silently at a thousand rows. Both are samples of
/// a window. This is the population Radar actually recorded — every succeeded
/// launch at the watermark — measured offline, from the store, deterministically.
///
/// That does not make the snapshot wrong or replaceable. The snapshot carries
/// the **recipient distribution**, which needs the launch block and which the
/// store did not record until ADR 0012. These two overlap on exactly two
/// figures, and on those two this is the better instrument.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Population {
    /// Succeeded launches recorded at or before the watermark.
    pub launches: u64,
    /// Of those, how many have had an outcome measured.
    pub measured: u64,
    /// Measured tokens whose curve filled over time.
    pub organic: u64,
    /// Measured tokens whose curve completed within three blocks of launch --
    /// see the module doc's "graduation is split" section.
    pub instant: u64,
    /// Measured tokens with no `CurveBuy` in any block after the launch
    /// block -- Robinhood's own definition; see the module doc's "stillborn"
    /// section.
    pub stillborn: u64,
}

impl Population {
    /// Share of measured tokens that filled their curve over time.
    ///
    /// `None` when nothing has been measured. **Not zero** — rule 9, and this is
    /// the direction that flatters: "0% of launches graduate" read off an empty
    /// denominator is a measurement of the outcome pass, published as a fact
    /// about the venue.
    #[must_use]
    pub fn organic_share(&self) -> Option<f64> {
        self.share(self.organic)
    }

    /// Share of measured tokens whose curve completed inside three slots.
    ///
    /// `None` when nothing has been measured; see [`Self::organic_share`].
    #[must_use]
    pub fn instant_share(&self) -> Option<f64> {
        self.share(self.instant)
    }

    /// Share of measured tokens that graduated at all, by either route.
    ///
    /// `None` when nothing has been measured; see [`Self::organic_share`].
    #[must_use]
    pub fn graduated_share(&self) -> Option<f64> {
        self.share(self.organic + self.instant)
    }

    /// Share of measured tokens that showed almost no life.
    ///
    /// `None` when nothing has been measured; see [`Self::organic_share`].
    #[must_use]
    pub fn stillborn_share(&self) -> Option<f64> {
        self.share(self.stillborn)
    }

    /// A share of the **measured** population, never of the recorded one.
    ///
    /// The denominator is `measured` rather than `launches` deliberately. The
    /// gap between them is how far behind the outcome pass is, and dividing by
    /// `launches` would fold Radar's own lag into a claim about the venue —
    /// understating every graduation rate by exactly the size of the backlog.
    fn share(&self, part: u64) -> Option<f64> {
        // Precision: `u64 as f64` is lossless below 2^53 and these are counts of
        // launches, six orders of magnitude short of it.
        #[expect(
            clippy::cast_precision_loss,
            reason = "counts of launches; 2^53 is six orders of magnitude away"
        )]
        (self.measured > 0).then(|| part as f64 / self.measured as f64)
    }
}

/// The chain an index written before [`CreatorIndex::chain`] existed describes.
///
/// Solana, because every index that predates the field was built by
/// `radar_research::creator_index` from pump.fun launches. One such file's
/// summary was still being served from the production box on 2026-09-17
/// (`docs/research/data/population.json`, watermark slot 447,301,081, 778,593
/// launches). Defaulting to Robinhood would relabel those pump.fun launches as
/// Pons v2's — the exact wrong-chain claim the field exists to stop.
const fn chain_before_the_field_existed() -> crate::firstparty::Chain {
    crate::firstparty::Chain::Solana
}

/// Every creator's record at one watermark.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CreatorIndex {
    /// Which chain's launches this index describes.
    ///
    /// # Why a file has to say this
    ///
    /// There is one index path, and two chains that can fill it. The consumer
    /// cannot tell them apart from the contents: a record is five counts, and
    /// the population totals are five more. So a Pons v2 index dropped at the
    /// path a pump.fun index used would be read as pump.fun's, and the reply
    /// would state 778,593 Solana launches as this chain's measured population.
    ///
    /// Deciding by the *token's* chain instead — which is what
    /// `sheet::FactSheet::build` did until 2026-09-17 — answers a different
    /// question. It asks what chain the question is about, not what chain the
    /// answer was measured on, and those differ exactly when it matters.
    ///
    /// This is the same tag [`crate::firstparty::Chain`] carries for the same
    /// reason (ADR 0028): one file, both chains, and never a comparison across
    /// them.
    #[serde(default = "chain_before_the_field_existed")]
    pub chain: crate::firstparty::Chain,
    /// The watermark this was computed at.
    ///
    /// A Solana slot on a Solana index, a block height on a Robinhood one. The
    /// name kept its Solana spelling so a file written before the `chain` field
    /// existed still parses; what it counts is the chain's own unit, and the
    /// only thing any consumer does with it is state when the measurement
    /// stopped.
    pub watermark_slot: u64,
    /// When it was built, as seconds since the epoch.
    pub built_at: u64,
    /// The totals, over the same pass that built the records.
    ///
    /// `Option`, and defaulted, because an index written before this field
    /// existed is still a valid index and is sitting on the production box right
    /// now. Absent means **not measured**, which the consumer must say nothing
    /// about — a `Population::default()` here would be five zeroes claiming that
    /// nothing has ever graduated.
    #[serde(default)]
    pub population: Option<Population>,
    /// Creator address to their record, written the way that chain writes an
    /// address: base58 on Solana, `0x` hex on Robinhood.
    ///
    /// Which of the two a key is, is [`CreatorIndex::chain`]'s to say. The two
    /// spellings cannot collide, but "cannot collide" is not the property that
    /// matters here — a lookup that misses tells the reader this creator has no
    /// record, and a wrong-chain index would say that about every creator on
    /// earth while sounding exactly like an index that had checked.
    pub creators: BTreeMap<String, Record>,
}

impl CreatorIndex {
    /// One creator's record, or `None` if this index has never seen them.
    ///
    /// `None` means **not in the record**, never "launched nothing". A creator
    /// absent from a store that starts in August is a creator who launched
    /// before Radar was watching, and a reply that read absence as innocence
    /// would be rule 9 broken in the direction that flatters.
    #[must_use]
    pub fn get(&self, creator: &str) -> Option<&Record> {
        self.creators.get(creator)
    }

    /// How many creators are in it.
    #[must_use]
    pub fn len(&self) -> usize {
        self.creators.len()
    }

    /// Whether it holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.creators.is_empty()
    }

    /// The floor above which `record.launches` counts as
    /// [`crate::sheet::Signal::RepeatLauncher`], measured from **this
    /// index's own distribution** -- never from Radar's Solana constants.
    ///
    /// # Why not `REPEAT_FLOOR`/`INFRASTRUCTURE_FLOOR`
    ///
    /// Those are real, measured numbers about a different thing: distinct
    /// launch *blocks* a transfer-signing wallet appears in over a
    /// *90-minute window*, on *Solana*. `launches` here is a creator's
    /// **lifetime** count, over this index's whole watermark, on whichever
    /// chain the address is. Same words ("repeat", "launcher"), different
    /// population, different window, different chain — design 0020 §3 names
    /// this trap twice, quoting research 0008's warning that came true in
    /// 0024: *"Six is a tool's default, not a law."* Importing either
    /// constant was considered for this packet and rejected for exactly that
    /// reason.
    ///
    /// # Order: excluded first, then measured
    ///
    /// `list` is applied **before** the percentile is taken, not after. The
    /// Pons v2 factory and its fee escrow launch or receive on every token by
    /// design; leaving either in the population a floor is measured against
    /// makes the floor a measurement about the factory. Research 0042
    /// records the Solana version doing exclusion first: its
    /// `Infrastructure` band *"excludes 13 router/fee-sink addresses
    /// covering 42% of launches"* before the bands mean anything.
    /// Computing the floor first and excluding afterwards would fit the
    /// floor to a contaminated population, and nothing downstream could tell
    /// that it happened — which is the defect this method exists to prevent.
    ///
    /// # The percentile, and why it is stated this exactly
    ///
    /// The 95th percentile of the remaining creators' `launches`, by the
    /// **nearest-rank** convention: sort ascending and take the value at
    /// index `((n - 1) * 95) / 100`, integer arithmetic, i.e. the value at or
    /// below that rank. There are several percentile conventions in common
    /// use (nearest-rank, several flavours of linear interpolation) and they
    /// disagree at the edges of a population this size, so this is written
    /// out rather than left to a library default: a reply that cites "the
    /// 95th percentile" has to mean one specific number, reproducibly, from
    /// a recording.
    ///
    /// The floor is that value, or **2**, whichever is larger — a creator
    /// with one launch cannot be a repeat launcher under the plain meaning of
    /// the word, and a percentile that lands on 1 is telling you the
    /// population is mostly first-timers, not that everyone is a repeat
    /// launcher.
    ///
    /// # Refuses below 100 remaining creators
    ///
    /// Returns `None` when fewer than 100 creators survive the exclusion.
    /// Under that, the 95th percentile is one of the top five values in the
    /// population and moves by a whole launch every time one creator is
    /// added — a threshold that jumps when the index grows is a threshold
    /// about the index, not about creators, and `Signal::RepeatLauncher`
    /// must not fire on one.
    /// # Which chain's first-party addresses are excluded
    ///
    /// This index's own ([`CreatorIndex::chain`]), never a chain the caller
    /// passes in. The exclusion's whole job is to drop the launch factory and
    /// its escrow from the distribution, and those are named on the list under
    /// the chain they run on: filtering a Pons v2 distribution against Solana's
    /// named addresses excludes nothing, leaves the factory's thousands of
    /// launches in the sample, and pushes the 95th percentile to a number no
    /// person reaches — which silently turns the signal off.
    #[must_use]
    pub fn repeat_launcher_floor(&self, list: &crate::firstparty::FirstPartyList) -> Option<u32> {
        let mut launches: Vec<u32> = self
            .creators
            .iter()
            .filter(|(address, _)| !list.contains(self.chain, address))
            .map(|(_, record)| record.launches)
            .collect();
        if launches.len() < 100 {
            return None;
        }
        launches.sort_unstable();
        let index = (launches.len() - 1) * 95 / 100;
        Some(launches[index].max(2))
    }

    /// The totals, for a reader that needs five numbers and not 116,000
    /// records.
    ///
    /// `None` when the population was never measured: a summary of zeroes
    /// would be five claims that nothing ever graduated.
    #[must_use]
    pub fn summary(&self) -> Option<Summary> {
        self.population.map(|population| Summary {
            chain: self.chain,
            built_at: self.built_at,
            watermark_slot: self.watermark_slot,
            creators: u64::try_from(self.creators.len()).unwrap_or(u64::MAX),
            population,
        })
    }

    /// Writes the index where a consumer will find it, and the summary beside
    /// it.
    ///
    /// # Errors
    ///
    /// The I/O error, or a serialisation failure.
    pub fn write(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        // Beside and renamed, for the reason every other file this project
        // writes is: a half-written index still parses as JSON right up until
        // the truncation, and a creator missing from it reads as a creator with
        // no record.
        let temp = format!("{path}.new");
        std::fs::write(&temp, json)?;
        std::fs::rename(&temp, path)?;
        // The public site reads the totals without parsing the records, so
        // they are published as their own small file, from the same pass, at
        // the same moment. Only when there is something to say: an index with
        // no population writes no summary, and a reader finds nothing rather
        // than zeroes.
        if let Some(summary) = self.summary() {
            summary.write(&summary_path_beside(path))?;
        }
        Ok(())
    }

    /// Reads an index from disk.
    ///
    /// # Errors
    ///
    /// The I/O error, or a parse failure.
    pub fn read(path: &str) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

/// Where the summary is published by default, beside the index.
pub const SUMMARY_PATH: &str = "docs/research/data/population.json";

/// The summary's path, given the index's: the same directory, its own name.
#[must_use]
pub fn summary_path_beside(index_path: &str) -> String {
    std::path::Path::new(index_path)
        .with_file_name("population.json")
        .to_string_lossy()
        .into_owned()
}

/// The population totals, published beside the index.
///
/// What the public site's stats document is built from. The index itself is
/// one record per creator -- 116,752 of them on 2026-09-04 -- and a public
/// endpoint that parsed it per request to read five totals would be the
/// three-second store scan in miniature, behind a viral link.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    /// Which chain these totals are measured over.
    ///
    /// Carried here as well as on the index because this is the file that
    /// leaves the machine: the public site states these five numbers, and
    /// "778,593 launches, 13,911 of them organic" is a different claim about
    /// pump.fun than about Pons v2. Defaulted for the same reason the index's
    /// is — a summary written before this field is a Solana one.
    #[serde(default = "chain_before_the_field_existed")]
    pub chain: crate::firstparty::Chain,
    /// When the index was built, as seconds since the epoch.
    pub built_at: u64,
    /// The watermark it was built at.
    pub watermark_slot: u64,
    /// How many creators the index holds.
    pub creators: u64,
    /// The totals.
    pub population: Population,
}

impl Summary {
    /// Writes the summary, beside and renamed like the index.
    ///
    /// # Errors
    ///
    /// The I/O error, or a serialisation failure.
    pub fn write(&self, path: &str) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let temp = format!("{path}.new");
        std::fs::write(&temp, json)?;
        std::fs::rename(&temp, path)
    }

    /// Reads a summary from disk.
    ///
    /// # Errors
    ///
    /// The I/O error, or a parse failure. Absent means **not measured**, and
    /// the caller says so rather than filling in zeroes.
    pub fn read(path: &str) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firstparty::{Chain, FirstPartyList};

    /// A `CreatorIndex` with one record per `1..=n`, addressed `c0`..`c{n-1}`
    /// -- `launches` set to its position, `1..=n`, so an assertion about the
    /// floor is also an assertion about which creator earned it.
    fn index_of_launches(counts: &[u32]) -> CreatorIndex {
        let mut creators = BTreeMap::new();
        for (i, &n) in counts.iter().enumerate() {
            creators.insert(
                format!("c{i}"),
                Record {
                    launches: n,
                    ..Record::default()
                },
            );
        }
        CreatorIndex {
            chain: Chain::Robinhood,
            watermark_slot: 1,
            built_at: 0,
            population: None,
            creators,
        }
    }

    fn empty_list() -> FirstPartyList {
        FirstPartyList::parse(r#"{"entries": []}"#).expect("parses")
    }

    #[test]
    fn the_95th_percentile_is_pinned_to_an_exact_number() {
        // 250 creators, launches 1..=250. `(250 - 1) * 95 / 100 == 236`
        // (integer division: 249 * 95 = 23655, 23655 / 100 == 236, not 237),
        // and the value at that 0-based rank in `1..=250` is 237.
        //
        // The numbers are chosen so the three ways this arithmetic is
        // commonly mutated each land on a different answer, not 237:
        // dropping the `- 1` gives 237/100*250=... -> index 237, value 238;
        // `* 94` instead of `* 95` gives index 234, value 235; `/ 99` instead
        // of `/ 100` gives index 239, value 240. A survivor changing any one
        // of them is caught by the pinned number, not just a direction.
        let counts: Vec<u32> = (1..=250).collect();
        let index = index_of_launches(&counts);
        assert_eq!(index.repeat_launcher_floor(&empty_list()), Some(237));
    }

    #[test]
    fn a_percentile_below_two_is_raised_to_the_floor_of_two() {
        // 150 creators who have all launched exactly once: the 95th
        // percentile of a population of all-1s is 1, and "1" cannot be the
        // floor for a *repeat* launcher, so this must read 2 -- pinned
        // exactly, not just asserted `> 1`, so a mutant that turns `.max(2)`
        // into `.max(1)` (or drops it) is caught by the number rather than a
        // direction.
        let counts = vec![1u32; 150];
        let index = index_of_launches(&counts);
        assert_eq!(index.repeat_launcher_floor(&empty_list()), Some(2));
    }

    #[test]
    fn exactly_a_hundred_remaining_creators_computes_a_floor_and_ninety_nine_refuses() {
        // The boundary is "below 100", not "at or below" -- one creator
        // apart, asserted both ways, so a mutant that turns `<` into `<=`
        // (refusing at exactly 100 too) or into `>` (never refusing) is
        // caught by a concrete `Some`/`None` on either side, not by a single
        // direction that a `&&`-vs-`||` swap could also satisfy.
        let ninety_nine = index_of_launches(&vec![5u32; 99]);
        assert_eq!(
            ninety_nine.repeat_launcher_floor(&empty_list()),
            None,
            "99 remaining creators must refuse"
        );

        let hundred = index_of_launches(&vec![5u32; 100]);
        assert_eq!(
            hundred.repeat_launcher_floor(&empty_list()),
            Some(5),
            "100 remaining creators must compute a floor"
        );
    }

    #[test]
    fn the_named_list_is_excluded_before_the_floor_is_computed_not_after() {
        // 150 ordinary creators (launches 1..=150) plus two named addresses
        // holding an enormous count each -- the shape research 0042 records
        // for Radar's own `Infrastructure` band, which "excludes 13
        // router/fee-sink addresses" *before* computing its bands.
        //
        // Order matters here in a way a single assertion can catch: exclude
        // first and 150 ordinary creators remain, giving floor 142
        // (`(150-1)*95/100 == 141`, value 142 at that rank in `1..=150`).
        // Compute first and exclude after -- i.e. delete the exclusion --
        // and the floor is measured over all 152 entries instead, landing on
        // 144. The two numbers are different by construction, so this test
        // fails if the exclusion is removed, which is the defect this
        // packet exists to prevent.
        let mut counts: Vec<u32> = (1..=150).collect();
        counts.push(500_000); // "the factory"
        counts.push(500_000); // "the escrow"
        let index = index_of_launches(&counts);

        let list = FirstPartyList::parse(
            r#"{"entries": [
                {"address": "c150", "chain": "robinhood", "role": "the factory", "source": "test", "added": "2026-09-15"},
                {"address": "c151", "chain": "robinhood", "role": "the escrow", "source": "test", "added": "2026-09-15"}
            ]}"#,
        )
        .expect("parses");

        assert_eq!(
            index.repeat_launcher_floor(&list),
            Some(142),
            "excluding first must give the ordinary population's own floor"
        );

        // Re-applying the bug: the same index, with no exclusion at all, is
        // measured over the contaminated population and lands on a
        // different number.
        assert_eq!(
            index.repeat_launcher_floor(&empty_list()),
            Some(144),
            "an empty list excludes nothing, so this is the contaminated floor \
             the real signal must never use"
        );
    }

    #[test]
    fn the_summary_is_written_beside_the_index_and_only_when_there_is_one() {
        // The public site reads this file and not the index. Re-apply the bug
        // by dropping the summary write from `write` and the read below fails.
        let dir = tempfile::tempdir().expect("a temp dir");
        let index_path = dir.path().join("creator-index.json");
        let index_path = index_path.to_string_lossy().into_owned();
        let summary_path = summary_path_beside(&index_path);
        assert!(summary_path.ends_with("population.json"), "{summary_path}");

        let mut creators = BTreeMap::new();
        creators.insert("c1".to_owned(), Record::default());
        creators.insert("c2".to_owned(), Record::default());
        let index = CreatorIndex {
            chain: Chain::Robinhood,
            watermark_slot: 444_374_676,
            built_at: 1_788_000_000,
            population: Some(Population {
                launches: 10,
                measured: 8,
                organic: 1,
                instant: 1,
                stillborn: 2,
            }),
            creators,
        };
        index.write(&index_path).expect("writes");
        let summary = Summary::read(&summary_path).expect("the summary is beside the index");
        assert_eq!(summary.creators, 2);
        assert_eq!(summary.watermark_slot, 444_374_676);
        assert_eq!(summary.built_at, 1_788_000_000);
        assert_eq!(summary.population.measured, 8);

        // An index with no population writes no summary: absent, not zeroes.
        // In its own directory, because the summary's name is fixed and the
        // first index's summary is already beside it.
        let older = CreatorIndex {
            population: None,
            ..index
        };
        let elsewhere = tempfile::tempdir().expect("a second temp dir");
        let older_path = elsewhere.path().join("creator-index.json");
        let older_path = older_path.to_string_lossy().into_owned();
        older.write(&older_path).expect("writes");
        assert!(
            Summary::read(&summary_path_beside(&older_path)).is_err(),
            "no population, so no summary file"
        );
    }

    #[test]
    fn a_creator_absent_from_the_index_is_unknown_rather_than_innocent() {
        // The store starts in August. A creator who launched before that is not
        // a creator who launched nothing, and a reply reading absence as
        // innocence would be rule 9 broken in the direction that flatters.
        let index = CreatorIndex {
            chain: Chain::Robinhood,
            watermark_slot: 1,
            built_at: 0,
            population: None,
            creators: BTreeMap::new(),
        };
        assert_eq!(index.get("nobody"), None);
        assert!(index.is_empty());
    }

    #[test]
    fn the_size_of_the_index_is_the_number_of_creators_in_it() {
        // It is printed by whatever builds the index, and it is how an operator
        // knows the build worked: "117,680 creators at slot N" against "0
        // creators" is the difference between a good index and a silently
        // empty one, and a constant would report the same either way.
        let mut creators = BTreeMap::new();
        assert_eq!(
            CreatorIndex {
                chain: Chain::Robinhood,
                watermark_slot: 1,
                built_at: 0,
                population: None,
                creators: creators.clone(),
            }
            .len(),
            0
        );

        for n in 0..3 {
            creators.insert(format!("creator-{n}"), Record::default());
        }
        let index = CreatorIndex {
            chain: Chain::Robinhood,
            watermark_slot: 1,
            built_at: 0,
            population: None,
            creators,
        };
        assert_eq!(index.len(), 3);
        assert!(!index.is_empty(), "three is not none");
    }

    #[test]
    fn an_index_round_trips_through_a_file() {
        let dir = std::env::temp_dir().join(format!("radar-cidx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let path = dir.join("index.json");
        let path = path.to_str().expect("a path").to_owned();

        let mut creators = BTreeMap::new();
        creators.insert(
            "abc".to_owned(),
            Record {
                launches: 47,
                measured: 40,
                organic: 0,
                instant: 2,
                stillborn: 31,
            },
        );
        let index = CreatorIndex {
            chain: Chain::Robinhood,
            watermark_slot: 444_339_860,
            built_at: 1_788_000_000,
            population: None,
            creators,
        };
        index.write(&path).expect("written");

        let back = CreatorIndex::read(&path).expect("read");
        assert_eq!(back.watermark_slot, 444_339_860);
        let record = back.get("abc").expect("the creator");
        assert_eq!(record.launches, 47);
        assert_eq!(record.measured, 40);
        assert_eq!(record.organic, 0);
        assert_eq!(record.instant, 2);
        assert_eq!(record.stillborn, 31);

        assert!(
            !std::path::Path::new(&format!("{path}.new")).exists(),
            "the temporary file must not survive"
        );
    }

    #[test]
    fn measured_is_kept_apart_from_launches() {
        // A gap between them means the outcome pass has not caught up, not that
        // those tokens did nothing. A consumer quoting a share must quote the
        // denominator it is a share of, and it can only do that if both numbers
        // are here.
        let record = Record {
            launches: 47,
            measured: 40,
            organic: 0,
            instant: 2,
            stillborn: 31,
        };
        assert!(record.measured <= record.launches);
        assert!(record.organic + record.instant + record.stillborn <= record.measured);
    }

    #[test]
    fn a_share_is_out_of_what_was_measured_not_out_of_what_was_launched() {
        // Re-applying this bug is a one-character edit -- `launches` for
        // `measured` in `share` -- and it is the one that quietly understates
        // every graduation rate by the size of Radar's own outcome backlog.
        //
        // Here the backlog is half the population, so the wrong denominator
        // halves every figure: a 50% graduation rate published as 25%, which is
        // a measurement of the outcome pass presented as a fact about the venue.
        let p = Population {
            launches: 200,
            measured: 100,
            organic: 30,
            instant: 20,
            stillborn: 40,
        };
        assert!((p.graduated_share().expect("measured") - 0.50).abs() < 1e-12);
        assert!((p.organic_share().expect("measured") - 0.30).abs() < 1e-12);
        assert!((p.instant_share().expect("measured") - 0.20).abs() < 1e-12);
        assert!((p.stillborn_share().expect("measured") - 0.40).abs() < 1e-12);
    }

    #[test]
    fn nothing_measured_yields_no_share_at_all() {
        // Rule 9. `0.0` here would be "nothing on this venue ever graduates",
        // which is both false and the direction that sounds authoritative.
        let p = Population {
            launches: 5_000,
            measured: 0,
            organic: 0,
            instant: 0,
            stillborn: 0,
        };
        assert_eq!(p.graduated_share(), None);
        assert_eq!(p.organic_share(), None);
        assert_eq!(p.instant_share(), None);
        assert_eq!(p.stillborn_share(), None);
    }

    #[test]
    fn an_index_written_before_the_population_existed_still_loads() {
        // The one sitting on the production box right now has no `population`
        // key. Refusing it would take the creator facts away -- the facts that
        // stopped every reply being the same reply -- to gain a field.
        let old = r#"{"watermark_slot":444361818,"built_at":1788000000,
                      "creators":{"aaa":{"launches":3,"measured":2,"organic":1,
                      "instant":0,"stillborn":1}}}"#;
        let index: CreatorIndex = serde_json::from_str(old).expect("an older index loads");
        assert_eq!(index.len(), 1);
        assert_eq!(
            index.population, None,
            "absent means not measured, and the consumer must say nothing"
        );
        assert_eq!(
            index.chain,
            Chain::Solana,
            "a file with no chain key is a pump.fun index, because that is the only \
             thing that wrote this shape before the key existed"
        );
    }

    #[test]
    fn the_chain_survives_a_round_trip_and_is_not_the_default() {
        // Robinhood deliberately: `chain` defaults to Solana, so a round trip
        // that used Solana would pass with the field dropped from the struct
        // entirely, and would be a test that cannot fail.
        let index = CreatorIndex {
            chain: Chain::Robinhood,
            watermark_slot: 9_001,
            built_at: 1_788_000_000,
            population: None,
            creators: BTreeMap::new(),
        };
        let json = serde_json::to_string(&index).expect("an index serialises");
        assert!(json.contains(r#""chain":"robinhood""#), "{json}");
        let back: CreatorIndex = serde_json::from_str(&json).expect("it reads back");
        assert_eq!(back.chain, Chain::Robinhood);
    }

    #[test]
    fn the_floor_excludes_this_indexs_own_chains_named_addresses() {
        // The exclusion that keeps the launch factory and its relayers out of
        // the distribution is looked up under **this index's** chain, not one a
        // caller passes in. Re-apply the bug -- read the same index as Solana
        // -- and twenty named Robinhood addresses stay in the sample, which
        // pushes the 95th percentile from 2 to 500.
        let mut entries = Vec::new();
        for i in 0..20u32 {
            entries.push(format!(
                r#"{{"address":"0x{i:040x}","chain":"robinhood",
                    "role":"a Pons v2 relayer","source":"research 0038",
                    "added":"2026-09-17"}}"#
            ));
        }
        let list = FirstPartyList::parse(&format!(r#"{{"entries":[{}]}}"#, entries.join(",")))
            .expect("the list parses");

        let mut creators = BTreeMap::new();
        // Exactly 100 ordinary creators survive the exclusion, which is the
        // smallest sample `repeat_launcher_floor` will compute a floor from.
        for i in 0..100u32 {
            creators.insert(
                format!("ordinary{i}"),
                Record {
                    launches: 1,
                    ..Record::default()
                },
            );
        }
        for i in 0..20u32 {
            creators.insert(
                format!("0x{i:040x}"),
                Record {
                    launches: 500,
                    ..Record::default()
                },
            );
        }
        let index = CreatorIndex {
            chain: Chain::Robinhood,
            watermark_slot: 1,
            built_at: 0,
            population: None,
            creators,
        };
        assert_eq!(
            index.repeat_launcher_floor(&list),
            Some(2),
            "the named addresses' 500 launches each must be out of the distribution"
        );

        let as_solana = CreatorIndex {
            chain: Chain::Solana,
            ..index
        };
        assert_eq!(
            as_solana.repeat_launcher_floor(&list),
            Some(500),
            "a wrong-chain exclusion excludes nothing, so the floor becomes a threshold about the relayers rather than about creators"
        );
    }
}
