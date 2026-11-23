#!/usr/bin/env bash
# Permission notifications are emitted by the live TUI and respect /notify off.
set -euo pipefail

booth=$1
kernel=$2
state_on=$(mktemp -d)
state_off=$(mktemp -d)
output_on=$(mktemp)
output_off=$(mktemp)
trap 'rm -rf "$state_on" "$state_off" "$output_on" "$output_off"' EXIT

( printf 'perm\n'; sleep 1.5; printf 'y'; sleep 0.5; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state_on' '$booth'" /dev/null \
    >"$output_on" 2>&1 || true

grep -aFq $'\033]9;booth: permission needed: bash\007\007' "$output_on"

( sleep 1; printf '/notify off\n'; sleep 0.5; printf 'perm\n'; sleep 1.5; printf 'y'; sleep 0.5; printf '\x04' ) |
  timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state_off' '$booth'" /dev/null \
    >"$output_off" 2>&1 || true

grep -aq 'notifications off' "$output_off"
if grep -aFq $'\033]9;booth: permission needed: bash\007\007' "$output_off"; then
  echo "notification emitted after /notify off" >&2
  exit 1
fi
