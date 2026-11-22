#!/usr/bin/env bash
# The keybinding file can move the external editor off Ctrl+G.
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
printf 'edited by binding' > "$1"
EDITOR
chmod +x "$root/editor.sh"
printf 'booth.editor: ctrl+t\n' > "$root/keybindings.yml"

( printf 'draft'; sleep 2; printf '\x14'; sleep 0.8; printf '\r'; sleep 1; printf '\x04' ) |
  timeout 20 env VISUAL= BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$root" EDITOR="$root/editor.sh" \
    script -qec "$booth" /dev/null >"$outfile" 2>&1 || true

grep -aq "edited by binding" "$outfile"
