// The player's figure as a flat front view, 16 × 32 texels: what shows at once, before the 3D figure is ready, and
// all that shows where there is no WebGL. Pure, so it is tested without a canvas.
import type { SkinModel } from '../../ipc/schema';

export const FLAT_W = 16;
export const FLAT_H = 32;
const SKIN = 64;

/** Where each part's front sits in the texture ([x, y, w, h]) and on the figure ([x, y]): the skin, then the outer layer
 *  over it. Slim arms are three texels wide and stand one texel in, so the figure keeps its middle. */
function fronts(model: SkinModel): [number, number, number, number, number, number][] {
  const arm = model === 'slim' ? 3 : 4;
  return [
    [8, 8, 8, 8, 4, 0],
    [20, 20, 8, 12, 4, 8],
    [44, 20, arm, 12, 4 - arm, 8],
    [36, 52, arm, 12, 12, 8],
    [4, 20, 4, 12, 4, 20],
    [20, 52, 4, 12, 8, 20],
    [40, 8, 8, 8, 4, 0],
    [20, 36, 8, 12, 4, 8],
    [44, 36, arm, 12, 4 - arm, 8],
    [52, 52, arm, 12, 12, 8],
    [4, 36, 4, 12, 4, 20],
    [4, 52, 4, 12, 8, 20],
  ];
}

/** The front view of a 64 × 64 RGBA skin, as 16 × 32 RGBA texels (rows top first). */
export function flatFront(skin: Uint8ClampedArray, model: SkinModel): Uint8ClampedArray {
  const out = new Uint8ClampedArray(FLAT_W * FLAT_H * 4);
  for (const [sx, sy, w, h, dx, dy] of fronts(model)) {
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const from = ((sy + y) * SKIN + sx + x) * 4;
        const to = ((dy + y) * FLAT_W + dx + x) * 4;
        const alpha = skin[from + 3]! / 255;
        if (alpha === 0) continue;
        const under = out[to + 3]! / 255;
        const both = alpha + under * (1 - alpha);
        for (let k = 0; k < 3; k++) out[to + k] = Math.round((skin[from + k]! * alpha + out[to + k]! * under * (1 - alpha)) / both);
        out[to + 3] = Math.round(both * 255);
      }
    }
  }
  return out;
}
