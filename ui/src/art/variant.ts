import { hash, PLOT_HUES } from './random';

/** How a picked landscape (`<base>~N`, N = 2..6, which only the image picker makes) differs from its
 *  base scene, so the six presets read as six places at tile size: one per plot hue, then a different
 *  sun and ridge. Every other seed (an instance id, a pinned seed) is drawn exactly as it always was. */
export interface Variant {
  /** Steps along the plot hues from the base seed's own hue. */
  hueShift: number;
  sun: { side: 'left' | 'right'; height: 'high' | 'mid' | 'low' };
  ridge: 'plains' | 'hills' | 'peaks';
}

const VARIANTS: Record<number, Variant> = {
  2: { hueShift: 1, sun: { side: 'right', height: 'high' }, ridge: 'plains' },
  3: { hueShift: 2, sun: { side: 'left', height: 'mid' }, ridge: 'peaks' },
  4: { hueShift: 3, sun: { side: 'right', height: 'low' }, ridge: 'hills' },
  5: { hueShift: 0, sun: { side: 'left', height: 'high' }, ridge: 'plains' },
  6: { hueShift: 1, sun: { side: 'left', height: 'low' }, ridge: 'peaks' },
};

/** The preset variant a seed names, or null for every seed that isn't `<base>~2` to `<base>~6`. */
export function presetVariant(seed: string): { base: string; variant: Variant } | null {
  const match = /^(.+)~([2-6])$/.exec(seed);
  if (!match) return null;
  return { base: match[1]!, variant: VARIANTS[Number(match[2])]! };
}

/** Which plot hue a seed's art is tinted toward (an index into PLOT_HUES). */
export function hueIndex(seed: string): number {
  const preset = presetVariant(seed);
  if (!preset) return hash(seed) % PLOT_HUES.length;
  return (hash(preset.base) + preset.variant.hueShift) % PLOT_HUES.length;
}
