#!/usr/bin/env bash
# Answers a permission prompt with "allow always".
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)

cleanup() {
  rm -rf "$state" "$output"
}
trap cleanup EXIT

( printf 'perm\n'; sleep 2.5; printf 'a'; sleep 1.5; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
  >"$output" 2>&1 || true

grep -aq "Allow always" "$output"
grep -aq "always-ok" "$output"
