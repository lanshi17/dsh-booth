#!/usr/bin/env bash
# Typing filters the model picker and the config reply refreshes effort.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'hi\n'; sleep 1; printf '/model\n'; sleep 0.6; printf 'three'; sleep 0.5
  printf '\r'; sleep 0.8; printf '/session\n'; sleep 0.6; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "model model-three" "$output"
grep -aq "All models" "$output"
grep -aq "effort Low" "$output"
