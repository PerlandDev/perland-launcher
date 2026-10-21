# Perland Launcher: Share My Instance as a .mrpack, Design

- **Date:** 2026-10-21
- **Issue:** PerlandDev/perland-launcher#83.
- **Feature:** D1 of "Share my instance" (D2, "Play this with friends", is a separate spec and pull request that needs an Engine change first).
- **Builds on:** `docs/superpowers/specs/2026-10-10-pack-sync-design.md` (`.mrpack` import, `sources/mrpack.rs`), `docs/superpowers/specs/2026-10-11-browse-design.md` (`browse::api::Api::version_files`, `browse::identify`), the diagnostics export (`diagnostics.rs`, `Settings → Diagnostics`), `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`.
- **Status:** the owner is away and asked for no questions. Every decision below is **our call**, made from `CLAUDE.md`, `PRODUCT.md` and `DESIGN.md`, with its reason.

## 1. Why

A player who built a good modded instance (Browse installs, a few mods dropped in by hand, tuned settings) has no way to hand it to a friend. Today the friend has to rebuild it mod by mod. The launcher already opens `.mrpack` files and Modrinth pack links, so writing a `.mrpack` closes the loop: the friend opens the file in Perland Launcher and gets the same instance.

## 2. Outcome

- **Entry point:** **Share…** in the instance's ⋯ menu (and its right-click and rail copies, which share one list). Not offered for server instances (they already have **Copy link**, which is the better way to share a server) and disabled while the game runs.
- **The Share dialog** ("Share {name}") lists what goes in, in plain words, each a toggle with what it holds:
  - **Mods** (on): "12 mods".
  - **Settings** (on): "Game and mod settings".
  - **Resource packs** (on): "3 resource packs".
  - **Shader packs** (on): "1 shader pack".
  - **Worlds** (off): "2 worlds, 340 MB".
  
  A row with nothing in it is not shown.
- **Before saving, one plain line says what happens to the mods:** "10 mods download from Modrinth. 2 aren't on Modrinth, so they go inside the file." When every mod is on Modrinth: "All mods download from Modrinth, so the file stays small." Then the file's approximate size: "About 48 MB."
- **Save file…** opens the system save dialog with `<instance name>.mrpack` in Downloads. While writing, the button reads "Saving…". After it, a notice: "Saved. Your friends open it in Perland Launcher." with **Show in folder**.
- **The friend** opens the file through **New instance → Import a .mrpack** (or by double-clicking it once file associations exist) and gets an instance named after it, with the same Minecraft version, loader and content.

## 3. Goals, non-goals, anti-goals

- **Goals:** a `.mrpack` that the launcher's own reader imports without loss; Modrinth-known mods, resource packs and shader packs as index entries with Modrinth's download links; everything else the player picked under the overrides; the not-on-Modrinth count said before saving; a round-trip test.
- **Non-goals:** uploading the file anywhere (D2 does that); publishing to Modrinth; CurseForge lookups; choosing single files (only categories); a pack version the player types; exporting server instances.
- **Anti-goals:** a token, an account name or a log in the file; a file the launcher itself can't import (too big, bad paths); a surprise about embedded mods after saving; launcher words ("instance", "overrides", "index") in the dialog.

## 4. Facts this rests on

- **Instance layout:** `<instances>/<id>/minecraft/` is the game folder; `<instances>/<id>/.perland/` (lockfile, lock) and `instance.json` live beside it, never inside. Account tokens live in `accounts.json` and the OS credential store; invite tokens in `instance.json`. None of these are in the game folder, so an allowlist of game-folder paths keeps them out by construction.
- **The reader** (`sources/mrpack.rs`): `formatVersion` 1, `game` `minecraft`; `dependencies` read by `game_from` (`minecraft` plus one of `forge`, `neoforge`, `fabric-loader`, `quilt-loader`); every index file needs a valid sha1 (sha512 optional), at least one `https` download and a path that passes `normalize_rel`; `overrides/` and `client-overrides/` entries are read, `server-overrides/` ignored; limits: index 16 MiB, one entry 1 GiB, all entries 8 GiB.
- **Modrinth's format** requires `hashes.sha1` and `hashes.sha512`, `fileSize`, and downloads only from `cdn.modrinth.com`, `github.com`, `raw.githubusercontent.com` and `gitlab.com`.
- **`POST /v2/version_files`** (`browse::api::Api::version_files`) answers, per known sha1, the version with its `files[]` (url, size, hashes). One request takes all hashes.
- **`GET /v2/projects?ids=[…]`** answers each project's `client_side` and `server_side` (`required`, `optional`, `unsupported`).
- **Content folders** Browse looks at: `mods/*.jar`, `resourcepacks/*.zip`, `shaderpacks/*.zip` (`browse::identify::content_files`).

