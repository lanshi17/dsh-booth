#!/usr/bin/env bash
# `^` autocomplete expands to a tagged model mention understood by the prompt path.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1; printf 'warm\r'; sleep 1; printf '^mod'; sleep 0.4; printf '\t'; sleep 0.2; printf ' hi\r'; sleep 1; printf '\004' ) |
  timeout 25 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "mention-tag" "$output"
