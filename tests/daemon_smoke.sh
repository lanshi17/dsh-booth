#!/usr/bin/env bash
# Starts the daemon, drives a task through its socket, and stops it again.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)

cleanup() {
  BOOTH_STATE_DIR="$state" "$booth" down >/dev/null 2>&1 || true
  rm -rf "$state"
}
trap cleanup EXIT

export BOOTH_STATE_DIR="$state"
export BOOTH_DSH_BIN="$kernel"

"$booth" up >/dev/null
test -S "$state/booth.sock"

output=$("$booth" run "hi" --cwd /tmp)
grep -q "hello" <<<"$output"

"$booth" status | grep -q "ready"
"$booth" sessions | grep -q "s-old"

"$booth" down >/dev/null
test ! -e "$state/booth.sock"
