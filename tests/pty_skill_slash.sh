#!/usr/bin/env bash
# A user-invocable skill is searchable in the slash menu and submitted as a prompt gesture.
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
  printf '/review'; sleep 1; printf '\r'; sleep 1; printf '\x04'
) | ( cd "$project" && timeout 20 script -qec "BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null ) \
  >"$output" 2>&1 || true

grep -aq "review a change" "$output"
if grep -aq "unknown command /review" "$output"; then
  echo "skill was treated as an unknown local command" >&2
  exit 1
fi
