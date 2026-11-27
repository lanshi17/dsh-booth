#!/usr/bin/env bash
# /share, /collab, /join and /leave run through the control channel.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1; printf '/share\n'; sleep 2.5
  printf '/collab on\n'; sleep 2.5
  printf '/join\n'; sleep 2.5
  printf '/leave\n'; sleep 2.5
  printf '\x04' ) |
  timeout 20 env BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$state" DSH_HOME="$state/dsh" \
    script -qec "stty rows 30 cols 100; $booth" /dev/null >"$output" 2>&1 || true

grep -aq "Sharing this session" "$output"
grep -aq "Collaboration: write" "$output"
grep -aq "Joining" "$output"
grep -aq "Stopped sharing" "$output"
