# Perland Launcher: Distribution (Plan 6), Design Addendum

- **Date:** 2026-10-14
- **Status:** Sections approved by the owner in brainstorming (2026-10-14); written spec awaiting owner review
- **Parent spec:** `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (§4 offline accounts, §5 Settings "launcher updates", §6 logs, diagnostics, self-update, installers, §7 release checklist).
- **Related:** `docs/superpowers/specs/2026-10-11-join-by-link-design.md` (installer-time `perland://` registration was deferred to this plan). **Design system:** `DESIGN.md`, `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Second repository:** `PerlandDev/PerlandDev-site` (Astro), page `/launcher` and `/es/launcher`.

## 1. Intent

The Engine is in closed beta, and every invite page now offers a Perland Launcher tab whose "don't have it yet?" link goes to `https://perland.online/launcher` (the Engine's `LAUNCHER_DOWNLOAD_URL`). The launcher must be downloadable from there. Testers today download a dev-mode `.exe` workflow artifact: it needs a GitHub login, expires after 30 days, has no installer, never updates, and registers `perland://` only when it first runs.

After this plan, a tester:

1. clicks the download on perland.online/launcher (no GitHub login);
2. installs per user, with no admin prompt, getting past SmartScreen with the page's guidance;
3. has `perland://join` links open the installed launcher;
4. adds an offline account (Microsoft sign-in, Plan 4, is still paused);
5. gets newer betas automatically: downloaded on start, installed on quit, never while a game runs.

### 1.1 Success criteria

- The Release workflow, run from the Actions tab, publishes a beta to GitHub Releases (pre-release) with a Windows NSIS installer, a Linux AppImage and a `.deb`, all with Ed25519 updater signatures.
- On Windows: installing needs no admin prompt; an invite page's Perland Launcher button opens the installed app through `perland://join`; offline accounts can be added.
- Publishing a newer beta: the installed launcher shows "Update ready" on its next start, installs when closed, and reopens on the new version with worlds and accounts intact.
- Settings → Launcher updates switches between Stable and Beta, shows the state, and offers Check now and Restart now. Every new string is in English and Spanish.
- The launcher writes its own log, rotated daily and kept 14 days, with the home path redacted and no tokens. **Export diagnostics** saves a zip of logs, `instance.json`, `managed.json`, `settings.json` and system info, with tokens and the home path redacted and without `accounts.json`.
- perland.online/launcher (en and es) offers the beta download and SmartScreen guidance. `LAUNCHER_DOWNLOAD_URL` still points at that page.
- `launcher-core` and `launcher-app` still have no Tauri or UI dependency.

### 1.2 Out of scope

- macOS.
- Authenticode signing. The pipeline has a dormant hook for Azure Trusted Signing (§3.5); builds ship unsigned.
- Microsoft sign-in (Plan 4, paused) and therefore any useful **stable** release: a Release build cannot add an account until Plan 4. The stable channel exists but stays empty.
- Flatpak, MSI, portable zips.
- Telemetry or uploading diagnostics anywhere.
- Carry-forwards split into a follow-up hardening session (§6.2).

## 2. Owner decisions (2026-10-14)

1. **Public beta: accepted.** Beta builds, with offline accounts open, are published publicly as GitHub **pre-releases** and linked from perland.online/launcher. The repository is GPL and public, so the offline gate is a policy for what Perland ships, not a security boundary.
2. **Beta gate: `BuildKind::Beta`, chosen at compile time.** Not a Cargo feature (features unify across the workspace and are easy to switch on by accident), and not tied to the update channel (a stable player could flip one toggle past the gate).
3. **Update UX: quiet note plus Restart now.** Download silently on start; a quiet note says the update is ready and installs on close, with **Restart now**, unavailable while anything runs.
4. **Release trigger: one-click Actions workflow.** Nothing publishes unless the owner runs it. It computes the version, tags, builds, signs and publishes.
5. **Carry-forwards:** this plan takes the public-release ones (metadata body caps, Windows argfile encoding, the mrpack corrupt-entry code). The rest go to a separate hardening session.
6. **Manifests: rolling GitHub releases** `channel-beta` and `channel-stable` (§3.3), not perland.online or the GitHub API.
7. **Tester-build workflow: retired.** The Release workflow's dry run replaces it.
8. **End-to-end test releases are published from the PR branch, before merge.** Only the beta channel may publish from a branch other than `main`; stable is `main`-only.
9. **Download page: keep the waitlist form** lower on the page, reworded to "get notified when it's out of beta"; the hero call to action becomes the beta download.

## 3. Builds and releases

### 3.1 Build kinds

`launcher_core::accounts::BuildKind` becomes `Dev | Beta | Release`:

```rust
pub fn current() -> Self {
    if cfg!(debug_assertions) {
        Self::Dev
    } else if option_env!("PERLAND_BUILD") == Some("beta") {
        Self::Beta
    } else {
        Self::Release
    }
}
```

- The choice is pulled into a pure `fn from_parts(debug: bool, flag: Option<&str>) -> BuildKind` so it can be unit tested; `current()` calls it.
- `offline_allowed(build, licensed_signin_seen)` is `true` for `Dev` and `Beta`, and for `Release` only after a licensed sign-in (unchanged).
- No build script is needed: rustc records `option_env!` variables as dependencies, so cargo rebuilds `launcher-core` when `PERLAND_BUILD` changes.
- The IPC `Build` enum gains `beta`. `ui/src/ipc/schema.ts` and the generated contract follow (`UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`). Settings' About line shows "· beta build" for Beta as it shows "· dev build" for Dev.
- Dev builds never check for updates (§4).

The parallel "Fix beta feedback" session turns off the webview context menu and DevTools in non-dev builds. Beta builds have no debug assertions, so they get that behaviour without changes.

### 3.2 Version

- The workspace `Cargo.toml` `version` is the single source of truth. `src-tauri/tauri.conf.json` drops its `version` field so Tauri uses the Cargo package version.
- `main` carries the **next base version**. This plan bumps it from `0.1.0` to `0.2.0`, so the first beta is `0.2.0-beta.1`. After a stable release the owner bumps it in a normal PR.
- Release versions:
  - beta: `<base>-beta.<N>`, where `N` is one more than the highest existing `v<base>-beta.*` tag (starting at 1);
  - stable: `<base>`; the workflow fails if `v<base>` already exists.
- The workflow writes the computed version into `Cargo.toml` (and `Cargo.lock`) inside the runner only; it is never committed. The tag `v<version>` points at the built commit.
- The calculation lives in `scripts/release/version.mjs` (pure function plus a thin CLI), unit tested with `node:test`.

### 3.3 Channels and manifests

Two rolling releases hold each channel's Tauri updater manifest and a fixed-name copy of the newest installers:

| Release tag | Holds |
|---|---|
| `channel-beta` | `latest.json`, `PerlandLauncher-beta-setup.exe`, `PerlandLauncher-beta.AppImage`, `PerlandLauncher-beta.deb` |
| `channel-stable` | `latest.json`, `PerlandLauncher-setup.exe`, `PerlandLauncher.AppImage`, `PerlandLauncher.deb` |

- Updater endpoints, baked into the app as constants in one place:
  - beta: `https://github.com/PerlandDev/perland-launcher/releases/download/channel-beta/latest.json`
  - stable: `https://github.com/PerlandDev/perland-launcher/releases/download/channel-stable/latest.json`
- A **beta** publish replaces `channel-beta`'s assets. A **stable** publish replaces `channel-stable`'s assets **and** `channel-beta`'s `latest.json` (but not its installer copies, which stay the newest beta), so beta players move up to a stable that is newer than their beta (semver: `0.2.0` > `0.2.0-beta.5`).
- The channel releases are named "Beta channel (updates)" and "Stable channel (updates)", with a body explaining they are update feeds and pointing at perland.online/launcher. `channel-beta` is a pre-release; `channel-stable` is not marked latest (`make_latest: false`) so the versioned release stays "Latest".
- `latest.json` follows Tauri v2's static format (`version`, `notes`, `pub_date`, `platforms.{windows-x86_64, linux-x86_64}.{signature, url}`). Its `url`s point at the **versioned** release's assets, not the fixed-name copies, so an in-flight update never changes under the updater. It is built by `scripts/release/manifest.mjs` from the `.sig` files, unit tested with `node:test`. Any platform missing a signature fails the publish.
- Moving off GitHub later needs one update that changes the endpoints first. Accepted.

### 3.4 Release workflow (`.github/workflows/release.yml`)

- **Trigger:** `workflow_dispatch` with inputs `channel` (`beta` | `stable`) and `dry_run` (boolean, default `true`).
- **Guard job** (`contents: read`): fails if `channel == stable` and the ref is not `main`; fails unless `TAURI_SIGNING_PRIVATE_KEY` is set; computes the version (§3.2) and exposes it as an output.
- **Build matrix** (`contents: read`, `timeout-minutes` on each):
  - `windows-latest`: `tauri build --bundles nsis`.
  - `ubuntu-22.04` (older glibc, so the AppImage runs on more distributions): `tauri build --bundles appimage,deb`, with the webview build dependencies CI already installs.
  - Each job patches the version into `Cargo.toml`, sets `PERLAND_BUILD=beta` for the beta channel, and passes `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` from secrets. `bundle.createUpdaterArtifacts` is `true`, so each bundle gets a `.sig`.
  - Uploads the bundles and signatures as workflow artifacts (also the dry-run output).
- **Publish job** (only when `dry_run` is false; the only job with `contents: write`):
  1. creates tag `v<version>` on the built commit and a GitHub Release for it (pre-release for beta) with the installers and `.sig` files;
  2. builds `latest.json` (§3.3);
  3. creates the channel release if it doesn't exist, then replaces its assets.
- **Concurrency:** one release at a time (`concurrency: release`, no cancel in progress), so two runs can't pick the same beta number.
- Secrets never reach logs: they are passed only through `env`, never echoed, and the workflow never runs `set -x`.
- `.github/workflows/tester-build.yml` is deleted.

### 3.5 Installers

- `bundle.active: true`, with targets passed per platform by the workflow. Local `pnpm tauri build` builds the host's default bundles.
- **Windows NSIS:** `bundle.windows.nsis.installMode: "currentUser"` (installs under `%LOCALAPPDATA%`, no admin prompt). `nsis.languages: ["English", "Spanish"]` with `displayLanguageSelector: false`, so the installer follows the Windows display language.
- **Linux:** `.deb` and AppImage.
- **`perland://` registration:**
  - NSIS and `.deb` register the scheme at install time from `plugins.deep-link.desktop.schemes` (the NSIS uninstaller removes it).
  - An AppImage has no install step, so at runtime a non-dev build calls `register_all()` only when `$APPIMAGE` is set. The condition is a pure function in `launcher-app` (`fn should_self_register(dev: bool, os: Os, appimage: bool) -> bool`), unit tested; `main.rs` only calls it.
  - Dev builds keep registering themselves as today.
- **Data folder:** unchanged (`%APPDATA%\PerlandLauncher`, `~/.local/share/PerlandLauncher`), so a tester's dev-mode worlds and accounts carry over to the installed build. The NSIS installer overwrites the dev-mode exe's `perland://` registration.
- **Authenticode signing (dormant):** when the repository variable `AZURE_TRUSTED_SIGNING` is `true`, the Windows job adds `bundle.windows.signCommand` (through `--config`) calling `trusted-signing-cli` with the Azure secrets. It is off for now. Tauri signs binaries and the installer before it computes the updater signature, so enabling it later changes nothing else.
- **Updater key:** the owner generates it on their own machine (`pnpm tauri signer generate`) and stores the private key and password as the secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. The **public** key goes in `plugins.updater.pubkey` in `tauri.conf.json` (it is not secret). Nobody asks for the private key or password in chat.

### 3.6 Smart App Control and SmartScreen

Unsigned installers trigger SmartScreen ("Windows protected your PC", passable through **More info** → **Run anyway**). Machines with **Smart App Control** enforcing block unsigned apps outright, with no override, and turning Smart App Control off on Windows 11 can't be undone without a reset. The download page says this plainly (§7) and doesn't push players to switch it off; signing is the real fix. This may also affect the owner's own PC during the end-to-end test (§8).

## 4. Auto-update

### 4.1 Units

- **`launcher-app::updates`** (new, no Tauri): the update state machine and policy.
  - `trait UpdateSource: Send + Sync`:
    - `async fn check(&self, channel: Channel) -> Result<Option<Available>>` (`Available { version: String }`);
    - `async fn download(&self, available: &Available, progress: &dyn Fn(u64, Option<u64>)) -> Result<Downloaded>` (`Downloaded` is opaque bytes plus version);
    - `fn install(&self, downloaded: Downloaded, relaunch: bool) -> Result<()>`.
  - `Updates<S: UpdateSource, H: Host>`: holds the state, emits `AppEvent::Update(UpdateState)`, and exposes `start()`, `check_now()`, `restart_now(busy: bool)`, `on_quit(busy: bool)`, `set_channel()`.
  - `fn endpoint(channel: Channel) -> &'static str` (§3.3).
- **`src-tauri/src/updater.rs`** (wiring only): `TauriUpdateSource` over `tauri-plugin-updater`, building an updater per check with `updater_builder().endpoints(vec![endpoint(channel)])`. The public key comes from config. Windows install mode is `passive`.
- The webview gets **no** updater capability. The UI talks to our commands only.

### 4.2 States

`UpdateState` (sent as the `update` event and returned by `update_status`):

| State | Meaning |
|---|---|
| `Off` | Dev build. Nothing is checked. |
| `Idle` | Not checked yet this session. |
| `Checking` | |
| `Downloading { percent: Option<u8> }` | |
| `Ready { version }` | Downloaded and verified; installs on quit or Restart now. |
| `UpToDate { checkedAt }` | |
| `Failed { error: LauncherError }` | The last check or download failed. |

### 4.3 Flow

- **On start** (Beta and Release builds): a few seconds after launch, `check`. If a newer version is available, download it in the background, then `Ready`. Checks do not repeat during the session except through **Check now** or a channel change.
- **Busy** means any game running, an install running, or a sync running. `AppCore` already knows this (`has_active_runs` plus the install slot); `updates` takes `busy` as an argument rather than reaching into runs.
- **Restart now:** refused with `update_busy` while busy (the UI also disables the button). Otherwise `install(relaunch: true)`.
- **On quit** (the `quit` command, and a window close with nothing running): if `Ready` and not busy, `install(relaunch: false)`. If the player confirmed quitting while a game still ran, the install waits for a later quit. A game is never interrupted.
- **Check now:** allowed in any state except `Checking`/`Downloading`. A `Ready` update stays ready; a newer one replaces it.
- **Channel change:** saves `updateChannel` and checks right away.

### 4.4 Channels

- `Settings` gains `update_channel: Option<Channel>` (`stable` | `beta`). `None` follows the build: Beta builds use `beta`, Release builds use `stable`. A missing field in an existing `settings.json` reads as `None`.
- **No downgrades:** the updater only offers higher versions. A beta player who switches to Stable stays on their beta until stable passes it; the hint says so.

### 4.5 Settings → Launcher updates

A new section in Settings, above About, styled through `/impeccable` and `DESIGN.md` (one lit button per view, lime only for selection, blue for progress):

- a Stable / Beta switch, with a one-line hint for the current choice;
- a status line: "Up to date · 0.2.0-beta.3", "Checking for updates…", "Downloading update… 40%", "0.2.0-beta.4 is ready. It installs when you close the launcher.", "Couldn't check for updates";
- **Check now**; **Restart now** when ready (disabled while busy, with the reason);
- Dev builds: "Updates are off in dev builds."

**Title-bar note:** while `Ready`, a quiet note in the title bar ("Update ready · installs when you close") with **Restart now**. It never takes the lit-button role.

### 4.6 Failures and error codes

- A failed check or download is quiet: logged, `Failed` in Settings ("Couldn't check for updates" plus Try again). No popup on start.
- A signature or integrity failure is never installed and gets its own code.
- New `ErrorCode`s, each with `errors.<code>.message` in `en.json` and `es.json`:
  - `update_failed` (check or download failed);
  - `update_invalid` (bad signature or manifest);
  - `update_busy` (Restart now while something runs).
- New commands: `update_status`, `update_check`, `update_restart`. The channel is set through the existing `settings_set`, which then triggers a check.

### 4.7 Verified early in the plan, not assumed

1. **Does the NSIS update always relaunch the app?** If `install(relaunch: false)` can't stop Tauri's NSIS installer from relaunching, install-on-quit becomes **install on the next start, before the window opens** (the downloaded bytes are kept in the data folder's `updates/`, re-verified, then installed). That still never interrupts a game, and the copy becomes "installs the next time you open the launcher". The plan's first task settles this.
2. **Can a `.deb` install update itself?** If `tauri-plugin-updater` can't, `.deb` installs show "Version X is out" with **Download**, which opens perland.online/launcher; the AppImage updates itself either way. The install type is detected at runtime and passed to `updates` as a flag.

## 5. Logs and diagnostics

### 5.1 Launcher log

- New `launcher-core::logging` on `tracing`, `tracing-subscriber` and `tracing-appender`. `src-tauri` calls `logging::init(&dirs)` once at startup and keeps the guard alive.
- Files: `logs/launcher.YYYY-MM-DD.log`, daily rotation, newest 14 kept. Pruning matches the `launcher` prefix only, so `<id>-latest.log` game logs are never touched.
- Every formatted line goes through `Redactor::for_current_user()` (home path → `~`) in a wrapping writer. Tokens are never logged, by rule; invite links already print without their token. **Plan 4 must add Microsoft tokens to this redactor.**
- Logged: startup (version, build kind, channel, OS, arch); update checks and results; run start and exit (instance id, Minecraft and loader versions, exit code); sync summaries; every `LauncherError` returned through IPC (code plus redacted detail).
- Level `info` by default; `PERLAND_LOG=debug` raises it.
- If the log can't be opened, the launcher runs without one (it never fails to start over logging).

### 5.2 Export diagnostics

- `launcher-core::diagnostics::export(dirs: &LauncherDirs, info: &SystemInfo, out: &Path) -> Result<()>` writes a zip:
  - `system.json`: launcher version, build kind, update channel, OS name and version, arch, total memory, installed Java runtimes, instance count;
  - `logs/`: the retained launcher logs and each `<id>-latest.log`, each cut to its last 5 MB;
  - `instances/<id>/instance.json` and `instances/<id>/managed.json`;
  - `settings.json`.
- Never included: `accounts.json`, `jvm-args.txt`, caches, game files.
- Every text entry passes through a `Redactor` built with the home path and every instance's invite token as secrets. Tests plant a token and the home path in each source and assert neither appears anywhere in the zip.
- The zip is written to a temp file next to `out` and renamed into place.
- **UI:** Settings → **Export diagnostics** opens a native save dialog (default `perland-diagnostics-YYYY-MM-DD.zip` in Downloads), then a toast "Diagnostics saved" with **Show in folder**. New command `diagnostics_export(path)`; the dialog runs in the webview through the existing dialog plugin, and the core validates that the path ends in `.zip`.

## 6. Carry-forwards

### 6.1 In this plan

- **Body-size caps on metadata fetches.** A shared `http::read_capped(response, limit) -> Result<Bytes>` reads at most `limit` bytes and fails with a stable code if the body is larger (the `Content-Length` is checked first when present). Used by `MetaClient` (Mojang, Fabric, Quilt, Forge, NeoForge JSON; about 32 MB), packwiz `pack.toml`, `index.toml` and metafiles (a few MB), and Modrinth API JSON. Invites and Browse images keep their existing caps.
- **Windows argfile encoding.** Java's launcher reads `@argfiles` in the system ANSI code page on Windows. On Windows the argfile is encoded with `WideCharToMultiByte(CP_ACP, WC_NO_BEST_FIT_CHARS)` (`windows-sys`, in `launcher-core`); if any JVM argument isn't representable, the argfile is skipped and the command line is used unchanged (the same fallback as the secret check). Other platforms keep UTF-8.
- **mrpack corrupt entry.** A damaged zip or entry inside a `.mrpack` fails with a new `ErrorCode::MrpackCorrupt` ("This .mrpack file is damaged") instead of the generic invalid-file error, with en/es strings.
- **Workflow `permissions` and `timeout-minutes`:** already present in `ci.yml`, `nightly.yml` and `tester-build.yml`; the new `release.yml` follows suit. Nothing else to do.
- **Join by link follow-ups:** the only distribution item (installer-time `perland://`) is §3.5.

### 6.2 Split to a follow-up hardening session

Object-cache GC; murmur2-only cache keys; mapping packwiz `meta_*` errors to pack codes; English fragments in error params. Proposed as its own session when this plan wraps up.

## 7. Download page (PerlandDev-site)

A small PR in `PerlandDev/PerlandDev-site`, styled through that repository's `/impeccable` surface `src-pages-launcher-astro`, with strings in `src/i18n/launcher.ts` (en and es, neutral Latin American Spanish with tú):

- The hero call to action "Get notified" becomes **Download the beta**, linking `…/releases/download/channel-beta/PerlandLauncher-beta-setup.exe`, with the sub-line "Windows 10/11 · closed beta".
- A secondary line for Linux: AppImage and `.deb` (`channel-beta` fixed-name assets).
- "Coming to Windows first" and the status section say it is in closed beta on Windows and Linux.
- The waitlist form stays lower on the page, reworded to "Get notified when it's out of beta".
- A "Windows warned you?" disclosure: SmartScreen's **More info** → **Run anyway**; Smart App Control blocks unsigned apps with no override and signing is coming; upgrading from a tester `.exe`: delete the old file, worlds and accounts stay.
- The Engine's `LAUNCHER_DOWNLOAD_URL` keeps pointing at `https://perland.online/launcher`, never at GitHub.
- The site PR merges only after the first beta is published, so its links never 404.

## 8. Testing and verification

**Unit and integration (TDD):**

- `BuildKind::from_parts` and `offline_allowed` for all three kinds; `app_info` reports `beta`.
- `should_self_register` for each OS, build and AppImage combination.
- `updates`: with a fake `UpdateSource` and fake host: start → available → downloading → ready; up to date; check failure → `Failed`, no event spam; Restart now refused while busy (`update_busy`); quit while busy doesn't install; quit when ready installs with `relaunch: false`; channel change re-checks with the new endpoint; Dev is `Off` and never checks.
- `Settings` reads an old file without `update_channel`.
- `logging`: a temp-dir log has the home path redacted; pruning leaves `*-latest.log` alone.
- `diagnostics::export`: zip contents and names; planted token and home path absent; `accounts.json` absent; 5 MB tail cut.
- `read_capped` against wiremock: under, at and over the limit, with and without `Content-Length`. Local inputs stay tiny (a limit of a few bytes in tests, never a large body).
- argfile: Windows-only tests for an ASCII argfile, a code-page-representable one, and the fallback.
- mrpack: a damaged archive gives `MrpackCorrupt`.
- `scripts/release/version.mjs` and `manifest.mjs`: `node:test`, run in CI.
- UI (Vitest + Testing Library): the Settings updates section in each state, the channel switch, the title-bar note, Restart now disabled while busy, Export diagnostics flow; en/es keys present.
- The IPC contract is regenerated, never hand-edited.

**Workflow:** a dry run of the Release workflow from the PR branch builds every bundle and signature.

**End to end (each publish waits for the owner's go-ahead; releases are public):**

1. The owner generates the updater key, adds both secrets, and shares the public key, which is committed to `tauri.conf.json`.
2. Dry-run the Release workflow; inspect the artifacts.
3. Publish `0.2.0-beta.1` from the PR branch. Install it on Windows from the `channel-beta` fixed-name installer: no admin prompt; the app opens; an offline account can be added; existing worlds are there.
4. On an invite page, press the Perland Launcher button: `perland://join` opens the installed app on the Join server screen.
5. Publish `0.2.0-beta.2`. Start the app: the title-bar note shows it ready. Quit, reopen: About shows `0.2.0-beta.2`; worlds and accounts are intact. Restart now is disabled while a game runs.
6. Linux spot check: the AppImage starts, `perland://` registers, and the `.deb` installs.
7. Export diagnostics from the installed build and check the zip by hand for the home path and tokens.

If Smart App Control blocks the unsigned installer on the owner's PC, the test moves to a machine without it enforcing (or a VM); that outcome is itself recorded on the download page.

## 9. Coordination

- A parallel "Fix beta feedback: launcher issues #4–#10" session may be changing `tauri.conf.json`, `main.rs`, the instance rail and rename. This plan's edits to `tauri.conf.json` are limited to `bundle`, `plugins.updater` and dropping `version`; to `main.rs`, the deep-link registration condition, updater and logging setup, and the quit hook. Rebase onto `main` before the PR.
- Plan 4 (Microsoft sign-in) stays paused and is not built here. When it ships: a stable release becomes meaningful, Microsoft tokens join the log redactor (§5.1), and the owner decides whether Beta keeps offline accounts open.
- The PR merges with a merge commit, so the beta tags made from the PR branch stay on `main`'s history.
