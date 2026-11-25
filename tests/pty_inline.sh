#!/usr/bin/env bash
# BOOTH_INLINE=1 keeps the old inline screen and splash.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'hi\n'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_INLINE=1 BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
  >"$output" 2>&1 || true

grep -aq "booth · dsh console" "$output"
grep -aq "hello" "$output"
if grep -aqF $'\x1b[?1049h' "$output"; then
  echo "inline mode entered the alternate screen" >&2
  exit 1
fi
