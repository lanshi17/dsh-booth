#!/usr/bin/env bash
# Walks every page of session/list and shows the stored title for each session.
set -euo pipefail

booth=$1
kernel=$2
root=$(mktemp -d)
out=$(mktemp)

cleanup() {
  rm -rf "$root" "$out"
}
trap cleanup EXIT

mkdir -p "$root/dsh/sessions/--tmp--"
for entry in "s-old:first session" "s-older:second session" "s-oldest:third session"; do
  id=${entry%%:*}
  title=${entry#*:}
  mkdir -p "$root/dsh/sessions/--tmp--/$id"
  printf '{"type":"session/title","data":{"title":"%s"}}\n' "$title" > "$root/title.jsonl"
  zstd -q -o "$root/dsh/sessions/--tmp--/$id/session.v4.jsonl.zstd" "$root/title.jsonl"
done

( printf '/sessions\n'; sleep 2; printf '\x1b'; sleep 0.3; printf '\x04' ) |
  timeout 20 env BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$root/state" DSH_HOME="$root/dsh" \
    script -qec "$booth" /dev/null > "$out" 2>&1 || true

grep -aq "first session" "$out"
grep -aq "second session" "$out"
grep -aq "third session" "$out"
