# Perland Launcher: Desktop App (Plan 5 pulled forward), Design

- **Date:** 2026-10-10
- **Status:** Draft, awaiting owner review
- **Parent spec:** `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (§1.3 decisions, §5 UI)
- **Builds on:** Plan 1, `launcher-core` and `perland-cli`, merged to `main` (224726d)
- **Product context:** `PRODUCT.md`. **Visual direction contract:** `.impeccable/surfaces/ui-src-app-tsx.md`. `DESIGN.md` is written at the finish, from the built UI, by the impeccable documenter.

## 1. Intent

Give Perland players a desktop app over `launcher-core`: open it, press Play, be in the game. This plan comes before Plans 2–4 by the owner's choice, so it covers only what the engine supports today:
- vanilla, Fabric and Quilt instances
- offline/dev accounts
- Play with progress, logs, Stop and crash reporting
- a local mods list
- settings and a first-run flow
- English and Spanish

Features from Plans 2–4 and 6 are either visibly "coming soon" or absent. They are never faked.

### 1.1 Success criteria

- In a dev build, a new player goes from first launch to a running vanilla game in one guided flow: language → offline name → "Latest release" → Play.
- A Fabric 1.20.1 instance installs and launches from the app on Windows. Stop kills it. A forced crash shows the crash card with "Copy for support".
- Every player-facing string, including every core error, renders in English and Spanish. CI fails when a translation is missing.
- Cancelling during download, or closing the window during play, never leaves an instance unlaunchable or a game orphaned without the player choosing it.
- `launcher-core` still has no Tauri or UI dependency.

### 1.2 Decisions made during brainstorming

| Topic | Decision |
|---|---|
| Scope | Section 2 below. Browse, `perland://pack`, Forge/NeoForge, update banners, the updater and diagnostics export are absent. Microsoft sign-in and the pack link and `.mrpack` tiles show as disabled "coming soon". |
| Error language | Core errors get a stable `ErrorCode` plus `params`. The UI translates by code and falls back to the English message. |
| Mods screen | A real local mods list (the jar files in `mods/`): toggle, add, remove, open folder. |
| Concurrency | Several instances may run at once, each instance once. Installs are serialized through a single queue. |
| Window | Frameless, with a custom title bar. Windows 11 snap-layout hover is lost; Win+Z and drag-snap still work. |
| Rust↔TS contract | Hand-written TS types, plus JSON contract fixtures emitted by a Rust test and checked in TS. No codegen dependency. |
| Launch orchestration | Moves from the CLI into `launcher_core::play`, which the CLI and the app share. |
| Visual world | **Arcade Cabinet**, inside the pinned game-forward layout. See §7 and the direction contract. |
| Brand mark | The legacy PerlandDev pixel land plots, redrawn as a clean vector. |
| Offline gate | Kept. Release builds show "Sign in with Microsoft · coming soon" and can't add offline accounts. Dev builds can. |

## 2. Screens

| Screen | Contents |
|---|---|
| **First run** | Welcome → language (from the OS locale, en/es) → account → first instance. Account: dev builds take an offline name; release builds show the disabled Microsoft button and explain the gate. First instance: "Latest release" vanilla by default, or pick a version and loader. |
| **Home / instance** | The rail of generated instance icons sits on the left, with + at the bottom. The selected instance fills the view: generated marquee art, name, version and loader, and a big **Play** button. Play's states are idle → queued → preparing (segmented progress; click to cancel) → running (becomes **Stop**) → idle. Next to it: **Mods** and **⋯** (rename, instance settings, open folder, delete). The status pill slot is reserved for sub-project 2 and renders nothing. |
| **Instance → Mods** | The jar files in `mods/` with an on/off toggle, add (file picker or drag-and-drop), remove and open folder. Vanilla instances show "Mods need Fabric or Quilt". |
| **Instance settings** | Memory (validated; soft warning against system RAM), Java (automatic, or a custom path with a "Test" button that runs `java -version`), rename, and Minecraft version and loader. Changes apply on the next Play. |
| **New instance** | Name, Minecraft version (releases, with a snapshots toggle), and loader (none, Fabric or Quilt, defaulting to latest stable). Disabled "Paste a pack link" and "Import .mrpack" tiles say "coming soon". |
| **Accounts** | A generated avatar from the UUID (never Mojang skins). Add offline (when allowed), switch, remove. A disabled "Add Microsoft account · coming soon". |
| **Settings** | Language, default memory for new instances, "Minimize while playing" (on by default), the data folder (read-only, with "Open"), and About: version, GPL-3.0 notice, source link, and "Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft." |
| **Activity drawer** | Current and recent tasks with progress, a live game log per running instance (virtualized, 5,000-line ring buffer), and a crash card on a non-zero exit that wasn't requested. The card shows the likely cause when known (out of memory → "Raise memory to N GB?" with a button that applies it), the last 200 lines, and "Copy for support". The log is already redacted. |
| **Damaged instance** | Rail entry with a warning badge. Its view shows the translated error and fix, with "Open folder" and "Delete". |
| **Close while playing** | A dialog: **Minimize instead** (default) or **Quit and stop games**. |

