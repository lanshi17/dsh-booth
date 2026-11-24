#!/usr/bin/env bash
# Checks ordered lists, blockquotes and nested bullets in the rendered transcript.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)

cleanup() {
  rm -rf "$state" "$output"
}
trap cleanup EXIT

( printf 'markdown\n'; sleep 2; printf '\x04' ) |
  timeout 20 script -qec "stty rows 50 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
  >"$output" 2>&1 || true

grep -aq "first ordered" "$output"
grep -aq "quoted line" "$output"
grep -aq "nested bullet" "$output"
grep -aq "┌" "$output"
grep -aq "└" "$output"
