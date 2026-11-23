#!/usr/bin/env bash
# Visual mode paints a reverse-video selection and a VISUAL band.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1; printf 'abc def'; sleep 0.2; printf '\033'; sleep 0.2; printf '0wve'; sleep 0.5; printf '\004' ) |
  timeout 20 script -qec "BOOTH_VIM=1 BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "VISUAL" "$output"
grep -aq $'\033\[7m' "$output"
