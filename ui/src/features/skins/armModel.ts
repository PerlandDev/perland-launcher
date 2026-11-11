// Switching between classic (4-texel) and slim (3-texel) arms. The two models lay the arms out differently on the
// skin: every slim arm face after the first side sits a column or two further left. Flipping the flag alone would
// make each face show its neighbour's texels (a hand wearing the shoulder's colours), so the arms are redrawn into the
// new layout. Both arms keep their inside edge against the body and differ only at the outside, so every texel keeps
// its place on the arm: slim arms drop each face's column at the outside, and classic ones widen there by copying it.
// Switching straight back gives the dropped columns back (see restoreArms). Pure, so it is tested without a screen.
import type { SkinModel } from '../../ipc/schema';
import { boxOf, faceAt, LAYERS, SKIN_SIZE, type Face, type Part, type Vec3 } from './layout';
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

/** Where column `i` of `face` sits on the figure: the middle of its top edge. */
function columnAt(face: Face, i: number): Vec3 {
  const [lt, rt] = face.corners;
  const k = (i + 0.5) / face.w;
  return [lt[0] + (rt[0] - lt[0]) * k, lt[1] + (rt[1] - lt[1]) * k, lt[2] + (rt[2] - lt[2]) * k];
}

/** For each column of `to`, the column of `from` (the same face of the other model) nearest it on the figure. */
function columnMap(from: Face, to: Face): number[] {
  const sources = Array.from({ length: from.w }, (_, i) => columnAt(from, i));
  return Array.from({ length: to.w }, (_, i) => {
    const at = columnAt(to, i);
    let best = 0;
    let bestDistance = Infinity;
    sources.forEach((p, j) => {
      const distance = Math.hypot(p[0] - at[0], p[1] - at[1], p[2] - at[2]);
      // Ties go to the column nearer the body, which both models share.
      if (distance < bestDistance - 1e-9) {
        best = j;
        bestDistance = distance;
      }
    });
    return best;
  });
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
    // Placed by the base boxes: the outer layer's sit on the same columns, a little larger.
    const fromBase = boxOf(from, part, 'base').faces;
    const toBase = boxOf(to, part, 'base').faces;
    for (const layer of LAYERS) {
      const src = boxOf(from, part, layer).faces;
      boxOf(to, part, layer).faces.forEach((face, f) => {
        const old = src.find((s) => s.name === face.name)!;
        const columns = columnMap(fromBase.find((s) => s.name === face.name)!, toBase[f]!);
        for (let j = 0; j < face.h; j++) {
          for (let i = 0; i < face.w; i++) next.set(key(face.x + i, face.y + j), px.get(old.x + columns[i]!, old.y + j));
        }
      });
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

/**
 * Undoes a switch of arm model made with `changes` (what convertArms returned), when every texel it changed still
 * holds what it put there: switching straight back then loses nothing. Returns the changes made, or null (and changes
 * nothing) when the arms were painted since.
 */
export function restoreArms(px: Pixels, changes: Change[]): Change[] | null {
  const same = (a: Rgba, b: Rgba) => sameColor(a, b) && a[3] === b[3];
  if (!changes.every((c) => same(px.get(c.x, c.y), c.after))) return null;
  const back = changes.map((c) => ({ x: c.x, y: c.y, before: c.after, after: c.before }));
  for (const c of back) px.set(c.x, c.y, c.after);
  return back;
}
