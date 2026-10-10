import { describe, expect, it } from 'vitest';
import { avatarCells } from './avatar';
import { marqueeScene } from './marquee';
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
      expect(layer.heights).toHaveLength(40);
      expect(layer.heights[0]).toBe(layer.heights[39]);
      for (const h of layer.heights) {
        expect(h).toBeGreaterThanOrEqual(1);
        expect(h).toBeLessThanOrEqual(20);
      }
    }
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