## 3. Architecture

```
perland-launcher/
├─ crates/launcher-core/   (+ play, settings, mods, meta::catalog, launch::{spawn, crash}, ErrorCode)
├─ crates/perland-cli/     (rewritten on top of launcher_core::play)
├─ src-tauri/              perland-launcher binary: app_core (plain Rust) + thin #[tauri::command] layer
├─ ui/                     React 19 + TS + Vite + Tailwind 4 (pnpm workspace member)
├─ package.json            root: @tauri-apps/cli, workspace scripts
└─ pnpm-workspace.yaml
```

Data flows one way. UI → `ipc/commands.ts` → Tauri command → `AppCore` → `launcher-core`. Core events → `EventSink` (wrapped per run with the instance id) → throttle and batch → `launcher://event` → `ipc/events.ts` → `runStore` → components.

## 4. Core additions (`launcher-core`)

### 4.1 Error codes

- `ErrorCode` is a closed enum, serialized as snake_case (`java_start_failed`, `offline`, `minecraft_version_not_found`, `instance_damaged`, …).
- `LauncherError` gains `code: ErrorCode` and `params: BTreeMap<String, String>`.
- Every constructor takes a code, for example `LauncherError::java(ErrorCode::JavaStartFailed, "Java couldn't be started")`. `.with_param("version", mc)` adds interpolation values.
- `message` stays the English fallback. `detail` stays technical and untranslated.
- `impl From<io::Error>` maps to `ErrorCode::DiskIo`.
- `ErrorCode::ALL: &[ErrorCode]` lists every variant. A unit test proves it's exhaustive with a `match` that has no wildcard arm.
- Every existing error site (about 35) gets its own code. Sites that share a meaning share a code: every "couldn't save X" goes to `disk_write_failed` with an `item` param.
- New codes for this plan:
  - `instance_busy`, `already_running`, `no_account`, `offline_locked`
  - `memory_out_of_range`, `name_too_long`
  - `mod_not_jar`, `mod_exists`, `mod_not_found`
  - `java_test_failed`, `settings_damaged`, `log_write_stopped`
  - `game_files_missing`, `game_crashed`

### 4.2 `launch::spawn` and `GameProcess`

These replace the blocking `run()`; the CLI moves to `spawn` + `wait`.

```rust
pub struct GameProcess { /* pid, kill tx, join handle, tail */ }
impl GameProcess {
    pub fn pid(&self) -> Option<u32>;
    pub fn killer(&self) -> GameKiller;            // Clone + Send; kill() is idempotent
    pub async fn wait(self) -> Result<GameExit>;
}
pub struct GameExit { pub code: Option<i32>, pub killed: bool, pub tail: Vec<String>, pub crash: Option<CrashHint> }
pub async fn spawn(cmd: &LaunchCommand, redactor: Redactor, sink: EventSink, log_file: &Path) -> Result<GameProcess>;
```

- An internal task `select!`s between `child.wait()` and the kill signal. On kill it calls `start_kill()`, waits for the child, and sets `killed = true`.
- **Output lines are capped at 8 KiB.** The bytes are truncated at a UTF-8 boundary and get a ` …[truncated]` suffix.
- **Log write failure:** the first failure emits `Event::Warning(LauncherError{code: LogWriteStopped})` once, stops writing the file, and keeps streaming.
- **Tail:** the last 200 redacted lines are kept for `GameExit.tail`.
- **`launch::crash::diagnose(tail) -> Option<CrashHint>`** returns `CrashHint::OutOfMemory` when it finds `java.lang.OutOfMemoryError`. It is only called when the exit wasn't requested and the code isn't 0. Other patterns can be added later without changing the API.
- `Event` gains `Warning(LauncherError)`.

