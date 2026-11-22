#!/usr/bin/env bash
# The status band shows the current reasoning effort and follows Shift+Tab.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
cleaned=$(mktemp)
trap 'rm -rf "$state" "$output" "$cleaned"' EXIT

( printf 'hi\n'; sleep 1; printf '\x1b[Z'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

sed -E 's/\x1B\[[0-9;]*[A-Za-z]//g' "$output" >"$cleaned"
grep -aq "effort High" "$cleaned"
grep -aq "effort Low" "$cleaned"
