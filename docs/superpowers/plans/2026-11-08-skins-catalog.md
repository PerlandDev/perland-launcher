# Skins catalog and character creator (launcher) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Catalog and Create tabs in the Skins screen: browse Perland's looks, put pieces on and off, recolour every area, save the look in the current account's wardrobe and wear the finished skin.

**Architecture:** launcher-core fetches and caches the Engine's catalog (manifest + PNGs by hash) and stores an optional `look` beside each wardrobe skin; `<id>.png` stays the finished image. launcher-app exposes four commands. The UI does recolouring and composition in pure TypeScript next to `paint.ts`, and the Create / Paint / Catalog tabs follow the approved mockups.

**Tech Stack:** Rust (reqwest, serde, tokio, wiremock tests), Tauri wiring, React + Vitest + Testing Library, three.js stage (unchanged).

**Spec:** `docs/superpowers/specs/2026-11-07-skins-catalog-design.md`, mockups in `docs/superpowers/specs/2026-11-07-skins-catalog-mockups/`. The Engine side is planned in PerlandEngine `docs/superpowers/plans/2026-11-08-skin-catalog-engine.md` (its Task 4 must be deployed for the live app; tests here use wiremock and mocks).

## Global Constraints

- Catalog format version 1; slots in drawing order `body`, `eyes`, `brows`, `mouth`, `face_detail`, `beard`, [painting], `bottom`, `shoes`, `top`, `belt`, `hands`, `neck`, `back`, `hair`, `glasses`, `hat`. One piece per slot.
- Recolour: `g == 128` → `c`; `g > 128` → `c + (255 - c) * (g - 128) / 127`; `g < 128` → `c * g / 128`; round half up. Same vectors as the Engine (Task 5 pins them).
- Mask red channel 0 fixed, 85/170/255 areas 1/2/3.
- Routes: `GET /api/v1/skin-catalog` (envelope, ETag), `GET /api/v1/skin-catalog/files/:sha256` (raw PNG).
- Caps: manifest 1 MiB; each PNG 64 KiB and exactly 64×64 (thumbnails 48×96); at most 4000 files; every file's SHA-256 must equal its name.
- Cache at `<root>/cache/skin-catalog/` (`manifest.json`, `files/<sha256>.png`), atomic writes (`fsutil::write_atomic`).
- Wardrobe: optional `look` on `SkinEntry`; painting at `<root>/skins/<id>.paint.png`; `<id>.png` is always the finished PNG written with the look under the wardrobe's lock.
- `crates/*` stay free of Tauri; no token is sent to the catalog routes; never log URLs with tokens (none here).
- Every UI string in `ui/src/i18n/en.json` and `es.json`; new `ErrorCode` `SkinCatalogUnavailable` with `errors.skin_catalog_unavailable.message` in both.
- Contract JSON regenerated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, never hand-edited.
- UI styling through `/impeccable`, DESIGN.md unchanged; the Catalog tab's one amber key is **Add to my wardrobe**, the stage keeps **Wear this skin**.
- Do not release a launcher build with the Catalog and Create tabs until the Engine holds the full agreed set (spec: content batches); releases need the owner's go-ahead.
- Before each commit: `pnpm --filter ui build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`.

## Review Focus

- The Engine answers a manifest with a newer `formatVersion`: keep using the cached one and show the "update the launcher" line; never wipe the cache.
- A file whose bytes don't match its hash (proxy corruption, tampering): discarded, not cached, the piece shows as unavailable; the rest of the catalog works.
- A wardrobe look whose piece was retired or whose files are no longer in the newest manifest: still recomposes from cached files; pruning keeps every hash any wardrobe look needs.
- Offline on first visit (no cache): Catalog and Create show the plain message, Paint and Wear work exactly as today.
- Restoring a history version or making a copy from one on a skin with a look: the result is a plain painted skin and its `.paint.png` is gone, so the next save can't resurrect stale pieces.

---

### Task 1: Catalog types and client

