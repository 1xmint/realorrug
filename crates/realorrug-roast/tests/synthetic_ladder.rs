// SPDX-License-Identifier: Apache-2.0
//! Research 0059: five hand-built fact sheets, one per [`Level`], run
//! through the full reply path (`report::build`, `voice::write`, and the
//! three checks `realorrug-cli`'s `replay.rs` runs) so the owner can read
//! what the bot actually says at the top and the bottom of the ladder --
//! not just at `CantTell`, which is the only level any real capture has
//! reached so far.
//!
//! Each sheet is constructed directly as a [`FactSheet`] struct literal
//! (every field is `pub`), never through [`FactSheet::build`] against a
//! [`realorrug_onchain::Dossier`] -- there is no real chain read behind any
//! of these, and the mint on every one says so
//! (`SYNTHETIC-<LEVEL>-NOT-A-REAL-TOKEN`). This is the shape
//! `tests/replay/README.md` names as allowed: "a hand-built [sheet] for a
//! case that needs a specific fact combination."
//!
//! This test does two things at once, deliberately not split into two
//! files: it asserts the level ([`verdict::level`]) each sheet earns, so a
//! later change to the ladder that silently reclassifies one fails loudly
//! here; and, in the same run, it writes the sheet, the report, the reply
//! and every check's verdict to `target/synthetic-ladder-report.txt` for
//! `docs/research/0059-what-the-bot-says-at-every-level.md` to quote from
//! verbatim. Splitting those two would mean generating the fixtures once
//! and asserting on them separately, two copies of "what the fixture is"
//! that could drift; one function keeps them one copy.
//!
//! It also writes each sheet as `tests/ladder/synthetic-<level>.sheet.json`,
//! in the same shape `accepted_replies_still_pass.rs` reads, so a reviewer
//! can run `realorrug replay` against them by hand. They live in their own
//! directory rather than next to the accepted cases because nobody has read
//! and signed off on what the model says about any of them, and
//! `tests/replay/` is defined as the set that has been signed off:
//! `accepted_replies_still_pass.rs` panics on any `<stem>.sheet.json` there
//! with no `<stem>.accepted.txt`, which is the right behaviour for a
//! half-written accepted pair and the reason these cannot sit beside them.

use realorrug_roast::sheet::{About, Fact, FactSheet, Signal};
use realorrug_roast::verdict::Level;
use realorrug_roast::{Assessment, Capture};
use realorrug_types::{ReadAt, Slot};

fn age_fact() -> Fact {
    Fact::exact(
        realorrug_roast::Kind::Age,
        "how long ago this token's launch block was, on the chain's own clock",
        900.0,
        "about 0.4 hours old at the read (900 slots after its launch block)",
    )
    .saying(
        realorrug_roast::Voice::Plain,
        "It launched about 0.4 hours ago -- 900 slots, by the read point.".to_owned(),
    )
}

fn holders_fact(n: f64, rendered: &str) -> Fact {
    Fact::exact(
        realorrug_roast::Kind::Holders,
        "addresses holding the token, not counting the bonding curve",
        n,
        rendered,
    )
}

fn largest_holder_share_fact(ratio: f64) -> Fact {
    Fact::share(
        realorrug_roast::Kind::LargestHolderShare,
        "share of circulating supply at the single largest address outside the curve",
        ratio,
    )
}

fn creator_launches_fact(n: f64) -> Fact {
    Fact::exact(
        realorrug_roast::Kind::CreatorLaunches,
        "tokens this creator has launched, in Real or Rug's record",
        n,
        format!("{n:.0}"),
    )
}

fn creator_organic_fact() -> Fact {
    Fact::exact(
        realorrug_roast::Kind::CreatorOrganic,
        "of the measured launches, how many ever filled a curve over time",
        0.0,
        "0 of 14",
    )
}

fn curve_liquidity_drained_fact() -> Fact {
    Fact {
        about: About::Price,
        kind: realorrug_roast::Kind::CurveLiquidity,
        label: "quote asset held in the bonding curve now, read at slot 900".to_owned(),
        rendered: "0.0000 SOL".to_owned(),
        values: vec![0.0],
        clauses: vec![realorrug_roast::Clause::new(
            realorrug_roast::Voice::Plain,
            "The curve holds 0.0000 SOL right now, as of slot 900.".to_owned(),
        )],
    }
}

