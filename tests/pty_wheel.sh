#!/usr/bin/env bash
# Fills the transcript past one window and checks the wheel scrolls back.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

{
  for index in $(seq 1 25); do
    printf 'marker-%02d\n' "$index"
    sleep 0.45
  done
  sleep 0.5
  printf '\x1b[<64;10;10M'
  sleep 0.5
  printf '\x04'
} |
  timeout 30 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "entries back" "$output"
