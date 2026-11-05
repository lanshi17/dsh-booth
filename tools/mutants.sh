#!/usr/bin/env bash
# Mutation layer for the release review.
#
# `cargo-mutants` generates mutants from the syntax tree, so it cannot skip one the way a
# hand-written list can. Any surviving mutant fails the layer: a survivor means no test asserts
# that behaviour. Equivalent mutants are listed in EVIDENCE with the reason, never silenced here.
#
# Scope: the lines this review changed, via `--in-diff`. The whole of a large file yields far
# more mutants than a review needs, and the claim this layer supports is "the changes are
# covered", which is what the diff expresses. `--in-place` is deliberately not used: cargo-mutants
# copies the tree, so a failure cannot leave a mutant behind.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

if ! command -v cargo-mutants >/dev/null 2>&1; then
  echo "cargo-mutants is not installed (cargo install cargo-mutants --locked)"
  exit 1
fi

# The diff against the trunk, written where cargo-mutants can read it.
BASE=${GAUNTLET_BASE:-master}
git diff "$(git merge-base HEAD "$BASE")" > target/review.diff
if [ ! -s target/review.diff ]; then
  echo "no diff against $BASE: nothing to mutate"
  exit 0
fi

# A time budget keeps the layer usable; a timeout is reported as unfinished, not as a pass.
timeout "${MUTANTS_TIMEOUT:-1800}" cargo mutants \
  --in-diff target/review.diff \
  --timeout 120 \
  --no-shuffle \
  --output target/mutants \
  --json >target/mutants.json 2>target/mutants.err
code=$?
if [ "$code" -eq 124 ]; then
  echo "mutation run hit the time budget; not a pass"
  exit 1
fi

# A run that hit its budget writes no report; cargo-mutants' own outcomes file is the fallback.
report=target/mutants.json
if [ ! -s "$report" ] && [ -s target/mutants/mutants.out/mutants.json ]; then
  report=target/mutants/mutants.out/mutants.json
  echo "the run did not finish; reading the partial report at $report"
fi
python3 tools/mutant_summary.py "$report"
exit $?
