import { describe, expect, test } from 'vitest';
import { compose, FACE_SLOTS, SLOTS } from './compose';

const N = 64 * 64 * 4;
const solid = (r: number, g: number, b: number, a = 255) => {
  const d = new Uint8ClampedArray(N);
  for (let i = 0; i < N; i += 4) d.set([r, g, b, a], i);
  return d;
};
const fixed = new Uint8ClampedArray(N);
const layer = (slot: string, c: [number, number, number, number]) => ({ slot, art: solid(...c), mask: fixed, colours: [] });
const first = (d: Uint8ClampedArray) => [...d.slice(0, 4)];

describe('compose', () => {
  test('slots are in the documented order', () => {
    expect(FACE_SLOTS).toEqual(['body', 'eyes', 'brows', 'mouth', 'face_detail', 'beard']);
    expect(SLOTS[0]).toBe('body');
    expect(SLOTS[SLOTS.length - 1]).toBe('hat');
  });

  test('painting sits above body and face, below clothes', () => {
    const painting = solid(0, 255, 0);
    const bodyOnly = compose(painting, [layer('body', [255, 0, 0, 255])]);
    expect(first(bodyOnly)).toEqual([0, 255, 0, 255]);
    const withShirt = compose(painting, [layer('top', [0, 0, 255, 255]), layer('body', [255, 0, 0, 255]), layer('beard', [9, 9, 9, 255])]);
    expect(first(withShirt)).toEqual([0, 0, 255, 255]);
    const noPaint = compose(null, [layer('top', [0, 0, 255, 255]), layer('body', [255, 0, 0, 255])]);
    expect(first(noPaint)).toEqual([0, 0, 255, 255]);
  });

  test('layers arrive in any order and blend with alpha', () => {
    const out = compose(null, [layer('hat', [0, 0, 0, 128]), layer('body', [200, 100, 50, 255])]);
    expect(first(out)).toEqual([100, 50, 25, 255]);
  });

  test('recolours grey art through its mask and colours', () => {
    const mask = solid(85, 0, 0);
    const out = compose(null, [{ slot: 'top', art: solid(128, 128, 128), mask, colours: ['#c0392b'] }]);
    expect(first(out)).toEqual([192, 57, 43, 255]);
    const noColour = compose(null, [{ slot: 'top', art: solid(128, 128, 128), mask, colours: [] }]);
    expect(first(noColour)).toEqual([128, 128, 128, 255]);
  });

  test('nothing gives transparent', () => {
    expect(first(compose(null, []))).toEqual([0, 0, 0, 0]);
  });
});
