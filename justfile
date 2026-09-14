# SPDX-License-Identifier: Apache-2.0
#
# Every check this repository runs, defined once. The workflow invokes these
# recipes rather than restating them, so a claim about what CI runs can only be
# kept true by being the thing CI runs.
#
# Local prerequisites beyond the Rust toolchain:
#
#   cargo deny     cargo install --locked cargo-deny
#   cargo mutants  cargo install --locked cargo-mutants (CI runs it; not on the owner's workstation)
#
# On Windows under MSYS or Git Bash the default host toolchain can resolve to a
# target whose linker is shadowed. Export the toolchain that works:
#
#   export REALORRUG_CARGO="cargo +stable-x86_64-pc-windows-gnullvm"

set shell := ["bash", "-euo", "pipefail", "-c"]

export RUSTFLAGS := env("RUSTFLAGS", "-D warnings")

cargo := env("REALORRUG_CARGO", "cargo")

# A floor, not a target. `cargo test` exits zero when a whole crate's tests are
# skipped, so the count is checked rather than trusted. Raise it as the suite
# grows; never lower it to make a run pass.
export MIN_TESTS := "1024"

# The public site's test floor.
export MIN_SITE_TESTS := "58"

_default:
    @just --list

# The edit-compile loop.
check: build tests lint fmt

# Everything a runner can do.
ci: build tests lint fmt cargo-deny licence-headers site

build:
    {{ cargo }} build --all-targets --locked

tests:
    #!/usr/bin/env bash
    set -euo pipefail
    output=$({{ cargo }} test --locked 2>&1) || { echo "$output"; exit 1; }
    echo "$output"
    passed=$(echo "$output" | awk '/^test result: ok\./ { s += $4 } END { print s + 0 }')
    echo "--- ${passed:-0} tests passed ---"
    if [ "${passed:-0}" -lt "$MIN_TESTS" ]; then
        echo "only ${passed:-0} tests ran; expected at least $MIN_TESTS." >&2
        echo "Either tests were skipped or the harness is lying. Raise MIN_TESTS" >&2
        echo "when the suite grows; never lower it to make this pass." >&2
        exit 1
    fi

lint:
    {{ cargo }} clippy --all-targets --locked -- -D warnings

fmt:
    {{ cargo }} fmt --check

