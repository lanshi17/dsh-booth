#!/usr/bin/env bash
# A project Markdown command expands and is sent as a prompt.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
project=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$project" "$output"' EXIT

mkdir -p "$project/.booth/commands"
cat >"$project/.booth/commands/review.md" <<'MD'
---
description: review a change
---
Review $ARGUMENTS carefully.
MD

(
  cd "$project"
  printf '/rev'; sleep 0.8; printf '\x1b'; sleep 0.2; printf '\x15'; sleep 0.2
  printf '/review file.txt\n'; sleep 2; printf '\x04'
) | ( cd "$project" && timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null ) \
  >"$output" 2>&1 || true

grep -aq "review a change" "$output"
grep -aq "Review file.txt carefully." "$output"
