# Skin editor ("Skins" screen): design

Date: 2026-11-07. Status: approved direction "Fitting room", owner answers below.

## Intent

The owner asked for "a 3D skin editor", then "the coolest and most complete but in brand skin
editor 3D for players". Players (often 12–25, casual, many Spanish-speaking) want to make their
character look like them and wear it in the game without leaving the launcher or visiting a
website. Success: a player opens Skins, paints on their figure, presses **Wear this skin**, and
sees it in game. Failure even if it works: a tool that feels like a generic paint program bolted
on, a screen that loses a player's work, or one that pretends offline accounts can upload.

## Owner answers (2026-11-07)

- **Entry:** its own screen, opened from the title-bar account (avatar and name) and from an
  "Edit skin" key on each account in Accounts.
- **Library:** a local wardrobe of skins in the launcher folder: new, import, duplicate, rename,
  delete, export, with autosave so nothing is lost.
- **Upload:** one amber **Wear this skin** key uploads with the chosen arm model; offline accounts
  get a plain line saying why it isn't available.
- **Layout:** "Fitting room": wardrobe column at left, the 3D figure large on a stage in the
  middle, the flat texture and colours at right, tools in a 9-slot hotbar along the bottom
  (keys 1–9). Chosen over "Painter's bench" (flat texture first) and "Library first" (grid then
  editor).

## Owner feedback during the build (2026-11-07)

- "Nice animations that look cool", and "people could see their character walking, sitting, crouching, going in a
  boat… or on a horse": the stage got a **Pose** menu (standing, walking, running, crouching, sitting, waving, in a
  boat, on a horse), a figure that is always a little alive, and a hop-and-turn when a skin is worn. The boat and
  horse are original block props drawn in code. Reduced motion holds each pose still.
- "More detail and noise in the models": the props use 16-texel pixel surfaces (grain, planks, water, strands)
  and the Perlander starter has texel grain and more features.
- A player with a paid account (via Discord): "the launcher should detect your skin, not give you a random one".
  A Microsoft account's empty wardrobe starts only from the skin it really wears; if that can't be read the error
  shows and nothing made-up is added, and if it wears the default skin the wardrobe says so. Only offline players
  start from the Perlander. (The generated title-bar avatar is replaced by the real face on the avatar branch.)
- The title-bar entry is its own shirt key after the account, so the account button still opens Accounts.

## Goals

- 3D figure with classic (4px) and slim (3px) arms, the outer layer shown or hidden, each body
  part shown or hidden per layer so inner surfaces are reachable.
- Orbit (drag empty space, or right-drag anywhere), zoom (wheel, keys), reset view.
- Paint on the 3D figure and on the flat texture, live in both.
- Tools: pencil, eraser, fill, eyedropper, shade (lighten/darken), noise (subtle random shade);
  mirror painting (left ↔ right) as a toggle; undo/redo (Ctrl+Z, Ctrl+Shift+Z / Ctrl+Y).
- Colours: current colour, hex field, the system colour picker, recent colours, a preset row
  of skin tones and common hair/cloth colours.
- Open the account's current skin (Microsoft), import a PNG (64×64 and legacy 64×32), export a
  PNG, start from blank or an original Perland starter figure.
- Microsoft accounts upload (`POST /minecraft/profile/skins`, multipart `variant` + `file`).
- Offline accounts edit and export but never upload, said plainly.

## Non-goals

- Capes, HD (128×128) skins, Bedrock skins, posing the figure by hand.
- A shared/online skin gallery or skin search (the launcher has no such service).
- Layers beyond Minecraft's own two (base and outer).

## Anti-goals

- No Mojang assets: no Steve/Alex textures. The starter figure is original and drawn in code.
- No new visual world: the launcher's Arcade Cabinet system (DESIGN.md) applies unchanged.
- Never log the Minecraft token; never send a skin anywhere but Minecraft services.

## Constraints

- three.js (MIT, GPL-compatible), pinned exact version, loaded only by the lazy Skins screen.
- CSP `img-src 'self' data: blob:`: textures reach the UI as `data:` URLs from the backend.
- `crates/*` stay free of Tauri. HTTP behaviour gets wiremock tests in launcher-core.
- Every string in `en.json` and `es.json`; new `ErrorCode`s get `errors.<code>.message` in both.
- 960 × 600 minimum window: all four regions fit without scrolling the screen.

## Approaches considered

1. **Fitting room (chosen):** the figure is the hero, matching the launcher's game-forward
   Home; the hotbar gives the tools a native-to-Minecraft home and 1–9 keys.
