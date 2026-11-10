import { describe, expect, it } from 'vitest';
import { aspectOf, bannerBox, clampZoom, coverZoom, dragFocus, leavesEdges, placeFrame, restateView, zoomAround, type Box, type Size } from './bannerLayout';

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
    expect(placeFrame(frame)).toEqual({ left: 125, width: 750, height: 500 });
    expect(bannerBox(frame, square, 50, { x: 0, y: 0 })).toEqual({ left: 125, top: 0, width: 250, height: 250 });
    expect(bannerBox(frame, square, 50, { x: 100, y: 100 })).toEqual({ left: 625, top: 250, width: 250, height: 250 });
  });

  it('keeps a chosen zoom and position the same against the header when the window changes width', () => {
    // The window the player adjusted it in, then maximised: wider, a little taller. Both are wider than the band.
    const small = { width: 1200, height: 750 };
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

describe('the shape a view was chosen at', () => {
  const wide = { width: 1848, height: 1008 };
  const picture = { width: 1920, height: 1080 };
  const focus = { x: 50, y: 60 };
  /** The part of the picture `frame` shows, as fractions of the picture's width and height. */
  const shown = (f: Size, box: Box) => ({
    left: Math.max(0, -box.left / box.width),
    right: Math.min(1, (f.width - box.left) / box.width),
    top: Math.max(0, -box.top / box.height),
    bottom: Math.min(1, (f.height - box.top) / box.height),
  });

  it('is a band as tall as a wider frame, or as wide as a narrower one', () => {
    expect(placeFrame({ width: 1000, height: 500 }, 1.5)).toEqual({ left: 125, width: 750, height: 500 });
    expect(placeFrame({ width: 600, height: 500 }, 1.5)).toEqual({ left: 0, width: 600, height: 400 });
  });

  it("shows a narrower window the same picture from side to side, smaller, never closer", () => {
    // Adjusted maximised, then the window made smaller and narrower.
    const aspect = aspectOf(wide) / 1000;
    const narrow = { width: 928, height: 648 };
    const there = shown(wide, bannerBox(wide, picture, 145, focus, aspect));
    const here = shown(narrow, bannerBox(narrow, picture, 145, focus, aspect));
    expect(here.left).toBeCloseTo(there.left, 2);
    expect(here.right).toBeCloseTo(there.right, 2);
    // The new room above and below shows more of the picture.
    expect(here.top).toBeLessThanOrEqual(there.top + 0.001);
    expect(here.bottom).toBeGreaterThanOrEqual(there.bottom - 0.001);
  });

  it('shows a wider or taller window all it showed, and more around it', () => {
    const narrow = { width: 928, height: 648 };
    const aspect = aspectOf(narrow) / 1000;
    const there = shown(narrow, bannerBox(narrow, picture, 200, focus, aspect));
    for (const other of [wide, { width: 928, height: 900 }]) {
      const here = shown(other, bannerBox(other, picture, 200, focus, aspect));
      expect(here.left).toBeLessThanOrEqual(there.left + 0.001);
      expect(here.right).toBeGreaterThanOrEqual(there.right - 0.001);
      expect(here.top).toBeLessThanOrEqual(there.top + 0.001);
      expect(here.bottom).toBeGreaterThanOrEqual(there.bottom - 0.001);
    }
  });

  it('is 3:2 for a view chosen before shapes were kept, so a window narrower than that no longer crops it', () => {
    const narrow = { width: 900, height: 700 };
    const box = bannerBox(narrow, picture, 150, focus);
    // 150% of the 600-tall band that fits the 900-wide frame, not of the frame's 700.
    expect(box.height).toBeCloseTo(900);
  });

  it('is restated for another shape on the zoom\'s step, without moving the picture', () => {
    // Chosen at 4:1: on this header its band is 1848 × 462, so 150 drew the picture 693 tall, 69 % of the header.
    const focus = { x: 30, y: 70 };
    const old = bannerBox(wide, picture, 150, focus, 4);
    const aspect = aspectOf(wide) / 1000;
    const view = restateView(wide, picture, 150, focus, 4, aspect);
    expect(view.zoom).toBe(70);
    const again = bannerBox(wide, picture, view.zoom, view.focus, aspect);
    // Its centre stays put; the step changes its size by under 2.5 %.
    const centre = (box: Box) => ({ x: box.left + box.width / 2, y: box.top + box.height / 2 });
    expect(Math.abs(centre(again).x - centre(old).x)).toBeLessThan(wide.width / 100);
    expect(Math.abs(centre(again).y - centre(old).y)).toBeLessThan(wide.height / 100);
    expect(Math.abs(again.height - old.height) / old.height).toBeLessThan(0.025);
  });

  it("is the header's width per 1000 of its height", () => {
    expect(aspectOf({ width: 1500, height: 1000 })).toBe(1500);
    expect(aspectOf(wide)).toBe(1833);
  });
});

it('clamps a zoom to 25–500 in steps of 5', () => {
  expect(clampZoom(12)).toBe(25);
  expect(clampZoom(501)).toBe(500);
  expect(clampZoom(152)).toBe(150);
});

it('gives the zoom that covers the frame, so adjusting starts from what Home shows', () => {
  expect(coverZoom(frame, square)).toBe(200);
  // At the frame's own shape, as "Adjust the background" keeps it, the zoom is in percent of the frame's height.
  expect(coverZoom({ width: 1065, height: 750 }, square, 1065 / 750)).toBe(140);
  // A wide picture covers by its height.
  expect(coverZoom(frame, { width: 3000, height: 1000 })).toBe(100);
  // Against the band of the shape the view keeps: covering 600 × 500 takes 600 px, twice the 2:1 band's 300.
  expect(coverZoom({ width: 600, height: 500 }, square, 2)).toBe(200);
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

it('says whether a picture leaves part of the frame uncovered', () => {
  expect(leavesEdges(frame, bannerBox(frame, square, null, null))).toBe(false);
  expect(leavesEdges(frame, bannerBox(frame, square, 200, null))).toBe(false);
  expect(leavesEdges(frame, bannerBox(frame, square, 150, null))).toBe(true);
});

describe('zoomAround', () => {
  /** Where the frame point (x, y) falls on the picture drawn in `box`, as fractions of its size. */
  const onPicture = (box: ReturnType<typeof bannerBox>, x: number, y: number) => ({ x: (x - box.left) / box.width, y: (y - box.top) / box.height });

  it('keeps the part of the picture under the pointer where it is', () => {
    const before = bannerBox(frame, square, 200, { x: 50, y: 50 });
    const focus = zoomAround(frame, square, 200, { x: 50, y: 50 }, 250, { x: 600, y: 300 });
    const after = bannerBox(frame, square, 250, focus);
    expect(onPicture(after, 600, 300).x).toBeCloseTo(onPicture(before, 600, 300).x, 2);
    expect(onPicture(after, 600, 300).y).toBeCloseTo(onPicture(before, 600, 300).y, 1);
  });

  it('starts a first zoom from the covering picture, so choosing a zoom never moves it', () => {
    const cover = bannerBox(frame, square, null, { x: 30, y: 60 });
    const zoomed = bannerBox(frame, square, 200, zoomAround(frame, square, null, { x: 30, y: 60 }, 200));
    expect(zoomed.left).toBeCloseTo(cover.left, 0);
    expect(zoomed.top).toBeCloseTo(cover.top, 0);
  });

  it('keeps the point under the pointer with a shape kept too', () => {
    const narrow = { width: 600, height: 500 };
    const before = bannerBox(narrow, square, 200, { x: 50, y: 50 }, 2);
    const after = bannerBox(narrow, square, 250, zoomAround(narrow, square, 200, { x: 50, y: 50 }, 250, { x: 400, y: 200 }, 2), 2);
    expect(onPicture(after, 400, 200).x).toBeCloseTo(onPicture(before, 400, 200).x, 1);
    expect(onPicture(after, 400, 200).y).toBeCloseTo(onPicture(before, 400, 200).y, 1);
  });

  it("zooms into the middle of what shows when there's no pointer", () => {
    const before = bannerBox(frame, square, 200, { x: 30, y: 40 });
    const after = bannerBox(frame, square, 260, zoomAround(frame, square, 200, { x: 30, y: 40 }, 260));
    expect(onPicture(after, 500, 250).x).toBeCloseTo(onPicture(before, 500, 250).x, 1);
    expect(onPicture(after, 500, 250).y).toBeCloseTo(onPicture(before, 500, 250).y, 1);
  });
});
