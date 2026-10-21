# Share My Instance as a .mrpack Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** "Share…" in the instance menu writes a `.mrpack` the launcher's own importer reads back.

**Architecture:** `launcher-core/src/share.rs` scans an allowlist of the game folder, looks content files up on Modrinth (one `version_files` call plus one `projects` call) and writes the archive. `launcher-app/src/share.rs` adds `share_preview`, `share_export`, `share_reveal`, reaching Modrinth through the `Browser` seam. The UI adds `features/share/ShareDialog.tsx` and a menu item.

**Tech Stack:** Rust (tokio, zip, serde_json, sha1/sha2, wiremock), Tauri 2 wiring, React + Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-21-share-instance-mrpack-design.md`

## Global Constraints

- Allowlist only: `mods/*.jar`; `config/`, `defaultconfigs/`, `kubejs/`, `options.txt`, `optionsof.txt`, `optionsshaders.txt`; `resourcepacks/`; `shaderpacks/`; `saves/` minus `session.lock`. Nothing else is read.
- Excluded inside those: names containing `token|secret|password|credential|auth` (any case) or ending `.key|.pem|.p12`; text files under 1 MiB containing `plt_` + 16 token chars, or `accessToken`.
- `options.txt` drops `lastServer`, `fullscreen`, `fullscreenResolution`, `overrideWidth`, `overrideHeight`; drops `resourcePacks`, `incompatibleResourcePacks` when resource packs are off.
- Allowed download hosts: `cdn.modrinth.com`, `github.com`, `raw.githubusercontent.com`, `gitlab.com`.
- Limits (the reader's): one embedded file ≤ 1 GiB, embedded total ≤ 8 GiB, else `share_too_large`.
- `client-overrides/`: resource packs, shader packs, `options.txt`, `optionsof.txt`, `optionsshaders.txt`. Everything else embedded: `overrides/`.
- Index: `formatVersion` 1, `game` `minecraft`, `versionId` = UTC date `YYYY-MM-DD`, `name` = instance name, `summary` "Shared from Perland Launcher".
- Every UI string in `en.json` and `es.json`; new `ErrorCode`s `share_path_invalid`, `share_too_large` with `errors.<code>.message` in both.
- Contract regenerated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.

## Review Focus

- A mod whose Modrinth file has no sha512 or sits off the allowed hosts → embedded, not a broken index entry (Task 2 test `off_host_or_no_sha512_is_embedded`).
- A world containing a symlink or an unrepresentable name → skipped, export still succeeds (Task 1 test `skips_symlinks`).
- Two files with the same bytes (same mod twice) → both handled, one lookup hash (Task 2 test `duplicate_bytes_share_a_lookup`).
- Save path without extension or with `.zip` → `.mrpack` added / `share_path_invalid` (Task 4 test).
- Export fails midway → no file under the real name (Task 3 test `failed_write_leaves_no_file`).

---

### Task 1: Scan the game folder

**Files:** Create `crates/launcher-core/src/share.rs` (register `pub mod share;` in `lib.rs`).

**Interfaces:**
- Produces: `#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)] #[serde(rename_all = "camelCase")] pub enum Category { Mods, Settings, ResourcePacks, ShaderPacks, Worlds }`; `pub struct ScannedFile { pub path: String, pub size: u64, pub category: Category, pub sha1: Option<String> }` (sha1 set for content files: `mods/*.jar`, `resourcepacks/*.zip`, `shaderpacks/*.zip` at top level); `pub struct Scan { pub files: Vec<ScannedFile>, pub skipped_secrets: usize }`; `pub async fn scan(game_dir: &Path) -> Result<Scan>`; `pub fn clean_options(text: &str, keep_resource_packs: bool) -> String`; `pub fn is_secret_name(name: &str) -> bool`; `pub fn has_secret_text(bytes: &[u8]) -> bool`.

- [ ] Tests (`#[cfg(test)]`): `scan_uses_the_allowlist` (logs/, crash-reports/, screenshots/, servers.dat, usercache.json, mods/x.jar.disabled, saves/w/session.lock absent; mods/a.jar, config/a.toml, options.txt, resourcepacks/r.zip, shaderpacks/s.zip, shaderpacks/s.zip.txt, saves/w/level.dat present with categories); `skips_secret_files` (config/discord_token.json, config/x.pem, config/a.toml containing `plt_0123456789abcdef0123`); `skips_symlinks` (unix only); `clean_options_drops_private_lines`.
- [ ] Implement with a blocking walk in `spawn_blocking`; paths through `fsutil::normalize_rel`.
- [ ] `cargo test -p launcher-core share::` passes; commit.

### Task 2: Look content up on Modrinth

**Files:** Modify `crates/launcher-core/src/browse/api.rs` (add `projects`), `share.rs`; Test `crates/launcher-core/tests/share.rs`.

**Interfaces:**
- Produces: `Api::projects(&self, ids: &[String]) -> Result<Vec<ApiProject>>` (`GET /projects?ids=<json array>`, ids filtered by `valid_modrinth_id`); `#[derive(Clone, Serialize, Deserialize)] pub struct Linked { pub sha1, pub sha512: String, pub size: u64, pub url: String, pub env: Option<(String, String)> }` (client, server); `pub async fn lookup(api: &Api, sha1s: &[String]) -> Result<HashMap<String, Linked>>` (keys lowercase sha1). Resource/shader pack env is set by the writer, not here.
- [ ] Wiremock tests: `known_files_link_to_modrinth` (one POST `/v2/version_files`, env from `server_side: unsupported` → `("required","unsupported")`); `off_host_or_no_sha512_is_embedded`; `projects_failure_drops_env_only`; `version_files_failure_is_an_error`; `duplicate_bytes_share_a_lookup`; empty input makes no request.
- [ ] Implement; run `cargo test -p launcher-core --test share`; commit.

### Task 3: Preview and write

**Files:** `share.rs`, `tests/share.rs`.

**Interfaces:**
- Produces: `#[derive(Clone, Copy, Serialize, Deserialize)] #[serde(rename_all = "camelCase")] pub struct ShareChoice { pub mods, pub settings, pub resource_packs, pub shader_packs, pub worlds: bool }`; `pub struct SharePlan { pub scan: Scan, pub linked: HashMap<String, Linked> }`; `#[derive(Serialize)] #[serde(rename_all = "camelCase")] pub struct CategoryCount { pub category: Category, pub files: usize, pub linked: usize, pub embedded_bytes: u64 }` (for Worlds, `files` counts world folders); `pub fn counts(plan: &SharePlan) -> Vec<CategoryCount>` (only non-empty categories, in enum order); `pub struct PackMeta { pub name: String, pub version_id: String, pub minecraft: String, pub loader: Option<LoaderSpec> }`; `pub async fn write(game_dir: &Path, plan: &SharePlan, choice: ShareChoice, meta: PackMeta, out: &Path) -> Result<()>`; `pub fn file_name_for(name: &str) -> String` (`<name>.mrpack`, `<>:"/\|?*` and control chars → `-`, empty → `pack`); `pub fn today_utc() -> String`; new `ErrorCode::SharePathInvalid`, `ErrorCode::ShareTooLarge` in `error.rs` (add to `ALL`).
- [ ] Tests: `round_trip_through_the_reader` (wiremock lookup, then `write`, then `mrpack::read_archive`: minecraft + fabric loader, linked mod is `Remote` with Modrinth URL, embedded mod/config/resource pack are `Archive` entries with matching sha1, resource pack under `client-overrides/`, worlds absent with `worlds: false`); `choice_leaves_categories_out`; `too_large_refuses_before_writing` (use a `Limits`-like parameter: `write_with(…, limits: share::Limits { entry, total })` to keep the test tiny); `failed_write_leaves_no_file`; `options_txt_is_cleaned_in_the_archive`; unit tests for `file_name_for`, `today_utc` format, dependency keys (`forge`, `neoforge`, `fabric-loader`, `quilt-loader`).
- [ ] Implement: writes `<out>.part`, renames; deflate.
- [ ] Run tests; commit.

### Task 4: App commands

**Files:** Create `crates/launcher-app/src/share.rs`; modify `lib.rs`, `browser.rs` (trait `fn share_lookup(&self, sha1s: Vec<String>) -> BoxFuture<Result<HashMap<String, Linked>>>`), `testing.rs` (`FakeBrowser.share_linked: Mutex<HashMap<String, Linked>>`, `share_lookups: Mutex<Vec<Vec<String>>>`), `app.rs` (field `last_share: Mutex<Option<PathBuf>>`), `dto.rs`, `tests/contract.rs`.

**Interfaces:**
- Produces: `SharePreview { name: String, counts: Vec<CategoryCount>, fileName: String }` (camelCase DTO); `AppCore::share_preview(&self, id) -> Result<SharePreview>` (refuses server instances with `share_unavailable`? no: the UI hides it; refuses only bad ids); `AppCore::share_export(&self, id, choice: ShareChoice, out: PathBuf) -> Result<PathBuf>` (busy → `busy(id)`; extension rules like diagnostics with `SharePathInvalid`; holds `InstanceLock` while writing); `AppCore::share_last() -> Option<PathBuf>`. The plan from preview is cached per instance id in `share_plans: Mutex<HashMap<String, SharePlan>>` and taken by export.
- [ ] Tests in `share.rs`: export writes a file the reader reads; busy refusal; `notes.txt` refused, `pack` gets `.mrpack`; `share_last` set after export.
- [ ] Contract: fixtures `share_preview`, new codes in `error_codes`; regenerate.
- [ ] Commit.

### Task 5: Tauri wiring and UI

**Files:** `src-tauri/src/commands.rs`, `main.rs`; `ui/src/ipc/commands.ts`, `schema.ts`, `mock.ts`; create `ui/src/features/share/ShareDialog.tsx`, `ShareDialog.test.tsx`, `share.ts` (pure text helpers + tests); modify `ui/src/features/home/InstanceMenu.tsx`; `ui/src/i18n/en.json`, `es.json`.

**Interfaces:**
- Consumes: commands `share_preview(id)`, `share_export(id, choice, path)`, `share_reveal()`.
- [ ] Tests: dialog shows rows for non-empty categories, worlds off by default; mods line "All mods download from Modrinth…" vs "{linked} mods download from Modrinth. {embedded} aren't on Modrinth, so they go inside the file."; Save calls `shareExport` with the choice and the picked path; preview error shows Try again; the menu has Share… for a vanilla/pack instance and not for a server instance, disabled while running.
- [ ] Implement through `/impeccable` (DESIGN.md, `.impeccable/surfaces/ui-src-app-tsx.md`).
- [ ] All CLAUDE.md checks; screenshots via `pnpm --filter ui dev:mock` + Playwright; commit.
