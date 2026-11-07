// Switching between classic (4-texel) and slim (3-texel) arms. The two models lay the arms out differently on the
// skin: every slim arm face after the first side sits a column or two further left. Flipping the flag alone would
// make each face show its neighbour's texels (a hand wearing the shoulder's colours), so the arms are redrawn into the
// new layout: each face keeps its rows, and its columns are resampled to the new width (keeping both edges, dropping
// or doubling an inside column). Pure, so it is tested without a screen.
import type { SkinModel } from '../../ipc/schema';
import { boxOf, faceAt, LAYERS, SKIN_SIZE, type Part } from './layout';
import { TRANSPARENT, sameColor, type Change, type Pixels, type Rgba } from './paint';

const ARMS: readonly Part[] = ['rightArm', 'leftArm'];

/** The base-layer arm texels classic arms use and slim ones don't. */
function classicOnly(): [number, number][] {
  const out: [number, number][] = [];
  for (const part of ARMS) {
    for (const face of boxOf('classic', part, 'base').faces) {
      for (let y = face.y; y < face.y + face.h; y++) {
        for (let x = face.x; x < face.x + face.w; x++) {
          const slim = faceAt('slim', x, y);
          if (!slim || slim.box.part !== part) out.push([x, y]);
        }
      }
    }
  }
  return out;
}
const CLASSIC_ONLY = classicOnly();

/**
 * Whether the texture is already drawn for `model`'s arms: a slim skin leaves the texels only classic arms use empty,
 * a classic one fills them (the base layer is opaque). Switching to a model the skin already fits only flips the flag,
 * so a skin imported with the wrong model is put right, not redrawn.
 */
export function fitsModel(px: Pixels, model: SkinModel): boolean {
  return model === 'slim' ? CLASSIC_ONLY.every(([x, y]) => px.get(x, y)[3] === 0) : CLASSIC_ONLY.every(([x, y]) => px.get(x, y)[3] > 0);
}

/** Redraws both arms (both layers) from `from`'s layout into `to`'s, and returns what changed. */
export function convertArms(px: Pixels, from: SkinModel, to: SkinModel): Change[] {
  if (from === to) return [];
  const next = new Map<number, Rgba>();
  const key = (x: number, y: number) => y * SKIN_SIZE + x;
  for (const part of ARMS) {
    for (const layer of LAYERS) {
      const src = boxOf(from, part, layer).faces;
      for (const face of src) for (let y = face.y; y < face.y + face.h; y++) for (let x = face.x; x < face.x + face.w; x++) next.set(key(x, y), TRANSPARENT);
    }
  }
  for (const part of ARMS) {
    for (const layer of LAYERS) {
      const src = boxOf(from, part, layer).faces;
      for (const face of boxOf(to, part, layer).faces) {
        const old = src.find((f) => f.name === face.name)!;
        for (let j = 0; j < face.h; j++) {
          for (let i = 0; i < face.w; i++) {
            // Sampled at texel centres: both edge columns survive, an inside one is dropped or doubled.
            const si = Math.min(old.w - 1, Math.floor(((i + 0.5) * old.w) / face.w));
            next.set(key(face.x + i, face.y + j), px.get(old.x + si, old.y + j));
          }
        }
      }
    }
  }
  const changes: Change[] = [];
  for (const [k, after] of next) {
    const x = k % SKIN_SIZE;
    const y = Math.floor(k / SKIN_SIZE);
    const before = px.get(x, y);
    if (sameColor(before, after) && before[3] === after[3]) continue;
    changes.push({ x, y, before, after });
  }
  for (const c of changes) px.set(c.x, c.y, c.after);
  return changes;
}
