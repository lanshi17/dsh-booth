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
cat tools/versions.txt 2>/dev/null || { echo "missing tools/versions.txt"; fail=1; }

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
cargo test
record tests $?

if [ "${1:-}" = "quick" ]; then
  status "result"
  [ "$fail" -eq 0 ] && echo "all quick layers passed" || echo "a layer failed"
  exit "$fail"
fi

status "changed-line coverage"
if command -v cargo-llvm-cov >/dev/null 2>&1; then
  cargo llvm-cov --json --output-path target/cov.json
  record "coverage report" $?
  # Only the lines this change touched matter; global coverage is vanity. The base is the last
  # release, so the check means "everything since the last release".
  python3 tools/changed_lines.py ${GAUNTLET_BASE:+"$GAUNTLET_BASE"}
  record coverage $?
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
for _ in 1 2 3; do cargo test --quiet || { record "suite health (repeat)" 1; break; }; done
record "suite health (repeat)" 0

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
