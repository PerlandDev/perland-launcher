import { describe, expect, test } from 'vitest';
import type { CatalogLook, CatalogManifest, CatalogPiece } from '../../ipc/schema';
import { searchLooks, searchPieces } from './catalogSearch';

const piece = (id: string, slot: string, en: string, es: string, tags: string[] = [], retired = false): CatalogPiece => ({
  id, slot, name: { en, es }, tags, areas: [], files: {}, thumb: '', retired,
});
const look = (id: string, en: string, es: string, tags: string[] = [], retired = false): CatalogLook => ({
  id, name: { en, es }, tags, model: 'classic', pieces: [], thumb: '', retired,
});
const manifest: CatalogManifest = {
  formatVersion: 1, version: 'v', license: 'CC0-1.0', slots: [], tags: [],
  pieces: [
    piece('tee', 'top', 'Tee', 'Camiseta', ['casual']),
    piece('tank', 'top', 'Tank', 'Tirantes', ['summer']),
    piece('old', 'top', 'Old tee', 'Camiseta vieja', [], true),
    piece('cap', 'hat', 'Cap', 'Gorra'),
  ],
  looks: [look('l1', 'Pirate', 'Pirata', ['fantasy']), look('l2', 'Cafe', 'Café', ['casual']), look('l3', 'Gone', 'Ido', [], true)],
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
    expect(searchLooks(manifest, '', '', 'en').map((l) => l.id)).toEqual(['l1', 'l2']);
  });
});
