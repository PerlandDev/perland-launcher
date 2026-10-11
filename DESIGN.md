---
name: Perland Launcher
description: A free, bilingual Minecraft launcher where every instance is an arcade cabinet and Play is the one lit button.
colors:
  ground: "#181410"
  panel: "#221c17"
  raised: "#2e261f"
  line: "#463a2f"
  ink: "#f6f1e7"
  ink-dim: "#c5b9a8"
  amber: "#ffc233"
  amber-hi: "#ffd76e"
  lime: "#c9cc2a"
  coral: "#ff5a5f"
  blue: "#2186cb"
  blue-ink: "#5cb3f0"
  dirt: "#7a4a2a"
  dirt-deep: "#4e2e19"
typography:
  display:
    fontFamily: "Bungee, ui-sans-serif, system-ui, sans-serif"
    fontSize: "clamp(2.5rem, 5.4vw, 5.5rem)"
    fontWeight: 400
    lineHeight: 0.95
  headline:
    fontFamily: "Bungee, ui-sans-serif, system-ui, sans-serif"
    fontSize: "2rem"
    fontWeight: 400
    lineHeight: 1
  play:
    fontFamily: "Bungee, ui-sans-serif, system-ui, sans-serif"
    fontSize: "2rem"
    fontWeight: 400
    lineHeight: 1
    letterSpacing: "0.02em"
  title:
    fontFamily: "Bungee, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.25rem"
    fontWeight: 400
    lineHeight: 1.4
  body:
    fontFamily: "Atkinson Hyperlegible Next Variable, ui-sans-serif, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Atkinson Hyperlegible Next Variable, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 600
    lineHeight: 1.43
  control:
    fontFamily: "Atkinson Hyperlegible Next Variable, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.9375rem"
    fontWeight: 650
    lineHeight: 1
  mono:
    fontFamily: "Atkinson Hyperlegible Mono, ui-monospace, monospace"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: "20px"
rounded:
  hairline: "2px"
  thumb: "3px"
  step: "4px"
  panel: "6px"
spacing:
  tight: "0.25rem"
  control-gap: "0.75rem"
  card: "1.25rem"
  dialog: "1.5rem"
  gutter: "2.5rem"
  section: "2.5rem"
  rail: "4.5rem"
  titlebar: "2.25rem"
  drawer-handle: "2.25rem"
  drawer-open: "18rem"
  column: "48rem"
  form-column: "42rem"
components:
  button-secondary:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
    typography: "{typography.control}"
    rounded: "{rounded.step}"
    padding: "0 1rem"
    height: "2.5rem"
  button-primary:
    backgroundColor: "{colors.amber}"
    textColor: "{colors.ground}"
    typography: "{typography.control}"
    rounded: "{rounded.step}"
    padding: "0 1rem"
    height: "2.5rem"
  button-primary-hover:
    backgroundColor: "{colors.amber-hi}"
  button-danger:
    backgroundColor: "{colors.coral}"
    textColor: "{colors.ground}"
    rounded: "{rounded.step}"
    padding: "0 1rem"
    height: "2.5rem"
  button-ghost:
    textColor: "{colors.ink}"
    rounded: "{rounded.step}"
    padding: "0 1rem"
    height: "2.5rem"
  button-disabled:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink-dim}"
    rounded: "{rounded.step}"
  button-large:
    padding: "0 1.5rem"
    height: "3rem"
  play:
    backgroundColor: "{colors.amber}"
    textColor: "{colors.ground}"
    typography: "{typography.play}"
    rounded: "{rounded.panel}"
    padding: "0.75rem 1.5rem"
    width: "17.5rem"
    height: "5.5rem"
  play-running:
    backgroundColor: "{colors.coral}"
    textColor: "{colors.ground}"
  input:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.step}"
    padding: "0.5rem 0.75rem"
    height: "2.5rem"
  segment:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
    typography: "{typography.control}"
    rounded: "{rounded.step}"
    padding: "0 1.125rem"
    height: "2.5rem"
  segment-checked:
    backgroundColor: "{colors.ground}"
  panel:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.panel}"
    padding: "1.25rem"
  dialog:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.panel}"
    padding: "1.5rem"
    width: "30rem"
  badge:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink-dim}"
    rounded: "{rounded.step}"
    padding: "0.125rem 0.5rem"
  rail-item:
    backgroundColor: "{colors.panel}"
    rounded: "{rounded.panel}"
    size: "3.5rem"
  rail-item-selected:
    backgroundColor: "{colors.raised}"
  titlebar-button:
    textColor: "{colors.ink-dim}"
    padding: "0 0.75rem"
    height: "2.25rem"