## 5. Decisions

| Question | Decision and reason |
|---|---|
| What each choice covers | **Mods:** `mods/*.jar` (turned-off `.jar.disabled` mods stay out: the player turned them off). **Settings:** `config/`, `defaultconfigs/`, `kubejs/`, `options.txt` (cleaned, below), `optionsof.txt`, `optionsshaders.txt`. **Resource packs:** `resourcepacks/` (zips and folders). **Shader packs:** `shaderpacks/` (zips, folders and their `.txt` settings). **Worlds:** `saves/` without `session.lock`. Nothing else in the game folder is ever read: an allowlist, so `logs/`, `crash-reports/`, `screenshots/`, `servers.dat`, `usercache.json`, `.perland/` and anything a mod writes elsewhere stay out |
| Worlds default | Off: worlds are big and personal, and the task says so |
| `options.txt` | Included with Settings, minus `lastServer` (the last server typed, can be a private address) and the window lines (`fullscreen`, `fullscreenResolution`, `overrideWidth`, `overrideHeight`), which belong to the player's screen. `resourcePacks` and `incompatibleResourcePacks` are dropped when Resource packs is off, so the friend's game doesn't look for packs that aren't there. `options.txt` has no account data; the cleaning is about privacy and the friend's screen |
| Files that may hold secrets | Inside the Settings folders and files (where mods keep config), a file is left out when its name contains `token`, `secret`, `password`, `credential` or `auth` (any case) or ends in `.key`, `.pem`, `.p12`, or when it is text under 1 MiB containing `plt_` followed by 16 or more token characters (a Perland player token) or `accessToken`. The dialog doesn't mention it (nothing to decide); the count is logged without names. Reason: some mods keep API keys in `config/`, and `CLAUDE.md` says nothing holding a token leaves |
| Which files become index entries | Mods, resource packs and shader packs (files, not folders) whose sha1 Modrinth knows, through one `version_files` call, and whose matched Modrinth file has a sha512, a size and a download on an allowed host. Everything else the player picked goes inside the file |
| Lookup failure | Modrinth unreachable or rate-limited: the dialog shows the error card with Try again and no Save button. Reason: falling back to embedding everything would silently make a huge file with mods the player may not have the right to share |
| Side of each entry (`env`) | From one `GET /projects?ids=` call: `server_side: unsupported` → `{client: required, server: unsupported}`, `client_side: unsupported` → `{client: unsupported, server: required}`; resource and shader packs are always client-only. Reason: D2 and any server reading the pack skip client-only mods. If that call fails, entries get no `env` (both sides) rather than failing the export, since the pack still works for players |
| `overrides/` vs `client-overrides/` | Mods, Settings and Worlds go under `overrides/`; resource packs, shader packs, `options.txt`, `optionsof.txt` and `optionsshaders.txt` under `client-overrides/`, so a server reading the pack doesn't receive them. The launcher reads both |
| Name, version, summary | `name` is the instance name; the file is `<name>.mrpack` with characters Windows refuses replaced by `-`. `versionId` is the export date, `YYYY-MM-DD` (UTC), so a friend's second import reads as a newer version. `summary` is "Shared from Perland Launcher" |
| Loader | From the instance's `game.loader`: `forge`, `neoforge`, `fabric-loader`, `quilt-loader` with its version; vanilla has only `minecraft` |
| Size limits | The export refuses before writing (`share_too_large`) when one embedded file is over 1 GiB or the embedded total is over 8 GiB: the reader's limits, so the launcher never writes a file it can't import. Message: "This is too big to share as one file. Leave out worlds, or large files." |
| Path rules | Every path goes through `normalize_rel`; symlinks are skipped; a name that fails is skipped. Two names that differ only by case keep the first in sorted order (the reader refuses a pack with both). A file that vanished or another program holds is skipped, counted in the log without names. Zip entries use `/` |
| Writing | Deflate, written to `<file>.part` beside the target and renamed at the end, so a failed export never leaves a half file under the real name. The target must end in `.mrpack` (a name without extension gets it, any other is `share_path_invalid`), like diagnostics |
| Running game | Share is disabled while the instance runs and `share_export` refuses with `instance_busy`: a world being saved would be copied half-written |
| Server instances | Not offered: their content follows the server's pack, and the invite link already shares them |
| Counting embedded mods | The preview reports, for Mods only, `{modrinth, embedded}`; resource and shader packs not on Modrinth are embedded too but not called out (they are not code) |
| Show in folder | Like diagnostics: the app remembers the last path it wrote, and `share_reveal` reveals that one only (never a path from the webview) |

