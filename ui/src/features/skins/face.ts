// A skin's face as the wardrobe and the expression chips show it: the head's front, 8 × 8 texels, with the outer
// (hat) layer over the base. `composeFace` works it out for a look without composing the other 4,000 texels.
import { compose, over, type ComposeLayer } from './compose';
import type { PixelBuffer } from './recolour';

export const FACE_SIZE = 8;
const SKIN = 64;
const TEXELS = FACE_SIZE * FACE_SIZE;
/** Where the head's front sits on the skin: the base layer's, then the outer layer's. */
const FRONTS: readonly (readonly [number, number])[] = [
  [8, 8],
  [40, 8],
];

/** The two head fronts of a 64 × 64 buffer, one after the other. */
function fronts(buffer: PixelBuffer): PixelBuffer {
  const out = new Uint8ClampedArray(FRONTS.length * TEXELS * 4);
  FRONTS.forEach(([x0, y0], n) => {
    for (let y = 0; y < FACE_SIZE; y++) {
      const from = ((y0 + y) * SKIN + x0) * 4;
      out.set(buffer.subarray(from, from + FACE_SIZE * 4), (n * TEXELS + y * FACE_SIZE) * 4);
    }
  });
  return out;
}

function flatten(both: PixelBuffer): PixelBuffer {
  const face = both.slice(0, TEXELS * 4);
  over(face, both.subarray(TEXELS * 4));
  return face;
}

/** The face of a finished skin. */
export function faceOf(skin: PixelBuffer): PixelBuffer {
  return flatten(fronts(skin));
}

/** The face `compose(painting, layers)` would have. */
export function composeFace(painting: PixelBuffer | null, layers: readonly ComposeLayer[]): PixelBuffer {
  const cut = layers.map((l): ComposeLayer => ({ ...l, art: fronts(l.art), mask: fronts(l.mask) }));
  return flatten(compose(painting && fronts(painting), cut, FRONTS.length * TEXELS));
}
