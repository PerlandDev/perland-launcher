# Skins catalog and character creator: design

Date: 2026-11-07. Status: owner approved the direction and approach A; this document is for approval.
Builds on `2026-11-07-skin-editor-design.md`, `2026-11-07-per-account-wardrobes-design.md` and
`2026-11-07-skin-version-history-design.md`. It replaces the editor spec's non-goal "a shared/online skin
gallery or skin search" with a staff-curated catalog.

## Intent

The owner asked for a skins catalog inside Perland, first from a trusted third party, then "our own skins
catalog within perland and perland engine" to "maintain safety", possibly proxying other providers. Players
are about 12 to 25. The research below rules out third parties, so the owner chose a **staff-curated
catalog**. On top of it, players get a **character creator**: "a lot of pieces for people to build their
skins, like jackets, shirts, pants... every type of accessory", with "all levels of customization just like
a character editor".

Success: a player opens Skins, picks a ready-made look or builds one from pieces, recolours anything, and
wears it in game in a few minutes, without leaving the launcher or visiting a website. Everything they can
see in the catalog was made and approved by Perland.

Failure even if it works: anything players upload or that comes from outside Perland reaching other players;
pieces that look flat or cheap next to the editor; losing a player's work or breaking wear, export, version
history or per-account wardrobes; a catalog that needs a launcher release for every new jacket.

## Third-party providers: why not (research, 2026-11-07)

Full notes and 34 sources: research done in session; summary here. The session's network blocked the
providers' own pages, so terms were read through search snippets; treat these as strong indications, not
verified quotes.

| Provider | Public API | Reuse of skins | Moderation |
| --- | --- | --- | --- |
| NameMC | None official; undocumented endpoints only, Cloudflare in front | Not granted | Some |
| MinecraftSkins.com (The Skindex) | None | Uploader keeps ownership, licence to the site only | Reports, DMCA |
| Planet Minecraft | None | Creators own their work; reuse needs each creator | Moderators, reports |
| NovaSkin, SkinMC, MCSkins, Tynker | None found | No licence found | SkinMC bans sexual/offensive skins; Tynker targets kids |
| MineSkin | Yes, keyed and paid | Not a catalog: signs skins for servers | n/a |
| Crafatar, Minotar, MC-Heads | Yes, open | Not a catalog: renders a known player's Mojang skin | n/a |
| Mojang | Per-player profile and textures only | No catalog or search | n/a |

None can legitimately be proxied or aggregated by a launcher backend, and none gives Perland control over
what a 12-year-old sees. Linking out was offered and set aside for safety.

## Owner decisions (2026-11-07)

- **Staff-curated catalog first.** No player uploads in this version.
- **Made with AI.** Concretely: Claude draws every skin and piece as pixel art written in code, which
  outputs the PNGs. Image-generation models cannot reliably draw the 64×64 UV layout. All art is original
  (no Mojang assets), and staff review it before it ships.
- **Character creator.** Body, face, hair, clothes and accessories as separate pieces. A piece sits on top
  of the skin and stays removable (my call, owner deferred). Every colour is the player's choice.
- **Starting content:** about 30 complete looks and about 120 pieces (owner: "sounds good"):
  - body and face: 20 face parts and 25 hairstyles
  - tops: 20 (tees, hoodies, jackets, coats, dresses)
  - bottoms: 15 (jeans, shorts, skirts, armour legs)
  - shoes: 10
  - hats: 15 (caps, beanies, crowns, helmets)
  - accessories: 15 (glasses, masks, scarves, backpacks, belts, gloves, wings on the back)
  - looks: casual, sporty, school uniform, adventurer, knight, pirate, astronaut, chef, wizard, cyberpunk, ...
- **Layout:** the mockups (Create / Paint / Catalog tabs) "look really good".
- **Approach A:** the catalog lives in PerlandEngine, the Engine serves it, the launcher keeps a copy.

## Goals

