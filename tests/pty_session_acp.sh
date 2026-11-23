#!/usr/bin/env bash
# /session and /acp expose the live ACP identifiers.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'hi\n'; sleep 1; printf '/session\n'; sleep 0.5; printf '/acp\n'; sleep 0.5
  printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "session s-1" "$output"
grep -aq "acp v1" "$output"
grep -aq "connected" "$output"
