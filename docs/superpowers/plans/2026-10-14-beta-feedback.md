# Closed-beta feedback (#4–#10) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the closed-beta reports filed from Discord: no browser menu or DevTools in shipped builds (#4, #6), renaming an instance renames its folder (#5), an expandable rail with names (#9, #10), drag-to-reorder (#7), and a per-instance image picker with presets and uploads (#8).

**Architecture:** Two PRs off `main`.
- **PR 1 (#4/#6):** a UI guard for the context menu and browser shortcuts, plus devtools gated on `cfg(dev)` when the window is built in `src-tauri`.
- **PR 2 (the rest):**
  - core: `InstanceStore::rename` moves the folder; an `instance_order` module; `art_seed` and `art` on `Instance`;
  - app: applies the order, cascades a rename, resolves the art;
  - UI: rail, reorder and picker.

**Tech Stack:** Rust (tokio, serde), Tauri 2.12, React 19 + TanStack Query + zod, Vitest + Testing Library, i18next.

**Spec:** the owner-approved in-chat design from 2026-10-14, recorded below as decisions. Issues PerlandDev/perland-launcher#4–#10.

## Owner decisions (2026-10-14)

- **#5:** renaming moves the folder. It is refused while the instance is busy, installing or locked. If Windows reports the folder in use, nothing changes and the player gets `instance_folder_in_use`. The generated art keeps its look through `artSeed`.
- **#9/#10:** a rail key toggles compact (4.5rem) ↔ wide (~15rem: icon, name, status word). The choice is remembered in `settings.json` (`railExpanded`).
- **#7:** drag to reorder in both rail modes. Alt+↑/↓ does the same from the keyboard. New instances land at the **top**. Existing instances keep A–Z until moved. Damaged instances are always last.
- **#8:** "Change image…" in the ⋯ menu opens a picker:
  - 6 generated landscapes (sets `artSeed`);
  - "Pack art", for pack instances;
  - "Upload your own": PNG/JPEG/WebP, ≤ `MAX_ART_BYTES` (5 MiB), sniffed, copied to `.perland/custom/`.

  One picture fills the rail icon (cropped square) and the marquee.
- **Two PRs:** #4/#6 first, then the rest.
- **Design deviation, recorded:** the instance order lives in its own file, `<root>/instance-order.json`, not in `Settings`. The UI's `settings_set` sends the whole `Settings` object, so a stale copy would overwrite the order on a language change.

## Execution notes (2026-10-14)

- **PR 1 (Tasks 1–2)** is done and open as PerlandDev/perland-launcher#18 (branch `claude/beta-feedback-shell`, not merged).
  - Its final review moved the guard from the rule in Task 1 to "menu only on text-entry fields": text/search/email/url/tel/password/number inputs, textareas and contenteditable.
  - It added F3 to the blocked keys.
  - PR 2 must not touch `ui/src/shell/` or `src-tauri/src/main.rs` window creation, apart from registering new commands.
- **PR 2 (Tasks 3–11)** runs subagent-driven in a cloud session on branch `claude/beta-feedback-instances`. That branch holds this plan, off `main`.
  - **Why the cloud:** Windows Smart App Control blocks `rustc.exe` on the owner's PC.
  - **Fresh agents per task:** fresh implementer and reviewer per task, and a whole-branch review at the end on the most capable model.
  - **Ledger:** `.superpowers/sdd/2026-10-14-beta-feedback/progress.md` is git-ignored. Record rulings there, then copy them into the PR body.
- **`/impeccable`:** it is the owner's personal skill and may be missing in the cloud. If it is, build Tasks 8–10 straight from `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`, and record in the PR that an `/impeccable` finish pass is still owed locally.
- **Not decided, so don't build:** a Copy button on the log view and error cards. Right-click Copy on selectable text went away in PR 1, and Ctrl+C still works.
- **Reporters:** do not post on the issues; the owner's local session handles reporter updates. The PR body carries `Fixes #5`, `Fixes #7`, `Fixes #8`, `Fixes #9` and `Fixes #10`.

## Global Constraints

- `crates/launcher-core` and `crates/launcher-app` never depend on Tauri. `src-tauri` is wiring only and has no tests.
- TDD: Rust unit tests in `#[cfg(test)]`; UI uses Vitest + Testing Library.
- Every UI string goes in `ui/src/i18n/en.json` and `ui/src/i18n/es.json` (neutral Latin American Spanish, "tú"). Every new `ErrorCode` needs `errors.<code>.message` in both.
- `ui/src/ipc/contract/*.json` is regenerated with `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. It is never hand-edited.
- UI styling goes through `/impeccable`, against `DESIGN.md` and `.impeccable/surfaces/ui-src-app-tsx.md`. The minimum window is 960 × 600, and Spanish runs about 30% longer.
- `tauri.conf.json` is shared with the Plan 6 session. PR 1's only change there is `"create": false`.
- Never run fuzz, stress or load tests locally. Keep test inputs tiny (art tests use a few bytes, plus one sparse file just over 5 MiB).
- Tokens never reach logs.
- Commit trailer, exactly: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Subagent prompts must repeat it verbatim.
- Gate before every commit that ends a task:
  - `pnpm --filter ui build`
  - `cargo fmt --all`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `pnpm -r lint`
  - `pnpm -r typecheck`
  - `pnpm -r test`

  Windows Smart App Control error 4551 is environmental: retry.

## Review Focus

1. **Folder open in Explorer:** renaming while Explorer (or an antivirus) holds the folder must leave the name, folder and `instance.json` unchanged, and show `instance_folder_in_use`. Task 3 tests `rename_io_error` for `PermissionDenied` and raw OS errors 5 and 32.
2. **Same-family names:** renaming to a name whose slug matches the current family ("Asalith Fields" → "asalith fields!", or `foo-2` → "Foo" while `foo` exists) must not move anything. Renaming onto another instance's slug gets the next `-N`. Task 3 tests both.
3. **Crash mid-rename:** a crash between the folder move and the `instance.json` rewrite must still load as a good instance. `load()` trusts the folder name (Task 3).
4. **Settings saves vs. order:** a settings save (language toggle) or the CLI creating an instance must never lose or scramble the custom order. The order file is separate, and unlisted instances float to the top (Tasks 4 and 5).
5. **Bad uploads:** an `.exe` renamed to `.png`, an SVG, a GIF, a 6 MiB PNG or a vanished path is refused with `art_file_invalid`, and the current image is untouched. A pack sync never overwrites a custom image (Task 6).

---

## PR 1: branch `claude/beta-feedback-shell` off `origin/main`

### Task 1: Browser guard (context menu and shortcuts)

**Files:**
- Create: `ui/src/shell/browserGuard.ts`, `ui/src/shell/browserGuard.test.ts`
- Modify: `ui/src/main.tsx` (install the guard when `!import.meta.env.DEV`)

**Interfaces:**
- Produces: `installBrowserGuard(target: Document): () => void`, which returns an uninstall function.

**Rules:**
- **`contextmenu`:** `preventDefault()` unless the event target is inside `input`, `textarea`, `select` or `[contenteditable]`. Text fields keep Cut, Copy and Paste.
- **`keydown`, capture phase:** `preventDefault()` for:
  - `F5`
  - `F7` (caret browsing)
  - Ctrl/⌘ + `R`, `P`, `S`, `U`, `F`, `G`, `J`, `H`
  - Ctrl+Shift+`R`

  Never block plain typing, Ctrl+C/V/X/A/Z/Y, Tab or arrows.

- [ ] **Step 1: Write the failing tests**

```ts
import { afterEach, expect, it } from 'vitest';
import { installBrowserGuard } from './browserGuard';

let uninstall = () => {};
afterEach(() => { uninstall(); document.body.innerHTML = ''; });

function fire(target: Element, event: Event) {
  target.dispatchEvent(event);
  return event.defaultPrevented;
}
const menu = () => new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
const key = (init: KeyboardEventInit) => new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });

