#!/usr/bin/env bash
# Refreshes the sessions picker and closes the active session from it.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)

mkdir -p "$state/sessions/--tmp--/s-old"
printf '%s\n' '{"type":"session/title","data":{"title":"stored title"}}' > "$state/title.jsonl"
zstd -q -o "$state/sessions/--tmp--/s-old/session.v4.jsonl.zstd" "$state/title.jsonl"

cleanup() {
  rm -rf "$state" "$output"
}
trap cleanup EXIT

( printf 'hi\n'; sleep 1; printf '/sessions\n'; sleep 1; printf 'r'; sleep 1; printf 'c'; sleep 1; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' DSH_HOME='$state' '$booth'" /dev/null \
  >"$output" 2>&1 || true

grep -aq "stored title" "$output"
grep -aq "session closed" "$output"
