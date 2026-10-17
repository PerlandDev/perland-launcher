---
version: 1
slug: "ui-src-app-tsx"
primary_target: "ui/src/App.tsx"
related_targets: ["ui/src"]
---

# Launcher app shell (all screens)

- **Scope:** the whole desktop app in `ui/`: title bar, instance rail, home/instance, activity drawer, new instance, instance settings, mods, accounts, settings, first run, Browse (search, project page, install confirm).
- **Mode:** Operate. The player's task is to get into the game. Expression never hides Play, state or a familiar affordance.
- **Audience and job:** Perland/Asalith players, often 12–25, casual, many Spanish-speaking. Their job is to open the launcher, press Play and be in the game. Secondary jobs: create an instance, add mods, switch accounts, read an error and fix it.
- **Constraints:** the pinned layout from the owner's brief (full-bleed art, big Play, left instance rail, custom title bar, dark only). Generated art and avatars only: no Mojang assets, no Modrinth branding. Unbuilt features are "coming soon" or absent. Every string exists in en and es; es runs about 30% longer. Minimum window 960 × 600.
- **Memorable moment:** pressing Play. The amber button physically drops onto its brown step, the segmented meter fills block by block through plain-language phases, and the marquee dims as the game takes over.
- **Server surfaces (join by link), decided 2026-10-12:**
  - *Invite only is not amber.* The spec asked for an "amber" line, but amber only means act here. Invite only is advice, so it is info: `blue-ink` text led by a pixel lock, on a laminate plate over marquee art. Amber stays on the one lit button.
  - *The rail's server mark is not a lamp.* It is three pixel signal bars in a ground socket at the cabinet's top-right, never lime. The bar count carries the state (online 3 ink, starting 2 blue, sleeping 1 dim, offline 0 plus a coral flatline, unknown 0), and the hover title adds the word. The lime Playing LED stays a solid square at bottom-right, so shape, corner and hue all differ.
  - *One lit button per view.* Home: Play only; the server plates' actions (Copy, Use a new link) are ghost or raised. Join server: "Check link" is lit only until a server card shows, then the action row's Add server / Open / Use this link is the only amber. Instance settings: Save.
  - *Status in words.* The LED (lime OK, blue starting, coral offline, hollow sleeping/unknown) always rides its word.
- **Servers screen (public server list, #50), decided 2026-10-17:**
  - *Cards, not rows.* The spec asks for a card grid, so this is the one place servers are boxed: flat `panel` tiles, nothing lit at rest.
  - *Play lights per card.* Each card's Play is raised at rest and turns amber only while its card is hovered or focused; the grid never shows two lit buttons.
  - *Filters are keys, not a form.* Sort and the filters are small select keys and a pressed-key toggle in one wrapping row, so at 960 × 600 the first row of cards stays in view; a set filter sinks with the lime lamp.
  - *Closed is calm.* An Engine with the list off reads "isn't open yet" in the flat empty slot: no coral, no alert, no Retry.
- **Server news (#51), decided 2026-10-17:**
  - *A line, not a feed.* One quiet flat key under the card's head and under Home's status plates; Play stays the only lit button and the card's Play still lights on card hover only.
  - *Unseen is info.* The new mark is the blue square lamp of the update plate, never amber, with "New" in the accessible name and on the post in the dialog.
  - *Nothing when there's nothing.* News off, no posts, offline or rate limited render no line, no empty box and no error; only the dialog's Show more may say it failed.
- **Unresolved:** final faces (candidates: Bungee-family signage display for marquee names; Atkinson Hyperlegible Next for body, chosen for legibility with young and non-native readers); exact neutrals; the vector redraw of the land-plot mark.

## Direction contract

THESIS: Every instance is an arcade cabinet. Its generated landscape is a backlit marquee, and Play is the one lit button you press. The app refuses the category default of a navy gradient, rounded cards and a glowing blue button.

OWN-WORLD: A warm laminate-black ground. The four land-plot hues have roles: amber means act (Play), lime means running or OK, coral means stop or fault, blue means info or progress. Dirt brown is the step under every raised control: hard offset under-layers, never blurred shadows. Corners are pixel-stepped from the mark. Five control states: raised, pressed, lit, recessed, fault. At rest only Play is lit.

STORY: The player sees their instance lit up, understands it's ready, presses Play, watches honest progress with real MB figures, and is in the game. Errors tell them what to do in their own language.

FIRST VIEWPORT: The custom title bar is a thin strip on top. A rail of authored plot icons runs down the left in the player's chosen width: compact 72 px of icons, or wide 240 px where each cabinet is a plate with its name and status word; the rail key at its top switches them and settings remember it. The marquee art fills the rest. The instance name is huge at lower left, with version and loader beneath it (when Home is under 55rem wide, as with the wide rail at 960 px, the name takes its own row and the controls sit beneath it at the right). The amber Play button, about 280 × 88, sits on its step at lower right, with Mods and ⋯ beside it as secondary controls. The activity drawer handle sits on the bottom edge.

SIGNATURE / MOTION: Play's press travel lands in 2 steps of 90 ms. The segmented meter fills by blocks with tabular readouts. In attract mode the marquee drifts in slow parallax and stops under reduced motion. State changes snap; they never ease.

FORM: Arcade Cabinet, position 3 of the ordered grounded list; seed key c94af540.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
