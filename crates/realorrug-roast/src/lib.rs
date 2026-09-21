// SPDX-License-Identifier: Apache-2.0
//! The public analyst's reply.
//!
//! # The pipeline
//!
//! ```text
//!   mint ──▶ realorrug-onchain ──▶ Dossier ──┐
//!                                        ├──▶ FactSheet ──▶ model ──▶ CHECKS ──▶ reply
//!   docs/research/data/0024 ──▶ BaseRates┘         │                     │
//!                                                  │                     └─ fail ─▶ template
//!                                                  └─ the ONLY thing the model sees
//! ```
//!
//! Four things decide four different questions, and keeping them apart is the
//! whole design:
//!
//! - **What the numbers are** — the instruments, deterministically.
//! - **The verdict** — a rule, so a refusal is reproducible from a recording.
//! - **What the headline is, what matters, the framing, the tone** — the model.
//!   This is real judgement and it is where the product's voice comes from.
//! - **Whether a number in the output is real** — a check, after generation.
//!
//! The model may not introduce a fact. That is a deliberate narrowing of "the
//! model makes judgements", and it is the same shape as `radar-signer`'s
//! `verify::check`, which re-decodes the bytes rather than trusting the
//! caller's description of them. **The signer re-reads the bytes it signs; the
//! roaster re-reads the numbers it posts.**
//!
//! # Its caller
//!
//! `radar roast <mint>`, in `radar-cli`, which prints the reply to stdout. The
//! whole pipeline is exercisable offline and with no X account, no credential
//! and no key — which is the point of building it before the adapter.
//!
//! # What it does not do
//!
//! It does not post anything, hold a credential, or know that X exists. It does
//! not read the store. It does not trade, sign, or touch `Policy::CLOSED`.

#![forbid(unsafe_code)]

pub mod assessment;
pub mod baserates;
pub mod capture;
pub mod clause;
pub mod creator;
pub mod fidelity;
pub mod firstparty;
pub mod forbidden;
pub mod render;
pub mod report;
pub mod salience;
pub mod sheet;
pub mod unknown;
pub mod verdict;
pub mod voice;

pub use assessment::Assessment;
pub use baserates::BaseRates;
pub use capture::Capture;
pub use clause::{Clause, Kind, Selection, Voice};
pub use creator::{CreatorIndex, Population};
pub use report::Report;
pub use sheet::{About, Fact, FactSheet};
pub use verdict::{Level, Verdict, level, level_from_score, template};
pub use voice::{Billed, Fellback, Reply, write};

use realorrug_model::Provider;
use realorrug_onchain::Dossier;
use realorrug_types::Address;

/// The verdict rule's own version, bumped whenever `verdict.rs`,
/// `assessment.rs` or a signal's firing condition changes in a way that
/// could move a published level or score for the same fact sheet.
///
/// Written onto every capture (`realorrug capture`, `realorrug-cli`) beside
/// the fact sheet, so a replay months later can say *why* a recomputed
/// verdict disagrees with the one a capture recorded: the facts did not
/// change, the rules did. Bump this by hand in the same commit as any change
/// to the rule it describes -- there is no test that can catch a forgotten
/// bump, because the rule change and the version are the same author's two
/// separate edits.
pub const RULES_VERSION: &str = "2026-09-21";

/// Builds the reply for a dossier.
///
/// The one function a caller needs. `rates`, `creators` and `provider` are each
/// `None` when the thing behind them is not configured or could not be read —
/// none is an error, and every one of them makes the reply **say less rather
/// than say more**, which is the only safe direction for a system whose claim
/// is that it states what it measured.
///
/// `self_mint` is the analyst's own token, or `None` when no token is special.
/// It is a required argument rather than a builder step a caller could omit,
/// because ADR 0013 constraint 5 is a property of every reply and a caller that
/// forgot it would produce a reply that looked exactly right.
#[must_use]
pub fn roast(
    dossier: &Dossier,
    rates: Option<&BaseRates>,
    creators: Option<&CreatorIndex>,
    provider: Option<&dyn Provider>,
    self_mint: Option<&Address>,
) -> (FactSheet, Reply) {
    // No named-list argument here: threading a real one through to this
    // public entry point (and from it into `realorrug-analyst`/`realorrug-cli`)
    // is packet 0037's next packet's job, not this one's. `None` here means
    // `RepeatLauncher` cannot fire yet from this call path -- deny by default
    // (AGENTS.md rule 7), same as an unconfigured `rates` or `creators`.
    let sheet = FactSheet::build(dossier, rates, creators, self_mint, None);
    let reply = write(&sheet, provider);
    (sheet, reply)
}