---

# Design System: Perland Launcher

## Overview

**Creative North Star: "The Arcade Cabinet"**

Every instance is an arcade cabinet. Its seeded landscape is a backlit marquee, the rail down the left is a row of cabinets with a coin-slot light beside the one you're standing at, and Play is the one lit button you press. The room is warm laminate black, never navy. Controls are physical: they stand on a brown dirt step taken from the PerlandDev land-plot mark, and pressing one drops its face onto that step in two snapped frames. Nothing glows, blurs or eases. Depth is hard offset layers, corners are pixel-notched, and state changes snap.

The density is low and the hierarchy is blunt, because the job is "open, press Play, be in the game" for young, often Spanish-speaking players. Home is almost all marquee art: the instance name is huge at lower left, and Play sits lower right with Mods and the overflow menu as quieter raised controls beside it. Every other screen is a plain 48rem column on the ground under a Bungee title, with sticky action rows. The four plot hues are strictly functional: each owns one meaning, and at rest only Play is lit.

The system rejects the launcher category default of a navy gradient, rounded cards and a glowing blue button. It also stays clear of the PerlandDev Panel's cyan chamfered "Director HUD" look.

**Key Characteristics:**
- Warm laminate-black ground (`ground`, `panel`, `raised`, `line`) with warm off-white ink.
- Four plot hues with one job each: amber acts, lime means running or OK, coral means stop or fault, blue means info or progress.
- Raised controls stand on a dirt step: hard offset under-layers, never a blurred shadow.
- Pixel-notched corners (`corner-shape: notch`) on every control and panel.
- Bungee signage display with a two-layer hard shade. Atkinson Hyperlegible Next for body text, Atkinson Hyperlegible Mono for logs and paths.
- Motion snaps in `steps()`: 180 ms press travel in two steps, a snapped marquee dim, and a slow attract-mode parallax that stops under reduced motion.

## Colors

A warm brown-black cabinet laminate carrying the four saturated land-plot hues, each used as a signal rather than as decoration.

### Primary
- **Coin-Op Amber** (`amber`): means act. It fills Play, the first-run "Start" and "Continue" calls, primary buttons, text selection, the caret, the focus ring, the coin-slot light beside the selected rail cabinet, and the mods drop-zone cue. It sits on a `dirt` step. Text on it is `ground`.
- **Lamp Amber** (`amber-hi`): the hover face of amber buttons and the lit top edge inside Play. Play's idle animation breathes that edge toward a paler cream in three steps.

### Secondary
- **Run-Lamp Lime** (`lime`): means running or OK, and marks a selection. It colours the rail LED on a cabinet that is preparing or playing, the activity count badge, the "on" knob of a toggle, the square lamp inside a checked segment, and success confirmations ("Java found", active account).
- **Fault Coral** (`coral`): means stop or fault. It colours Play while the game runs (with a pale coral lit edge, `#ff9a9d`), danger buttons, danger menu items, the warning diamond on error cards, toasts and damaged rail cabinets, invalid-field rings, field error text and the title-bar close hover.

### Tertiary
- **Signal Blue** (`blue`): means info or progress, and is only ever a fill. It lights the blocks of the segmented meter, the filled part of the memory slider track, and the completed first-run step bars.
- **Readout Blue** (`blue-ink`): blue as text on the dark ground, for links, inline "add an account" prompts, info icons and the high-memory advisory.

