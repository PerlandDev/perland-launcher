# Skin editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A "Skins" screen where players keep a wardrobe of skins, paint them on a 3D figure and a flat texture, and wear one on their Microsoft account.

**Architecture:** launcher-core owns PNG validation, the on-disk library and the two Minecraft services calls; launcher-app exposes them as `AppCore` methods (fakeable through `MsaApi`); src-tauri wires commands. The UI keeps all paint logic pure (`ui/src/features/skins/*.ts`, Vitest) and renders with three.js behind a thin adapter, in a lazily loaded route.

**Tech Stack:** Rust (image 0.25, reqwest multipart, wiremock), React 19, three.js `0.181.2` (exact pin), Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-11-07-skin-editor-design.md`. Direction contract: `.impeccable/surfaces/ui-src-features-skins-skinstudio-tsx.md`.

## Global Constraints

- No Tauri in `crates/*`. `src-tauri` is wiring only.
- Never log the Minecraft token; it travels only in `Authorization: Bearer`.
- Textures cross IPC as `data:image/png;base64,…`.
- Library: `<root>/skins/library.json` + `<root>/skins/<id>.png`; textures always normalised 64×64 RGBA.
- Imports read at most 1 MiB; only 64×64 and 64×32 accepted.
- New ErrorCodes: `SkinInvalid`, `SkinNotFound`, `SkinRejected`, `SkinOfflineAccount`, each with `errors.<code>.message` in en and es.
- Every UI string in `en.json` and `es.json` (neutral Latin American Spanish, *tú*).
- three.js pinned exactly, imported only from `ui/src/features/skins/`; the route is `React.lazy`.
- No Mojang textures in the repo; the starter is drawn in code.
- Contract JSON regenerated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`.

## Review Focus

- A 64×32 legacy skin imports with left arm/leg filled from the right (mirrored), not blank. → Task 1 test `legacy_skin_gets_left_limbs`.
- A PNG of the right size but palette/greyscale/16-bit decodes to RGBA correctly. → Task 1 test `greyscale_png_normalises_to_rgba`.
- A damaged `library.json` doesn't break the screen; it is set aside and the library starts empty. → Task 2 test `damaged_library_starts_empty`.
- Fill on a face never leaks into the neighbouring face or the unused texture area. → Task 6 test `fill_stays_inside_face`.
- Mirror on the head maps the front face's left column to its right column, and right arm ↔ left arm in both models (slim arms 3 wide). → Task 6 tests `mirror_head_front`, `mirror_slim_arms`.

---

### Task 1: PNG validation and legacy conversion (launcher-core `skins_png`)

**Files:** Create `crates/launcher-core/src/skins_png.rs`; modify `lib.rs` (mod), `error.rs` (add the four codes).

**Interfaces:** Produces `pub fn normalise(bytes: &[u8]) -> Result<Vec<u8>>` (PNG bytes in, normalised 64×64 RGBA PNG out, `SkinInvalid` otherwise); `pub const MAX_SKIN_BYTES: usize = 1 << 20`; `pub fn data_url(png: &[u8]) -> String`; `pub fn from_data_url(url: &str) -> Result<Vec<u8>>` (then `normalise`).

- [ ] Tests: `square_skin_round_trips` (64×64 pixels unchanged), `legacy_skin_gets_left_limbs` (64×32: pixel at right-leg front (4,20) appears mirrored in left-leg front region (16+?,52) per the game's conversion; rows 32–63 otherwise transparent), `wrong_size_is_skin_invalid` (32×32 → `ErrorCode::SkinInvalid`), `too_big_is_skin_invalid`, `not_png_is_skin_invalid`, `greyscale_png_normalises_to_rgba`, `data_url_round_trips`.
- [ ] Implement with `image` (decode with `Limits` 64×64 max side via guessed format, require PNG), legacy copy: for each of right leg (0,16) and right arm (40,16) boxes, copy each face rect into left leg (16,48) / left arm (32,48) with the face horizontally flipped and the inner/outer side faces swapped (game's `ImageBufferDownload` behaviour).
- [ ] `cargo test -p launcher-core skins_png` passes; commit `feat(skins): validate and normalise skin PNGs`.

### Task 2: Library on disk (launcher-core `skins_library`)

**Files:** Create `crates/launcher-core/src/skins_library.rs`; modify `paths.rs` (`pub fn skins(&self) -> PathBuf` → `<root>/skins`).

**Interfaces:** `#[serde(rename_all="snake_case")] pub enum SkinModel { Classic, Slim }`; `pub struct SkinEntry { id: String, name: String, model: SkinModel, updated: u64, worn_by: Option<String> }`; `pub struct SkinLibrary::new(dir)` with async `list() -> Result<Vec<SkinEntry>>` (newest `updated` first), `create(name, model, png) -> Result<SkinEntry>`, `texture(id) -> Result<Vec<u8>>`, `save_texture(id, png) -> Result<SkinEntry>`, `update(id, name: Option<String>, model: Option<SkinModel>) -> Result<SkinEntry>`, `duplicate(id, copy_name) -> Result<SkinEntry>`, `delete(id) -> Result<()>`, `mark_worn(id, uuid) -> Result<()>` (clears that uuid from every other entry). Unknown id → `SkinNotFound`. Names trimmed, 1–32 chars else `InvalidName`-style existing code (`ErrorCode::InvalidName` if present, else reuse the offline-name code).

- [ ] Tests (tempdir): `create_then_list_and_texture`, `save_texture_bumps_updated`, `duplicate_copies_texture`, `delete_removes_png`, `mark_worn_moves_lamp`, `unknown_id_is_not_found`, `damaged_library_starts_empty` (garbage json → `list()` is empty and `library.json.damaged` exists), `entry_without_png_is_dropped`.
- [ ] Implement with `fsutil` atomic writes; ids `skin-<12 hex>` from `rand`/time; textures pass through `skins_png::normalise` on every write.
- [ ] Commit `feat(skins): a local skin library`.

### Task 3: Minecraft services calls (launcher-core `msa`)

**Files:** Modify `crates/launcher-core/src/msa/minecraft.rs`; create `crates/launcher-core/tests/skins_http.rs`.

**Interfaces:** `pub struct ActiveSkin { pub url: String, pub model: SkinModel }`; `MsaClient::active_skin(&self, mc: &McToken) -> Result<Option<ActiveSkin>>` (GET `minecraft/profile`, the `skins[]` item whose `state == "ACTIVE"`, `variant` "SLIM" → Slim); `MsaClient::skin_texture(&self, url: &str) -> Result<Vec<u8>>` (only `https://textures.minecraft.net/…` unless the endpoints are a test base; capped at `MAX_SKIN_BYTES`; no auth header); `MsaClient::upload_skin(&self, mc: &McToken, png: Vec<u8>, model: SkinModel) -> Result<()>` (multipart `variant` = `classic`/`slim`, `file` = `skin.png` `image/png`).

- [ ] wiremock tests: `active_skin_reads_slim_variant`, `no_skins_is_none`, `upload_sends_multipart_with_bearer` (matcher on header and body containing `name="variant"` and `slim`), `upload_400_is_skin_rejected`, `upload_401_is_sign_in_again`, `upload_429_is_rate_limited`, `upload_500_is_host_trouble`, `texture_from_other_host_is_refused`.
- [ ] Implement next to `profile()`; reuse `request_failed`, `host_trouble`, `rate_limited`, `sign_in_again`, `read_capped`. The textures base is an `MsaEndpoints.textures: Url` field (`https://textures.minecraft.net` in production) so tests point it at wiremock.
- [ ] Commit `feat(skins): read the active skin and upload one`.

### Task 4: App methods (launcher-app `skins.rs`)

**Files:** Create `crates/launcher-app/src/skins.rs`; modify `msa.rs` (`MsaApi` gets `active_skin(token) -> BoxFuture<Result<Option<(Vec<u8>, SkinModel)>>>` and `upload_skin(token, png, model) -> BoxFuture<Result<()>>`), `testing.rs` (FakeMsa answers both, records uploads), `lib.rs`, `app.rs` (a `skins: SkinLibrary` field), `tests/contract.rs`.

**Interfaces:** `AppCore` methods: `skins_list() -> Result<Vec<SkinEntry>>`, `skin_texture(id) -> Result<String>` (data URL), `skin_create(name, model, texture: String) -> Result<SkinEntry>`, `skin_import(path: PathBuf) -> Result<SkinEntry>` (name from file stem), `skin_save(id, texture: String) -> Result<SkinEntry>`, `skin_update(id, name, model)`, `skin_duplicate(id, copy_name)`, `skin_delete(id)`, `skin_export(id, path: PathBuf) -> Result<PathBuf>` (adds `.png` when missing), `skin_current(uuid) -> Result<Option<CurrentSkin>>` where `CurrentSkin { texture: String, model: SkinModel }` (offline → `None`), `skin_wear(id, uuid) -> Result<Vec<SkinEntry>>`. **Seam:** `current_skin` in this file is the single place to switch to the avatar branch's `cache/skins/<uuid>.png` once merged.

- [ ] Tests (in-module, `testing` fakes): `wear_uploads_with_entry_model_and_marks_worn`, `wear_offline_is_skin_offline_account`, `current_offline_is_none`, `current_microsoft_is_data_url`, `import_legacy_file_becomes_64x64`, `export_adds_png_extension`.
- [ ] `skin_wear` uses `session_for` (same refresh as a launch) to get the token: `McToken { access_token: session.access_token, expires_at: u64::MAX }`.
- [ ] Contract: add `skin_entries` (two entries) and `current_skin`; regenerate.
- [ ] Commit `feat(skins): app commands for the skin library and wearing a skin`.

### Task 5: Tauri wiring and IPC client

**Files:** `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`, `ui/src/ipc/schema.ts` (zod/TS types `SkinEntry`, `SkinModel`, `CurrentSkin`), `ui/src/ipc/commands.ts`, `ui/src/ipc/contract.test.ts` (parse the two new fixtures), `ui/src/ipc/mock*` (mock backend for `dev:mock`, in memory).

- [ ] Commands named exactly as Task 4's methods, each through `logged(...)`.
- [ ] `pnpm --filter ui test -- contract` passes. Commit `feat(skins): wire the skin commands`.

### Task 6: Pure paint logic (UI)

**Files:** Create in `ui/src/features/skins/`: `layout.ts`, `mirror.ts`, `paint.ts`, `history.ts`, `starter.ts`, with `*.test.ts`.

**Interfaces:**
- `layout.ts`: `type Part = 'head'|'body'|'rightArm'|'leftArm'|'rightLeg'|'leftLeg'`; `type Layer = 'base'|'outer'`; `type FaceName = 'front'|'back'|'left'|'right'|'top'|'bottom'`; `interface Face { name: FaceName; x: number; y: number; w: number; h: number }`; `interface Box { part: Part; layer: Layer; size: [w,h,d]; origin: [x,y,z]; faces: Face[] }`; `boxes(model: SkinModel): Box[]`; `faceAt(model, x, y): { box: Box; face: Face } | null`.
- `mirror.ts`: `mirrorTexel(model, x, y): [number, number] | null`.
- `paint.ts`: `type Rgba = [r,g,b,a]`; `class Pixels { constructor(data?: Uint8ClampedArray); get(x,y): Rgba; set(x,y,c): void; clone(): Pixels }`; `type Tool = 'pencil'|'eraser'|'fill'|'picker'|'shade'|'noise'`; `applyTool(px, model, tool, x, y, opts: { color: Rgba; mirror: boolean; darken?: boolean; rand?: () => number }): Change[]` where `Change = { x, y, before: Rgba, after: Rgba }`; `pick(px, x, y): Rgba`.
- `history.ts`: `class History { push(stroke: Change[]); undo(px): boolean; redo(px): boolean; canUndo; canRedo }` capped at 200 strokes.
- `starter.ts`: `starterSkin(): Pixels`, `blankSkin(): Pixels` (flat grey mannequin, outer layer empty).

- [ ] Tests: `head_front_is_8_8_8_8`, `slim_arm_front_is_3_wide`, `faceAt_unused_area_is_null`, `mirror_head_front`, `mirror_slim_arms`, `mirror_body_back`, `pencil_writes_and_mirrors`, `eraser_on_base_writes_transparent` (base layer erases to transparent; the UI shows it as a hole), `fill_stays_inside_face`, `shade_darkens_by_step`, `noise_is_deterministic_with_rand`, `undo_redo_round_trip`, `history_cap`, `starter_has_opaque_base_layer` (every base texel used by `boxes('classic')` has alpha 255).
- [ ] Commit `feat(skins): pure paint, mirror and history logic`.

### Task 7: The Skins screen

**Files:** Create `ui/src/features/skins/SkinStudio.tsx` (screen, lazy default export), `Stage.tsx` + `stageScene.ts` (three.js adapter: `createStage(canvas, opts) → { setTexture(px), setModel, setVisible(part, layer, on), resetView(), pick(clientX, clientY) → {x,y}|null, dispose() }`), `TexturePanel.tsx`, `Hotbar.tsx`, `Wardrobe.tsx`, `ColourPanel.tsx`, `WearKey.tsx`, `useSkinDoc.ts` (pixels, history, autosave 600 ms), `skins.css` if Tailwind classes aren't enough; tests `SkinStudio.test.tsx`, `Hotbar.test.tsx`, `Wardrobe.test.tsx`, `TexturePanel.test.tsx` (mock `stageScene`). Modify `ui/src/state/route.ts` (`{ name: 'skins'; skin?: string }`), `ui/src/App.tsx` (lazy route), `features/titlebar/TitleBar.tsx` (account opens Skins), `features/accounts/Accounts.tsx` (Edit skin key), `ui/package.json` (`"three": "0.181.2"`, `"@types/three": "0.181.0"`), i18n `skins.*` in en/es, `errors.skin_*`.

- [ ] Tests: hotbar keys 1–9 select tools, mirror toggles with lime lamp (`aria-pressed`), Ctrl+Z/Ctrl+Y undo/redo; painting on the texture panel at a texel changes it and autosaves once (fake timers); wardrobe New → Starter creates an entry; delete asks first; offline account shows the explanation and no Wear key; Microsoft Wear calls `skinWear` and shows the worn lamp; a `SkinInvalid` import shows the translated error.
- [ ] Follow DESIGN.md and the direction contract; run the detector; screenshot at 960×600 and 1440×900 with `dev:mock`.
- [ ] Commit `feat(skins): the Skins screen`.

### Task 8: Finish

- [ ] Pre-commit list from CLAUDE.md; DESIGN.md gets a "Skins" component section (documenter); NOTICE unchanged (no ported code; three.js is a dependency).
- [ ] Push, draft PR, subscribe.
