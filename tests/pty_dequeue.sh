#!/usr/bin/env bash
# Alt+Up restores a queued prompt to the editor and Ctrl+D preserves it as a draft.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf 'slow\n'; sleep 1; printf '/queue queued-via-command\n'; sleep 1
  printf '\x1b[1;3A'; sleep 0.8; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "queued-via-command" "$state/draft"
