#!/usr/bin/env bash
# A leading $ runs Python locally and never becomes a model prompt.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf '$print("python-ok")\n'; sleep 1; printf '$2+2\n'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq 'python-ok' "$output"
grep -aq '4   ' "$output"
grep -aq 'python' "$output"
if grep -aq '"text":"$print' "$state/history.jsonl"; then
  echo 'local python command was added to prompt history' >&2
  exit 1
fi
if grep -aq 'hello' "$output"; then
  echo 'local python command unexpectedly caused a model turn' >&2
  exit 1
fi
