#!/usr/bin/env bash
# Codex apply_patch input renders per-file operations and stats.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'patchtool\n'; sleep 1.5; printf '\x04' ) |
  timeout 20 script -qec "stty rows 30 cols 100; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "Patch · 3 files · +3 −1" "$output"
grep -aq "A new.txt" "$output"
grep -aq "M src/a.cpp -> src/b.cpp" "$output"
grep -aq "D old.txt" "$output"
