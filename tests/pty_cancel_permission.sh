#!/usr/bin/env bash
# Ctrl+C resolves a pending permission card and keeps the console alive.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'perm\n'; sleep 1; printf '\x03'; sleep 1; printf '/status\n'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "cancelled" "$output"
grep -aq "hello" "$output"
grep -aq "booth · " "$output"
