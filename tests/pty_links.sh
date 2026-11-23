#!/usr/bin/env bash
# `/links` lists URLs from the transcript without launching a browser.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1; printf 'see https://example.com/docs now\r'; sleep 1.5; printf '/links\r'; sleep 1; printf '\033'; sleep 0.3; printf '\004' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "links · 1" "$output"
grep -aq "example.com" "$output"