## 6. Design

### 6.1 launcher-core: `share.rs` (new)

- `pub struct ShareChoice { mods, settings, resource_packs, shader_packs, worlds: bool }`.
- `pub async fn scan(game_dir) -> Result<Scan>`: walks the allowlist once and returns the candidate files per category with sizes and, for content files, their sha1 (worlds are sized, not hashed). Secret-looking files are dropped here.
- `pub async fn plan(api: &Api, scan: &Scan) -> Result<SharePlan>`: one `version_files` call over every content sha1, then one `projects` call for the matched project ids (its failure only drops `env`). Each content file becomes `Linked { path, sha1, sha512, size, url, env }` or `Embedded { path, size }`.
- `pub fn preview(plan, choice) -> SharePreview`: counts per category, mods `{modrinth, embedded}`, worlds count, approximate size (embedded bytes plus index).
- `pub async fn write(game_dir, plan, choice, meta: PackMeta, out) -> Result<()>`: builds `modrinth.index.json` (sorted paths), streams the chosen embedded files into `overrides/` or `client-overrides/`, cleans `options.txt`, checks limits first, writes `.part`, renames.
- `browse::api::Api::projects(ids)` (new): `GET /projects?ids=[…]`, ids checked with `valid_modrinth_id`.

### 6.2 launcher-app: `share.rs` (new)

- `share_preview(id) -> SharePreview` (DTO with counts for all five categories, mods split, worlds size, total sizes per category so the UI can sum the chosen ones) — scans and plans, keeping the plan by instance id for the export (dropped when the dialog asks again).
- `share_export(id, choice, path) -> String` (the written path). Uses the stored plan when present, else plans again. Refuses while the instance runs.
- `share_reveal()` reveals the last written file.
- New `ErrorCode`s: `share_path_invalid`, `share_too_large`, each with `errors.<code>.message` in en and es. Contract fixtures regenerated.

### 6.3 src-tauri

Three commands wired to the app (`share_preview`, `share_export`, `share_reveal`), nothing else.

### 6.4 UI (through `/impeccable`)

- `ui/src/features/share/ShareDialog.tsx`: the dialog of §2, using the existing `Dialog`, `Toggle` and `Button`; loading state "Looking up your mods…"; error card with Try again.
- `useInstanceActions` gains **Share…** (icon `Share` or the closest existing one), before **Open folder**, hidden for server instances, disabled while running.
- Mock: `share_preview` and `share_export` in `ipc/mock.ts` so `pnpm --filter ui dev:mock` shows it.
- Strings in `en.json` and `es.json`.

## 7. Tests

- core unit: allowlist and exclusions (logs, screenshots, `servers.dat`, `session.lock`, secret-looking names and contents), `options.txt` cleaning, loader keys, index JSON shape, file name sanitizing, size limits.
- core wiremock (`tests/share.rs`): one `version_files` call; known mods become entries with Modrinth's URL, sha512, size and env; unknown ones are embedded; a match on a host outside the allowed list is embedded; `projects` failure drops env only; `version_files` failure is an error.
- **Round trip:** export an instance with a linked mod, an embedded mod, a config file and a resource pack, then `mrpack::read_archive` on the result: same Minecraft version and loader, linked file is `Remote` with the right URL and hashes, embedded files are `Archive` entries with matching sha1, nothing outside the allowlist.
- app: busy refusal, path rules, reveal only the last file.
- UI (Vitest): the dialog's rows, the mods line (all on Modrinth / some embedded), worlds off by default, save calls `shareExport` with the choice, the menu item hidden for server instances.
