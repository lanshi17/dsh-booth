#!/usr/bin/env bash
# Selects /status from the slash menu with the arrow keys.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)

cleanup() {
  rm -rf "$state" "$output"
}
trap cleanup EXIT

( printf '/'; sleep 0.8; printf '\x1b[B'; sleep 0.2; printf '\x1b[B'; sleep 0.2; printf '\r'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
  >"$output" 2>&1 || true

grep -aq "model auto" "$output"
