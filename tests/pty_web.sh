#!/usr/bin/env bash
# Web search cards show the answer and source links.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'webtool\n'; sleep 1.5; printf '\x04' ) |
  timeout 20 script -qec "stty rows 30 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq 'Web search "news" · 2 sources' "$output"
grep -aq "Fresh answer." "$output"
grep -aq "Docs" "$output"
grep -aq "https://example.com/docs" "$output"
