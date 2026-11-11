import { describe, expect, it } from 'vitest';
import { flatFront, FLAT_H, FLAT_W } from './flatFront';

const SKIN = 64;
function skin(paint: (set: (x: number, y: number, rgba: [number, number, number, number]) => void) => void): Uint8ClampedArray {
  const data = new Uint8ClampedArray(SKIN * SKIN * 4);
  paint((x, y, rgba) => data.set(rgba, (y * SKIN + x) * 4));
  return data;
}
const at = (flat: Uint8ClampedArray, x: number, y: number) => Array.from(flat.subarray((y * FLAT_W + x) * 4, (y * FLAT_W + x) * 4 + 4));

describe('flatFront', () => {
  it('is a 16 × 32 front view: head over body between the arms, over the legs', () => {
    const flat = flatFront(
      skin((set) => {
        set(8, 8, [1, 0, 0, 255]); // head front, top-left
        set(20, 20, [2, 0, 0, 255]); // body front, top-left
        set(44, 20, [3, 0, 0, 255]); // right arm front, top-left
        set(4, 20, [4, 0, 0, 255]); // right leg front, top-left
        set(20, 52, [5, 0, 0, 255]); // left leg front, top-left
      }),
      'classic',
    );
    expect(flat).toHaveLength(FLAT_W * FLAT_H * 4);
    expect(at(flat, 4, 0)).toEqual([1, 0, 0, 255]);
    expect(at(flat, 4, 8)).toEqual([2, 0, 0, 255]);
    expect(at(flat, 0, 8)).toEqual([3, 0, 0, 255]);
    expect(at(flat, 4, 20)).toEqual([4, 0, 0, 255]);
    expect(at(flat, 8, 20)).toEqual([5, 0, 0, 255]);
  });

  it('draws slim arms one texel in, so the figure keeps its middle', () => {
    const flat = flatFront(
      skin((set) => set(44, 20, [3, 0, 0, 255])),
      'slim',
    );
    expect(at(flat, 0, 8)).toEqual([0, 0, 0, 0]);
    expect(at(flat, 1, 8)).toEqual([3, 0, 0, 255]);
  });

  it('lays the outer layer over the skin, keeping what shows through part see-through paint', () => {
    const flat = flatFront(
      skin((set) => {
        set(8, 8, [200, 0, 0, 255]);
        set(40, 8, [0, 0, 200, 128]); // hat, half see-through
        set(9, 8, [200, 0, 0, 255]);
        set(41, 8, [0, 200, 0, 255]); // hat, solid
      }),
      'classic',
    );
    const [r, , b, a] = at(flat, 4, 0);
    expect(a).toBe(255);
    expect(r).toBeGreaterThan(80);
    expect(b).toBeGreaterThan(80);
    expect(at(flat, 5, 0)).toEqual([0, 200, 0, 255]);
  });
});
