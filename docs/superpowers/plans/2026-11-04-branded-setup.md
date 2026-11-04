# Branded one-click Windows setup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A new `perland-setup.exe`: one branded window that installs Perland Launcher by running today's NSIS installer silently, then opens the launcher.

**Architecture:** New workspace crate `crates/perland-setup`. Pure, platform-neutral modules (`model`, `text`, `draw`, `payload`) hold every decision and are unit tested on Linux. One `cfg(windows)` module (`win`) is the shell: window, input, accessibility, registry, processes. The NSIS installer is embedded at build time from `PERLAND_SETUP_PAYLOAD`.

**Tech Stack:** Rust 2021, `winit` 0.30, `softbuffer` 0.4, `tiny-skia` 0.11 (`png` feature), `fontdue` 0.9, `accesskit` + `accesskit_winit`, `semver` 1, `windows-sys` (registry, processes, language, clipboard), `winresource` (build-dependency). Release scripts in Node (`node:test`).

**Spec:** `docs/superpowers/specs/2026-11-04-branded-setup-design.md`

## Global Constraints

- The crate must not depend on Tauri, `launcher-core` or `launcher-app`. Nothing under `src-tauri/` and nothing in `tauri.conf.json` changes.
- `latest.json` and the updater manifest never name the setup app.
- Window 640 × 400 logical pixels, borderless, centred, not resizable.
- Colours: ground `#181410`, raised `#2e261f`, line `#463a2f`, ink `#f6f1e7`, ink-dim `#c5b9a8`, amber `#ffc233`, amber-hi `#ffd76e`; radius 6px. Plot colours appear only inside the pinwheel image.
- Fonts: Bungee (display, buttons) and Atkinson Hyperlegible Next (body), vendored TTFs with OFL licences.
- English and Spanish (neutral Latin American, informal "tú"); `es*` Windows UI language means Spanish, anything else English.
- Registry (per-user, from Tauri 2.12's NSIS template): `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Perland Launcher`, values `DisplayVersion`, `InstallLocation` (written with surrounding quotes), `MainBinaryName` (`perland-launcher.exe`).
- Tauri's NSIS template force-closes a running app in silent mode (`RmShutdown` with `RmForceShutdown`), so the setup app's running check always comes first and the payload is never started while `perland-launcher.exe` runs.
- Silent payload invocation: `<payload> /S`. NSIS exits 0 on success, non-zero otherwise (2 on Abort).
- `/S` on the setup app: launcher running → exit 2; newer version installed → exit 0 without running; otherwise the payload's exit code. Never launches.
- The NSIS process is never killed. Stopping is possible only during Getting ready.
- No downgrade: with a newer version installed the only action is Open.
- Release names: `PerlandLauncher_<version>_x64-installer.exe`; channel copies `PerlandLauncher-installer.exe` (stable) and `PerlandLauncher-beta-installer.exe` (beta).
- Log: `%TEMP%\perland-setup.log`, overwritten each run.
- Before committing (repo rule): `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`; for script changes `node --test scripts/release/`. Also `cargo check -p perland-setup --target x86_64-pc-windows-msvc` from Linux.

## Review Focus

1. **Installed version string that isn't semver** (a hand-edited registry, an old dev build): treat as "older" and offer Update, never panic. Test in Task 1.
2. **`InstallLocation` with or without quotes, trailing backslash**: the exe path is still found. Test in Task 4 (`installed_exe` helper is pure).
3. **Clicks during a state change** (double-clicking Install, clicking Open twice): only one install, one launch. Test in Task 1 (events in the wrong state are ignored).
4. **Longest Spanish strings at 1× and 1.5×**: no text clipped or overlapping the buttons. Test in Task 3.
5. **The temp folder is removed after a failed install** and after stopping during Getting ready. Test in Task 4.

---

### Task 1: Crate, state machine and version decisions

**Files:**
- Create: `crates/perland-setup/Cargo.toml`, `crates/perland-setup/src/main.rs`, `crates/perland-setup/src/lib.rs`, `crates/perland-setup/src/model.rs`
- Modify: `Cargo.toml` (workspace `members`; add `semver = "1"` to `[workspace.dependencies]`)

**Interfaces:**
- Produces (`model.rs`):
  - `pub enum Offer { Install, Update { from: String }, Reinstall, Newer }`. `Reinstall` = same version installed (Open primary, Reinstall secondary); `Newer` = newer installed (Open only).
  - `pub fn offer(installed: Option<&str>, payload: &str) -> Offer`. An unparsable installed version gives `Update { from }`.
  - `pub enum Step { GettingReady, Installing, FinishingUp }`
  - `pub enum ErrorCode { PayloadMissing, TempUnwritable, InstallerFailed(i32), InstallNotFound, Unknown }` with `fn code(&self) -> &'static str` (`payload_missing`, …).
  - `pub enum Screen { Ready(Offer), LauncherRunning, Installing { step: Step, progress: f32, confirm_stop: bool }, Opening, Installed, Error(ErrorCode) }`. `Installed` is the "launch failed, here is OPEN" screen.
  - `pub enum Action { Primary, Secondary, Close, CopyDetails }`
  - `pub enum Event { Pressed(Action), RunningChecked(bool), PayloadWritten, PayloadFailed(ErrorCode), InstallerExited(i32), Confirmed(bool), Launched(bool), Tick(std::time::Duration) }`. `Confirmed(true)` = registry shows this version after the install.
  - `pub enum Effect { CheckRunning, WritePayload, RunInstaller, ConfirmInstall, Launch, DeletePayload, CopyDetails, Exit }`
  - `pub struct Machine { pub screen: Screen, … }` with `pub fn new(offer: Offer, payload_present: bool) -> Machine` and `pub fn on(&mut self, event: Event) -> Vec<Effect>`.
  - `pub fn installing_progress(elapsed: Duration) -> f32`: `0.15 + 0.75 * (1 - e^(-3t/20s))`, never above 0.90.
  - `pub enum SilentPlan { Run, Exit(i32) }`, `pub fn silent_plan(installed: Option<&str>, payload: &str, running: bool) -> SilentPlan`.

Transitions (the tests pin them): Ready + Primary → `CheckRunning` (Install/Update/Reinstall-secondary) or `Launch` (Open on Reinstall/Newer). `RunningChecked(true)` → LauncherRunning; LauncherRunning + Primary (Try again) → `CheckRunning`. `RunningChecked(false)` → Installing{GettingReady, 0.0} + `WritePayload` (or Error(PayloadMissing) when `payload_present` is false). `PayloadWritten` → Installing{Installing, 0.15} + `RunInstaller`; `Tick` advances progress only in step Installing. `InstallerExited(0)` → FinishingUp + `ConfirmInstall` + `DeletePayload`; non-zero → Error(InstallerFailed(code)) + `DeletePayload`. `Confirmed(true)` → Opening + `Launch`; `Confirmed(false)` → Error(InstallNotFound). `Launched(true)` → `Exit` (after the fade, the shell's timing); `Launched(false)` → Installed. Close: Ready/LauncherRunning/Error/Installed → `Exit`; GettingReady → sets `confirm_stop`; Primary while `confirm_stop` → `DeletePayload` + `Exit`; Secondary while `confirm_stop` → clears it; Close in steps Installing/FinishingUp → nothing. Error + Primary (Try again) → `CheckRunning`. Error + CopyDetails → `CopyDetails`. Any event not listed for the current screen → no effects, screen unchanged.

- [ ] **Step 1: Write the failing tests** in `model.rs` `#[cfg(test)]`: `offer_none_is_install`, `offer_older_is_update` (`"0.1.0"` vs `"0.2.0"` → `Update{from:"0.1.0"}`), `offer_same_is_reinstall`, `offer_newer_is_newer` (`"0.3.0"`; also `"0.2.0"` vs payload `"0.2.0-beta.3"` → Newer), `offer_garbage_is_update` (`"dev"`), `install_happy_path` (Primary → [CheckRunning], RunningChecked(false) → [WritePayload], PayloadWritten → [RunInstaller], InstallerExited(0) → [ConfirmInstall, DeletePayload], Confirmed(true) → [Launch], Launched(true) → [Exit]), `double_primary_starts_one_install` (second Primary while Installing → []), `running_launcher_blocks`, `installer_failure_is_error_with_code` (exit 2 → `Error(InstallerFailed(2))`, effects include DeletePayload), `close_during_getting_ready_asks_then_stops`, `close_while_installing_does_nothing`, `open_on_newer_launches`, `launch_failure_shows_installed`, `progress_bounds` (t=0 → 0.15, t=20s ≈ 0.86 ± 0.01, t=10min ≤ 0.90), `tick_outside_installing_is_ignored`, `silent_plan_cases` (running → Exit(2); newer → Exit(0); older/none/same → Run), `payload_missing_errors`.
- [ ] **Step 2: Run** `cargo test -p perland-setup` → fails to compile (no items).
- [ ] **Step 3: Implement** the crate (`lib.rs` exposes `model`, `text`, `draw`, `payload`, `#[cfg(windows)] win`; `main.rs` is `#![cfg_attr(windows, windows_subsystem = "windows")]`, calls `perland_setup::win::run()` on Windows and prints "Perland Launcher Setup runs on Windows." and exits 1 elsewhere) and `model.rs` as specified.
- [ ] **Step 4: Run** `cargo test -p perland-setup` → PASS; `cargo clippy -p perland-setup --all-targets -- -D warnings` clean.
- [ ] **Step 5: Commit** `feat(setup): crate and state machine for the one-click setup`.

### Task 2: Strings in English and Spanish

**Files:** Create `crates/perland-setup/src/text.rs`

**Interfaces:**
- Produces: `pub enum Lang { En, Es }`; `pub fn lang_from_langid(langid: u16) -> Lang` (primary language `langid & 0x3ff == 0x0a` → Es); `pub struct Strings { … }` with `&'static str` fields and `pub fn strings(lang: Lang) -> &'static Strings`; formatters `pub fn version_line(lang, offer: &Offer, payload: &str) -> String` and `pub fn error_text(lang, code: &ErrorCode) -> String`; `pub fn step_text(lang, step: Step) -> &'static str`; `pub fn button_labels(lang, screen: &Screen) -> (Option<&'static str>, Option<&'static str>)` (primary, secondary).

Copy (English; Spanish carries the same meaning):
- Title `PERLAND LAUNCHER`; tagline "Play Minecraft on Perland servers, in one click." / "Juega Minecraft en servidores de Perland, con un clic."
- Buttons: INSTALL / INSTALAR, UPDATE / ACTUALIZAR, OPEN / ABRIR, Reinstall / Reinstalar, TRY AGAIN / REINTENTAR, Copy details / Copiar detalles, Stop / Detener, Keep installing / Seguir instalando.
- Version line: "Version 0.2.0 · Installs for you only, no admin needed"; Update: "Version 0.1.0 → 0.2.0"; Reinstall/Newer title "Perland Launcher is installed" and line "Version 0.2.0 is installed".
- Installing title INSTALLING… / INSTALANDO…; steps "Getting ready", "Installing", "Finishing up" / "Preparando", "Instalando", "Terminando".
- Stop prompt "Stop the install?"; while NSIS runs: "Almost there, this can't be stopped now".
- Opening: "Opening Perland Launcher…". Running: "Close Perland Launcher to continue. If a game is running, quit it first."
- Errors: payload_missing "This setup has no installer inside. Download it again from perland.online/launcher."; temp_unwritable "Couldn't prepare the install. Check there's free space on your drive and try again."; installer_failed "The install didn't finish (code {n}). Try again."; install_not_found "The install finished, but Perland Launcher wasn't found. Try again."; unknown "Something went wrong. Copy the details and send them to us."

- [ ] **Step 1: Failing tests:** `langid_spanish_variants` (0x0c0a, 0x080a, 0x2c0a → Es; 0x0409, 0x0403 → En), `every_screen_has_labels_in_both_languages` (iterate screens × langs, labels non-empty where the screen has a button), `error_text_names_code` (`InstallerFailed(2)` contains "2"), `version_line_update_shows_both`.
- [ ] **Step 2: Run** → fail. **Step 3: Implement.** **Step 4: Run** → pass.
- [ ] **Step 5: Commit** `feat(setup): setup strings in English and Spanish`.

### Task 3: Assets, drawing and the render example

**Files:**
- Create: `crates/perland-setup/assets/` (`Bungee-Regular.ttf`, `AtkinsonHyperlegibleNext.ttf`, `OFL-Bungee.txt`, `OFL-Atkinson.txt`, `pinwheel-on-dark-512.png` copied from `perland-brand/logo/icon/png/perland-icon-on-dark-512.png`), `crates/perland-setup/src/draw.rs`, `crates/perland-setup/examples/render.rs`
- Modify: `NOTICE` (fonts and the pinwheel under "Bundled" for the setup app)

**Interfaces:**
- Consumes: `Screen`, `Offer`, `Action` (Task 1); `Lang`, `strings`, `button_labels` etc. (Task 2).
- Produces: `pub const WIDTH: u32 = 640; pub const HEIGHT: u32 = 400;` `pub struct Ui { pub lang: Lang, pub payload_version: &'static str /*or String*/, pub hover: Option<Action>, pub focus: Option<Action>, pub fade: f32 }`; `pub struct Button { pub action: Action, pub rect: tiny_skia::Rect, pub label: String, pub primary: bool }`; `pub fn buttons(screen: &Screen, ui: &Ui) -> Vec<Button>` (logical px, includes the × close at top-right, 36 × 36, when closing is allowed); `pub fn hit(screen, ui, x: f32, y: f32) -> Option<Action>`; `pub fn drag_region(y: f32) -> bool` (top 36px outside the ×); `pub fn render(screen: &Screen, ui: &Ui, scale: f32) -> tiny_skia::Pixmap`; `pub struct TextFit { pub text: String, pub width: f32, pub max: f32 }` and `pub fn texts(screen, ui) -> Vec<TextFit>` (every drawn line with its measured width and the room it has, for tests).

Layout (logical px): pinwheel 96 × 96 centred, top 56; title Bungee 28px centred at y 176; tagline/step text Atkinson 15px ink-dim at y 206; primary button 200 × 52, radius 6, centred, top 248, amber fill (amber-hi on hover), label Bungee 18px in ground colour; secondary text button 15px ink-dim under it, top 312; version line Atkinson 13px ink-dim centred at y 372; progress bar 400 × 6 at y 250, raised track, amber fill, radius 3; focus ring 2px ink, 3px outside the button. Fonts are parsed once (`std::sync::OnceLock`).

- [ ] **Step 1: Failing tests:** `buttons_inside_window` (every screen × lang × scale 1.0: rects within 0..640, 0..400), `no_text_overflows` (every `TextFit` has `width <= max`, both langs), `hit_finds_primary` (centre of the primary rect → `Some(Primary)`), `close_hidden_while_installing` (no `Close` button in step Installing), `render_has_expected_size` (scale 1.5 → 960 × 600), `render_paints_ground` (pixel (4, 390) equals ground colour).
- [ ] **Step 2: Run** → fail. **Step 3: Implement** `draw.rs`. Text: `fontdue` rasterise per glyph at `size * scale`, composite onto the pixmap with the colour; kerning optional.
- [ ] **Step 4: Run** → pass. Implement `examples/render.rs`: writes `target/setup-renders/<screen>-<lang>@<scale>x.png` for every screen (Ready Install/Update/Reinstall/Newer, LauncherRunning, Installing ×3 steps and confirm_stop, Opening, Installed, Error InstallerFailed(2)) in both langs at 1× and 2×. Run `cargo run -p perland-setup --example render` and look at the PNGs.
- [ ] **Step 5: Commit** `feat(setup): draw the setup window in the launcher's look`.

### Task 4: Payload and installed-app helpers

**Files:** Create `crates/perland-setup/build.rs`, `crates/perland-setup/src/payload.rs`

**Interfaces:**
- Produces: `pub fn embedded() -> Option<&'static [u8]>` (None when the build had no `PERLAND_SETUP_PAYLOAD`; `build.rs` copies the file to `OUT_DIR/payload.bin` and sets `cfg(has_payload)`, with `rerun-if-env-changed` and `rerun-if-changed` on the path; declare the cfg with `cargo::rustc-check-cfg`); `pub struct PayloadDir` with `pub fn write(bytes: &[u8], temp_root: &Path) -> io::Result<PayloadDir>` (creates `perland-setup-<16 hex random>/PerlandLauncherSetup.exe`), `pub fn exe(&self) -> &Path`, and `Drop` removing the folder; `pub fn installed_exe(install_location: &str, main_binary: Option<&str>) -> PathBuf` (strips surrounding quotes and a trailing backslash; default binary `perland-launcher.exe`).
- On Windows, `build.rs` also embeds the icon (`../../src-tauri/icons/icon.ico`), version info (ProductName "Perland Launcher", FileDescription "Perland Launcher Setup", CompanyName "Perland", version from `CARGO_PKG_VERSION`) and a manifest (`asInvoker`, `PerMonitorV2`, common controls v6) through `winresource`, only when `CARGO_CFG_TARGET_OS == "windows"` and the host can run the resource compiler (skip with a warning otherwise, so `cargo check --target x86_64-pc-windows-msvc` works from Linux).

- [ ] **Step 1: Failing tests:** `write_creates_exe_and_drop_removes_dir`, `dropped_after_error_path_still_removed` (write, then drop without running: folder gone), `two_writes_get_different_dirs`, `installed_exe_cases` (`"\"C:\\Users\\a\\AppData\\Local\\Perland Launcher\""`, the same unquoted, with trailing `\`, with `MainBinaryName` `"other.exe"`), `no_payload_in_test_builds` (`embedded().is_none()` when the env var is unset — mark `#[cfg(not(has_payload))]`).
- [ ] **Step 2: Run** → fail. **Step 3: Implement.** **Step 4: Run** → pass; also `PERLAND_SETUP_PAYLOAD=README.md cargo test -p perland-setup` passes with `embedded()` returning the bytes (a test under `#[cfg(has_payload)]`).
- [ ] **Step 5: Commit** `feat(setup): embed and unpack the NSIS installer`.

### Task 5: The Windows shell

**Files:** Create `crates/perland-setup/src/win/mod.rs` (event loop, window, frames), `win/system.rs` (registry, processes, language, clipboard, launch), `win/a11y.rs` (AccessKit tree), `win/log.rs`

**Interfaces:**
- Consumes: everything above.
- Produces: `pub fn run() -> i32` (process exit code). Internals: `fn installed() -> Option<(String /*version*/, PathBuf /*exe*/)>` (reads the Global Constraints registry key with `RegGetValueW`), `fn launcher_running() -> bool` (Toolhelp32 snapshot, exe name `perland-launcher.exe` case-insensitive), `fn ui_lang() -> Lang` (`GetUserDefaultUILanguage`), `fn launch(exe: &Path) -> bool` (`Command` with `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`, no args), `fn copy_to_clipboard(text: &str)`, `log::line(&str)`.

Behaviour:
- `run()` first checks the command line: `/S` (case-insensitive) → `silent_plan`, then on `Run` write the payload, run `/S`, wait, delete, return its code. No window.
- Otherwise: build `Machine::new(offer(installed, VERSION), embedded().is_some())`, open the winit window (640 × 400 logical, `with_decorations(false)`, `with_resizable(false)`, centred, title "Perland Launcher Setup", window icon from the embedded ico or the PNG), present frames with softbuffer from `draw::render` at the window's scale factor.
- Effects run off the UI thread where they block (`WritePayload`, `RunInstaller` wait, `ConfirmInstall`, `CheckRunning`), sending `Event`s back through an `EventLoopProxy`. A 60 fps `Tick` timer only while Installing or fading; otherwise redraw on input only.
- Input: mouse move → hover; click → `hit`; press in `drag_region` → `drag_window()`; Enter → `Primary`, Esc → `Close`, Tab/Shift-Tab cycle focus among `buttons`, Space/Enter on focus → that action. `Launched(true)` starts a 200 ms fade then exits 0.
- AccessKit: a window node named "Perland Launcher Setup", a label node with the title and status text, a button node per `Button` (name = label; × named "Close"); focus follows `Ui.focus`; AccessKit actions `Click`/`Focus` map to the same events.
- Log every effect, event, version and exit code to `%TEMP%\perland-setup.log`. `CopyDetails` copies "Perland Launcher Setup <version>\nError: <code>\nLog: <path>".

- [ ] **Step 1:** Write `system.rs` and `log.rs`; `cargo check -p perland-setup --target x86_64-pc-windows-msvc` clean (no unit tests: this layer is Windows API calls; the decisions are tested in Tasks 1–4).
- [ ] **Step 2:** Write `mod.rs` and `a11y.rs`; same check plus `cargo clippy -p perland-setup --target x86_64-pc-windows-msvc -- -D warnings` clean.
- [ ] **Step 3:** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` on Linux still pass.
- [ ] **Step 4: Commit** `feat(setup): Windows window, accessibility and install effects`.

### Task 6: Release workflow and names

**Files:** Modify `scripts/release/lib.mjs`, `scripts/release/lib.test.mjs`, `scripts/release/cli.mjs`, `.github/workflows/release.yml`, `.github/workflows/ci.yml` (only if the Windows CI job needs the target; it builds natively, so expect no change), `README.md` (Releasing: one line on the setup app)

**Interfaces:**
- Produces in `lib.mjs`: `export function setupReleaseName(version)` → `PerlandLauncher_${version}_x64-installer.exe`; `export function setupChannelName(channel)` → `PerlandLauncher${channel === 'beta' ? '-beta' : ''}-installer.exe`; `export function findSetup(paths)` → the path ending `perland-setup.exe`.
- `cli.mjs stage` takes an optional 4th argument, the setup exe path, and copies it to `setupReleaseName(version)` (no `.sig`). `channel-copies` copies `setupReleaseName(version)` to `setupChannelName(channel)` when present in `dist`.
- `release.yml` Windows job: after "Build and sign the updater artifacts", a step `Build the setup app` (`if: runner.os == 'Windows'`, bash): find the NSIS bundle (`target/release/bundle/nsis/*_x64-setup.exe`), `PERLAND_SETUP_PAYLOAD="$nsis" cargo build -p perland-setup --release`, then when `SIGN_WINDOWS` is true run the same `trusted-signing-cli` command on `target/release/perland-setup.exe`. The stage step passes `target/release/perland-setup.exe` as the 4th argument on Windows.

- [ ] **Step 1: Failing tests** in `lib.test.mjs`: `setupReleaseName('0.2.0-beta.1')`, `setupChannelName('stable')` → `PerlandLauncher-installer.exe`, `setupChannelName('beta')` → `PerlandLauncher-beta-installer.exe`, `findSetup` finds `target/release/perland-setup.exe` and ignores `perland-setup.pdb`.
- [ ] **Step 2: Run** `node --test scripts/release/` → fail. **Step 3: Implement** lib, cli and workflow. **Step 4: Run** → pass; `node scripts/release/cli.mjs stage 0.0.1 <fixture dir> <out> <fake exe>` by hand produces the renamed file.
- [ ] **Step 5: Commit** `ci(release): build, sign and publish the setup app`.

### Task 7: Renders in the PR and final checks

- [ ] Run the full pre-commit list from Global Constraints.
- [ ] Generate the renders (Task 3 example), host them on an assets-only branch (as the engine's `pr-screenshots` skill does), and add them to PR #143's body with the Windows manual test list from the spec §7 as unchecked boxes for the owner.
- [ ] Mark the plan's boxes and push.
