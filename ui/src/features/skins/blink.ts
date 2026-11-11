// The figure blinks: when the open skin wears a catalog eyes piece that shows, the stage swaps its texture for a
// closed-eyes frame for a moment every few seconds. The frame is built from data, never from new art: the skin
// without the eyes piece where the eyes were, with their lowest row darkened into a lash line. Only the stage ever
// shows it; the document, the flat texture and the saved files never do. Pure, so it is tested without WebGL.
import { compose, type ComposeLayer } from './compose';
import type { PixelBuffer } from './recolour';

/** How long the eyes stay shut. */
export const BLINK_MS = 130;
/** A blink comes this long after the one before, somewhere between the two. */
export const BLINK_MIN_MS = 3000;
export const BLINK_MAX_MS = 6000;
/** How much darker the lash line is than the skin under it. */
const LASH = 0.7;

const SKIN = 64;
/** The head's front on the base layer: where a lash line can go. */
const FACE = { x: 8, y: 8, size: 8 };

/** A skin's own blink rhythm: figures of different skins don't blink together, and one skin always blinks alike. */
export function blinkSeed(id: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < id.length; i++) h = Math.imul(h ^ id.charCodeAt(i), 0x01000193) >>> 0;
  return h;
}

/** The wait before blink `n` (from 0) of a skin, in whole milliseconds. */
export function blinkGap(seed: number, n: number): number {
  let h = (seed ^ Math.imul(n + 1, 0x9e3779b1)) >>> 0;
  h = Math.imul(h ^ (h >>> 15), 0x2c1b3c6d) >>> 0;
  h = Math.imul(h ^ (h >>> 12), 0x297a2d39) >>> 0;
  h = (h ^ (h >>> 15)) >>> 0;
  return BLINK_MIN_MS + Math.round((h / 0xffffffff) * (BLINK_MAX_MS - BLINK_MIN_MS));
}

/**
 * The closed-eyes frame of the skin `shown` on the stage, which is `compose(painting, layers)`; null when there is
 * nothing to blink: no eyes piece, eyes that don't show (the painting, glasses or a hat cover them: the skin is the
 * same with and without them), no skin under the eyes, or a stage that shows something else where the eyes are.
 * Only the texels the eyes change differ from `shown`.
 */
export function closedEyes(shown: PixelBuffer, painting: PixelBuffer | null, layers: readonly ComposeLayer[]): PixelBuffer | null {
  if (!layers.some((l) => l.slot === 'eyes')) return null;
  const open = compose(painting, layers);
  const shut = compose(
    painting,
    layers.filter((l) => l.slot !== 'eyes'),
  );
  const eyes: number[] = [];
  for (let i = 0; i < open.length; i += 4) {
    if (open[i] === shut[i] && open[i + 1] === shut[i + 1] && open[i + 2] === shut[i + 2] && open[i + 3] === shut[i + 3]) continue;
    // Without skin under them, shut eyes would be a hole in the head.
    if (shut[i + 3] === 0) return null;
    for (let c = i; c < i + 4; c++) if (shown[c] !== open[c]) return null;
    eyes.push(i);
  }
  if (eyes.length === 0) return null;
  const closed = new Uint8ClampedArray(shown);
  for (const i of eyes) closed.set(shut.subarray(i, i + 4), i);
  // The lash line: the lowest row of the face the eyes reach.
  const onFace = eyes.filter((i) => {
    const x = (i / 4) % SKIN;
    const y = Math.floor(i / 4 / SKIN);
    return x >= FACE.x && x < FACE.x + FACE.size && y >= FACE.y && y < FACE.y + FACE.size;
  });
  const lowest = Math.max(...onFace.map((i) => Math.floor(i / 4 / SKIN)));
  for (const i of onFace) {
    if (Math.floor(i / 4 / SKIN) !== lowest) continue;
    for (let c = i; c < i + 3; c++) closed[c] = Math.round(closed[c]! * LASH);
  }
  return closed;
}
