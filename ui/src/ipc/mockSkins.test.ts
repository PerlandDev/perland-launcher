import { describe, expect, it } from 'vitest';
import { createMockBackend } from './mock';
import { mockCatalog } from './mockCatalog';
import type { CatalogAnswer, SkinEntry, SkinLook } from './schema';

const PNG = 'data:image/png;base64,';

describe('mock skin catalog', () => {
  it('serves the first-batch manifest and its files', async () => {
    const { handle } = createMockBackend();
    const { manifest, status } = (await handle('skin_catalog')) as CatalogAnswer;
    expect(status).toBe('fresh');
    // The Engine's first batch: 22 pieces and 3 looks.
    expect(manifest!.pieces.map((p) => p.id)).toEqual([
      'back-cape',
      'body-plain',
      'bottom-jeans',
      'bottom-plate-greaves',
      'bottom-space-suit',
      'brows-straight',
      'eyes-round',
      'glasses-round',
      'hair-short',
      'hands-gloves',
      'hat-cap',
      'hat-knight-helm',
      'hat-space-helmet',
      'mouth-smile',
      'neck-scarf',
      'shoes-boots',
      'shoes-moon-boots',
      'shoes-sneakers',
      'top-bomber',
      'top-plate-armour',
      'top-space-suit',
      'top-tee',
    ]);
    expect(manifest!.looks.map((l) => l.id)).toEqual(['look-astronaut', 'look-knight', 'look-street']);
    const piece = manifest!.pieces[0]!;
    expect(((await handle('skin_catalog_file', { sha256: piece.thumb })) as string).startsWith(PNG)).toBe(true);
    await expect(handle('skin_catalog_file', { sha256: 'nope' })).rejects.toMatchObject({ code: 'skin_catalog_unavailable' });
  });

  it('is unavailable in the nocatalog scenario', async () => {
    const { handle } = createMockBackend({ skins: 'nocatalog' });
    expect(await handle('skin_catalog')).toEqual({ manifest: null, status: 'unavailable' });
  });

  it('saves and clears a look with its painting', async () => {
    const { handle, state } = createMockBackend();
    const skin = state.skins.entries[0]!;
    const look: SkinLook = { pieces: [{ id: 'hat-cap', colours: { main: '#2f5aa8' } }], painting: false };
    const texture = `${PNG}AAAA`;
    expect(await handle('skin_painting', { id: skin.id })).toBeNull();
    const saved = (await handle('skin_look_save', { id: skin.id, look, painting: `${PNG}BBBB`, texture })) as SkinEntry;
    expect(saved.look).toEqual({ pieces: look.pieces, painting: true });
    expect(await handle('skin_painting', { id: skin.id })).toBe(`${PNG}BBBB`);
    await expect(handle('skin_look_save', { id: skin.id, look: { pieces: [{ id: 'x', colours: { a: 'red' } }], painting: false }, painting: null, texture })).rejects.toMatchObject({ code: 'skin_invalid' });
    const cleared = (await handle('skin_look_clear', { id: skin.id })) as SkinEntry;
    expect(cleared.look).toBeUndefined();
    expect(await handle('skin_painting', { id: skin.id })).toBeNull();
  });
});

it('the mock catalog serves every file its manifest names', () => {
  const { manifest, files } = mockCatalog();
  const named = [
    ...manifest.pieces.flatMap((p) => [p.thumb, ...Object.values(p.files).flatMap((f) => [f.art, f.mask])]),
    ...manifest.looks.flatMap((l) => [l.thumb, ...(l.painting ? [l.painting] : [])]),
  ];
  for (const hash of named) expect(files.get(hash)?.startsWith(PNG), hash).toBe(true);
  expect(files.size).toBe(new Set(named).size);
});