it('blocks the page context menu but keeps it in text fields', () => {
  uninstall = installBrowserGuard(document);
  document.body.innerHTML = '<div id="d"><span id="s">x</span></div><input id="i"><textarea id="t"></textarea><div contenteditable="true"><b id="c">x</b></div>';
  expect(fire(document.getElementById('s')!, menu())).toBe(true);
  expect(fire(document.getElementById('i')!, menu())).toBe(false);
  expect(fire(document.getElementById('t')!, menu())).toBe(false);
  expect(fire(document.getElementById('c')!, menu())).toBe(false);
});

it('blocks reload, print, save, find and view-source, not editing keys', () => {
  uninstall = installBrowserGuard(document);
  const body = document.body;
  for (const k of [{ key: 'F5' }, { key: 'r', ctrlKey: true }, { key: 'p', ctrlKey: true }, { key: 's', ctrlKey: true }, { key: 'u', ctrlKey: true }, { key: 'f', ctrlKey: true }, { key: 'R', ctrlKey: true, shiftKey: true }, { key: 'p', metaKey: true }])
    expect(fire(body, key(k)), JSON.stringify(k)).toBe(true);
  for (const k of [{ key: 'a' }, { key: 'c', ctrlKey: true }, { key: 'v', ctrlKey: true }, { key: 'a', ctrlKey: true }, { key: 'z', ctrlKey: true }, { key: 'Tab' }, { key: 'ArrowDown' }])
    expect(fire(body, key(k)), JSON.stringify(k)).toBe(false);
});

