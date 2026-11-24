#!/usr/bin/env bash
# Ctrl+O reveals the full tool output.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'longtool\n'; sleep 1; printf '\x0f'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "… 10 earlier lines · Ctrl+O" "$output"
grep -aq "line 01" "$output"
grep -aq "line 13" "$output"
