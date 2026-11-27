#!/usr/bin/env bash
# /context reports missing usage until the kernel sends it.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'hi\n'; sleep 2; printf '/context\n'; sleep 1.2; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "context: not reported yet" "$output"
