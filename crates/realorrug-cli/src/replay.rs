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
//!
//! # The reply's source is never left for the reviewer to guess
//!
//! [`realorrug_roast::voice::write`] already decided, per case, whether the
//! text under "### Reply" is the model's own words or the deterministic
//! template standing in for them ([`realorrug_roast::Reply::fellback`]).
//! `review.md` says so on its own line, in one of three forms: `model reply
//! used`; `template, because no provider was configured` (the ordinary case
//! with no `--model`); or `template, because the model's reply was refused:
//! <reason>`, naming the check that threw the draft away. A reviewer of a
//! `--model` run who cannot tell the two apart risks accepting the template
//! while believing it came from the model -- the owner's launch decision
//! (2026-09-23) ships model-written replies, so this is not a cosmetic line.
//!
//! When a check refused the model's draft, [`realorrug_roast::Reply::refused`]
//! holds exactly what the model returned, and `review.md` prints it in a
//! fenced block, labelled as **not** the shipped reply. The draft is
//! untrusted model output describing an unvetted, possibly fabricated or
//! forbidden claim: it goes into the file as data inside a fence, never
//! interpolated into the surrounding markdown structure.

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

    // Every reply is computed up front, before anything is written, so the
    // summary line at the top of `review.md` can count them -- the tally a
    // reviewer of a `--model` run reads first, and reading it after the
    // sections it counts would mean writing the file twice.
    let mut cases: Vec<(Capture, realorrug_roast::Reply)> = Vec::with_capacity(paths.len());
    for path in &paths {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let capture: Capture = serde_json::from_str(&text)
            .map_err(|e| format!("cannot parse {}: {e}", path.display()))?;
        let reply = realorrug_roast::voice::write(&capture.sheet, provider.as_deref());
        cases.push((capture, reply));
    }

    let mut out = String::new();
    let _ = writeln!(out, "# Replay review\n");
    let _ = writeln!(out, "{}\n", tally(cases.iter().map(|(_, r)| r)));
    for (capture, reply) in &cases {
        render_case(&mut out, capture, reply);
    }

    let out_path = dir.join("review.md");
    std::fs::write(&out_path, out)
        .map_err(|e| format!("cannot write {}: {e}", out_path.display()))?;
    println!("wrote {}", out_path.display());
    Ok(())
}