it('uninstalls cleanly', () => {
  installBrowserGuard(document)();
  document.body.innerHTML = '<div id="d"></div>';
  expect(fire(document.getElementById('d')!, menu())).toBe(false);
});
```

- [ ] **Step 2:** Run `pnpm --filter ui test -- browserGuard`. Expected: FAIL (module not found).
- [ ] **Step 3: Implement**

```ts
/** Shipped builds are an app, not a browser page: no page menu (Save as, Print, Inspect) and no browser shortcuts. Text fields keep their menu for copy and paste. */
const EDITABLE = 'input, textarea, select, [contenteditable=""], [contenteditable="true"]';
const CTRL_KEYS = new Set(['r', 'p', 's', 'u', 'f', 'g', 'j', 'h']);

function blockedKey(e: KeyboardEvent): boolean {
  if (e.key === 'F5' || e.key === 'F7') return true;
  return (e.ctrlKey || e.metaKey) && !e.altKey && CTRL_KEYS.has(e.key.toLowerCase());
}

export function installBrowserGuard(target: Document): () => void {
  const onMenu = (e: MouseEvent) => {
    if (!(e.target instanceof Element && e.target.closest(EDITABLE))) e.preventDefault();
  };
  const onKey = (e: KeyboardEvent) => {
    if (blockedKey(e)) e.preventDefault();
  };
  target.addEventListener('contextmenu', onMenu);
  target.addEventListener('keydown', onKey, true);
  return () => {
    target.removeEventListener('contextmenu', onMenu);
    target.removeEventListener('keydown', onKey, true);
  };
}
```

In `ui/src/main.tsx`, before rendering: `if (!import.meta.env.DEV) installBrowserGuard(document);` Developers on the Vite dev server (`pnpm tauri dev`, `dev:mock`) keep the full menu and Inspect.

- [ ] **Step 4:** Run the tests again. Expected: PASS.
- [ ] **Step 5:** Commit: `fix(ui): no browser page menu or shortcuts outside dev (#4, #6)`.

### Task 2: DevTools only in `tauri dev`

**Files:**
- Modify: `src-tauri/tauri.conf.json` (add `"create": false` to the `main` window, nothing else), `src-tauri/src/main.rs` (in `setup`, first thing).

Why: tester builds set `CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true`, and Tauri turns devtools on whenever `debug_assertions` is set. `cfg(dev)` (emitted by tauri-build) is true only without the `custom-protocol` feature, which means `tauri dev` and never `tauri build`.

- [ ] **Step 1: Implement** (`src-tauri` has no tests by rule)

```rust
// In setup, before anything that looks the window up:
let config = app
    .config()
    .app
    .windows
    .iter()
    .find(|w| w.label == "main")
    .ok_or("the main window is missing from tauri.conf.json")?
    .clone();
// DevTools only while developing (`tauri dev`), even in tester builds with debug assertions on.
tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
    .devtools(cfg!(dev))
    .build()?;
```

- [ ] **Step 2:** Check that `pnpm tauri dev` still opens the window with its saved size and position (window-state), Inspect works, and a second launch focuses the window.
- [ ] **Step 3:** Run the full gate, commit `fix(app): DevTools only in tauri dev, also for tester builds (#6)`, rebase on `origin/main`, then push and open PR 1 with `Fixes #4` and `Fixes #6`. Smoke-check list for the owner: on a tester build (`CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true pnpm tauri build`):
  - right-clicking the page shows nothing;
  - right-clicking the rename field shows Cut, Copy and Paste with no Inspect;
  - F12, Ctrl+Shift+I, Ctrl+P and F5 do nothing.

---

## PR 2: branch `claude/beta-feedback-instances` off `origin/main`

### Task 3: Core rename moves the folder

**Files:**
- Modify: `crates/launcher-core/src/instances.rs`, `crates/launcher-core/src/error.rs` (`InstanceFolderInUse` → `"instance_folder_in_use"`, in the "packs and sync" group next to `InstanceLocked`), `ui/src/i18n/{en,es}.json`.

**Interfaces:**
- Produces:
  - `Instance.art_seed: Option<String>` (`#[serde(default)]`, camelCase `artSeed`)
  - `Instance.art: ArtMode` (`#[serde(default)]`; `enum ArtMode { #[default] Auto, Generated, Custom }`, lowercase)
  - `InstanceStore::rename(&self, id: &str, name: &str) -> Result<Instance>` (the returned `id` may differ)
  - `pub fn same_family(id: &str, base: &str) -> bool`
  - `fn rename_io_error(e: std::io::Error, id: &str) -> LauncherError`

**Behaviour:**
- `load(id)` sets `instance.id = id` after parsing: the folder is the truth.
- `rename`:
  1. Load and validate the name. Set `base = slugify(name)`.
  2. If `same_family(id, base)` (id == base, or id == `{base}-{N}` with N ≥ 2), save the name only.
  3. Otherwise pick the first free `base`, `base-2`, …, and run `InstanceLock::try_acquire(perland_dir(id))?`. Drop the lock before the move: Windows can't rename a folder with a file open in it.
  4. `tokio::fs::rename(old, new)`, mapping errors through `rename_io_error`.
  5. Set `instance.id = new`; `art_seed.get_or_insert(old id)`; set the name; `save`. If the save fails, rename back and return the save error.
- `rename_io_error`: `PermissionDenied`, or raw OS error 5 or 32 →
  - `LauncherError::disk(ErrorCode::InstanceFolderInUse, "Another program is using this instance's folder")`
  - `.with_fix("Close any window or program using the folder, then try again")`
  - `.with_param("id", id)`

  Anything else → `disk_write_failed` with the detail.

- [ ] **Step 1: Write failing tests** (in `instances.rs` tests):
  - `rename_moves_the_folder_and_keeps_the_art`: create "Pack" → rename to "Asalith Fields". Expect id `asalith-fields`, `instances/pack` gone, `instances/asalith-fields/minecraft` and `.perland` present, `art_seed == Some("pack")`, and `load("asalith-fields")` equal to the result.
  - `rename_within_the_family_keeps_the_folder`: "Asalith Fields" → "asalith fields!" keeps the id. With `foo` and `foo-2`, renaming `foo-2` to "Foo" keeps `foo-2`, and `art_seed` stays `None` when nothing moved.
  - `rename_onto_a_taken_slug_gets_a_suffix`: "A" and "B" exist; renaming B to "a" gives `a-2`.
  - `rename_keeps_a_second_art_seed`: rename twice; `art_seed` is still the first id.
  - `rename_is_refused_while_locked`: hold `InstanceLock::try_acquire(perland_dir)`, then rename to a new slug → `InstanceLocked`, and the folder is unmoved.
  - `load_trusts_the_folder_name`: write `instance.json` with `"id": "old"` into folder `new`; `load("new").id == "new"`.
  - `rename_io_error_maps_in_use`: `io::Error::from(ErrorKind::PermissionDenied)` and `io::Error::from_raw_os_error(32)` → `InstanceFolderInUse`; `ErrorKind::Other` → `DiskWriteFailed`.
  - Update `update_rename_and_delete` (it asserts that the id is kept: change it to the family case).
- [ ] **Step 2:** `cargo test -p launcher-core instances`. Expected: FAIL.
- [ ] **Step 3:** Implement as described, and add `errors.instance_folder_in_use` with `message` and `fix` (the same shape as `instance_locked`):
  - en: "Another program is using this instance's folder." / "Close any window or program using the folder, then try again."
  - es: "Otro programa está usando la carpeta de esta instancia." / "Cierra cualquier ventana o programa que use la carpeta y vuelve a intentarlo."
- [ ] **Step 4:** Tests pass. `ErrorCode` round-trip tests (error.rs) include the new code.
- [ ] **Step 5:** Commit `feat(core): renaming an instance renames its folder (#5)`.

### Task 4: Core instance order

**Files:**
- Create: `crates/launcher-core/src/instance_order.rs` (+ `pub mod instance_order;` in `lib.rs`)
- Modify: `crates/launcher-core/src/paths.rs` (`pub fn instance_order(&self) -> PathBuf { self.root.join("instance-order.json") }`)

**Interfaces:**
- Produces:
  - `pub fn apply_order(instances: &mut Vec<Instance>, order: &[String]) -> Vec<String>`: sorts in place and returns the order to store.
  - `pub struct OrderStore` with `new(path)`, `async load(&self) -> Vec<String>` (missing or damaged → empty), and `async save(&self, ids: &[String]) -> Result<()>` (atomic, `{"version":1,"ids":[…]}`).
  - `pub fn rename_in_order(order: &mut [String], old: &str, new: &str)`.

**Rules for `apply_order`:**
- Instances **not** in `order` come first, A–Z by lowercased name.
- Then those in `order`, in that order.
- Ids in `order` with no instance are dropped from the returned order.
- The returned order is the full resulting id list. An empty stored order therefore yields today's A–Z, and a new instance lands on top.

- [ ] **Step 1: Write failing unit tests:**
  - empty order → A–Z, and returns all ids;
  - order `[c, a]` with instances a, b, c → `b, c, a` (unlisted b on top);
  - a stale id is dropped;
  - `rename_in_order` replaces in place;
  - `OrderStore` round-trips, a missing file → empty, garbage → empty.
- [ ] **Step 2:** Run. Expected: FAIL. **Step 3:** Implement. **Step 4:** PASS.
- [ ] **Step 5:** Commit `feat(core): a stored instance order with new instances on top (#7)`.

### Task 5: App order, reorder, rename cascade, rail setting

**Files:**
- Modify:
  - `crates/launcher-app/src/app.rs` (an `order: tokio::sync::Mutex<OrderStore>` field; `instances_list`, `instance_reorder`, `instance_rename`)
  - `crates/launcher-core/src/settings.rs` (`pub rail_expanded: bool`, default `false`; the struct is already `#[serde(default)]`)
  - `src-tauri/src/commands.rs` + `main.rs` (`instance_reorder`)
  - `ui/src/ipc/{schema.ts,commands.ts,mock.ts}`
  - `crates/launcher-app/tests/contract.rs` (samples for `artSeed`, `art` and `railExpanded`), then regenerate the contract.

**Interfaces:**
- Consumes: Task 3's `rename`, and Task 4's `apply_order`, `OrderStore` and `rename_in_order`.
- Produces:
  - `AppCore::instances_list()`: applies the order to `listing.instances`, and saves the returned order when it differs from the stored one. Damaged instances stay last, A–Z by id.
  - `AppCore::instance_reorder(&self, ids: Vec<String>) -> Result<InstanceListing>`: validates each id (`valid_id`), saves, returns the newly ordered listing.
  - `AppCore::instance_rename`: also refused while `is_installing`. After the core rename, when the id changed:
    - `rename_in_order` + save;
    - best-effort rename of `logs/<old>-latest.log` → `logs/<new>-latest.log`;
    - drop `manual` and `plans` entries keyed by the old id.
  - UI: `commands.instanceReorder(ids: string[]) => InstanceListing`; zod gets `artSeed: z.string().nullable()`, `art: z.enum(['auto','generated','custom'])` and `railExpanded: z.boolean()`.

- [ ] **Step 1: Write failing app tests** (in `app.rs` tests, using the existing `core(...)` helpers):
  - `listing_puts_new_instances_on_top`: create A, B → listing `A, B`; create C → `C, A, B`.
  - `reorder_is_kept_and_survives_a_settings_save`: reorder `[B, C, A]`; `settings_set(lang = es)`; the listing is still `B, C, A`.
  - `rename_keeps_the_place_in_the_order`: order `[B, A]`; rename A to "Zed" → listing `B, zed`.
  - `rename_moves_the_latest_log`.
  - `rename_refused_while_installing`.
  - `reorder_rejects_bad_ids` (`../x` → `instance_id_invalid`).
- [ ] **Step 2:** `cargo test -p launcher-app`. Expected: FAIL. **Step 3:** Implement, wire the Tauri command, update zod, commands and mock (the mock keeps an `order` array and applies the same rule). **Step 4:** PASS. Run `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, then `pnpm --filter ui test`.
- [ ] **Step 5:** Commit `feat(app): instance order, reorder command and rename cascade (#5, #7)`.

### Task 6: App instance art (presets, pack, upload)

**Files:**
- Modify:
  - `crates/launcher-app/src/packs.rs`: rename `pack_art` → `instance_art`, with the same `PackArtData` DTO renamed `InstanceArtData`; add `instance_art_set`
  - `crates/launcher-app/src/dto.rs` (`ArtChoice`)
  - `crates/launcher-core/src/error.rs` (`ArtFileInvalid` → `"art_file_invalid"`)
  - i18n
  - `src-tauri` commands + `main.rs`
  - `ui/src/ipc/*`, `ui/src/state/queries.ts` (`usePackArt(id, enabled)` becomes `useInstanceArt(id)`, enabled for every instance, query key `qk.instanceArt(id)`; update its callers in `Rail.tsx` and `Home.tsx`)
  - contract.

**Interfaces:**
- Produces:
  - `#[serde(tag = "kind", rename_all = "lowercase")] enum ArtChoice { Generated { seed: String }, Pack, Upload { path: PathBuf } }`
  - `AppCore::instance_art(&self, id) -> Result<InstanceArtData>`:
    - `art == Custom` → the custom image (`.perland/custom/image.{png,jpg,webp}`) as both `icon` and `banner`;
    - `Generated` → both `None`;
    - `Auto` → today's pack art.
  - `AppCore::instance_art_set(&self, id, choice: ArtChoice) -> Result<Instance>`:
    - `Generated` → `art = Generated`, `art_seed = Some(seed)`, remove `.perland/custom`. A seed that is empty or over 64 chars is refused with `ArtFileInvalid`, and nothing changes;
    - `Pack` → refused with `no_pack` unless the source is a pack; `art = Auto`; remove custom;
    - `Upload` → read metadata (must be a file, ≤ `MAX_ART_BYTES`), read bytes, `sniff` ∈ {png, jpg, webp}, else `ArtFileInvalid` with fix "Pick a PNG, JPEG or WebP image under 5 MB" and nothing changes; then write `image.<ext>` atomically into a fresh `.perland/custom/` and set `art = Custom`.
  - en/es `errors.art_file_invalid.message`:
    - en: "That file can't be used as an instance image." / fix "Pick a PNG, JPEG or WebP image under 5 MB."
    - es: "Ese archivo no se puede usar como imagen de la instancia." / "Elige una imagen PNG, JPEG o WebP de menos de 5 MB."

- [ ] **Step 1: Write failing tests:**
  - `custom_image_wins_over_pack_art_and_survives_a_sync` (write pack art, upload a tiny PNG, rewrite `.perland/art/icon.png` as a sync would; `instance_art` still returns the custom one);
  - `generated_choice_hides_pack_art_and_sets_the_seed`;
  - `pack_choice_needs_a_pack`;
  - `uploads_are_checked` (exe bytes named `.png`, an SVG, a GIF, a sparse file of `MAX_ART_BYTES + 1` made with `set_len`, a missing path → all `art_file_invalid`, `art` unchanged);
  - `upload_replaces_a_previous_upload_of_another_type` (png then webp → only `image.webp` remains).
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS, then regenerate the contract.
- [ ] **Step 5:** Commit `feat(app): instance image presets and uploads (#8)`.

### Task 7: UI follows a rename to its new id

**Files:**
- Modify: `ui/src/state/queries.ts` (`useRenameInstance`), `ui/src/features/home/InstanceMenu.tsx`, `ui/src/features/home/Home.test.tsx`.

**Behaviour:** on success, when `result.id !== id`:
- `navigate({ ...currentRoute, id: result.id })` for routes carrying that id;
- `rememberInstance(result.id)`;
- invalidate the instance listing, `instanceArt(old)` and the server-status queries.

- [ ] **Step 1:** Failing test, with the mock moving ids like core: rename "Creative Flat" → "Sky Base"; the route becomes `{ name: 'home', id: 'sky-base' }`, the title shows "Sky Base", and `localStorage` holds the last instance `sky-base`.
- [ ] **Steps 2–4:** FAIL → implement → PASS. **Step 5:** Commit `feat(ui): follow a renamed instance to its new folder (#5)`.

### Task 8: Expandable rail (`/impeccable`)

**Files:**
- Modify: `ui/src/features/rail/Rail.tsx`, `ui/src/features/rail/Rail.test.tsx`, `ui/src/App.tsx` (grid column `4.5rem` ↔ `15rem` from `settings.railExpanded`), `ui/src/styles/*` (rail rules), i18n (`rail.expand`, `rail.collapse`), `DESIGN.md` (rail section) and the surface contract.

**Behaviour:**
- A rail key (`aria-pressed`, label "Show names" / "Hide names"; es "Mostrar nombres" / "Ocultar nombres") toggles `railExpanded` through `useSettingsMutation`.
- Wide rows show the icon, the visible name (one line, ellipsis, full name in `title`) and the status word (Playing / Preparing / server word).
- The accessible name stays the same as today, so existing tests hold.
- Placement, row styling and the 960px fit are decided in `/impeccable` (the reporter suggested the top). Arcade Cabinet rules apply: notched corners, lime LED, amber coin-slot light.

- [ ] **Step 1:** Failing tests:
  - the toggle flips `railExpanded` (mock settings) and names appear as visible text (`getByText('Asalith Fields')`);
  - compact mode shows no visible names;
  - arrow-key navigation still works when wide.
- [ ] **Steps 2–4:** FAIL → implement via `/impeccable` → PASS. Screenshot both modes at 960 × 600 in en and es through `pnpm --filter ui dev:mock`.
- [ ] **Step 5:** Commit `feat(ui): a wide rail with instance names (#9, #10)`.

### Task 9: Reorder by drag and by keyboard

**Files:**
- Create: `ui/src/features/rail/reorder.ts` + `reorder.test.ts`
- Modify: `Rail.tsx`, `Rail.test.tsx`, `ui/src/state/queries.ts` (`useReorderInstances`, optimistic `setQueryData`), i18n (`rail.moved`: "{{name}} moved to position {{position}} of {{count}}" / "{{name}} pasó al puesto {{position}} de {{count}}").

**Interfaces:**
- Produces:
  - `moveItem(ids: string[], from: number, to: number): string[]`
  - `dropIndex(midpoints: number[], y: number): number`

**Behaviour:**
- Pointer drag uses pointer events, never HTML5 DnD (Tauri `dragDropEnabled: true` swallows it on Windows):
  - it starts after 4px of movement;
  - a click under that threshold still selects;
  - a drop line shows the target slot.
- Alt+↑/↓ on a focused cabinet moves it and keeps focus on it.
- The result is announced in an `aria-live="polite"` region.
- Damaged cabinets can't be dragged and aren't drop targets. Only `listing.instances` ids are sent.

- [ ] **Step 1:** Failing tests:
  - `moveItem` edge cases (first↔last, same index);
  - `dropIndex` (above the first midpoint → 0, below the last → n);
  - Rail: Alt+↓ on "Asalith Fields" calls `instanceReorder` with `['creative-flat','asalith-fields', …]`, keeps focus on it and announces "Asalith Fields moved to position 2 of 2";
  - Alt+↑ on the first item does nothing.
- [ ] **Steps 2–4:** FAIL → implement (styling through `/impeccable`) → PASS. Manual drag check in `dev:mock`.
- [ ] **Step 5:** Commit `feat(ui): reorder instances by drag or Alt+arrows (#7)`.

### Task 10: "Change image…" picker (`/impeccable`)

**Files:**
- Create: `ui/src/features/home/ImageDialog.tsx` + `ImageDialog.test.tsx`
- Modify:
  - `InstanceMenu.tsx` (a menu item "Change image…" / "Cambiar imagen…")
  - `Home.tsx` and `Rail.tsx`: seed = `artSeed(instance)`, art from `useInstanceArt`
  - `ui/src/art/seed.ts` (+ test)
  - i18n
  - `DESIGN.md`.

**Interfaces:**
- Produces:
  - `artSeed(i: { id: string; artSeed: string | null }): string` = `i.artSeed ?? i.id`
  - `presetSeeds(i): string[]`: `base = artSeed(i).replace(/~\d+$/, '')`, giving `[base, base~2 … base~6]`.

**Behaviour:**
- The dialog shows 6 small `Marquee` tiles from `presetSeeds`, a "Pack art" tile (pack instances only) and "Upload your own…". Upload uses `@tauri-apps/plugin-dialog` `open({ filters: [{ name, extensions: ['png','jpg','jpeg','webp'] }] })`.
- The current choice wears the lime selection lamp.
- Choosing a tile calls `instanceArtSet`. Errors show in an inline `ErrorCard`.
- On success, invalidate `instanceArt(id)` and the listing.

- [ ] **Step 1:** Failing tests:
  - `presetSeeds` (strips `~N`, six unique seeds);
  - the dialog: picking the third landscape sends `{ kind: 'generated', seed: 'asalith-fields~3' }`;
  - "Pack art" appears only for packs;
  - an `art_file_invalid` mock error renders its translated message.
- [ ] **Steps 2–4:** FAIL → implement via `/impeccable` → PASS.
- [ ] **Step 5:** Commit `feat(ui): change an instance's image (#8)`.

### Task 11: Finish PR 2

- [ ] Run the full gate. Rebase on `origin/main` (PR 1 and Plan 6 may have landed). Regenerate the contract if `main` touched DTOs.
- [ ] Open PR 2 with `Fixes #5`, `Fixes #7`, `Fixes #8`, `Fixes #9`, `Fixes #10` and an owner smoke checklist:
  - rename with Explorer open on the folder → error, nothing changed; close it → the folder is renamed, the art and the place in the list are unchanged;
  - a pack instance renamed mid-list then played;
  - new instance on top; drag and Alt+arrows;
  - wide rail at 960px in es;
  - upload a PNG and a fake `.png`; pick a landscape on a pack instance; "Pack art" brings it back.
- [ ] Reporter updates per `reporter-updates` when work starts (drafts shown to the owner) and for #5's `-2` explanation.
