#!/usr/bin/env bash
# /vibe dispatches a prompt through the control channel.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1; printf '/vibe inspect the parser\n'; sleep 1.5
  printf '\x04' ) |
  timeout 20 env BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$state" DSH_HOME="$state/dsh" \
    script -qec "stty rows 30 cols 100; $booth" /dev/null >"$output" 2>&1 || true

grep -aq "vibe worker report" "$output"
