// The figure seen flat from the front, 16 × 32 texels: the history's tiny versions, and the stage while its 3D
// scene is still being made.
import type { SkinModel } from '../../ipc/schema';
import { SKIN_SIZE } from './layout';
import type { Pixels } from './paint';

export const FRONT_W = 16;
export const FRONT_H = 32;

/** Where each part's front face sits in the texture ([x, y, w, h]) and on the 16 × 32 figure ([x, y]), base then outer. */
export function frontParts(model: SkinModel): [number, number, number, number, number, number][] {
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

/** The front view's RGBA texels (rows top first), the outer layer drawn over the base as the game blends it. */
export function frontView(skin: Pixels, model: SkinModel): Uint8ClampedArray<ArrayBuffer> {
  const out = new Uint8ClampedArray(FRONT_W * FRONT_H * 4);
  const src = skin.data;
  for (const [sx, sy, w, h, dx, dy] of frontParts(model)) {
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const s = ((sy + y) * SKIN_SIZE + sx + x) * 4;
        const d = ((dy + y) * FRONT_W + dx + x) * 4;
        const a = src[s + 3]! / 255;
        if (a === 0) continue;
        const under = out[d + 3]! / 255;
        const alpha = a + under * (1 - a);
        for (let k = 0; k < 3; k++) out[d + k] = (src[s + k]! * a + out[d + k]! * under * (1 - a)) / alpha;
        out[d + 3] = alpha * 255;
      }
    }
  }
  return out;
}
