#!/usr/bin/env bash
# Todo tool calls render a checklist, collapsed until Ctrl+O.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'todotool\n'; sleep 2; printf '\x0f'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "stty rows 30 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "Todo · 2/10" "$output"
grep -aq "Todo 2/10 · 1 active" "$output"
grep -aq "Task 1" "$output"
grep -aq "… +2 todos" "$output"
grep -aq "Task 10" "$output"