# Mutation testing over the changed lines.
#
# The answer to the thing MIN_TESTS cannot see. A floor catches tests being
# *deleted*; it says nothing about an assertion *loosened in place* --
#
#     assert_eq!(v, Verdict::Blocked);   ->   assert!(matches!(v, _));
#
# -- which leaves the count identical and the coverage identical and the suite
# green. A mutant is a small edit to the implementation; if the tests still pass
# with it in place, they do not constrain that behaviour.
#
# `--in-diff` scopes it to what the branch changed, so cost tracks the size of
# the change rather than the size of the repository.
#
# A timeout is `inconclusive`, never a pass: mutation testing runs the suite once
# per mutant, and an infinite loop introduced by a mutant looks exactly like a
# slow one. Reporting that as "caught" would be the check lying in the direction
# that feels good.
mutants base="origin/main" shard="":
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v cargo-mutants >/dev/null 2>&1; then
        echo "cargo-mutants is not installed:" >&2
        echo "  cargo install --locked cargo-mutants" >&2
        exit 127
    fi
    # No fetching. A recipe that changes the repository it is run in is a recipe
    # that can damage it: an earlier draft used `--depth=1` and left the working
    # clone shallow, which silently broke `merge-base` for every command after
    # it. CI checks out with `fetch-depth: 0`, and a local run uses the history
    # that is already there.
    if ! git rev-parse --verify --quiet {{ base }} >/dev/null; then
        echo "{{ base }} is not in this repository." >&2
        echo "Fetch it yourself, then re-run -- this recipe will not touch your git state." >&2
        exit 1
    fi
    # A real file rather than `<(...)`: process substitution hands the tool a
    # /dev/fd path, which it cannot open on every platform this recipe has to run
    # on. Verified by it failing that way first.
    # `git diff` does not see untracked files, so a brand-new module is invisible
    # to `--in-diff` and passes without being mutated at all. That happened: a
    # local run reported 28 mutants and CI, where the files were committed,
    # found 74 and four misses in the new code. A check that reports absence the
    # same way it reports success is not a check -- LEARNINGS 5, in the tooling
    # rather than in the code.
    #
    # Refusing rather than staging them, because a recipe that changes the
    # repository it is run in is a recipe that can damage one.
    untracked=$(git ls-files --others --exclude-standard -- '*.rs')
    if [ -n "$untracked" ]; then
        echo "untracked Rust files are invisible to \`git diff\` and would NOT be mutated:" >&2
        echo "$untracked" | sed 's/^/  /' >&2
        echo "" >&2
        echo "Stage them first (\`git add -N <path>\` is enough), or this check" >&2
        echo "passes without having looked at your new code." >&2
        exit 1
    fi

    diff_file=$(mktemp)
    trap 'rm -f "$diff_file"' EXIT
    # Merge-base on the left so the scope is what this branch changed rather than
    # everything that has landed on the base since. Working tree on the right --
    # not HEAD -- because `--in-diff` matches the diff against the *source it is
    # mutating*, and a diff of committed state against a tree with uncommitted
    # edits is rejected as stale. In CI the two are the same thing.
    merge_base=$(git merge-base {{ base }} HEAD)
    git diff "$merge_base" > "$diff_file"
    if [ ! -s "$diff_file" ]; then
        echo "no changes against {{ base }}; nothing to mutate."
        exit 0
    fi
    # Sharded, because this check scales with the size of the diff and nothing
    # else in CI does. An early branch here produced 28 mutants; a forty-commit
    # one produced 408, which never once finished inside a runner's life -- every
    # attempt was killed part-way, reporting nothing, which is the worst failure
    # a check can have because it looks identical to a real finding.
    #
    # `--shard k/n` splits the *set*, so every mutant is still tested; the work
    # is spread across parallel jobs rather than dropped. `--jobs 2` inside each
    # shard rather than the runner's four cores: each job builds the workspace,
    # so the limit is memory, and a job that OOMs intermittently fails in a way
    # that looks like a finding too.
    shard_arg=""
    if [ -n "{{ shard }}" ]; then
        shard_arg="--shard {{ shard }}"
    fi
    # `--jobs 1`. Two parallel jobs means cargo-mutants keeps two complete
    # source-and-target copies, and a hosted runner has about 14GB free -- a
    # workspace target directory is several of those. Shards died mid-run with
    # no error and no exit code, which is what a runner running out of room
    # looks like from inside the job.
    #
    # Sharding is what buys the parallelism now, across machines that each have
    # their own disk, rather than inside one.
    # `--timeout 60`, lowered from 300 on 2026-09-03. That budget existed
    # because a mutant that hangs was an expected outcome: several loops in this
    # workspace were a single mutation away from never terminating, and each one
    # cost a runner five full minutes to report nothing useful. Two of them took
    # ten minutes of a fourteen-minute shard in one run.
    #
    # Those loops are bounded now, so a timeout is a signal rather than a cost of
    # doing business. If this fires, the mutant found a loop that can be made not
    # to terminate -- fix the loop, do not raise the number back. Raise it only
    # for a genuinely slow *test*, and say which test in the same commit.
    {{ cargo }} mutants --in-diff "$diff_file" --jobs 1 $shard_arg         --timeout 60 --minimum-test-timeout 60 -- --offline

cargo-deny:
    {{ cargo }} deny check

licence-headers:
    #!/usr/bin/env bash
    set -euo pipefail
    missing=0
    while IFS= read -r f; do
        if ! head -3 "$f" | grep -q "SPDX-License-Identifier"; then
            echo "missing SPDX header: $f"
            missing=1
        fi
    done < <(
        find crates -name '*.rs' -not -path '*/target/*'
        find site \( -name '*.ts' -o -name '*.tsx' \) 2>/dev/null | grep -v node_modules | grep -v '/dist/' || true
    )
    exit $missing

# The public site: install, audit, test with a floor, build.
site:
    #!/usr/bin/env bash
    set -euo pipefail
    cd site
    npm ci
    # Retried only when the audit could not run. A finding fails at once; an
    # unreachable registry is an unknown, not a clean audit, and fails after
    # three attempts.
    audit_log=$(mktemp)
    for attempt in 1 2 3; do
      if npm audit --audit-level=high 2>&1 | tee "$audit_log"; then
        break
      fi
      if ! grep -q 'audit endpoint returned an error' "$audit_log"; then
        echo "--- npm audit found something; not retrying ---" >&2
        exit 1
      fi
      if [ "$attempt" = 3 ]; then
        echo "--- npm audit could not reach the registry in 3 attempts ---" >&2
        exit 1
      fi
      sleep $((attempt * 15))
    done
    # NO_COLOR, because vitest wraps the count in escapes a pattern reads past.
    NO_COLOR=1 npm run test 2>&1 | tee /tmp/realorrug-site-tests.log
    passed=$(grep -oE 'Tests +[0-9]+ passed' /tmp/realorrug-site-tests.log || true)
    passed=$(echo "$passed" | grep -oE '[0-9]+' | head -1 || true)
    passed=${passed:-0}
    if [ "$passed" -lt "$MIN_SITE_TESTS" ]; then
      echo "--- $passed site tests passed, floor is $MIN_SITE_TESTS ---" >&2
      exit 1
    fi
    echo "--- $passed site tests passed ---"
    npm run build
