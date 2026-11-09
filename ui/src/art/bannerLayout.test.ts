import { describe, expect, it } from 'vitest';
import { bannerBox, clampZoom, coverZoom, dragFocus, focusFor, leavesEdges, placeFrame } from './bannerLayout';

const frame = { width: 1000, height: 500 };
const square = { width: 500, height: 500 };

describe('bannerBox', () => {
  it('covers the frame when no zoom was chosen, from the bottom middle by default', () => {
    expect(bannerBox(frame, square, null, null)).toEqual({ left: 0, top: -500, width: 1000, height: 1000 });
    expect(bannerBox(frame, square, null, { x: 50, y: 50 })).toEqual({ left: 0, top: -250, width: 1000, height: 1000 });
  });

  it("draws a chosen zoom as the picture's height in percent of the frame's", () => {
    expect(bannerBox(frame, square, 100, null)).toEqual({ left: 250, top: 0, width: 500, height: 500 });
    expect(bannerBox(frame, square, 50, null)).toEqual({ left: 375, top: 250, width: 250, height: 250 });
    expect(bannerBox(frame, square, 200, null).height).toBe(1000);
  });

  it('places a zoomed picture across a 3:2 band in the middle of the frame, so extra width goes to both sides', () => {
    // The 1000 × 500 frame's band is 750 wide, from 125 to 875.
    expect(placeFrame(frame)).toEqual({ left: 125, width: 750 });
    expect(bannerBox(frame, square, 50, { x: 0, y: 0 })).toEqual({ left: 125, top: 0, width: 250, height: 250 });
    expect(bannerBox(frame, square, 50, { x: 100, y: 100 })).toEqual({ left: 625, top: 250, width: 250, height: 250 });
  });

  it('keeps a chosen zoom and position the same against the header when the window changes width', () => {
    // The window the player adjusted it in, then maximised: wider, a little taller.
    const small = { width: 1065, height: 750 };
    const wide = { width: 1808, height: 1030 };
    const focus = { x: 70, y: 40 };
    const before = bannerBox(small, square, 107, focus);
    const after = bannerBox(wide, square, 107, focus);
    expect(after.height / wide.height).toBeCloseTo(before.height / small.height);
    // Its centre keeps its distance from the header's centre, in header heights: the new width goes to both sides.
    const offset = (box: typeof before, f: typeof small) => ({
      x: (box.left + box.width / 2 - f.width / 2) / f.height,
      y: (box.top + box.height / 2 - f.height / 2) / f.height,
    });
    expect(offset(after, wide).x).toBeCloseTo(offset(before, small).x);
    expect(offset(after, wide).y).toBeCloseTo(offset(before, small).y);
  });
});

it('clamps a zoom to 25–500 in steps of 5', () => {
  expect(clampZoom(12)).toBe(25);
  expect(clampZoom(501)).toBe(500);
  expect(clampZoom(152)).toBe(150);
});

it('gives the zoom that covers the frame, so adjusting starts from what Home shows', () => {
  expect(coverZoom(frame, square)).toBe(200);
  expect(coverZoom({ width: 1065, height: 750 }, square)).toBe(140);
  // A wide picture covers by its height.
  expect(coverZoom(frame, { width: 3000, height: 1000 })).toBe(100);
});

describe('dragFocus', () => {
  it('moves the picture with the pointer inside its free space', () => {
    const box = bannerBox(frame, square, 100, null);
    expect(dragFocus({ x: 50, y: 100 }, { width: 150, height: 0 }, frame, box)).toEqual({ x: 80, y: 100 });
  });

  it('keeps an axis with no free space and moves an overflowing one the other way', () => {
    const box = bannerBox(frame, square, null, null);
    const moved = dragFocus({ x: 50, y: 100 }, { width: 300, height: 100 }, frame, box);
    expect(moved).toEqual({ x: 50, y: 80 });
    expect(Number.isFinite(moved.x) && Number.isFinite(moved.y)).toBe(true);
  });

  it('stays inside 0–100', () => {
    const box = bannerBox(frame, square, 100, null);
    expect(dragFocus({ x: 90, y: 0 }, { width: 900, height: -50 }, frame, box)).toEqual({ x: 100, y: 0 });
  });
});

it('finds the position that draws a zoomed picture where a box already is, so choosing a zoom never moves it', () => {
  const cover = bannerBox(frame, square, null, { x: 30, y: 60 });
  const focus = focusFor(frame, cover, 200);
  const zoomed = bannerBox(frame, square, 200, focus);
  expect(zoomed.left).toBeCloseTo(cover.left, 0);
  expect(zoomed.top).toBeCloseTo(cover.top, 0);
});

it('says whether a picture leaves part of the frame uncovered', () => {
  expect(leavesEdges(frame, bannerBox(frame, square, null, null))).toBe(false);
  expect(leavesEdges(frame, bannerBox(frame, square, 200, null))).toBe(false);
  expect(leavesEdges(frame, bannerBox(frame, square, 150, null))).toBe(true);
});
