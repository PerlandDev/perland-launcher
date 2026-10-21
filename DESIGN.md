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
    fontSize: "clamp(2.5rem, 5.2cqi, 5.5rem)"
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
  row-name:
    fontFamily: "Bungee, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.125rem"
    fontWeight: 400
    lineHeight: 1.25
  tag:
    fontFamily: "Atkinson Hyperlegible Next Variable, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 600
    lineHeight: "1.375rem"
  badge:
    fontFamily: "Atkinson Hyperlegible Next Variable, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.75rem"
    fontWeight: 650
  md-h2:
    fontFamily: "Atkinson Hyperlegible Next Variable, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.375rem"
    fontWeight: 700
    lineHeight: 1.25
  md-h3:
    fontFamily: "Atkinson Hyperlegible Next Variable, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.125rem"
    fontWeight: 700
    lineHeight: 1.25
  md-h4:
    fontFamily: "Atkinson Hyperlegible Next Variable, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 700
    lineHeight: 1.25
  md-h5:
    fontFamily: "Atkinson Hyperlegible Next Variable, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 700
    lineHeight: 1.25
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
  rail-wide: "15rem"
  titlebar: "2.25rem"
  drawer-handle: "2.25rem"
  drawer-open: "18rem"
  column: "48rem"
  form-column: "42rem"
  wide-column: "64rem"
  wide-header: "69rem"
  project-aside: "17rem"
  project-gap: "2rem"
  measure: "44rem"
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
    typography: "{typography.badge}"
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
  chip:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.step}"
    padding: "0 0.75rem"
    height: "2rem"
  chip-pressed:
    backgroundColor: "{colors.ground}"
  chip-more:
    textColor: "{colors.ink-dim}"
    rounded: "{rounded.step}"
    padding: "0 0.75rem"
    height: "2rem"
  result-row:
    textColor: "{colors.ink}"
    rounded: "{rounded.panel}"
    padding: "0.75rem"
  result-row-hover:
    backgroundColor: "{colors.panel}"
  project-icon:
    backgroundColor: "{colors.panel}"
    rounded: "{rounded.step}"
    size: "48px"
  tag:
    textColor: "{colors.ink-dim}"
    typography: "{typography.tag}"
    rounded: "{rounded.thumb}"
    padding: "0 0.4375rem"
  installed-lamp:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.ink}"
    rounded: "{rounded.step}"
    padding: "0.5rem 0.875rem"
    height: "2.5rem"
  action-plate:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.panel}"
    padding: "1.25rem"
    width: "{spacing.project-aside}"
  gallery-thumb:
    backgroundColor: "{colors.ground}"
    rounded: "{rounded.step}"
    height: "7.5rem"
  lightbox:
    backgroundColor: "{colors.panel}"
    rounded: "{rounded.panel}"
    padding: "1.5rem"
    width: "64rem"
  content-row:
    textColor: "{colors.ink}"
    padding: "0.75rem 0"
  content-icon:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink-dim}"
    rounded: "{rounded.step}"
    size: "2.5rem"
  note-plate:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    rounded: "{rounded.panel}"
    padding: "0.75rem 1rem"
  empty-slot:
    textColor: "{colors.ink-dim}"
    rounded: "{rounded.panel}"
    padding: "1.25rem"
  art-tile:
    backgroundColor: "{colors.ground}"
    rounded: "{rounded.step}"
  art-key:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
    typography: "{typography.control}"
    rounded: "{rounded.step}"
    padding: "0.5rem 0.75rem 0.5rem 0.5rem"
    height: "3rem"
  art-key-selected:
    backgroundColor: "{colors.ground}"
---

# Design System: Perland Launcher

## Overview

**Creative North Star: "The Arcade Cabinet"**

Every instance is an arcade cabinet. Its seeded landscape is a backlit marquee, the rail down the left is a row of cabinets with a coin-slot light beside the one you're standing at, and Play is the one lit button you press. The room is warm laminate black by default (a player may pick one of three other dark rooms, see Themes), never a navy gradient. Controls are physical: they stand on a brown dirt step taken from the PerlandDev land-plot mark, and pressing one drops its face onto that step in two snapped frames. Nothing glows, blurs or eases. Depth is hard offset layers, corners are pixel-notched, and state changes snap.

The density is low and the hierarchy is blunt, because the job is "open, press Play, be in the game" for young, often Spanish-speaking players. Home is almost all marquee art: the instance name is huge at lower left, and Play sits lower right with "Mods & packs" and the overflow menu as quieter raised controls beside it. Every other screen is a plain 48rem column on the ground under a Bungee title, with sticky action rows. The four plot hues are strictly functional: each owns one meaning, and at rest only Play is lit.

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
- **Dirt** (`dirt`): the step under amber controls and Play, and the first layer of the marquee name's and icon plate's shade.
- **Deep Dirt** (`dirt-deep`): the step under every other raised control, dialogs, menus, toasts and the first-run card. It is also the inner line of the marquee's T-molding and the sub-text colour on Play's amber face.

### Named Rules
**The One Lit Button Rule.** At rest, Play is the only lit thing on screen. Amber means "press this to act". Never use it as a decorative accent, a heading colour or a selected-state fill.

**The Lime Lamp Rule.** A chosen option shows a small square lime lamp and sits pressed into its slot. Selection never borrows amber, because amber is reserved for acting.

**The Hue Has One Job Rule.** Lime is running or OK, coral is stop or fault, blue is info or progress. A status never relies on hue alone, so pair it with a label, an icon or a shape (the LED, the warning diamond, the meter blocks). Amber is never a status or a notice: advice such as a server's invite-only line is info, so it is `blue-ink` text led by a pixel lock.

### Themes
A player picks the room in Settings → Appearance → Theme (`theme` in settings). A theme is a different material for the same cabinet: it swaps **only the eight neutrals** (`ground`, `panel`, `raised`, `line`, `ink`, `ink-dim`, `dirt`, `dirt-deep`). The role hues (`amber`, `amber-hi`, `lime`, `coral`, `blue`, `blue-ink`) are identical in every theme, so every rule above holds unchanged. Every theme is dark.

| Theme (en / es) | ground | panel | raised | line | ink | ink-dim | dirt | dirt-deep |
|---|---|---|---|---|---|---|---|---|
| **Laminate** / Laminado (default): warm brown-black | `#181410` | `#221c17` | `#2e261f` | `#463a2f` | `#f6f1e7` | `#c5b9a8` | `#7a4a2a` | `#4e2e19` |
| **Stone** / Piedra: cool deepslate grey | `#141619` | `#1d2024` | `#282c31` | `#3f454d` | `#eef1f4` | `#b6bec8` | `#5b636d` | `#363b42` |
| **Night** / Noche: deep blue night sky, flat (never a gradient) | `#10131f` | `#181c2c` | `#22283b` | `#363f5c` | `#eef0fa` | `#b3bad3` | `#4a4f8a` | `#2b2f57` |
| **Forest** / Bosque: dark moss | `#111712` | `#19211a` | `#232d24` | `#3a4a3b` | `#eef3ea` | `#b5c3b0` | `#4f6b3a` | `#2f4223` |

- **Mechanism:** `@theme` holds Laminate. Each theme is a `:root[data-theme='…']` block in `app.css` that overrides the eight `--color-*` properties; Tailwind's utilities read the variables, so the whole app changes live. `applyAppearance` sets `data-theme` (and `data-font`) on `<html>` whenever the settings change, and once before the first render from a `localStorage` mirror, so a start never flashes the old room.
- **Contrast floor (tested):** in every theme `ink`, `ink-dim`, `blue-ink` and `coral` reach 4.5:1 on `ground`, `panel` and `raised`; `ground` text reaches 4.5:1 on `amber` and `coral` faces; `lime` reaches 3:1 on `ground`. `styles/themes.test.ts` parses `app.css` and fails below this. The tightest pair is `coral` on Stone's `raised` (4.60:1).
- **No literal neutrals in chrome.** Anything the room colours goes through a token, translucent ones through `color-mix` (the dialog backdrop is `ground` darkened by half at 78%).
- **Generated art keeps its own palette.** Marquees, avatars, plot icons and monogram plates are pictures, not chrome, and are drawn from Laminate's ground and dirt in every theme.
- **The picker:** four swatch keys in a row. Each is a raised key whose face is a small sample of that theme's room, painted through `[data-swatch]`: a `ground` tile in a 1px `line` ring holding a `raised` key (with a 2px `ink` label stroke) on its `dirt-deep` step and an amber pixel on a `dirt` step, then the theme's name. The chosen key presses into its slot with the lime lamp before the name. The amber pixel is a sample of the theme, not a lit control.

