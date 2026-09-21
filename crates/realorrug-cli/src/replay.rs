// SPDX-License-Identifier: Apache-2.0
//! `realorrug replay <dir> [--model]` — recomputes every capture in `<dir>`
//! offline and writes `<dir>/review.md` for an operator to read and accept
//! or reject (plan 0002 phase 2, unit 3).
//!
//! # No network
//!
//! Every `*.sheet.json` in `<dir>` already carries the fact sheet
//! `realorrug capture` built from a chain read taken once, at capture time.
//! Replay never re-reads the chain: the level, the report and the reply are
//! all recomputed from that frozen sheet alone, which is what makes a replay
//! reproducible days or months after the mint it is about may have changed
//! or gone quiet.
//!
//! # Template by default, `--model` by exception
//!
//! Without `--model`, every case gets the deterministic template
//! ([`realorrug_roast::voice::write`] with no provider) — free, and the same
//! reply the analyst would ship with no credential configured. `--model`
//! asks the configured provider for real, which costs a call per case; the
//! packet reserves that flag for the owner, not for routine replay runs.
//!
//! # The three checks
//!
//! `realorrug_roast::fidelity::check`, `realorrug_roast::forbidden::check`
//! and `realorrug_roast::unknown::check` each run against both the reply and
//! the report text, because both are text an operator could post or paste,
//! and either one fabricating a number, naming an accusation or reassuring
//! about a gap is the same failure `voice::write`'s own gate exists to catch
//! for the reply alone.

use std::fmt::Write as _;
use std::path::Path;

use realorrug_model::Provider;
use realorrug_roast::fidelity::Authorised;
use realorrug_roast::{Assessment, Capture};

/// Runs the command.
///
/// # Errors
///
/// A message when `<dir>` is missing, is not a directory, holds no
/// `*.sheet.json`, or when a capture cannot be parsed or `review.md` cannot
/// be written.
pub fn run(args: &[String]) -> Result<(), String> {
    let dir_arg =
        dir_arg_from(args).ok_or_else(|| "usage: realorrug replay <dir> [--model]".to_owned())?;
    let dir = Path::new(&dir_arg);
    if !dir.is_dir() {
        return Err(format!("cannot read {dir_arg}: no such directory"));
    }

    let provider: Option<Box<dyn Provider>> = if wants_model(args) {
        Some(
            realorrug_model::from_vars(&|k| std::env::var(k).ok())
                .map_err(|e| format!("--model was given but no provider is configured: {e}"))?,
        )
    } else {
        None
    };

    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read {dir_arg}: {e}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.to_string_lossy().ends_with(".sheet.json"))
        .collect();
    // Sorted, so a review written twice from the same directory reads in the
    // same order -- a `read_dir` order that changed between two runs would
    // otherwise make a diff of two `review.md`s noisy for no reason.
    paths.sort();

    if paths.is_empty() {
        return Err(format!("no *.sheet.json capture found in {dir_arg}"));
    }

    let mut out = String::new();
    let _ = writeln!(out, "# Replay review\n");
    for path in &paths {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let capture: Capture = serde_json::from_str(&text)
            .map_err(|e| format!("cannot parse {}: {e}", path.display()))?;
        render_case(&mut out, &capture, provider.as_deref());
    }

    let out_path = dir.join("review.md");
    std::fs::write(&out_path, out)
        .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
    println!("wrote {}", out_path.display());
    Ok(())
}

/// One check's name, whether it passed, and why -- the row `render_case`
/// writes for each of the three checks against each of the two texts.
struct CheckRow {
    name: &'static str,
    ok: bool,
    reason: String,
}

