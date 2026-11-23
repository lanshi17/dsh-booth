#!/usr/bin/env bash
# Typing filters the session picker and Enter resumes that session.
set -euo pipefail

booth=$1
kernel=$2
root=$(mktemp -d)
output=$(mktemp)
trap 'rm -rf "$root" "$output"' EXIT

for entry in "s-old:first session" "s-older:second session" "s-oldest:third session"; do
  id=${entry%%:*}
  title=${entry#*:}
  mkdir -p "$root/dsh/sessions/--tmp--/$id"
  printf '{"type":"session/title","data":{"title":"%s"}}\n' "$title" > "$root/title.jsonl"
  zstd -q -o "$root/dsh/sessions/--tmp--/$id/session.v4.jsonl.zstd" "$root/title.jsonl"
done

( printf '/sessions\n'; sleep 1; printf 'second'; sleep 0.5; printf '\r'; sleep 1.2
  printf '/session\n'; sleep 0.6; printf '\x04' ) |
  timeout 20 env BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$root/state" DSH_HOME="$root/dsh" \
    script -qec "$booth" /dev/null >"$output" 2>&1 || true

grep -aq "session s-older · second session" "$output"
