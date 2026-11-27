#!/usr/bin/env bash
# /providers opens the provider configuration list.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf '/providers\n'; sleep 1; printf '\x1b'; sleep 0.3; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
  >"$output" 2>&1 || true

grep -aq "providers" "$output"
grep -aq "mock" "$output"
grep -aq "https://old.example/v1" "$output"
grep -aq "no key" "$output"
grep -aq "Provider configuration" "$output"
grep -aq "Enter edits provider" "$output"
