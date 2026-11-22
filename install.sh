#!/usr/bin/env sh
# Installs the booth binary. The booth dsh profile is created on first start.
set -eu

root=$(cd "$(dirname "$0")" && pwd)
prefix=${PREFIX:-$HOME/.local/bin}
binary=${BOOTH_BINARY:-$root/build/booth}

if [ ! -x "$binary" ]; then
  echo "no binary at $binary; run: cmake -S . -B build -G Ninja && cmake --build build" >&2
  exit 1
fi

mkdir -p "$prefix"
install -m 0755 "$binary" "$prefix/booth"
echo "installed $prefix/booth"
echo "booth starts dsh --profile booth; the profile is created on first run"
