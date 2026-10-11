# Perland Launcher: Pack Sources and Sync (Plan 3), Design Addendum

- **Date:** 2026-10-10
- **Status:** Draft, awaiting owner review
- **Parent spec:** `docs/superpowers/specs/2026-10-09-launcher-core-design.md` (approved). §2.1, §2.3, §3 and §5 are the requirements; this addendum only records the split, the owner's answers and the decisions the parent spec leaves open.
- **UI spec:** `docs/superpowers/specs/2026-10-10-launcher-desktop-ui-design.md`. **Design system:** `DESIGN.md`, `.impeccable/surfaces/ui-src-app-tsx.md`.

## 1. Split

Plan 3 is too large for one plan and PR. It ships in three parts, each with its own plan and PR against `main`:

| Part | Scope | Verified by |
|---|---|---|
| **3a: core + CLI** | path hardening, downloader fixes, object cache, the `sources` (packwiz, `.mrpack`, Modrinth) and `sync` modules, CurseForge entries, the `launcher.toml` sidecar, per-instance and install locks, CLI `pack`/`sync`, nightly fuzzing and a real-pack nightly | wiremock tests + the real Asalith pack from the CLI, then an owner launch of that instance from the desktop app |
| **3b: desktop app** | app commands for packs, New-instance pack link and `.mrpack` tiles, the Play-time update check and "ask" dialog, update banner + changelog, Mods "Your additions" and optional toggles, the "Download these manually" dialog with Downloads-folder watching, pack art in the marquee, `perland://pack?url=` | Vitest/RTL + `AppCore` tests + owner smoke |
| **3c: Browse** | Modrinth search (modpacks, mods, resource packs, shaders) and install into a new or existing instance (spec §5) | its own plan |

## 2. Owner decisions (2026-10-10)

1. **Split:** 3a / 3b / 3c as above. Browse is not part of 3a or 3b.
2. **`ask` policy at Play:** when an update is available, Play pauses on a small dialog: "Update ready: 1.0.3 → 1.0.4", the change summary, **Update and play** (the one lit button) and **Play 1.0.3**. Between plays, the same update shows as a banner on the instance view.
3. **Who gets `auto`:** a pack opts in through its sidecar: `launcher.toml` `[sync] policy = "auto"`. Asalith adds that line. Everything else defaults to `ask`. The sidecar only sets the default when the instance is created; the player can change it later.
4. **Copy:** short and casual. Banner "Update ready: 1.0.3 → 1.0.4" with **Update**; changelog "+3 added · 2 updated · 1 removed", expandable to names, plus "Kept your version of N files" when the `.mrpack` override rule kept a player's edit. Spanish: "¡Actualización lista!: 1.0.3 → 1.0.4", **Actualizar**.

## 3. Decisions this addendum makes where the parent spec is silent

### 3.1 Sources

- **`DesiredState`** gains what the parent struct implies but doesn't name:
  - `name`, `stamp` (what the cheap check compares), `about` (description, art URLs, policy hint), and `archive` (the cached `.mrpack` that archive entries are read from).
  - Each file gets `name` (for changelogs), `mode` (`replace | preserve | replace_unless_modified`) instead of a bare `preserve` flag, and `optional: Option<{ default, description }>`.
  - A `manual` hint (CurseForge page + file name) replaces `blocked_hint`.
- **The stamp:**
  - packwiz: the sha256 of `pack.toml`'s bytes. `pack.toml` holds the index hash *and* the versions, so a loader-only bump is still an update.
  - Modrinth: the version id.
  - Local `.mrpack`: its sha1. A local file never updates; importing a newer file is the update path.
- **Modrinth update rule:** the newest version for the instance's **current Minecraft version** that is at least as stable as the installed one (release → release only; beta → beta or release; alpha → anything). Updates never move a modpack to another Minecraft version.
- **`SourceRef::Mrpack`** becomes `{ name, sha1 }`: the display file name and the cached archive's sha1. No instance with this variant exists yet, so nothing needs migrating. The UI schema is updated with it.
- **CurseForge CDN:**
  - The URL is `https://mediafilez.forgecdn.net/files/{id / 1000}/{id % 1000}/{percent-encoded file name}`. This was checked against the real Asalith entries on 2026-10-10:
    - `edge.forgecdn.net` redirects there and answers ranged requests with 404.
    - A raw `+` in the name returns 403.
  - The manual page is `https://www.curseforge.com/projects/{project-id}`.
  - A CDN 403/404 marks the file "blocked" instead of failing the sync.
