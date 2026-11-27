#!/usr/bin/env bash
# /reload-plugins re-execs booth and resumes the marked session through the new kernel.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( sleep 1.2; printf '/rename before restart\n'; sleep 1.2
  printf '/reload-plugins\n'; sleep 5
  printf '/status\n'; sleep 2
  printf '\x04' ) |
  timeout 30 env BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$state" DSH_HOME="$state/dsh" \
    script -qec "stty rows 30 cols 100; $booth" /dev/null >"$output" 2>&1 || true

grep -aq "reloading plugins (restarting booth)" "$output"
grep -aq "session s-1" "$output"
test "$(grep -c 'supervisor: ready' "$state/booth.log")" -ge 2
