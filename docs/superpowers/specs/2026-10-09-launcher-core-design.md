# Perland Launcher — Sub-project 1: Launcher Core — Design

- **Date:** 2026-10-09
- **Status:** Draft, awaiting review
- **Repo:** `perland-launcher` (this repository)

## 1. Intent

Perland's mission is to fill what Minecraft is missing, starting with free servers. Players on Perland-hosted servers today install packs by hand: they download a `.mrpack` per release, use different steps for each launcher (Modrinth App, Prism, a TLauncher zip), and have to delete and reinstall to update. If they mix versions, the client fails or the server rejects it with "Incompatible FML modded server" (see `asalith-modpack/README.md`).

The Perland Launcher is a **free, open-source (GPL-3.0-only), modern Minecraft launcher**. It is built for Perland players first and works as a general-purpose launcher too. Packs stay in sync automatically, so players never manage mod versions themselves.

### 1.1 Program decomposition

The full vision is split into four sub-projects. Each one gets its own spec → plan → build cycle:

| # | Sub-project | Depends on |
|---|---|---|
| **1** | **Launcher core** (this spec): Microsoft sign-in, instances, Java, loaders, packwiz/`.mrpack`/Modrinth install, automatic sync, UI | — |
| 2 | Join by link: a public Engine server endpoint, a "Play" button in the Panel, a `perland://join/...` deep link | 1 |
| 3 | Perland accounts: sign in with Perland, a "My servers" list, update notices, linking a Minecraft UUID to a Perland user | 2 |
| 4 | Free servers from the launcher: an Engine free plan with quotas and abuse limits, and a create-and-invite flow | 3 |

This spec covers **only sub-project 1**. Where noted, it leaves extension points for 2–4, but it does not build them.

### 1.2 Success criteria

- A new player can go from installing the launcher to playing Asalith Fields in **three clicks or fewer** after signing in.
- When a pack changes upstream, the next launch updates it with no player action. The player's own added files and preserved configs survive.
- An interrupted sync (crash, power loss, network drop) never leaves an instance unlaunchable.
- Ships on Windows (primary) and Linux. macOS comes later.

### 1.3 Decisions made during brainstorming

