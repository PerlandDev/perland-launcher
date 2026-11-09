# Background Zoom Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a player zoom and move a picture background from an "Adjust the background" view in Change image, with dark or pixel edges when the picture no longer covers Home's header.

**Architecture:** The core stores `banner_zoom` and `banner_edges` next to `banner_focus` and saves all three through one command, `instance_banner_view_set`. The UI computes the picture's box in the marquee with one pure function, used by Home and by the preview, which renders Home's own header inert and scaled down inside the dialog. Position moves out of Change image into the new view.

**Tech Stack:** Rust (launcher-core, launcher-app, serde), Tauri wiring, React 19 + TanStack Query + zod, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-11-08-background-zoom-design.md`

## Global Constraints

- Zoom is a percent of the size that just covers the header: 100 is today's look; allowed 25 to 300, steps of 5.
- Position keeps `object-position`'s meaning on both sides of 100; the default is x 50, y 100 (bottom middle).
- Edges: `dark` (default) or `pixel`; pixel = the picture shrunk to 24 cells across, nearest-neighbour, about 40% brightness. No blur anywhere.
- Old `instance.json` files load unchanged; an unknown `bannerEdges` loads as `dark`.
- A new background (any `instance_art_set`) resets focus, zoom and edges to `None`.
- Saves: on drag end, 300 ms after the last key press or wheel step, on slider release, at once for Reset and the edge segments.
- Every UI string in `en.json` and `es.json` (neutral Latin American Spanish, "tú"). Copy from the spec: "Adjust…" / "Ajustar…", "Adjust the background" / "Ajustar el fondo", "Drag the picture to move it and use the zoom to bring it closer or further away." / "Arrastra la imagen para moverla y usa el zoom para acercarla o alejarla.", "Zoom", "Edges" / "Bordes", "Dark" / "Oscuros", "Pixel" / "Pixelados", "Reset" / "Restablecer", "Done" / "Listo", "Back" / "Volver".
- The contract JSON is generated: `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`. Never hand-edit it.
- `src-tauri` is wiring only, no tests. Never put logic there.
- Before each commit: `pnpm --filter ui build`, `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `pnpm -r lint`, `pnpm -r typecheck`, `pnpm -r test`.

## Review Focus

1. A picture whose box exactly matches the frame on one axis (zoom 100): dragging along that axis must not produce NaN or Infinity; the focus on that axis stays as it was. Test in Task 3.
2. The pixel-edges image can't be made (no canvas, a picture that fails to decode): the marquee falls back to dark edges, never a broken image. Test in Task 4.
3. Pixel art blown up 2x or more must still get hard pixels when zoomed: `data-upscale` is decided from the displayed box, not the frame. Test in Task 4.
4. Picking a new background while a view save is still waiting (300 ms timer): the waiting save is dropped, the view starts from the defaults. Test in Task 6.
5. Closing the dialog (Done, Escape, Back) while a save is waiting flushes it, as Position does today. Test in Task 6.

---

### Task 1: Core: zoom and edges on the instance

**Files:**
- Modify: `crates/launcher-core/src/instances.rs` (next to `Focus`, `Instance`, its `new` and the tests near `old_instance_json_has_no_banner_focus`)

**Interfaces:**
- Produces: `pub enum BannerEdges { Dark, Pixel }` (serde lowercase, `#[serde(other)]` on `Dark`, `Default` = `Dark`); `Instance.banner_zoom: Option<u16>` and `Instance.banner_edges: Option<BannerEdges>`, both `#[serde(default)]`, serialized as `bannerZoom` / `bannerEdges` (null when `None`, like `bannerFocus`); `impl Focus { pub const MAX }` unchanged; `pub const MIN_BANNER_ZOOM: u16 = 25; pub const MAX_BANNER_ZOOM: u16 = 300;`.

