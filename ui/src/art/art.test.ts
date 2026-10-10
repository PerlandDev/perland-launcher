import { describe, expect, it } from 'vitest';
import { avatarCells } from './avatarCells';
import { COLS, marqueeScene } from './marqueeScene';
import { plotCells } from './plot';
import { hash, mix, rng } from './random';

describe('random', () => {
  it('is stable across runs', () => {
    expect(hash('asalith-fields')).toBe(hash('asalith-fields'));
    expect(hash('a')).not.toBe(hash('b'));
    const a = rng(42);
    const b = rng(42);
    expect([a(), a(), a()]).toEqual([b(), b(), b()]);
  });

  it('mixes colours', () => {
    expect(mix('#000000', '#ffffff', 0.5)).toBe('#808080');
    expect(mix('#ff0000', '#0000ff', 0)).toBe('#ff0000');
  });
});

describe('marquee', () => {
  it('is deterministic per instance and differs between instances', () => {
    expect(marqueeScene('asalith-fields')).toEqual(marqueeScene('asalith-fields'));
    expect(marqueeScene('asalith-fields')).not.toEqual(marqueeScene('creative-flat'));
  });

  it('tiles seamlessly and stays inside the frame', () => {
    const scene = marqueeScene('asalith-fields');
    expect(scene.layers.length).toBeGreaterThanOrEqual(4);
    for (const layer of scene.layers) {
      expect(layer.heights).toHaveLength(COLS);
      expect(layer.heights[0]).toBe(layer.heights[COLS - 1]);
      for (const h of layer.heights) {
        expect(h).toBeGreaterThanOrEqual(1);
        expect(h).toBeLessThanOrEqual(24);
      }
    }
  });

  it('is lit from behind, never a navy sky', () => {
    for (const seed of ['asalith-fields', 'creative-flat', 'old-pack', 'perland']) {
      const scene = marqueeScene(seed);
      // The sky is warm black: red at or above green, green at or above blue.
      const [r = 0, g = 0, b = 0] = [1, 3, 5].map((i) => parseInt(scene.sky.slice(i, i + 2), 16));
      expect(r).toBeGreaterThanOrEqual(g);
      expect(g).toBeGreaterThanOrEqual(b);
      // The light source sits in the upper right, clear of the name (lower left) and centred cards.
      expect(scene.sun.x / COLS).toBeGreaterThanOrEqual(0.78);
      expect(scene.sun.x / COLS).toBeLessThanOrEqual(0.88);
      expect(scene.glow).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it('never cuts a one-column drip deeper than a single cell into a hill', () => {
    for (const seed of ['asalith-fields', 'creative-flat', 'old-pack', 'perland', 'a', 'b', 'c']) {
      for (const { heights } of marqueeScene(seed).layers) {
        for (let i = 0; i < heights.length; i++) {
          const left = heights[(i - 1 + heights.length) % heights.length]!;
          const right = heights[(i + 1) % heights.length]!;
          expect(Math.min(left, right) - heights[i]!).toBeLessThanOrEqual(1);
        }
      }
    }
  });

  it('steps the hills from a backlit far band to a dark near floor', () => {
    const lum = (hex: string) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).reduce((a, b) => a + b, 0) / 3;
    const bodies = marqueeScene('asalith-fields').layers.map((l) => lum(l.body));
    for (let i = 1; i < bodies.length; i++) expect(bodies[i - 1]! - bodies[i]!).toBeGreaterThanOrEqual(14);
  });

  it('keeps the nearest band dark and tall enough for the name to sit on', () => {
    const near = marqueeScene('asalith-fields').layers.at(-1)!;
    expect(Math.min(...near.heights)).toBeGreaterThanOrEqual(9);
    expect(Math.max(...near.heights)).toBeLessThanOrEqual(13);
    const lum = (hex: string) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).reduce((a, b) => a + b, 0) / 3;
    expect(lum(near.body)).toBeLessThan(40);
    expect(lum(near.rim)).toBeLessThan(110);
  });
});

describe('plot icon and avatar', () => {
  it('draw inside an 8×8 grid, deterministically', () => {
    for (const cells of [plotCells('pack'), avatarCells('5627dd98-e6be-3c21-b8a8-e92344183641')]) {
      expect(cells.length).toBeGreaterThan(8);
      for (const c of cells) {
        expect(c.x).toBeGreaterThanOrEqual(0);
        expect(c.x).toBeLessThan(8);
        expect(c.y).toBeGreaterThanOrEqual(0);
        expect(c.y).toBeLessThan(8);
      }
    }
    expect(plotCells('pack')).toEqual(plotCells('pack'));
  });

  it('makes avatars symmetric', () => {
    const cells = avatarCells('uuid-1');
    for (const c of cells) {
      expect(cells.some((o) => o.x === 7 - c.x && o.y === c.y && o.fill === c.fill)).toBe(true);
    }
  });
});
