#!/usr/bin/env bash
# Thinking is hidden by default; only tool/command cards remain.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'think\n'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "hello" "$output"
if grep -aq "Thought for\\|Thinking" "$output"; then
  echo "thinking was visible by default" >&2
  exit 1
fi
