import { describe, expect, it } from 'vitest';
import { boxes, SKIN_SIZE } from './layout';
import { geometryFor } from './stageScene';

describe('stage geometry', () => {
  it('every face samples only its own texels, never the one beside its edge', () => {
    // Exactly on a face's edge the GPU may land on either texel, and the one outside belongs to another face (on an
    // empty outer layer, a painted neighbour shows as a stray dot). The corners sit a hair inside instead.
    for (const model of ['classic', 'slim'] as const) {
      for (const box of boxes(model)) {
        const uv = geometryFor(box, [0, 0, 0]).getAttribute('uv');
        box.faces.forEach((face, i) => {
          for (let k = i * 4; k < i * 4 + 4; k++) {
            const u = uv.getX(k) * SKIN_SIZE;
            const v = uv.getY(k) * SKIN_SIZE;
            for (const [t, lo, n] of [[u, face.x, face.w], [v, face.y, face.h]] as const) {
              const inside = Math.min(t - lo, lo + n - t);
              expect(inside).toBeGreaterThan(1e-3);
              expect(inside).toBeLessThan(0.05);
            }
          }
        });
      }
    }
  });
});
