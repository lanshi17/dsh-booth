#!/usr/bin/env bash
# /providers edit opens the external editor and applies baseURL/apiKey.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
output=$(mktemp)
editor=$state/editor.sh
trap 'rm -rf "$state" "$output"' EXIT
cat >"$editor" <<'SH'
#!/bin/sh
printf 'baseURL=https://new.example/v1\napiKey=sk-test-123\n' > "$1"
SH
chmod +x "$editor"

( sleep 1; printf '/providers edit mock\n'; sleep 3
  printf '/providers add fresh\n'; sleep 3
  printf '/providers remove fresh --key\n'; sleep 2
  printf '\x04' ) |
  timeout 25 env -u VISUAL EDITOR="$editor" BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$state" DSH_HOME="$state/dsh" \
    script -qec "stty rows 30 cols 100; $booth" /dev/null >"$output" 2>&1 || true

grep -aq "Updated mock" "$output"
grep -aq "Updated fresh" "$output"
grep -aq "Removed user configuration for fresh" "$output"
grep -aq "Enter send" "$output"
if grep -aq "sk-test-123" "$output"; then exit 1; fi
