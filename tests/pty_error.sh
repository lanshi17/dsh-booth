#!/usr/bin/env bash
# A prompt failure pins a retry-able error banner above the composer.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'failing\n'; sleep 1.5; printf '\x04' ) |
  timeout 20 script -qec "stty rows 30 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "model authentication failed" "$output"
grep -aq "error · /retry to try again" "$output"
