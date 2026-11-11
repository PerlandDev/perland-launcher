import { describe, expect, test } from 'vitest';
import { CLOTH_COLOURS, HAIR_COLOURS, IRIS_COLOURS, paletteFor, SKIN_TONES } from './palettes';
import { parseHex } from './recolour';

const HEX = /^#[0-9a-f]{6}$/;
const luma = (hex: string) => {
  const [r, g, b] = parseHex(hex)!;
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};

describe('palettes', () => {
  test.each([
    ['skin tones', SKIN_TONES, 12],
    ['iris colours', IRIS_COLOURS, 10],
    ['hair colours', HAIR_COLOURS, 14],
    ['cloth colours', CLOTH_COLOURS, 16],
  ] as const)('%s: every entry is a #rrggbb, none twice', (_name, list, count) => {
    expect(list).toHaveLength(count);
    for (const c of list) expect(c).toMatch(HEX);
    expect(new Set(list).size).toBe(list.length);
  });

  test('the believable skin tones run from very light to very dark, the two fantasy ones last', () => {
    const real = SKIN_TONES.slice(0, 10).map(luma);
    expect(real).toEqual([...real].sort((a, b) => b - a));
    expect(real[0]).toBeGreaterThan(215);
    expect(real[9]).toBeLessThan(60);
    // A green and a blue-grey: the only two that aren't red-over-green-over-blue.
    const [gr, gg, gb] = parseHex(SKIN_TONES[10])!;
    expect(gg).toBeGreaterThan(Math.max(gr, gb));
    const [br, , bb] = parseHex(SKIN_TONES[11])!;
    expect(bb).toBeGreaterThan(br);
    for (const c of SKIN_TONES.slice(0, 10)) {
      const [r, g, b] = parseHex(c)!;
      expect(r).toBeGreaterThan(g);
      expect(g).toBeGreaterThan(b);
    }
  });

  test('the palette follows the area: skin, iris, hair, then cloth for everything else', () => {
    expect(paletteFor('body', 'skin')).toBe(SKIN_TONES);
    expect(paletteFor('eyes', 'iris')).toBe(IRIS_COLOURS);
    for (const slot of ['hair', 'brows', 'beard']) {
      expect(paletteFor(slot, 'main')).toBe(HAIR_COLOURS);
      expect(paletteFor(slot, 'tips')).toBe(HAIR_COLOURS);
    }
    // `main` is a hair colour only on hair.
    expect(paletteFor('top', 'main')).toBe(CLOTH_COLOURS);
    expect(paletteFor('hat', 'tips')).toBe(CLOTH_COLOURS);
    expect(paletteFor('hair', 'band')).toBe(CLOTH_COLOURS);
    expect(paletteFor('eyes', 'lash')).toBe(CLOTH_COLOURS);
    expect(paletteFor('glasses', 'frame')).toBe(CLOTH_COLOURS);
  });
});
