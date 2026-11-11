#!/bin/sh
# One-shot install for booth. From a shell:
#
#   curl -fsSL https://raw.githubusercontent.com/lanshi17/dsh-booth/master/install.sh | sh
#
# It installs anything missing (Rust, Node 22, pnpm, dsh), clones this repository,
# builds booth, prepares the superagent profile, and starts the service. When a
# terminal is available it then opens the console.
#
#   BOOTH_HOME          checkout directory (default: ~/.dsh/src/dsh-booth)
#   BOOTH_REF           git ref to install (default: master)
#   DSH_VERSION         @deepseek-ai/dsh version (default: 0.2.1-rc.3)
#   BOOTH_NO_UP=1       install only; do not start the service
#   BOOTH_NO_CONSOLE=1  start the service, but do not open the console
set -eu

REPO="${BOOTH_REPO:-https://github.com/lanshi17/dsh-booth.git}"
REF="${BOOTH_REF:-master}"
DSH_VERSION="${DSH_VERSION:-0.2.1-rc.3}"
HOME="${HOME:-$(cd && pwd)}"
DEST="${BOOTH_HOME:-$HOME/.dsh/src/dsh-booth}"
PREFIX="${BOOTH_PREFIX:-$HOME/.local}"
PNPM_HOME="${PNPM_HOME:-$HOME/.local/share/pnpm}"

say() { printf '%s\n' "$*"; }
die() { printf 'install: %s\n' "$*" >&2; exit 1; }

have() { command -v "$1" >/dev/null 2>&1; }

prepend() {
  case ":$PATH:" in
    *":$1:"*) ;;
    *) PATH="$1:$PATH" ;;
  esac
  export PATH
}

os=$(uname -s)
cpu=$(uname -m)
case "$os" in
  Linux) node_os=linux ;;
  Darwin) node_os=darwin ;;
  *) die "unsupported system: $os" ;;
esac
case "$cpu" in
  x86_64 | amd64) node_cpu=x64 ;;
  aarch64 | arm64) node_cpu=arm64 ;;
  *) die "unsupported architecture: $cpu" ;;
esac

have git || die "git is required; install it, then run this again"
have curl || die "curl is required; install it, then run this again"

if ! have cargo; then
  say "installing Rust"
  curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi
prepend "$HOME/.cargo/bin"
have cargo || die "cargo is not on PATH after installing Rust"

node_major=0
if have node; then
  node_major=$(node -v | sed 's/^v//' | cut -d. -f1)
fi
if [ "$node_major" -lt 22 ]; then
  say "installing Node 22"
  sums=$(curl -fsSL https://nodejs.org/dist/latest-v22.x/SHASUMS256.txt)
  name=$(printf '%s\n' "$sums" | awk -v os="$node_os" -v cpu="$node_cpu" '
    $2 ~ ("node-v[0-9.]+-" os "-" cpu "\\.tar\\.xz$") { print $2; exit }')
  [ -n "$name" ] || die "cannot find a Node 22 build for $node_os-$node_cpu"
  hash=$(printf '%s\n' "$sums" | awk -v name="$name" '$2 == name { print $1; exit }')
  tmp=$(mktemp -d)
  curl -fsSL "https://nodejs.org/dist/latest-v22.x/$name" -o "$tmp/$name"
  if have sha256sum; then
    printf '%s  %s\n' "$hash" "$name" | (cd "$tmp" && sha256sum -c -)
  elif have shasum; then
    printf '%s  %s\n' "$hash" "$name" | (cd "$tmp" && shasum -a 256 -c -)
  else
    die "need sha256sum or shasum to verify the Node download"
  fi
  mkdir -p "$PREFIX"
  tar -xJf "$tmp/$name" -C "$PREFIX"
  rm -rf "$tmp"
  extracted=$(printf '%s\n' "$name" | sed 's/\.tar\.xz$//')
  prepend "$PREFIX/$extracted/bin"
fi
have node || die "node is not on PATH"
node_major=$(node -v | sed 's/^v//' | cut -d. -f1)
[ "$node_major" -ge 22 ] || die "node $(node -v) is too old; booth needs Node 22 or newer"

if ! have pnpm; then
  say "installing pnpm"
  curl -fsSL https://get.pnpm.io/install.sh | env PNPM_HOME="$PNPM_HOME" SHELL=/bin/sh sh -
fi
prepend "$PNPM_HOME"
have pnpm || die "pnpm is not on PATH after installing it"

if ! have dsh; then
  say "installing dsh $DSH_VERSION"
  mkdir -p "$PREFIX"
  npm install -g --prefix "$PREFIX" "@deepseek-ai/dsh@$DSH_VERSION"
fi
prepend "$PREFIX/bin"
have dsh || die "dsh is not on PATH after installing it"

if [ -d "$DEST/.git" ]; then
  say "updating $DEST"
  git -C "$DEST" fetch origin "$REF"
  git -C "$DEST" checkout --detach FETCH_HEAD
  git -C "$DEST" submodule update --init --recursive
else
  say "cloning $REPO"
  mkdir -p "$(dirname "$DEST")"
  git clone --recursive --branch "$REF" "$REPO" "$DEST"
fi

say "installing booth"
cargo install --path "$DEST" --locked --force
prepend "$HOME/.cargo/bin"
have booth || die "booth is not on PATH after installing it"

if [ "${BOOTH_NO_UP:-}" = 1 ]; then
  say "installed. start it with: booth init && booth up && booth"
  exit 0
fi

say "preparing the profile"
BOOTH_SUPERAGENT_DIR="$DEST/superagent" booth init
say "starting the service"
BOOTH_SUPERAGENT_DIR="$DEST/superagent" booth up

if [ "${BOOTH_NO_CONSOLE:-}" = 1 ] || [ ! -r /dev/tty ]; then
  say "the service is up. open the console with: booth"
  exit 0
fi

say "opening the console"
exec booth </dev/tty >/dev/tty