/// `CantTell`: a required fact was not read. No signal, because a signal
/// needs the fact it is drawn from to have been read at all.
fn sheet_cant_tell() -> FactSheet {
    FactSheet {
        mint: "SYNTHETIC-CANTTELL-NOT-A-REAL-TOKEN".to_owned(),
        read_at: Some(ReadAt::Solana(Slot(900))),
        facts: Vec::new(),
        untrusted: Vec::new(),
        unknown: vec![
            "where the checked early buyers got their money before their first purchase could \
             not be read"
                .to_owned(),
        ],
        signals: Vec::new(),
        twins: Vec::new(),
        skipped: Vec::new(),
    }
}

/// `NothingUglyYet`: every required fact was read, and none of them fired a
/// signal.
fn sheet_nothing_ugly_yet() -> FactSheet {
    FactSheet {
        mint: "SYNTHETIC-NOTHINGUGLYYET-NOT-A-REAL-TOKEN".to_owned(),
        read_at: Some(ReadAt::Solana(Slot(900))),
        facts: vec![age_fact()],
        untrusted: Vec::new(),
        unknown: Vec::new(),
        signals: Vec::new(),
        twins: Vec::new(),
        skipped: Vec::new(),
    }
}

/// `Sketchy`: exactly one live-risk signal, `HolderConcentration` -- a real
/// red flag (one address holding most of supply) with an innocent
/// explanation still open (it could be an exchange wallet or the pool
/// itself), and no other signal to combine it with.
fn sheet_sketchy() -> FactSheet {
    FactSheet {
        mint: "SYNTHETIC-SKETCHY-NOT-A-REAL-TOKEN".to_owned(),
        read_at: Some(ReadAt::Solana(Slot(900))),
        facts: vec![
            age_fact(),
            holders_fact(6.0, "6"),
            largest_holder_share_fact(0.61),
        ],
        untrusted: Vec::new(),
        unknown: Vec::new(),
        signals: vec![Signal::HolderConcentration],
        twins: vec![
            "the largest address outside the curve could be an exchange's wallet or the pool \
             itself, which this read does not resolve"
                .to_owned(),
        ],
        skipped: Vec::new(),
    }
}

/// `RugMechanicsLive`: two distinct episodes among the live-risk signals --
/// `HolderConcentration` (the `Holders` episode) and `RepeatLauncher` (the
/// `CreatorHistory` episode) -- fired together, neither of them the
/// completed-rug pair `verdict::rugged_pair` looks for.
fn sheet_rug_mechanics_live() -> FactSheet {
    FactSheet {
        mint: "SYNTHETIC-RUGMECHANICSLIVE-NOT-A-REAL-TOKEN".to_owned(),
        read_at: Some(ReadAt::Solana(Slot(900))),
        facts: vec![
            age_fact(),
            holders_fact(4.0, "4"),
            largest_holder_share_fact(0.74),
            creator_launches_fact(14.0),
            creator_organic_fact(),
        ],
        untrusted: Vec::new(),
        unknown: Vec::new(),
        signals: vec![Signal::HolderConcentration, Signal::RepeatLauncher],
        twins: vec![
            "the largest address outside the curve could be an exchange's wallet or the pool \
             itself, which this read does not resolve"
                .to_owned(),
            "a creator who launches often and rarely graduates could be an inexperienced \
             repeat hobbyist rather than someone running the same play on purpose"
                .to_owned(),
        ],
        skipped: Vec::new(),
    }
}

/// `Rugged`: the observed, completed pair `verdict::rugged_pair` requires --
/// `LiquidityGone` (the curve's reserves are read as zero, pre-graduation)
/// together with `HolderConcentration`.
fn sheet_rugged() -> FactSheet {
    FactSheet {
        mint: "SYNTHETIC-RUGGED-NOT-A-REAL-TOKEN".to_owned(),
        read_at: Some(ReadAt::Solana(Slot(900))),
        facts: vec![
            age_fact(),
            curve_liquidity_drained_fact(),
            holders_fact(5.0, "5"),
            largest_holder_share_fact(0.89),
        ],
        untrusted: Vec::new(),
        unknown: Vec::new(),
        signals: vec![Signal::LiquidityGone, Signal::HolderConcentration],
        twins: vec![
            "a curve read at zero reserves pre-graduation is also what a fully-sold-through \
             graduation looks like for one block, before the AMM pool is credited"
                .to_owned(),
            "the largest address outside the curve could be an exchange's wallet or the pool \
             itself, which this read does not resolve"
                .to_owned(),
        ],
        skipped: Vec::new(),
    }
}

