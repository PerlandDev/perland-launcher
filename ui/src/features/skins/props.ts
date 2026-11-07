// The stage's props (the boat and the horse) are built the way the game builds its entity models: boxes, each
// unwrapped onto a texture atlas one texel a model unit, and every face painted texel by texel. The painting is done
// here in code, in the launcher's own colours; nothing is Mojang's. Pure, so it is tested without WebGL.
import type { Face, FaceName, Vec3 } from './layout';

export type Rgb = [number, number, number];

/** Paints one face: `put(i, j, colour)` sets the texel `i` columns in and `j` rows down, seen from outside (a top or
 *  bottom face has the box's front edge along its last row; a side's columns run back to front on the right side and
 *  front to back on the left, as on the skin). */
export type Painter = (face: Face, put: (i: number, j: number, c: Rgb) => void) => void;

export interface Piece {
  /** Width (x), height (y) and depth (z) in model units. */
  size: Vec3;
  center: Vec3;
  paint: Painter;
}

export class Atlas {
  readonly data: Uint8ClampedArray;
  constructor(
    readonly width: number,
    readonly height: number,
  ) {
    this.data = new Uint8ClampedArray(width * height * 4);
  }

  set(x: number, y: number, [r, g, b]: Rgb) {
    this.data.set([r, g, b, 255], (y * this.width + x) * 4);
  }
}

/** A small deterministic hash in [0, 1], so a prop is painted the same every time. */
export function hash(x: number, y: number, seed: number): number {
  let h = (x * 374761393 + y * 668265263 + seed * 2147483647) | 0;
  h = Math.imul(h ^ (h >>> 13), 1274126177);
  return ((h ^ (h >>> 16)) >>> 0) / 4294967295;
}

const clamp = (v: number) => Math.max(0, Math.min(255, Math.round(v)));
export const mul = (c: Rgb, k: number): Rgb => [clamp(c[0] * k), clamp(c[1] * k), clamp(c[2] * k)];
export const hex = (n: number): Rgb => [(n >> 16) & 255, (n >> 8) & 255, n & 255];

/** The faces of a box at its real size, with texture rectangles of whole texels from (u, v), laid out like a skin box. */
function unwrap(u: number, v: number, texels: Vec3, [sx, sy, sz]: Vec3, [cx, cy, cz]: Vec3): Face[] {
  const [w, h, d] = texels;
  const p = (x: number, y: number, z: number): Vec3 => [cx + (x * sx) / 2, cy + (y * sy) / 2, cz + (z * sz) / 2];
  const face = (name: FaceName, x: number, y: number, fw: number, fh: number, corners: [Vec3, Vec3, Vec3, Vec3], normal: Vec3): Face => ({
    name,
    x,
    y,
    w: fw,
    h: fh,
    corners,
    normal,
  });
  return [
    face('top', u + d, v, w, d, [p(-1, 1, -1), p(1, 1, -1), p(-1, 1, 1), p(1, 1, 1)], [0, 1, 0]),
    face('bottom', u + d + w, v, w, d, [p(-1, -1, -1), p(1, -1, -1), p(-1, -1, 1), p(1, -1, 1)], [0, -1, 0]),
    face('right', u, v + d, d, h, [p(-1, 1, -1), p(-1, 1, 1), p(-1, -1, -1), p(-1, -1, 1)], [-1, 0, 0]),
    face('front', u + d, v + d, w, h, [p(-1, 1, 1), p(1, 1, 1), p(-1, -1, 1), p(1, -1, 1)], [0, 0, 1]),
    face('left', u + d + w, v + d, d, h, [p(1, 1, 1), p(1, 1, -1), p(1, -1, 1), p(1, -1, -1)], [1, 0, 0]),
    face('back', u + d + w + d, v + d, w, h, [p(1, 1, -1), p(-1, 1, -1), p(1, -1, -1), p(-1, -1, -1)], [0, 0, -1]),
  ];
}