**Files:**
- Create: `crates/launcher-core/src/skin_catalog/mod.rs`, `crates/launcher-core/src/skin_catalog/manifest.rs`, `crates/launcher-core/src/skin_catalog/client.rs`
- Modify: `crates/launcher-core/src/lib.rs`, `crates/launcher-core/src/error.rs` (`ErrorCode::SkinCatalogUnavailable`)
- Test: `crates/launcher-core/tests/skin_catalog.rs`

**Interfaces:**
- Produces:
  - Serde types mirroring the Engine manifest (camelCase): `CatalogText { en, es }`, `CatalogArea { id, name, default }`, `CatalogFiles { art, mask }`, `CatalogPiece { id, slot, name, tags, areas, files: BTreeMap<String, CatalogFiles>, thumb, retired }`, `LookPiece { id, colours: BTreeMap<String, String> }`, `CatalogLook { id, name, tags, model: SkinModel, pieces, painting: Option<String>, thumb, retired }`, `CatalogTag { id, name }`, `Manifest { format_version, version, license, slots, tags, pieces, looks }` (all `Serialize + Deserialize + Clone + PartialEq`).
  - `pub const FORMAT_VERSION: u32 = 1;`
  - `impl Manifest { pub fn hashes(&self) -> BTreeSet<String> }` (every art, mask, thumb, painting hash)
  - `pub enum Fetched { NotModified, Manifest(Manifest) }`
  - `pub struct CatalogClient { /* InviteClient + site Url, like PublicListClient */ }`
  - `impl CatalogClient { pub fn new(invites: InviteClient, site: Url) -> Self; pub async fn manifest(&self, etag: Option<&str>) -> Result<Fetched>; pub async fn file(&self, sha256: &str) -> Result<Vec<u8>> }`

