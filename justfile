# SPDX-License-Identifier: Apache-2.0
#
# Every check this repository runs, defined once. The workflow invokes these
# recipes rather than restating them, so a claim about what CI runs can only be
# kept true by being the thing CI runs.
#
# Local prerequisites beyond the Rust toolchain:
#
#   cargo deny     cargo install --locked cargo-deny
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
export MIN_TESTS := "960"

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
