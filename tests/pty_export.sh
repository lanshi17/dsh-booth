#!/usr/bin/env bash
# Exports the transcript to a markdown file and checks its content.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)

cleanup() {
  rm -rf "$state" "$output"
}
trap cleanup EXIT

( printf 'hi\n'; sleep 1.5; printf '/export %s/out.md\n' "$state"; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
  >"$output" 2>&1 || true

test -f "$state/out.md"
grep -q "# booth transcript" "$state/out.md"
grep -q "hello" "$state/out.md"
