#!/usr/bin/env bash
# Ctrl+C clears an idle draft and Up brings it back.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'unsent draft'; sleep 0.6; printf '\x03'; sleep 0.4; printf '\x1b[A'; sleep 0.3
  printf 'X'; sleep 0.3; printf '\r'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
  >"$output" 2>&1 || true

grep -aq "draft cleared" "$output"
grep -aq "unsent draftX" "$output"
