// SPDX-License-Identifier: Apache-2.0
//! Assertions that the repository's claims about itself are true.
//!
//! Documentation rots quietly. A crate table listing something that does not
//! exist, a link to a moved ADR, a rule citing a file nobody kept — none of those
//! break a build, and all of them cost the reader the benefit of the doubt on
//! everything else in the document.
//!
//! This crate is why `AGENTS.md` can say "verify before you claim" and mean it.
//! It exists because the failure it prevents already happened once, in a sibling
//! repository, and is recorded in `LEARNINGS.md`: a design was documented as
//! canonical, citing functions in a file that was not in the working tree and not
//! in git.
//!
//! **Carried from Radar on 2026-09-13** ([ADR 0024]). The checks tied to
//! Radar's own files -- its signer, its risk policy, `LEARNINGS.md`,
//! `docs/STATE.md`, its required-checks list -- were left behind; a `LEARNINGS`
//! or `design 0004` in a comment below names Radar's document, in
//! github.com/1xmint/theradar. The payout-reach check is new here.
//!
//! [ADR 0024]: https://github.com/1xmint/realorrug/blob/main/docs/adr/0024-the-bot-stands-alone.md
//!
//! In Radar it caught its own version of that on the first run. Three crate directories
//! existed with no manifest — including this one — and the two that were never
//! going to be written were deleted rather than left to look like work in
//! progress.
//!
//! # What is checked
//!
//! Only things that are mechanically decidable. A test that tries to judge
//! whether prose is *accurate* would either be wrong or be a second copy of the
//! prose; these check that every artefact the prose points at is really there.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The repository root, found by walking up from this crate.
///
/// # Panics
///
/// Panics if the root cannot be located, which means the crate has been moved
/// and every path below it is wrong anyway.
#[must_use]
pub fn root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while !dir.join("Cargo.toml").exists() || !dir.join("crates").is_dir() {
        assert!(dir.pop(), "no repository root above the manifest directory");
    }
    dir
}

/// Directories under `crates/`.
///
/// # Panics
///
/// Panics if `crates/` cannot be read.
#[must_use]
pub fn crate_directories() -> BTreeSet<String> {
    std::fs::read_dir(root().join("crates"))
        .expect("crates/ must be readable")
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect()
}

/// Crates listed in the workspace `members` array.
///
/// Parsed by hand rather than with a TOML crate: this is one array of quoted
/// strings on one line, and a dependency added to check a dependency list is a
/// dependency that has to be justified.
///
/// # Panics
///
/// Panics if the root manifest has no `members` array.
#[must_use]
pub fn workspace_members() -> BTreeSet<String> {
    let manifest = std::fs::read_to_string(root().join("Cargo.toml"))
        .expect("the root manifest must be readable");
    let start = manifest
        .find("members = [")
        .expect("the root manifest must declare workspace members");
    let rest = &manifest[start..];
    let end = rest.find(']').expect("the members array must be closed");

    rest[..end]
        .split('"')
        .filter(|s| s.starts_with("crates/"))
        .map(|s| s.trim_start_matches("crates/").to_owned())
        .collect()
}

/// Relative markdown links found in a document, excluding anchors and URLs.
#[must_use]
pub fn relative_links(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes: Vec<char> = markdown.chars().collect();
    let mut i = 0;

    // Bounded by the input length rather than by trusting the cursor to
    // advance -- see `test_paths` below, which had the same shape and was
    // reported by CI as two five-minute timeouts before it was changed.
    for _ in 0..=bytes.len() {
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == ']' && bytes.get(i + 1) == Some(&'(') {
            let mut j = i + 2;
            let mut target = String::new();
            for _ in 0..=bytes.len() {
                if j >= bytes.len() || bytes[j] == ')' {
                    break;
                }
                target.push(bytes[j]);
                j += 1;
            }
            i = j;
            let target = target.split_whitespace().next().unwrap_or("").to_owned();
            if !target.is_empty()
                && !target.starts_with('#')
                && !target.contains("://")
                && !target.starts_with("mailto:")
            {
                // Strip a trailing anchor: the file is what must exist.
                out.push(target.split('#').next().unwrap_or(&target).to_owned());
            }
        }
        i += 1;
    }
    out
}

