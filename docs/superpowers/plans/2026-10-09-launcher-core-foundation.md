# Launcher Core Foundation (Plan 1 of 6) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A headless Rust library (`launcher-core`) and a developer CLI (`perland-cli`) that can install vanilla, Fabric or Quilt Minecraft, fetch Mojang's official Java, and launch the game with an offline/dev account. Every step is covered by tests and CI on Windows and Linux.

**Architecture:** A Cargo workspace. `crates/launcher-core` holds all logic, with no UI or Tauri dependency. It is split into focused modules: `download` (hash-checked, resumable, concurrent fetches), `meta` (upstream metadata with an ETag cache, plus the version JSON model, rules, maven coordinates and loader merge), `java`, `install`, `instances`, `accounts` and `launch`. `crates/perland-cli` is a thin harness used for manual smoke tests and the nightly real-install CI job. The Tauri app (Plan 5) will call the same library.

**Tech Stack:** Rust stable (edition 2021), tokio, reqwest (rustls), serde/serde_json, thiserror, sha1/sha2/md-5, zip, clap. Tests use tempfile and wiremock. CI runs on GitHub Actions (windows-latest and ubuntu-latest).

**Spec:** `docs/superpowers/specs/2026-10-09-launcher-core-design.md`

### Plan series (the spec is delivered in six plans, each ends in working software)

| Plan | Delivers |
|---|---|
| **1 (this)** | Core foundation: download, metadata, rules, vanilla/Fabric/Quilt install, Mojang Java runtimes, instances, offline/dev accounts, launch, CLI harness, CI |
| 2 | Forge and NeoForge (installer processors), Java detection improvements |
| 3 | Pack sources and sync: `DesiredState`, packwiz, `.mrpack`, Modrinth API, the sync engine (lockfile, staging, trash, `preserve`), content-addressed object cache, CurseForge fallback, `launcher.toml`, nightly fuzzing |
| 4 | Microsoft sign-in (PKCE loopback + device code), XSTS error mapping, OS keyring storage, offline unlock |
| 5 | Desktop app: Tauri 2 shell + React UI (styled with `/impeccable`), en/es, first run, Browse, `perland://pack` |
| 6 | Distribution: updater, NSIS/AppImage/deb, signing, release CI, log rotation, diagnostics export |

**One deviation from the spec, on purpose:** spec §2.1 puts a content-addressed cache in `download`. In Plan 1, game files already live in shared `libraries/`, `assets/` and `versions/` folders, so ten instances on the same version still download once. The `cache/objects` store only adds value for pack files, so it arrives with sync in Plan 3.

## Global Constraints

- License **GPL-3.0-only**. No Modrinth branding assets anywhere.
- `launcher-core` must not depend on Tauri or any UI crate.
- Rust stable, edition 2021. Must build and pass tests on **Windows x64 and Linux x64**.
- Data directory name: `PerlandLauncher` (`%APPDATA%\PerlandLauncher`, or `$XDG_DATA_HOME/PerlandLauncher`, falling back to `~/.local/share/PerlandLauncher`).
- Upstream URLs exactly as in spec §2.2: `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json`, `https://meta.fabricmc.net/v2`, `https://meta.quiltmc.org/v3`, the Mojang `java-runtime` manifest.
- Every HTTP request sends a descriptive `User-Agent` (`PerlandLauncher/<version> (+https://github.com/PerlandDev/perland-launcher)`).
- User-facing failures are a `LauncherError` with a category (`Network, Auth, Integrity, Disk, Java, Loader, Pack, Game`), a plain-language message, an optional fix and an optional technical detail.
- Tokens never reach logs. Game output goes through `Redactor`.
- No telemetry.
- **Heavy tests (fuzzing, stress, load) only run in CI, never on a developer PC.** Local tests use tiny inputs.
- Every commit message ends with the trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **The same destination twice in one batch.** Mojang's asset index `5` has 3598 entries but only 3575 unique hashes. Two concurrent writers on one `.part` file would corrupt it. Expected behaviour: duplicates are collapsed. Pinned by `fetch_all_dedupes_destinations` (Task 3) and `plan_assets_dedupes_hashes` (Task 9).
2. **Year-style and non-`1.x` version ids** (`26.3`, `26.4-snapshot-3` are the current latest). Expected behaviour: versions are looked up by exact id and nothing parses a `1.` prefix. Pinned by `manifest_finds_year_style_ids` (Task 5).
3. **Instance names that make bad folder names.** For example `CON` (reserved on Windows), `!!!`, or the same name twice. Expected behaviour: a valid, unique folder id. Pinned by `slugify_handles_reserved_and_symbols` and `duplicate_names_get_suffix` (Task 10).
4. **Redaction over-matching.** The offline access token is `"0"`. Redacting it would wipe every `0` in the game log. Expected behaviour: real tokens and the home path are redacted, and secrets shorter than 4 characters are ignored. Pinned by `short_secrets_are_ignored` (Task 12).
5. **A stale `.part` file left from an older version of the same file.** Resuming it produces a hash mismatch. Expected behaviour: discard and download fresh, without bothering the player. Pinned by `stale_part_file_self_heals` (Task 3).

---

### Task 0: Toolchain prerequisites (owner runs once on this PC)

The PC has Node 24, pnpm 11 and Java 25, but **no Rust and no MSVC build tools**. Installing the build tools needs an admin (UAC) prompt, so the owner runs these.

- [ ] **Step 1: Install the Visual Studio 2022 Build Tools with the C++ workload**

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

- [ ] **Step 2: Install rustup and the stable toolchain**

```powershell
winget install --id Rustlang.Rustup
rustup default stable
rustup component add rustfmt clippy
```

- [ ] **Step 3: Verify (open a new terminal first)**

Run: `cargo --version; rustc --version`
Expected: two version lines, with `rustc` at 1.85 or newer.

---

### Task 1: Workspace scaffold, repo files, CI, core primitives

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `.gitattributes`, `LICENSE`, `NOTICE`, `README.md`, `CLAUDE.md`, `AGENTS.md`, `.github/workflows/ci.yml`
- Create: `crates/launcher-core/Cargo.toml`, `crates/launcher-core/src/lib.rs`, `src/error.rs`, `src/events.rs`, `src/platform.rs`, `src/paths.rs` (all under `crates/launcher-core/`)

**Interfaces:**
- Produces:
  - `error::{ErrorCategory, LauncherError, Result<T>}` with the constructors `LauncherError::{new, network, auth, integrity, disk, java, loader, pack, game}`, the builders `.with_fix(impl Into<String>)` and `.with_detail(impl Display)`, and `From<std::io::Error>` (category `Disk`).
  - `events::{Event, EventSink, noop_sink}`, where `Event::Progress { task: String, done: u64, total: u64, bytes: u64 }`, `Event::Log { line: String }` and `Event::StateChanged { instance: String, state: String }`, and `EventSink = Arc<dyn Fn(Event) + Send + Sync>`.
  - `platform::{Os, Arch, Platform}` with `Platform { os, arch }` and the methods `current()`, `mojang_os_name() -> &'static str`, `classpath_separator() -> &'static str` and `java_runtime_key() -> Option<&'static str>`.
  - `paths::LauncherDirs` with `new(root)`, `default_root() -> Option<PathBuf>`, `root()`, `libraries()`, `assets()`, `versions()`, `java()`, `instances()`, `meta()`, `logs()`, `version_dir(id)`, `version_json(id)`, `client_jar(id)`, `natives_dir(id)` and `log_config(file_id)`.

- [ ] **Step 1: Create the workspace files**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/launcher-core"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "GPL-3.0-only"
repository = "https://github.com/PerlandDev/perland-launcher"

[workspace.dependencies]
tokio = { version = "1", features = ["fs", "io-util", "macros", "process", "rt-multi-thread", "sync", "time"] }
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json", "stream"] }
futures-util = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
sha1 = "0.10"
sha2 = "0.10"
md-5 = "0.10"
hex = "0.4"
zip = { version = "2", default-features = false, features = ["deflate"] }
clap = { version = "4", features = ["derive"] }
tempfile = "3"
wiremock = "0.6"
```

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

`.gitattributes`:
```
* text=auto eol=lf
*.json -text
```

`crates/launcher-core/Cargo.toml`:
```toml
[package]
name = "launcher-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
tokio.workspace = true
reqwest.workspace = true
futures-util.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
sha1.workspace = true
sha2.workspace = true
md-5.workspace = true
hex.workspace = true
zip.workspace = true

[dev-dependencies]
tempfile.workspace = true
wiremock.workspace = true
```

- [ ] **Step 2: Add the license and repo docs**

Run: `curl -sSfo LICENSE https://www.gnu.org/licenses/gpl-3.0.txt`
Expected: `LICENSE` starts with `GNU GENERAL PUBLIC LICENSE` / `Version 3, 29 June 2007`.

`NOTICE`:
```
Perland Launcher
Copyright (C) 2026 PerlandDev contributors
Licensed under the GNU General Public License, version 3 only (GPL-3.0-only).

Third-party code ported into this repository is listed below with its origin.
Files ported from Modrinth's app-lib (https://github.com/modrinth/code, packages/app-lib,
GPL-3.0-only) keep an attribution header naming the original file.

(no ported files yet)

"Minecraft" is a trademark of Mojang Synergies AB. This project is not affiliated with Mojang or Microsoft.
```

`README.md`:
````markdown
# Perland Launcher

A free, open-source Minecraft launcher for Perland players, and anyone else. Packs stay in sync on their own: press Play and you're on the right version.

> Status: early development. See `docs/superpowers/specs/` for the design and `docs/superpowers/plans/` for the build plans.

## Develop

Requirements: Rust stable (with MSVC build tools on Windows).

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## License

GPL-3.0-only. See `LICENSE` and `NOTICE`.
````

`CLAUDE.md` (and an identical `AGENTS.md`):
```markdown
# Perland Launcher: agent guide

- Design: `docs/superpowers/specs/2026-10-09-launcher-core-design.md`. Plans: `docs/superpowers/plans/`.
- `crates/launcher-core` holds all logic and must never depend on Tauri/UI crates. `crates/perland-cli` is a dev harness.
- TDD: write the failing test first. Pure logic gets `#[cfg(test)]` unit tests; HTTP behaviour gets `tests/*.rs` with wiremock.
- Never run fuzzing, stress or load tests locally. They run in CI only. Keep local test inputs tiny.
- Errors shown to players are `LauncherError` (category + plain message + optional fix + detail). Never put tokens in logs.
- UI styling and design-system work goes through the `/impeccable` skill.
- License GPL-3.0-only. Never copy Modrinth branding. Ported code keeps an attribution header and gets a `NOTICE` entry.
- Before committing: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
```

- [ ] **Step 3: Add the CI workflow**

`.github/workflows/ci.yml`:
```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:

jobs:
  rust:
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, ubuntu-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
```

- [ ] **Step 4: Write the failing tests for the primitives**

`crates/launcher-core/src/lib.rs`:
```rust
//! Perland Launcher core: everything except the UI.
pub mod error;
pub mod events;
pub mod paths;
pub mod platform;

pub use error::{ErrorCategory, LauncherError, Result};
```

`crates/launcher-core/src/error.rs` (tests first, with the types stubbed so it compiles):
```rust
use std::fmt::Display;

use serde::Serialize;

/// What kind of thing went wrong. The UI picks icons and wording from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Network,
    Auth,
    Integrity,
    Disk,
    Java,
    Loader,
    Pack,
    Game,
}

/// A failure a player can read: a plain message, an optional fix, and technical detail behind a toggle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct LauncherError {
    pub category: ErrorCategory,
    pub message: String,
    pub fix: Option<String>,
    pub detail: Option<String>,
}

pub type Result<T> = std::result::Result<T, LauncherError>;

impl LauncherError {
    pub fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builders_fill_fix_and_detail() {
        let e = LauncherError::network("Couldn't reach Mojang")
            .with_fix("Check your connection")
            .with_detail("dns error");
        assert_eq!(e.category, ErrorCategory::Network);
        assert_eq!(e.to_string(), "Couldn't reach Mojang");
        assert_eq!(e.fix.as_deref(), Some("Check your connection"));
        assert_eq!(e.detail.as_deref(), Some("dns error"));
    }

    #[test]
    fn io_errors_become_disk_errors() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let e: LauncherError = io.into();
        assert_eq!(e.category, ErrorCategory::Disk);
        assert!(e.fix.is_some());
        assert!(e.detail.unwrap().contains("denied"));
    }

    #[test]
    fn category_serializes_snake_case() {
        let json = serde_json::to_string(&LauncherError::integrity("bad")).unwrap();
        assert!(json.contains("\"category\":\"integrity\""));
    }
}
```

`crates/launcher-core/src/platform.rs` (tests):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    const WIN: Platform = Platform { os: Os::Windows, arch: Arch::X64 };
    const LINUX: Platform = Platform { os: Os::Linux, arch: Arch::X64 };

    #[test]
    fn mojang_names_and_separators() {
        assert_eq!(WIN.mojang_os_name(), "windows");
        assert_eq!(LINUX.mojang_os_name(), "linux");
        assert_eq!(Platform { os: Os::MacOs, arch: Arch::Arm64 }.mojang_os_name(), "osx");
        assert_eq!(WIN.classpath_separator(), ";");
        assert_eq!(LINUX.classpath_separator(), ":");
    }

    #[test]
    fn java_runtime_keys_match_mojang_index() {
        assert_eq!(WIN.java_runtime_key(), Some("windows-x64"));
        assert_eq!(LINUX.java_runtime_key(), Some("linux"));
        assert_eq!(Platform { os: Os::Linux, arch: Arch::Arm64 }.java_runtime_key(), None);
    }
}
```

`crates/launcher-core/src/paths.rs` (tests):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_under_root() {
        let d = LauncherDirs::new("/data/PerlandLauncher");
        assert_eq!(d.client_jar("1.20.1"), PathBuf::from("/data/PerlandLauncher/versions/1.20.1/1.20.1.jar"));
        assert_eq!(d.version_json("fabric-loader-0.16.14-1.20.1"), PathBuf::from("/data/PerlandLauncher/versions/fabric-loader-0.16.14-1.20.1/fabric-loader-0.16.14-1.20.1.json"));
        assert_eq!(d.natives_dir("1.12.2"), PathBuf::from("/data/PerlandLauncher/versions/1.12.2/natives"));
        assert_eq!(d.log_config("client-1.12.xml"), PathBuf::from("/data/PerlandLauncher/assets/log_configs/client-1.12.xml"));
        assert!(d.instances().starts_with(d.root()));
    }
}
```

`crates/launcher-core/src/events.rs` (tests):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn sink_receives_events_and_serializes_tagged() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        let sink: EventSink = Arc::new(move |e| s2.lock().unwrap().push(e));
        sink(Event::Progress { task: "t".into(), done: 1, total: 2, bytes: 3 });
        assert_eq!(seen.lock().unwrap().len(), 1);
        let json = serde_json::to_string(&seen.lock().unwrap()[0]).unwrap();
        assert!(json.starts_with("{\"type\":\"progress\""));
        noop_sink()(Event::Log { line: "x".into() });
    }
}
```

- [ ] **Step 5: Run the tests to verify they fail**

Run: `cargo test -p launcher-core`
Expected: compile errors or `todo!()` panics. `network`, `with_fix`, `Platform` and the other names don't exist yet.

- [ ] **Step 6: Implement the primitives**

Complete `error.rs`, replacing the stubbed `impl`:
```rust
impl LauncherError {
    pub fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        Self { category, message: message.into(), fix: None, detail: None }
    }
    pub fn network(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Network, message)
    }
    pub fn auth(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Auth, message)
    }
    pub fn integrity(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Integrity, message)
    }
    pub fn disk(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Disk, message)
    }
    pub fn java(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Java, message)
    }
    pub fn loader(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Loader, message)
    }
    pub fn pack(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Pack, message)
    }
    pub fn game(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Game, message)
    }
    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }
    pub fn with_detail(mut self, detail: impl Display) -> Self {
        self.detail = Some(detail.to_string());
        self
    }
}

impl From<std::io::Error> for LauncherError {
    fn from(e: std::io::Error) -> Self {
        LauncherError::disk("Couldn't read or write a launcher file")
            .with_fix("Check free disk space and that the launcher folder isn't read-only")
            .with_detail(e)
    }
}
```

`events.rs` (above its tests):
```rust
use std::sync::Arc;

use serde::Serialize;

/// Everything the core reports while it works. The UI only renders these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Progress { task: String, done: u64, total: u64, bytes: u64 },
    Log { line: String },
    StateChanged { instance: String, state: String },
}

pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

pub fn noop_sink() -> EventSink {
    Arc::new(|_| {})
}
```

`platform.rs` (above its tests):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Linux,
    MacOs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X64,
    X86,
    Arm64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

