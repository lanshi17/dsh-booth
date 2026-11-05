#!/usr/bin/env bash
# Mutation layer for the release review.
#
# `cargo-mutants` generates mutants from the syntax tree, so it cannot skip one the way a
# hand-written list can. Any surviving mutant fails the layer: a survivor means no test asserts
# that behaviour. Equivalent mutants are listed in EVIDENCE with the reason, never silenced here.
#
# Scope: the files this review changed. `--in-place` is deliberately not used; cargo-mutants
# copies the tree, so a failure cannot leave a mutant behind.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

if ! command -v cargo-mutants >/dev/null 2>&1; then
  echo "cargo-mutants is not installed (cargo install cargo-mutants --locked)"
  exit 1
fi

# `--file` takes one path per flag, so the list is turned into repeated flags.
FILES=${MUTANTS_FILES:-"src/intake.rs src/service.rs src/app.rs"}
file_flags=()
for file in $FILES; do
  file_flags+=(--file "$file")
done

# A time budget keeps the layer usable; a timeout is reported as unfinished, not as a pass.
timeout "${MUTANTS_TIMEOUT:-1800}" cargo mutants \
  "${file_flags[@]}" \
  --timeout 120 \
  --no-shuffle \
  --output target/mutants \
  --json >target/mutants.json 2>target/mutants.err
code=$?
if [ "$code" -eq 124 ]; then
  echo "mutation run hit the time budget; not a pass"
  exit 1
fi

python3 tools/mutant_summary.py target/mutants.json
exit $?
