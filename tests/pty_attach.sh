#!/usr/bin/env bash
# /attach sends the next prompt with an ACP resource link.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
attachment=$(mktemp)
printf 'notes' >"$attachment"
name=$(basename "$attachment")
trap 'rm -rf "$state" "$output" "$attachment"' EXIT

( printf '/attach %s\n' "$attachment"; sleep 0.8; printf 'hi\n'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "resource:$name" "$output"
grep -aq "hello" "$output"
