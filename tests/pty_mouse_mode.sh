#!/usr/bin/env bash
# tmux defaults to released mouse capture; /mouse on re-enables app tracking.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1; printf '/mouse on\r'; sleep 0.8; printf '\x04' ) |
  timeout 20 script -qec "TMUX=fake BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aFq $'\033[?1003l' "$output"
grep -aFq $'\033[?1003h' "$output"
grep -aq "mouse capture on" "$output"
grep -aq "on" "$state/mouse"
