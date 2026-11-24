#!/usr/bin/env bash
# Large write diffs stop at 40 lines until Ctrl+O.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'bigdiff\n'; sleep 1; printf '\x0f'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "stty rows 90 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "Diff +60 −0" "$output"
grep -aq "… +21 lines · Ctrl+O" "$output"
grep -aq "line 60" "$output"
