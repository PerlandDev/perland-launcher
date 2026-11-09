import { describe, expect, it } from 'vitest';
import { convertArms, fitsModel, restoreArms } from './armModel';
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

  it('every texel stays where it sits on the arm: slim arms lose the column at the outside, by the shoulder edge', () => {
    const px = paintedClassic();
    const before = px.clone();
    convertArms(px, 'classic', 'slim');
    const slim = (part: Part, name: string) => boxOf('slim', part, 'base').faces.find((f) => f.name === name)!;
    const classic = (part: Part, name: string) => boxOf('classic', part, 'base').faces.find((f) => f.name === name)!;
    // The right arm's front runs from its outside (left, seen from the front) in to the body: its first column goes.
    for (let i = 0; i < 3; i++) expect(px.get(slim('rightArm', 'front').x + i, 25)).toEqual(before.get(classic('rightArm', 'front').x + 1 + i, 25));
    // Its back runs from the body out: its last column goes.
    for (let i = 0; i < 3; i++) expect(px.get(slim('rightArm', 'back').x + i, 25)).toEqual(before.get(classic('rightArm', 'back').x + i, 25));
    // The left arm mirrors it: its front's last column goes, its back's first.
    for (let i = 0; i < 3; i++) expect(px.get(slim('leftArm', 'front').x + i, 55)).toEqual(before.get(classic('leftArm', 'front').x + i, 55));
    for (let i = 0; i < 3; i++) expect(px.get(slim('leftArm', 'back').x + i, 55)).toEqual(before.get(classic('leftArm', 'back').x + 1 + i, 55));
    // The outside face itself is the same face, moved in.
    const side = slim('rightArm', 'right');
    for (let i = 0; i < 4; i++) expect(px.get(side.x + i, 25)).toEqual(before.get(classic('rightArm', 'right').x + i, 25));
  });

  it('slim to classic widens each face at the outside, copying its outside column', () => {
    const px = starterSkin('slim');
    const front = boxOf('slim', 'rightArm', 'base').faces.find((f) => f.name === 'front')!;
    for (let i = 0; i < 3; i++) px.set(front.x + i, 25, [10 * (i + 1), 0, 0, 255]);
    convertArms(px, 'slim', 'classic');
    const wide = boxOf('classic', 'rightArm', 'base').faces.find((f) => f.name === 'front')!;
    expect([0, 1, 2, 3].map((i) => px.get(wide.x + i, 25)[0])).toEqual([10, 10, 20, 30]);
  });

  it('switching back, with the arms untouched since, gives back exactly what was there', () => {
    const px = paintedClassic();
    const before = px.clone();
    const there = convertArms(px, 'classic', 'slim');
    const back = restoreArms(px, there);
    expect(back).not.toBeNull();
    expect(px.data).toEqual(before.data);
    // Each restored texel is a change from the slim arms back, so it undoes in one step too.
    for (const c of back!) expect(c.after).toEqual(before.get(c.x, c.y));
  });

  it('switching back after painting an arm redraws instead', () => {
    const px = paintedClassic();
    const there = convertArms(px, 'classic', 'slim');
    const front = boxOf('slim', 'rightArm', 'base').faces.find((f) => f.name === 'front')!;
    px.set(front.x, front.y, [1, 2, 3, 255]);
    const snapshot = px.clone();
    expect(restoreArms(px, there)).toBeNull();
    expect(px.data).toEqual(snapshot.data);
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
