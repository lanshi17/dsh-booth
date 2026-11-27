#!/usr/bin/env bash
# /clear drops the local transcript and closes the live session so the next prompt starts fresh.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'hi\n'; sleep 1; printf '/clear\n'; sleep 1.5
  printf '/export %s/cleared.md\n' "$state"; sleep 1.2; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

test -f "$state/cleared.md"
grep -aq "context cleared" "$output"
! grep -aq "hello" "$state/cleared.md"
! grep -aq "## You" "$state/cleared.md"
