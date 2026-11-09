# Adjust the background: position and zoom

Date: 2026-11-08. Status: approved by the owner in the brainstorming session (mockups only; nothing built yet).

## Intent

A player who uploads a picture as an instance's background can only choose which point stays in view: the
marquee always covers Home's header (`object-fit: cover`), so a square picture such as a character drawing
comes out huge, with its head and feet cut off, and its feet behind the instance name. The player wants to
**zoom in and out** and **move** the picture until it sits well, seeing the result before leaving.

## Goals

- Zoom a picture background in (crop tighter) and out (see more of it, down to all of it).
- Move it, as Position does today, together with the zoom.
- When the picture no longer covers the header, the player picks what fills the edges: **Dark** or **Pixel**.
- A preview that shows exactly what Home will show, with the name and Play drawn dimmed over it, so the player
  sees whether the picture sits behind them, and nothing covers the preview itself.
- "Change image…" stays as compact as it is today.

## Non-goals

- Cropping or editing the picture file. The stored picture is never changed.
- Adjusting the icon, the generated landscapes (drawn to fit) or anything outside Home's header.
- Rotation, filters or per-window-size settings.

## Anti-goals

- A control that covers the picture while adjusting (the in-place toolbar was rejected for this).
- A dialog stacked on top of another dialog.
- Blur anywhere: DESIGN.md bans it ("Nothing glows, blurs or eases"). The owner first asked for blurred edges
  and accepted the pixel edges as their on-brand replacement.

## Approaches considered

1. **Store the view, draw it live (chosen).** `instance.json` keeps the zoom and edge choice next to
   `bannerFocus`; Home draws the picture with them. Lossless and re-adjustable, Reset goes back to today's look,
   and it follows Position's path (live preview, save on release). Costs a core field, the IPC contract, the mock
   and the UI.
2. **Bake a cropped file in the core.** Rejected: loses the original (zooming out after zooming in can't bring
   the cut part back), and the header changes shape with the window, so a fixed crop is wrong after a resize.
3. **Keep it in the UI only (browser storage).** Rejected: lost on reinstall, not part of the instance.

For the UI, three layouts were tried and rejected before the chosen one: Position and zoom inline in
"Change image…" (the dialog grows too tall), editing on Home itself with a floating toolbar (the toolbar covers
the picture), and a second dialog stacked over the first.

## What the owner decided

### Entry point

- In "Change image…", the **Background** heading row gets a ghost key on its right: a pixel "adjust" glyph and
  "Adjust…" / "Ajustar…". It shows only while a picture is the background (an upload, a screenshot, or the
  pack's art), the same rule Position uses today. A generated landscape has nothing to adjust.
- The inline **Position** section leaves "Change image…": its job moves into the adjust view.

### The adjust view

- The **same dialog** switches to the view; no second dialog. Its header is the launcher's standard back key
  (ghost icon key, `ChevronDown` turned 90°, labelled "Back" / "Volver", as in Skins and `ScreenFrame`) and the
  title "Adjust the background" / "Ajustar el fondo".
- One dim line under it: "Drag the picture to move it and use the zoom to bring it closer or further away." /
  "Arrastra la imagen para moverla y usa el zoom para acercarla o alejarla."
- **The preview** fills the dialog's width inside a recessed well (`ground`, 1px `line` ring, notched corners).
  It has the shape of Home's header at the current window size and shows Home's header as it will look: the
  picture with its zoom, position and edges, and Home's overlays (title, version line, Play, Mods & packs, ⋯,
  Continue, the two top keys) dimmed to about 40% and inert. It must fit with the controls and footer at
  960 × 600 without the dialog scrolling.
- **Moving:** drag the picture in the preview (grab cursor). For the keyboard and screen readers, two
  visually hidden range inputs carry the position as today ("Left to right" / "Top to bottom", steps of 5).
- **Zoom row:** "Zoom" label, a ghost "−" key, the standard slider, a ghost "+" key, and the value in percent
  (tabular). The mouse wheel over the preview zooms too.
- **Edges row:** "Edges" / "Bordes" with two small segments, **Dark** / **Oscuros** and **Pixel** /
  **Pixelados**. It shows only while the zoom leaves part of the header uncovered; while hidden it keeps its
  height, so the view does not jump as the zoom changes. Dark is the default.
- **Footer:** ghost "Reset" / "Restablecer" at the left (back to today's look: default position, 100% zoom,
  dark edges; dimmed while already there) and secondary "Done" / "Listo" at the right, with the dialog's normal
  spacing below it. Nothing is lit.
- Every change shows at once in the preview and in the real marquee behind the backdrop, and saves the way
  Position saves today: when a drag ends, 300 ms after the last key press or wheel step, when the slider is
  released, and at once for Reset and the edge segments.

### How the picture is drawn

- **Zoom** is a percentage of the size that just covers the header: 100% is today's look. Above 100% the
  picture is larger than the header (more is cut off); below, it is smaller and leaves edges.
- **Position** keeps `object-position`'s meaning on both sides of 100%: x% and y% place the picture inside the
  free space (or the overflow), so 50/100, the default, is the bottom middle as today.
- **Dark edges:** the header's own `ground` shows around the picture.
- **Pixel edges:** the same picture shrunk to about 24 cells across (keeping its shape), drawn with hard pixels
  (nearest-neighbour) to cover the header, darkened (about 40% brightness) behind the picture. It reads as
  blocks, never as blur.
- The stepped scrim, grille and vignette stay over everything, as now.
- Picking a new background (any pick in "Change image…") starts again from the defaults, as Position does now.

### Core and data

- `Instance` gains `banner_zoom: Option<u16>` (percent; `None` is 100) and `banner_edges: Option<BannerEdges>`
  (`dark` | `pixel`; `None` is dark; an unknown value from a newer launcher loads as dark). Old files load
  unchanged.
- The banner settings are saved together through one command that replaces `instance_banner_focus_set`:
  `instance_banner_view_set(id, view)` with `view = { focus, zoom, edges }`. Same refusals as today (busy,
  installing). A focus outside 0–100 stays `banner_focus_invalid`; a zoom outside the allowed range is a new
  `banner_zoom_invalid` with messages in `en.json` and `es.json`.
- A new background resets all three (as `instance_art_set` resets the focus today).

## Calls left to the builder (marked as mine)

- **Zoom range: 25% to 300%, in steps of 5** (my call). 25% shows any normal picture whole at any window size
  the launcher allows; 300% is tight enough for a detail.
- **Done closes the whole dialog**; Back returns to "Change image…" (my call, so the two keys don't do the same
  thing).
- How the preview reproduces Home's header (rendering the header itself inert and scaled, or a dedicated
  preview component) and how the pixel image is made (a small canvas, cached per picture).
- The adjust glyph (from pixelarticons, like every other icon).
- Updating DESIGN.md's "Change image" and "Marquee" sections to the new behaviour.

## Done means

- The flow above works in `pnpm --filter ui dev:mock` and in the app, at 960 × 600 and larger.
- Tests: core (fields, defaults, lenient loading, validation, reset on a new background), app command,
  contract regenerated, UI (entry key visibility, view switch, drag/keys/slider/wheel saves, edges row
  visibility, Reset, Back and Done), all strings in both languages.