### Neutral
- **Laminate Black** (`ground`): the window, title bar, rail and screen background. Also the recessed fill of inputs, toggles, the meter channel and a checked segment.
- **Cabinet Panel** (`panel`): cards, dialogs, the activity drawer, rail cabinets at rest and disabled control faces.
- **Raised Face** (`raised`): the face of secondary buttons, segments, menus, badges and the selected rail cabinet. Hover lifts it with 7% white.
- **Seam Line** (`line`): 1 px inset borders on recessed controls, empty meter blocks, scrollbars, dividers and the hairline above sticky action rows.
- **Warm Ink** (`ink`): primary text, the slider thumb.
- **Dim Ink** (`ink-dim`): secondary text, hints, placeholders, file paths, title-bar icons at rest.
- **Dirt** (`dirt`): the step under amber controls and Play, and the first layer of the marquee name's shade.
- **Deep Dirt** (`dirt-deep`): the step under every other raised control, dialogs, menus, toasts and the first-run card. It is also the inner line of the marquee's T-molding and the sub-text colour on Play's amber face.

### Named Rules
**The One Lit Button Rule.** At rest, Play is the only lit thing on screen. Amber means "press this to act". Never use it as a decorative accent, a heading colour or a selected-state fill.

**The Lime Lamp Rule.** A chosen option shows a small square lime lamp and sits pressed into its slot. Selection never borrows amber, because amber is reserved for acting.

**The Hue Has One Job Rule.** Lime is running or OK, coral is stop or fault, blue is info or progress. A status never relies on hue alone, so pair it with a label, an icon or a shape (the LED, the warning diamond, the meter blocks).

## Typography

**Display Font:** Bungee (self-hosted via `@fontsource/bungee`, with a system sans fallback)
**Body Font:** Atkinson Hyperlegible Next Variable (self-hosted, with a system sans fallback)
**Label/Mono Font:** Atkinson Hyperlegible Mono (self-hosted, with `ui-monospace` fallback)

**Character:** Bungee is arcade-marquee signage: blocky, all-caps by design, and loud. Atkinson Hyperlegible is the opposite, a face built for readers with low vision and for readers still learning, here chosen for young and non-native players. Signage names things. Atkinson says everything else.

### Hierarchy
- **Display** (Bungee 400, `clamp(2.5rem, 5.4vw, 5.5rem)`, 0.95): the instance name on the marquee, wrapping anywhere, with the two-layer marquee shade.
- **Play** (Bungee 400, 2rem, 0.02em tracking): the word on Play, Stop and Stopping. While Play is preparing it switches to the 650-weight body face, left-aligned, so the phase label and meter fit.
- **Headline** (Bungee 400, 2rem, 1): screen titles in the screen frame. First-run step titles run 1.875–2.25rem.
- **Title** (Bungee 400, 1.25rem): small signage such as the About heading and the tabular memory readout.
- **Body** (Atkinson Next 400, 15px, 1.5): all running text. Paragraphs use `text-wrap: pretty` and headings use `balance`.
- **Control** (Atkinson Next 650, 0.9375rem, 1): button and segment labels. Large buttons use 1rem.
- **Label** (Atkinson Next 600, 0.875rem): field labels, the drawer handle, title-bar account name. Hints and errors use the same size at 400.
- **Mono** (Atkinson Mono 400, 13px / 20px): the game log, error detail, Java and data-folder paths. Paths are user-selectable and truncate.

### Named Rules
**The Hard Shade Rule.** Signage over art carries a two-layer hard shade, never a blur: `dirt` offset 0.045em, then `ground` offset 0.09em. The version line under the name sits on a translucent laminate plate, so the art never decides its contrast.

**The Signage Names, Atkinson Speaks Rule.** Bungee appears only on names and titles (the instance, the screen, the welcome, Play, a big readout). Sentences, labels, buttons and errors are always Atkinson.

**The Tabular Rule.** Every live number (files, MB, memory, counts) uses tabular figures so readouts don't jitter while they count.

## Layout

The shell is a fixed desktop grid with a minimum window of 960 × 600 and no phone breakpoints:

