#!/usr/bin/env bash
# /skills lists a project skill and Enter inserts /name into the composer.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
project=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$project" "$output"' EXIT

mkdir -p "$project/.dsh/skills/review"
cat >"$project/.dsh/skills/review/SKILL.md" <<'MD'
---
name: review
description: review a change
---
Review the change.
MD

(
  cd "$project"
  printf '/skills'; sleep 1; printf '\r'; sleep 0.5; printf 'review'; sleep 0.5; printf '\r'; sleep 0.5; printf 'X'; sleep 0.3; printf '\r'; sleep 1; printf '\x04'
) | ( cd "$project" && timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null ) \
  >"$output" 2>&1 || true

grep -aq "review a change" "$output"
grep -aq "/review X" "$output"
