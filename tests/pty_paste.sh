#!/usr/bin/env bash
# A bracketed multi-line paste stays in the composer and never submits.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1; printf '\x1b[200~paste-line-1\npaste-line-2\x1b[201~'; sleep 1
  printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "paste-line-1" "$output"
grep -aq "paste-line-2" "$output"
if grep -aq "hello" "$output"; then
  echo "paste submitted a prompt" >&2
  exit 1
fi
