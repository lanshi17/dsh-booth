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

# The diff this review is about: everything since the last release, or since the trunk when no
# release tag exists yet. Written where cargo-mutants can read it. The directory is made first: a
# checkout that has not built yet has no target/, and a failed redirect would otherwise look like
# a missing diff.
BASE=${GAUNTLET_BASE:-$(git describe --tags --abbrev=0 2>/dev/null || echo master)}
mkdir -p target || exit 2
git diff "$(git merge-base HEAD "$BASE")" > target/review.diff
if [ ! -s target/review.diff ]; then
  # Fail closed: an empty diff here means the base is wrong, not that the change is risk-free.
  echo "no diff against $BASE — pass GAUNTLET_BASE with the commit this change started from"
  exit 1
fi

# An earlier run's report must not be read as this run's result. When cargo-mutants fails before
# it starts, it leaves the old outcomes.json in place, and that report can say everything was
# caught. Removing it makes a missing report mean "this run did not produce one".
rm -rf target/mutants

# A time budget keeps the layer usable; a timeout is reported as unfinished, not as a pass.
timeout "${MUTANTS_TIMEOUT:-1800}" cargo mutants \
  --in-diff target/review.diff \
  --timeout 120 \
  --no-shuffle \
  --output target/mutants >target/mutants.log 2>target/mutants.err
code=$?
if [ "$code" -eq 124 ]; then
  echo "mutation run hit the time budget; not a pass"
  exit 1
fi
# Exit 2 means missed mutants, which the summary below judges. Any other nonzero status is the
# run itself failing; there is no report for it that describes this run.
if [ "$code" -ne 0 ] && [ "$code" -ne 2 ]; then
  echo "cargo-mutants did not finish (exit $code); see target/mutants.err"
  exit 1
fi

# cargo-mutants 27 writes its outcome counts and per-mutant results here; the `--json` stream is
# only for `--list`, so this is the only machine-readable report of a run.
report=target/mutants/mutants.out/outcomes.json
# A diff with no mutable lines (tests or docs only) exits 0 and writes no report: there is
# nothing to catch, which is a pass, not an unreadable file. A run that mutated something always
# writes one.
if [ "$code" -eq 0 ] && [ ! -s "$report" ]; then
  echo "no mutants in the changed lines — nothing to catch"
  exit 0
fi
python3 tools/mutant_summary.py "$report"
exit $?
