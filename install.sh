#!/bin/sh
# Install and start the latest booth release:
#   curl -fsSL https://raw.githubusercontent.com/lanshi17/dsh-booth/master/install.sh | sh
set -eu

REPOSITORY=https://github.com/lanshi17/dsh-booth
DSH_VERSION=0.2.1-rc.3
HOME="${HOME:-$(cd && pwd)}"
PREFIX="$HOME/.local"

say() { printf '%s\n' "$*"; }
die() { printf 'install: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }
add_path() { PATH="$1:$PATH"; export PATH; }

have curl || die 'curl is required'
[ "$(uname -s)" = Linux ] || die 'booth currently supports Linux only'
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64; node_os_arch=linux-x64 ;;
  aarch64 | arm64) arch=aarch64; node_os_arch=linux-arm64 ;;
  *) die "unsupported architecture: $(uname -m)" ;;
esac

mkdir -p "$PREFIX" "$PREFIX/bin"

if have booth; then
  previous_bin=$(dirname "$(command -v booth)")
  if [ -w "$previous_bin" ]; then BIN_DIR=$previous_bin; else BIN_DIR="$PREFIX/bin"; fi
else
  BIN_DIR="$PREFIX/bin"
fi
mkdir -p "$BIN_DIR"

DATA_DIR="$(dirname "$BIN_DIR")/share/dsh-booth"

# Make the installed commands available in new login shells. Keep the edit to
# the active shell family; the install command has already updated this process.
case "${SHELL##*/}" in
  zsh) shell_profile="$HOME/.zprofile" ;;
  bash) shell_profile="$HOME/.bash_profile" ;;
  fish) shell_profile="$HOME/.config/fish/config.fish" ;;
  *) shell_profile="$HOME/.profile" ;;
esac
case "$BIN_DIR" in
  "$PREFIX/bin") profile_paths="$BIN_DIR" ;;
  *) profile_paths="$BIN_DIR:$PREFIX/bin" ;;
esac
mkdir -p "$(dirname "$shell_profile")"
if [ ! -f "$shell_profile" ]; then : > "$shell_profile"; fi
if ! grep -Fq "$BIN_DIR" "$shell_profile" || ! grep -Fq "$PREFIX/bin" "$shell_profile"; then
  if [ "${SHELL##*/}" = fish ]; then
    printf '\nfish_add_path "%s" "%s"\n' "$BIN_DIR" "$PREFIX/bin" >> "$shell_profile"
  else
    printf '\nexport PATH="%s:$PATH"\n' "$profile_paths" >> "$shell_profile"
  fi
fi
add_path "$BIN_DIR"
add_path "$PREFIX/bin"

# dsh needs Node 22.19+ or 24+. Reuse it if present; otherwise install Node 22
# LTS after verifying the checksum published alongside the archive.
node_ok=false
if have node; then
  node_version=$(node -p 'process.versions.node' 2>/dev/null || true)
  node_major=$(printf '%s' "$node_version" | cut -d. -f1)
  node_minor=$(printf '%s' "$node_version" | cut -d. -f2)
  if { [ "$node_major" = 22 ] && [ "${node_minor:-0}" -ge 19 ]; } || [ "${node_major:-0}" -ge 24 ]; then
    node_ok=true
  fi
