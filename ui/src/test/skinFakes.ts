// Stand-ins for the Skins screen's canvas and WebGL parts, which jsdom lacks. The codec encodes texels to a checksum
// (so tests can tell textures apart) and decodes a catalog piece's art and mask to plain shapes that recolour: the art
// mid grey and the mask every texel in the piece's first colour area, both covering one band of rows per slot. Any other
// texture decodes to the starter skin. The stage shows the checksum of the texels it draws.
import { createElement, forwardRef, useImperativeHandle } from 'react';
import { mockCatalog } from '../ipc/mockCatalog';
import { SLOTS } from '../features/skins/compose';
import type { Pixels } from '../features/skins/paint';
import { starterSkin } from '../features/skins/starter';

export function checksum(data: Uint8ClampedArray): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < data.length; i++) h = Math.imul(h ^ data[i]!, 0x01000193) >>> 0;
  return h.toString(16).padStart(8, '0');
}

export function fakeCodec() {
  const { manifest, files } = mockCatalog();
  const roles = new Map<string, { kind: 'art' | 'mask'; slot: string }>();
  for (const p of manifest.pieces) {
    for (const f of Object.values(p.files)) {
      roles.set(files.get(f.art)!, { kind: 'art', slot: p.slot });
      roles.set(files.get(f.mask)!, { kind: 'mask', slot: p.slot });
    }
  }
  const band = (slot: string, kind: 'art' | 'mask') => {
    const out = new Uint8ClampedArray(64 * 64 * 4);
    const row0 = Math.max(0, SLOTS.indexOf(slot)) * 4;
    for (let y = row0; y < row0 + 4; y++) {
      for (let x = 0; x < 64; x++) {
        const i = (y * 64 + x) * 4;
        out.set(kind === 'art' ? [128, 128, 128, 255] : [85, 0, 0, 255], i);
      }
    }
    return out;
  };
  return {
    decodeTexture: (url: string) => {
      const role = roles.get(url);
      return Promise.resolve(role ? band(role.slot, role.kind) : starterSkin().data);
    },
    encodeTexture: (data: Uint8ClampedArray) => `data:image/png;base64,SUM${checksum(data)}`,
  };
}

/** The stage without WebGL: an element that names the checksum of what it draws. */
export const FakeStage = forwardRef(function FakeStage({ pixels, rev }: { pixels: Pixels | null; rev: number }, ref) {
  useImperativeHandle(ref, () => ({ resetView() {}, zoom() {}, celebrate() {} }));
  return createElement('div', { 'data-testid': 'stage', 'data-rev': rev, 'data-sum': pixels ? checksum(pixels.data) : '' });
});
