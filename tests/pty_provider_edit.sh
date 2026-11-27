#!/usr/bin/env bash
# The standalone provider editor removes, creates and saves routes.
set -euo pipefail

provider=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$output"' EXIT

(
  sleep 8
  printf '\r'; sleep 2
  printf '\030'; sleep 2
  printf 'n'; sleep 2
  printf 'fresh'; sleep 1
  printf '\t'; sleep 0.5
  printf 'Fresh'; sleep 0.5
  printf '\023'; sleep 2
  printf 'q'
) | timeout 40 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' DSH_HOME='$state/dsh' '$provider'" /dev/null \
  >"$output" 2>&1 || true

grep -aq "booth-provider" "$output"
grep -aq "provider routes" "$output"
grep -aq "provider · mock" "$output"
grep -aq "Removed user configuration for mock" "$output"
grep -aq "new provider" "$output"
grep -aq "Updated fresh" "$output"