fi
if [ "$node_ok" != true ]; then
  say 'installing Node 22 LTS'
  sums=$(curl -fsSL https://nodejs.org/dist/latest-v22.x/SHASUMS256.txt)
  archive=$(printf '%s\n' "$sums" | awk -v os="$node_os_arch" '$2 ~ "node-v.*-" os "[.]tar[.]xz$" { print $2; exit }')
  [ -n "$archive" ] || die "no Node 22 build for $node_os_arch"
  expected=$(printf '%s\n' "$sums" | awk -v name="$archive" '$2 == name { print $1; exit }')
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT HUP INT TERM
  curl -fsSL "https://nodejs.org/dist/latest-v22.x/$archive" -o "$tmp/$archive"
  actual=$(sha256sum "$tmp/$archive" | awk '{ print $1 }')
  [ "$actual" = "$expected" ] || die 'Node download checksum did not match'
  tar -xJf "$tmp/$archive" -C "$PREFIX"
  node_home="$PREFIX/${archive%.tar.xz}"
  ln -sf "$node_home/bin/node" "$PREFIX/bin/node"
  ln -sf "$node_home/bin/npm" "$PREFIX/bin/npm"
  ln -sf "$node_home/bin/npx" "$PREFIX/bin/npx"
  add_path "$node_home/bin"
  rm -rf "$tmp"
  trap - EXIT HUP INT TERM
fi
have node || die 'Node installation failed'

# Bun is the package manager and runtime used by the team service. Reuse it when
# present; otherwise install the official Bun binary into this user's prefix.
if ! have bun; then
  say 'installing Bun'
  bun_home="$PREFIX/.bun"
  curl -fsSL https://bun.sh/install | BUN_INSTALL="$bun_home" bash -s --
  add_path "$bun_home/bin"
fi
have bun || die 'Bun installation failed'
add_path "$(bun pm bin -g 2>/dev/null || true)"

if ! have dsh || [ "$(dsh --version 2>/dev/null || true)" != "$DSH_VERSION" ]; then
  say 'installing dsh'
  bun add --global --exact "@deepseek-ai/dsh@$DSH_VERSION"
fi
add_path "$(bun pm bin -g 2>/dev/null || true)"
bun --version >/dev/null 2>&1 || die 'Bun could not be started'
dsh --version >/dev/null 2>&1 || die 'dsh could not be started'

case "$arch" in x86_64) ASSET=booth-linux-x86_64 ;; aarch64) ASSET=booth-linux-aarch64 ;; esac
release_url="$REPOSITORY/releases/latest/download"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
say 'installing booth'
curl -fsSL "$release_url/$ASSET" -o "$tmp/$ASSET"
curl -fsSL "$release_url/$ASSET.sha256" -o "$tmp/$ASSET.sha256"
(cd "$tmp" && sha256sum -c "$ASSET.sha256") || die 'booth download checksum did not match'
curl -fsSL "$release_url/booth-runtime.tar.gz" -o "$tmp/booth-runtime.tar.gz"
curl -fsSL "$release_url/booth-runtime.tar.gz.sha256" -o "$tmp/booth-runtime.tar.gz.sha256"
(cd "$tmp" && sha256sum -c booth-runtime.tar.gz.sha256) || die 'booth runtime download checksum did not match'
chmod 755 "$tmp/$ASSET"
mkdir -p "$DATA_DIR"
version=$($tmp/$ASSET --version | sed -n 's/^booth //p')
case "$version" in '' | *[!0-9.]* ) die 'downloaded booth reported an invalid version' ;; esac
runtime_dir="$DATA_DIR/releases/$version"
if [ ! -f "$runtime_dir/.booth-runtime-installed" ]; then
  staged_runtime="$DATA_DIR/releases/.${version}.$$"
  mkdir -p "$staged_runtime"
  tar -xzf "$tmp/booth-runtime.tar.gz" -C "$staged_runtime"
  [ -f "$staged_runtime/superagent/lib/index.js" ] || die 'team bundle is incomplete'
  [ -f "$staged_runtime/superagent/node_modules/zod/package.json" ] || die 'team bundle dependencies are incomplete'
  [ -f "$staged_runtime/superagent/node_modules/proper-lockfile/package.json" ] || die 'team bundle dependencies are incomplete'
  [ -f "$staged_runtime/local-source/index.js" ] || die 'local portal source is missing'
  printf '%s\n' "$version" > "$staged_runtime/superagent/.booth-prebuilt"
  : > "$staged_runtime/.booth-runtime-installed"
  mv "$staged_runtime" "$runtime_dir"
fi
mkdir -p "$DATA_DIR/omp-portal"
cp "$runtime_dir/omp-portal/extension.ts" "$runtime_dir/omp-portal/config.yml" "$runtime_dir/omp-portal/LICENSE" "$DATA_DIR/omp-portal/"
printf '%s\n' "$runtime_dir/superagent" > "$DATA_DIR/bundle-path.tmp"
mv "$DATA_DIR/bundle-path.tmp" "$DATA_DIR/bundle-path"
mv "$tmp/$ASSET" "$BIN_DIR/booth"

say 'preparing the team service'
booth init
say 'starting the team service'
booth restart

if [ "${BOOTH_NO_CONSOLE:-}" = 1 ] || [ ! -r /dev/tty ]; then
  say 'installed booth. Open it with: booth'
  exit 0
fi

if ! have omp || ! omp --version >/dev/null 2>&1; then
  say 'installing OMP'
  curl -fsSL https://omp.sh/install | env PI_INSTALL_DIR="$BIN_DIR" sh -
fi
have omp || die 'OMP installation failed; booth is running, install OMP with: curl -fsSL https://omp.sh/install | sh'

# Bun's global installer can leave OMP's platform package out when an existing
# source install is reused. OMP itself still answers `--version`, but the first
# real console start then fails when pi-natives tries to load its .node addon.
# Repair only that source-install shape; the standalone OMP binary carries its
# native addon and has no @oh-my-pi/pi-natives package to repair.
ensure_omp_native() {
  native_package="@oh-my-pi/pi-natives-$node_os_arch"
  global_bin=$(bun pm bin -g 2>/dev/null || true)
  [ -n "$global_bin" ] || return 0
  global_modules="$(dirname "$global_bin")/install/global/node_modules"
  wrapper="$global_modules/@oh-my-pi/pi-natives"
  [ -f "$wrapper/package.json" ] || return 0

  version=$(node -e 'process.stdout.write(require(process.argv[1]).version)' "$wrapper/package.json" 2>/dev/null || true)
  [ -n "$version" ] || die "cannot determine the installed OMP native package version"
  leaf="$global_modules/$native_package"
  if find "$leaf" -maxdepth 2 -type f -name "pi_natives.${node_os_arch}*.node" -print -quit 2>/dev/null | grep -q .; then
    return 0
  fi

  say "repairing OMP native addon ($native_package@$version)"
  bun add --global --exact "$native_package@$version"
  find "$leaf" -maxdepth 2 -type f -name "pi_natives.${node_os_arch}*.node" -print -quit 2>/dev/null | grep -q . \
    || die "OMP native addon installation failed; run: bun add --global --exact $native_package@$version"
}

ensure_omp_native

say 'opening the booth console'
exec booth </dev/tty >/dev/tty
