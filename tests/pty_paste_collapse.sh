#!/usr/bin/env bash
# A large bracketed paste collapses to a marker and expands when submitted.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

paste=""
for index in $(seq 1 12); do
  if [ "$index" -gt 1 ]; then paste="${paste}"$'\n'; fi
  paste="${paste}paste-line-${index}"
done

( sleep 1; printf '\x1b[200~%s\x1b[201~' "$paste"; sleep 0.8
  printf '\r'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "Paste #1, +12 lines" "$output"
grep -aq "paste-line-11" "$output"
