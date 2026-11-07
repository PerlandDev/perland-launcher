import { describe, expect, it } from 'vitest';
import { bake, fur, planks, type Piece } from './props';

const pieces: Piece[] = [
  { size: [10, 10, 22], center: [0, 16, 0], paint: fur([150, 100, 60]) },
  { size: [4, 11, 4], center: [3, 5.5, 8], paint: fur([150, 100, 60]) },
  { size: [0.3, 0.3, 9], center: [2, 9, 3], paint: fur([40, 30, 20]) },
  { size: [2, 6, 28], center: [9, 6, 0], paint: planks([164, 125, 75]) },
];

describe('prop atlas', () => {
  it('packs every face into the atlas without overlaps', () => {
    const { atlas, faces } = bake(pieces);
    const seen = new Set<number>();
    for (const box of faces) {
      for (const f of box) {
        expect(f.x + f.w).toBeLessThanOrEqual(atlas.width);
        expect(f.y + f.h).toBeLessThanOrEqual(atlas.height);
        for (let y = f.y; y < f.y + f.h; y++) {
          for (let x = f.x; x < f.x + f.w; x++) {
            expect(seen.has(y * atlas.width + x), `${f.name} ${x},${y}`).toBe(false);
            seen.add(y * atlas.width + x);
          }
        }
      }
    }
  });

  it('places each box where its piece says, at its real size', () => {
    const { faces } = bake(pieces);
    const front = faces[0]!.find((f) => f.name === 'front')!;
    for (const c of front.corners) expect(c[2]).toBeCloseTo(11);
    const xs = front.corners.map((c) => c[0]);
    expect(Math.min(...xs)).toBeCloseTo(-5);
    expect(Math.max(...xs)).toBeCloseTo(5);
    // A thin piece still gets at least one texel a face.
    expect(faces[2]!.every((f) => f.w >= 1 && f.h >= 1)).toBe(true);
  });

  it('paints every face texel opaque, the same way each time', () => {
    const a = bake(pieces);
    const b = bake(pieces);
    expect(a.atlas.data).toEqual(b.atlas.data);
    for (const box of a.faces) {
      for (const f of box) {
        for (let y = f.y; y < f.y + f.h; y++) for (let x = f.x; x < f.x + f.w; x++) expect(a.atlas.data[(y * a.atlas.width + x) * 4 + 3]).toBe(255);
      }
    }
  });
});
