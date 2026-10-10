# Desktop App (Plan 5 of 6, pulled forward) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Windows-first Tauri 2 desktop app over `launcher-core`, in English and Spanish:
- vanilla, Fabric and Quilt instances
- offline accounts
- Play with progress, logs, Stop and a crash card
- a local mods list
- settings and a first-run flow

It is styled in the "Arcade Cabinet" world inside the owner's pinned game-forward layout.

**Architecture:**
- **`launcher-core`** gains error codes, translatable progress tasks, a killable `GameProcess`, a shared `play` module, a metadata catalog, store operations, settings and local mods.
- **`launcher-app`** (new, plain Rust, no Tauri) owns all app state and the run state machine behind two traits:
  - `Runner`: prepare and start a game
  - `Host`: emit events, minimize and restore the window

  It is fully unit-tested with fakes, and it emits the JSON contract fixtures the UI checks.
- **`src-tauri`** is thin wiring: plugins, commands, the window close guard.
- **`ui/`** is React 19 + TypeScript + Vite + Tailwind 4, with zod schemas mirroring the Rust DTOs. A browser mock mode lets screens be built and styled without Tauri.

**Tech Stack:**
- **Rust:** stable edition 2021 (toolchain 1.99 on the dev PC), tokio, serde, Tauri 2.12 plus the dialog, opener, single-instance and window-state plugins, sysinfo.
- **Node:** 24, pnpm 11.
- **UI:**
  - React 19.3, Vite 8, Tailwind 4.3, TypeScript 6.0 (typescript-eslint doesn't support TS 7 yet)
  - Vitest 5, Testing Library, ESLint 10
  - TanStack Query 5, TanStack Virtual 3, i18next 26 and react-i18next 17, zod 4
  - pixelarticons icons; Bungee and Atkinson Hyperlegible Next/Mono fonts via Fontsource

**Spec:** `docs/superpowers/specs/2026-10-10-launcher-desktop-ui-design.md`. Product context: `PRODUCT.md`. Visual direction contract: `.impeccable/surfaces/ui-src-app-tsx.md`.

**Deliberate deviation from the spec:** spec §5.2 places `AppCore` inside `src-tauri`. This plan moves it into its own crate, `crates/launcher-app`. Test executables that link Tauri on Windows can fail to start (the common-controls manifest is only embedded for the app binary), so every logic and contract test must live in a crate with no Tauri dependency. `src-tauri` keeps `test = false` and holds wiring only. Behavior is identical to the spec.

## Global Constraints

- License **GPL-3.0-only**. No Modrinth branding. No Mojang artwork, skins, logos or fonts.
- `launcher-core` and `launcher-app` must not depend on Tauri or any UI crate.
- Rust stable, edition 2021. Must build and pass on **Windows x64 and Linux x64**.
- Data folder: `%APPDATA%\PerlandLauncher` (or `$XDG_DATA_HOME/PerlandLauncher` → `~/.local/share/PerlandLauncher`). The env var `PERLAND_LAUNCHER_ROOT` overrides it for development.
- **Errors and text:**
  - Every player-facing failure is a `LauncherError` with a `category`, a stable `code` (`ErrorCode`, snake_case), an English `message`, an optional `fix`, an optional technical `detail`, and `params`.
  - Tokens never reach logs. Game output goes through `Redactor`.
  - Every UI string exists in both `ui/src/i18n/en.json` and `ui/src/i18n/es.json`. Spanish is neutral Latin American with informal *tú*.
- **Not built in this plan (never fake it):** Microsoft sign-in (Plan 4), Forge/NeoForge (Plan 2), packs/Browse/sync/`perland://` (Plan 3), the updater and installers (Plan 6). Microsoft sign-in and the pack link and `.mrpack` tiles appear as disabled "coming soon". Everything else is absent.
- **Release builds** keep the offline-account gate (`BuildKind::current()`).
- **Heavy tests (fuzzing, stress, load) never run on this PC.** Local test inputs stay tiny.
- **Before every commit:**
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - once `ui/` exists, `pnpm -r lint && pnpm -r typecheck && pnpm -r test`
- Every commit message ends with the trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Machine notes:**
  - If `cargo` isn't on PATH, prepend `%USERPROFILE%\.cargo\bin`.
  - Windows Smart App Control may block a freshly built unsigned test exe (os error 4551). That's environmental: rerun or rebuild.
- **UI styling** follows the Arcade Cabinet direction contract.
  - Before editing any file under `ui/src/ui/`, `ui/src/features/` or `ui/src/styles/`, read `C:\Users\mmxtu\.claude\skills\impeccable\reference\craft-floor.md`.
  - Pixel motifs stay within the land-plot mark's stepped geometry: no pixel fonts for body text, no retro costume.

## Review Focus

These are inputs a real player will hit that no other test would catch. Each one is pinned by a test in the owning task.

1. **Quilt's loader list arrives unordered, with betas first** (`meta.quiltmc.org` returns `0.20.0-beta.9` before `0.24.0`). The picker must default to the newest *stable* loader. Pinned by `quilt_versions_sorted_newest_first` (Task 5).
2. **A very long instance name, created twice.** The slug is capped at 48 characters, and the second copy gets `-2`. That id must still pass `is_valid_instance_id`, or the second instance becomes unloadable. Pinned by `long_names_cap_slug_and_suffix_stays_valid` (Task 6).
3. **A mod that prints one enormous line with no newline** (a binary dump). Memory must not grow without bound, the line is truncated at a character boundary, and the next line still arrives. Pinned by `capped_line_drops_the_overflow` (Task 3).
4. **Drag-dropped mod files with odd names.** `Sodium.JAR` (uppercase extension) is accepted. Re-adding a mod that is currently disabled (`x.jar.disabled` exists) is refused rather than creating an on/off twin. Pinned by `add_accepts_uppercase_jar_and_refuses_disabled_twin` (Task 9).
5. **Mojang adds a new version `type`** (anything beyond release, snapshot, old_beta, old_alpha). The whole version list must still load, skipping the unknown kind. Pinned by `unknown_version_kind_is_skipped` (Task 5).

## File Structure

```
Cargo.toml                         workspace: + crates/launcher-app, src-tauri; + sysinfo, tauri deps
package.json, pnpm-workspace.yaml  pnpm workspace root (ui), @tauri-apps/cli
crates/launcher-core/src/
  error.rs        ErrorCode (macro-built with ALL), LauncherError.code/params        [modify]
  events.rs       TaskId, Event::Progress{task: TaskId}, Event::Warning              [modify]
  launch/mod.rs   spawn → GameProcess/GameKiller/GameExit, capped lines, tail        [modify]
  launch/crash.rs CrashHint, diagnose(tail)                                          [create]
  play.rs         Prepared, prepare, custom_java, StartSpec, command, check_files, start [create]
  java.rs         + probe(path)                                                       [modify]
  meta/catalog.rs McVersion, LoaderVersion, minecraft_versions, loader_versions, compare_versions [create]
  instances.rs    capped slugs, validation, list_all, update, rename, delete          [modify]
  accounts.rs     list, set_active, remove                                            [modify]
  settings.rs     Lang, Settings, SettingsStore                                       [create]
  mods.rs         LocalMod, list, set_enabled, add, remove                            [create]
crates/perland-cli/src/main.rs     uses play::* and spawn/wait                        [modify]
crates/launcher-app/
  src/lib.rs      module wiring + re-exports
  src/events.rs   AppEvent, RunStateKind, ExitInfo, Host, ProgressThrottle
  src/logs.rs     LogStore, spawn_batcher
  src/dto.rs      AppInfo, Build, NewInstance, OpenTarget, RunSnapshot, JavaTest
  src/runner.rs   Kill, RunningGame, Runner, CoreRunner
  src/app.rs      AppConfig, AppCore (+ non-run commands)
  src/runs.rs     play/stop/stop_all/run_states/log_tail state machine
  src/testing.rs  FakeRunner, RecordingHost (cfg(test))
  tests/contract.rs  writes/compares ui/src/ipc/contract/*.json
src-tauri/
  Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/*
  src/main.rs     builder, plugins, setup, close guard
  src/commands.rs #[tauri::command] wrappers
  src/host.rs     TauriHost (emit/minimize/restore), system_memory_mb
ui/
  package.json, vite.config.ts, tsconfig.json, eslint.config.js, index.html
  src/main.tsx, src/App.tsx, src/test/setup.ts
  src/assets/mark.svg
  src/styles/app.css           tokens (@theme), components layer, browser surfaces
  src/ipc/schema.ts            zod schemas → TS types
  src/ipc/commands.ts          typed invoke wrappers (only invoke callers)
  src/ipc/events.ts            listen + decode AppEvent, close-requested
  src/ipc/errors.ts            describeError, taskLabel
  src/ipc/mock.ts              browser mock mode (mockIPC + scripted play)
  src/ipc/contract/*.json      Rust-emitted fixtures (do not hand-edit)
  src/i18n/index.ts, en.json, es.json
  src/state/queries.ts, runStore.ts, route.ts, toasts.ts
  src/art/random.ts, marquee.tsx, plotIcon.tsx, avatar.tsx
  src/ui/Icon.ts, Button.tsx, PlayButton.tsx, Meter.tsx, Dialog.tsx, Toggle.tsx, Field.tsx,
         Select.tsx, Menu.tsx, Toasts.tsx, ErrorCard.tsx, ComingSoon.tsx, format.ts
  src/features/titlebar, rail, home, activity, new-instance, instance-settings, mods,
               accounts, settings, first-run, close-guard (one folder each, Component + test)
.github/workflows/ci.yml           + pnpm, ui build/lint/typecheck/test, webkit deps
CLAUDE.md, AGENTS.md, NOTICE, README.md                                              [modify]
```

---

## Phase 1: Core (`launcher-core`)

### Task 1: Error codes on every `LauncherError`

**Files:**
- Modify: `crates/launcher-core/src/error.rs` (whole file)
- Modify: `crates/launcher-core/src/lib.rs` (re-export)
- Modify (call sites): `accounts.rs`, `download/mod.rs`, `http.rs`, `install/mod.rs`, `install/natives.rs`, `instances.rs`, `java.rs`, `launch/mod.rs`, `meta/loaders.rs`, `meta/mod.rs` (all under `crates/launcher-core/src/`)

**Interfaces:**
- Produces: `launcher_core::ErrorCode` (`Copy`, `Serialize` snake_case, `ErrorCode::ALL: &[ErrorCode]`); `LauncherError { category, code, message, fix, detail, params: BTreeMap<String,String> }`; constructors `LauncherError::{network,auth,integrity,disk,java,loader,pack,game}(code: ErrorCode, message)`; `LauncherError::new(category, code, message)`; `.with_param(key: &str, value: impl Display)`.

- [ ] **Step 1: Write the failing tests**

Replace the `#[cfg(test)] mod tests` block in `crates/launcher-core/src/error.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn builders_fill_fix_detail_and_params() {
        let e = LauncherError::network(ErrorCode::HostUnreachable, "Couldn't reach Mojang")
            .with_fix("Check your connection")
            .with_detail("dns error")
            .with_param("host", "piston-meta.mojang.com");
        assert_eq!(e.category, ErrorCategory::Network);
        assert_eq!(e.code, ErrorCode::HostUnreachable);
        assert_eq!(e.to_string(), "Couldn't reach Mojang");
        assert_eq!(e.fix.as_deref(), Some("Check your connection"));
        assert_eq!(e.detail.as_deref(), Some("dns error"));
        assert_eq!(
            e.params.get("host").map(String::as_str),
            Some("piston-meta.mojang.com")
        );
    }

    #[test]
    fn io_errors_become_disk_errors() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let e: LauncherError = io.into();
        assert_eq!(e.category, ErrorCategory::Disk);
        assert_eq!(e.code, ErrorCode::DiskIo);
        assert!(e.fix.is_some());
        assert!(e.detail.unwrap().contains("denied"));
    }

    #[test]
    fn serializes_code_and_params() {
        let json = serde_json::to_value(
            LauncherError::java(ErrorCode::JavaStartFailed, "Java couldn't be started")
                .with_param("version", "1.20.1"),
        )
        .unwrap();
        assert_eq!(json["category"], "java");
        assert_eq!(json["code"], "java_start_failed");
        assert_eq!(json["params"]["version"], "1.20.1");
        assert!(json["fix"].is_null());
        assert!(json["detail"].is_null());
    }

    #[test]
    fn every_code_is_unique_snake_case() {
        let mut seen = HashSet::new();
        for code in ErrorCode::ALL {
            let s = serde_json::to_value(code).unwrap();
            let s = s.as_str().unwrap().to_string();
            assert!(
                s.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{s}"
            );
            assert!(seen.insert(s.clone()), "duplicate {s}");
        }
        assert!(ErrorCode::ALL.contains(&ErrorCode::Internal));
        assert!(ErrorCode::ALL.len() >= 50);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher-core --lib error::`
Expected: compile errors (`ErrorCode` not found, `with_param` not found).

- [ ] **Step 3: Implement `error.rs`**

Replace everything above the test module in `crates/launcher-core/src/error.rs` with:

```rust
use std::{collections::BTreeMap, fmt::Display};

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

macro_rules! error_codes {
    ($($variant:ident),* $(,)?) => {
        /// A stable name for every player-facing failure. The UI translates by this code;
        /// `LauncherError::message` is only the English fallback.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ErrorCode {
            $($variant),*
        }

        impl ErrorCode {
            /// Every code in declaration order. Built by the same macro as the enum, so it can't miss one.
            pub const ALL: &'static [ErrorCode] = &[$(ErrorCode::$variant),*];
        }
    };
}

error_codes! {
    // accounts
    OfflineNameInvalid, AccountsDamaged, OfflineLocked, MicrosoftUnavailable, NoAccount,
    // disk
    DiskIo, DiskWriteFailed,
    // network
    DownloadNoUrl, DownloadCorrupt, HostUnreachable, DownloadRestarted, HostTrouble,
    DownloadRejected, ConnectionDropped, HttpClientFailed, Offline,
    // game files
    UnsafeGameData, LibraryUnverifiable, NoClientDownload, MinecraftVersionNotFound,
    MojangMismatch, NoAssetIndex, NativesExtractFailed, VersionInUse, ArchiveInvalid,
    // instances
    InstanceNotFound, InstanceDamaged, InstanceIdInvalid, InstanceBusy, AlreadyRunning,
    NameInvalid, MemoryOutOfRange,
    // java
    JavaUnavailable, JavaStartFailed, JavaPathMissing, JavaTestFailed,
    // launch
    NoGameArguments, GameFilesMissing,
    // loaders and metadata
    VersionInvalid, LoaderUnsupported, LoaderVersionNotFound, MetaCacheMissing, MetaNotFound,
    MetaChecksum, MetaUnexpected,
    // settings and logs
    SettingsDamaged, LogWriteStopped,
    // mods
    ModNotJar, ModExists, ModNotFound, ModNameInvalid,
    // anything unexpected
    Internal,
}

/// A failure a player can read: a plain message, an optional fix, and technical detail behind a toggle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct LauncherError {
    pub category: ErrorCategory,
    pub code: ErrorCode,
    pub message: String,
    pub fix: Option<String>,
    pub detail: Option<String>,
    /// Values the translated message interpolates, e.g. `{"version": "1.20.1"}`.
    pub params: BTreeMap<String, String>,
}

pub type Result<T> = std::result::Result<T, LauncherError>;

impl LauncherError {
    pub fn new(category: ErrorCategory, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            category,
            code,
            message: message.into(),
            fix: None,
            detail: None,
            params: BTreeMap::new(),
        }
    }
    pub fn network(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Network, code, message)
    }
    pub fn auth(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Auth, code, message)
    }
    pub fn integrity(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Integrity, code, message)
    }
    pub fn disk(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Disk, code, message)
    }
    pub fn java(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Java, code, message)
    }
    pub fn loader(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Loader, code, message)
    }
    pub fn pack(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Pack, code, message)
    }
    pub fn game(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Game, code, message)
    }
    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }
    pub fn with_detail(mut self, detail: impl Display) -> Self {
        self.detail = Some(detail.to_string());
        self
    }
    pub fn with_param(mut self, key: &str, value: impl Display) -> Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }
}

impl From<std::io::Error> for LauncherError {
    fn from(e: std::io::Error) -> Self {
        LauncherError::disk(ErrorCode::DiskIo, "Couldn't read or write a launcher file")
            .with_fix("Check free disk space and that the launcher folder isn't read-only")
            .with_detail(e)
    }
}
```

In `crates/launcher-core/src/lib.rs`, change the re-export line to:

```rust
pub use error::{ErrorCategory, ErrorCode, LauncherError, Result};
```

- [ ] **Step 4: Migrate every call site**

Add `ErrorCode` to each file's `use crate::error::{...}` (or `use crate::{..., ErrorCode}`) import, then replace each constructor exactly as listed. Keep every existing `.with_fix(...)` and `.with_detail(...)` as it is. Only the constructor's arguments change, and the listed `.with_param` calls are added.

| Site (`crates/launcher-core/src/`) | New constructor |
|---|---|
| `accounts.rs` `validate_offline_name` | `LauncherError::auth(ErrorCode::OfflineNameInvalid, "Offline names must be 3–16 letters, numbers or underscores")` |
| `accounts.rs` `load` (damaged) | `LauncherError::disk(ErrorCode::AccountsDamaged, "The accounts file is damaged")` |
| `accounts.rs` `save` | `LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save accounts").with_param("item", "accounts")` |
| `accounts.rs` `add_offline` gate | `LauncherError::auth(ErrorCode::OfflineLocked, "Offline accounts unlock after you sign in once with a Microsoft account that owns Minecraft")` |
| `download/mod.rs` `last_err` init | `LauncherError::network(ErrorCode::DownloadNoUrl, "No download address was given for this file")` |
| `download/mod.rs` verify map_err | `LauncherError::disk(ErrorCode::DiskIo, "Couldn't check a downloaded file")` |
| `download/mod.rs` damaged | `LauncherError::integrity(ErrorCode::DownloadCorrupt, format!("{} was damaged while downloading", file_name(&req.dest))).with_param("file", file_name(&req.dest))` |
| `download/mod.rs` send error | `LauncherError::network(ErrorCode::HostUnreachable, format!("Couldn't reach {}", host_of(url))).with_param("host", host_of(url))` |
| `download/mod.rs` 416 | `LauncherError::network(ErrorCode::DownloadRestarted, "The download had to restart")` |
| `download/mod.rs` 429/5xx | `LauncherError::network(ErrorCode::HostTrouble, format!("{} is having trouble ({status})", host_of(url))).with_param("host", host_of(url)).with_param("status", status)` |
| `download/mod.rs` other status | `LauncherError::network(ErrorCode::DownloadRejected, format!("{url} answered {status}")).with_param("host", host_of(url)).with_param("status", status)` |
| `download/mod.rs` chunk error | `LauncherError::network(ErrorCode::ConnectionDropped, "The connection dropped during a download")` |
| `http.rs` `client` | `LauncherError::network(ErrorCode::HttpClientFailed, "Couldn't start the HTTP client")` |
| `install/mod.rs` `join_remote` | `LauncherError::integrity(ErrorCode::UnsafeGameData, "Minecraft's file list contains an unsafe path")` |
| `install/mod.rs` `check_version_id` | `LauncherError::integrity(ErrorCode::UnsafeGameData, "Minecraft's version data contains an unsafe version name")` |
| `install/mod.rs` `published_sha1` | `LauncherError::integrity(ErrorCode::LibraryUnverifiable, "Couldn't verify a library download")` |
| `install/mod.rs` `maven_path` | `LauncherError::integrity(ErrorCode::UnsafeGameData, "Minecraft's file list contains an unsafe path")` |
| `install/mod.rs` `plan_client` | `LauncherError::loader(ErrorCode::NoClientDownload, format!("Minecraft {} has no client download", resolved.vanilla_id)).with_param("version", &resolved.vanilla_id)` |
| `install/mod.rs` `resolve` not found | `LauncherError::loader(ErrorCode::MinecraftVersionNotFound, format!("Minecraft {mc} doesn't exist")).with_param("version", mc)` |
| `install/mod.rs` `resolve` mismatch | `LauncherError::integrity(ErrorCode::MojangMismatch, "Mojang returned a different version than requested")` |
| `install/mod.rs` no asset index | `LauncherError::loader(ErrorCode::NoAssetIndex, format!("{} has no asset index", json.id)).with_param("version", &json.id)` |
| `install/mod.rs` natives join | `LauncherError::disk(ErrorCode::NativesExtractFailed, "Couldn't extract native libraries")` |
| `install/mod.rs` version save | `LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save the version file").with_param("item", "version")` |
| `install/natives.rs` in use | `LauncherError::game(ErrorCode::VersionInUse, "Another copy of this Minecraft version is running")` |
| `install/natives.rs` `extract_one` | `LauncherError::integrity(ErrorCode::ArchiveInvalid, format!("{} isn't a valid archive", jar.display())).with_param("file", jar.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())` |
| `instances.rs` `save` | `LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save the instance").with_param("item", "instance")` |
| `instances.rs` `load` missing | `LauncherError::disk(ErrorCode::InstanceNotFound, format!("Instance \"{id}\" wasn't found")).with_param("id", id)` |
| `instances.rs` `load` damaged | `LauncherError::disk(ErrorCode::InstanceDamaged, format!("The settings file of \"{id}\" is damaged")).with_param("id", id)` |
| `instances.rs` `check_id` | `LauncherError::disk(ErrorCode::InstanceIdInvalid, "That instance id isn't valid")` |
| `java.rs` `select_runtime` | `LauncherError::java(ErrorCode::JavaUnavailable, format!("There's no official Java ({component}) for this computer")).with_param("component", component)` |
| `java.rs` `plan_runtime` | `LauncherError::integrity(ErrorCode::UnsafeGameData, "The Java download list contains an unsafe path")` |
| `launch/mod.rs` no game args | `LauncherError::loader(ErrorCode::NoGameArguments, format!("{} has no game arguments", json.id)).with_param("version", &json.id)` |
| `launch/mod.rs` spawn failure | `LauncherError::java(ErrorCode::JavaStartFailed, "Java couldn't be started")` |
| `meta/loaders.rs` invalid value | `LauncherError::loader(ErrorCode::VersionInvalid, format!("\"{value}\" isn't a valid version")).with_param("value", value)` |
| `meta/loaders.rs` unsupported | `LauncherError::loader(ErrorCode::LoaderUnsupported, format!("{kind:?} isn't supported yet")).with_param("loader", format!("{kind:?}"))` |
| `meta/loaders.rs` `map_profile_error` | `LauncherError::loader(ErrorCode::LoaderVersionNotFound, format!("Couldn't get {kind:?} {loader} for Minecraft {mc}")).with_param("loader", format!("{kind:?}")).with_param("loader_version", loader).with_param("version", mc)` |
| `meta/loaders.rs` test `offline` | `LauncherError::network(ErrorCode::Offline, "You seem to be offline")` |
| `meta/loaders.rs` test `missing` | `LauncherError::loader(ErrorCode::MetaNotFound, "https://x/profile/json answered 404 Not Found")` |
| `meta/mod.rs` 304 without cache | `LauncherError::network(ErrorCode::MetaCacheMissing, format!("{url} said nothing changed, but there's no saved copy"))` |
| `meta/mod.rs` body interrupted | `LauncherError::network(ErrorCode::ConnectionDropped, format!("Downloading {url} was interrupted"))` |
| `meta/mod.rs` 4xx | `LauncherError::loader(ErrorCode::MetaNotFound, message).with_param("status", status.as_u16())` |
| `meta/mod.rs` 5xx | `LauncherError::network(ErrorCode::HostTrouble, message).with_param("host", host_of(url)).with_param("status", status.as_u16())` |
| `meta/mod.rs` offline | `LauncherError::network(ErrorCode::Offline, "You seem to be offline, and this hasn't been downloaded before")` |
| `meta/mod.rs` checksum | `LauncherError::integrity(ErrorCode::MetaChecksum, format!("Data from {url} didn't match its checksum"))` |
| `meta/mod.rs` `parse_json` | `LauncherError::loader(ErrorCode::MetaUnexpected, format!("Unexpected data from {url}"))` |

`meta/mod.rs` has no `host_of` yet. Add this private helper at the bottom of the file (outside `impl`):

```rust
fn host_of(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
}
```

- [ ] **Step 5: Run the whole core suite**

Run: `cargo test -p launcher-core`
Expected: all tests pass, including the four new `error::tests`. If a site was missed, the compiler names it: fix it from the table.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/launcher-core
git commit -m "feat(core): stable error codes and params on every LauncherError

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Translatable progress tasks and warning events

**Files:**
- Modify: `crates/launcher-core/src/events.rs`
- Modify: `crates/launcher-core/src/download/mod.rs` (`fetch_all`)
- Modify: `crates/launcher-core/src/install/mod.rs` (`install` → `fetch_all` call)
- Modify: `crates/launcher-core/src/java.rs` (`ensure_runtime` → `fetch_all` call)
- Modify: `crates/launcher-core/tests/download.rs` (`fetch_all_dedupes_destinations`)
- Modify: `crates/perland-cli/src/main.rs` (progress printing)

**Interfaces:**
- Produces: `launcher_core::events::TaskId { key: String, params: BTreeMap<String,String> }` with `TaskId::new(key)` and `.with(key, value)`; `Event::Progress { task: TaskId, done, total, bytes }`; `Event::Warning(LauncherError)`; `Downloader::fetch_all(reqs, task: &TaskId, sink)`.
- Task keys used by core: `install_minecraft` `{version}`, `download_java` `{version}`.

- [ ] **Step 1: Write the failing tests**

Replace the test module in `crates/launcher-core/src/events.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ErrorCode, LauncherError};
    use std::sync::Mutex;

    #[test]
    fn sink_receives_events_and_serializes_tagged() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        let sink: EventSink = Arc::new(move |e| s2.lock().unwrap().push(e));
        sink(Event::Progress {
            task: TaskId::new("install_minecraft").with("version", "1.20.1"),
            done: 1,
            total: 2,
            bytes: 3,
        });
        assert_eq!(seen.lock().unwrap().len(), 1);
        let json = serde_json::to_value(&seen.lock().unwrap()[0]).unwrap();
        assert_eq!(json["type"], "progress");
        assert_eq!(json["task"]["key"], "install_minecraft");
        assert_eq!(json["task"]["params"]["version"], "1.20.1");
        noop_sink()(Event::Log { line: "x".into() });
    }

    #[test]
    fn warning_carries_a_launcher_error() {
        let e = Event::Warning(LauncherError::disk(ErrorCode::LogWriteStopped, "stopped"));
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["type"], "warning");
        assert_eq!(json["code"], "log_write_stopped");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher-core --lib events::`
Expected: compile error, `TaskId` not found.

- [ ] **Step 3: Implement `events.rs`**

Replace everything above the tests with:

```rust
use std::{collections::BTreeMap, fmt::Display, sync::Arc};

use serde::Serialize;

use crate::LauncherError;

/// A progress task the UI can translate: a stable key plus the values its label interpolates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskId {
    pub key: String,
    pub params: BTreeMap<String, String>,
}

impl TaskId {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            params: BTreeMap::new(),
        }
    }
    pub fn with(mut self, key: &str, value: impl Display) -> Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }
}

/// Everything the core reports while it works. The UI only renders these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Progress {
        task: TaskId,
        done: u64,
        total: u64,
        bytes: u64,
    },
    Log {
        line: String,
    },
    StateChanged {
        instance: String,
        state: String,
    },
    /// Something went wrong that doesn't stop the current job (e.g. the game log stopped saving).
    Warning(LauncherError),
}

pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

pub fn noop_sink() -> EventSink {
    Arc::new(|_| {})
}
```

- [ ] **Step 4: Thread `TaskId` through `fetch_all` and its callers**

In `crates/launcher-core/src/download/mod.rs`, change the import to `events::{Event, EventSink, TaskId},` and change `fetch_all` to:

```rust
    /// Download everything, at most `max_concurrent` at a time. Duplicate destinations are fetched once.
    pub async fn fetch_all(
        &self,
        reqs: Vec<DownloadRequest>,
        task: &TaskId,
        sink: &EventSink,
    ) -> Result<()> {
        let mut seen = HashSet::new();
        let reqs: Vec<DownloadRequest> = reqs
            .into_iter()
            .filter(|r| seen.insert(r.dest.clone()))
            .collect();
        let total = reqs.len() as u64;
        let (mut done, mut bytes) = (0u64, 0u64);
        sink(Event::Progress {
            task: task.clone(),
            done,
            total,
            bytes,
        });
        let mut results = futures_util::stream::iter(
            reqs.into_iter()
                .map(|r| async move { self.fetch(&r).await.map(|()| r.size.unwrap_or(0)) }),
        )
        .buffer_unordered(self.max_concurrent);
        while let Some(result) = results.next().await {
            bytes += result?;
            done += 1;
            sink(Event::Progress {
                task: task.clone(),
                done,
                total,
                bytes,
            });
        }
        Ok(())
    }
```

In `crates/launcher-core/src/install/mod.rs` `Installer::install`, replace the `fetch_all` call with:

```rust
        self.downloader
            .fetch_all(
                downloads,
                &TaskId::new("install_minecraft").with("version", &resolved.vanilla_id),
                sink,
            )
            .await?;
```

and add `TaskId` to its `events::` import. In `crates/launcher-core/src/java.rs` `ensure_runtime`, replace the `fetch_all` call with:

```rust
    downloader
        .fetch_all(
            plan.downloads,
            &TaskId::new("download_java").with("version", &entry.version.name),
            sink,
        )
        .await?;
```

and import `crate::events::TaskId`.

In `crates/launcher-core/tests/download.rs` `fetch_all_dedupes_destinations`, import `launcher_core::events::TaskId`, then call `downloader().fetch_all(reqs, &TaskId::new("test"), &sink)` and expect:

```rust
        Event::Progress {
            task: TaskId::new("test"),
            done: 2,
            total: 2,
            bytes: 10
        }
```

In `crates/perland-cli/src/main.rs`, change the sink's arms to:

```rust
    let sink: EventSink = Arc::new(|event| match event {
        Event::Progress {
            task, done, total, ..
        } => eprint!("\r{}: {done}/{total}    ", task.key),
        Event::Log { line } => println!("{line}"),
        Event::Warning(e) => eprintln!("\nwarning: {}", e.message),
        Event::StateChanged { .. } => {}
    });
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates
git commit -m "feat(core): translatable progress task ids and warning events

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: A killable `GameProcess` with capped lines, a tail and crash hints

**Files:**
- Create: `crates/launcher-core/src/launch/crash.rs`
- Modify: `crates/launcher-core/src/launch/mod.rs` (replace `run`, `decode_line`, `GameExit`; add spawn machinery)
- Modify: `crates/perland-cli/src/main.rs` (use `spawn` + `wait`)

**Interfaces:**
- Consumes: `Event::Warning`, `ErrorCode::{LogWriteStopped, JavaStartFailed, Internal}` (Tasks 1–2).
- Produces:
  - `launch::spawn(cmd: &LaunchCommand, redactor: Redactor, sink: EventSink, log_file: &Path) -> Result<GameProcess>`
  - `GameProcess::{pid() -> Option<u32>, killer() -> GameKiller, wait(self) -> Result<GameExit>}`
  - `GameKiller: Clone + Send + Sync`, with `kill(&self)` (idempotent)
  - `GameExit { code: Option<i32>, killed: bool, tail: Vec<String>, crash: Option<CrashHint> }`
  - `launch::crash::{CrashHint, diagnose}`, with `CrashHint` re-exported as `launch::CrashHint`
  - constants `MAX_LINE_BYTES = 8192` and `TAIL_LINES = 200`

- [ ] **Step 1: Write the crash diagnosis test and module**

Create `crates/launcher-core/src/launch/crash.rs`:

```rust
//! Likely causes for a game that exited on its own with an error.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashHint {
    /// Java ran out of memory: the player can raise the instance's memory.
    OutOfMemory,
}

/// Look through the last lines of output for a cause the launcher can explain.
pub fn diagnose(tail: &[String]) -> Option<CrashHint> {
    tail.iter()
        .any(|line| line.contains("java.lang.OutOfMemoryError"))
        .then_some(CrashHint::OutOfMemory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_memory_is_recognised() {
        let tail = vec![
            "[12:00:01] [Render thread/INFO]: Loading".to_string(),
            "Exception in thread \"main\" java.lang.OutOfMemoryError: Java heap space".to_string(),
        ];
        assert_eq!(diagnose(&tail), Some(CrashHint::OutOfMemory));
    }

    #[test]
    fn ordinary_output_has_no_hint() {
        assert_eq!(diagnose(&["Stopping!".to_string()]), None);
        assert_eq!(diagnose(&[]), None);
    }
}
```

Add `pub mod crash;` and `pub use crash::CrashHint;` at the top of `launch/mod.rs`.

- [ ] **Step 2: Write the failing spawn tests**

In `crates/launcher-core/src/launch/mod.rs`, delete the tests `decode_line_is_lossy_and_strips_line_endings` and `run_streams_redacted_output`, and add these tests inside `mod tests` (keep the other tests):

```rust
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncWrite;

    fn collect() -> (EventSink, Arc<Mutex<Vec<Event>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        (Arc::new(move |e| s2.lock().unwrap().push(e)), seen)
    }

    fn logs(seen: &Arc<Mutex<Vec<Event>>>) -> Vec<String> {
        seen.lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Event::Log { line } => Some(line.clone()),
                _ => None,
            })
            .collect()
    }

    /// Prints "alpha" then "bravo" on every OS. On Windows the `&` must sit inside an argument
    /// without spaces: an argument with spaces gets quoted, and cmd won't split a quoted `&`.
    fn two_lines(cwd: &Path) -> LaunchCommand {
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("cmd", &["/C", "echo", "alpha&echo", "bravo"])
        } else {
            ("sh", &["-c", "echo alpha; echo bravo"])
        };
        LaunchCommand {
            program: program.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: cwd.to_path_buf(),
            classpath: vec![],
        }
    }

    #[test]
    fn decode_line_is_lossy_and_strips_line_endings() {
        let out = decode_line(b"caf\xe9 ok\r\n", false);
        assert!(out.contains("caf") && out.ends_with(" ok"), "{out:?}");
        assert!(!out.contains('\r') && !out.contains('\n'), "{out:?}");
        assert_eq!(decode_line(b"plain\n", false), "plain");
        assert_eq!(decode_line(b"\r\n", false), "");
    }

    #[test]
    fn truncated_lines_end_on_a_whole_character() {
        // "ab" then the first byte of a two-byte "ñ": the cut must drop the half character.
        assert_eq!(decode_line(b"ab\xc3", true), "ab …[truncated]");
    }

    #[tokio::test]
    async fn capped_line_drops_the_overflow() {
        let mut input = vec![b'a'; MAX_LINE_BYTES * 3];
        input.extend_from_slice(b"\nnext\n");
        let mut reader = tokio::io::BufReader::new(&input[..]);
        let mut buf = Vec::new();
        let (got, truncated) = read_capped_line(&mut reader, &mut buf, MAX_LINE_BYTES)
            .await
            .unwrap();
        assert!(got && truncated);
        assert_eq!(buf.len(), MAX_LINE_BYTES);
        buf.clear();
        let (got, truncated) = read_capped_line(&mut reader, &mut buf, MAX_LINE_BYTES)
            .await
            .unwrap();
        assert!(got && !truncated);
        assert_eq!(buf, b"next\n");
        buf.clear();
        let (got, _) = read_capped_line(&mut reader, &mut buf, MAX_LINE_BYTES)
            .await
            .unwrap();
        assert!(!got, "end of stream");
    }

    #[test]
    fn tail_keeps_the_last_lines() {
        let mut tail = Tail::default();
        for i in 0..(TAIL_LINES + 5) {
            tail.push(i.to_string());
        }
        let lines = tail.lines();
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines[0], "5");
    }

    #[tokio::test]
    async fn spawn_streams_redacted_output_and_writes_the_log() {
        let tmp = tempfile::tempdir().unwrap();
        let (sink, seen) = collect();
        let log = tmp.path().join("logs/latest.log");
        let redactor = Redactor::new().with_secret("bravo");
        let game = spawn(&two_lines(tmp.path()), redactor, sink, &log)
            .await
            .unwrap();
        let exit = game.wait().await.unwrap();
        assert_eq!(exit.code, Some(0));
        assert!(!exit.killed);
        assert_eq!(exit.crash, None);
        assert_eq!(logs(&seen), vec!["alpha", "<redacted>"]);
        assert_eq!(exit.tail, vec!["alpha", "<redacted>"]);
        let written = std::fs::read_to_string(&log).unwrap();
        assert!(written.contains("alpha") && !written.contains("bravo"), "{written}");
    }

    #[tokio::test]
    async fn kill_stops_a_running_process() {
        let tmp = tempfile::tempdir().unwrap();
        let (program, args) = if cfg!(windows) {
            ("ping", vec!["-n", "30", "127.0.0.1"])
        } else {
            ("sleep", vec!["30"])
        };
        let cmd = LaunchCommand {
            program: program.into(),
            args: args.into_iter().map(String::from).collect(),
            cwd: tmp.path().into(),
            classpath: vec![],
        };
        let (sink, _) = collect();
        let game = spawn(&cmd, Redactor::new(), sink, &tmp.path().join("l.log"))
            .await
            .unwrap();
        assert!(game.pid().is_some());
        let killer = game.killer();
        killer.kill();
        killer.kill(); // idempotent
        let exit = tokio::time::timeout(std::time::Duration::from_secs(15), game.wait())
            .await
            .expect("the killed process must exit")
            .unwrap();
        assert!(exit.killed);
        assert_ne!(exit.code, Some(0));
        assert_eq!(exit.crash, None, "a requested stop is never a crash");
    }

    struct FailingWriter;
    impl AsyncWrite for FailingWriter {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
            _: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            std::task::Poll::Ready(Err(std::io::Error::other("disk full")))
        }
        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn log_write_failure_warns_once_and_keeps_streaming() {
        let tmp = tempfile::tempdir().unwrap();
        let (sink, seen) = collect();
        let game = spawn_with_log(
            &two_lines(tmp.path()),
            Redactor::new(),
            sink,
            Box::new(FailingWriter),
        )
        .unwrap();
        game.wait().await.unwrap();
        let warnings: Vec<ErrorCode> = seen
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Event::Warning(err) => Some(err.code),
                _ => None,
            })
            .collect();
        assert_eq!(warnings, vec![ErrorCode::LogWriteStopped]);
        assert_eq!(logs(&seen), vec!["alpha", "bravo"]);
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p launcher-core --lib launch::`
Expected: compile errors (`spawn`, `read_capped_line`, `Tail`, `spawn_with_log` not found).

- [ ] **Step 4: Implement the spawn machinery**

In `crates/launcher-core/src/launch/mod.rs`:

1. Change the imports at the top to:

```rust
pub mod crash;
pub mod redact;

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    process::Child,
    sync::{mpsc, oneshot, watch},
};

pub use crash::CrashHint;
pub use redact::Redactor;

use crate::{
    accounts::Session,
    error::{ErrorCode, LauncherError, Result},
    events::{Event, EventSink},
    install::{classpath, ResolvedVersion},
    meta::rules::{resolve_args, RuleContext},
    paths::LauncherDirs,
    platform::Platform,
};
```

2. Replace the old `GameExit` struct with:

```rust
/// Longest output line kept; the rest of a longer line is dropped.
pub const MAX_LINE_BYTES: usize = 8 * 1024;
/// How many of the last output lines a finished game reports.
pub const TAIL_LINES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameExit {
    pub code: Option<i32>,
    /// True when the launcher stopped the game on request.
    pub killed: bool,
    /// The last `TAIL_LINES` redacted output lines.
    pub tail: Vec<String>,
    /// A likely cause, only for exits nobody asked for that weren't clean.
    pub crash: Option<CrashHint>,
}

/// Asks a running game to stop. Cheap to clone; calling `kill` twice is harmless.
#[derive(Clone)]
pub struct GameKiller(Arc<watch::Sender<bool>>);

impl GameKiller {
    pub fn kill(&self) {
        // Err only means the game already finished and nobody is listening.
        let _ = self.0.send(true);
    }
}

/// A started game. Dropping this without waiting never kills the game.
pub struct GameProcess {
    pid: Option<u32>,
    killer: GameKiller,
    done: oneshot::Receiver<Result<GameExit>>,
}

impl GameProcess {
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
    pub fn killer(&self) -> GameKiller {
        self.killer.clone()
    }
    pub async fn wait(self) -> Result<GameExit> {
        self.done.await.unwrap_or_else(|_| {
            Err(LauncherError::game(
                ErrorCode::Internal,
                "Lost track of the game process",
            ))
        })
    }
}

#[derive(Default)]
struct Tail(VecDeque<String>);

impl Tail {
    fn push(&mut self, line: String) {
        if self.0.len() == TAIL_LINES {
            self.0.pop_front();
        }
        self.0.push_back(line);
    }
    fn lines(&self) -> Vec<String> {
        self.0.iter().cloned().collect()
    }
}
```

3. Replace `decode_line` and the whole `run` function with:

```rust
/// Decode one raw output line lossily (invalid bytes become U+FFFD) and strip the trailing `\n` / `\r\n`.
/// A truncated line is cut back to its last whole character and marked.
fn decode_line(bytes: &[u8], truncated: bool) -> String {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    if !truncated {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let keep = match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        // An incomplete sequence at the very end is the half character the cap cut through.
        Err(e) if e.error_len().is_none() => e.valid_up_to(),
        Err(_) => bytes.len(),
    };
    let mut line = String::from_utf8_lossy(&bytes[..keep]).into_owned();
    line.push_str(" …[truncated]");
    line
}

/// Read one line into `buf`, keeping at most `max` bytes of it. The rest of an over-long line is
/// consumed and dropped so a runaway line can't grow memory. Returns `(got_a_line, truncated)`;
/// `got_a_line` is false at end of stream.
async fn read_capped_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    max: usize,
) -> std::io::Result<(bool, bool)> {
    let mut got = false;
    let mut truncated = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok((got, truncated));
        }
        got = true;
        let (chunk, found_newline) = match available.iter().position(|&b| b == b'\n') {
            Some(i) => (&available[..=i], true),
            None => (available, false),
        };
        let room = max.saturating_sub(buf.len());
        if chunk.len() > room {
            truncated = true;
        }
        buf.extend_from_slice(&chunk[..chunk.len().min(room)]);
        let used = chunk.len();
        reader.consume(used);
        if found_newline {
            return Ok((true, truncated));
        }
    }
}

async fn read_lines(reader: Box<dyn AsyncRead + Unpin + Send>, tx: mpsc::UnboundedSender<String>) {
    // Read raw bytes: the game may write in a non-UTF-8 charset, which must not end capture.
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match read_capped_line(&mut reader, &mut buf, MAX_LINE_BYTES).await {
            Ok((true, truncated)) => {
                if tx.send(decode_line(&buf, truncated)).is_err() {
                    break;
                }
            }
            Ok((false, _)) | Err(_) => break,
        }
    }
}

type LogWriter = Box<dyn AsyncWrite + Send + Unpin>;

/// Redact each line, append it to the log (until writing fails once), keep the tail, and stream it.
async fn pump_lines(
    mut rx: mpsc::UnboundedReceiver<String>,
    redactor: Redactor,
    sink: EventSink,
    mut log: LogWriter,
    tail: Arc<Mutex<Tail>>,
) {
    let mut log_ok = true;
    while let Some(line) = rx.recv().await {
        let clean = redactor.redact(&line);
        if log_ok {
            let wrote = match log.write_all(clean.as_bytes()).await {
                Ok(()) => log.write_all(b"\n").await,
                Err(e) => Err(e),
            };
            if let Err(e) = wrote {
                log_ok = false;
                sink(Event::Warning(
                    LauncherError::disk(
                        ErrorCode::LogWriteStopped,
                        "The game log stopped being saved to disk",
                    )
                    .with_fix("Check free disk space. The game keeps running.")
                    .with_detail(e),
                ));
            }
        }
        tail.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(clean.clone());
        sink(Event::Log { line: clean });
    }
    if log_ok {
        let _ = log.flush().await;
    }
}

/// Wait for the child, killing it when asked. Returns `(exit code, killed)`.
async fn wait_for_exit(
    mut child: Child,
    mut kill_rx: watch::Receiver<bool>,
) -> Result<(Option<i32>, bool)> {
    let mut killed = false;
    let mut listening = true;
    loop {
        tokio::select! {
            status = child.wait() => return Ok((status?.code(), killed)),
            changed = kill_rx.changed(), if listening && !killed => match changed {
                Ok(()) if *kill_rx.borrow_and_update() => {
                    killed = true;
                    let _ = child.start_kill();
                }
                Ok(()) => {}
                // Every handle was dropped: keep waiting, never kill.
                Err(_) => listening = false,
            },
        }
    }
}

/// Start the game. Output lines are redacted, written to `log_file`, kept as a tail and streamed
/// to `sink`. The returned handle can kill the game and wait for its exit.
pub async fn spawn(
    cmd: &LaunchCommand,
    redactor: Redactor,
    sink: EventSink,
    log_file: &Path,
) -> Result<GameProcess> {
    tokio::fs::create_dir_all(&cmd.cwd).await?;
    if let Some(parent) = log_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Create the log file before spawning: if this fails, no game process exists yet to be orphaned.
    let log = tokio::fs::File::create(log_file).await?;
    spawn_with_log(cmd, redactor, sink, Box::new(log))
}

/// Everything after the process starts runs without an `.await` until its tasks own it, so a
/// caller cancelled at its next `.await` can never drop a started game that nobody waits for.
fn spawn_with_log(
    cmd: &LaunchCommand,
    redactor: Redactor,
    sink: EventSink,
    log: LogWriter,
) -> Result<GameProcess> {
    let mut command = tokio::process::Command::new(&cmd.program);
    command
        .args(&cmd.args)
        .current_dir(&cmd.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = command.spawn().map_err(|e| {
        LauncherError::java(ErrorCode::JavaStartFailed, "Java couldn't be started")
            .with_fix("Reinstall Java from the instance settings")
            .with_detail(e)
    })?;
    let pid = child.id();
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (line_tx, line_rx) = mpsc::unbounded_channel::<String>();
    let readers: [Box<dyn AsyncRead + Unpin + Send>; 2] = [Box::new(stdout), Box::new(stderr)];
    for reader in readers {
        tokio::spawn(read_lines(reader, line_tx.clone()));
    }
    drop(line_tx);
    let tail = Arc::new(Mutex::new(Tail::default()));
    let pump = tokio::spawn(pump_lines(line_rx, redactor, sink, log, tail.clone()));
    let (kill_tx, kill_rx) = watch::channel(false);
    let (done_tx, done_rx) = oneshot::channel();
    tokio::spawn(async move {
        let result = wait_for_exit(child, kill_rx).await;
        // Let the output drain; a grandchild holding the pipes open must not hang the exit.
        let _ = tokio::time::timeout(Duration::from_secs(5), pump).await;
        let tail = tail.lock().unwrap_or_else(|e| e.into_inner()).lines();
        let exit = result.map(|(code, killed)| {
            let crash = if !killed && code != Some(0) {
                crash::diagnose(&tail)
            } else {
                None
            };
            GameExit {
                code,
                killed,
                tail,
                crash,
            }
        });
        let _ = done_tx.send(exit);
    });
    Ok(GameProcess {
        pid,
        killer: GameKiller(Arc::new(kill_tx)),
        done: done_rx,
    })
}
```

4. In `crates/perland-cli/src/main.rs`, replace the `launch::run(...)` call at the end of `launch_instance` with:

```rust
    let game = launch::spawn(
        &cmd,
        redactor,
        sink.clone(),
        &dirs.logs().join(format!("{}-latest.log", instance.id)),
    )
    .await?;
    let exit = game.wait().await?;
    eprintln!("game exited with {:?}", exit.code);
    Ok(())
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p launcher-core --lib launch::`
Expected: PASS, including `capped_line_drops_the_overflow`, `kill_stops_a_running_process` and `log_write_failure_warns_once_and_keeps_streaming`. If Smart App Control blocks the test exe (os error 4551), rerun.

- [ ] **Step 6: Full suite, lint, commit**

```bash
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates
git commit -m "feat(core): killable GameProcess with capped lines, tail, crash hints and log-write warning

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The shared `play` module, `java::probe`, and the CLI on top of it

**Files:**
- Create: `crates/launcher-core/src/play.rs`
- Modify: `crates/launcher-core/src/lib.rs` (`pub mod play;`)
- Modify: `crates/launcher-core/src/java.rs` (add `probe`)
- Modify: `crates/perland-cli/src/main.rs` (`launch_instance` rewritten)

**Interfaces:**
- Consumes: `launch::{spawn, build_command, missing_files, LaunchSpec, LaunchCommand, Redactor, GameProcess}`, `install::{Installer, ResolvedVersion}`, `instances::Instance`, `accounts::Session`.
- Produces:
  - `play::Prepared { resolved: ResolvedVersion, java: PathBuf }`, which derives `Clone`
  - `play::prepare(&Installer, &Instance, &EventSink) -> Result<Prepared>`
  - `play::custom_java(&Path) -> Result<PathBuf>`
  - `play::StartSpec<'a> { dirs, game_dir, instance, prepared, session, platform, launcher_version }`
  - `play::command(&StartSpec) -> Result<LaunchCommand>`
  - `play::check_files(&LaunchCommand) -> Result<()>`
  - `play::start(&StartSpec<'_>, EventSink) -> Result<GameProcess>`
  - `java::probe(&Path) -> Result<u32>`

- [ ] **Step 1: Write the failing tests**

Create `crates/launcher-core/src/play.rs` containing only the tests for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accounts::offline_account,
        instances::{GameSpec, InstanceStore},
        test_support::version,
        ErrorCode,
    };

    #[test]
    fn custom_java_must_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope/java.exe");
        assert_eq!(
            custom_java(&missing).unwrap_err().code,
            ErrorCode::JavaPathMissing
        );
        let present = tmp.path().join("java.exe");
        std::fs::write(&present, "").unwrap();
        assert_eq!(custom_java(&present).unwrap(), present);
    }

    #[tokio::test]
    async fn start_refuses_when_files_are_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = LauncherDirs::new(tmp.path());
        let store = InstanceStore::new(dirs.instances());
        let instance = store
            .create(
                "Smoke",
                GameSpec {
                    minecraft: "1.20.1".into(),
                    loader: None,
                },
            )
            .await
            .unwrap();
        let java = tmp.path().join("java.exe");
        std::fs::write(&java, "").unwrap();
        let prepared = Prepared {
            resolved: ResolvedVersion {
                vanilla_id: "1.20.1".into(),
                json: version("1.20.1.json"),
            },
            java,
        };
        let session = offline_account("Steve").unwrap().offline_session();
        let game_dir = store.game_dir(&instance.id);
        let spec = StartSpec {
            dirs: &dirs,
            game_dir: &game_dir,
            instance: &instance,
            prepared: &prepared,
            session: &session,
            platform: Platform::current(),
            launcher_version: "0.1.0",
        };
        let err = start(&spec, crate::events::noop_sink())
            .await
            .err()
            .expect("libraries are not on disk");
        assert_eq!(err.code, ErrorCode::GameFilesMissing);
        let count: usize = err.params["count"].parse().unwrap();
        assert!(count > 10, "{count}");
    }
}
```

Add `pub mod play;` to `crates/launcher-core/src/lib.rs` (alphabetical, after `paths`).

Add these tests to `crates/launcher-core/src/java.rs`'s test module:

```rust
    #[tokio::test]
    async fn probe_rejects_a_missing_java() {
        let tmp = tempfile::tempdir().unwrap();
        let err = probe(&tmp.path().join("nope")).await.unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::JavaTestFailed);
    }

    #[tokio::test]
    async fn probe_reads_the_java_on_path() {
        // Skipped where no java is installed.
        if std::process::Command::new("java").arg("-version").output().is_err() {
            return;
        }
        let major = probe(std::path::Path::new("java")).await.unwrap();
        assert!(major >= 8, "{major}");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher-core --lib play:: java::`
Expected: compile errors (`custom_java`, `Prepared`, `StartSpec`, `start`, `probe` not found).

- [ ] **Step 3: Implement `play.rs`**

Put this above the tests in `crates/launcher-core/src/play.rs`:

```rust
//! The launch steps both front ends share: get everything ready, then start the game.
use std::path::{Path, PathBuf};

use crate::{
    accounts::Session,
    error::{ErrorCode, LauncherError, Result},
    events::EventSink,
    install::{Installer, ResolvedVersion},
    instances::Instance,
    java,
    launch::{self, GameProcess, LaunchCommand, LaunchSpec, Redactor},
    paths::LauncherDirs,
    platform::Platform,
};

/// Everything a launch needs that `prepare` downloaded or found.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub resolved: ResolvedVersion,
    pub java: PathBuf,
}

/// Resolve the version, install its files, and find Java (the instance's custom one, or Mojang's).
pub async fn prepare(
    installer: &Installer,
    instance: &Instance,
    sink: &EventSink,
) -> Result<Prepared> {
    let loader = instance
        .game
        .loader
        .as_ref()
        .map(|l| (l.kind, l.version.as_str()));
    let resolved = installer.resolve(&instance.game.minecraft, loader).await?;
    installer.install(&resolved, sink).await?;
    let java = match &instance.java.path {
        Some(path) => custom_java(path)?,
        None => {
            java::ensure_runtime(
                &installer.meta,
                &installer.downloader,
                &installer.dirs.java(),
                installer.platform,
                &java::component_for(&resolved.json),
                sink,
            )
            .await?
        }
    };
    Ok(Prepared { resolved, java })
}

/// A custom Java path must point at an existing file; checked before anything is spawned.
pub fn custom_java(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        Ok(path.to_path_buf())
    } else {
        Err(
            LauncherError::java(ErrorCode::JavaPathMissing, "The chosen Java wasn't found")
                .with_fix("Pick Java again in the instance settings, or switch back to automatic")
                .with_detail(path.display()),
        )
    }
}

pub struct StartSpec<'a> {
    pub dirs: &'a LauncherDirs,
    pub game_dir: &'a Path,
    pub instance: &'a Instance,
    pub prepared: &'a Prepared,
    pub session: &'a Session,
    pub platform: Platform,
    pub launcher_version: &'a str,
}

pub fn command(spec: &StartSpec) -> Result<LaunchCommand> {
    launch::build_command(&LaunchSpec {
        dirs: spec.dirs,
        resolved: &spec.prepared.resolved,
        game_dir: spec.game_dir,
        java: &spec.prepared.java,
        session: spec.session,
        memory_mb: spec.instance.java.memory_mb,
        platform: spec.platform,
        launcher_version: spec.launcher_version,
    })
}

/// Fail before spawning when Java or a classpath entry is missing.
pub fn check_files(cmd: &LaunchCommand) -> Result<()> {
    let missing = launch::missing_files(cmd);
    if missing.is_empty() {
        return Ok(());
    }
    let list = missing
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    Err(LauncherError::game(
        ErrorCode::GameFilesMissing,
        format!("{} game files are missing", missing.len()),
    )
    .with_param("count", missing.len())
    .with_fix("Press Play again to repair the instance")
    .with_detail(list))
}

/// Build the command, check its files, and start the game with a redacted log.
pub async fn start(spec: &StartSpec<'_>, sink: EventSink) -> Result<GameProcess> {
    let cmd = command(spec)?;
    check_files(&cmd)?;
    let redactor =
        Redactor::for_current_user().with_secret(spec.session.access_token.clone());
    let log = spec
        .dirs
        .logs()
        .join(format!("{}-latest.log", spec.instance.id));
    launch::spawn(&cmd, redactor, sink, &log).await
}
```

If `ResolvedVersion` doesn't already derive `Clone`, add `#[derive(Debug, Clone)]` to it in `install/mod.rs`.

- [ ] **Step 4: Implement `java::probe`**

Add to `crates/launcher-core/src/java.rs` (importing `std::process::Stdio`, `std::time::Duration` and `crate::ErrorCode` as needed):

```rust
/// Run `<path> -version` and return its major version. Backs the "Test" button for a custom Java.
pub async fn probe(path: &Path) -> Result<u32> {
    let failed = || {
        LauncherError::java(ErrorCode::JavaTestFailed, "That doesn't look like a working Java")
            .with_fix("Pick the java executable inside a Java installation's bin folder")
            .with_detail(path.display())
    };
    let mut command = tokio::process::Command::new(path);
    command
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .map_err(|_| failed())?
        .map_err(|_| failed())?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    parse_java_version(&text).ok_or_else(failed)
}
```

- [ ] **Step 5: Rewrite the CLI's `launch_instance` on top of `play`**

In `crates/perland-cli/src/main.rs`, update the imports to bring in `play::{self, StartSpec}`, `launch::Redactor` and `ErrorCode`, and drop the now-unused `java`, `LaunchSpec` and `launch` items. Then replace the body of `launch_instance` from `let installer = ...` to the end with:

```rust
    let installer = Installer {
        dirs: dirs.clone(),
        meta: MetaClient::new(http.clone(), dirs.meta()),
        downloader: Downloader::new(http),
        platform,
    };
    let sink: EventSink = Arc::new(|event| match event {
        Event::Progress {
            task, done, total, ..
        } => eprint!("\r{}: {done}/{total}    ", task.key),
        Event::Log { line } => println!("{line}"),
        Event::Warning(e) => eprintln!("\nwarning: {}", e.message),
        Event::StateChanged { .. } => {}
    });

    let prepared = play::prepare(&installer, &instance, &sink).await?;
    eprintln!();
    let game_dir = store.game_dir(&instance.id);
    let spec = StartSpec {
        dirs,
        game_dir: &game_dir,
        instance: &instance,
        prepared: &prepared,
        session: &session,
        platform,
        launcher_version: env!("CARGO_PKG_VERSION"),
    };

    if dry_run {
        let cmd = play::command(&spec)?;
        let redactor =
            Redactor::for_current_user().with_secret(session.access_token.clone());
        println!(
            "{}",
            redactor.redact(&format!("{} {}", cmd.program.display(), cmd.args.join(" ")))
        );
        play::check_files(&cmd)?;
        eprintln!(
            "dry run OK: java and {} classpath entries present",
            cmd.classpath.len()
        );
        return Ok(());
    }

    let game = play::start(&spec, sink).await?;
    let exit = game.wait().await?;
    eprintln!("game exited with {:?}", exit.code);
    Ok(())
```

Update the two CLI-only errors to carry codes:
- `LauncherError::auth(ErrorCode::NoAccount, "No account selected")`
- `LauncherError::auth(ErrorCode::MicrosoftUnavailable, "Microsoft accounts arrive in a later build")`

The `Create` arm's mismatched-flags error becomes `LauncherError::loader(ErrorCode::VersionInvalid, "Pass both --loader and --loader-version")`. The `LauncherDirs` error becomes `LauncherError::disk(ErrorCode::DiskIo, "Couldn't find a data folder")`.

- [ ] **Step 6: Run the tests and a CLI dry run**

Run: `cargo test --workspace`
Expected: PASS.

If the smoke instance from Plan 1 still exists, run `cargo run -q -p perland-cli -- --root target/pl-smoke launch --instance smoke-fabric --dry-run`. Expected: a redacted command line and `dry run OK`. Otherwise skip this check.

- [ ] **Step 7: Lint and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates
git commit -m "feat(core): shared play module and java probe; CLI launches through play

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Version catalog for the "New instance" pickers

**Files:**
- Create: `crates/launcher-core/src/meta/catalog.rs`
- Modify: `crates/launcher-core/src/meta/mod.rs` (`pub mod catalog;`)
- Modify: `crates/launcher-core/src/meta/loaders.rs` (make `valid_component` `pub(crate)`)
- Test: `crates/launcher-core/tests/catalog.rs`

**Interfaces:**
- Produces:
  - `meta::catalog::McKind { Release, Snapshot, OldBeta, OldAlpha }`, serialized snake_case
  - `McVersion { id, kind, release_time }`, serialized camelCase
  - `LoaderVersion { version, stable }`
  - `minecraft_versions(&MetaClient)` and `minecraft_versions_at(&MetaClient, url)`
  - `loader_versions(&MetaClient, LoaderKind, mc)` and `loader_versions_at(&MetaClient, LoaderKind, mc, fabric_base, quilt_base)`
  - `compare_versions(a, b) -> Ordering`
  - consts `FABRIC_META` and `QUILT_META`

- [ ] **Step 1: Write the failing integration tests**

Create `crates/launcher-core/tests/catalog.rs`:

```rust
use std::time::Duration;

use launcher_core::{
    meta::{
        catalog::{loader_versions_at, minecraft_versions_at, McKind},
        loaders::LoaderKind,
        MetaClient,
    },
    ErrorCode,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn meta(dir: &std::path::Path) -> MetaClient {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    MetaClient::new(http, dir)
}

async fn serve(server: &MockServer, at: &str, status: u16, body: &str) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn unknown_version_kind_is_skipped() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/manifest.json",
        200,
        r#"{"latest":{"release":"26.3","snapshot":"26.4-snapshot-3"},"versions":[
          {"id":"26.4-snapshot-3","type":"snapshot","url":"u","sha1":"s","releaseTime":"2026-10-01T00:00:00+00:00"},
          {"id":"26.3","type":"release","url":"u","sha1":"s","releaseTime":"2026-09-01T00:00:00+00:00"},
          {"id":"x-experiment","type":"experiment","url":"u","sha1":"s","releaseTime":"2026-08-01T00:00:00+00:00"},
          {"id":"b1.7.3","type":"old_beta","url":"u","sha1":"s","releaseTime":"2011-07-08T00:00:00+00:00"}
        ]}"#,
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let list = minecraft_versions_at(&meta(tmp.path()), &format!("{}/manifest.json", server.uri()))
        .await
        .unwrap();
    let ids: Vec<&str> = list.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, vec!["26.4-snapshot-3", "26.3", "b1.7.3"]);
    assert_eq!(list[1].kind, McKind::Release);
    assert_eq!(list[2].kind, McKind::OldBeta);
    assert_eq!(list[1].release_time, "2026-09-01T00:00:00+00:00");
}

#[tokio::test]
async fn quilt_versions_sorted_newest_first() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/quilt/versions/loader/1.20.1",
        200,
        r#"[{"loader":{"version":"0.20.0-beta.9"}},{"loader":{"version":"0.24.0"}},
            {"loader":{"version":"0.20.2-beta.1"}},{"loader":{"version":"0.19.0"}}]"#,
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let base = server.uri();
    let list = loader_versions_at(
        &meta(tmp.path()),
        LoaderKind::Quilt,
        "1.20.1",
        &format!("{base}/fabric"),
        &format!("{base}/quilt"),
    )
    .await
    .unwrap();
    let got: Vec<(&str, bool)> = list.iter().map(|v| (v.version.as_str(), v.stable)).collect();
    assert_eq!(
        got,
        vec![
            ("0.24.0", true),
            ("0.20.2-beta.1", false),
            ("0.20.0-beta.9", false),
            ("0.19.0", true)
        ]
    );
}

#[tokio::test]
async fn fabric_uses_its_stable_flag() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/fabric/versions/loader/1.20.1",
        200,
        r#"[{"loader":{"version":"0.19.5","stable":true}},{"loader":{"version":"0.19.4","stable":false}}]"#,
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let base = server.uri();
    let list = loader_versions_at(
        &meta(tmp.path()),
        LoaderKind::Fabric,
        "1.20.1",
        &format!("{base}/fabric"),
        &format!("{base}/quilt"),
    )
    .await
    .unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].stable && !list[1].stable);
}

#[tokio::test]
async fn unknown_minecraft_version_has_no_loaders() {
    let server = MockServer::start().await;
    serve(&server, "/fabric/versions/loader/9.9.9", 400, "[]").await;
    let tmp = tempfile::tempdir().unwrap();
    let base = server.uri();
    let list = loader_versions_at(
        &meta(tmp.path()),
        LoaderKind::Fabric,
        "9.9.9",
        &format!("{base}/fabric"),
        &format!("{base}/quilt"),
    )
    .await
    .unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn forge_is_not_offered_yet() {
    let tmp = tempfile::tempdir().unwrap();
    let err = loader_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.20.1", "x", "y")
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::LoaderUnsupported);
}

#[tokio::test]
async fn being_offline_is_reported_not_hidden() {
    let tmp = tempfile::tempdir().unwrap();
    // Port 9 (discard) refuses connections immediately.
    let err = loader_versions_at(
        &meta(tmp.path()),
        LoaderKind::Fabric,
        "1.20.1",
        "http://127.0.0.1:9/fabric",
        "http://127.0.0.1:9/quilt",
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::Offline);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher-core --test catalog`
Expected: compile error, `meta::catalog` not found.

- [ ] **Step 3: Implement `catalog.rs`**

In `crates/launcher-core/src/meta/loaders.rs`, change `fn valid_component` to `pub(crate) fn valid_component`. Add `pub mod catalog;` to `crates/launcher-core/src/meta/mod.rs`. Create `crates/launcher-core/src/meta/catalog.rs`:

```rust
//! Version lists for the "New instance" pickers.
use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::{
    loaders::{valid_component, LoaderKind},
    mojang::VERSION_MANIFEST_URL,
    MetaClient,
};
use crate::error::{ErrorCategory, ErrorCode, LauncherError, Result};

pub const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
pub const QUILT_META: &str = "https://meta.quiltmc.org/v3";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McKind {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
    /// Any type Mojang adds later; skipped instead of breaking the list.
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McVersion {
    pub id: String,
    pub kind: McKind,
    pub release_time: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<ManifestEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestEntry {
    id: String,
    #[serde(rename = "type")]
    kind: McKind,
    #[serde(default)]
    release_time: String,
}

#[derive(Deserialize)]
struct LoaderEntry {
    loader: LoaderInfo,
}

#[derive(Deserialize)]
struct LoaderInfo {
    version: String,
    #[serde(default)]
    stable: Option<bool>,
}

/// Every Minecraft version, newest first (Mojang's order).
pub async fn minecraft_versions(meta: &MetaClient) -> Result<Vec<McVersion>> {
    minecraft_versions_at(meta, VERSION_MANIFEST_URL).await
}

pub async fn minecraft_versions_at(meta: &MetaClient, url: &str) -> Result<Vec<McVersion>> {
    let manifest: Manifest = meta.get_json(url).await?;
    Ok(manifest
        .versions
        .into_iter()
        .filter(|v| v.kind != McKind::Other)
        .map(|v| McVersion {
            id: v.id,
            kind: v.kind,
            release_time: v.release_time,
        })
        .collect())
}

/// Loader versions for one Minecraft version, newest first. Empty when the loader has none for it.
pub async fn loader_versions(
    meta: &MetaClient,
    kind: LoaderKind,
    mc: &str,
) -> Result<Vec<LoaderVersion>> {
    loader_versions_at(meta, kind, mc, FABRIC_META, QUILT_META).await
}

pub async fn loader_versions_at(
    meta: &MetaClient,
    kind: LoaderKind,
    mc: &str,
    fabric_base: &str,
    quilt_base: &str,
) -> Result<Vec<LoaderVersion>> {
    let url = match kind {
        LoaderKind::Fabric => format!("{fabric_base}/versions/loader/{mc}"),
        LoaderKind::Quilt => format!("{quilt_base}/versions/loader/{mc}"),
        LoaderKind::Forge | LoaderKind::NeoForge => {
            return Err(LauncherError::loader(
                ErrorCode::LoaderUnsupported,
                format!("{kind:?} isn't supported yet"),
            )
            .with_param("loader", format!("{kind:?}"))
            .with_fix("Use Fabric or Quilt for now; Forge and NeoForge are coming"))
        }
    };
    if !valid_component(mc) {
        return Err(
            LauncherError::loader(ErrorCode::VersionInvalid, format!("\"{mc}\" isn't a valid version"))
                .with_param("value", mc),
        );
    }
    let entries: Vec<LoaderEntry> = match meta.get_json(&url).await {
        Ok(entries) => entries,
        // The loader answered "no such game version": that's an empty list, not a failure.
        Err(e) if e.category == ErrorCategory::Loader => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out: Vec<LoaderVersion> = entries
        .into_iter()
        .map(|e| LoaderVersion {
            stable: e.loader.stable.unwrap_or(!e.loader.version.contains('-')),
            version: e.loader.version,
        })
        .collect();
    out.sort_by(|a, b| compare_versions(&b.version, &a.version));
    out.dedup_by(|a, b| a.version == b.version);
    Ok(out)
}

/// Compare dotted versions numerically; a release sorts above its own pre-releases
/// (`0.24.0` > `0.24.0-beta.2` > `0.24.0-beta.1`). Non-numeric parts compare as text.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    fn split(v: &str) -> (Vec<&str>, Option<Vec<&str>>) {
        match v.split_once('-') {
            Some((base, pre)) => (base.split('.').collect(), Some(pre.split('.').collect())),
            None => (v.split('.').collect(), None),
        }
    }
    fn parts(a: &[&str], b: &[&str]) -> Ordering {
        for i in 0..a.len().max(b.len()) {
            let (x, y) = (a.get(i).copied().unwrap_or("0"), b.get(i).copied().unwrap_or("0"));
            let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                (Ok(x), Ok(y)) => x.cmp(&y),
                _ => x.cmp(y),
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    }
    let (base_a, pre_a) = split(a);
    let (base_b, pre_b) = split(b);
    parts(&base_a, &base_b).then_with(|| match (pre_a, pre_b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => parts(&x, &y),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_versions_orders_releases_and_prereleases() {
        assert_eq!(compare_versions("0.24.0", "0.20.2"), Ordering::Greater);
        assert_eq!(compare_versions("0.10.0", "0.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("0.24.0", "0.24.0-beta.2"), Ordering::Greater);
        assert_eq!(compare_versions("0.24.0-beta.10", "0.24.0-beta.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.0", "1.0.0"), Ordering::Equal);
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --test catalog` and `cargo test -p launcher-core --lib meta::catalog`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/launcher-core
git commit -m "feat(core): Minecraft and loader version catalog, newest stable first

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Instance store operations and validation

**Files:**
- Modify: `crates/launcher-core/src/instances.rs`

**Interfaces:**
- Produces:
  - consts `MAX_NAME_CHARS = 64`, `MAX_SLUG_LEN = 48`, `MAX_ID_LEN = 64`, `MIN_MEMORY_MB = 512`, `MAX_MEMORY_MB = 65_536`
  - `validate_name(&str) -> Result<String>` (trimmed)
  - `validate_memory(u32) -> Result<()>`
  - `validate_java(&JavaSettings) -> Result<()>`
  - `InstanceListing { instances: Vec<Instance>, damaged: Vec<DamagedInstance> }` (`Serialize`, `Default`)
  - `DamagedInstance { id: String, error: LauncherError }` (`Serialize`)
  - `InstanceStore::{list_all, update(&Instance) -> Result<Instance>, rename(id, name) -> Result<Instance>, delete(id) -> Result<()>}`
  - `create` now validates the name

- [ ] **Step 1: Write the failing tests**

Add to the test module in `crates/launcher-core/src/instances.rs`:

```rust
    use crate::ErrorCode;

    #[tokio::test]
    async fn long_names_cap_slug_and_suffix_stays_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        let name = "A".repeat(60);
        let first = store.create(&name, vanilla("1.21.1")).await.unwrap();
        assert_eq!(first.id.len(), MAX_SLUG_LEN);
        let second = store.create(&name, vanilla("1.21.1")).await.unwrap();
        assert_eq!(second.id, format!("{}-2", first.id));
        assert!(is_valid_instance_id(&second.id), "{}", second.id);
        assert_eq!(store.load(&second.id).await.unwrap().name, name);
    }

    #[tokio::test]
    async fn names_are_validated() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        for bad in ["", "   ", &"x".repeat(65)] {
            let err = store.create(bad, vanilla("1.21.1")).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::NameInvalid, "{bad:?}");
        }
        assert_eq!(validate_name("  ñandú  ").unwrap(), "ñandú");
    }

    #[test]
    fn memory_bounds() {
        assert!(validate_memory(MIN_MEMORY_MB).is_ok());
        assert!(validate_memory(MAX_MEMORY_MB).is_ok());
        assert_eq!(
            validate_memory(256).unwrap_err().code,
            ErrorCode::MemoryOutOfRange
        );
        assert_eq!(
            validate_memory(MAX_MEMORY_MB + 1).unwrap_err().code,
            ErrorCode::MemoryOutOfRange
        );
    }

    #[tokio::test]
    async fn list_all_reports_damaged_instances() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        store.create("Good", vanilla("1.21.1")).await.unwrap();
        let bad = store.create("Bad", vanilla("1.21.1")).await.unwrap();
        std::fs::write(store.instance_dir(&bad.id).join("instance.json"), "{oops").unwrap();
        std::fs::create_dir_all(tmp.path().join("Not An Id")).unwrap();
        std::fs::write(tmp.path().join("Not An Id/instance.json"), "{}").unwrap();
        let listing = store.list_all().await.unwrap();
        assert_eq!(listing.instances.len(), 1);
        assert_eq!(listing.instances[0].name, "Good");
        assert_eq!(listing.damaged.len(), 1);
        assert_eq!(listing.damaged[0].id, "bad");
        assert_eq!(listing.damaged[0].error.code, ErrorCode::InstanceDamaged);
    }

    #[tokio::test]
    async fn update_rename_and_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        let mut instance = store.create("Pack", vanilla("1.21.1")).await.unwrap();
        instance.java.memory_mb = 6144;
        instance.game.minecraft = "1.20.1".into();
        let saved = store.update(&instance).await.unwrap();
        assert_eq!(store.load("pack").await.unwrap(), saved);

        instance.java.memory_mb = 100;
        assert_eq!(
            store.update(&instance).await.unwrap_err().code,
            ErrorCode::MemoryOutOfRange
        );
        instance.java.memory_mb = 4096;
        instance.java.path = Some(tmp.path().join("missing-java"));
        assert_eq!(
            store.update(&instance).await.unwrap_err().code,
            ErrorCode::JavaPathMissing
        );

        let renamed = store.rename("pack", "  New name ").await.unwrap();
        assert_eq!(renamed.name, "New name");
        assert_eq!(renamed.id, "pack", "rename keeps the id and folder");

        store.delete("pack").await.unwrap();
        assert!(!store.instance_dir("pack").exists());
        assert_eq!(
            store.delete("pack").await.unwrap_err().code,
            ErrorCode::InstanceNotFound
        );
        assert_eq!(
            store.delete("../x").await.unwrap_err().code,
            ErrorCode::InstanceIdInvalid
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher-core --lib instances::`
Expected: compile errors (`MAX_SLUG_LEN`, `validate_name`, `list_all`, … not found).

- [ ] **Step 3: Implement**

In `crates/launcher-core/src/instances.rs`:

1. Add these constants and validators after the imports:

```rust
pub const MAX_NAME_CHARS: usize = 64;
/// Longest slug made from a name, before any `-N` suffix for duplicates.
pub const MAX_SLUG_LEN: usize = 48;
/// Longest valid id: a capped slug plus room for a `-N` suffix.
pub const MAX_ID_LEN: usize = 64;
pub const MIN_MEMORY_MB: u32 = 512;
pub const MAX_MEMORY_MB: u32 = 65_536;

/// The trimmed name, if it is 1–64 characters.
pub fn validate_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    let chars = trimmed.chars().count();
    if (1..=MAX_NAME_CHARS).contains(&chars) {
        Ok(trimmed.to_string())
    } else {
        Err(LauncherError::disk(
            ErrorCode::NameInvalid,
            "Instance names must be 1–64 characters",
        )
        .with_param("max", MAX_NAME_CHARS))
    }
}

pub fn validate_memory(memory_mb: u32) -> Result<()> {
    if (MIN_MEMORY_MB..=MAX_MEMORY_MB).contains(&memory_mb) {
        Ok(())
    } else {
        Err(LauncherError::java(
            ErrorCode::MemoryOutOfRange,
            format!("Memory must be between {MIN_MEMORY_MB} and {MAX_MEMORY_MB} MB"),
        )
        .with_param("min", MIN_MEMORY_MB)
        .with_param("max", MAX_MEMORY_MB))
    }
}

pub fn validate_java(java: &JavaSettings) -> Result<()> {
    validate_memory(java.memory_mb)?;
    if let Some(path) = &java.path {
        crate::play::custom_java(path)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DamagedInstance {
    pub id: String,
    pub error: LauncherError,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct InstanceListing {
    pub instances: Vec<Instance>,
    pub damaged: Vec<DamagedInstance>,
}
```

2. In `InstanceStore::create`, add `let name = validate_name(name)?;` as the first line, and use `name` (already trimmed) for `Instance.name` and for `slugify(&name)`.

3. Add these methods to `impl InstanceStore`:

```rust
    /// Every instance, plus the folders whose `instance.json` couldn't be read.
    pub async fn list_all(&self) -> Result<InstanceListing> {
        let mut listing = InstanceListing::default();
        let mut entries = match tokio::fs::read_dir(&self.dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(listing),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let id = entry.file_name().to_string_lossy().into_owned();
            // Folders the launcher could never have created are not ours to report.
            if !is_valid_instance_id(&id) || !is_instance_dir(&entry.path()) {
                continue;
            }
            match self.load(&id).await {
                Ok(instance) => listing.instances.push(instance),
                Err(error) => listing.damaged.push(DamagedInstance { id, error }),
            }
        }
        listing.instances.sort_by_key(|i| i.name.to_lowercase());
        listing.damaged.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(listing)
    }

    /// Save changes to an existing instance after validating them.
    pub async fn update(&self, instance: &Instance) -> Result<Instance> {
        check_id(&instance.id)?;
        let mut instance = instance.clone();
        instance.name = validate_name(&instance.name)?;
        validate_java(&instance.java)?;
        self.load(&instance.id).await?;
        self.save(&instance).await?;
        Ok(instance)
    }

    /// Change the display name only; the id and folder stay the same.
    pub async fn rename(&self, id: &str, name: &str) -> Result<Instance> {
        let mut instance = self.load(id).await?;
        instance.name = validate_name(name)?;
        self.save(&instance).await?;
        Ok(instance)
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        check_id(id)?;
        let dir = self.instance_dir(id);
        if !tokio::fs::try_exists(&dir).await? {
            return Err(LauncherError::disk(
                ErrorCode::InstanceNotFound,
                format!("Instance \"{id}\" wasn't found"),
            )
            .with_param("id", id));
        }
        tokio::fs::remove_dir_all(&dir).await?;
        Ok(())
    }
```

4. Replace `is_valid_instance_id` and `slugify` with the capped versions:

```rust
/// True for exactly the ids `create` can produce: a slug (capped at `MAX_SLUG_LEN`) plus an
/// optional `-N` suffix. Rejects path separators, `.`/`..`, absolute paths and Windows device names.
pub fn is_valid_instance_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_LEN && finish_slug(slug_core(id), None) == id
}

/// A folder-safe id: lowercase ASCII letters/digits joined by single dashes, at most
/// `MAX_SLUG_LEN` long; never empty, never a Windows device name.
pub fn slugify(name: &str) -> String {
    finish_slug(slug_core(name), Some(MAX_SLUG_LEN))
}

fn slug_core(name: &str) -> String {
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
    slug
}

fn finish_slug(mut slug: String, cap: Option<usize>) -> String {
    if let Some(cap) = cap {
        slug.truncate(cap); // ASCII only, so any index is a char boundary
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

Add `serde::Serialize` (already imported) and `ErrorCode` to the imports.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-core --lib instances::`
Expected: PASS (old and new tests).

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/launcher-core
git commit -m "feat(core): instance list_all/update/rename/delete with name, memory and slug limits

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Account switching and removal

**Files:**
- Modify: `crates/launcher-core/src/accounts.rs`

**Interfaces:**
- Produces: `AccountStore::list() -> Result<AccountsFile>`, `set_active(uuid) -> Result<AccountsFile>`, `remove(uuid) -> Result<AccountsFile>`. `AccountsFile` is already `Serialize` (camelCase: `accounts`, `active`, `licensedSigninSeen`).

- [ ] **Step 1: Write the failing test**

Add to the test module in `crates/launcher-core/src/accounts.rs`:

```rust
    #[tokio::test]
    async fn switch_and_remove_accounts() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AccountStore::new(tmp.path().join("accounts.json"));
        let steve = store.add_offline("Steve", BuildKind::Dev).await.unwrap();
        let alex = store.add_offline("Alex", BuildKind::Dev).await.unwrap();
        assert_eq!(store.list().await.unwrap().active.as_deref(), Some(alex.uuid.as_str()));

        let file = store.set_active(&steve.uuid).await.unwrap();
        assert_eq!(file.active.as_deref(), Some(steve.uuid.as_str()));
        assert_eq!(
            store.set_active("not-a-uuid").await.unwrap_err().code,
            crate::ErrorCode::NoAccount
        );

        // Removing the active account makes the first remaining one active.
        let file = store.remove(&steve.uuid).await.unwrap();
        assert_eq!(file.accounts, vec![alex.clone()]);
        assert_eq!(file.active.as_deref(), Some(alex.uuid.as_str()));

        let file = store.remove(&alex.uuid).await.unwrap();
        assert!(file.accounts.is_empty());
        assert_eq!(file.active, None);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p launcher-core --lib accounts::`
Expected: compile errors (`list`, `set_active`, `remove` not found).

- [ ] **Step 3: Implement**

Add to `impl AccountStore` in `crates/launcher-core/src/accounts.rs`:

```rust
    pub async fn list(&self) -> Result<AccountsFile> {
        self.load().await
    }

    pub async fn set_active(&self, uuid: &str) -> Result<AccountsFile> {
        let mut file = self.load().await?;
        if !file.accounts.iter().any(|a| a.uuid == uuid) {
            return Err(LauncherError::auth(
                ErrorCode::NoAccount,
                "That account isn't on this computer anymore",
            ));
        }
        file.active = Some(uuid.to_string());
        self.save(&file).await?;
        Ok(file)
    }

    /// Forget an account. If it was active, the first remaining account becomes active.
    pub async fn remove(&self, uuid: &str) -> Result<AccountsFile> {
        let mut file = self.load().await?;
        file.accounts.retain(|a| a.uuid != uuid);
        if file.active.as_deref() == Some(uuid) {
            file.active = file.accounts.first().map(|a| a.uuid.clone());
        }
        self.save(&file).await?;
        Ok(file)
    }
```

- [ ] **Step 4: Run, lint and commit**

```bash
cargo test -p launcher-core --lib accounts::
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/launcher-core
git commit -m "feat(core): switch and remove accounts

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected from the test command: PASS.

---

### Task 8: Launcher settings

**Files:**
- Create: `crates/launcher-core/src/settings.rs`
- Modify: `crates/launcher-core/src/lib.rs` (`pub mod settings;`)

**Interfaces:**
- Produces:
  - `settings::Lang { En, Es }`, serialized lowercase
  - `settings::Settings { version: u32, language: Option<Lang>, default_memory_mb: u32, minimize_on_play: bool, first_run_done: bool }`, with serde camelCase, `#[serde(default)]` and `Default`
  - `SettingsStore::new(path)`
  - `SettingsStore::load() -> (Settings, Option<LauncherError>)`
  - `SettingsStore::save(&Settings) -> Result<()>`, which validates memory

- [ ] **Step 1: Write the failing tests**

Create `crates/launcher-core/src/settings.rs` with the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    #[tokio::test]
    async fn missing_file_gives_defaults_without_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(tmp.path().join("settings.json"));
        let (settings, warning) = store.load().await;
        assert_eq!(settings, Settings::default());
        assert!(warning.is_none());
        assert_eq!(settings.default_memory_mb, 4096);
        assert!(settings.minimize_on_play);
        assert!(!settings.first_run_done);
    }

    #[tokio::test]
    async fn round_trip_in_camel_case() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(tmp.path().join("settings.json"));
        let settings = Settings {
            language: Some(Lang::Es),
            first_run_done: true,
            ..Settings::default()
        };
        store.save(&settings).await.unwrap();
        let raw = std::fs::read_to_string(tmp.path().join("settings.json")).unwrap();
        assert!(raw.contains("\"language\": \"es\""), "{raw}");
        assert!(raw.contains("\"firstRunDone\": true"), "{raw}");
        assert_eq!(store.load().await, (settings, None));
    }

    #[tokio::test]
    async fn settings_unknown_language_falls_back() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, r#"{"language":"fr","futureField":1}"#).unwrap();
        let (settings, warning) = SettingsStore::new(&path).load().await;
        assert_eq!(settings, Settings::default());
        assert_eq!(warning.unwrap().code, ErrorCode::SettingsDamaged);
    }

    #[tokio::test]
    async fn unknown_fields_alone_are_fine() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, r#"{"language":"en","futureField":1}"#).unwrap();
        let (settings, warning) = SettingsStore::new(&path).load().await;
        assert_eq!(settings.language, Some(Lang::En));
        assert!(warning.is_none());
    }

    #[tokio::test]
    async fn save_validates_memory() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(tmp.path().join("settings.json"));
        let bad = Settings {
            default_memory_mb: 64,
            ..Settings::default()
        };
        assert_eq!(
            store.save(&bad).await.unwrap_err().code,
            ErrorCode::MemoryOutOfRange
        );
    }
}
```

Add `pub mod settings;` to `lib.rs` (alphabetical, after `play`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher-core --lib settings::`
Expected: compile errors.

- [ ] **Step 3: Implement**

Put above the tests in `crates/launcher-core/src/settings.rs`:

```rust
//! Launcher-wide preferences in `settings.json`.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    error::{ErrorCode, LauncherError, Result},
    fsutil::write_atomic,
    instances::validate_memory,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    Es,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    /// `None` until the player picks one; the UI then follows the OS language.
    pub language: Option<Lang>,
    pub default_memory_mb: u32,
    pub minimize_on_play: bool,
    pub first_run_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            language: None,
            default_memory_mb: 4096,
            minimize_on_play: true,
            first_run_done: false,
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Never fails: a missing file means defaults, a damaged one means defaults plus a warning.
    pub async fn load(&self) -> (Settings, Option<LauncherError>) {
        let bytes = match tokio::fs::read(&self.path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Settings::default(), None),
            Err(e) => return (Settings::default(), Some(damaged(e))),
        };
        match serde_json::from_slice(&bytes) {
            Ok(settings) => (settings, None),
            Err(e) => (Settings::default(), Some(damaged(e))),
        }
    }

    pub async fn save(&self, settings: &Settings) -> Result<()> {
        validate_memory(settings.default_memory_mb)?;
        let bytes = serde_json::to_vec_pretty(settings).map_err(|e| {
            LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save settings")
                .with_param("item", "settings")
                .with_detail(e)
        })?;
        write_atomic(&self.path, &bytes).await
    }
}

fn damaged(e: impl std::fmt::Display) -> LauncherError {
    LauncherError::disk(
        ErrorCode::SettingsDamaged,
        "Your launcher settings couldn't be read, so the defaults are in use",
    )
    .with_fix("Set your preferences again in Settings")
    .with_detail(e)
}
```

- [ ] **Step 4: Run, lint and commit**

```bash
cargo test -p launcher-core --lib settings::
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/launcher-core
git commit -m "feat(core): launcher settings with safe defaults

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected from the test command: PASS.

---

### Task 9: Local mods (the jar files in `mods/`)

**Files:**
- Create: `crates/launcher-core/src/mods.rs`
- Modify: `crates/launcher-core/src/lib.rs` (`pub mod mods;`)

**Interfaces:**
- Produces:
  - `mods::LocalMod { file_name: String, enabled: bool, size: u64 }`, serialized camelCase as `fileName`
  - `mods::list(game_dir) -> Result<Vec<LocalMod>>`
  - `set_enabled(game_dir, file_name, enabled) -> Result<LocalMod>`
  - `add(game_dir, src: &Path) -> Result<LocalMod>`
  - `remove(game_dir, file_name) -> Result<()>`

- [ ] **Step 1: Write the failing tests**

Create `crates/launcher-core/src/mods.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    fn jar(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[tokio::test]
    async fn list_finds_enabled_and_disabled_jars_only() {
        let tmp = tempfile::tempdir().unwrap();
        let mods = tmp.path().join("mods");
        std::fs::create_dir_all(mods.join("sub.jar")).unwrap(); // a folder, not a mod
        jar(&mods, "b.jar", b"12");
        jar(&mods, "A.jar.disabled", b"1");
        jar(&mods, "notes.txt", b"x");
        jar(&mods, "half.jar.part", b"x");
        let list = list(tmp.path()).await.unwrap();
        assert_eq!(
            list,
            vec![
                LocalMod { file_name: "A.jar.disabled".into(), enabled: false, size: 1 },
                LocalMod { file_name: "b.jar".into(), enabled: true, size: 2 },
            ]
        );
    }

    #[tokio::test]
    async fn missing_mods_folder_is_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(list(tmp.path()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn toggle_renames_both_ways() {
        let tmp = tempfile::tempdir().unwrap();
        jar(&tmp.path().join("mods"), "sodium.jar", b"abc");
        let off = set_enabled(tmp.path(), "sodium.jar", false).await.unwrap();
        assert_eq!(off.file_name, "sodium.jar.disabled");
        assert!(!off.enabled);
        assert!(tmp.path().join("mods/sodium.jar.disabled").is_file());
        let on = set_enabled(tmp.path(), "sodium.jar.disabled", true).await.unwrap();
        assert_eq!(on.file_name, "sodium.jar");
        assert_eq!(on.size, 3);
        assert_eq!(
            set_enabled(tmp.path(), "gone.jar", false).await.unwrap_err().code,
            ErrorCode::ModNotFound
        );
    }

    #[tokio::test]
    async fn add_accepts_uppercase_jar_and_refuses_disabled_twin() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = tmp.path().join("downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let game = tmp.path().join("game");
        let src = jar(&downloads, "Sodium.JAR", b"abcd");
        let added = add(&game, &src).await.unwrap();
        assert_eq!(added, LocalMod { file_name: "Sodium.JAR".into(), enabled: true, size: 4 });
        set_enabled(&game, "Sodium.JAR", false).await.unwrap();
        assert_eq!(add(&game, &src).await.unwrap_err().code, ErrorCode::ModExists);
        assert_eq!(list(&game).await.unwrap().len(), 1, "no on/off twins");
    }

    #[tokio::test]
    async fn add_refuses_non_jars_and_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let notes = jar(tmp.path(), "notes.txt", b"x");
        assert_eq!(add(tmp.path(), &notes).await.unwrap_err().code, ErrorCode::ModNotJar);
        let folder = tmp.path().join("folder.jar");
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(add(tmp.path(), &folder).await.unwrap_err().code, ErrorCode::ModNotJar);
    }

    #[tokio::test]
    async fn names_cannot_escape_the_mods_folder() {
        let tmp = tempfile::tempdir().unwrap();
        for bad in ["../x.jar", "a/b.jar", "C:x.jar", "..", ""] {
            assert_eq!(
                remove(tmp.path(), bad).await.unwrap_err().code,
                ErrorCode::ModNameInvalid,
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn remove_deletes_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        jar(&tmp.path().join("mods"), "x.jar.disabled", b"1");
        remove(tmp.path(), "x.jar.disabled").await.unwrap();
        assert!(list(tmp.path()).await.unwrap().is_empty());
        assert_eq!(
            remove(tmp.path(), "x.jar").await.unwrap_err().code,
            ErrorCode::ModNotFound
        );
    }
}
```

Add `pub mod mods;` to `lib.rs` (alphabetical, after `meta`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher-core --lib mods::`
Expected: compile errors.

- [ ] **Step 3: Implement**

Put above the tests in `crates/launcher-core/src/mods.rs`:

```rust
//! The plain jar files in an instance's `mods/` folder: list, turn on or off, add, remove.
//! A disabled mod is the same file with `.disabled` appended, which Fabric and Quilt ignore.
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    error::{ErrorCode, LauncherError, Result},
    fsutil::safe_join,
};

const DISABLED: &str = ".disabled";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMod {
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
}

fn mods_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mods")
}

/// `(name without .disabled, enabled)` when `file_name` is one plain path component ending in
/// `.jar` (any case), optionally followed by `.disabled`.
fn jar_name(file_name: &str) -> Option<(String, bool)> {
    let plain = !file_name.is_empty()
        && file_name != "."
        && file_name != ".."
        && !file_name.contains(['/', '\\', ':', '\0']);
    if !plain {
        return None;
    }
    let (base, enabled) = match file_name.strip_suffix(DISABLED) {
        Some(base) => (base, false),
        None => (file_name, true),
    };
    base.to_ascii_lowercase()
        .ends_with(".jar")
        .then(|| (base.to_string(), enabled))
}

fn invalid(file_name: &str) -> LauncherError {
    LauncherError::disk(ErrorCode::ModNameInvalid, "That isn't a mod file name").with_detail(file_name)
}

fn not_found(file_name: &str) -> LauncherError {
    LauncherError::disk(
        ErrorCode::ModNotFound,
        format!("{file_name} isn't in this instance anymore"),
    )
    .with_param("file", file_name)
}

fn exists(base: &str) -> LauncherError {
    LauncherError::disk(ErrorCode::ModExists, format!("{base} is already in this instance"))
        .with_param("file", base)
}

fn path_in(dir: &Path, file_name: &str) -> Result<PathBuf> {
    safe_join(dir, file_name).ok_or_else(|| invalid(file_name))
}

/// Refuse a name already present in either state, so on and off copies never coexist.
async fn ensure_free(dir: &Path, base: &str) -> Result<()> {
    for candidate in [base.to_string(), format!("{base}{DISABLED}")] {
        if tokio::fs::try_exists(path_in(dir, &candidate)?).await? {
            return Err(exists(base));
        }
    }
    Ok(())
}

pub async fn list(game_dir: &Path) -> Result<Vec<LocalMod>> {
    let mut entries = match tokio::fs::read_dir(mods_dir(game_dir)).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some((_, enabled)) = jar_name(&file_name) else {
            continue;
        };
        let meta = entry.metadata().await?;
        if meta.is_file() {
            out.push(LocalMod {
                file_name,
                enabled,
                size: meta.len(),
            });
        }
    }
    out.sort_by_key(|m| m.file_name.to_lowercase());
    Ok(out)
}

pub async fn set_enabled(game_dir: &Path, file_name: &str, enabled: bool) -> Result<LocalMod> {
    let (base, currently) = jar_name(file_name).ok_or_else(|| invalid(file_name))?;
    let dir = mods_dir(game_dir);
    let from = path_in(&dir, file_name)?;
    let meta = tokio::fs::metadata(&from)
        .await
        .map_err(|_| not_found(file_name))?;
    if !meta.is_file() {
        return Err(not_found(file_name));
    }
    if currently == enabled {
        return Ok(LocalMod {
            file_name: file_name.to_string(),
            enabled,
            size: meta.len(),
        });
    }
    let to_name = if enabled {
        base.clone()
    } else {
        format!("{base}{DISABLED}")
    };
    let to = path_in(&dir, &to_name)?;
    if tokio::fs::try_exists(&to).await? {
        return Err(exists(&base));
    }
    tokio::fs::rename(&from, &to).await?;
    Ok(LocalMod {
        file_name: to_name,
        enabled,
        size: meta.len(),
    })
}

/// Copy a jar into `mods/` (via a `.part` file, so a half-copied mod is never loaded).
pub async fn add(game_dir: &Path, src: &Path) -> Result<LocalMod> {
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let not_jar = || {
        LauncherError::disk(ErrorCode::ModNotJar, "Only .jar files can be added as mods")
            .with_param("file", &name)
    };
    let base = match jar_name(&name) {
        Some((base, true)) => base,
        _ => return Err(not_jar()),
    };
    let meta = tokio::fs::metadata(src).await.map_err(|_| not_found(&name))?;
    if !meta.is_file() {
        return Err(not_jar());
    }
    let dir = mods_dir(game_dir);
    tokio::fs::create_dir_all(&dir).await?;
    ensure_free(&dir, &base).await?;
    let dest = path_in(&dir, &base)?;
    let part = path_in(&dir, &format!("{base}.part"))?;
    if let Err(e) = tokio::fs::copy(src, &part).await {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e.into());
    }
    tokio::fs::rename(&part, &dest).await?;
    Ok(LocalMod {
        file_name: base,
        enabled: true,
        size: meta.len(),
    })
}

pub async fn remove(game_dir: &Path, file_name: &str) -> Result<()> {
    jar_name(file_name).ok_or_else(|| invalid(file_name))?;
    let path = path_in(&mods_dir(game_dir), file_name)?;
    if !path.is_file() {
        return Err(not_found(file_name));
    }
    tokio::fs::remove_file(&path).await?;
    Ok(())
}
```

- [ ] **Step 4: Run, lint and commit**

```bash
cargo test -p launcher-core --lib mods::
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/launcher-core
git commit -m "feat(core): local mods list with toggle, add and remove

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected from the test commands: PASS.

---
## Phase 2: App logic (`crates/launcher-app`, no Tauri)

### Task 10: `launcher-app` crate with app events, DTOs and the progress throttle

**Files:**
- Modify: `Cargo.toml` (workspace members)
- Create: `crates/launcher-app/Cargo.toml`
- Create: `crates/launcher-app/src/lib.rs`
- Create: `crates/launcher-app/src/events.rs`
- Create: `crates/launcher-app/src/dto.rs`

**Interfaces:**
- Consumes: `launcher_core::{events::TaskId, launch::CrashHint, LauncherError, accounts::BuildKind, instances::LoaderSpec}`.
- Produces:
  - `RunStateKind { Idle, Queued, Preparing, Running, Stopping }`, serialized snake_case
  - `ExitInfo { code: Option<i32>, killed, cancelled, crash: Option<CrashHint>, error: Option<LauncherError> }` (`Default`, `is_crash()`)
  - `AppEvent`, serialized with tag `type` in snake_case:
    - `Progress { instance: Option<String>, task: TaskId, done, total, bytes }`
    - `Log { instance: String, lines: Vec<String> }`
    - `RunState { instance: String, state: RunStateKind, exit: Option<ExitInfo> }`
    - `Warning { instance: Option<String>, error: LauncherError }`
  - `trait Host: Send + Sync + 'static { fn emit(&self, AppEvent); fn minimize(&self); fn restore(&self); }`
  - `ProgressThrottle::new(Duration)` and `admit(&mut self, task: &str, done, total, now: Instant) -> bool`
  - `PROGRESS_INTERVAL = 100ms`
  - DTOs, serialized camelCase:
    - `Build { Dev, Release }` (snake_case) with `From<BuildKind>`
    - `AppInfo { version, build, offline_allowed, system_memory_mb, data_dir }`
    - `NewInstance { name, minecraft, loader: Option<LoaderSpec> }` (`Deserialize` too)
    - `OpenTarget`, tagged `kind` in snake_case: `Instance { id }`, `Mods { id }`, `Data`, `Logs` (`Deserialize` too)
    - `RunSnapshot { instance, state }`
    - `JavaTest { major }`

- [ ] **Step 1: Create the crate with failing tests**

In the root `Cargo.toml`, set `members = ["crates/launcher-core", "crates/launcher-app", "crates/perland-cli"]`.

Create `crates/launcher-app/Cargo.toml`:

```toml
[package]
name = "launcher-app"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
launcher-core = { path = "../launcher-core" }
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
futures-util.workspace = true

[dev-dependencies]
tempfile.workspace = true
tokio = { workspace = true, features = ["test-util"] }
```

Create `crates/launcher-app/src/lib.rs`:

```rust
//! Everything the desktop app does, as plain Rust: commands in, events out. No Tauri here,
//! so all of it is testable with the fakes in `testing`.
pub mod dto;
pub mod events;

pub use events::{AppEvent, ExitInfo, Host, RunStateKind};
```

Create `crates/launcher-app/src/events.rs` with only its tests for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use launcher_core::ErrorCode;

    #[test]
    fn throttle_keeps_first_and_last_and_spaces_the_rest() {
        let mut t = ProgressThrottle::new(Duration::from_millis(100));
        let start = Instant::now();
        assert!(t.admit("a", 0, 10, start));
        assert!(!t.admit("a", 1, 10, start + Duration::from_millis(10)));
        assert!(t.admit("a", 2, 10, start + Duration::from_millis(150)));
        assert!(!t.admit("a", 3, 10, start + Duration::from_millis(160)));
        assert!(t.admit("a", 10, 10, start + Duration::from_millis(170)), "the last update always passes");
        assert!(t.admit("b", 1, 10, start + Duration::from_millis(20)), "tasks are tracked separately");
    }

    #[test]
    fn events_serialize_with_snake_case_tags() {
        let e = AppEvent::RunState {
            instance: "pack".into(),
            state: RunStateKind::Preparing,
            exit: None,
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["type"], "run_state");
        assert_eq!(json["state"], "preparing");
        assert!(json["exit"].is_null());
        let w = AppEvent::Warning {
            instance: None,
            error: launcher_core::LauncherError::disk(ErrorCode::SettingsDamaged, "x"),
        };
        assert_eq!(serde_json::to_value(&w).unwrap()["error"]["code"], "settings_damaged");
    }

    #[test]
    fn crash_means_an_unrequested_unclean_exit() {
        assert!(ExitInfo { code: Some(1), ..ExitInfo::default() }.is_crash());
        assert!(ExitInfo { code: None, ..ExitInfo::default() }.is_crash());
        assert!(!ExitInfo { code: Some(0), ..ExitInfo::default() }.is_crash());
        assert!(!ExitInfo { code: None, killed: true, ..ExitInfo::default() }.is_crash());
        assert!(!ExitInfo { cancelled: true, ..ExitInfo::default() }.is_crash());
        let failed = ExitInfo {
            error: Some(launcher_core::LauncherError::network(ErrorCode::Offline, "x")),
            ..ExitInfo::default()
        };
        assert!(!failed.is_crash(), "a failed prepare is an error, not a crash");
    }
}
```

Create `crates/launcher-app/src/dto.rs` with only its tests for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_target_round_trips_tagged() {
        let t: OpenTarget = serde_json::from_str(r#"{"kind":"mods","id":"pack"}"#).unwrap();
        assert_eq!(t, OpenTarget::Mods { id: "pack".into() });
        let d: OpenTarget = serde_json::from_str(r#"{"kind":"data"}"#).unwrap();
        assert_eq!(d, OpenTarget::Data);
    }

    #[test]
    fn app_info_is_camel_case() {
        let info = AppInfo {
            version: "0.1.0".into(),
            build: Build::Dev,
            offline_allowed: true,
            system_memory_mb: 16384,
            data_dir: "C:/x".into(),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["offlineAllowed"], true);
        assert_eq!(json["build"], "dev");
        assert_eq!(json["systemMemoryMb"], 16384);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher-app`
Expected: compile errors (`ProgressThrottle`, `AppEvent`, `OpenTarget`, … not found).

- [ ] **Step 3: Implement `events.rs`**

Put above the tests:

```rust
//! What the app tells the UI, and the host (the window) it tells it through.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use launcher_core::{events::TaskId, launch::CrashHint, LauncherError};
use serde::Serialize;

/// Progress for one task reaches the UI at most this often (first and last updates always do).
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStateKind {
    Idle,
    Queued,
    Preparing,
    Running,
    Stopping,
}

/// How a run ended. Exactly one of these is usually interesting: an error, a cancel, a kill, or an exit code.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub killed: bool,
    pub cancelled: bool,
    pub crash: Option<CrashHint>,
    pub error: Option<LauncherError>,
}

impl ExitInfo {
    /// An exit nobody asked for that wasn't clean: the window comes back and the crash card shows.
    pub fn is_crash(&self) -> bool {
        !self.killed && !self.cancelled && self.error.is_none() && self.code != Some(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    Progress {
        instance: Option<String>,
        task: TaskId,
        done: u64,
        total: u64,
        bytes: u64,
    },
    Log {
        instance: String,
        lines: Vec<String>,
    },
    RunState {
        instance: String,
        state: RunStateKind,
        exit: Option<ExitInfo>,
    },
    Warning {
        instance: Option<String>,
        error: LauncherError,
    },
}

/// The window the app runs in. Tauri implements it; tests record what it was asked to do.
pub trait Host: Send + Sync + 'static {
    fn emit(&self, event: AppEvent);
    fn minimize(&self);
    fn restore(&self);
}

/// Drops progress updates that arrive faster than the UI can usefully show them.
pub struct ProgressThrottle {
    interval: Duration,
    last: HashMap<String, Instant>,
}

impl ProgressThrottle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: HashMap::new(),
        }
    }

    /// True when this update should reach the UI: a task's first and last updates always do,
    /// the ones between at most once per interval.
    pub fn admit(&mut self, task: &str, done: u64, total: u64, now: Instant) -> bool {
        let edge = done == 0 || done >= total;
        match self.last.get(task) {
            Some(prev) if !edge && now.duration_since(*prev) < self.interval => false,
            _ => {
                self.last.insert(task.to_string(), now);
                true
            }
        }
    }
}
```

- [ ] **Step 4: Implement `dto.rs`**

Put above the tests:

```rust
//! Shapes the commands take and return that aren't core types.
use launcher_core::{accounts::BuildKind, instances::LoaderSpec};
use serde::{Deserialize, Serialize};

use crate::events::RunStateKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Build {
    Dev,
    Release,
}

impl From<BuildKind> for Build {
    fn from(kind: BuildKind) -> Self {
        match kind {
            BuildKind::Dev => Build::Dev,
            BuildKind::Release => Build::Release,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub build: Build,
    pub offline_allowed: bool,
    pub system_memory_mb: u64,
    pub data_dir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewInstance {
    pub name: String,
    pub minecraft: String,
    pub loader: Option<LoaderSpec>,
}

/// Folders the UI may ask to open. Never a raw path, so the webview can't open arbitrary places.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpenTarget {
    Instance { id: String },
    Mods { id: String },
    Data,
    Logs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunSnapshot {
    pub instance: String,
    pub state: RunStateKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaTest {
    pub major: u32,
}
```

- [ ] **Step 5: Run, lint and commit**

```bash
cargo test -p launcher-app
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/launcher-app
git commit -m "feat(app): launcher-app crate with app events, DTOs and progress throttle

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected from `cargo test -p launcher-app`: PASS.

---

### Task 11: Game log batching and the per-instance log store

**Files:**
- Create: `crates/launcher-app/src/logs.rs`
- Modify: `crates/launcher-app/src/lib.rs` (`pub mod logs;`)

**Interfaces:**
- Consumes: `AppEvent::Log`, `Host` (Task 10).
- Produces:
  - `LOG_KEEP = 5_000`, `BATCH_MAX = 200`, `BATCH_WINDOW = 50ms`
  - `LogStore` (`Clone` + `Default`), with `reset(id)`, `push(id, &[String])` and `tail(id) -> Vec<String>`
  - `spawn_batcher<H: Host>(instance: String, host: Arc<H>, store: LogStore) -> (mpsc::UnboundedSender<String>, JoinHandle<()>)`. The task ends after the last sender is dropped and the remaining lines are flushed.

- [ ] **Step 1: Write the failing tests**

Create `crates/launcher-app/src/logs.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AppEvent, Host};
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct Batches(StdMutex<Vec<Vec<String>>>);
    impl Host for Batches {
        fn emit(&self, event: AppEvent) {
            if let AppEvent::Log { lines, .. } = event {
                self.0.lock().unwrap().push(lines);
            }
        }
        fn minimize(&self) {}
        fn restore(&self) {}
    }

    #[tokio::test(start_paused = true)]
    async fn bursts_are_split_into_batches_of_200() {
        let host = Arc::new(Batches::default());
        let store = LogStore::default();
        let (tx, task) = spawn_batcher("pack".into(), host.clone(), store.clone());
        for i in 0..450 {
            tx.send(format!("line {i}")).unwrap();
        }
        drop(tx);
        task.await.unwrap();
        let sizes: Vec<usize> = host.0.lock().unwrap().iter().map(Vec::len).collect();
        assert_eq!(sizes, vec![200, 200, 50]);
        assert_eq!(store.tail("pack").len(), 450);
        assert_eq!(store.tail("pack")[449], "line 449");
    }

    #[tokio::test(start_paused = true)]
    async fn a_lone_line_is_flushed_after_the_window() {
        let host = Arc::new(Batches::default());
        let (tx, _task) = spawn_batcher("pack".into(), host.clone(), LogStore::default());
        tx.send("hello".into()).unwrap();
        tokio::time::sleep(BATCH_WINDOW * 2).await;
        assert_eq!(*host.0.lock().unwrap(), vec![vec!["hello".to_string()]]);
    }

    #[test]
    fn store_keeps_the_last_5000_and_resets() {
        let store = LogStore::default();
        let lines: Vec<String> = (0..(LOG_KEEP + 10)).map(|i| i.to_string()).collect();
        store.push("a", &lines);
        let tail = store.tail("a");
        assert_eq!(tail.len(), LOG_KEEP);
        assert_eq!(tail[0], "10");
        store.reset("a");
        assert!(store.tail("a").is_empty());
        assert!(store.tail("unknown").is_empty());
    }
}
```

Add `pub mod logs;` to `lib.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher-app logs::`
Expected: compile errors.

- [ ] **Step 3: Implement**

Put above the tests in `crates/launcher-app/src/logs.rs`:

```rust
//! Game output reaches the UI in small batches, and the latest run's lines are kept for re-opening.
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{sync::mpsc, task::JoinHandle, time::Instant};

use crate::events::{AppEvent, Host};

/// Lines kept per instance for `log_tail`.
pub const LOG_KEEP: usize = 5_000;
/// Most lines in one `log` event.
pub const BATCH_MAX: usize = 200;
/// How long a batch waits for more lines before it's sent.
pub const BATCH_WINDOW: Duration = Duration::from_millis(50);

/// The latest run's output for each instance.
#[derive(Clone, Default)]
pub struct LogStore(Arc<Mutex<HashMap<String, VecDeque<String>>>>);

impl LogStore {
    pub fn reset(&self, id: &str) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), VecDeque::new());
    }

    pub fn push(&self, id: &str, lines: &[String]) {
        let mut all = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let kept = all.entry(id.to_string()).or_default();
        kept.extend(lines.iter().cloned());
        while kept.len() > LOG_KEEP {
            kept.pop_front();
        }
    }

    pub fn tail(&self, id: &str) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .map(|lines| lines.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/// Start the task that groups one run's output lines into `log` events. Send lines through the
/// returned sender; once every sender is dropped the task flushes what's left and ends.
pub fn spawn_batcher<H: Host>(
    instance: String,
    host: Arc<H>,
    store: LogStore,
) -> (mpsc::UnboundedSender<String>, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let task = tokio::spawn(async move {
        while let Some(first) = rx.recv().await {
            let mut lines = vec![first];
            let deadline = Instant::now() + BATCH_WINDOW;
            while lines.len() < BATCH_MAX {
                match tokio::time::timeout_at(deadline, rx.recv()).await {
                    Ok(Some(line)) => lines.push(line),
                    Ok(None) | Err(_) => break,
                }
            }
            store.push(&instance, &lines);
            host.emit(AppEvent::Log {
                instance: instance.clone(),
                lines,
            });
        }
    });
    (tx, task)
}
```

- [ ] **Step 4: Run, lint and commit**

```bash
cargo test -p launcher-app logs::
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/launcher-app
git commit -m "feat(app): batch game log lines and keep the latest run per instance

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected from the test command: PASS.

---

### Task 12: The `Runner` seam, the real `CoreRunner`, and test fakes

**Files:**
- Create: `crates/launcher-app/src/runner.rs`
- Create: `crates/launcher-app/src/testing.rs`
- Modify: `crates/launcher-app/src/lib.rs`

**Interfaces:**
- Consumes:
  - `launcher_core::play::{prepare, start, StartSpec, Prepared}`
  - `launch::{GameProcess, GameKiller, GameExit}`
  - `install::Installer`, `instances::{Instance, InstanceStore}`, `accounts::Session`, `events::EventSink`
- Produces:
  - `type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>`
  - `trait Kill: Send + Sync { fn kill(&self); }`, implemented for `GameKiller`
  - `trait RunningGame: Send + 'static { fn killer(&self) -> Arc<dyn Kill>; fn wait(self: Box<Self>) -> BoxFuture<Result<GameExit>>; }`, implemented for `GameProcess`
  - `trait Runner: Send + Sync + 'static`, with:
    - `type Prepared: Send + 'static`
    - `fn prepare(&self, Instance, EventSink) -> BoxFuture<Result<Self::Prepared>>`
    - `fn start(&self, Instance, Self::Prepared, Session, EventSink) -> BoxFuture<Result<Box<dyn RunningGame>>>`
  - `CoreRunner::new(Arc<Installer>, launcher_version: String)`
  - `CoreRunner::installer(&self) -> &Arc<Installer>`
  - For tests only (`#[cfg(test)] pub(crate) mod testing`): `FakeRunner`, `RecordingHost`, `exit(code)`

- [ ] **Step 1: Write the failing test**

Create `crates/launcher-app/src/runner.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use launcher_core::launch::{spawn, LaunchCommand, Redactor};

    #[tokio::test]
    async fn game_process_is_a_running_game() {
        let tmp = tempfile::tempdir().unwrap();
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("ping", &["-n", "30", "127.0.0.1"])
        } else {
            ("sleep", &["30"])
        };
        let cmd = LaunchCommand {
            program: program.into(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: tmp.path().into(),
            classpath: vec![],
        };
        let game = spawn(&cmd, Redactor::new(), launcher_core::events::noop_sink(), &tmp.path().join("l.log"))
            .await
            .unwrap();
        let game: Box<dyn RunningGame> = Box::new(game);
        game.killer().kill();
        let exit = tokio::time::timeout(std::time::Duration::from_secs(15), game.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(exit.killed);
    }
}
```

Update `crates/launcher-app/src/lib.rs` to:

```rust
//! Everything the desktop app does, as plain Rust: commands in, events out. No Tauri here,
//! so all of it is testable with the fakes in `testing`.
pub mod dto;
pub mod events;
pub mod logs;
pub mod runner;
#[cfg(test)]
pub(crate) mod testing;

pub use events::{AppEvent, ExitInfo, Host, RunStateKind};
pub use runner::{CoreRunner, Kill, Runner, RunningGame};
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p launcher-app runner::`
Expected: compile errors (`RunningGame` not found).

- [ ] **Step 3: Implement `runner.rs`**

Put above the tests:

```rust
//! The seam between the app's run state machine and actually installing and starting a game.
use std::{future::Future, pin::Pin, sync::Arc};

use launcher_core::{
    accounts::Session,
    events::EventSink,
    install::Installer,
    instances::{Instance, InstanceStore},
    launch::{GameExit, GameKiller, GameProcess},
    play::{self, Prepared, StartSpec},
    Result,
};

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

pub trait Kill: Send + Sync {
    fn kill(&self);
}

impl Kill for GameKiller {
    fn kill(&self) {
        GameKiller::kill(self)
    }
}

pub trait RunningGame: Send + 'static {
    fn killer(&self) -> Arc<dyn Kill>;
    fn wait(self: Box<Self>) -> BoxFuture<Result<GameExit>>;
}

impl RunningGame for GameProcess {
    fn killer(&self) -> Arc<dyn Kill> {
        Arc::new(GameProcess::killer(self))
    }
    fn wait(self: Box<Self>) -> BoxFuture<Result<GameExit>> {
        Box::pin(GameProcess::wait(*self))
    }
}

/// Installs and starts games. The real one uses `launcher_core::play`; tests use `FakeRunner`.
pub trait Runner: Send + Sync + 'static {
    type Prepared: Send + 'static;
    fn prepare(&self, instance: Instance, sink: EventSink) -> BoxFuture<Result<Self::Prepared>>;
    fn start(
        &self,
        instance: Instance,
        prepared: Self::Prepared,
        session: Session,
        sink: EventSink,
    ) -> BoxFuture<Result<Box<dyn RunningGame>>>;
}

pub struct CoreRunner {
    installer: Arc<Installer>,
    launcher_version: String,
}

impl CoreRunner {
    pub fn new(installer: Arc<Installer>, launcher_version: String) -> Self {
        Self {
            installer,
            launcher_version,
        }
    }
    pub fn installer(&self) -> &Arc<Installer> {
        &self.installer
    }
}

impl Runner for CoreRunner {
    type Prepared = Prepared;

    fn prepare(&self, instance: Instance, sink: EventSink) -> BoxFuture<Result<Prepared>> {
        let installer = self.installer.clone();
        Box::pin(async move { play::prepare(&installer, &instance, &sink).await })
    }

    fn start(
        &self,
        instance: Instance,
        prepared: Prepared,
        session: Session,
        sink: EventSink,
    ) -> BoxFuture<Result<Box<dyn RunningGame>>> {
        let installer = self.installer.clone();
        let version = self.launcher_version.clone();
        Box::pin(async move {
            let game_dir = InstanceStore::new(installer.dirs.instances()).game_dir(&instance.id);
            let spec = StartSpec {
                dirs: &installer.dirs,
                game_dir: &game_dir,
                instance: &instance,
                prepared: &prepared,
                session: &session,
                platform: installer.platform,
                launcher_version: &version,
            };
            let game = play::start(&spec, sink).await?;
            Ok(Box::new(game) as Box<dyn RunningGame>)
        })
    }
}
```

- [ ] **Step 4: Write the test fakes**

Create `crates/launcher-app/src/testing.rs`:

```rust
//! Fakes for the app tests: a runner whose preparing and exits the test controls, and a host
//! that records everything it was told.
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use launcher_core::{
    accounts::Session,
    events::{Event, EventSink, TaskId},
    instances::Instance,
    launch::GameExit,
    ErrorCode, LauncherError, Result,
};
use tokio::sync::{oneshot, Semaphore};

use crate::{
    events::{AppEvent, ExitInfo, Host, RunStateKind},
    runner::{BoxFuture, Kill, Runner, RunningGame},
};

pub(crate) fn exit(code: Option<i32>) -> GameExit {
    GameExit {
        code,
        killed: false,
        tail: vec![],
        crash: None,
    }
}

#[derive(Default)]
pub(crate) struct RecordingHost {
    pub events: Mutex<Vec<AppEvent>>,
    pub minimized: AtomicUsize,
    pub restored: AtomicUsize,
}

impl RecordingHost {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn states(&self, id: &str) -> Vec<RunStateKind> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::RunState { instance, state, .. } if instance == id => Some(*state),
                _ => None,
            })
            .collect()
    }

    pub fn last_exit(&self, id: &str) -> Option<ExitInfo> {
        self.events.lock().unwrap().iter().rev().find_map(|e| match e {
            AppEvent::RunState {
                instance,
                exit: Some(exit),
                ..
            } if instance == id => Some(exit.clone()),
            _ => None,
        })
    }

    pub fn log_lines(&self, id: &str) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::Log { instance, lines } if instance == id => Some(lines.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    pub fn progress_count(&self, id: &str) -> usize {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, AppEvent::Progress { instance: Some(i), .. } if i == id))
            .count()
    }

    pub fn warnings(&self) -> Vec<ErrorCode> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::Warning { error, .. } => Some(error.code),
                _ => None,
            })
            .collect()
    }

    /// Wait until `id` has reported `state` (panics after 5 s).
    pub async fn wait_for(&self, id: &str, state: RunStateKind) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !self.states(id).contains(&state) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "{id} never reached {state:?}; saw {:?}",
                self.states(id)
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
}

impl Host for RecordingHost {
    fn emit(&self, event: AppEvent) {
        self.events.lock().unwrap().push(event);
    }
    fn minimize(&self) {
        self.minimized.fetch_add(1, Ordering::SeqCst);
    }
    fn restore(&self) {
        self.restored.fetch_add(1, Ordering::SeqCst);
    }
}

type ExitSlot = Arc<Mutex<Option<oneshot::Sender<GameExit>>>>;

/// `prepare` emits `progress_steps + 1` progress events, then waits for one gate permit.
/// `start` emits `start_lines` as log output; the game runs until `finish` or a kill.
pub(crate) struct FakeRunner {
    gate: Arc<Semaphore>,
    pub prepare_error: Mutex<Option<LauncherError>>,
    pub progress_steps: Mutex<u64>,
    pub start_lines: Mutex<Vec<String>>,
    exits: Mutex<HashMap<String, ExitSlot>>,
}

impl FakeRunner {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            gate: Arc::new(Semaphore::new(0)),
            prepare_error: Mutex::new(None),
            progress_steps: Mutex::new(0),
            start_lines: Mutex::new(Vec::new()),
            exits: Mutex::new(HashMap::new()),
        })
    }

    /// Let `n` waiting (or future) prepares finish.
    pub fn open_gate(&self, n: usize) {
        self.gate.add_permits(n);
    }

    /// End a running fake game with this exit.
    pub fn finish(&self, id: &str, exit: GameExit) {
        let slot = self.exits.lock().unwrap().get(id).cloned().expect("game started");
        if let Some(tx) = slot.lock().unwrap().take() {
            let _ = tx.send(exit);
        }
    }
}

impl Runner for FakeRunner {
    type Prepared = ();

    fn prepare(&self, instance: Instance, sink: EventSink) -> BoxFuture<Result<()>> {
        let gate = self.gate.clone();
        let error = self.prepare_error.lock().unwrap().take();
        let steps = *self.progress_steps.lock().unwrap();
        Box::pin(async move {
            for done in 0..=steps {
                sink(Event::Progress {
                    task: TaskId::new("install_minecraft").with("version", &instance.game.minecraft),
                    done,
                    total: steps,
                    bytes: done,
                });
            }
            gate.acquire().await.expect("gate is never closed").forget();
            match error {
                Some(e) => Err(e),
                None => Ok(()),
            }
        })
    }

    fn start(
        &self,
        instance: Instance,
        _prepared: (),
        _session: Session,
        sink: EventSink,
    ) -> BoxFuture<Result<Box<dyn RunningGame>>> {
        let lines = self.start_lines.lock().unwrap().clone();
        let (tx, rx) = oneshot::channel();
        let slot: ExitSlot = Arc::new(Mutex::new(Some(tx)));
        self.exits.lock().unwrap().insert(instance.id.clone(), slot.clone());
        Box::pin(async move {
            for line in lines {
                sink(Event::Log { line });
            }
            Ok(Box::new(FakeGame { rx, slot, sink }) as Box<dyn RunningGame>)
        })
    }
}

struct FakeGame {
    rx: oneshot::Receiver<GameExit>,
    slot: ExitSlot,
    /// Held until the game ends, like a real game's output pump.
    sink: EventSink,
}

struct FakeKill(ExitSlot);

impl Kill for FakeKill {
    fn kill(&self) {
        if let Some(tx) = self.0.lock().unwrap().take() {
            let _ = tx.send(GameExit {
                code: None,
                killed: true,
                tail: vec![],
                crash: None,
            });
        }
    }
}

impl RunningGame for FakeGame {
    fn killer(&self) -> Arc<dyn Kill> {
        Arc::new(FakeKill(self.slot.clone()))
    }
    fn wait(self: Box<Self>) -> BoxFuture<Result<GameExit>> {
        Box::pin(async move {
            let FakeGame { rx, sink, .. } = *self;
            let exit = rx
                .await
                .map_err(|_| LauncherError::game(ErrorCode::Internal, "fake game dropped"));
            drop(sink);
            exit
        })
    }
}
```

- [ ] **Step 5: Run, lint and commit**

```bash
cargo test -p launcher-app
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/launcher-app
git commit -m "feat(app): Runner seam with the real CoreRunner and test fakes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected from the test command: PASS. Clippy may flag unused fake helpers as dead code until Task 14. If it does, add `#![allow(dead_code)]` at the top of `testing.rs`; it's test-only code.

---

### Task 13: `AppCore`: settings, instances, accounts, catalog, mods, folders, Java test

**Files:**
- Create: `crates/launcher-app/src/app.rs`
- Modify: `crates/launcher-app/src/lib.rs` (`pub mod app;`, re-export `AppConfig`, `AppCore`)

**Interfaces:**
- Consumes: Tasks 5–12 (`catalog`, `InstanceStore` ops, `AccountStore` ops, `SettingsStore`, `mods`, `java::probe`, `Runner`, `Host`, `LogStore`).
- Produces:
  - `AppConfig { dirs: LauncherDirs, build: BuildKind, launcher_version: String, system_memory_mb: u64 }`
  - `AppCore<R: Runner, H: Host>`, built with `AppCore::new(config, meta: MetaClient, runner: Arc<R>, host: Arc<H>) -> Arc<Self>`
  - methods, all `async` and `Result`-returning unless noted:
    - `app_info`
    - `settings_get` (returns `Settings`, no `Result`) and `settings_set(Settings)`
    - `instances_list`, `instance_create(NewInstance)`, `instance_update(Instance)`, `instance_rename(id, name)`, `instance_delete(id)`
    - `catalog_minecraft`, `catalog_loaders(LoaderKind, mc)`
    - `accounts_list`, `account_add_offline(name)`, `account_set_active(uuid)`, `account_remove(uuid)`
    - `mods_list(id)`, `mod_set_enabled(id, file, enabled)`, `mods_add(id, Vec<PathBuf>)`, `mod_remove(id, file)`
    - `folder(OpenTarget) -> Result<PathBuf>`
    - `java_test(PathBuf) -> Result<JavaTest>`
  - `pub(crate) fn is_busy(&self, id) -> bool`
  - `pub(crate) struct Run { phase, abort: Option<AbortHandle>, killer: Option<Arc<dyn Kill>> }`
  - `pub(crate) fields`: `runs`, `install_slot`, `logs`, `runner`, `host`, `instances`, `accounts`, `config`

- [ ] **Step 1: Write the failing tests**

Create `crates/launcher-app/src/app.rs` with:

```rust
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testing::{FakeRunner, RecordingHost};
    use launcher_core::{instances::LoaderSpec, meta::loaders::LoaderKind, ErrorCode};

    pub(crate) type TestCore = AppCore<FakeRunner, RecordingHost>;

    pub(crate) fn core(
        build: BuildKind,
    ) -> (Arc<TestCore>, Arc<FakeRunner>, Arc<RecordingHost>, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = LauncherDirs::new(tmp.path());
        let runner = FakeRunner::new();
        let host = RecordingHost::new();
        let meta = MetaClient::new(launcher_core::http::client("test").unwrap(), dirs.meta());
        let core = AppCore::new(
            AppConfig {
                dirs,
                build,
                launcher_version: "0.1.0".into(),
                system_memory_mb: 16384,
            },
            meta,
            runner.clone(),
            host.clone(),
        );
        (core, runner, host, tmp)
    }

    fn vanilla(name: &str) -> NewInstance {
        NewInstance {
            name: name.into(),
            minecraft: "1.20.1".into(),
            loader: None,
        }
    }

    #[tokio::test]
    async fn app_info_reflects_build_and_offline_gate() {
        let (dev, _, _, _t1) = core(BuildKind::Dev);
        let info = dev.app_info().await.unwrap();
        assert!(info.offline_allowed);
        assert_eq!(info.build, Build::Dev);
        assert_eq!(info.system_memory_mb, 16384);
        let (release, _, _, _t2) = core(BuildKind::Release);
        assert!(!release.app_info().await.unwrap().offline_allowed);
        assert_eq!(
            release.account_add_offline("Steve").await.unwrap_err().code,
            ErrorCode::OfflineLocked
        );
    }

    #[tokio::test]
    async fn create_uses_the_default_memory_and_refuses_forge() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        core.settings_set(Settings {
            default_memory_mb: 6144,
            ..Settings::default()
        })
        .await
        .unwrap();
        let created = core.instance_create(vanilla("Pack")).await.unwrap();
        assert_eq!(created.java.memory_mb, 6144);
        let forge = NewInstance {
            loader: Some(LoaderSpec {
                kind: LoaderKind::Forge,
                version: "47.2.0".into(),
            }),
            ..vanilla("Forge")
        };
        assert_eq!(
            core.instance_create(forge).await.unwrap_err().code,
            ErrorCode::LoaderUnsupported
        );
        assert_eq!(core.instances_list().await.unwrap().instances.len(), 1);
    }

    #[tokio::test]
    async fn damaged_settings_warn_once() {
        let (core, _, host, tmp) = core(BuildKind::Dev);
        std::fs::write(tmp.path().join("settings.json"), "{nope").unwrap();
        assert_eq!(core.settings_get().await, Settings::default());
        core.settings_get().await;
        assert_eq!(host.warnings(), vec![ErrorCode::SettingsDamaged]);
    }

    #[tokio::test]
    async fn folders_resolve_inside_the_data_root() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let pack = core.instance_create(vanilla("Pack")).await.unwrap();
        let mods = core
            .folder(OpenTarget::Mods { id: pack.id.clone() })
            .await
            .unwrap();
        assert!(mods.is_dir() && mods.starts_with(tmp.path()));
        assert!(core.folder(OpenTarget::Logs).await.unwrap().is_dir());
        assert!(core.folder(OpenTarget::Data).await.unwrap().is_dir());
        assert_eq!(
            core.folder(OpenTarget::Instance { id: "gone".into() })
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceNotFound
        );
        assert_eq!(
            core.folder(OpenTarget::Mods { id: "../x".into() })
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceIdInvalid
        );
    }

    #[tokio::test]
    async fn mods_round_trip_through_the_core() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let pack = core.instance_create(vanilla("Pack")).await.unwrap();
        let src = tmp.path().join("sodium.jar");
        std::fs::write(&src, b"jar").unwrap();
        let list = core.mods_add(&pack.id, vec![src]).await.unwrap();
        assert_eq!(list.len(), 1);
        let list = core
            .mod_set_enabled(&pack.id, "sodium.jar", false)
            .await
            .unwrap();
        assert_eq!(list[0].file_name, "sodium.jar.disabled");
        let list = core.mod_remove(&pack.id, "sodium.jar.disabled").await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn accounts_add_switch_remove() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        let steve = core.account_add_offline("Steve").await.unwrap();
        let file = core.account_add_offline("Alex").await.unwrap();
        assert_eq!(file.accounts.len(), 2);
        let steve_uuid = steve.accounts[0].uuid.clone();
        let file = core.account_set_active(&steve_uuid).await.unwrap();
        assert_eq!(file.active.as_deref(), Some(steve_uuid.as_str()));
        let file = core.account_remove(&steve_uuid).await.unwrap();
        assert_eq!(file.accounts.len(), 1);
    }

    #[tokio::test]
    async fn java_test_reports_failure() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        assert_eq!(
            core.java_test(tmp.path().join("nope")).await.unwrap_err().code,
            ErrorCode::JavaTestFailed
        );
    }
}
```

Update `lib.rs` to add `pub mod app;` and `pub use app::{AppConfig, AppCore};`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher-app app::`
Expected: compile errors.

- [ ] **Step 3: Implement `app.rs`**

Put above the tests:

```rust
//! The app's state and every command except playing (see `runs.rs`).
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use launcher_core::{
    accounts::{AccountStore, AccountsFile, BuildKind},
    instances::{is_valid_instance_id, GameSpec, Instance, InstanceListing, InstanceStore},
    java,
    meta::{
        catalog::{self, LoaderVersion, McVersion},
        loaders::LoaderKind,
        MetaClient,
    },
    mods::{self, LocalMod},
    paths::LauncherDirs,
    settings::{Settings, SettingsStore},
    ErrorCode, LauncherError, Result,
};
use tokio::{sync::Semaphore, task::AbortHandle};

use crate::{
    dto::{AppInfo, JavaTest, NewInstance, OpenTarget},
    events::{AppEvent, Host, RunStateKind},
    logs::LogStore,
    runner::{Kill, Runner},
};

pub struct AppConfig {
    pub dirs: LauncherDirs,
    pub build: BuildKind,
    pub launcher_version: String,
    pub system_memory_mb: u64,
}

/// One instance's current run. Absent from `runs` means idle.
pub(crate) struct Run {
    pub(crate) phase: RunStateKind,
    /// Set while queued or preparing: aborting the task cancels the run.
    pub(crate) abort: Option<AbortHandle>,
    /// Set once the game is running: stopping kills it.
    pub(crate) killer: Option<Arc<dyn Kill>>,
}

pub struct AppCore<R: Runner, H: Host> {
    pub(crate) config: AppConfig,
    pub(crate) instances: InstanceStore,
    pub(crate) accounts: tokio::sync::Mutex<AccountStore>,
    settings: tokio::sync::Mutex<SettingsStore>,
    settings_warned: AtomicBool,
    meta: MetaClient,
    pub(crate) runner: Arc<R>,
    pub(crate) host: Arc<H>,
    /// One install at a time, so two installs never write the same shared files at once.
    pub(crate) install_slot: Arc<Semaphore>,
    pub(crate) runs: Mutex<HashMap<String, Run>>,
    pub(crate) logs: LogStore,
}

fn busy(id: &str) -> LauncherError {
    LauncherError::game(ErrorCode::InstanceBusy, "Stop the game before changing this instance")
        .with_param("id", id)
}

fn valid_id(id: &str) -> Result<()> {
    if is_valid_instance_id(id) {
        Ok(())
    } else {
        Err(LauncherError::disk(ErrorCode::InstanceIdInvalid, "That instance id isn't valid"))
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    pub fn new(config: AppConfig, meta: MetaClient, runner: Arc<R>, host: Arc<H>) -> Arc<Self> {
        let root = config.dirs.root().to_path_buf();
        Arc::new(Self {
            instances: InstanceStore::new(config.dirs.instances()),
            accounts: tokio::sync::Mutex::new(AccountStore::new(root.join("accounts.json"))),
            settings: tokio::sync::Mutex::new(SettingsStore::new(root.join("settings.json"))),
            settings_warned: AtomicBool::new(false),
            meta,
            runner,
            host,
            install_slot: Arc::new(Semaphore::new(1)),
            runs: Mutex::new(HashMap::new()),
            logs: LogStore::default(),
            config,
        })
    }

    pub(crate) fn is_busy(&self, id: &str) -> bool {
        self.runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(id)
    }

    pub async fn app_info(&self) -> Result<AppInfo> {
        let accounts = self.accounts.lock().await.list().await?;
        Ok(AppInfo {
            version: self.config.launcher_version.clone(),
            build: self.config.build.into(),
            offline_allowed: launcher_core::accounts::offline_allowed(
                self.config.build,
                accounts.licensed_signin_seen,
            ),
            system_memory_mb: self.config.system_memory_mb,
            data_dir: self.config.dirs.root().display().to_string(),
        })
    }

    /// The saved settings, or the defaults. A damaged file is reported once as a warning.
    pub async fn settings_get(&self) -> Settings {
        let (settings, warning) = self.settings.lock().await.load().await;
        if let Some(error) = warning {
            if !self.settings_warned.swap(true, Ordering::SeqCst) {
                self.host.emit(AppEvent::Warning {
                    instance: None,
                    error,
                });
            }
        }
        settings
    }

    pub async fn settings_set(&self, settings: Settings) -> Result<Settings> {
        self.settings.lock().await.save(&settings).await?;
        Ok(settings)
    }

    pub async fn instances_list(&self) -> Result<InstanceListing> {
        self.instances.list_all().await
    }

    pub async fn instance_create(&self, request: NewInstance) -> Result<Instance> {
        if let Some(loader) = &request.loader {
            if matches!(loader.kind, LoaderKind::Forge | LoaderKind::NeoForge) {
                return Err(LauncherError::loader(
                    ErrorCode::LoaderUnsupported,
                    format!("{:?} isn't supported yet", loader.kind),
                )
                .with_param("loader", format!("{:?}", loader.kind)));
            }
        }
        let memory = self.settings_get().await.default_memory_mb;
        let mut instance = self
            .instances
            .create(
                &request.name,
                GameSpec {
                    minecraft: request.minecraft,
                    loader: request.loader,
                },
            )
            .await?;
        if instance.java.memory_mb != memory {
            instance.java.memory_mb = memory;
            instance = self.instances.update(&instance).await?;
        }
        Ok(instance)
    }

    /// Version and Java changes wait until the game stops; other edits are fine while it runs.
    pub async fn instance_update(&self, instance: Instance) -> Result<Instance> {
        if self.is_busy(&instance.id) {
            let current = self.instances.load(&instance.id).await?;
            if current.game != instance.game || current.java != instance.java {
                return Err(busy(&instance.id));
            }
        }
        self.instances.update(&instance).await
    }

    pub async fn instance_rename(&self, id: &str, name: &str) -> Result<Instance> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        self.instances.rename(id, name).await
    }

    pub async fn instance_delete(&self, id: &str) -> Result<()> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        self.instances.delete(id).await
    }

    pub async fn catalog_minecraft(&self) -> Result<Vec<McVersion>> {
        catalog::minecraft_versions(&self.meta).await
    }

    pub async fn catalog_loaders(&self, kind: LoaderKind, mc: &str) -> Result<Vec<LoaderVersion>> {
        catalog::loader_versions(&self.meta, kind, mc).await
    }

    pub async fn accounts_list(&self) -> Result<AccountsFile> {
        self.accounts.lock().await.list().await
    }

    pub async fn account_add_offline(&self, name: &str) -> Result<AccountsFile> {
        let store = self.accounts.lock().await;
        store.add_offline(name, self.config.build).await?;
        store.list().await
    }

    pub async fn account_set_active(&self, uuid: &str) -> Result<AccountsFile> {
        self.accounts.lock().await.set_active(uuid).await
    }

    pub async fn account_remove(&self, uuid: &str) -> Result<AccountsFile> {
        self.accounts.lock().await.remove(uuid).await
    }

    async fn game_dir(&self, id: &str) -> Result<PathBuf> {
        self.instances.load(id).await?;
        Ok(self.instances.game_dir(id))
    }

    pub async fn mods_list(&self, id: &str) -> Result<Vec<LocalMod>> {
        mods::list(&self.game_dir(id).await?).await
    }

    pub async fn mod_set_enabled(&self, id: &str, file: &str, enabled: bool) -> Result<Vec<LocalMod>> {
        let dir = self.game_dir(id).await?;
        mods::set_enabled(&dir, file, enabled).await?;
        mods::list(&dir).await
    }

    pub async fn mods_add(&self, id: &str, paths: Vec<PathBuf>) -> Result<Vec<LocalMod>> {
        let dir = self.game_dir(id).await?;
        for path in &paths {
            mods::add(&dir, path).await?;
        }
        mods::list(&dir).await
    }

    pub async fn mod_remove(&self, id: &str, file: &str) -> Result<Vec<LocalMod>> {
        let dir = self.game_dir(id).await?;
        mods::remove(&dir, file).await?;
        mods::list(&dir).await
    }

    /// A folder to open in the file manager. Works for damaged instances too.
    pub async fn folder(&self, target: OpenTarget) -> Result<PathBuf> {
        let path = match target {
            OpenTarget::Instance { id } => {
                valid_id(&id)?;
                let dir = self.instances.instance_dir(&id);
                if !tokio::fs::try_exists(&dir).await? {
                    return Err(LauncherError::disk(
                        ErrorCode::InstanceNotFound,
                        format!("Instance \"{id}\" wasn't found"),
                    )
                    .with_param("id", &id));
                }
                return Ok(dir);
            }
            OpenTarget::Mods { id } => {
                valid_id(&id)?;
                self.instances.game_dir(&id).join("mods")
            }
            OpenTarget::Data => self.config.dirs.root().to_path_buf(),
            OpenTarget::Logs => self.config.dirs.logs(),
        };
        tokio::fs::create_dir_all(&path).await?;
        Ok(path)
    }

    pub async fn java_test(&self, path: PathBuf) -> Result<JavaTest> {
        Ok(JavaTest {
            major: java::probe(&path).await?,
        })
    }
}
```

- [ ] **Step 4: Run, lint and commit**

```bash
cargo test -p launcher-app app::
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/launcher-app
git commit -m "feat(app): AppCore commands for settings, instances, accounts, catalog, mods, folders and Java

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected from the test command: PASS.

---

### Task 14: The run state machine (Play, Stop, concurrent runs, window hints)

**Files:**
- Create: `crates/launcher-app/src/runs.rs`
- Modify: `crates/launcher-app/src/lib.rs` (`mod runs;`)

**Interfaces:**
- Consumes: `AppCore` internals (Task 13), `spawn_batcher`/`LogStore` (Task 11), `ProgressThrottle`/`PROGRESS_INTERVAL`/`ExitInfo` (Task 10), `Runner`/`RunningGame` (Task 12).
- Produces, on `AppCore`:
  - `play(self: &Arc<Self>, id) -> Result<()>`. It returns once the run is queued.
  - `stop(&self, id) -> Result<()>`. It is sync and idempotent.
  - `stop_all(&self, wait: Duration)`. It is async and returns once nothing runs, or after `wait`.
  - `has_active_runs(&self) -> bool`
  - `run_states(&self) -> Vec<RunSnapshot>`
  - `log_tail(&self, id) -> Vec<String>`
- Events: every `RunState` for an instance arrives in this order: `queued → preparing → running → (stopping →) idle`. Each run ends with exactly one `idle` carrying `exit`.

- [ ] **Step 1: Write the failing tests**

Create `crates/launcher-app/src/runs.rs` with:

```rust
#[cfg(test)]
mod tests {
    use std::{sync::atomic::Ordering, time::Duration};

    use launcher_core::{
        accounts::BuildKind, launch::CrashHint, settings::Settings, ErrorCode, LauncherError,
    };

    use crate::{
        app::tests::{core, TestCore},
        dto::NewInstance,
        events::RunStateKind::{self, *},
        testing::{exit, FakeRunner, RecordingHost},
    };
    use std::sync::Arc;

    async fn ready(
        minimize: bool,
    ) -> (Arc<TestCore>, Arc<FakeRunner>, Arc<RecordingHost>, tempfile::TempDir) {
        let (core, runner, host, tmp) = core(BuildKind::Dev);
        core.settings_set(Settings {
            minimize_on_play: minimize,
            ..Settings::default()
        })
        .await
        .unwrap();
        core.account_add_offline("Steve").await.unwrap();
        for name in ["Alpha", "Beta"] {
            core.instance_create(NewInstance {
                name: name.into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        }
        (core, runner, host, tmp)
    }

    #[tokio::test]
    async fn play_runs_through_every_state() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        assert!(core.has_active_runs());
        runner.finish("alpha", exit(Some(0)));
        host.wait_for("alpha", Idle).await;
        assert_eq!(host.states("alpha"), vec![Queued, Preparing, Running, Idle]);
        assert_eq!(host.last_exit("alpha").unwrap().code, Some(0));
        assert_eq!(host.minimized.load(Ordering::SeqCst), 1);
        assert_eq!(host.restored.load(Ordering::SeqCst), 1);
        assert!(!core.has_active_runs());
    }

    #[tokio::test]
    async fn play_needs_an_account() {
        let (core, _, _, _t) = ready(true).await;
        let uuid = core.accounts_list().await.unwrap().accounts[0].uuid.clone();
        core.account_remove(&uuid).await.unwrap();
        assert_eq!(core.play("alpha").await.unwrap_err().code, ErrorCode::NoAccount);
    }

    #[tokio::test]
    async fn same_instance_cannot_run_twice() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        assert_eq!(
            core.play("alpha").await.unwrap_err().code,
            ErrorCode::AlreadyRunning
        );
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        core.stop("alpha").unwrap();
        host.wait_for("alpha", Idle).await;
    }

    #[tokio::test]
    async fn stop_while_queued_cancels_without_touching_the_slot() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        host.wait_for("alpha", Preparing).await; // alpha holds the install slot
        core.play("beta").await.unwrap(); // beta waits for it
        core.stop("beta").unwrap();
        assert_eq!(host.states("beta"), vec![Queued, Idle]);
        assert!(host.last_exit("beta").unwrap().cancelled);
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        core.stop("alpha").unwrap();
        host.wait_for("alpha", Idle).await;
    }

    #[tokio::test]
    async fn stop_while_preparing_releases_the_slot() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        host.wait_for("alpha", Preparing).await;
        core.stop("alpha").unwrap();
        assert!(host.last_exit("alpha").unwrap().cancelled);
        core.play("beta").await.unwrap();
        host.wait_for("beta", Preparing).await;
        runner.open_gate(1);
        host.wait_for("beta", Running).await;
        core.stop("beta").unwrap();
        host.wait_for("beta", Idle).await;
    }

    #[tokio::test]
    async fn stop_while_running_kills_the_game() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        core.stop("alpha").unwrap();
        core.stop("alpha").unwrap(); // a second click is harmless
        host.wait_for("alpha", Idle).await;
        assert_eq!(host.states("alpha"), vec![Queued, Preparing, Running, Stopping, Idle]);
        let exit = host.last_exit("alpha").unwrap();
        assert!(exit.killed && !exit.is_crash());
    }

    #[tokio::test]
    async fn two_instances_run_side_by_side() {
        let (core, runner, host, _t) = ready(false).await;
        core.play("alpha").await.unwrap();
        core.play("beta").await.unwrap();
        runner.open_gate(2);
        host.wait_for("alpha", Running).await;
        host.wait_for("beta", Running).await;
        let mut states = core.run_states();
        states.sort_by(|a, b| a.instance.cmp(&b.instance));
        assert_eq!(states.len(), 2);
        assert!(states.iter().all(|s| s.state == Running));
        runner.finish("alpha", exit(Some(0)));
        runner.finish("beta", exit(Some(0)));
        host.wait_for("alpha", Idle).await;
        host.wait_for("beta", Idle).await;
    }

    #[tokio::test]
    async fn prepare_failure_ends_in_idle_with_the_error() {
        let (core, runner, host, _t) = ready(true).await;
        *runner.prepare_error.lock().unwrap() =
            Some(LauncherError::network(ErrorCode::Offline, "offline"));
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Idle).await;
        let exit = host.last_exit("alpha").unwrap();
        assert_eq!(exit.error.unwrap().code, ErrorCode::Offline);
        assert_eq!(host.minimized.load(Ordering::SeqCst), 0);
        assert!(!core.has_active_runs());
    }

    #[tokio::test]
    async fn crash_brings_the_window_back() {
        let (core, runner, host, _t) = ready(false).await;
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        runner.finish(
            "alpha",
            launcher_core::launch::GameExit {
                code: Some(1),
                killed: false,
                tail: vec![],
                crash: Some(CrashHint::OutOfMemory),
            },
        );
        host.wait_for("alpha", Idle).await;
        assert_eq!(host.minimized.load(Ordering::SeqCst), 0);
        assert_eq!(host.restored.load(Ordering::SeqCst), 1);
        assert_eq!(host.last_exit("alpha").unwrap().crash, Some(CrashHint::OutOfMemory));
    }

    #[tokio::test]
    async fn busy_instances_refuse_changes() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        assert_eq!(
            core.instance_rename("alpha", "New").await.unwrap_err().code,
            ErrorCode::InstanceBusy
        );
        assert_eq!(
            core.instance_delete("alpha").await.unwrap_err().code,
            ErrorCode::InstanceBusy
        );
        let mut changed = core.instances.load("alpha").await.unwrap();
        changed.java.memory_mb = 8192;
        assert_eq!(
            core.instance_update(changed).await.unwrap_err().code,
            ErrorCode::InstanceBusy
        );
        runner.finish("alpha", exit(Some(0)));
        host.wait_for("alpha", Idle).await;
        core.instance_rename("alpha", "New").await.unwrap();
    }

    #[tokio::test]
    async fn game_logs_are_batched_and_kept_for_log_tail() {
        let (core, runner, host, _t) = ready(true).await;
        *runner.start_lines.lock().unwrap() = vec!["[INFO] one".into(), "[INFO] two".into()];
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        runner.finish("alpha", exit(Some(0)));
        host.wait_for("alpha", Idle).await;
        assert_eq!(host.log_lines("alpha"), vec!["[INFO] one", "[INFO] two"]);
        assert_eq!(core.log_tail("alpha"), vec!["[INFO] one", "[INFO] two"]);
        // The lines arrive before the idle event.
        let events = host.events.lock().unwrap();
        let log_at = events.iter().position(|e| matches!(e, crate::AppEvent::Log { .. })).unwrap();
        let idle_at = events
            .iter()
            .position(|e| matches!(e, crate::AppEvent::RunState { state: RunStateKind::Idle, .. }))
            .unwrap();
        assert!(log_at < idle_at);
    }

    #[tokio::test]
    async fn progress_is_throttled_but_keeps_the_ends() {
        let (core, runner, host, _t) = ready(true).await;
        *runner.progress_steps.lock().unwrap() = 100;
        core.play("alpha").await.unwrap();
        host.wait_for("alpha", Preparing).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(host.progress_count("alpha"), 2, "first and last of 101 updates");
        core.stop("alpha").unwrap();
    }

    #[tokio::test]
    async fn stop_all_waits_for_every_run() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        core.play("beta").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await; // beta is still preparing or queued
        core.stop_all(Duration::from_secs(2)).await;
        assert!(!core.has_active_runs());
        assert!(host.last_exit("alpha").unwrap().killed);
        assert!(host.last_exit("beta").unwrap().cancelled);
    }
}
```

Add `mod runs;` to `lib.rs` (private: its methods are inherent methods on `AppCore`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p launcher-app runs::`
Expected: compile errors (`play`, `stop`, … not found).

- [ ] **Step 3: Implement `runs.rs`**

Put above the tests:

```rust
//! Play and Stop: the run state machine. A run is `queued` until it holds the install slot,
//! `preparing` while installing, `running` while the game is up, `stopping` after a kill, and
//! leaves the map (idle) with exactly one final `run_state` event carrying how it ended.
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use launcher_core::{
    accounts::{Provider, Session},
    events::{Event, EventSink},
    instances::Instance,
    ErrorCode, LauncherError, Result,
};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    app::{AppCore, Run},
    dto::RunSnapshot,
    events::{AppEvent, ExitInfo, Host, ProgressThrottle, RunStateKind, PROGRESS_INTERVAL},
    logs::spawn_batcher,
    runner::Runner,
};

impl<R: Runner, H: Host> AppCore<R, H> {
    /// Queue a run. Everything after this arrives as events.
    pub async fn play(self: &Arc<Self>, id: &str) -> Result<()> {
        let instance = self.instances.load(id).await?;
        let account = self.accounts.lock().await.active().await?.ok_or_else(|| {
            LauncherError::auth(ErrorCode::NoAccount, "Add an account before playing")
                .with_fix("Open Accounts and add one")
        })?;
        if account.provider != Provider::Offline {
            return Err(LauncherError::auth(
                ErrorCode::MicrosoftUnavailable,
                "Microsoft accounts arrive in a later version",
            ));
        }
        let session = account.offline_session();
        let minimize = self.settings_get().await.minimize_on_play;

        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        if runs.contains_key(id) {
            return Err(
                LauncherError::game(ErrorCode::AlreadyRunning, "This instance is already running")
                    .with_param("id", id),
            );
        }
        self.logs.reset(id);
        // Emitted under the lock, before the task exists, so `queued` always comes first.
        self.emit_state(id, RunStateKind::Queued, None);
        let core = Arc::clone(self);
        let task = tokio::spawn(async move { core.drive(instance, session, minimize).await });
        runs.insert(
            id.to_string(),
            Run {
                phase: RunStateKind::Queued,
                abort: Some(task.abort_handle()),
                killer: None,
            },
        );
        Ok(())
    }

    /// Cancel a queued or preparing run, or kill a running game. Unknown or idle ids are fine.
    pub fn stop(&self, id: &str) -> Result<()> {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        let Some(run) = runs.get_mut(id) else {
            return Ok(());
        };
        match run.phase {
            RunStateKind::Queued | RunStateKind::Preparing => {
                if let Some(abort) = run.abort.take() {
                    abort.abort();
                }
                runs.remove(id);
                self.emit_state(
                    id,
                    RunStateKind::Idle,
                    Some(ExitInfo {
                        cancelled: true,
                        ..ExitInfo::default()
                    }),
                );
            }
            RunStateKind::Running => {
                run.phase = RunStateKind::Stopping;
                if let Some(killer) = &run.killer {
                    killer.kill();
                }
                self.emit_state(id, RunStateKind::Stopping, None);
            }
            RunStateKind::Stopping | RunStateKind::Idle => {}
        }
        Ok(())
    }

    /// Stop everything and wait (up to `wait`) until every run has ended. Used when quitting.
    pub async fn stop_all(&self, wait: Duration) {
        let ids: Vec<String> = self
            .runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect();
        for id in ids {
            let _ = self.stop(&id);
        }
        let deadline = tokio::time::Instant::now() + wait;
        while self.has_active_runs() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    pub fn has_active_runs(&self) -> bool {
        !self.runs.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    }

    pub fn run_states(&self) -> Vec<RunSnapshot> {
        self.runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(id, run)| RunSnapshot {
                instance: id.clone(),
                state: run.phase,
            })
            .collect()
    }

    pub fn log_tail(&self, id: &str) -> Vec<String> {
        self.logs.tail(id)
    }

    fn emit_state(&self, id: &str, state: RunStateKind, exit: Option<ExitInfo>) {
        self.host.emit(AppEvent::RunState {
            instance: id.to_string(),
            state,
            exit,
        });
    }

    fn set_phase(&self, id: &str, phase: RunStateKind) {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = runs.get_mut(id) {
            run.phase = phase;
            self.emit_state(id, phase, None);
        }
    }

    /// Core events for one run become app events tagged with the instance.
    fn run_sink(&self, id: &str, log_tx: mpsc::UnboundedSender<String>) -> EventSink {
        let host = self.host.clone();
        let id = id.to_string();
        let throttle = Mutex::new(ProgressThrottle::new(PROGRESS_INTERVAL));
        Arc::new(move |event| match event {
            Event::Progress {
                task,
                done,
                total,
                bytes,
            } => {
                let admit = throttle
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .admit(&task.key, done, total, Instant::now());
                if admit {
                    host.emit(AppEvent::Progress {
                        instance: Some(id.clone()),
                        task,
                        done,
                        total,
                        bytes,
                    });
                }
            }
            Event::Log { line } => {
                let _ = log_tx.send(line);
            }
            Event::Warning(error) => host.emit(AppEvent::Warning {
                instance: Some(id.clone()),
                error,
            }),
            Event::StateChanged { .. } => {}
        })
    }

    async fn drive(self: Arc<Self>, instance: Instance, session: Session, minimize: bool) {
        let id = instance.id.clone();
        let (log_tx, batcher) = spawn_batcher(id.clone(), self.host.clone(), self.logs.clone());
        let sink = self.run_sink(&id, log_tx);
        let Ok(permit) = self.install_slot.clone().acquire_owned().await else {
            return;
        };
        self.set_phase(&id, RunStateKind::Preparing);
        let prepared = match self.runner.prepare(instance.clone(), sink.clone()).await {
            Ok(prepared) => prepared,
            Err(error) => return self.finish(&id, failed(error), sink, batcher).await,
        };
        drop(permit);
        let game = match self
            .runner
            .start(instance, prepared, session, sink.clone())
            .await
        {
            Ok(game) => game,
            Err(error) => return self.finish(&id, failed(error), sink, batcher).await,
        };
        // No `.await` between `start` returning and recording the killer, so a Stop can't slip
        // in and leave the game running with nobody holding its killer.
        {
            let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
            match runs.get_mut(&id) {
                Some(run) => {
                    run.phase = RunStateKind::Running;
                    run.killer = Some(game.killer());
                    run.abort = None;
                    self.emit_state(&id, RunStateKind::Running, None);
                }
                None => {
                    game.killer().kill();
                    return;
                }
            }
        }
        if minimize {
            self.host.minimize();
        }
        let info = match game.wait().await {
            Ok(exit) => ExitInfo {
                code: exit.code,
                killed: exit.killed,
                crash: exit.crash,
                ..ExitInfo::default()
            },
            Err(error) => failed(error),
        };
        let crashed = info.is_crash();
        self.finish(&id, info, sink, batcher).await;
        if minimize || crashed {
            self.host.restore();
        }
    }

    async fn finish(&self, id: &str, info: ExitInfo, sink: EventSink, batcher: JoinHandle<()>) {
        drop(sink);
        // Let the last log lines reach the UI before the idle state does.
        let _ = tokio::time::timeout(Duration::from_secs(2), batcher).await;
        let removed = self
            .runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id)
            .is_some();
        // A cancelled run was already removed and reported by `stop`.
        if removed {
            self.emit_state(id, RunStateKind::Idle, Some(info));
        }
    }
}

fn failed(error: LauncherError) -> ExitInfo {
    ExitInfo {
        error: Some(error),
        ..ExitInfo::default()
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p launcher-app`
Expected: PASS. All 13 `runs::tests` and the earlier tests pass. If `stop_while_queued_cancels_without_touching_the_slot` hangs, check that `drive` acquires the slot only after the sink is created, and that `stop` removes the run before emitting.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/launcher-app
git commit -m "feat(app): run state machine with cancel, kill, concurrent runs and window hints

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 15: Rust→TS contract fixtures

**Files:**
- Create: `crates/launcher-app/tests/contract.rs`
- Create (generated, committed): `ui/src/ipc/contract/*.json`

**Interfaces:**
- Produces, in `ui/src/ipc/contract/` (pretty JSON with a trailing newline):
  - `instance.json`, `instance_listing.json`, `accounts.json`, `settings.json`, `app_info.json`
  - `mc_versions.json`, `loader_versions.json`, `local_mods.json`
  - `events.json` (every `AppEvent` variant), `error.json`, `error_codes.json` (`ErrorCode::ALL`)
  - `new_instance.json`, `open_targets.json`, `run_snapshots.json`, `java_test.json`
- Rule: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract` rewrites them. Without that variable, the test fails when a fixture is stale.

- [ ] **Step 1: Write the contract test**

Create `crates/launcher-app/tests/contract.rs`:

```rust
//! Writes one sample of every type the UI receives into `ui/src/ipc/contract/`, or checks the
//! committed copies are current. The UI's tests parse these with its zod schemas, so a renamed or
//! retyped field fails on one side or the other.
use std::{collections::BTreeMap, path::PathBuf};

use launcher_app::{
    dto::{AppInfo, Build, JavaTest, NewInstance, OpenTarget, RunSnapshot},
    events::{AppEvent, ExitInfo, RunStateKind},
};
use launcher_core::{
    accounts::{offline_account, AccountsFile},
    events::TaskId,
    instances::{
        DamagedInstance, GameSpec, Instance, InstanceListing, JavaSettings, LoaderSpec, SourceRef,
        UpdatePolicy,
    },
    launch::CrashHint,
    meta::{
        catalog::{LoaderVersion, McKind, McVersion},
        loaders::LoaderKind,
    },
    mods::LocalMod,
    settings::{Lang, Settings},
    ErrorCode, LauncherError,
};
use serde::Serialize;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/ipc/contract")
}

fn check(name: &str, value: impl Serialize) {
    let path = dir().join(format!("{name}.json"));
    let fresh = serde_json::to_string_pretty(&value).unwrap() + "\n";
    if std::env::var_os("UPDATE_CONTRACT").is_some() {
        std::fs::create_dir_all(dir()).unwrap();
        std::fs::write(&path, fresh).unwrap();
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    assert_eq!(
        current, fresh,
        "{name}.json is stale: run `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract` and commit the result"
    );
}

fn fabric_instance() -> Instance {
    Instance {
        id: "asalith-fields".into(),
        name: "Asalith Fields".into(),
        source: SourceRef::None,
        game: GameSpec {
            minecraft: "1.20.1".into(),
            loader: Some(LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.16.14".into(),
            }),
        },
        update_policy: UpdatePolicy::Ask,
        optional_choices: BTreeMap::new(),
        java: JavaSettings {
            path: None,
            memory_mb: 4096,
        },
    }
}

fn vanilla_instance() -> Instance {
    Instance {
        id: "creative-flat".into(),
        name: "Creative Flat".into(),
        source: SourceRef::None,
        game: GameSpec {
            minecraft: "26.3".into(),
            loader: None,
        },
        update_policy: UpdatePolicy::Ask,
        optional_choices: BTreeMap::new(),
        java: JavaSettings {
            path: Some(PathBuf::from("C:/Java/bin/java.exe")),
            memory_mb: 6144,
        },
    }
}

fn sample_error() -> LauncherError {
    LauncherError::loader(ErrorCode::MinecraftVersionNotFound, "Minecraft 9.9 doesn't exist")
        .with_param("version", "9.9")
        .with_fix("Check the version number")
        .with_detail("manifest has no 9.9")
}

#[test]
fn contract_fixtures_are_current() {
    check("instance", fabric_instance());
    check(
        "instance_listing",
        InstanceListing {
            instances: vec![fabric_instance(), vanilla_instance()],
            damaged: vec![DamagedInstance {
                id: "old-pack".into(),
                error: LauncherError::disk(
                    ErrorCode::InstanceDamaged,
                    "The settings file of \"old-pack\" is damaged",
                )
                .with_param("id", "old-pack")
                .with_fix("Restore instance.json from a backup or recreate the instance")
                .with_detail("expected value at line 1 column 1"),
            }],
        },
    );
    let steve = offline_account("Steve").unwrap();
    let alex = offline_account("Alex_Builds").unwrap();
    check(
        "accounts",
        AccountsFile {
            active: Some(steve.uuid.clone()),
            accounts: vec![steve, alex],
            licensed_signin_seen: false,
        },
    );
    check(
        "settings",
        Settings {
            language: Some(Lang::Es),
            ..Settings::default()
        },
    );
    check(
        "app_info",
        AppInfo {
            version: "0.1.0".into(),
            build: Build::Dev,
            offline_allowed: true,
            system_memory_mb: 16384,
            data_dir: "C:/Users/player/AppData/Roaming/PerlandLauncher".into(),
        },
    );
    check(
        "mc_versions",
        vec![
            McVersion { id: "26.4-snapshot-3".into(), kind: McKind::Snapshot, release_time: "2026-10-01T09:00:00+00:00".into() },
            McVersion { id: "26.3".into(), kind: McKind::Release, release_time: "2026-09-15T09:00:00+00:00".into() },
            McVersion { id: "1.21.1".into(), kind: McKind::Release, release_time: "2024-08-08T12:00:00+00:00".into() },
            McVersion { id: "1.20.1".into(), kind: McKind::Release, release_time: "2023-06-12T13:25:51+00:00".into() },
            McVersion { id: "b1.7.3".into(), kind: McKind::OldBeta, release_time: "2011-07-07T22:00:00+00:00".into() },
        ],
    );
    check(
        "loader_versions",
        vec![
            LoaderVersion { version: "0.19.5".into(), stable: true },
            LoaderVersion { version: "0.19.4".into(), stable: true },
            LoaderVersion { version: "0.20.0-beta.1".into(), stable: false },
        ],
    );
    check(
        "local_mods",
        vec![
            LocalMod { file_name: "fabric-api-0.92.2+1.20.1.jar".into(), enabled: true, size: 2_310_144 },
            LocalMod { file_name: "sodium-fabric-0.5.8+mc1.20.1.jar".into(), enabled: true, size: 1_048_576 },
            LocalMod { file_name: "zoomify-2.11.2.jar.disabled".into(), enabled: false, size: 524_288 },
        ],
    );
    check(
        "events",
        vec![
            AppEvent::Progress {
                instance: Some("asalith-fields".into()),
                task: TaskId::new("install_minecraft").with("version", "1.20.1"),
                done: 1204,
                total: 3575,
                bytes: 312_000_000,
            },
            AppEvent::Progress {
                instance: None,
                task: TaskId::new("download_java").with("version", "17.0.15"),
                done: 0,
                total: 1,
                bytes: 0,
            },
            AppEvent::Log {
                instance: "asalith-fields".into(),
                lines: vec![
                    "[12:00:01] [Render thread/INFO]: Setting user: Steve".into(),
                    "[12:00:02] [Render thread/INFO]: Backend library: LWJGL version 3.3.1".into(),
                ],
            },
            AppEvent::RunState { instance: "asalith-fields".into(), state: RunStateKind::Queued, exit: None },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo { code: Some(1), crash: Some(CrashHint::OutOfMemory), ..ExitInfo::default() }),
            },
            AppEvent::RunState {
                instance: "creative-flat".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo { error: Some(sample_error()), ..ExitInfo::default() }),
            },
            AppEvent::RunState {
                instance: "creative-flat".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo { cancelled: true, ..ExitInfo::default() }),
            },
            AppEvent::Warning {
                instance: Some("asalith-fields".into()),
                error: LauncherError::disk(ErrorCode::LogWriteStopped, "The game log stopped being saved to disk")
                    .with_fix("Check free disk space. The game keeps running."),
            },
        ],
    );
    check("error", sample_error());
    check("error_codes", ErrorCode::ALL);
    check(
        "new_instance",
        NewInstance {
            name: "Asalith Fields".into(),
            minecraft: "1.20.1".into(),
            loader: Some(LoaderSpec { kind: LoaderKind::Fabric, version: "0.19.5".into() }),
        },
    );
    check(
        "open_targets",
        vec![
            OpenTarget::Instance { id: "asalith-fields".into() },
            OpenTarget::Mods { id: "asalith-fields".into() },
            OpenTarget::Data,
            OpenTarget::Logs,
        ],
    );
    check(
        "run_snapshots",
        vec![RunSnapshot { instance: "asalith-fields".into(), state: RunStateKind::Running }],
    );
    check("java_test", JavaTest { major: 17 });
}
```

- [ ] **Step 2: Run it to verify it fails (no fixtures yet)**

Run: `cargo test -p launcher-app --test contract`
Expected: FAIL with `instance.json is stale`.

- [ ] **Step 3: Generate the fixtures**

Run (Git Bash): `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`
PowerShell equivalent: `$env:UPDATE_CONTRACT='1'; cargo test -p launcher-app --test contract; Remove-Item Env:UPDATE_CONTRACT`
Expected: PASS, and 15 files appear in `ui/src/ipc/contract/`.

Check a few by eye:
- `instance.json` has `"memoryMb": 4096`, `"loader": {"kind": "fabric", ...}` and `"source": {"kind": "none"}`.
- `events.json` has `"type": "run_state"` and `"crash": "out_of_memory"`.
- `error_codes.json` is a flat array of snake_case strings.

- [ ] **Step 4: Run it again without the variable**

Run: `cargo test -p launcher-app --test contract`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/launcher-app ui/src/ipc/contract
git commit -m "test(app): Rust-emitted contract fixtures for the UI

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
## Phase 3: Workspace, shell and CI

### Task 16: pnpm workspace, UI scaffold and the land-plot mark

**Files:**
- Create: `package.json`, `pnpm-workspace.yaml` (repo root)
- Modify: `.gitignore`
- Create: `ui/package.json`, `ui/index.html`, `ui/vite.config.ts`, `ui/tsconfig.json`, `ui/eslint.config.js`
- Create: `ui/src/main.tsx`, `ui/src/App.tsx`, `ui/src/App.test.tsx`, `ui/src/test/setup.ts`, `ui/src/styles/app.css`, `ui/src/vite-env.d.ts`
- Create: `ui/src/assets/mark.svg`

**Interfaces:**
- Produces:
  - root scripts: `pnpm tauri`, `pnpm lint`/`typecheck`/`test` (all run recursively)
  - `ui` scripts: `dev` (port 1420), `dev:mock` (`--mode mock`), `build`, `typecheck`, `lint`, `test`
  - `ui/src/assets/mark.svg`: the land-plot mark, viewBox `0 0 140 120`

- [ ] **Step 1: Write the root workspace files**

`package.json`:

```json
{
  "name": "perland-launcher-workspace",
  "private": true,
  "license": "GPL-3.0-only",
  "packageManager": "pnpm@11.5.0",
  "scripts": {
    "tauri": "tauri",
    "lint": "pnpm -r lint",
    "typecheck": "pnpm -r typecheck",
    "test": "pnpm -r test"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.12.1"
  }
}
```

`pnpm-workspace.yaml`:

```yaml
packages:
  - ui
```

Append to `.gitignore`:

```
# Tauri generated schemas
src-tauri/gen/
```

- [ ] **Step 2: Write the UI package and configs**

`ui/package.json`:

```json
{
  "name": "ui",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "license": "GPL-3.0-only",
  "scripts": {
    "dev": "vite",
    "dev:mock": "vite --mode mock",
    "build": "tsc --noEmit && vite build",
    "typecheck": "tsc --noEmit",
    "lint": "eslint .",
    "test": "vitest run"
  },
  "dependencies": {
    "@fontsource-variable/atkinson-hyperlegible-next": "^5.3.0",
    "@fontsource/atkinson-hyperlegible-mono": "^5.3.0",
    "@fontsource/bungee": "^5.3.0",
    "@tanstack/react-query": "^5.104.1",
    "@tanstack/react-virtual": "^3.14.14",
    "@tauri-apps/api": "^2.12.2",
    "@tauri-apps/plugin-dialog": "^2.8.1",
    "@tauri-apps/plugin-opener": "^2.7.0",
    "i18next": "^26.4.3",
    "pixelarticons": "^2.4.2",
    "react": "^19.3.0",
    "react-dom": "^19.3.0",
    "react-i18next": "^17.0.16",
    "zod": "^4.6.5"
  },
  "devDependencies": {
    "@eslint/js": "^10.0.1",
    "@tailwindcss/vite": "^4.3.3",
    "@testing-library/dom": "^10.4.2",
    "@testing-library/jest-dom": "^7.0.1",
    "@testing-library/react": "^16.3.3",
    "@testing-library/user-event": "^14.6.7",
    "@types/react": "^19.3.0",
    "@types/react-dom": "^19.3.0",
    "@vitejs/plugin-react": "^6.1.2",
    "eslint": "^10.12.0",
    "eslint-plugin-react-hooks": "^7.1.1",
    "eslint-plugin-react-refresh": "^0.5.7",
    "globals": "^17.13.0",
    "jsdom": "^30.1.2",
    "tailwindcss": "^4.3.3",
    "typescript": "~6.0.3",
    "typescript-eslint": "^8.71.1",
    "vite": "^8.3.4",
    "vitest": "^5.0.3"
  }
}
```

`ui/vite.config.ts`:

```ts
/// <reference types="vitest/config" />
import tailwindcss from '@tailwindcss/vite';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  build: { target: 'es2022', sourcemap: true },
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
    css: false,
    restoreMocks: true,
  },
});
```

`ui/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2023", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "noFallthroughCasesInSwitch": true,
    "noEmit": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "verbatimModuleSyntax": true,
    "skipLibCheck": true,
    "types": ["vite/client"]
  },
  "include": ["src", "vite.config.ts"]
}
```

`ui/eslint.config.js`:

```js
import js from '@eslint/js';
import { defineConfig, globalIgnores } from 'eslint/config';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default defineConfig([
  globalIgnores(['dist']),
  {
    files: ['**/*.{ts,tsx}'],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: { ecmaVersion: 2022, globals: globals.browser },
  },
]);
```

If ESLint reports that `reactHooks.configs.flat` is undefined, the plugin moved its flat config. Use the key from `node_modules/eslint-plugin-react-hooks/README.md` (in 7.x it is `configs.flat.recommended`, with `configs['recommended-latest']` as a fallback).

`ui/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Perland Launcher</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`ui/src/vite-env.d.ts`:

```ts
/// <reference types="vite/client" />
```

`ui/src/styles/app.css` (tokens arrive in Task 23):

```css
@import 'tailwindcss';
```

`ui/src/test/setup.ts`:

```ts
import '@testing-library/jest-dom/vitest';
```

- [ ] **Step 3: Draw the land-plot mark**

Create `ui/src/assets/mark.svg`. It is redrawn from the legacy PerlandDev raster (`E:\PerlandDev-site\public\logo.png`): four terraced plots, with dirt steps under the coral and blue ones. The geometry was measured on a 140×120 grid.

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 140 120" shape-rendering="crispEdges" role="img" aria-label="Perland">
  <!-- Perland land-plot mark. Vector redraw of the legacy PerlandDev raster logo
       (E:\PerlandDev-site\public\logo.png), measured on a 140x120 grid, 2026-10-10. -->
  <g fill="#C9CC2A">
    <rect x="41" y="16" width="16" height="13"/>
    <rect x="22" y="29" width="35" height="14"/>
    <rect x="2" y="43" width="55" height="14"/>
  </g>
  <g fill="#FFC233">
    <rect x="61" y="3" width="17" height="13"/>
    <rect x="61" y="16" width="34" height="13"/>
    <rect x="61" y="29" width="53" height="15"/>
  </g>
  <g fill="#7A4A2A">
    <rect x="116" y="60" width="20" height="11"/>
    <rect x="99" y="74" width="17" height="11"/>
    <rect x="83" y="88" width="16" height="11"/>
    <rect x="2" y="61" width="19" height="9"/>
    <rect x="21" y="75" width="18" height="12"/>
    <rect x="39" y="88" width="20" height="12"/>
    <rect x="59" y="104" width="21" height="12"/>
  </g>
  <g fill="#FF5A5F">
    <rect x="83" y="47" width="53" height="13"/>
    <rect x="83" y="60" width="33" height="14"/>
    <rect x="83" y="74" width="16" height="14"/>
  </g>
  <g fill="#2186CB">
    <rect x="21" y="61" width="59" height="14"/>
    <rect x="39" y="75" width="41" height="13"/>
    <rect x="59" y="88" width="21" height="16"/>
  </g>
  <g fill="none" stroke="#3C3C28" stroke-width="1.4" stroke-linecap="square">
    <polyline points="23,37.5 33.5,37.5 33.5,44.5 40,44.5"/>
    <line x1="81.5" y1="16" x2="81.5" y2="23"/>
    <polyline points="112.5,50 112.5,62.5 105.5,62.5 105.5,72"/>
  </g>
</svg>
```

- [ ] **Step 4: Write the failing test and a minimal App**

`ui/src/App.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import App from './App';

it('shows the launcher name', () => {
  render(<App />);
  expect(screen.getByRole('heading', { name: 'Perland Launcher' })).toBeInTheDocument();
});
```

Run `pnpm install` at the repo root, then `pnpm --filter ui test`.
Expected: FAIL, because `./App` doesn't exist.

`ui/src/App.tsx`:

```tsx
import mark from './assets/mark.svg';

export default function App() {
  return (
    <main className="grid min-h-screen place-items-center">
      <img src={mark} alt="" width={96} height={82} />
      <h1>Perland Launcher</h1>
    </main>
  );
}
```

`ui/src/main.tsx`:

```tsx
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import './styles/app.css';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
```

- [ ] **Step 5: Run every UI check**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
pnpm --filter ui build
```

Expected: the test passes; typecheck, lint and build succeed; `ui/dist/index.html` exists.

- [ ] **Step 6: Commit**

```bash
git add package.json pnpm-workspace.yaml pnpm-lock.yaml .gitignore ui
git commit -m "feat(ui): pnpm workspace, React/Vite/Tailwind scaffold and the land-plot mark

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 17: The Tauri shell (`src-tauri`)

**Files:**
- Modify: `Cargo.toml` (workspace members)
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`
- Create: `src-tauri/icons/*` (generated from the mark)
- Create: `src-tauri/src/main.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/host.rs`

**Interfaces:**
- Consumes: everything public on `AppCore` (Tasks 13–14), `CoreRunner` (Task 12), `Host` (Task 10).
- Produces:
  - **Events:** `launcher://event`, whose payload is an `AppEvent`, and `launcher://close-requested` (no payload).
  - **Commands:** invoked from JS by these names, with these camelCase argument keys:
    - `app_info()`
    - `settings_get()`, `settings_set({settings})`
    - `instances_list()`, `instance_create({request})`, `instance_update({instance})`, `instance_rename({id, name})`, `instance_delete({id})`
    - `catalog_minecraft()`, `catalog_loaders({kind, mc})`
    - `accounts_list()`, `account_add_offline({name})`, `account_set_active({uuid})`, `account_remove({uuid})`
    - `play({id})`, `stop({id})`, `run_states()`, `log_tail({id})`
    - `mods_list({id})`, `mod_set_enabled({id, file, enabled})`, `mods_add({id, paths})`, `mod_remove({id, file})`
    - `open_folder({target})`, `java_test({path})`, `quit({stopGames})`

- [ ] **Step 1: Crate, build script and config**

Set the workspace members in the root `Cargo.toml` to `["crates/launcher-core", "crates/launcher-app", "crates/perland-cli", "src-tauri"]`.

`src-tauri/Cargo.toml`:

```toml
[package]
name = "perland-launcher"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Perland Launcher desktop app"

# No test harness: test executables that link Tauri can fail to start on Windows. All logic and
# its tests live in crates/launcher-app.
[[bin]]
name = "perland-launcher"
path = "src/main.rs"
test = false

[build-dependencies]
tauri-build = { version = "2.7.1", features = [] }

[dependencies]
launcher-core = { path = "../crates/launcher-core" }
launcher-app = { path = "../crates/launcher-app" }
tauri = { version = "2.12.2", features = [] }
tauri-plugin-dialog = "2.8.1"
tauri-plugin-opener = "2.7.0"
tauri-plugin-single-instance = "2.5.2"
tauri-plugin-window-state = "2.5.0"
sysinfo = { version = "0.39", default-features = false, features = ["system"] }
```

`src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::build()
}
```

`src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Perland Launcher",
  "version": "0.1.0",
  "identifier": "dev.perland.launcher",
  "build": {
    "beforeDevCommand": "pnpm --filter ui dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm --filter ui build",
    "frontendDist": "../ui/dist"
  },
  "app": {
    "withGlobalTauri": false,
    "windows": [
      {
        "label": "main",
        "title": "Perland Launcher",
        "width": 1200,
        "height": 760,
        "minWidth": 960,
        "minHeight": 600,
        "center": true,
        "decorations": false,
        "dragDropEnabled": true
      }
    ],
    "security": {
      "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src ipc: http://ipc.localhost",
      "devCsp": null
    }
  },
  "bundle": {
    "active": false,
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.ico", "icons/icon.png"]
  }
}
```

`src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "The main window: the app's own commands plus exactly these plugin calls.",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:allow-minimize",
    "core:window:allow-toggle-maximize",
    "core:window:allow-is-maximized",
    "core:window:allow-close",
    "core:window:allow-start-dragging",
    "dialog:allow-open",
    {
      "identifier": "opener:allow-open-url",
      "allow": [{ "url": "https://github.com/PerlandDev/*" }, { "url": "https://www.gnu.org/licenses/*" }]
    }
  ]
}
```

- [ ] **Step 2: Generate the icons from the mark**

Run: `pnpm tauri icon ui/src/assets/mark.svg -o src-tauri/icons`
Expected: `src-tauri/icons/` holds `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.ico`, `icon.png` and more.

If the CLI refuses SVG input, render a 1024×1024 PNG first: open `mark.svg` in the browser pane, screenshot it at 1024 px, save it as `src-tauri/icons/mark-1024.png`, and run `pnpm tauri icon src-tauri/icons/mark-1024.png -o src-tauri/icons`.

- [ ] **Step 3: The host**

`src-tauri/src/host.rs`:

```rust
//! The real window behind `launcher_app::Host`.
use launcher_app::{AppEvent, Host};
use tauri::{AppHandle, Emitter, Manager};

pub const APP_EVENT: &str = "launcher://event";

pub struct TauriHost {
    app: AppHandle,
}

impl TauriHost {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl Host for TauriHost {
    fn emit(&self, event: AppEvent) {
        let _ = self.app.emit(APP_EVENT, event);
    }

    fn minimize(&self) {
        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.minimize();
        }
    }

    fn restore(&self) {
        if let Some(window) = self.app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

/// Total RAM in MB, for the memory slider's soft warning.
pub fn system_memory_mb() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.total_memory() / (1024 * 1024)
}
```

- [ ] **Step 4: The commands**

`src-tauri/src/commands.rs`:

```rust
//! One thin wrapper per UI command. All behavior lives in `launcher_app::AppCore`.
use std::{path::PathBuf, sync::Arc, time::Duration};

use launcher_app::dto::{AppInfo, JavaTest, NewInstance, OpenTarget, RunSnapshot};
use launcher_core::{
    accounts::AccountsFile,
    instances::{Instance, InstanceListing},
    meta::{
        catalog::{LoaderVersion, McVersion},
        loaders::LoaderKind,
    },
    mods::LocalMod,
    settings::Settings,
    ErrorCode, LauncherError,
};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::Core;

type Res<T> = Result<T, LauncherError>;
type CoreState<'a> = State<'a, Arc<Core>>;

#[tauri::command]
pub async fn app_info(core: CoreState<'_>) -> Res<AppInfo> {
    core.app_info().await
}

#[tauri::command]
pub async fn settings_get(core: CoreState<'_>) -> Res<Settings> {
    Ok(core.settings_get().await)
}

#[tauri::command]
pub async fn settings_set(core: CoreState<'_>, settings: Settings) -> Res<Settings> {
    core.settings_set(settings).await
}

#[tauri::command]
pub async fn instances_list(core: CoreState<'_>) -> Res<InstanceListing> {
    core.instances_list().await
}

#[tauri::command]
pub async fn instance_create(core: CoreState<'_>, request: NewInstance) -> Res<Instance> {
    core.instance_create(request).await
}

#[tauri::command]
pub async fn instance_update(core: CoreState<'_>, instance: Instance) -> Res<Instance> {
    core.instance_update(instance).await
}

#[tauri::command]
pub async fn instance_rename(core: CoreState<'_>, id: String, name: String) -> Res<Instance> {
    core.instance_rename(&id, &name).await
}

#[tauri::command]
pub async fn instance_delete(core: CoreState<'_>, id: String) -> Res<()> {
    core.instance_delete(&id).await
}

#[tauri::command]
pub async fn catalog_minecraft(core: CoreState<'_>) -> Res<Vec<McVersion>> {
    core.catalog_minecraft().await
}

#[tauri::command]
pub async fn catalog_loaders(
    core: CoreState<'_>,
    kind: LoaderKind,
    mc: String,
) -> Res<Vec<LoaderVersion>> {
    core.catalog_loaders(kind, &mc).await
}

#[tauri::command]
pub async fn accounts_list(core: CoreState<'_>) -> Res<AccountsFile> {
    core.accounts_list().await
}

#[tauri::command]
pub async fn account_add_offline(core: CoreState<'_>, name: String) -> Res<AccountsFile> {
    core.account_add_offline(&name).await
}

#[tauri::command]
pub async fn account_set_active(core: CoreState<'_>, uuid: String) -> Res<AccountsFile> {
    core.account_set_active(&uuid).await
}

#[tauri::command]
pub async fn account_remove(core: CoreState<'_>, uuid: String) -> Res<AccountsFile> {
    core.account_remove(&uuid).await
}

#[tauri::command]
pub async fn play(core: CoreState<'_>, id: String) -> Res<()> {
    core.inner().play(&id).await
}

#[tauri::command]
pub fn stop(core: CoreState<'_>, id: String) -> Res<()> {
    core.stop(&id)
}

#[tauri::command]
pub fn run_states(core: CoreState<'_>) -> Vec<RunSnapshot> {
    core.run_states()
}

#[tauri::command]
pub fn log_tail(core: CoreState<'_>, id: String) -> Vec<String> {
    core.log_tail(&id)
}

#[tauri::command]
pub async fn mods_list(core: CoreState<'_>, id: String) -> Res<Vec<LocalMod>> {
    core.mods_list(&id).await
}

#[tauri::command]
pub async fn mod_set_enabled(
    core: CoreState<'_>,
    id: String,
    file: String,
    enabled: bool,
) -> Res<Vec<LocalMod>> {
    core.mod_set_enabled(&id, &file, enabled).await
}

#[tauri::command]
pub async fn mods_add(core: CoreState<'_>, id: String, paths: Vec<PathBuf>) -> Res<Vec<LocalMod>> {
    core.mods_add(&id, paths).await
}

#[tauri::command]
pub async fn mod_remove(core: CoreState<'_>, id: String, file: String) -> Res<Vec<LocalMod>> {
    core.mod_remove(&id, &file).await
}

#[tauri::command]
pub async fn open_folder(app: AppHandle, core: CoreState<'_>, target: OpenTarget) -> Res<()> {
    let path = core.folder(target).await?;
    app.opener()
        .open_path(path.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| {
            LauncherError::disk(ErrorCode::Internal, "Couldn't open the folder").with_detail(e)
        })
}

#[tauri::command]
pub async fn java_test(core: CoreState<'_>, path: PathBuf) -> Res<JavaTest> {
    core.java_test(path).await
}

/// Called by the close dialog. Stopping waits a few seconds so the kills land before exit.
#[tauri::command]
pub async fn quit(app: AppHandle, core: CoreState<'_>, stop_games: bool) -> Res<()> {
    if stop_games {
        core.stop_all(Duration::from_secs(5)).await;
    }
    app.exit(0);
    Ok(())
}
```

- [ ] **Step 5: Wire it together**

`src-tauri/src/main.rs`:

```rust
// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod host;

use std::{path::PathBuf, sync::Arc};

use launcher_app::{AppConfig, AppCore, CoreRunner};
use launcher_core::{
    accounts::BuildKind, download::Downloader, install::Installer, meta::MetaClient,
    paths::LauncherDirs, platform::Platform,
};
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_window_state::StateFlags;

pub type Core = AppCore<CoreRunner, host::TauriHost>;

const CLOSE_REQUESTED: &str = "launcher://close-requested";

fn main() {
    tauri::Builder::default()
        // Must be first: a second launch focuses the running launcher instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let core = build_core(app.handle().clone())?;
            app.manage(core);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing while a game runs asks first (the UI shows the dialog).
            if let WindowEvent::CloseRequested { api, .. } = event {
                if let Some(core) = window.try_state::<Arc<Core>>() {
                    if core.has_active_runs() {
                        api.prevent_close();
                        let _ = window.emit(CLOSE_REQUESTED, ());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::settings_get,
            commands::settings_set,
            commands::instances_list,
            commands::instance_create,
            commands::instance_update,
            commands::instance_rename,
            commands::instance_delete,
            commands::catalog_minecraft,
            commands::catalog_loaders,
            commands::accounts_list,
            commands::account_add_offline,
            commands::account_set_active,
            commands::account_remove,
            commands::play,
            commands::stop,
            commands::run_states,
            commands::log_tail,
            commands::mods_list,
            commands::mod_set_enabled,
            commands::mods_add,
            commands::mod_remove,
            commands::open_folder,
            commands::java_test,
            commands::quit,
        ])
        .run(tauri::generate_context!())
        .expect("Perland Launcher failed to start");
}

/// `PERLAND_LAUNCHER_ROOT` points development runs at a scratch data folder.
fn build_core(app: tauri::AppHandle) -> Result<Arc<Core>, Box<dyn std::error::Error>> {
    let root = std::env::var_os("PERLAND_LAUNCHER_ROOT")
        .map(PathBuf::from)
        .or_else(LauncherDirs::default_root)
        .ok_or("couldn't find a data folder")?;
    let dirs = LauncherDirs::new(root);
    let version = env!("CARGO_PKG_VERSION").to_string();
    let http = launcher_core::http::client(&version)?;
    let meta = MetaClient::new(http.clone(), dirs.meta());
    let installer = Arc::new(Installer {
        dirs: dirs.clone(),
        meta: meta.clone(),
        downloader: Downloader::new(http),
        platform: Platform::current(),
    });
    let runner = Arc::new(CoreRunner::new(installer, version.clone()));
    let host = Arc::new(host::TauriHost::new(app));
    let config = AppConfig {
        dirs,
        build: BuildKind::current(),
        launcher_version: version,
        system_memory_mb: host::system_memory_mb(),
    };
    Ok(AppCore::new(config, meta, runner, host))
}
```

- [ ] **Step 6: Build and run it**

```bash
pnpm --filter ui build
cargo build -p perland-launcher
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: everything compiles cleanly. Tauri embeds `ui/dist`, so `pnpm --filter ui build` must have run at least once before any cargo command that compiles `src-tauri`. CI does this too (Task 18).

Then, in PowerShell:

```powershell
$env:PERLAND_LAUNCHER_ROOT = "$PWD\target\pl-dev"; pnpm tauri dev
```

Expected: a frameless 1200×760 window shows the mark and "Perland Launcher". Drag it with Win+arrow keys only, since there's no drag region yet. Close it from the taskbar.

- [ ] **Step 7: Run the full Rust gate and commit**

```bash
cargo fmt --all
cargo test --workspace
git add Cargo.toml Cargo.lock src-tauri
git commit -m "feat(shell): Tauri 2 shell wiring AppCore commands, events and the close guard

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: tests pass (`src-tauri` has no test harness by design).

---

### Task 18: CI and contributor docs

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `CLAUDE.md`, `AGENTS.md`, `README.md`, `NOTICE`

- [ ] **Step 1: Replace `ci.yml`**

```yaml
name: CI
on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, ubuntu-latest]
    runs-on: ${{ matrix.os }}
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@v4
      - name: Linux webview build dependencies
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
      - uses: pnpm/action-setup@v4
        with:
          version: 11
      - uses: actions/setup-node@v4
        with:
          node-version: 24
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - run: pnpm --filter ui build
      - run: pnpm -r lint
      - run: pnpm -r typecheck
      - run: pnpm -r test
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
```

- [ ] **Step 2: Update `CLAUDE.md` and `AGENTS.md` (identical content)**

Replace both files with:

```markdown
# Perland Launcher: agent guide

- Design: `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (core) and `docs/superpowers/specs/2026-10-10-launcher-desktop-ui-design.md` (desktop app). Plans: `docs/superpowers/plans/`.
- `crates/launcher-core` holds all launcher logic. `crates/launcher-app` holds the app's state and run state machine. Neither may depend on Tauri or UI crates. `src-tauri` is wiring only and has no tests. `crates/perland-cli` is a dev harness. `ui/` is the React app.
- TDD: write the failing test first. Pure logic gets `#[cfg(test)]` unit tests; HTTP behaviour gets `tests/*.rs` with wiremock; UI gets Vitest + Testing Library.
- Never run fuzzing, stress or load tests locally. They run in CI only. Keep local test inputs tiny.
- Errors shown to players are `LauncherError` (category + stable `code` + plain message + optional fix + detail + params). The UI translates by `code`. Never put tokens in logs.
- Every UI string goes in both `ui/src/i18n/en.json` and `ui/src/i18n/es.json` (neutral Latin American Spanish, informal "tú"). A new `ErrorCode` needs `errors.<code>.message` in both.
- `ui/src/ipc/contract/*.json` is generated: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit it.
- UI styling and design-system work goes through the `/impeccable` skill. The visual direction is `.impeccable/surfaces/ui-src-app-tsx.md`, and the design system is `DESIGN.md`.
- Run the app: `pnpm install`, then `pnpm --filter ui dev:mock` (browser, mocked backend) or `pnpm tauri dev` (real app; set `PERLAND_LAUNCHER_ROOT` to a scratch folder).
- License GPL-3.0-only. Never copy Modrinth branding or Mojang assets. Ported code keeps an attribution header and gets a `NOTICE` entry.
- Before committing: `pnpm --filter ui build` (Tauri embeds `ui/dist`), `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`.
```

- [ ] **Step 3: Update `README.md` and `NOTICE`**

In `README.md`'s Develop section, add:

```markdown
### Desktop app

    pnpm install
    pnpm --filter ui dev:mock      # UI in a browser with a mocked backend
    pnpm tauri dev                 # the real app (PowerShell: $env:PERLAND_LAUNCHER_ROOT="$PWD\target\pl-dev")
```

Append to `NOTICE`:

```
Bundled fonts (SIL Open Font License 1.1):
  Bungee, by David Jonathan Ross
  Atkinson Hyperlegible Next and Atkinson Hyperlegible Mono, by the Braille Institute of America

Icons: pixelarticons by Gerrit Halfmann (MIT License)
```

- [ ] **Step 4: Verify locally and commit**

```bash
pnpm -r lint && pnpm -r typecheck && pnpm -r test
cargo fmt --all --check
git add .github CLAUDE.md AGENTS.md README.md NOTICE
git commit -m "ci: build and test the UI and the Tauri shell on Windows and Linux

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
## Phase 4: UI foundation

### Task 19: IPC schemas, typed commands and event decoding

**Files:**
- Create: `ui/src/ipc/schema.ts`, `ui/src/ipc/commands.ts`, `ui/src/ipc/events.ts`
- Test: `ui/src/ipc/contract.test.ts`, `ui/src/ipc/commands.test.ts`
- Modify: `ui/src/test/setup.ts`

**Interfaces:**
- Consumes: `ui/src/ipc/contract/*.json` (Task 15), and command names and argument keys (Task 17).
- Produces:
  - zod schemas plus inferred types: `LauncherError`, `ErrorCategory`, `LoaderKind`, `LoaderSpec`, `GameSpec`, `SourceRef`, `JavaSettings`, `Instance`, `DamagedInstance`, `InstanceListing`, `Account`, `AccountsFile`, `Lang`, `Settings`, `AppInfo`, `McVersion`, `LoaderVersion`, `LocalMod`, `NewInstance`, `OpenTarget`, `RunStateKind`, `CrashHint`, `ExitInfo`, `TaskId`, `AppEvent`, `RunSnapshot`, `JavaTest`. Each schema is exported in camelCase (for example `instance`, `appEvent`).
  - `commands.*`, one typed wrapper per command (listed in Step 4).
  - `decodeEvent(raw): AppEvent | null`
  - `onAppEvent(handler): Promise<UnlistenFn>`
  - `onCloseRequested(handler): Promise<UnlistenFn>`
  - `APP_EVENT`, `CLOSE_REQUESTED`

- [ ] **Step 1: Write the failing contract test**

`ui/src/ipc/contract.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { ZodType } from 'zod';
import accounts from './contract/accounts.json';
import appInfo from './contract/app_info.json';
import errorFx from './contract/error.json';
import errorCodes from './contract/error_codes.json';
import events from './contract/events.json';
import instanceFx from './contract/instance.json';
import instanceListing from './contract/instance_listing.json';
import javaTest from './contract/java_test.json';
import loaderVersions from './contract/loader_versions.json';
import localMods from './contract/local_mods.json';
import mcVersions from './contract/mc_versions.json';
import newInstance from './contract/new_instance.json';
import openTargets from './contract/open_targets.json';
import runSnapshots from './contract/run_snapshots.json';
import settingsFx from './contract/settings.json';
import { decodeEvent } from './events';
import * as s from './schema';

const cases: [string, ZodType, unknown][] = [
  ['accounts', s.accountsFile, accounts],
  ['app_info', s.appInfo, appInfo],
  ['error', s.launcherError, errorFx],
  ['instance', s.instance, instanceFx],
  ['instance_listing', s.instanceListing, instanceListing],
  ['java_test', s.javaTest, javaTest],
  ['loader_versions', s.loaderVersion.array(), loaderVersions],
  ['local_mods', s.localMod.array(), localMods],
  ['mc_versions', s.mcVersion.array(), mcVersions],
  ['new_instance', s.newInstance, newInstance],
  ['open_targets', s.openTarget.array(), openTargets],
  ['run_snapshots', s.runSnapshot.array(), runSnapshots],
  ['settings', s.settings, settingsFx],
  ['events', s.appEvent.array(), events],
];

describe('Rust contract fixtures', () => {
  it.each(cases)('%s matches its schema exactly', (_name, schema, fixture) => {
    const result = schema.safeParse(fixture);
    expect(result.success, JSON.stringify(result.error?.issues)).toBe(true);
  });

  it('lists error codes as snake_case strings', () => {
    expect(errorCodes.length).toBeGreaterThan(40);
    for (const code of errorCodes) expect(code).toMatch(/^[a-z0-9_]+$/);
  });

  it('decodes every event and rejects junk', () => {
    for (const e of events) expect(decodeEvent(e)).not.toBeNull();
    expect(decodeEvent({ type: 'nope' })).toBeNull();
    expect(decodeEvent({ type: 'log', instance: 'x' })).toBeNull();
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm --filter ui test`
Expected: FAIL, because `./schema` and `./events` don't exist.

- [ ] **Step 3: Write the schemas**

`ui/src/ipc/schema.ts`:

```ts
// Mirrors of the Rust types the UI receives. Strict objects, so a field added or renamed in Rust
// fails the contract test (ui/src/ipc/contract.test.ts) instead of being silently ignored.
import { z } from 'zod';

export const errorCategory = z.enum(['network', 'auth', 'integrity', 'disk', 'java', 'loader', 'pack', 'game']);
export const launcherError = z.strictObject({
  category: errorCategory,
  code: z.string(),
  message: z.string(),
  fix: z.string().nullable(),
  detail: z.string().nullable(),
  params: z.record(z.string(), z.string()),
});

export const loaderKind = z.enum(['fabric', 'quilt', 'forge', 'neoforge']);
export const loaderSpec = z.strictObject({ kind: loaderKind, version: z.string() });
export const gameSpec = z.strictObject({ minecraft: z.string(), loader: loaderSpec.nullable() });
export const sourceRef = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('none') }),
  z.strictObject({ kind: z.literal('packwiz'), url: z.string() }),
  z.strictObject({ kind: z.literal('mrpack'), location: z.string() }),
  z.strictObject({ kind: z.literal('modrinth'), projectId: z.string(), versionId: z.string() }),
]);
export const javaSettings = z.strictObject({ path: z.string().nullable(), memoryMb: z.number().int() });
export const instance = z.strictObject({
  id: z.string(),
  name: z.string(),
  source: sourceRef,
  game: gameSpec,
  updatePolicy: z.enum(['auto', 'ask', 'pinned']),
  optionalChoices: z.record(z.string(), z.boolean()),
  java: javaSettings,
});
export const damagedInstance = z.strictObject({ id: z.string(), error: launcherError });
export const instanceListing = z.strictObject({ instances: z.array(instance), damaged: z.array(damagedInstance) });

export const account = z.strictObject({ provider: z.enum(['microsoft', 'offline']), username: z.string(), uuid: z.string() });
export const accountsFile = z.strictObject({
  accounts: z.array(account),
  active: z.string().nullable(),
  licensedSigninSeen: z.boolean(),
});

export const lang = z.enum(['en', 'es']);
export const settings = z.strictObject({
  version: z.number().int(),
  language: lang.nullable(),
  defaultMemoryMb: z.number().int(),
  minimizeOnPlay: z.boolean(),
  firstRunDone: z.boolean(),
});
export const appInfo = z.strictObject({
  version: z.string(),
  build: z.enum(['dev', 'release']),
  offlineAllowed: z.boolean(),
  systemMemoryMb: z.number().int(),
  dataDir: z.string(),
});

export const mcVersion = z.strictObject({
  id: z.string(),
  kind: z.enum(['release', 'snapshot', 'old_beta', 'old_alpha']),
  releaseTime: z.string(),
});
export const loaderVersion = z.strictObject({ version: z.string(), stable: z.boolean() });
export const localMod = z.strictObject({ fileName: z.string(), enabled: z.boolean(), size: z.number().int() });
export const newInstance = z.strictObject({ name: z.string(), minecraft: z.string(), loader: loaderSpec.nullable() });
export const openTarget = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('instance'), id: z.string() }),
  z.strictObject({ kind: z.literal('mods'), id: z.string() }),
  z.strictObject({ kind: z.literal('data') }),
  z.strictObject({ kind: z.literal('logs') }),
]);

export const runStateKind = z.enum(['idle', 'queued', 'preparing', 'running', 'stopping']);
export const crashHint = z.enum(['out_of_memory']);
export const exitInfo = z.strictObject({
  code: z.number().int().nullable(),
  killed: z.boolean(),
  cancelled: z.boolean(),
  crash: crashHint.nullable(),
  error: launcherError.nullable(),
});
export const taskId = z.strictObject({ key: z.string(), params: z.record(z.string(), z.string()) });
export const appEvent = z.discriminatedUnion('type', [
  z.strictObject({
    type: z.literal('progress'),
    instance: z.string().nullable(),
    task: taskId,
    done: z.number(),
    total: z.number(),
    bytes: z.number(),
  }),
  z.strictObject({ type: z.literal('log'), instance: z.string(), lines: z.array(z.string()) }),
  z.strictObject({
    type: z.literal('run_state'),
    instance: z.string(),
    state: runStateKind,
    exit: exitInfo.nullable(),
  }),
  z.strictObject({ type: z.literal('warning'), instance: z.string().nullable(), error: launcherError }),
]);
export const runSnapshot = z.strictObject({ instance: z.string(), state: runStateKind });
export const javaTest = z.strictObject({ major: z.number().int() });

export type ErrorCategory = z.infer<typeof errorCategory>;
export type LauncherError = z.infer<typeof launcherError>;
export type LoaderKind = z.infer<typeof loaderKind>;
export type LoaderSpec = z.infer<typeof loaderSpec>;
export type GameSpec = z.infer<typeof gameSpec>;
export type SourceRef = z.infer<typeof sourceRef>;
export type JavaSettings = z.infer<typeof javaSettings>;
export type Instance = z.infer<typeof instance>;
export type DamagedInstance = z.infer<typeof damagedInstance>;
export type InstanceListing = z.infer<typeof instanceListing>;
export type Account = z.infer<typeof account>;
export type AccountsFile = z.infer<typeof accountsFile>;
export type Lang = z.infer<typeof lang>;
export type Settings = z.infer<typeof settings>;
export type AppInfo = z.infer<typeof appInfo>;
export type McVersion = z.infer<typeof mcVersion>;
export type LoaderVersion = z.infer<typeof loaderVersion>;
export type LocalMod = z.infer<typeof localMod>;
export type NewInstance = z.infer<typeof newInstance>;
export type OpenTarget = z.infer<typeof openTarget>;
export type RunStateKind = z.infer<typeof runStateKind>;
export type CrashHint = z.infer<typeof crashHint>;
export type ExitInfo = z.infer<typeof exitInfo>;
export type TaskId = z.infer<typeof taskId>;
export type AppEvent = z.infer<typeof appEvent>;
export type RunSnapshot = z.infer<typeof runSnapshot>;
export type JavaTest = z.infer<typeof javaTest>;
```

`ui/src/ipc/events.ts`:

```ts
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { appEvent, type AppEvent } from './schema';

export const APP_EVENT = 'launcher://event';
export const CLOSE_REQUESTED = 'launcher://close-requested';

/** Parse one event from Rust; anything that doesn't match the schema is dropped with a warning. */
export function decodeEvent(raw: unknown): AppEvent | null {
  const result = appEvent.safeParse(raw);
  if (!result.success) {
    console.warn('Ignoring an event the UI does not understand', result.error.issues);
    return null;
  }
  return result.data;
}

export function onAppEvent(handler: (event: AppEvent) => void): Promise<UnlistenFn> {
  return listen<unknown>(APP_EVENT, (message) => {
    const event = decodeEvent(message.payload);
    if (event) handler(event);
  });
}

export function onCloseRequested(handler: () => void): Promise<UnlistenFn> {
  return listen(CLOSE_REQUESTED, () => handler());
}
```

- [ ] **Step 4: Write the commands and their test**

`ui/src/ipc/commands.test.ts`:

```ts
import { mockIPC } from '@tauri-apps/api/mocks';
import { expect, it } from 'vitest';
import { commands } from './commands';

it('sends the argument names Rust expects', async () => {
  const calls: [string, unknown][] = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    return null;
  });
  await commands.play('pack');
  await commands.modSetEnabled('pack', 'a.jar', false);
  await commands.quit(true);
  await commands.catalogLoaders('quilt', '1.20.1');
  expect(calls).toEqual([
    ['play', { id: 'pack' }],
    ['mod_set_enabled', { id: 'pack', file: 'a.jar', enabled: false }],
    ['quit', { stopGames: true }],
    ['catalog_loaders', { kind: 'quilt', mc: '1.20.1' }],
  ]);
});

it('rejects with the LauncherError Rust returned', async () => {
  mockIPC(() => {
    throw { category: 'game', code: 'already_running', message: 'busy', fix: null, detail: null, params: {} };
  });
  await expect(commands.play('pack')).rejects.toMatchObject({ code: 'already_running' });
});
```

`ui/src/ipc/commands.ts`:

```ts
import { invoke } from '@tauri-apps/api/core';
import type {
  AccountsFile,
  AppInfo,
  Instance,
  InstanceListing,
  JavaTest,
  LoaderKind,
  LoaderVersion,
  LocalMod,
  McVersion,
  NewInstance,
  OpenTarget,
  RunSnapshot,
  Settings,
} from './schema';

/** The only place the UI calls into Rust. A failed call rejects with a `LauncherError` object. */
export const commands = {
  appInfo: () => invoke<AppInfo>('app_info'),
  settingsGet: () => invoke<Settings>('settings_get'),
  settingsSet: (settings: Settings) => invoke<Settings>('settings_set', { settings }),
  instancesList: () => invoke<InstanceListing>('instances_list'),
  instanceCreate: (request: NewInstance) => invoke<Instance>('instance_create', { request }),
  instanceUpdate: (instance: Instance) => invoke<Instance>('instance_update', { instance }),
  instanceRename: (id: string, name: string) => invoke<Instance>('instance_rename', { id, name }),
  instanceDelete: (id: string) => invoke<null>('instance_delete', { id }),
  catalogMinecraft: () => invoke<McVersion[]>('catalog_minecraft'),
  catalogLoaders: (kind: LoaderKind, mc: string) => invoke<LoaderVersion[]>('catalog_loaders', { kind, mc }),
  accountsList: () => invoke<AccountsFile>('accounts_list'),
  accountAddOffline: (name: string) => invoke<AccountsFile>('account_add_offline', { name }),
  accountSetActive: (uuid: string) => invoke<AccountsFile>('account_set_active', { uuid }),
  accountRemove: (uuid: string) => invoke<AccountsFile>('account_remove', { uuid }),
  play: (id: string) => invoke<null>('play', { id }),
  stop: (id: string) => invoke<null>('stop', { id }),
  runStates: () => invoke<RunSnapshot[]>('run_states'),
  logTail: (id: string) => invoke<string[]>('log_tail', { id }),
  modsList: (id: string) => invoke<LocalMod[]>('mods_list', { id }),
  modSetEnabled: (id: string, file: string, enabled: boolean) =>
    invoke<LocalMod[]>('mod_set_enabled', { id, file, enabled }),
  modsAdd: (id: string, paths: string[]) => invoke<LocalMod[]>('mods_add', { id, paths }),
  modRemove: (id: string, file: string) => invoke<LocalMod[]>('mod_remove', { id, file }),
  openFolder: (target: OpenTarget) => invoke<null>('open_folder', { target }),
  javaTest: (path: string) => invoke<JavaTest>('java_test', { path }),
  quit: (stopGames: boolean) => invoke<null>('quit', { stopGames }),
};
```

Replace `ui/src/test/setup.ts` with:

```ts
import '@testing-library/jest-dom/vitest';
import { clearMocks } from '@tauri-apps/api/mocks';
import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';

// jsdom may lack <dialog> methods; the app relies on showModal/close.
if (typeof HTMLDialogElement !== 'undefined' && !HTMLDialogElement.prototype.showModal) {
  HTMLDialogElement.prototype.showModal = function showModal(this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close = function close(this: HTMLDialogElement) {
    this.removeAttribute('open');
    this.dispatchEvent(new Event('close'));
  };
}

afterEach(() => {
  cleanup();
  clearMocks();
});
```

- [ ] **Step 5: Run the tests, typecheck, lint and commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): zod schemas mirroring the Rust contract, typed commands and event decoding

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS. If a fixture fails its schema, the schema is wrong (the fixture is generated from Rust). Fix the schema, never the JSON.

---

### Task 20: English and Spanish, translated errors and task labels

**Files:**
- Create: `ui/src/i18n/index.ts`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Create: `ui/src/ipc/errors.ts`
- Test: `ui/src/i18n/i18n.test.ts`, `ui/src/ipc/errors.test.ts`
- Modify: `ui/src/test/setup.ts` (load i18n, pin English)

**Interfaces:**
- Produces:
  - `detectLang(language?: string): Lang`
  - the default export `i18n` (initialized, with `escapeValue: false`)
  - `isLauncherError(value): value is LauncherError`
  - `describeError(i18n, error: unknown): { message, fix, detail }`
  - `taskLabel(i18n, task: TaskId): string`
  - `supportText(lines: string[]): string`
  - translation keys used by every later task: the groups in `en.json` below

- [ ] **Step 1: Write the failing tests**

`ui/src/i18n/i18n.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import errorCodes from '../ipc/contract/error_codes.json';
import en from './en.json';
import es from './es.json';
import { detectLang } from './index';

type Tree = { [key: string]: string | Tree };

function flatten(tree: Tree, prefix = ''): Record<string, string> {
  return Object.entries(tree).reduce<Record<string, string>>((out, [key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof value === 'string') out[path] = value;
    else Object.assign(out, flatten(value, path));
    return out;
  }, {});
}

const placeholders = (text: string) => [...text.matchAll(/{{(\w+)}}/g)].map((m) => m[1]).sort();

describe('translations', () => {
  const flatEn = flatten(en as Tree);
  const flatEs = flatten(es as Tree);

  it('have the same keys in English and Spanish', () => {
    expect(Object.keys(flatEs).sort()).toEqual(Object.keys(flatEn).sort());
  });

  it('use the same placeholders in both languages', () => {
    for (const [key, text] of Object.entries(flatEn)) {
      expect(placeholders(flatEs[key] ?? ''), key).toEqual(placeholders(text));
    }
  });

  it('translate every error code the core can produce', () => {
    for (const code of errorCodes) {
      expect(flatEn[`errors.${code}.message`], code).toBeTruthy();
      expect(flatEs[`errors.${code}.message`], code).toBeTruthy();
    }
  });

  it('never leave a Spanish string identical to a long English one by accident', () => {
    const same = Object.keys(flatEn).filter(
      (k) => flatEn[k] === flatEs[k] && (flatEn[k]?.length ?? 0) > 24 && !k.startsWith('app.'),
    );
    expect(same).toEqual([]);
  });
});

describe('detectLang', () => {
  it('picks Spanish for any Spanish locale and English otherwise', () => {
    expect(detectLang('es-AR')).toBe('es');
    expect(detectLang('ES')).toBe('es');
    expect(detectLang('en-US')).toBe('en');
    expect(detectLang('pt-BR')).toBe('en');
    expect(detectLang(undefined)).toBe('en');
  });
});
```

`ui/src/ipc/errors.test.ts`:

```ts
import { afterEach, describe, expect, it } from 'vitest';
import i18n from '../i18n';
import { describeError, isLauncherError, supportText, taskLabel } from './errors';

const notFound = {
  category: 'loader',
  code: 'minecraft_version_not_found',
  message: "Minecraft 9.9 doesn't exist",
  fix: 'Check the version number',
  detail: 'manifest has no 9.9',
  params: { version: '9.9' },
} as const;

afterEach(() => i18n.changeLanguage('en'));

describe('describeError', () => {
  it('translates by code with params', async () => {
    await i18n.changeLanguage('es');
    const text = describeError(i18n, notFound);
    expect(text.message).toBe('Minecraft 9.9 no existe.');
    expect(text.fix).toBe('Elige una versión de la lista.');
    expect(text.detail).toBe('manifest has no 9.9');
  });

  it("falls back to the core's English for codes it doesn't know", () => {
    const text = describeError(i18n, { ...notFound, code: 'brand_new_code' });
    expect(text.message).toBe("Minecraft 9.9 doesn't exist");
    expect(text.fix).toBe('Check the version number');
  });

  it('wraps anything that is not a LauncherError', () => {
    expect(isLauncherError(new Error('x'))).toBe(false);
    const text = describeError(i18n, new Error('boom'));
    expect(text.message).toBe('Something unexpected went wrong.');
    expect(text.detail).toBe('boom');
  });
});

it('labels progress tasks with their params', async () => {
  expect(taskLabel(i18n, { key: 'install_minecraft', params: { version: '1.20.1' } })).toBe(
    'Installing Minecraft 1.20.1',
  );
  expect(taskLabel(i18n, { key: 'mystery', params: {} })).toBe('mystery');
});

it('builds support text from lines', () => {
  expect(supportText(['a', 'b'])).toBe('a\nb');
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter ui test`
Expected: FAIL (missing modules).

- [ ] **Step 3: Write `en.json`**

`ui/src/i18n/en.json`:

```json
{
  "app": { "name": "Perland Launcher" },
  "common": {
    "cancel": "Cancel",
    "close": "Close",
    "save": "Save",
    "saving": "Saving…",
    "saved": "Saved",
    "delete": "Delete",
    "rename": "Rename",
    "openFolder": "Open folder",
    "back": "Back",
    "comingSoon": "Coming soon",
    "details": "Details",
    "hideDetails": "Hide details",
    "copy": "Copy",
    "copied": "Copied",
    "retry": "Try again",
    "loading": "Loading…",
    "continue": "Continue",
    "dismiss": "Dismiss"
  },
  "titlebar": {
    "minimize": "Minimize",
    "maximize": "Maximize",
    "restore": "Restore down",
    "close": "Close",
    "accounts": "Accounts",
    "settings": "Settings",
    "noAccount": "Add account"
  },
  "rail": {
    "label": "Instances",
    "newInstance": "New instance",
    "playing": "Playing",
    "preparing": "Getting ready",
    "damaged": "Needs attention"
  },
  "home": {
    "emptyTitle": "No instances yet",
    "emptyBody": "An instance is one Minecraft setup: a version, a loader and its own worlds.",
    "emptyAction": "Create an instance",
    "minecraftLine": "Minecraft {{version}}",
    "loaderLine": "{{loader}} {{version}}",
    "vanilla": "Vanilla",
    "mods": "Mods",
    "more": "More options",
    "instanceSettings": "Instance settings",
    "renameTitle": "Rename instance",
    "nameLabel": "Name",
    "deleteTitle": "Delete {{name}}?",
    "deleteBody": "This removes the instance and everything in it, including worlds and screenshots. It can't be undone.",
    "deleteConfirm": "Delete instance"
  },
  "play": {
    "play": "Play",
    "stop": "Stop",
    "stopping": "Stopping…",
    "cancel": "Cancel",
    "cancelLabel": "Cancel: {{label}}",
    "queued": "Waiting for another download…",
    "preparing": "Getting ready…",
    "files": "{{done}} of {{total}} files",
    "noAccount": "Add an account to play"
  },
  "progress": {
    "install_minecraft": "Installing Minecraft {{version}}",
    "download_java": "Downloading Java {{version}}"
  },
  "activity": {
    "title": "Activity",
    "open": "Show activity",
    "close": "Hide activity",
    "empty": "Nothing is happening right now. Press Play and the game's output shows up here.",
    "log": "Game log",
    "logEmpty": "The game hasn't printed anything yet.",
    "crashTitle": "{{name}} closed unexpectedly",
    "crashCode": "Exit code {{code}}",
    "oomTitle": "Minecraft ran out of memory",
    "oomBody": "Give it more memory and press Play again.",
    "raiseMemory": "Raise to {{gb}} GB",
    "failedTitle": "{{name}} couldn't start",
    "copyForSupport": "Copy for support",
    "stopped": "Stopped",
    "jumpToEnd": "Jump to the latest line"
  },
  "newInstance": {
    "title": "New instance",
    "name": "Name",
    "namePlaceholder": "Minecraft {{version}}",
    "version": "Minecraft version",
    "showSnapshots": "Show snapshots",
    "loader": "Mod loader",
    "loaderNone": "None",
    "loaderFabric": "Fabric",
    "loaderQuilt": "Quilt",
    "loaderVersion": "Loader version",
    "stable": "{{version}} (stable)",
    "beta": "{{version}} (beta)",
    "noLoaderForVersion": "{{loader}} isn't available for this Minecraft version yet.",
    "packLink": "Paste a pack link",
    "mrpack": "Import a .mrpack file",
    "create": "Create instance",
    "creating": "Creating…",
    "versionsFailed": "Couldn't load the version list."
  },
  "instanceSettings": {
    "title": "Instance settings",
    "game": "Game",
    "memory": "Memory",
    "memoryValue": "{{gb}} GB",
    "memoryHint": "This computer has {{total}} GB.",
    "memoryHigh": "That's more than half of this computer's memory. Other apps may slow down.",
    "java": "Java",
    "javaAuto": "Automatic (recommended)",
    "javaCustom": "A Java installation I choose",
    "javaPick": "Choose…",
    "javaNone": "No Java chosen yet",
    "javaTest": "Test",
    "javaOk": "Java {{major}} works",
    "javaFiles": "Java program",
    "applyNote": "Changes apply the next time you press Play."
  },
  "damaged": {
    "title": "“{{id}}” can't be opened",
    "deleteTitle": "Delete “{{id}}”?",
    "deleteBody": "This removes the folder and everything in it. It can't be undone."
  },
  "mods": {
    "title": "Mods",
    "count_one": "{{count}} mod",
    "count_other": "{{count}} mods",
    "add": "Add mods",
    "dropHere": "Drop .jar files to add them",
    "empty": "No mods yet. Add .jar files, or drop them anywhere on this screen.",
    "vanillaNote": "Mods need Fabric or Quilt. You can switch this instance's loader in its settings.",
    "toggle": "Use {{name}}",
    "remove": "Remove {{name}}",
    "removeTitle": "Remove {{name}}?",
    "removeBody": "The file is deleted from this instance's mods folder.",
    "removeConfirm": "Remove",
    "pickTitle": "Choose mods",
    "jarFiles": "Mod files"
  },
  "accounts": {
    "title": "Accounts",
    "inUse": "In use",
    "use": "Use",
    "remove": "Remove",
    "removeLabel": "Remove {{name}}",
    "removeTitle": "Remove {{name}}?",
    "removeBody": "You can add it again any time. Your worlds stay where they are.",
    "addOffline": "Add offline account",
    "offlineName": "Player name",
    "offlineHint": "3–16 letters, numbers or underscores. Other players see this name.",
    "offlineLocked": "Offline accounts unlock after you sign in once with a Microsoft account that owns Minecraft.",
    "microsoft": "Sign in with Microsoft",
    "offlineBadge": "Offline",
    "empty": "No accounts yet."
  },
  "settings": {
    "title": "Settings",
    "language": "Language",
    "languageSystem": "Same as this computer",
    "english": "English",
    "spanish": "Español",
    "memory": "Memory for new instances",
    "minimize": "Minimize while playing",
    "minimizeHint": "The launcher comes back when the game closes.",
    "dataFolder": "Data folder",
    "about": "About",
    "version": "Version {{version}}",
    "devBuild": "Developer build",
    "license": "Free software under the GNU General Public License v3.",
    "licenseLink": "Read the license",
    "source": "Source code",
    "notOfficial": "Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft."
  },
  "firstRun": {
    "welcomeTitle": "Welcome to Perland",
    "welcomeBody": "Free Minecraft, set up in a few clicks.",
    "start": "Let's go",
    "languageTitle": "Pick your language",
    "accountTitle": "Who's playing?",
    "accountBody": "Pick the name other players will see.",
    "accountLockedBody": "Microsoft sign-in is coming soon. You can set up the rest now and add an account later.",
    "skip": "Skip for now",
    "instanceTitle": "Your first instance",
    "instanceBody": "Start with the newest Minecraft, or pick a version.",
    "latestRelease": "Newest Minecraft ({{version}})",
    "chooseVersion": "Pick a version",
    "finish": "Start playing",
    "step": "Step {{n}} of {{total}}"
  },
  "closeGuard": {
    "title": "A game is still running",
    "body": "Quitting the launcher stops it. Minimize the launcher instead to keep playing.",
    "minimize": "Minimize instead",
    "quit": "Quit and stop games"
  },
  "errors": {
    "offline_name_invalid": { "message": "Player names must be 3–16 letters, numbers or underscores." },
    "accounts_damaged": { "message": "Your accounts file is damaged.", "fix": "Remove accounts.json from the data folder and add your accounts again." },
    "offline_locked": { "message": "Offline accounts unlock after you sign in once with a Microsoft account that owns Minecraft.", "fix": "Microsoft sign-in is coming soon." },
    "microsoft_unavailable": { "message": "Microsoft accounts aren't supported in this version yet." },
    "no_account": { "message": "Add an account before playing.", "fix": "Open Accounts and add one." },
    "disk_io": { "message": "Couldn't read or write a launcher file.", "fix": "Check that there's free disk space and that the launcher folder isn't read-only." },
    "disk_write_failed": { "message": "Couldn't save a launcher file.", "fix": "Check that there's free disk space and that the launcher folder isn't read-only." },
    "download_no_url": { "message": "This file has no download address.", "fix": "Try again later." },
    "download_corrupt": { "message": "{{file}} got damaged while downloading.", "fix": "Try again. If it keeps happening, the file on the server may be broken." },
    "host_unreachable": { "message": "Couldn't reach {{host}}.", "fix": "Check your internet connection." },
    "download_restarted": { "message": "The download had to start over.", "fix": "Try again." },
    "host_trouble": { "message": "{{host}} is having trouble right now.", "fix": "Try again in a few minutes." },
    "download_rejected": { "message": "{{host}} refused the download.", "fix": "Try again later." },
    "connection_dropped": { "message": "The connection dropped during a download.", "fix": "Check your internet connection and try again." },
    "http_client_failed": { "message": "The launcher couldn't start its network connection.", "fix": "Restart the launcher." },
    "offline": { "message": "You seem to be offline, and this hasn't been downloaded before.", "fix": "Connect to the internet and try again." },
    "unsafe_game_data": { "message": "Minecraft's file list contains something unsafe, so it was skipped.", "fix": "Try again later." },
    "library_unverifiable": { "message": "Couldn't check that a downloaded file is genuine.", "fix": "Try again later." },
    "no_client_download": { "message": "Minecraft {{version}} has no game download.", "fix": "Pick a different version." },
    "minecraft_version_not_found": { "message": "Minecraft {{version}} doesn't exist.", "fix": "Pick a version from the list." },
    "mojang_mismatch": { "message": "Mojang sent a different version than the one asked for.", "fix": "Try again later." },
    "no_asset_index": { "message": "{{version}} is missing its list of game assets.", "fix": "Pick a different version." },
    "natives_extract_failed": { "message": "Couldn't unpack Minecraft's native libraries.", "fix": "Close any running Minecraft and try again." },
    "version_in_use": { "message": "Another copy of this Minecraft version is running.", "fix": "Close it and try again." },
    "archive_invalid": { "message": "{{file}} isn't a valid archive.", "fix": "Press Play again to download it again." },
    "instance_not_found": { "message": "This instance doesn't exist anymore." },
    "instance_damaged": { "message": "The settings file of “{{id}}” is damaged.", "fix": "Delete the instance and create it again." },
    "instance_id_invalid": { "message": "That instance can't be found." },
    "instance_busy": { "message": "Stop the game before changing this instance." },
    "already_running": { "message": "This instance is already running." },
    "name_invalid": { "message": "Names must be 1–64 characters." },
    "memory_out_of_range": { "message": "Memory must be between {{min}} and {{max}} MB." },
    "java_unavailable": { "message": "There's no official Java ({{component}}) for this computer.", "fix": "Choose a Java installation in the instance settings." },
    "java_start_failed": { "message": "Java couldn't be started.", "fix": "Set Java back to automatic in the instance settings." },
    "java_path_missing": { "message": "The chosen Java wasn't found.", "fix": "Pick Java again in the instance settings, or set it back to automatic." },
    "java_test_failed": { "message": "That doesn't look like a working Java.", "fix": "Pick the java program inside a Java installation's bin folder." },
    "no_game_arguments": { "message": "{{version}} doesn't say how to start the game.", "fix": "Pick a different version." },
    "game_files_missing": { "message": "{{count}} game files are missing.", "fix": "Press Play again to repair the instance." },
    "version_invalid": { "message": "“{{value}}” isn't a valid version.", "fix": "Pick a version from the list." },
    "loader_unsupported": { "message": "{{loader}} isn't supported yet.", "fix": "Use Fabric or Quilt for now." },
    "loader_version_not_found": { "message": "{{loader}} {{loader_version}} isn't available for Minecraft {{version}}.", "fix": "Pick a loader version from the list." },
    "meta_cache_missing": { "message": "A saved download list is missing.", "fix": "Connect to the internet and try again." },
    "meta_not_found": { "message": "A version list wasn't found on the server.", "fix": "Check that this version exists." },
    "meta_checksum": { "message": "A downloaded version list was damaged.", "fix": "Try again in a minute." },
    "meta_unexpected": { "message": "A server sent data the launcher doesn't understand.", "fix": "Try again later." },
    "settings_damaged": { "message": "Your launcher settings couldn't be read, so the defaults are in use.", "fix": "Set your preferences again in Settings." },
    "log_write_stopped": { "message": "The game log stopped being saved to disk.", "fix": "Check free disk space. The game keeps running." },
    "mod_not_jar": { "message": "Only .jar files can be added as mods." },
    "mod_exists": { "message": "{{file}} is already in this instance." },
    "mod_not_found": { "message": "{{file}} isn't in this instance anymore." },
    "mod_name_invalid": { "message": "That isn't a mod file name." },
    "internal": { "message": "Something unexpected went wrong.", "fix": "Restart the launcher. If it keeps happening, copy the details for support." }
  }
}
```

- [ ] **Step 4: Write `es.json`**

`ui/src/i18n/es.json` (neutral Latin American Spanish, informal *tú*):

```json
{
  "app": { "name": "Perland Launcher" },
  "common": {
    "cancel": "Cancelar",
    "close": "Cerrar",
    "save": "Guardar",
    "saving": "Guardando…",
    "saved": "Guardado",
    "delete": "Borrar",
    "rename": "Cambiar nombre",
    "openFolder": "Abrir carpeta",
    "back": "Volver",
    "comingSoon": "Muy pronto",
    "details": "Detalles",
    "hideDetails": "Ocultar detalles",
    "copy": "Copiar",
    "copied": "Copiado",
    "retry": "Reintentar",
    "loading": "Cargando…",
    "continue": "Continuar",
    "dismiss": "Descartar"
  },
  "titlebar": {
    "minimize": "Minimizar",
    "maximize": "Maximizar",
    "restore": "Restaurar",
    "close": "Cerrar",
    "accounts": "Cuentas",
    "settings": "Configuración",
    "noAccount": "Agregar cuenta"
  },
  "rail": {
    "label": "Instancias",
    "newInstance": "Nueva instancia",
    "playing": "Jugando",
    "preparing": "Preparando",
    "damaged": "Requiere atención"
  },
  "home": {
    "emptyTitle": "Todavía no tienes instancias",
    "emptyBody": "Una instancia es una configuración de Minecraft: una versión, un loader y sus propios mundos.",
    "emptyAction": "Crear una instancia",
    "minecraftLine": "Minecraft {{version}}",
    "loaderLine": "{{loader}} {{version}}",
    "vanilla": "Vanilla",
    "mods": "Mods",
    "more": "Más opciones",
    "instanceSettings": "Configuración de la instancia",
    "renameTitle": "Cambiar nombre de la instancia",
    "nameLabel": "Nombre",
    "deleteTitle": "¿Borrar {{name}}?",
    "deleteBody": "Se borra la instancia y todo lo que tiene, incluidos mundos y capturas. No se puede deshacer.",
    "deleteConfirm": "Borrar instancia"
  },
  "play": {
    "play": "Jugar",
    "stop": "Detener",
    "stopping": "Deteniendo…",
    "cancel": "Cancelar",
    "cancelLabel": "Cancelar: {{label}}",
    "queued": "Esperando otra descarga…",
    "preparing": "Preparando…",
    "files": "{{done}} de {{total}} archivos",
    "noAccount": "Agrega una cuenta para jugar"
  },
  "progress": {
    "install_minecraft": "Instalando Minecraft {{version}}",
    "download_java": "Descargando Java {{version}}"
  },
  "activity": {
    "title": "Actividad",
    "open": "Mostrar actividad",
    "close": "Ocultar actividad",
    "empty": "No pasa nada por ahora. Presiona Jugar y lo que muestre el juego aparece aquí.",
    "log": "Registro del juego",
    "logEmpty": "El juego todavía no ha mostrado nada.",
    "crashTitle": "{{name}} se cerró de forma inesperada",
    "crashCode": "Código de salida {{code}}",
    "oomTitle": "Minecraft se quedó sin memoria",
    "oomBody": "Dale más memoria y presiona Jugar otra vez.",
    "raiseMemory": "Subir a {{gb}} GB",
    "failedTitle": "{{name}} no pudo iniciar",
    "copyForSupport": "Copiar para soporte",
    "stopped": "Detenido",
    "jumpToEnd": "Ir a la última línea"
  },
  "newInstance": {
    "title": "Nueva instancia",
    "name": "Nombre",
    "namePlaceholder": "Minecraft {{version}}",
    "version": "Versión de Minecraft",
    "showSnapshots": "Mostrar snapshots",
    "loader": "Loader de mods",
    "loaderNone": "Ninguno",
    "loaderFabric": "Fabric",
    "loaderQuilt": "Quilt",
    "loaderVersion": "Versión del loader",
    "stable": "{{version}} (estable)",
    "beta": "{{version}} (beta)",
    "noLoaderForVersion": "{{loader}} todavía no está disponible para esta versión de Minecraft.",
    "packLink": "Pegar un enlace de pack",
    "mrpack": "Importar un archivo .mrpack",
    "create": "Crear instancia",
    "creating": "Creando…",
    "versionsFailed": "No se pudo cargar la lista de versiones."
  },
  "instanceSettings": {
    "title": "Configuración de la instancia",
    "game": "Juego",
    "memory": "Memoria",
    "memoryValue": "{{gb}} GB",
    "memoryHint": "Esta computadora tiene {{total}} GB.",
    "memoryHigh": "Es más de la mitad de la memoria de esta computadora. Otras apps podrían ir más lentas.",
    "java": "Java",
    "javaAuto": "Automático (recomendado)",
    "javaCustom": "Una instalación de Java que yo elija",
    "javaPick": "Elegir…",
    "javaNone": "Todavía no elegiste Java",
    "javaTest": "Probar",
    "javaOk": "Java {{major}} funciona",
    "javaFiles": "Programa de Java",
    "applyNote": "Los cambios se aplican la próxima vez que presiones Jugar."
  },
  "damaged": {
    "title": "No se puede abrir “{{id}}”",
    "deleteTitle": "¿Borrar “{{id}}”?",
    "deleteBody": "Se borra la carpeta y todo lo que tiene. No se puede deshacer."
  },
  "mods": {
    "title": "Mods",
    "count_one": "{{count}} mod",
    "count_other": "{{count}} mods",
    "add": "Agregar mods",
    "dropHere": "Suelta archivos .jar para agregarlos",
    "empty": "Todavía no hay mods. Agrega archivos .jar o suéltalos en cualquier parte de esta pantalla.",
    "vanillaNote": "Los mods necesitan Fabric o Quilt. Puedes cambiar el loader de esta instancia en su configuración.",
    "toggle": "Usar {{name}}",
    "remove": "Quitar {{name}}",
    "removeTitle": "¿Quitar {{name}}?",
    "removeBody": "Se borra el archivo de la carpeta de mods de esta instancia.",
    "removeConfirm": "Quitar",
    "pickTitle": "Elegir mods",
    "jarFiles": "Archivos de mods"
  },
  "accounts": {
    "title": "Cuentas",
    "inUse": "En uso",
    "use": "Usar",
    "remove": "Quitar",
    "removeLabel": "Quitar {{name}}",
    "removeTitle": "¿Quitar {{name}}?",
    "removeBody": "Puedes volver a agregarla cuando quieras. Tus mundos se quedan donde están.",
    "addOffline": "Agregar cuenta sin conexión",
    "offlineName": "Nombre de jugador",
    "offlineHint": "De 3 a 16 letras, números o guiones bajos. Otros jugadores ven este nombre.",
    "offlineLocked": "Las cuentas sin conexión se desbloquean después de iniciar sesión una vez con una cuenta de Microsoft que tenga Minecraft.",
    "microsoft": "Iniciar sesión con Microsoft",
    "offlineBadge": "Sin conexión",
    "empty": "Todavía no hay cuentas."
  },
  "settings": {
    "title": "Configuración",
    "language": "Idioma",
    "languageSystem": "El mismo que esta computadora",
    "english": "English",
    "spanish": "Español",
    "memory": "Memoria para instancias nuevas",
    "minimize": "Minimizar mientras juegas",
    "minimizeHint": "El launcher vuelve cuando se cierra el juego.",
    "dataFolder": "Carpeta de datos",
    "about": "Acerca de",
    "version": "Versión {{version}}",
    "devBuild": "Versión de desarrollo",
    "license": "Software libre bajo la Licencia Pública General de GNU v3.",
    "licenseLink": "Leer la licencia",
    "source": "Código fuente",
    "notOfficial": "No es un producto oficial de Minecraft. No está aprobado por Mojang ni Microsoft, ni asociado con ellos."
  },
  "firstRun": {
    "welcomeTitle": "Te damos la bienvenida a Perland",
    "welcomeBody": "Minecraft gratis, listo en unos clics.",
    "start": "Empezar",
    "languageTitle": "Elige tu idioma",
    "accountTitle": "¿Quién va a jugar?",
    "accountBody": "Elige el nombre que verán los demás jugadores.",
    "accountLockedBody": "El inicio de sesión con Microsoft llegará pronto. Puedes preparar lo demás ahora y agregar una cuenta después.",
    "skip": "Omitir por ahora",
    "instanceTitle": "Tu primera instancia",
    "instanceBody": "Empieza con el Minecraft más nuevo, o elige una versión.",
    "latestRelease": "El Minecraft más nuevo ({{version}})",
    "chooseVersion": "Elegir una versión",
    "finish": "A jugar",
    "step": "Paso {{n}} de {{total}}"
  },
  "closeGuard": {
    "title": "Todavía hay un juego abierto",
    "body": "Si cierras el launcher, el juego se detiene. Minimiza el launcher para seguir jugando.",
    "minimize": "Mejor minimizar",
    "quit": "Salir y detener juegos"
  },
  "errors": {
    "offline_name_invalid": { "message": "Los nombres deben tener de 3 a 16 letras, números o guiones bajos." },
    "accounts_damaged": { "message": "Tu archivo de cuentas está dañado.", "fix": "Borra accounts.json de la carpeta de datos y vuelve a agregar tus cuentas." },
    "offline_locked": { "message": "Las cuentas sin conexión se desbloquean después de iniciar sesión una vez con una cuenta de Microsoft que tenga Minecraft.", "fix": "El inicio de sesión con Microsoft llegará pronto." },
    "microsoft_unavailable": { "message": "Esta versión todavía no admite cuentas de Microsoft." },
    "no_account": { "message": "Agrega una cuenta antes de jugar.", "fix": "Abre Cuentas y agrega una." },
    "disk_io": { "message": "No se pudo leer o escribir un archivo del launcher.", "fix": "Revisa que haya espacio libre y que la carpeta del launcher no sea de solo lectura." },
    "disk_write_failed": { "message": "No se pudo guardar un archivo del launcher.", "fix": "Revisa que haya espacio libre y que la carpeta del launcher no sea de solo lectura." },
    "download_no_url": { "message": "Este archivo no tiene dirección de descarga.", "fix": "Inténtalo más tarde." },
    "download_corrupt": { "message": "{{file}} se dañó durante la descarga.", "fix": "Inténtalo de nuevo. Si sigue pasando, puede que el archivo del servidor esté roto." },
    "host_unreachable": { "message": "No se pudo conectar con {{host}}.", "fix": "Revisa tu conexión a internet." },
    "download_restarted": { "message": "La descarga tuvo que empezar de nuevo.", "fix": "Inténtalo de nuevo." },
    "host_trouble": { "message": "{{host}} tiene problemas en este momento.", "fix": "Inténtalo en unos minutos." },
    "download_rejected": { "message": "{{host}} rechazó la descarga.", "fix": "Inténtalo más tarde." },
    "connection_dropped": { "message": "La conexión se cortó durante una descarga.", "fix": "Revisa tu conexión a internet y vuelve a intentarlo." },
    "http_client_failed": { "message": "El launcher no pudo iniciar su conexión de red.", "fix": "Reinicia el launcher." },
    "offline": { "message": "Parece que no tienes conexión, y esto no se ha descargado antes.", "fix": "Conéctate a internet y vuelve a intentarlo." },
    "unsafe_game_data": { "message": "La lista de archivos de Minecraft tiene algo inseguro, así que se omitió.", "fix": "Inténtalo más tarde." },
    "library_unverifiable": { "message": "No se pudo comprobar que un archivo descargado sea auténtico.", "fix": "Inténtalo más tarde." },
    "no_client_download": { "message": "Minecraft {{version}} no tiene descarga del juego.", "fix": "Elige otra versión." },
    "minecraft_version_not_found": { "message": "Minecraft {{version}} no existe.", "fix": "Elige una versión de la lista." },
    "mojang_mismatch": { "message": "Mojang envió una versión distinta a la que se pidió.", "fix": "Inténtalo más tarde." },
    "no_asset_index": { "message": "A {{version}} le falta su lista de recursos del juego.", "fix": "Elige otra versión." },
    "natives_extract_failed": { "message": "No se pudieron descomprimir las bibliotecas nativas de Minecraft.", "fix": "Cierra cualquier Minecraft abierto y vuelve a intentarlo." },
    "version_in_use": { "message": "Hay otra copia de esta versión de Minecraft abierta.", "fix": "Ciérrala y vuelve a intentarlo." },
    "archive_invalid": { "message": "{{file}} no es un archivo comprimido válido.", "fix": "Presiona Jugar otra vez para descargarlo de nuevo." },
    "instance_not_found": { "message": "Esta instancia ya no existe." },
    "instance_damaged": { "message": "El archivo de configuración de “{{id}}” está dañado.", "fix": "Borra la instancia y créala de nuevo." },
    "instance_id_invalid": { "message": "No se encuentra esa instancia." },
    "instance_busy": { "message": "Detén el juego antes de cambiar esta instancia." },
    "already_running": { "message": "Esta instancia ya está abierta." },
    "name_invalid": { "message": "Los nombres deben tener de 1 a 64 caracteres." },
    "memory_out_of_range": { "message": "La memoria debe estar entre {{min}} y {{max}} MB." },
    "java_unavailable": { "message": "No hay Java oficial ({{component}}) para esta computadora.", "fix": "Elige una instalación de Java en la configuración de la instancia." },
    "java_start_failed": { "message": "No se pudo iniciar Java.", "fix": "Vuelve a poner Java en automático en la configuración de la instancia." },
    "java_path_missing": { "message": "No se encontró el Java elegido.", "fix": "Elige Java otra vez en la configuración de la instancia, o vuelve a ponerlo en automático." },
    "java_test_failed": { "message": "Eso no parece un Java que funcione.", "fix": "Elige el programa java dentro de la carpeta bin de una instalación de Java." },
    "no_game_arguments": { "message": "{{version}} no indica cómo iniciar el juego.", "fix": "Elige otra versión." },
    "game_files_missing": { "message": "Faltan {{count}} archivos del juego.", "fix": "Presiona Jugar otra vez para reparar la instancia." },
    "version_invalid": { "message": "“{{value}}” no es una versión válida.", "fix": "Elige una versión de la lista." },
    "loader_unsupported": { "message": "{{loader}} todavía no es compatible.", "fix": "Por ahora usa Fabric o Quilt." },
    "loader_version_not_found": { "message": "{{loader}} {{loader_version}} no está disponible para Minecraft {{version}}.", "fix": "Elige una versión del loader de la lista." },
    "meta_cache_missing": { "message": "Falta una lista de descargas guardada.", "fix": "Conéctate a internet y vuelve a intentarlo." },
    "meta_not_found": { "message": "No se encontró una lista de versiones en el servidor.", "fix": "Revisa que esta versión exista." },
    "meta_checksum": { "message": "Una lista de versiones descargada estaba dañada.", "fix": "Inténtalo en un minuto." },
    "meta_unexpected": { "message": "Un servidor envió datos que el launcher no entiende.", "fix": "Inténtalo más tarde." },
    "settings_damaged": { "message": "No se pudo leer tu configuración del launcher, así que se usan los valores predeterminados.", "fix": "Vuelve a ajustar tus preferencias en Configuración." },
    "log_write_stopped": { "message": "El registro del juego dejó de guardarse en el disco.", "fix": "Revisa el espacio libre. El juego sigue abierto." },
    "mod_not_jar": { "message": "Solo se pueden agregar archivos .jar como mods." },
    "mod_exists": { "message": "{{file}} ya está en esta instancia." },
    "mod_not_found": { "message": "{{file}} ya no está en esta instancia." },
    "mod_name_invalid": { "message": "Ese no es un nombre de archivo de mod." },
    "internal": { "message": "Ocurrió algo inesperado.", "fix": "Reinicia el launcher. Si sigue pasando, copia los detalles para soporte." }
  }
}
```

- [ ] **Step 5: Write `i18n/index.ts` and `ipc/errors.ts`**

`ui/src/i18n/index.ts`:

```ts
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import type { Lang } from '../ipc/schema';
import en from './en.json';
import es from './es.json';

/** The OS language decides until the player picks one: any Spanish locale gets Spanish. */
export function detectLang(language: string | undefined): Lang {
  return language?.toLowerCase().startsWith('es') ? 'es' : 'en';
}

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, es: { translation: es } },
  lng: detectLang(typeof navigator === 'undefined' ? undefined : navigator.language),
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
});

export default i18n;
```

`ui/src/ipc/errors.ts`:

```ts
import type { i18n as I18n } from 'i18next';
import type { LauncherError, TaskId } from './schema';

export interface ErrorText {
  message: string;
  fix: string | null;
  detail: string | null;
}

export function isLauncherError(value: unknown): value is LauncherError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'code' in value &&
    'message' in value &&
    'category' in value &&
    'params' in value
  );
}

/** A failure in the player's language: translated by code, or the core's English as a fallback. */
export function describeError(i18n: I18n, error: unknown): ErrorText {
  if (!isLauncherError(error)) {
    return {
      message: i18n.t('errors.internal.message'),
      fix: i18n.t('errors.internal.fix'),
      detail: error instanceof Error ? error.message : String(error),
    };
  }
  const key = `errors.${error.code}`;
  const known = i18n.exists(`${key}.message`);
  const message = known ? i18n.t(`${key}.message`, error.params) : error.message;
  // A translated message never pairs with an untranslated fix: no mixed-language cards.
  const fix = i18n.exists(`${key}.fix`) ? i18n.t(`${key}.fix`, error.params) : known ? null : error.fix;
  return { message, fix, detail: error.detail };
}

export function taskLabel(i18n: I18n, task: TaskId): string {
  const key = `progress.${task.key}`;
  return i18n.exists(key) ? i18n.t(key, task.params) : task.key;
}

/** Text for "Copy for support". The core already redacted tokens and the home folder. */
export function supportText(lines: string[]): string {
  return lines.join('\n');
}
```

Replace `ui/src/test/setup.ts`'s imports block so i18n loads and tests run in English. Add after the existing imports:

```ts
import i18n from '../i18n';

void i18n.changeLanguage('en');
```

- [ ] **Step 6: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): English and Spanish translations, translated errors and task labels

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS. If the "identical long string" test flags a key, the Spanish string was left untranslated. Translate it rather than relaxing the test.

---

### Task 21: UI state: queries, run store, route, toasts, test renderer

**Files:**
- Create: `ui/src/state/queries.ts`, `ui/src/state/runStore.ts`, `ui/src/state/route.ts`, `ui/src/state/toasts.ts`, `ui/src/state/activity.ts`
- Create: `ui/src/test/render.tsx`
- Test: `ui/src/state/runStore.test.ts`, `ui/src/state/route.test.ts`, `ui/src/state/toasts.test.ts`

**Interfaces:**
- Consumes: `commands`, schema types (Task 19).
- Produces:
  - **`queries.ts`:**
    - `createQueryClient()`, `queryClient`, `qk`
    - read hooks: `useAppInfo`, `useSettings`, `useInstances`, `useAccounts`, `useMcVersions(enabled?)`, `useLoaderVersions(kind | null, mc | null)`, `useMods(id)`
    - mutation hooks: `useSaveSettings`, `useCreateInstance`, `useUpdateInstance`, `useRenameInstance`, `useDeleteInstance`, `useAccountMutations`, `useModMutations(id)`
    - Each mutation hook takes `{ inline?: boolean }`. A failed mutation becomes a toast unless `inline` is set.
  - **`runStore.ts`:**
    - `LOG_CAP = 5000`, `RunView { state, progress: ProgressView | null, lastExit: ExitInfo | null }`
    - `createRunStore()`, with `subscribe`, `run(id)`, `log(id)`, `apply(event)`, `hydrate(snapshots, tails)`, `dismissExit(id)`, `active()`
    - `runStore`, `useRun(id)`, `useLog(id)`, `useActiveIds()`
  - **`route.ts`:** `Route` (a union), `navigate(route)`, `useRoute()`, `instanceOf(route) -> string | null`, `rememberInstance`, `lastInstance()`, `resetRoute()`
  - **`toasts.ts`:** `Toast { id, error }`, `pushError(error)`, `dismissToast(id)`, `useToasts()`, `clearToasts()`
  - **`activity.ts`:** `openActivity(id?)`, `closeActivity()`, `useActivity(): { open, focus: string | null }`, `resetActivity()`
  - **`test/render.tsx`:** `renderWithProviders(ui, { ipc? })` returns `{ qc, user, ...render }`

- [ ] **Step 1: Write the failing tests**

`ui/src/state/runStore.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { createRunStore, LOG_CAP } from './runStore';

const progress = (done: number) =>
  ({
    type: 'progress',
    instance: 'pack',
    task: { key: 'install_minecraft', params: { version: '1.20.1' } },
    done,
    total: 10,
    bytes: done * 100,
  }) as const;

describe('runStore', () => {
  it('follows a run from queued to idle', () => {
    const store = createRunStore();
    expect(store.run('pack').state).toBe('idle');
    store.apply({ type: 'run_state', instance: 'pack', state: 'queued', exit: null });
    store.apply({ type: 'run_state', instance: 'pack', state: 'preparing', exit: null });
    store.apply(progress(4));
    expect(store.run('pack').progress?.done).toBe(4);
    store.apply({ type: 'run_state', instance: 'pack', state: 'running', exit: null });
    expect(store.run('pack').progress).toBeNull();
    expect(store.active()).toEqual(['pack']);
    const exit = { code: 1, killed: false, cancelled: false, crash: 'out_of_memory', error: null } as const;
    store.apply({ type: 'run_state', instance: 'pack', state: 'idle', exit });
    expect(store.run('pack')).toMatchObject({ state: 'idle', lastExit: exit });
    expect(store.active()).toEqual([]);
    store.dismissExit('pack');
    expect(store.run('pack').lastExit).toBeNull();
  });

  it('starts a fresh log on every new run and caps it', () => {
    const store = createRunStore();
    store.apply({ type: 'log', instance: 'pack', lines: ['old'] });
    store.apply({ type: 'run_state', instance: 'pack', state: 'queued', exit: null });
    expect(store.log('pack')).toEqual([]);
    const many = Array.from({ length: LOG_CAP + 10 }, (_, i) => `l${i}`);
    store.apply({ type: 'log', instance: 'pack', lines: many });
    expect(store.log('pack')).toHaveLength(LOG_CAP);
    expect(store.log('pack')[0]).toBe('l10');
  });

  it('keeps snapshots stable between unrelated updates', () => {
    const store = createRunStore();
    const before = store.run('a');
    store.apply({ type: 'log', instance: 'b', lines: ['x'] });
    expect(store.run('a')).toBe(before);
    expect(store.log('missing')).toBe(store.log('also-missing'));
  });

  it('hydrates after a reload', () => {
    const store = createRunStore();
    store.hydrate([{ instance: 'pack', state: 'running' }], { pack: ['a', 'b'] });
    expect(store.run('pack').state).toBe('running');
    expect(store.log('pack')).toEqual(['a', 'b']);
  });
});
```

`ui/src/state/route.test.ts`:

```ts
import { afterEach, expect, it } from 'vitest';
import { instanceOf, lastInstance, navigate, rememberInstance, resetRoute, routeSnapshot } from './route';

afterEach(resetRoute);

it('navigates and reports the instance a route is about', () => {
  expect(routeSnapshot()).toEqual({ name: 'home', id: null });
  navigate({ name: 'mods', id: 'pack' });
  expect(instanceOf(routeSnapshot())).toBe('pack');
  navigate({ name: 'settings' });
  expect(instanceOf(routeSnapshot())).toBeNull();
});

it('remembers the last instance when storage works', () => {
  rememberInstance('pack');
  expect(lastInstance()).toBe('pack');
});
```

`ui/src/state/toasts.test.ts`:

```ts
import { afterEach, expect, it } from 'vitest';
import { clearToasts, dismissToast, pushError, toastsSnapshot } from './toasts';

afterEach(clearToasts);

it('queues and dismisses errors', () => {
  pushError({ category: 'disk', code: 'disk_io', message: 'x', fix: null, detail: null, params: {} });
  pushError(new Error('boom'));
  const toasts = toastsSnapshot();
  expect(toasts).toHaveLength(2);
  dismissToast(toasts[0]!.id);
  expect(toastsSnapshot()).toHaveLength(1);
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter ui test`
Expected: FAIL (missing modules).

- [ ] **Step 3: Implement the stores**

`ui/src/state/runStore.ts`:

```ts
import { useSyncExternalStore } from 'react';
import type { AppEvent, ExitInfo, RunSnapshot, RunStateKind, TaskId } from '../ipc/schema';

/** Lines kept per instance, matching the Rust side's log_tail. */
export const LOG_CAP = 5000;

export interface ProgressView {
  task: TaskId;
  done: number;
  total: number;
  bytes: number;
}

export interface RunView {
  state: RunStateKind;
  progress: ProgressView | null;
  lastExit: ExitInfo | null;
}

const IDLE: RunView = { state: 'idle', progress: null, lastExit: null };
const NO_LINES: string[] = [];

export function createRunStore() {
  let runs = new Map<string, RunView>();
  let logs = new Map<string, string[]>();
  let activeIds: string[] = [];
  const listeners = new Set<() => void>();

  const notify = () => listeners.forEach((listener) => listener());

  function setRun(id: string, update: (run: RunView) => RunView) {
    const next = new Map(runs);
    next.set(id, update(runs.get(id) ?? IDLE));
    runs = next;
    activeIds = [...runs].filter(([, run]) => run.state !== 'idle').map(([key]) => key);
  }

  function setLog(id: string, lines: string[]) {
    const next = new Map(logs);
    next.set(id, lines.length > LOG_CAP ? lines.slice(lines.length - LOG_CAP) : lines);
    logs = next;
  }

  return {
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    run: (id: string): RunView => runs.get(id) ?? IDLE,
    log: (id: string): string[] => logs.get(id) ?? NO_LINES,
    active: (): string[] => activeIds,
    apply(event: AppEvent) {
      switch (event.type) {
        case 'progress': {
          const id = event.instance;
          if (!id) return;
          const { task, done, total, bytes } = event;
          setRun(id, (run) => ({ ...run, progress: { task, done, total, bytes } }));
          break;
        }
        case 'log':
          setLog(event.instance, (logs.get(event.instance) ?? NO_LINES).concat(event.lines));
          break;
        case 'run_state': {
          const { instance: id, state, exit } = event;
          if (state === 'queued') setLog(id, NO_LINES);
          setRun(id, (run) => ({
            state,
            progress: state === 'queued' || state === 'preparing' ? run.progress : null,
            lastExit: state === 'queued' ? null : (exit ?? run.lastExit),
          }));
          break;
        }
        case 'warning':
          return; // shown as toasts, see main.tsx
      }
      notify();
    },
    hydrate(snapshots: RunSnapshot[], tails: Record<string, string[]>) {
      for (const snapshot of snapshots) setRun(snapshot.instance, (run) => ({ ...run, state: snapshot.state }));
      for (const [id, lines] of Object.entries(tails)) setLog(id, lines);
      notify();
    },
    dismissExit(id: string) {
      setRun(id, (run) => ({ ...run, lastExit: null }));
      notify();
    },
  };
}

export type RunStore = ReturnType<typeof createRunStore>;

export const runStore = createRunStore();

export function useRun(id: string): RunView {
  return useSyncExternalStore(runStore.subscribe, () => runStore.run(id));
}

export function useLog(id: string): string[] {
  return useSyncExternalStore(runStore.subscribe, () => runStore.log(id));
}

export function useActiveIds(): string[] {
  return useSyncExternalStore(runStore.subscribe, runStore.active);
}
```

`ui/src/state/route.ts`:

```ts
import { useSyncExternalStore } from 'react';

export type Route =
  | { name: 'home'; id: string | null }
  | { name: 'mods'; id: string }
  | { name: 'instanceSettings'; id: string }
  | { name: 'newInstance' }
  | { name: 'accounts' }
  | { name: 'settings' };

const HOME: Route = { name: 'home', id: null };
const LAST_KEY = 'perland.lastInstance';

let current: Route = HOME;
const listeners = new Set<() => void>();

export function navigate(route: Route) {
  current = route;
  const id = instanceOf(route);
  if (id) rememberInstance(id);
  listeners.forEach((listener) => listener());
}

export function routeSnapshot(): Route {
  return current;
}

export function useRoute(): Route {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    routeSnapshot,
  );
}

export function instanceOf(route: Route): string | null {
  return 'id' in route ? route.id : null;
}

/** Per-viewer convenience: reopen the last instance. Storage may be unavailable; that's fine. */
export function rememberInstance(id: string) {
  try {
    localStorage.setItem(LAST_KEY, id);
  } catch {
    // ignore: storage is optional
  }
}

export function lastInstance(): string | null {
  try {
    return localStorage.getItem(LAST_KEY);
  } catch {
    return null;
  }
}

export function resetRoute() {
  current = HOME;
  try {
    localStorage.removeItem(LAST_KEY);
  } catch {
    // ignore
  }
  listeners.forEach((listener) => listener());
}
```

`ui/src/state/toasts.ts`:

```ts
import { useSyncExternalStore } from 'react';

export interface Toast {
  id: number;
  error: unknown;
}

let toasts: Toast[] = [];
let nextId = 1;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((listener) => listener());

export function pushError(error: unknown) {
  toasts = [...toasts, { id: nextId++, error }];
  notify();
}

export function dismissToast(id: number) {
  toasts = toasts.filter((toast) => toast.id !== id);
  notify();
}

export function clearToasts() {
  toasts = [];
  notify();
}

export function toastsSnapshot(): Toast[] {
  return toasts;
}

export function useToasts(): Toast[] {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    toastsSnapshot,
  );
}
```

`ui/src/state/activity.ts`:

```ts
import { useSyncExternalStore } from 'react';

interface ActivityState {
  open: boolean;
  focus: string | null;
}

let state: ActivityState = { open: false, focus: null };
const listeners = new Set<() => void>();
const set = (next: ActivityState) => {
  state = next;
  listeners.forEach((listener) => listener());
};

/** Open the activity drawer, optionally on one instance's tab (used when a game crashes). */
export function openActivity(focus: string | null = state.focus) {
  set({ open: true, focus });
}

export function closeActivity() {
  set({ ...state, open: false });
}

export function resetActivity() {
  set({ open: false, focus: null });
}

export function useActivity(): ActivityState {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    () => state,
  );
}
```

`ui/src/state/queries.ts`:

```ts
import { QueryClient, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { commands } from '../ipc/commands';
import type { Instance, LoaderKind, NewInstance, Settings } from '../ipc/schema';
import { pushError } from './toasts';

export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: false, staleTime: 30_000 },
      mutations: { retry: false },
    },
  });
}

export const queryClient = createQueryClient();

export const qk = {
  appInfo: ['appInfo'] as const,
  settings: ['settings'] as const,
  instances: ['instances'] as const,
  accounts: ['accounts'] as const,
  mcVersions: ['catalog', 'minecraft'] as const,
  loaders: (kind: LoaderKind, mc: string) => ['catalog', 'loaders', kind, mc] as const,
  mods: (id: string) => ['mods', id] as const,
};

const TEN_MINUTES = 10 * 60_000;

export const useAppInfo = () => useQuery({ queryKey: qk.appInfo, queryFn: commands.appInfo, staleTime: Infinity });
export const useSettings = () => useQuery({ queryKey: qk.settings, queryFn: commands.settingsGet });
export const useInstances = () => useQuery({ queryKey: qk.instances, queryFn: commands.instancesList });
export const useAccounts = () => useQuery({ queryKey: qk.accounts, queryFn: commands.accountsList });
export const useMcVersions = (enabled = true) =>
  useQuery({ queryKey: qk.mcVersions, queryFn: commands.catalogMinecraft, staleTime: TEN_MINUTES, enabled });
export const useLoaderVersions = (kind: LoaderKind | null, mc: string | null) =>
  useQuery({
    queryKey: qk.loaders(kind ?? 'fabric', mc ?? ''),
    queryFn: () => commands.catalogLoaders(kind as LoaderKind, mc as string),
    enabled: kind !== null && !!mc,
    staleTime: TEN_MINUTES,
  });
export const useMods = (id: string) => useQuery({ queryKey: qk.mods(id), queryFn: () => commands.modsList(id) });

interface Opts {
  /** The caller shows the error itself (forms); otherwise it becomes a toast. */
  inline?: boolean;
}
const onError = (opts: Opts) => (error: unknown) => {
  if (!opts.inline) pushError(error);
};

export function useSaveSettings(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (settings: Settings) => commands.settingsSet(settings),
    onSuccess: (saved) => qc.setQueryData(qk.settings, saved),
    onError: onError(opts),
  });
}

export function useCreateInstance(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: NewInstance) => commands.instanceCreate(request),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function useUpdateInstance(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (instance: Instance) => commands.instanceUpdate(instance),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function useRenameInstance(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => commands.instanceRename(id, name),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function useDeleteInstance(opts: Opts = {}) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => commands.instanceDelete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.instances }),
    onError: onError(opts),
  });
}

export function useAccountMutations(opts: Opts = {}) {
  const qc = useQueryClient();
  const onSuccess = (file: Awaited<ReturnType<typeof commands.accountsList>>) => qc.setQueryData(qk.accounts, file);
  return {
    addOffline: useMutation({ mutationFn: commands.accountAddOffline, onSuccess, onError: onError(opts) }),
    setActive: useMutation({ mutationFn: commands.accountSetActive, onSuccess, onError: onError(opts) }),
    remove: useMutation({ mutationFn: commands.accountRemove, onSuccess, onError: onError(opts) }),
  };
}

export function useModMutations(id: string, opts: Opts = {}) {
  const qc = useQueryClient();
  const onSuccess = (list: Awaited<ReturnType<typeof commands.modsList>>) => qc.setQueryData(qk.mods(id), list);
  return {
    toggle: useMutation({
      mutationFn: ({ file, enabled }: { file: string; enabled: boolean }) => commands.modSetEnabled(id, file, enabled),
      onSuccess,
      onError: onError(opts),
    }),
    add: useMutation({ mutationFn: (paths: string[]) => commands.modsAdd(id, paths), onSuccess, onError: onError(opts) }),
    remove: useMutation({ mutationFn: (file: string) => commands.modRemove(id, file), onSuccess, onError: onError(opts) }),
  };
}
```

`ui/src/test/render.tsx`:

```tsx
import { QueryClientProvider } from '@tanstack/react-query';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { render } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactElement } from 'react';
import { createQueryClient } from '../state/queries';

export type IpcHandler = (cmd: string, args?: unknown) => unknown;

/** Render with a fresh query client and a mocked Rust side (events mocked too). */
export function renderWithProviders(ui: ReactElement, { ipc }: { ipc?: IpcHandler } = {}) {
  mockWindows('main');
  mockIPC((cmd, args) => (ipc ? ipc(cmd, args) : null), { shouldMockEvents: true });
  const qc = createQueryClient();
  const user = userEvent.setup();
  return { qc, user, ...render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>) };
}
```

- [ ] **Step 4: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): query hooks, run store, route, toasts and activity state

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---

### Task 22: Browser mock mode

**Files:**
- Create: `ui/src/ipc/mock.ts`
- Test: `ui/src/ipc/mock.test.ts`

**Interfaces:**
- Consumes: contract fixtures, schema types, `APP_EVENT`.
- Produces:
  - `MockOptions { firstRun?: boolean; crash?: boolean; release?: boolean }`
  - `createMockBackend(options) -> { state, handle(cmd, args) }`
  - `installMock(options)`, which installs `mockWindows('main')` and `mockIPC(handle, { shouldMockEvents: true })`
  - a scripted `play` that emits:
    1. `queued`
    2. `preparing` with 41 progress steps, 60 ms apart
    3. `running` with a log line every 400 ms
    4. with `crash`, after 2.5 s: `idle` + `{ code: 1, crash: 'out_of_memory' }`
  - `stop` cancels or kills, like Rust does.
  - Plugin commands: `plugin:dialog|open` returns a fake jar path (an array when `multiple` is set); `plugin:window|is_maximized` returns `false`; every other `plugin:*` returns `null`.

- [ ] **Step 1: Write the failing test**

`ui/src/ipc/mock.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { createMockBackend } from './mock';
import type { AccountsFile, InstanceListing, Settings } from './schema';

describe('mock backend', () => {
  it('serves the contract fixtures', async () => {
    const { handle } = createMockBackend();
    const listing = (await handle('instances_list')) as InstanceListing;
    expect(listing.instances.map((i) => i.id)).toEqual(['asalith-fields', 'creative-flat']);
    expect(listing.damaged).toHaveLength(1);
    const settings = (await handle('settings_get')) as Settings;
    expect(settings.firstRunDone).toBe(true);
  });

  it('starts empty for first run', async () => {
    const { handle } = createMockBackend({ firstRun: true });
    expect(((await handle('instances_list')) as InstanceListing).instances).toEqual([]);
    expect(((await handle('accounts_list')) as AccountsFile).accounts).toEqual([]);
    expect(((await handle('settings_get')) as Settings).firstRunDone).toBe(false);
  });

  it('creates instances and toggles mods like the real backend', async () => {
    const { handle } = createMockBackend();
    const created = (await handle('instance_create', {
      request: { name: 'New One', minecraft: '1.21.1', loader: null },
    })) as { id: string };
    expect(created.id).toBe('new-one');
    const mods = (await handle('mod_set_enabled', {
      id: 'asalith-fields',
      file: 'zoomify-2.11.2.jar.disabled',
      enabled: true,
    })) as { fileName: string; enabled: boolean }[];
    expect(mods.find((m) => m.fileName === 'zoomify-2.11.2.jar')?.enabled).toBe(true);
  });

  it('rejects offline accounts in a release-like build', async () => {
    const { handle } = createMockBackend({ release: true });
    await expect(handle('account_add_offline', { name: 'Steve' })).rejects.toMatchObject({ code: 'offline_locked' });
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm --filter ui test`
Expected: FAIL, because `./mock` doesn't exist.

- [ ] **Step 3: Implement the mock**

`ui/src/ipc/mock.ts`:

```ts
// Browser-only backend for `pnpm --filter ui dev:mock` and tests: seeded from the Rust contract
// fixtures, with a scripted Play. main.tsx only imports it in mock mode, so the Tauri build
// never runs it.
import { emit } from '@tauri-apps/api/event';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import accountsFx from './contract/accounts.json';
import appInfoFx from './contract/app_info.json';
import listingFx from './contract/instance_listing.json';
import loaderVersionsFx from './contract/loader_versions.json';
import localModsFx from './contract/local_mods.json';
import mcVersionsFx from './contract/mc_versions.json';
import settingsFx from './contract/settings.json';
import { APP_EVENT } from './events';
import type {
  AccountsFile,
  AppEvent,
  AppInfo,
  Instance,
  InstanceListing,
  LauncherError,
  LocalMod,
  NewInstance,
  RunStateKind,
  Settings,
} from './schema';

export interface MockOptions {
  firstRun?: boolean;
  crash?: boolean;
  release?: boolean;
}

interface MockRun {
  state: RunStateKind;
  cancelled: boolean;
}

const clone = <T>(value: T): T => structuredClone(value);
const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function fail(category: LauncherError['category'], code: string, message: string, params = {}): never {
  throw { category, code, message, fix: null, detail: null, params } satisfies LauncherError;
}

function slug(name: string) {
  const s = name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
  return s.slice(0, 48) || 'instance';
}

function seed(options: MockOptions) {
  const listing = clone(listingFx) as InstanceListing;
  const accounts = clone(accountsFx) as AccountsFile;
  const settings: Settings = { ...(clone(settingsFx) as Settings), firstRunDone: true, language: null };
  if (options.firstRun) {
    listing.instances = [];
    listing.damaged = [];
    accounts.accounts = [];
    accounts.active = null;
    settings.firstRunDone = false;
  }
  const appInfo: AppInfo = {
    ...(clone(appInfoFx) as AppInfo),
    build: options.release ? 'release' : 'dev',
    offlineAllowed: !options.release,
  };
  const mods = new Map<string, LocalMod[]>([['asalith-fields', clone(localModsFx) as LocalMod[]]]);
  return { listing, accounts, settings, appInfo, mods, runs: new Map<string, MockRun>(), options };
}

export type MockState = ReturnType<typeof seed>;

function event(e: AppEvent) {
  void emit(APP_EVENT, e);
}

function setState(state: MockState, id: string, next: RunStateKind) {
  const run = state.runs.get(id);
  if (run) run.state = next;
  event({ type: 'run_state', instance: id, state: next, exit: null });
}

async function scriptPlay(state: MockState, instance: Instance) {
  const id = instance.id;
  const run: MockRun = { state: 'queued', cancelled: false };
  state.runs.set(id, run);
  event({ type: 'run_state', instance: id, state: 'queued', exit: null });
  await wait(300);
  if (run.cancelled) return;
  setState(state, id, 'preparing');
  for (let done = 0; done <= 40; done++) {
    if (run.cancelled) return;
    event({
      type: 'progress',
      instance: id,
      task: { key: 'install_minecraft', params: { version: instance.game.minecraft } },
      done,
      total: 40,
      bytes: done * 7_800_000,
    });
    await wait(60);
  }
  setState(state, id, 'running');
  const started = Date.now();
  let n = 0;
  while (state.runs.get(id) === run && run.state === 'running') {
    event({ type: 'log', instance: id, lines: [`[12:00:${String(n % 60).padStart(2, '0')}] [Render thread/INFO]: Tick ${n}`] });
    n += 1;
    if (state.options.crash && Date.now() - started > 2500) {
      event({ type: 'log', instance: id, lines: ['Exception in thread "main" java.lang.OutOfMemoryError: Java heap space'] });
      state.runs.delete(id);
      event({
        type: 'run_state',
        instance: id,
        state: 'idle',
        exit: { code: 1, killed: false, cancelled: false, crash: 'out_of_memory', error: null },
      });
      return;
    }
    await wait(400);
  }
}

async function handle(state: MockState, cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  const find = (id: unknown) => state.listing.instances.find((i) => i.id === id);
  switch (cmd) {
    case 'app_info':
      return state.appInfo;
    case 'settings_get':
      return state.settings;
    case 'settings_set':
      state.settings = args.settings as Settings;
      return state.settings;
    case 'instances_list':
      return state.listing;
    case 'instance_create': {
      const request = args.request as NewInstance;
      let id = slug(request.name);
      for (let n = 2; find(id); n++) id = `${slug(request.name)}-${n}`;
      const created: Instance = {
        id,
        name: request.name.trim(),
        source: { kind: 'none' },
        game: { minecraft: request.minecraft, loader: request.loader },
        updatePolicy: 'ask',
        optionalChoices: {},
        java: { path: null, memoryMb: state.settings.defaultMemoryMb },
      };
      state.listing.instances = [...state.listing.instances, created].sort((a, b) => a.name.localeCompare(b.name));
      return created;
    }
    case 'instance_update': {
      const updated = args.instance as Instance;
      state.listing.instances = state.listing.instances.map((i) => (i.id === updated.id ? updated : i));
      return updated;
    }
    case 'instance_rename': {
      const target = find(args.id) ?? fail('disk', 'instance_not_found', 'gone');
      target.name = String(args.name).trim();
      return target;
    }
    case 'instance_delete':
      state.listing.instances = state.listing.instances.filter((i) => i.id !== args.id);
      state.listing.damaged = state.listing.damaged.filter((d) => d.id !== args.id);
      return null;
    case 'catalog_minecraft':
      return mcVersionsFx;
    case 'catalog_loaders':
      return loaderVersionsFx;
    case 'accounts_list':
      return state.accounts;
    case 'account_add_offline': {
      if (!state.appInfo.offlineAllowed) fail('auth', 'offline_locked', 'Offline accounts are locked');
      const name = String(args.name);
      if (!/^\w{3,16}$/.test(name)) fail('auth', 'offline_name_invalid', 'bad name');
      const uuid = `mock-${name.toLowerCase()}`;
      state.accounts.accounts = [...state.accounts.accounts.filter((a) => a.uuid !== uuid), { provider: 'offline', username: name, uuid }];
      state.accounts.active = uuid;
      return state.accounts;
    }
    case 'account_set_active':
      state.accounts.active = String(args.uuid);
      return state.accounts;
    case 'account_remove': {
      state.accounts.accounts = state.accounts.accounts.filter((a) => a.uuid !== args.uuid);
      if (state.accounts.active === args.uuid) state.accounts.active = state.accounts.accounts[0]?.uuid ?? null;
      return state.accounts;
    }
    case 'play': {
      const target = find(args.id) ?? fail('disk', 'instance_not_found', 'gone');
      if (!state.accounts.active) fail('auth', 'no_account', 'Add an account before playing');
      if (state.runs.has(target.id)) fail('game', 'already_running', 'already running');
      void scriptPlay(state, target);
      return null;
    }
    case 'stop': {
      const id = String(args.id);
      const run = state.runs.get(id);
      if (!run) return null;
      if (run.state === 'queued' || run.state === 'preparing') {
        run.cancelled = true;
        state.runs.delete(id);
        event({ type: 'run_state', instance: id, state: 'idle', exit: { code: null, killed: false, cancelled: true, crash: null, error: null } });
      } else if (run.state === 'running') {
        setState(state, id, 'stopping');
        setTimeout(() => {
          state.runs.delete(id);
          event({ type: 'run_state', instance: id, state: 'idle', exit: { code: null, killed: true, cancelled: false, crash: null, error: null } });
        }, 400);
      }
      return null;
    }
    case 'run_states':
      return [...state.runs].map(([instance, run]) => ({ instance, state: run.state }));
    case 'log_tail':
      return [];
    case 'mods_list':
      return state.mods.get(String(args.id)) ?? [];
    case 'mod_set_enabled': {
      const id = String(args.id);
      const file = String(args.file);
      const base = file.replace(/\.disabled$/, '');
      const list = (state.mods.get(id) ?? []).map((m) =>
        m.fileName === file ? { ...m, fileName: args.enabled ? base : `${base}.disabled`, enabled: Boolean(args.enabled) } : m,
      );
      state.mods.set(id, list);
      return list;
    }
    case 'mods_add': {
      const id = String(args.id);
      const added = (args.paths as string[]).map((p) => ({ fileName: p.split(/[\\/]/).pop() ?? p, enabled: true, size: 1_200_000 }));
      const list = [...(state.mods.get(id) ?? []), ...added].sort((a, b) => a.fileName.localeCompare(b.fileName));
      state.mods.set(id, list);
      return list;
    }
    case 'mod_remove': {
      const id = String(args.id);
      const list = (state.mods.get(id) ?? []).filter((m) => m.fileName !== args.file);
      state.mods.set(id, list);
      return list;
    }
    case 'open_folder':
    case 'quit':
      return null;
    case 'java_test':
      return { major: 17 };
    case 'plugin:dialog|open': {
      // Mirrors the real dialog: an array when `multiple` is set, otherwise one path.
      const path = 'C:/Users/player/Downloads/lithium-fabric-0.11.2.jar';
      return (args.options as { multiple?: boolean } | undefined)?.multiple ? [path] : path;
    }
    case 'plugin:window|is_maximized':
      return false;
    default:
      if (cmd.startsWith('plugin:')) return null;
      return fail('game', 'internal', `mock has no command ${cmd}`);
  }
}

export function createMockBackend(options: MockOptions = {}) {
  const state = seed(options);
  return { state, handle: (cmd: string, args?: unknown) => handle(state, cmd, (args ?? {}) as Record<string, unknown>) };
}

export function installMock(options: MockOptions = {}) {
  const backend = createMockBackend(options);
  mockWindows('main');
  mockIPC((cmd, args) => backend.handle(cmd, args), { shouldMockEvents: true });
  return backend;
}
```

- [ ] **Step 4: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): browser mock backend seeded from the contract fixtures

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---

### Task 23: The Arcade Cabinet design system: tokens, fonts and primitives

Before editing, read `C:\Users\mmxtu\.claude\skills\impeccable\reference\craft-floor.md` and the direction contract at `.impeccable/surfaces/ui-src-app-tsx.md`.

**Files:**
- Modify: `ui/src/styles/app.css`
- Create: `ui/src/ui/Icon.ts`, `ui/src/ui/format.ts`, `ui/src/ui/Button.tsx`, `ui/src/ui/Meter.tsx`, `ui/src/ui/PlayButton.tsx`, `ui/src/ui/Dialog.tsx`, `ui/src/ui/Toggle.tsx`, `ui/src/ui/Field.tsx`, `ui/src/ui/Select.tsx`, `ui/src/ui/Menu.tsx`, `ui/src/ui/Toasts.tsx`, `ui/src/ui/ErrorCard.tsx`, `ui/src/ui/ComingSoon.tsx`
- Test: `ui/src/ui/ui.test.tsx`, `ui/src/ui/format.test.ts`

**Interfaces:**
- Consumes: `describeError`, `taskLabel` (Task 20); `RunView`, `ProgressView` (Task 21); `useToasts`, `dismissToast`.
- Produces:
  - **CSS classes:**
    - `.btn` with `.btn-primary` / `.btn-danger` / `.btn-ghost` and sizes `.btn-lg` / `.btn-icon`
    - `.play` (with `data-state`), `.meter`, `.panel`, `.field-input`, `.toggle`, `.dialog`, `.menu`, `.badge`, `.stepped`, `.selectable`, `.tabular`, `.sr-only`
    - utility colors: `bg-ground`, `bg-panel`, `bg-raised`, `text-ink`, `text-ink-dim`, `text-amber`, `text-lime`, `text-coral`, `text-blue-ink`, `border-line`
    - fonts: `font-display` (Bungee), `font-sans`, `font-mono`
  - **Components:**
    - `Button({variant, size, icon, ...})`
    - `Meter({value, label, decorative?})`
    - `PlayButton({state, progress, disabled?, disabledReason?, onPlay, onStop})`
    - `Dialog({open, onClose, title, children, actions})`
    - `Toggle({checked, onChange, label})`
    - `Field({label, hint?, error?, children})`, which clones its child `<input>` with ids and aria props
    - `Select({label, value, onChange, options, disabled?})`
    - `Menu({label, items: {label, onSelect, danger?}[]})`
    - `Toasts()`
    - `ErrorCard({error, actions?})`
    - `ComingSoon({label, icon?})`
  - **Formatters:** `formatBytes(n, lang)`, `formatCount(n, lang)`, `formatGb(mb, lang)`
  - **Icons:** `Icon.*`, re-exported from `pixelarticons/react`

- [ ] **Step 1: Write the failing tests**

`ui/src/ui/format.test.ts`:

```ts
import { expect, it } from 'vitest';
import { formatBytes, formatCount, formatGb } from './format';

it('formats sizes for players', () => {
  expect(formatBytes(312_000_000, 'en')).toBe('312 MB');
  expect(formatBytes(1_500_000_000, 'en')).toBe('1.5 GB');
  expect(formatBytes(900, 'en')).toBe('1 KB');
  expect(formatCount(3575, 'en')).toBe('3,575');
  expect(formatCount(35750, 'es')).toMatch(/35.750/); // es only groups from five digits
  expect(formatGb(6144, 'en')).toBe('6');
  expect(formatGb(2560, 'en')).toBe('2.5');
});
```

`ui/src/ui/ui.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { ComingSoon } from './ComingSoon';
import { Dialog } from './Dialog';
import { ErrorCard } from './ErrorCard';
import { Menu } from './Menu';
import { Meter } from './Meter';
import { PlayButton } from './PlayButton';
import { Toggle } from './Toggle';

describe('Meter', () => {
  it('reports progress as a percentage and lights whole segments', () => {
    const { container } = render(<Meter value={0.5} label="Installing" />);
    expect(screen.getByRole('progressbar', { name: 'Installing' })).toHaveAttribute('aria-valuenow', '50');
    expect(container.querySelectorAll('[data-on]')).toHaveLength(10);
  });
});

describe('PlayButton', () => {
  it('plays when idle', async () => {
    const onPlay = vi.fn();
    render(<PlayButton state="idle" progress={null} onPlay={onPlay} onStop={vi.fn()} />);
    await userEvent.click(screen.getByRole('button', { name: 'Play' }));
    expect(onPlay).toHaveBeenCalledOnce();
  });

  it('shows the task and cancels while preparing', async () => {
    const onStop = vi.fn();
    render(
      <PlayButton
        state="preparing"
        progress={{ task: { key: 'install_minecraft', params: { version: '1.20.1' } }, done: 5, total: 10, bytes: 0 }}
        onPlay={vi.fn()}
        onStop={onStop}
      />,
    );
    const button = screen.getByRole('button', { name: 'Cancel: Installing Minecraft 1.20.1' });
    expect(button).toHaveTextContent('5 of 10 files');
    await userEvent.click(button);
    expect(onStop).toHaveBeenCalledOnce();
  });

  it('becomes Stop while running and explains when it is disabled', () => {
    const { rerender } = render(<PlayButton state="running" progress={null} onPlay={vi.fn()} onStop={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Stop' })).toBeEnabled();
    rerender(<PlayButton state="stopping" progress={null} onPlay={vi.fn()} onStop={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Stopping…' })).toBeDisabled();
    rerender(
      <PlayButton state="idle" progress={null} disabled disabledReason="Add an account to play" onPlay={vi.fn()} onStop={vi.fn()} />,
    );
    expect(screen.getByRole('button', { name: 'Play' })).toHaveAccessibleDescription('Add an account to play');
  });
});

describe('Toggle', () => {
  it('is a switch', async () => {
    const onChange = vi.fn();
    render(<Toggle checked={false} onChange={onChange} label="Minimize while playing" />);
    await userEvent.click(screen.getByRole('switch', { name: 'Minimize while playing' }));
    expect(onChange).toHaveBeenCalledWith(true);
  });
});

describe('Dialog', () => {
  it('shows its title and closes on Escape', async () => {
    const onClose = vi.fn();
    render(
      <Dialog open title="Delete Pack?" onClose={onClose} actions={<button type="button">Delete</button>}>
        Body
      </Dialog>,
    );
    expect(screen.getByRole('dialog', { name: 'Delete Pack?' })).toBeInTheDocument();
    screen.getByRole('dialog').dispatchEvent(new Event('cancel', { cancelable: true }));
    expect(onClose).toHaveBeenCalled();
  });
});

describe('ErrorCard', () => {
  it('shows the translated message, the fix, and details on request', async () => {
    render(
      <ErrorCard
        error={{ category: 'disk', code: 'disk_io', message: 'x', fix: 'y', detail: 'permission denied', params: {} }}
      />,
    );
    expect(screen.getByText("Couldn't read or write a launcher file.")).toBeInTheDocument();
    expect(screen.queryByText('permission denied')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Details' }));
    expect(screen.getByText('permission denied')).toBeInTheDocument();
  });
});

describe('ComingSoon', () => {
  it('is visibly unavailable and not interactive', () => {
    render(<ComingSoon label="Paste a pack link" />);
    const tile = screen.getByText('Paste a pack link').closest('[aria-disabled]');
    expect(tile).toHaveAttribute('aria-disabled', 'true');
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    expect(screen.getByText('Coming soon')).toBeInTheDocument();
  });
});

describe('Menu', () => {
  it('opens, runs an item, and closes on Escape returning focus', async () => {
    const onSelect = vi.fn();
    render(<Menu label="More options" items={[{ label: 'Rename', onSelect }]} />);
    const trigger = screen.getByRole('button', { name: 'More options' });
    await userEvent.click(trigger);
    expect(screen.getByRole('menuitem', { name: 'Rename' })).toHaveFocus();
    await userEvent.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
    await userEvent.click(trigger);
    await userEvent.click(screen.getByRole('menuitem', { name: 'Rename' }));
    expect(onSelect).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter ui test`
Expected: FAIL (missing modules).

- [ ] **Step 3: Write the tokens and component styles**

Replace `ui/src/styles/app.css` with:

```css
@import 'tailwindcss';
@import '@fontsource/bungee/400.css';
@import '@fontsource-variable/atkinson-hyperlegible-next';
@import '@fontsource/atkinson-hyperlegible-mono/400.css';

/* Arcade Cabinet: warm laminate black; the four land-plot hues each own one job;
   raised controls sit on a dirt-brown step; corners are pixel-notched. */
@theme {
  --color-ground: #16121a;
  --color-panel: #201a26;
  --color-raised: #2b2333;
  --color-line: #3d3346;
  --color-ink: #f6f1e7;
  --color-ink-dim: #bfb3c4;
  --color-amber: #ffc233; /* act: Play, primary */
  --color-amber-hi: #ffd76e;
  --color-lime: #c9cc2a; /* running, OK, progress fill */
  --color-coral: #ff5a5f; /* stop, fault */
  --color-blue: #2186cb; /* info */
  --color-blue-ink: #5cb3f0; /* info text on ground */
  --color-dirt: #7a4a2a; /* the step under raised controls */
  --color-dirt-deep: #4e2e19;

  --font-display: 'Bungee', ui-sans-serif, system-ui, sans-serif;
  --font-sans: 'Atkinson Hyperlegible Next Variable', ui-sans-serif, system-ui, sans-serif;
  --font-mono: 'Atkinson Hyperlegible Mono', ui-monospace, monospace;

  --radius-step: 4px;
  --ease-out-expo: cubic-bezier(0.16, 1, 0.3, 1);
}

@layer base {
  html,
  body,
  #root {
    height: 100%;
  }
  body {
    margin: 0;
    background: var(--color-ground);
    color: var(--color-ink);
    font-family: var(--font-sans);
    font-size: 15px;
    line-height: 1.5;
    overflow: hidden;
    user-select: none;
    -webkit-font-smoothing: antialiased;
  }
  h1,
  h2,
  h3 {
    text-wrap: balance;
  }
  p {
    text-wrap: pretty;
  }
  ::selection {
    background: var(--color-amber);
    color: var(--color-ground);
  }
  input,
  textarea {
    user-select: text;
    caret-color: var(--color-amber);
  }
  :focus-visible {
    outline: 2px solid var(--color-amber);
    outline-offset: 3px;
  }
  * {
    scrollbar-width: thin;
    scrollbar-color: var(--color-line) transparent;
  }
  a {
    color: var(--color-blue-ink);
    text-underline-offset: 3px;
  }
  @media (prefers-reduced-motion: reduce) {
    *,
    *::before,
    *::after {
      animation-duration: 0.01ms !important;
      animation-iteration-count: 1 !important;
      transition-duration: 0.01ms !important;
    }
  }
}

@layer components {
  .stepped {
    border-radius: var(--radius-step);
    corner-shape: notch;
  }
  .selectable {
    user-select: text;
  }
  .tabular {
    font-variant-numeric: tabular-nums;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .panel {
    background: var(--color-panel);
    border-radius: 6px;
    corner-shape: notch;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    padding: 0.125rem 0.5rem;
    font-size: 0.75rem;
    font-weight: 650;
    color: var(--color-ink-dim);
    background: var(--color-raised);
    border-radius: var(--radius-step);
    corner-shape: notch;
  }

  /* Raised controls sit on a dirt step; pressing drops the face onto it in two 90 ms steps. */
  .btn {
    --step: 4px;
    --face: var(--color-raised);
    --face-ink: var(--color-ink);
    --base: var(--color-dirt-deep);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    min-height: 2.5rem;
    padding: 0 1rem;
    font-weight: 650;
    font-size: 0.9375rem;
    line-height: 1;
    white-space: nowrap;
    color: var(--face-ink);
    background: var(--face);
    border: 0;
    border-radius: var(--radius-step);
    corner-shape: notch;
    box-shadow: 0 var(--step) 0 var(--base);
    cursor: pointer;
    transition:
      translate 180ms steps(2, end),
      box-shadow 180ms steps(2, end),
      background-color 120ms linear;
  }
  .btn svg {
    width: 1.25rem;
    height: 1.25rem;
    flex: none;
  }
  .btn:hover {
    --face: color-mix(in oklab, var(--color-raised), white 7%);
  }
  .btn:active:not(:disabled),
  .btn[data-pressed='true'] {
    translate: 0 var(--step);
    box-shadow: 0 0 0 var(--base);
  }
  .btn:disabled {
    --face: var(--color-panel);
    --face-ink: var(--color-ink-dim);
    translate: 0 var(--step);
    box-shadow: inset 0 0 0 1px var(--color-line);
    cursor: not-allowed;
  }
  .btn-primary {
    --face: var(--color-amber);
    --face-ink: var(--color-ground);
    --base: var(--color-dirt);
  }
  .btn-primary:hover {
    --face: var(--color-amber-hi);
  }
  .btn-danger {
    --face: var(--color-coral);
    --face-ink: var(--color-ground);
  }
  .btn-danger:hover {
    --face: color-mix(in oklab, var(--color-coral), white 12%);
  }
  .btn-ghost {
    --face: transparent;
    box-shadow: none;
  }
  .btn-ghost:hover {
    --face: var(--color-raised);
  }
  .btn-ghost:active:not(:disabled) {
    translate: 0 0;
  }
  .btn-lg {
    --step: 6px;
    min-height: 3rem;
    padding: 0 1.5rem;
    font-size: 1rem;
  }
  .btn-icon {
    min-height: 2.25rem;
    min-width: 2.25rem;
    padding: 0;
  }

  /* The one lit button. */
  .play {
    --step: 8px;
    --lit: var(--color-amber-hi);
    display: grid;
    align-content: center;
    gap: 0.5rem;
    width: 17.5rem;
    min-height: 5.5rem;
    padding: 0.75rem 1.5rem;
    color: var(--color-ground);
    background: var(--color-amber);
    border: 0;
    border-radius: 6px;
    corner-shape: notch;
    box-shadow:
      0 var(--step) 0 var(--color-dirt),
      inset 0 3px 0 var(--lit);
    font-family: var(--font-display);
    font-size: 2rem;
    line-height: 1;
    letter-spacing: 0.02em;
    text-align: center;
    cursor: pointer;
    transition:
      translate 180ms steps(2, end),
      box-shadow 180ms steps(2, end);
  }
  .play[data-state='idle']:not(:disabled) {
    animation: play-lit 2.4s steps(3, end) infinite alternate;
  }
  .play:active:not(:disabled) {
    translate: 0 var(--step);
    box-shadow:
      0 0 0 var(--color-dirt),
      inset 0 3px 0 var(--lit);
  }
  .play[data-state='running'] {
    --lit: #ff9a9d;
    background: var(--color-coral);
    box-shadow:
      0 var(--step) 0 var(--color-dirt-deep),
      inset 0 3px 0 var(--lit);
  }
  .play[data-state='queued'],
  .play[data-state='preparing'] {
    color: var(--color-ink);
    background: var(--color-panel);
    box-shadow:
      0 var(--step) 0 var(--color-dirt-deep),
      inset 0 0 0 2px var(--color-line);
    font-family: var(--font-sans);
    font-size: 0.9375rem;
    font-weight: 650;
    letter-spacing: 0;
    text-align: left;
  }
  .play:disabled {
    color: var(--color-ink-dim);
    background: var(--color-panel);
    box-shadow: inset 0 0 0 2px var(--color-line);
    translate: 0 var(--step);
    cursor: not-allowed;
    animation: none;
  }
  @keyframes play-lit {
    to {
      --lit: #fff2c4;
    }
  }

  .meter {
    display: grid;
    grid-template-columns: repeat(20, 1fr);
    gap: 3px;
    height: 0.75rem;
  }
  .meter > span {
    background: var(--color-line);
  }
  .meter > span[data-on] {
    background: var(--color-lime);
  }

  .field-input {
    width: 100%;
    min-height: 2.5rem;
    padding: 0.5rem 0.75rem;
    color: var(--color-ink);
    background: var(--color-ground);
    border: 0;
    border-radius: var(--radius-step);
    corner-shape: notch;
    box-shadow:
      inset 0 2px 0 rgb(0 0 0 / 0.45),
      inset 0 0 0 1px var(--color-line);
    font: inherit;
  }
  .field-input::placeholder {
    color: var(--color-ink-dim);
  }
  .field-input[aria-invalid='true'] {
    box-shadow:
      inset 0 2px 0 rgb(0 0 0 / 0.45),
      inset 0 0 0 2px var(--color-coral);
  }

  .toggle {
    position: relative;
    display: inline-flex;
    width: 3rem;
    height: 1.75rem;
    padding: 3px;
    background: var(--color-ground);
    border: 0;
    border-radius: var(--radius-step);
    corner-shape: notch;
    box-shadow: inset 0 0 0 1px var(--color-line);
    cursor: pointer;
  }
  .toggle > span {
    width: 1.375rem;
    height: 100%;
    background: var(--color-ink-dim);
    border-radius: 2px;
    transition: translate 180ms steps(2, end);
  }
  .toggle[aria-checked='true'] > span {
    translate: 1.125rem 0;
    background: var(--color-lime);
  }

  .dialog {
    width: min(30rem, calc(100vw - 4rem));
    padding: 1.5rem;
    color: var(--color-ink);
    background: var(--color-panel);
    border: 0;
    border-radius: 6px;
    corner-shape: notch;
    box-shadow:
      0 8px 0 var(--color-dirt-deep),
      0 24px 48px rgb(0 0 0 / 0.5);
  }
  .dialog::backdrop {
    background: rgb(10 8 12 / 0.72);
  }

  .menu {
    position: absolute;
    right: 0;
    bottom: calc(100% + 0.5rem);
    z-index: 30;
    min-width: 13rem;
    padding: 0.375rem;
    background: var(--color-raised);
    border-radius: 6px;
    corner-shape: notch;
    box-shadow:
      0 6px 0 var(--color-dirt-deep),
      0 18px 36px rgb(0 0 0 / 0.45);
  }
  .menu [role='menuitem'] {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.625rem;
    padding: 0.5rem 0.75rem;
    color: var(--color-ink);
    background: transparent;
    border: 0;
    border-radius: var(--radius-step);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .menu [role='menuitem']:hover,
  .menu [role='menuitem']:focus-visible {
    background: var(--color-panel);
    outline: none;
  }
  .menu [role='menuitem'][data-danger] {
    color: var(--color-coral);
  }
  .menu svg {
    width: 1.125rem;
    height: 1.125rem;
    flex: none;
  }
}
```

- [ ] **Step 4: Write the primitives**

`ui/src/ui/Icon.ts`:

```ts
// One icon system: pixelarticons (MIT), drawn on a 24px pixel grid that matches the mark.
export {
  Check,
  ChevronDown,
  Close,
  Copy,
  Download,
  ExternalLink,
  Folder,
  Gear,
  InfoBox,
  Languages,
  MemoryStick,
  Minus,
  MoreHorizontal,
  Package,
  Pencil,
  Play,
  Plus,
  Reload,
  Square,
  Terminal,
  Trash,
  User,
  Users,
  WarningDiamond,
  WindowFrame,
} from 'pixelarticons/react';
```

Check that each name exists with `ls ui/node_modules/pixelarticons/react | grep -E "^(Gear|WindowFrame|WarningDiamond|MemoryStick)\.js$"`. If one is missing, pick the closest icon of the same meaning from that folder.

`ui/src/ui/format.ts`:

```ts
const units = ['KB', 'MB', 'GB'] as const;

export function formatBytes(bytes: number, lang: string): string {
  let value = bytes / 1000;
  let unit: (typeof units)[number] = 'KB';
  for (const next of units.slice(1)) {
    if (value < 1000) break;
    value /= 1000;
    unit = next;
  }
  const digits = value < 10 && unit !== 'KB' ? 1 : 0;
  const shown = new Intl.NumberFormat(lang, { maximumFractionDigits: digits }).format(Math.max(value, 1));
  return `${shown} ${unit}`;
}

export function formatCount(n: number, lang: string): string {
  return new Intl.NumberFormat(lang).format(n);
}

/** Memory in GB for labels: 6144 → "6", 2560 → "2.5". */
export function formatGb(mb: number, lang: string): string {
  return new Intl.NumberFormat(lang, { maximumFractionDigits: 1 }).format(mb / 1024);
}
```

`ui/src/ui/Button.tsx`:

```tsx
import type { ButtonHTMLAttributes, ReactNode, Ref } from 'react';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'danger' | 'ghost';
  size?: 'md' | 'lg' | 'icon';
  icon?: ReactNode;
  /** React 19 passes refs as props; it reaches the <button> through `rest`. */
  ref?: Ref<HTMLButtonElement>;
}

export function Button({ variant = 'secondary', size = 'md', icon, className, children, type = 'button', ...rest }: ButtonProps) {
  const classes = [
    'btn',
    variant !== 'secondary' && `btn-${variant}`,
    size === 'lg' && 'btn-lg',
    size === 'icon' && 'btn-icon',
    className,
  ]
    .filter(Boolean)
    .join(' ');
  return (
    <button type={type} className={classes} {...rest}>
      {icon}
      {children}
    </button>
  );
}
```

`ui/src/ui/Meter.tsx`:

```tsx
export function Meter({
  value,
  label,
  segments = 20,
  decorative = false,
}: {
  value: number;
  label: string;
  segments?: number;
  /** Inside a button the button's name already says it; hide the meter from assistive tech. */
  decorative?: boolean;
}) {
  const clamped = Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0));
  const lit = Math.round(clamped * segments);
  const a11y = decorative
    ? { 'aria-hidden': true }
    : {
        role: 'progressbar',
        'aria-label': label,
        'aria-valuemin': 0,
        'aria-valuemax': 100,
        'aria-valuenow': Math.round(clamped * 100),
      };
  return (
    <div className="meter" {...a11y}>
      {Array.from({ length: segments }, (_, i) => (
        <span key={i} data-on={i < lit ? '' : undefined} />
      ))}
    </div>
  );
}
```

`ui/src/ui/PlayButton.tsx`:

```tsx
import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { taskLabel } from '../ipc/errors';
import type { RunStateKind } from '../ipc/schema';
import type { ProgressView } from '../state/runStore';
import { formatBytes, formatCount } from './format';
import { Meter } from './Meter';

export interface PlayButtonProps {
  state: RunStateKind;
  progress: ProgressView | null;
  disabled?: boolean;
  disabledReason?: string;
  onPlay: () => void;
  onStop: () => void;
}

export function PlayButton({ state, progress, disabled, disabledReason, onPlay, onStop }: PlayButtonProps) {
  const { t, i18n } = useTranslation();
  const reasonId = useId();
  const lang = i18n.language;

  if (state === 'queued' || state === 'preparing') {
    const label =
      state === 'queued' ? t('play.queued') : progress ? taskLabel(i18n, progress.task) : t('play.preparing');
    const fraction = progress && progress.total > 0 ? progress.done / progress.total : 0;
    return (
      <button type="button" className="play" data-state={state} onClick={onStop} aria-label={t('play.cancelLabel', { label })}>
        <span className="flex items-baseline justify-between gap-3">
          <span className="truncate">{label}</span>
          <span className="text-ink-dim">{t('play.cancel')}</span>
        </span>
        <Meter value={fraction} label={label} decorative />
        {progress && progress.total > 0 && (
          <span className="tabular text-sm font-normal text-ink-dim">
            {t('play.files', { done: formatCount(progress.done, lang), total: formatCount(progress.total, lang) })}
            {progress.bytes > 0 && ` · ${formatBytes(progress.bytes, lang)}`}
          </span>
        )}
      </button>
    );
  }

  if (state === 'running' || state === 'stopping') {
    return (
      <button type="button" className="play" data-state="running" onClick={onStop} disabled={state === 'stopping'}>
        {state === 'stopping' ? t('play.stopping') : t('play.stop')}
      </button>
    );
  }

  return (
    <>
      <button
        type="button"
        className="play"
        data-state="idle"
        onClick={onPlay}
        disabled={disabled}
        aria-describedby={disabled && disabledReason ? reasonId : undefined}
      >
        {t('play.play')}
      </button>
      {disabled && disabledReason && (
        <span id={reasonId} className="sr-only">
          {disabledReason}
        </span>
      )}
    </>
  );
}
```

`ui/src/ui/Dialog.tsx`:

```tsx
import { useEffect, useId, useRef, type ReactNode } from 'react';

/** A native modal <dialog>: focus is trapped, Escape closes, focus returns to the opener. */
export function Dialog({
  open,
  onClose,
  title,
  children,
  actions,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children?: ReactNode;
  actions: ReactNode;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      className="dialog"
      aria-labelledby={titleId}
      onClose={() => {
        if (open) onClose();
      }}
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
    >
      {open && (
        <>
          <h2 id={titleId} className="text-lg font-bold">
            {title}
          </h2>
          {children && <div className="mt-2 text-ink-dim">{children}</div>}
          <div className="mt-6 flex justify-end gap-3">{actions}</div>
        </>
      )}
    </dialog>
  );
}
```

`ui/src/ui/Toggle.tsx`:

```tsx
export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (next: boolean) => void; label: string }) {
  return (
    <button type="button" role="switch" aria-checked={checked} aria-label={label} className="toggle" onClick={() => onChange(!checked)}>
      <span />
    </button>
  );
}
```

`ui/src/ui/Field.tsx`:

```tsx
import { cloneElement, useId, type ReactElement } from 'react';

type InputProps = { id?: string; 'aria-describedby'?: string; 'aria-invalid'?: boolean };

/** A label above, the control, then a hint or an error. The child input gets the ids it needs. */
export function Field({
  label,
  hint,
  error,
  children,
}: {
  label: string;
  hint?: string | null;
  error?: string | null;
  children: ReactElement<InputProps>;
}) {
  const id = useId();
  const noteId = `${id}-note`;
  const note = error ?? hint;
  return (
    <div className="grid gap-1.5">
      <label htmlFor={id} className="text-sm font-semibold">
        {label}
      </label>
      {cloneElement(children, {
        id,
        'aria-describedby': note ? noteId : undefined,
        'aria-invalid': error ? true : undefined,
      })}
      {note && (
        <p id={noteId} className={error ? 'text-sm text-coral' : 'text-sm text-ink-dim'}>
          {note}
        </p>
      )}
    </div>
  );
}
```

`ui/src/ui/Select.tsx`:

```tsx
import { useId } from 'react';

export interface Option {
  value: string;
  label: string;
}

export function Select({
  label,
  value,
  onChange,
  options,
  disabled,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: Option[];
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <div className="grid gap-1.5">
      <label htmlFor={id} className="text-sm font-semibold">
        {label}
      </label>
      <select id={id} className="field-input" value={value} disabled={disabled} onChange={(e) => onChange(e.target.value)}>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </div>
  );
}
```

`ui/src/ui/Menu.tsx`:

```tsx
import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { Button } from './Button';
import { MoreHorizontal } from './Icon';

export interface MenuItem {
  label: string;
  onSelect: () => void;
  icon?: ReactNode;
  danger?: boolean;
}

export function Menu({ label, items }: { label: string; items: MenuItem[] }) {
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    list.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const onPointer = (event: PointerEvent) => {
      if (!list.current?.contains(event.target as Node) && !trigger.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener('pointerdown', onPointer);
    return () => document.removeEventListener('pointerdown', onPointer);
  }, [open]);

  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const entries = [...(list.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    const index = entries.indexOf(document.activeElement as HTMLElement);
    if (event.key === 'Escape') {
      event.preventDefault();
      close();
    } else if (event.key === 'ArrowDown') {
      event.preventDefault();
      entries[(index + 1) % entries.length]?.focus();
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      entries[(index - 1 + entries.length) % entries.length]?.focus();
    }
  };

  return (
    <div className="relative">
      <Button
        ref={trigger}
        size="icon"
        className="btn-lg"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        icon={<MoreHorizontal />}
        onClick={() => setOpen((v) => !v)}
      />
      {open && (
        <div ref={list} role="menu" aria-label={label} className="menu" onKeyDown={onKeyDown}>
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              role="menuitem"
              data-danger={item.danger ? '' : undefined}
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
            >
              {item.icon}
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
```

`ui/src/ui/ErrorCard.tsx`:

```tsx
import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { describeError } from '../ipc/errors';
import { Button } from './Button';
import { WarningDiamond } from './Icon';

/** A failure, in the player's language: what happened, what to do, details on request. */
export function ErrorCard({ error, actions }: { error: unknown; actions?: ReactNode }) {
  const { t, i18n } = useTranslation();
  const [showDetails, setShowDetails] = useState(false);
  const text = describeError(i18n, error);
  return (
    <div role="alert" className="panel grid gap-3 p-5">
      <div className="flex items-start gap-3">
        <WarningDiamond className="mt-0.5 h-6 w-6 flex-none text-coral" aria-hidden />
        <div className="grid gap-1">
          <p className="font-semibold">{text.message}</p>
          {text.fix && <p className="text-ink-dim">{text.fix}</p>}
        </div>
      </div>
      <div className="flex flex-wrap gap-3">
        {actions}
        {text.detail && (
          <Button variant="ghost" aria-expanded={showDetails} onClick={() => setShowDetails((v) => !v)}>
            {showDetails ? t('common.hideDetails') : t('common.details')}
          </Button>
        )}
      </div>
      {showDetails && text.detail && (
        <pre className="selectable max-h-48 overflow-auto rounded bg-ground p-3 font-mono text-sm whitespace-pre-wrap text-ink-dim">
          {text.detail}
        </pre>
      )}
    </div>
  );
}
```

`ui/src/ui/ComingSoon.tsx`:

```tsx
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

/** A feature that exists in the plan but not yet in the launcher: visible, honest, inert. */
export function ComingSoon({ label, icon }: { label: string; icon?: ReactNode }) {
  const { t } = useTranslation();
  return (
    <div aria-disabled="true" className="panel flex items-center gap-3 px-4 py-3 text-ink-dim opacity-80">
      {icon}
      <span className="font-semibold">{label}</span>
      <span className="badge ml-auto">{t('common.comingSoon')}</span>
    </div>
  );
}
```

`ui/src/ui/Toasts.tsx`:

```tsx
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { describeError } from '../ipc/errors';
import { dismissToast, useToasts, type Toast } from '../state/toasts';
import { Button } from './Button';
import { Close, WarningDiamond } from './Icon';

function ToastItem({ toast }: { toast: Toast }) {
  const { t, i18n } = useTranslation();
  const text = describeError(i18n, toast.error);
  useEffect(() => {
    const timer = setTimeout(() => dismissToast(toast.id), 8000);
    return () => clearTimeout(timer);
  }, [toast.id]);
  return (
    <li className="panel flex items-start gap-3 p-4 shadow-[0_6px_0_var(--color-dirt-deep)]">
      <WarningDiamond className="mt-0.5 h-5 w-5 flex-none text-coral" aria-hidden />
      <div className="grid flex-1 gap-0.5">
        <p className="font-semibold">{text.message}</p>
        {text.fix && <p className="text-sm text-ink-dim">{text.fix}</p>}
      </div>
      <Button variant="ghost" size="icon" aria-label={t('common.dismiss')} icon={<Close />} onClick={() => dismissToast(toast.id)} />
    </li>
  );
}

export function Toasts() {
  const toasts = useToasts();
  return (
    <ol role="status" aria-live="polite" className="pointer-events-auto fixed right-4 bottom-4 z-40 grid w-[24rem] gap-3">
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} />
      ))}
    </ol>
  );
}
```

- [ ] **Step 5: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): Arcade Cabinet tokens, fonts and primitives (buttons, Play, meter, dialog, menu, cards)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS. If `corner-shape` is flagged by a CSS linter, keep it. It degrades to a plain 4 px radius on engines without support (WebKitGTK).

---

### Task 24: Generated art: marquee landscapes, rail plot icons and avatars

Before editing, read `C:\Users\mmxtu\.claude\skills\impeccable\reference\craft-floor.md`.

**Files:**
- Create: `ui/src/art/random.ts`, `ui/src/art/marquee.ts`, `ui/src/art/Marquee.tsx`, `ui/src/art/plot.ts`, `ui/src/art/PlotIcon.tsx`, `ui/src/art/avatar.ts`, `ui/src/art/Avatar.tsx`
- Test: `ui/src/art/art.test.ts`

**Interfaces:**
- Produces:
  - `hash(text) -> number` (FNV-1a), `rng(seed) -> () => number` (mulberry32), `mix(a, b, t) -> hex`, `PLOT_HUES`
  - `marqueeScene(seed) -> { sky: string[], sun: {x, y, color}, layers: {grass, body, heights: number[], duration}[] }`. A layer's heights repeat with period 40 columns.
  - `<Marquee seed attract className />`. It is an SVG with a 480×270 viewBox, with layers that drift in parallax while `attract` is true.
  - `plotCells(seed) -> {x, y, fill}[]` on an 8×8 grid, and `<PlotIcon seed size />`
  - `avatarCells(uuid) -> {x, y, fill}[]` on an 8×8 grid, left-right symmetric, and `<Avatar uuid size />`

- [ ] **Step 1: Write the failing tests**

`ui/src/art/art.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { avatarCells } from './avatar';
import { marqueeScene } from './marquee';
import { plotCells } from './plot';
import { hash, mix, rng } from './random';

describe('random', () => {
  it('is stable across runs', () => {
    expect(hash('asalith-fields')).toBe(hash('asalith-fields'));
    expect(hash('a')).not.toBe(hash('b'));
    const a = rng(42);
    const b = rng(42);
    expect([a(), a(), a()]).toEqual([b(), b(), b()]);
  });

  it('mixes colours', () => {
    expect(mix('#000000', '#ffffff', 0.5)).toBe('#808080');
    expect(mix('#ff0000', '#0000ff', 0)).toBe('#ff0000');
  });
});

describe('marquee', () => {
  it('is deterministic per instance and differs between instances', () => {
    expect(marqueeScene('asalith-fields')).toEqual(marqueeScene('asalith-fields'));
    expect(marqueeScene('asalith-fields')).not.toEqual(marqueeScene('creative-flat'));
  });

  it('tiles seamlessly and stays inside the frame', () => {
    const scene = marqueeScene('asalith-fields');
    expect(scene.layers.length).toBeGreaterThanOrEqual(4);
    for (const layer of scene.layers) {
      expect(layer.heights).toHaveLength(40);
      expect(layer.heights[0]).toBe(layer.heights[39]);
      for (const h of layer.heights) {
        expect(h).toBeGreaterThanOrEqual(1);
        expect(h).toBeLessThanOrEqual(20);
      }
    }
  });
});

describe('plot icon and avatar', () => {
  it('draw inside an 8×8 grid, deterministically', () => {
    for (const cells of [plotCells('pack'), avatarCells('5627dd98-e6be-3c21-b8a8-e92344183641')]) {
      expect(cells.length).toBeGreaterThan(8);
      for (const c of cells) {
        expect(c.x).toBeGreaterThanOrEqual(0);
        expect(c.x).toBeLessThan(8);
        expect(c.y).toBeGreaterThanOrEqual(0);
        expect(c.y).toBeLessThan(8);
      }
    }
    expect(plotCells('pack')).toEqual(plotCells('pack'));
  });

  it('makes avatars symmetric', () => {
    const cells = avatarCells('uuid-1');
    for (const c of cells) {
      expect(cells.some((o) => o.x === 7 - c.x && o.y === c.y && o.fill === c.fill)).toBe(true);
    }
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm --filter ui test`
Expected: FAIL (missing modules).

- [ ] **Step 3: Implement the generators**

`ui/src/art/random.ts`:

```ts
/** FNV-1a: a stable 32-bit hash, so an instance's art is the same on every computer. */
export function hash(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** mulberry32: a tiny seeded PRNG returning numbers in [0, 1). */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** The mark's four plot colours, in mark order: lime, amber, coral, blue. */
export const PLOT_HUES = ['#c9cc2a', '#ffc233', '#ff5a5f', '#2186cb'] as const;
export const DIRT = '#7a4a2a';
export const GROUND = '#16121a';

export function mix(a: string, b: string, t: number): string {
  const pa = [1, 3, 5].map((i) => parseInt(a.slice(i, i + 2), 16));
  const pb = [1, 3, 5].map((i) => parseInt(b.slice(i, i + 2), 16));
  return `#${pa
    .map((v, i) => Math.round(v + ((pb[i] ?? 0) - v) * t).toString(16).padStart(2, '0'))
    .join('')}`;
}
```

`ui/src/art/marquee.ts`:

```ts
import { DIRT, GROUND, hash, mix, PLOT_HUES, rng } from './random';

/** The marquee is a 480×270 frame on a 12-unit pixel grid: 40 columns, 22.5 rows. */
export const COLS = 40;
export const CELL = 12;
const HALF = COLS / 2;

export interface MarqueeLayer {
  grass: string;
  body: string;
  /** Height of each column in cells, mirrored so the strip tiles without a seam. */
  heights: number[];
  /** Seconds for one full drift; nearer layers move faster. */
  duration: number;
}

export interface MarqueeScene {
  sky: string[];
  sun: { x: number; y: number; color: string };
  layers: MarqueeLayer[];
}

/** A terraced landscape lit from behind, seeded by the instance id. */
export function marqueeScene(seed: string): MarqueeScene {
  const random = rng(hash(seed));
  const accent = PLOT_HUES[hash(seed) % PLOT_HUES.length] ?? PLOT_HUES[0];
  const skyTop = '#1d1630';
  const horizon = mix(skyTop, accent, 0.5);
  const sky = Array.from({ length: 6 }, (_, i) => mix(skyTop, horizon, i / 5));
  const sun = {
    x: 4 + Math.floor(random() * (COLS - 10)),
    y: 4 + Math.floor(random() * 4),
    color: mix(accent, '#fff6d8', 0.6),
  };
  // The nearest band is the dark cabinet floor the instance name sits on.
  const bands = [
    { min: 12, max: 17, hue: 0.25 },
    { min: 9, max: 14, hue: 0.4 },
    { min: 6, max: 10, hue: 0.6 },
    { min: 4, max: 7, hue: 1 },
  ];
  const layers = bands.map((band, depth) => {
    const half: number[] = [];
    let h = band.min + Math.floor(random() * (band.max - band.min + 1));
    for (let i = 0; i < HALF; i++) {
      const roll = random();
      if (roll < 0.3) h -= 1;
      else if (roll > 0.7) h += 1;
      h = Math.max(band.min, Math.min(band.max, h));
      half.push(h);
    }
    const near = depth === bands.length - 1;
    return {
      grass: near ? accent : mix(GROUND, PLOT_HUES[(hash(seed) + depth) % PLOT_HUES.length] ?? accent, band.hue * 0.6),
      body: near ? GROUND : mix(GROUND, DIRT, 0.15 + depth * 0.12),
      heights: [...half, ...half.slice().reverse()],
      duration: 160 - depth * 35,
    };
  });
  return { sky, sun, layers };
}

/** One layer as a path of columns, drawn twice side by side so it can drift and wrap. */
export function layerPath(heights: number[], rows: number, offsetCols: number, grassOnly: boolean): string {
  return heights
    .map((h, i) => {
      const x = (i + offsetCols) * CELL;
      const top = (rows - h) * CELL;
      const height = grassOnly ? CELL : h * CELL - CELL;
      const y = grassOnly ? top : top + CELL;
      return `M${x} ${y}h${CELL}v${height}h-${CELL}z`;
    })
    .join('');
}
```

`ui/src/art/Marquee.tsx`:

```tsx
import { useMemo } from 'react';
import { CELL, COLS, layerPath, marqueeScene } from './marquee';

const W = COLS * CELL; // 480
const H = 270;
const ROWS = H / CELL; // 22.5

/** The instance's backlit marquee. In attract mode the layers drift in slow parallax. */
export function Marquee({ seed, attract, className }: { seed: string; attract: boolean; className?: string }) {
  const scene = useMemo(() => marqueeScene(seed), [seed]);
  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="xMidYMax slice"
      shapeRendering="crispEdges"
      aria-hidden="true"
      className={className}
    >
      {scene.sky.map((color, i) => (
        <rect key={color + i} x={0} y={(i * H) / scene.sky.length / 1.6} width={W} height={H} fill={color} />
      ))}
      <rect x={scene.sun.x * CELL} y={scene.sun.y * CELL} width={CELL * 3} height={CELL * 3} fill={scene.sun.color} />
      {scene.layers.map((layer, i) => (
        <g key={i} className="marquee-layer" style={{ animationDuration: `${layer.duration}s`, animationPlayState: attract ? 'running' : 'paused' }}>
          <path d={layerPath(layer.heights, ROWS, 0, false) + layerPath(layer.heights, ROWS, COLS, false)} fill={layer.body} />
          <path d={layerPath(layer.heights, ROWS, 0, true) + layerPath(layer.heights, ROWS, COLS, true)} fill={layer.grass} />
        </g>
      ))}
    </svg>
  );
}
```

Add to `ui/src/styles/app.css` inside `@layer components`:

```css
  .marquee-layer {
    animation: marquee-drift linear infinite;
  }
  @keyframes marquee-drift {
    to {
      transform: translateX(-480px);
    }
  }
```

`ui/src/art/plot.ts`:

```ts
import { DIRT, hash, PLOT_HUES, rng } from './random';

export interface Cell {
  x: number;
  y: number;
  fill: string;
}

/** A tiny terraced plot like one quarter of the mark: stepped grass on dirt, in the instance's hue. */
export function plotCells(seed: string): Cell[] {
  const random = rng(hash(`plot:${seed}`));
  const hue = PLOT_HUES[hash(seed) % PLOT_HUES.length] ?? PLOT_HUES[0];
  const rising = random() < 0.5;
  const cells: Cell[] = [];
  let height = 2 + Math.floor(random() * 2);
  for (let col = 0; col < 8; col++) {
    const x = rising ? col : 7 - col;
    if (col > 0 && col % 2 === 0 && random() < 0.8) height = Math.min(7, height + 1);
    const top = 8 - height;
    cells.push({ x, y: top, fill: hue });
    for (let y = top + 1; y < 8; y++) cells.push({ x, y, fill: DIRT });
  }
  return cells;
}
```

`ui/src/art/PlotIcon.tsx`:

```tsx
import { useMemo } from 'react';
import { plotCells } from './plot';

export function PlotIcon({ seed, size = 44 }: { seed: string; size?: number }) {
  const cells = useMemo(() => plotCells(seed), [seed]);
  return (
    <svg viewBox="0 0 8 8" width={size} height={size} shapeRendering="crispEdges" aria-hidden="true">
      {cells.map((c) => (
        <rect key={`${c.x}-${c.y}`} x={c.x} y={c.y} width={1} height={1} fill={c.fill} />
      ))}
    </svg>
  );
}
```

`ui/src/art/avatar.ts`:

```ts
import type { Cell } from './plot';
import { hash, mix, PLOT_HUES, rng } from './random';

/** A blocky, symmetric 8×8 head generated from the account UUID: never a Mojang skin. */
export function avatarCells(uuid: string): Cell[] {
  const random = rng(hash(`avatar:${uuid}`));
  const hue = PLOT_HUES[hash(uuid) % PLOT_HUES.length] ?? PLOT_HUES[0];
  const skin = mix('#e8b48a', '#7a4a2a', random() * 0.6);
  const hair = mix(hue, '#16121a', 0.35);
  const cells: Cell[] = [];
  for (let y = 0; y < 8; y++) {
    for (let x = 0; x < 4; x++) {
      let fill = skin;
      if (y < 2 || (y === 2 && random() < 0.5) || (x === 0 && y < 5 && random() < 0.6)) fill = hair;
      if (y === 4 && x === 2) fill = '#16121a'; // eyes
      if (y === 6 && x === 3) fill = mix(skin, '#16121a', 0.45); // mouth
      cells.push({ x, y, fill }, { x: 7 - x, y, fill });
    }
  }
  return cells;
}
```

`ui/src/art/Avatar.tsx`:

```tsx
import { useMemo } from 'react';
import { avatarCells } from './avatar';

export function Avatar({ uuid, size = 32 }: { uuid: string; size?: number }) {
  const cells = useMemo(() => avatarCells(uuid), [uuid]);
  return (
    <svg viewBox="0 0 8 8" width={size} height={size} shapeRendering="crispEdges" aria-hidden="true" className="stepped flex-none">
      {cells.map((c) => (
        <rect key={`${c.x}-${c.y}`} x={c.x} y={c.y} width={1} height={1} fill={c.fill} />
      ))}
    </svg>
  );
}
```

- [ ] **Step 4: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): generated marquee landscapes, plot icons and avatars

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---
## Phase 5: Screens

Every screen task starts by reading `C:\Users\mmxtu\.claude\skills\impeccable\reference\craft-floor.md` and the direction contract. Screens are tested in isolation against the mock backend (Task 22). Task 34 assembles the app.

### Task 25: Title bar, instance rail, screen frame and the test backend

**Files:**
- Create: `ui/src/test/backend.tsx`
- Create: `ui/src/features/home/pick.ts`, `ui/src/features/home/labels.ts`
- Create: `ui/src/ui/ScreenFrame.tsx`
- Create: `ui/src/features/titlebar/TitleBar.tsx`, `ui/src/features/titlebar/TitleBar.test.tsx`
- Create: `ui/src/features/rail/Rail.tsx`, `ui/src/features/rail/Rail.test.tsx`
- Modify: `ui/src/state/runStore.ts` (add `reset()`), `ui/src/test/setup.ts` (reset global stores), `ui/src/styles/app.css` (title bar and rail styles)

**Interfaces:**
- Consumes: Tasks 19–24.
- Produces:
  - `renderApp(ui, options?: MockOptions) -> { calls: [cmd, args][], backend, user, qc, ... }`
  - `pickId(listing, requested) -> string | null`
  - `pick(listing, requested) -> { kind: 'instance', instance } | { kind: 'damaged', damaged } | { kind: 'empty' }`
  - `loaderName(kind)`, `versionLine(t, instance)`
  - `ScreenFrame({ title, subtitle?, onBack?, actions?, children })`
  - `TitleBar()`, `Rail()`

- [ ] **Step 1: Test plumbing**

Add a `reset()` method to the object returned by `createRunStore()` in `ui/src/state/runStore.ts`:

```ts
    reset() {
      runs = new Map();
      logs = new Map();
      activeIds = [];
      notify();
    },
```

Replace the `afterEach` in `ui/src/test/setup.ts` with:

```ts
afterEach(async () => {
  cleanup();
  clearMocks();
  runStore.reset();
  resetRoute();
  clearToasts();
  resetActivity();
  await i18n.changeLanguage('en');
});
```

Import `runStore` from `../state/runStore`, `resetRoute` from `../state/route`, `clearToasts` from `../state/toasts` and `resetActivity` from `../state/activity`. Also add stubs at the top of `setup.ts` for what jsdom lacks and TanStack Virtual uses:

```ts
if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}
if (!Element.prototype.scrollTo) {
  Element.prototype.scrollTo = function scrollTo() {};
}
```

Create `ui/src/test/backend.tsx`:

```tsx
import type { ReactElement } from 'react';
import { createMockBackend, type MockOptions } from '../ipc/mock';
import { renderWithProviders } from './render';

/** Render against the fixture-seeded mock backend and record every command sent to it. */
export function renderApp(ui: ReactElement, options: MockOptions = {}) {
  const backend = createMockBackend(options);
  const calls: [string, unknown][] = [];
  const result = renderWithProviders(ui, {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      return backend.handle(cmd, args);
    },
  });
  return { ...result, backend, calls };
}

export const sent = (calls: [string, unknown][], cmd: string) => calls.filter(([c]) => c === cmd).map(([, args]) => args);
```

- [ ] **Step 2: Shared helpers**

`ui/src/features/home/pick.ts`:

```ts
import type { DamagedInstance, Instance, InstanceListing } from '../../ipc/schema';
import { lastInstance } from '../../state/route';

export type Picked =
  | { kind: 'instance'; instance: Instance }
  | { kind: 'damaged'; damaged: DamagedInstance }
  | { kind: 'empty' };

/** The instance a screen is about: the requested one, else the last one opened, else the first. */
export function pickId(listing: InstanceListing | undefined, requested: string | null): string | null {
  if (!listing) return null;
  const ids = [...listing.instances.map((i) => i.id), ...listing.damaged.map((d) => d.id)];
  if (requested && ids.includes(requested)) return requested;
  const last = lastInstance();
  if (last && ids.includes(last)) return last;
  return ids[0] ?? null;
}

export function pick(listing: InstanceListing | undefined, requested: string | null): Picked {
  const id = pickId(listing, requested);
  const instance = listing?.instances.find((i) => i.id === id);
  if (instance) return { kind: 'instance', instance };
  const damaged = listing?.damaged.find((d) => d.id === id);
  if (damaged) return { kind: 'damaged', damaged };
  return { kind: 'empty' };
}
```

`ui/src/features/home/labels.ts`:

```ts
import type { TFunction } from 'i18next';
import type { Instance, LoaderKind } from '../../ipc/schema';

const NAMES: Record<LoaderKind, string> = { fabric: 'Fabric', quilt: 'Quilt', forge: 'Forge', neoforge: 'NeoForge' };

export const loaderName = (kind: LoaderKind) => NAMES[kind];

/** "Minecraft 1.20.1 · Fabric 0.16.14" or "Minecraft 26.3 · Vanilla". */
export function versionLine(t: TFunction, instance: Instance): string {
  const mc = t('home.minecraftLine', { version: instance.game.minecraft });
  const loader = instance.game.loader;
  const second = loader ? t('home.loaderLine', { loader: loaderName(loader.kind), version: loader.version }) : t('home.vanilla');
  return `${mc} · ${second}`;
}
```

`ui/src/ui/ScreenFrame.tsx`:

```tsx
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from './Button';
import { ChevronDown } from './Icon';

/** The frame every non-home screen uses: a title row with optional back and actions, then content. */
export function ScreenFrame({
  title,
  subtitle,
  onBack,
  actions,
  children,
}: {
  title: string;
  subtitle?: string;
  onBack?: () => void;
  actions?: ReactNode;
  children: ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <section className="flex h-full flex-col">
      <header className="flex flex-wrap items-end justify-between gap-6 px-10 pt-9 pb-6">
        <div className="flex min-w-0 items-center gap-4">
          {onBack && (
            <Button variant="ghost" size="icon" aria-label={t('common.back')} onClick={onBack} icon={<ChevronDown className="rotate-90" />} />
          )}
          <div className="min-w-0">
            <h1 className="truncate font-display text-[2rem] leading-none">{title}</h1>
            {subtitle && <p className="mt-2 text-ink-dim">{subtitle}</p>}
          </div>
        </div>
        {actions && <div className="flex flex-wrap gap-3">{actions}</div>}
      </header>
      <div className="min-h-0 flex-1 overflow-auto px-10 pb-10">{children}</div>
    </section>
  );
}
```

Add to `ui/src/styles/app.css` inside `@layer components`:

```css
  .tb-btn {
    display: grid;
    place-items: center;
    height: 2.25rem;
    min-width: 2.75rem;
    padding: 0 0.75rem;
    gap: 0.5rem;
    color: var(--color-ink-dim);
    background: transparent;
    border: 0;
    font: inherit;
    font-size: 0.875rem;
    cursor: pointer;
  }
  .tb-btn svg {
    width: 1.125rem;
    height: 1.125rem;
  }
  .tb-btn:hover {
    color: var(--color-ink);
    background: var(--color-raised);
  }
  .tb-close:hover {
    color: var(--color-ground);
    background: var(--color-coral);
  }
  .rail-item {
    position: relative;
    display: grid;
    place-items: center;
    width: 3.5rem;
    height: 3.5rem;
    background: transparent;
    border: 0;
    border-radius: 6px;
    corner-shape: notch;
    cursor: pointer;
  }
  .rail-item:hover {
    background: var(--color-panel);
  }
  .rail-item[data-selected] {
    background: var(--color-raised);
    box-shadow: 0 4px 0 var(--color-dirt-deep);
  }
  /* The lit coin-slot light beside the selected cabinet. */
  .rail-item[data-selected]::before {
    content: '';
    position: absolute;
    left: -0.625rem;
    top: 30%;
    width: 4px;
    height: 40%;
    background: var(--color-amber);
  }
  .rail-led {
    position: absolute;
    right: 0.375rem;
    bottom: 0.375rem;
    width: 0.5rem;
    height: 0.5rem;
    background: var(--color-lime);
  }
  .rail-warn {
    position: absolute;
    right: 0.125rem;
    bottom: 0.125rem;
    width: 1.125rem;
    height: 1.125rem;
    color: var(--color-coral);
  }
```

- [ ] **Step 3: Write the failing tests**

`ui/src/features/titlebar/TitleBar.test.tsx`:

```tsx
import { screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { TitleBar } from './TitleBar';

it('drives the window and shows the active account', async () => {
  const { user, calls } = renderApp(<TitleBar />);
  expect(await screen.findByRole('button', { name: /Steve/ })).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Minimize' }));
  await user.click(screen.getByRole('button', { name: 'Maximize' }));
  expect(sent(calls, 'plugin:window|minimize')).toHaveLength(1);
  expect(sent(calls, 'plugin:window|toggle_maximize')).toHaveLength(1);
  await user.click(screen.getByRole('button', { name: 'Settings' }));
  expect(routeSnapshot()).toEqual({ name: 'settings' });
  await user.click(screen.getByRole('button', { name: /Steve/ }));
  expect(routeSnapshot()).toEqual({ name: 'accounts' });
});

it('asks for an account when there is none', async () => {
  renderApp(<TitleBar />, { firstRun: true });
  expect(await screen.findByRole('button', { name: 'Add account' })).toBeInTheDocument();
});
```

`ui/src/features/rail/Rail.test.tsx`:

```tsx
import { act, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp } from '../../test/backend';
import { Rail } from './Rail';

it('lists instances, then damaged ones, and selects on click', async () => {
  const { user } = renderApp(<Rail />);
  const asalith = await screen.findByRole('button', { name: 'Asalith Fields' });
  expect(screen.getByRole('button', { name: 'Creative Flat' })).toBeInTheDocument();
  expect(screen.getByRole('button', { name: 'old-pack, Needs attention' })).toBeInTheDocument();
  expect(asalith).toHaveAttribute('aria-current', 'page');
  await user.click(screen.getByRole('button', { name: 'Creative Flat' }));
  expect(routeSnapshot()).toEqual({ name: 'home', id: 'creative-flat' });
});

it('moves focus with the arrow keys and enters at the selected item', async () => {
  const { user } = renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  await user.tab();
  expect(screen.getByRole('button', { name: 'Asalith Fields' })).toHaveFocus();
  await user.keyboard('{ArrowDown}');
  expect(screen.getByRole('button', { name: 'Creative Flat' })).toHaveFocus();
  await user.keyboard('{End}');
  expect(screen.getByRole('button', { name: 'old-pack, Needs attention' })).toHaveFocus();
});

it('marks a running instance', async () => {
  renderApp(<Rail />);
  await screen.findByRole('button', { name: 'Asalith Fields' });
  act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null }));
  expect(screen.getByRole('button', { name: 'Asalith Fields, Playing' })).toBeInTheDocument();
});
```

Run: `pnpm --filter ui test`
Expected: FAIL (missing components).

- [ ] **Step 4: Implement the title bar and the rail**

`ui/src/features/titlebar/TitleBar.tsx`:

```tsx
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Avatar } from '../../art/Avatar';
import mark from '../../assets/mark.svg';
import { useAccounts } from '../../state/queries';
import { navigate } from '../../state/route';
import { Close, Gear, Minus, Square, WindowFrame } from '../../ui/Icon';

/** The custom title bar: drag anywhere that isn't a button. */
export function TitleBar() {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const active = accounts.data?.accounts.find((a) => a.uuid === accounts.data?.active) ?? null;
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    let alive = true;
    let unlisten: (() => void) | undefined;
    const sync = () =>
      void win.isMaximized().then((value) => {
        if (alive) setMaximized(Boolean(value));
      });
    sync();
    void win.onResized(sync).then((stop) => {
      if (alive) unlisten = stop;
      else stop();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  const win = getCurrentWindow();
  return (
    <header className="flex h-9 items-center justify-between bg-ground" data-tauri-drag-region>
      <div className="flex items-center gap-2 pl-3" data-tauri-drag-region>
        <img src={mark} alt="" width={21} height={18} data-tauri-drag-region />
        <span className="text-sm font-bold" data-tauri-drag-region>
          {t('app.name')}
        </span>
      </div>
      <div className="flex items-center">
        <button type="button" className="tb-btn flex" onClick={() => navigate({ name: 'accounts' })}>
          {active ? (
            <>
              <Avatar uuid={active.uuid} size={20} />
              <span className="font-semibold text-ink">{active.username}</span>
            </>
          ) : (
            t('titlebar.noAccount')
          )}
        </button>
        <button type="button" className="tb-btn" aria-label={t('titlebar.settings')} onClick={() => navigate({ name: 'settings' })}>
          <Gear aria-hidden />
        </button>
        <span className="mx-1 h-4 w-px bg-line" aria-hidden />
        <button type="button" className="tb-btn" aria-label={t('titlebar.minimize')} onClick={() => void win.minimize()}>
          <Minus aria-hidden />
        </button>
        <button
          type="button"
          className="tb-btn"
          aria-label={maximized ? t('titlebar.restore') : t('titlebar.maximize')}
          onClick={() => void win.toggleMaximize()}
        >
          {maximized ? <WindowFrame aria-hidden /> : <Square aria-hidden />}
        </button>
        <button type="button" className="tb-btn tb-close" aria-label={t('titlebar.close')} onClick={() => void win.close()}>
          <Close aria-hidden />
        </button>
      </div>
    </header>
  );
}
```

`ui/src/features/rail/Rail.tsx`:

```tsx
import { useMemo, useRef, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { PlotIcon } from '../../art/PlotIcon';
import { useInstances } from '../../state/queries';
import { navigate, useRoute, instanceOf } from '../../state/route';
import { useRun } from '../../state/runStore';
import { Button } from '../../ui/Button';
import { Plus, WarningDiamond } from '../../ui/Icon';
import { pickId } from '../home/pick';

interface Item {
  id: string;
  name: string;
  damaged: boolean;
}

function RailItem({
  item,
  selected,
  entry,
  setRef,
}: {
  item: Item;
  selected: boolean;
  /** The one item Tab lands on: the selected one, or the first when none is selected. */
  entry: boolean;
  setRef: (el: HTMLButtonElement | null) => void;
}) {
  const { t } = useTranslation();
  const run = useRun(item.id);
  const status = item.damaged
    ? t('rail.damaged')
    : run.state === 'running' || run.state === 'stopping'
      ? t('rail.playing')
      : run.state === 'queued' || run.state === 'preparing'
        ? t('rail.preparing')
        : null;
  return (
    <button
      ref={setRef}
      type="button"
      className="rail-item"
      data-selected={selected ? '' : undefined}
      aria-current={selected ? 'page' : undefined}
      aria-label={status ? `${item.name}, ${status}` : item.name}
      title={item.name}
      tabIndex={entry ? 0 : -1}
      onClick={() => navigate({ name: 'home', id: item.id })}
    >
      <span className={item.damaged ? 'opacity-40 grayscale' : undefined}>
        <PlotIcon seed={item.id} size={40} />
      </span>
      {status && !item.damaged && <span className="rail-led" aria-hidden />}
      {item.damaged && <WarningDiamond className="rail-warn" aria-hidden />}
    </button>
  );
}

/** Every instance as a cabinet icon; the selected one has the lit light. Arrow keys move along it. */
export function Rail() {
  const { t } = useTranslation();
  const { data } = useInstances();
  const route = useRoute();
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const items = useMemo<Item[]>(
    () => [
      ...(data?.instances ?? []).map((i) => ({ id: i.id, name: i.name, damaged: false })),
      ...(data?.damaged ?? []).map((d) => ({ id: d.id, name: d.id, damaged: true })),
    ],
    [data],
  );
  const selected = route.name === 'home' || 'id' in route ? pickId(data, instanceOf(route)) : null;

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    const buttons = refs.current.filter((b): b is HTMLButtonElement => b !== null);
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next =
      event.key === 'ArrowDown' ? index + 1 : event.key === 'ArrowUp' ? index - 1 : event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : null;
    if (next === null || buttons.length === 0) return;
    event.preventDefault();
    buttons[(next + buttons.length) % buttons.length]?.focus();
  };

  return (
    <nav aria-label={t('rail.label')} className="flex min-h-0 flex-col items-center gap-3 bg-ground py-4">
      <ul className="flex min-h-0 flex-col items-center gap-3 overflow-y-auto px-3" onKeyDown={onKeyDown}>
        {items.map((item, index) => (
          <li key={item.id}>
            <RailItem
              item={item}
              selected={item.id === selected}
              entry={selected ? item.id === selected : index === 0}
              setRef={(el) => {
                refs.current[index] = el;
              }}
            />
          </li>
        ))}
      </ul>
      <Button variant="ghost" size="icon" className="btn-lg" aria-label={t('rail.newInstance')} title={t('rail.newInstance')} icon={<Plus />} onClick={() => navigate({ name: 'newInstance' })} />
    </nav>
  );
}
```

- [ ] **Step 5: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): custom title bar, instance rail and screen frame

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---

### Task 26: Home: the lit cabinet, Play, the instance menu and damaged instances

**Files:**
- Create: `ui/src/features/home/Home.tsx`, `ui/src/features/home/InstanceMenu.tsx`, `ui/src/features/home/DamagedView.tsx`, `ui/src/features/home/EmptyState.tsx`
- Test: `ui/src/features/home/Home.test.tsx`

**Interfaces:**
- Consumes: `pick`, `versionLine`, `Marquee`, `PlayButton`, `Menu`, `Dialog`, `Field`, `ErrorCard`, `useRun`, the query and mutation hooks, `commands`, `pushError`.
- Produces: `Home({ id: string | null })`.

- [ ] **Step 1: Write the failing tests**

`ui/src/features/home/Home.test.tsx`:

```tsx
import { act, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { Home } from './Home';

describe('Home', () => {
  it('shows the selected instance with its versions and plays it', async () => {
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    expect(await screen.findByRole('heading', { name: 'Asalith Fields' })).toBeInTheDocument();
    expect(screen.getByText('Minecraft 1.20.1 · Fabric 0.16.14')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Play' }));
    expect(sent(calls, 'play')).toEqual([{ id: 'asalith-fields' }]);
  });

  it('shows progress while preparing and stops while running', async () => {
    const { user, calls } = renderApp(<Home id="asalith-fields" />);
    await screen.findByRole('heading', { name: 'Asalith Fields' });
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'preparing', exit: null });
      runStore.apply({
        type: 'progress',
        instance: 'asalith-fields',
        task: { key: 'install_minecraft', params: { version: '1.20.1' } },
        done: 3,
        total: 6,
        bytes: 0,
      });
    });
    expect(screen.getByRole('button', { name: 'Cancel: Installing Minecraft 1.20.1' })).toBeInTheDocument();
    act(() => runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null }));
    await user.click(screen.getByRole('button', { name: 'Stop' }));
    expect(sent(calls, 'stop')).toEqual([{ id: 'asalith-fields' }]);
  });

  it('shows the empty state for a fresh install', async () => {
    renderApp(<Home id="creative-flat" />, { firstRun: true });
    expect(await screen.findByRole('heading', { name: 'No instances yet' })).toBeInTheDocument();
  });

  it('disables Play without an account and links to Accounts', async () => {
    const { backend, user } = renderApp(<Home id="creative-flat" />);
    backend.state.accounts.active = null;
    await screen.findByRole('heading', { name: 'Creative Flat' });
    expect(screen.getByRole('button', { name: 'Play' })).toBeDisabled();
    await user.click(await screen.findByRole('button', { name: 'Add an account to play' }));
    expect(routeSnapshot()).toEqual({ name: 'accounts' });
  });

  it('offers to create the first instance when there are none', async () => {
    const { user } = renderApp(<Home id={null} />, { firstRun: true });
    await user.click(await screen.findByRole('button', { name: 'Create an instance' }));
    expect(routeSnapshot()).toEqual({ name: 'newInstance' });
  });

  it('explains a damaged instance and can delete it', async () => {
    const { user, calls } = renderApp(<Home id="old-pack" />);
    expect(await screen.findByText('The settings file of “old-pack” is damaged.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Delete' }));
    const dialog = screen.getByRole('dialog', { name: 'Delete “old-pack”?' });
    await user.click(within(dialog).getByRole('button', { name: 'Delete' }));
    expect(sent(calls, 'instance_delete')).toEqual([{ id: 'old-pack' }]);
  });

  it('renames from the menu', async () => {
    const { user, calls } = renderApp(<Home id="creative-flat" />);
    await screen.findByRole('heading', { name: 'Creative Flat' });
    await user.click(screen.getByRole('button', { name: 'More options' }));
    await user.click(screen.getByRole('menuitem', { name: 'Rename' }));
    const input = screen.getByRole('textbox', { name: 'Name' });
    await user.clear(input);
    await user.type(input, 'Flat World');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(sent(calls, 'instance_rename')).toEqual([{ id: 'creative-flat', name: 'Flat World' }]);
  });
});
```

Run: `pnpm --filter ui test`
Expected: FAIL.

- [ ] **Step 2: Implement**

`ui/src/features/home/EmptyState.tsx`:

```tsx
import { useTranslation } from 'react-i18next';
import mark from '../../assets/mark.svg';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Plus } from '../../ui/Icon';

export function EmptyState() {
  const { t } = useTranslation();
  return (
    <div className="grid h-full place-items-center p-10">
      <div className="grid max-w-md justify-items-center gap-5 text-center">
        <img src={mark} alt="" width={140} height={120} />
        <h1 className="font-display text-3xl">{t('home.emptyTitle')}</h1>
        <p className="text-ink-dim">{t('home.emptyBody')}</p>
        <Button variant="primary" size="lg" icon={<Plus />} onClick={() => navigate({ name: 'newInstance' })}>
          {t('home.emptyAction')}
        </Button>
      </div>
    </div>
  );
}
```

`ui/src/features/home/DamagedView.tsx`:

```tsx
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { DamagedInstance } from '../../ipc/schema';
import { useDeleteInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Folder, Trash } from '../../ui/Icon';

export function DamagedView({ damaged }: { damaged: DamagedInstance }) {
  const { t } = useTranslation();
  const [confirming, setConfirming] = useState(false);
  const remove = useDeleteInstance();
  return (
    <div className="grid h-full place-items-center p-10">
      <div className="grid w-full max-w-xl gap-6">
        <h1 className="font-display text-3xl">{t('damaged.title', { id: damaged.id })}</h1>
        <ErrorCard
          error={damaged.error}
          actions={
            <>
              <Button icon={<Folder />} onClick={() => commands.openFolder({ kind: 'instance', id: damaged.id }).catch(pushError)}>
                {t('common.openFolder')}
              </Button>
              <Button variant="danger" icon={<Trash />} onClick={() => setConfirming(true)}>
                {t('common.delete')}
              </Button>
            </>
          }
        />
      </div>
      {confirming && (
        <Dialog
          open
          onClose={() => setConfirming(false)}
          title={t('damaged.deleteTitle', { id: damaged.id })}
          actions={
            <>
              <Button onClick={() => setConfirming(false)}>{t('common.cancel')}</Button>
              <Button
                variant="danger"
                onClick={() =>
                  remove.mutate(damaged.id, {
                    onSuccess: () => {
                      setConfirming(false);
                      navigate({ name: 'home', id: null });
                    },
                  })
                }
              >
                {t('common.delete')}
              </Button>
            </>
          }
        >
          {t('damaged.deleteBody')}
        </Dialog>
      )}
    </div>
  );
}
```

`ui/src/features/home/InstanceMenu.tsx`:

```tsx
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { useDeleteInstance, useRenameInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Folder, Gear, Pencil, Trash } from '../../ui/Icon';
import { Menu } from '../../ui/Menu';

function RenameDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
  const { t } = useTranslation();
  const [name, setName] = useState(instance.name);
  const rename = useRenameInstance({ inline: true });
  return (
    <Dialog
      open
      onClose={onClose}
      title={t('home.renameTitle')}
      actions={
        <>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button
            variant="primary"
            disabled={!name.trim() || rename.isPending}
            onClick={() => rename.mutate({ id: instance.id, name }, { onSuccess: onClose })}
          >
            {t('common.save')}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 pt-2">
        <Field label={t('home.nameLabel')}>
          <input className="field-input" value={name} maxLength={64} onChange={(e) => setName(e.target.value)} />
        </Field>
        {rename.isError && <ErrorCard error={rename.error} />}
      </div>
    </Dialog>
  );
}

function DeleteDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
  const { t } = useTranslation();
  const remove = useDeleteInstance();
  return (
    <Dialog
      open
      onClose={onClose}
      title={t('home.deleteTitle', { name: instance.name })}
      actions={
        <>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button
            variant="danger"
            onClick={() =>
              remove.mutate(instance.id, {
                onSuccess: () => {
                  onClose();
                  navigate({ name: 'home', id: null });
                },
              })
            }
          >
            {t('home.deleteConfirm')}
          </Button>
        </>
      }
    >
      {t('home.deleteBody')}
    </Dialog>
  );
}

export function InstanceMenu({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const [dialog, setDialog] = useState<'rename' | 'delete' | null>(null);
  const close = () => setDialog(null);
  return (
    <>
      <Menu
        label={t('home.more')}
        items={[
          { label: t('home.instanceSettings'), icon: <Gear />, onSelect: () => navigate({ name: 'instanceSettings', id: instance.id }) },
          { label: t('common.rename'), icon: <Pencil />, onSelect: () => setDialog('rename') },
          {
            label: t('common.openFolder'),
            icon: <Folder />,
            onSelect: () => void commands.openFolder({ kind: 'instance', id: instance.id }).catch(pushError),
          },
          { label: t('common.delete'), icon: <Trash />, danger: true, onSelect: () => setDialog('delete') },
        ]}
      />
      {dialog === 'rename' && <RenameDialog instance={instance} onClose={close} />}
      {dialog === 'delete' && <DeleteDialog instance={instance} onClose={close} />}
    </>
  );
}
```

`ui/src/features/home/Home.tsx`:

```tsx
import { useTranslation } from 'react-i18next';
import { Marquee } from '../../art/Marquee';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { useAccounts, useInstances } from '../../state/queries';
import { navigate } from '../../state/route';
import { useRun } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Package } from '../../ui/Icon';
import { PlayButton } from '../../ui/PlayButton';
import { DamagedView } from './DamagedView';
import { EmptyState } from './EmptyState';
import { InstanceMenu } from './InstanceMenu';
import { versionLine } from './labels';
import { pick } from './pick';

function InstanceView({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const run = useRun(instance.id);
  const accounts = useAccounts();
  const hasAccount = Boolean(accounts.data?.active);
  return (
    <section className="relative h-full overflow-hidden" aria-labelledby="instance-name">
      <Marquee seed={instance.id} attract={run.state === 'idle'} className="absolute inset-0 h-full w-full" />
      <div className="absolute inset-x-0 bottom-0 flex flex-wrap items-end justify-between gap-x-10 gap-y-6 px-10 pt-6 pb-12">
        <div className="min-w-0 max-w-[60ch]">
          <h1 id="instance-name" className="font-display text-[clamp(2.5rem,5.4vw,5.5rem)] leading-[0.95]">
            {instance.name}
          </h1>
          <p className="mt-3 text-ink-dim">{versionLine(t, instance)}</p>
        </div>
        <div className="flex items-end gap-3">
          <Button size="lg" icon={<Package />} onClick={() => navigate({ name: 'mods', id: instance.id })}>
            {t('home.mods')}
          </Button>
          <InstanceMenu instance={instance} />
          <div className="grid justify-items-end gap-3">
            <PlayButton
              state={run.state}
              progress={run.progress}
              disabled={!hasAccount}
              disabledReason={t('play.noAccount')}
              onPlay={() => void commands.play(instance.id).catch(pushError)}
              onStop={() => void commands.stop(instance.id).catch(pushError)}
            />
            {!hasAccount && accounts.isSuccess && (
              <button type="button" className="text-sm text-blue-ink underline underline-offset-4" onClick={() => navigate({ name: 'accounts' })}>
                {t('play.noAccount')}
              </button>
            )}
          </div>
        </div>
      </div>
    </section>
  );
}

export function Home({ id }: { id: string | null }) {
  const listing = useInstances();
  if (listing.isPending) return <div className="h-full bg-panel" aria-busy="true" />;
  const picked = pick(listing.data, id);
  switch (picked.kind) {
    case 'instance':
      return <InstanceView instance={picked.instance} />;
    case 'damaged':
      return <DamagedView damaged={picked.damaged} />;
    case 'empty':
      return <EmptyState />;
  }
}
```

- [ ] **Step 3: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): home with the lit cabinet, Play, instance menu, empty and damaged states

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---

### Task 27: Activity drawer: progress, the game log and the crash card

**Files:**
- Create: `ui/src/state/exit.ts`, `ui/src/state/exit.test.ts`
- Create: `ui/src/features/activity/ActivityDrawer.tsx`, `ui/src/features/activity/LogView.tsx`, `ui/src/features/activity/CrashCard.tsx`
- Test: `ui/src/features/activity/ActivityDrawer.test.tsx`

**Interfaces:**
- Consumes: `useActivity`, `openActivity`, `closeActivity`, `runStore`, `useRun`, `useLog`, `useActiveIds`, `useUpdateInstance`, `useAppInfo`, `useInstances`, `Meter`, `ErrorCard`, `taskLabel`, `supportText`.
- Produces:
  - `isCrash(exit) -> boolean`, mirroring Rust's `ExitInfo::is_crash`
  - `suggestMemory(currentMb, systemMb) -> number | null`
  - `ActivityDrawer()`, `LogView({ lines })`, `CrashCard({ instance, exit, lines })`

- [ ] **Step 1: Write the failing tests**

`ui/src/state/exit.test.ts`:

```ts
import { expect, it } from 'vitest';
import { isCrash, suggestMemory } from './exit';

const base = { code: 0, killed: false, cancelled: false, crash: null, error: null } as const;

it('matches the Rust definition of a crash', () => {
  expect(isCrash({ ...base, code: 1 })).toBe(true);
  expect(isCrash({ ...base, code: null })).toBe(true);
  expect(isCrash(base)).toBe(false);
  expect(isCrash({ ...base, code: null, killed: true })).toBe(false);
  expect(isCrash({ ...base, code: null, cancelled: true })).toBe(false);
});

it('suggests 2 GB more memory, within what the computer can spare', () => {
  expect(suggestMemory(4096, 16384)).toBe(6144);
  expect(suggestMemory(10240, 16384)).toBe(12288);
  expect(suggestMemory(12288, 16384)).toBeNull(); // already at 75 %
  expect(suggestMemory(2048, 4096)).toBe(3072);
});
```

`ui/src/features/activity/ActivityDrawer.test.tsx`:

```tsx
import { act, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { openActivity } from '../../state/activity';
import { runStore } from '../../state/runStore';
import { renderApp, sent } from '../../test/backend';
import { ActivityDrawer } from './ActivityDrawer';

describe('ActivityDrawer', () => {
  it('opens from its handle and shows the game log', async () => {
    const { user } = renderApp(<ActivityDrawer />);
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'running', exit: null });
      runStore.apply({ type: 'log', instance: 'asalith-fields', lines: ['[INFO] Hello', '[INFO] World'] });
    });
    await user.click(screen.getByRole('button', { name: /Activity/ }));
    const log = await screen.findByRole('log', { name: 'Game log' });
    expect(log).toHaveTextContent('[INFO] Hello');
    expect(log).toHaveTextContent('[INFO] World');
  });

  it('explains an out-of-memory crash and raises memory in one click', async () => {
    const { user, calls } = renderApp(<ActivityDrawer />);
    act(() => {
      runStore.apply({ type: 'run_state', instance: 'asalith-fields', state: 'queued', exit: null });
      runStore.apply({
        type: 'run_state',
        instance: 'asalith-fields',
        state: 'idle',
        exit: { code: 1, killed: false, cancelled: false, crash: 'out_of_memory', error: null },
      });
      openActivity('asalith-fields');
    });
    expect(await screen.findByText('Minecraft ran out of memory')).toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: 'Raise to 6 GB' }));
    const update = sent(calls, 'instance_update')[0] as { instance: { java: { memoryMb: number } } };
    expect(update.instance.java.memoryMb).toBe(6144);
  });

  it('shows a failed start with the translated error', async () => {
    renderApp(<ActivityDrawer />);
    act(() => {
      runStore.apply({
        type: 'run_state',
        instance: 'creative-flat',
        state: 'idle',
        exit: {
          code: null,
          killed: false,
          cancelled: false,
          crash: null,
          error: { category: 'network', code: 'offline', message: 'x', fix: null, detail: null, params: {} },
        },
      });
      openActivity('creative-flat');
    });
    expect(await screen.findByText("Creative Flat couldn't start")).toBeInTheDocument();
    expect(screen.getByText("You seem to be offline, and this hasn't been downloaded before.")).toBeInTheDocument();
  });
});
```

Run: `pnpm --filter ui test`
Expected: FAIL.

- [ ] **Step 2: Implement**

`ui/src/state/exit.ts`:

```ts
import type { ExitInfo } from '../ipc/schema';

/** Same rule as Rust's `ExitInfo::is_crash`: an exit nobody asked for that wasn't clean. */
export function isCrash(exit: ExitInfo): boolean {
  return !exit.killed && !exit.cancelled && exit.error === null && exit.code !== 0;
}

/** 2 GB more, never above three quarters of this computer's memory. Null when there's no room. */
export function suggestMemory(currentMb: number, systemMb: number): number | null {
  const ceiling = Math.floor((systemMb * 0.75) / 1024) * 1024;
  const next = Math.min(currentMb + 2048, ceiling, 65536);
  return next > currentMb ? next : null;
}
```

`ui/src/features/activity/LogView.tsx`:

```tsx
import { useVirtualizer } from '@tanstack/react-virtual';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '../../ui/Button';

/** The game's output, virtualized; follows the newest line until the player scrolls up. */
export function LogView({ lines }: { lines: string[] }) {
  const { t } = useTranslation();
  const scroller = useRef<HTMLDivElement>(null);
  const [follow, setFollow] = useState(true);
  const virtualizer = useVirtualizer({
    count: lines.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 20,
    overscan: 30,
    initialRect: { width: 800, height: 320 },
  });

  useEffect(() => {
    if (follow && lines.length > 0) virtualizer.scrollToIndex(lines.length - 1, { align: 'end' });
  }, [follow, lines.length, virtualizer]);

  if (lines.length === 0) {
    return <p className="p-4 text-ink-dim">{t('activity.logEmpty')}</p>;
  }

  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={scroller}
        role="log"
        aria-label={t('activity.log')}
        className="selectable h-full overflow-auto bg-ground py-2 font-mono text-[13px] leading-5"
        onScroll={(event) => {
          const el = event.currentTarget;
          setFollow(el.scrollHeight - el.scrollTop - el.clientHeight < 40);
        }}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
          {virtualizer.getVirtualItems().map((row) => (
            <div
              key={row.key}
              className="absolute left-0 w-full truncate px-4 whitespace-pre"
              style={{ transform: `translateY(${row.start}px)`, height: row.size }}
            >
              {lines[row.index]}
            </div>
          ))}
        </div>
      </div>
      {!follow && (
        <Button className="absolute right-4 bottom-4" onClick={() => setFollow(true)}>
          {t('activity.jumpToEnd')}
        </Button>
      )}
    </div>
  );
}
```

If ESLint flags `useVirtualizer` with `react-hooks/incompatible-library`, add `// eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual returns new functions each render by design` directly above the `useVirtualizer` call.

`ui/src/features/activity/CrashCard.tsx`:

```tsx
import { useTranslation } from 'react-i18next';
import { supportText } from '../../ipc/errors';
import type { ExitInfo, Instance } from '../../ipc/schema';
import { suggestMemory } from '../../state/exit';
import { useAppInfo, useUpdateInstance } from '../../state/queries';
import { runStore } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { formatGb } from '../../ui/format';
import { Close, Copy, MemoryStick, WarningDiamond } from '../../ui/Icon';
import { versionLine } from '../home/labels';

/** Why the game ended, in plain words, with the one fix the launcher can apply itself. */
export function CrashCard({ instance, exit, lines }: { instance: Instance; exit: ExitInfo; lines: string[] }) {
  const { t, i18n } = useTranslation();
  const appInfo = useAppInfo();
  const update = useUpdateInstance();
  const raised = appInfo.data ? suggestMemory(instance.java.memoryMb, appInfo.data.systemMemoryMb) : null;
  const copy = () =>
    navigator.clipboard
      .writeText(
        supportText([
          instance.name,
          versionLine(t, instance),
          `exit code: ${exit.code ?? 'none'}`,
          exit.error ? `error: ${exit.error.code}: ${exit.error.message}` : '',
          exit.error?.detail ?? '',
          '',
          ...lines.slice(-200),
        ]),
      )
      .catch(pushError);
  const dismiss = (
    <Button variant="ghost" size="icon" aria-label={t('common.dismiss')} icon={<Close />} onClick={() => runStore.dismissExit(instance.id)} />
  );

  if (exit.error) {
    return (
      <div className="grid gap-3">
        <div className="flex items-center justify-between gap-3">
          <h3 className="font-bold">{t('activity.failedTitle', { name: instance.name })}</h3>
          {dismiss}
        </div>
        <ErrorCard error={exit.error} actions={<Button icon={<Copy />} onClick={copy}>{t('activity.copyForSupport')}</Button>} />
      </div>
    );
  }

  const oom = exit.crash === 'out_of_memory';
  return (
    <div role="alert" className="panel grid gap-3 p-5">
      <div className="flex items-start gap-3">
        <WarningDiamond className="mt-0.5 h-6 w-6 flex-none text-coral" aria-hidden />
        <div className="grid flex-1 gap-1">
          <h3 className="font-bold">{oom ? t('activity.oomTitle') : t('activity.crashTitle', { name: instance.name })}</h3>
          <p className="text-ink-dim">{oom ? t('activity.oomBody') : t('activity.crashCode', { code: exit.code ?? '?' })}</p>
        </div>
        {dismiss}
      </div>
      <div className="flex flex-wrap gap-3">
        {oom && raised && (
          <Button
            variant="primary"
            icon={<MemoryStick />}
            onClick={() =>
              update.mutate(
                { ...instance, java: { ...instance.java, memoryMb: raised } },
                { onSuccess: () => runStore.dismissExit(instance.id) },
              )
            }
          >
            {t('activity.raiseMemory', { gb: formatGb(raised, i18n.language) })}
          </Button>
        )}
        <Button icon={<Copy />} onClick={copy}>
          {t('activity.copyForSupport')}
        </Button>
      </div>
    </div>
  );
}
```

`ui/src/features/activity/ActivityDrawer.tsx`:

```tsx
import { useTranslation } from 'react-i18next';
import { taskLabel } from '../../ipc/errors';
import type { Instance } from '../../ipc/schema';
import { closeActivity, openActivity, useActivity } from '../../state/activity';
import { isCrash } from '../../state/exit';
import { useInstances } from '../../state/queries';
import { useActiveIds, useLog, useRun } from '../../state/runStore';
import { formatBytes, formatCount } from '../../ui/format';
import { ChevronDown } from '../../ui/Icon';
import { Meter } from '../../ui/Meter';
import { CrashCard } from './CrashCard';
import { LogView } from './LogView';

function InstanceActivity({ instance }: { instance: Instance }) {
  const { t, i18n } = useTranslation();
  const run = useRun(instance.id);
  const lines = useLog(instance.id);
  const exit = run.lastExit;
  const showExit = exit && (exit.error || isCrash(exit));
  const progress = run.progress;
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 p-5">
      {showExit && <CrashCard instance={instance} exit={exit} lines={lines} />}
      {progress && (run.state === 'queued' || run.state === 'preparing') && (
        <div className="grid gap-2">
          <div className="flex items-baseline justify-between gap-4">
            <span className="font-semibold">{taskLabel(i18n, progress.task)}</span>
            <span className="tabular text-sm text-ink-dim">
              {t('play.files', { done: formatCount(progress.done, i18n.language), total: formatCount(progress.total, i18n.language) })}
              {progress.bytes > 0 && ` · ${formatBytes(progress.bytes, i18n.language)}`}
            </span>
          </div>
          <Meter value={progress.total > 0 ? progress.done / progress.total : 0} label={taskLabel(i18n, progress.task)} />
        </div>
      )}
      <LogView lines={lines} />
    </div>
  );
}

/** A drawer along the bottom: what's downloading, what the game prints, and why it stopped. */
export function ActivityDrawer() {
  const { t } = useTranslation();
  const activity = useActivity();
  const active = useActiveIds();
  const listing = useInstances();
  const instances = listing.data?.instances ?? [];
  const focusId = activity.focus ?? active[0] ?? instances[0]?.id ?? null;
  const focus = instances.find((i) => i.id === focusId) ?? null;
  const tabs = instances.filter((i) => active.includes(i.id) || i.id === focusId);

  return (
    <div className={`absolute inset-x-0 bottom-0 z-20 flex flex-col bg-panel ${activity.open ? 'h-[46%]' : ''}`}>
      <button
        type="button"
        className="flex h-9 flex-none items-center gap-3 px-5 text-sm font-semibold text-ink-dim hover:text-ink"
        aria-expanded={activity.open}
        onClick={() => (activity.open ? closeActivity() : openActivity())}
      >
        <ChevronDown className={`h-4 w-4 ${activity.open ? '' : 'rotate-180'}`} aria-hidden />
        {t('activity.title')}
        {active.length > 0 && <span className="badge text-lime">{active.length}</span>}
      </button>
      {activity.open && (
        <div className="flex min-h-0 flex-1 flex-col">
          {tabs.length > 1 && (
            <div role="tablist" className="flex gap-1 px-5">
              {tabs.map((i) => (
                <button
                  key={i.id}
                  role="tab"
                  type="button"
                  aria-selected={i.id === focusId}
                  className="rounded-t px-3 py-1.5 text-sm aria-selected:bg-ground"
                  onClick={() => openActivity(i.id)}
                >
                  {i.name}
                </button>
              ))}
            </div>
          )}
          {focus ? <InstanceActivity instance={focus} /> : <p className="p-5 text-ink-dim">{t('activity.empty')}</p>}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 3: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): activity drawer with progress, virtualized game log and crash card

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---

### Task 28: New instance (with the shared version picker)

**Files:**
- Create: `ui/src/features/new-instance/useVersionChoice.ts`, `ui/src/features/new-instance/VersionFields.tsx`, `ui/src/features/new-instance/NewInstance.tsx`
- Test: `ui/src/features/new-instance/NewInstance.test.tsx`
- Modify: `ui/src/styles/app.css` (`.seg` segmented control)

**Interfaces:**
- Consumes: `useMcVersions`, `useLoaderVersions`, `useCreateInstance`, `Select`, `Toggle`, `Field`, `ComingSoon`, `ErrorCard`, `ScreenFrame`, `navigate`.
- Produces:
  - `type LoaderChoice = 'none' | 'fabric' | 'quilt'`
  - `useVersionChoice(initial?: GameSpec)` returns the state plus `resolved: { minecraft: string | null; loader: LoaderSpec | null; ready: boolean }`
  - `<VersionFields choice withLoader />`
  - `NewInstance()`

- [ ] **Step 1: Write the failing test**

`ui/src/features/new-instance/NewInstance.test.tsx`:

```tsx
import { screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { NewInstance } from './NewInstance';

it('creates a Fabric instance with the newest release and stable loader by default', async () => {
  const { user, calls } = renderApp(<NewInstance />);
  const version = await screen.findByRole('combobox', { name: 'Minecraft version' });
  await screen.findByRole('option', { name: '26.3' });
  expect(version).toHaveValue('26.3');
  expect(screen.queryByRole('option', { name: '26.4-snapshot-3' })).not.toBeInTheDocument();
  await user.click(screen.getByRole('radio', { name: 'Fabric' }));
  expect(await screen.findByRole('combobox', { name: 'Loader version' })).toHaveValue('0.19.5');
  await user.click(screen.getByRole('button', { name: 'Create instance' }));
  expect(sent(calls, 'instance_create')).toEqual([
    { request: { name: 'Minecraft 26.3', minecraft: '26.3', loader: { kind: 'fabric', version: '0.19.5' } } },
  ]);
  await vi.waitFor(() => expect(routeSnapshot()).toEqual({ name: 'home', id: 'minecraft-26-3' }));
});

it('shows snapshots on request and keeps pack imports as coming soon', async () => {
  const { user } = renderApp(<NewInstance />);
  await screen.findByRole('option', { name: '26.3' });
  await user.click(screen.getByRole('switch', { name: 'Show snapshots' }));
  expect(screen.getByRole('option', { name: '26.4-snapshot-3' })).toBeInTheDocument();
  expect(screen.getByText('Paste a pack link').closest('[aria-disabled]')).toHaveAttribute('aria-disabled', 'true');
  expect(screen.getByText('Import a .mrpack file').closest('[aria-disabled]')).toHaveAttribute('aria-disabled', 'true');
});
```

Run: `pnpm --filter ui test`
Expected: FAIL.

- [ ] **Step 2: Implement**

`ui/src/features/new-instance/useVersionChoice.ts`:

```ts
import { useState } from 'react';
import type { GameSpec, LoaderSpec } from '../../ipc/schema';
import { useLoaderVersions, useMcVersions } from '../../state/queries';

export type LoaderChoice = 'none' | 'fabric' | 'quilt';

/** Version and loader state for a form. Unset choices resolve to the newest release and the
 *  newest stable loader, derived on render (no effects), so defaults follow the lists as they load. */
export function useVersionChoice(initial?: GameSpec) {
  const versions = useMcVersions();
  const [snapshots, setSnapshots] = useState(false);
  const [minecraft, setMinecraft] = useState<string | null>(initial?.minecraft ?? null);
  const [loader, setLoader] = useState<LoaderChoice>(
    initial?.loader?.kind === 'fabric' || initial?.loader?.kind === 'quilt' ? initial.loader.kind : 'none',
  );
  const [loaderVersion, setLoaderVersion] = useState<string | null>(initial?.loader?.version ?? null);

  const all = versions.data ?? [];
  const list = all.filter((v) => v.kind === 'release' || (snapshots && v.kind === 'snapshot') || v.id === minecraft);
  const mc = minecraft && (list.some((v) => v.id === minecraft) || !versions.data) ? minecraft : (list[0]?.id ?? minecraft);

  const loaders = useLoaderVersions(loader === 'none' ? null : loader, mc);
  const loaderList = loaders.data ?? [];
  // A version the player (or the instance) already has is kept even if the list no longer offers
  // it; only an unset choice falls back to the newest stable loader.
  const chosenLoader = loaderVersion ?? ((loaderList.find((v) => v.stable) ?? loaderList[0])?.version ?? null);

  const resolvedLoader: LoaderSpec | null = loader === 'none' || !chosenLoader ? null : { kind: loader, version: chosenLoader };
  return {
    versions,
    loaders,
    snapshots,
    setSnapshots,
    list,
    mc,
    setMinecraft: (next: string) => {
      setMinecraft(next);
      setLoaderVersion(null);
    },
    loader,
    setLoader: (next: LoaderChoice) => {
      setLoader(next);
      setLoaderVersion(null);
    },
    loaderList,
    chosenLoader,
    setLoaderVersion,
    resolved: {
      minecraft: mc,
      loader: resolvedLoader,
      ready: Boolean(mc) && (loader === 'none' || resolvedLoader !== null),
    },
  };
}

export type VersionChoice = ReturnType<typeof useVersionChoice>;
```

`ui/src/features/new-instance/VersionFields.tsx`:

```tsx
import { useTranslation } from 'react-i18next';
import { ErrorCard } from '../../ui/ErrorCard';
import { Select } from '../../ui/Select';
import { Toggle } from '../../ui/Toggle';
import { loaderName } from '../home/labels';
import type { LoaderChoice, VersionChoice } from './useVersionChoice';

const LOADERS: LoaderChoice[] = ['none', 'fabric', 'quilt'];
const LOADER_KEY = { none: 'newInstance.loaderNone', fabric: 'newInstance.loaderFabric', quilt: 'newInstance.loaderQuilt' } as const;

export function VersionFields({ choice, withLoader = true }: { choice: VersionChoice; withLoader?: boolean }) {
  const { t } = useTranslation();
  const options = choice.list.map((v) => ({ value: v.id, label: v.id }));
  if (choice.mc && !options.some((o) => o.value === choice.mc)) options.unshift({ value: choice.mc, label: choice.mc });
  return (
    <div className="grid gap-6">
      <div className="grid gap-3">
        <Select label={t('newInstance.version')} value={choice.mc ?? ''} onChange={choice.setMinecraft} options={options} disabled={options.length === 0} />
        <div className="flex items-center gap-3">
          <Toggle checked={choice.snapshots} onChange={choice.setSnapshots} label={t('newInstance.showSnapshots')} />
          <span className="text-sm text-ink-dim" aria-hidden>
            {t('newInstance.showSnapshots')}
          </span>
        </div>
        {choice.versions.isError && <ErrorCard error={choice.versions.error} />}
      </div>
      {withLoader && (
        <fieldset className="grid gap-3">
          <legend className="mb-1.5 text-sm font-semibold">{t('newInstance.loader')}</legend>
          <div className="flex flex-wrap gap-2">
            {LOADERS.map((kind) => (
              <label key={kind} className="seg" data-checked={choice.loader === kind ? '' : undefined}>
                <input type="radio" name="loader" className="sr-only" value={kind} checked={choice.loader === kind} onChange={() => choice.setLoader(kind)} />
                {t(LOADER_KEY[kind])}
              </label>
            ))}
          </div>
          {choice.loader !== 'none' &&
            (choice.loaders.isSuccess && choice.loaderList.length === 0 ? (
              <p className="text-ink-dim">{t('newInstance.noLoaderForVersion', { loader: loaderName(choice.loader) })}</p>
            ) : (
              <Select
                label={t('newInstance.loaderVersion')}
                value={choice.chosenLoader ?? ''}
                onChange={choice.setLoaderVersion}
                disabled={choice.loaderList.length === 0 && !choice.chosenLoader}
                options={[
                  ...(choice.chosenLoader && !choice.loaderList.some((v) => v.version === choice.chosenLoader)
                    ? [{ value: choice.chosenLoader, label: choice.chosenLoader }]
                    : []),
                  ...choice.loaderList.map((v) => ({
                    value: v.version,
                    label: t(v.stable ? 'newInstance.stable' : 'newInstance.beta', { version: v.version }),
                  })),
                ]}
              />
            ))}
          {choice.loaders.isError && <ErrorCard error={choice.loaders.error} />}
        </fieldset>
      )}
    </div>
  );
}
```

Add to `app.css` inside `@layer components`:

```css
  .seg {
    display: inline-flex;
    align-items: center;
    min-height: 2.5rem;
    padding: 0 1.125rem;
    font-weight: 650;
    background: var(--color-raised);
    border-radius: var(--radius-step);
    corner-shape: notch;
    box-shadow: 0 4px 0 var(--color-dirt-deep);
    cursor: pointer;
    transition: translate 180ms steps(2, end), box-shadow 180ms steps(2, end);
  }
  .seg[data-checked] {
    color: var(--color-ground);
    background: var(--color-amber);
    box-shadow: 0 4px 0 var(--color-dirt);
  }
  .seg:has(input:focus-visible) {
    outline: 2px solid var(--color-amber);
    outline-offset: 3px;
  }
```

`ui/src/features/new-instance/NewInstance.tsx`:

```tsx
import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCreateInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ComingSoon } from '../../ui/ComingSoon';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Download, Package } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { useVersionChoice } from './useVersionChoice';
import { VersionFields } from './VersionFields';

export function NewInstance() {
  const { t } = useTranslation();
  const choice = useVersionChoice();
  const [name, setName] = useState('');
  const create = useCreateInstance({ inline: true });
  const fallbackName = t('newInstance.namePlaceholder', { version: choice.resolved.minecraft ?? '' });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const { minecraft, loader, ready } = choice.resolved;
    if (!ready || !minecraft) return;
    create.mutate(
      { name: name.trim() || fallbackName, minecraft, loader },
      { onSuccess: (instance) => navigate({ name: 'home', id: instance.id }) },
    );
  };

  return (
    <ScreenFrame title={t('newInstance.title')} onBack={() => navigate({ name: 'home', id: null })}>
      <form onSubmit={submit} className="grid max-w-2xl gap-8">
        <Field label={t('newInstance.name')}>
          <input className="field-input" value={name} placeholder={fallbackName} maxLength={64} onChange={(e) => setName(e.target.value)} />
        </Field>
        <VersionFields choice={choice} />
        <div className="grid gap-3 sm:grid-cols-2">
          <ComingSoon label={t('newInstance.packLink')} icon={<Download className="h-5 w-5" />} />
          <ComingSoon label={t('newInstance.mrpack')} icon={<Package className="h-5 w-5" />} />
        </div>
        {create.isError && <ErrorCard error={create.error} />}
        <div className="flex justify-end gap-3">
          <Button onClick={() => navigate({ name: 'home', id: null })}>{t('common.cancel')}</Button>
          <Button type="submit" variant="primary" size="lg" disabled={!choice.resolved.ready || create.isPending}>
            {create.isPending ? t('newInstance.creating') : t('newInstance.create')}
          </Button>
        </div>
      </form>
    </ScreenFrame>
  );
}
```

- [ ] **Step 3: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): new instance screen with version and loader pickers

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---

### Task 29: Instance settings (memory, Java, versions, name)

**Files:**
- Create: `ui/src/ui/MemorySlider.tsx`
- Create: `ui/src/features/instance-settings/InstanceSettings.tsx`
- Test: `ui/src/features/instance-settings/InstanceSettings.test.tsx`

**Interfaces:**
- Consumes: `useVersionChoice`, `VersionFields`, `useUpdateInstance`, `useAppInfo`, `useInstances`, `commands.javaTest`, `open` from `@tauri-apps/plugin-dialog`.
- Produces:
  - `MemorySlider({ label, valueMb, onChange, systemMb })`, with a 512 MB step, a range of 512 to min(system, 65536), and a soft warning above half of system memory
  - `InstanceSettings({ id })`

- [ ] **Step 1: Write the failing test**

`ui/src/features/instance-settings/InstanceSettings.test.tsx`:

```tsx
import { fireEvent, screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { renderApp, sent } from '../../test/backend';
import { InstanceSettings } from './InstanceSettings';

it('saves memory and a tested custom Java', async () => {
  const { user, calls } = renderApp(<InstanceSettings id="asalith-fields" />);
  const slider = await screen.findByRole('slider', { name: 'Memory' });
  fireEvent.change(slider, { target: { value: '10240' } });
  expect(screen.getByText('10 GB')).toBeInTheDocument();
  expect(screen.getByText(/more than half of this computer's memory/)).toBeInTheDocument();
  await user.click(screen.getByRole('radio', { name: 'A Java installation I choose' }));
  await user.click(screen.getByRole('button', { name: 'Choose…' }));
  await user.click(screen.getByRole('button', { name: 'Test' }));
  expect(await screen.findByText('Java 17 works')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: 'Save' }));
  const saved = sent(calls, 'instance_update')[0] as { instance: { java: { memoryMb: number; path: string } } };
  expect(saved.instance.java).toEqual({ memoryMb: 10240, path: 'C:/Users/player/Downloads/lithium-fabric-0.11.2.jar' });
  expect(await screen.findByText('Saved')).toBeInTheDocument();
});

it('keeps the current loader and its version, even when the list no longer offers it', async () => {
  renderApp(<InstanceSettings id="asalith-fields" />);
  expect(await screen.findByRole('radio', { name: 'Fabric' })).toBeChecked();
  // The fixture list offers 0.19.x; the instance uses 0.16.14 and must keep it.
  expect(await screen.findByRole('combobox', { name: 'Loader version' })).toHaveValue('0.16.14');
});
```

(The mock's dialog always returns that jar path. For a Java pick it stands in for any chosen file; the test only checks it's passed through.)

Run: `pnpm --filter ui test`
Expected: FAIL.

- [ ] **Step 2: Implement**

`ui/src/ui/MemorySlider.tsx`:

```tsx
import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { formatGb } from './format';

export function MemorySlider({
  label,
  valueMb,
  onChange,
  systemMb,
}: {
  label: string;
  valueMb: number;
  onChange: (mb: number) => void;
  systemMb: number;
}) {
  const { t, i18n } = useTranslation();
  const id = useId();
  const max = Math.max(1024, Math.min(65536, Math.floor(systemMb / 512) * 512));
  const high = valueMb > systemMb / 2;
  return (
    <div className="grid gap-2">
      <div className="flex items-baseline justify-between">
        <label htmlFor={id} className="text-sm font-semibold">
          {label}
        </label>
        <span className="tabular font-display text-xl">{t('instanceSettings.memoryValue', { gb: formatGb(valueMb, i18n.language) })}</span>
      </div>
      <input
        id={id}
        type="range"
        min={512}
        max={max}
        step={512}
        value={Math.min(valueMb, max)}
        onChange={(e) => onChange(Number(e.target.value))}
        className="accent-[var(--color-amber)]"
      />
      <p className={high ? 'text-sm text-coral' : 'text-sm text-ink-dim'}>
        {high ? t('instanceSettings.memoryHigh') : t('instanceSettings.memoryHint', { total: formatGb(systemMb, i18n.language) })}
      </p>
    </div>
  );
}
```

`ui/src/features/instance-settings/InstanceSettings.tsx`:

```tsx
import { open } from '@tauri-apps/plugin-dialog';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { useAppInfo, useInstances, useUpdateInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Check } from '../../ui/Icon';
import { MemorySlider } from '../../ui/MemorySlider';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { useVersionChoice } from '../new-instance/useVersionChoice';
import { VersionFields } from '../new-instance/VersionFields';

function Form({ instance, systemMb }: { instance: Instance; systemMb: number }) {
  const { t } = useTranslation();
  const choice = useVersionChoice(instance.game);
  const [name, setName] = useState(instance.name);
  const [memoryMb, setMemoryMb] = useState(instance.java.memoryMb);
  const [javaMode, setJavaMode] = useState<'auto' | 'custom'>(instance.java.path ? 'custom' : 'auto');
  const [javaPath, setJavaPath] = useState<string | null>(instance.java.path);
  const [javaResult, setJavaResult] = useState<{ ok: number } | { error: unknown } | null>(null);
  const update = useUpdateInstance({ inline: true });

  const pickJava = async () => {
    const chosen = await open({ multiple: false, directory: false, title: t('instanceSettings.javaFiles') });
    if (typeof chosen === 'string') {
      setJavaPath(chosen);
      setJavaResult(null);
    }
  };
  const testJava = () => {
    if (!javaPath) return;
    commands.javaTest(javaPath).then(
      (r) => setJavaResult({ ok: r.major }),
      (error: unknown) => setJavaResult({ error }),
    );
  };
  const save = () =>
    update.mutate({
      ...instance,
      name,
      game: { minecraft: choice.resolved.minecraft ?? instance.game.minecraft, loader: choice.resolved.loader },
      java: { memoryMb, path: javaMode === 'custom' ? javaPath : null },
    });

  return (
    <div className="grid max-w-2xl gap-10">
      <Field label={t('home.nameLabel')}>
        <input className="field-input" value={name} maxLength={64} onChange={(e) => setName(e.target.value)} />
      </Field>
      <section className="grid gap-4" aria-labelledby="game-heading">
        <h2 id="game-heading" className="text-lg font-bold">{t('instanceSettings.game')}</h2>
        <VersionFields choice={choice} />
      </section>
      <MemorySlider label={t('instanceSettings.memory')} valueMb={memoryMb} onChange={setMemoryMb} systemMb={systemMb} />
      <fieldset className="grid gap-3">
        <legend className="mb-1.5 text-lg font-bold">{t('instanceSettings.java')}</legend>
        {(['auto', 'custom'] as const).map((mode) => (
          <label key={mode} className="flex items-center gap-3">
            <input type="radio" name="java" checked={javaMode === mode} onChange={() => setJavaMode(mode)} className="accent-[var(--color-amber)]" />
            {mode === 'auto' ? t('instanceSettings.javaAuto') : t('instanceSettings.javaCustom')}
          </label>
        ))}
        {javaMode === 'custom' && (
          <div className="grid gap-3 pl-7">
            <p className="selectable truncate font-mono text-sm text-ink-dim">{javaPath ?? t('instanceSettings.javaNone')}</p>
            <div className="flex gap-3">
              <Button onClick={() => void pickJava()}>{t('instanceSettings.javaPick')}</Button>
              <Button disabled={!javaPath} onClick={testJava}>{t('instanceSettings.javaTest')}</Button>
            </div>
            {javaResult && 'ok' in javaResult && (
              <p className="flex items-center gap-2 text-lime">
                <Check className="h-5 w-5" aria-hidden />
                {t('instanceSettings.javaOk', { major: javaResult.ok })}
              </p>
            )}
            {javaResult && 'error' in javaResult && <ErrorCard error={javaResult.error} />}
          </div>
        )}
      </fieldset>
      {update.isError && <ErrorCard error={update.error} />}
      <div className="flex items-center justify-end gap-4">
        <p className="text-sm text-ink-dim">{update.isSuccess ? t('common.saved') : t('instanceSettings.applyNote')}</p>
        <Button variant="primary" size="lg" disabled={!choice.resolved.ready || update.isPending} onClick={save}>
          {update.isPending ? t('common.saving') : t('common.save')}
        </Button>
      </div>
    </div>
  );
}

export function InstanceSettings({ id }: { id: string }) {
  const { t } = useTranslation();
  const listing = useInstances();
  const appInfo = useAppInfo();
  const instance = listing.data?.instances.find((i) => i.id === id);
  return (
    <ScreenFrame title={t('instanceSettings.title')} subtitle={instance?.name} onBack={() => navigate({ name: 'home', id })}>
      {instance && appInfo.data ? <Form key={instance.id} instance={instance} systemMb={appInfo.data.systemMemoryMb} /> : null}
    </ScreenFrame>
  );
}
```

- [ ] **Step 3: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): instance settings with memory, Java test and version changes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---

### Task 30: Mods

**Files:**
- Create: `ui/src/features/mods/Mods.tsx`
- Test: `ui/src/features/mods/Mods.test.tsx`

**Interfaces:**
- Consumes: `useMods`, `useModMutations`, `useInstances`, `open` (plugin-dialog), `getCurrentWebview().onDragDropEvent`, `commands.openFolder`, `Toggle`, `Dialog`, `ScreenFrame`, `formatBytes`.
- Produces: `Mods({ id })`.

- [ ] **Step 1: Write the failing test**

`ui/src/features/mods/Mods.test.tsx`:

```tsx
import { screen, within } from '@testing-library/react';
import { expect, it } from 'vitest';
import { renderApp, sent } from '../../test/backend';
import { Mods } from './Mods';

it('lists, toggles, adds and removes mods', async () => {
  const { user, calls } = renderApp(<Mods id="asalith-fields" />);
  expect(await screen.findByText('sodium-fabric-0.5.8+mc1.20.1')).toBeInTheDocument();
  expect(screen.getByText('Asalith Fields · 3 mods')).toBeInTheDocument();
  const zoomify = screen.getByRole('switch', { name: 'Use zoomify-2.11.2' });
  expect(zoomify).toHaveAttribute('aria-checked', 'false');
  await user.click(zoomify);
  expect(sent(calls, 'mod_set_enabled')).toEqual([{ id: 'asalith-fields', file: 'zoomify-2.11.2.jar.disabled', enabled: true }]);

  await user.click(screen.getByRole('button', { name: 'Add mods' }));
  expect(sent(calls, 'mods_add')).toEqual([{ id: 'asalith-fields', paths: ['C:/Users/player/Downloads/lithium-fabric-0.11.2.jar'] }]);

  await user.click(await screen.findByRole('button', { name: 'Remove sodium-fabric-0.5.8+mc1.20.1' }));
  const dialog = screen.getByRole('dialog');
  await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
  expect(sent(calls, 'mod_remove')).toEqual([{ id: 'asalith-fields', file: 'sodium-fabric-0.5.8+mc1.20.1.jar' }]);
});

it('tells vanilla players they need a loader', async () => {
  renderApp(<Mods id="creative-flat" />);
  expect(await screen.findByText(/Mods need Fabric or Quilt/)).toBeInTheDocument();
});
```

Run: `pnpm --filter ui test`
Expected: FAIL.

- [ ] **Step 2: Implement**

`ui/src/features/mods/Mods.tsx`:

```tsx
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { LocalMod } from '../../ipc/schema';
import { useInstances, useModMutations, useMods } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { formatBytes } from '../../ui/format';
import { Folder, InfoBox, Package, Plus, Trash } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { Toggle } from '../../ui/Toggle';

const displayName = (fileName: string) => fileName.replace(/\.disabled$/, '').replace(/\.jar$/i, '');

export function Mods({ id }: { id: string }) {
  const { t, i18n } = useTranslation();
  const listing = useInstances();
  const instance = listing.data?.instances.find((i) => i.id === id);
  const mods = useMods(id);
  const { toggle, add, remove } = useModMutations(id);
  const [removing, setRemoving] = useState<LocalMod | null>(null);
  const [dragging, setDragging] = useState(false);
  const addMods = add.mutate;

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const { type } = event.payload;
        if (type === 'enter' || type === 'over') setDragging(true);
        else if (type === 'leave') setDragging(false);
        else if (type === 'drop') {
          setDragging(false);
          if (event.payload.paths.length > 0) addMods(event.payload.paths);
        }
      })
      .then((stop) => {
        if (alive) unlisten = stop;
        else stop();
      });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [addMods]);

  const pick = async () => {
    const chosen = await open({ multiple: true, title: t('mods.pickTitle'), filters: [{ name: t('mods.jarFiles'), extensions: ['jar'] }] });
    if (Array.isArray(chosen) && chosen.length > 0) addMods(chosen);
  };

  const list = mods.data ?? [];
  return (
    <ScreenFrame
      title={t('mods.title')}
      subtitle={instance ? `${instance.name} · ${t('mods.count', { count: list.length })}` : undefined}
      onBack={() => navigate({ name: 'home', id })}
      actions={
        <>
          <Button icon={<Folder />} onClick={() => void commands.openFolder({ kind: 'mods', id }).catch(pushError)}>
            {t('common.openFolder')}
          </Button>
          <Button variant="primary" icon={<Plus />} onClick={() => void pick()}>
            {t('mods.add')}
          </Button>
        </>
      }
    >
      <div className={`grid max-w-3xl gap-4 ${dragging ? 'outline-2 outline-offset-8 outline-amber outline-dashed' : ''}`}>
        {instance && !instance.game.loader && (
          <p className="panel flex items-start gap-3 p-4 text-ink-dim">
            <InfoBox className="mt-0.5 h-5 w-5 flex-none text-blue-ink" aria-hidden />
            {t('mods.vanillaNote')}
          </p>
        )}
        {dragging && <p className="font-semibold text-amber">{t('mods.dropHere')}</p>}
        {mods.isError && <ErrorCard error={mods.error} />}
        {mods.isSuccess && list.length === 0 && <p className="text-ink-dim">{t('mods.empty')}</p>}
        <ul className="grid gap-2">
          {list.map((mod) => {
            const name = displayName(mod.fileName);
            return (
              <li key={mod.fileName} className={`panel flex items-center gap-4 px-4 py-3 ${mod.enabled ? '' : 'opacity-60'}`}>
                <Package className="h-6 w-6 flex-none text-ink-dim" aria-hidden />
                <div className="min-w-0 flex-1">
                  <p className="truncate font-semibold">{name}</p>
                  <p className="tabular text-sm text-ink-dim">{formatBytes(mod.size, i18n.language)}</p>
                </div>
                <Toggle checked={mod.enabled} label={t('mods.toggle', { name })} onChange={(on) => toggle.mutate({ file: mod.fileName, enabled: on })} />
                <Button variant="ghost" size="icon" aria-label={t('mods.remove', { name })} icon={<Trash />} onClick={() => setRemoving(mod)} />
              </li>
            );
          })}
        </ul>
      </div>
      {removing && (
        <Dialog
          open
          onClose={() => setRemoving(null)}
          title={t('mods.removeTitle', { name: displayName(removing.fileName) })}
          actions={
            <>
              <Button onClick={() => setRemoving(null)}>{t('common.cancel')}</Button>
              <Button variant="danger" onClick={() => remove.mutate(removing.fileName, { onSuccess: () => setRemoving(null) })}>
                {t('mods.removeConfirm')}
              </Button>
            </>
          }
        >
          {t('mods.removeBody')}
        </Dialog>
      )}
    </ScreenFrame>
  );
}
```

- [ ] **Step 3: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): local mods list with toggle, add, drop and remove

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---

### Task 31: Accounts

**Files:**
- Create: `ui/src/features/accounts/offlineName.ts`, `ui/src/features/accounts/Accounts.tsx`
- Test: `ui/src/features/accounts/Accounts.test.tsx`

**Interfaces:**
- Consumes: `useAccounts`, `useAccountMutations`, `useAppInfo`, `Avatar`, `ComingSoon`, `Field`, `Dialog`, `ScreenFrame`.
- Produces: `Accounts()`, and `OFFLINE_NAME = /^[A-Za-z0-9_]{3,16}$/` exported from `offlineName.ts` (kept out of the component file so React Fast Refresh stays happy).

- [ ] **Step 1: Write the failing test**

`ui/src/features/accounts/Accounts.test.tsx`:

```tsx
import { screen, within } from '@testing-library/react';
import { expect, it } from 'vitest';
import { renderApp, sent } from '../../test/backend';
import { Accounts } from './Accounts';

it('switches, adds and removes offline accounts', async () => {
  const { user, calls } = renderApp(<Accounts />);
  const alex = (await screen.findByText('Alex_Builds')).closest('li')!;
  expect(within(screen.getByText('Steve').closest('li')!).getByText('In use')).toBeInTheDocument();
  await user.click(within(alex).getByRole('button', { name: 'Use' }));
  expect(sent(calls, 'account_set_active')).toHaveLength(1);

  const input = screen.getByRole('textbox', { name: 'Player name' });
  await user.type(input, 'no spaces!');
  await user.click(screen.getByRole('button', { name: 'Add offline account' }));
  expect(screen.getByText('Player names must be 3–16 letters, numbers or underscores.')).toBeInTheDocument();
  expect(sent(calls, 'account_add_offline')).toEqual([]);
  await user.clear(input);
  await user.type(input, 'Notch');
  await user.click(screen.getByRole('button', { name: 'Add offline account' }));
  expect(sent(calls, 'account_add_offline')).toEqual([{ name: 'Notch' }]);

  await user.click(within(alex).getByRole('button', { name: 'Remove Alex_Builds' }));
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove' }));
  expect(sent(calls, 'account_remove')).toHaveLength(1);
});

it('keeps Microsoft sign-in as coming soon and locks offline in release builds', async () => {
  renderApp(<Accounts />, { release: true });
  expect(await screen.findByText(/Offline accounts unlock after you sign in once/)).toBeInTheDocument();
  expect(screen.queryByRole('textbox', { name: 'Player name' })).not.toBeInTheDocument();
  expect(screen.getByText('Sign in with Microsoft').closest('[aria-disabled]')).toHaveAttribute('aria-disabled', 'true');
});
```

Run: `pnpm --filter ui test`
Expected: FAIL.

- [ ] **Step 2: Implement**

`ui/src/features/accounts/offlineName.ts`:

```ts
/** Same rule as the core's validate_offline_name: 3–16 ASCII letters, digits or underscores. */
export const OFFLINE_NAME = /^[A-Za-z0-9_]{3,16}$/;
```

`ui/src/features/accounts/Accounts.tsx`:

```tsx
import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Avatar } from '../../art/Avatar';
import type { Account } from '../../ipc/schema';
import { useAccountMutations, useAccounts, useAppInfo } from '../../state/queries';
import { Button } from '../../ui/Button';
import { ComingSoon } from '../../ui/ComingSoon';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Check, Trash, User } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { OFFLINE_NAME } from './offlineName';

export function Accounts() {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const appInfo = useAppInfo();
  const { setActive, remove } = useAccountMutations();
  // Adding shows its error inline under the form instead of as a toast.
  const add = useAccountMutations({ inline: true }).addOffline;
  const [name, setName] = useState('');
  const [invalid, setInvalid] = useState(false);
  const [removing, setRemoving] = useState<Account | null>(null);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!OFFLINE_NAME.test(name)) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    add.mutate(name, { onSuccess: () => setName('') });
  };

  const list = accounts.data?.accounts ?? [];
  return (
    <ScreenFrame title={t('accounts.title')}>
      <div className="grid max-w-2xl gap-10">
        <ul className="grid gap-2">
          {list.length === 0 && accounts.isSuccess && <li className="text-ink-dim">{t('accounts.empty')}</li>}
          {list.map((account) => {
            const inUse = account.uuid === accounts.data?.active;
            return (
              <li key={account.uuid} className="panel flex items-center gap-4 px-4 py-3">
                <Avatar uuid={account.uuid} size={40} />
                <div className="min-w-0 flex-1">
                  <p className="truncate font-bold">{account.username}</p>
                  <span className="badge mt-1">{t('accounts.offlineBadge')}</span>
                </div>
                {inUse ? (
                  <span className="flex items-center gap-2 font-semibold text-lime">
                    <Check className="h-5 w-5" aria-hidden />
                    {t('accounts.inUse')}
                  </span>
                ) : (
                  <Button onClick={() => setActive.mutate(account.uuid)}>{t('accounts.use')}</Button>
                )}
                <Button variant="ghost" size="icon" aria-label={t('accounts.removeLabel', { name: account.username })} icon={<Trash />} onClick={() => setRemoving(account)} />
              </li>
            );
          })}
        </ul>

        <section className="grid gap-4" aria-labelledby="add-heading">
          <h2 id="add-heading" className="text-lg font-bold">
            {t('accounts.addOffline')}
          </h2>
          {appInfo.data?.offlineAllowed ? (
            <form onSubmit={submit} className="flex items-start gap-3">
              <div className="flex-1">
                <Field
                  label={t('accounts.offlineName')}
                  hint={t('accounts.offlineHint')}
                  error={invalid ? t('errors.offline_name_invalid.message') : null}
                >
                  <input className="field-input" value={name} maxLength={16} autoComplete="off" onChange={(e) => setName(e.target.value)} />
                </Field>
              </div>
              <Button type="submit" variant="primary" className="mt-7" disabled={add.isPending}>
                {t('accounts.addOffline')}
              </Button>
            </form>
          ) : (
            appInfo.isSuccess && <p className="text-ink-dim">{t('accounts.offlineLocked')}</p>
          )}
          {add.isError && <ErrorCard error={add.error} />}
          <ComingSoon label={t('accounts.microsoft')} icon={<User className="h-5 w-5" />} />
        </section>
      </div>
      {removing && (
        <Dialog
          open
          onClose={() => setRemoving(null)}
          title={t('accounts.removeTitle', { name: removing.username })}
          actions={
            <>
              <Button onClick={() => setRemoving(null)}>{t('common.cancel')}</Button>
              <Button variant="danger" onClick={() => remove.mutate(removing.uuid, { onSuccess: () => setRemoving(null) })}>
                {t('accounts.remove')}
              </Button>
            </>
          }
        >
          {t('accounts.removeBody')}
        </Dialog>
      )}
    </ScreenFrame>
  );
}
```

- [ ] **Step 3: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): accounts with offline add, switch, remove and Microsoft coming soon

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---

### Task 32: Settings and About

**Files:**
- Create: `ui/src/features/settings/Settings.tsx`
- Test: `ui/src/features/settings/Settings.test.tsx`

**Interfaces:**
- Consumes: `useSettings`, `useSaveSettings`, `useAppInfo`, `detectLang`, `i18n`, `MemorySlider`, `Toggle`, `Select`, `openUrl` from `@tauri-apps/plugin-opener`, `commands.openFolder`.
- Produces: `Settings()`, and `applyLanguage(lang: Lang | null)`, exported from `ui/src/i18n/index.ts`.

- [ ] **Step 1: Write the failing test**

`ui/src/features/settings/Settings.test.tsx`:

```tsx
import { screen } from '@testing-library/react';
import { expect, it } from 'vitest';
import { renderApp, sent } from '../../test/backend';
import { Settings } from './Settings';

it('switches language, saves preferences and opens the data folder', async () => {
  const { user, calls } = renderApp(<Settings />);
  const language = await screen.findByRole('combobox', { name: 'Language' });
  await user.selectOptions(language, 'es');
  expect(await screen.findByRole('heading', { name: 'Configuración' })).toBeInTheDocument();
  expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { language: 'es' } });

  await user.click(screen.getByRole('switch', { name: 'Minimizar mientras juegas' }));
  expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { minimizeOnPlay: false } });

  await user.click(screen.getByRole('button', { name: 'Abrir carpeta' }));
  expect(sent(calls, 'open_folder')).toEqual([{ target: { kind: 'data' } }]);
  expect(screen.getByText(/No es un producto oficial de Minecraft/)).toBeInTheDocument();
});
```

Run: `pnpm --filter ui test`
Expected: FAIL.

- [ ] **Step 2: Implement**

Add to `ui/src/i18n/index.ts`:

```ts
/** Use the player's choice, or the computer's language when they haven't picked one. */
export function applyLanguage(lang: Lang | null) {
  const target = lang ?? detectLang(typeof navigator === 'undefined' ? undefined : navigator.language);
  if (i18n.language !== target) void i18n.changeLanguage(target);
}
```

`ui/src/features/settings/Settings.tsx`:

```tsx
import { openUrl } from '@tauri-apps/plugin-opener';
import { useTranslation } from 'react-i18next';
import mark from '../../assets/mark.svg';
import { applyLanguage } from '../../i18n';
import { commands } from '../../ipc/commands';
import type { Lang, Settings as SettingsData } from '../../ipc/schema';
import { useAppInfo, useSaveSettings, useSettings } from '../../state/queries';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { ExternalLink, Folder } from '../../ui/Icon';
import { MemorySlider } from '../../ui/MemorySlider';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { Select } from '../../ui/Select';
import { Toggle } from '../../ui/Toggle';

const SOURCE = 'https://github.com/PerlandDev/perland-launcher';
const LICENSE = 'https://www.gnu.org/licenses/gpl-3.0.html';

export function Settings() {
  const { t } = useTranslation();
  const settings = useSettings();
  const appInfo = useAppInfo();
  const save = useSaveSettings();
  const data = settings.data;
  if (!data || !appInfo.data) return <ScreenFrame title={t('settings.title')}>{null}</ScreenFrame>;
  const change = (patch: Partial<SettingsData>) => save.mutate({ ...data, ...patch });

  return (
    <ScreenFrame title={t('settings.title')}>
      <div className="grid max-w-2xl gap-10">
        <Select
          label={t('settings.language')}
          value={data.language ?? 'system'}
          options={[
            { value: 'system', label: t('settings.languageSystem') },
            { value: 'en', label: t('settings.english') },
            { value: 'es', label: t('settings.spanish') },
          ]}
          onChange={(value) => {
            const language = value === 'system' ? null : (value as Lang);
            applyLanguage(language);
            change({ language });
          }}
        />
        <MemorySlider
          label={t('settings.memory')}
          valueMb={data.defaultMemoryMb}
          systemMb={appInfo.data.systemMemoryMb}
          onChange={(defaultMemoryMb) => change({ defaultMemoryMb })}
        />
        <div className="flex items-start justify-between gap-6">
          <div>
            <p className="font-semibold" aria-hidden>
              {t('settings.minimize')}
            </p>
            <p className="text-sm text-ink-dim">{t('settings.minimizeHint')}</p>
          </div>
          <Toggle checked={data.minimizeOnPlay} label={t('settings.minimize')} onChange={(minimizeOnPlay) => change({ minimizeOnPlay })} />
        </div>
        <div className="grid gap-2">
          <p className="text-sm font-semibold">{t('settings.dataFolder')}</p>
          <div className="flex items-center gap-3">
            <p className="selectable min-w-0 flex-1 truncate font-mono text-sm text-ink-dim">{appInfo.data.dataDir}</p>
            <Button icon={<Folder />} onClick={() => void commands.openFolder({ kind: 'data' }).catch(pushError)}>
              {t('common.openFolder')}
            </Button>
          </div>
        </div>
        <section className="panel grid gap-4 p-6" aria-labelledby="about-heading">
          <div className="flex items-center gap-4">
            <img src={mark} alt="" width={56} height={48} />
            <div>
              <h2 id="about-heading" className="font-display text-xl">{t('app.name')}</h2>
              <p className="text-sm text-ink-dim">
                {t('settings.version', { version: appInfo.data.version })}
                {appInfo.data.build === 'dev' && ` · ${t('settings.devBuild')}`}
              </p>
            </div>
          </div>
          <p>{t('settings.license')}</p>
          <div className="flex flex-wrap gap-3">
            <Button icon={<ExternalLink />} onClick={() => void openUrl(LICENSE).catch(pushError)}>
              {t('settings.licenseLink')}
            </Button>
            <Button icon={<ExternalLink />} onClick={() => void openUrl(SOURCE).catch(pushError)}>
              {t('settings.source')}
            </Button>
          </div>
          <p className="text-sm text-ink-dim">{t('settings.notOfficial')}</p>
        </section>
      </div>
    </ScreenFrame>
  );
}
```

- [ ] **Step 3: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): settings with language, memory, minimize, data folder and About

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---

### Task 33: First run

**Files:**
- Create: `ui/src/features/first-run/FirstRun.tsx`
- Test: `ui/src/features/first-run/FirstRun.test.tsx`

**Interfaces:**
- Consumes: `TitleBar`, `Marquee`, `useAppInfo`, `useSettings`, `useSaveSettings`, `useAccountMutations`, `useCreateInstance`, `useVersionChoice`, `VersionFields`, `applyLanguage`, `detectLang`, `ComingSoon`, `navigate`.
- Produces: `FirstRun()`, a 4-step flow: welcome, then language, then account, then first instance. Finishing creates a vanilla instance, saves `firstRunDone: true`, and navigates home.

- [ ] **Step 1: Write the failing test**

`ui/src/features/first-run/FirstRun.test.tsx`:

```tsx
import { screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { routeSnapshot } from '../../state/route';
import { renderApp, sent } from '../../test/backend';
import { FirstRun } from './FirstRun';

it('walks a new player from welcome to their first instance', async () => {
  const { user, calls } = renderApp(<FirstRun />, { firstRun: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'Español' }));
  expect(await screen.findByRole('heading', { name: '¿Quién va a jugar?' })).toBeInTheDocument();
  await user.type(screen.getByRole('textbox', { name: 'Nombre de jugador' }), 'Steve');
  await user.click(screen.getByRole('button', { name: 'Continuar' }));
  expect(sent(calls, 'account_add_offline')).toEqual([{ name: 'Steve' }]);
  expect(await screen.findByRole('heading', { name: 'Tu primera instancia' })).toBeInTheDocument();
  const finish = await screen.findByRole('button', { name: 'A jugar' });
  await vi.waitFor(() => expect(finish).toBeEnabled()); // the version list has loaded
  await user.click(finish);
  await vi.waitFor(() => expect(routeSnapshot()).toMatchObject({ name: 'home', id: 'minecraft-26-3' }));
  expect(sent(calls, 'instance_create')).toEqual([{ request: { name: 'Minecraft 26.3', minecraft: '26.3', loader: null } }]);
  expect(sent(calls, 'settings_set').at(-1)).toMatchObject({ settings: { firstRunDone: true, language: 'es' } });
});

it('lets release builds skip the account step honestly', async () => {
  const { user } = renderApp(<FirstRun />, { firstRun: true, release: true });
  await user.click(await screen.findByRole('button', { name: "Let's go" }));
  await user.click(screen.getByRole('button', { name: 'English' }));
  expect(await screen.findByText(/Microsoft sign-in is coming soon/)).toBeInTheDocument();
  expect(screen.getByText('Sign in with Microsoft').closest('[aria-disabled]')).toHaveAttribute('aria-disabled', 'true');
  await user.click(screen.getByRole('button', { name: 'Skip for now' }));
  expect(await screen.findByRole('heading', { name: 'Your first instance' })).toBeInTheDocument();
});
```

Run: `pnpm --filter ui test`
Expected: FAIL.

- [ ] **Step 2: Implement**

`ui/src/features/first-run/FirstRun.tsx`:

```tsx
import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Marquee } from '../../art/Marquee';
import mark from '../../assets/mark.svg';
import { applyLanguage } from '../../i18n';
import type { Lang } from '../../ipc/schema';
import { useAccountMutations, useAppInfo, useCreateInstance, useSaveSettings, useSettings } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ComingSoon } from '../../ui/ComingSoon';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { User } from '../../ui/Icon';
import { OFFLINE_NAME } from '../accounts/offlineName';
import { TitleBar } from '../titlebar/TitleBar';
import { useVersionChoice } from '../new-instance/useVersionChoice';
import { VersionFields } from '../new-instance/VersionFields';

const STEPS = 4;

function Steps({ current }: { current: number }) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-2">
      <span className="sr-only">{t('firstRun.step', { n: current + 1, total: STEPS })}</span>
      {Array.from({ length: STEPS }, (_, i) => (
        <span key={i} aria-hidden className={`h-2 w-6 ${i <= current ? 'bg-amber' : 'bg-line'}`} />
      ))}
    </div>
  );
}

export function FirstRun() {
  const { t, i18n } = useTranslation();
  const appInfo = useAppInfo();
  const settings = useSettings();
  const saveSettings = useSaveSettings({ inline: true });
  const { addOffline } = useAccountMutations({ inline: true });
  const create = useCreateInstance({ inline: true });
  const choice = useVersionChoice();
  const [step, setStep] = useState(0);
  const [lang, setLang] = useState<Lang>(i18n.language === 'es' ? 'es' : 'en');
  const [name, setName] = useState('');
  const [invalid, setInvalid] = useState(false);
  const [picking, setPicking] = useState(false);

  const chooseLang = (next: Lang) => {
    setLang(next);
    applyLanguage(next);
    setStep(2);
  };
  const submitAccount = (event: FormEvent) => {
    event.preventDefault();
    if (!OFFLINE_NAME.test(name)) {
      setInvalid(true);
      return;
    }
    addOffline.mutate(name, { onSuccess: () => setStep(3) });
  };
  const finish = () => {
    const { minecraft } = choice.resolved;
    if (!minecraft || !settings.data) return;
    create.mutate(
      { name: t('newInstance.namePlaceholder', { version: minecraft }), minecraft, loader: null },
      {
        onSuccess: (instance) =>
          saveSettings.mutate(
            { ...settings.data, firstRunDone: true, language: lang },
            { onSuccess: () => navigate({ name: 'home', id: instance.id }) },
          ),
      },
    );
  };

  const latest = choice.list.find((v) => v.kind === 'release')?.id;
  const failure = addOffline.error ?? create.error ?? saveSettings.error;

  return (
    <div className="grid h-full grid-rows-[2.25rem_1fr]">
      <TitleBar />
      <main className="relative min-h-0 overflow-hidden">
        <Marquee seed="perland" attract className="absolute inset-0 h-full w-full" />
        <div className="relative grid h-full place-items-center p-10">
          <div className="panel grid w-full max-w-lg gap-6 p-8 shadow-[0_8px_0_var(--color-dirt-deep)]">
            {step === 0 && (
              <>
                <img src={mark} alt="" width={112} height={96} />
                <h1 className="font-display text-4xl leading-none">{t('firstRun.welcomeTitle')}</h1>
                <p className="text-ink-dim">{t('firstRun.welcomeBody')}</p>
                <Button variant="primary" size="lg" onClick={() => setStep(1)}>
                  {t('firstRun.start')}
                </Button>
              </>
            )}
            {step === 1 && (
              <>
                <h1 className="font-display text-3xl leading-none">{t('firstRun.languageTitle')}</h1>
                <div className="grid gap-3 sm:grid-cols-2">
                  <Button size="lg" variant={lang === 'en' ? 'primary' : 'secondary'} onClick={() => chooseLang('en')}>
                    English
                  </Button>
                  <Button size="lg" variant={lang === 'es' ? 'primary' : 'secondary'} onClick={() => chooseLang('es')}>
                    Español
                  </Button>
                </div>
              </>
            )}
            {step === 2 && (
              <>
                <h1 className="font-display text-3xl leading-none">{t('firstRun.accountTitle')}</h1>
                {appInfo.data?.offlineAllowed ? (
                  <form onSubmit={submitAccount} className="grid gap-4">
                    <p className="text-ink-dim">{t('firstRun.accountBody')}</p>
                    <Field
                      label={t('accounts.offlineName')}
                      hint={t('accounts.offlineHint')}
                      error={invalid ? t('errors.offline_name_invalid.message') : null}
                    >
                      <input className="field-input" value={name} maxLength={16} autoComplete="off" onChange={(e) => setName(e.target.value)} />
                    </Field>
                    <Button type="submit" variant="primary" size="lg" disabled={addOffline.isPending}>
                      {t('common.continue')}
                    </Button>
                  </form>
                ) : (
                  <>
                    <p className="text-ink-dim">{t('firstRun.accountLockedBody')}</p>
                    <ComingSoon label={t('accounts.microsoft')} icon={<User className="h-5 w-5" />} />
                    <Button size="lg" onClick={() => setStep(3)}>
                      {t('firstRun.skip')}
                    </Button>
                  </>
                )}
              </>
            )}
            {step === 3 && (
              <>
                <h1 className="font-display text-3xl leading-none">{t('firstRun.instanceTitle')}</h1>
                <p className="text-ink-dim">{t('firstRun.instanceBody')}</p>
                {picking ? (
                  <VersionFields choice={choice} withLoader={false} />
                ) : (
                  <div className="grid gap-2">
                    <p className="font-semibold">{t('firstRun.latestRelease', { version: latest ?? '…' })}</p>
                    <button type="button" className="justify-self-start text-sm text-blue-ink underline underline-offset-4" onClick={() => setPicking(true)}>
                      {t('firstRun.chooseVersion')}
                    </button>
                  </div>
                )}
                <Button variant="primary" size="lg" disabled={!choice.resolved.ready || create.isPending || saveSettings.isPending} onClick={finish}>
                  {t('firstRun.finish')}
                </Button>
              </>
            )}
            {failure && <ErrorCard error={failure} />}
            {step > 0 && <Steps current={step} />}
          </div>
        </div>
      </main>
    </div>
  );
}
```

- [ ] **Step 3: Run, typecheck, lint, commit**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
git add ui
git commit -m "feat(ui): first-run flow from welcome to the first instance

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: PASS.

---

### Task 34: Assemble the app: shell, routing, events, close guard and bootstrap

**Files:**
- Create: `ui/src/features/close-guard/CloseGuard.tsx`
- Create: `ui/src/state/handleEvent.ts`, `ui/src/state/handleEvent.test.ts`
- Modify: `ui/src/App.tsx`, `ui/src/App.test.tsx`, `ui/src/main.tsx`

**Interfaces:**
- Consumes: every screen, `onAppEvent`, `onCloseRequested`, `runStore`, `pushError`, `openActivity`, `isCrash`, `queryClient`, `applyLanguage`.
- Produces:
  - `handleEvent(event)`, which:
    - updates the run store
    - turns warnings into toasts
    - opens the activity drawer on a crash or a failed start
  - `App()`, which shows the first-run flow or the shell
  - `CloseGuard()`
  - In `main.tsx`: mock install (mock mode only), the event subscription, hydration, and render.

- [ ] **Step 1: Write the failing tests**

`ui/src/state/handleEvent.test.ts`:

```ts
import { expect, it } from 'vitest';
import { resetActivity, useActivity } from './activity';
import { handleEvent } from './handleEvent';
import { runStore } from './runStore';
import { clearToasts, toastsSnapshot } from './toasts';
import { renderHook } from '@testing-library/react';

it('turns warnings into toasts and opens the drawer on a crash', () => {
  clearToasts();
  resetActivity();
  handleEvent({
    type: 'warning',
    instance: 'pack',
    error: { category: 'disk', code: 'log_write_stopped', message: 'x', fix: null, detail: null, params: {} },
  });
  expect(toastsSnapshot()).toHaveLength(1);
  handleEvent({
    type: 'run_state',
    instance: 'pack',
    state: 'idle',
    exit: { code: 1, killed: false, cancelled: false, crash: null, error: null },
  });
  const { result } = renderHook(() => useActivity());
  expect(result.current).toEqual({ open: true, focus: 'pack' });
  expect(runStore.run('pack').lastExit?.code).toBe(1);
});

it('leaves the drawer alone for a requested stop', () => {
  resetActivity();
  handleEvent({
    type: 'run_state',
    instance: 'pack',
    state: 'idle',
    exit: { code: null, killed: true, cancelled: false, crash: null, error: null },
  });
  const { result } = renderHook(() => useActivity());
  expect(result.current.open).toBe(false);
});
```

Replace `ui/src/App.test.tsx`:

```tsx
import { emit } from '@tauri-apps/api/event';
import { act, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { CLOSE_REQUESTED } from './ipc/events';
import { renderApp, sent } from './test/backend';
import App from './App';

describe('App', () => {
  it('starts with the first-run flow on a fresh install', async () => {
    renderApp(<App />, { firstRun: true });
    expect(await screen.findByRole('heading', { name: 'Welcome to Perland' })).toBeInTheDocument();
  });

  it('shows the rail and the selected instance otherwise', async () => {
    renderApp(<App />);
    expect(await screen.findByRole('navigation', { name: 'Instances' })).toBeInTheDocument();
    expect(await screen.findByRole('heading', { name: 'Asalith Fields' })).toBeInTheDocument();
  });

  it('asks before quitting while a game runs', async () => {
    const { user, calls } = renderApp(<App />);
    await screen.findByRole('heading', { name: 'Asalith Fields' });
    await act(() => emit(CLOSE_REQUESTED));
    const dialog = await screen.findByRole('dialog', { name: 'A game is still running' });
    expect(dialog).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Quit and stop games' }));
    expect(sent(calls, 'quit')).toEqual([{ stopGames: true }]);
  });
});
```

Run: `pnpm --filter ui test`
Expected: FAIL.

- [ ] **Step 2: Implement**

`ui/src/state/handleEvent.ts`:

```ts
import type { AppEvent } from '../ipc/schema';
import { openActivity } from './activity';
import { isCrash } from './exit';
import { runStore } from './runStore';
import { pushError } from './toasts';

/** Everything the UI does with an event from Rust. */
export function handleEvent(event: AppEvent) {
  runStore.apply(event);
  if (event.type === 'warning') pushError(event.error);
  if (event.type === 'run_state' && event.state === 'idle' && event.exit && (event.exit.error || isCrash(event.exit))) {
    openActivity(event.instance);
  }
}
```

`ui/src/features/close-guard/CloseGuard.tsx`:

```tsx
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { onCloseRequested } from '../../ipc/events';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';

/** Closing the window while a game runs asks first; minimizing is the safe default. */
export function CloseGuard() {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void onCloseRequested(() => setOpen(true)).then((stop) => {
      if (alive) unlisten = stop;
      else stop();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  return (
    <Dialog
      open={open}
      onClose={() => setOpen(false)}
      title={t('closeGuard.title')}
      actions={
        <>
          <Button
            variant="primary"
            onClick={() => {
              setOpen(false);
              void getCurrentWindow().minimize();
            }}
          >
            {t('closeGuard.minimize')}
          </Button>
          <Button variant="danger" onClick={() => void commands.quit(true).catch(pushError)}>
            {t('closeGuard.quit')}
          </Button>
        </>
      }
    >
      {t('closeGuard.body')}
    </Dialog>
  );
}
```

`ui/src/App.tsx`:

```tsx
import { useEffect } from 'react';
import { ActivityDrawer } from './features/activity/ActivityDrawer';
import { Accounts } from './features/accounts/Accounts';
import { CloseGuard } from './features/close-guard/CloseGuard';
import { FirstRun } from './features/first-run/FirstRun';
import { Home } from './features/home/Home';
import { InstanceSettings } from './features/instance-settings/InstanceSettings';
import { Mods } from './features/mods/Mods';
import { NewInstance } from './features/new-instance/NewInstance';
import { Rail } from './features/rail/Rail';
import { Settings } from './features/settings/Settings';
import { TitleBar } from './features/titlebar/TitleBar';
import { applyLanguage } from './i18n';
import { useSettings } from './state/queries';
import { useRoute, type Route } from './state/route';
import { Toasts } from './ui/Toasts';

function Screen({ route }: { route: Route }) {
  switch (route.name) {
    case 'home':
      return <Home id={route.id} />;
    case 'mods':
      return <Mods id={route.id} />;
    case 'instanceSettings':
      return <InstanceSettings id={route.id} />;
    case 'newInstance':
      return <NewInstance />;
    case 'accounts':
      return <Accounts />;
    case 'settings':
      return <Settings />;
  }
}

function Shell() {
  const route = useRoute();
  return (
    <div className="grid h-full grid-rows-[2.25rem_1fr]">
      <TitleBar />
      <div className="grid min-h-0 grid-cols-[4.5rem_1fr]">
        <Rail />
        <main className="relative min-h-0 overflow-hidden pb-9">
          <Screen route={route} />
          <ActivityDrawer />
        </main>
      </div>
    </div>
  );
}

export default function App() {
  const settings = useSettings();
  const language = settings.data?.language ?? null;
  useEffect(() => applyLanguage(language), [language]);

  if (settings.isPending) return <div className="h-full bg-ground" aria-busy="true" />;
  return (
    <>
      {settings.data && !settings.data.firstRunDone ? <FirstRun /> : <Shell />}
      <Toasts />
      <CloseGuard />
    </>
  );
}
```

`ui/src/main.tsx`:

```tsx
import { QueryClientProvider } from '@tanstack/react-query';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import './i18n';
import { commands } from './ipc/commands';
import { onAppEvent } from './ipc/events';
import { handleEvent } from './state/handleEvent';
import { queryClient } from './state/queries';
import { runStore } from './state/runStore';
import './styles/app.css';

async function bootstrap() {
  if (import.meta.env.MODE === 'mock') {
    const { installMock } = await import('./ipc/mock');
    const params = new URLSearchParams(window.location.search);
    installMock({ firstRun: params.has('firstRun'), crash: params.has('crash'), release: params.has('release') });
  }
  await onAppEvent(handleEvent);
  // After a reload, pick up games that are still running and their recent output.
  const snapshots = await commands.runStates();
  const tails = Object.fromEntries(
    await Promise.all(snapshots.map(async (s) => [s.instance, await commands.logTail(s.instance)] as const)),
  );
  runStore.hydrate(snapshots, tails);

  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </StrictMode>,
  );
}

void bootstrap();
```

The drawer handle takes 2.25 rem along the bottom of `main` (the `pb-9`); Home's bottom row already leaves room for it (`pb-12`).

- [ ] **Step 3: Run every UI check and look at it in the browser**

```bash
pnpm --filter ui test
pnpm --filter ui typecheck
pnpm --filter ui lint
pnpm --filter ui build
```

Expected: everything passes.

Then start mock mode with `pnpm --filter ui dev:mock` and open `http://localhost:1420/` in the browser pane. Check each of these:
- the rail with three entries
- the Asalith Fields cabinet
- Play → progress in the button → Playing with a log in Activity → Stop
- `?crash` → the crash card opens by itself
- `?firstRun` → the welcome flow
- `?release` → locked offline accounts

- [ ] **Step 4: Commit**

```bash
git add ui
git commit -m "feat(ui): assemble the app shell with routing, event handling and the close guard

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---
## Phase 6: Finish

### Task 35: Impeccable finish: inspect, review, document (controller task)

The **controller** (the main session) runs this task. It is not handed to an implementer subagent, because it drives the browser pane and spawns impeccable's own reviewer and documenter agents. The direction contract's FINISH line is binding: *unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.*

**Files:**
- Create: `.claude/launch.json`
- Create: `.impeccable/review/*.png` (gitignored evidence)
- Create: `DESIGN.md`, `.impeccable/design.json` (written by the documenter agent)
- Modify: whatever the review's fix batch touches under `ui/src/`

- [ ] **Step 1: Serve mock mode in the browser pane**

Create `.claude/launch.json`:

```json
{
  "version": "0.0.1",
  "configurations": [
    {
      "name": "ui-mock",
      "runtimeExecutable": "pnpm",
      "runtimeArgs": ["--filter", "ui", "dev:mock"],
      "port": 1420
    }
  ]
}
```

Start it with `preview_start` (`name: "ui-mock"`).

- [ ] **Step 2: One batched capture round**

At **1200×760** (the default window) and **960×600** (the minimum), capture these into `.impeccable/review/`. Settle motion first: wait for the progress script to reach the state being captured.
- `home.png` and `home-960.png`: the Asalith Fields cabinet at idle
- `preparing.png`: after Play, while the meter fills
- `running.png`: running, with the Activity drawer open showing the log
- `crash.png`: `/?crash`, after the crash card opens
- `new-instance.png`, `instance-settings.png`, `mods.png`, `accounts.png`, `settings.png`
- `first-run.png`: `/?firstRun`, the welcome step, plus `first-run-account.png`
- `release-accounts.png`: `/?release`, the Accounts screen
- `es-home.png`: Spanish, picked from Settings, to check the 30 % longer strings

Open every file once and confirm it shows what its name says.

- [ ] **Step 3: Detector and one fix batch**

Run: `C:/Users/mmxtu/.claude/skills/impeccable/scripts/impeccable detect --json ui/src`

Batch-fix what the captures and detector show: mechanical findings, overflow at 960 px, truncated Spanish, contrast, and missing states. Then do **one** confirming capture round over the same files. Two rounds is the ceiling.

- [ ] **Step 4: Finish review**

Spawn `impeccable-finish-reviewer` fresh, with no forked history. Give it this packet:
- the original request
- the confirmed answers: game-forward pinned, Arcade Cabinet chosen, dark only, en/es, no Mojang assets
- `ui/src/App.tsx` as the artifact path, and the screenshot paths above
- the direction contract (`.impeccable/surfaces/ui-src-app-tsx.md`)
- the remaining detector findings
- the craft-floor path
- "code-led build; no approved comp"

Act on its disposition word (`recapture`, `rebuild`, `fix` or `ship`) exactly as `new-work.md` §7 describes. A `fix` gets one batch, a recapture, and a verdict pass from the same reviewer.

- [ ] **Step 5: Raster provenance**

Run: `C:/Users/mmxtu/.claude/skills/impeccable/scripts/impeccable embed-prompt --scan src-tauri/icons ui/src/assets`

For every flagged file, embed its origin: "Generated by `tauri icon` from ui/src/assets/mark.svg, a vector redraw of the legacy PerlandDev logo (E:\PerlandDev-site\public\logo.png), 2026-10-10."

- [ ] **Step 6: Document the system**

Spawn `impeccable-documenter` with:
- the project root
- `ui/src/App.tsx` as the artifact
- the direction contract
- `PRODUCT.md`
- `C:\Users\mmxtu\.claude\skills\impeccable\reference\document.md`
- the write boundary `DESIGN.md`, `.impeccable/design.json`

Check that both files exist and carry tokens (colors, type, the step and notch rules, the five control states, motion).

- [ ] **Step 7: Run the full gate and commit**

```bash
pnpm --filter ui build
pnpm -r lint && pnpm -r typecheck && pnpm -r test
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
git add DESIGN.md .impeccable/design.json .claude/launch.json ui src-tauri/icons
git commit -m "design: Arcade Cabinet finish review fixes and DESIGN.md

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 36: Real-app smoke test, final verification and the pull request

**Files:**
- No new files. The smoke test only produces notes for the PR description.

- [ ] **Step 1: Fresh full gate**

```bash
pnpm install --frozen-lockfile
pnpm --filter ui build
pnpm -r lint && pnpm -r typecheck && pnpm -r test
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: everything passes. Record the test counts for the PR.

- [ ] **Step 2: Real-app smoke test on Windows (owner-assisted)**

Run in PowerShell:

```powershell
$env:PERLAND_LAUNCHER_ROOT = "$PWD\target\pl-dev"; pnpm tauri dev
```

Go through this checklist. The owner drives it, or the controller does with computer-use if the owner grants access to the app window.

1. **First run:** Welcome → English → offline name → newest Minecraft → "Start playing" lands on Home.
2. **Window:** the title bar drags the window; minimize, maximize/restore and close work; Win+arrow snapping works.
3. **New instance:** Fabric 1.20.1 with the default stable loader. Play shows install progress in the button. The game window opens, and the launcher minimizes.
4. **Running:** the Activity drawer shows the live game log. Stop kills the game, and the launcher comes back.
5. **Close guard:** while the game runs, closing the launcher shows the dialog. Minimize keeps the game; "Quit and stop games" stops it and exits.
6. **Crash card:** set the instance's memory to 512 MB with several mods and press Play. Expected: the crash card ("ran out of memory", or the exit code) with "Raise to … GB" and "Copy for support". The copied text contains no access token or home path.
7. **Mods:** add a jar by drag-drop and with "Add mods", toggle it, remove it.
8. **Spanish:** Settings → Español. Every screen reads in Spanish and no button label is cut off.

Fix anything that fails, as a TDD task with its own commit, and rerun Step 1.

- [ ] **Step 3: Push and open the PR**

```bash
git push -u origin HEAD
gh pr create --base main --title "Desktop app: Tauri shell, React UI (Arcade Cabinet), en/es (Plan 5)" --body-file target/pr-body.md
```

Write the PR body to `target/pr-body.md` first. It covers:
- what shipped, screen by screen
- what's intentionally "coming soon" or absent
- the `launcher-app` crate deviation and why
- the error-code and contract-fixture mechanisms
- the smoke-test checklist with results
- the test counts
- it ends with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`

After opening the PR, bind it with the `ccd_pr` tools, read its CI, and offer Auto-fix if checks fail.