- **launcher.toml:** `[art] icon`, `[art] banner` (relative to `pack.toml`, or absolute `https://` URLs), `[about] description` and `[sync] policy` (`auto` or `ask`). Unknown keys are ignored. A 404 means no sidecar.
- **Art:** the core downloads it into `instances/<id>/.perland/art/` (each image ≤ 5 MiB; PNG, JPEG, WebP or SVG by signature), because the app's CSP allows no remote images. If anything fails, it falls back silently to the generated gradient.

### 3.2 Sync

- **Two snapshots per instance:**
  - `.perland/pack.json` holds the resolved `DesiredState` of the installed version.
  - `.perland/managed.json` is the lockfile: per path the sha1, size, mode and display name of what was written.
  
  `pack.json` lets a pinned or offline instance repair itself and lets optional toggles apply without the network. `managed.json` is always written last.
- **Lockfile hash:** the sha1 of the installed bytes, whatever the pack declared, so "has the player modified it?" is a single comparison.
- **Fast path:** if the stamp and optional choices are unchanged and every managed file exists with its recorded size, sync does nothing. Otherwise it plans with full hashes (`ExistingCheck::Hash` semantics).
- **Object cache:**
  - Path: `cache/objects/<2-char prefix>/<key>`.
  - Key: the first declared hash among sha1, sha256, sha512 and md5 (hex), else `m2-<murmur2>`. Lengths and prefixes never collide.
  - A hit is verified against every declared hash before use, and a bad object is re-downloaded.
  - Files are **copied** from the cache into staging. Hardlinks would let an in-place edit in one instance leak into another instance and into the cache.
  - Garbage collection of unreferenced objects is deferred (Plan 6 maintenance).
- **Free space:** the check needs (known sizes not yet in the cache) + (known sizes being staged) + 100 MiB. packwiz metafiles carry no sizes, so for packwiz the check is a floor; a full disk during download still fails cleanly with `disk_io`.
- **Trash:** anything sync overwrites or removes goes to `.perland/trash/<unix-seconds>/<path>`. Folders older than 7 days are purged at the start of each sync.
- **Locks:**
  - **Per instance:** an OS file lock on `.perland/lock` (`File::try_lock`), held by sync and, from 3b, by a running game. A second holder gets `instance_locked`.
  - **Installs:** a blocking OS file lock on `<root>/.install.lock` held by `Installer::install`. This serializes app and CLI installs of shared version files across processes (the Plan 2 carry-forward).
- **Game change:** when the pack's Minecraft or loader version changed, sync installs the new version **before** swapping files. It then updates `instance.json`'s `game`, and writes `pack.json` and `managed.json` last. If the loader install fails, the instance is left as it was.
- **Changelog** (`SyncReport`): added, updated and removed names, `kept_modified` paths, and from/to pack versions. A `preview` computes the same lists from `DesiredState` versus the lockfile without downloading anything, for the "ask" dialog.
- **Player edits to managed mods:** the Mods screen (3b) shows managed mods read-only except optional ones, whose toggle changes `optionalChoices`. Renaming a managed jar to `.disabled` is not offered.

### 3.3 Carry-forwards

| Item | Where | Decision |
|---|---|---|
| `safe_join` accepts `.`, `a/.`, `:`, device names | 3a | It rejects `.` segments, empty segments, `:` anywhere, trailing dot or space, `<>"|?*`, control characters and Windows device names (with any extension). Pack paths that are equal case-insensitively are also rejected. |
| `ExistingCheck::Hash` for pack files | 3a | The sync planner hashes on-disk files, and cache objects are fetched with `ExistingCheck::Hash`. |
| murmur2 tail and mismatch tests | 3a | Added. |
| Full URLs in player-facing messages; lock map keyed on raw `PathBuf` | 3a | Messages name the host only, and detail drops query and fragment. Lock keys are absolute (lowercased on Windows), and entries are pruned after use. |
| `java_test` / `mods_add` take webview paths | 3b | Revisited together with `perland://`. |
| Concurrent app + CLI installs | 3a | The `<root>/.install.lock` file lock. |
| Natives marker | 3a | The old marker is deleted before the old folder is removed. |
| Legacy (pre-1.7) asset layouts | — | Not pack-related; stays deferred. |
| Stale `loader_unsupported` copy | 3a | Updated with the i18n work. |

## 4. Plan 3b decisions (2026-10-11)