/// The three checks, run against one piece of text.
///
/// A free function rather than a method on `Capture`: it takes exactly the
/// two things a check needs (the numbers fidelity may cite, and the text)
/// and nothing else, so it reads the same whether the text is the reply or
/// the report.
fn checks_for(authorised: &[Authorised], text: &str) -> Vec<CheckRow> {
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
            name: "unsupported accusation",
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

/// Writes one case's section of `review.md`.
fn render_case(out: &mut String, capture: &Capture, provider: Option<&dyn Provider>) {
    let level = realorrug_roast::level(&capture.sheet);
    let assessment = Assessment::from(&capture.sheet);
    let report = realorrug_roast::report::build(&capture.sheet);
    let reply = realorrug_roast::voice::write(&capture.sheet, provider);

    let heading = if capture.label.is_empty() {
        capture.mint.clone()
    } else {
        format!("{} ({})", capture.label, capture.mint)
    };
    let _ = writeln!(out, "## {heading}\n");
    let _ = writeln!(out, "- mint: {}", capture.mint);
    let _ = writeln!(out, "- captured at: {}", capture.captured_at);
    if capture.rules_version == realorrug_roast::RULES_VERSION {
        let _ = writeln!(out, "- rules version: {}", capture.rules_version);
    } else {
        let _ = writeln!(
            out,
            "- rules version: {} -- **differs from current {}**",
            capture.rules_version,
            realorrug_roast::RULES_VERSION
        );
    }
    if capture.level == level {
        let _ = writeln!(out, "- level: {level:?}");
    } else {
        let _ = writeln!(
            out,
            "- level: saved {:?} -- **recomputed as {:?}**",
            capture.level, level
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "### Report\n");
    let _ = writeln!(out, "{}", report.render(&assessment));

    let _ = writeln!(out, "### Reply\n");
    let _ = writeln!(out, "{}\n", reply.text);

    let _ = writeln!(out, "### Checks\n");
    let sheet_authorised = capture.sheet.authorised();
    for row in checks_for(&sheet_authorised, &reply.text) {
        let mark = if row.ok { "PASS" } else { "FAIL" };
        let _ = writeln!(out, "- {mark} -- {} (reply) -- {}", row.name, row.reason);
    }
    // The report's own trailing line states `assessment.risk_index` and
    // `assessment.coverage` -- real numbers `report::build` computed from
    // `capture.sheet`, not a model's invention, but not in
    // `sheet.authorised()` either (that list is *sheet facts*, and a risk
    // index is a score built from them). Fidelity's job is catching a
    // number nothing measured; authorising these here says what they are
    // instead of asking fidelity to guess, the same way `unknown::check`'s
    // own test had to reword that same line rather than weaken the check.
    let mut report_authorised = sheet_authorised;
    report_authorised.push(Authorised::anywhere(f64::from(assessment.risk_index)));
    report_authorised.push(Authorised::anywhere(100.0));
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count of facts read is well inside f64's exact integer range and this is a \
                  comparison against a literal the report renders, not arithmetic"
    )]
    {
        report_authorised.push(Authorised::anywhere(assessment.coverage.read as f64));
        report_authorised.push(Authorised::anywhere(assessment.coverage.applicable as f64));
    }
    let report_text = report.render(&assessment);
    for row in checks_for(&report_authorised, &report_text) {
        let mark = if row.ok { "PASS" } else { "FAIL" };
        let _ = writeln!(out, "- {mark} -- {} (report) -- {}", row.name, row.reason);
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "accept? (yes/no, note): \n");
}

/// The directory a `realorrug replay` invocation names, if any.
///
/// Positional, the same shape `capture.rs`'s `mint_arg_from` uses for its own
/// first argument -- a flag in that slot is never mistaken for the
/// directory.
fn dir_arg_from(args: &[String]) -> Option<String> {
    args.get(1).filter(|a| !a.starts_with("--")).cloned()
}

/// Whether `--model` was given.
fn wants_model(args: &[String]) -> bool {
    args.iter().any(|a| a == "--model")
}

#[cfg(test)]
mod tests {
    use super::*;
    use realorrug_roast::{Assessment, FactSheet};

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_directory_is_the_first_bare_argument() {
        assert_eq!(
            dir_arg_from(&args(&["replay", "some/dir"])),
            Some("some/dir".to_owned())
        );
        assert_eq!(dir_arg_from(&args(&["replay", "--model"])), None);
        assert_eq!(dir_arg_from(&args(&["replay"])), None);
    }

    #[test]
    fn the_model_flag_is_read() {
        assert!(wants_model(&args(&["replay", "dir", "--model"])));
        assert!(!wants_model(&args(&["replay", "dir"])));
    }

    #[test]
    fn a_missing_directory_is_refused() {
        let dir = std::env::temp_dir().join("realorrug-replay-no-such-dir");
        let _ = std::fs::remove_dir_all(&dir);
        let err = run(&args(&["replay", dir.to_str().expect("path")])).expect_err("no dir");
        assert!(err.ends_with("no such directory"), "{err}");
    }

    fn empty_dossier() -> realorrug_onchain::Dossier {
        realorrug_onchain::Dossier {
            mint: "11111111111111111111111111111112"
                .parse()
                .expect("an address"),
            read_at: None,
            launch: None,
            curve: None,
            creator_transactions: None,
            chain_launch: None,
            holders: None,
            funding: None,
            market: None,
            token_ownership: None,
            creator_cash_flow: None,
            powers: None,
            unavailable: Vec::new(),
            calls: 0,
            elapsed_ms: 0,
        }
    }