- A **Catalog** tab: search, tag chips, a grid of looks, a larger preview with the pieces it's made of, and
  **Add to my wardrobe**, which adds a fully editable copy to the wardrobe shown (the current account's).
- A **Create** tab: categories (Looks, Body, Face, Hair, Tops, Bottoms, Shoes, Hats, Accessories), a grid of
  pieces, put on / take off, up to three colour areas per piece each with quick colours, the system picker
  and a hex field, a **Wearing** row with × per piece, **Make part of the skin**.
- **Paint** is today's editor, unchanged except that it paints the layer under the pieces.
- Pieces keep their light and shadow when recoloured.
- Classic and slim arms for every piece.
- Works offline after the first catalog download. Works for offline accounts (wear stays Microsoft-only).
- English and Spanish names for every piece, look, colour area and tag.
- Staff add content through pull requests to PerlandEngine with a picture to review; no launcher release.

## Non-goals

- Player uploads, sharing looks with friends, ratings, comments, a public gallery on the web.
- Proxying, mirroring or linking third-party catalogs.
- A panel screen for managing the catalog (later, if the catalog outgrows pull requests).
- Capes, HD (128×128) skins, Bedrock, animated pieces, patterns or textures as player-choosable options
  (pieces may have fixed detail; players choose colours).
- Offline skins in game (open question from the per-account spec, unchanged).

## Anti-goals

- No content from players or third parties reaches another player.
- No Mojang assets, no Steve/Alex lookalikes, no copying other launchers' or sites' art or branding.
- No account or Perland sign-in needed to browse or use the catalog.
- The finished skin is still one 64×64 PNG: wear, export, face thumbnails and version history keep working
  on it unchanged.

## Approaches considered

- **A. Engine serves the catalog, launcher keeps a copy (chosen).** Content grows without launcher
  releases; matches "within perland and perland engine"; offline covered by the cache. Costs Engine work.
- **B. Ship everything inside the launcher.** No Engine work, offline from day one, but every new piece
  waits for a release and it isn't the Engine-served catalog the owner asked for.
- **C. Built-in starter set plus Engine extras.** Two delivery paths for the same content; the cache in A
  already covers offline.

Technical choices (mine): pieces are PNGs drawn by a generator program, not hand-edited files, so staff
review code plus a contact sheet and every piece is regenerated consistently. Recolouring and putting pieces
together happen in the launcher UI in TypeScript, next to `paint.ts`, because the UI already holds RGBA
buffers and Vitest covers pure logic; the backend keeps storing PNGs.

## How it works

### The data model

**Slots** (fixed, ordered; drawing order = this order, later on top):

`body`, `eyes`, `brows`, `mouth`, `face_detail` (freckles, blush, scars), `beard`, `bottom`, `shoes`, `top`,
`belt`, `hands`, `neck`, `back`, `hair`, `glasses`, `hat`.

One piece per slot. Face parts and accessories have their own slots so a player can wear glasses, a scarf,
gloves, a backpack and a hat at once. The order is part of the format; changing it is a catalog format
version bump.

**A piece**: `id` (slug, never reused), `slot`, `name {en, es}`, `tags`, up to three colour `areas`
(`id`, `name {en, es}`, `default` hex), and per arm model (`classic`, `slim`) two files:

- `art`: a 64×64 RGBA PNG in skin UV layout. Opaque pixels are the piece. Pixels in a colour area are drawn
  in grey, where the grey level is the shading (128 = the chosen colour as is, lighter toward white,
  darker toward black). Pixels outside any area keep their own colour (fixed detail such as a zip, a logo,
  metal).
- `mask`: a 64×64 PNG whose red channel marks the area of each pixel: 0 fixed, 85 area 1, 170 area 2, 255
  area 3. A mask pixel is only meaningful where `art` is opaque.

A piece that never touches the arms may point both models at the same files. The `body` slot's pieces cover
the whole base layer and have an area `skin`; hair pieces have `main` and optionally `tips`.

**Recolouring** (pure function, `recolour(art, mask, colours)`): for an area pixel with grey `g` and chosen
colour `c`: `g = 128` gives `c`; `g > 128` mixes `c` toward white by `(g-128)/127`; `g < 128` mixes toward
black by `(128-g)/128`. Alpha from `art`. The exact curve may be tuned by the builder if it looks better;
the 128 = chosen colour rule stays so defaults look as designed.

**A look** (catalog entry): `id`, `name {en, es}`, `tags`, `model` (classic or slim), `pieces: [{ id,
colours: { areaId: hex } }]`, and optionally `painting` (a 64×64 PNG under the pieces, for hand-painted
looks). Thumbnails are rendered by the generator.

**Composition** (pure, `compose(look, files, model)`): draw the `body` and face slots (`body`, `eyes`,
`brows`, `mouth`, `face_detail`, `beard`) in order, then the **painting**, then every other slot in order,
each recoloured, with normal alpha-over (my call: the painting sits above the body and face and under the
clothes, so a player can paint tattoos, make-up or a whole imported skin and still wear pieces over it;
transparent painting texels show the body below). Outer-layer texels are just texels in the
outer-layer UV regions, so jackets that stand out are drawn there. The result is the 64×64 PNG the wardrobe
saves.

### PerlandEngine: content, generator and routes

Content and generator live in PerlandEngine (private), so the art never ships in the public launcher repo.

- `cmd/skin-catalog`: a Go program holding the art as code: drawing helpers over a Go copy of the skin UV
  layout (every part's faces for both models and both layers, matching the launcher's `layout.ts`) and one
  function per piece and look. It writes `internal/skincatalog/data/` (PNGs named by their SHA-256, and
  `manifest.json`) and a contact sheet (every piece in its default colours and in two other colourings on
  a figure, every look, both models) for review.
- CI regenerates and fails when the committed data differs from what the generator writes, and runs content
  checks: every id unique and slug-shaped, every name present in en and es, every piece has both models,
  every `art` pixel lies inside the valid skin UV regions, mask areas exist in the piece's `areas`, every
  look references existing pieces and areas, a look has at most one piece per slot.
- `internal/skincatalog` embeds `data/` (`go:embed`) and serves it. Adding content = a pull request with the
  contact sheet (screenshots per the Engine's `pr-screenshots` skill), merged and deployed like any change.
- Pieces and looks are never deleted from the manifest once published, only marked `retired: true` (hidden
  from browsing, still served), so a player's look that uses one keeps working.
- **Published art never changes** (my call): `internal/skincatalog/published.json` records every published
  piece id with its file hashes, and CI fails when a recorded piece's hashes change or a recorded id
  disappears. Changing a piece's art means a new id (the old one may be retired). Names, tags and default
  colours may change in place. So a saved look (ids and colours) always recomposes the same, and the
  launcher's cache keeps every file of every piece a wardrobe look uses.
- **Content batches** (my call): the first Engine pull request ships the format, routes, generator and a
  small batch (a few pieces per category, a few looks) so the launcher can be built and tested against it;
  the rest of the agreed set follows in further Engine pull requests. The launcher release that shows the
  Catalog and Create tabs to players waits until the full agreed set (about 30 looks, 120 pieces) is merged.

Routes (public tag, no auth, any origin, standard envelope for JSON):

| Route | Answer |
| --- | --- |
| `GET /api/v1/skin-catalog` | the manifest: `formatVersion`, `version` (its own SHA-256), `license`, `slots`, `tags`, `pieces`, `looks`, with every file referenced by hash. `ETag` = version, 304 on `If-None-Match`, `Cache-Control: public, max-age=300`. |
| `GET /api/v1/skin-catalog/files/:sha256` | the PNG, raw, `Cache-Control: public, max-age=31536000, immutable`; 404 for a hash not in the data. |

Per-client-IP limits through `publiclink.Limiter` (manifest 60 a minute, files 600 a minute), plain 429.
`SKIN_CATALOG_ENABLED` (default true; off answers 404 like the public list). Changelog, roadmap, overlay
examples and swagger per the Engine's "Finishing a change". No database, no new error codes.

Licence: the catalog's art is published as **CC0** so players may use and change it freely for their own
skins (my call; the manifest's `license` field carries it). Shown as "By Perland".

### Launcher: fetch, cache, storage

**launcher-core `skin_catalog`** (HTTP tests with wiremock):

- The Engine URL is the one the launcher already uses for Perland routes (`check_engine_url`, as
  `public_list.rs` does). No token is sent.
- Fetch the manifest with `If-None-Match`; caps: manifest 1 MiB, each PNG 64 KiB and exactly 64×64,
  at most 4000 files. Every downloaded file's SHA-256 must equal its name, else it's discarded.
- Cache under `<root>/cache/skin-catalog/`: `manifest.json` and `files/<sha256>.png`, atomic writes.
  Files are fetched on demand (the grid's thumbnails first) and kept; files no longer referenced by the
  manifest and by no wardrobe look are pruned on manifest update.
- An unknown `formatVersion` (newer than the launcher understands) keeps the last cached manifest and shows
  "update the launcher to see new pieces".
- Offline or Engine down: the cached manifest is used; with no cache, Catalog and Create show a plain
  message and Paint works as today. New `ErrorCode` `SkinCatalogUnavailable`.

**Wardrobe storage** (additive to the per-account format):

- An entry may carry `look`: `{ pieces: [{ id, colours }], painting: bool }`. When `painting` is true the
  painting under the pieces is `<root>/skins/<id>.paint.png`. `<id>.png` stays the finished image, always
  written together with the look under the wardrobe's lock.
- An entry without `look` is a plain painted skin, exactly as today.
- Delete removes `<id>.paint.png` too; duplicate copies it.
- Version history keeps finished PNGs only. Restoring a version (or making a copy from one) gives a plain
  painted skin: the look is dropped and the restored PNG becomes the painting. That keeps history's format
  unchanged; a later version can store looks too.

**launcher-app commands** (contract JSON regenerated):

| Command | Does |
| --- | --- |
| `skin_catalog()` | `{ manifest, status: fresh | cached | unavailable }`, refreshing in the background when cached |
| `skin_catalog_file(sha256)` | the PNG as a data URL (CSP `img-src data:`), fetching and caching it if needed |
| `skin_look_save(id, look, painting?, texture)` | stores the look, the painting PNG and the finished PNG together |
| `skin_look_clear(id)` | drops the look; the finished PNG becomes the plain painting |

Adding a catalog look to the wardrobe = `skin_create(name, model, texture, owner = shown account)` with the
composed PNG, then `skin_look_save`. Wear, export and history use the finished PNG as today.

### Launcher UI

As in the mockups the owner approved, `2026-11-07-skins-catalog-mockups/` (`create-tops.png`,
`create-face.png`, `catalog.png`; `mockups.html` is their source; figures and pieces there are placeholders,
not the art). Create / Paint / Catalog tabs above the existing stage; wardrobe left; figure centre with
**Wear this skin**:

- **Catalog:** search box (names in the current language, tags), tag chips (New, Fantasy, Sport, Jobs,
  Sci-fi, Casual, Spooky, ...; from the manifest), a grid of looks with thumbnails, a preview panel with the
  look on a figure (posable like the stage), its pieces listed, and **Add to my wardrobe** (amber, the
  tab's one amber key), which selects the new skin and switches to Create.
- **Create:** category list; a grid of pieces in that category (retired ones hidden unless worn); the chosen
  piece's colour areas; **Take off**, **Make part of the skin**. The latter bakes **every** worn piece into
  the painting and leaves no pieces (my call: baking one middle piece would change how the pieces drawn
  before it look); it asks first and is one undo step. Looks category = the catalog's looks applied
  to the current skin (replaces the pieces, keeps the painting) after a confirm when pieces would be lost.
  **New** in the wardrobe gains "Build from pieces" (starts with a default body).
- **Wearing** row under the stage: one chip per worn piece with ×.
- **Paint:** today's tools paint the painting layer; the stage shows the finished skin. When pieces cover
  what the player paints, a quiet line says "Pieces are on top. Take them off or make them part of the skin
  to paint over them." Eyedropper picks from the finished skin.
- Every change autosaves 600 ms after the last change, like strokes. Undo/redo covers piece and colour
  changes too (same history stack).
- Pure logic with Vitest: `recolour`, `compose`, slot replacement, look ↔ wardrobe conversion, search.
  The stage adapter stays mocked.
- All strings in `en.json` and `es.json`; `errors.skin_catalog_unavailable.message` in both.

## Safety

Everything in the catalog is drawn by Claude in code, reviewed by staff in a pull request with a contact
sheet, and merged by staff. Nothing a player makes leaves their launcher except through Minecraft's own
skin upload, as today. No moderation queue or reporting is needed in this version.

## Left to the builder

The art itself (within the agreed counts and styles), exact shading curve, tag list beyond the examples,
default colours, quick-colour palettes per area, spacing within DESIGN.md (UI styling through `/impeccable`),
how thumbnails are rendered by the generator, cache pruning timing, the exact limits' numbers if the ones
above prove awkward, splitting the work into Engine and launcher pull requests.