/// One case: its name, its sheet, and the level `verdict::level` must
/// return for the assertion in [`the_five_levels_are_earned_and_recorded`]
/// to hold.
struct Case {
    name: &'static str,
    sheet: FactSheet,
    expect: Level,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "cant-tell",
            sheet: sheet_cant_tell(),
            expect: Level::CantTell,
        },
        Case {
            name: "nothing-ugly-yet",
            sheet: sheet_nothing_ugly_yet(),
            expect: Level::NothingUglyYet,
        },
        Case {
            name: "sketchy",
            sheet: sheet_sketchy(),
            expect: Level::Sketchy,
        },
        Case {
            name: "rug-mechanics-live",
            sheet: sheet_rug_mechanics_live(),
            expect: Level::RugMechanicsLive,
        },
        Case {
            name: "rugged",
            sheet: sheet_rugged(),
            expect: Level::Rugged,
        },
    ]
}

/// One check row, matching `realorrug-cli`'s own `replay.rs` shape.
struct CheckRow {
    name: &'static str,
    ok: bool,
    reason: String,
}

fn checks_for(authorised: &[realorrug_roast::fidelity::Authorised], text: &str) -> Vec<CheckRow> {
    let fabricated = realorrug_roast::fidelity::check(text, authorised);
    let forbidden = realorrug_roast::forbidden::check(text);
    let unknown = realorrug_roast::unknown::check(text);
    vec![
        CheckRow {
            name: "evidence fidelity",
            ok: fabricated.is_empty(),
            reason: if fabricated.is_empty() {
                "every number is on the fact sheet".to_owned()
            } else {
                fabricated
                    .iter()
                    .map(|f| format!("{} ({:?})", f.literal, f.why))
                    .collect::<Vec<_>>()
                    .join("; ")
            },
        },
        CheckRow {
            name: "forbidden::check (blanket, production)",
            ok: forbidden.is_empty(),
            reason: if forbidden.is_empty() {
                "no forbidden phrase found".to_owned()
            } else {
                forbidden
                    .iter()
                    .map(|v| format!("\"{}\" -- {}", v.phrase, v.because))
                    .collect::<Vec<_>>()
                    .join("; ")
            },
        },
        CheckRow {
            name: "unknown-data",
            ok: unknown.is_empty(),
            reason: if unknown.is_empty() {
                "no gap called safe, and the risk index called no probability".to_owned()
            } else {
                unknown
                    .iter()
                    .map(|v| format!("\"{}\" -- {}", v.phrase, v.because))
                    .collect::<Vec<_>>()
                    .join("; ")
            },
        },
    ]
}

/// The level-aware trio `voice.rs` already uses for a model draft, run
/// against the same text the blanket [`realorrug_roast::forbidden::check`]
/// judged above -- the comparison research 0059 exists to report.
fn level_aware_rows(text: &str, level: Level) -> Vec<CheckRow> {
    let target = realorrug_roast::forbidden::check_target(text);
    let by_level = realorrug_roast::forbidden::check_level(text, level);
    let unconditional = realorrug_roast::forbidden::check_unconditional(text);
    vec![
        CheckRow {
            name: "check_target (person-directed accusation)",
            ok: target.is_empty(),
            reason: if target.is_empty() {
                "no accusation aimed at a person, account or company".to_owned()
            } else {
                target
                    .iter()
                    .map(|v| format!("\"{}\" -- {}", v.phrase, v.because))
                    .collect::<Vec<_>>()
                    .join("; ")
            },
        },
        CheckRow {
            name: "check_level (word this level has not earned)",
            ok: by_level.is_empty(),
            reason: if by_level.is_empty() {
                "no word above this level's earned vocabulary".to_owned()
            } else {
                by_level
                    .iter()
                    .map(|v| format!("\"{}\" -- {}", v.phrase, v.because))
                    .collect::<Vec<_>>()
                    .join("; ")
            },
        },
        CheckRow {
            name: "check_unconditional (advice/honeypot/cabal-identity ban)",
            ok: unconditional.is_empty(),
            reason: if unconditional.is_empty() {
                "no unconditionally-banned phrase found".to_owned()
            } else {
                unconditional
                    .iter()
                    .map(|v| format!("\"{}\" -- {}", v.phrase, v.because))
                    .collect::<Vec<_>>()
                    .join("; ")
            },
        },
    ]
}