- [ ] **Step 1: Write the failing tests** in `instances.rs`'s test module:
  - `old_instance_json_has_no_banner_zoom_or_edges`: the minimal JSON (`{"id":"a","name":"A","source":{"kind":"none"},"game":{"minecraft":"1.20.1"}}`) loads with `banner_zoom == None`, `banner_edges == None`; serializing gives `bannerZoom` and `bannerEdges` as JSON null; setting `Some(150)` / `Some(BannerEdges::Pixel)` round-trips as `150` / `"pixel"`.
  - `unknown_banner_edges_load_as_dark`: `"bannerEdges":"glow"` loads as `Some(BannerEdges::Dark)` and the instance is not damaged.
- [ ] **Step 2:** `cargo test -p launcher-core banner_` → FAIL (fields missing).
- [ ] **Step 3: Implement** the enum, the two fields (doc comments: what `None` means), the constants, and `None` in every `Instance` literal (`instances.rs` `new`, tests, `crates/launcher-app/tests/contract.rs`).
- [ ] **Step 4:** `cargo test -p launcher-core` → PASS.
- [ ] **Step 5: Commit** `feat(core): keep a background's zoom and edges on the instance`.

### Task 2: App command, contract, schema and mock

**Files:**
- Modify: `crates/launcher-core/src/error.rs` (new `ErrorCode::BannerZoomInvalid`, wire `banner_zoom_invalid`, in `ALL` and in the codes test)
- Modify: `crates/launcher-app/src/dto.rs` (`BannerView`), `crates/launcher-app/src/packs.rs` (replace `instance_banner_focus_set`; reset in `instance_art_set` at the "A new picture starts from the default position" line; migrate the tests at ~1873–1961)
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/main.rs` (rename the command)
- Modify: `crates/launcher-app/tests/contract.rs` (a `banner_views` fixture; `bannerZoom: Some(150)`, `bannerEdges: Some(Pixel)` on the instance fixture that has `Focus { x: 30, y: 60 }`), then regenerate `ui/src/ipc/contract/*.json`
- Modify: `ui/src/ipc/schema.ts` (Instance: `bannerZoom: z.number().int().nullable()`, `bannerEdges: z.enum(['dark','pixel']).nullable()`; `BannerView` schema and type; `ui/src/ipc/contract.test.ts` parses `banner_views.json`)
- Modify: `ui/src/ipc/commands.ts` (`instanceBannerViewSet`), `ui/src/ipc/mock.ts` + `mock.test.ts` (same rules as the core; `instance_art_set` resets the three), `ui/src/ipc/mockFree.ts` and `ui/src/features/browse/compat.test.ts` (instance literals gain the two fields)
- Modify: `ui/src/i18n/en.json`, `es.json`: `errors.banner_zoom_invalid.message` = "That zoom is out of range." / "Ese zoom está fuera de rango."

**Interfaces:**
- Consumes: Task 1's fields, `MIN_BANNER_ZOOM`, `MAX_BANNER_ZOOM`, `BannerEdges`.
- Produces: Rust `pub struct BannerView { pub focus: Option<Focus>, pub zoom: Option<u16>, pub edges: Option<BannerEdges> }` (camelCase serde); `AppCore::instance_banner_view_set(&self, id: &str, view: BannerView) -> Result<Instance>`; IPC `instance_banner_view_set { id, view }`; TS `type BannerView = { focus: Focus | null; zoom: number | null; edges: 'dark' | 'pixel' | null }`; `commands.instanceBannerViewSet(id: string, view: BannerView): Promise<Instance>`.

- [ ] **Step 1: Write the failing Rust tests** (rename the three focus tests in `packs.rs`):
  - `banner_view_is_saved_and_reset_by_a_new_background`: set `{ focus: (20,80), zoom: 150, edges: Pixel }` → returned and reloaded instance carry all three; set all `None` → all `None`; set again, then `instance_art_set(Generated…)` → all three `None` on disk.
  - `banner_view_out_of_range_is_refused`: focus `(101,0)` → `BannerFocusInvalid`; zoom `24` and `301` → `BannerZoomInvalid`; nothing saved; zoom `25`, `300` and focus `(100,0)` accepted.
  - `banner_view_refused_while_busy`: as the old busy test.
  - In `error.rs`'s codes test: `(ErrorCode::BannerZoomInvalid, "banner_zoom_invalid")`.
- [ ] **Step 2:** `cargo test -p launcher-app banner_view` → FAIL.
- [ ] **Step 3: Implement** `instance_banner_view_set` (same busy/installing refusals as today, focus check then zoom check, then load, set the three fields, `save`), the reset in `instance_art_set`, the error code, the DTO, the Tauri rename.
- [ ] **Step 4:** `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract`, then `cargo test --workspace` → PASS.
- [ ] **Step 5: Write the failing TS tests**: `mock.test.ts` — `instance_banner_view_set` saves the three, refuses zoom 24/301 with `banner_zoom_invalid`, and `instance_art_set` clears them; `contract.test.ts` parses `banner_views.json` and the instance fixtures.
- [ ] **Step 6:** `pnpm --filter ui test -- mock contract` → FAIL; implement schema, command, mock; → PASS. `i18n.test.ts` passes with the new error message.
- [ ] **Step 7: Commit** `feat: save a background's position, zoom and edges together`.

### Task 3: The picture's box in the marquee

**Files:**
- Create: `ui/src/art/bannerLayout.ts`, `ui/src/art/bannerLayout.test.ts`

**Interfaces:**
- Produces:
  - `type Size = { width: number; height: number }`, `type Box = { left: number; top: number; width: number; height: number }`
  - `const DEFAULT_FOCUS: Focus = { x: 50, y: 100 }`, `const MIN_ZOOM = 25`, `const MAX_ZOOM = 300`, `const ZOOM_STEP = 5`
  - `bannerBox(frame: Size, picture: Size, zoom: number | null, focus: Focus | null): Box` — scale = cover scale × zoom/100; `left = (frame.width − width) × x/100`, `top = (frame.height − height) × y/100`.
  - `clampZoom(zoom: number): number` — rounded to the step, clamped to 25–300.
  - `dragFocus(start: Focus, delta: Size /* frame px */, frame: Size, box: Box): Focus` — moves the picture with the pointer; an axis with no free space (`frame − box` is 0) keeps its value; result rounded and clamped 0–100.
  - `covers(zoom: number | null): boolean` — `(zoom ?? 100) >= 100`.

- [ ] **Step 1: Write the failing tests**:
  - `bannerBox` at zoom null/100 with a 1000×500 frame and a 500×500 picture → `{ width: 1000, height: 1000, left: 0, top: -500 }` for the default focus (bottom middle), `top: -250` for y 50.
  - Same picture at zoom 50 → `{ width: 500, height: 500, left: 250, top: 0 }` at the default focus; at focus (0,0) → left 0, top 0.
  - Zoom 200 → width 2000.
  - `clampZoom(12) === 25`, `clampZoom(301) === 300`, `clampZoom(152) === 150`.
  - `dragFocus` at zoom 50 (box 500 wide in a 1000 frame): `delta.x = +100` from x 50 → 70; at zoom 100 with a 1000×1000 box in a 1000×500 frame, `delta.x = 300` keeps x, `delta.y = +100` from y 100 → 80 (dragging down shows more of the top); never NaN.
  - `covers(null)`, `covers(100)` true; `covers(95)` false.
- [ ] **Step 2:** `pnpm --filter ui test -- bannerLayout` → FAIL.
- [ ] **Step 3: Implement** the module.
- [ ] **Step 4:** → PASS.
- [ ] **Step 5: Commit** `feat(ui): work out a zoomed background's box`.

