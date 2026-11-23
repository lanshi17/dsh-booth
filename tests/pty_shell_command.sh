#!/usr/bin/env bash
# Local shell commands run during a model turn without creating a model prompt.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'slow\n'; sleep 1; printf '!printf shell-ok\n'; sleep 1; printf '/cancel\n'; sleep 0.5
  printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq 'shell-ok' "$output"
grep -aq '"text":"slow"' "$state/history.jsonl"
if grep -aq '"text":"!printf shell-ok"' "$state/history.jsonl"; then
  echo 'local shell command was added to prompt history' >&2
  exit 1
fi
if grep -aq 'hello' "$output"; then
  echo 'local shell command unexpectedly caused a model turn' >&2
  exit 1
fi
