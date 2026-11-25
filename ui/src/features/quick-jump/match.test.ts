import { describe, expect, it } from 'vitest';
import { rank, score } from './match';

const entry = (id: string, label: string, context: string[] = [], current = false) => ({ id, label, context, current });

describe('score', () => {
  it('ignores case and accents, so a player can type Spanish without them', () => {
    expect(score('configuracion', entry('s', 'Configuración'))).not.toBeNull();
    expect(score('ÍCONOS', entry('i', 'Íconos de instancias'))).not.toBeNull();
    expect(score('idioma', entry('l', 'Idioma'))).not.toBeNull();
  });

  it('needs every word of the query, in the label or its context', () => {
    expect(score('mods asalith', entry('m', 'Mods & packs', ['Asalith Fields']))).not.toBeNull();
    expect(score('mods castle', entry('m', 'Mods & packs', ['Asalith Fields']))).toBeNull();
    expect(score('zzz', entry('m', 'Mods & packs'))).toBeNull();
  });

  it('ranks the start of a word above a match inside one', () => {
    const prefix = score('pack', entry('a', 'Resource packs'))!;
    const inside = score('ack', entry('b', 'Resource packs'))!;
    expect(prefix).toBeGreaterThan(inside);
  });

  it('matches an empty query to everything', () => {
    expect(score('  ', entry('a', 'Anything'))).toBe(0);
  });
});

describe('rank', () => {
  it('puts word-prefix matches before substring ones, whatever their order in the list', () => {
    const entries = [entry('inside', 'Graphics card'), entry('prefix', 'Card games')];
    expect(rank(entries, 'card').map((e) => e.id)).toEqual(['prefix', 'inside']);
    expect(rank([entry('sub', 'Graphics'), entry('pre', 'Phone')], 'ph').map((e) => e.id)).toEqual(['pre', 'sub']);
  });

  it("puts the current instance's results first when they match as well", () => {
    const entries = [entry('other', 'Mods & packs', ['Creative Flat']), entry('mine', 'Mods & packs', ['Asalith Fields'], true)];
    expect(rank(entries, 'mods').map((e) => e.id)).toEqual(['mine', 'other']);
  });

  it('keeps the list order otherwise, and drops what does not match', () => {
    const entries = [entry('a', 'Skins'), entry('b', 'Servers'), entry('c', 'Settings')];
    expect(rank(entries, 's').map((e) => e.id)).toEqual(['a', 'b', 'c']);
    expect(rank(entries, 'serv').map((e) => e.id)).toEqual(['b']);
  });
});
