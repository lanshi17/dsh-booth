#!/usr/bin/env sh
# Installs the booth binary. The booth dsh profile is created on first start.
set -eu

root=$(cd "$(dirname "$0")" && pwd)
prefix=${PREFIX:-$HOME/.local/bin}
if [ -n "${BOOTH_BINARY:-}" ]; then
  binary=$BOOTH_BINARY
else
  binary=$root/build/booth
  release=$root/build-release/booth
  if [ -x "$release" ] && { [ ! -x "$binary" ] || [ "$release" -nt "$binary" ]; }; then
    binary=$release
  fi
fi

if [ ! -x "$binary" ]; then
  echo "no binary at $binary; run: cmake -S . -B build -G Ninja && cmake --build build" >&2
  exit 1
fi

mkdir -p "$prefix"
install -m 0755 "$binary" "$prefix/booth"
share=$(cd "$prefix/.." && pwd)/share/booth
install -d "$share"
install -m 0644 "$root/CHANGELOG.md" "$share/CHANGELOG.md"
echo "installed $prefix/booth from $binary"
if command -v python3 >/dev/null 2>&1 && command -v dsh >/dev/null 2>&1; then
  python3 "$root/tools/patch_dsh_model_mentions.py" || echo "warning: could not patch installed dsh model mentions" >&2
  python3 "$root/tools/patch_dsh_service_tier.py" || echo "warning: could not patch installed dsh service tier" >&2
  if [ -f "$HOME/.omp/agent/models.yml" ]; then
    python3 "$root/tools/sync_omp_models.py" || echo "warning: could not import OMP models" >&2
  fi
fi
echo "booth starts dsh --profile booth; the profile is created on first run"
