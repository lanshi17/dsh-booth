#!/usr/bin/env bash
# The composer shows a filled context gauge once the kernel reports usage.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'usage\n'; sleep 1.5; printf '\x04' ) |
  timeout 20 script -qec "stty rows 24 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "██" "$output"
grep -aq "25%" "$output"
