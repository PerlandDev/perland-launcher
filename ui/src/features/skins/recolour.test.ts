import { describe, expect, test } from 'vitest';
import { recolour } from './recolour';

const px = (r: number, g: number, b: number, a: number) => new Uint8ClampedArray([r, g, b, a]);

describe('recolour', () => {
  test('matches the Engine vectors', () => {
    const cases: [number, number[]][] = [
      [128, [192, 57, 43]],
      [255, [255, 255, 255]],
      [0, [0, 0, 0]],
      [191, [223, 155, 148]],
      [64, [96, 29, 22]],
    ];
    for (const [g, want] of cases) {
      const out = recolour(px(g, g, g, 200), px(85, 0, 0, 255), ['#c0392b']);
      expect([...out]).toEqual([...want, 200]);
    }
  });

  test('uses the colour of the mask area', () => {
    const out = recolour(px(128, 128, 128, 255), px(170, 0, 0, 255), ['#ff0000', '#00ff00', '#0000ff']);
    expect([...out]).toEqual([0, 255, 0, 255]);
    expect([...recolour(px(128, 128, 128, 255), px(255, 0, 0, 255), ['#ff0000', '#00ff00', '#0000ff'])]).toEqual([0, 0, 255, 255]);
  });

  test('fixed pixels, missing colours and alpha are kept', () => {
    expect([...recolour(px(10, 20, 30, 99), px(0, 0, 0, 255), ['#ffffff'])]).toEqual([10, 20, 30, 99]);
    expect([...recolour(px(10, 20, 30, 99), px(170, 0, 0, 255), ['#ffffff'])]).toEqual([10, 20, 30, 99]);
    expect([...recolour(px(10, 20, 30, 99), px(85, 0, 0, 255), [undefined])]).toEqual([10, 20, 30, 99]);
    expect(recolour(px(128, 128, 128, 0), px(85, 0, 0, 255), ['#102030'])[3]).toBe(0);
  });
});
