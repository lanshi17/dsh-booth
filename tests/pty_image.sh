#!/usr/bin/env bash
# Ctrl+V reads a clipboard raster and sends it as an ACP image block.
set -euo pipefail

booth=$1
kernel=$2
state=$(mktemp -d)
bin=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$bin" "$output"' EXIT

printf '#!/bin/sh\nprintf '%s' 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6ZQAAAABJRU5ErkJggg==' | base64 -d\n' >"$bin/wl-paste"
chmod +x "$bin/wl-paste"

( sleep 1; printf '\026'; sleep 0.3; printf 'look\r'; sleep 0.8; printf '\004' ) |
  timeout 20 script -qec "PATH='$bin':\$PATH BOOTH_DSH_BIN='$kernel' BOOTH_STATE_DIR='$state' '$booth'" /dev/null \
    >"$output" 2>&1 || true

grep -aq "image/png" "$output"
