#!/usr/bin/env bash
# Local commands stay usable while a turn is running.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'slow\n'; sleep 1.0; printf '/export %s/busy.md\n' "$state"; sleep 1.0
  printf '/status\n'; sleep 1.0; printf '\x03'; sleep 0.4; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

test -f "$state/busy.md"
grep -aq "slow" "$state/busy.md"
grep -aq "booth · " "$output"
