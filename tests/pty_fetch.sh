#!/usr/bin/env bash
# web_fetch cards show the URL, status, preview and truncation footer.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'fetchtool\n'; sleep 1.5; printf '\x04' ) |
  timeout 20 script -qec "stty rows 30 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "Web fetch · example.com · HTTP 200" "$output"
grep -aq "body 1" "$output"
grep -aq "Content truncated" "$output"
