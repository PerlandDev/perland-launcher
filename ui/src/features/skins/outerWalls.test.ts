import { describe, expect, it } from 'vitest';
import { boxOf } from './layout';
import { outerWalls } from './outerWalls';

const hat = boxOf('classic', 'head', 'outer');
const front = hat.faces.find((f) => f.name === 'front')!; // 40..47, 8..15
const side = hat.faces.find((f) => f.name === 'left')!; // 48..55, 8..15: the player's left, at +x
const solidSet = (texels: [number, number][]) => {
  const set = new Set(texels.map(([x, y]) => y * 64 + x));
  return (x: number, y: number) => set.has(y * 64 + x);
};

describe('outer walls', () => {
  it('a lone texel stands on the base as a little block: four walls, as deep as the gap', () => {
    const walls = outerWalls(hat, solidSet([[43, 11]]));
    expect(walls).toHaveLength(4);
    for (const wall of walls) {
      expect(wall.texel).toEqual([43, 11]);
      // The walls run from the outer surface (z = 4.5) back to the head (z = 4).
      const zs = wall.corners.map((c) => c[2]);
      expect(Math.max(...zs)).toBeCloseTo(4.5);
      expect(Math.min(...zs)).toBeCloseTo(4);
      expect(wall.normal[2]).toBeCloseTo(0);
    }
  });

  it('neighbours share no wall', () => {
    expect(outerWalls(hat, solidSet([[43, 11], [44, 11]]))).toHaveLength(6);
  });

  it('a texel on the edge closes its side unless the next face carries on', () => {
    // The front's last column meets the +x side's first column.
    const edge: [number, number] = [front.x + front.w - 1, 11];
    expect(outerWalls(hat, solidSet([edge]))).toHaveLength(4);
    const around: [number, number] = [side.x, 11];
    const walls = outerWalls(hat, solidSet([edge, around]));
    expect(walls.filter((w) => w.texel[0] === edge[0])).toHaveLength(3);
    expect(walls.filter((w) => w.texel[0] === around[0])).toHaveLength(3);
  });

  it('a fully painted layer is a closed shell with no walls', () => {
    expect(outerWalls(hat, () => true)).toEqual([]);
  });
});
