#!/usr/bin/env bash
# SSH sessions default to inline output (native scrollback) and released mouse capture.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1; printf '\x04' ) |
  timeout 15 script -qec "SSH_CONNECTION='10.0.0.1 1 10.0.0.2 2' BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

if grep -aq $'\x1b[?1049h' "$output"; then
  echo "SSH session entered the alternate screen" >&2
  exit 1
fi
if grep -aq $'\x1b[?1000h' "$output"; then
  echo "SSH session captured the mouse" >&2
  exit 1
fi
grep -aq "booth" "$output"
