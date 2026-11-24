#!/usr/bin/env bash
# The permission card previews an edit diff before approval.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'diffperm\n'; sleep 1.2; printf 'n'; sleep 0.3; printf '\x04' ) |
  timeout 20 script -qec "stty rows 30 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "src/a.cpp" "$output"
grep -aq -- "- old" "$output"
grep -aq -- "+ new" "$output"
