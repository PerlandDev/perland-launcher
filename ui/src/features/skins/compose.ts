// Layers recoloured pieces onto a skin: face slots, the player's painting, then clothes and accessories.
import { recolour, type PixelBuffer } from './recolour';

export const SLOTS: readonly string[] = ['body', 'eyes', 'brows', 'mouth', 'face_detail', 'beard', 'bottom', 'shoes', 'top', 'belt', 'hands', 'neck', 'back', 'hair', 'glasses', 'hat'];
export const FACE_SLOTS: readonly string[] = ['body', 'eyes', 'brows', 'mouth', 'face_detail', 'beard'];

export interface ComposeLayer {
  slot: string;
  /** The catalog piece this layer draws, when known. */
  id?: string;
  art: PixelBuffer;
  mask: PixelBuffer;
  colours: string[];
}

/** Alpha-over `src` onto `dst` in place. */
function over(dst: PixelBuffer, src: PixelBuffer) {
  for (let i = 0; i + 3 < dst.length; i += 4) {
    const sa = src[i + 3]! / 255;
    if (sa === 0) continue;
    const da = dst[i + 3]! / 255;
    const oa = sa + da * (1 - sa);
    for (let c = 0; c < 3; c++) {
      dst[i + c] = Math.floor((src[i + c]! * sa + dst[i + c]! * da * (1 - sa)) / oa + 0.5);
    }
    dst[i + 3] = Math.floor(oa * 255 + 0.5);
  }
}

const rank = (slot: string) => {
  const i = SLOTS.indexOf(slot);
  return i < 0 ? SLOTS.length : i;
};

/** Each layer's grey art is recoloured through its mask and colours; a fixed mask or no colours leaves it as it is. */
export function compose(painting: PixelBuffer | null, layers: readonly ComposeLayer[]): PixelBuffer {
  const out = new Uint8ClampedArray(64 * 64 * 4);
  const sorted = [...layers].sort((a, b) => rank(a.slot) - rank(b.slot));
  const face = sorted.filter((l) => FACE_SLOTS.includes(l.slot));
  const rest = sorted.filter((l) => !FACE_SLOTS.includes(l.slot));
  const paint = (l: ComposeLayer) => over(out, recolour(l.art, l.mask, l.colours));
  for (const l of face) paint(l);
  if (painting) over(out, painting);
  for (const l of rest) paint(l);
  return out;
}
