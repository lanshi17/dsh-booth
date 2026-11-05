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
  # Only the lines this branch changed matter; global coverage is vanity.
  base="${GAUNTLET_BASE:-origin/master}"
  cargo llvm-cov --summary-only --json --output-path target/cov.json >/dev/null 2>&1
  covered=$(python3 - <<'PY' 2>/dev/null
import json
d = json.load(open("target/cov.json"))
t = d["data"][0]["totals"]["lines"]
print(f'{t["covered"]}/{t["count"]}')
PY
)
  if [ -z "${covered:-}" ]; then
    echo "could not read target/cov.json"
    record coverage 1
  else
    echo "whole-project lines covered: $covered (changed-line check below)"
    python3 tools/changed_lines.py "$base"
    record coverage $?
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
  cargo audit
  record "supply chain (audit)" $?
else
  echo "cargo-audit is not installed (cargo install cargo-audit)"
  record "supply chain (audit)" 1
fi

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
