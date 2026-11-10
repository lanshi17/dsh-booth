#!/usr/bin/env bash
# The single entry point for every gauntlet layer. Run from anywhere; it works in the repo root.
#
#   tools/gauntlet.sh            every layer
#   tools/gauntlet.sh quick      tests, format, lint, types only
#
# A layer that cannot run is a failure, never a pass (old-coder: checkers fail closed).
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

fail=0
status() { printf '\n=== %s ===\n' "$1"; }
record() {
  # record <layer> <exit code>
  if [ "$2" -ne 0 ]; then
    printf 'FAIL %s (exit %s)\n' "$1" "$2"
    fail=1
  else
    printf 'PASS %s\n' "$1"
  fi
}

status "versions"
# Record this layer like every other one, and fail closed on a record that cannot report the
# toolchain: `cat` succeeds on an empty file, which would print no versions and still pass the
# whole run.
if [ ! -e tools/versions.txt ]; then
  echo "missing tools/versions.txt"
  record versions 1
elif [ ! -s tools/versions.txt ]; then
  echo "tools/versions.txt is empty"
  record versions 1
else
  cat tools/versions.txt
  record versions $?
fi

status "tool tests"
python3 -m unittest discover -s tools -p 'test_*.py'
record "tool tests" $?

status "format"
cargo fmt --check
record format $?

status "types"
cargo check --all-targets
record types $?

status "lint"
cargo clippy --all-targets -- -D warnings
record lint $?

status "tests"
# `cargo test` exits 0 after running nothing: a target can carry no tests, and a suite whose
# tests are all `#[ignore]`d reports "0 passed" too. A branch that deletes its tests would then
# turn the layer green. Read the counts from this run and fail when none of them ran.
test_out=$(cargo test 2>&1)
test_code=$?
printf '%s\n' "$test_out"
if [ "$test_code" -eq 0 ] &&
  ! grep -q '^test result: .* [1-9][0-9]* passed' <<<"$test_out"; then
  echo "cargo test ran no tests"
  test_code=1
fi
record tests "$test_code"

if [ "${1:-}" = "quick" ]; then
  status "result"
  [ "$fail" -eq 0 ] && echo "all quick layers passed" || echo "a layer failed"
  exit "$fail"
fi

status "changed-line coverage"
if command -v cargo-llvm-cov >/dev/null 2>&1; then
  # A failed report leaves the previous run's target/cov.json in place. Reading it would measure
  # an older tree — a line this change adds looks like a comment to a report that never saw it —
  # so remove it first: no fresh report then means no measurement, which fails closed below.
  rm -f target/cov.json
  cargo llvm-cov --json --output-path target/cov.json
  report=$?
  if [ "$report" -eq 0 ]; then
    # Only the lines this change touched matter; global coverage is vanity. The base is the last
    # release, so the check means "everything since the last release".
    python3 tools/changed_lines.py ${GAUNTLET_BASE:+"$GAUNTLET_BASE"}
    record coverage $?
  else
    # Record the layer once: a failed report must not be followed by a PASS for unmeasured code.
    record coverage "$report"
  fi
else
  echo "cargo-llvm-cov is not installed (cargo install cargo-llvm-cov)"
  record coverage 1
fi

status "mutation"
tools/mutants.sh
record mutation $?

status "supply chain"
if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit --deny warnings
  record "supply chain (audit)" $?
else
  echo "cargo-audit is not installed (cargo install cargo-audit)"
  record "supply chain (audit)" 1
fi

status "suite health (random order, repeated)"
# No randomization plugin for cargo; repeating the suite in both orders is the substitute, and
# EVIDENCE records it as SUBSTITUTED rather than as the real layer.
cargo test -- --test-threads=1
record "suite health (serial)" $?
# Record the repeat layer once: a failed repeat must not be overwritten by a later PASS line.
repeat=0
for _ in 1 2 3; do
  cargo test --quiet || { repeat=$?; break; }
done
record "suite health (repeat)" "$repeat"

status "secrets"
tools/secrets.sh
record secrets $?

status "real execution"
tools/smoke.sh
record "real execution" $?

status "result"
if [ "$fail" -eq 0 ]; then
  echo "all layers passed"
else
  echo "a layer failed — see the FAIL lines above"
fi
exit "$fail"
