#!/usr/bin/env bash
# Perland Launcher: cloud development environment setup (Ubuntu), e.g. the setup script of a
# Claude Code on the web environment. Mirrors .github/workflows/ci.yml. Safe to re-run.
#
# Environment settings it expects (configured outside this script, never committed):
# - Network access to crates.io, static.rust-lang.org, sh.rustup.rs, registry.npmjs.org,
#   nodejs.org, cli.github.com and github.com (plus api.modrinth.com,
#   piston-meta.mojang.com and meta.fabricmc.net for live install checks; unit tests don't need them).
# - Optional GH_TOKEN secret (Issues + Pull requests read/write on PerlandDev repos) so `gh` works,
#   which the reporter-updates skill needs to post to Perlita.
#
# Linux only: Windows-specific work (NSIS installer, SmartScreen, signing) and the real app window
# can't be checked here; the owner smoke-tests those on Windows.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
SUDO=""
if [ "$(id -u)" -ne 0 ]; then SUDO="sudo"; fi

# 1. System packages: Tauri/WebKit build deps (same list as CI), Java for the launch tests, basics.
$SUDO apt-get update -y
$SUDO apt-get install -y --no-install-recommends \
  build-essential pkg-config curl wget file git unzip xz-utils ca-certificates gnupg \
  libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  openjdk-21-jdk-headless

# 2. GitHub CLI: PRs, CI status and Perlita reporter updates.
if ! command -v gh >/dev/null 2>&1; then
  curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg \
    | $SUDO dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg status=none
  echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" \
    | $SUDO tee /etc/apt/sources.list.d/github-cli.list >/dev/null
  $SUDO apt-get update -y
  $SUDO apt-get install -y gh
fi

# 3. Rust stable with rustfmt and clippy (rust-toolchain.toml pins "stable").
if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
# shellcheck disable=SC1091
. "$HOME/.cargo/env"
rustup toolchain install stable --component rustfmt,clippy
rustup default stable

# 4. Node 24 and pnpm 11.5.0 (package.json "packageManager"). Node comes from the official
# nodejs.org tarball, checked against its SHASUMS256.txt, into /opt/node24: deb.nodesource.com
# isn't reachable from every environment, and an image's own Node (e.g. /opt/node22/bin) can sit
# ahead of /usr/bin on PATH. node, npm and pnpm are linked into ~/.local/bin, which comes first.
NODE_DIR=/opt/node24
LINK_DIR="$HOME/.local/bin"
if ! "$NODE_DIR/bin/node" --version 2>/dev/null | grep -q '^v24\.'; then
  case "$(uname -m)" in
    x86_64) NODE_ARCH=x64 ;;
    aarch64 | arm64) NODE_ARCH=arm64 ;;
    *) echo "No Node 24 build for $(uname -m)" >&2; exit 1 ;;
  esac
  NODE_DIST=https://nodejs.org/dist/latest-v24.x
  NODE_TMP="$(mktemp -d)"
  curl -fsSL "$NODE_DIST/SHASUMS256.txt" -o "$NODE_TMP/SHASUMS256.txt"
  NODE_TARBALL="$(grep -o "node-v24\.[0-9.]*-linux-$NODE_ARCH\.tar\.xz" "$NODE_TMP/SHASUMS256.txt" | head -n 1)"
  curl -fsSL "$NODE_DIST/$NODE_TARBALL" -o "$NODE_TMP/$NODE_TARBALL"
  (cd "$NODE_TMP" && grep " $NODE_TARBALL\$" SHASUMS256.txt | sha256sum -c --quiet -)
  $SUDO rm -rf "$NODE_DIR"
  $SUDO mkdir -p "$NODE_DIR"
  $SUDO tar -xJf "$NODE_TMP/$NODE_TARBALL" -C "$NODE_DIR" --strip-components=1
  rm -rf "$NODE_TMP"
fi
export PATH="$NODE_DIR/bin:$PATH"
$SUDO env PATH="$PATH" npm install -g pnpm@11.5.0
mkdir -p "$LINK_DIR"
for bin in "$NODE_DIR"/bin/*; do ln -sf "$bin" "$LINK_DIR/$(basename "$bin")"; done

# 5. Warm the caches when run from the repo root (skipped otherwise).
if [ -f package.json ] && grep -q '"packageManager": "pnpm@' package.json; then
  pnpm install --frozen-lockfile
  pnpm --filter ui build # Tauri embeds ui/dist, so cargo builds need it
  cargo fetch
fi

echo "rust: $(rustc --version) | node: $(node --version) | pnpm: $(pnpm --version) | java: $(java -version 2>&1 | grep -m 1 version)"
