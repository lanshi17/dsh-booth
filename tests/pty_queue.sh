#!/usr/bin/env bash
# A prompt typed during a run is queued and sent when the turn ends.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'slow\n'; sleep 1.0; printf 'queued-prompt\n'; sleep 1.0
  printf '\x03'; sleep 1.5; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "1 queued" "$output"
grep -aq "↳ queued-prompt" "$output"
grep -aq "hello" "$output"
grep -aq '"text":"queued-prompt"' "$state/history.jsonl"
