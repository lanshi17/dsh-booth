#!/usr/bin/env bash
# Enter opens the in-TUI provider page; Ctrl+D removes, Ctrl+S saves a new route.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

(
  sleep 2
  printf '/providers\r'; sleep 2
  printf '\r'; sleep 2
  printf '\030'; sleep 2
  printf '/providers\r'; sleep 2
  printf 'new'; sleep 0.5
  printf '\r'; sleep 2
  printf 'fresh'; sleep 0.5
  printf '\t'; sleep 0.3
  printf 'Fresh'; sleep 0.3
  printf '\023'; sleep 2
  printf '\004'
) | timeout 30 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' DSH_HOME='$state/dsh' '$booth'" /dev/null \
  >"$output" 2>&1 || true

grep -aq "provider · mock" "$output"
grep -aq "Removed user configuration for mock" "$output"
grep -aq "new provider" "$output"
grep -aq "Updated fresh" "$output"
grep -aq "Enter send" "$output"
