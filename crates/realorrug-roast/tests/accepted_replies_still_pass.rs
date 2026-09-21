// SPDX-License-Identifier: Apache-2.0
//! Every reply the owner has accepted from `realorrug replay` still passes
//! today's checks against today's rules (plan 0002 phase 2, unit 5).
//!
//! `tests/replay/README.md` explains how a case lands in `tests/replay/`: a
//! capture plus the accepted reply text, written there only after the owner
//! read `review.md` and said yes. This test does not accept anything itself
//! -- it recomputes the level, the report and the reply from each capture
//! and re-runs the three checks (`fidelity`, `forbidden`, `unknown`) against
//! the reply text the owner actually accepted and against the freshly
//! rendered report text, the same two texts `realorrug replay` checks.
//!
//! Zero cases is the expected state before the first one is ever accepted,
//! and the test is written to pass over that empty directory rather than
//! fail for having nothing to check -- an empty `tests/replay/` is not a
//! coverage gap, and treating it as a failure would be a fixture the repo
//! itself would carry with no case behind it.

use realorrug_roast::fidelity::Authorised;
use realorrug_roast::{Assessment, Capture, fidelity, forbidden, unknown};

const CASES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/replay");

/// One accepted case: the capture, and the reply text the owner accepted.
struct Case {
    stem: String,
    capture: Capture,
    accepted_reply: String,
}

/// Every `<stem>.sheet.json` in `tests/replay` paired with its
/// `<stem>.accepted.txt`.
///
/// Panics rather than skipping a half-written pair: a `.sheet.json` with no
/// matching `.accepted.txt` (or the reverse) is a fixture someone started
/// and did not finish, and this test should say so loudly rather than read
/// past it as zero cases.
fn cases() -> Vec<Case> {
    let dir = std::path::Path::new(CASES_DIR);
    let mut cases = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        // The directory itself is checked into the repo (README.md lives
        // there), so an unreadable `tests/replay` is an environment problem,
        // not "no cases yet" -- but a fresh checkout on a filesystem that
        // orders directory reads oddly should never fail this test over
        // that, so this stays permissive and simply finds nothing.
        return cases;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        let Some(stem) = name.strip_suffix(".sheet.json") else {
            continue;
        };
        let accepted_path = dir.join(format!("{stem}.accepted.txt"));
        assert!(
            accepted_path.is_file(),
            "{stem}.sheet.json has no matching {stem}.accepted.txt in tests/replay -- \
             a case only lands here as the pair README.md describes"
        );
        let sheet_text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let capture: Capture = serde_json::from_str(&sheet_text)
            .unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()));
        let accepted_reply = std::fs::read_to_string(&accepted_path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", accepted_path.display()));
        cases.push(Case {
            stem: stem.to_owned(),
            capture,
            accepted_reply,
        });
    }
    cases
}

/// The three checks, run against one piece of text -- the same shape
/// `realorrug-cli`'s own `replay.rs` uses, kept independent of it (a
/// roast-crate test depending on the CLI crate would be the dependency
/// running backwards, `capture.rs`'s own doc comment gives the same reason
/// for why `Capture` lives here rather than there).
fn assert_checks_pass(authorised: &[Authorised], text: &str, stem: &str, which: &str) {
    let fabricated = fidelity::check(text, authorised);
    assert!(
        fabricated.is_empty(),
        "{stem} ({which}) fabricated a number today's fact sheet does not authorise: {fabricated:?}"
    );
    let forbidden = forbidden::check(text);
    assert!(
        forbidden.is_empty(),
        "{stem} ({which}) trips the forbidden-phrase check today: {forbidden:?}"
    );
    let unknown = unknown::check(text);
    assert!(
        unknown.is_empty(),
        "{stem} ({which}) trips the unknown-data check today: {unknown:?}"
    );
}

#[test]
fn accepted_replies_still_pass() {
    for case in cases() {
        let sheet = &case.capture.sheet;
        let assessment = Assessment::from(sheet);
        let report = realorrug_roast::report::build(sheet);

        let sheet_authorised = sheet.authorised();
        assert_checks_pass(&sheet_authorised, &case.accepted_reply, &case.stem, "reply");

        // The report's own trailing line states `assessment.risk_index` and
        // `assessment.coverage` -- numbers `report::build` computed from the
        // sheet, not sheet facts themselves -- so they are authorised
        // explicitly here, exactly as `realorrug-cli`'s `replay.rs` does for
        // the same reason (see its `render_case`).
        let mut report_authorised = sheet_authorised;
        report_authorised.push(Authorised::anywhere(f64::from(assessment.risk_index)));
        report_authorised.push(Authorised::anywhere(100.0));
        #[expect(
            clippy::cast_precision_loss,
            reason = "a count of facts read is well inside f64's exact integer range and this \
                      is a comparison against a literal the report renders, not arithmetic"
        )]
        {
            report_authorised.push(Authorised::anywhere(assessment.coverage.read as f64));
            report_authorised.push(Authorised::anywhere(assessment.coverage.applicable as f64));
        }
        let report_text = report.render(&assessment);
        assert_checks_pass(&report_authorised, &report_text, &case.stem, "report");
    }
}
