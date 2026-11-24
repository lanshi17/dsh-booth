#!/usr/bin/env bash
# Assistant image chunks render as a transcript placeholder.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'imagechunk\n'; sleep 2; printf '\x04' ) |
  timeout 20 script -qec "stty rows 24 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "\[image: image/png" "$output"