- [ ] **Step 1: Write the failing tests** (wiremock; the Engine is found through the site's `config.json` the way `tests/public_list.rs` does)

```rust
#[tokio::test] async fn manifest_is_read_from_the_envelope()          // 200 → Fetched::Manifest with format_version 1
#[tokio::test] async fn manifest_sends_if_none_match_and_reads_304()  // header `"v1"` sent; 304 → NotModified
#[tokio::test] async fn manifest_over_one_mib_is_refused()            // SkinCatalogUnavailable
#[tokio::test] async fn file_must_hash_to_its_name()                  // wrong bytes → SkinCatalogUnavailable, param host only
#[tokio::test] async fn file_must_be_a_png_within_64_kib()
#[tokio::test] async fn bad_hash_is_refused_before_any_request()      // "../x", upper case, 63 chars → error, no request recorded
#[tokio::test] async fn engine_down_is_unavailable_naming_the_host()
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p launcher-core --test skin_catalog`
Expected: FAIL (module missing)

- [ ] **Step 3: Implement** with `http::read_capped` and the `invite::client` helpers `public_list.rs` uses (`check_engine_url`, `host_trouble`, `rate_limited`); PNG dimension check with the `png` crate already used by `skins_png`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p launcher-core --test skin_catalog`
Expected: PASS

- [ ] **Step 5: Commit** `feat(skins): catalog client for the Engine's skin catalog`

### Task 2: Catalog cache

**Files:**
- Create: `crates/launcher-core/src/skin_catalog/cache.rs`
- Test: `#[cfg(test)]` in `cache.rs`

**Interfaces:**
- Consumes: Task 1 types.
- Produces:
  - `pub enum CatalogStatus { Fresh, Cached, Unavailable, TooNew }` (serde snake_case)
  - `pub struct CatalogCache { dir: PathBuf }`; `impl CatalogCache { pub fn new(dir) -> Self; pub async fn manifest(&self) -> Option<Manifest>; pub async fn store_manifest(&self, m: &Manifest) -> Result<()>; pub async fn file(&self, sha256: &str) -> Option<Vec<u8>>; pub async fn store_file(&self, sha256: &str, png: &[u8]) -> Result<()>; pub async fn prune(&self, keep: &BTreeSet<String>) -> Result<usize> }`
  - `pub struct SkinCatalog { client: CatalogClient, cache: CatalogCache }`; `impl SkinCatalog { pub async fn load(&self) -> (Option<Manifest>, CatalogStatus); pub async fn file(&self, sha256: &str) -> Result<Vec<u8>>; pub async fn prune(&self, wardrobe_keep: &BTreeSet<String>) -> Result<usize> }`

`load` rules: fetch with the cached manifest's version as ETag; `NotModified` → (cached, Fresh); new manifest with `format_version > FORMAT_VERSION` → (cached or None, TooNew), cache untouched; new valid manifest → store, (new, Fresh); fetch error → (cached, Cached) or (None, Unavailable). `file`: cache hit, else fetch, verify, store. `prune` keeps the newest manifest's hashes plus `wardrobe_keep`.

- [ ] **Step 1: Write the failing tests:** `load_uses_cache_when_offline`, `load_keeps_cache_for_newer_format`, `load_stores_new_manifest`, `file_is_cached_after_first_fetch`, `corrupt_cached_file_is_refetched` (cached bytes not matching the hash are deleted and fetched), `prune_keeps_wardrobe_hashes`, `damaged_manifest_json_counts_as_no_cache`.

- [ ] **Step 2: Run** `cargo test -p launcher-core skin_catalog::cache` → FAIL.

- [ ] **Step 3: Implement** (client behind a small trait so tests inject a fake).

- [ ] **Step 4: Run** → PASS.

- [ ] **Step 5: Commit** `feat(skins): cache the skin catalog for offline use`

### Task 3: Looks in the wardrobe

**Files:**
- Modify: `crates/launcher-core/src/skins_library.rs`, `crates/launcher-core/src/skins_library/versions.rs`
- Test: `#[cfg(test)]` in both files

**Interfaces:**
- Produces:
  - `pub struct WornPiece { pub id: String, pub colours: BTreeMap<String, String> }`, `pub struct SkinLook { pub pieces: Vec<WornPiece>, pub painting: bool }` (camelCase serde)
  - `SkinEntry.look: Option<SkinLook>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`; list `version` stays 2)
  - `impl SkinLibrary { pub async fn save_look(&self, id: &str, look: SkinLook, painting: Option<&[u8]>, texture: &[u8]) -> Result<SkinEntry>; pub async fn clear_look(&self, id: &str) -> Result<SkinEntry>; pub async fn painting(&self, id: &str) -> Result<Option<Vec<u8>>>; pub async fn look_hashes(&self, manifest: &Manifest) -> BTreeSet<String> }` (`look_hashes`: every file of every piece any entry's look wears, across all owners)

Rules: `save_look` normalises both PNGs through `skins_png`, writes `<id>.paint.png` (or removes it when `painting` is `None`), `<id>.png` and the list under one lock; it takes a version like `save_texture` does. `clear_look` drops `look` and the paint file, keeping `<id>.png` as the plain painting. `delete` also removes `<id>.paint.png`; `duplicate` copies it and the look. `versions::restore` and `version_copy` leave a skin without a look and without `<id>.paint.png`. More than 16 pieces or two of one slot are `SkinInvalid` (slot read from the id via the caller is not available here, so the limit is 16 pieces and unique ids; slot uniqueness is the UI's job and Task 4's check).

- [ ] **Step 1: Write the failing tests:** `save_look_writes_three_files_together`, `save_look_without_painting_removes_paint_file`, `clear_look_keeps_finished_png`, `delete_removes_paint_file`, `duplicate_copies_look_and_painting`, `restore_drops_look` (in versions.rs), `old_list_without_look_loads` (a version-2 file from today's tests), `look_hashes_cover_all_owners`.

- [ ] **Step 2: Run** `cargo test -p launcher-core skins_library` → FAIL.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run** → PASS.

- [ ] **Step 5: Commit** `feat(skins): keep a look of catalog pieces beside a wardrobe skin`

### Task 4: Commands, wiring and contract

**Files:**
- Modify: `crates/launcher-app/src/skins.rs`, `crates/launcher-app/src/runner.rs` (build `SkinCatalog` next to `PublicListClient`, cache dir `<root>/cache/skin-catalog`), `src-tauri/src/` command registration, `crates/launcher-app/tests/contract.rs` (samples), `ui/src/ipc/commands.ts`, `ui/src/ipc/mockSkins.ts`, `ui/src/i18n/en.json`, `ui/src/i18n/es.json`
- Generated: `ui/src/ipc/contract/*.json`

**Interfaces:**
- Consumes: Tasks 1–3.
- Produces (Rust and TS names identical):
  - `skin_catalog() -> { manifest: Manifest | null, status: CatalogStatus }`
  - `skin_catalog_file(sha256) -> string` (data URL)
  - `skin_painting(id) -> string | null` (data URL)
  - `skin_look_save(id, look: SkinLook, painting: string | null, texture: string) -> SkinEntry` (data URLs)
  - `skin_look_clear(id) -> SkinEntry`
  - After a successful `skin_catalog` with a new manifest, prune in the background with `look_hashes`.
  - TS types `CatalogManifest`, `CatalogPiece`, `CatalogLook`, `CatalogStatus`, `SkinLook`, `WornPiece`; `SkinEntry.look?: SkinLook`.
  - Mock (`mockSkins.ts`): an in-memory manifest with the Engine's first-batch ids (`body-plain`, `eyes-round`, `brows-straight`, `mouth-smile`, `hair-short`, `top-tee`, `top-bomber`, `bottom-jeans`, `shoes-sneakers`, `hat-cap`, `glasses-round`, `neck-scarf`; looks `look-street`, `look-knight`, `look-astronaut`) with tiny generated PNGs, so `pnpm --filter ui dev:mock` shows the tabs.

- [ ] **Step 1: Write the failing tests:** launcher-app unit tests `skin_catalog_reports_status`, `skin_look_save_round_trips` (data URLs in, entry with look out), `skin_painting_none_without_look`; add the new types to the contract samples.

- [ ] **Step 2: Run** `cargo test -p launcher-app` → FAIL.

- [ ] **Step 3: Implement**, wire in `src-tauri`, add the TS command wrappers and mock cases, add `errors.skin_catalog_unavailable.message` (en: "Couldn't reach Perland's skin catalog. Check your connection and try again."; es: "No pudimos conectar con el catálogo de skins de Perland. Revisa tu conexión e inténtalo de nuevo.").

- [ ] **Step 4: Regenerate and run**

Run: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract && cargo test --workspace && pnpm -r typecheck`
Expected: PASS, contract files updated

- [ ] **Step 5: Commit** `feat(skins): catalog and look commands`

### Task 5: Recolour, compose and looks (pure UI logic)

**Files:**
- Create: `ui/src/features/skins/recolour.ts`, `compose.ts`, `look.ts`, `catalogSearch.ts`
- Test: `recolour.test.ts`, `compose.test.ts`, `look.test.ts`, `catalogSearch.test.ts`

**Interfaces:**
- Consumes: `layout.ts` (UV rects), Task 4 TS types.
- Produces:
  - `type Rgba = Uint8ClampedArray` (64×64×4, as `paint.ts` uses)
  - `recolour(art: Rgba, mask: Rgba, colours: [string?, string?, string?]): Rgba`
  - `SLOTS: readonly string[]` (order above), `FACE_SLOTS = ['body','eyes','brows','mouth','face_detail','beard']`
  - `compose(painting: Rgba | null, layers: { slot: string; art: Rgba; mask: Rgba; colours: string[] }[]): Rgba`
  - `putOn(look: SkinLook, piece: CatalogPiece): SkinLook` (replaces the piece in the same slot; colours start at the areas' defaults); `takeOff(look, pieceId): SkinLook`; `setColour(look, pieceId, areaId, hex): SkinLook`; `applyLook(look: SkinLook, catalogLook: CatalogLook): SkinLook` (replaces pieces, keeps `painting`); `bakeAll(painting: Rgba | null, composed: Rgba): { painting: Rgba; look: SkinLook }` (painting = composed, no pieces)
  - `searchLooks(manifest, query, tag, lang): CatalogLook[]` and `searchPieces(manifest, slot, query, lang): CatalogPiece[]` (case- and accent-insensitive, retired hidden unless worn)

- [ ] **Step 1: Write the failing tests**

```ts
test('recolour matches the Engine vectors', () => {
  // colour #c0392b; grey 128 → [192,57,43]; 255 → [255,255,255]; 0 → [0,0,0]; 191 → [223,155,148]; 64 → [96,29,22]
});
test('fixed pixels and alpha are kept', ...);
test('painting sits above body and face, below clothes', ...); // same setup as the Engine's TestCompose…
test('putOn replaces the piece in the same slot', ...);
test('putOn starts with the area defaults', ...);
test('applyLook keeps the painting', ...);
test('bakeAll leaves no pieces and the composed pixels as painting', ...);
test('search ignores case and accents in the current language', ...); // "camiseta" finds "Camiseta" (es); "TEE" finds "Tee"
test('retired pieces are hidden unless worn', ...);
```

- [ ] **Step 2: Run** `pnpm --filter ui test -- recolour compose look catalogSearch` → FAIL.

- [ ] **Step 3: Implement.** Rounding `Math.floor(v + 0.5)`.

- [ ] **Step 4: Run** → PASS.

- [ ] **Step 5: Commit** `feat(skins): recolour and compose catalog pieces`

### Task 6: Catalog data hook and the Catalog tab

**Files:**
- Create: `ui/src/features/skins/useCatalog.ts`, `ui/src/features/skins/CatalogPanel.tsx`
- Modify: `ui/src/features/skins/SkinStudio.tsx` (tabs Create / Paint / Catalog in the head, as in `create-tops.png`), `skins.css`, `en.json`, `es.json`
- Test: `ui/src/features/skins/CatalogPanel.test.tsx`, `SkinStudio.test.tsx`

**Interfaces:**
- Consumes: Task 4 commands, Task 5 `searchLooks`, `compose`, `recolour`.
- Produces: `useCatalog(): { manifest: CatalogManifest | null; status: CatalogStatus; image(sha256): string | undefined; layers(look: SkinLook, model): Promise<Layer[]> }` (images loaded on demand and memoised; thumbnails first). `CatalogPanel` props `{ onAdded(entry: SkinEntry): void }`.

Behaviour (per `catalog.png`): search box, tag chips from the manifest (first chip "All"), grid of look thumbnails, preview panel with the look on a posable figure, its pieces as chips, amber **Add to my wardrobe**: composes the look on its model, calls `skin_create(name, model, texture, owner = shown account)` then `skin_look_save`, selects the new skin and switches to Create. Status `unavailable` with no manifest: a plain message (key `skins.catalog.unavailable`), `cached`: a quiet "Showing the saved catalog" line, `too_new`: "Update the launcher to see the newest pieces."

- [ ] **Step 1: Write the failing tests:** `shows looks and filters by tag`, `searching narrows the grid`, `add to my wardrobe creates a skin for the shown account and saves its look` (asserts `skin_create` called with the account uuid as owner, then `skin_look_save`), `unavailable without cache shows the message and Paint still works`, `tabs switch between Create, Paint and Catalog`.

- [ ] **Step 2: Run** `pnpm --filter ui test -- CatalogPanel SkinStudio` → FAIL.

- [ ] **Step 3: Implement** with `/impeccable` for styling against DESIGN.md and the mockups.

- [ ] **Step 4: Run** → PASS.

- [ ] **Step 5: Commit** `feat(skins): Catalog tab`

### Task 7: The Create tab

**Files:**
- Create: `ui/src/features/skins/CreatePanel.tsx`, `ui/src/features/skins/AreaColours.tsx`, `ui/src/features/skins/WearingRow.tsx`, `ui/src/features/skins/useLook.ts`
- Modify: `SkinStudio.tsx`, `useSkinDoc.ts` (the finished texture is `compose(painting, layers)` when the skin has a look), `history.ts` (an entry kind for look changes), `Wardrobe.tsx` (New → "Build from pieces", starting with `body-plain`), `skins.css`, `en.json`, `es.json`
- Test: `CreatePanel.test.tsx`, `useLook.test.ts`

**Interfaces:**
- Consumes: Tasks 4–6.
- Produces: `useLook(id, model): { look: SkinLook | null; painting: Rgba | null; putOn(piece); takeOff(id); setColour(pieceId, areaId, hex); applyLook(catalogLook); bakeAll(); }`. Every change recomposes the stage texture at once and autosaves through `skin_look_save` 600 ms after the last change (the same debounce as strokes); undo/redo restores the previous `{look, painting}`.

Behaviour (per `create-tops.png`, `create-face.png`): category list (Looks, Body, Face, Hair, Tops, Bottoms, Shoes, Hats, Accessories → slots: Body `body`; Face `eyes`, `brows`, `mouth`, `face_detail`, `beard` as sub-groups; Hair `hair`; Tops `top`; Bottoms `bottom`; Shoes `shoes`; Hats `hat`; Accessories `glasses`, `neck`, `back`, `hands`, `belt`), piece grid with an ON badge for worn ones, area colours for the selected worn piece (quick colours, system picker, hex field reusing `ColourPanel`'s picker), **Take off**, **Make part of the skin** (confirm, then `bakeAll`, one undo step). Looks category applies a catalog look after a confirm when worn pieces would be replaced. **Wearing** row under the stage: one chip per piece with ×.

- [ ] **Step 1: Write the failing tests:** `putting on a top replaces the previous top`, `changing an area colour updates the stage texture`, `take off removes the piece and the chip`, `make part of the skin asks, then leaves no pieces and one undo brings them back`, `changes autosave once after 600 ms`, `build from pieces starts with the plain body`, `a piece missing from the catalog stays worn and is labelled unavailable`.

- [ ] **Step 2: Run** `pnpm --filter ui test -- CreatePanel useLook` → FAIL.

- [ ] **Step 3: Implement** (styling via `/impeccable`).

- [ ] **Step 4: Run** → PASS.

- [ ] **Step 5: Commit** `feat(skins): Create tab with pieces and colours`

### Task 8: Paint under pieces, then the whole-branch check

**Files:**
- Modify: `useSkinDoc.ts` (strokes edit the painting when the skin has a look; eyedropper reads the composed texture), `TexturePanel.tsx`, `Stage.tsx` (no changes to the three.js scene; it shows the composed texture), `HistoryPanel.tsx` (restoring a version on a skin with a look says it becomes a plain painted skin), `en.json`, `es.json`
- Test: `SkinStudio.test.tsx`, `paint.test.ts` (unchanged), new cases in `useLook.test.ts`

**Interfaces:**
- Consumes: Task 7 `useLook`.

- [ ] **Step 1: Write the failing tests:** `painting a skin with pieces edits the painting under them`, `a stroke hidden by a piece shows the pieces-on-top hint`, `eyedropper picks the composed colour`, `restoring a version of a skin with pieces warns and clears the look`, `skins without a look paint exactly as before` (existing tests stay green).

- [ ] **Step 2: Run** `pnpm --filter ui test` → FAIL on the new cases.

- [ ] **Step 3: Implement.** Hint copy (en): "Pieces are on top. Take them off or make them part of the skin to paint over them." (es): "Las piezas están encima. Quítalas o hazlas parte de la skin para pintar sobre ellas."

- [ ] **Step 4: Full check**

Run: `pnpm --filter ui build && cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm -r lint && pnpm -r typecheck && pnpm -r test`
Expected: all pass. Then `pnpm --filter ui dev:mock`, open Skins, and walk through Catalog → Add to my wardrobe → Create (put on, recolour, take off, make part of the skin) → Paint → Wear; capture screenshots for the PR.

- [ ] **Step 5: Commit** `feat(skins): paint under catalog pieces`
