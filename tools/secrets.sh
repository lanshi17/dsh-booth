#!/usr/bin/env bash
# Secret scan for the release. Fails closed: a missing scanner or an unreadable input is a
# failure, never a pass.
#
# Two rulers: the tracked tree (what ships) and the full history (what a tag publishes).
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

fail=0
# Values that look like real credentials, not test doubles. Provider keys keep their prefix in
# the secret (`sk-or-v1-...` for OpenRouter, `sk-proj-...` for OpenAI), so an `sk-` tail may
# contain `-` or `_`; without them such a key is split at the prefix and passes as prose.
PATTERN='(sk-[A-Za-z0-9_-]{20,}|ghp_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----)'

# scan <label> <command...>
# The scanner's exit code is read directly: 1 means "no match" for grep, anything else is a
# broken scanner and therefore a failed layer.
scan() {
  label=$1
  shift
  out=$("$@" 2>&1)
  code=$?
  case "$code" in
    0)
      echo "== $label =="
      echo "$out" | head -20
      fail=1
      ;;
    1)
      echo "== $label == (clean)"
      ;;
    *)
      echo "== $label == scanner failed with exit $code"
      echo "$out" | head -5
      fail=1
      ;;
  esac
}

scan "tracked files" git grep -nE "$PATTERN" -- . ':!tools/secrets.sh'

# History needs its own path: `git log -p | grep` would hide git's own failure inside the pipe.
# The patch is written under target/, which a checkout that has not built yet does not have. Make
# it first: otherwise the redirect fails and the layer blames git for an unreadable history.
mkdir -p target || exit 2
git log --all -p >target/history.patch 2>target/history.err
if [ $? -ne 0 ]; then
  echo "== history == cannot read git history"
  head -3 target/history.err
  fail=1
else
  scan "history" grep -nE "$PATTERN" target/history.patch
fi

echo "== files that must never be committed =="
for path in provider.json .credentials.yaml oauth-url.txt booth-inbox.jsonl booth-outbox.jsonl; do
  if git ls-files --error-unmatch "$path" >/dev/null 2>&1; then
    echo "tracked: $path"
    fail=1
  fi
done

if [ "$fail" -eq 0 ]; then
  echo "no credential-shaped strings, no secret files tracked"
fi
exit "$fail"