/// Every markdown document in the repository.
///
/// **Discovered, not listed.** This was a hand-written enumeration of four root
/// files plus `docs/adr`, `docs/research` and `deploy`, and `docs/` itself was
/// not among them — so `docs/STATE.md` was created, committed, and checked by
/// nothing. Fifteen of its links were broken on the day it landed, because it
/// had been cut out of root-level `AGENTS.md` and its `docs/...` paths were
/// never re-based; every conformance rule passed, because none of them looked.
///
/// A list of what to check is a second thing to keep in sync with the tree, and
/// it fails silently in the direction that reports success. Deriving it from
/// [`known_files`] cannot go stale: a document added anywhere is checked from
/// the commit that adds it.
/// Every markdown file tracked in the repository's documented areas.
///
/// # Panics
///
/// Panics if a directory that should exist cannot be read.
#[must_use]
pub fn documents() -> Vec<PathBuf> {
    let root = root();
    let mut out: Vec<PathBuf> = known_files()
        .iter()
        // Extension rather than a suffix match, and case-insensitively. On
        // Windows `AGENTS.MD` and `AGENTS.md` are the same file, and this
        // repository has already lost a document to exactly that collision --
        // so a `.MD` is a document worth checking, not one worth skipping.
        .filter(|f| {
            Path::new(f.as_str())
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        })
        .map(|f| root.join(f))
        .collect();
    out.retain(|p| p.exists());
    out.sort();
    out
}

/// Resolves a link found in `document` against the repository.
#[must_use]
pub fn resolve(document: &Path, link: &str) -> PathBuf {
    document
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(link)
}

