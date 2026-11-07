// Mirror painting: a texel's twin on the other side of the figure. Right arm ↔ left arm, right leg ↔ left leg, a part's
// right side ↔ its left side; head and body mirror onto themselves. The column flips within the face and the row stays.
import type { SkinModel } from '../../ipc/schema';
import { boxOf, faceAt, type FaceName, type Part } from './layout';

const PART_TWIN: Record<Part, Part> = {
  head: 'head',
  body: 'body',
  rightArm: 'leftArm',
  leftArm: 'rightArm',
  rightLeg: 'leftLeg',
  leftLeg: 'rightLeg',
};

const FACE_TWIN: Record<FaceName, FaceName> = {
  front: 'front',
  back: 'back',
  top: 'top',
  bottom: 'bottom',
  right: 'left',
  left: 'right',
};

export function mirrorTexel(model: SkinModel, x: number, y: number): [number, number] | null {
  const at = faceAt(model, x, y);
  if (!at) return null;
  const { box, face } = at;
  const twin = boxOf(model, PART_TWIN[box.part], box.layer).faces.find((f) => f.name === FACE_TWIN[face.name])!;
  const col = x - face.x;
  const row = y - face.y;
  return [twin.x + (face.w - 1 - col), twin.y + row];
}