    fn hand_built_capture(mint: &str, label: &str) -> Capture {
        let sheet = FactSheet::build(&empty_dossier(), None, None, None, None);
        let level = realorrug_roast::level(&sheet);
        let assessment = Assessment::from(&sheet);
        Capture {
            mint: mint.to_owned(),
            label: label.to_owned(),
            captured_at: "2026-09-21T00:00:00Z".to_owned(),
            rules_version: realorrug_roast::RULES_VERSION.to_owned(),
            level,
            assessment,
            sheet,
        }
    }

    /// A hand-built capture in a temp dir, replayed with no network and no
    /// `--model`: `review.md` is written, names the case, and states a
    /// level -- the no-network path the packet asks this test to cover.
    #[test]
    fn replay_reads_a_hand_built_capture_with_no_network() {
        let dir = std::env::temp_dir().join("realorrug-replay-hand-built");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");

        let capture = hand_built_capture("SoMeMiNt", "ordinary launch");
        let json = serde_json::to_string_pretty(&capture).expect("encode");
        std::fs::write(dir.join("SoMeMiNt.sheet.json"), json).expect("write capture");

        run(&args(&["replay", dir.to_str().expect("path")])).expect("replay");

        let review = std::fs::read_to_string(dir.join("review.md")).expect("review.md");
        assert!(review.contains("ordinary launch"), "{review}");
        assert!(review.contains("SoMeMiNt"), "{review}");
        assert!(review.contains("### Report"), "{review}");
        assert!(review.contains("### Reply"), "{review}");
        assert!(review.contains("### Checks"), "{review}");
        assert!(review.contains("accept?"), "{review}");
        // No provider was configured and `--model` was not given, so the
        // template shipped -- and the template is what `fidelity`/`forbidden`
        // are already proven to pass, so every check row reads PASS.
        assert!(!review.contains("FAIL"), "{review}");
    }

    /// A capture whose `rules_version` does not match the crate's current
    /// one is flagged, rather than silently compared as equal.
    #[test]
    fn a_rules_version_mismatch_is_flagged() {
        let dir = std::env::temp_dir().join("realorrug-replay-old-rules");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");

        let mut capture = hand_built_capture("SoMeMiNt", "");
        capture.rules_version = "1999-01-01".to_owned();
        let json = serde_json::to_string_pretty(&capture).expect("encode");
        std::fs::write(dir.join("SoMeMiNt.sheet.json"), json).expect("write capture");

        run(&args(&["replay", dir.to_str().expect("path")])).expect("replay");

        let review = std::fs::read_to_string(dir.join("review.md")).expect("review.md");
        assert!(review.contains("differs from current"), "{review}");
    }

    /// A saved level that today's rules no longer reach is called out, so the
    /// reviewer sees the verdict moved rather than reading the new one as the
    /// one that was captured.
    #[test]
    fn a_level_that_changed_since_capture_is_flagged() {
        let dir = std::env::temp_dir().join("realorrug-replay-level-moved");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");

        let mut capture = hand_built_capture("SoMeMiNt", "");
        assert_ne!(
            capture.level,
            realorrug_roast::Level::Rugged,
            "an empty sheet cannot be Rugged"
        );
        capture.level = realorrug_roast::Level::Rugged;
        let json = serde_json::to_string_pretty(&capture).expect("encode");
        std::fs::write(dir.join("SoMeMiNt.sheet.json"), json).expect("write capture");

        run(&args(&["replay", dir.to_str().expect("path")])).expect("replay");

        let review = std::fs::read_to_string(dir.join("review.md")).expect("review.md");
        assert!(
            review.contains("saved Rugged -- **recomputed as"),
            "{review}"
        );
    }

    /// A label-less capture heads its section with the mint alone, not an
    /// empty pair of parentheses.
    #[test]
    fn a_capture_with_no_label_headed_by_the_mint_alone() {
        let dir = std::env::temp_dir().join("realorrug-replay-no-label");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");

        let capture = hand_built_capture("SoMeMiNt", "");
        let json = serde_json::to_string_pretty(&capture).expect("encode");
        std::fs::write(dir.join("SoMeMiNt.sheet.json"), json).expect("write capture");

        run(&args(&["replay", dir.to_str().expect("path")])).expect("replay");

        let review = std::fs::read_to_string(dir.join("review.md")).expect("review.md");
        assert!(review.contains("## SoMeMiNt\n"), "{review}");
    }
}
