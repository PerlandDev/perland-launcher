import { describe, expect, test } from 'vitest';
import type { CatalogLook, CatalogManifest, CatalogPiece } from '../../ipc/schema';
import { browseLooks, lookShelves, noFilter, searchLooks, searchPieces, tagFacets, type LookFilter } from './catalogSearch';

const piece = (id: string, slot: string, en: string, es: string, tags: string[] = [], retired = false): CatalogPiece => ({
  id, slot, name: { en, es }, tags, areas: [], files: {}, thumb: '', retired,
});
const look = (id: string, en: string, es: string, tags: string[] = [], retired = false): CatalogLook => ({
  id, name: { en, es }, tags, model: 'classic', pieces: [], thumb: '', retired,
});
const manifest: CatalogManifest = {
  formatVersion: 1, version: 'v', license: 'CC0-1.0', slots: [],
  tags: [{ id: 'fantasy', name: { en: 'Fantasy', es: 'Fantasía' } }, { id: 'sports', name: { en: 'Sports', es: 'Deporte' } }],
  pieces: [
    piece('tee', 'top', 'Tee', 'Camiseta', ['casual']),
    piece('tank', 'top', 'Tank', 'Tirantes', ['summer']),
    piece('old', 'top', 'Old tee', 'Camiseta vieja', [], true),
    piece('cap', 'hat', 'Cap', 'Gorra'),
  ],
  looks: [look('l1', 'Pirate', 'Pirata', ['fantasy']), look('l4', 'Runner', 'Corredora', ['sports']), look('l2', 'Cafe', 'Café', ['casual']), look('l3', 'Gone', 'Ido', [], true)],
  expressions: [],
};

describe('catalog search', () => {
  test('ignores case and accents in the current language', () => {
    expect(searchPieces(manifest, 'top', 'camiseta', 'es').map((p) => p.id)).toEqual(['tee']);
    expect(searchPieces(manifest, 'top', 'TEE', 'en').map((p) => p.id)).toEqual(['tee']);
    expect(searchPieces(manifest, 'top', 'camiseta', 'en')).toEqual([]);
    expect(searchLooks(manifest, 'CAFE', '', 'es').map((l) => l.id)).toEqual(['l2']);
  });

  test('matches tag ids and filters by slot', () => {
    expect(searchPieces(manifest, 'top', 'summer', 'en').map((p) => p.id)).toEqual(['tank']);
    expect(searchPieces(manifest, 'hat', '', 'en').map((p) => p.id)).toEqual(['cap']);
    expect(searchLooks(manifest, 'fantasy', '', 'en').map((l) => l.id)).toEqual(['l1']);
  });

  test('tag filter on looks', () => {
    expect(searchLooks(manifest, '', 'casual', 'en').map((l) => l.id)).toEqual(['l2']);
  });

  test('retired pieces are hidden unless worn', () => {
    expect(searchPieces(manifest, 'top', '', 'en').map((p) => p.id)).toEqual(['tee', 'tank']);
    expect(searchPieces(manifest, 'top', '', 'en', new Set(['old'])).map((p) => p.id)).toEqual(['tee', 'tank', 'old']);
    expect(searchLooks(manifest, '', '', 'en').map((l) => l.id)).toEqual(['l1', 'l4', 'l2']);
  });

  test('matches tag names in the current language', () => {
    expect(searchLooks(manifest, 'deporte', '', 'es').map((l) => l.id)).toEqual(['l4']);
    expect(searchLooks(manifest, 'fantasia', '', 'es').map((l) => l.id)).toEqual(['l1']);
    expect(searchLooks(manifest, 'Fantasía', '', 'es').map((l) => l.id)).toEqual(['l1']);
    // The other language's tag names don't count.
    expect(searchLooks(manifest, 'deporte', '', 'en')).toEqual([]);
    // Tag ids still match in any language.
    expect(searchLooks(manifest, 'sports', '', 'es').map((l) => l.id)).toEqual(['l4']);
  });
});

