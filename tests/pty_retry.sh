#!/usr/bin/env bash
# /retry sends the last prompt again without retyping it.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'hi\n'; sleep 1; printf '/retry\n'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

test "$(grep -c '"text":"hi"' "$state/history.jsonl")" -eq 2
grep -aq "hello" "$output"
