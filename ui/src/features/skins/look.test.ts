import { describe, expect, test } from 'vitest';
import type { CatalogLook, CatalogPiece, SkinLook } from '../../ipc/schema';
import { applyLook, bakeAll, putOn, setColour, takeOff } from './look';

const piece = (id: string, slot: string, areas: [string, string][] = [['main', '#112233']]): CatalogPiece => ({
  id,
  slot,
  name: { en: id, es: id },
  tags: [],
  areas: areas.map(([a, d]) => ({ id: a, name: { en: a, es: a }, default: d })),
  files: {},
  thumb: '',
  retired: false,
});
const empty: SkinLook = { pieces: [], painting: true };

describe('look', () => {
  test('putOn starts with the area defaults', () => {
    const l = putOn(empty, piece('tee', 'top', [['main', '#112233'], ['trim', '#ffffff']]), []);
    expect(l.pieces).toEqual([{ id: 'tee', colours: { main: '#112233', trim: '#ffffff' } }]);
    expect(l.painting).toBe(true);
  });

  test('putOn replaces the piece in the same slot', () => {
    const a = piece('tee', 'top');
    const b = piece('hoodie', 'top');
    const hat = piece('cap', 'hat');
    const all = [a, b, hat];
    const l = putOn(putOn(putOn(empty, a, all), hat, all), b, all);
    expect(l.pieces.map((p) => p.id).sort()).toEqual(['cap', 'hoodie']);
    // wearing the same piece again resets it,
    expect(putOn(putOn(empty, a, [a]), a, [a]).pieces).toHaveLength(1);
  });

  test('takeOff and setColour', () => {
    const l = putOn(putOn(empty, piece('tee', 'top'), []), piece('cap', 'hat'), []);
    expect(takeOff(l, 'tee').pieces.map((p) => p.id)).toEqual(['cap']);
    expect(setColour(l, 'tee', 'main', '#ff0000').pieces[0]!.colours.main).toBe('#ff0000');
    expect(l.pieces[0]!.colours.main).toBe('#112233');
  });

  test('applyLook keeps the painting', () => {
    const cl: CatalogLook = { id: 'x', name: { en: 'x', es: 'x' }, tags: [], model: 'classic', pieces: [{ id: 'cap', colours: { main: '#000000' } }], thumb: '', retired: false };
    const out = applyLook({ pieces: [{ id: 'tee', colours: {} }], painting: true }, cl);
    expect(out).toEqual({ pieces: [{ id: 'cap', colours: { main: '#000000' } }], painting: true });
    expect(applyLook({ pieces: [], painting: false }, cl).painting).toBe(false);
  });

  test('bakeAll leaves no pieces and the composed pixels as painting', () => {
    const composed = new Uint8ClampedArray(4).fill(7);
    const out = bakeAll(null, composed);
    expect(out.painting).toBe(composed);
    expect(out.look).toEqual({ pieces: [], painting: true });
  });
});
