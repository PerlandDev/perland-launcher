import { describe, expect, test } from 'vitest';
import { compose, type ComposeLayer } from './compose';
import { composeFace, FACE_SIZE, faceOf } from './face';

const SIZE = 64;
const blank = () => new Uint8ClampedArray(SIZE * SIZE * 4);
const put = (buf: Uint8ClampedArray, x: number, y: number, rgba: [number, number, number, number]) => buf.set(rgba, (y * SIZE + x) * 4);
const at = (face: Uint8ClampedArray, x: number, y: number) => [...face.slice((y * FACE_SIZE + x) * 4, (y * FACE_SIZE + x) * 4 + 4)];

/** A layer that fills `rect` with mid grey in its first colour area. */
function layer(slot: string, colour: string, [x0, y0, x1, y1]: [number, number, number, number]): ComposeLayer {
  const art = blank();
  const mask = blank();
  for (let y = y0; y < y1; y++) {
    for (let x = x0; x < x1; x++) {
      put(art, x, y, [128, 128, 128, 255]);
      put(mask, x, y, [85, 0, 0, 255]);
    }
  }
  return { slot, art, mask, colours: [colour] };
}

describe('face', () => {
  test('faceOf reads the head front with the outer layer over it', () => {
    const skin = blank();
    put(skin, 8, 8, [10, 20, 30, 255]);
    put(skin, 15, 15, [40, 50, 60, 255]);
    // The outer layer's front starts at (40, 8): opaque replaces, clear leaves the base, half blends.
    put(skin, 47, 15, [200, 0, 0, 255]);
    put(skin, 41, 8, [0, 0, 255, 128]);
    put(skin, 9, 8, [255, 255, 255, 255]);
    // Texels outside the two fronts don't show.
    put(skin, 7, 8, [1, 1, 1, 255]);
    put(skin, 16, 8, [2, 2, 2, 255]);
    const face = faceOf(skin);
    expect(face).toHaveLength(FACE_SIZE * FACE_SIZE * 4);
    expect(at(face, 0, 0)).toEqual([10, 20, 30, 255]);
    expect(at(face, 7, 7)).toEqual([200, 0, 0, 255]);
    expect(at(face, 1, 0)).toEqual([127, 127, 255, 255]);
    expect(at(face, 2, 0)).toEqual([0, 0, 0, 0]);
  });

  test('composeFace is the face of the composed skin, without composing the rest', () => {
    const painting = blank();
    for (let y = 8; y < 12; y++) for (let x = 8; x < 16; x++) put(painting, x, y, [90, 60, 30, 255]);
    put(painting, 44, 10, [0, 255, 0, 200]);
    const layers = [
      layer('body', '#d9a27c', [0, 0, 40, 64]),
      layer('eyes', '#2266cc', [9, 11, 15, 13]),
      layer('hair', '#e0c068', [40, 8, 48, 10]),
      layer('hat', '#ff0000', [8, 8, 16, 9]),
      layer('top', '#00ff00', [16, 16, 40, 32]),
    ];
    expect(composeFace(painting, layers)).toEqual(faceOf(compose(painting, layers)));
    expect(composeFace(null, layers)).toEqual(faceOf(compose(null, layers)));
    expect(composeFace(painting, [])).toEqual(faceOf(painting));
    // Face pieces go under the painting, the rest over it.
    const face = composeFace(painting, layers);
    expect(at(face, 1, 3)).toEqual([90, 60, 30, 255]);
    expect(at(face, 1, 4)).toEqual([0x22, 0x66, 0xcc, 255]);
    expect(at(face, 0, 0)).toEqual([0xe0, 0xc0, 0x68, 255]);
  });
});
