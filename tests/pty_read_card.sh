#!/usr/bin/env bash
# Read tool cards show three output lines until expanded.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'readtool\n'; sleep 2; printf '\x04' ) |
  timeout 20 script -qec "stty rows 30 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "line 1" "$output"
grep -aq "line 3" "$output"
grep -aq "… +7 lines" "$output"