/// Builds every synthetic sheet, asserts the level it earns, renders it
/// through the full reply path, writes the synthetic `.sheet.json` fixtures
/// to `tests/ladder/`, and writes a full transcript to
/// `target/synthetic-ladder-report.txt` for research 0059 to quote.
#[test]
fn the_five_levels_are_earned_and_recorded() {
    use std::fmt::Write as _;

    let ladder_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ladder");
    std::fs::create_dir_all(&ladder_dir)
        .unwrap_or_else(|e| panic!("create {}: {e}", ladder_dir.display()));
    let mut out = String::new();

    for case in cases() {
        let level = realorrug_roast::level(&case.sheet);
        assert_eq!(
            level, case.expect,
            "sheet {:?} was built to earn {:?} but verdict::level returned {:?} -- the ladder \
             moved under this fixture",
            case.name, case.expect, level
        );

        let assessment = Assessment::from(&case.sheet);
        let report = realorrug_roast::report::build(&case.sheet);
        // No provider configured: the deterministic template ships, the same
        // reply the product ships with no model credential configured.
        let reply = realorrug_roast::voice::write(&case.sheet, None);
        let report_text = report.render(&assessment);

        let sheet_authorised = case.sheet.authorised();
        let reply_rows = checks_for(&sheet_authorised, &reply.text);
        let mut report_authorised = sheet_authorised.clone();
        report_authorised.push(realorrug_roast::fidelity::Authorised::anywhere(f64::from(
            assessment.risk_index,
        )));
        report_authorised.push(realorrug_roast::fidelity::Authorised::anywhere(100.0));
        #[expect(
            clippy::cast_precision_loss,
            reason = "a count of facts read is well inside f64's exact integer range, same as \
                      realorrug-cli's replay.rs"
        )]
        {
            report_authorised.push(realorrug_roast::fidelity::Authorised::anywhere(
                assessment.coverage.read as f64,
            ));
            report_authorised.push(realorrug_roast::fidelity::Authorised::anywhere(
                assessment.coverage.applicable as f64,
            ));
        }
        let report_rows = checks_for(&report_authorised, &report_text);

        let _ = writeln!(out, "## {} -- {level:?}\n", case.name);
        let _ = writeln!(out, "### Sheet\n");
        let _ = writeln!(
            out,
            "```json\n{}\n```\n",
            serde_json::to_string_pretty(&case.sheet).expect("encode sheet")
        );
        let _ = writeln!(out, "### Report\n");
        let _ = writeln!(out, "```\n{report_text}\n```\n");
        let _ = writeln!(out, "### Reply\n");
        let _ = writeln!(out, "```\n{}\n```\n", reply.text);
        let _ = writeln!(out, "### Checks -- reply text\n");
        for row in &reply_rows {
            let mark = if row.ok { "PASS" } else { "FAIL" };
            let _ = writeln!(out, "- {mark} -- {} -- {}", row.name, row.reason);
        }
        let _ = writeln!(out, "\n### Checks -- report text\n");
        for row in &report_rows {
            let mark = if row.ok { "PASS" } else { "FAIL" };
            let _ = writeln!(out, "- {mark} -- {} -- {}", row.name, row.reason);
        }

        // The level-aware trio, on the same reply text, for Rugged and
        // RugMechanicsLive specifically -- the comparison the packet asks
        // for, since those are the two levels the fourteen-phrase blanket
        // ban has the most vocabulary to take away from.
        if matches!(level, Level::Rugged | Level::RugMechanicsLive) {
            let aware_reply = level_aware_rows(&reply.text, level);
            let aware_report = level_aware_rows(&report_text, level);
            let _ = writeln!(
                out,
                "\n### Level-aware trio (check_target + check_level + check_unconditional) -- \
                 reply text, NOT called in production outside voice.rs's model path\n"
            );
            for row in &aware_reply {
                let mark = if row.ok { "PASS" } else { "FAIL" };
                let _ = writeln!(out, "- {mark} -- {} -- {}", row.name, row.reason);
            }
            let _ = writeln!(
                out,
                "\n### Level-aware trio -- report text, NOT called in production\n"
            );
            for row in &aware_report {
                let mark = if row.ok { "PASS" } else { "FAIL" };
                let _ = writeln!(out, "- {mark} -- {} -- {}", row.name, row.reason);
            }
        }
        let _ = writeln!(out, "\n---\n");

        // The fixture, written to `tests/ladder/` -- deliberately not beside
        // the accepted replay cases, which are only the ones the owner has
        // signed off on.
        let capture = Capture {
            mint: case.sheet.mint.clone(),
            label: format!("synthetic-{}", case.name),
            captured_at: "2026-09-22T00:00:00Z".to_owned(),
            rules_version: realorrug_roast::RULES_VERSION.to_owned(),
            level,
            assessment,
            sheet: case.sheet,
        };
        let json = serde_json::to_string_pretty(&capture).expect("encode capture");
        let path = ladder_dir.join(format!("synthetic-{}.sheet.json", case.name));
        std::fs::write(&path, json).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    }

    let target_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let _ = std::fs::create_dir_all(&target_dir);
    let out_path = target_dir.join("synthetic-ladder-report.txt");
    std::fs::write(&out_path, &out).unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));
}
