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
