---
version: 1
slug: "ui-src-app-tsx"
primary_target: "ui/src/App.tsx"
related_targets: ["ui/src"]
---

# Launcher app shell (all screens)

- **Scope:** the whole desktop app in `ui/`: title bar, instance rail, home/instance, activity drawer, new instance, instance settings, mods, accounts, settings, first run.
- **Mode:** Operate. The player's task is to get into the game. Expression never hides Play, state or a familiar affordance.
- **Audience and job:** Perland/Asalith players, often 12–25, casual, many Spanish-speaking. Their job is to open the launcher, press Play and be in the game. Secondary jobs: create an instance, add mods, switch accounts, read an error and fix it.
- **Constraints:** the pinned layout from the owner's brief (full-bleed art, big Play, left instance rail, custom title bar, dark only). Generated art and avatars only: no Mojang assets, no Modrinth branding. Unbuilt features are "coming soon" or absent. Every string exists in en and es; es runs about 30% longer. Minimum window 960 × 600.
- **Memorable moment:** pressing Play. The amber button physically drops onto its brown step, the segmented meter fills block by block through plain-language phases, and the marquee dims as the game takes over.
- **Unresolved:** final faces (candidates: Bungee-family signage display for marquee names; Atkinson Hyperlegible Next for body, chosen for legibility with young and non-native readers); exact neutrals; the vector redraw of the land-plot mark.

## Direction contract

THESIS: Every instance is an arcade cabinet. Its generated landscape is a backlit marquee, and Play is the one lit button you press. The app refuses the category default of a navy gradient, rounded cards and a glowing blue button.

OWN-WORLD: A warm laminate-black ground. The four land-plot hues have roles: amber means act (Play), lime means running or OK, coral means stop or fault, blue means info or progress. Dirt brown is the step under every raised control: hard offset under-layers, never blurred shadows. Corners are pixel-stepped from the mark. Five control states: raised, pressed, lit, recessed, fault. At rest only Play is lit.

STORY: The player sees their instance lit up, understands it's ready, presses Play, watches honest progress with real MB figures, and is in the game. Errors tell them what to do in their own language.

FIRST VIEWPORT: The custom title bar is a thin strip on top. A 72 px rail of authored plot icons runs down the left. The marquee art fills the rest. The instance name is huge at lower left, with version and loader beneath it. The amber Play button, about 280 × 88, sits on its step at lower right, with Mods and ⋯ beside it as secondary controls. The activity drawer handle sits on the bottom edge.

SIGNATURE / MOTION: Play's press travel lands in 2 steps of 90 ms. The segmented meter fills by blocks with tabular readouts. In attract mode the marquee drifts in slow parallax and stops under reduced motion. State changes snap; they never ease.

FORM: Arcade Cabinet, position 3 of the ordered grounded list; seed key c94af540.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