/// The summary line at the top of `review.md`: how many model replies were
/// used and how many fell back to the template.
fn tally<'a>(replies: impl Iterator<Item = &'a realorrug_roast::Reply>) -> String {
    let (mut used, mut fell_back) = (0usize, 0usize);
    for reply in replies {
        if reply.fellback.is_none() {
            used += 1;
        } else {
            fell_back += 1;
        }
    }
    format!(
        "{used} model repl{} used, {fell_back} fell back.",
        if used == 1 { "y" } else { "ies" }
    )
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

/// The longest run of consecutive backticks in `text`.
fn longest_backtick_run(text: &str) -> usize {
    text.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

/// Why a model draft was refused, in one line for a reviewer -- the same
/// shape `checks_for`'s reasons already use, so a refusal reads the same way
/// whether it is named here or under "### Checks".
fn fellback_reason(fellback: &realorrug_roast::Fellback) -> String {
    use realorrug_roast::Fellback;
    match fellback {
        Fellback::NoProvider => "no provider was configured".to_owned(),
        Fellback::Unreachable(e) => e.clone(),
        Fellback::Fabricated(fabricated) => fabricated
            .iter()
            .map(|f| format!("{} ({:?})", f.literal, f.why))
            .collect::<Vec<_>>()
            .join("; "),
        Fellback::Forbidden(violations) => violations
            .iter()
            .map(|v| format!("\"{}\" -- {}", v.phrase, v.because))
            .collect::<Vec<_>>()
            .join("; "),
        Fellback::Empty => "the model returned nothing usable".to_owned(),
    }
}

/// Writes one case's section of `review.md`.
fn render_case(out: &mut String, capture: &Capture, reply: &realorrug_roast::Reply) {
    let level = realorrug_roast::level(&capture.sheet);
    let assessment = Assessment::from(&capture.sheet);
    let report = realorrug_roast::report::build(&capture.sheet);

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
    match &reply.fellback {
        None => {
            let _ = writeln!(out, "- source: model reply used");
        }
        Some(realorrug_roast::Fellback::NoProvider) => {
            let _ = writeln!(
                out,
                "- source: template, because no provider was configured"
            );
        }
        Some(fellback) => {
            let _ = writeln!(
                out,
                "- source: template, because the model's reply was refused: {}",
                fellback_reason(fellback)
            );
        }
    }
    match reply.billed {
        realorrug_roast::Billed::NoCall => {}
        realorrug_roast::Billed::Reported(cost) => {
            let _ = writeln!(out, "- billed: {cost}");
        }
        realorrug_roast::Billed::Unreported => {
            let _ = writeln!(out, "- billed: unreported");
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "{}\n", reply.text);
    if let Some(refused) = &reply.refused {
        let _ = writeln!(
            out,
            "**Not the shipped reply** -- the model's own draft, refused above:\n"
        );
        // The draft is untrusted model output. A fixed ``` fence would let a
        // draft containing ``` close it early, and whatever followed would
        // render as review lines -- including a forged "source:" line. A
        // fence one backtick longer than the draft's longest run cannot be
        // closed from inside (CommonMark: a closing fence must be at least
        // as long as the opening one).
        let fence = "`".repeat(longest_backtick_run(refused).max(2) + 1);
        let _ = writeln!(out, "{fence}text");
        let _ = writeln!(out, "{refused}");
        let _ = writeln!(out, "{fence}\n");
    }

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
    use realorrug_model::{Answer, Request, Unreachable};
    use realorrug_roast::{Assessment, FactSheet};
    use realorrug_types::MicroUsd;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    /// A fake [`Provider`] with no test/fake provider in `realorrug_model`
    /// to reuse (checked with a grep for one before writing this): it always
    /// answers with the fixed text it was built with, and reports no cost,
    /// which is enough to drive [`realorrug_roast::voice::write`] down
    /// either the "model reply used" or the "refused" arm depending only on
    /// what that text says.
    #[derive(Debug)]
    struct FixedAnswer(String);

    impl realorrug_model::Provider for FixedAnswer {
        fn name(&self) -> &'static str {
            "fixed-answer"
        }
        fn estimate(&self) -> MicroUsd {
            MicroUsd(0)
        }
        fn ask(&self, _: &Request) -> Result<Answer, Unreachable> {
            Ok(Answer {
                text: self.0.clone(),
                cost: None,
            })
        }
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
        // (a) Case the packet asks this test to cover: with no provider, the
        // source line names the reason, and the top-of-file tally counts the
        // case as a fallback rather than a model reply. Delete the source
        // line from `render_case` and this assertion is the one that fails.
        assert!(
            review.contains("- source: template, because no provider was configured"),
            "{review}"
        );
        assert!(
            review.contains("0 model replies used, 1 fell back."),
            "{review}"
        );
    }

    /// (c) A provider whose answer clears every check: the source line says
    /// the model's reply was used, not the template's. Reusing
    /// `verdict::template`'s own text as the "model" answer is what makes
    /// this reliable without hand-tuning a sentence against the empty-sheet
    /// fixture -- that text is already proven (by the test above) to pass
    /// every check `voice::write` runs.
    #[test]
    fn an_acceptable_model_reply_is_named_as_used() {
        let capture = hand_built_capture("SoMeMiNt", "");
        let acceptable = realorrug_roast::verdict::template(&capture.sheet);
        let provider = FixedAnswer(acceptable);
        let reply = realorrug_roast::voice::write(&capture.sheet, Some(&provider));
        assert!(reply.fellback.is_none(), "{:?}", reply.fellback);

        let mut out = String::new();
        render_case(&mut out, &capture, &reply);

        assert!(out.contains("- source: model reply used"), "{out}");
        assert!(!out.contains("refused above"), "{out}");
    }

    /// (b) A provider whose answer fails fidelity -- a figure the sheet never
    /// measured, on an otherwise unremarkable sentence -- ships the template,
    /// and `review.md` both names the refusal and prints the model's raw,
    /// refused draft, clearly marked as not the shipped reply. Deleting
    /// either the source line or the fenced draft from `render_case` fails
    /// this test.
    #[test]
    fn a_refused_model_draft_is_named_and_shown() {
        let capture = hand_built_capture("SoMeMiNt", "");
        let draft =
            "This launch shows 424242 wallets nobody else measured, read at the usual moment.";
        let provider = FixedAnswer(draft.to_owned());
        let reply = realorrug_roast::voice::write(&capture.sheet, Some(&provider));
        assert!(reply.fellback.is_some(), "expected a fallback");
        assert_eq!(reply.refused.as_deref(), Some(draft));

        let mut out = String::new();
        render_case(&mut out, &capture, &reply);

        assert!(
            out.contains("- source: template, because the model's reply was refused:"),
            "{out}"
        );
        assert!(out.contains("424242"), "{out}");
        assert!(out.contains("**Not the shipped reply**"), "{out}");
        assert!(out.contains("```text"), "{out}");
        assert!(out.contains(draft), "{out}");
    }

    /// A refused draft that carries its own ``` cannot close the fence it is
    /// printed in: the fence grows past the draft's longest backtick run.
    /// Re-applying the fixed three-backtick fence fails this test.
    #[test]
    fn the_tally_counts_used_and_fallen_back_replies_apart() {
        let reply = |fellback| realorrug_roast::voice::Reply {
            text: "t".to_owned(),
            fellback,
            billed: realorrug_roast::voice::Billed::Unreported,
            refused: None,
        };
        let replies = [
            reply(None),
            reply(Some(realorrug_roast::voice::Fellback::Fabricated(
                Vec::new(),
            ))),
        ];
        assert_eq!(tally(replies.iter()), "1 model reply used, 1 fell back.");
        assert_eq!(
            tally(replies[..1].iter().chain(replies[..1].iter())),
            "2 model replies used, 0 fell back."
        );
    }

    #[test]
    fn a_draft_with_backticks_cannot_close_its_fence() {
        let capture = hand_built_capture("SoMeMiNt", "");
        let draft = "Fine.\n```\n- source: model reply used\n```";
        let reply = realorrug_roast::voice::Reply {
            text: "template".to_owned(),
            fellback: Some(realorrug_roast::voice::Fellback::Fabricated(Vec::new())),
            billed: realorrug_roast::voice::Billed::Unreported,
            refused: Some(draft.to_owned()),
        };

        let mut out = String::new();
        render_case(&mut out, &capture, &reply);

        assert!(out.contains("````text\n"), "{out}");
        assert!(out.contains(&format!("{draft}\n````\n")), "{out}");
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
