import { describe, expect, it } from 'vitest';
import { History, HISTORY_CAP, mergeStroke } from './history';
import { boxes, boxOf, faceAt } from './layout';
import { mirrorTexel } from './mirror';
import { applyTool, floodRegion, fromHex, line, noise, Pixels, shade, toHex, type Rgba } from './paint';
import { blankSkin, starterSkin } from './starter';

const RED: Rgba = [200, 0, 0, 255];
const opts = { color: RED, mirror: false };

describe('layout', () => {
  it('head_front_is_8_8_8_8', () => {
    const front = boxOf('classic', 'head', 'base').faces.find((f) => f.name === 'front')!;
    expect([front.x, front.y, front.w, front.h]).toEqual([8, 8, 8, 8]);
    const hat = boxOf('classic', 'head', 'outer').faces.find((f) => f.name === 'front')!;
    expect([hat.x, hat.y]).toEqual([40, 8]);
  });

  it('slim_arm_front_is_3_wide', () => {
    const front = boxOf('slim', 'rightArm', 'base').faces.find((f) => f.name === 'front')!;
    expect([front.x, front.y, front.w, front.h]).toEqual([44, 20, 3, 12]);
    const back = boxOf('slim', 'leftArm', 'base').faces.find((f) => f.name === 'back')!;
    expect([back.x, back.w]).toEqual([43, 3]);
  });

  it('faceAt_unused_area_is_null', () => {
    expect(faceAt('classic', 0, 0)).toBeNull();
    expect(faceAt('classic', 60, 20)).toBeNull();
    // Slim arms leave the last column of the classic arm's back empty.
    expect(faceAt('slim', 55, 20)).toBeNull();
    expect(faceAt('classic', 55, 20)?.box.part).toBe('rightArm');
  });

  it('faces never overlap', () => {
    for (const model of ['classic', 'slim'] as const) {
      const seen = new Set<number>();
      for (const box of boxes(model)) {
        for (const f of box.faces) {
          for (let y = f.y; y < f.y + f.h; y++) {
            for (let x = f.x; x < f.x + f.w; x++) {
              expect(seen.has(y * 64 + x), `${model} ${box.part} ${box.layer} ${f.name} ${x},${y}`).toBe(false);
              seen.add(y * 64 + x);
            }
          }
        }
      }
    }
  });
});

describe('mirror', () => {
  it('mirror_head_front', () => {
    expect(mirrorTexel('classic', 8, 10)).toEqual([15, 10]);
    expect(mirrorTexel('classic', 11, 12)).toEqual([12, 12]);
    // The head's right side lands on its left side.
    expect(mirrorTexel('classic', 0, 8)).toEqual([23, 8]);
  });

  it('mirror_slim_arms', () => {
    // Right arm front (44..47) ↔ left arm front (36..39), slim.
    expect(mirrorTexel('slim', 44, 20)).toEqual([38, 52]);
    expect(mirrorTexel('slim', 38, 52)).toEqual([44, 20]);
    expect(mirrorTexel('classic', 44, 20)).toEqual([39, 52]);
  });

  it('mirror_body_back', () => {
    expect(mirrorTexel('classic', 32, 20)).toEqual([39, 20]);
  });

  it('mirrors back onto itself for every texel', () => {
    for (const model of ['classic', 'slim'] as const) {
      for (let y = 0; y < 64; y++) {
        for (let x = 0; x < 64; x++) {
          const m = mirrorTexel(model, x, y);
          if (m) expect(mirrorTexel(model, m[0], m[1])).toEqual([x, y]);
        }
      }
    }
  });
});