### 4.3 `play`

```rust
pub struct Prepared { pub resolved: ResolvedVersion, pub java: PathBuf }
pub async fn prepare(installer: &Installer, instance: &Instance, sink: &EventSink) -> Result<Prepared>;
pub async fn start(dirs: &LauncherDirs, store: &InstanceStore, instance: &Instance,
                   prepared: &Prepared, session: &Session, platform: Platform,
                   launcher_version: &str, sink: EventSink) -> Result<GameProcess>;
```

- **`prepare`:** resolve → install → custom Java path, or `java::ensure_runtime`. It emits `Progress` with stable task keys (`install_minecraft`, `download_java`) plus params, so the UI can translate them.
- **`start`:** `build_command` → `missing_files` (anything missing fails with `game_files_missing`) → a `Redactor` with the home path and the session token → `spawn`, logging to `logs/<id>-latest.log`.
- **The CLI's `launch` uses these.** `--dry-run` stays, now built on `prepare` + `build_command` + `missing_files`.

### 4.4 `meta::catalog`

- `minecraft_versions(meta) -> Vec<McVersion { id, kind: Release|Snapshot|OldBeta|OldAlpha, release_time }>`, newest first, from the Mojang manifest.
- `loader_versions(meta, kind, mc) -> Vec<LoaderVersion { version, stable }>` for Fabric (`/v2/versions/loader/{mc}`) and Quilt (`/v3/versions/loader/{mc}`). It returns an empty list when that Minecraft version has no loader, and a `LoaderUnsupported` error for Forge/NeoForge.
- Both go through the ETag cache, so they work offline once cached.

### 4.5 Store operations and validation

- **`InstanceStore`:**
  - `list_all() -> InstanceListing { instances, damaged: Vec<DamagedInstance { id, error }> }`. `list()` stays.
  - `update(&Instance)`, which validates.
  - `rename(id, name)`, which changes the display name only; the id and folder stay.
  - `delete(id)`, which removes the folder.
- **Validation:**
  - names: 1–64 characters after trimming
  - slugs: capped at 48 characters (before any `-N` suffix)
  - `memory_mb`: 512 to 65,536
  - a custom Java path must be an existing file
- **`AccountStore`:**
  - `set_active(uuid)`
  - `remove(uuid)`: removing the active account makes the first remaining account active, or none
  - `list() -> AccountsFile`

### 4.6 `settings`

`Settings { version: 1, language: Option<Lang>, default_memory_mb: 4096, minimize_on_play: true, first_run_done: false }`, stored in `settings.json` and written with `write_atomic`.

`SettingsStore::load()` returns `(Settings, Option<LauncherError>)`. A damaged file yields defaults plus a `settings_damaged` warning, so startup is never blocked.

### 4.7 `mods`

- **Functions:**
  - `list(game_dir) -> Vec<LocalMod { file_name, enabled, size }>`, covering `mods/*.jar` and `mods/*.jar.disabled`, sorted case-insensitively.
  - `set_enabled(game_dir, file_name, bool)`, by rename.
  - `add(game_dir, src) -> LocalMod`, by copy.
  - `remove(game_dir, file_name)`.
- **Validation:**
  - file names are a single path component ending in `.jar` or `.jar.disabled`, checked with `safe_join`
  - `add` refuses anything that isn't `.jar` (`mod_not_jar`)
  - `add` refuses a name that exists in either state (`mod_exists`)
  - `mods/` is created on demand

## 5. Tauri shell (`src-tauri`)

### 5.1 Crate and plugins

The binary is `perland-launcher`, and it's a member of the Cargo workspace. Dependencies: `tauri` 2 and four plugins.
- `dialog` (mod picker, Java path)
- `opener` (folders; `https://` links on an allow-list)
- `single-instance` (a second launch focuses the window; Plan 3 routes `perland://` through it)
- `window-state`

The `fs` and `shell` plugins are deliberately absent.

### 5.2 `AppCore` (plain Rust, no Tauri types)

