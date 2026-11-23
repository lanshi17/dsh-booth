#!/usr/bin/env bash
# /cancel cancels the active turn without leaving the console.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'slow\n'; sleep 1; printf '/cancel\n'; sleep 1; printf '/status\n'; sleep 0.6
  printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "cancelled" "$output"
grep -aq "booth · " "$output"
