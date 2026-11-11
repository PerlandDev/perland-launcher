import { describe, expect, it } from 'vitest';
import { BLINK_MAX_MS, BLINK_MIN_MS, BLINK_MS, blinkGap, blinkSeed, closedEyes } from './blink';
import { compose, type ComposeLayer } from './compose';

const SIZE = 64;
const at = (x: number, y: number) => (y * SIZE + x) * 4;
const blank = () => new Uint8ClampedArray(SIZE * SIZE * 4);
const filled = (rgba: number[]) => {
  const out = blank();
  for (let i = 0; i < out.length; i += 4) out.set(rgba, i);
  return out;
};
const NO_MASK = blank();
/** A layer drawing `rgba` on the given texels, as it is (no recolouring). */
const layer = (slot: string, texels: [number, number][], rgba: number[]): ComposeLayer => {
  const art = blank();
  for (const [x, y] of texels) art.set(rgba, at(x, y));
  return { slot, id: slot, art, mask: NO_MASK, colours: [] };
};

const SKIN = [200, 150, 120, 255];
const BODY: ComposeLayer = { slot: 'body', id: 'body', art: filled(SKIN), mask: NO_MASK, colours: [] };
/** Two eyes on the head's front (origin 8, 8): rows 4 and 5, columns 1-2 and 5-6. */
const EYE_TEXELS: [number, number][] = [4, 5].flatMap((row) => [1, 2, 5, 6].map((col): [number, number] => [8 + col, 8 + row]));
const EYES = layer('eyes', EYE_TEXELS, [20, 30, 90, 255]);
const rgba = (buffer: Uint8ClampedArray, x: number, y: number) => [...buffer.slice(at(x, y), at(x, y) + 4)];

describe('blink timing', () => {
  it('a blink is short and comes every 3 to 6 seconds', () => {
    expect(BLINK_MS).toBe(130);
    const seed = blinkSeed('skin-0a1b2c3d4e5f');
    const gaps = Array.from({ length: 200 }, (_, n) => blinkGap(seed, n));
    for (const gap of gaps) {
      expect(gap).toBeGreaterThanOrEqual(BLINK_MIN_MS);
      expect(gap).toBeLessThanOrEqual(BLINK_MAX_MS);
      expect(Number.isInteger(gap)).toBe(true);
    }
    expect(BLINK_MIN_MS).toBe(3000);
    expect(BLINK_MAX_MS).toBe(6000);
    // Not a metronome: the gaps spread over the range.
    expect(Math.max(...gaps) - Math.min(...gaps)).toBeGreaterThan(2000);
    expect(new Set(gaps).size).toBeGreaterThan(100);
  });

  it('is the same for one skin every time, and different from skin to skin', () => {
    const a = blinkSeed('skin-a');
    const b = blinkSeed('skin-b');
    expect(blinkSeed('skin-a')).toBe(a);
    expect(a).not.toBe(b);
    expect(blinkGap(a, 3)).toBe(blinkGap(a, 3));
    const run = (seed: number) => Array.from({ length: 8 }, (_, n) => blinkGap(seed, n));
    expect(run(a)).not.toEqual(run(b));
  });
});

describe('closedEyes', () => {
  it('shows skin where the eyes were, with the lowest row darkened into a lash line', () => {
    const layers = [BODY, EYES];
    const shown = compose(null, layers);
    const closed = closedEyes(shown, null, layers)!;
    expect(closed).not.toBeNull();
    // The upper eye row is plain skin; the lower one is 30% darker.
    for (const col of [1, 2, 5, 6]) {
      expect(rgba(closed, 8 + col, 12)).toEqual(SKIN);
      expect(rgba(closed, 8 + col, 13)).toEqual([140, 105, 84, 255]);
    }
    // Nothing else changed, and what it was given is as it was.
    const changed = new Set(EYE_TEXELS.map(([x, y]) => at(x, y)));
    for (let i = 0; i < closed.length; i += 4) if (!changed.has(i)) expect(closed[i]).toBe(shown[i]);
    expect(rgba(shown, 9, 12)).toEqual([20, 30, 90, 255]);
  });

  it('keeps the player’s painting and the pieces on top as they are shown', () => {
    const painting = blank();
    painting.set([10, 200, 10, 255], at(20, 20));
    const hat = layer('hat', [[8, 8], [9, 8]], [90, 0, 0, 255]);
    const layers = [BODY, EYES, hat];
    const shown = compose(painting, layers);
    const closed = closedEyes(shown, painting, layers)!;
    expect(rgba(closed, 20, 20)).toEqual([10, 200, 10, 255]);
    expect(rgba(closed, 8, 8)).toEqual([90, 0, 0, 255]);
  });

  it('is nothing with no eyes piece', () => {
    const layers = [BODY];
    expect(closedEyes(compose(null, layers), null, layers)).toBeNull();
  });

  it('is nothing when the painting covers the eyes', () => {
    const painting = filled([90, 90, 90, 255]);
    const layers = [BODY, EYES];
    expect(closedEyes(compose(painting, layers), painting, layers)).toBeNull();
  });

  it('is nothing when glasses or a hat cover the eyes', () => {
    const visor = layer('glasses', EYE_TEXELS, [0, 0, 0, 255]);
    const layers = [BODY, EYES, visor];
    expect(closedEyes(compose(null, layers), null, layers)).toBeNull();
  });

  it('blinks the eye texels that still show under see-through glasses', () => {
    const lens = layer('glasses', EYE_TEXELS, [0, 0, 0, 80]);
    const layers = [BODY, EYES, lens];
    const closed = closedEyes(compose(null, layers), null, layers);
    expect(closed).not.toBeNull();
    expect(rgba(closed!, 9, 12)).not.toEqual(rgba(compose(null, layers), 9, 12));
  });

  it('is nothing when there is no skin under the eyes to close them with', () => {
    const layers = [EYES];
    expect(closedEyes(compose(null, layers), null, layers)).toBeNull();
  });

  it('is nothing when what the stage shows is not this look (a stroke not yet under the pieces)', () => {
    const layers = [BODY, EYES];
    const shown = compose(null, layers);
    shown.set([255, 0, 0, 255], at(9, 12));
    expect(closedEyes(shown, null, layers)).toBeNull();
  });

  it('eyes drawn somewhere else on the face get their own lowest row as the lash line', () => {
    const low = layer('eyes', [[10, 14], [13, 14]], [0, 0, 0, 255]);
    const layers = [BODY, low];
    const closed = closedEyes(compose(null, layers), null, layers)!;
    expect(rgba(closed, 10, 14)).toEqual([140, 105, 84, 255]);
    expect(rgba(closed, 13, 14)).toEqual([140, 105, 84, 255]);
  });
});
