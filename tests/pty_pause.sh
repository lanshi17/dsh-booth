#!/usr/bin/env bash
# /pause freezes the local dsh kernel and a second /pause resumes it.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1.2; printf '/pause\n'; sleep 1.5
  printf '/status\n'; sleep 1.5
  printf '/pause\n'; sleep 1.5
  printf '/status\n'; sleep 1.5
  printf '\x04' ) |
  timeout 25 env BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$state" DSH_HOME="$state/dsh" \
    script -qec "stty rows 30 cols 100; $booth" /dev/null >"$output" 2>&1 || true

grep -aq "agents paused" "$output"
grep -aq "agents resumed" "$output"
