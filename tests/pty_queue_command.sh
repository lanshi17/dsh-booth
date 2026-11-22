#!/usr/bin/env bash
# /queue adds a prompt to the pending queue during a run.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'slow\n'; sleep 1; printf '/queue queued-via-command\n'; sleep 1
  printf '\x03'; sleep 2; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "1 queued" "$output"
grep -aq "hello" "$output"
grep -aq '"text":"queued-via-command"' "$state/history.jsonl"