The owner chose 4.1–4.4. The rest follows from §1–§3 and the 3a code, and the owner approved it with this section.

### 4.1 Owner answers

1. **Update banner:** a slim marquee plate at lower left, just above the instance name. It reads "Update ready: 1.0.3 → 1.0.4 · +3 added · 2 updated · 1 removed", with an **Update** button. The button is never lit, because Play stays the one lit button.
2. **Play copy:**
   - While checking: "Checking for updates…" / "Buscando actualizaciones…".
   - While syncing: the existing `sync_pack` task label ("Updating Asalith") with the block meter and files/MB.
   - The first sync of a never-synced pack says "Installing Asalith" (`install_pack`).
3. **`perland://pack?url=`:** the window comes forward and opens New instance with the link filled in and a "From <host>" line. The player clicks **Add pack**; nothing downloads before that click. If first-run setup isn't done, the link waits until it is.
4. **First sync:** Add only creates the instance (name, game version, art, policy) and opens it. The first Play installs everything, with progress on the Play button, then launches.

### 4.2 Play is one run

`play` is still one run in the existing state machine, which gains three phases: `checking`, `asking` and `syncing`.

```
idle → checking → [asking] → queued → syncing → preparing → running → [stopping] → idle
```

- **The lock:** the run takes `InstanceLock` before `checking` and drops it when the run leaves the map. The CLI or another window can't change the files from the update check until the game exits. A lock held elsewhere ends the run at once with `instance_locked`.
- **`checking`:** runs only for a pack instance that has synced before and isn't `pinned`. It calls `Syncer::find_update`: the cheap check, then resolving the update and a preview. Then:
  - Offline or a failed check: the run carries on with the installed version and emits the `update_check_failed` warning ("Couldn't check for updates").
- **`asking`:** happens only for `ask` with an update available.
  - The run waits, not holding the install slot, until the UI calls `play_answer(id, update: bool)`. Stop cancels it like a queued run.
  - The offer travels as a `PackUpdate` on the `run_state` event and on `run_states` snapshots, so a reloaded UI shows the dialog again.
- **`queued` → `syncing`:** sync runs under the existing `install_slot`, because it can install a new loader and writes the shared object cache. `preparing` follows under the same permit.
  - Vanilla instances skip `checking` and `syncing`.
- **Update-only runs:** the banner's **Update** uses the same machine through `pack_update(id)`: `queued` → `syncing` → `idle`, with no game.
- **How a run ends:** `ExitInfo` gains `synced: Option<SyncSummary>` (from/to version, counts, `keptModified`) and `manual: Vec<ManualFile>`.
  - The UI shows "Kept your version of N files" from `synced`.
  - It opens "Download these manually" from `manual`. The error is then `manual_downloads_needed`.

### 4.3 Core additions (additive; `ensure` keeps its behavior for the CLI)

- **`Syncer::find_update(store, &Instance) -> Result<Option<PendingUpdate>>`:** `Err` is the warning; `Ok(None)` covers up to date, pinned and never synced. `PendingUpdate` holds the source to save, the `DesiredState`, the `Changes`, and `kept`: override files the player edited that the update would keep. `kept` is computed by surveying only the wanted `ReplaceUnlessModified` files, so it never hashes the mods.
- **`Syncer::ensure_locked(&lock, store, id, accept: Option<PendingUpdate>, sink) -> Result<Ensured>`:**
  - It syncs the accepted update, or the installed version. A never-synced instance resolves its source.
  - **A failed update falls back:** if syncing to an accepted update fails with a `network` error, it syncs the installed version instead. That hits the fast path when nothing changed. The warning is `update_download_failed` ("Couldn't download the update; playing 1.0.3").
  - This is safe because sync touches the game folder only after every file is staged and the new loader is installed.
  - `ensure` becomes `try_acquire` + `find_update` (by policy) + `ensure_locked`.
- **`Syncer::check` honors `Pinned`:** it returns `UpToDate`.
- **`manual::Scanner`:** a reusable scanner that remembers each Downloads file's digests by (path, size, modified time), so polling every 2 s hashes only new or changed files. `scan` stays as a one-shot wrapper.
- **Modrinth, installed version deleted upstream:** when `GET /version/<installed>` answers 404/410, the check treats the installed version as a release with no date and offers the newest release for the instance's Minecraft version if its id differs. It no longer fails forever.
- **Redirects (`http.rs`):**
  - It follows 10 hops, not 9.
  - It refuses plain `http` for any hop when any earlier URL in the chain was `https` (loopback targets excepted). It no longer checks only the previous hop.

