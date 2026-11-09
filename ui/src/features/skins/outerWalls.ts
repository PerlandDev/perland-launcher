// The outer layer in 3D: every painted texel of the outer layer stands on the body as a small block, not a sheet
// floating off it. The outer box's own faces are the blocks' tops; this adds their sides, running from the outer
// surface back to the body wherever a painted texel borders an empty one (on its own face or round the box's edge).
// Pure, so it is tested without WebGL.
import type { Box, Face, Vec3 } from './layout';

export interface Wall {
  /** Outer edge start, outer edge end, then the same two pulled back to the body. */
  corners: [Vec3, Vec3, Vec3, Vec3];
  /** The direction the wall faces, away from its texel. */
  normal: Vec3;
  /** The skin texel the wall is the side of. */
  texel: [number, number];
}

const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
const scale = (a: Vec3, k: number): Vec3 => [a[0] * k, a[1] * k, a[2] * k];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const unit = (a: Vec3): Vec3 => scale(a, 1 / Math.hypot(...a));
const same = (a: Vec3, b: Vec3) => a[0] === b[0] && a[1] === b[1] && a[2] === b[2];

/** The point at (u, v) of a face, both 0–1 from its top-left corner. */
function at(face: Face, u: number, v: number): Vec3 {
  const [lt, rt, lb] = face.corners;
  return add(lt, add(scale(sub(rt, lt), u), scale(sub(lb, lt), v)));
}

/** The skin texel of `face` under a point on it. */
function texelOf(face: Face, p: Vec3): [number, number] {
  const [lt, rt, lb] = face.corners;
  const across = sub(rt, lt);
  const down = sub(lb, lt);
  const u = dot(sub(p, lt), across) / dot(across, across);
  const v = dot(sub(p, lt), down) / dot(down, down);
  const i = Math.min(face.w - 1, Math.max(0, Math.floor(u * face.w)));
  const j = Math.min(face.h - 1, Math.max(0, Math.floor(v * face.h)));
  return [face.x + i, face.y + j];
}

/** The walls of `box` (an outer-layer box) where `solid` texels meet empty ones. */
export function outerWalls(box: Box, solid: (x: number, y: number) => boolean): Wall[] {
  const depth = box.inflate;
  if (depth <= 0) return [];
  const walls: Wall[] = [];
  for (const face of box.faces) {
    const [lt, rt, lb] = face.corners;
    const right = unit(sub(rt, lt));
    const down = unit(sub(lb, lt));
    const back = scale(face.normal, -depth);
    for (let j = 0; j < face.h; j++) {
      for (let i = 0; i < face.w; i++) {
        const x = face.x + i;
        const y = face.y + j;
        if (!solid(x, y)) continue;
        const u0 = i / face.w;
        const u1 = (i + 1) / face.w;
        const v0 = j / face.h;
        const v1 = (j + 1) / face.h;
        const edges: { a: Vec3; b: Vec3; dir: Vec3; di: number; dj: number }[] = [
          { a: at(face, u0, v0), b: at(face, u0, v1), dir: scale(right, -1), di: -1, dj: 0 },
          { a: at(face, u1, v1), b: at(face, u1, v0), dir: right, di: 1, dj: 0 },
          { a: at(face, u1, v0), b: at(face, u0, v0), dir: scale(down, -1), di: 0, dj: -1 },
          { a: at(face, u0, v1), b: at(face, u1, v1), dir: down, di: 0, dj: 1 },
        ];
        for (const { a, b, dir, di, dj } of edges) {
          const ni = i + di;
          const nj = j + dj;
          let next: [number, number];
          if (ni >= 0 && nj >= 0 && ni < face.w && nj < face.h) {
            next = [face.x + ni, face.y + nj];
          } else {
            // Round the box's edge: the face that carries on is the one facing the way this edge points.
            const turn = box.faces.find((f) => same(f.normal, dir.map(Math.round) as Vec3));
            if (!turn) continue;
            const mid = scale(add(a, b), 0.5);
            next = texelOf(turn, add(mid, scale(face.normal, -1e-3)));
          }
          if (solid(next[0], next[1])) continue;
          walls.push({ corners: [a, b, add(a, back), add(b, back)], normal: dir, texel: [x, y] });
        }
      }
    }
  }
  return walls;
}
