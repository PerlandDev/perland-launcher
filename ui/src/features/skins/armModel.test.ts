import { describe, expect, it } from 'vitest';
import { convertArms, fitsModel } from './armModel';
import { boxOf, type Part } from './layout';
import { Pixels, type Rgba } from './paint';
import { starterSkin } from './starter';

const ARMS: Part[] = ['rightArm', 'leftArm'];

/** A classic skin whose every arm face is filled with its own colour, so a texel's source face is plain to see. */
function paintedClassic(): Pixels {
  const px = starterSkin('classic');
  let n = 0;
  for (const part of ARMS) {
    for (const layer of ['base', 'outer'] as const) {
      for (const face of boxOf('classic', part, layer).faces) {
        n += 1;
        for (let y = face.y; y < face.y + face.h; y++) for (let x = face.x; x < face.x + face.w; x++) px.set(x, y, [n * 9, x, y, 255]);
      }
    }
  }
  return px;
}

describe('arm model', () => {
  it('classic to slim moves every face to its slim place, so the hands never wear the shoulders', () => {
    const px = paintedClassic();
    const before = px.clone();
    convertArms(px, 'classic', 'slim');
    for (const part of ARMS) {
      for (const layer of ['base', 'outer'] as const) {
        const from = boxOf('classic', part, layer).faces;
        for (const face of boxOf('slim', part, layer).faces) {
          const src = from.find((f) => f.name === face.name)!;
          for (let y = face.y; y < face.y + face.h; y++) {
            for (let x = face.x; x < face.x + face.w; x++) {
              // Each slim texel comes from the same face, same row, of the classic skin.
              const [tag, , row] = px.get(x, y);
              expect(tag, `${part} ${layer} ${face.name} ${x},${y}`).toBe(before.get(src.x, src.y)[0]);
              expect(row - src.y).toBe(y - face.y);
            }
          }
        }
      }
    }
  });

  it('keeps the outside columns and drops an inside one', () => {
    const px = paintedClassic();
    const before = px.clone();
    convertArms(px, 'classic', 'slim');
    const front = boxOf('slim', 'rightArm', 'base').faces.find((f) => f.name === 'front')!; // 44..46
    const src = boxOf('classic', 'rightArm', 'base').faces.find((f) => f.name === 'front')!; // 44..47
    expect(px.get(front.x, 25)).toEqual(before.get(src.x, 25));
    expect(px.get(front.x + 2, 25)).toEqual(before.get(src.x + 3, 25));
  });

  it('clears what slim arms leave unused', () => {
    const px = paintedClassic();
    convertArms(px, 'classic', 'slim');
    expect(px.get(55, 25)[3]).toBe(0);
    expect(px.get(50, 17)[3]).toBe(0);
    expect(fitsModel(px, 'slim')).toBe(true);
  });

  it('slim and back keeps the sides exactly and gives classic its full width', () => {
    const px = paintedClassic();
    const before = px.clone();
    convertArms(px, 'classic', 'slim');
    convertArms(px, 'slim', 'classic');
    const side = boxOf('classic', 'leftArm', 'base').faces.find((f) => f.name === 'right')!;
    for (let y = side.y; y < side.y + side.h; y++) for (let x = side.x; x < side.x + side.w; x++) expect(px.get(x, y)).toEqual(before.get(x, y));
    expect(fitsModel(px, 'classic')).toBe(true);
  });

  it('returns the changes, so the switch undoes in one step', () => {
    const px = paintedClassic();
    const before = px.clone();
    const changes = convertArms(px, 'classic', 'slim');
    expect(changes.length).toBeGreaterThan(0);
    for (const c of changes) {
      expect(c.before).toEqual(before.get(c.x, c.y));
      expect(c.after).toEqual(px.get(c.x, c.y));
    }
  });

  it('tells a skin already drawn for slim arms from one drawn for classic', () => {
    expect(fitsModel(starterSkin('slim'), 'slim')).toBe(true);
    expect(fitsModel(starterSkin('classic'), 'classic')).toBe(true);
    expect(fitsModel(starterSkin('classic'), 'slim')).toBe(false);
    const hole: Rgba = [0, 0, 0, 0];
    const slimLooking = starterSkin('slim');
    slimLooking.set(54, 25, hole);
    expect(fitsModel(slimLooking, 'classic')).toBe(false);
  });
});