describe('tools', () => {
  it('pencil_writes_and_mirrors', () => {
    const px = new Pixels();
    const changes = applyTool(px, 'classic', 'pencil', 8, 10, { ...opts, mirror: true }, mirrorTexel);
    expect(changes).toHaveLength(2);
    expect(px.get(8, 10)).toEqual(RED);
    expect(px.get(15, 10)).toEqual(RED);
  });

  it('pencil outside every face changes nothing', () => {
    const px = new Pixels();
    expect(applyTool(px, 'classic', 'pencil', 0, 0, opts, mirrorTexel)).toEqual([]);
  });

  it('eraser_on_base_writes_transparent', () => {
    const px = starterSkin();
    applyTool(px, 'classic', 'eraser', 9, 9, opts, mirrorTexel);
    expect(px.get(9, 9)[3]).toBe(0);
  });

  it('fill_stays_inside_face', () => {
    const px = blankSkin();
    // The head front is one grey; the head's top and right side touch it on the texture.
    applyTool(px, 'classic', 'fill', 10, 10, opts, mirrorTexel);
    expect(px.get(8, 8)).toEqual(RED);
    expect(px.get(15, 15)).toEqual(RED);
    expect(px.get(10, 7)).not.toEqual(RED);
    expect(px.get(7, 10)).not.toEqual(RED);
    expect(px.get(16, 10)).not.toEqual(RED);
  });

  it('fill stops at a different colour', () => {
    const px = blankSkin();
    for (let y = 8; y < 16; y++) px.set(12, y, [0, 0, 0, 255]);
    expect(floodRegion(px, 'classic', 9, 9)).toHaveLength(4 * 8);
  });

  it('shade_darkens_by_step', () => {
    expect(shade([100, 200, 50, 255], true)).toEqual([88, 176, 44, 255]);
    expect(shade([100, 200, 50, 255], false)).toEqual([119, 207, 75, 255]);
    expect(shade([1, 2, 3, 0], true)).toEqual([1, 2, 3, 0]);
  });

  it('noise_is_deterministic_with_rand', () => {
    expect(noise([100, 100, 100, 255], () => 1)).toEqual([114, 114, 114, 255]);
    expect(noise([100, 100, 100, 255], () => 0)).toEqual([86, 86, 86, 255]);
  });

  it('shade touches a texel once per stroke', () => {
    const px = blankSkin();
    const touched = new Set<number>();
    const before = px.get(9, 9);
    applyTool(px, 'classic', 'shade', 9, 9, { ...opts, darken: true }, mirrorTexel, touched);
    applyTool(px, 'classic', 'shade', 9, 9, { ...opts, darken: true }, mirrorTexel, touched);
    expect(px.get(9, 9)).toEqual(shade(before, true));
  });

  it('line has no gaps', () => {
    expect(line(0, 0, 3, 1)).toEqual([[0, 0], [1, 0], [2, 1], [3, 1]]);
    expect(line(2, 2, 2, 2)).toEqual([[2, 2]]);
  });

  it('hex round trips', () => {
    expect(toHex([255, 16, 0, 255])).toBe('#ff1000');
    expect(fromHex('#ff1000')).toEqual([255, 16, 0, 255]);
    expect(fromHex('abc')).toEqual([170, 187, 204, 255]);
    expect(fromHex('nope')).toBeNull();
  });
});

describe('history', () => {
  it('undo_redo_round_trip', () => {
    const px = new Pixels();
    const h = new History();
    h.push(applyTool(px, 'classic', 'pencil', 9, 9, opts, mirrorTexel));
    expect(h.undo(px)).toBe(true);
    expect(px.get(9, 9)[3]).toBe(0);
    expect(h.redo(px)).toBe(true);
    expect(px.get(9, 9)).toEqual(RED);
    expect(h.redo(px)).toBe(false);
  });

  it('a new stroke clears redo', () => {
    const px = new Pixels();
    const h = new History();
    h.push(applyTool(px, 'classic', 'pencil', 9, 9, opts, mirrorTexel));
    h.undo(px);
    h.push(applyTool(px, 'classic', 'pencil', 10, 9, opts, mirrorTexel));
    expect(h.canRedo).toBe(false);
  });

  it('history_cap', () => {
    const px = new Pixels();
    const h = new History();
    for (let i = 0; i < HISTORY_CAP + 5; i++) h.push([{ x: 9, y: 9, before: [0, 0, 0, 0], after: [i % 255, 0, 0, 255] }]);
    let undone = 0;
    while (h.undo(px)) undone++;
    expect(undone).toBe(HISTORY_CAP);
  });

  it('mergeStroke keeps first before and last after', () => {
    const a: Rgba = [1, 1, 1, 255];
    const b: Rgba = [2, 2, 2, 255];
    const c: Rgba = [3, 3, 3, 255];
    expect(mergeStroke([{ x: 1, y: 1, before: a, after: b }, { x: 1, y: 1, before: b, after: c }])).toEqual([
      { x: 1, y: 1, before: a, after: c },
    ]);
  });
});

describe('starter skins', () => {
  it('starter_has_opaque_base_layer', () => {
    for (const model of ['classic', 'slim'] as const) {
      for (const make of [starterSkin, blankSkin]) {
        const px = make(model);
        for (const box of boxes(model).filter((b) => b.layer === 'base')) {
          for (const f of box.faces) {
            for (let y = f.y; y < f.y + f.h; y++) {
              for (let x = f.x; x < f.x + f.w; x++) expect(px.get(x, y)[3], `${box.part} ${f.name}`).toBe(255);
            }
          }
        }
        // The outer layer starts empty.
        const hat = boxOf(model, 'head', 'outer').faces[0]!;
        expect(px.get(hat.x, hat.y)[3]).toBe(0);
      }
    }
  });
});
