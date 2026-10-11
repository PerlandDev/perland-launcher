import { expect, it } from 'vitest';
import { pixelUpscale } from './upscale';

it('keeps hard pixels only for art blown up at least twice', () => {
  expect(pixelUpscale({ width: 16, height: 16 }, { width: 40, height: 40 })).toBe(true);
  expect(pixelUpscale({ width: 480, height: 270 }, { width: 1200, height: 560 })).toBe(true);
});

it('scales real art smoothly when it shrinks or barely grows', () => {
  expect(pixelUpscale({ width: 512, height: 512 }, { width: 40, height: 40 })).toBe(false);
  expect(pixelUpscale({ width: 1920, height: 1080 }, { width: 1200, height: 560 })).toBe(false);
  expect(pixelUpscale({ width: 1280, height: 720 }, { width: 1400, height: 560 })).toBe(false);
});

it('covers the box on its longer side, like object-fit: cover', () => {
  // 400 wide fits a 700 box at 1.75x, but its 100 height must stretch 4x to fill 400.
  expect(pixelUpscale({ width: 400, height: 100 }, { width: 700, height: 400 })).toBe(true);
});

it('treats an image with no size yet as not upscaled', () => {
  expect(pixelUpscale({ width: 0, height: 0 }, { width: 40, height: 40 })).toBe(false);
});