impl Platform {
    pub fn current() -> Self {
        let os = if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        };
        let arch = if cfg!(target_arch = "x86_64") {
            Arch::X64
        } else if cfg!(target_arch = "aarch64") {
            Arch::Arm64
        } else {
            Arch::X86
        };
        Self { os, arch }
    }

    /// The `os.name` value used in Mojang version JSON rules.
    pub fn mojang_os_name(&self) -> &'static str {
        match self.os {
            Os::Windows => "windows",
            Os::Linux => "linux",
            Os::MacOs => "osx",
        }
    }

    pub fn classpath_separator(&self) -> &'static str {
        match self.os {
            Os::Windows => ";",
            _ => ":",
        }
    }

    /// Key into Mojang's java-runtime `all.json`.
    pub fn java_runtime_key(&self) -> Option<&'static str> {
        match (self.os, self.arch) {
            (Os::Windows, Arch::X64) => Some("windows-x64"),
            (Os::Windows, Arch::X86) => Some("windows-x86"),
            (Os::Windows, Arch::Arm64) => Some("windows-arm64"),
            (Os::Linux, Arch::X64) => Some("linux"),
            (Os::Linux, Arch::X86) => Some("linux-i386"),
            (Os::MacOs, Arch::X64) => Some("mac-os"),
            (Os::MacOs, Arch::Arm64) => Some("mac-os-arm64"),
            _ => None,
        }
    }
}
```

`paths.rs` (above its tests):
```rust
use std::path::{Path, PathBuf};

/// The launcher's data folder and the shared game-file folders inside it.
#[derive(Debug, Clone)]
pub struct LauncherDirs {
    root: PathBuf,
}

impl LauncherDirs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `%APPDATA%\PerlandLauncher` on Windows; `$XDG_DATA_HOME/PerlandLauncher` or `~/.local/share/PerlandLauncher` elsewhere.
    pub fn default_root() -> Option<PathBuf> {
        if cfg!(windows) {
            std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("PerlandLauncher"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
                .map(|p| p.join("PerlandLauncher"))
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn libraries(&self) -> PathBuf {
        self.root.join("libraries")
    }
    pub fn assets(&self) -> PathBuf {
        self.root.join("assets")
    }
    pub fn versions(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn java(&self) -> PathBuf {
        self.root.join("java")
    }
    pub fn instances(&self) -> PathBuf {
        self.root.join("instances")
    }
    pub fn meta(&self) -> PathBuf {
        self.root.join("meta")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions().join(id)
    }
    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }
    pub fn client_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }
    pub fn natives_dir(&self, id: &str) -> PathBuf {
        self.version_dir(id).join("natives")
    }
    pub fn log_config(&self, file_id: &str) -> PathBuf {
        self.assets().join("log_configs").join(file_id)
    }
}
```

- [ ] **Step 7: Run tests, format and lint**

Run: `cargo fmt --all; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace`
Expected: no clippy warnings, and 7 tests pass.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "chore: scaffold workspace, repo docs, CI and core primitives" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Hashing (sha1/sha256/sha512 + CurseForge murmur2)

**Files:**
- Create: `crates/launcher-core/src/download/mod.rs` (only `pub mod hash; pub use hash::Hashes;` for now), `crates/launcher-core/src/download/hash.rs`
- Modify: `crates/launcher-core/src/lib.rs` (add `pub mod download;`)

**Interfaces:**
- Produces:
  - `download::hash::{sha1_hex, sha256_hex, sha512_hex, curseforge_murmur2}`
  - `download::Hashes { sha1, sha256, sha512: Option<String>, murmur2: Option<u32> }` with `Hashes::sha1(impl Into<String>)`, `is_empty()` and `verify(&[u8]) -> bool`. Every hash that is present must match, comparing case-insensitively.

- [ ] **Step 1: Write the failing tests** (vectors computed independently with Node's crypto and a reference murmur2):

`crates/launcher-core/src/download/hash.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha_vectors() {
        assert_eq!(sha1_hex(b"hello"), "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d");
        assert_eq!(sha256_hex(b"hello"), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        assert_eq!(
            sha512_hex(b"hello"),
            "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043"
        );
    }

    #[test]
    fn murmur2_matches_curseforge_fingerprints() {
        assert_eq!(curseforge_murmur2(b"hello world"), 2824650221);
        // CurseForge strips tab, LF, CR and space before hashing.
        assert_eq!(curseforge_murmur2(b"helloworld"), 2824650221);
        assert_eq!(curseforge_murmur2(b"a\tb\r\nc d"), curseforge_murmur2(b"abcd"));
        assert_eq!(curseforge_murmur2(b"a\tb\r\nc d"), 3376380438);
        assert_eq!(curseforge_murmur2(b""), 1540447798);
    }

    #[test]
    fn verify_requires_every_present_hash() {
        let mut h = Hashes::sha1("AAF4C61DDCC5E8A2DABEDE0F3B482CD9AEA9434D");
        assert!(h.verify(b"hello"), "comparison is case-insensitive");
        assert!(!h.verify(b"jello"));
        h.sha256 = Some("00".into());
        assert!(!h.verify(b"hello"), "a wrong sha256 fails even when sha1 matches");
        assert!(Hashes::default().is_empty());
        assert!(Hashes::default().verify(b"anything"));
        let cf = Hashes { murmur2: Some(2824650221), ..Default::default() };
        assert!(cf.verify(b"hello world"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p launcher-core hash`
Expected: compile errors (`sha1_hex` and `Hashes` are not defined).

- [ ] **Step 3: Implement** (above the tests in `hash.rs`):
```rust
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use sha2::{Sha256, Sha512};

/// The checksums a source publishes for one file. Any subset may be present.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hashes {
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    pub sha512: Option<String>,
    pub murmur2: Option<u32>,
}

impl Hashes {
    pub fn sha1(hex: impl Into<String>) -> Self {
        Self { sha1: Some(hex.into()), ..Default::default() }
    }

    pub fn is_empty(&self) -> bool {
        self.sha1.is_none() && self.sha256.is_none() && self.sha512.is_none() && self.murmur2.is_none()
    }

    /// True when every hash that is present matches `data`.
    pub fn verify(&self, data: &[u8]) -> bool {
        let checks: [(&Option<String>, fn(&[u8]) -> String); 3] =
            [(&self.sha1, sha1_hex), (&self.sha256, sha256_hex), (&self.sha512, sha512_hex)];
        for (expected, f) in checks {
            if let Some(expected) = expected {
                if !expected.eq_ignore_ascii_case(&f(data)) {
                    return false;
                }
            }
        }
        match self.murmur2 {
            Some(m) => m == curseforge_murmur2(data),
            None => true,
        }
    }
}

pub fn sha1_hex(data: &[u8]) -> String {
    hex::encode(Sha1::digest(data))
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

pub fn sha512_hex(data: &[u8]) -> String {
    hex::encode(Sha512::digest(data))
}

/// CurseForge's file fingerprint: MurmurHash2 (seed 1) over the bytes with tab/LF/CR/space removed.
pub fn curseforge_murmur2(data: &[u8]) -> u32 {
    const M: u32 = 0x5bd1_e995;
    let bytes: Vec<u8> = data.iter().copied().filter(|b| !matches!(b, 9 | 10 | 13 | 32)).collect();
    let mut h: u32 = 1 ^ (bytes.len() as u32);
    let mut chunks = bytes.chunks_exact(4);
    for c in &mut chunks {
        let mut k = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        k = k.wrapping_mul(M);
        k ^= k >> 24;
        k = k.wrapping_mul(M);
        h = h.wrapping_mul(M) ^ k;
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        if rem.len() == 3 {
            h ^= (rem[2] as u32) << 16;
        }
        if rem.len() >= 2 {
            h ^= (rem[1] as u32) << 8;
        }
        h ^= rem[0] as u32;
        h = h.wrapping_mul(M);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(M);
    h ^= h >> 15;
    h
}
```

`crates/launcher-core/src/download/mod.rs`:
```rust
pub mod hash;

pub use hash::Hashes;
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core hash`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(core): sha1/sha256/sha512 and CurseForge murmur2 hashing" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Downloader (retries, fallback URLs, resume, verification, concurrency)

**Files:**
- Modify: `crates/launcher-core/src/download/mod.rs`
- Test: `crates/launcher-core/tests/download.rs`

**Interfaces:**
- Consumes: `Hashes` (Task 2), `LauncherError`, `Event` and `EventSink` (Task 1).
- Produces:
  - `download::DownloadRequest { urls: Vec<String>, dest: PathBuf, hashes: Hashes, size: Option<u64> }`, with `DownloadRequest::new(url: impl Into<String>, dest: impl Into<PathBuf>, hashes: Hashes, size: Option<u64>)`.
  - `download::ExistingCheck::{Size, Hash}`.
  - `download::Downloader` (`Clone`), with `Downloader::new(reqwest::Client)`, `.with_backoff(Duration)`, `.with_concurrency(usize)` and `.with_existing_check(ExistingCheck)`, plus the methods `async fetch(&self, &DownloadRequest) -> Result<()>` and `async fetch_all(&self, Vec<DownloadRequest>, task: &str, sink: &EventSink) -> Result<()>`.

Behaviour:
- **An existing destination is skipped when valid.** With `ExistingCheck::Size` (the default), valid means the size matches. With `Hash`, or when no size is known, valid means the hashes match.
- **Download path:** the file downloads to `<dest>.part`, resuming with `Range` when a part file exists. A `206` response appends; a `200` response restarts.
- **Retry rules:** a `429`, a `5xx` or a dropped connection is retried with exponential backoff, up to 3 times. Any other `4xx` moves on to the next URL.
- **Verification:** after the download the hashes are checked, then `.part` is renamed onto `dest`. On a mismatch, the part is deleted and the whole thing is tried once more. A second mismatch is an `Integrity` error.
- **`fetch_all`:** drops duplicate destinations, runs up to 8 downloads at once, emits `Progress` at the start and after each file, and returns the first error.

- [ ] **Step 1: Write the failing tests**

`crates/launcher-core/tests/download.rs`:
```rust
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use launcher_core::{
    download::{DownloadRequest, Downloader, ExistingCheck, Hashes},
    events::{Event, EventSink},
    ErrorCategory,
};
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

const HELLO_SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";
const HELLO_WORLD_SHA1: &str = "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed";

fn downloader() -> Downloader {
    Downloader::new(reqwest::Client::new()).with_backoff(Duration::from_millis(1))
}

fn part_of(dest: &Path) -> PathBuf {
    PathBuf::from(format!("{}.part", dest.display()))
}

async fn serve(server: &MockServer, at: &str, status: u16, body: &str, times: u64) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .expect(times)
        .mount(server)
        .await;
}

#[tokio::test]
async fn downloads_and_verifies() {
    let server = MockServer::start().await;
    serve(&server, "/hello.txt", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("a/b/hello.txt");
    let req = DownloadRequest::new(format!("{}/hello.txt", server.uri()), dest.clone(), Hashes::sha1(HELLO_SHA1), Some(5));
    downloader().fetch(&req).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello");
    assert!(!part_of(&dest).exists());
}

#[tokio::test]
async fn skips_existing_file_with_matching_size() {
    let server = MockServer::start().await;
    serve(&server, "/hello.txt", 200, "hello", 0).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    std::fs::write(&dest, "hello").unwrap();
    let req = DownloadRequest::new(format!("{}/hello.txt", server.uri()), dest, Hashes::sha1(HELLO_SHA1), Some(5));
    downloader().fetch(&req).await.unwrap();
}

#[tokio::test]
async fn hash_mode_redownloads_same_size_wrong_content() {
    let server = MockServer::start().await;
    serve(&server, "/hello.txt", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    std::fs::write(&dest, "jello").unwrap();
    let req = DownloadRequest::new(format!("{}/hello.txt", server.uri()), dest.clone(), Hashes::sha1(HELLO_SHA1), Some(5));
    downloader().with_existing_check(ExistingCheck::Hash).fetch(&req).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello");
}

#[tokio::test]
async fn falls_back_to_next_url_on_404() {
    let server = MockServer::start().await;
    serve(&server, "/missing", 404, "", 1).await;
    serve(&server, "/hello.txt", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    let mut req = DownloadRequest::new(format!("{}/missing", server.uri()), dest.clone(), Hashes::sha1(HELLO_SHA1), None);
    req.urls.push(format!("{}/hello.txt", server.uri()));
    downloader().fetch(&req).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello");
}

#[tokio::test]
async fn retries_server_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    serve(&server, "/flaky", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    let req = DownloadRequest::new(format!("{}/flaky", server.uri()), dest.clone(), Hashes::sha1(HELLO_SHA1), None);
    downloader().fetch(&req).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello");
}

#[tokio::test]
async fn persistent_hash_mismatch_is_integrity_error() {
    let server = MockServer::start().await;
    serve(&server, "/bad", 200, "jello", 2).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    let req = DownloadRequest::new(format!("{}/bad", server.uri()), dest.clone(), Hashes::sha1(HELLO_SHA1), None);
    let err = downloader().fetch(&req).await.unwrap_err();
    assert_eq!(err.category, ErrorCategory::Integrity);
    assert!(err.fix.is_some());
    assert!(!dest.exists());
    assert!(!part_of(&dest).exists());
}

#[tokio::test]
async fn resumes_partial_download_with_range() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hw.txt"))
        .and(header("range", "bytes=5-"))
        .respond_with(ResponseTemplate::new(206).set_body_string(" world"))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hw.txt");
    std::fs::write(part_of(&dest), "hello").unwrap();
    let req = DownloadRequest::new(format!("{}/hw.txt", server.uri()), dest.clone(), Hashes::sha1(HELLO_WORLD_SHA1), Some(11));
    downloader().fetch(&req).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello world");
}

#[tokio::test]
async fn stale_part_file_self_heals() {
    // A .part left over from an older version of this file: resuming it gives garbage, so it must be discarded.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hello.txt"))
        .and(header("range", "bytes=3-"))
        .respond_with(ResponseTemplate::new(206).set_body_string("lo"))
        .expect(1)
        .mount(&server)
        .await;
    serve(&server, "/hello.txt", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    std::fs::write(part_of(&dest), "OLD").unwrap();
    let req = DownloadRequest::new(format!("{}/hello.txt", server.uri()), dest.clone(), Hashes::sha1(HELLO_SHA1), None);
    downloader().fetch(&req).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello");
}

#[tokio::test]
async fn fetch_all_dedupes_destinations() {
    let server = MockServer::start().await;
    serve(&server, "/x", 200, "hello", 1).await;
    serve(&server, "/y", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    let reqs = vec![
        DownloadRequest::new(format!("{}/x", server.uri()), a.clone(), Hashes::sha1(HELLO_SHA1), Some(5)),
        DownloadRequest::new(format!("{}/x", server.uri()), a.clone(), Hashes::sha1(HELLO_SHA1), Some(5)),
        DownloadRequest::new(format!("{}/y", server.uri()), b.clone(), Hashes::sha1(HELLO_SHA1), Some(5)),
    ];
    let events = Arc::new(Mutex::new(Vec::new()));
    let e2 = events.clone();
    let sink: EventSink = Arc::new(move |e| e2.lock().unwrap().push(e));
    downloader().fetch_all(reqs, "Test", &sink).await.unwrap();
    assert!(a.exists() && b.exists());
    let last = events.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last, Event::Progress { task: "Test".into(), done: 2, total: 2, bytes: 10 });
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p launcher-core --test download`
Expected: compile errors (`DownloadRequest` and `Downloader` are not defined).

- [ ] **Step 3: Implement** by replacing `crates/launcher-core/src/download/mod.rs` with:
```rust
pub mod hash;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

pub use hash::Hashes;

use crate::{
    error::{LauncherError, Result},
    events::{Event, EventSink},
};

/// One file to put on disk, with every URL it can come from.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadRequest {
    pub urls: Vec<String>,
    pub dest: PathBuf,
    pub hashes: Hashes,
    pub size: Option<u64>,
}

impl DownloadRequest {
    pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>, hashes: Hashes, size: Option<u64>) -> Self {
        Self { urls: vec![url.into()], dest: dest.into(), hashes, size }
    }
}

/// How an already-present destination is judged. `Size` is fast; `Hash` reads the file (used by "Repair").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingCheck {
    Size,
    Hash,
}

#[derive(Clone)]
pub struct Downloader {
    client: reqwest::Client,
    max_concurrent: usize,
    retries: u32,
    backoff: Duration,
    existing: ExistingCheck,
}

enum Attempt {
    Retry(LauncherError),
    NextUrl(LauncherError),
    Fatal(LauncherError),
}

impl Downloader {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client, max_concurrent: 8, retries: 3, backoff: Duration::from_millis(500), existing: ExistingCheck::Size }
    }

    pub fn with_backoff(mut self, backoff: Duration) -> Self {
        self.backoff = backoff;
        self
    }

    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.max_concurrent = n.max(1);
        self
    }

    pub fn with_existing_check(mut self, check: ExistingCheck) -> Self {
        self.existing = check;
        self
    }

    async fn already_valid(&self, req: &DownloadRequest) -> bool {
        let Ok(meta) = tokio::fs::metadata(&req.dest).await else { return false };
        if !meta.is_file() {
            return false;
        }
        match (self.existing, req.size) {
            (ExistingCheck::Size, Some(size)) => meta.len() == size,
            _ => {
                if req.hashes.is_empty() {
                    return true;
                }
                match tokio::fs::read(&req.dest).await {
                    Ok(data) => req.hashes.verify(&data),
                    Err(_) => false,
                }
            }
        }
    }

    pub async fn fetch(&self, req: &DownloadRequest) -> Result<()> {
        if self.already_valid(req).await {
            return Ok(());
        }
        if let Some(parent) = req.dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let part = part_path(&req.dest);
        let mut last_err = LauncherError::network("No download address was given for this file");
        for _round in 0..2 {
            let mut downloaded = false;
            'urls: for url in &req.urls {
                for attempt in 0..=self.retries {
                    match self.download_to_part(url, &part).await {
                        Ok(()) => {
                            downloaded = true;
                            break 'urls;
                        }
                        Err(Attempt::Retry(e)) => {
                            last_err = e;
                            if attempt < self.retries {
                                tokio::time::sleep(self.backoff * 2u32.pow(attempt)).await;
                            }
                        }
                        Err(Attempt::NextUrl(e)) => {
                            last_err = e;
                            continue 'urls;
                        }
                        Err(Attempt::Fatal(e)) => return Err(e),
                    }
                }
            }
            if !downloaded {
                return Err(last_err);
            }
            let data = tokio::fs::read(&part).await?;
            if req.hashes.verify(&data) {
                tokio::fs::rename(&part, &req.dest).await?;
                return Ok(());
            }
            let _ = tokio::fs::remove_file(&part).await;
            last_err = LauncherError::integrity(format!("{} was damaged while downloading", file_name(&req.dest)))
                .with_fix("Try again. If it keeps happening, the file on the server may be broken.");
        }
        Err(last_err)
    }

    async fn download_to_part(&self, url: &str, part: &Path) -> std::result::Result<(), Attempt> {
        let existing = tokio::fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
        let mut request = self.client.get(url);
        if existing > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
        }
        let response = request.send().await.map_err(|e| {
            Attempt::Retry(
                LauncherError::network(format!("Couldn't reach {}", host_of(url)))
                    .with_fix("Check your internet connection")
                    .with_detail(e),
            )
        })?;
        let status = response.status().as_u16();
        let append = match status {
            206 => true,
            200 => false,
            416 => {
                let _ = tokio::fs::remove_file(part).await;
                return Err(Attempt::Retry(LauncherError::network("The download had to restart")));
            }
            429 | 500..=599 => {
                return Err(Attempt::Retry(LauncherError::network(format!("{} is having trouble ({status})", host_of(url)))))
            }
            _ => return Err(Attempt::NextUrl(LauncherError::network(format!("{url} answered {status}")))),
        };
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(part)
            .await
            .map_err(|e| Attempt::Fatal(e.into()))?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                Attempt::Retry(LauncherError::network("The connection dropped during a download").with_detail(e))
            })?;
            file.write_all(&chunk).await.map_err(|e| Attempt::Fatal(e.into()))?;
        }
        file.flush().await.map_err(|e| Attempt::Fatal(e.into()))?;
        Ok(())
    }

    /// Download everything, at most `max_concurrent` at a time. Duplicate destinations are fetched once.
    pub async fn fetch_all(&self, reqs: Vec<DownloadRequest>, task: &str, sink: &EventSink) -> Result<()> {
        let mut seen = HashSet::new();
        let reqs: Vec<DownloadRequest> = reqs.into_iter().filter(|r| seen.insert(r.dest.clone())).collect();
        let total = reqs.len() as u64;
        let (mut done, mut bytes) = (0u64, 0u64);
        sink(Event::Progress { task: task.to_string(), done, total, bytes });
        let mut results = futures_util::stream::iter(
            reqs.into_iter().map(|r| async move { self.fetch(&r).await.map(|()| r.size.unwrap_or(0)) }),
        )
        .buffer_unordered(self.max_concurrent);
        while let Some(result) = results.next().await {
            bytes += result?;
            done += 1;
            sink(Event::Progress { task: task.to_string(), done, total, bytes });
        }
        Ok(())
    }
}

fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

fn host_of(url: &str) -> &str {
    url.split("://").nth(1).and_then(|rest| rest.split('/').next()).unwrap_or(url)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --test download`
Expected: 9 passed.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(core): resumable, verified, concurrent downloader" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Metadata client with ETag cache and offline fallback

**Files:**
- Create: `crates/launcher-core/src/fsutil.rs`, `crates/launcher-core/src/meta/mod.rs`
- Modify: `crates/launcher-core/src/lib.rs` (add `pub mod fsutil; pub mod meta;`)
- Test: `crates/launcher-core/tests/meta.rs`

**Interfaces:**
- Consumes: `sha1_hex` (Task 2) and `LauncherError` (Task 1).
- Produces:
  - `fsutil::write_atomic(path: &Path, data: &[u8]) -> Result<()>` (async). It writes `<path>.tmp` and then renames it over `path`.
  - `meta::MetaClient` (`Clone`) with `MetaClient::new(reqwest::Client, dir: impl Into<PathBuf>)`, `async get_bytes(&self, url) -> Result<Vec<u8>>`, `async get_json<T: DeserializeOwned>(&self, url) -> Result<T>` and `async get_verified_json<T>(&self, url, sha1: &str) -> Result<T>`.
  - `meta::parse_json<T>(url: &str, bytes: &[u8]) -> Result<T>` (`pub`), which turns a parse failure into a `Loader` error.

Behaviour of `get_bytes`:
- Cached bodies and ETags are stored as `<dir>/<sha1(url)>.body` and `.etag`.
- A request is sent with `If-None-Match` when an ETag is cached. A `304` returns the cached body, and a `200` stores the new body and ETag.
- Any network error or non-success status falls back to the cached body if there is one. Otherwise it returns a `Network` error with a fix.

Behaviour of `get_verified_json`: a cached body whose sha1 matches is used **without any network access**, because Mojang package URLs are content-addressed.

- [ ] **Step 1: Write the failing tests**

`crates/launcher-core/tests/meta.rs`:
```rust
use std::time::Duration;

use launcher_core::{download::hash::sha1_hex, meta::MetaClient, ErrorCategory};
use serde::Deserialize;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[derive(Debug, Deserialize, PartialEq)]
struct A {
    a: u32,
}

const BODY: &str = "{\"a\":1}";

fn http() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap()
}

