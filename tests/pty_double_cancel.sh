#!/usr/bin/env bash
# Two consecutive Ctrl+C presses exit; Ctrl+D exits directly.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1; printf '\x03'; sleep 0.3; printf '\x03' ) |
  timeout 8 env BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$state" DSH_HOME="$state/dsh" \
    script -qec "stty rows 30 cols 100; $booth" /dev/null >"$output" 2>&1 || true

grep -aq "Ctrl+C again to quit" "$output"
# No Ctrl+D: the second Ctrl+C must have ended the process.
