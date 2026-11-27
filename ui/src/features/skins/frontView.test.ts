import { expect, it } from 'vitest';
import { FRONT_H, FRONT_W, frontView } from './frontView';
import { Pixels } from './paint';

const at = (data: Uint8ClampedArray, x: number, y: number) => Array.from(data.slice((y * FRONT_W + x) * 4, (y * FRONT_W + x) * 4 + 4));

it('lays the front faces out as the figure stands, the player’s right on the viewer’s left', () => {
  const skin = new Pixels();
  skin.set(8, 8, [255, 0, 0, 255]); // the face's top-left texel
  skin.set(44, 20, [0, 255, 0, 255]); // the right arm's front, top-left
  skin.set(36, 52, [0, 0, 255, 255]); // the left arm's front, top-left
  skin.set(20, 52, [9, 9, 9, 255]); // the left leg's front, top-left
  const view = frontView(skin, 'classic');
  expect(view).toHaveLength(FRONT_W * FRONT_H * 4);
  expect(at(view, 4, 0)).toEqual([255, 0, 0, 255]);
  expect(at(view, 0, 8)).toEqual([0, 255, 0, 255]);
  expect(at(view, 12, 8)).toEqual([0, 0, 255, 255]);
  expect(at(view, 8, 20)).toEqual([9, 9, 9, 255]);
});

it('draws the outer layer over the base, part see-through paint blended', () => {
  const skin = new Pixels();
  skin.set(8, 8, [200, 0, 0, 255]);
  skin.set(40, 8, [0, 0, 200, 128]); // the hat over the same texel, half see-through
  skin.set(9, 8, [200, 0, 0, 255]);
  skin.set(41, 8, [0, 0, 0, 0]); // an empty hat texel leaves the face showing
  const view = frontView(skin, 'classic');
  const [r, g, b, a] = at(view, 4, 0);
  expect(r).toBeGreaterThan(90);
  expect(r).toBeLessThan(110);
  expect(g).toBe(0);
  expect(b).toBeGreaterThan(90);
  expect(a).toBe(255);
  expect(at(view, 5, 0)).toEqual([200, 0, 0, 255]);
});

it('gives slim arms three texels, standing against the body', () => {
  const skin = new Pixels();
  for (let x = 44; x < 48; x++) skin.set(x, 20, [0, 255, 0, 255]);
  const view = frontView(skin, 'slim');
  expect(at(view, 0, 8)[3]).toBe(0);
  expect(at(view, 1, 8)).toEqual([0, 255, 0, 255]);
  expect(at(view, 3, 8)).toEqual([0, 255, 0, 255]);
});
