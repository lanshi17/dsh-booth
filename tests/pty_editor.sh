#!/usr/bin/env bash
# Opens $EDITOR from the composer and submits the edited text.
set -euo pipefail

booth=$1
kernel=$2
root=$(mktemp -d)
outfile=$(mktemp)

cleanup() {
  rm -rf "$root" "$outfile"
}
trap cleanup EXIT

cat > "$root/editor.sh" <<'EDITOR'
#!/bin/sh
printf 'edited prompt' > "$1"
EDITOR
chmod +x "$root/editor.sh"

( printf 'draft'; sleep 2; printf '\x07'; sleep 0.8; printf '\r'; sleep 1; printf '\x04' ) |
  timeout 20 env VISUAL= BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$root/state" EDITOR="$root/editor.sh" \
    script -qec "$booth" /dev/null > "$outfile" 2>&1 || true

grep -aq "edited prompt" "$outfile"
