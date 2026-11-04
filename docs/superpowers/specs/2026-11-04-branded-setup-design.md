# Branded one-click setup for Windows

- **Date:** 2026-11-04
- **Status:** design, waiting for the owner's approval
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

**Non-goals**
- Updates. They keep running the plain NSIS installer in passive mode, exactly as today (`latest.json`, `/R` relaunch, install-on-quit).
- The uninstaller. It stays Tauri's NSIS uninstaller.
- Linux. AppImage and `.deb` have no installer UI.
- Choosing an install folder or any other option. One click means none.
- Changing the download page (PerlandDev-site) or the private dev repository's dev installer. Both are follow-ups once a release carries the setup app.
- Code signing. It stays dormant, as for the NSIS installer; the setup app gets the same hook (§6.4).

**Anti-goals**
- **Any change that can break updates for existing players.** The NSIS installer, its template and the updater configuration are not touched.
- A setup that kills a running launcher, or a running game, without asking.
- A setup that claims success when the launcher is not installed.
- A setup that asks for administrator rights.
- Using the pinwheel's plot colours as UI colours (the brand rule). They appear only inside the logo.

## 3. Approaches considered

1. **Branded NSIS wizard.** Tauri's `installerIcon`, `headerImage` and `sidebarImage` on today's wizard. Cheap, but it still looks like a Windows wizard. Not chosen: it isn't one-click.
2. **One-click NSIS (our own copy of Tauri's template).** One borderless NSIS page with bitmap art. It stays a single `.exe`, but NSIS draws with old Windows controls (no smooth shapes, fonts or animation outside bitmaps), and the updater runs that same template, so a mistake breaks updates for every player and every Tauri upgrade needs a re-check of the fork. Not chosen.
3. **A small Rust setup app that wraps today's NSIS installer.** **Chosen by the owner.** It draws its own window and runs the unchanged NSIS installer silently. The best-looking result, and the update path is untouched. The costs are a second `.exe` in each release (and later, a second file to sign), a new step in the release workflow, and coarse progress, because silent NSIS reports no percentage.
4. **Our own installer replacing NSIS.** Ruled out: Tauri's updater on Windows installs only NSIS or MSI.

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

**Already installed** (same or newer version): the title reads "Perland Launcher is installed", the primary button is **OPEN**, and a quiet secondary text button reads **Reinstall**.

**Installing**
```
│                     [pinwheel 96px]                      │
│                   INSTALLING…                            │
│   ███████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    │  amber on raised, 6px tall
│                   Copying files                          │  step text, ink-dim
```
Steps: *Getting ready* (extracting the installer), *Installing* (NSIS running; this is also where WebView2 downloads on an older Windows 10 without it), *Finishing up*. While installing, the close button asks "Stop the install?" in the window before it does anything.

**Done**: "Opening Perland Launcher…", the launcher starts, and the setup window closes about a second later. If the launcher can't be started, the window stays with "Perland Launcher is installed" and an **OPEN** button.

**Launcher running** (before installing or updating): "Close Perland Launcher to continue. If a game is running, quit it first." with a **TRY AGAIN** button. The setup app never ends the process itself.

**Error**: a plain sentence per error (§6.6), **TRY AGAIN**, and a quiet **Copy details** button that copies the log path and the error code.

Keys: Enter presses the primary button, Esc closes (or asks, while installing), Tab moves between the buttons, and the focused button gets a visible ring.

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
- `/S` on the setup app's own command line runs the payload silently with no window (for admins and scripts) and exits with its code.

### 6.4 The `.exe` itself

- Embedded Windows manifest: `requestedExecutionLevel asInvoker` (Windows guesses "needs admin" for files named like installers without one), per-monitor DPI awareness v2, common controls v6.
- Version info (product name, version, company Perland, file description "Perland Launcher Setup") and the icon, through `winresource` or `embed-resource`.
- GUI subsystem (no console window).
- Signing: when `AZURE_TRUSTED_SIGNING` is on, the release workflow signs the setup app after building it, with the same command the NSIS installer uses (the payload is already signed by `tauri build`).

### 6.4a Logging

A plain log at `%TEMP%\perland-setup.log` (overwritten each run): steps, versions, exit codes, paths. No tokens or personal data exist here to leak.

### 6.5 Release workflow

In the Windows build job, after `pnpm tauri build`: find the NSIS installer, `cargo build -p perland-setup --release` with `PERLAND_SETUP_PAYLOAD` pointing at it, sign if signing is on, and stage it. Names (new, alongside today's):

- Versioned release asset: `PerlandLauncher_<version>_x64-installer.exe`.
- Channel fixed-name copy: `PerlandLauncher-<channel>-installer.exe`.

`latest.json` is unchanged and never names the setup app. Its own `.sig` is not needed (the updater never fetches it). The naming helpers live in `scripts/release/lib.mjs` with `node:test` tests, like today's.

### 6.6 Errors

Each has a stable code and a sentence in both languages: `payload_missing` (built without an installer), `temp_unwritable`, `installer_failed` (with the exit code), `install_not_found` (exit 0 but nothing in the registry), `launch_failed` (installed, couldn't open: shows OPEN instead of an error), `launcher_running` (its own screen). An unexpected failure is `unknown` with the log path.

## 7. Testing

- Unit tests (`#[cfg(test)]`) for the state machine, version decisions, progress easing, strings, layout (buttons inside the window, text not overflowing at either language's longest string), payload write and cleanup (to a temp dir), exit-code mapping, and the release-name helpers.
- The render example's PNGs are reviewed by the owner before merge and go into the PR body.
- A manual end-to-end on Windows from a dry-run release: fresh install, already installed, update over an older version, launcher running, Esc while installing, Spanish Windows, 150% scaling, Narrator reading the buttons; then an in-app update still works afterwards.

## 8. Left to the builder

- The TTF rasteriser crate and the exact `winit` / `softbuffer` / AccessKit versions.
- The exact registry keys and the running-app behaviour of Tauri's NSIS template, confirmed from the resolved `tauri-bundler` source.
- Final copy wording within the meaning above, and spacing within the sketches.
