// Layers recoloured pieces onto a skin: face slots, the player's painting, then clothes and accessories.
import type { Rgba } from './recolour';

export const SLOTS: readonly string[] = ['body', 'eyes', 'brows', 'mouth', 'face_detail', 'beard', 'bottom', 'shoes', 'top', 'belt', 'hands', 'neck', 'back', 'hair', 'glasses', 'hat'];
export const FACE_SLOTS: readonly string[] = ['body', 'eyes', 'brows', 'mouth', 'face_detail', 'beard'];

export interface ComposeLayer {
  slot: string;
  art: Rgba;
  mask: Rgba;
  colours: string[];
}

/** Alpha-over `src` onto `dst` in place. */
function over(dst: Rgba, src: Rgba) {
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

/** `layers` carry already recoloured art in `art`; their `mask` and `colours` are not read here. */
export function compose(painting: Rgba | null, layers: readonly ComposeLayer[]): Rgba {
  const out = new Uint8ClampedArray(64 * 64 * 4);
  const sorted = [...layers].sort((a, b) => rank(a.slot) - rank(b.slot));
  const face = sorted.filter((l) => FACE_SLOTS.includes(l.slot));
  const rest = sorted.filter((l) => !FACE_SLOTS.includes(l.slot));
  for (const l of face) over(out, l.art);
  if (painting) over(out, painting);
  for (const l of rest) over(out, l.art);
  return out;
}
