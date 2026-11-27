import { describe, expect, test } from 'vitest';
import { mockCatalog } from '../../ipc/mockCatalog';
import type { CatalogManifest, CatalogPiece, SkinLook, WornPiece } from '../../ipc/schema';
import { SLOTS } from './compose';
import { HAIR_COLOURS, IRIS_COLOURS, OUTFIT_SCHEMES } from './palettes';
import { surprise } from './surprise';

const HEX = /^#[0-9a-f]{6}$/;

/** A small seeded generator (mulberry32): the same outfits on every run. */
function seeded(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const piece = (id: string, slot: string, areas: [string, string][] = [['main', '#808080']], more: Partial<CatalogPiece> = {}): CatalogPiece => ({
  id,
  slot,
  name: { en: id, es: id },
  tags: [],
  areas: areas.map(([a, d]) => ({ id: a, name: { en: a, es: a }, default: d })),
  files: { classic: { art: `${id}-a`, mask: `${id}-m` }, slim: { art: `${id}-a`, mask: `${id}-m` } },
  thumb: '',
  retired: false,
  ...more,
});

/** Three pieces a slot (four tops), one retired piece in every slot, and a top drawn for classic arms only. */
function catalog(): CatalogManifest {
  const pieces: CatalogPiece[] = [piece('body-plain', 'body', [['skin', '#d9a27c']]), piece('body-freckled', 'body', [['skin', '#d9a27c'], ['spots', '#b5651d']])];
  const areas: Record<string, [string, string][]> = {
    eyes: [['iris', '#4a3324'], ['lash', '#1e1310']],
    brows: [['main', '#3b2618']],
    mouth: [],
    face_detail: [['main', '#b5651d']],
    beard: [['main', '#3b2618'], ['tips', '#5a3a28']],
    hair: [['main', '#4b2f1f'], ['tips', '#7a5236'], ['band', '#cc0000']],
    glasses: [['frame', '#2b2b2b'], ['lens', '#88ccee']],
  };
  for (const slot of SLOTS.filter((s) => s !== 'body')) {
    for (const n of [1, 2, 3]) pieces.push(piece(`${slot}-${n}`, slot, areas[slot] ?? [['main', '#808080'], ['trim', '#ffffff'], ['zip', '#c9c9c9']]));
    pieces.push(piece(`${slot}-retired`, slot, areas[slot], { retired: true }));
  }
  pieces.push(piece('top-classic-only', 'top', [['main', '#808080']], { files: { classic: { art: 'a', mask: 'm' } } }));
  return { ...mockCatalog().manifest, pieces, looks: [], expressions: [] };
}

const EMPTY: SkinLook = { pieces: [], painting: true };
const REQUIRED = ['eyes', 'brows', 'mouth', 'hair', 'top', 'bottom', 'shoes'];
const OPTIONAL = ['hat', 'glasses', 'neck', 'back', 'hands', 'belt', 'face_detail', 'beard'];
const bySlot = (manifest: CatalogManifest, look: SkinLook) => {
  const slotOf = new Map(manifest.pieces.map((p) => [p.id, p.slot]));
  const out = new Map<string, WornPiece[]>();
  for (const w of look.pieces) out.set(slotOf.get(w.id)!, [...(out.get(slotOf.get(w.id)!) ?? []), w]);
  return out;
};
const outfits = (manifest: CatalogManifest, look: SkinLook, runs: number, seed = 1) => {
  const random = seeded(seed);
  return Array.from({ length: runs }, () => surprise(manifest, look, random));
};

describe('surprise', () => {
  test('the same random numbers give the same outfit, other numbers another', () => {
    const m = catalog();
    expect(surprise(m, EMPTY, seeded(7))).toEqual(surprise(m, EMPTY, seeded(7)));
    const distinct = new Set(Array.from({ length: 20 }, (_, seed) => JSON.stringify(surprise(m, EMPTY, seeded(seed)))));
    expect(distinct.size).toBeGreaterThan(15);
  });

  test('one piece a slot: always a body, a face, hair, a top, a bottom and shoes; the extras only sometimes', () => {
    const m = catalog();
    const known = new Set(m.pieces.map((p) => p.id));
    const counts = new Map<string, number>();
    const all = outfits(m, EMPTY, 2000);
    for (const look of all) {
      for (const w of look.pieces) expect(known.has(w.id)).toBe(true);
      const slots = bySlot(m, look);
      for (const [slot, worn] of slots) {
        expect(worn).toHaveLength(1);
        counts.set(slot, (counts.get(slot) ?? 0) + 1);
      }
      for (const slot of ['body', ...REQUIRED]) expect(slots.has(slot)).toBe(true);
    }
    const share = (slot: string) => (counts.get(slot) ?? 0) / all.length;
    expect(share('hat')).toBeGreaterThan(0.3);
    expect(share('hat')).toBeLessThan(0.4);
    expect(share('glasses')).toBeGreaterThan(0.16);
    expect(share('glasses')).toBeLessThan(0.24);
    for (const slot of OPTIONAL.filter((s) => s !== 'hat' && s !== 'glasses')) {
      expect(share(slot)).toBeGreaterThan(0.07);
      expect(share(slot)).toBeLessThan(0.23);
    }
    // Every piece on offer turns up, in the order the skin is drawn.
    const seen = new Set(all.flatMap((look) => look.pieces.map((w) => w.id)));
    for (const slot of SLOTS.filter((s) => s !== 'body')) for (const n of [1, 2, 3]) expect(seen.has(`${slot}-${n}`)).toBe(true);
    const slotOf = new Map(m.pieces.map((p) => [p.id, p.slot]));
    for (const look of all.slice(0, 50)) {
      const order = look.pieces.map((w) => SLOTS.indexOf(slotOf.get(w.id)!));
      expect(order).toEqual([...order].sort((a, b) => a - b));
    }
  });

  test('never a retired piece, nor one without art for the arms', () => {
    const m = catalog();
    const retired = new Set(m.pieces.filter((p) => p.retired).map((p) => p.id));
    const random = seeded(3);
    let classicOnly = 0;
    for (let i = 0; i < 500; i++) {
      const classic = surprise(m, EMPTY, random, 'classic');
      const slim = surprise(m, EMPTY, random, 'slim');
      for (const w of [...classic.pieces, ...slim.pieces]) expect(retired.has(w.id)).toBe(false);
      if (classic.pieces.some((w) => w.id === 'top-classic-only')) classicOnly += 1;
      expect(slim.pieces.some((w) => w.id === 'top-classic-only')).toBe(false);
    }
    expect(classicOnly).toBeGreaterThan(0);
  });

  test('the body and its colours stay; a skin with no body gets the plain one', () => {
    const m = catalog();
    const body: WornPiece = { id: 'body-freckled', colours: { skin: '#3f2a1f', spots: '#000000' } };
    const worn: SkinLook = { pieces: [{ id: 'top-1', colours: { main: '#ff0000' } }, body, { id: 'hat-2', colours: {} }, { id: 'long-gone', colours: {} }], painting: false };
    for (const look of outfits(m, worn, 200)) {
      expect(look.pieces[0]).toEqual(body);
      expect(look.painting).toBe(false);
      // The outfit replaces everything else, pieces the catalog no longer has included.
      expect(look.pieces.some((w) => w.id === 'long-gone')).toBe(false);
    }
    for (const look of outfits(m, EMPTY, 50)) {
      expect(look.pieces[0]).toEqual({ id: 'body-plain', colours: { skin: '#d9a27c' } });
      expect(look.painting).toBe(true);
    }
    // A worn body the catalog has retired is still the player's body.
    const retired: WornPiece = { id: 'body-old', colours: { skin: '#8fa3b8' } };
    const old = { ...m, pieces: [...m.pieces, piece('body-old', 'body', [['skin', '#d9a27c']], { retired: true })] };
    expect(surprise(old, { pieces: [retired], painting: false }, seeded(1)).pieces[0]).toEqual(retired);
  });

  test('every area has a colour, each a #rrggbb', () => {
    const m = catalog();
    const byId = new Map(m.pieces.map((p) => [p.id, p]));
    for (const look of outfits(m, EMPTY, 300)) {
      for (const w of look.pieces) {
        expect(Object.keys(w.colours).sort()).toEqual(byId.get(w.id)!.areas.map((a) => a.id).sort());
        for (const c of Object.values(w.colours)) expect(c).toMatch(HEX);
      }
    }
  });

  test('hair, brows and beard share one hair colour; eyes take an eye colour', () => {
    const m = catalog();
    for (const look of outfits(m, EMPTY, 300)) {
      const slots = bySlot(m, look);
      const hair = slots.get('hair')![0]!.colours.main!;
      expect(HAIR_COLOURS).toContain(hair);
      expect(slots.get('brows')![0]!.colours.main).toBe(hair);
      const beard = slots.get('beard')?.[0];
      if (beard) expect(beard.colours.main).toBe(hair);
      const eyes = slots.get('eyes')![0]!;
      expect(IRIS_COLOURS).toContain(eyes.colours.iris);
      // What isn't an iris keeps the piece's own colour.
      expect(eyes.colours.lash).toBe('#1e1310');
      expect(slots.get('face_detail')?.[0]?.colours.main ?? '#b5651d').toBe('#b5651d');
    }
  });

  test('the player\'s hair colour stays about half of the time', () => {
    const m = catalog();
    const mine = '#12ab34';
    const worn: SkinLook = { pieces: [{ id: 'hair-1', colours: { main: mine, tips: '#56cd78', band: '#000000' } }], painting: false };
    const all = outfits(m, worn, 1000);
    const kept = all.filter((look) => bySlot(m, look).get('hair')![0]!.colours.main === mine);
    expect(kept.length).toBeGreaterThan(420);
    expect(kept.length).toBeLessThan(580);
    // Kept whole: the tips too, and the brows follow.
    for (const look of kept) {
      const slots = bySlot(m, look);
      expect(slots.get('hair')![0]!.colours.tips).toBe('#56cd78');
      expect(slots.get('brows')![0]!.colours.main).toBe(mine);
    }
    // With no hair on, the brows' colour is the one to keep.
    const browsOnly: SkinLook = { pieces: [{ id: 'brows-2', colours: { main: mine } }], painting: false };
    const keptBrows = outfits(m, browsOnly, 400).filter((look) => bySlot(m, look).get('hair')![0]!.colours.main === mine);
    expect(keptBrows.length).toBeGreaterThan(150);
    expect(keptBrows.length).toBeLessThan(250);
  });

  test('the clothes of an outfit come from one scheme, the top in its first colour', () => {
    const m = catalog();
    const cloth = ['top', 'bottom', 'shoes', 'hat', 'neck', 'back', 'hands', 'belt'];
    const used = new Set<number>();
    for (const look of outfits(m, EMPTY, 600)) {
      const slots = bySlot(m, look);
      const colours = cloth.flatMap((slot) => Object.values(slots.get(slot)?.[0]?.colours ?? {}));
      // The hair's band and the glasses' frame are dressed with the clothes; the lens keeps its own colour.
      colours.push(slots.get('hair')![0]!.colours.band!);
      const glasses = slots.get('glasses')?.[0];
      if (glasses) {
        colours.push(glasses.colours.frame!);
        expect(glasses.colours.lens).toBe('#88ccee');
      }
      const scheme = OUTFIT_SCHEMES.findIndex((s) => colours.every((c) => s.includes(c)));
      expect(scheme).toBeGreaterThanOrEqual(0);
      used.add(scheme);
      const top = slots.get('top')![0]!;
      if (top.id !== 'top-classic-only') {
        expect(top.colours.main).toBe(OUTFIT_SCHEMES[scheme]![0]);
        // Main, trim and the third area are three different colours of it.
        expect(new Set(Object.values(top.colours)).size).toBe(3);
      }
      // Top and bottom never in the same main colour.
      expect(slots.get('bottom')![0]!.colours.main).not.toBe(top.colours.main);
    }
    expect(used.size).toBe(OUTFIT_SCHEMES.length);
  });

  test('a catalog with gaps still dresses what it can', () => {
    const m = catalog();
    const sparse = { ...m, pieces: m.pieces.filter((p) => p.slot === 'top' || p.slot === 'eyes') };
    const look = surprise(sparse, EMPTY, seeded(5));
    expect([...bySlot(sparse, look).keys()].sort()).toEqual(['eyes', 'top']);
    expect(surprise({ ...m, pieces: [] }, EMPTY, seeded(5))).toEqual(EMPTY);
  });

  test('the mock catalog dresses a whole character', () => {
    const { manifest } = mockCatalog();
    const look = surprise(manifest, EMPTY, seeded(11));
    const slots = bySlot(manifest, look);
    for (const slot of ['body', ...REQUIRED]) expect(slots.get(slot)).toHaveLength(1);
  });
});
