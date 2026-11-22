#!/usr/bin/env bash
# Runs the TUI against the daemon socket rather than spawning its own kernel.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)

cleanup() {
  BOOTH_STATE_DIR="$state" "$booth" down >/dev/null 2>&1 || true
  rm -rf "$state" "$output"
}
trap cleanup EXIT

export BOOTH_STATE_DIR="$state"
export BOOTH_DSH_BIN="$kernel"

"$booth" up >/dev/null

( printf 'hi\n'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_STATE_DIR='$state' '$booth'" /dev/null >"$output" 2>&1 || true

grep -aq "hello" "$output"
# The daemon still owns the kernel after the console exits.
"$booth" status | grep -q "ready"
"$booth" down >/dev/null