describe('browsing looks', () => {
  const tag = (id: string, en: string, es: string) => ({ id, name: { en, es } });
  const slim = (l: CatalogLook): CatalogLook => ({ ...l, model: 'slim' });
  const shop: CatalogManifest = {
    ...manifest,
    // "sports" has no look that is still offered, "empty" none at all.
    tags: [tag('casual', 'Casual', 'Casual'), tag('empty', 'Empty', 'Vacío'), tag('fantasy', 'Fantasy', 'Fantasía'), tag('new', 'New', 'Nuevo'), tag('sports', 'Sports', 'Deporte')],
    looks: [
      look('wizard', 'Wizard', 'Mago', ['new', 'fantasy']),
      slim(look('elf', 'Élf', 'Elfa', ['fantasy'])),
      look('barista', 'barista', 'Barista', ['casual']),
      slim(look('zebra', 'Zebra 10', 'Cebra 10', ['new'])),
      look('zebra2', 'Zebra 9', 'Cebra 9', []),
      look('gone', 'Gone', 'Ido', ['sports'], true),
    ],
  };
  const all: LookFilter = { query: '', tag: '', arms: '', sort: 'featured' };
  const ids = (looks: CatalogLook[]) => looks.map((l) => l.id);

  test('featured keeps the catalog order; A to Z sorts by the name in the current language', () => {
    expect(ids(browseLooks(shop, all, 'en'))).toEqual(['wizard', 'elf', 'barista', 'zebra', 'zebra2']);
    // Case and accents don't count, and numbers sort as numbers.
    expect(ids(browseLooks(shop, { ...all, sort: 'name' }, 'en'))).toEqual(['barista', 'elf', 'wizard', 'zebra2', 'zebra']);
    expect(ids(browseLooks(shop, { ...all, sort: 'name' }, 'es'))).toEqual(['barista', 'zebra2', 'zebra', 'elf', 'wizard']);
  });

  test('search, tag and arms narrow together', () => {
    expect(ids(browseLooks(shop, { ...all, arms: 'slim' }, 'en'))).toEqual(['elf', 'zebra']);
    expect(ids(browseLooks(shop, { ...all, arms: 'classic', tag: 'fantasy' }, 'en'))).toEqual(['wizard']);
    expect(ids(browseLooks(shop, { ...all, query: 'zebra', arms: 'slim' }, 'en'))).toEqual(['zebra']);
    expect(ids(browseLooks(shop, { ...all, query: 'fantasia' }, 'es'))).toEqual(['wizard', 'elf']);
  });

  test('tag keys: only tags with a look, each with its count, in the catalog order', () => {
    expect(tagFacets(shop, all, 'en')).toEqual({
      total: 5,
      tags: [
        { id: 'casual', name: 'Casual', count: 1 },
        { id: 'fantasy', name: 'Fantasy', count: 2 },
        { id: 'new', name: 'New', count: 2 },
      ],
    });
    expect(tagFacets(shop, all, 'es').tags.map((t) => t.name)).toEqual(['Casual', 'Fantasía', 'Nuevo']);
  });

  test('tag keys count what the search and the arms leave, whatever tag is chosen', () => {
    const slimOnly = tagFacets(shop, { ...all, arms: 'slim', tag: 'fantasy' }, 'en');
    expect(slimOnly).toEqual({ total: 2, tags: [{ id: 'fantasy', name: 'Fantasy', count: 1 }, { id: 'new', name: 'New', count: 1 }] });
    expect(tagFacets(shop, { ...all, query: 'wiz' }, 'en').tags.map((t) => [t.id, t.count])).toEqual([['fantasy', 1], ['new', 1]]);
  });

  test('the chosen tag keeps its key even with nothing left under it', () => {
    expect(tagFacets(shop, { ...all, query: 'barista', tag: 'fantasy' }, 'en')).toEqual({
      total: 1,
      tags: [{ id: 'casual', name: 'Casual', count: 1 }, { id: 'fantasy', name: 'Fantasy', count: 0 }],
    });
  });

  test('shelves: one per tag with looks, New first, each with its count and its first looks', () => {
    const shelves = lookShelves(shop, browseLooks(shop, all, 'en'), 'en', 1);
    expect(shelves.map((s) => [s.id, s.name, s.count, ids(s.looks)])).toEqual([
      ['new', 'New', 2, ['wizard']],
      ['casual', 'Casual', 1, ['barista']],
      ['fantasy', 'Fantasy', 2, ['wizard']],
    ]);
    // Twelve by default, in the order the looks came in.
    expect(ids(lookShelves(shop, browseLooks(shop, { ...all, sort: 'name' }, 'en'), 'en')[2]!.looks)).toEqual(['elf', 'wizard']);
  });

  test('nothing searched or filtered: sorting alone still counts as everything', () => {
    expect(noFilter(all)).toBe(true);
    expect(noFilter({ ...all, sort: 'name' })).toBe(true);
    expect(noFilter({ ...all, query: '  ' })).toBe(true);
    expect(noFilter({ ...all, query: 'a' })).toBe(false);
    expect(noFilter({ ...all, tag: 'new' })).toBe(false);
    expect(noFilter({ ...all, arms: 'slim' })).toBe(false);
  });
});
