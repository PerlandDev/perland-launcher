import { describe, expect, it } from 'vitest';
import { COLS, marqueeScene } from './marqueeScene';
import { plotCells } from './plot';
import { hash } from './random';
import { presetSeeds } from './seed';

/** A fingerprint of everything a scene or plot icon draws, so any pixel change shows. */
const print = (value: unknown) => hash(JSON.stringify(value));

describe('base seeds (ruling R7)', () => {
  // Recorded from the recipe before the preset variants existed: an instance's own art never changes.
  it('draw exactly what they drew before', () => {
    const seeds = ['asalith-fields', 'sky-islands', 'creative-flat', 'perland', 'survival', 'a~1', 'b~7', 'c~12'];
    expect(seeds.map((s) => print(marqueeScene(s)))).toMatchInlineSnapshot(`
      [
        592786442,
        2789189147,
        2226172046,
        2518846104,
        2781673248,
        2412438922,
        2024596345,
        2992626972,
      ]
    `);
    expect(seeds.map((s) => print(plotCells(s)))).toMatchInlineSnapshot(`
      [
        336480436,
        3087465871,
        3892616825,
        2569482479,
        3998232915,
        3087465871,
        3984798743,
        2850425192,
      ]
    `);
    expect(marqueeScene('asalith-fields').glow).toMatchInlineSnapshot(`"#decd94"`);
    expect(marqueeScene('asalith-fields').sun).toMatchInlineSnapshot(`
      {
        "color": "#f5ebcf",
        "x": 48,
        "y": 5,
      }
    `);
  });
});

/** What sets a landscape apart at tile size: its bloom's colour, the sun's side and height, and how tall the far ridge stands. */
function look(seed: string) {
  const scene = marqueeScene(seed);
  const far = scene.layers[0]!.heights;
  const ridge = far.reduce((sum, h) => sum + h, 0) / far.length;
  return {
    glow: scene.glow,
    side: scene.sun.x < COLS / 2 ? 'left' : 'right',
    height: scene.sun.y < 4 ? 'high' : scene.sun.y > 9 ? 'low' : 'mid',
    ridge: ridge < 17 ? 'plains' : ridge > 21 ? 'peaks' : 'hills',
  };
}

describe('the six presets', () => {
  it.each(['asalith-fields', 'sky-islands', 'creative-flat', 'perland', 'survival', 'x'])('of %s are six different landscapes', (id) => {
    const looks = presetSeeds({ id, artSeed: null }).map(look);
    for (let a = 0; a < looks.length; a++) {
      for (let b = a + 1; b < looks.length; b++) {
        const differ = (Object.keys(looks[a]!) as (keyof ReturnType<typeof look>)[]).filter((k) => looks[a]![k] !== looks[b]![k]);
        expect(differ.length, `tiles ${a + 1} and ${b + 1}: ${JSON.stringify(looks[a])} vs ${JSON.stringify(looks[b])}`).toBeGreaterThanOrEqual(2);
      }
    }
    // Their blooms are tinted toward different plot hues, not one dusk.
    expect(new Set(looks.map((l) => l.glow)).size).toBeGreaterThanOrEqual(4);
  });

  it('gives a preset its own plot icon hue, the same as its scene', () => {
    const seeds = presetSeeds({ id: 'asalith-fields', artSeed: null });
    const hues = seeds.map((s) => plotCells(s).find((c) => c.fill !== '#7a4a2a')!.fill);
    expect(new Set(hues.slice(0, 4)).size).toBe(4);
  });
});
