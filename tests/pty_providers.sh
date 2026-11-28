#!/usr/bin/env bash
# /providers launches the standalone booth-provider editor; Esc returns to booth.
set -euo pipefail

booth=$1
kernel=$2
provider=$3
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

( printf '/providers\n'; sleep 2; printf 'm'; sleep 1; printf '\x1b'; sleep 2; printf '\x04' ) |
  timeout 25 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' DSH_HOME='$state/dsh' BOOTH_PROVIDER_BIN='$provider' '$booth'" /dev/null \
  >"$output" 2>&1 || true

grep -aq "booth-provider" "$output"
grep -aq "provider routes" "$output"
grep -aq "Select provider to edit" "$output"
grep -aq "Mock" "$output"
grep -aq "Type to search" "$output"
grep -aq "Search: m" "$output"
grep -aq "Enter send" "$output"