### 4.4 App commands (launcher-app; Tauri only forwards)

| Command | Returns | Notes |
|---|---|---|
| `pack_add(PackAddRequest)` | `Instance` | `{kind: link, url}`: an https `pack.toml` or `modrinth:<slug>[@version]`, never a file path. `{kind: file, path}`: a regular file ending `.mrpack`. Runs `add_pack` (art included). |
| `play_answer(id, update)` | `()` | Only while `asking`. |
| `pack_update(id)` | `()` | Update-only run. `already_running` while a run exists. |
| `pack_status(id)` | `PackStatus` | `{ installedVersion, update: Option<PackUpdate>, checkError: Option<LauncherError> }`. Feeds the banner. Takes no lock (it only reads), so it works while the game runs. |
| `pack_contents(id)` | `PackContents` | `{ managed: [{ path, name, optional: Option<{ enabled, description }> }], additions: [LocalMod] }`, for jars under `mods/` from `pack.json` and `managed.json`. |
| `pack_set_optional(id, path, enabled)` | `Instance` | Saves `optionalChoices`. It applies on the next sync (Play or Update), offline from `pack.json` and the cache. Refused for paths that aren't optional in `pack.json`. |
| `manual_import(id)` | `ManualProgress { found, total, remaining: [ManualFile] }` | Scans Downloads with the instance's `Scanner` against the list from the run's `ExitInfo.manual`, which `AppCore` keeps per instance. Imports matches into the object cache. |
| `pack_art(id)` | `PackArt { icon: Option<data URL>, banner: Option<data URL> }` | Reads `.perland/art/`. Missing art gives `None`. SVG is rendered only through `<img>` (no script). |
| `deep_link_take()` | `Option<String>` | The pending `perland://pack` link's https URL, consumed once. A new link also emits `AppEvent::DeepLink { url }`. |

- **Mods guard:** `mod_set_enabled` and `mod_remove` refuse a managed file with a new code, `mod_managed` ("This mod comes with the pack"). The UI never offers those actions on pack mods.
- **Webview paths:**
  - `mods_add` accepts only regular `.jar` files.
  - `pack_add` file mode accepts only regular `.mrpack` files.
  - `java_test` accepts only a regular file named `java`, `javaw`, `java.exe` or `javaw.exe`.
  - Remote input (the deep link) carries only an https URL through `PackInput::parse`, and must parse as `Packwiz`.
- **Deep link:**
  - `tauri-plugin-deep-link` 2.x, scheme `perland`. `single-instance` gets its `deep-link` feature so a second launch forwards the URL.
  - Dev runs call `register_all()`.
  - `launcher_app::deep_link::parse(raw)` accepts only `perland://pack?url=<https …>` and is unit-tested.
- **Opener allow-list:** add `https://www.curseforge.com/projects/*` for the manual-download pages.
- **Instance settings:** an Updates control, "Automatic / Ask me / Don't update", edits `updatePolicy`. It is shown only for pack instances.

### 4.5 UI surfaces (through `/impeccable`)

- **Update dialog:** "Update ready: 1.0.3 → 1.0.4", the summary "+3 added · 2 updated · 1 removed" that expands to names, and "Kept your version of N files" when `kept` is non-empty. Buttons: **Update and play** (the one lit button) and **Play 1.0.3**.
- **Banner:** as in 4.1. "Couldn't check for updates" appears as a quiet plate line, not an error card.
- **New instance:** the two "coming soon" tiles become live tabs: **Pack link**, an input plus Add pack, and **Import .mrpack**, a file dialog. Each has inline errors by code.
- **Mods on a pack instance:**
  - **From the pack:** read-only rows. Optional ones get a toggle and the description, plus "Applies next time you play".
  - **Your additions:** the existing jar list and actions, limited to additions.
- **"Download these manually":** a list of name and file name with an **Open page** button each, and a live "Found 2 of 5" while polling every 2 s. When all are found, the primary button **Continue** re-runs Play. **Open Downloads folder** is a secondary action.
- **Pack art:** the banner image sits behind the marquee name and plate, with the scene's vignette and grille over it so the type stays legible. The icon replaces `PlotIcon` in the rail. Missing art falls back to the generated scene.
- **Mock backend:** pack instances, a scripted ask, an update, a manual-download scenario, art. Forge returns Forge versions.
