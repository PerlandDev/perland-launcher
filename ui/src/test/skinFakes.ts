// Stand-ins for the Skins screen's canvas and WebGL parts, which jsdom lacks. The codec encodes texels to a checksum
// (so tests can tell textures apart) and decodes a catalog piece's art and mask to plain shapes that recolour: the art
// mid grey and the mask every texel in the piece's first colour area, both covering one band of rows per slot. Any other
// texture decodes to the starter skin. The stage shows the checksum of the texels it draws, what its camera frames and the closed-eyes frame it was handed.
import { createElement, forwardRef, useImperativeHandle } from 'react';
import { mockCatalog, type MockCatalog } from '../ipc/mockCatalog';
import type { CatalogExpression } from '../ipc/schema';
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

/** The stage without WebGL: an element that names the checksum of what it draws. A press paints texel (10, 10) when
 *  the stage is paintable, as a press on the figure's face would. */
export const FakeStage = forwardRef(function FakeStage(
  {
    pixels,
    rev,
    paintable = true,
    label,
    focus = 'figure',
    focusKey = '',
    blink = null,
    onBegin,
    onPaint,
    onEnd,
  }: { pixels: Pixels | null; rev: number; paintable?: boolean; label?: string; focus?: string; focusKey?: string; blink?: Pixels | null; onBegin: () => void; onPaint: (x: number, y: number) => void; onEnd: () => void },
  ref,
) {
  useImperativeHandle(ref, () => ({ resetView() {}, zoom() {}, celebrate() {} }));
  return createElement('div', {
    'data-testid': 'stage',
    'data-rev': rev,
    'data-sum': pixels ? checksum(pixels.data) : '',
    'data-focus': focus,
    'data-focus-key': focusKey,
    'data-blink': blink ? checksum(blink.data) : '',
    'aria-label': label,
    onPointerDown: () => {
      if (!paintable) return;
      onBegin();
      onPaint(10, 10);
      onEnd();
    },
  });
});

/** Expressions for the tests: the first is the mock catalog's own face, the last names a piece no catalog has. */
export const FAKE_EXPRESSIONS: CatalogExpression[] = [
  { id: 'expr-happy', name: { en: 'Happy', es: 'Feliz' }, eyes: 'eyes-round', brows: 'brows-straight', mouth: 'mouth-smile' },
  { id: 'expr-angry', name: { en: 'Angry', es: 'Enojado' }, eyes: 'eyes-fierce', brows: 'brows-angry', mouth: 'mouth-gritted' },
  { id: 'expr-determined', name: { en: 'Determined', es: 'Decidido' }, eyes: 'eyes-round', brows: 'brows-angry', mouth: 'mouth-smile' },
  { id: 'expr-broken', name: { en: 'Broken', es: 'Roto' }, eyes: 'eyes-round', brows: 'brows-straight', mouth: 'mouth-gone' },
];

/** The files of a face piece `addFaces` adds, as the mock backend names them. */
export const fakeFaceFiles = (id: string) => [`fake-${id}-art`, `fake-${id}-mask`];

/**
 * Gives the mock catalog more faces: `FAKE_EXPRESSIONS`, the pieces they name (drawn like the catalog's own eyes,
 * brows and mouth, under file names of their own) and `eyes-unused`, which no expression names.
 */
export function addFaces(catalog: MockCatalog) {
  const { manifest, files } = catalog;
  const clone = (from: string, id: string, name: string, colour?: string) => {
    const original = manifest.pieces.find((p) => p.id === from)!;
    const [art, mask] = fakeFaceFiles(id) as [string, string];
    files.set(art, files.get(original.files.classic!.art)!);
    files.set(mask, files.get(original.files.classic!.mask)!);
    manifest.pieces.push({
      ...original,
      id,
      name: { en: name, es: name },
      areas: original.areas.map((a) => ({ ...a, default: colour ?? a.default })),
      files: { classic: { art, mask }, slim: { art, mask } },
    });
  };
  clone('eyes-round', 'eyes-fierce', 'Fierce eyes', '#b8651e');
  clone('eyes-round', 'eyes-unused', 'Unused eyes');
  clone('brows-straight', 'brows-angry', 'Angry brows');
  clone('mouth-smile', 'mouth-gritted', 'Gritted teeth');
  manifest.expressions = FAKE_EXPRESSIONS.map((e) => ({ ...e }));
}
