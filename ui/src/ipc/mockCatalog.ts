// The mock skin catalog for `pnpm --filter ui dev:mock` and the tests: an in-memory manifest in the Engine's exact JSON shape
// (see crates/launcher-core/tests/fixtures/skin-catalog-manifest.json) with its first-batch ids, and small solid-colour PNGs
// built by hand (no canvas), made on demand when a file is asked for.
import type { CatalogLook, CatalogManifest, CatalogPiece } from './schema';

const PNG_URL = 'data:image/png;base64,';

const CRC_TABLE = (() => {
  const t: number[] = [];
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t.push(c >>> 0);
  }
  return t;
})();

function crc32(bytes: number[]): number {
  let c = 0xffffffff;
  for (const b of bytes) c = CRC_TABLE[(c ^ b) & 0xff]! ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function u32(n: number): number[] {
  return [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255];
}

function chunk(type: string, data: number[]): number[] {
  const body = [...type].map((c) => c.charCodeAt(0)).concat(data);
  return [...u32(data.length), ...body, ...u32(crc32(body))];
}

/** A `width`×`height` PNG of one `[r, g, b, a]` colour as a data URL (stored, not compressed, deflate). */
export function solidPng(width: number, height: number, rgba: [number, number, number, number]): string {
  const row = [0, ...Array.from({ length: width }, () => rgba).flat()];
  const raw = Array.from({ length: height }, () => row).flat();
  const z: number[] = [0x78, 0x01];
  for (let at = 0; at < raw.length; at += 65535) {
    const part = raw.slice(at, at + 65535);
    const last = at + 65535 >= raw.length ? 1 : 0;
    z.push(last, part.length & 255, part.length >>> 8, ~part.length & 255, (~part.length >>> 8) & 255, ...part);
  }
  let [a, b] = [1, 0];
  for (const v of raw) {
    a = (a + v) % 65521;
    b = (b + a) % 65521;
  }
  z.push(...u32(((b << 16) | a) >>> 0));
  const png = [137, 80, 78, 71, 13, 10, 26, 10, ...chunk('IHDR', [...u32(width), ...u32(height), 8, 6, 0, 0, 0]), ...chunk('IDAT', z), ...chunk('IEND', [])];
  return PNG_URL + btoa(String.fromCharCode(...png));
}

/** A stand-in for a SHA-256: 64 lower-case hex characters made from `seed`. */
function fakeHash(seed: string): string {
  let h = 2166136261;
  let out = '';
  for (let i = 0; out.length < 64; i++) {
    h = Math.imul(h ^ (seed.charCodeAt(i % seed.length) + i), 16777619) >>> 0;
    out += h.toString(16).padStart(8, '0');
  }
  return out.slice(0, 64);
}

function hexColour(hex: string): [number, number, number, number] {
  return [parseInt(hex.slice(1, 3), 16), parseInt(hex.slice(3, 5), 16), parseInt(hex.slice(5, 7), 16), 255];
}

const SLOTS = ['body', 'eyes', 'brows', 'mouth', 'face_detail', 'beard', 'bottom', 'shoes', 'top', 'belt', 'hands', 'neck', 'back', 'hair', 'glasses', 'hat'];

const TAGS = [
  { id: 'casual', name: { en: 'Casual', es: 'Casual' } },
  { id: 'fantasy', name: { en: 'Fantasy', es: 'Fantasía' } },
  { id: 'new', name: { en: 'New', es: 'Nuevo' } },
  { id: 'sci-fi', name: { en: 'Sci-fi', es: 'Ciencia ficción' } },
];

/** id, slot, English and Spanish names, tags, and the areas with their default colours. */
const PIECES: [string, string, string, string, string[], [string, string, string, string][]][] = [
  ['body-plain', 'body', 'Plain', 'Liso', [], [['skin', 'Skin', 'Piel', '#8d5b3e']]],
  ['eyes-round', 'eyes', 'Round', 'Redondos', [], [['iris', 'Iris', 'Iris', '#2a1a10']]],
  ['brows-straight', 'brows', 'Straight', 'Rectas', [], [['main', 'Main', 'Principal', '#1e140d']]],
  ['mouth-smile', 'mouth', 'Smile', 'Sonrisa', [], []],
  ['hair-short', 'hair', 'Short', 'Corto', ['casual'], [['main', 'Main', 'Principal', '#3b2a1c']]],
  ['top-tee', 'top', 'T-shirt', 'Camiseta', ['casual', 'new'], [['main', 'Main', 'Principal', '#4e8cd6'], ['trim', 'Trim', 'Ribete', '#ffffff']]],
  ['top-bomber', 'top', 'Bomber jacket', 'Chaqueta bomber', ['casual'], [['main', 'Main', 'Principal', '#2f5a3a'], ['trim', 'Trim', 'Ribete', '#d4af37']]],
  ['bottom-jeans', 'bottom', 'Jeans', 'Vaqueros', ['casual'], [['main', 'Main', 'Principal', '#35508a']]],
  ['shoes-sneakers', 'shoes', 'Sneakers', 'Zapatillas', ['casual', 'new'], [['main', 'Main', 'Principal', '#f4f4f4'], ['sole', 'Sole', 'Suela', '#c0392b']]],
  ['hat-cap', 'hat', 'Cap', 'Gorra', ['casual'], [['main', 'Main', 'Principal', '#2f5aa8']]],
  ['glasses-round', 'glasses', 'Round glasses', 'Gafas redondas', [], [['frame', 'Frame', 'Montura', '#222222']]],
  ['neck-scarf', 'neck', 'Scarf', 'Bufanda', ['casual', 'fantasy'], [['main', 'Main', 'Principal', '#7a1f2b'], ['trim', 'Trim', 'Ribete', '#d4af37']]],
];

const BASE: [string, Record<string, string>][] = [
  ['body-plain', { skin: '#8d5b3e' }],
  ['eyes-round', { iris: '#2a1a10' }],
  ['brows-straight', { main: '#1e140d' }],
  ['mouth-smile', {}],
];

/** id, English and Spanish names, tags, and the worn pieces after the base ones. */
const LOOKS: [string, string, string, string[], [string, Record<string, string>][], boolean][] = [
  [
    'look-street',
    'Street',
    'Calle',
    ['casual', 'new'],
    [['hair-short', { main: '#3b2a1c' }], ['top-tee', { main: '#4e8cd6', trim: '#ffffff' }], ['bottom-jeans', { main: '#35508a' }], ['shoes-sneakers', { main: '#f4f4f4', sole: '#c0392b' }], ['hat-cap', { main: '#2f5aa8' }]],
    false,
  ],
  [
    'look-knight',
    'Knight',
    'Caballero',
    ['fantasy'],
    [['hair-short', { main: '#2b2b2b' }], ['top-bomber', { main: '#8a949e', trim: '#d4af37' }], ['bottom-jeans', { main: '#5d666e' }], ['shoes-sneakers', { main: '#4a4f56', sole: '#2b2b2b' }], ['neck-scarf', { main: '#7a1f2b', trim: '#d4af37' }]],
    true,
  ],
  [
    'look-astronaut',
    'Astronaut',
    'Astronauta',
    ['new', 'sci-fi'],
    [['top-tee', { main: '#eceff1', trim: '#e07b24' }], ['bottom-jeans', { main: '#eceff1' }], ['shoes-sneakers', { main: '#a9b2bc', sole: '#4a4f56' }], ['glasses-round', { frame: '#e07b24' }], ['hat-cap', { main: '#f4f6f8' }]],
    false,
  ],
];

export interface MockCatalog {
  manifest: CatalogManifest;
  /** Every file the manifest names, as a data URL, by hash. */
  files: Map<string, string>;
}

export function mockCatalog(): MockCatalog {
  const files = new Map<string, string>();
  const put = (seed: string, size: number, colour: [number, number, number, number]) => {
    const hash = fakeHash(seed);
    files.set(hash, solidPng(size, size, colour));
    return hash;
  };
  const pieces: CatalogPiece[] = PIECES.map(([id, slot, en, es, tags, areas]) => {
    const base = hexColour(areas[0]?.[3] ?? '#999999');
    const model = (m: string) => ({ art: put(`${id}/${m}/art`, 64, base), mask: put(`${id}/${m}/mask`, 64, [0, 0, 0, 0]) });
    return {
      id,
      slot,
      name: { en, es },
      tags,
      areas: areas.map(([area, nameEn, nameEs, def]) => ({ id: area, name: { en: nameEn, es: nameEs }, default: def })),
      files: { classic: model('classic'), slim: model('slim') },
      thumb: put(`${id}/thumb`, 16, base),
      retired: false,
    };
  });
  const looks: CatalogLook[] = LOOKS.map(([id, en, es, tags, worn, painted]) => ({
    id,
    name: { en, es },
    tags,
    model: 'classic',
    pieces: [...BASE, ...worn].map(([piece, colours]) => ({ id: piece, colours })),
    ...(painted ? { painting: put(`${id}/painting`, 64, [214, 92, 74, 160]) } : {}),
    thumb: put(`${id}/thumb`, 16, hexColour(Object.values(worn[1]?.[1] ?? {})[0] ?? '#999999')),
    retired: false,
  }));
  return { manifest: { formatVersion: 1, version: fakeHash('mock-catalog-v1'), license: 'CC0-1.0', slots: SLOTS, tags: TAGS, pieces, looks }, files };
}
