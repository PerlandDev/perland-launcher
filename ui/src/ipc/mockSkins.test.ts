import { describe, expect, it } from 'vitest';
import { createMockBackend } from './mock';
import type { CatalogAnswer, SkinEntry, SkinLook } from './schema';

const PNG = 'data:image/png;base64,';

describe('mock skin catalog', () => {
  it('serves the first-batch manifest and its files', async () => {
    const { handle } = createMockBackend();
    const { manifest, status } = (await handle('skin_catalog')) as CatalogAnswer;
    expect(status).toBe('fresh');
    expect(manifest!.pieces.map((p) => p.id)).toEqual(
      expect.arrayContaining(['body-plain', 'eyes-round', 'brows-straight', 'mouth-smile', 'hair-short', 'top-tee', 'top-bomber', 'bottom-jeans', 'shoes-sneakers', 'hat-cap', 'glasses-round', 'neck-scarf']),
    );
    expect(manifest!.looks.map((l) => l.id)).toEqual(['look-street', 'look-knight', 'look-astronaut']);
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
