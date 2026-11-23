#!/usr/bin/env bash
# Tab completes a word used in an earlier prompt.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'wordcompletionprobe\r'; sleep 1; printf 'wordcompletionpr'; sleep 0.4
  printf '\t'; sleep 0.2; printf '\r'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

test "$(grep -c '"text":"wordcompletionprobe"' "$state/history.jsonl")" -eq 2
