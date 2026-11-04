# Setup install options and branded uninstaller Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an Options view to the one-click setup (install folder, shortcuts, what and where) and a branded uninstaller in the same window and look.

**Architecture:** The amendment extends the existing crate `crates/perland-setup` (plan `2026-11-04-branded-setup.md`, done). New pure modules `options.rs` (folder rules, installer arguments, shortcut removal) and `uninstall.rs` (the uninstaller's command-line modes, state machine, data targets) are unit tested on Linux. `model::Screen`, `text` and `draw` gain the Options and uninstaller screens, so both apps share one look. A second binary, `perland-uninstall`, calls `win::run_uninstall()`. A Tauri NSIS hook, added only by the release workflow, points Windows' Uninstall at it.

**Tech Stack:** As before (Rust, winit, softbuffer, tiny-skia, fontdue, AccessKit, windows-sys). Folder picker: `rfd` 0.15 (Windows `IFileOpenDialog`, no GTK on Windows). NSIS hook (`.nsh`).

**Spec:** `docs/superpowers/specs/2026-11-04-branded-setup-design.md` (amendment: §2, §3 approaches 5–6, §4a, §4b, §6.4a, §6.5, §6.7, §7).

## Global Constraints

- Install stays one click: Options is a link on the Ready screens; Install never waits on a choice.
- The NSIS template, `tauri.conf.json` and the updater are untouched. The hook and the bundled uninstaller are added only through `tauri build --config` in `.github/workflows/release.yml`.
- Default folder `%LOCALAPPDATA%\Perland Launcher`; data `%APPDATA%\PerlandLauncher`; webview data `%APPDATA%\dev.perland.launcher` and `%LOCALAPPDATA%\dev.perland.launcher`; credentials: Credential Manager entries of service `online.perland.launcher` (keyring target names `<user>.online.perland.launcher`).
- Folder rules (spec §4a): refuse unwritable folders and anything under `Program Files`, `Program Files (x86)` or `Windows`; an empty/new folder or one holding `perland-launcher.exe` is used as is; any other folder gets `Perland Launcher` appended; refuse when that appended folder exists and holds other files.
- NSIS arguments: fresh install `/S`, plus `/NS` when both shortcuts are off, plus `/D=<path>` last and unquoted when the folder isn't the default. Update and Reinstall: `/S /UPDATE`, no folder, shortcuts untouched.
- One shortcut off on a fresh install: delete `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Perland Launcher.lnk` or `%USERPROFILE%\Desktop\Perland Launcher.lnk` (the shell resolves the real Desktop via `SHGetKnownFolderPath`) after the install.
- Installed size from `PERLAND_SETUP_INSTALLED_BYTES` at build time; absent → no size line.
- Uninstaller: data box unticked by default; ticked, the button reads UNINSTALL AND DELETE; `/S` never deletes data and exits 2 while the launcher runs; with `_?=` in its arguments it opens no window, runs `<dir>\uninstall.exe` with the same arguments and exits with its code.
- Removal: `"<dir>\uninstall.exe" /S _?=<dir>`, then delete `uninstall.exe`, `perland-uninstall.exe` and the folder only if empty. Never `remove_dir_all` on the install folder.
- The window stays 640 × 400 (the Options view fits; a test asserts it).
- English and Spanish (neutral Latin American, "tú") for every new string.
- Before committing: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo clippy -p perland-setup --target x86_64-pc-windows-msvc --all-targets -- -D warnings`.

## Review Focus

1. **A folder path with spaces** (`D:\My Games`) reaches NSIS as `/D=D:\My Games\Perland Launcher`, last and unquoted. Test in Task 1.
2. **A drive root** (`D:\`, never empty) resolves to `D:\Perland Launcher`, not a refusal. Test in Task 1.
3. **Uninstalling with "delete my data" when the data folder doesn't exist** succeeds with nothing reported missing. Test in Task 3.
4. **A very long install path** is shortened with a middle ellipsis in the Options view, never overflowing. Test in Task 4.
5. **The uninstaller started from the install folder** relaunches from temp; started from temp with `--from-temp` it opens the window; both decided from arguments alone. Test in Task 3.

---

### Task 1: Install options logic

**Files:** Create `crates/perland-setup/src/options.rs`; modify `src/lib.rs` (`pub mod options;`), `Cargo.toml` (dev-dep only if needed).

**Interfaces:**
- Produces:
  - `pub struct Choice { pub folder: Option<PathBuf>, pub start_menu: bool, pub desktop: bool }` with `Default` = `{ None, true, true }`. `folder: None` = the default folder.
  - `pub enum FolderProblem { NotWritable, NeedsAdmin, NotEmpty }` with `fn code(&self) -> &'static str` (`not_writable`, `needs_admin`, `not_empty`).
  - `pub fn resolve_folder(picked: &Path, protected: &[PathBuf]) -> Result<PathBuf, FolderProblem>`: the spec's rules against the real filesystem; writability = create and remove `<target or its nearest existing parent>\.perland-write-test`; comparison of `protected` roots ignores ASCII case.
  - `pub fn installer_args(choice: &Choice, fresh: bool) -> Vec<String>`.
  - `pub fn shortcuts_to_remove(choice: &Choice, fresh: bool, start_menu: &Path, desktop: &Path) -> Vec<PathBuf>`.
  - `pub fn installed_bytes() -> Option<u64>` (from `option_env!("PERLAND_SETUP_INSTALLED_BYTES")`; `build.rs` adds `rerun-if-env-changed`).
  - `pub fn size_text(bytes: u64) -> String` → `"About N MB"`-style number only: `"25 MB"` (rounded up to whole MB; under 1 MB → `"1 MB"`).

- [ ] **Step 1: Failing tests** (tempdir-backed): `empty_folder_used_as_is`, `new_folder_used_as_is` (a path that doesn't exist yet, parent writable), `folder_with_launcher_used_as_is` (contains `perland-launcher.exe`), `other_folder_gets_subfolder` (`D:\Games` with a file → `…\Games\Perland Launcher`), `drive_like_root_gets_subfolder` (non-empty tempdir root → `<root>\Perland Launcher`), `subfolder_with_other_files_is_refused` (`NotEmpty`), `subfolder_with_launcher_is_fine`, `protected_root_needs_admin` (a `protected` entry `<tmp>\ProgramFiles`, picked `<tmp>\programfiles\X` → `NeedsAdmin`), `unwritable_is_refused` (`#[cfg(unix)]`, mode 0o555 dir → `NotWritable`; skip when running as root, `nix`-free check via trying a write first), `args_default_install` (`["/S"]`), `args_custom_folder_last_unquoted` (`Choice{folder: Some("D:\\My Games\\Perland Launcher")}` → `["/S", "/D=D:\\My Games\\Perland Launcher"]`), `args_no_shortcuts` (`["/S", "/NS"]`, and with a folder `/NS` comes before `/D=`), `args_update` (`fresh: false` → `["/S", "/UPDATE"]` whatever the choice), `remove_one_shortcut` (desktop off → `[desktop]`; start menu off → `[start_menu]`; both off or both on → `[]`; `fresh: false` → `[]`), `size_text_rounds_up` (`25_000_001` → `"25 MB"`, `100` → `"1 MB"`; MB = 1_000_000).
- [ ] **Step 2: Run** `cargo test -p perland-setup options` → FAIL (missing items).
- [ ] **Step 3: Implement** `options.rs`.
- [ ] **Step 4: Run** → PASS.
- [ ] **Step 5: Commit** `feat(setup): install folder rules and installer arguments`.

### Task 2: The Options screen in the setup's state machine

**Files:** Modify `crates/perland-setup/src/model.rs`.

**Interfaces:**
- Consumes: `Choice`, `FolderProblem` (Task 1).
- Changes: `Event` loses `Copy` (keeps `Clone`, `PartialEq`). `Action` gains `Options`, `ChangeFolder`, `ToggleStartMenu`, `ToggleDesktop`, `Back`. `Screen` gains `Options { problem: Option<FolderProblem> }`. `Event` gains `FolderPicked(Option<PathBuf>)` (None = dialog cancelled) and `FolderResolved(Result<PathBuf, FolderProblem>)`. `Effect` gains `PickFolder`, `ResolveFolder(PathBuf)` (Effect loses `Copy`, keeps `Clone`), `RemoveShortcuts`.
- `Machine` gains `pub choice: Choice` and `pub fn fresh(&self) -> bool` (the offer was `Install`).
- Rules: Ready + `Options` → `Options{problem: None}`. Options + `Back` or `Close` → back to the Ready screen it came from (Close here never exits). Options + `ChangeFolder` → `[PickFolder]` only when `fresh()`. `FolderPicked(Some(p))` → `[ResolveFolder(p)]`; `FolderPicked(None)` → nothing. `FolderResolved(Ok(p))` → `choice.folder = Some(p)`, problem cleared; `Err(e)` → `problem: Some(e)`, folder unchanged. `ToggleStartMenu` / `ToggleDesktop` flip only when `fresh()`. Options + `Primary` → the install starts exactly as Ready + Primary does (CheckRunning), but is refused (no effects) while `problem` is `Some`. After `InstallerExited(0)` on a fresh install, effects are `[RemoveShortcuts, ConfirmInstall, DeletePayload]`; otherwise unchanged.

- [ ] **Step 1: Failing tests:** `options_opens_and_back_returns` (Ready(Install) → Options → Back → Ready(Install); Close in Options also returns, never `Exit`), `change_folder_picks_then_resolves` (ChangeFolder → `[PickFolder]`; `FolderPicked(Some(p))` → `[ResolveFolder(p)]`; `FolderResolved(Ok(q))` → `m.choice.folder == Some(q)`), `cancelled_picker_changes_nothing`, `refused_folder_blocks_install` (`FolderResolved(Err(NotEmpty))` → `Options{problem: Some(NotEmpty)}`, Primary → `[]`; a later `Ok` clears it and Primary → `[CheckRunning]`), `update_cannot_change_folder_or_shortcuts` (Ready(Update) → Options; ChangeFolder → `[]`; ToggleDesktop leaves `choice.desktop == true`), `toggles_flip_on_fresh_install`, `fresh_install_removes_shortcuts_after_success` (InstallerExited(0) → `[RemoveShortcuts, ConfirmInstall, DeletePayload]`), `update_does_not_remove_shortcuts` (Update path → `[ConfirmInstall, DeletePayload]`). Existing tests keep passing (adjust only for `Event` no longer `Copy`).
- [ ] **Step 2: Run** `cargo test -p perland-setup model` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p perland-setup` → PASS.
- [ ] **Step 5: Commit** `feat(setup): options screen in the state machine`.

### Task 3: Uninstaller logic

**Files:** Create `crates/perland-setup/src/uninstall.rs`; modify `src/lib.rs`, `src/model.rs` (shared `Screen` variants).

**Interfaces:**
- Produces in `uninstall.rs`:
  - `pub enum Mode { Handoff { dir: PathBuf, args: Vec<String> }, Silent { dir: PathBuf }, Relaunch { dir: PathBuf }, Window { dir: PathBuf } }` and `pub fn mode(args: &[String], exe_dir: &Path) -> Mode`. Rules, in order: any argument starting `_?=` → `Handoff` (dir = that argument's value, args = all arguments as given); `/S` (any case) → `Silent` (dir = `exe_dir`); `--from-temp <dir>` → `Window { dir }`; else `Relaunch { dir: exe_dir }`.
  - `pub enum UStep { RemovingApp, DeletingData }`.
  - `pub enum UEvent { Pressed(Action), RunningChecked(bool), UninstallerExited(i32), DataDeleted(Vec<String>), DataSize(u64) }` and `pub enum UEffect { CheckRunning, RunUninstaller, DeleteData, CopyDetails, Exit }`.
  - `pub struct UMachine { pub screen: Screen, pub delete_data: bool, pub data_bytes: Option<u64>, … }`, `UMachine::new() -> UMachine` (screen `Uninstall(Confirm)`, `delete_data: false`), `fn on(&mut self, e: UEvent) -> Vec<UEffect>`.
  - `pub fn data_targets(appdata: &Path, local_appdata: &Path) -> Vec<PathBuf>` → `[appdata/PerlandLauncher, appdata/dev.perland.launcher, local_appdata/dev.perland.launcher]`.
  - `pub fn delete_paths(paths: &[PathBuf]) -> Vec<String>` (each path that could not be removed, as displayed; a missing path is fine).
  - `pub fn dir_size(path: &Path) -> u64` (no symlinks followed; missing → 0).
  - `pub fn credential_filter(service: &str) -> String` → `"*.online.perland.launcher"`.
  - `pub fn uninstaller_args(dir: &Path) -> Vec<String>` → `["/S", "_?=<dir>"]`.
  - `pub fn cleanup_command(temp_dir: &Path) -> (String, Vec<String>)` → `("cmd", ["/c", "ping -n 4 127.0.0.1 >nul & rmdir /s /q \"<dir>\""])`.
- Produces in `model.rs`: `Screen::Uninstall(UScreen)` with `pub enum UScreen { Confirm, Removing { step: UStep, progress: f32 }, Done { kept_data: bool }, Failed { code: i32, not_deleted: Vec<String> } }`; `Screen` loses nothing. `Action` gains `ToggleDeleteData`. The uninstaller reuses `Screen::LauncherRunning`.
- Rules: Confirm + `ToggleDeleteData` flips; Confirm + `Primary` → `[CheckRunning]` (once, as the setup's guard); `RunningChecked(true)` → LauncherRunning (Primary there → CheckRunning again); `RunningChecked(false)` → Removing{RemovingApp, 0.0} + `[RunUninstaller]`; `UninstallerExited(0)` → with `delete_data` Removing{DeletingData, 0.6} + `[DeleteData]`, else `Done{kept_data: true}`; non-zero → `Failed{code, not_deleted: []}`; `DataDeleted([])` → `Done{kept_data: false}`; `DataDeleted(list)` → `Failed{code: 0, not_deleted: list}`; `DataSize(n)` stores `data_bytes`. Close on Confirm/LauncherRunning/Done/Failed → `[Exit]`; Done + Primary (CLOSE) → `[Exit]`; Failed + CopyDetails → `[CopyDetails]`. Close while Removing does nothing.

- [ ] **Step 1: Failing tests:** `mode_handoff` (`["_?=C:\\X"]` → Handoff{dir: "C:\\X", args: ["_?=C:\\X"]}; also with `/P` before it), `mode_silent`, `mode_from_temp_opens_window`, `mode_default_relaunches`, `keep_data_path` (Primary, RunningChecked(false) → [RunUninstaller]; UninstallerExited(0) → []; screen Done{kept_data: true}), `delete_data_path` (toggle, …, UninstallerExited(0) → [DeleteData]; DataDeleted([]) → Done{kept_data: false}), `uninstaller_failure`, `data_not_deleted_is_reported`, `running_launcher_blocks_uninstall`, `close_while_removing_does_nothing`, `data_targets_names_three_folders`, `delete_paths_missing_is_fine` (tempdir, one existing dir with a file and one missing → `[]`, existing gone), `dir_size_counts_files` (two files of 3 and 4 bytes → 7), `credential_filter_is_the_service`, `uninstaller_args_in_place`, `cleanup_command_quotes_the_folder` (path with a space appears inside quotes).
- [ ] **Step 2: Run** `cargo test -p perland-setup uninstall` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p perland-setup` → PASS.
- [ ] **Step 5: Commit** `feat(setup): uninstaller modes, state machine and data targets`.

### Task 4: Words and drawing for Options and the uninstaller

**Files:** Modify `src/text.rs`, `src/draw.rs`, `examples/render.rs`.

**Interfaces:**
- `Ui` gains `pub paths: Paths` with `pub struct Paths { pub install: PathBuf, pub data: PathBuf, pub installed_bytes: Option<u64>, pub choice: Choice, pub data_bytes: Option<u64>, pub delete_data: bool }` (the shell fills it; `install` = the folder in force: chosen, current or default).
- `pub fn ellipsize_middle(text: &str, max: f32) -> String` in `draw.rs` (body face, 15px) keeps the start and the end with `…` between.
- `text.rs`: `Strings` gains the copy below; `button_labels` covers the new screens; `all_screens()` includes `Options{None}`, `Options{Some(NotEmpty)}`, and every `Uninstall` variant plus `Failed` with two names.
- Copy (EN; ES carries the same meaning):
  - Ready footer adds the link "Options" / "Opciones" after the version line (drawn as a text button, `Action::Options`).
  - Options: title `INSTALL OPTIONS` / `OPCIONES DE INSTALACIÓN`; "Install to" / "Instalar en"; "Change…" / "Cambiar…"; "About {size} · For you only, no admin needed" / "Unos {size} · Solo para ti, sin permisos de administrador"; update/reinstall: "Updates keep the folder it's in" / "Las actualizaciones usan la carpeta actual"; "Your games, worlds and settings stay in" / "Tus juegos, mundos y ajustes se quedan en"; "What gets installed" / "Qué se instala"; items "Perland Launcher, and its uninstaller", "Microsoft WebView2, only if Windows doesn't have it", "perland:// links, so invite pages open the launcher", "An entry in Settings → Apps" / "Perland Launcher y su desinstalador", "Microsoft WebView2, solo si Windows no lo tiene", "Enlaces perland://, para que las invitaciones abran el launcher", "Una entrada en Configuración → Aplicaciones"; checkboxes "Start menu shortcut", "Desktop shortcut" / "Acceso en el menú Inicio", "Acceso en el escritorio"; update: "Shortcuts stay as they are" / "Los accesos directos se quedan como están"; buttons INSTALL / UPDATE / OPEN as on Ready, "Back" / "Volver"; problems: not_writable "You can't write to this folder. Pick another one." / "No puedes escribir en esta carpeta. Elige otra.", needs_admin "This folder needs administrator rights. Pick one in your user folder or another drive." / "Esta carpeta necesita permisos de administrador. Elige una en tu carpeta de usuario o en otro disco.", not_empty "There's already a Perland Launcher folder here with other files. Pick another folder." / "Ya hay una carpeta Perland Launcher aquí con otros archivos. Elige otra carpeta.".
  - Uninstaller: window title "Uninstall Perland Launcher" / "Desinstalar Perland Launcher"; headline `UNINSTALL PERLAND LAUNCHER` / `DESINSTALAR PERLAND LAUNCHER`; "Removes the app from" / "Quita la app de"; "and its shortcuts, perland:// links and Settings entry." / "y sus accesos directos, enlaces perland:// y su entrada en Configuración."; checkbox "Also delete my games, worlds, settings and sign-ins" / "También borrar mis juegos, mundos, ajustes e inicios de sesión"; under it `{data path} · {size}` (size absent until known); buttons UNINSTALL / DESINSTALAR, UNINSTALL AND DELETE / DESINSTALAR Y BORRAR, "Cancel" / "Cancelar"; headline while removing `UNINSTALLING…` / `DESINSTALANDO…`, steps "Removing the app" / "Quitando la app", "Deleting your data" / "Borrando tus datos"; done headline `UNINSTALLED` / `DESINSTALADO`, "Perland Launcher is uninstalled." / "Perland Launcher está desinstalado.", kept: "Your games and worlds were kept in {data path}." / "Tus juegos y mundos se quedaron en {data path}.", button CLOSE / CERRAR; failed headline as the setup's error; code: "The uninstall didn't finish (code {n}). Try again from Settings → Apps." / "La desinstalación no terminó (código {n}). Vuelve a intentarlo desde Configuración → Aplicaciones."; not deleted: "The app is uninstalled, but these couldn't be deleted: {names}." / "La app está desinstalada, pero no se pudo borrar: {names}.", button "Copy details".
- Layout (logical px, no logo on Options): title Bungee 20 at y 40; "Install to" label 13 at 72; path 15 at 94 (`ellipsize_middle` to 440) with **Change…** secondary button right-aligned, 96×30, top 80; size or problem line 13 at 118 (problem in ink, prefixed by a 3px amber bar); data label 13 at 146, data path 15 at 166 (ellipsized to 560); list label 13 at 196, items 14 at 216/236/256/276 with a 4px bullet; checkboxes (18×18, 2px line border, amber fill and ground tick when on) at y 300 with labels, or the "stay as they are" line; primary 200×48 at top 336, x centred minus 50; **Back** text button right of it. Uninstall Confirm: logo as Ready; headline at 176; "Removes the app from" 13 at 204; path 15 at 224; second sentence 13 at 246; checkbox at 272 with label; data path · size 13 at 296; primary 260×48 at top 322, x 140 (wider for UNINSTALL AND DELETE / DESINSTALAR Y BORRAR), with the **Cancel** text button right of it, as Back on Options.
- `buttons()` returns checkboxes as `Button`s with their actions (`ToggleStartMenu`, `ToggleDesktop`, `ToggleDeleteData`) and the Options link and Change… as secondary buttons.

- [ ] **Step 1: Failing tests:** extend `buttons_inside_window`, `no_text_overflows`, `every_character_has_a_glyph` and `every_screen_has_words_in_both_languages` to the new screens (via `all_screens`); add `ellipsize_keeps_both_ends` (a 200-character path → width ≤ max, starts with `C:\Users`, ends with `Perland Launcher`), `long_paths_fit_options_view` (Ui with a 260-character install path in both languages: `no_text_overflows` holds), `options_link_on_ready` (`hit` on the link's rect → `Some(Options)`), `checkboxes_are_hit` (Options: `ToggleDesktop` rect hit → `Some(ToggleDesktop)`; Uninstall Confirm: `ToggleDeleteData`), `update_options_have_no_checkboxes`, `delete_button_label_follows_the_box` (Confirm with `delete_data: true` → primary label `UNINSTALL AND DELETE`), `no_overlap_in_options` (every pair of drawn rects — text lines' boxes and buttons — on Options in both languages doesn't intersect).
- [ ] **Step 2: Run** → FAIL. **Step 3: Implement.** **Step 4: Run** `cargo test -p perland-setup` → PASS; `cargo run -p perland-setup --example render` also writes `14-options-en/es`, `15-options-folder-problem`, `16-options-update`, `17-uninstall-confirm`, `18-uninstall-delete-ticked`, `19-uninstalling`, `20-uninstalled-kept`, `21-uninstall-failed`; open each and check.
- [ ] **Step 5: Commit** `feat(setup): draw the options view and the uninstaller`.

### Task 5: Windows shell for options and the uninstaller

**Files:** Modify `src/win/mod.rs`, `src/win/system.rs`, `src/win/a11y.rs`, `Cargo.toml` (`rfd = { version = "0.15", default-features = false }` under the Windows target; windows-sys features `Win32_Security_Credentials`, `Win32_UI_Shell`, `Win32_System_Com`); create `src/bin/perland-uninstall.rs`; move the setup's entry to `src/main.rs` unchanged.

**Interfaces:**
- `pub fn run_uninstall() -> i32` in `win/mod.rs`, dispatching on `uninstall::mode(&args, exe_dir)`:
  - `Handoff` → run `<dir>\uninstall.exe` with the args, wait, return its code; no window, no log beyond a line.
  - `Silent` → `launcher_running()` → 2; else run with `uninstaller_args`, wait, `finish_removal(dir)`, return the code.
  - `Relaunch` → copy own exe to a fresh `%TEMP%\perland-uninstall-<16 hex>\perland-uninstall.exe` (reuse `PayloadDir::write` with the exe's bytes), start it with `--from-temp <dir>`, return 0. If the copy fails, fall into `Window` in place (removal then leaves `perland-uninstall.exe` behind; logged).
  - `Window` → the same event loop as the setup, driving `UMachine` (generalise `App` over a small `trait Flow { fn on(&mut self, Event-or-UEvent) … }` or a two-variant enum; builder's call, as long as `win::run()` stays as it is for the setup). Effects: `RunUninstaller` → thread runs `uninstall.exe` with `uninstaller_args(dir)`, then `finish_removal(dir)`, sends `UninstallerExited(code)`; `DeleteData` → thread runs `delete_paths(data_targets(..))` plus `system::delete_credentials()` (a failure adds "Saved sign-ins" / "Inicios de sesión guardados"), sends `DataDeleted`; on window open a thread sends `DataSize(dir_size(data))`; `Exit` → spawn `cleanup_command(own temp dir)` hidden (`CREATE_NO_WINDOW`) when running from temp, then exit.
- `fn finish_removal(dir: &Path)`: delete `uninstall.exe` and `perland-uninstall.exe` in `dir` (missing is fine), then `remove_dir` (non-recursive) — log, never fail.
- `system::delete_credentials() -> Result<usize, String>`: `CredEnumerateW(credential_filter("online.perland.launcher"))`, `CredDeleteW` each (generic type).
- `system::known_folders() -> (PathBuf, PathBuf, PathBuf, PathBuf)`: LocalAppData, RoamingAppData, Desktop, Programs (Start menu) via `SHGetKnownFolderPath`; `protected_roots()` from `FOLDERID_ProgramFiles`, `ProgramFilesX86`, `Windows`.
- Setup side: `PickFolder` → `rfd::FileDialog::new().set_directory(default parent).pick_folder()` on the UI thread, then `FolderPicked`; `ResolveFolder(p)` → thread `resolve_folder(&p, &protected_roots())` → `FolderResolved`; `RunInstaller` uses `installer_args(&machine.choice, machine.fresh())`; `RemoveShortcuts` deletes `shortcuts_to_remove(...)` (logged). `Ui.paths` filled from `known_folders()`, the registry's current folder, `installed_bytes()`.
- a11y: checkbox nodes `Role::CheckBox` with `set_toggled(Toggled::True/False)`; the Options link and Change… as buttons; the problem line in the live status.
- Window title for the uninstaller: "Uninstall Perland Launcher" (single-instance mutex name `Local\PerlandLauncherUninstall`).

- [ ] **Step 1:** Implement; `cargo clippy -p perland-setup --target x86_64-pc-windows-msvc --all-targets -- -D warnings` clean (Windows layer: no unit tests, as before; its decisions are Tasks 1–4).
- [ ] **Step 2:** `cargo test --workspace` and Linux clippy pass; `cargo build -p perland-setup --bins` builds both binaries on Linux (they print and exit).
- [ ] **Step 3: Commit** `feat(setup): options and uninstaller in the Windows shell`.

### Task 6: Hook, release workflow and docs

**Files:** Create `src-tauri/windows/hooks.nsh`; modify `.github/workflows/release.yml`, `README.md` (Releasing line), `NOTICE` (none needed), `docs/superpowers/specs/...` (no change).

- `hooks.nsh`:
  ```nsis
  !macro NSIS_HOOK_POSTINSTALL
    IfFileExists "$INSTDIR\perland-uninstall.exe" 0 +3
      WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\perland-uninstall.exe$\""
      WriteRegStr SHCTX "${UNINSTKEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  !macroend
  ```
- `release.yml` Windows job, before "Build and sign the updater artifacts": a step `Build the uninstaller` (`cargo build -p perland-setup --release --bin perland-uninstall`, signed like the setup when signing is on); the build step adds, on Windows only, `--config '{"bundle":{"resources":{"../target/release/perland-uninstall.exe":"perland-uninstall.exe"},"windows":{"nsis":{"installerHooks":"windows/hooks.nsh"}}}}'`. The setup step sets `PERLAND_SETUP_INSTALLED_BYTES` to the byte sum of `target/release/perland-launcher.exe` and `target/release/perland-uninstall.exe`.
- Verification: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"`; the `--config` JSON parses (`node -e 'JSON.parse(process.argv[1])' '<json>'`).
- [ ] **Step 1:** Write the hook and workflow changes. **Step 2:** Verify as above. **Step 3: Commit** `ci(release): bundle the branded uninstaller and point Windows at it`.

### Task 7: Renders, desktop mock-ups, PR and final checks

- [ ] Full pre-commit list; renders of every new screen in both languages, opened and checked.
- [ ] Desktop mock-ups (as before): the Options view open, the uninstaller from a simulated Settings → Apps, the uninstall done screen.
- [ ] Publish to `pr-assets/branded-setup` (a new commit; links by SHA), update PR #143's body (screens, what changed, the extended Windows checklist from spec §7), send the images to the owner.