/// Every file this repository is made of, relative to its root, `/`-separated.
///
/// Git first, because the failure this supports is a file that exists on
/// somebody's disk and in no commit — and only git can tell those apart.
///
/// Falling back to walking the tree when git cannot answer is deliberate and is
/// weaker on purpose. `cargo mutants` builds in a copy with no `.git`, and a
/// check that skipped itself there would be vacuous exactly where it is least
/// observed. Existence is a smaller claim than tracked-ness, and it is a claim.
#[must_use]
pub fn known_files() -> BTreeSet<String> {
    let from_git = std::process::Command::new("git")
        .arg("-C")
        .arg(root())
        .args(["ls-files"])
        .output();
    if let Ok(out) = from_git
        && out.status.success()
    {
        {
            let files = parse_ls_files(&String::from_utf8_lossy(&out.stdout));
            // A mutant deleting this `!` survives, and it is worth writing down
            // rather than chasing: inverted, a successful `git ls-files` falls
            // through to walking the tree, which produces an equally usable
            // list. The two branches disagree about *provenance* — tracked
            // versus merely present — and no test can see that difference from
            // inside a checkout where everything present is also tracked.
            if !files.is_empty() {
                return files;
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(&root(), &root(), &mut out);
    out
}

/// Parses `git ls-files` output into repository-relative paths.
///
/// Split out from the process call because the process call cannot be tested and
/// this can. Blank lines are dropped and separators normalised to `/`, both of
/// which matter: an empty entry would match every suffix query and quietly make
/// the check that uses this pass for any path at all.
#[must_use]
pub fn parse_ls_files(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .map(|l| l.trim().replace('\\', "/"))
        .filter(|l| !l.is_empty())
        .collect()
}

/// Collects every file under `dir` as a path relative to `base`.
fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut BTreeSet<String>) {
    // `target` is build output and `.git` is not source. Descending into either
    // would take minutes and find nothing a document would ever name.
    const SKIP: &[&str] = &["target", ".git", "node_modules"];
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if SKIP.iter().any(|s| *s == name) {
            continue;
        }
        if path.is_dir() {
            walk(&path, base, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Paths named inside backticks in a document.
///
/// Deliberately conservative. A code span holds all sorts of things — type
/// names, flags, function calls — and flagging those would make the check
/// noisy enough to be turned off, which is worse than not having it. So a span
/// counts as a path only if it looks like one and nothing else: it contains a
/// `/` or a known file extension, has no whitespace, parentheses or leading
/// dashes, and does not end in `()`.
#[must_use]
pub fn code_span_paths(text: &str) -> BTreeSet<String> {
    const EXTENSIONS: &[&str] = &[".rs", ".toml", ".md", ".yml", ".yaml", ".service", ".timer"];
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else { break };
        let span = &after[..end];
        rest = &after[end + 1..];

        let looks_like_a_path = (span.contains('/')
            || EXTENSIONS.iter().any(|e| span.ends_with(e)))
            && !span.contains(char::is_whitespace)
            && !span.contains('(')
            && !span.starts_with('-')
            && !span.contains('*')
            // `other-repo:path/to/file` names a file somewhere else, and a
            // leading `/` names one on a machine. Neither is a claim about this
            // repository, and both are how such a claim should be written --
            // which is the point, because LEARNINGS entry 1 is about a citation
            // that gave a reader no way to tell.
            && !span.contains(':')
            && !span.starts_with('/');
        // A bare `a/b` with no extension is as likely to be prose as a path.
        let has_extension = EXTENSIONS.iter().any(|e| span.ends_with(e));
        if looks_like_a_path && has_extension {
            out.insert(span.to_owned());
        }
    }
    out
}

/// Whether `crate_name` lists `dependency` outside `[dev-dependencies]`.
///
/// The distinction is load-bearing rather than pedantic. A test may reach
/// anything it needs to check, and a rule that read the whole manifest would
/// forbid the test that enforces the rule. What ships is what sits above that
/// header.
#[must_use]
pub fn production_dependency(crate_name: &str, dependency: &str) -> bool {
    let manifest = root().join("crates").join(crate_name).join("Cargo.toml");
    std::fs::read_to_string(&manifest).is_ok_and(|text| {
        text.lines()
            .take_while(|l| !l.trim_start().starts_with("[dev-dependencies]"))
            .any(|l| l.split('#').next().unwrap_or("").contains(dependency))
    })
}

/// Markdown files present in the working tree that `git` does not know about.
///
/// Every check in this crate reads [`known_files`], which is `git ls-files`. A
/// markdown document is therefore checked by *nothing* until it is added to the
/// index -- its links are not resolved, the paths it names are not verified, its
/// status field is not required. This was found by verifying design `0004`,
/// which passed only once it had been `git add -N`'d, and it is the same hole
/// the justfile already closes for Rust.
///
/// `--exclude-standard` means `.gitignore` is the escape hatch: a scratch file
/// that should not be checked should say so there.
#[must_use]
pub fn untracked_documents() -> Vec<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root())
        .args(["ls-files", "--others", "--exclude-standard"])
        .output();
    let Ok(out) = out else { return Vec::new() };
    if !out.status.success() {
        return Vec::new();
    }
    parse_ls_files(&String::from_utf8_lossy(&out.stdout))
        .into_iter()
        .filter(|f| {
            Path::new(f.as_str())
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        })
        .collect()
}

/// Shell lines a workflow actually runs, with comments and YAML keys dropped.
///
/// Deliberately crude, and crude in the safe direction. It keeps every line
/// inside a `run:` block and drops anything after a `#`, so a rule built on it
/// reads what a runner would execute rather than what a comment says about it.
/// A `#` inside a quoted shell string would be dropped too; that under-reports,
/// which is the direction a heuristic in a conformance check should fail in.
#[must_use]
pub fn workflow_run_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut indent = None;
    for line in text.lines() {
        let depth = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        // A `run:` on one line carries its command with it; a `run: |` opens a
        // block that continues while the indentation stays deeper than the key.
        //
        // The `- ` is stripped first because a `run:` is usually the first key
        // of a list item and is written `- run:`. Missing that made the first
        // version of this find nothing at all, which the test below caught.
        let key = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if let Some(rest) = key.strip_prefix("run:") {
            let rest = rest.trim();
            indent = Some(depth);
            if !rest.is_empty() && rest != "|" && rest != ">" {
                out.push(rest.to_string());
            }
            continue;
        }
        match indent {
            Some(_) if trimmed.is_empty() => {}
            Some(open) if depth > open => {
                let code = trimmed.split('#').next().unwrap_or("").trim();
                if !code.is_empty() {
                    out.push(code.to_string());
                }
            }
            Some(_) => indent = None,
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_crate_directory_is_a_workspace_member() {
        // The check that would have caught three empty scaffolds sitting in
        // crates/ looking like work in progress, one of which was this crate.
        let members = workspace_members();
        let missing: Vec<String> = crate_directories()
            .into_iter()
            .filter(|name| !members.contains(name))
            .collect();
        assert!(
            missing.is_empty(),
            "these directories are in crates/ but not workspace members: {missing:?}\n\
             Either add them to the members array, or delete them — a directory named \
             after a crate that does not exist reads as work in progress."
        );
    }

    #[test]
    fn every_workspace_member_has_a_manifest() {
        for name in workspace_members() {
            let manifest = root().join("crates").join(&name).join("Cargo.toml");
            assert!(
                manifest.exists(),
                "workspace member `{name}` has no Cargo.toml at {}",
                manifest.display()
            );
        }
    }

    #[test]
    fn every_crate_has_source() {
        // A manifest with no source is a crate that compiles to nothing while
        // appearing in every listing.
        for name in workspace_members() {
            let src = root().join("crates").join(&name).join("src");
            let has_source = std::fs::read_dir(&src).is_ok_and(|entries| {
                entries
                    .filter_map(Result::ok)
                    .any(|e| e.path().extension().is_some_and(|x| x == "rs"))
            });
            assert!(has_source, "crate `{name}` has no Rust source in src/");
        }
    }

    #[test]
    fn every_relative_link_in_the_documentation_resolves() {
        // The failure this crate exists for: a document citing a file that is
        // not in the tree and not in git.
        let mut broken = Vec::new();
        for document in documents() {
            let Ok(text) = std::fs::read_to_string(&document) else {
                continue;
            };
            for link in relative_links(&text) {
                let target = resolve(&document, &link);
                if !target.exists() {
                    broken.push(format!("{} -> {link}", document.display()));
                }
            }
        }
        assert!(
            broken.is_empty(),
            "broken links:\n  {}",
            broken.join("\n  ")
        );
    }

    #[test]
    fn every_adr_referenced_by_number_exists() {
        // ADRs are cited as "ADR 0003" in prose as often as they are linked, and
        // a number with no file behind it is worse than no citation.
        let adr_dir = root().join("docs/adr");
        let numbers: BTreeSet<String> = std::fs::read_dir(&adr_dir)
            .expect("docs/adr must be readable")
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|name| name.split('-').next().map(ToOwned::to_owned))
            .filter(|n| n.len() == 4 && n.chars().all(|c| c.is_ascii_digit()))
            .collect();

        let mut missing = Vec::new();
        for document in documents() {
            let Ok(text) = std::fs::read_to_string(&document) else {
                continue;
            };
            for (index, _) in text.match_indices("ADR ") {
                let cited: String = text[index + 4..]
                    .chars()
                    .take(4)
                    .filter(char::is_ascii_digit)
                    .collect();
                // "Radar ADR 0007" cites Radar's record, which stayed behind at
                // the split (ADR 0024). The prefix is how a document says so.
                let radars = text[..index].ends_with("Radar ");
                if cited.len() == 4 && !radars && !numbers.contains(&cited) {
                    missing.push(format!("{} cites ADR {cited}", document.display()));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "citations with no ADR behind them:\n  {}",
            missing.join("\n  ")
        );
    }

    #[test]
    fn the_readme_crate_table_matches_the_workspace() {
        // A table listing a crate that does not exist, or omitting one that
        // does, is the most-read wrong thing in the repository.
        let readme =
            std::fs::read_to_string(root().join("README.md")).expect("README must be readable");
        for name in workspace_members() {
            if name == "repo-conformance" {
                // Deliberately absent from the table: it ships no capability,
                // and listing the invigilator among the players reads oddly.
                continue;
            }
            assert!(
                readme.contains(&format!("`{name}`"))
                    || readme.contains(&format!("`crates/{name}`")),
                "crate `{name}` exists but the README's table does not mention it"
            );
        }
    }

    #[test]
    fn the_deploy_units_referenced_by_the_deploy_guide_exist() {
        // Instructions that install a file which is not there fail on a
        // production box, at the worst moment, in front of whoever was trusted
        // with the access.
        let guide = root().join("deploy/README.md");
        let Ok(text) = std::fs::read_to_string(&guide) else {
            return;
        };
        let mut missing = Vec::new();
        for (index, _) in text.match_indices("deploy/") {
            // `theradar:deploy/x.timer` names a file in another repository, the
            // same spelling `code_span_paths` already treats as external.
            if text[..index].ends_with(':') {
                continue;
            }
            let path: String = text[index..]
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '`' && *c != ')')
                .collect();
            if !root().join(&path).exists() {
                missing.push(path);
            }
        }
        assert!(
            missing.is_empty(),
            "the deploy guide names files that do not exist: {missing:?}"
        );
    }

    /// Every workspace crate `name` reaches through production dependencies,
    /// itself included.
    fn reaches(name: &str) -> BTreeSet<String> {
        let members = workspace_members();
        let mut seen = BTreeSet::from([name.to_owned()]);
        let mut todo = vec![name.to_owned()];
        while let Some(next) = todo.pop() {
            for member in &members {
                if !seen.contains(member) && production_dependency(&next, &format!("{member}.")) {
                    seen.insert(member.clone());
                    todo.push(member.clone());
                }
            }
        }
        seen
    }

    #[test]
    fn no_crate_that_holds_a_model_can_reach_the_payout() {
        // AGENTS.md rule 1: model judgement never moves money, and a path from
        // a model to the payout key is wrong. Radar held this for its signer
        // with a manifest check; this is the same check for the payout, and it
        // follows dependencies all the way down, because a path through a
        // third crate is still a path.
        //
        // `realorrug-cli` is the one crate outside this rule, and it is named
        // rather than hidden: it is the operator's binary, and it carries both
        // `roast` (a model) and the contest's payout fallback. Two subcommands
        // of one process started by a person, not a model deciding to pay. If
        // that ever changes, this is the line to re-read.
        const MODEL_SIDE: &[&str] = &[
            "realorrug-agent",
            "realorrug-model",
            "realorrug-provider",
            "realorrug-roast",
            "realorrug-analyst",
            "realorrug-serve",
        ];
        const PAYOUT: &str = "realorrug-payout";

        let payout_reaches = reaches(PAYOUT);
        assert!(
            payout_reaches.len() > 1,
            "the payout reaches nothing, so the dependency reader is broken and this check would pass vacuously"
        );
        for model in MODEL_SIDE {
            assert!(
                !reaches(model).contains(PAYOUT),
                "{model} reaches {PAYOUT}; a crate a model sits behind must have no path to the payout (AGENTS.md rule 1)"
            );
            assert!(
                !payout_reaches.contains(*model),
                "{PAYOUT} reaches {model}; the payout pays what the contest's pure rule permits and must not reach a model (AGENTS.md rule 1)"
            );
        }
        // And the reader sees a real edge, or both loops above prove nothing.
        assert!(reaches("realorrug-cli").contains(PAYOUT));
        assert!(reaches("realorrug-analyst").contains("realorrug-model"));
    }

    #[test]
    fn no_workflow_runs_the_node_toolchain_itself() {
        // On 2026-09-04 `just web` was taught to retry `npm audit` when the
        // registry's advisory endpoint is unreachable -- a vulnerability and an
        // unreachable registry are different answers, and the exit code alone
        // does not tell them apart. The `web` job went green with the fix. The
        // release job failed eight minutes later, on a 503 from that same
        // endpoint, because it held its own inline copy of the three commands
        // and the fix had landed in the recipe.
        //
        // So the rule is narrow and it is exactly the failure: the Node
        // toolchain has one definition, in the `web` recipe, and a workflow
        // reaches it through `just`. Anything a future job needs from npm is a
        // change to that recipe, which is also what makes this check cheap --
        // there is no reasonable change it fires on.
        //
        // `cargo` is deliberately NOT included. `release-linux` builds the
        // binaries directly and should: that is a release artifact rather than
        // a check, and no recipe owns it.
        let dir = root().join(".github/workflows");
        let mut offenders = Vec::new();
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).expect(".github/workflows must be readable") {
            let path = entry.expect("a readable directory entry").path();
            if path.extension().is_none_or(|e| e != "yml") {
                continue;
            }
            seen += 1;
            let text = std::fs::read_to_string(&path).expect("a readable workflow");
            for line in workflow_run_lines(&text) {
                if line.split_whitespace().next() == Some("npm") {
                    offenders.push(format!("{}: {line}", path.display()));
                }
            }
        }
        // Without this the check passes when the directory is empty, misread or
        // renamed -- which is LEARNINGS 5's shape, an absent answer reported the
        // same way as a clean one.
        assert!(
            seen > 0,
            "no workflows were read; the check would pass vacuously"
        );
        assert!(
            offenders.is_empty(),
            "a workflow runs npm directly instead of `just site`: {}. The recipe owns the Node toolchain, and a second copy is a fix that lands in only one of them.",
            offenders.join(", ")
        );
    }

    #[test]
    fn a_run_block_is_read_as_commands_and_a_comment_is_not_one() {
        // The rule above is only as good as this: a comment that *mentions* the
        // command it forbids must not be read as running it, and the comment
        // written beside that fix does mention it.
        let one = |t: &str| workflow_run_lines(t);

        assert_eq!(one("      - run: just web"), vec!["just web"]);
        assert_eq!(
            one("      - run: |
          npm ci
          npm run build
"),
            vec!["npm ci", "npm run build"]
        );
        // A comment inside the block, and a trailing comment on a real command.
        assert_eq!(
            one("      - run: |
          # npm ci is what this replaced
          just web # not npm
"),
            vec!["just web"]
        );
        // A comment *outside* any run block, which is where the explanation for
        // the fix actually lives.
        assert!(
            one("      # this step used to run npm ci inline
      - uses: actions/checkout@v5")
            .is_empty()
        );
        // A blank line inside a block does not end it. YAML block scalars
        // allow them and a long `run:` uses them to group commands, so reading
        // one as the end of the block would silently stop checking everything
        // after it -- which is the worst way for this to be wrong, because the
        // check would still pass. CI found this one: the guard that skips a
        // blank line survived mutation to `false`, and with it false a blank
        // line falls through to the arm that closes the block.
        assert_eq!(
            one("      - run: |
          npm ci

          npm run build
"),
            vec!["npm ci", "npm run build"]
        );
        // The block ends when the indentation returns to the key's level.
        assert_eq!(
            one("      - run: |
          npm ci
      - uses: actions/checkout@v5
"),
            vec!["npm ci"]
        );
        assert!(one("").is_empty());
    }

    #[test]
    fn no_markdown_document_is_invisible_to_these_checks() {
        // Declared first: an item after a statement is a clippy warning, and CI
        // builds with `-D warnings`.
        struct Probe(PathBuf);
        impl Drop for Probe {
            fn drop(&mut self) {
                // Best effort on purpose: a failed assertion must not be
                // replaced by a panic-in-drop about the cleanup.
                let _ = std::fs::remove_file(&self.0);
            }
        }

        // The hole this closes is not hypothetical and was not found by
        // reasoning: design 0004 was written, run against this suite, and
        // passed -- because the suite could not see it. It passed for real only
        // after `git add -N`. Everything in this crate reads `git ls-files`, so
        // an unstaged document is checked by nothing at the moment when its
        // links and its claims are most likely to be wrong.
        //
        // The justfile already guards the identical hole for Rust. This is that
        // guard for prose.
        let untracked = untracked_documents();
        assert!(
            untracked.is_empty(),
            "these markdown files exist but git does not know about them, so every check in this crate silently skipped them: {untracked:?} -- run `git add -N <path>` to make them visible, or add them to .gitignore to say they are not documents"
        );

        // And the other half, in the same test rather than its own, because a
        // second test creating an untracked file would race the assertion above.
        //
        // The assertion above is satisfied by a function that always returns
        // nothing, which is exactly what CI reported: two survivors that gutted
        // `untracked_documents` and nothing failed. A rule that cannot tell "no
        // untracked documents" from "I did not look" is LEARNINGS 5, and this
        // crate exists to catch that shape.
        // Skipped where there is no git directory to ask -- under `cargo
        // mutants`, which works in a copy of the sources without one. There
        // `untracked_documents` is dead whatever its body says, so failing here
        // would be failing for the environment. `.cargo/mutants.toml` records
        // the two mutants that consequently survive a mutation run, as
        // unreachable in that sandbox rather than as equivalent: they are not
        // equivalent, they disable the rule.
        //
        // Written inline rather than as a `git_is_usable()` helper, which was
        // the first attempt: a function whose only job is to gate a test is a
        // function whose mutation to `false` skips the test and can never be
        // killed. Introducing an unkillable mutant to kill two others is a bad
        // trade.
        if !root().join(".git").exists() {
            return;
        }

        let probe = Probe(root().join("zz-untracked-probe.md"));
        std::fs::write(&probe.0, "# not a document, a probe\n").expect("write the probe");
        let seen = untracked_documents();
        assert!(
            seen.iter().any(|f| f.ends_with("zz-untracked-probe.md")),
            "untracked_documents did not report a markdown file that is present and untracked, so the assertion above proves nothing. It saw: {seen:?}"
        );
    }

    #[test]
    fn every_numbered_document_declares_a_status() {
        // Design 0004 measured this convention at 38 of 48: every ADR carried
        // `**Status:**` and ten research notes did not -- and the ten were the
        // oldest, which is to say the ones most likely to have been overtaken.
        // A reader cannot tell an old note that still holds from one that was
        // corrected two months ago, and the notes that were corrected are
        // exactly the ones a fresh session is most likely to reason from.
        //
        // The field is deliberately free text rather than an enum. In
        // `docs/adr/` it says accepted or superseded; in `docs/research/` it
        // says how strongly the thing was measured, which is a sentence and not
        // a keyword. What is checkable is that the question was answered at all,
        // near the top, where somebody skimming will see it.
        const HEADER_LINES: usize = 15;

        let mut missing = Vec::new();
        for dir in ["docs/adr", "docs/design", "docs/research"] {
            let path = root().join(dir);
            let entries = std::fs::read_dir(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            for entry in entries {
                let file = entry.expect("directory entry").path();
                if file.extension().is_none_or(|e| e != "md") {
                    continue;
                }
                // `README.md` describes a directory rather than making a claim
                // in it, so there is nothing for a status to be about.
                if file.file_name().is_some_and(|n| n == "README.md") {
                    continue;
                }
                let text = std::fs::read_to_string(&file)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
                if !text
                    .lines()
                    .take(HEADER_LINES)
                    .any(|l| l.contains("**Status:**"))
                {
                    missing.push(format!("{dir}/{}", file.file_name().unwrap().display()));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "these documents do not declare a status in their first {HEADER_LINES} lines: {missing:?} -- a numbered document without one cannot be told apart from a superseded one, and the oldest are the likeliest to have been overtaken; say what it is worth now, in a sentence, not a keyword"
        );
    }

    #[test]
    fn every_file_path_named_in_the_documentation_exists_and_is_tracked() {
        // LEARNINGS entry 1 has said "nothing catches a recurrence" since the
        // day it was written. The failure it records is a sibling repository
        // documenting a design as canonical while citing a source file that is
        // in no commit — surviving only as stale build output.
        //
        // Relative *links* are already checked. This checks paths named in prose
        // and in code spans, which is how that repository's claim was written and
        // is the form a link checker cannot see.
        // Two levels, because this has to hold in two places. In a checkout,
        // git decides: the failure being guarded against is a file that exists
        // on somebody's disk and in no commit. Under `cargo mutants` the tree is
        // copied without a `.git`, so git can decide nothing -- and the honest
        // response is to fall back to existence on disk rather than to skip,
        // which would make the whole check vacuous exactly where it is least
        // observed.
        let known = known_files();
        assert!(
            !known.is_empty(),
            "no files found at all; the check would pass vacuously"
        );

        // Matched by suffix, because a document names a path the way a reader
        // would follow it -- `pipeline.rs` from prose about that file, or
        // `radar-store/tests/watermark_holds.rs` from a paragraph already inside
        // `crates/`. Requiring a repository-root path would flag correct prose,
        // and a check that flags correct prose is a check somebody turns off.
        let resolves = |named: &str| {
            known
                .iter()
                .any(|t| t == named || t.ends_with(&format!("/{named}")))
        };

        let mut missing = Vec::new();
        for doc in documents() {
            let Ok(text) = std::fs::read_to_string(&doc) else {
                continue;
            };
            for path in code_span_paths(&text) {
                if !resolves(&path) {
                    missing.push(format!("{} names {path}", doc.display()));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "documentation names files that are not tracked in git:\n  {}",
            missing.join("\n  ")
        );
    }

    #[test]
    fn ls_files_output_becomes_paths_and_never_an_empty_one() {
        // An empty entry is the dangerous one: `resolves` asks whether any known
        // path ends with `/{named}`, and "" would not match that — but a bare ""
        // in the set makes `is_empty()` false, so the fallback to walking the
        // tree never happens and the whole check runs against a set of nothing.
        let parsed = parse_ls_files("a/b.rs\n\n  c.toml  \n\n");
        assert_eq!(parsed.len(), 2, "blank lines are not paths: {parsed:?}");
        assert!(parsed.contains("a/b.rs"));
        assert!(parsed.contains("c.toml"), "surrounding space is trimmed");
        assert!(
            !parsed.contains(""),
            "an empty path matches nothing and hides that"
        );

        // Windows separators normalise, or every suffix match fails on Windows.
        assert!(
            parse_ls_files("crates\\realorrug-types\\src\\lib.rs")
                .contains("crates/realorrug-types/src/lib.rs")
        );

        // Nothing in, nothing out — which is what makes the caller fall back.
        assert!(parse_ls_files("").is_empty());
        assert!(parse_ls_files("\n\n").is_empty());
    }

    #[test]
    fn spans_are_paired_so_prose_between_them_is_not_read_as_one() {
        // Advancing past the closing backtick is what keeps spans paired. Off by
        // one and the *gaps* become spans, so `a.rs` followed by `b.rs` reads the
        // prose between them as a third — and the check starts reporting paths
        // nobody wrote.
        let found = code_span_paths("`a.rs` then not/a/path.rs then `b.rs`");
        assert_eq!(found.len(), 2, "exactly the two spans: {found:?}");
        assert!(
            found.contains("a.rs") && found.contains("b.rs"),
            "{found:?}"
        );
        assert!(
            !found.contains("not/a/path.rs"),
            "unquoted prose is not a code span: {found:?}"
        );
    }

    #[test]
    fn a_code_span_excludes_its_own_backticks() {
        // Off-by-one here is invisible in the happy path and fatal to the check:
        // a span read as "`Cargo.toml" resolves against nothing, so every named
        // path would be reported missing — or, read the other way, nothing is
        // extracted at all and the check passes for everything.
        let found = code_span_paths("see `Cargo.toml` here");
        assert_eq!(found.len(), 1, "{found:?}");
        let only = found.iter().next().expect("one");
        assert_eq!(
            only, "Cargo.toml",
            "the delimiters are not part of the path"
        );
        assert!(!only.starts_with('`') && !only.ends_with('`'));
    }

    #[test]
    fn the_path_extractor_finds_paths_and_ignores_prose() {
        // The check above is only worth having if this is right: too greedy and
        // it fails on ordinary backticked words, too narrow and it passes
        // vacuously — which is the failure mode it exists to prevent.
        let found = code_span_paths(
            "see `crates/realorrug-types/src/lib.rs` and `Cargo.toml`, but not `Slot` \
             or `foo.bar()` or `--flag` or `a/b`",
        );
        assert!(
            found.contains("crates/realorrug-types/src/lib.rs"),
            "{found:?}"
        );
        assert!(found.contains("Cargo.toml"), "{found:?}");
        assert!(
            !found.contains("Slot"),
            "a type name is not a path: {found:?}"
        );
        assert!(
            !found.contains("foo.bar()"),
            "a call is not a path: {found:?}"
        );
        assert!(!found.contains("--flag"), "a flag is not a path: {found:?}");
        // A bare `a/b` with no extension is as likely to be prose as a path.
        assert!(!found.contains("a/b"), "{found:?}");

        // A path somewhere else is not a claim about this repository, and has to
        // be written so that both a reader and this check can tell.
        let external =
            code_span_paths("`claw-net:internal/x.md` and `/etc/systemd/system/y.service`");
        assert!(external.is_empty(), "{external:?}");
    }

    #[test]
    fn links_are_extracted_the_way_a_reader_would_read_them() {
        // The extractor itself, checked — otherwise a bug here silently turns
        // every assertion above into a test that passes by finding nothing.
        let markdown = "See [one](docs/a.md) and [two](docs/b.md#section), \
                        [ext](https://example.com), [anchor](#here).";
        assert_eq!(
            relative_links(markdown),
            vec!["docs/a.md".to_owned(), "docs/b.md".to_owned()]
        );
    }

    #[test]
    fn the_extractor_finds_something_in_the_real_documents() {
        // Guards against the failure mode where every link test passes because
        // no links were found at all.
        let total: usize = documents()
            .iter()
            .filter_map(|d| std::fs::read_to_string(d).ok())
            .map(|text| relative_links(&text).len())
            .sum();
        assert!(
            total > 10,
            "only {total} relative links found across the docs"
        );
    }
}
