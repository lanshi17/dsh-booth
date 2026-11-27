#!/usr/bin/env bash
# SSH+tmux: Ctrl+V reads tmux show-buffer; copy writes tmux load-buffer.
set -euo pipefail
booth=$1
kernel=$2
state=$(mktemp -d)
bin=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$state" "$bin" "$output"' EXIT
buffer=$state/tmux-buffer.txt
cat >"$bin/tmux" <<SH
#!/bin/sh
case "\$1" in
  show-buffer|save-buffer) printf 'tmux buffer text';;
  load-buffer) cat > "$buffer";;
  show-options) printf 'off';;
esac
SH
chmod +x "$bin/tmux"

(
  sleep 2
  printf '\026'; sleep 1
  printf 'hi\r'; sleep 2
  printf '/copy answer\r'; sleep 2
  printf '\004'
) | timeout 25 env PATH="$bin:$PATH" TMUX='/tmp/tmux-1000/default,1,0' SSH_CONNECTION='10.0.0.1 1 10.0.0.2 2' \
      BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$state" DSH_HOME="$state/dsh" \
      script -qec "stty rows 30 cols 100; $booth" /dev/null >"$output" 2>&1 || true

grep -aq "tmux buffer pasted" "$output"
grep -aq "tmux buffer text" "$output"
grep -aq "hello" "$buffer"
grep -aq "]52;c;" "$output"
