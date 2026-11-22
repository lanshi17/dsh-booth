#!/usr/bin/env bash
# The wheel moves the model picker and Enter applies the highlighted model.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'hi\n'; sleep 1; printf '/model\n'; sleep 0.8; printf '\x1b[<65;10;10M'
  sleep 0.5; printf '\r'; sleep 0.8; printf '/status\n'; sleep 0.8; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "model model-two" "$output"
