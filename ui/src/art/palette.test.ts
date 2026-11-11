import { afterEach, expect, it } from 'vitest';
import { THEMES } from '../state/appearance';
import { marqueeScene } from './marqueeScene';
import { PALETTES } from './palette';
import { presetSeeds } from './seed';

/** The six landscapes Change image offers an instance, as a list of seeds. */
const six = presetSeeds({ id: 'asalith-fields', name: 'Asalith Fields', artSeed: null } as never);

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

it('has a landscape palette for every theme, and Laminate is the one the landscapes always had', () => {
  expect(Object.keys(PALETTES).sort()).toEqual([...THEMES].sort());
  expect(marqueeScene('asalith-fields')).toEqual(marqueeScene('asalith-fields', PALETTES.laminate));
});

it('draws the same landscapes in each theme, in its own colours', () => {
  for (const seed of six) {
    const laminate = marqueeScene(seed, PALETTES.laminate);
    for (const theme of THEMES.filter((t) => t !== 'laminate')) {
      const themed = marqueeScene(seed, PALETTES[theme]);
      // The same hills and the same sun: only the colours change.
      expect(themed.layers.map((l) => l.heights)).toEqual(laminate.layers.map((l) => l.heights));
      expect([themed.sun.x, themed.sun.y]).toEqual([laminate.sun.x, laminate.sun.y]);
      expect(themed.sky).not.toBe(laminate.sky);
      expect(themed.layers.map((l) => l.rim)).not.toEqual(laminate.layers.map((l) => l.rim));
    }
  }
});

it("keeps a theme's six landscapes six different lights", () => {
  for (const theme of THEMES) {
    const glows = new Set(six.map((seed) => marqueeScene(seed, PALETTES[theme]).glow));
    expect(glows.size, theme).toBeGreaterThanOrEqual(4);
  }
});
