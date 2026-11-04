# Branded one-click setup for Windows

- **Date:** 2026-11-04
- **Status:** approved 2026-11-04; amended the same day with install options (§4a) and a branded uninstaller (§4b, §6.7), waiting for the owner's approval of the amendment
- **Related:** `docs/superpowers/specs/2026-10-14-distribution-design.md` (§3.5 installers, §4.7 updater), `DESIGN.md` (the launcher's look), the brand kit in `PerlandDev/perland-brand`.

## 1. Why

A player's first contact with Perland Launcher is the Windows installer. Today that is Tauri's stock NSIS wizard: grey Windows pages, the default NSIS icon, nothing that says Perland. The owner wants the first run to look and feel like Perland, and like the launcher it is about to open: **one window, one click, then the launcher opens**, in the spirit of Discord's or Riot's setup.

## 2. Goals, non-goals, anti-goals

**Goals**
- A new Windows download, the **setup app**: one branded window with an Install button, a progress bar and an automatic hand-off to the launcher.
- It looks like the launcher. It uses the launcher's dark palette and amber primary from `DESIGN.md`, Bungee for display text, Atkinson Hyperlegible for body text, and the pinwheel from the brand kit as the logo.
- English and Spanish, following the Windows display language, the same rule the NSIS installer uses today.
- No admin prompt, per-user install, as today.
- Keyboard use and screen reader names for the window's controls.
- **Install options for those who want them** (owner, amendment): the install folder, the two shortcuts, and plain information on what gets installed and where, including where the player's games and worlds live. Install stays one click; the options are one link away.
- **A branded uninstaller** in the same window and look (owner, amendment): what it removes and where, and a clear choice to also delete the player's games, worlds, settings and saved sign-ins.

**Non-goals**
- Updates. They keep running the plain NSIS installer in passive mode, exactly as today (`latest.json`, `/R` relaunch, install-on-quit).
- Linux. AppImage and `.deb` have no installer UI.
- Options beyond the folder and the two shortcuts (start with Windows, file associations, per-machine installs).
- Changing the download page (PerlandDev-site) or the private dev repository's dev installer. Both are follow-ups once a release carries the setup app.
- Code signing. It stays dormant, as for the NSIS installer; the setup app gets the same hook (§6.4).

**Anti-goals**
- **Any change that can break updates for existing players.** The NSIS template and the updater configuration are not touched. The one addition on that path is a Tauri installer hook for the branded uninstaller (§6.7), and only in release builds.
- An uninstaller that deletes the player's games, worlds or sign-ins without being asked, or that says it deleted them when it didn't.
- An options view that hides the one-click path or makes Install wait on a choice.
- A setup that kills a running launcher, or a running game, without asking.
- A setup that claims success when the launcher is not installed.
- A setup that asks for administrator rights.
- Using the pinwheel's plot colours as UI colours (the brand rule). They appear only inside the logo.

## 3. Approaches considered

1. **Branded NSIS wizard.** Tauri's `installerIcon`, `headerImage` and `sidebarImage` on today's wizard. Cheap, but it still looks like a Windows wizard. Not chosen: it isn't one-click.
2. **One-click NSIS (our own copy of Tauri's template).** One borderless NSIS page with bitmap art. It stays a single `.exe`, but NSIS draws with old Windows controls (no smooth shapes, fonts or animation outside bitmaps), and the updater runs that same template, so a mistake breaks updates for every player and every Tauri upgrade needs a re-check of the fork. Not chosen.
3. **A small Rust setup app that wraps today's NSIS installer.** **Chosen by the owner.** It draws its own window and runs the unchanged NSIS installer silently. The best-looking result, and the update path is untouched. The costs are a second `.exe` in each release (and later, a second file to sign), a new step in the release workflow, and coarse progress, because silent NSIS reports no percentage.
4. **Our own installer replacing NSIS.** Ruled out: Tauri's updater on Windows installs only NSIS or MSI.

For the uninstaller (amendment):

5. **Brand Tauri's NSIS uninstaller** with the supported `installerIcon` / `headerImage` / `sidebarImage` settings and fix its data checkbox with a hook. Low risk, but it stays a Windows wizard. Not chosen.
6. **Our own branded uninstaller**, shipped inside the app; a Tauri installer hook makes Windows' Uninstall button open it, and it runs NSIS's uninstaller silently. **Chosen by the owner.** It touches the update path through the hook (§6.7), which needs a Windows test of install, update and uninstall.

## 4. What the player sees

The window is 640 × 400 logical pixels, borderless, centred, not resizable, on the launcher's ground colour. A thin title strip holds a drag area and a close button (×). Sketches:

**Ready** (nothing installed, or an older version)
```
┌──────────────────────────────────────────────────────────┐
│                                                       ×  │
│                                                          │
│                     [pinwheel 96px]                      │
│                                                          │
│                   PERLAND LAUNCHER                       │  Bungee, ink
│        Play Minecraft on Perland servers, in one click.  │  Atkinson, ink-dim
│                                                          │
│                    ┌──────────────┐                      │
│                    │   INSTALL    │                      │  amber button, Bungee
│                    └──────────────┘                      │
│                                                          │
│  Version 0.2.0 · Installs for you only, no admin needed  │  small, ink-dim
└──────────────────────────────────────────────────────────┘
```
With an older version installed the button reads **UPDATE** and the small line names both versions.

**Already installed**: the title reads "Perland Launcher is installed" and the primary button is **OPEN**. With the same version installed, a quiet secondary text button reads **Reinstall** (a repair). With a **newer** version installed there is no Reinstall: the setup app never downgrades (the launcher's data may already be in a newer format, and the updater would only bring the newer version back). *Our call.*

**Installing**
```
│                     [pinwheel 96px]                      │
│                   INSTALLING…                            │
│   ███████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    │  amber on raised, 6px tall
│                   Copying files                          │  step text, ink-dim
```
Steps: *Getting ready* (extracting the installer), *Installing* (NSIS running; this is also where WebView2 downloads on an older Windows 10 without it), *Finishing up*. During *Getting ready* the close button asks "Stop the install?" in the window, and yes deletes the extracted payload and closes. Once the NSIS installer is running it is never killed (half an update could break a working install): the close button is disabled with the hint "Almost there, this can't be stopped now", and Esc does nothing. *Our call.*

**Done**: "Opening Perland Launcher…", the launcher starts, and the setup window closes about a second later. If the launcher can't be started, the window stays with "Perland Launcher is installed" and an **OPEN** button.

**Launcher running** (before installing or updating): "Close Perland Launcher to continue. If a game is running, quit it first." with a **TRY AGAIN** button. The setup app never ends the process itself.

**Error**: a plain sentence per error (§6.6), **TRY AGAIN**, and a quiet **Copy details** button that copies the log path and the error code.

### 4a. Install options

The Ready screen's footer gets a quiet **Options** link (keyboard reachable, a button for screen readers) after the version line. It opens an Options view in the same window; nothing else changes about the one-click path.

```
│  INSTALL OPTIONS                                          │
│  Install to   C:\Users\ana\AppData\Local\Perland Launcher    │
│               [Change…]                                    │
│               About 25 MB · For you only, no admin needed  │
│  Your games, worlds and settings stay in                   │
│               C:\Users\ana\AppData\Roaming\PerlandLauncher    │
│  What gets installed                                       │
│   · Perland Launcher, and its uninstaller                  │
│   · Microsoft WebView2, only if Windows doesn't have it    │
│   · perland:// links, so invite pages open the launcher    │
│   · An entry in Settings → Apps                            │
│  [✓] Start menu shortcut   [✓] Desktop shortcut            │
│                 ┌──────────────┐                           │
│                 │   INSTALL    │   Back                    │
```

- **Folder:** only on a fresh install. Update and Reinstall show the current folder read-only ("Updates keep the folder it's in"), because the installer reuses the recorded folder. **Change…** opens Windows' folder picker. The chosen folder is checked before Install is allowed: it must be writable by this user without admin (a test file is created and removed), must not be inside `Program Files` or `Windows` (they need admin), and must not be an existing non-empty folder unless it already holds Perland Launcher, in which case the setup appends `Perland Launcher` to it. A refused folder says why under the field and Install stays off.
- **Paths are the player's real paths**, read from Windows, never placeholders.
- **Size:** "About N MB", from the launcher's installed size measured at release build time and embedded in the setup.
- **Shortcuts:** both ticked by default. Unticking both skips them; unticking one removes that one after the install.
- Back and Esc return to the Ready screen keeping the choices. The view fits 640 × 400; if the content is taller in a language, the window grows to fit rather than scrolling.

### 4b. The uninstaller

Windows' Settings → Apps → Uninstall (and the Start menu's Uninstall) opens a branded window in the same look:

```
│                     [pinwheel 96px]                       │
│                UNINSTALL PERLAND LAUNCHER                 │
│  Removes the app from                                     │
│    C:\Users\ana\AppData\Local\Perland Launcher                │
│  and its shortcuts, perland:// links and Settings entry.  │
│  [ ] Also delete my games, worlds, settings and sign-ins  │
│      C:\Users\ana\AppData\Roaming\PerlandLauncher · 3.2 GB      │
│                 ┌──────────────┐                          │
│                 │  UNINSTALL   │   Cancel                 │
```

- The data box is **unticked by default**. Ticked, it names what will go and the folder's size, and the button reads **UNINSTALL AND DELETE**.
- Before anything happens it checks the launcher isn't running (same screen and wording as the setup's).
- Steps: *Removing the app*, then *Deleting your data* when asked. Done: "Perland Launcher is uninstalled." (and "Your games and worlds were kept in …" when they were). An error says what was and wasn't removed, with Copy details.
- English and Spanish, keyboard and screen reader as the setup.

Keys: Enter presses the primary button, Esc closes (asks during Getting ready, does nothing while NSIS runs), Tab moves between the buttons, and the focused button gets a visible ring.

## 5. Look

- Colours from `DESIGN.md`: ground `#181410`, raised `#2e261f`, line `#463a2f`, ink `#f6f1e7`, ink-dim `#c5b9a8`, amber `#ffc233`, amber-hi `#ffd76e` (hover). Corner radius 6px (`rounded.panel`).
- The pinwheel is the brand kit's `logo/icon` **on-dark** version, vendored as PNG at 512 px and scaled down with good filtering. The brand rules apply: no recolouring, glow or effects.
- Fonts: Bungee and Atkinson Hyperlegible Next, vendored as TTF under the crate's `assets/` with their OFL licence files, and listed in `NOTICE`.
- Motion: the progress bar eases; the Done screen fades out over about 200 ms. Nothing else moves.
- The window and taskbar icon is `src-tauri/icons/icon.ico`.

## 6. How it works

### 6.1 Crate and parts

A new workspace member, `crates/perland-setup` (binary `perland-setup`). It must not depend on Tauri or `launcher-core`. It is split so most of it builds and is tested on every platform:

- `model`: the screen state machine (Ready, AlreadyInstalled, LauncherRunning, Installing{step, progress}, Done, Error), the decision from the installed version and this payload's version (semver compare), and progress easing. Pure, unit tested.
- `text`: every string in English and Spanish (neutral Latin American Spanish, informal "tú"), picked from the Windows UI language (`es*` is Spanish, anything else English). Pure, unit tested; a test checks both languages have every key.
- `draw`: lays out and draws a screen into a pixel buffer with `tiny-skia` (shapes, the logo) and a TTF rasteriser (`fontdue` or `ab_glyph`, the builder's pick). Platform-neutral. An example (`cargo run -p perland-setup --example render`) writes every screen in both languages to PNG at 1× and 2×, for review and for the PR's screenshots.
- `windows` (`cfg(windows)`): the window (`winit` + `softbuffer`), DPI scaling, input, AccessKit (`accesskit_winit`) for the buttons and the status text, the registry reads, process checks, running the payload and launching the launcher.

On Linux and macOS the binary prints that it is for Windows and exits, so `cargo build/test/clippy --workspace` keeps working on Linux CI.

### 6.2 The payload

The NSIS installer is embedded at build time. `build.rs` reads `PERLAND_SETUP_PAYLOAD` (a path to the built `*_x64-setup.exe`) and the version from the workspace; without the variable (local builds, tests) it embeds nothing, and the setup app's Install button shows the "no installer inside" error. The release workflow always sets it.

At install time the payload is written to a fresh folder under `%TEMP%` (`perland-setup-<random>`), run, and the folder is removed afterwards, also on failure.

### 6.3 Running the installer

- Run `<payload> /S` and wait for it. Exit code 0 is success; anything else is an error carrying the code.
- Before running: detect a running `perland-launcher.exe` (from the installed folder or anywhere) and show the "Launcher running" screen instead. The builder must check what Tauri's NSIS template (for the tauri-bundler version the workspace resolves) does with a running app in silent mode and make sure the setup app's check comes first.
- Installed version and folder: read from the registry keys Tauri's NSIS template writes for a per-user install (the builder confirms the exact keys from the template). The same read after the install confirms success and gives the path of `perland-launcher.exe` to start.
- Progress: Getting ready runs to 15% as the payload is written; Installing eases from 15% toward 90% over an expected 20 s and never passes 90% until the process exits; Finishing up goes to 100% once the registry read confirms the install. The text, not the bar, is what is promised.
- Launch: start the installed exe detached, with no arguments, then close.
- `/S` on the setup app's own command line runs the payload silently with no window (for admins and scripts). It applies the same checks as the window: a running launcher is never closed, it exits with code `2` instead; a newer installed version exits `0` without touching it; otherwise it exits with the NSIS installer's code (`0` success). It never starts the launcher afterwards. *Our call.*

### 6.4 The `.exe` itself

- Embedded Windows manifest: `requestedExecutionLevel asInvoker` (Windows guesses "needs admin" for files named like installers without one), per-monitor DPI awareness v2, common controls v6.
- Version info (product name, version, company Perland, file description "Perland Launcher Setup") and the icon, through `winresource` or `embed-resource`.
- GUI subsystem (no console window).
- Signing: when `AZURE_TRUSTED_SIGNING` is on, the release workflow signs the setup app after building it, with the same command the NSIS installer uses (the payload is already signed by `tauri build`).

### 6.4a Install options (implementation)

- The folder picker is Windows' own (`IFileOpenDialog` with `FOS_PICKFOLDERS`, or the `rfd` crate; builder's call).
- A chosen folder reaches NSIS as `/D=<path>`, which must be the **last** argument and unquoted, even with spaces (NSIS's rule). Tauri's template keeps a `/D` folder (it sets the default only when `$INSTDIR` is its placeholder) and records it under `HKCU\Software\Perland\Perland Launcher`, which every later update reads, so updates follow the folder.
- Shortcuts: both unticked → `/NS`. One unticked → after the install, the setup deletes that `.lnk` (`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Perland Launcher.lnk` or the desktop's). Updates run with `/UPDATE` and never recreate shortcuts.
- Size: the release workflow measures the installed size and passes it as `PERLAND_SETUP_INSTALLED_BYTES`; without it the size line is left out.
- The folder rules and the argument building are pure functions with unit tests.

### 6.4b Logging

A plain log at `%TEMP%\perland-setup.log` (overwritten each run): steps, versions, exit codes, paths. No tokens or personal data exist here to leak.

### 6.7 The uninstaller (implementation)

- **A second binary** in the same crate, `perland-uninstall.exe`, sharing `draw`, `text` and the Windows shell, with its own state machine (`model::uninstall`). It never embeds the NSIS payload.
- **How it gets on disk:** the release workflow builds it before `tauri build` and adds, through `--config` (so local builds and `tauri dev` are unchanged), `bundle.resources` placing it in the install folder, and `bundle.windows.nsis.installerHooks` pointing at `src-tauri/windows/hooks.nsh`. The hook's `NSIS_HOOK_POSTINSTALL`, when `$INSTDIR\perland-uninstall.exe` exists, writes the uninstall key's `UninstallString` to it and `QuietUninstallString` to `"$INSTDIR\uninstall.exe" /S`. It runs on every install and update, so the key always points at the uninstaller of the version installed; a build without the file leaves Tauri's values.
- **Running from the install folder:** a running exe can't be deleted, so the uninstaller copies itself to a fresh `%TEMP%` folder and restarts from there, passing the install folder. That copy can't delete itself either, so as it exits it starts a hidden, detached `cmd` that waits a few seconds and removes its temp folder (best effort, logged).
- **Removing the app:** it runs `"$INSTDIR\uninstall.exe" /S _?=$INSTDIR` and waits (with `_?=` NSIS runs in place instead of re-launching from temp and returning at once), then removes `uninstall.exe` and the empty folder itself. Exit code 0 is success.
- **Deleting data, only when ticked:** `%APPDATA%\PerlandLauncher` (games, worlds, settings, logs), the webview's `%APPDATA%\dev.perland.launcher` and `%LOCALAPPDATA%\dev.perland.launcher`, and every Windows Credential Manager entry the launcher saved (service `online.perland.launcher`: the Perland account session and the Microsoft refresh tokens). A failure to delete any of these is reported by name, never silently skipped.
- **The data folder size** is measured in the background when the window opens and shown when known.
- `/S` on the uninstaller removes the app silently and never deletes data.

### 6.5 Release workflow

In the Windows build job, after `pnpm tauri build`: find the NSIS installer, `cargo build -p perland-setup --release` with `PERLAND_SETUP_PAYLOAD` pointing at it, sign if signing is on, and stage it. Names (new, alongside today's):

- Versioned release asset: `PerlandLauncher_<version>_x64-installer.exe`.
- Channel fixed-name copy, following today's `channelName` pattern (no word for stable): `PerlandLauncher-installer.exe` for stable, `PerlandLauncher-beta-installer.exe` for beta. These become the download page's lasting links. *Our call, for consistency with the existing names.*

`latest.json` is unchanged and never names the setup app.

Before `tauri build` on Windows: `cargo build -p perland-setup --release --bin perland-uninstall`, then `tauri build` with the extra `--config` of §6.7. The installed size for §6.4a is the size of `target/release/perland-launcher.exe` plus `perland-uninstall.exe` (the app has no other installed files), measured after `tauri build` and passed to the setup app's build. Its own `.sig` is not needed (the updater never fetches it). The naming helpers live in `scripts/release/lib.mjs` with `node:test` tests, like today's.

### 6.6 Errors

Each has a stable code and a sentence in both languages: `payload_missing` (built without an installer), `temp_unwritable`, `installer_failed` (with the exit code), `install_not_found` (exit 0 but nothing in the registry), `launch_failed` (installed, couldn't open: shows OPEN instead of an error), `launcher_running` (its own screen). An unexpected failure is `unknown` with the log path.

## 7. Testing

- Unit tests (`#[cfg(test)]`) for the state machine (including no downgrade and no stop once NSIS runs), version decisions, the `/S` exit codes, progress easing, strings, layout (buttons inside the window, text not overflowing at either language's longest string), payload write and cleanup (to a temp dir), exit-code mapping, and the release-name helpers.
- The render example's PNGs are reviewed by the owner before merge and go into the PR body.
- Unit tests for the options (folder rules, `/D` and `/NS` building, which shortcut to remove) and the uninstaller's state machine and data list.
- A manual end-to-end on Windows from a dry-run release also covers: install to a custom folder, then an in-app update lands there; one shortcut unticked; Program Files refused; uninstall from Settings → Apps keeping data, then again deleting data (folder, webview data and Credential Manager entries gone); uninstall while the launcher runs; no leftover install or temp folders.
- The original manual end-to-end on Windows from a dry-run release: fresh install, already installed (same and newer), update over an older version, launcher running (window and `/S`), close during Getting ready and while installing, Spanish Windows, 150% scaling, Narrator reading the buttons; then an in-app update still works afterwards.

## 8. Left to the builder

- The TTF rasteriser crate and the exact `winit` / `softbuffer` / AccessKit versions.
- The exact registry keys and the running-app behaviour of Tauri's NSIS template, confirmed from the resolved `tauri-bundler` source.
- Final copy wording within the meaning above, and spacing within the sketches.