| Topic | Decision |
|---|---|
| Base | **Build from scratch** on Tauri 2 + Rust + React. We do **not** fork Prism (dated Qt UI) or the Modrinth App (tied to Modrinth's services and business). We selectively **port** GPL-3.0-only code from Modrinth's `packages/app-lib`, with attribution. |
| Who maintains it | Mostly AI-assisted, so the stack was chosen on merit |
| Platforms | Windows + Linux in v1; macOS once an Apple Developer account exists |
| Content sources | packwiz URL, `.mrpack`, and Modrinth browsing. CurseForge *entries inside packwiz packs* are supported; CurseForge *browsing* is out of scope |
| Player-added mods in pack instances | **Lenient**: allowed, listed as "Your additions", never touched by sync |
| Offline accounts | **Prism-style gate**: available only after a licensed Microsoft account has signed in on that install. Free in dev builds. Superseded by `2026-10-27-microsoft-sign-in-design.md`: beta and stable builds have no offline accounts |
| Visual direction | **Game-forward**: full-bleed pack art, a big Play button, an instance icon rail |
| Styling process | `/impeccable` writes the launcher's `PRODUCT.md`/`DESIGN.md` and does styling and polish at implementation time |

## 2. Architecture

A Tauri 2 desktop app. All launcher logic lives in a plain Rust library with no Tauri dependency, so it can be tested on its own. The Tauri layer is a thin bridge: commands go in, progress events come out. The UI is React 19 + TypeScript + Vite + Tailwind 4.

```
perland-launcher/
├─ crates/
│  └─ launcher-core/
│     ├─ auth/        Microsoft → Xbox Live → XSTS → Minecraft token; refresh; OS keyring storage
│     ├─ meta/        version metadata from upstream sources (cached)
│     ├─ java/        detect installed JREs; download Mojang's official runtimes
│     ├─ download/    concurrent, resumable, hash-checked downloads; shared content-addressed cache
│     ├─ install/     vanilla client (libraries, natives, assets) + loaders
│     ├─ sources/     InstanceSource trait → DesiredState
│     ├─ sync/        diff DesiredState vs disk using the managed-files lockfile; apply via staging
│     ├─ instances/   on-disk layout, instance.json, import/export
│     └─ launch/      JVM/game args from version JSON, spawn, log capture, crash detection
├─ src-tauri/         commands, event forwarding, perland:// handler, updater, tray
├─ ui/                React app
└─ docs/superpowers/specs/
```

### 2.1 Unit boundaries

- **`sources` → `sync` is the central seam.** Every source reduces to a `DesiredState`:
  ```
  DesiredState {
    minecraft: String, loader: Option<{ kind: Fabric|Quilt|Forge|NeoForge, version }>,
    files: [ { path, urls: [..], hashes: { sha1?, sha256?, sha512?, murmur2? }, size?,
               side: Client|Server|Both, optional: bool, preserve: bool, blocked_hint?: Url } ],
    pack_version: String, art: Option<PackArt>
  }
  ```
  A file's payload is either remote (`urls`) or an **archive entry**. `.mrpack` `overrides/` and `client-overrides/` become archive entries read from the cached `.mrpack`; since `.mrpack` has no `preserve` flag, override files outside `mods/` are replaced on update **only if the player hasn't modified them** (on-disk hash still equals the hash in the lockfile); otherwise the player's version is kept and the update changelog says so. packwiz files keep packwiz semantics (pack wins unless `preserve`). `sync` never knows where a pack came from. Sub-project 2 adds a `PerlandServer` source that returns the same struct.
- **`download`** is the only module that touches the network for payloads. Libraries, assets, mods and Java runtimes all go through one content-addressed cache, keyed by sha1 or sha256, so identical files are stored once across instances.
- **`launcher-core` emits structured events** (`Progress{task, done, total, bytes}`, `Log`, `StateChanged`). The UI only renders them.

### 2.2 Upstream metadata sources

| Data | Source |
|---|---|
| Minecraft versions | `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` |
| Java runtimes | Mojang's `java-runtime` manifest (the one the official launcher uses), Windows x64 + Linux |
| Fabric | `https://meta.fabricmc.net/v2` |
| Quilt | `https://meta.quiltmc.org/v3` |
| Forge | `https://maven.minecraftforge.net` (installer jar → processors) |
| NeoForge | `https://maven.neoforged.net/releases` (installer jar → processors) |
| Modrinth | `https://api.modrinth.com/v2` (no key; send a descriptive `User-Agent`) |

Metadata is cached on disk with ETags, so the launcher can start offline.

### 2.3 On-disk layout

Under the Tauri app data directory (`%APPDATA%\PerlandLauncher` or `~/.local/share/PerlandLauncher`):

```
cache/objects/<hash-prefix>/<hash>     content-addressed blobs
meta/                                  cached manifests
libraries/  assets/  versions/         shared vanilla/loader files
java/<runtime-name>/                   Mojang runtimes
instances/<id>/
  instance.json
  minecraft/                           game directory (mods/, config/, saves/, ...)
  .perland/managed.json                lockfile of managed files
  .perland/staging/  .perland/trash/
logs/
```

`instance.json`: `id, name, source{kind: packwiz|mrpack|modrinth|none, url|projectId+versionId}, game{minecraft, loader, loaderVersion}, updatePolicy{auto|ask|pinned}, optionalChoices{path: bool}, java{path?, memoryMb}, art`.

### 2.4 License

GPL-3.0-only. Every file ported from Modrinth's `app-lib` keeps an attribution header, and `NOTICE` lists them. No Modrinth branding assets are copied.

## 3. Pack sync

### 3.1 When it runs

- **Before every launch** of an instance that comes from a pack, and on demand through "Check for updates".
- The check is cheap. For packwiz, fetch `pack.toml` and compare the index hash, then stop if nothing changed. For `.mrpack` or Modrinth, compare the version ID through the Modrinth API.
- **Offline:** launch with the last synced state and show "Couldn't check for updates".
- **Update policy:** Perland server packs default to `auto`. Packs a player installed themselves default to `ask`. `pinned` never updates.

### 3.2 Decision table

| On disk | In DesiredState | In lockfile | Action |
|---|---|---|---|
| missing | yes | — | download |
| same hash | yes | — | skip |
| different hash | yes | — | replace (pack wins), **unless `preserve`** |
| different hash | yes, `preserve` | — | keep the player's file |
| present | no | yes | move to `.perland/trash/` (deleted after 7 days) |
| present | no | no | **player's file, never touched**, shown as "Your additions" |

- Only `side ∈ {client, both}` is installed.
- `preserve` (packwiz index flag): the file is written only if it is absent.
- Optional files (packwiz `option.optional`, mrpack `env` = `optional`) appear as toggles. Choices persist in `optionalChoices`.

### 3.3 Applying a sync

1. Take the per-instance lock. Refuse if the game is running from this instance.
2. Check free disk space against the total download size.
3. Download every needed file into `.perland/staging/` and verify its hash.
4. If the Minecraft or loader version changed, install the loader first.
5. Move staged files into place, then move removed files to the trash.
6. Write `managed.json` **last**.

If the process stops before step 6, the previous lockfile still describes a working state, and the next run picks up from the cache.

### 3.4 CurseForge entries in packwiz packs

1. Build the CDN URL from the file ID and file name, then download and verify the hash.
2. If that fails (the author blocked third-party distribution), show a **"Download these manually"** dialog that opens each CurseForge page in the browser.
3. Watch the user's Downloads folder and pick up any file whose hash matches.

### 3.5 Optional pack art sidecar (`launcher.toml`)

packwiz has no field for art, so an optional file next to `pack.toml` provides it:

```toml
[art]
icon = "assets/favicon.svg"
banner = "assets/og-asalith.png"   # paths relative to pack.toml, or absolute URLs
[about]
description = "Official Asalith server modpack"
```

For Modrinth and `.mrpack`, the launcher uses the project icon and featured gallery image. Otherwise it falls back to a landscape gradient generated from the pack name.

## 4. Accounts and sign-in

- **Flow:** OAuth authorization code + PKCE in the **system browser** with a loopback redirect on `127.0.0.1`. The fallback is device code ("go to microsoft.com/link, enter ABCD-1234"). No embedded webview.
- **Token chain:** MSA → Xbox Live `user.authenticate` → XSTS (`rp://api.minecraftservices.com/`) → `login_with_xbox` → entitlements (ownership) → profile (name, UUID, skin).
- **Friendly XSTS errors:** no Xbox profile (2148916233), child account needs a Family (2148916238), region unavailable (2148916235), no game ownership.
- **Storage:** the MSA refresh token goes in the OS credential store (Windows Credential Manager, or Linux Secret Service). Without a Secret Service, it falls back to an encrypted file with a one-time notice. The Minecraft access token lives in memory only and is refreshed before it expires. Tokens are redacted from all logs.
- **Multiple accounts**, with one active. The `Account` type has a `provider` field (`microsoft | offline`) so a `perland` provider (sub-project 3) can be added later.
- **Offline accounts:** release builds unlock them after one successful licensed Microsoft sign-in on that install. Dev builds allow them freely.
  Superseded by `2026-10-27-microsoft-sign-in-design.md`: beta and stable builds have no offline accounts.
- The **Azure client ID** is injected at build time (CI secret or local `.env`) and is never committed.

### 4.1 Owner action items (outside the code, start now)

1. Azure portal → Entra ID → App registrations → New registration. Use "Personal Microsoft accounts only" and the public client redirect `http://localhost`. Enable "Allow public client flows" for device code.
2. Submit Mojang's Minecraft API permission request for that client ID. The form is linked from Microsoft/Mojang's developer docs; verify the current link before submitting. Approval can take weeks. Until it comes through, development uses dev-build offline accounts.

## 5. UI and experience

Game-forward direction. Final styling and the design system come from `/impeccable` at implementation time.

| Screen | Contents |
|---|---|
| Home / instance | An instance icon rail. The selected instance fills the view: hero art, name, loader and version, a status pill (reserved for sub-project 2), a big **Play** button, and **Mods** and **⋯** buttons |
| Instance → Mods | Managed mods, **Your additions**, optional toggles, an update banner with a changelog of mods added, removed and updated |
| Browse | Modrinth search across modpacks, mods, resource packs and shaders. Install into an existing or new instance |
| New instance | Paste a pack URL (packwiz) · import a `.mrpack` · vanilla or a bare loader |
| Accounts | Skin heads, add, switch, remove, offline (when unlocked) |
| Settings | Java (automatic by default), memory, directories, launcher updates (stable/beta), theme, language (**English + Spanish in v1**) |
| Activity drawer | Download and sync progress, game log, crash report with "copy for support" |

- **What happens on Play:** check for an update → sync (progress shown on the button) → check Java → launch. The launcher minimizes while the game runs. On a crash it comes back to the front with the likely cause ("Mod X requires Y", "Out of memory, raise to 6 GB?").
- **First run:** welcome → Microsoft sign-in → "Got a pack link? Paste it" or "Browse Modrinth".
- **`perland://`:** registered in v1, handling only `perland://pack?url=<packwiz pack.toml URL>`. Sub-project 2 adds `join`.

## 6. Reliability, errors and distribution

- **Typed errors** in the core, each with a category (`Network, Auth, Integrity, Disk, Java, Loader, Pack, Game`), a plain-language message, an optional suggested fix and technical detail. The UI shows the message and a fix, with detail behind a "Details" toggle.
- **Network:** retries with exponential backoff, resumes partial files, falls back to the next URL in `urls`.
- **Integrity:** on a hash mismatch, delete and download again once, then fail that file with a clear message.
- **Concurrency:** a per-instance lock file.
- **Logs:** structured, rotated daily, kept 14 days. Tokens and the home path are redacted. **Export diagnostics** creates a zip of logs, `instance.json`, `managed.json` and system info.
- **No telemetry in v1.**
- **Launcher self-update:** the Tauri updater, signed with Ed25519, from GitHub Releases. It checks on startup, installs on quit and never interrupts a game. Channels: `stable` and `beta`.
- **Windows:** an NSIS installer, per user, with no admin prompt. Signing goes through Azure Trusted Signing once it's set up; until then builds are unsigned and SmartScreen warns.
- **Linux:** AppImage + `.deb`. Flatpak comes later.

## 7. Testing

**`launcher-core`:**
- Unit tests for every row of the sync table, `preserve`, sides, optional files, each hash algorithm (including CurseForge's whitespace-stripped murmur2), Java selection, and argument building.
- Fixture tests against recorded Mojang, Fabric, Quilt, Forge and NeoForge metadata, so they run offline and give the same result every time.
- Auth tested against a `wiremock` server: the full chain, each XSTS error code, and refresh.
- End-to-end sync against a local test server serving a tiny packwiz pack in a temporary directory: install, update, remove, "Your additions" survive, `preserve`, an interrupted sync followed by a resume.

**Nightly CI only:**
- A real-pack test: resolve Asalith, install vanilla + Forge 1.20.1, and run a **launch dry-run** that builds the full command line and checks every classpath entry exists.
- Fuzzing the pack and TOML parsers.
- **Heavy tests (fuzzing, stress) never run on a developer PC.**

**UI:** Vitest + React Testing Library with the Tauri commands mocked. App-level WebDriver tests are deferred until after v1.

**Manual release checklist (Windows):** fresh install, sign in, install Asalith, play, crash recovery, pack update, launcher self-update. Linux is spot-checked.

**CI on every push and PR (Windows + Ubuntu):** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `pnpm lint`, typecheck, `vitest`. A `v*` tag builds signed release artifacts.

## 8. Repository

- Cargo workspace (`crates/*`, `src-tauri`) + pnpm workspace (`ui`).
- `LICENSE` (GPL-3.0-only), `NOTICE`, `README.md`, `CLAUDE.md`/`AGENTS.md` following the sibling Perland repos.
- The GitHub remote (proposed: `PerlandDev/perland-launcher`, public) is created only after explicit owner confirmation.

## 9. Out of scope for sub-project 1

macOS builds · CurseForge browsing · Perland accounts, join links and server status (sub-projects 2–3) · creating servers (sub-project 4) · telemetry or crash reporting · Flatpak · app-level WebDriver tests · mod developer tooling · skin editing.
