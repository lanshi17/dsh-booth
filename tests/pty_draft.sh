#!/usr/bin/env bash
# Ctrl+D saves an unsent draft and the next start restores it.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
first=$(mktemp)
second=$(mktemp)
trap 'rm -rf "$state" "$first" "$second"' EXIT

( printf 'unsent draft'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$first" 2>&1 || true

( sleep 0.8; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$second" 2>&1 || true

grep -aq "unsent draft" "$second"