### Task 4: Home draws the zoom and the edges

**Files:**
- Create: `ui/src/art/pixelEdges.ts` (+ test)
- Modify: `ui/src/art/Marquee.tsx`, `ui/src/art/Marquee.test.tsx`, `ui/src/styles/app.css` (`.marquee-edges`), `ui/src/features/home/Home.tsx` (pass `zoom={instance.bannerZoom}` and `edges={instance.bannerEdges}`)

**Interfaces:**
- Consumes: `bannerBox`, `covers` (Task 3); `Instance.bannerZoom`, `Instance.bannerEdges` (Task 2).
- Produces: `Marquee` props `zoom?: number | null`, `edges?: 'dark' | 'pixel' | null`; `pixelEdges(src: string, cells?: number /* 24 */): Promise<string | null>` (null when no 2D canvas or the picture fails); `usePixelEdges(src: string | null): string | null` (cached per src).

- [ ] **Step 1: Write the failing tests** in `Marquee.test.tsx` (mock the image's natural size and `ResizeObserver` as the file already does for `data-upscale`):
  - zoom null → the banner keeps today's path: no inline box, `objectPosition` only when a focus is set.
  - zoom 50 with a measured 1000×500 frame and a 500×500 picture → the banner's inline style is `left: 250px; top: 0px; width: 500px; height: 500px` and it has `data-zoomed`.
  - zoom 50 + edges `pixel` with `pixelEdges` mocked to a data URL → an `img.marquee-edges` with that src sits behind the banner; mocked to null → no `.marquee-edges`.
  - zoom 300 on a 16×16 picture → `data-upscale` is set (decided from the displayed box).
  - `pixelEdges` test: with `getContext` returning null → resolves null.
- [ ] **Step 2:** `pnpm --filter ui test -- Marquee pixelEdges` → FAIL.
- [ ] **Step 3: Implement**: when `covers(zoom)` and zoom is null or 100, keep today's CSS path; otherwise position the banner absolutely from `bannerBox(box, natural, zoom, focus)` (`object-fit: fill`). `.marquee-edges`: absolute, inset 0, cover, `image-rendering: pixelated`, `filter: brightness(0.4)`. Home passes the two props.
- [ ] **Step 4:** → PASS.
- [ ] **Step 5: Commit** `feat(ui): Home draws a background's zoom and edges`.

### Task 5: Saving the view from the UI

**Files:**
- Modify: `ui/src/state/queries.ts` (`usePreviewBannerFocus` → `usePreviewBannerView`, `useSetBannerFocus` → `useSetBannerView`), and their callers

**Interfaces:**
- Consumes: `commands.instanceBannerViewSet`, `BannerView` (Task 2).
- Produces: `usePreviewBannerView(): (id: string, view: BannerView) => void` (writes `bannerFocus`, `bannerZoom`, `bannerEdges` into the cached listing); `useSetBannerView(opts)`: mutation `{ id, view }`, same `PICTURE_SAVES` scope and error behaviour as today.

- [ ] **Step 1: Write the failing test** in `ui/src/state/instanceQueries.test.tsx`: previewing `{ focus: {x:10,y:20}, zoom: 60, edges: 'pixel' }` changes the cached instance's three fields; a refused save re-reads the listing.
- [ ] **Step 2:** → FAIL. **Step 3:** implement. **Step 4:** → PASS (the old focus hooks are gone; `ImageDialog` keeps compiling by passing `{ focus, zoom: instance.bannerZoom, edges: instance.bannerEdges }` until Task 6).
- [ ] **Step 5: Commit** `feat(ui): preview and save a background's whole view`.

### Task 6: Change image: the Adjust key and the adjust view

Use `/impeccable` for the styling in this task; follow `DESIGN.md` (keys, segments, slider, wells).

**Files:**
- Create: `ui/src/features/home/BannerAdjust.tsx` (replaces `FocusPicker.tsx`, which is deleted), `ui/src/features/home/BannerAdjust.test.tsx` (the Position tests move here from `ImageDialog.test.tsx` lines ~170–310, rewritten for the view)
- Modify: `ui/src/ui/Dialog.tsx` (+ test: optional `back?: { label: string; onBack: () => void }` renders a ghost icon key with `ChevronDown` turned 90° before the title, as `ScreenFrame` does)
- Modify: `ui/src/features/home/ImageDialog.tsx`, `ImageDialog.test.tsx`, `ui/src/features/home/Home.tsx` (`data-home-hero` on `InstanceView`'s section; export `InstanceView` for the preview), `ui/src/styles/app.css` (drop `.focus-*`, add the preview well and rows), `ui/src/i18n/en.json`, `es.json` (`image.adjust`, `image.adjustTitle`, `image.adjustHint`, `image.zoom`, `image.zoomIn`, `image.zoomOut`, `image.edges`, `image.edgesDark`, `image.edgesPixel`; keep `image.positionX/Y`; drop `image.position`, `image.positionHint`)

**Interfaces:**
- Consumes: Tasks 3–5; `InstanceView` from `Home.tsx`.
- Produces: `BannerAdjust({ instance, onDone, ref }: { instance: Instance; onDone: () => void; ref?: Ref<BannerAdjustHandle> })` with `BannerAdjustHandle = { flush(): void; cancel(): void }` (same meaning as `FocusPickerHandle`).

- [ ] **Step 1: Write the failing tests** (`ImageDialog.test.tsx`, `BannerAdjust.test.tsx`, `Dialog.test.tsx`):
  - The Background heading row has an "Adjust…" key only for an upload, a screenshot or the pack's art; none for a landscape; no "Position" group in Change image any more.
  - Pressing it shows "Adjust the background" with a "Back" key; Back returns to the choices; Done closes the dialog.
  - The preview is `aria-hidden` and `inert` and holds a copy of Home's header (`[data-home-hero]`).
  - Keyboard: "Left to right" + ArrowRight ×2 → Home's marquee follows at once, `instance_banner_view_set` is sent once 300 ms later with `{ focus: { x: 60, y: 100 }, zoom: null, edges: null }`.
  - Zoom slider to 60 → saved on release with `zoom: 60`; "−" from 100 → 95; wheel up over the preview → +5, saved 300 ms after the last step.
  - The Edges row is hidden at zoom ≥ 100 (keeps its space: `visibility: hidden`) and shown below; picking Pixel saves at once with `edges: 'pixel'`.
  - Reset sends `{ focus: null, zoom: null, edges: null }` and is `aria-disabled` while already at the defaults.
  - Review Focus 4: a pending save is dropped when a new background is picked. Review Focus 5: closing with Escape or Done flushes a pending save.
  - `Dialog` with `back` renders the key before the title and calls `onBack`.
- [ ] **Step 2:** `pnpm --filter ui test -- ImageDialog BannerAdjust Dialog` → FAIL.
- [ ] **Step 3: Implement**: `ImageDialog` keeps a `view: 'choose' | 'adjust'` state and passes the `back` prop and footer actions per view (Reset with `mr-auto`, then Done). `BannerAdjust` measures `[data-home-hero]` once on open (fallback 1000×562), renders `<InstanceView>` at that size scaled to the well's width inside an `inert`, `aria-hidden` box with Home's overlays at 40% opacity, and a transparent drag layer on top (`dragFocus`, grab cursor, wheel ±`ZOOM_STEP`); the scheduling (`'drag' | 'later' | 'now'`, 300 ms, flush/cancel) carries over from `FocusPicker`.
- [ ] **Step 4:** → PASS; `pnpm --filter ui dev:mock` at 960×600: the view fits without scrolling.
- [ ] **Step 5: Commit** `feat(ui): adjust a background's position and zoom from a preview`.

### Task 7: Docs, screenshots and the full check

**Files:**
- Modify: `DESIGN.md` ("Change image": the Adjust key and view replace Position; "Marquee": zoom, dark and pixel edges)

- [ ] **Step 1:** Update `DESIGN.md`; run the Impeccable detector on the changed UI files (`impeccable detect --json …`) and fix what it reports.
- [ ] **Step 2:** All the Global Constraints checks → PASS.
- [ ] **Step 3:** Before/after screenshots in `pnpm --filter ui dev:mock` (Change image with a picture background; the adjust view at 100%, 150%, 60% dark, 60% pixel; Home after Done) for the owner.
- [ ] **Step 4: Commit** `docs(design): adjusting a background`.
