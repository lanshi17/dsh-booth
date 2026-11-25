#!/usr/bin/env bash
# Ctrl+T reveals thinking, which is hidden by default.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'think\n'; sleep 1; printf '\x14'; sleep 0.8; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "thinking shown" "$output"
grep -aq "Thought for" "$output"
grep -aq "click to expand" "$output"
