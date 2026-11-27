import { afterEach, expect, it } from 'vitest';
import { MADE_THEMES } from '../state/appearance';
import { marqueeScene } from './marqueeScene';
import { currentPalette, customPalette, PALETTES } from './palette';
import { presetSeeds } from './seed';

/** The six landscapes Change image offers an instance, as a list of seeds. */
const six = presetSeeds({ id: 'asalith-fields', name: 'Asalith Fields', artSeed: null } as never);

afterEach(() => {
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.room;
});

it('has a landscape palette for every made theme, and Laminate is the one the landscapes always had', () => {
  expect(Object.keys(PALETTES).sort()).toEqual([...MADE_THEMES].sort());
  expect(marqueeScene('asalith-fields')).toEqual(marqueeScene('asalith-fields', PALETTES.laminate));
});

it('draws the same landscapes in each theme, in its own colours', () => {
  for (const seed of six) {
    const laminate = marqueeScene(seed, PALETTES.laminate);
    for (const theme of MADE_THEMES.filter((t) => t !== 'laminate')) {
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
  for (const theme of MADE_THEMES) {
    const glows = new Set(six.map((seed) => marqueeScene(seed, PALETTES[theme]).glow));
    expect(glows.size, theme).toBeGreaterThanOrEqual(4);
  }
});

it("paints the player's own room's landscapes from its two colours", () => {
  const palette = customPalette({ background: '#101820', buttons: '#ff6ad5' });
  expect(palette.ground).toBe('#101820');
  // Its lamp and hues lean on the buttons colour, so the room and its landscapes are one family.
  expect(palette.hues).toContain('#ff6ad5');
  const scene = marqueeScene('asalith-fields', palette);
  expect(scene.layers.map((l) => l.heights)).toEqual(marqueeScene('asalith-fields', PALETTES.laminate).layers.map((l) => l.heights));
  expect(scene.sky).not.toBe(marqueeScene('asalith-fields', PALETTES.laminate).sky);
  // Six different lights, as every made room has.
  expect(new Set(six.map((seed) => marqueeScene(seed, palette).glow)).size).toBeGreaterThanOrEqual(4);
});

it("follows the player's own room on the page", () => {
  document.documentElement.dataset.theme = 'custom';
  document.documentElement.dataset.room = '#101820,#ff6ad5';
  expect(currentPalette()).toEqual(customPalette({ background: '#101820', buttons: '#ff6ad5' }));
  // A room the page doesn't carry falls back to the default, never to a crash.
  delete document.documentElement.dataset.room;
  expect(currentPalette()).toEqual(PALETTES.laminate);
});
