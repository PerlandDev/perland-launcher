#!/usr/bin/env bash
# Perland Launcher: cloud development environment setup (Ubuntu), e.g. the setup script of a
# Claude Code on the web environment. Mirrors .github/workflows/ci.yml. Safe to re-run.
#
# Environment settings it expects (configured outside this script, never committed):
# - Network access to crates.io, static.rust-lang.org, sh.rustup.rs, registry.npmjs.org,
#   deb.nodesource.com, cli.github.com and github.com (plus api.modrinth.com,
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
  build-essential pkg-config curl wget file git unzip ca-certificates gnupg \
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
rustup toolchain install stable --component rustfmt clippy
rustup default stable

# 4. Node 24 and pnpm 11.5.0 (package.json "packageManager").
if ! node --version 2>/dev/null | grep -q '^v24\.'; then
  curl -fsSL https://deb.nodesource.com/setup_24.x | $SUDO -E bash -
  $SUDO apt-get install -y nodejs
fi
$SUDO npm install -g pnpm@11.5.0

# 5. Warm the caches when run from the repo root (skipped otherwise).
if [ -f package.json ] && grep -q '"packageManager": "pnpm@' package.json; then
  pnpm install --frozen-lockfile
  pnpm --filter ui build # Tauri embeds ui/dist, so cargo builds need it
  cargo fetch
fi

echo "rust: $(rustc --version) | node: $(node --version) | pnpm: $(pnpm --version) | java: $(java -version 2>&1 | head -n 1)"