/** Packs `pieces` onto one atlas (rows of unwrapped boxes), paints them, and returns each piece's faces. */
export function bake(pieces: Piece[], width = 128): { atlas: Atlas; faces: Face[][] } {
  const texels = pieces.map((p) => p.size.map((s) => Math.max(1, Math.round(s))) as Vec3);
  const spots: [number, number][] = [];
  let x = 0;
  let y = 0;
  let row = 0;
  for (const [w, h, d] of texels) {
    const fw = 2 * (w + d);
    const fh = d + h;
    if (x + fw > width) {
      x = 0;
      y += row;
      row = 0;
    }
    spots.push([x, y]);
    x += fw;
    row = Math.max(row, fh);
  }
  const atlas = new Atlas(width, y + row);
  const faces = pieces.map((piece, n) => {
    const made = unwrap(spots[n]![0], spots[n]![1], texels[n]!, piece.size, piece.center);
    for (const f of made) piece.paint(f, (i, j, c) => atlas.set(f.x + i, f.y + j, c));
    return made;
  });
  return { atlas, faces };
}

/** Whether column `i` of a side face is `k` texels from the box's front edge. */
export function fromFront(face: Face, i: number): number {
  return face.name === 'right' ? face.w - 1 - i : i;
}

// The painters. Each lays a material over every texel of a face; detail goes on top with `over`.

/** A coat: lighter along the back, darker toward the belly, with a soft dapple. */
export function fur(base: Rgb, seed = 1): Painter {
  return (face, put) => {
    for (let j = 0; j < face.h; j++) {
      for (let i = 0; i < face.w; i++) {
        let k = face.name === 'top' ? 1.08 : face.name === 'bottom' ? 0.8 : 1.07 - (0.2 * j) / Math.max(1, face.h - 1);
        const r = hash(face.x + i, face.y + j, seed);
        k *= r < 0.1 ? 0.9 : r > 0.94 ? 1.07 : 0.97 + hash((face.x + i) >> 1, (face.y + j) >> 1, seed + 3) * 0.06;
        put(i, j, mul(base, k));
      }
    }
  };
}

/** Hair (mane, tail): strands running the length of the face, each its own shade. */
export function hair(base: Rgb, seed = 2): Painter {
  return (face, put) => {
    const along = face.h >= face.w;
    for (let j = 0; j < face.h; j++) {
      for (let i = 0; i < face.w; i++) {
        const strand = along ? face.x + i : face.y + j;
        put(i, j, mul(base, 0.8 + hash(strand, face.name.length, seed) * 0.32 + (hash(face.x + i, face.y + j, seed) - 0.5) * 0.08));
      }
    }
  };
}

/** Planks: boards along the face's long side, a dark seam between them, staggered butt joints, and grain streaks
 *  running along each board. */
export function planks(base: Rgb, seed = 3): Painter {
  return (face, put) => {
    const along = face.w >= face.h;
    const [len, across] = along ? [face.w, face.h] : [face.h, face.w];
    const board = across >= 8 ? 4 : 3;
    for (let b = 0; b < across; b++) {
      for (let a = 0; a < len; a++) {
        const n = Math.floor(b / board);
        const seam = b % board === board - 1 && b < across - 1;
        const joint = !seam && (a + n * 9 + 4) % 16 === 0;
        // Each board its own tone; streaks run along it, a few texels long.
        const streak = hash(Math.floor((a + n * 5) / 5), b, seed) - 0.5;
        let k = 0.92 + hash(n, seed, 11) * 0.1 + streak * 0.09;
        if (seam) k = 0.7;
        else if (joint) k = 0.78;
        const [i, j] = along ? [a, b] : [b, a];
        put(i, j, mul(base, k));
      }
    }
  };
}

/** Leather: a fine grain with a light stitch line just inside its edges. */
export function leather(base: Rgb, seed = 4): Painter {
  return (face, put) => {
    for (let j = 0; j < face.h; j++) {
      for (let i = 0; i < face.w; i++) {
        const edge = face.w >= 4 && face.h >= 4 && (i === 1 || j === 1 || i === face.w - 2 || j === face.h - 2) && (i + j) % 2 === 0;
        put(i, j, mul(base, edge ? 1.35 : 0.94 + hash(face.x + i, face.y + j, seed) * 0.1));
      }
    }
  };
}

/** Metal: a light top edge over a darker body. */
export function metal(base: Rgb): Painter {
  return (face, put) => {
    for (let j = 0; j < face.h; j++) for (let i = 0; i < face.w; i++) put(i, j, mul(base, j === 0 || face.name === 'top' ? 1.2 : 0.92));
  };
}

/** `base`, then `detail` over it. */
export function over(base: Painter, detail: Painter): Painter {
  return (face, put) => {
    base(face, put);
    detail(face, put);
  };
}
