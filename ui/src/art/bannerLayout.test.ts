import { describe, expect, it } from 'vitest';
import { bannerBox, clampZoom, covers, dragFocus } from './bannerLayout';

const frame = { width: 1000, height: 500 };
const square = { width: 500, height: 500 };

describe('bannerBox', () => {
  it('covers the frame at 100, from the bottom middle by default', () => {
    expect(bannerBox(frame, square, null, null)).toEqual({ left: 0, top: -500, width: 1000, height: 1000 });
    expect(bannerBox(frame, square, 100, null)).toEqual({ left: 0, top: -500, width: 1000, height: 1000 });
    expect(bannerBox(frame, square, null, { x: 50, y: 50 })).toEqual({ left: 0, top: -250, width: 1000, height: 1000 });
  });

  it('draws the picture smaller below 100, placed in the free space by the focus', () => {
    expect(bannerBox(frame, square, 50, null)).toEqual({ left: 250, top: 0, width: 500, height: 500 });
    expect(bannerBox(frame, square, 50, { x: 0, y: 0 })).toEqual({ left: 0, top: 0, width: 500, height: 500 });
  });

  it('draws it bigger above 100', () => {
    expect(bannerBox(frame, square, 200, null).width).toBe(2000);
  });
});

it('clamps a zoom to 25–300 in steps of 5', () => {
  expect(clampZoom(12)).toBe(25);
  expect(clampZoom(301)).toBe(300);
  expect(clampZoom(152)).toBe(150);
});

describe('dragFocus', () => {
  it('moves the picture with the pointer inside its free space', () => {
    const box = bannerBox(frame, square, 50, null);
    expect(dragFocus({ x: 50, y: 100 }, { width: 100, height: 0 }, frame, box)).toEqual({ x: 70, y: 100 });
  });

  it('keeps an axis with no free space and moves an overflowing one the other way', () => {
    const box = bannerBox(frame, square, 100, null);
    const moved = dragFocus({ x: 50, y: 100 }, { width: 300, height: 100 }, frame, box);
    expect(moved).toEqual({ x: 50, y: 80 });
    expect(Number.isFinite(moved.x) && Number.isFinite(moved.y)).toBe(true);
  });

  it('stays inside 0–100', () => {
    const box = bannerBox(frame, square, 50, null);
    expect(dragFocus({ x: 90, y: 0 }, { width: 900, height: -50 }, frame, box)).toEqual({ x: 100, y: 0 });
  });
});

it('says whether a zoom still covers the frame', () => {
  expect(covers(null)).toBe(true);
  expect(covers(100)).toBe(true);
  expect(covers(95)).toBe(false);
});
