// Where every face of the player model sits on the 64 × 64 skin, for classic (4 texel) and slim (3 texel) arms.
// The figure faces +z, so the player's right is -x. Each face's texels run left to right and top to bottom as the
// game draws them: front and side faces as seen from outside, top and bottom with the front edge at their bottom row.
import type { SkinModel } from '../../ipc/schema';

export const SKIN_SIZE = 64;

export type Part = 'head' | 'body' | 'rightArm' | 'leftArm' | 'rightLeg' | 'leftLeg';
export type Layer = 'base' | 'outer';
export type FaceName = 'front' | 'back' | 'right' | 'left' | 'top' | 'bottom';
export type Vec3 = [number, number, number];

export const PARTS: readonly Part[] = ['head', 'body', 'rightArm', 'leftArm', 'rightLeg', 'leftLeg'];
export const LAYERS: readonly Layer[] = ['base', 'outer'];

export interface Face {
  name: FaceName;
  /** The face's rectangle on the skin, in texels. */
  x: number;
  y: number;
  w: number;
  h: number;
  /** The 3D corners of the face's top-left, top-right, bottom-left and bottom-right texel corners. */
  corners: [Vec3, Vec3, Vec3, Vec3];
  /** The outward direction. */
  normal: Vec3;
}

export interface Box {
  part: Part;
  layer: Layer;
  /** Width (x), height (y) and depth (z) in texels, before inflation. */
  size: Vec3;
  /** The centre in model space (texels; the feet stand at y = 0). */
  center: Vec3;
  /** How far the outer layer stands off the base, in texels per side. */
  inflate: number;
  faces: Face[];
}

/** The texture origin of each part's box, base and outer. */
const ORIGINS: Record<Part, Record<Layer, [number, number]>> = {
  head: { base: [0, 0], outer: [32, 0] },
  body: { base: [16, 16], outer: [16, 32] },
  rightArm: { base: [40, 16], outer: [40, 32] },
  leftArm: { base: [32, 48], outer: [48, 48] },
  rightLeg: { base: [0, 16], outer: [0, 32] },
  leftLeg: { base: [16, 48], outer: [0, 48] },
};

function partSize(part: Part, model: SkinModel): Vec3 {
  switch (part) {
    case 'head':
      return [8, 8, 8];
    case 'body':
      return [8, 12, 4];
    case 'rightArm':
    case 'leftArm':
      return [model === 'slim' ? 3 : 4, 12, 4];
    default:
      return [4, 12, 4];
  }
}

function partCenter(part: Part, model: SkinModel): Vec3 {
  const arm = model === 'slim' ? 5.5 : 6;
  switch (part) {
    case 'head':
      return [0, 28, 0];
    case 'body':
      return [0, 18, 0];
    case 'rightArm':
      return [-arm, 18, 0];
    case 'leftArm':
      return [arm, 18, 0];
    case 'rightLeg':
      return [-2, 6, 0];
    case 'leftLeg':
      return [2, 6, 0];
  }
}

function faces(u: number, v: number, [w, h, d]: Vec3, [cx, cy, cz]: Vec3, inflate: number): Face[] {
  const hx = w / 2 + inflate;
  const hy = h / 2 + inflate;
  const hz = d / 2 + inflate;
  const p = (x: number, y: number, z: number): Vec3 => [cx + x * hx, cy + y * hy, cz + z * hz];
  return [
    { name: 'top', x: u + d, y: v, w, h: d, corners: [p(-1, 1, -1), p(1, 1, -1), p(-1, 1, 1), p(1, 1, 1)], normal: [0, 1, 0] },
    { name: 'bottom', x: u + d + w, y: v, w, h: d, corners: [p(-1, -1, -1), p(1, -1, -1), p(-1, -1, 1), p(1, -1, 1)], normal: [0, -1, 0] },
    { name: 'right', x: u, y: v + d, w: d, h, corners: [p(-1, 1, -1), p(-1, 1, 1), p(-1, -1, -1), p(-1, -1, 1)], normal: [-1, 0, 0] },
    { name: 'front', x: u + d, y: v + d, w, h, corners: [p(-1, 1, 1), p(1, 1, 1), p(-1, -1, 1), p(1, -1, 1)], normal: [0, 0, 1] },
    { name: 'left', x: u + d + w, y: v + d, w: d, h, corners: [p(1, 1, 1), p(1, 1, -1), p(1, -1, 1), p(1, -1, -1)], normal: [1, 0, 0] },
    { name: 'back', x: u + d + w + d, y: v + d, w, h, corners: [p(1, 1, -1), p(-1, 1, -1), p(1, -1, -1), p(-1, -1, -1)], normal: [0, 0, -1] },
  ];
}

const cache = new Map<SkinModel, Box[]>();

/** Every box of the model: six parts, each with a base and an outer layer. */
export function boxes(model: SkinModel): Box[] {
  const known = cache.get(model);
  if (known) return known;
  const made = PARTS.flatMap((part) =>
    LAYERS.map((layer): Box => {
      const size = partSize(part, model);
      const center = partCenter(part, model);
      const inflate = layer === 'base' ? 0 : part === 'head' ? 0.5 : 0.25;
      const [u, v] = ORIGINS[part][layer];
      return { part, layer, size, center, inflate, faces: faces(u, v, size, center, inflate) };
    }),
  );
  cache.set(model, made);
  return made;
}

/** The box and face texel (x, y) belongs to, or null for the skin's unused area. */
export function faceAt(model: SkinModel, x: number, y: number): { box: Box; face: Face } | null {
  for (const box of boxes(model)) {
    for (const face of box.faces) {
      if (x >= face.x && x < face.x + face.w && y >= face.y && y < face.y + face.h) return { box, face };
    }
  }
  return null;
}

/** The box of `part` on `layer`. */
export function boxOf(model: SkinModel, part: Part, layer: Layer): Box {
  return boxes(model).find((b) => b.part === part && b.layer === layer)!;
}
