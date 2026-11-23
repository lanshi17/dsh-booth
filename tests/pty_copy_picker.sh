#!/usr/bin/env bash
# /copy opens the picker and copies the selected answer.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'markdown\n'; sleep 1; printf '/copy\n'; sleep 0.8; printf '\r'; sleep 0.8
  printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "copied" "$output"