- **Title bar:** 2.25rem tall across the top, on `ground`, and draggable everywhere except its buttons. The mark and name sit left. The account (avatar and name), settings, a 1 px divider and the window controls sit right. On first run and the settings-failed screen it is **minimal**: only the window controls, because nothing else is reachable yet.
- **Rail:** 4.5rem (72 px) down the left, on `ground`. Generated plot-icon cabinets are 3.5rem squares with 0.75rem between them, and a ghost "+" sits at the foot. The list uses roving arrow keys.
- **Main:** fills the rest. The activity drawer is pinned along its bottom edge as a 2.25rem handle. Opened, the drawer is 18rem tall, and `main` gets 18rem of bottom padding to match so the screen (and Play) stays above it rather than under it.
- **Home:** the marquee fills `main` edge to edge. A bottom grid of `[minmax(0,1fr) auto]` with a 2.5rem column gap and 2.5rem side gutters holds the name and version plate at lower left. The raised Mods button, the overflow menu and Play sit at lower right, aligned to their bottoms, with Play last at the far right.
- **Other screens:** a screen frame with a Bungee title row (2.5rem gutters, 2.25rem top), then a scrolling column capped at 48rem. Forms inside it run at 42rem, with 2.5rem between sections. Primary actions sit in a **sticky action row** at the bottom of the scroller on `ground`, with a 1 px `line` hairline above, so Save or Create stays reachable in long Spanish layouts.
- **First run:** the "perland" marquee in attract mode fills the window behind a centered 32rem panel card on a deep-dirt step. Blue bars across the top of the card count the steps.
- **Toasts** stack at the bottom-right in a 24rem column. **Dialogs** are `min(30rem, 100vw − 4rem)`, centred with auto margins (the CSS reset zeroes them) and capped at `100dvh − 4rem` tall, scrolling inside, so a long list never pushes the actions off a 600 px window.
- **Home, pack with an update:** the update plate takes its own full-width row of the bottom grid, above both the name and Play, so it reads on one line at 960 px.

Rhythm: 0.75rem between sibling controls, 1.25–1.5rem inside cards and dialogs, 2.5rem gutters and section gaps.

## Elevation & Depth

Depth is physical and hard-edged. Nothing in the system uses a blurred shadow. A raised object stands on a solid, zero-blur offset under-layer in dirt brown (the "step"). A recessed object is sunk with a 2 px dark inset top edge plus a 1 px `line` inset ring. Pressing an object removes the step and translates the face down by exactly the step height, so the face lands where the step was.

### Shadow Vocabulary
- **Step** (`box-shadow: 0 4px 0 var(--color-dirt-deep)`): secondary buttons, segments and the selected rail cabinet. Amber buttons use `dirt`.
- **Large step** (`0 6px 0`): large buttons, menus and toasts.
- **Play step** (`0 8px 0 var(--color-dirt)` plus `inset 0 3px 0` lit edge): Play only. When running, the step turns to `dirt-deep`.
- **Card step** (`0 8px 0 var(--color-dirt-deep)`): dialogs and the first-run card.
- **Recess** (`inset 0 2px 0 rgb(0 0 0 / 0.45), inset 0 0 0 1px var(--color-line)`): inputs, selects, the slider track and a checked segment.
- **T-molding** (`inset 4px 4px 0 var(--color-ground), inset 6px 6px 0 var(--color-dirt-deep)`): the marquee's frame where it meets the title bar and rail.
- **Hairline** (`0 -1px 0 var(--color-line)`): above sticky action rows.

### Named Rules
**The Dirt Step Rule.** Everything that can be pressed stands on a hard dirt step: `dirt` under amber, `dirt-deep` under everything else. Pressing travels the full step height and removes it. No blur, ever.

**The Spent Step Rule.** A control whose action is running stays pressed down. Play while queued or preparing, a button with `aria-disabled="true"` while pending, and a disabled control all sit on their spent step with no under-layer.

## Shapes

Corners are pixel-notched, taken from the stepped grid of the land-plot mark: `border-radius` plus `corner-shape: notch` cuts a square step out of each corner rather than rounding it. Use 4px (`step`) on controls, inputs, badges and menu items, and 6px (`panel`) on Play, panels, dialogs, menus and rail cabinets. Small inner pieces (meter channel, toggle knob, slider track) use 2–3px. Where `corner-shape` is unsupported the same radii fall back to a slight rounding, which is acceptable.

