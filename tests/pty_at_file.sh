#!/usr/bin/env bash
# @path completion attaches the file as an ACP resource link.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
fixture=$PWD/at-fixture.txt
printf 'fixture' >"$fixture"
trap 'rm -rf "$state" "$output" "$fixture"' EXIT

( printf '@at-fixture'; sleep 0.8; printf '\t'; sleep 0.4; printf '\r'; sleep 1
  printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "resource:at-fixture.txt" "$output"
grep -aq "hello" "$output"
