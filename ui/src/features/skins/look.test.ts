import { describe, expect, test } from 'vitest';
import type { CatalogExpression, CatalogLook, CatalogPiece, SkinLook } from '../../ipc/schema';
import { applyExpression, applyLook, bakeAll, expressionPieces, putOn, setColour, takeOff, wearsExpression } from './look';

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
    // Wearing the same piece again leaves one of it.
    expect(putOn(putOn(empty, a, [a]), a, [a]).pieces).toHaveLength(1);
  });

  test('putOn keeps the colours of matching areas of the piece it replaces', () => {
    const short = piece('hair-short', 'hair', [['main', '#4b2f1f'], ['tips', '#7a5236']]);
    const long = piece('hair-long', 'hair', [['main', '#e0c068'], ['tips', '#f0e0a0'], ['band', '#cc0000']]);
    const round = piece('eyes-round', 'eyes', [['iris', '#4a3324']]);
    const cat = piece('eyes-cat', 'eyes', [['iris', '#b9d232']]);
    const closed = piece('eyes-closed', 'eyes', [['lash', '#1e1310']]);
    const all = [short, long, round, cat, closed];
    // Hair and face colours are the character's: they carry even when they are the old piece's defaults.
    const worn: SkinLook = { pieces: [{ id: 'hair-short', colours: { main: '#ff00aa', tips: '#7a5236' } }, { id: 'eyes-round', colours: { iris: '#4a3324' } }], painting: false };
    const swapped = putOn(putOn(worn, long, all), cat, all);
    expect(swapped.pieces).toEqual([
      { id: 'hair-long', colours: { main: '#ff00aa', tips: '#7a5236', band: '#cc0000' } },
      { id: 'eyes-cat', colours: { iris: '#4a3324' } },
    ]);
    // No matching area: the default. And the colour is not remembered past a piece without the area.
    const shut = putOn(swapped, closed, all);
    expect(shut.pieces[1]).toEqual({ id: 'eyes-closed', colours: { lash: '#1e1310' } });
    expect(putOn(shut, cat, all).pieces[1]).toEqual({ id: 'eyes-cat', colours: { iris: '#b9d232' } });
    // A colour the worn piece never recorded counts as its default.
    expect(putOn({ pieces: [{ id: 'eyes-round', colours: {} }], painting: false }, cat, all).pieces[0]!.colours).toEqual({ iris: '#4a3324' });
  });

  test('putOn carries a colour the player chose for clothes, not the old piece\'s own defaults', () => {
    const tee = piece('tee', 'top', [['main', '#e8e4dc'], ['trim', '#3a6ea5']]);
    const bomber = piece('bomber', 'top', [['main', '#4a5d3a'], ['trim', '#2b2b2b'], ['zip', '#c9c9c9']]);
    const all = [tee, bomber];
    // Untouched: the new top looks like its thumbnail.
    expect(putOn(putOn(empty, tee, all), bomber, all).pieces).toEqual([{ id: 'bomber', colours: { main: '#4a5d3a', trim: '#2b2b2b', zip: '#c9c9c9' } }]);
    // A chosen main survives the new top; the untouched trim takes the new top's default.
    const red = setColour(putOn(empty, tee, all), 'tee', 'main', '#FF0000');
    expect(putOn(red, bomber, all).pieces).toEqual([{ id: 'bomber', colours: { main: '#FF0000', trim: '#2b2b2b', zip: '#c9c9c9' } }]);
    // Putting the worn piece on again keeps it as it is.
    expect(putOn(red, tee, all).pieces).toEqual([{ id: 'tee', colours: { main: '#FF0000', trim: '#3a6ea5' } }]);
  });

  test('putOn on an empty slot matches the hair: brows and beard take the hair colour, hair takes the brows', () => {
    const hair = piece('hair-short', 'hair', [['main', '#4b2f1f'], ['tips', '#7a5236']]);
    const brows = piece('brows-straight', 'brows', [['main', '#3b2618']]);
    const beard = piece('beard-full', 'beard', [['main', '#3b2618'], ['tips', '#555555']]);
    const tee = piece('tee', 'top', [['main', '#e8e4dc']]);
    const all = [hair, brows, beard, tee];
    const blond: SkinLook = { pieces: [{ id: 'hair-short', colours: { main: '#e0c068', tips: '#f0e0a0' } }], painting: false };
    expect(putOn(blond, brows, all).pieces[1]).toEqual({ id: 'brows-straight', colours: { main: '#e0c068' } });
    // Only `main` is matched: the beard's other areas keep their defaults.
    expect(putOn(blond, beard, all).pieces[1]).toEqual({ id: 'beard-full', colours: { main: '#e0c068', tips: '#555555' } });
    // No hair, but brows: new hair starts from them.
    const browsOnly: SkinLook = { pieces: [{ id: 'brows-straight', colours: { main: '#aa5500' } }], painting: false };
    expect(putOn(browsOnly, hair, all).pieces[1]).toEqual({ id: 'hair-short', colours: { main: '#aa5500', tips: '#7a5236' } });
    // The hair wins over the brows for a beard, and the brows stand in when there is no hair.
    expect(putOn(putOn(blond, brows, all), beard, all).pieces[2]!.colours.main).toBe('#e0c068');
    expect(putOn(browsOnly, beard, all).pieces[1]!.colours.main).toBe('#aa5500');
    // Nothing to match, or a slot that isn't hair: the defaults.
    expect(putOn(empty, brows, all).pieces[0]!.colours).toEqual({ main: '#3b2618' });
    expect(putOn(blond, tee, all).pieces[1]!.colours).toEqual({ main: '#e8e4dc' });
    // Replacing worn brows carries their own colour, not the hair's.
    const dark = putOn(blond, brows, all);
    const thick = piece('brows-thick', 'brows', [['main', '#000000']]);
    expect(putOn(setColour(dark, 'brows-straight', 'main', '#101010'), thick, [...all, thick]).pieces[1]).toEqual({ id: 'brows-thick', colours: { main: '#101010' } });
  });

  test('applyExpression puts the three face pieces on and keeps the player\'s colours', () => {
    const round = piece('eyes-round', 'eyes', [['iris', '#4a3324']]);
    const fierce = piece('eyes-fierce', 'eyes', [['iris', '#b8651e']]);
    const straight = piece('brows-straight', 'brows', [['main', '#3b2618']]);
    const angry = piece('brows-angry', 'brows', [['main', '#3b2618']]);
    const smile = piece('mouth-smile', 'mouth', []);
    const gritted = piece('mouth-gritted', 'mouth', []);
    const tee = piece('tee', 'top');
    const all = [round, fierce, straight, angry, smile, gritted, tee];
    const expr = (id: string, eyes: string, brows: string, mouth: string): CatalogExpression => ({ id, name: { en: id, es: id }, eyes, brows, mouth });
    const mad = expr('expr-angry', 'eyes-fierce', 'brows-angry', 'mouth-gritted');
    const worn: SkinLook = {
      pieces: [{ id: 'eyes-round', colours: { iris: '#2266cc' } }, { id: 'tee', colours: { main: '#ff0000' } }, { id: 'brows-straight', colours: { main: '#e0c068' } }, { id: 'mouth-smile', colours: {} }],
      painting: true,
    };
    expect(wearsExpression(worn, mad)).toBe(false);
    const out = applyExpression(worn, mad, all);
    expect(out).toEqual({
      pieces: [{ id: 'tee', colours: { main: '#ff0000' } }, { id: 'eyes-fierce', colours: { iris: '#2266cc' } }, { id: 'brows-angry', colours: { main: '#e0c068' } }, { id: 'mouth-gritted', colours: {} }],
      painting: true,
    });
    expect(wearsExpression(out, mad)).toBe(true);
    // Already worn: nothing changes, so there is nothing to undo.
    expect(applyExpression(out, mad, all)).toBe(out);
    // A face with no pieces yet gets all three; one sharing pieces with the expression only the others.
    expect(applyExpression(empty, mad, all).pieces.map((p) => p.id)).toEqual(['eyes-fierce', 'brows-angry', 'mouth-gritted']);
    const determined = expr('expr-determined', 'eyes-round', 'brows-angry', 'mouth-smile');
    expect(applyExpression(worn, determined, all).pieces).toEqual([
      { id: 'eyes-round', colours: { iris: '#2266cc' } },
      { id: 'tee', colours: { main: '#ff0000' } },
      { id: 'mouth-smile', colours: {} },
      { id: 'brows-angry', colours: { main: '#e0c068' } },
    ]);
  });

  test('an expression naming a piece the catalog lacks, retired or of another slot is not offered', () => {
    const round = piece('eyes-round', 'eyes', [['iris', '#4a3324']]);
    const straight = piece('brows-straight', 'brows');
    const smile = piece('mouth-smile', 'mouth', []);
    const old = { ...piece('mouth-old', 'mouth', []), retired: true };
    const all = [round, straight, smile, old];
    const expr = (eyes: string, brows: string, mouth: string): CatalogExpression => ({ id: 'e', name: { en: 'e', es: 'e' }, eyes, brows, mouth });
    expect(expressionPieces(expr('eyes-round', 'brows-straight', 'mouth-smile'), all)).toEqual([round, straight, smile]);
    expect(expressionPieces(expr('eyes-round', 'brows-straight', 'mouth-gone'), all)).toBeNull();
    expect(expressionPieces(expr('eyes-round', 'brows-straight', 'mouth-old'), all)).toBeNull();
    expect(expressionPieces(expr('brows-straight', 'eyes-round', 'mouth-smile'), all)).toBeNull();
    const worn: SkinLook = { pieces: [{ id: 'eyes-round', colours: {} }], painting: false };
    expect(applyExpression(worn, expr('eyes-round', 'brows-straight', 'mouth-gone'), all)).toBe(worn);
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