2. **Painter's bench:** best for pixel-precise work but buries the 3D figure the owner asked for.
3. **Library first:** calm and familiar, but two screens deep before any painting.

Technical: paint by raycasting the three.js meshes and mapping the hit UV to a texel (chosen),
versus a 2D-only editor with a 3D preview (rejected: the owner asked to paint on the model).

## How it works

### Library on disk (launcher-core `skins_library`)

`<root>/skins/library.json` lists entries `{ id, name, model: classic|slim, updated, worn_by? }`;
each skin's texture is `<root>/skins/<id>.png`, always a normalised 64×64 RGBA PNG. Ids are
generated slugs. Writes are atomic (`fsutil`). A damaged `library.json` is kept aside and the
library starts empty rather than failing the screen; a missing PNG drops that entry on load.

PNG handling (`skins_png`, pure, unit-tested): read at most 1 MiB, decode with size limits,
accept 64×64 or 64×32 only (else `SkinInvalid`), convert 64×32 to 64×64 the way the game does
(copy the right arm and leg to the left slots, each face mirrored), encode RGBA PNG.

### Backend commands (launcher-app → src-tauri wiring)

`skins_list`, `skin_texture(id) → data URL`, `skin_create(name, model, start: blank|texture)`
(the UI passes the starter or current-skin PNG as a data URL), `skin_import(path)`,
`skin_save(id, png data URL)`, `skin_update(id, name?, model?)`, `skin_duplicate(id)`,
`skin_delete(id)`, `skin_export(id, path)`, `skin_current(uuid) → { texture, model } | null`,
`skin_wear(id, uuid)`.

### Current skin and upload (launcher-core `msa`)

- `MsaClient::profile_skin(mc)` reads `GET /minecraft/profile`'s ACTIVE skin (url + variant),
  then fetches the texture from the URL on `textures.minecraft.net` only (capped, one host).
  **Seam:** the parallel branch `claude/cool-edison-kg93c4` adds `skins.rs` with a session-server
  texture fetch and `cache/skins/<uuid>.png`. It wasn't on GitHub when this was written, so the
  launcher-app `current_skin` function is the one place to switch to that cache once it lands.
- `MsaClient::upload_skin(mc, png, variant)`: multipart POST to `minecraft/profile/skins`.
  200 → ok; 400 → `SkinRejected`; 401 → `MicrosoftSignInAgain`; 429 → `MicrosoftRateLimited`;
  5xx → host trouble. The token goes only in the `Authorization` header and never in logs.
- `skin_wear` gets a fresh session through the same refresh path launches use, uploads, and
  records `worn_by` on the entry. An offline account returns `SkinOfflineAccount`.

New `ErrorCode`s: `SkinInvalid`, `SkinNotFound`, `SkinRejected`, `SkinOfflineAccount`.

### UI (`ui/src/features/skins/`, lazy route `{ name: 'skins' }`)

- **Pure logic (Vitest):** `layout.ts` (every part's box size, position and per-face UV rect for
  both models and both layers), `mirror.ts` (texel ↔ mirrored texel map), `paint.ts` (pencil,
  eraser, flood fill within one face, eyedropper, shade, noise on an RGBA buffer), `history.ts`
  (undo/redo of stroke diffs, capped), `starter.ts` (the original starter figure).
- **Stage:** a three.js scene built from `layout.ts`, nearest-filtered `DataTexture`, the outer
  layer as inflated boxes with alpha test, hover texel highlight, orbit/zoom by pointer, wheel
  and keys. The scene lives behind a small adapter so tests mock it (jsdom has no WebGL).
- **Texture panel:** the 64×64 canvas upscaled with crisp pixels, part outlines and a hover
  readout; painting works the same as on the figure.
- **Hotbar:** nine raised keys (Pencil, Eraser, Fill, Eyedropper, Shade, Noise, Mirror, Undo,
  Redo) with 1–9 numbers; the chosen tool presses into its slot with the lime lamp.
- **Wardrobe:** New (Blank, Starter, From my account), Import, the list with face thumbnails,
  a "Wearing" lamp, and a ⋯ menu (Rename, Duplicate, Export, Delete with confirmation).
- **Wear this skin:** the screen's one amber key under the stage. Offline: no key, an `ink-dim`
  line explains. Errors are toasts via the existing `LauncherError` flow.
- Autosave 600 ms after the last stroke; a quiet "Saved" readout.

## Left to the builder

Exact spacing within DESIGN.md's rhythm, preset colour values, the starter figure's artwork,
shade/noise strengths, the undo cap.
