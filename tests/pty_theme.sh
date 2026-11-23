#!/usr/bin/env bash
# A theme.conf accent reaches the rendered status band.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
printf 'accent=#ff0000\n' >"$state/theme.conf"
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "38;2;255;0;0" "$output"
