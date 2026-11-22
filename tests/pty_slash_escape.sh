#!/usr/bin/env bash
# Escape closes the slash menu and Enter submits the literal text.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf '/cl'; sleep 0.8; printf '\x1b'; sleep 0.4; printf '\r'; sleep 0.8; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "unknown command /cl" "$output"
