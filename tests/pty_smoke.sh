#!/usr/bin/env bash
# Runs the real binary in a pseudo-terminal against the fake kernel and checks what a person sees.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'hi\n'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
  >"$output" 2>&1 || true

grep -aq "hello" "$output"
grep -aqF $'\x1b[?1049h' "$output"
test -f "$state/booth.log"
test -f "$state/history.jsonl"
grep -aq '"text":"hi"' "$state/history.jsonl"
