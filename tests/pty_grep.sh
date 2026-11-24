#!/usr/bin/env bash
# Grep tool cards group matches by file and show the pattern.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'greptool\n'; sleep 1.5; printf '\x04' ) |
  timeout 20 script -qec "stty rows 30 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq 'Grep "TODO" · 2 matches' "$output"
grep -aq "src/a.cpp" "$output"
grep -aq "TODO one" "$output"
