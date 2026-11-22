#!/usr/bin/env bash
# The menu shows every command, including the transcript commands.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf '/'; sleep 1; printf '\x1b'; sleep 0.3; printf '\x7f'; sleep 0.2; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "retry" "$output"
grep -aq "clear the local transcript" "$output"
grep -aq "send the last prompt again" "$output"
