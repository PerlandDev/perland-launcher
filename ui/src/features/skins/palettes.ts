// The quick colours of the Create tab: what a colour area offers at one click, chosen by what the area is.
// Every entry is a lower-case `#rrggbb`; no list repeats a colour.

/** Ten believable tones, very light to very dark, then two for fantasy characters: a green and a blue-grey. */
export const SKIN_TONES: readonly string[] = [
  '#fbe3d2',
  '#f6d7c3',
  '#f0c8a8',
  '#e5b48d',
  '#d9a27c',
  '#c68863',
  '#a86b4b',
  '#8a5637',
  '#6b3e26',
  '#3f2a1f',
  '#7fae6a',
  '#8fa3b8',
];

/** Browns, hazel, amber, green, blues and grey, then two for fantasy characters: violet and red. */
export const IRIS_COLOURS: readonly string[] = ['#4a3324', '#6b4423', '#8a6a2f', '#b8651e', '#4c7a45', '#3f7fc4', '#7fb8e0', '#7d8a96', '#7a5cc2', '#c0392b'];

/** Naturals first (black to blond), then dyed: pink, teal, lilac, electric blue, green, white. */
export const HAIR_COLOURS: readonly string[] = [
  '#1c1612',
  '#3b2618',
  '#4b2f1f',
  '#6f4326',
  '#8e3b1f',
  '#c2622d',
  '#b08a4f',
  '#e0c068',
  '#e86aa6',
  '#2fa59a',
  '#b79be0',
  '#2f6fe0',
  '#4caf50',
  '#ececec',
];

/** Neutrals, then saturated hues around the wheel, then pastels. */
export const CLOTH_COLOURS: readonly string[] = [
  '#f2f0ea',
  '#9aa0a6',
  '#2b2b2b',
  '#7a4f2e',
  '#c0392b',
  '#e07b24',
  '#f2c14e',
  '#4c9a4a',
  '#1f9e9e',
  '#2e6fd6',
  '#1f3b73',
  '#7d4cc9',
  '#f4a6c0',
  '#a8e0c0',
  '#a9d4f5',
  '#cdb8f0',
];

/** Slots whose `main` and `tips` are hair. */
const HAIR_SLOTS: ReadonlySet<string> = new Set(['hair', 'brows', 'beard']);

/** The quick colours of the area `areaId` on a piece of `slot`. */
export function paletteFor(slot: string, areaId: string): readonly string[] {
  if (areaId === 'skin') return SKIN_TONES;
  if (areaId === 'iris') return IRIS_COLOURS;
  if ((areaId === 'main' || areaId === 'tips') && HAIR_SLOTS.has(slot)) return HAIR_COLOURS;
  return CLOTH_COLOURS;
}

/**
 * Outfits that look designed: four colours each, in the order the clothes take them. The first is the outfit's main
 * colour (the top), the second its lighter partner (trims, soles), the third the accent (a scarf, a hat, a third
 * area), the fourth what grounds it (shoes, belt, gloves).
 */
export const OUTFIT_SCHEMES: readonly (readonly [string, string, string, string])[] = [
  // Harbour: navy, cream, gold, rust.
  ['#1f3b73', '#efe6d0', '#d4a72c', '#a4502a'],
  // Forest: pine, sand, amber, bark.
  ['#2f5d3a', '#d9c9a3', '#c8792b', '#4a3323'],
  // Sunset: red, butter, orange, maroon.
  ['#d9483b', '#f6e2b3', '#f2a03d', '#5a2a27'],
  // Arctic: ice, sky, blue, slate.
  ['#dfe7ee', '#7fb8e0', '#2e6fd6', '#3a4654'],
  // Bubblegum: pink, blush, purple, plum.
  ['#f4a6c0', '#fdf3f6', '#7d4cc9', '#5b3a7a'],
  // Midnight: ink, grey, neon cyan, black.
  ['#2b2b33', '#6b6f7a', '#2fd4f2', '#17171c'],
  // Royal: purple, parchment, gold, deep violet.
  ['#5b2a86', '#e8d8a8', '#d4af37', '#2a1740'],
  // Desert: sand, ivory, teal, leather.
  ['#c9a36a', '#f1e4c6', '#2f8f8a', '#6b4226'],
  // Mint: mint, off-white, coral, deep green.
  ['#a8e0c0', '#f7f4ea', '#f29e8e', '#3f6b5a'],
  // Lava: charcoal, red, orange, soot.
  ['#2b2b2b', '#c0392b', '#f2a03d', '#3a2420'],
  // Meadow: grass, butter, pink, earth.
  ['#6aa84f', '#f2e6a0', '#e86aa6', '#6b4a2b'],
  // Ocean: teal, foam, yellow, deep sea.
  ['#1f9e9e', '#e9f4f1', '#f2c14e', '#174b5a'],
  // Mono: white, black, red, grey.
  ['#f2f0ea', '#2b2b2b', '#c0392b', '#55585e'],
  // Autumn: auburn, wheat, olive, dark brown.
  ['#8e3b1f', '#e3c9a0', '#5c7a3a', '#3b2618'],
];
