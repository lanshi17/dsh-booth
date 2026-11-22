#!/usr/bin/env bash
# Resumes a persisted dsh session and checks that the TUI replays its text.
set -euo pipefail

booth=$1
kernel=$2
root=$(mktemp -d)
out=$(mktemp)

cleanup() {
  rm -rf "$root" "$out"
}
trap cleanup EXIT

mkdir -p "$root/dsh/sessions/--tmp--/s-old"
cat > "$root/session.jsonl" <<'JSONL'
{"type":"session/title","data":{"title":"stored title"}}
{"type":"user/message","data":{"source":{"kind":"user"},"content":[{"type":"text","text":"replayed question"}]}}
{"type":"tool/call","time":1000,"data":{"callId":"c1","name":"bash","arguments":"{\"command\":\"ls\"}"}}
{"type":"tool/result","time":1033,"data":{"message":{"toolCallId":"c1","isError":false,"content":[{"type":"text","text":"tool-replay-output"}]}}}
{"type":"assistant/message","data":{"message":{"content":[{"type":"reasoning","text":"replayed thinking"},{"type":"text","text":"replayed answer"}]}}}
JSONL
zstd -q -o "$root/dsh/sessions/--tmp--/s-old/session.v4.jsonl.zstd" "$root/session.jsonl"

( printf 'hi\n'; sleep 1; printf '/sessions\n'; sleep 1; printf '\r'; sleep 2; printf '\x04' ) |
  timeout 20 env BOOTH_DSH_BIN="$kernel" BOOTH_STATE_DIR="$root/state" DSH_HOME="$root/dsh" \
    script -qec "$booth" /dev/null > "$out" 2>&1 || true

grep -aq "replayed answer" "$out"
grep -aq "tool-replay-output" "$out"
grep -aq "resumed stored title" "$out"
