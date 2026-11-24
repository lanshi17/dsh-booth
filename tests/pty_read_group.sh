#!/usr/bin/env bash
# Consecutive read results group into one card with one line per file.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'readgroup\n'; sleep 2; printf '\x04' ) |
  timeout 20 script -qec "stty rows 30 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "Read 2 files" "$output"
grep -aq "src/a.cpp" "$output"
grep -aq "src/b.cpp" "$output"
grep -aq "Ctrl+O" "$output"
