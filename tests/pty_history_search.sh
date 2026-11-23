#!/usr/bin/env bash
# Ctrl+R finds an older prompt and Enter reloads it into the composer.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'alpha\n'; sleep 0.8; printf 'beta\n'; sleep 0.8; printf '\x12'; sleep 0.4
  printf 'alp'; sleep 0.5; printf '\r'; sleep 0.4; printf '\r'; sleep 0.8; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

test "$(grep -c '"text":"alpha"' "$state/history.jsonl")" -eq 2