## Typography

**Display Font:** Bungee (self-hosted via `@fontsource/bungee`, with a system sans fallback)
**Body Font:** Atkinson Hyperlegible Next Variable (self-hosted, with a system sans fallback)
**Label/Mono Font:** Atkinson Hyperlegible Mono (self-hosted, with `ui-monospace` fallback)

**Text font choice:** Settings → Appearance → Text font (`font` in settings) picks the body face: "Easy to read" / "Fácil de leer" (Atkinson Hyperlegible Next, the default) or "This computer's" / "La de tu computadora" (`system-ui, 'Segoe UI', sans-serif`, no download). `:root[data-font='system']` swaps `--font-sans` only. Bungee signage and the Atkinson Mono log never change. Each option's label is set in its own face.

**Character:** Bungee is arcade-marquee signage: blocky, all-caps by design, and loud. Atkinson Hyperlegible is the opposite, a face built for readers with low vision and for readers still learning, here chosen for young and non-native players. Signage names things. Atkinson says everything else.

### Hierarchy
- **Display** (Bungee 400, `clamp(2.5rem, 5.2cqi, 5.5rem)`, 0.95): the instance name on the marquee, with the two-layer marquee shade. It is sized against Home's own width (a container), not the window's, so the wide rail shrinks it honestly. 5.2cqi is 46px at the 960 px minimum with the compact rail, sized so an 8-letter name keeps its full size (see Layout, Home). It wraps only between words, never inside one: a word too wide for its column scales the whole name down until that word fits (`--name-fit`, measured by `useNameFit` whenever the column's width, the name or the fonts change). Only at the 1.125rem floor, the smallest Bungee step (a row name), may a word still break, which takes a single word of about 25 letters or more at the 960 px minimum.
- **Play** (Bungee 400, 2rem, 0.02em tracking): the word on Play, Stop and Stopping. While Play is preparing it switches to the 650-weight body face, left-aligned, so the phase label and meter fit.
- **Headline** (Bungee 400, 2rem, 1): screen titles in the screen frame. First-run step titles run 1.875–2.25rem.
- **Title** (Bungee 400, 1.25rem): small signage such as the About heading and the tabular memory readout. A server's name on the Join server card is the one in-between step, 1.75rem, so it sits under the 2rem screen title.
- **Body** (Atkinson Next 400, 15px, 1.5): all running text. Paragraphs use `text-wrap: pretty` and headings use `balance`.
- **Control** (Atkinson Next 650, 0.9375rem, 1): button and segment labels. Large buttons use 1rem.
- **Label** (Atkinson Next 600, 0.875rem): field labels, the drawer handle, title-bar account name. Hints and errors use the same size at 400.
- **Mono** (Atkinson Mono 400, 13px / 20px): the game log, error detail, Java and data-folder paths. Paths are user-selectable and truncate.
- **Row name** (Bungee 400, 1.125rem, 1.25): a project's name in a Browse result row, the one piece of signage in a row.
- **Tag** (Atkinson Next 600, 0.8125rem, 1.375rem line): category tags on rows and the project page. **Badge** text is 650 at 0.75rem.

### Project descriptions
A project's description is Markdown set in Atkinson, at a 44rem measure, selectable. Paragraphs and blocks are spaced 0.875rem.
- **Headings** go by weight, never Bungee: 700, line-height 1.25, 1.75rem above and 0.5rem below. h2 is 1.375rem, h3 1.125rem, h4 1rem, and h5/h6 0.875rem in `ink-dim`.
- **Lists:** square bullets, 1.5rem indent, markers in `ink-dim` with tabular figures.
- **Code:** inline code is mono 13px in a `ground` well with a 1px `line` ring and 3px notched corners. A `pre` block is the same well at 13px / 20px, 0.75rem × 1rem padding, scrolling sideways.
- **Blockquote:** `ink-dim` on a `panel` block with notched 4px corners.
- **Tables:** tabular figures, a `panel` header row in 600 0.875rem, a 1px `line` rule under each cell, scrolling sideways when wide.
- **Rule:** a 2px pixel rule of 4px `line` blocks with 4px gaps, 1.75rem above and below.
- **Links** are buttons that open in the system browser, set in `blue-ink`, underlined at a 3px offset. Hover turns them `ink` with a 2px underline.
- **Images** are notched at 4px and never wider than the column. An image the launcher won't load shows its alt text in a 0.875rem `ink-dim` slot with a 1px `line` ring, never a broken picture.

### Named Rules
**The Hard Shade Rule.** Signage over art carries a two-layer hard shade, never a blur: `dirt` offset 0.045em, then `ground` offset 0.09em. The version line under the name sits on a translucent laminate plate, so the art never decides its contrast. Once an instance has a minute of play, the same line ends with the time played on this computer ("Minecraft 1.20.1 · Fabric 0.16.14 · 12 h played", "… · 12 h de juego"), in the plate's own voice: no badge, lamp or second plate. Time reads in its two biggest units ("3 h 5 min", "1 year 35 days").

**The Signage Names, Atkinson Speaks Rule.** Bungee appears only on names and titles (the instance, the screen, the welcome, Play, a big readout). Sentences, labels, buttons and errors are always Atkinson.

**The Tabular Rule.** Every live number (files, MB, memory, counts) uses tabular figures so readouts don't jitter while they count.

## Layout

The shell is a fixed desktop grid with a minimum window of 960 × 600 and no phone breakpoints:

- **Title bar:** 2.25rem tall across the top, on `ground`, and draggable everywhere except its buttons. The mark and name sit left. The account (avatar and name), settings, a 1 px divider and the window controls sit right. On first run and the settings-failed screen it is **minimal**: only the window controls, because nothing else is reachable yet.
- **Rail:** down the left, on `ground`, in one of two widths the player picks with the rail key at its top, remembered in settings (`railExpanded`). **Compact** is 4.5rem (72 px): icon cabinets are 3.5rem squares with 0.75rem between them. **Wide** is 15rem (240 px): each cabinet becomes a 3.5rem-tall plate across the rail (0.75rem gutters) with the icon in its 3.5rem slot at left and the name and status word beside it. The rail key sits at the top in both widths, then the cabinets (they scroll), then the Servers and Browse keys and a ghost "+" at the foot. Wide, the keys stretch to the plates' width with their label beside the icon, the icon centred on the cabinet icons' axis. The switch snaps; nothing slides. The list uses roving arrow keys in both widths.
- **Main:** fills the rest. The activity drawer is pinned along its bottom edge as a 2.25rem handle. Opened, the drawer is 18rem tall, and `main` gets 18rem of bottom padding to match so the screen (and Play) stays above it rather than under it.
- **Home:** the marquee fills `main` edge to edge. A bottom grid of `[minmax(0,1fr) auto]` with a 1rem column gap and 2.5rem side gutters holds the name and version plate at lower left. The raised "Mods & packs" button, the overflow menu and Play sit at lower right, aligned to their bottoms, with Play last at the far right. "Mods & packs" stays a large button but with 1rem side padding. When the instance has somewhere to continue, a standard raised **Continue** key ("Continue" in `ink-dim`, the world or server name in `ink`, at most Play's 17.5rem wide, the name truncating) sits 0.75rem above Play and right-aligned to it; it is never lit. With the compact rail at the 960 px minimum this leaves the name's column about 275 px (279 px in Spanish), and the name, sized at 5.2cqi of Home's own width (46px there), fits a word of up to 8 letters of typical width at full size (CREATIVE 247 px, SURVIVAL 258 px, MODPACKS 265 px). A longer word, or 8 very wide letters (WWWWWWWW), shrinks the name until it fits (SUPERVIVENCIA EXTRAORDINARIA sets at 28px there) rather than splitting or overflowing. Because the name follows Home's width rather than the window's, the wide rail shrinks it honestly, and from 55rem of Home width up the column grows faster than the name. Below 55rem of Home width (the wide rail under a window of about 1120 px, so always at the 960 px minimum) the bottom grid drops to one column: the name and plates take their own full-width row, and the controls sit 1.25rem below at the right, Play still last, so the name never squeezes into a column a few letters wide.
- **Other screens:** a screen frame with a Bungee title row (2.5rem gutters, 2.25rem top), then a scrolling column capped at 48rem. Forms inside it run at 42rem, with 2.5rem between sections. Primary actions sit in a **sticky action row** at the bottom of the scroller on `ground`, with a 1 px `line` hairline above, so Save or Create stays reachable in long Spanish layouts. Every screen frame opens with a ghost **back key** (a square-capped chevron, "Back", title "Back (Alt+←)") before the title: it goes to Home, or to the screen the frame came from (an instance's Home, Mods, Browse results). Alt+Left does the same anywhere on the screen, except in a text field or under an open dialog. Escape stays with dialogs, menus and drags.
- **First run:** the "perland" marquee in attract mode fills the window behind a centered 32rem panel card on a deep-dirt step. Blue bars across the top of the card count the steps.
- **Toasts** stack at the bottom-right in a 24rem column. **Dialogs** are `min(30rem, 100vw − 4rem)`, centred with auto margins (the CSS reset zeroes them) and capped at `100dvh − 4rem` tall, scrolling inside, so a long list never pushes the actions off a 600 px window.
- **Accounts, time played:** under the account list, a quiet 0.875rem `ink-dim` line sums every instance's time on this computer ("Time played on this computer: 12 h 30 min"). It is absent under a minute. It is a fact, not a stat: no number styling, card or lamp.
- **Accounts, medals:** under the time played, a **Medals** section: a 700 1.125rem heading with "8 of 11 earned" in `ink-dim` tabular at its right, then one `panel` case holding the medals in two columns (1.25rem rows, 1.5rem between columns). Each medal is the badge, then its 700 name (`ink-dim` while locked), one 0.875rem `ink-dim` state line (the goal while locked, "Gold · 120 h" or "Gold · Earned" once earned), for a ladder "Next: Diamond at 500 h" or "Top tier", and the meter. Only the local time medal shows without Perland's stats; signed in but not linked, the server medals show locked under one `ink-dim` line, "Link your Minecraft account to earn the server medals." Under the case, **Achievements by server**: one flat `panel` row per server with achievements (700 name, `ink-dim` tabular count, a dim chevron), raised on hover, opening the achievements dialog. Nothing in the section is lit: Add offline account keeps Accounts' one primary.
- **Home, pack with an update:** the update plate takes its own full-width row of the bottom grid, above both the name and Play, so it reads on one line at 960 px.
- **Wide screens (Browse):** the screen frame's wide variant widens the column to 64rem for screens with a grid or a side panel; its title row widens to 69rem to stay aligned with it.
- **Browse controls:** one grid row aligned to the bottom edge with a 0.75rem gap. Search takes `minmax(12rem, 1fr)`, and only the selects present get a track (`minmax(9rem, auto)` each), so a scoped Browse with only Sort doesn't leave an empty column. Category keys wrap below with 0.75rem between rows and 0.5rem between keys.
- **Project page:** a `minmax(0, 1fr) 17rem` grid with a 2rem gap, kept down to the 960 px window. The description and gallery read on the left. The right column holds the action plate, then links and license, 1.5rem apart. The action plate is sticky at the top of the scroller. A 6px `ground` collar fills its notched corners, and a 1.5rem strip of `ground` above it covers the gap, so scrolled text never shows over or through the plate.

Rhythm: 0.75rem between sibling controls, 1.25–1.5rem inside cards and dialogs, 2.5rem gutters and section gaps.

## Elevation & Depth

Depth is physical and hard-edged. Nothing in the system uses a blurred shadow. A raised object stands on a solid, zero-blur offset under-layer in dirt brown (the "step"). A recessed object is sunk with a 2 px dark inset top edge plus a 1 px `line` inset ring. Pressing an object removes the step and translates the face down by exactly the step height, so the face lands where the step was.

### Shadow Vocabulary
- **Step** (`box-shadow: 0 4px 0 var(--color-dirt-deep)`): secondary buttons, segments and the selected rail cabinet. Amber buttons use `dirt`.
- **Large step** (`0 6px 0`): large buttons, menus and toasts.
- **Play step** (`0 8px 0 var(--color-dirt)` plus `inset 0 3px 0` lit edge): Play only. When running, the step turns to `dirt-deep`.
- **Card step** (`0 8px 0 var(--color-dirt-deep)`): dialogs and the first-run card, and a carried rail cabinet (with a 1px `ink-dim` seam ring).
- **Recess** (`inset 0 2px 0 rgb(0 0 0 / 0.45), inset 0 0 0 1px var(--color-line)`): inputs, selects, the slider track and a checked segment, and the open slot in the rail while a cabinet is carried.
- **T-molding** (`inset 4px 4px 0 var(--color-ground), inset 6px 6px 0 var(--color-dirt-deep)`): the marquee's frame where it meets the title bar and rail.
- **Hairline** (`0 -1px 0 var(--color-line)`): above sticky action rows.
- **Small step** (`0 3px 0 var(--color-dirt-deep)`): category keys in Browse.
- **Ground collar** (`0 0 0 6px var(--color-ground)`): the sticky action plate, so its notched corners never show scrolled text.
- **Ring** (`inset 0 0 0 1px var(--color-line)`): flat labels that are not pressable, such as tags, inline code, code blocks and unloaded-image slots, and the empty slot on Mods & packs.
- **Seam rule** (`0 1px 0 var(--color-line)`, plus `0 -1px 0` on the first item): between lines of the install manifest.

### Named Rules
**The Dirt Step Rule.** Everything that can be pressed stands on a hard dirt step: `dirt` under amber, `dirt-deep` under everything else. Pressing travels the full step height and removes it. No blur, ever.

**The Spent Step Rule.** A control whose action is running stays pressed down. Play while queued or preparing, a button with `aria-disabled="true"` while pending, and a disabled control all sit on their spent step with no under-layer.

## Shapes

Corners are pixel-notched, taken from the stepped grid of the land-plot mark: `border-radius` plus `corner-shape: notch` cuts a square step out of each corner rather than rounding it. Use 4px (`step`) on controls, inputs, badges and menu items, and 6px (`panel`) on Play, panels, dialogs, menus and rail cabinets. Small inner pieces (meter channel, toggle knob, slider track) use 2–3px, and so do flat ringed labels (tags, inline code, unloaded-image slots) at 3px. Where `corner-shape` is unsupported the same radii fall back to a slight rounding, which is acceptable.

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
- **Dialog:** the backdrop is the theme's `ground` darkened by half at 78% (`rgb(12 9 6 / 0.78)` in Laminate).

### Navigation
- **Rail:** `panel` cabinets with notched 6px corners. Hover raises them to `raised`. The selected cabinet is `raised` on a 4px `dirt-deep` step, with a 4px amber **coin-slot light** to its left. A lime LED square in the lower-right corner marks preparing or playing. A damaged instance is greyed to 40% with a coral warning diamond. Without art, a cabinet shows its generated **monogram**: the first letter or digit of the name's first two words ("AF") in `ink` Bungee with the hard shade, on a 40px laminate plate tinted 28% toward the instance's plot hue, over its plot icon's terraces flattened to an 8–14px base (4px grass in the hue, then dirt). The plate stays dark so it never reads as lit, and the letters hold at least 8:1 on all four tints. A name with no letter or digit keeps the plain plot icon. A pack's own icon replaces the monogram (40px, 3px notched corners, the same notch the plate has), falling back to it if it fails to decode. A server cabinet carries its last-known status as three pixel **signal bars** (2px wide, 4/7/10px tall) in a 14px notched `ground` socket at the **top-right** corner. The **count** of lit bars carries the state, so hue never carries it alone: online 3/3 in `ink`, starting 2/3 in `blue`, sleeping 1/3 in `ink-dim`, offline 0/3 with a 10 × 2px `coral` flatline along the socket's foot, unknown 0/3 (empty bars in a dim seam colour). It never uses lime, the LED's square or the LED's corner, so it can't be read as the Playing lamp. The status word is in the cabinet's accessible name and in its hover title ("Survival · Online · 3/20"). **Round icons** are a setting (`iconShape`, "Instance icons" in Settings → Appearance: Square or Round, a segmented choice). Round clips only the 40px tile (the monogram, the plot icon or a pack's picture) to a pixel-stepped circle on a 16-cell grid: the curve is made of 2.5px squares, the same hard steps as the notched corners, never a smooth arc. In the narrow rail the cabinet follows the tile: its plate, its dirt step (4px selected, 8px carried), the carried cabinet's dim-ink ring and the open socket's seam ring are the same stepped circle at 56px, so no square plate or square shadow shows around a round icon. Wide, a cabinet is a row with its name, so it keeps its notched plate. The cabinet's own marks are not part of the tile, so the coin-slot light, the LED, the signal bars and the warning diamond stay whole and may overlap the circle's empty corners.
- **Wide rail rows:** the same cabinet, stretched: icon, marks and coin-slot light stay exactly where they are in the compact rail, and to the right of the icon slot the name (Atkinson 650, 0.9375rem, `ink`; a navigation label, so not Bungee) sits over the status word (0.8125rem `ink-dim`, tabular): "Playing", "Getting ready", "Needs attention", the server word, or two of them joined by " · ". An idle instance shows the name alone. Both truncate on one line with an ellipsis, and the name's hover title holds it whole. The accessible name is unchanged from the compact rail.
- **Rail key:** a ghost key at the rail's top, 2.25rem tall: no face or step, `ink-dim` at rest like the title-bar icons, and `ink` on the ghost's `raised` hover face. Compact it is an icon (a pixel arrow into a bar, pointing out: "Show names"); wide it reads "Hide names" beside the arrow pointing back. It carries `aria-pressed`. It is chrome, so it never stands on a step and is never lit.
- **Reordering the rail:** the player's own order, in both widths. Dragging a healthy cabinet (pointer events, after 4px of travel; less is still a click that selects) picks it up. **Held**, it is a `raised` face on the 8px card step (`0 8px 0` `dirt-deep`) inside a 1px `ink-dim` seam ring, so it never reads as the selected cabinet at rest (4px step, no ring); its coin-slot light goes out until it lands. It rides the pointer 1:1, held between the first and last healthy places. The cabinets between its place and the landing place **snap** one place over (never slide), so the **open slot** is always where it will land: the slot a pressed key leaves behind, a recessed `ground` socket (the recess inset and `line` ring, 6px notches), never a hole in the rail. The carried cabinet hangs over it. Once the landing place differs from its own, an **amber drop line** marks the open slot: a 4px notched bar on a 2px `dirt` step in the 0.75rem gap above the socket, drawn over the carried cabinet. It is amber because it is an insertion caret, like the text caret and the mods drop cue, and it shows only while carrying. Release snaps the list into its new order; Escape (which goes no further than the rail) and a cancelled pointer put the cabinet back. Alt+↑/↓ moves the focused cabinet one place, keeps focus on it, and a polite live region says "Asalith Fields moved to position 2 of 3". Damaged cabinets are never carried, never move and are never drop targets; they always sit last.
- **Title bar:** flat text buttons 2.25rem tall in `ink-dim`. Hover shows `ink` on `raised`, and the close button's hover is `ground` on coral.
- **Update-ready note:** when a launcher update is downloaded, a quiet line sits left of the account: `ink-dim` 0.875rem words ("Update ready · installs when you close") and a flat 600-weight `ink` "Restart now" title-bar button. It is never lit and never stepped. It stays on one line: the words truncate (full text in the hover title) while Restart now and the account keep their width, and the header carries `min-width: 0` so the shell's grid can't grow past the window. While a game runs, Restart now is disabled (`ink-dim` at 60%) and the words themselves change to the reason ("close your games to restart"), linked by `aria-describedby`, because a disabled button can't take focus to show a tooltip.
- **Overflow menu:** a `raised` popover on a 6px `dirt-deep` step with a `line` ring. Items hover to `panel`, and danger items are coral.
- **Instance actions (#62):** one list (`useInstanceActions`) feeds every place an instance's actions appear, so they can't drift apart. Home's ⋯ key, a right-click anywhere on Home and a right-click on a rail cabinet open the same menu. A right-click (or the keyboard's menu key) opens it **floating**: in the window's own layer at the click, or beside the cabinet, flipping above or sliding left rather than leave the window. Right-click is only a shortcut: Home also shows **Change image** and **Instance settings** as raised secondary keys (2.5rem, 4px step, never lit) at its top right, 1.5rem from the top and on the 2.5rem gutter, away from Play's corner; both stay in the ⋯ list too. A **wide** rail row ends in a quiet 2rem ghost ⋯ key ("More for {name}"), shown while the row is hovered, holds focus, is selected or has its menu open, and always on a touch screen; the row keeps 2.75rem for it so the name truncates before it. Its hover face sinks to `panel`, because the row under it is already `raised`. Only the row Tab enters the rail at gives the key a tab stop. The compact rail has no room for a key, so its cabinets take the right-click and the menu key, and Home's keys stand in. Damaged cabinets have no menu.

### Activity Drawer
A `panel` strip pinned to the bottom of `main`. The 2.25rem handle has a chevron, the label and a lime count badge when something runs. Opened, it is 18rem tall and holds instance tabs (when more than one), a labelled meter with tabular counts, the crash card, and the mono log on a `ground` well with a "follow" button.

### Update Plate
The pack's update status, on the marquee above the name. It is the version plate's translucent laminate with notched 4px corners, led by an 8px square status lamp: **blue** when an update is on offer, **lime** when the pack just updated ("Updated to X · Kept your version of N files"). A failed check shows no lamp and sets its text in `ink-dim`. The change line is `ink-dim` so the title leads. Its action is a compact 2rem secondary button that is **never lit**, because Play owns the lit amber on Home. The plate's foot leaves room for the button's 4px step. Its layout is the shared `.plate-row`, which the server rows below reuse. A **server** instance's plate has the blue lamp and reads "Server updated · updates when you play" with no button, because a server instance always updates at Play.

### Server surfaces
- **Status readout** (`StatusPill`): an 8px square LED and the status word, 600-weight 0.875rem tabular, on a laminate chip with notched 4px corners. Set into a panel it gains the 1px `line` ring of a recessed readout; over marquee art it is a plain plate. LED: **lime** online, **blue** starting (blinking to `line` in two snapped frames, steady under reduced motion), **coral** offline, **hollow `ink-dim`** sleeping (a lamp switched off), **hollow and dimmer** unknown. Never amber. The word always carries the state ("Online · 3/20", "Sleeping · join to wake it").
- **Server panel** on Home: small plates in the name column under the version plate, 0.5rem apart. The status readout and the address plate (an `ink-dim` "Address" label, the address in 13px mono, a compact ghost Copy that swaps to a check and "Copied", its accessible name following the label, with a polite sr-only "Address copied") share a wrapping row, then the invite-only plate and the dead-link plate follow when they apply. At 960px they wrap rather than truncate.
- **Dead-link plate:** quiet, not an error card. The `.plate-row` led by an `ink-dim` pixel broken-link icon, the sentence in `ink`, and a compact raised "Use a new link" that is never lit. Only the sentence is the live region.
- **Invite-only line:** `blue-ink` text led by an 18px pixel lock (info, not amber). On the Join screen it sits on the ground under the card; on Home it rides a laminate plate.
- **Server card** (Join server): a `panel` with an icon window at left: a recessed `ground` well (6px notched, 0.5rem padding) holding the 64px server icon at a whole number of **device** pixels per icon pixel, at least 2 (`64 × k / devicePixelRatio` CSS px with `k = max(1, floor(2 × dpr))`: 128px at 1, 1.5 and 2 dppx, 102.4px at Windows' 125%; recomputed when the screen's scale changes; `image-rendering: pixelated`), or the generated plot icon at the same size when there is none or it fails to decode. Beside it: the 1.75rem Bungee name with the team mark, the MOTD, and the status readout. A `line` hairline separates the facts below: the version line in 600 weight, the pack line (tabular), and "From host" in `ink-dim`. Nothing on the card is lit.
- **Team mark:** a 12px two-cell checker (two diagonal 6px cells) in the team's accent: cyan `#4cc6c9`, blue `#6f9cf0`, violet `#a888f2`, magenta `#e274c8`, anything else `ink-dim`. It is a crest from the land-plot grid, not a lamp, so it is never a solid square and never lit. The accents are the team's content, like § colours.
- **MOTD:** uncoloured runs are `ink-dim`, as in the game. The 16 § colours keep their Minecraft identity but are lifted where needed to reach 4.5:1 on `panel` and on a plate over the brightest lamp: 0 `#9a9a9a`, 1 `#8a8aff`, 2 `#2bb82b`, 3 `#14b4b4`, 4 `#ff6464`, 5 `#d86ad8`, 6 `#ffaa00`, 7 `#c2c2c2`, 8 `#a6a6a6`, 9 `#9c9cff`, a `#55ff55`, b `#55ffff`, c `#ff7c7c`, d `#ff85ff`, e `#ffff55`, f `#ffffff`. They are text, never a fill or a control.
- **Join server screen:** one lit button at a time. Until a server is on screen, "Check link" is the primary; once the card shows, it drops to secondary and "Add server" (or "Open", or "Use this link" when relinking) is the only amber, in the sticky action row with its `line` hairline. That row always holds Cancel, so a failed or unwanted link (an error card, a deep link) always has a way out. The name-mismatch warning is a 600-weight sentence led by the coral warning diamond.
- **Manage server link** (#63): the ⋯ menu's "Manage server link" opens the standard dialog widened to 36rem, titled "Server link". It holds the Join screen's server card set **bare** on the dialog's panel (no panel of its own, so never a card in a card, and the tile's smaller icon), the invite-only line when it applies, then "Invite link" and "Address", each a 600 0.875rem label over a `.well` copy row (mono, selectable, a ghost Copy that swaps to a check and "Copied"). While the invite is read, an `ink-dim` "Checking the server…" line; a dead or unreachable link is the standard error card, and the saved invite and address still show. Only the body scrolls, so the actions stay in view at 960 × 600: a ghost **Unlink** at left, then raised Change link (the relink screen) and Close. Nothing is lit. While the instance runs, Unlink and Change link are disabled and described by "Stop the game to change or remove the link." **Unlink** opens the standard confirm ("Unlink {name} from its server?", the body saying the worlds, mods and settings stay and Play stops updating it, raised Cancel back to the sheet and the coral Unlink, the only filled key), then a notice toast.
- **Instance settings, Server:** a section like Game: the "Invite from host" line, then Copy link and Use a new link as secondary buttons. Save stays the only primary.
- **Servers screen** (Perland's public servers, from the rail's Servers key): the wide screen frame, "Servers" over "Find a Perland server and press Play.", then a stack of sections (an empty slot for the player's own servers, then **Public servers** under a 700-weight 1.125rem heading). The sort and filters are one wrapping row of **filter keys**, the category keys' size: a `raised` 2rem key on a 3px `dirt-deep` step holding the filter's name in 600 `ink-dim`, then its choice in 650 `ink` and a square-capped chevron, with a native select inside (transparent; its focus rings the whole key in amber). A filter set to anything but Any sinks into its `ground` slot with the 8px lime lamp, like a pressed category key; Sort always has a value, so it never sinks. "Has room" is a category key (`aria-pressed`). Region appears only when the list names two regions or more. The cards sit in a `repeat(auto-fill, minmax(17rem, 1fr))` grid, 1rem apart: two a row at 960 px with either rail, three when the column has room.
- **Server tile:** a flat `panel` card, 1.25rem padding. At top, the 64px server icon in the recessed `server-icon` window at the whole number of device pixels per icon pixel nearest 64 CSS px (`cardIconSize`: 64px at 1 and 2 dppx, 51.2 at 1.25, 85.3 at 1.5), or the generated plot icon; beside it the name in Bungee 1.25rem (wraps between words, never cut), the game line in 0.875rem `ink-dim` (the modpack's name and version, else "Vanilla 1.21.1" or "Forge 1.20.1"), and the status readout in the list's own words ("7 / 20 playing", "Sleeping, Play wakes it", "Starting"; not a live region, so a refresh announces nothing). Invite only and, for a server the launcher can't join yet, "Join from Minecraft: <address>" with a ghost Copy follow below. **Play** fills the card's foot as a large key with its word in Bungee 1.125rem: raised (`raised` face, `dirt-deep` step) at rest, and lit (amber face on its `dirt` step, `amber-hi` under the pointer) only while its card is hovered or holds focus, so the grid is never a wall of amber and one card is lit at a time. While it joins it sits pressed on its spent step reading "Joining…" in Atkinson 650. A card that can't be joined has the flat disabled Play. While the list loads, four empty tiles hold the icon window and Play's slot in seam rings, without shimmer. The closed list ("isn't open yet"), an empty list and "No servers match" (with a ghost Clear filters) are the flat `empty-slot`, never an error card.
- **Server news** (#51): one quiet line, never a feed. On a server tile it sits under the head as a flat key (no face or step until hover shows `raised`, it never travels and is never lit): a 1rem `ink-dim` pixel megaphone, the post in 0.875rem `ink` (an event adds " · " and its time in `ink-dim`, tabular; a modpack post reads "Modpack updated: 3 mods added, 1 removed"), cut to one line with an ellipsis (the whole line in its hover title). News the player hasn't opened ends the line with an 8px square `blue` lamp, the update plate's "on offer" lamp, never amber, and the button's name ends ", new". On Home the same line rides a laminate plate under the status and address (at most 36rem). The card asks for news only while it is on screen. No news, news off, offline or rate limited: no line at all, no empty box, no error. **News dialog:** the standard dialog widened to 36rem, titled "News from <server>"; only its list scrolls, so the ghost Close stays in view, and the list takes focus first. Posts are separated by `line` hairlines: a 700 1rem title, then a 0.8125rem `ink-dim` when line (a pixel calendar and the event time, or "Posted Oct 16") with a blue-lamp "New" in `blue-ink` for an unseen post, the picture at its own size (shrunk to fit, never blown up, 4px notches), and the body as plain text in `ink` with its line breaks (`pre-line`, 65ch). A modpack post lists its parts as a two-column table (600 `ink-dim` label, up to 10 names, then "and N more"). Show more is a raised secondary; a failed page says so inline with Try again. Nothing in it is lit.
- **Create a server for friends** (#49): in Your servers, the free server's place is the flat `empty-slot` holding one line ("Your own free server, for up to 10 players.") and a raised secondary "Create a server for friends" key, never lit, so the cards' Play keeps the only amber. When creating isn't possible the same slot holds the reason as one `ink-dim` line, with no button. The **create dialog** is the standard dialog through three steps, one heading per step, never a stepper: **pick** (the limits as one 0.875rem paragraph built from the Engine's numbers, then "What to play" as two segments, Vanilla and A modpack, the modpack a select of the player's Modrinth instances or "Install a modpack from Browse first." with a `blue-ink` link, then the name field with its hint; a refusal is one `coral` line under the form, a bad name the field's own error), **creating** (a status sentence and Close, nothing animated) and **ready** (the invite link, or the address without one, in a `.well` row: 0.8125rem mono `ink`, selectable, wrapping anywhere, and a compact ghost Copy at its end that swaps to a check and "Copied"). Create and Play now are each their step's one lit button. Once the server exists, its **owner card** is first in Your servers: the server tile tagged Owner, with the warning lines under its head ("Deleted on 14 Nov unless someone plays.", "It's out of space and stays off.") as 0.875rem 600-weight sentences led by a 1rem coral warning diamond, never a card of its own; while it is being created or deleted the readout reads "Being created…" / "Being deleted…" on the blue starting LED and Play is the flat disabled key. Its ⋯ menu holds the coral **Delete server**, which opens the standard confirm ("Delete {name}?", the consequence as the body, raised Cancel and the coral Delete server, the only filled key).
- **Manage in Perland:** while a Perland panel is configured, every Your servers card the player owns (the free server's owner card and each Owner-tagged card) has **Manage in Perland** (external-link icon) first in its ⋯ menu, which opens the panel's page for that server in the system browser. It is a plain menu item, never a button on the tile, so Play keeps the card's only lit key; an Owner card that has nothing else gets the ⋯ key for it. Without a panel the item is absent, never disabled. Invited and followed cards never show it.
- **Server stats** (#52): the linked player's numbers on a server, a fact and never a stat. On a server tile, under the news line: one 0.875rem `ink-dim` tabular line in the game line's voice, "12 achievements · 1 day 6 h played" (the count only above zero, the time only from a minute), with "Your stats on this server" before it for screen readers. On Home, a `stats-plate` under the news line: the same laminate plate with a 1rem `ink-dim` pixel trophy and "7 achievements" in 0.875rem `ink`, only above zero. Home never shows the server's playtime, since its version line holds the time played on this computer. No lamp, amber, lime or badge anywhere; signed out, not linked or with the add-on off, nothing shows. When the server sent its achievement IDs, the line and the plate are flat keys (no face or step until hover shows `raised`, never travelling, never lit, like the news line) that open the **achievements dialog**: the standard dialog widened to 36rem, "Achievements on <server>", the count and "The server keeps your achievements." in `ink-dim`, then only its list scrolls. The list is one section per game tab (Minecraft, Nether, The End, Adventure, Husbandry) that has anything earned, then one per mod by its ID namespace, then Other, separated by `line` hairlines: a 700 heading with "7 of 16" in `ink-dim` tabular at right (a mod shows its count only), then the names in two 0.875rem columns, earned first in `ink` led by a 1rem `lime` check (OK), then the tab's vanilla advancements not yet earned in `ink-dim` led by a dim pixel lock, each with "Earned:" or "Not yet:" for screen readers. Vanilla names are the launcher's own en/es table; a mod's are made from its ID ("Andesite alloy"), said once under the list.
- **Medals** (#32): pictures, not chrome. A medal is a 48px pixel badge on a 16-cell grid: a 12-cell octagon disc hung from a two-strap `dirt` ribbon, its 1-cell rim and face in the tier's metal (copper `#7d3f1d`/`#c77a48`, iron `#5f646c`/`#b4b9c0`, gold `#8a5f0c`/`#e0aa35`, diamond `#1d6f6a`/`#4cc3b8`, each with a 3-cell lit corner), and a pixelarticons emblem in laminate black (`#181410`, at least 5:1 on every face) at the disc's centre. The metals are an art palette in every theme and never stand in for the role hues; the tier is always also said in words. A locked medal is the room's `ground` sunk in a `line` rim and ribbon with an `ink-dim` emblem, an empty slot to fill. Ladder progress is the blue block meter at 10 blocks, 0.5rem tall, at most 12rem, labelled "12 h of 4 days 4 h" for assistive tech only.

### Dialogs for the pack flow
- **Update ready:** one lit primary, "Update and play". The change line and kept line sit above a "Show what changed" disclosure whose names list is a well.
- **Download these manually:** each missing file is a well row. Continue is flat and disabled until every file is found, then lit as the only primary.

### Settings: launcher updates and diagnostics
Settings has no lit button: it saves as you go, so Check now, Restart now, Export diagnostics and About's links are all raised secondaries, and none compete.
- **Sections:** a 700-weight 1.125rem heading over its content, 1rem apart, like Instance settings' Game.
- **Appearance:** a section after Minimize holding Theme (the swatch keys, see Colors → Themes), Text font (two segments, each set in its own face) and Instance icons, 1.5rem apart, each under a 600 0.875rem legend. A choice applies to the whole app at once and saves like every other setting; a failed save puts the stored look back.
- **Channel:** Stable and Beta are the standard pressed segments with the lime lamp, under a 600 0.875rem legend, with the channel's hint in `ink-dim` below (linked by `aria-describedby`). On this screen lime marks only the chosen channel.
- **Status readout:** a note plate holding the state in words, which is the live region. The blue lamp lights while an update is on its way (checking, downloading, ready); idle, up to date and a failed check have no lamp. A failure shows the error's own message with its fix beneath in `ink-dim`. While downloading, a labelled 20-block blue meter and a tabular 600-weight percent sit under the words, outside the live region, so a screen reader hears each state once instead of every percent.
- **Actions:** Restart now (when ready) then Check now, a raised row under the plate. A disabled Restart now is described by the visible "Close your games first." beside it.
- **Diagnostics:** the heading, one `ink-dim` sentence on what the zip holds and leaves out, and a raised Export diagnostics led by the download glyph.

### Share an instance
The ⋯ menu's "Share…" (never on a server instance, whose invite link is how it is shared; disabled while the game runs) opens the standard dialog titled "Share {name}". One `ink` sentence says what the file is for, then "What goes in" (a 600 0.875rem `ink` heading) over the choices as file rows split by `line` seams, 0.5rem tall padding: a 600 `ink` name over one tabular `ink-dim` line ("14 mods", "Game and mod settings", "2 worlds, 340 MB"), and the standard toggle at right named by the row's name. Only rows with something in them show; Worlds starts off. The Mods row carries a third line, the mods sentence ("12 mods download from Modrinth. 2 aren't on Modrinth, so they go inside the file.") in 0.875rem `ink`, in a polite live region: no lamp, badge or coral, because a mod inside the file is a fact, not a fault. While the lookup runs, one status line ("Looking up your mods…"); a failed lookup is the standard error card with a raised Try again, and no Save. Only the body scrolls, so the action row stays in view at 960 × 600: the tabular size at left ("About 48 MB.", 0.875rem `ink`), then a ghost Cancel and the lit **Save file…**, the dialog's one amber key. With every choice off, Save stays pressed-disabled and the size gives way to a 600-weight "Turn on at least one thing to share." (or "Stop the game to share it." while the game runs, or "This is too big to share as one file. Leave out worlds." past the 8 GiB a pack can carry). Cancel and Esc wait while the file is written. A saved file closes the dialog with a notice toast carrying Show in folder.

### Change image
The ⋯ menu's "Change image…" opens a dialog (card step, widened to 46rem) that picks an instance's two pictures apart: the **icon** (the rail and Home's icon plate) and the **background** (the marquee). An upload for one never fills the other. The item is disabled while the instance runs or prepares, because the core refuses the change then.
- **Lead:** one `ink-dim` sentence saying what each does ("The icon marks this instance; the background fills the screen behind its name.").
- **Two groups side by side:** Icon in a 13rem column at left, Background in the rest, 1.5rem apart, so both fit at 960 × 600 without scrolling while a landscape is the background. With a picture background, Position adds a row under Background and the dialog scrolls inside itself at 960 × 600, with Done at the end of the scroll. Each is a radio group named by a 600-weight 0.875rem `ink` legend (a heading), one tab stop: the arrow keys, Home and End move between choices, and Space or Enter picks. Moving never picks, because every pick saves and an Upload key opens the file picker.
- **Icon:** stacked keys, one per row. "Initials" holds the instance's generated monogram, filling its slot; "Pack icon" (packs only) and "Upload your own…" ("Sube el tuyo…", for *el ícono*) are the keys below.
- **Landscapes:** six small marquees, three to a row 0.75rem apart, from `presetSeeds` (the instance's art seed with any `~N` stripped, then `~2` to `~6`), so picking one never changes the six on offer. The first is the instance's own scene, drawn exactly as the marquee always drew it. The five `~N` seeds each pin a variant so the six read as six places at tile size: the bloom leans twice as far (32%) into its plot hue, `~2` to `~4` walk through the three other plot hues, and the rims keep that hue but for one contrasting band. Each also sets the sun's side (right or left) and height (high, mid, or low and setting behind the hills) and the ridge (plains, hills or peaks), so any two of the six differ in at least two of four things: the bloom's colour, the sun's side, the sun's height and the ridge (and at least four of the six blooms differ). The rail's plot icon takes the same hue. Only the picker makes `~N` seeds, so an instance id or a pinned seed never changes how it looks. Each is a pressable 16:9 tile on a `ground` fill with notched 4px corners, a 1px `line` ring drawn over the art and a 4px `dirt-deep` step. Tiles are thumbnails, not cabinet fronts: no T-molding and no attract drift. Hover brightens the art one notch (`brightness(1.18)`). Each is named "Landscape 3" for screen readers.
- **Pack art and Upload** (Background): raised keys in a two-column row under the landscapes (Pack art only on a pack instance): a 2rem recessed `ground` icon slot with 3px notches, then the 650-weight label. The slot holds the package or upload glyph in `ink-dim`, or the picture itself while that choice is in use (nearest-neighbour only at 2x or more). Each Upload (Icon's and Background's) says what suits it before the pick, as a dim second line under its label (Atkinson 400 0.8125rem `ink-dim`), which is the key's description, not part of its name: the limits, then a size hint for that slot. Background: "PNG, JPEG or WebP, up to 5 MB. Best at 1920 × 1080 or larger." / "PNG, JPEG o WebP, hasta 5 MB. Ideal: 1920 × 1080 o más." Icon: "PNG, JPEG or WebP, up to 5 MB. Square works best." / "PNG, JPEG o WebP, hasta 5 MB. Ideal: cuadrada." Its words are tied with no-break spaces into phrases (the kinds, the limit, each hint sentence), so a line only ever breaks between them, never mid-list or inside "1920 × 1080".
- **The current choice** is pressed into its slot with the 8px lime lamp: in a 3px `ground` socket at a tile's top-left, and on a key after the icon slot, before the label, leading like every other lamp. For the background it is an upload; else the pack's art while it is on auto; else the landscape whose seed matches. For the icon it follows `icon ?? (art is custom ? custom : auto)`: custom is Upload (a round-1 upload stays the icon), auto is Pack icon on a pack and Initials elsewhere, generated is Initials. A current Pack or Upload key shows its picture in the slot: the icon in Icon, the banner in Background. A choice applies at once and the lamp moves to it; the rail and the marquee behind the backdrop change with it. While either group saves, every choice in **both** is `aria-disabled` with a progress cursor, since two saves at once would race on the instance's file.
- **Position** (Background, while a picture is the background: an upload, or the pack's art): which part of the picture stays in view when the marquee crops it. It sits under the Background keys, named by the same 600-weight `ink` legend, as a labelled group described by its one dim line ("Drag to choose which part of the picture stays in view." / "Arrastra para elegir qué parte de la imagen se ve."). A 12rem recessed well (`ground`, 1px `line` ring, notched 4px corners, 0.5rem padding) holds the whole picture letterboxed in a 16:9 stage, so the player sees all of it, not the crop. On it, two 1px ink hairlines (each over a 1px hard dark edge, so they read on light and dark art) cross at the chosen point, and the crossing is the 8px lime lamp in its 3px `ground` socket: lime marks what is in use, as on the tiles. A press or a drag anywhere on the well moves it (a crosshair cursor; past an edge it stays on the edge) and saves when the pointer lets go (a pointer the OS cancels keeps the last point shown). For the keyboard and screen readers the well itself is hidden; two visually hidden range inputs ("Left to right" / "De izquierda a derecha", "Top to bottom" / "De arriba abajo", 0–100 in steps of 5, read as a percent) carry it, and the arrow keys on either move the point the way they point by 5, saving 300 ms after the last press. While one has keyboard focus the well takes the 2px amber focus ring. Beside the well, the line and a ghost "Reset" / "Restablecer" that goes back to the default (the bottom middle, which is what the marquee showed before a position existed, not the middle); it is dimmed while the default is in use. The marquee behind the backdrop follows every move at once (`object-position`), before the save. A new background (any pick) starts from the default again; a generated landscape has no Position, because it is drawn to fit.
- **Nothing is lit.** The only action is a secondary "Done": choices apply as they are made, so there is nothing to confirm, and Play keeps the room's one amber. A refused picture shows as the standard error card under the keys.

### Well
A recessed row inside a panel or dialog: `ground` fill, 1px `line` inset ring, notched 4px corners. Used for the names list and the manual-download rows, which would otherwise vanish into a `panel` host.

### Pack screens
- **New instance** offers four source modes as the standard pressed segments with the lime lamp: version, pack link, .mrpack file, and "From another launcher". The link hint is the field's own hint (linked by `aria-describedby`); "No file chosen yet" is a body sentence in `ink-dim`, and only a chosen file name is mono. **From another launcher** starts with a raised "Choose folder…" (a directory picker) over an `ink-dim` body sentence telling the player where to find the folder and to close the game there first. While the picked folder is read, the button is disabled and a small `ink-dim` status line ("Reading the folder…" / "Leyendo la carpeta…", `role="status"`) stands where the folder's name will go. Once a folder is read, its name sits beside the button in mono, the sentence under it becomes the size line ("We'll copy 1.2 GB: …", real MB/GB through the shared byte formatter), and the name and `VersionFields` below are prefilled from what was detected (a part that wasn't detected gets the usual default). The sticky action row keeps raised Cancel and the one lit Import. While it copies, the fields are disabled, Import sits pressed on its spent step ("Importing…", `aria-disabled`), and the row's free width on the left holds a tabular `ink-dim` readout ("Copying… 617 MB of 1.2 GB") over the 20-block meter; Cancel then cancels the copy, and a cancelled import returns to the form with no error card. Leaving the form doesn't stop the copy: coming back to From another launcher while it runs shows the same copying state (the folder's name, the size line, the disabled name, the readout and meter, Cancel and the spent Import) instead of the pick step. If it finishes while the player is elsewhere, a notice toast says "{name} is ready" / "{name} está lista"; a failure meanwhile is the usual error toast, and a cancel is never toasted.
- **Mods & packs** on a pack instance splits each tab into labelled sections: "From the pack" (on Mods, optional mods as toggles) and the player's own files ("Your additions" on Mods). See Mods & packs below.
- **Instance settings, Updates** is a segment group; the pack's game line and the "pack picks" hint sit tight (0.25rem) under the Game heading.
- **Instance settings, Java** (#53): two segments, "Automatic (recommended)" and "A Java installation I choose". Automatic says in one 0.875rem `ink-dim` line which Java it uses ("Java 21 · Automatic (Mojang's official Java, managed by Perland)"). Choosing lists the Javas found on this computer as a radio group of full-width segment rows (`.seg-row`): vendor and version in 650 `ink`, then " · Java 17" and a note in `ink-dim`: " · Recommended alternative" on an Eclipse Temurin (Adoptium) that fits, else " · Right for this version" on any Java that fits, with the path under it in `ink-dim` mono; the chosen row presses in with the lime lamp on its first line. Matches come first, Temurin first among equals. A raised secondary "Browse…" follows. When no Temurin fits, one `ink-dim` line suggests Temurin of the needed version from Adoptium, with a raised secondary "Get Temurin 17" (external-link icon) that opens adoptium.net. Automatic stays "(recommended)" and never mentions Adoptium: Temurin is the advanced player's alternative, not the default. A Java that doesn't match the version gets the 600-weight sentence led by the coral warning diamond and a "Use this Java anyway" switch; Save waits for it. Nothing here is lit.

### Browse
Browse lists projects as rows on the ground, never cards. Filters are small keys that sink and light the lime lamp. On a project page the action plate holds the view's one lit button.
- **Kind tabs** are the standard segments, pressed with the lime lamp.
- **Search field:** the standard recessed input, led by the icon set's own search glyph (pixelarticons, the same paths as the rail's Browse key) at 1.25rem in `ink-dim`, with its clear button drawn as the matching close glyph. Text starts 2.625rem in.
- **Scope plate:** when Browse adds to one instance, a `panel` plate with notched 6px corners leads with an 8px square **blue** info lamp and a 600-weight line, with a ghost "Browse everything" at its right.
- **Category keys:** a smaller segment. 2rem tall, 0.75rem padding, 650 at 0.875rem, a `raised` face on a 3px `dirt-deep` step, pressing 3px in `steps(2, end)`. Pressed, a key sinks 3px into a recessed `ground` slot with the 8px lime lamp. "More" is a ghost key in `ink-dim`: no face or step until hover shows `raised`.
- **Result rows:** rows on the ground with notched 6px corners and 0.75rem padding, split by 1px `line` hairlines inset 0.75rem from each side. Under the pointer or keyboard focus a row lifts to `panel` and its icon slot to `raised`. Each row reads in three lines: the Bungee row name and "by author" (0.875rem `ink-dim`) on one baseline, a two-line clamped summary in `ink-dim`, then tabular counts in 0.875rem `ink-dim` with the tags. The project icon (48px in rows, 64px on the project page) sits in a notched 4px `panel` slot so transparent art never floats, and falls back to the generated plot icon.
- **Tags:** a label, not a key. Flat, with a 1px `line` ring, 3px notched corners and `ink-dim` text. Never stepped.
- **Installed lamp:** the slot a pressed key leaves behind. A recessed `ground` slot, 2.5rem tall, with the 8px lime lamp and 650-weight text. It replaces a row's Install when the project is already in the instance, and reads "Already in …" on the action plate.
- **Row Install** is a standard secondary, never lit, because a list of lit buttons would make many.
- **End of results:** a 1px `line` rule across the column with the words set into it, 0.875rem `ink-dim`.
- **Action plate:** a `panel` with 1.25rem padding holding the version and target selects, then the page's one lit primary, large: Install, or Create instance. While installing, the button stays pressed on its spent step and a labelled blue meter appears beneath it.
- **Server needs it too:** an info line led by the 8px blue lamp, in 600 0.875rem `blue-ink`.
- **Gallery:** a sideways-scrolling strip of thumbnails 7.5rem tall, 0.75rem apart. Each is a pressable frame on a `ground` fill with notched 4px corners and a 4px `dirt-deep` step, with a zoom-in cursor. It opens a lightbox dialog up to 64rem wide (`min(64rem, 100vw − 4rem)`), with the image capped at `100dvh − 15rem` tall and notched. An image still loading holds a 16:9 `ground` slot.
- **Install confirm:** a dialog whose additions read as a manifest, one line per item between 1px `line` seam rules, with the tabular total below. The dialog's primary is its only lit button.
- **Rail Browse key:** a large raised key on the rail, never lit. While global Browse is open it sits pressed in a recessed `ground` slot. In the wide rail it stretches with the plates and reads "Browse" beside its glyph. The **Servers** key above it (a pixel server-rack glyph) is the same key and sits pressed while the Servers screen is open.

### Mods & packs
An instance's own files, listed as rows on the ground under the screen title "Mods & packs". It adapts Browse's rows, but a row is not a link, so it never lifts.
- **Tabs:** Mods · Resource packs · Shaders, as the standard segments 0.5rem apart, pressed with the lime lamp, the same as Browse's kind tabs. Each tab is its own route. The column is 48rem with a 1rem gap.
- **Content rows:** 0.75rem vertical padding and no side padding, with a 1rem gap. A full-width 1px `line` seam (`border-top`) splits each row from the one above. A row never lifts, because only its own controls press. Each row reads as the 600-weight name (truncating, and `ink-dim` when the mod is off) over its tabular size in 0.875rem `ink-dim`, then its controls: a toggle and a ghost icon Remove.
- **Icon slot:** a 2.5rem `panel` slot with notched 4px corners, holding the file glyph at 1.5rem in `ink-dim`. A mod row (the player's jars and a pack's own) shows the mod's own icon instead when its jar has one (else its Modrinth project's icon, when Modrinth knows the jar): the PNG fills the slot (contained, clipped to the notches, on the `panel` so transparent art never floats), with hard pixels only at 2x or more (the pack-art rule). An off mod's icon greys to 50% with its `ink-dim` name. The icon is asked for only once its row is on screen; until it arrives, or when there is none or it fails to decode, the glyph stays.
- **Sections:** a heading (Atkinson 700, 1.125rem) over its rows, 0.25rem apart. A section sits 0.5rem below what precedes it, and 1.5rem after another section, so 2.5rem with the column's 1rem gap.
- **Note plate:** a `panel` plate with notched 6px corners and 0.75rem × 1rem padding, led by the 8px square blue lamp centred on the first text line (0.4375rem down). It carries the vanilla notes, the running notes and the Mods tab's busy and skipped-files notice. With `data-lamp="none"` it drops the lamp, for a readout with nothing on its way (Settings' update status).
- **Empty slot:** a flat 1px `line` ring with notched 6px corners, 1.25rem padding and `ink-dim` text, never stepped. On Resource packs and Shaders it shows only when the folder holds nothing at all, never under a pack's own files, and never on a vanilla instance's Shaders tab, where the loader note already explains and there is no Find.
- **Labels:** Folder and Pack (a pack's required mod) use the flat `.tag`.
- **Turn-it-on hint:** an `.info-line` under the tabs on Resource packs and Shaders. It is hidden on a vanilla instance's Shaders tab, where the vanilla note takes its place.
- **Optional mods:** a pack's optional-mod toggle keeps a 2.25rem spacer where Remove would be, so every toggle on the tab shares one column.
- **One lit button:** "Add mods" in the Mods tab's action row. Find and Open folder stay secondary. The pack tabs have no lit button, and the remove confirm lights only its coral danger button.

**The Rows, Not Cards Rule.** Browse results sit on the ground as rows split by seam hairlines and lift to `panel` only under the pointer or keyboard focus. Never box a result in a card, and never step a tag.

### Marquee (signature)
The instance's backlit marquee, generated from a seed (FNV-1a hash, mulberry32 PRNG) so an instance looks the same on every computer. The seed is the instance's art seed (a picked landscape, or its id from before a rename), else its id, and the rail's generated icon grows from the same seed. The recipe:
1. **Sky:** `ground` with 4% of the lamp mixed in. It is a warm black, never navy.
2. **Lamp bloom:** a radial bloom of the warm tube light (`#ffd98a`, tinted 15% toward one plot hue) behind the horizon, fading from 0.7 to 0.26 to 0 opacity.
3. **Sun:** a pixel-stepped disc of radius 4 cells in pale lamp cream, at upper right. The one exception is a picked preset: a `~2` to `~6` seed sets the sun's side (left or right) and height (high, mid, or low and setting behind the hills); see Change image. It is kept whole: when the panel is cropped short (drawer open), it moves down so it is never closer to the visible top than its radius plus one cell, and it sets behind the hills instead of being sliced.
4. **Hills:** four mirrored, seamlessly tiling silhouette bands with stepped values. The far bands catch the most lamp light (0.5, 0.3, 0.14). Each carries a one-cell rim in a plot hue. Drops deeper than one cell span at least two columns, so no single-cell "drips".
5. **Near floor:** the nearest band is nearly `ground` (8% dirt) with a muted rim, a dark floor the name sits on.
6. **Grille and vignette:** a 3-unit horizontal grille at 10% black, then a radial vignette to 55% black at the edges.
7. **Bezel:** the T-molding inset (`ground` 4px, then `dirt-deep` 6px) along the top and left where the marquee meets the title bar and rail.
- **Attract mode:** at rest the hill layers drift in parallax (180 s, 140 s, 100 s, 60 s per loop, nearer layers faster). The drift pauses when not idle and is neutralised under `prefers-reduced-motion`.
- **Pack banner:** a pack's own art replaces steps 1 to 5, anchored to the floor (bottom middle) with `object-fit: cover`, under the same grille and vignette. The player's uploaded background takes the same place. If the player chose a Position in Change image, that point stays in view instead (an inline `object-position`); the stylesheet's bottom middle is the default. A four-band **stepped scrim** keeps the name and plates legible: laminate bands in whole 8-unit cells, `crispEdges`, at 0.6, 0.42, 0.26 and 0.12 opacity from the floor up. No blur and no smooth gradient. If the banner fails to decode the panel falls back to the generated scene.
- **Pack art scaling:** `image-rendering: pixelated` applies only where the art is blown up at least 2x (`data-upscale`: the larger cover ratio of box to natural size). Real art that shrinks or grows slightly scales smoothly, because nearest-neighbour on a small upscale gives uneven pixels. The same rule governs rail icons and the icon plate.
- **Icon plate:** when the instance has a real icon (a pack's, or the player's upload), a 4.5rem framed plate sits 1.25rem left of the name, bottoms aligned: the icon at 60px inside a 0.375rem `ground` frame with a 1px `line` ring and notched 6px corners. It is signage, not a key, so it wears the name's two-layer hard shade (`dirt` 4px, then `ground` 4px) rather than a control's step, and never lights. Generated icons get no plate: the monogram would only repeat the name. The icon is never the background. A plate takes 5.75rem from the name's column, so with one the bottom grid drops to one column under 64rem of Home width instead of 55rem (the compact rail at the 960 px minimum included), and an 8-letter name still keeps its full size.
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
- **Do** show a server's state as a square LED plus its word, keep the rail's server mark in signal bars at the top-right with the bar count carrying the state, and show a 64px server icon only at a whole number of device pixels per icon pixel.
- **Do** pad `main` by the drawer's open height so Play is never covered, and keep primary form actions in a sticky row.
- **Do** let attract-mode parallax stop under reduced motion. State changes snap.
- **Do** list Browse results as rows on the ground split by inset seam hairlines, and make filters small 3px-step keys that sink into a slot with the lime lamp.
- **Do** keep the project page's one lit button in its sticky action plate. Row Install and the rail Browse key stay raised, never lit.
- **Do** list an instance's files as rows on the ground split by full-width seams, with the glyph in a notched `panel` slot. Don't box them in panel cards, and don't lift a row that isn't a link.

### Don't:
- **Don't** use blurred box-shadows, glows or soft elevation. Depth is hard offset layers only.
- **Don't** fill a selection, a heading or decoration with amber. A lit amber selection makes two lit buttons.
- **Don't** use a navy gradient, rounded cards or a glowing blue button, which are the launcher category default this system replaces.
- **Don't** blur or smooth-gradient a scrim over art. Use stepped, cell-aligned bands.
- **Don't** ease state changes or animate progress smoothly. Use `steps()` or snap.
- **Don't** set sentences, labels or errors in Bungee. Don't set the log or paths in anything but Atkinson Hyperlegible Mono.
- **Don't** use any hue as the only carrier of status. Pair it with a label, an icon or a shape.
- **Don't** use amber for a status or a notice (sleeping, invite only). Amber only ever means act.
- **Don't** reuse the PerlandDev Panel's cyan chamfered mark or HUD language, Modrinth branding or Mojang artwork. Marquees, plot icons and avatars are generated.
- **Don't** show Modrinth branding in Browse. Show only a project's own title, summary, icon and gallery.
- **Don't** box Browse results in cards or give a tag a step. Tags are flat ringed labels.
