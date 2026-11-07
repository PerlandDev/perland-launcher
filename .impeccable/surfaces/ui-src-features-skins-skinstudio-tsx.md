---
version: 1
slug: "ui-src-features-skins-skinstudio-tsx"
primary_target: "ui/src/features/skins/SkinStudio.tsx"
related_targets: []
---

# Skins screen (skin editor)

Scope: the Skins screen (`ui/src/features/skins/`), mode Operate. Audience: Perland players making and wearing their own Minecraft skin. Job: paint a skin on a 3D figure, keep a wardrobe of skins, wear one on a Microsoft account. Constraints: inside the Arcade Cabinet world; 960 × 600 minimum; offline accounts never upload.

## Direction contract

FORM: Fitting room (seed:e698ed3630afc512, roll degraded: no challengers).

THESIS: The figure is the hero, the way the marquee is on Home: a skin editor is a fitting room, not a paint program. It refuses the category default of a toolbar-over-canvas paint app with a small preview.

OWN-WORLD: Laminate ground, dirt-stepped raised keys, notched corners, Bungee only for the screen title and the skin's name. The stage is a recessed ground well with a pixel plinth of dirt steps under the figure. Lime lamp marks the chosen tool and the worn skin; amber only on Wear this skin.

STORY: The player sees their figure big, sees their wardrobe, sees the tools in a hotbar they already know from the game, and believes painting is direct: touch the figure, it changes.

FIRST VIEWPORT: Wardrobe column (15rem) | stage with the figure, model and layer controls along its top, Wear this skin lower right | texture panel (flat 64×64, colour picker, swatches). The hotbar runs the full width at the bottom.

SIGNATURE: The 9-slot hotbar with 1–9 keys, each slot a raised key that presses into its slot with the lime lamp; painting on the figure live in both views.

RISK: Left-drag must both paint and orbit: drag on the figure paints, drag on empty stage or right-drag orbits. Hover texel highlight makes the target clear.
