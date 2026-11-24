#!/usr/bin/env bash
# A 24-row terminal scrolls the help card to its final command and alias rows.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1; printf '/help\r'; sleep 0.8; printf '\x1b[6~'; sleep 0.4; printf '\x1b[6~'; sleep 0.5; printf 'q'; sleep 0.2; printf '\x04' ) |
  timeout 20 script -qec "stty rows 24 cols 80; BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "keys and commands" "$output"
grep -aq "Aliases: /resume" "$output"
grep -aq "/quit leave" "$output"
grep -aq "scroll · Esc/q close" "$output"
