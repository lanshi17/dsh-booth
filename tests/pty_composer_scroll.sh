#!/usr/bin/env bash
# A tall draft shows only a window and marks the hidden rows.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

{
  printf 'l1'
  for index in 2 3 4 5 6 7 8 9 10; do printf '\x1b\rl%d' "$index"; done
  sleep 1
  printf '\x04'
} |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "↑2" "$output"