The generated art keeps the same grid. The marquee is a 480 × 270 panel on an 8-unit cell grid drawn with `crispEdges`. Plot icons are 8 × 8 cells, the sun is a pixel-stepped disc, and status marks are solid squares (the rail LED, the lime selection lamp, meter blocks). Icons are stroked SVG with square caps.

## Components

### Buttons
Tactile, physical, and loud only when they mean it.
- **Shape:** notched 4px corners. 2.5rem tall with 1rem padding, or 3rem tall with 1.5rem padding and a 6px step when large. Icons are 1.25rem.
- **Five states:** **raised** (face on its step), **pressed** (`:active` or `data-pressed`: translated down by the step height with the step removed), **lit** (Play only), **recessed** (inputs, the meter channel, a checked segment), and **fault** (coral). A pending action uses `aria-disabled="true"`, which keeps focus and shows the button pressed with a progress cursor. A true `:disabled` button drops to a flat `panel` face with a `line` ring.
- **Primary:** amber face on a `dirt` step with `ground` text. Hover is `amber-hi`. It is for the one act on a screen (Create, Save, Continue).
- **Secondary:** `raised` face on a `dirt-deep` step. Hover mixes in 7% white.
- **Danger:** coral face with `ground` text, for destructive confirmations.
- **Ghost:** no face or step until hover shows `raised`. It does not travel when pressed. Used for back, dismiss, details and the rail "+".
- **Motion:** `translate` and `box-shadow` over 180 ms in `steps(2, end)`: two 90 ms frames, never an ease.

