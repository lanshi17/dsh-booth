#!/usr/bin/env bash
# PageDown advances ten rows; three more Downs select /new after /links, /notify and /theme were added.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf '/'; sleep 0.8; printf '\x1b[6~'; sleep 0.3; printf '\x1b[B'; sleep 0.3; printf '\x1b[B'; sleep 0.3; printf '\x1b[B'; sleep 0.8; printf '\r'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "new session" "$output"
! grep -aq "booth — keys" "$output"
