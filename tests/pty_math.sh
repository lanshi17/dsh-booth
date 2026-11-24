#!/usr/bin/env bash
# Inline LaTeX renders as Unicode in the transcript; code spans stay raw.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'math\n'; sleep 2; printf '\x04' ) |
  timeout 20 script -qec "stty rows 30 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "Energy" "$output"
grep -aq "mc²" "$output"
grep -aq "½" "$output"
grep -aqF "x̂" "$output"
grep -aqF "a+b" "$output"
grep -aqF "───" "$output"
grep -aqF "┌───" "$output"
grep -aqF "√ x+1" "$output"
