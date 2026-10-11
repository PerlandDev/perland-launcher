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