```rust
pub struct AppCore<R: Runner, E: EventOut> {
    dirs, instances: InstanceStore, accounts: Mutex<AccountStore>, settings: Mutex<SettingsStore>,
    runner: R, out: E, install_slot: Semaphore(1), runs: Mutex<HashMap<String, Run>>,
    logs: Mutex<HashMap<String, VecDeque<String>>>,   // last 5,000 lines of each instance's latest run, for log_tail
}
enum Run { Queued { abort }, Preparing { abort }, Running { killer: Box<dyn Kill> }, Stopping }
trait Runner: Send + Sync + 'static {
    async fn prepare(&self, instance: &Instance, sink: EventSink) -> Result<Prepared>;
    async fn start(&self, instance: &Instance, prepared: &Prepared, session: &Session, sink: EventSink)
        -> Result<Box<dyn RunningGame>>;
}
trait RunningGame: Send { fn killer(&self) -> Box<dyn Kill>; fn wait(self: Box<Self>) -> BoxFuture<'static, Result<GameExit>>; }
trait Kill: Send + Sync { fn kill(&self); }
trait EventOut: Send + Sync + 'static { fn emit(&self, event: AppEvent); fn close_requested(&self); }
```

The real `Runner` wraps `launcher_core::play`, and `GameProcess` implements `RunningGame`. Tests use a scripted fake whose games exit on command.

### 5.3 Commands

Every command returns `Result<T, LauncherError>`. Instance-scoped commands check `is_valid_instance_id` first.