async fn serve_with_etag(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/m.json"))
        .respond_with(ResponseTemplate::new(200).insert_header("ETag", "\"v1\"").set_body_string(BODY))
        .up_to_n_times(1)
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn caches_and_revalidates_with_etag() {
    let server = MockServer::start().await;
    serve_with_etag(&server).await;
    Mock::given(method("GET"))
        .and(path("/m.json"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let url = format!("{}/m.json", server.uri());
    assert_eq!(meta.get_json::<A>(&url).await.unwrap(), A { a: 1 });
    assert_eq!(meta.get_json::<A>(&url).await.unwrap(), A { a: 1 });
}

#[tokio::test]
async fn offline_uses_cached_copy() {
    let server = MockServer::start().await;
    serve_with_etag(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let url = format!("{}/m.json", server.uri());
    meta.get_bytes(&url).await.unwrap();
    drop(server);
    assert_eq!(meta.get_json::<A>(&url).await.unwrap(), A { a: 1 });
}

#[tokio::test]
async fn offline_without_cache_is_network_error() {
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let err = meta.get_bytes("http://127.0.0.1:9/nothing.json").await.unwrap_err();
    assert_eq!(err.category, ErrorCategory::Network);
    assert!(err.fix.is_some());
}

#[tokio::test]
async fn verified_cache_hit_needs_no_network() {
    let server = MockServer::start().await;
    serve_with_etag(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let url = format!("{}/m.json", server.uri());
    let sha = sha1_hex(BODY.as_bytes());
    assert_eq!(meta.get_verified_json::<A>(&url, &sha).await.unwrap(), A { a: 1 });
    drop(server);
    assert_eq!(meta.get_verified_json::<A>(&url, &sha).await.unwrap(), A { a: 1 });
}

#[tokio::test]
async fn verified_mismatch_is_integrity_error() {
    let server = MockServer::start().await;
    serve_with_etag(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let err = meta
        .get_verified_json::<A>(&format!("{}/m.json", server.uri()), "0000000000000000000000000000000000000000")
        .await
        .unwrap_err();
    assert_eq!(err.category, ErrorCategory::Integrity);
}

#[tokio::test]
async fn bad_json_is_loader_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/bad.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let err = meta.get_json::<A>(&format!("{}/bad.json", server.uri())).await.unwrap_err();
    assert_eq!(err.category, ErrorCategory::Loader);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p launcher-core --test meta`
Expected: compile error (`meta` module not found).

- [ ] **Step 3: Implement**

`crates/launcher-core/src/fsutil.rs`:
```rust
use std::path::{Path, PathBuf};

use crate::error::Result;

/// Write a file so readers never see a half-written version: write `<path>.tmp`, then rename over `path`.
pub async fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    tokio::fs::write(&tmp, data).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}
```

`crates/launcher-core/src/meta/mod.rs`:
```rust
use std::path::PathBuf;

use reqwest::header::{ETAG, IF_NONE_MATCH};
use serde::de::DeserializeOwned;

use crate::{
    download::hash::sha1_hex,
    error::{LauncherError, Result},
    fsutil::write_atomic,
};

/// Fetches upstream metadata and keeps the last good copy on disk, so the launcher works offline.
#[derive(Clone)]
pub struct MetaClient {
    http: reqwest::Client,
    dir: PathBuf,
}

impl MetaClient {
    pub fn new(http: reqwest::Client, dir: impl Into<PathBuf>) -> Self {
        Self { http, dir: dir.into() }
    }

    fn body_path(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{}.body", sha1_hex(url.as_bytes())))
    }

    fn etag_path(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{}.etag", sha1_hex(url.as_bytes())))
    }

    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let body_path = self.body_path(url);
        let cached = tokio::fs::read(&body_path).await.ok();
        let mut request = self.http.get(url);
        if cached.is_some() {
            if let Ok(etag) = tokio::fs::read_to_string(self.etag_path(url)).await {
                request = request.header(IF_NONE_MATCH, etag.trim());
            }
        }
        match request.send().await {
            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_MODIFIED => {
                cached.ok_or_else(|| LauncherError::network(format!("{url} said nothing changed, but there's no saved copy")))
            }
            Ok(resp) if resp.status().is_success() => {
                let etag = resp.headers().get(ETAG).and_then(|v| v.to_str().ok()).map(str::to_owned);
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| LauncherError::network(format!("Downloading {url} was interrupted")).with_detail(e))?;
                write_atomic(&body_path, &bytes).await?;
                match etag {
                    Some(tag) => write_atomic(&self.etag_path(url), tag.as_bytes()).await?,
                    None => {
                        let _ = tokio::fs::remove_file(self.etag_path(url)).await;
                    }
                }
                Ok(bytes.to_vec())
            }
            Ok(resp) => match cached {
                Some(c) => Ok(c),
                None => Err(LauncherError::network(format!("{url} answered {}", resp.status()))
                    .with_fix("Try again in a few minutes")),
            },
            Err(e) => match cached {
                Some(c) => Ok(c),
                None => Err(LauncherError::network("You seem to be offline, and this hasn't been downloaded before")
                    .with_fix("Connect to the internet and try again")
                    .with_detail(e)),
            },
        }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let bytes = self.get_bytes(url).await?;
        parse_json(url, &bytes)
    }

    /// For content-addressed URLs (Mojang packages): a cached copy with the right sha1 is used without any network access.
    pub async fn get_verified_json<T: DeserializeOwned>(&self, url: &str, sha1: &str) -> Result<T> {
        if let Ok(cached) = tokio::fs::read(self.body_path(url)).await {
            if sha1_hex(&cached).eq_ignore_ascii_case(sha1) {
                return parse_json(url, &cached);
            }
        }
        let bytes = self.get_bytes(url).await?;
        if !sha1_hex(&bytes).eq_ignore_ascii_case(sha1) {
            return Err(LauncherError::integrity(format!("Data from {url} didn't match its checksum"))
                .with_fix("Try again in a minute"));
        }
        parse_json(url, &bytes)
    }
}

pub fn parse_json<T: DeserializeOwned>(url: &str, bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes)
        .map_err(|e| LauncherError::loader(format!("Unexpected data from {url}")).with_detail(e))
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --test meta`
Expected: 6 passed.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(core): metadata client with ETag cache and offline fallback" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Mojang version model and fixtures

**Files:**
- Create: `crates/launcher-core/src/meta/mojang.rs`, `crates/launcher-core/src/test_support.rs`
- Create: the fixtures `crates/launcher-core/tests/fixtures/{1.20.1.json,1.12.2.json,asset-index-5.json}`
- Modify: `crates/launcher-core/src/meta/mod.rs` (add `pub mod mojang;` at the top) and `crates/launcher-core/src/lib.rs` (add `#[cfg(test)] pub(crate) mod test_support;`)

**Interfaces:**
- Produces, in `meta::mojang`:
  - `VERSION_MANIFEST_URL` and `RESOURCES_URL` (`https://resources.download.minecraft.net`).
  - `VersionManifest { latest: Latest, versions: Vec<ManifestEntry> }` with `.find(id) -> Option<&ManifestEntry>`, where `ManifestEntry { id, kind (json "type"), url, sha1 }`.
  - `VersionJson { id, inherits_from: Option<String>, main_class, arguments: Option<Arguments>, minecraft_arguments: Option<String>, asset_index: Option<AssetIndexRef>, assets: Option<String>, downloads: Option<VersionDownloads>, java_version: Option<JavaVersionReq>, libraries: Vec<Library>, logging: Option<Logging>, kind: Option<String> }` (Serialize + Deserialize, camelCase).
  - `Arguments { game: Vec<ArgValue>, jvm: Vec<ArgValue> }`, and `ArgValue::{Plain(String), Conditional { rules: Vec<Rule>, value: OneOrMany }}` (untagged).
  - `OneOrMany::{One(String), Many(Vec<String>)}` with `.to_vec()`.
  - `Rule { action: RuleAction, os: Option<OsRule>, features: Option<HashMap<String, bool>> }`, `RuleAction::{Allow, Disallow}` and `OsRule { name, arch, version: Option<String> }`.
  - `Library { name, downloads: Option<LibraryDownloads>, url: Option<String>, sha1/sha256/sha512: Option<String>, size: Option<u64>, rules: Option<Vec<Rule>>, natives: Option<HashMap<String,String>>, extract: Option<Extract> }`, `LibraryDownloads { artifact: Option<Artifact>, classifiers: Option<HashMap<String, Artifact>> }` and `Artifact { path: Option<String>, sha1, size: u64, url }`.
  - `VersionDownloads { client: Artifact }`, `AssetIndexRef { id, sha1, size, url, total_size: Option<u64> }`, `JavaVersionReq { component, major_version: u32 }`, `Logging { client: Option<LoggingClient> }`, `LoggingClient { argument, file: LoggingFile, kind: Option<String> }`, `LoggingFile { id, sha1, size, url }`, `Extract { exclude: Vec<String> }`.
  - `AssetIndex { objects: HashMap<String, AssetObject> }` and `AssetObject { hash, size }`.
- Produces, in `test_support` (tests only): `fixture(name) -> String`, `version(name) -> VersionJson`, `WINDOWS_X64` and `LINUX_X64`.

- [ ] **Step 1: Record the fixtures**

```bash
mkdir -p crates/launcher-core/tests/fixtures && cd crates/launcher-core/tests/fixtures
curl -sSfo 1.20.1.json https://piston-meta.mojang.com/v1/packages/760a390e336b2b6202ec5906757cee3a929a7c85/1.20.1.json
curl -sSfo 1.12.2.json https://piston-meta.mojang.com/v1/packages/832d95b9f40699d4961394dcf6cf549e65f15dc5/1.12.2.json
curl -sSfo asset-index-5.json https://piston-meta.mojang.com/v1/packages/c8035a2d33a185b89644c37f1fb98e5d0120273d/5.json
cd -
```
Expected: three files of roughly 35 KB, 18 KB and 413 KB.

- [ ] **Step 2: Write `test_support` and the failing tests**

`crates/launcher-core/src/test_support.rs`:
```rust
//! Helpers shared by unit tests.
#![allow(dead_code)]

use std::path::Path;

use crate::{
    meta::mojang::VersionJson,
    platform::{Arch, Os, Platform},
};

pub const WINDOWS_X64: Platform = Platform { os: Os::Windows, arch: Arch::X64 };
pub const LINUX_X64: Platform = Platform { os: Os::Linux, arch: Arch::X64 };

pub fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()))
}

pub fn version(name: &str) -> VersionJson {
    serde_json::from_str(&fixture(name)).unwrap()
}
```

The tests at the bottom of `crates/launcher-core/src/meta/mojang.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture, version};

    #[test]
    fn parses_modern_version() {
        let v = version("1.20.1.json");
        assert_eq!(v.id, "1.20.1");
        assert_eq!(v.main_class, "net.minecraft.client.main.Main");
        let java = v.java_version.as_ref().unwrap();
        assert_eq!((java.component.as_str(), java.major_version), ("java-runtime-gamma", 17));
        assert_eq!(v.asset_index.as_ref().unwrap().id, "5");
        assert_eq!(v.libraries.len(), 88);
        assert_eq!(v.downloads.as_ref().unwrap().client.sha1, "0c3ec587af28e5a785c0b4a7b8a30f9a8f78f838");
        let args = v.arguments.as_ref().unwrap();
        assert_eq!(args.game[0], ArgValue::Plain("--username".into()));
        assert!(args.jvm.iter().any(|a| matches!(a, ArgValue::Conditional { .. })));
        assert_eq!(v.logging.as_ref().unwrap().client.as_ref().unwrap().file.id, "client-1.12.xml");
    }

    #[test]
    fn parses_legacy_version() {
        let v = version("1.12.2.json");
        assert!(v.arguments.is_none());
        assert!(v.minecraft_arguments.as_ref().unwrap().starts_with("--username ${auth_player_name}"));
        assert_eq!(v.java_version.as_ref().unwrap().component, "jre-legacy");
        let lwjgl = v.libraries.iter().find(|l| l.name == "org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209").unwrap();
        assert_eq!(lwjgl.natives.as_ref().unwrap()["windows"], "natives-windows");
        assert!(lwjgl.downloads.as_ref().unwrap().classifiers.as_ref().unwrap().contains_key("natives-windows"));
    }

    #[test]
    fn round_trips_through_serde() {
        let v = version("1.20.1.json");
        let again: VersionJson = serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        assert_eq!(again.libraries.len(), v.libraries.len());
        assert_eq!(again.arguments, v.arguments);
    }

    #[test]
    fn manifest_finds_year_style_ids() {
        let json = r#"{"latest":{"release":"26.3","snapshot":"26.4-snapshot-3"},"versions":[
            {"id":"26.4-snapshot-3","type":"snapshot","url":"https://x/a.json","sha1":"aa"},
            {"id":"26.3","type":"release","url":"https://x/b.json","sha1":"bb"},
            {"id":"1.20.1","type":"release","url":"https://x/c.json","sha1":"cc"}]}"#;
        let m: VersionManifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.find("26.3").unwrap().url, "https://x/b.json");
        assert_eq!(m.find("26.4-snapshot-3").unwrap().kind, "snapshot");
        assert!(m.find("1.20").is_none(), "lookup is exact, never a prefix match");
    }

    #[test]
    fn parses_asset_index() {
        let idx: AssetIndex = serde_json::from_str(&fixture("asset-index-5.json")).unwrap();
        assert_eq!(idx.objects.len(), 3598);
        assert_eq!(idx.objects["icons/icon_128x128.png"].hash, "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356");
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p launcher-core mojang`
Expected: compile errors (the types are not defined).

- [ ] **Step 4: Implement the types** (above the tests in `mojang.rs`):
```rust
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const VERSION_MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
}

impl VersionManifest {
    /// Exact id lookup. Ids are opaque: "1.20.1", "26.3" and "26.4-snapshot-3" are all valid.
    pub fn find(&self, id: &str) -> Option<&ManifestEntry> {
        self.versions.iter().find(|v| v.id == id)
    }
}

/// A Mojang version JSON, or a loader profile that inherits from one (same shape).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    pub main_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_arguments: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<VersionDownloads>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaVersionReq>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<Logging>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<ArgValue>,
    #[serde(default)]
    pub jvm: Vec<ArgValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: OneOrMany },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s.clone()],
            OneOrMany::Many(v) => v.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    /// Maven repository base URL (Fabric/Quilt style libraries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha512: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<Rule>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<Extract>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionDownloads {
    pub client: Artifact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersionReq {
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Logging {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LoggingFile,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p launcher-core mojang`
Expected: 5 passed.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(core): Mojang version/asset model with recorded fixtures" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Rules, argument resolution, maven coordinates

**Files:**
- Create: `crates/launcher-core/src/meta/rules.rs`, `crates/launcher-core/src/meta/maven.rs`
- Modify: `crates/launcher-core/src/meta/mod.rs` (add `pub mod maven; pub mod rules;`)

**Interfaces:**
- Consumes: the `mojang` types (Task 5) and `Platform`/`Arch` (Task 1).
- Produces, in `rules`:
  - `RuleContext { platform: Platform, features: HashSet<String> }`, with `RuleContext::new(platform)` (no features) and `.with_feature(name)`.
  - `rules_allow(Option<&[Rule]>, &RuleContext) -> bool`.
  - `natives_arch_ok(library_name: &str, Arch) -> bool`.
  - `library_applies(&Library, &RuleContext) -> bool`.
  - `resolve_args(&[ArgValue], &RuleContext) -> Vec<String>`.
- Produces, in `maven`: `MavenCoord { group, artifact, version, classifier: Option<String>, extension }` with `MavenCoord::parse(&str) -> Option<Self>`, `.path() -> String` and `.identity() -> String` (`group:artifact[:classifier]`).

Rules semantics (Mojang's):
- No rules at all means allowed. Otherwise the result starts as "not allowed", and the **last** matching rule decides.
- A rule matches when its `os.name`, `os.arch` and every `features` entry all match.
- Mojang uses the arch names `x86`, `x86_64` and `arm64`.
- Libraries whose name has a `natives-*` classifier ending in `-x86` or `-arm64` are only for that arch. `-arm32` is never used. A plain `natives-<os>` classifier is for x64.

- [ ] **Step 1: Write the failing tests**

At the bottom of `rules.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{version, LINUX_X64, WINDOWS_X64};

    #[test]
    fn library_counts_match_mojang_rules() {
        let v = version("1.20.1.json");
        let win = RuleContext::new(WINDOWS_X64);
        let linux = RuleContext::new(LINUX_X64);
        assert_eq!(v.libraries.iter().filter(|l| library_applies(l, &win)).count(), 50);
        assert_eq!(v.libraries.iter().filter(|l| library_applies(l, &linux)).count(), 52);
    }

    #[test]
    fn native_classifier_arch_filter() {
        assert!(natives_arch_ok("org.lwjgl:lwjgl:3.3.1:natives-windows", Arch::X64));
        assert!(!natives_arch_ok("org.lwjgl:lwjgl:3.3.1:natives-windows-x86", Arch::X64));
        assert!(!natives_arch_ok("org.lwjgl:lwjgl:3.3.1:natives-windows-arm64", Arch::X64));
        assert!(natives_arch_ok("org.lwjgl:lwjgl:3.3.1:natives-windows-arm64", Arch::Arm64));
        assert!(!natives_arch_ok("org.lwjgl:lwjgl:3.3.1:natives-linux-arm32", Arch::X64));
        assert!(natives_arch_ok("org.lwjgl:lwjgl:3.3.1", Arch::X86));
    }

    #[test]
    fn jvm_args_follow_os_and_arch_rules() {
        let v = version("1.20.1.json");
        let jvm = resolve_args(&v.arguments.as_ref().unwrap().jvm, &RuleContext::new(WINDOWS_X64));
        assert!(jvm.iter().any(|a| a.starts_with("-XX:HeapDumpPath=")));
        assert!(!jvm.contains(&"-XstartOnFirstThread".to_string()));
        assert!(!jvm.contains(&"-Xss1M".to_string()));
        assert!(jvm.contains(&"${classpath}".to_string()));
    }

    #[test]
    fn game_args_follow_feature_rules() {
        let v = version("1.20.1.json");
        let game_args = &v.arguments.as_ref().unwrap().game;
        let plain = resolve_args(game_args, &RuleContext::new(WINDOWS_X64));
        assert!(!plain.contains(&"--demo".to_string()));
        assert!(!plain.contains(&"--width".to_string()));
        let sized = resolve_args(game_args, &RuleContext::new(WINDOWS_X64).with_feature("has_custom_resolution"));
        assert!(sized.contains(&"--width".to_string()));
    }

    #[test]
    fn last_matching_rule_wins() {
        let rules: Vec<Rule> = serde_json::from_str(
            r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#,
        )
        .unwrap();
        assert!(rules_allow(Some(rules.as_slice()), &RuleContext::new(WINDOWS_X64)));
        let mac = Platform { os: crate::platform::Os::MacOs, arch: Arch::Arm64 };
        assert!(!rules_allow(Some(rules.as_slice()), &RuleContext::new(mac)));
        assert!(rules_allow(None, &RuleContext::new(mac)));
    }
}
```

At the bottom of `maven.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_builds_paths() {
        let c = MavenCoord::parse("org.ow2.asm:asm:9.8").unwrap();
        assert_eq!(c.path(), "org/ow2/asm/asm/9.8/asm-9.8.jar");
        assert_eq!(c.identity(), "org.ow2.asm:asm");
        let n = MavenCoord::parse("org.lwjgl:lwjgl:3.3.1:natives-windows").unwrap();
        assert_eq!(n.path(), "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar");
        assert_eq!(n.identity(), "org.lwjgl:lwjgl:natives-windows");
        let z = MavenCoord::parse("de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip").unwrap();
        assert_eq!(z.path(), "de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip");
    }

    #[test]
    fn rejects_malformed() {
        assert!(MavenCoord::parse("just-a-name").is_none());
        assert!(MavenCoord::parse("a::1").is_none());
        assert!(MavenCoord::parse("a:b:c:d:e").is_none());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p launcher-core rules maven`
Expected: compile errors.

- [ ] **Step 3: Implement**

The top of `rules.rs`:
```rust
use std::collections::HashSet;

use super::mojang::{ArgValue, Library, Rule, RuleAction};
use crate::platform::{Arch, Platform};

/// What the rules are evaluated against: this computer, plus enabled launcher features.
#[derive(Debug, Clone)]
pub struct RuleContext {
    pub platform: Platform,
    pub features: HashSet<String>,
}

impl RuleContext {
    pub fn new(platform: Platform) -> Self {
        Self { platform, features: HashSet::new() }
    }

    pub fn with_feature(mut self, name: &str) -> Self {
        self.features.insert(name.to_string());
        self
    }
}

pub fn rules_allow(rules: Option<&[Rule]>, ctx: &RuleContext) -> bool {
    let Some(rules) = rules else { return true };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule, ctx) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

fn rule_matches(rule: &Rule, ctx: &RuleContext) -> bool {
    if let Some(os) = &rule.os {
        if os.name.as_deref().is_some_and(|n| n != ctx.platform.mojang_os_name()) {
            return false;
        }
        if os.arch.as_deref().is_some_and(|a| a != mojang_arch(ctx.platform.arch)) {
            return false;
        }
    }
    if let Some(features) = &rule.features {
        if features.iter().any(|(name, wanted)| ctx.features.contains(name) != *wanted) {
            return false;
        }
    }
    true
}

fn mojang_arch(arch: Arch) -> &'static str {
    match arch {
        Arch::X64 => "x86_64",
        Arch::X86 => "x86",
        Arch::Arm64 => "arm64",
    }
}

/// LWJGL 3 ships one library per arch (`natives-windows`, `-x86`, `-arm64`) under a single os rule.
pub fn natives_arch_ok(library_name: &str, arch: Arch) -> bool {
    let Some(classifier) = library_name.split(':').nth(3) else { return true };
    if !classifier.starts_with("natives-") {
        return true;
    }
    if classifier.ends_with("-x86") {
        arch == Arch::X86
    } else if classifier.ends_with("-arm64") || classifier.ends_with("-aarch_64") {
        arch == Arch::Arm64
    } else if classifier.ends_with("-arm32") {
        false
    } else {
        arch == Arch::X64
    }
}

pub fn library_applies(lib: &Library, ctx: &RuleContext) -> bool {
    rules_allow(lib.rules.as_deref(), ctx) && natives_arch_ok(&lib.name, ctx.platform.arch)
}

/// Flatten argument templates, keeping only those whose rules allow this context.
pub fn resolve_args(args: &[ArgValue], ctx: &RuleContext) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            ArgValue::Plain(s) => out.push(s.clone()),
            ArgValue::Conditional { rules, value } => {
                if rules_allow(Some(rules.as_slice()), ctx) {
                    out.extend(value.to_vec());
                }
            }
        }
    }
    out
}
```

The top of `maven.rs`:
```rust
/// A maven coordinate `group:artifact:version[:classifier][@extension]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub extension: String,
}

impl MavenCoord {
    pub fn parse(name: &str) -> Option<Self> {
        let (coords, extension) = match name.split_once('@') {
            Some((c, e)) => (c, e.to_string()),
            None => (name, "jar".to_string()),
        };
        let parts: Vec<&str> = coords.split(':').collect();
        if !(3..=4).contains(&parts.len()) || parts.iter().any(|p| p.is_empty()) {
            return None;
        }
        Some(Self {
            group: parts[0].to_string(),
            artifact: parts[1].to_string(),
            version: parts[2].to_string(),
            classifier: parts.get(3).map(|c| c.to_string()),
            extension,
        })
    }

    /// Repository-relative path, always with `/` separators.
    pub fn path(&self) -> String {
        let classifier = self.classifier.as_ref().map(|c| format!("-{c}")).unwrap_or_default();
        format!(
            "{}/{}/{}/{}-{}{}.{}",
            self.group.replace('.', "/"),
            self.artifact,
            self.version,
            self.artifact,
            self.version,
            classifier,
            self.extension
        )
    }

    /// `group:artifact[:classifier]`: two libraries with the same identity are the same library at different versions.
    pub fn identity(&self) -> String {
        match &self.classifier {
            Some(c) => format!("{}:{}:{}", self.group, self.artifact, c),
            None => format!("{}:{}", self.group, self.artifact),
        }
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core rules maven`
Expected: 7 passed.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(core): Mojang rule evaluation, argument resolution, maven coordinates" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Fabric/Quilt profiles and version merging

**Files:**
- Create: `crates/launcher-core/src/meta/loaders.rs`, `crates/launcher-core/src/meta/merge.rs`
- Create: the fixture `crates/launcher-core/tests/fixtures/fabric-1.20.1-0.16.14.json`
- Modify: `crates/launcher-core/src/meta/mod.rs` (add `pub mod loaders; pub mod merge;`)

**Interfaces:**
- Consumes: `VersionJson`, `Arguments` and `MetaClient` (Tasks 4–5), and `MavenCoord` (Task 6).
- Produces:
  - `loaders::LoaderKind::{Fabric, Quilt, Forge, NeoForge}` (serde lowercase, `Copy`).
  - `loaders::profile_url(LoaderKind, mc: &str, loader: &str) -> Option<String>`, which is `None` for Forge and NeoForge until Plan 2.
  - `loaders::fetch_profile(&MetaClient, LoaderKind, mc, loader) -> Result<VersionJson>` (async).
  - `merge::merge(parent: VersionJson, child: VersionJson) -> VersionJson`. The result takes the child's id and main class and `inherits_from = None`. Its libraries are the child's first, then the parent's minus any whose `MavenCoord::identity` the child also has. When both sides have arguments, game and jvm are concatenated parent-then-child. Every other field is the child's if present, otherwise the parent's.

- [ ] **Step 1: Record the fixture**

```bash
curl -sSfo crates/launcher-core/tests/fixtures/fabric-1.20.1-0.16.14.json "https://meta.fabricmc.net/v2/versions/loader/1.20.1/0.16.14/profile/json"
```

- [ ] **Step 2: Write the failing tests**

At the bottom of `merge.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        meta::mojang::ArgValue,
        test_support::version,
    };

    #[test]
    fn fabric_profile_merges_onto_vanilla() {
        let merged = merge(version("1.20.1.json"), version("fabric-1.20.1-0.16.14.json"));
        assert_eq!(merged.id, "fabric-loader-0.16.14-1.20.1");
        assert_eq!(merged.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert!(merged.inherits_from.is_none());
        assert_eq!(merged.libraries.len(), 8 + 88);
        assert_eq!(merged.libraries[0].name, "org.ow2.asm:asm:9.8");
        assert_eq!(merged.asset_index.as_ref().unwrap().id, "5");
        assert!(merged.downloads.is_some(), "client download comes from vanilla");
        let jvm = &merged.arguments.as_ref().unwrap().jvm;
        assert_eq!(jvm.last().unwrap(), &ArgValue::Plain("-DFabricMcEmu= net.minecraft.client.main.Main ".into()));
        assert_eq!(merged.java_version.as_ref().unwrap().major_version, 17);
    }

    #[test]
    fn child_library_replaces_same_identity() {
        let parent: VersionJson = serde_json::from_str(
            r#"{"id":"p","mainClass":"P","libraries":[{"name":"a:b:1"},{"name":"c:d:1"}]}"#,
        )
        .unwrap();
        let child: VersionJson =
            serde_json::from_str(r#"{"id":"c","mainClass":"C","libraries":[{"name":"a:b:2"}]}"#).unwrap();
        let names: Vec<String> = merge(parent, child).libraries.into_iter().map(|l| l.name).collect();
        assert_eq!(names, vec!["a:b:2", "c:d:1"]);
    }
}
```

At the bottom of `loaders.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_urls() {
        assert_eq!(
            profile_url(LoaderKind::Fabric, "1.20.1", "0.16.14").unwrap(),
            "https://meta.fabricmc.net/v2/versions/loader/1.20.1/0.16.14/profile/json"
        );
        assert_eq!(
            profile_url(LoaderKind::Quilt, "1.20.1", "0.29.1").unwrap(),
            "https://meta.quiltmc.org/v3/versions/loader/1.20.1/0.29.1/profile/json"
        );
        assert!(profile_url(LoaderKind::Forge, "1.20.1", "47.4.22").is_none());
    }

    #[test]
    fn kind_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&LoaderKind::NeoForge).unwrap(), "\"neoforge\"");
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p launcher-core merge loaders`
Expected: compile errors.

- [ ] **Step 4: Implement**

The top of `loaders.rs`:
```rust
use serde::{Deserialize, Serialize};

use super::{mojang::VersionJson, MetaClient};
use crate::error::{LauncherError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

/// Where to get a loader's launch profile (a version JSON that inherits from vanilla).
pub fn profile_url(kind: LoaderKind, mc: &str, loader: &str) -> Option<String> {
    match kind {
        LoaderKind::Fabric => Some(format!("https://meta.fabricmc.net/v2/versions/loader/{mc}/{loader}/profile/json")),
        LoaderKind::Quilt => Some(format!("https://meta.quiltmc.org/v3/versions/loader/{mc}/{loader}/profile/json")),
        LoaderKind::Forge | LoaderKind::NeoForge => None,
    }
}

pub async fn fetch_profile(meta: &MetaClient, kind: LoaderKind, mc: &str, loader: &str) -> Result<VersionJson> {
    let Some(url) = profile_url(kind, mc, loader) else {
        return Err(LauncherError::loader(format!("{kind:?} isn't supported yet"))
            .with_fix("Use Fabric or Quilt for now; Forge and NeoForge are coming"));
    };
    meta.get_json(&url).await.map_err(|e| {
        LauncherError::loader(format!("Couldn't get {kind:?} {loader} for Minecraft {mc}"))
            .with_fix("Check that this loader version exists for this Minecraft version")
            .with_detail(e.message)
    })
}
```

The top of `merge.rs`:
```rust
use std::collections::HashSet;

use super::{
    maven::MavenCoord,
    mojang::{Arguments, VersionJson},
};

/// Apply a loader profile (`child`, with `inheritsFrom`) on top of the vanilla version (`parent`).
pub fn merge(parent: VersionJson, child: VersionJson) -> VersionJson {
    let child_ids: HashSet<String> =
        child.libraries.iter().filter_map(|l| MavenCoord::parse(&l.name)).map(|c| c.identity()).collect();
    let mut libraries = child.libraries;
    libraries.extend(parent.libraries.into_iter().filter(|l| {
        MavenCoord::parse(&l.name).map(|c| !child_ids.contains(&c.identity())).unwrap_or(true)
    }));
    let arguments = match (parent.arguments, child.arguments) {
        (Some(p), Some(c)) => Some(Arguments { game: [p.game, c.game].concat(), jvm: [p.jvm, c.jvm].concat() }),
        (p, c) => c.or(p),
    };
    VersionJson {
        id: child.id,
        inherits_from: None,
        main_class: child.main_class,
        arguments,
        minecraft_arguments: child.minecraft_arguments.or(parent.minecraft_arguments),
        asset_index: child.asset_index.or(parent.asset_index),
        assets: child.assets.or(parent.assets),
        downloads: child.downloads.or(parent.downloads),
        java_version: child.java_version.or(parent.java_version),
        libraries,
        logging: child.logging.or(parent.logging),
        kind: child.kind.or(parent.kind),
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p launcher-core merge loaders`
Expected: 4 passed.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(core): Fabric/Quilt profiles and loader-onto-vanilla merge" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Mojang Java runtimes

**Files:**
- Create: `crates/launcher-core/src/java.rs`
- Create: the fixtures `crates/launcher-core/tests/fixtures/{java-all.json,jre17-windows-x64.json}`
- Modify: `crates/launcher-core/src/lib.rs` (add `pub mod java;`)

**Interfaces:**
- Consumes: `MetaClient` (Task 4), `Downloader`/`DownloadRequest`/`Hashes` (Tasks 2–3), `Artifact`/`VersionJson` (Task 5), `Platform`/`Os` (Task 1), `write_atomic` (Task 4) and `EventSink`.
- Produces:
  - `java::RUNTIME_INDEX_URL`.
  - `RuntimeIndex(pub HashMap<String, HashMap<String, Vec<RuntimeEntry>>>)`, `RuntimeEntry { manifest: ManifestRef, version: RuntimeVersion }`, `RuntimeManifest { files: BTreeMap<String, RuntimeFile> }` and `RuntimeFile::{File { executable, downloads }, Directory, Link { target }}`.
  - `component_for(&VersionJson) -> String`, which defaults to `"jre-legacy"`.
  - `select_runtime(&RuntimeIndex, Platform, component) -> Result<&RuntimeEntry>`.
  - `plan_runtime(&RuntimeManifest, dest: &Path) -> RuntimePlan { downloads, executables, links }`.
  - `java_executable(runtime_dir, Platform) -> PathBuf`: `bin/javaw.exe` on Windows (so no console window opens), `bin/java` elsewhere.
  - `async ensure_runtime(&MetaClient, &Downloader, java_root: &Path, Platform, component: &str, &EventSink) -> Result<PathBuf>`. It installs into `java_root/<component>` and writes a `.perland-runtime` marker containing the manifest sha1. Later calls with the same sha1 return straight away.
  - `parse_java_version(output: &str) -> Option<u32>`.

- [ ] **Step 1: Record the fixtures**

```bash
curl -sSfo crates/launcher-core/tests/fixtures/java-all.json "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json"
curl -sSfo crates/launcher-core/tests/fixtures/jre17-windows-x64.json "https://piston-meta.mojang.com/v1/packages/89ce85ccb518c62e18b4b58d63399ba2d9611426/manifest.json"
```

- [ ] **Step 2: Write the failing tests** (at the bottom of `java.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        test_support::{fixture, version, WINDOWS_X64},
        ErrorCategory,
    };

    #[test]
    fn picks_component_from_version() {
        assert_eq!(component_for(&version("1.20.1.json")), "java-runtime-gamma");
        assert_eq!(component_for(&version("1.12.2.json")), "jre-legacy");
    }

    #[test]
    fn selects_runtime_for_platform() {
        let index: RuntimeIndex = serde_json::from_str(&fixture("java-all.json")).unwrap();
        let entry = select_runtime(&index, WINDOWS_X64, "java-runtime-gamma").unwrap();
        assert_eq!(entry.version.name, "17.0.15");
        assert_eq!(entry.manifest.sha1, "89ce85ccb518c62e18b4b58d63399ba2d9611426");
        let err = select_runtime(&index, WINDOWS_X64, "java-runtime-nope").unwrap_err();
        assert_eq!(err.category, ErrorCategory::Java);
        assert!(err.fix.is_some());
    }

    #[test]
    fn plans_runtime_files() {
        let manifest: RuntimeManifest = serde_json::from_str(&fixture("jre17-windows-x64.json")).unwrap();
        let dest = Path::new("/j/java-runtime-gamma");
        let plan = plan_runtime(&manifest, dest);
        assert_eq!(plan.downloads.len(), 403);
        let javaw = plan.downloads.iter().find(|d| d.dest == dest.join("bin/javaw.exe")).unwrap();
        assert_eq!(javaw.hashes.sha1.as_deref(), Some("503664a377eb2e63a5f74b273299f622fca2bf95"));
        assert_eq!(javaw.size, Some(39424));
        assert!(plan.executables.contains(&dest.join("bin/javaw.exe")));
        assert!(plan.links.is_empty());
    }

    #[test]
    fn executable_path_per_os() {
        assert_eq!(java_executable(Path::new("/j/x"), WINDOWS_X64), Path::new("/j/x/bin/javaw.exe"));
        assert_eq!(java_executable(Path::new("/j/x"), crate::test_support::LINUX_X64), Path::new("/j/x/bin/java"));
    }

    #[test]
    fn parses_java_version_output() {
        assert_eq!(parse_java_version("openjdk version \"17.0.15\" 2025-04-15"), Some(17));
        assert_eq!(parse_java_version("java version \"1.8.0_202\"\nJava(TM) SE Runtime"), Some(8));
        assert_eq!(parse_java_version("openjdk version \"25.0.3\" 2026-04-21 LTS"), Some(25));
        assert_eq!(parse_java_version("not java"), None);
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p launcher-core java`
Expected: compile errors.

- [ ] **Step 4: Implement** (the top of `java.rs`):
```rust
//! Mojang's official Java runtimes: the same builds the vanilla launcher uses.
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    download::{DownloadRequest, Downloader, Hashes},
    error::{LauncherError, Result},
    events::EventSink,
    fsutil::write_atomic,
    meta::{
        mojang::{Artifact, VersionJson},
        MetaClient,
    },
    platform::{Os, Platform},
};

pub const RUNTIME_INDEX_URL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct RuntimeIndex(pub HashMap<String, HashMap<String, Vec<RuntimeEntry>>>);

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeEntry {
    pub manifest: ManifestRef,
    pub version: RuntimeVersion,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestRef {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeVersion {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct RuntimeManifest {
    pub files: BTreeMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RuntimeFile {
    File {
        #[serde(default)]
        executable: bool,
        downloads: RuntimeDownloads,
    },
    Directory,
    Link {
        target: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct RuntimeDownloads {
    pub raw: Artifact,
}

pub struct RuntimePlan {
    pub downloads: Vec<DownloadRequest>,
    pub executables: Vec<PathBuf>,
    pub links: Vec<(PathBuf, String)>,
}

pub fn component_for(version: &VersionJson) -> String {
    version.java_version.as_ref().map(|j| j.component.clone()).unwrap_or_else(|| "jre-legacy".to_string())
}

pub fn select_runtime<'a>(index: &'a RuntimeIndex, platform: Platform, component: &str) -> Result<&'a RuntimeEntry> {
    platform
        .java_runtime_key()
        .and_then(|key| index.0.get(key))
        .and_then(|components| components.get(component))
        .and_then(|entries| entries.first())
        .ok_or_else(|| {
            LauncherError::java(format!("There's no official Java ({component}) for this computer"))
                .with_fix("Choose a Java installation in the instance settings")
        })
}

pub fn plan_runtime(manifest: &RuntimeManifest, dest: &Path) -> RuntimePlan {
    let mut plan = RuntimePlan { downloads: vec![], executables: vec![], links: vec![] };
    for (rel, file) in &manifest.files {
        let path = dest.join(rel);
        match file {
            RuntimeFile::File { executable, downloads } => {
                let raw = &downloads.raw;
                plan.downloads.push(DownloadRequest::new(raw.url.clone(), path.clone(), Hashes::sha1(&raw.sha1), Some(raw.size)));
                if *executable {
                    plan.executables.push(path);
                }
            }
            RuntimeFile::Directory => {}
            RuntimeFile::Link { target } => plan.links.push((path, target.clone())),
        }
    }
    plan
}

pub fn java_executable(runtime_dir: &Path, platform: Platform) -> PathBuf {
    match platform.os {
        Os::Windows => runtime_dir.join("bin").join("javaw.exe"),
        _ => runtime_dir.join("bin").join("java"),
    }
}

/// Make sure Mojang's runtime `component` is installed and return its java executable.
pub async fn ensure_runtime(
    meta: &MetaClient,
    downloader: &Downloader,
    java_root: &Path,
    platform: Platform,
    component: &str,
    sink: &EventSink,
) -> Result<PathBuf> {
    let index: RuntimeIndex = meta.get_json(RUNTIME_INDEX_URL).await?;
    let entry = select_runtime(&index, platform, component)?;
    let dir = java_root.join(component);
    let marker = dir.join(".perland-runtime");
    let exe = java_executable(&dir, platform);
    let installed = tokio::fs::read_to_string(&marker).await.map(|s| s.trim() == entry.manifest.sha1).unwrap_or(false);
    if installed && exe.is_file() {
        return Ok(exe);
    }
    let manifest: RuntimeManifest = meta.get_verified_json(&entry.manifest.url, &entry.manifest.sha1).await?;
    let plan = plan_runtime(&manifest, &dir);
    downloader.fetch_all(plan.downloads, &format!("Downloading Java {}", entry.version.name), sink).await?;
    finish_unix(&plan.executables, &plan.links)?;
    write_atomic(&marker, entry.manifest.sha1.as_bytes()).await?;
    Ok(exe)
}

#[cfg(unix)]
fn finish_unix(executables: &[PathBuf], links: &[(PathBuf, String)]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for exe in executables {
        std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o755))?;
    }
    for (link, target) in links {
        let _ = std::fs::remove_file(link);
        if let Some(parent) = link.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::os::unix::fs::symlink(target, link)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn finish_unix(_executables: &[PathBuf], _links: &[(PathBuf, String)]) -> Result<()> {
    Ok(())
}

/// Major version from `java -version` output: `"17.0.15"` → 17, `"1.8.0_202"` → 8.
pub fn parse_java_version(output: &str) -> Option<u32> {
    let start = output.find('"')? + 1;
    let rest = &output[start..];
    let version = &rest[..rest.find('"')?];
    let mut parts = version.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p launcher-core java`
Expected: 5 passed.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(core): Mojang Java runtime selection and installation" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Vanilla install (client, libraries, natives, assets, logging)

**Files:**
- Create: `crates/launcher-core/src/install/mod.rs`, `crates/launcher-core/src/install/natives.rs`
- Modify: `crates/launcher-core/src/lib.rs` (add `pub mod install;`)

**Interfaces:**
- Consumes: Tasks 1–7.
- Produces:
  - `install::ResolvedVersion { vanilla_id: String, json: VersionJson }`, where `json` is the merged JSON when a loader is used.
  - `install::NativeJar { jar: PathBuf, exclude: Vec<String> }` and `install::LibraryPlan { downloads: Vec<DownloadRequest>, natives: Vec<NativeJar> }`.
  - `install::library_artifact(&Library, &LauncherDirs) -> Option<DownloadRequest>`.
  - `install::plan_libraries(&VersionJson, &LauncherDirs, &RuleContext) -> LibraryPlan`.
  - `install::classpath(&ResolvedVersion, &LauncherDirs, &RuleContext) -> Vec<PathBuf>`: library jars in listed order, deduplicated, then `dirs.client_jar(vanilla_id)`.
  - `install::plan_client(&ResolvedVersion, &LauncherDirs) -> Result<DownloadRequest>`.
  - `install::plan_assets(&AssetIndex, &LauncherDirs) -> Vec<DownloadRequest>`, which keeps unique, well-formed (40-character) hashes only.
  - `install::plan_logging(&VersionJson, &LauncherDirs) -> Option<DownloadRequest>`.
  - `install::Installer { pub dirs, pub meta, pub downloader, pub platform }` with `async resolve(&self, mc: &str, loader: Option<(LoaderKind, &str)>) -> Result<ResolvedVersion>` and `async install(&self, &ResolvedVersion, &EventSink) -> Result<()>`.
  - `install::natives::extract_all(&[NativeJar], dest: &Path) -> Result<()>`.
- Unsupported in Plan 1: the legacy "virtual" and "map_to_resources" asset layouts (Minecraft 1.7.2 and older).

- [ ] **Step 1: Write the failing tests**

At the bottom of `install/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        meta::{merge::merge, mojang::AssetIndex},
        test_support::{fixture, version, WINDOWS_X64},
    };

    fn dirs() -> LauncherDirs {
        LauncherDirs::new("/pl")
    }

    #[test]
    fn modern_libraries_have_no_extraction() {
        let plan = plan_libraries(&version("1.20.1.json"), &dirs(), &RuleContext::new(WINDOWS_X64));
        assert_eq!(plan.downloads.len(), 50);
        assert!(plan.natives.is_empty());
        let names: Vec<String> = plan.downloads.iter().map(|d| d.dest.to_string_lossy().replace('\\', "/")).collect();
        assert!(names.iter().any(|n| n.ends_with("org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar")));
        assert!(!names.iter().any(|n| n.ends_with("natives-windows-x86.jar")));
    }

    #[test]
    fn legacy_natives_are_downloaded_and_extracted() {
        let plan = plan_libraries(&version("1.12.2.json"), &dirs(), &RuleContext::new(WINDOWS_X64));
        assert_eq!(plan.downloads.len(), 36);
        assert_eq!(plan.natives.len(), 3);
        let jar = plan.natives[0].jar.to_string_lossy().replace('\\', "/");
        assert!(jar.contains("-natives-windows"), "{jar}");
    }

    #[test]
    fn fabric_library_resolves_through_maven_url() {
        let lib = &version("fabric-1.20.1-0.16.14.json").libraries[0];
        let d = library_artifact(lib, &dirs()).unwrap();
        assert_eq!(d.urls[0], "https://maven.fabricmc.net/org/ow2/asm/asm/9.8/asm-9.8.jar");
        assert_eq!(d.hashes.sha1.as_deref(), Some("dc19ecb3f7889b7860697215cae99c0f9b6f6b4b"));
        assert_eq!(d.size, Some(126113));
        assert_eq!(d.dest, dirs().libraries().join("org/ow2/asm/asm/9.8/asm-9.8.jar"));
    }

    #[test]
    fn client_jar_uses_vanilla_id() {
        let resolved = ResolvedVersion {
            vanilla_id: "1.20.1".into(),
            json: merge(version("1.20.1.json"), version("fabric-1.20.1-0.16.14.json")),
        };
        let client = plan_client(&resolved, &dirs()).unwrap();
        assert_eq!(client.dest, dirs().client_jar("1.20.1"));
        assert_eq!(client.hashes.sha1.as_deref(), Some("0c3ec587af28e5a785c0b4a7b8a30f9a8f78f838"));
        let cp = classpath(&resolved, &dirs(), &RuleContext::new(WINDOWS_X64));
        assert!(cp[0].to_string_lossy().replace('\\', "/").ends_with("asm-9.8.jar"));
        assert_eq!(cp.last().unwrap(), &dirs().client_jar("1.20.1"));
        assert_eq!(cp.len(), 8 + 50 + 1);
    }

    #[test]
    fn plan_assets_dedupes_hashes() {
        let index: AssetIndex = serde_json::from_str(&fixture("asset-index-5.json")).unwrap();
        let plan = plan_assets(&index, &dirs());
        assert_eq!(plan.len(), 3575);
        let icon = plan.iter().find(|d| d.hashes.sha1.as_deref() == Some("b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356")).unwrap();
        assert_eq!(icon.urls[0], "https://resources.download.minecraft.net/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356");
        assert_eq!(icon.dest, dirs().assets().join("objects/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"));
    }

    #[test]
    fn logging_config_goes_to_log_configs() {
        let d = plan_logging(&version("1.20.1.json"), &dirs()).unwrap();
        assert_eq!(d.dest, dirs().log_config("client-1.12.xml"));
    }
}
```

At the bottom of `install/natives.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_jar(path: &Path) {
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in [("META-INF/MANIFEST.MF", "m"), ("lwjgl64.dll", "dll"), ("sub/libx.so", "so")] {
            w.start_file(name, opts).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn extracts_respecting_excludes_and_cleans_old_files() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("natives.jar");
        make_jar(&jar);
        let dest = tmp.path().join("natives");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("stale.dll"), "old").unwrap();
        extract_all(&[NativeJar { jar, exclude: vec!["META-INF/".into()] }], &dest).unwrap();
        assert_eq!(fs::read_to_string(dest.join("lwjgl64.dll")).unwrap(), "dll");
        assert!(dest.join("sub/libx.so").exists());
        assert!(!dest.join("META-INF").exists());
        assert!(!dest.join("stale.dll").exists());
    }

    #[test]
    fn corrupt_jar_is_integrity_error() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("bad.jar");
        fs::write(&jar, "not a zip").unwrap();
        let err = extract_all(&[NativeJar { jar, exclude: vec![] }], &tmp.path().join("n")).unwrap_err();
        assert_eq!(err.category, crate::ErrorCategory::Integrity);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p launcher-core install`
Expected: compile errors.

- [ ] **Step 3: Implement**

The top of `install/natives.rs`:
```rust
use std::{fs, io, path::Path};

use super::NativeJar;
use crate::error::{LauncherError, Result};

/// Extract native libraries for versions that still ship them inside jars (1.18 and older).
/// The folder is rebuilt from scratch so files from a previous version never linger.
pub fn extract_all(jars: &[NativeJar], dest: &Path) -> Result<()> {
    if jars.is_empty() {
        return Ok(());
    }
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::create_dir_all(dest)?;
    for native in jars {
        extract_one(&native.jar, &native.exclude, dest)?;
    }
    Ok(())
}

fn extract_one(jar: &Path, exclude: &[String], dest: &Path) -> Result<()> {
    let bad = |e: zip::result::ZipError| {
        LauncherError::integrity(format!("{} isn't a valid archive", jar.display()))
            .with_fix("Repair the instance to download it again")
            .with_detail(e)
    };
    let mut archive = zip::ZipArchive::new(fs::File::open(jar)?).map_err(bad)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(bad)?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else { continue };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if exclude.iter().any(|ex| rel_str.starts_with(ex.as_str())) {
            continue;
        }
        let out = dest.join(&rel);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        io::copy(&mut entry, &mut fs::File::create(&out)?)?;
    }
    Ok(())
}
```

The top of `install/mod.rs`:
```rust
pub mod natives;

use std::{collections::HashSet, path::PathBuf};

use crate::{
    download::{DownloadRequest, Downloader, Hashes},
    error::{LauncherError, Result},
    events::EventSink,
    fsutil::write_atomic,
    meta::{
        loaders::{self, LoaderKind},
        maven::MavenCoord,
        merge::merge,
        mojang::{AssetIndex, Library, VersionJson, VersionManifest, RESOURCES_URL, VERSION_MANIFEST_URL},
        parse_json,
        rules::{library_applies, RuleContext},
        MetaClient,
    },
    paths::LauncherDirs,
    platform::{Arch, Platform},
};

const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";

/// The version to launch: `json` is vanilla, or vanilla merged with a loader profile.
#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    pub vanilla_id: String,
    pub json: VersionJson,
}

#[derive(Debug, Clone)]
pub struct NativeJar {
    pub jar: PathBuf,
    pub exclude: Vec<String>,
}

#[derive(Debug, Default)]
pub struct LibraryPlan {
    pub downloads: Vec<DownloadRequest>,
    pub natives: Vec<NativeJar>,
}

/// Where a library's main jar comes from and goes to. `None` for natives-only libraries.
pub fn library_artifact(lib: &Library, dirs: &LauncherDirs) -> Option<DownloadRequest> {
    if let Some(downloads) = &lib.downloads {
        let art = downloads.artifact.as_ref()?;
        let rel = art.path.clone().or_else(|| MavenCoord::parse(&lib.name).map(|c| c.path()))?;
        return Some(DownloadRequest::new(art.url.clone(), dirs.libraries().join(rel), Hashes::sha1(&art.sha1), Some(art.size)));
    }
    let coord = MavenCoord::parse(&lib.name)?;
    let rel = coord.path();
    let base = lib.url.as_deref().unwrap_or(MOJANG_LIBRARIES);
    let hashes = Hashes { sha1: lib.sha1.clone(), sha256: lib.sha256.clone(), sha512: lib.sha512.clone(), murmur2: None };
    Some(DownloadRequest::new(format!("{}/{}", base.trim_end_matches('/'), rel), dirs.libraries().join(rel), hashes, lib.size))
}

fn native_classifier(lib: &Library, platform: Platform) -> Option<String> {
    let key = lib.natives.as_ref()?.get(platform.mojang_os_name())?;
    Some(key.replace("${arch}", if platform.arch == Arch::X86 { "32" } else { "64" }))
}

pub fn plan_libraries(version: &VersionJson, dirs: &LauncherDirs, ctx: &RuleContext) -> LibraryPlan {
    let mut plan = LibraryPlan::default();
    for lib in version.libraries.iter().filter(|l| library_applies(l, ctx)) {
        if let Some(d) = library_artifact(lib, dirs) {
            plan.downloads.push(d);
        }
        let Some(classifier) = native_classifier(lib, ctx.platform) else { continue };
        let Some(art) = lib.downloads.as_ref().and_then(|d| d.classifiers.as_ref()).and_then(|c| c.get(&classifier)) else {
            continue;
        };
        let rel = art.path.clone().or_else(|| MavenCoord::parse(&format!("{}:{classifier}", lib.name)).map(|c| c.path()));
        if let Some(rel) = rel {
            let jar = dirs.libraries().join(rel);
            plan.downloads.push(DownloadRequest::new(art.url.clone(), jar.clone(), Hashes::sha1(&art.sha1), Some(art.size)));
            plan.natives.push(NativeJar { jar, exclude: lib.extract.as_ref().map(|e| e.exclude.clone()).unwrap_or_default() });
        }
    }
    plan
}

pub fn classpath(resolved: &ResolvedVersion, dirs: &LauncherDirs, ctx: &RuleContext) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut cp: Vec<PathBuf> = resolved
        .json
        .libraries
        .iter()
        .filter(|l| library_applies(l, ctx))
        .filter_map(|l| library_artifact(l, dirs))
        .map(|d| d.dest)
        .filter(|p| seen.insert(p.clone()))
        .collect();
    cp.push(dirs.client_jar(&resolved.vanilla_id));
    cp
}

pub fn plan_client(resolved: &ResolvedVersion, dirs: &LauncherDirs) -> Result<DownloadRequest> {
    let client = resolved
        .json
        .downloads
        .as_ref()
        .map(|d| &d.client)
        .ok_or_else(|| LauncherError::loader(format!("Minecraft {} has no client download", resolved.vanilla_id)))?;
    Ok(DownloadRequest::new(client.url.clone(), dirs.client_jar(&resolved.vanilla_id), Hashes::sha1(&client.sha1), Some(client.size)))
}

pub fn plan_assets(index: &AssetIndex, dirs: &LauncherDirs) -> Vec<DownloadRequest> {
    let mut seen = HashSet::new();
    index
        .objects
        .values()
        .filter(|o| o.hash.len() == 40 && seen.insert(o.hash.clone()))
        .map(|o| {
            let prefix = &o.hash[..2];
            DownloadRequest::new(
                format!("{RESOURCES_URL}/{prefix}/{}", o.hash),
                dirs.assets().join("objects").join(prefix).join(&o.hash),
                Hashes::sha1(&o.hash),
                Some(o.size),
            )
        })
        .collect()
}

pub fn plan_logging(version: &VersionJson, dirs: &LauncherDirs) -> Option<DownloadRequest> {
    let file = &version.logging.as_ref()?.client.as_ref()?.file;
    Some(DownloadRequest::new(file.url.clone(), dirs.log_config(&file.id), Hashes::sha1(&file.sha1), Some(file.size)))
}

pub struct Installer {
    pub dirs: LauncherDirs,
    pub meta: MetaClient,
    pub downloader: Downloader,
    pub platform: Platform,
}

impl Installer {
    /// Look up the vanilla version (and loader profile) and merge them. Works offline once cached.
    pub async fn resolve(&self, mc: &str, loader: Option<(LoaderKind, &str)>) -> Result<ResolvedVersion> {
        let manifest: VersionManifest = self.meta.get_json(VERSION_MANIFEST_URL).await?;
        let entry = manifest
            .find(mc)
            .ok_or_else(|| LauncherError::loader(format!("Minecraft {mc} doesn't exist")).with_fix("Check the version number"))?;
        let vanilla: VersionJson = self.meta.get_verified_json(&entry.url, &entry.sha1).await?;
        let json = match loader {
            Some((kind, version)) => merge(vanilla.clone(), loaders::fetch_profile(&self.meta, kind, mc, version).await?),
            None => vanilla.clone(),
        };
        Ok(ResolvedVersion { vanilla_id: vanilla.id, json })
    }

    /// Download everything the version needs, extract natives, and save the merged JSON for offline launches.
    pub async fn install(&self, resolved: &ResolvedVersion, sink: &EventSink) -> Result<()> {
        let ctx = RuleContext::new(self.platform);
        let json = &resolved.json;
        let index_ref = json
            .asset_index
            .as_ref()
            .ok_or_else(|| LauncherError::loader(format!("{} has no asset index", json.id)))?;
        let index_path = self.dirs.assets().join("indexes").join(format!("{}.json", index_ref.id));
        self.downloader
            .fetch(&DownloadRequest::new(index_ref.url.clone(), index_path.clone(), Hashes::sha1(&index_ref.sha1), Some(index_ref.size)))
            .await?;
        let index: AssetIndex = parse_json(&index_ref.url, &tokio::fs::read(&index_path).await?)?;

        let libraries = plan_libraries(json, &self.dirs, &ctx);
        let mut downloads = vec![plan_client(resolved, &self.dirs)?];
        downloads.extend(libraries.downloads);
        downloads.extend(plan_assets(&index, &self.dirs));
        downloads.extend(plan_logging(json, &self.dirs));
        self.downloader.fetch_all(downloads, &format!("Installing Minecraft {}", resolved.vanilla_id), sink).await?;

        natives::extract_all(&libraries.natives, &self.dirs.natives_dir(&json.id))?;
        let bytes = serde_json::to_vec_pretty(json)
            .map_err(|e| LauncherError::disk("Couldn't save the version file").with_detail(e))?;
        write_atomic(&self.dirs.version_json(&json.id), &bytes).await?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core install`
Expected: 8 passed.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(core): vanilla install planning, natives extraction and installer" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Instances

**Files:**
- Create: `crates/launcher-core/src/instances.rs`
- Modify: `crates/launcher-core/src/lib.rs` (add `pub mod instances;`)

**Interfaces:**
- Consumes: `LoaderKind` (Task 7), `write_atomic` (Task 4) and `LauncherError`.
- Produces:
  - `instances::SourceRef::{None, Packwiz { url }, Mrpack { location }, Modrinth { project_id, version_id }}`, serialized as `{"kind": ...}`. Plan 1 only creates `None`; Plan 3 uses the rest.
  - `LoaderSpec { kind: LoaderKind, version }` and `GameSpec { minecraft, loader: Option<LoaderSpec> }`.
  - `UpdatePolicy::{Auto, Ask, Pinned}`, defaulting to `Ask`, and `JavaSettings { path: Option<PathBuf>, memory_mb: u32 }`, defaulting to 4096 MB.
  - `Instance { id, name, source, game, update_policy, optional_choices: BTreeMap<String,bool>, java }`, camelCase JSON.
  - `InstanceStore::new(dir)` with `instance_dir(id)`, `game_dir(id)` (`<id>/minecraft`), `perland_dir(id)` (`<id>/.perland`), and the async `create(name, GameSpec) -> Result<Instance>`, `save(&Instance)`, `load(id) -> Result<Instance>` and `list() -> Result<Vec<Instance>>` (sorted by name, case-insensitively).
  - `slugify(name) -> String`.

- [ ] **Step 1: Write the failing tests** (at the bottom of `instances.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCategory;

    fn vanilla(mc: &str) -> GameSpec {
        GameSpec { minecraft: mc.into(), loader: None }
    }

    #[test]
    fn slugify_handles_reserved_and_symbols() {
        assert_eq!(slugify("Asalith Fields"), "asalith-fields");
        assert_eq!(slugify("  ¡Hola Mundo!  "), "hola-mundo");
        assert_eq!(slugify("!!!"), "instance");
        assert_eq!(slugify("CON"), "con-instance");
        assert_eq!(slugify("lpt1"), "lpt1-instance");
        assert_eq!(slugify("Fabric 1.21 / test"), "fabric-1-21-test");
    }

    #[tokio::test]
    async fn create_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        let game = GameSpec { minecraft: "1.20.1".into(), loader: Some(LoaderSpec { kind: LoaderKind::Fabric, version: "0.16.14".into() }) };
        let created = store.create("Asalith Fields", game).await.unwrap();
        assert_eq!(created.id, "asalith-fields");
        assert!(store.game_dir("asalith-fields").is_dir());
        assert!(store.perland_dir("asalith-fields").is_dir());
        assert_eq!(store.load("asalith-fields").await.unwrap(), created);
        let raw = std::fs::read_to_string(store.instance_dir("asalith-fields").join("instance.json")).unwrap();
        assert!(raw.contains("\"updatePolicy\": \"ask\""), "{raw}");
        assert!(raw.contains("\"kind\": \"none\""), "{raw}");
        assert!(raw.contains("\"memoryMb\": 4096"), "{raw}");
    }

    #[tokio::test]
    async fn duplicate_names_get_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        assert_eq!(store.create("Pack", vanilla("1.21.1")).await.unwrap().id, "pack");
        assert_eq!(store.create("Pack", vanilla("1.21.1")).await.unwrap().id, "pack-2");
        assert_eq!(store.create("pack!", vanilla("1.21.1")).await.unwrap().id, "pack-3");
    }

    #[tokio::test]
    async fn list_is_sorted_and_tolerates_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path().join("instances"));
        assert!(store.list().await.unwrap().is_empty());
        store.create("zeta", vanilla("1.21.1")).await.unwrap();
        store.create("Alpha", vanilla("1.21.1")).await.unwrap();
        let names: Vec<String> = store.list().await.unwrap().into_iter().map(|i| i.name).collect();
        assert_eq!(names, vec!["Alpha", "zeta"]);
    }

    #[tokio::test]
    async fn missing_instance_is_disk_error() {
        let tmp = tempfile::tempdir().unwrap();
        let err = InstanceStore::new(tmp.path()).load("nope").await.unwrap_err();
        assert_eq!(err.category, ErrorCategory::Disk);
    }

    #[test]
    fn modrinth_source_uses_camel_case() {
        let s = SourceRef::Modrinth { project_id: "abc".into(), version_id: "def".into() };
        assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"kind":"modrinth","projectId":"abc","versionId":"def"}"#);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p launcher-core instances`
Expected: compile errors.

- [ ] **Step 3: Implement** (the top of `instances.rs`):
```rust
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{LauncherError, Result},
    fsutil::write_atomic,
    meta::loaders::LoaderKind,
};

/// Where an instance's content comes from. Plan 1 creates only `None`; packs arrive in Plan 3.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SourceRef {
    None,
    Packwiz {
        url: String,
    },
    Mrpack {
        location: String,
    },
    #[serde(rename_all = "camelCase")]
    Modrinth {
        project_id: String,
        version_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoaderSpec {
    pub kind: LoaderKind,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSpec {
    pub minecraft: String,
    #[serde(default)]
    pub loader: Option<LoaderSpec>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdatePolicy {
    Auto,
    #[default]
    Ask,
    Pinned,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaSettings {
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default = "default_memory_mb")]
    pub memory_mb: u32,
}

fn default_memory_mb() -> u32 {
    4096
}

impl Default for JavaSettings {
    fn default() -> Self {
        Self { path: None, memory_mb: default_memory_mb() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub source: SourceRef,
    pub game: GameSpec,
    #[serde(default)]
    pub update_policy: UpdatePolicy,
    #[serde(default)]
    pub optional_choices: BTreeMap<String, bool>,
    #[serde(default)]
    pub java: JavaSettings,
}

pub struct InstanceStore {
    dir: PathBuf,
}

impl InstanceStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }

    pub fn game_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("minecraft")
    }

    pub fn perland_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join(".perland")
    }

    fn json_path(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("instance.json")
    }

    pub async fn create(&self, name: &str, game: GameSpec) -> Result<Instance> {
        let base = slugify(name);
        let mut id = base.clone();
        let mut n = 2;
        while tokio::fs::try_exists(self.instance_dir(&id)).await? {
            id = format!("{base}-{n}");
            n += 1;
        }
        let instance = Instance {
            id,
            name: name.trim().to_string(),
            source: SourceRef::None,
            game,
            update_policy: UpdatePolicy::Ask,
            optional_choices: BTreeMap::new(),
            java: JavaSettings::default(),
        };
        tokio::fs::create_dir_all(self.game_dir(&instance.id)).await?;
        tokio::fs::create_dir_all(self.perland_dir(&instance.id)).await?;
        self.save(&instance).await?;
        Ok(instance)
    }

    pub async fn save(&self, instance: &Instance) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(instance)
            .map_err(|e| LauncherError::disk("Couldn't save the instance").with_detail(e))?;
        write_atomic(&self.json_path(&instance.id), &bytes).await
    }

    pub async fn load(&self, id: &str) -> Result<Instance> {
        let path = self.json_path(id);
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| LauncherError::disk(format!("Instance \"{id}\" wasn't found")).with_detail(e))?;
        serde_json::from_slice(&bytes).map_err(|e| {
            LauncherError::disk(format!("The settings file of \"{id}\" is damaged"))
                .with_fix("Restore instance.json from a backup or recreate the instance")
                .with_detail(e)
        })
    }

    pub async fn list(&self) -> Result<Vec<Instance>> {
        let mut out = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let id = entry.file_name().to_string_lossy().into_owned();
            if is_instance_dir(&entry.path()) {
                if let Ok(instance) = self.load(&id).await {
                    out.push(instance);
                }
            }
        }
        out.sort_by_key(|i| i.name.to_lowercase());
        Ok(out)
    }
}

fn is_instance_dir(path: &Path) -> bool {
    path.join("instance.json").is_file()
}

const WINDOWS_RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1",
    "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// A folder-safe id: lowercase ASCII letters/digits joined by single dashes; never empty, never a Windows device name.
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "instance".to_string()
    } else if WINDOWS_RESERVED.contains(&slug.as_str()) {
        format!("{slug}-instance")
    } else {
        slug
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core instances`
Expected: 6 passed.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(core): instance model and store" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Offline/dev accounts

**Files:**
- Create: `crates/launcher-core/src/accounts.rs`
- Modify: `crates/launcher-core/src/lib.rs` (add `pub mod accounts;`)

**Interfaces:**
- Consumes: `write_atomic` and `LauncherError`.
- Produces:
  - `accounts::Provider::{Microsoft, Offline}` and `Account { provider, username, uuid /* dashed */ }` with `.offline_session() -> Session`.
  - `Session { username, uuid /* undashed */, access_token, user_type, xuid: Option<String> }`.
  - `offline_uuid(name) -> String`, the same as Java's `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`.
  - `validate_offline_name(name) -> Result<()>` (3–16 characters of `[A-Za-z0-9_]`) and `offline_account(name) -> Result<Account>`.
  - `BuildKind::{Dev, Release}` with `BuildKind::current()` (`Dev` exactly when `debug_assertions` is on), and `offline_allowed(BuildKind, licensed_signin_seen: bool) -> bool`.
  - `AccountsFile { accounts, active: Option<String /* uuid */>, licensed_signin_seen: bool }` and `AccountStore::new(path)` with the async `load()`, `save(&AccountsFile)`, `add_offline(name, BuildKind) -> Result<Account>` and `active() -> Result<Option<Account>>`.
- Spec §4 rule: release builds unlock offline accounts only after a licensed Microsoft sign-in, which Plan 4 records by setting `licensed_signin_seen`. Dev builds are always unlocked.

- [ ] **Step 1: Write the failing tests** (at the bottom of `accounts.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCategory;

    #[test]
    fn offline_uuid_matches_java() {
        assert_eq!(offline_uuid("Notch"), "b50ad385-829d-3141-a216-7e7d7539ba7f");
        assert_eq!(offline_uuid("Steve"), "5627dd98-e6be-3c21-b8a8-e92344183641");
    }

    #[test]
    fn validates_offline_names() {
        assert!(validate_offline_name("Steve").is_ok());
        assert!(validate_offline_name("a_b_c").is_ok());
        assert!(validate_offline_name("ab").is_err());
        assert!(validate_offline_name("has space").is_err());
        assert!(validate_offline_name("seventeen_chars_x").is_err());
        assert_eq!(validate_offline_name("ñandú").unwrap_err().category, ErrorCategory::Auth);
    }

    #[test]
    fn offline_session_shape() {
        let s = offline_account("Steve").unwrap().offline_session();
        assert_eq!(s.uuid, "5627dd98e6be3c21b8a8e92344183641");
        assert_eq!(s.access_token, "0");
        assert_eq!(s.user_type, "legacy");
    }

    #[test]
    fn offline_gate() {
        assert!(offline_allowed(BuildKind::Dev, false));
        assert!(!offline_allowed(BuildKind::Release, false));
        assert!(offline_allowed(BuildKind::Release, true));
    }

    #[tokio::test]
    async fn store_gates_and_activates() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AccountStore::new(tmp.path().join("accounts.json"));
        let err = store.add_offline("Steve", BuildKind::Release).await.unwrap_err();
        assert_eq!(err.category, ErrorCategory::Auth);
        assert!(err.fix.is_some());
        let a = store.add_offline("Steve", BuildKind::Dev).await.unwrap();
        store.add_offline("Steve", BuildKind::Dev).await.unwrap();
        assert_eq!(store.load().await.unwrap().accounts.len(), 1, "re-adding does not duplicate");
        assert_eq!(store.active().await.unwrap(), Some(a));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p launcher-core accounts`
Expected: compile errors.

- [ ] **Step 3: Implement** (the top of `accounts.rs`):
```rust
use std::path::PathBuf;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::{
    error::{LauncherError, Result},
    fsutil::write_atomic,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Microsoft,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub provider: Provider,
    pub username: String,
    pub uuid: String,
}

/// What the game needs to start. Never log `access_token`.
#[derive(Debug, Clone)]
pub struct Session {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub user_type: String,
    pub xuid: Option<String>,
}

impl Account {
    pub fn offline_session(&self) -> Session {
        Session {
            username: self.username.clone(),
            uuid: self.uuid.replace('-', ""),
            access_token: "0".to_string(),
            user_type: "legacy".to_string(),
            xuid: None,
        }
    }
}

/// Same as Java's `UUID.nameUUIDFromBytes("OfflinePlayer:" + name)`, which is what servers expect for offline players.
pub fn offline_uuid(username: &str) -> String {
    let mut h = Md5::digest(format!("OfflinePlayer:{username}").as_bytes());
    h[6] = (h[6] & 0x0f) | 0x30;
    h[8] = (h[8] & 0x3f) | 0x80;
    let x = hex::encode(h);
    format!("{}-{}-{}-{}-{}", &x[0..8], &x[8..12], &x[12..16], &x[16..20], &x[20..32])
}

pub fn validate_offline_name(name: &str) -> Result<()> {
    let ok = (3..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(())
    } else {
        Err(LauncherError::auth("Offline names must be 3–16 letters, numbers or underscores"))
    }
}

pub fn offline_account(name: &str) -> Result<Account> {
    validate_offline_name(name)?;
    Ok(Account { provider: Provider::Offline, username: name.to_string(), uuid: offline_uuid(name) })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildKind {
    Dev,
    Release,
}

impl BuildKind {
    pub fn current() -> Self {
        if cfg!(debug_assertions) {
            Self::Dev
        } else {
            Self::Release
        }
    }
}

/// Release builds unlock offline accounts only after a licensed Microsoft sign-in on this install.
pub fn offline_allowed(build: BuildKind, licensed_signin_seen: bool) -> bool {
    build == BuildKind::Dev || licensed_signin_seen
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsFile {
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub active: Option<String>,
    #[serde(default)]
    pub licensed_signin_seen: bool,
}

pub struct AccountStore {
    path: PathBuf,
}

impl AccountStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub async fn load(&self) -> Result<AccountsFile> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| LauncherError::disk("The accounts file is damaged").with_detail(e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AccountsFile::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn save(&self, file: &AccountsFile) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(file)
            .map_err(|e| LauncherError::disk("Couldn't save accounts").with_detail(e))?;
        write_atomic(&self.path, &bytes).await
    }

    pub async fn add_offline(&self, name: &str, build: BuildKind) -> Result<Account> {
        let mut file = self.load().await?;
        if !offline_allowed(build, file.licensed_signin_seen) {
            return Err(LauncherError::auth("Offline accounts unlock after you sign in once with a Microsoft account that owns Minecraft")
                .with_fix("Add a Microsoft account first"));
        }
        let account = offline_account(name)?;
        file.accounts.retain(|a| a.uuid != account.uuid);
        file.accounts.push(account.clone());
        file.active = Some(account.uuid.clone());
        self.save(&file).await?;
        Ok(account)
    }

    pub async fn active(&self) -> Result<Option<Account>> {
        let file = self.load().await?;
        Ok(file.active.and_then(|uuid| file.accounts.into_iter().find(|a| a.uuid == uuid)))
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core accounts`
Expected: 5 passed.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(core): offline/dev accounts with release-build gate" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Launch (command building, dry run, process + redacted logs)

**Files:**
- Create: `crates/launcher-core/src/launch/mod.rs`, `crates/launcher-core/src/launch/redact.rs`
- Modify: `crates/launcher-core/src/lib.rs` (add `pub mod launch;`)

**Interfaces:**
- Consumes: `ResolvedVersion` and `classpath` (Task 9), `Session` (Task 11), `resolve_args` and `RuleContext` (Task 6), `LauncherDirs` and `Platform` (Task 1), `Event` and `EventSink`.
- Produces:
  - `launch::LAUNCHER_NAME = "perland-launcher"`.
  - `launch::LaunchSpec<'a> { dirs, resolved, game_dir: &Path, java: &Path, session, memory_mb: u32, platform, launcher_version: &str }`.
  - `launch::LaunchCommand { program: PathBuf, args: Vec<String>, cwd: PathBuf, classpath: Vec<PathBuf> }`.
  - `launch::substitute(template, &HashMap<&str, String>) -> String`, which leaves unknown `${x}` untouched.
  - `launch::build_command(&LaunchSpec) -> Result<LaunchCommand>`.
  - `launch::missing_files(&LaunchCommand) -> Vec<PathBuf>`, the program and classpath entries that don't exist.
  - `launch::GameExit { code: Option<i32> }` and `launch::run(&LaunchCommand, &Redactor, &EventSink, log_file: &Path) -> Result<GameExit>` (async).
  - `launch::Redactor` (`Default`) with `new()`, `for_current_user()`, `.with_home(..)`, `.with_secret(..)` (ignores secrets shorter than 4 characters) and `.redact(&str) -> String`.

Argument order: `-Xmx{memory}M`, then the jvm args (the version's rules-filtered list, or the legacy default of `-Djava.library.path=${natives_directory} -cp ${classpath}`), then the logging argument, then the main class, then the game args (rules-filtered, or legacy `minecraftArguments` split on whitespace).

- [ ] **Step 1: Write the failing tests**

At the bottom of `launch/redact.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_tokens_and_home() {
        let r = Redactor::new().with_secret("eyJhbGciOiJ.secret.token").with_home("C:\\Users\\mmxtu");
        let out = r.redact("token=eyJhbGciOiJ.secret.token dir=C:\\Users\\mmxtu\\AppData");
        assert_eq!(out, "token=<redacted> dir=~\\AppData");
    }

    #[test]
    fn short_secrets_are_ignored() {
        let r = Redactor::new().with_secret("0").with_secret("");
        assert_eq!(r.redact("Loaded 100 chunks in 0.5s"), "Loaded 100 chunks in 0.5s");
    }
}
```

At the bottom of `launch/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accounts::offline_account,
        meta::merge::merge,
        test_support::{version, LINUX_X64, WINDOWS_X64},
    };

    fn build(resolved: &ResolvedVersion, platform: Platform) -> LaunchCommand {
        let dirs = LauncherDirs::new("/pl");
        let session = offline_account("Steve").unwrap().offline_session();
        build_command(&LaunchSpec {
            dirs: &dirs,
            resolved,
            game_dir: Path::new("/pl/instances/x/minecraft"),
            java: Path::new("/pl/java/x/bin/java"),
            session: &session,
            memory_mb: 4096,
            platform,
            launcher_version: "0.1.0",
        })
        .unwrap()
    }

    fn arg_after<'a>(args: &'a [String], flag: &str) -> &'a str {
        let i = args.iter().position(|a| a == flag).unwrap_or_else(|| panic!("{flag} missing"));
        &args[i + 1]
    }

    #[test]
    fn substitute_replaces_known_and_keeps_unknown() {
        let vars = HashMap::from([("a", "1".to_string())]);
        assert_eq!(substitute("x${a}y${b}z", &vars), "x1y${b}z");
        assert_eq!(substitute("open ${a", &vars), "open ${a");
    }

    #[test]
    fn modern_command_on_windows() {
        let resolved = ResolvedVersion { vanilla_id: "1.20.1".into(), json: version("1.20.1.json") };
        let cmd = build(&resolved, WINDOWS_X64);
        let a = &cmd.args;
        assert_eq!(a[0], "-Xmx4096M");
        let cp = arg_after(a, "-cp");
        assert!(cp.contains(';') && cp.ends_with("1.20.1.jar"), "{cp}");
        assert!(a.iter().any(|x| x.starts_with("-Dlog4j.configurationFile=") && x.ends_with("client-1.12.xml")));
        assert!(a.iter().any(|x| x == "-Dminecraft.launcher.brand=perland-launcher"));
        assert!(a.contains(&"net.minecraft.client.main.Main".to_string()));
        assert_eq!(arg_after(a, "--username"), "Steve");
        assert_eq!(arg_after(a, "--uuid"), "5627dd98e6be3c21b8a8e92344183641");
        assert_eq!(arg_after(a, "--assetIndex"), "5");
        assert_eq!(arg_after(a, "--version"), "1.20.1");
        assert!(!a.contains(&"--demo".to_string()));
        assert!(!a.contains(&"-XstartOnFirstThread".to_string()));
        assert!(!a.iter().any(|x| x.contains("${")), "no unresolved templates: {a:?}");
        let main = a.iter().position(|x| x == "net.minecraft.client.main.Main").unwrap();
        assert!(a.iter().position(|x| x == "-cp").unwrap() < main, "jvm args come before the main class");
        assert_eq!(cmd.cwd, Path::new("/pl/instances/x/minecraft"));
        assert_eq!(cmd.classpath.len(), 51);
    }

    #[test]
    fn legacy_command_on_linux() {
        let resolved = ResolvedVersion { vanilla_id: "1.12.2".into(), json: version("1.12.2.json") };
        let cmd = build(&resolved, LINUX_X64);
        assert!(cmd.args.iter().any(|x| x.starts_with("-Djava.library.path=") && x.ends_with("natives")));
        assert!(arg_after(&cmd.args, "-cp").contains(':'));
        assert_eq!(arg_after(&cmd.args, "--username"), "Steve");
        assert_eq!(arg_after(&cmd.args, "--assetIndex"), "1.12");
    }

    #[test]
    fn fabric_command_uses_knot() {
        let resolved = ResolvedVersion {
            vanilla_id: "1.20.1".into(),
            json: merge(version("1.20.1.json"), version("fabric-1.20.1-0.16.14.json")),
        };
        let cmd = build(&resolved, WINDOWS_X64);
        assert!(cmd.args.contains(&"net.fabricmc.loader.impl.launch.knot.KnotClient".to_string()));
        assert!(cmd.args.contains(&"-DFabricMcEmu= net.minecraft.client.main.Main ".to_string()));
        assert_eq!(arg_after(&cmd.args, "--version"), "fabric-loader-0.16.14-1.20.1");
    }

    #[test]
    fn dry_run_reports_missing_files() {
        let tmp = tempfile::tempdir().unwrap();
        let program = tmp.path().join("java");
        let jar = tmp.path().join("a.jar");
        let cmd = LaunchCommand { program: program.clone(), args: vec![], cwd: tmp.path().into(), classpath: vec![jar.clone()] };
        assert_eq!(missing_files(&cmd), vec![program.clone(), jar.clone()]);
        std::fs::write(&program, "").unwrap();
        std::fs::write(&jar, "").unwrap();
        assert!(missing_files(&cmd).is_empty());
    }

    #[tokio::test]
    async fn run_streams_redacted_output() {
        // Uses the java already on PATH (present on CI runners and dev machines); skipped if absent.
        if std::process::Command::new("java").arg("-version").output().is_err() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let l2 = lines.clone();
        let sink: EventSink = std::sync::Arc::new(move |e| {
            if let Event::Log { line } = e {
                l2.lock().unwrap().push(line);
            }
        });
        let cmd = LaunchCommand { program: "java".into(), args: vec!["-version".into()], cwd: tmp.path().join("game"), classpath: vec![] };
        let redactor = Redactor::new().with_secret("version");
        let log = tmp.path().join("logs/latest.log");
        let exit = run(&cmd, &redactor, &sink, &log).await.unwrap();
        assert_eq!(exit.code, Some(0));
        let all = lines.lock().unwrap().join("\n");
        assert!(all.contains("<redacted>"), "{all}");
        assert!(!all.contains("version"), "{all}");
        assert!(std::fs::read_to_string(&log).unwrap().contains("<redacted>"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p launcher-core launch`
Expected: compile errors.

- [ ] **Step 3: Implement**

The top of `launch/redact.rs`:
```rust
/// Scrubs secrets and the user's home path from anything written to logs.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    secrets: Vec<String>,
    home: Option<String>,
}

impl Redactor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_current_user() -> Self {
        let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
        Self::new().with_home(home)
    }

    pub fn with_home(mut self, home: impl Into<String>) -> Self {
        let home = home.into();
        if !home.is_empty() {
            self.home = Some(home);
        }
        self
    }

    /// Secrets shorter than 4 characters (like the offline token "0") are ignored; redacting them would mangle every log line.
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        let secret = secret.into();
        if secret.len() >= 4 {
            self.secrets.push(secret);
        }
        self
    }

    pub fn redact(&self, input: &str) -> String {
        let mut out = input.to_string();
        for secret in &self.secrets {
            out = out.replace(secret.as_str(), "<redacted>");
        }
        if let Some(home) = &self.home {
            out = out.replace(home.as_str(), "~");
        }
        out
    }
}
```

The top of `launch/mod.rs`:
```rust
pub mod redact;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};

pub use redact::Redactor;

use crate::{
    accounts::Session,
    error::{LauncherError, Result},
    events::{Event, EventSink},
    install::{classpath, ResolvedVersion},
    meta::rules::{resolve_args, RuleContext},
    paths::LauncherDirs,
    platform::Platform,
};

pub const LAUNCHER_NAME: &str = "perland-launcher";

pub struct LaunchSpec<'a> {
    pub dirs: &'a LauncherDirs,
    pub resolved: &'a ResolvedVersion,
    pub game_dir: &'a Path,
    pub java: &'a Path,
    pub session: &'a Session,
    pub memory_mb: u32,
    pub platform: Platform,
    pub launcher_version: &'a str,
}

#[derive(Debug, Clone)]
pub struct LaunchCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub classpath: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameExit {
    pub code: Option<i32>,
}

/// Replace `${name}` with values from `vars`; unknown names are left as written.
pub fn substitute(template: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match vars.get(key) {
                    Some(value) => out.push_str(value),
                    None => {
                        out.push_str("${");
                        out.push_str(key);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

pub fn build_command(spec: &LaunchSpec) -> Result<LaunchCommand> {
    let ctx = RuleContext::new(spec.platform);
    let json = &spec.resolved.json;
    let classpath = classpath(spec.resolved, spec.dirs, &ctx);
    let cp_string =
        classpath.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join(spec.platform.classpath_separator());
    let assets_index = json.asset_index.as_ref().map(|a| a.id.clone()).or_else(|| json.assets.clone()).unwrap_or_default();
    let s = spec.session;
    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", s.username.clone()),
        ("version_name", json.id.clone()),
        ("game_directory", path_str(spec.game_dir)),
        ("assets_root", path_str(&spec.dirs.assets())),
        ("game_assets", path_str(&spec.dirs.assets())),
        ("assets_index_name", assets_index),
        ("auth_uuid", s.uuid.clone()),
        ("auth_access_token", s.access_token.clone()),
        ("auth_session", s.access_token.clone()),
        ("clientid", String::new()),
        ("auth_xuid", s.xuid.clone().unwrap_or_else(|| "0".to_string())),
        ("user_type", s.user_type.clone()),
        ("user_properties", "{}".to_string()),
        ("version_type", json.kind.clone().unwrap_or_else(|| "release".to_string())),
        ("natives_directory", path_str(&spec.dirs.natives_dir(&json.id))),
        ("launcher_name", LAUNCHER_NAME.to_string()),
        ("launcher_version", spec.launcher_version.to_string()),
        ("classpath", cp_string),
        ("classpath_separator", spec.platform.classpath_separator().to_string()),
        ("library_directory", path_str(&spec.dirs.libraries())),
    ]);

    let mut args = vec![format!("-Xmx{}M", spec.memory_mb)];
    match &json.arguments {
        Some(a) if !a.jvm.is_empty() => args.extend(resolve_args(&a.jvm, &ctx).iter().map(|t| substitute(t, &vars))),
        _ => args.extend(["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"].iter().map(|t| substitute(t, &vars))),
    }
    if let Some(client) = json.logging.as_ref().and_then(|l| l.client.as_ref()) {
        let path_var = HashMap::from([("path", path_str(&spec.dirs.log_config(&client.file.id)))]);
        args.push(substitute(&client.argument, &path_var));
    }
    args.push(json.main_class.clone());
    match (&json.arguments, &json.minecraft_arguments) {
        (Some(a), _) if !a.game.is_empty() => args.extend(resolve_args(&a.game, &ctx).iter().map(|t| substitute(t, &vars))),
        (_, Some(legacy)) => args.extend(legacy.split_whitespace().map(|t| substitute(t, &vars))),
        _ => return Err(LauncherError::loader(format!("{} has no game arguments", json.id))),
    }
    Ok(LaunchCommand { program: spec.java.to_path_buf(), args, cwd: spec.game_dir.to_path_buf(), classpath })
}

/// Files the command needs that aren't on disk. Empty means the launch can proceed.
pub fn missing_files(cmd: &LaunchCommand) -> Vec<PathBuf> {
    std::iter::once(&cmd.program).chain(cmd.classpath.iter()).filter(|p| !p.is_file()).cloned().collect()
}

/// Start the game, stream every output line (redacted) to the sink and the log file, and wait for it to exit.
pub async fn run(cmd: &LaunchCommand, redactor: &Redactor, sink: &EventSink, log_file: &Path) -> Result<GameExit> {
    tokio::fs::create_dir_all(&cmd.cwd).await?;
    if let Some(parent) = log_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut command = tokio::process::Command::new(&cmd.program);
    command.args(&cmd.args).current_dir(&cmd.cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = command.spawn().map_err(|e| {
        LauncherError::java("Java couldn't be started").with_fix("Reinstall Java from the instance settings").with_detail(e)
    })?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let readers: [Box<dyn AsyncRead + Unpin + Send>; 2] = [Box::new(stdout), Box::new(stderr)];
    for reader in readers {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    let mut log = tokio::fs::File::create(log_file).await?;
    while let Some(line) = rx.recv().await {
        let clean = redactor.redact(&line);
        log.write_all(clean.as_bytes()).await?;
        log.write_all(b"\n").await?;
        sink(Event::Log { line: clean });
    }
    log.flush().await?;
    let status = child.wait().await?;
    Ok(GameExit { code: status.code() })
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core launch`
Expected: 8 passed (`run_streams_redacted_output` returns early if `java` isn't on PATH).

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add -A
git commit -m "feat(core): launch command building, dry run, process runner with log redaction" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 13: `perland-cli` harness, nightly real-install CI, manual smoke test

**Files:**
- Create: `crates/perland-cli/Cargo.toml`, `crates/perland-cli/src/main.rs`, `.github/workflows/nightly.yml`
- Modify: `Cargo.toml` (`members = ["crates/launcher-core", "crates/perland-cli"]`)

**Interfaces:**
- Consumes: all public `launcher-core` APIs from Tasks 1–12.
- Produces: the binary `perland-cli [--root DIR] <create|list|offline|launch>`.

- [ ] **Step 1: Create the crate**

`crates/perland-cli/Cargo.toml`:
```toml
[package]
name = "perland-cli"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[dependencies]
launcher-core = { path = "../launcher-core" }
tokio.workspace = true
reqwest.workspace = true
clap.workspace = true
```

`crates/perland-cli/src/main.rs`:
```rust
//! Developer harness for launcher-core: create instances, add offline accounts, install and launch.
use std::{path::PathBuf, process::ExitCode, sync::Arc, time::Duration};

use clap::{Parser, Subcommand, ValueEnum};
use launcher_core::{
    accounts::{AccountStore, BuildKind, Provider},
    download::Downloader,
    events::{Event, EventSink},
    install::Installer,
    instances::{GameSpec, InstanceStore, LoaderSpec},
    java,
    launch::{self, LaunchSpec, Redactor},
    meta::{loaders::LoaderKind, MetaClient},
    paths::LauncherDirs,
    platform::Platform,
    LauncherError,
};

#[derive(Parser)]
#[command(name = "perland-cli", about = "Developer harness for the Perland launcher core")]
struct Cli {
    /// Data folder (defaults to the launcher's normal data folder)
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create an instance and print its id
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        mc: String,
        #[arg(long, value_enum)]
        loader: Option<CliLoader>,
        #[arg(long)]
        loader_version: Option<String>,
    },
    /// List instances
    List,
    /// Add an offline account and make it active
    Offline { name: String },
    /// Install (if needed) and launch an instance with the active account
    Launch {
        #[arg(long)]
        instance: String,
        /// Build the command and check every file exists, without starting the game
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum CliLoader {
    Fabric,
    Quilt,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error [{:?}]: {}", e.category, e.message);
            if let Some(fix) = &e.fix {
                eprintln!("  fix: {fix}");
            }
            if let Some(detail) = &e.detail {
                eprintln!("  detail: {detail}");
            }
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), LauncherError> {
    let root = cli
        .root
        .or_else(LauncherDirs::default_root)
        .ok_or_else(|| LauncherError::disk("Couldn't find a data folder").with_fix("Pass --root <folder>"))?;
    let dirs = LauncherDirs::new(root);
    let store = InstanceStore::new(dirs.instances());
    let accounts = AccountStore::new(dirs.root().join("accounts.json"));
    match cli.cmd {
        Cmd::Create { name, mc, loader, loader_version } => {
            let loader = match (loader, loader_version) {
                (Some(kind), Some(version)) => Some(LoaderSpec {
                    kind: match kind {
                        CliLoader::Fabric => LoaderKind::Fabric,
                        CliLoader::Quilt => LoaderKind::Quilt,
                    },
                    version,
                }),
                (None, None) => None,
                _ => return Err(LauncherError::loader("Pass both --loader and --loader-version")),
            };
            let instance = store.create(&name, GameSpec { minecraft: mc, loader }).await?;
            println!("{}", instance.id);
        }
        Cmd::List => {
            for i in store.list().await? {
                println!("{}\t{}\t{}", i.id, i.name, i.game.minecraft);
            }
        }
        Cmd::Offline { name } => {
            let account = accounts.add_offline(&name, BuildKind::current()).await?;
            println!("{} {}", account.username, account.uuid);
        }
        Cmd::Launch { instance, dry_run } => launch_instance(&dirs, &store, &accounts, &instance, dry_run).await?,
    }
    Ok(())
}

async fn launch_instance(
    dirs: &LauncherDirs,
    store: &InstanceStore,
    accounts: &AccountStore,
    id: &str,
    dry_run: bool,
) -> Result<(), LauncherError> {
    let instance = store.load(id).await?;
    let account = accounts
        .active()
        .await?
        .ok_or_else(|| LauncherError::auth("No account selected").with_fix("Run `perland-cli offline <name>` first"))?;
    if account.provider != Provider::Offline {
        return Err(LauncherError::auth("Microsoft accounts arrive in a later build"));
    }
    let session = account.offline_session();
    let http = reqwest::Client::builder()
        .user_agent(concat!("PerlandLauncher/", env!("CARGO_PKG_VERSION"), " (+https://github.com/PerlandDev/perland-launcher)"))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| LauncherError::network("Couldn't start the HTTP client").with_detail(e))?;
    let platform = Platform::current();
    let installer = Installer {
        dirs: dirs.clone(),
        meta: MetaClient::new(http.clone(), dirs.meta()),
        downloader: Downloader::new(http),
        platform,
    };
    let sink: EventSink = Arc::new(|event| match event {
        Event::Progress { task, done, total, .. } => eprint!("\r{task}: {done}/{total}    "),
        Event::Log { line } => println!("{line}"),
        Event::StateChanged { .. } => {}
    });

    let loader = instance.game.loader.as_ref().map(|l| (l.kind, l.version.as_str()));
    let resolved = installer.resolve(&instance.game.minecraft, loader).await?;
    installer.install(&resolved, &sink).await?;
    eprintln!();
    let java_path = match &instance.java.path {
        Some(path) => path.clone(),
        None => {
            let component = java::component_for(&resolved.json);
            java::ensure_runtime(&installer.meta, &installer.downloader, &dirs.java(), platform, &component, &sink).await?
        }
    };
    eprintln!();

    let game_dir = store.game_dir(&instance.id);
    let cmd = launch::build_command(&LaunchSpec {
        dirs,
        resolved: &resolved,
        game_dir: &game_dir,
        java: &java_path,
        session: &session,
        memory_mb: instance.java.memory_mb,
        platform,
        launcher_version: env!("CARGO_PKG_VERSION"),
    })?;
    let redactor = Redactor::for_current_user().with_secret(session.access_token.clone());

    if dry_run {
        println!("{}", redactor.redact(&format!("{} {}", cmd.program.display(), cmd.args.join(" "))));
        let missing = launch::missing_files(&cmd);
        if missing.is_empty() {
            eprintln!("dry run OK: java and {} classpath entries present", cmd.classpath.len());
            return Ok(());
        }
        for path in &missing {
            eprintln!("missing: {}", path.display());
        }
        return Err(LauncherError::loader(format!("{} required files are missing", missing.len())));
    }

    let exit = launch::run(&cmd, &redactor, &sink, &dirs.logs().join(format!("{}-latest.log", instance.id))).await?;
    eprintln!("game exited with {:?}", exit.code);
    Ok(())
}
```

Then edit the root `Cargo.toml` to `members = ["crates/launcher-core", "crates/perland-cli"]`.

- [ ] **Step 2: Build and run the CLI's offline commands**

Run: `cargo build -p perland-cli && cargo run -q -p perland-cli -- --root target/pl-smoke offline Steve && cargo run -q -p perland-cli -- --root target/pl-smoke create --name "Smoke Fabric" --mc 1.20.1 --loader fabric --loader-version 0.16.14 && cargo run -q -p perland-cli -- --root target/pl-smoke list`
Expected: `Steve 5627dd98-e6be-3c21-b8a8-e92344183641`, then `smoke-fabric`, then a tab-separated line `smoke-fabric	Smoke Fabric	1.20.1`.

- [ ] **Step 3: Add the nightly real-install workflow**

`.github/workflows/nightly.yml`:
```yaml
name: Nightly
on:
  schedule:
    - cron: "0 4 * * *"
  workflow_dispatch:

jobs:
  real-install:
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, ubuntu-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install Fabric 1.20.1 + Mojang Java, then launch dry-run
        shell: bash
        run: |
          ROOT="$RUNNER_TEMP/perland"
          cargo build -q -p perland-cli
          cargo run -q -p perland-cli -- --root "$ROOT" offline CiBot
          ID=$(cargo run -q -p perland-cli -- --root "$ROOT" create --name "CI Fabric" --mc 1.20.1 --loader fabric --loader-version 0.16.14)
          cargo run -q -p perland-cli -- --root "$ROOT" launch --instance "$ID" --dry-run
      - name: Second run is offline-capable and skips downloads
        shell: bash
        run: |
          ROOT="$RUNNER_TEMP/perland"
          ID=$(cargo run -q -p perland-cli -- --root "$ROOT" list | cut -f1 | head -n1)
          time cargo run -q -p perland-cli -- --root "$ROOT" launch --instance "$ID" --dry-run
```

- [ ] **Step 4: Manual smoke test on the Windows PC** (a normal download of about 700 MB, not a stress test)

Run: `cargo run -q -p perland-cli -- --root target/pl-smoke launch --instance smoke-fabric --dry-run`
Expected: progress lines, then `dry run OK: java and 59 classpath entries present`.

Then run it for real: `cargo run -q -p perland-cli -- --root target/pl-smoke launch --instance smoke-fabric`
Expected: the Minecraft 1.20.1 window opens to the title screen, and the streamed log contains `Loading Minecraft 1.20.1 with Fabric Loader 0.16.14`. Log lines stream to the terminal. Closing the game prints `game exited with Some(0)`. Check that `target/pl-smoke/logs/smoke-fabric-latest.log` contains no `C:\Users\<name>` paths.

- [ ] **Step 5: Run the full checks, then commit and push**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add -A
git commit -m "feat(cli): perland-cli dev harness and nightly real-install CI" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git push
```
Expected: the CI workflow passes on windows-latest and ubuntu-latest. Trigger `Nightly` once by hand with `gh workflow run nightly.yml` and confirm both jobs pass.
