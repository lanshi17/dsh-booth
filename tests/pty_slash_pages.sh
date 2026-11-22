#!/usr/bin/env bash
# PageDown moves the slash menu to the last row, so Enter runs /quit.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf '/'; sleep 0.8; printf '\x1b[6~'; sleep 0.3; printf '\r'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

! grep -aq "booth — keys" "$output"