| Command | Returns |
|---|---|
| `app_info` | `{ version, build: dev\|release, offline_allowed, system_memory_mb, data_dir }` |
| `settings_get` / `settings_set(Settings)` | `Settings` |
| `instances_list` | `InstanceListing` |
| `instance_create({name, minecraft, loader?})` | `Instance` (memory from settings' default) |
| `instance_update(Instance)` | `Instance`. Changing game or java while busy → `instance_busy` |
| `instance_rename(id, name)` / `instance_delete(id)` | `Instance` / `()`. Busy → `instance_busy` |
| `catalog_minecraft` / `catalog_loaders(kind, mc)` | lists from §4.4 |
| `accounts_list` / `account_add_offline(name)` / `account_set_active(uuid)` / `account_remove(uuid)` | `AccountsFile` |
| `play(id)` / `stop(id)` | `()`. `play` returns once the state is `Queued`; everything after arrives as events |
| `run_states` | `Vec<{ instance, state }>` (UI resync after reload) |
| `log_tail(id)` | last ≤ 5,000 lines kept by the shell for that instance's latest run |
| `mods_list(id)` / `mod_set_enabled(id, file, on)` / `mods_add(id, paths[])` / `mod_remove(id, file)` | `Vec<LocalMod>` |
| `open_folder(target)` | `()`. The target is `Instance{id}`, `Mods{id}`, `Data` or `Logs`; never a raw path |
| `java_test(path)` | `{ major: u32 }` or `java_test_failed` |
| `quit(stop_games: bool)` | `()`. Called by the close dialog |

### 5.4 Events

Everything goes through one channel, `launcher://event`, carrying `AppEvent` (tagged `type`, snake_case):

```
progress  { instance?: string, task: string, params: {..}, done: u64, total: u64, bytes: u64 }   // ≤10 Hz per task; final always sent
log       { instance: string, lines: string[] }                                               // flushed every 50 ms or 200 lines
run_state { instance: string, state: idle|queued|preparing|running|stopping,
            exit?: { code?: i32, killed: bool, cancelled: bool, crash?: CrashHint, error?: LauncherError } }
warning   { instance?: string, error: LauncherError }
```

### 5.5 Play, Stop and window behavior

- **`play(id)`:**
  1. Refuse with `already_running` (if a run exists), `no_account`, or a damaged-instance error.
  2. Insert `Queued` and spawn a task that waits for `install_slot` → `Preparing` → `runner.prepare` → releases the slot → `runner.start` → `Running`.
  3. If `minimize_on_play` is set, minimize the window.
  4. When `wait()` returns, emit `run_state idle` with `exit` and remove the run.
  5. On a crash, unminimize and focus the window.
- **Failures:** at any step, emit `run_state idle` with `exit.error`.
- **`stop(id)`:** `Queued`/`Preparing` → abort the task, release the slot if held, emit `idle` with `cancelled: true`. `Running` → `killer.kill()` and set the state to `Stopping`; the waiter emits `idle` with `killed: true`.
- **Why abort is safe:** downloads go to `.part` and are renamed, `Downloader` locks per destination across clones, natives extraction and `write_atomic` are atomic, and the Java marker is written last.
- **Closing the window:** `CloseRequested` with any non-idle run → prevent the close and emit `launcher://close-requested` (no payload; separate from `AppEvent`). The dialog then calls `quit(true)` or minimizes. With no active runs, the window closes normally.

### 5.6 Window and security

- Window: `decorations: false`, default 1200×760, minimum 960×600. The title bar uses `data-tauri-drag-region`, with window minimize, toggle-maximize, close and start-dragging permissions.
- CSP: `default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; font-src 'self'; connect-src ipc: http://ipc.localhost`. No remote origins.
- Capabilities list only the app's commands and the exact plugin permissions above.
- Release builds keep the offline gate (`BuildKind::current()`).

## 6. UI (`ui/`)

### 6.1 Stack

React 19, TypeScript (strict), Vite, and Tailwind 4 via `@tailwindcss/vite`, plus:
- `@tanstack/react-query`
- `@tanstack/react-virtual`
- `i18next` and `react-i18next`
- `@tauri-apps/api` with the dialog and opener JS packages

Tooling: Vitest, jsdom, Testing Library (react, user-event, jest-dom), ESLint flat config with typescript-eslint and react-hooks. Fonts are bundled locally.

### 6.2 Structure

```
ui/src/
  ipc/        types.ts · commands.ts (only invoke callers) · events.ts · errors.ts · contract/*.json · mock.ts (dev:mock)
  state/      queries.ts (TanStack hooks + invalidation) · runStore.ts (useSyncExternalStore) · route.ts (typed route store)
  i18n/       index.ts · en.json · es.json
  art/        marquee.ts (seeded terraced-landscape SVG) · plotIcon.ts (seeded rail icon) · avatar.ts (seeded blocky avatar)
  ui/         design-system primitives (Button/PlayButton, Dialog, Toggle, Field, Select, Toast, ErrorCard, ComingSoon, Meter)
  features/   titlebar · rail · home · activity · new-instance · instance-settings · mods · accounts · settings · first-run · close-guard
  App.tsx · main.tsx
```

- **Routes** are `home(id?)`, `mods(id)`, `instanceSettings(id)`, `newInstance`, `accounts`, `settings`, `firstRun`. There's no URL router. First run shows while `settings.first_run_done` is false.
- **`runStore`** holds per-instance `{ state, progress by task, log ring buffer (5,000), lastExit }`. It subscribes once at startup and resyncs with `run_states` plus `log_tail` after a reload.
- **Error translation (`errors.ts`):** `t('errors.<code>.message', params)`, with `errors.<code>.fix` when it exists. If a key is missing, it falls back to `LauncherError.message`/`fix`.
- **Progress task keys** translate from `progress.<task>` with params.
- **Language:** settings first. If none is set, use `navigator.language` (anything starting with `es` → es, otherwise en). Spanish is neutral Latin American with *tú* (`PRODUCT.md`).
- **`pnpm --filter ui dev:mock`** runs the UI in a normal browser with `mockIPC`, serving contract-fixture data and a scripted Play (queued → preparing with progress → running with log lines → exit, or a crash with an OOM hint). Visual work and screenshots use this mode.

### 6.3 Accessibility

- The rail uses a roving tabindex with arrow keys.
- Focus is always visible.
- Dialogs trap focus and return it to their trigger.
- Every action is reachable by keyboard.
- Status always has a text label as well as color.
- `prefers-reduced-motion` stops the drift and the button-light pulse.
- Layouts tolerate Spanish strings up to about 30% longer, and the Play label never truncates.

## 7. Visual system

The binding source is the direction contract in `.impeccable/surfaces/ui-src-app-tsx.md`, inside the owner-pinned game-forward layout. In summary:
- **Ground:** warm laminate black.
- **Hues and roles:** the four land-plot hues have fixed roles: amber means act (Play), lime means running or OK, coral means stop or fault, blue means info or progress.
- **Raised controls:** each sits on a dirt-brown step, a hard offset with no blur.
- **Corners:** pixel-stepped corners taken from the mark.
- **Control states:** five fixed states: raised, pressed, lit, recessed (disabled), fault.
- **At rest:** only Play is lit.
- **Art:** the marquee art is generated, seeded by instance id.
- **Type:** a signage face for marquee names and a hyperlegible body face; the final faces are picked during the build.
- **Signature moment:** Play's press travels in two 90 ms steps, then the segmented meter fills block by block with tabular readouts. State changes snap.

The build is code-led; no image generation is available on this machine. Styling and polish go through `/impeccable` in browser mock mode. The finish reviewer audits the result against the contract, and the documenter then writes `DESIGN.md` and `.impeccable/design.json` from the built UI. The land-plot mark is redrawn as an SVG on its pixel grid from `E:\PerlandDev-site\public\logo.png`, with its provenance noted.

## 8. Contract between Rust and TS

- A test in `src-tauri` (`contract.rs`) serializes one sample of each DTO, every `AppEvent` variant and `ErrorCode::ALL` into `ui/src/ipc/contract/*.json`.
- **Locally**, the test writes the files when `UPDATE_CONTRACT=1` and otherwise compares and fails on drift.
- **In CI**, it always compares.

On the TS side:
- `contract.test.ts` imports each fixture, asserts it `satisfies` the hand-written type at typecheck, and runs the event decoder over it.
- `i18n.test.ts` asserts that `en` and `es` have identical key sets and that every `ErrorCode` has `errors.<code>.message` in both.

## 9. Testing

- **Core (TDD, unit + wiremock):**
  - error-code exhaustiveness and serialization
  - spawn/kill/wait, using the `java -version` pattern already used and skipping when Java is absent
  - kill sets `killed`
  - line cap
  - the log-write-stopped warning, by making the log path unwritable after creation
  - crash diagnose
  - `play::prepare` against wiremock fixtures with tiny files
  - catalog (Fabric and Quilt list fixtures; offline from cache)
  - the store operations and their validation
  - settings: defaults, round-trip, a damaged file
  - mods: list, toggle, add rejections, path safety
- **Shell (`AppCore` with the fake `Runner` and collecting `EventOut`):**
  - the full state machine
  - stop in each state, including the slot being released after cancel
  - two instances running concurrently
  - the same instance refused
  - busy refusals for rename, delete and update
  - progress throttling with the final event kept
  - log batching
  - close-requested with runs active
  - contract fixtures up to date
- **UI (Vitest + RTL + mockIPC):**
  - the Play button through every state, plus cancel and stop calls
  - the log ring-buffer cap
  - error translation and fallback
  - i18n parity
  - first-run gating, including a release-like `app_info` with offline disallowed
  - coming-soon controls are disabled and inert
  - damaged-instance actions
  - mods add, toggle and remove
  - the close dialog
  - rail keyboard navigation
- **Manual smoke (Windows):** `pnpm tauri dev`, then first run, a vanilla latest release, Play, Stop, a Fabric 1.20.1 instance, and a crash card (memory set to 512 on a modded instance, or a test-only crash). Linux is covered by CI building and testing `src-tauri`.
- **Never local:** fuzzing, stress or load tests (CI only, per the repo rules).

## 10. CI and repo

- **`ci.yml`, both OSes:**
  - add pnpm (via `pnpm/action-setup`) and Node 24
  - `pnpm install --frozen-lockfile`
  - `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`
  - add `src-tauri` to the cargo fmt/clippy/test matrix
  - Ubuntu installs `libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev build-essential`
  - Bundling stays in Plan 6.
- **Before every commit:**
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `pnpm -r lint && pnpm -r typecheck && pnpm -r test`
- **`CLAUDE.md`/`AGENTS.md`:** add the UI commands, the contract-fixture rule (`UPDATE_CONTRACT=1`), and "every new string goes in both en.json and es.json".
- **`NOTICE`:** no new ported code is expected. Bundled fonts are listed with their licenses (OFL).

## 11. Risks

- **Tauri 2 on Windows with Smart App Control.** Unsigned dev executables are sometimes blocked (os error 4551). This is environmental: retry or rebuild.
- **Frameless window edge cases** (resize borders, maximize, multiple monitors). Mitigation: the `window-state` plugin, and the manual smoke checks maximize and restore.
- **Error-code migration touches every core error site.** Mitigation: it's done as one mechanical task with the exhaustiveness test, before any UI work.
- **Arcade kitsch.** Mitigation: the contract limits pixel motifs to the mark's stepped geometry, and the finish reviewer checks it.

## 12. Out of scope

Microsoft sign-in (Plan 4) · Forge/NeoForge (Plan 2) · pack links, `.mrpack`, Browse, sync, `perland://` handling (Plan 3) · updater, installers, signing, log rotation, diagnostics export (Plan 6) · light theme · macOS · WebDriver app tests · telemetry.