### Play (signature)
The one lit button: 17.5rem × 5.5rem minimum with notched 6px corners. It has an amber face, an 8px `dirt` step and a 3px lit top edge that breathes in three steps every 2.4 s while idle.
- **Idle:** "PLAY" in Bungee 2rem.
- **Checking / Asking / Queued / Preparing / Syncing:** pressed down onto its spent step, with the lit edge darkened and the breathing stopped. The face switches to body type, with 1rem side padding, and always holds three rows in one fixed height: the phase label alone on the full-width top row (it truncates rather than push anything out), the recessed 20-block blue meter in its `ground` channel, then a readout row with the tabular counts at left and "Cancel" in `dirt-deep` at bottom-right. The readout row stays present even when empty. Counts read "done of total files" and the MB figure is its own never-truncating span, in real MB; from 1000 files the counts shorten to compact "done/total" so the MB tail survives in Spanish. Pressing it cancels.
- **Checking** has no count, so one blue block sweeps the channel and back across the 20 block positions in `steps(20, jump-none)`. It is decorative and stays blue (progress). Under reduced motion it is hidden, leaving the honest empty channel. **Asking** (an update is waiting on the player's choice in a dialog) keeps the empty meter, because nothing is progressing.
- **Running:** coral face, `dirt-deep` step, pale coral lit edge, "STOP".
- **Disabled (no account):** a flat `panel` face with a 2px `line` ring, sitting pressed, plus a screen-reader reason and a visible blue-ink link to Accounts.

### Inputs / Fields
- **Style:** recessed. `ground` fill, the recess inset (dark top edge plus `line` ring), notched 4px corners, 2.5rem tall. Selects draw their own square-capped chevron in `ink-dim`.
- **Field:** a 600-weight 0.875rem label above, then the control, then a hint (`ink-dim`) or an error (`coral`), linked by `aria-describedby`.
- **Focus:** the global 2px amber outline at a 3px offset. The caret is amber.
- **Error:** the ring thickens to 2px `coral`.

### Segments, Toggle, Slider
- **Segment:** a raised `raised` face on a 4px `dirt-deep` step. When checked it is pressed into its slot: it translates down 4px, the face recesses to `ground`, and an 8px square **lime lamp** appears before the label. Focus shows the amber ring on the whole segment.
- **Toggle:** a recessed 3rem × 1.75rem `ground` channel. The knob is `ink-dim` when off, and when on it steps right (180 ms, `steps(2)`) and turns lime. Pending or disabled is `aria-disabled` at 50% opacity.
- **Slider:** a recessed 0.625rem track filled with `blue` up to the value. The thumb is a notched `ink` block on a 3px `dirt-deep` step, and the value reads out above it in tabular Bungee.

### Meter
A recessed 20-block segmented bar: 3px gaps, 0.75rem tall. Empty blocks are `line` and lit blocks are `blue`. It fills block by block, never smoothly. In the drawer it is a labelled progressbar. Inside Play it is decorative because Play's name already says it.

### Cards / Containers
- **Panel:** `panel` fill with notched 6px corners and no border. Padding is 1.25rem (error and crash cards), 1.5rem (dialogs, About) or 2rem (the first-run card).
- **Elevation:** flat on the ground, except dialogs, toasts and the first-run card, which stand on a deep-dirt card step.
- **Error card:** a coral warning diamond, the plain message in 600 weight, the fix in `ink-dim`, actions, and a ghost "Details" toggle that reveals the mono detail on a `ground` well.
- **Dialog:** the backdrop is `rgb(12 9 6 / 0.78)`.

### Navigation
- **Rail:** `panel` cabinets with notched 6px corners. Hover raises them to `raised`. The selected cabinet is `raised` on a 4px `dirt-deep` step, with a 4px amber **coin-slot light** to its left. A lime LED square in the lower-right corner marks preparing or playing. A damaged instance is greyed to 40% with a coral warning diamond. A pack's own icon replaces the generated plot icon (40px, 3px notched corners), falling back to the plot icon if it fails to decode.
- **Title bar:** flat text buttons 2.25rem tall in `ink-dim`. Hover shows `ink` on `raised`, and the close button's hover is `ground` on coral.
- **Overflow menu:** a `raised` popover on a 6px `dirt-deep` step with a `line` ring. Items hover to `panel`, and danger items are coral.

### Activity Drawer
A `panel` strip pinned to the bottom of `main`. The 2.25rem handle has a chevron, the label and a lime count badge when something runs. Opened, it is 18rem tall and holds instance tabs (when more than one), a labelled meter with tabular counts, the crash card, and the mono log on a `ground` well with a "follow" button.

### Update Plate
The pack's update status, on the marquee above the name. It is the version plate's translucent laminate with notched 4px corners, led by an 8px square status lamp: **blue** when an update is on offer, **lime** when the pack just updated ("Updated to X · Kept your version of N files"). A failed check shows no lamp and sets its text in `ink-dim`. The change line is `ink-dim` so the title leads. Its action is a compact 2rem secondary button that is **never lit**, because Play owns the lit amber on Home. The plate's foot leaves room for the button's 4px step.

### Dialogs for the pack flow
- **Update ready:** one lit primary, "Update and play". The change line and kept line sit above a "Show what changed" disclosure whose names list is a well.
- **Download these manually:** each missing file is a well row. Continue is flat and disabled until every file is found, then lit as the only primary.

### Well
A recessed row inside a panel or dialog: `ground` fill, 1px `line` inset ring, notched 4px corners. Used for the names list and the manual-download rows, which would otherwise vanish into a `panel` host.

### Pack screens
- **New instance** offers three source modes as the standard pressed segments with the lime lamp: version, pack link, .mrpack file. The link hint is the field's own hint (linked by `aria-describedby`); "No file chosen yet" is a body sentence in `ink-dim`, and only a chosen file name is mono.
- **Mods** splits into two labelled sections: "From the pack" (optional mods as toggles) and "Your additions".
- **Instance settings, Updates** is a segment group; the pack's game line and the "pack picks" hint sit tight (0.25rem) under the Game heading.

### Marquee (signature)
The instance's backlit marquee, generated from a seed (FNV-1a hash, mulberry32 PRNG) so an instance looks the same on every computer. The recipe:
1. **Sky:** `ground` with 4% of the lamp mixed in. It is a warm black, never navy.
2. **Lamp bloom:** a radial bloom of the warm tube light (`#ffd98a`, tinted 15% toward one plot hue) behind the horizon, fading from 0.7 to 0.26 to 0 opacity.
3. **Sun:** a pixel-stepped disc of radius 4 cells in pale lamp cream, at upper right. It is kept whole: when the panel is cropped short (drawer open), it moves down so it is never closer to the visible top than its radius plus one cell, and it sets behind the hills instead of being sliced.
4. **Hills:** four mirrored, seamlessly tiling silhouette bands with stepped values. The far bands catch the most lamp light (0.5, 0.3, 0.14). Each carries a one-cell rim in a plot hue. Drops deeper than one cell span at least two columns, so no single-cell "drips".
5. **Near floor:** the nearest band is nearly `ground` (8% dirt) with a muted rim, a dark floor the name sits on.
6. **Grille and vignette:** a 3-unit horizontal grille at 10% black, then a radial vignette to 55% black at the edges.
7. **Bezel:** the T-molding inset (`ground` 4px, then `dirt-deep` 6px) along the top and left where the marquee meets the title bar and rail.
- **Attract mode:** at rest the hill layers drift in parallax (180 s, 140 s, 100 s, 60 s per loop, nearer layers faster). The drift pauses when not idle and is neutralised under `prefers-reduced-motion`.
- **Pack banner:** a pack's own art replaces steps 1 to 5, anchored to the floor with `object-fit: cover`, under the same grille and vignette. A four-band **stepped scrim** keeps the name and plates legible: laminate bands in whole 8-unit cells, `crispEdges`, at 0.6, 0.42, 0.26 and 0.12 opacity from the floor up. No blur and no smooth gradient. If the banner fails to decode the panel falls back to the generated scene.
- **Pack art scaling:** `image-rendering: pixelated` applies only where the art is blown up at least 2x (`data-upscale`: the larger cover ratio of box to natural size). Real art that shrinks or grows slightly scales smoothly, because nearest-neighbour on a small upscale gives uneven pixels. The same rule governs rail icons.
- **Dim:** while a game is checking, queued, preparing or running, the panel and any pack banner snap to `brightness(0.5) saturate(0.75)` over 180 ms in `steps(2)`.

## Do's and Don'ts

### Do:
- **Do** keep Play the only lit control at rest. Amber is for acting: Play, the single primary action on a screen, focus, the caret and the coin-slot light.
- **Do** mark a chosen option with the pressed segment and its 8px lime lamp.
- **Do** stand every pressable control on a hard dirt step (`0 4px 0`, `6px` large, `8px` for Play and cards), and press it the full step height in `steps(2, end)` over 180 ms.
- **Do** keep a pending control pressed with `aria-disabled="true"` so it keeps focus. Reserve `disabled` for unavailable controls.
- **Do** notch every corner with `corner-shape: notch` (4px controls, 6px panels).
- **Do** recess inputs, toggles, the slider track and the meter channel into `ground` with the 2px dark top edge and 1px `line` ring.
- **Do** shade signage over art with the two-layer hard shade (`dirt` 0.045em, `ground` 0.09em), and seat small text on a laminate plate.
- **Do** fill progress block by block in blue with tabular counts in real MB, and never truncate the MB figure.
- **Do** keep update actions raised, never lit: a status plate leads with a square lamp (blue offer, lime done), and the dialog's one primary is the only amber.
- **Do** sink rows inside panels and dialogs as a `.well`, and scale pack art with nearest-neighbour only at 2x or more.
- **Do** pad `main` by the drawer's open height so Play is never covered, and keep primary form actions in a sticky row.
- **Do** let attract-mode parallax stop under reduced motion. State changes snap.

### Don't:
- **Don't** use blurred box-shadows, glows or soft elevation. Depth is hard offset layers only.
- **Don't** fill a selection, a heading or decoration with amber. A lit amber selection makes two lit buttons.
- **Don't** use a navy gradient, rounded cards or a glowing blue button, which are the launcher category default this system replaces.
- **Don't** blur or smooth-gradient a scrim over art. Use stepped, cell-aligned bands.
- **Don't** ease state changes or animate progress smoothly. Use `steps()` or snap.
- **Don't** set sentences, labels or errors in Bungee. Don't set the log or paths in anything but Atkinson Hyperlegible Mono.
- **Don't** use any hue as the only carrier of status. Pair it with a label, an icon or a shape.
- **Don't** reuse the PerlandDev Panel's cyan chamfered mark or HUD language, Modrinth branding or Mojang artwork. Marquees, plot icons and avatars are generated.
