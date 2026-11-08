import { act, waitFor } from '@testing-library/react';
import { createElement } from 'react';
import { afterEach, beforeAll, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import type { CatalogPiece, SkinLook, WornPiece } from '../../ipc/schema';
import { sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { useCatalog } from './useCatalog';
import { useLook, type LookEditor } from './useLook';
import { useSkinDoc, type SkinDoc, type StrokeOptions } from './useSkinDoc';

vi.mock('./codec', async () => (await import('../../test/skinFakes')).fakeCodec());

afterEach(() => {
  vi.useRealTimers();
});

const ID = 'skin-0a1b2c3d4e5f';
const noop = () => {};

/** Holds the catalog's file reads while closed, so a change waits on its pieces. */
function gate() {
  let release: () => void = () => {};
  let held: Promise<void> | null = null;
  return {
    close() {
      held = new Promise((r) => (release = r));
    },
    open() {
      held = null;
      release();
    },
    get held() {
      return held;
    },
  };
}

function open(look?: SkinLook, files = gate(), noCatalog = false) {
  const backend = createMockBackend();
  if (noCatalog) backend.state.skins.catalogStatus = 'unavailable';
  if (look) backend.state.skins.entries.find((e) => e.id === ID)!.look = look;
  const calls: [string, unknown][] = [];
  const got: { doc?: SkinDoc; editor?: LookEditor; ready?: boolean; renders: { ids: string[] | undefined; piecesReady: boolean }[] } = { renders: [] };
  function Harness() {
    const doc = useSkinDoc(ID, 'classic', noop, noop, look);
    const catalog = useCatalog();
    got.doc = doc;
    got.editor = useLook(ID, 'classic', doc, catalog);
    got.ready = doc.pixels !== null && catalog.manifest !== null;
    got.renders.push({ ids: doc.look?.look.pieces.map((p) => p.id), piecesReady: doc.piecesReady });
    return null;
  }
  renderWithProviders(createElement(Harness), {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      const held = files.held;
      if (cmd === 'skin_catalog_file' && held) return held.then(() => backend.handle(cmd, args));
      return backend.handle(cmd, args);
    },
  });
  return { got, calls };
}

const ids = (look: SkinLook | null | undefined) => look?.pieces.map((p) => p.id);
const catalogPiece = async (id: string) => (await import('../../ipc/mockCatalog')).mockCatalog().manifest.pieces.find((p) => p.id === id)!;
let EYES_PIECE: CatalogPiece | undefined;
let TEE_PIECE: CatalogPiece | undefined;
let BOOTS_PIECE: CatalogPiece | undefined;
beforeAll(async () => {
  [EYES_PIECE, TEE_PIECE, BOOTS_PIECE] = await Promise.all([catalogPiece('eyes-round'), catalogPiece('top-tee'), catalogPiece('shoes-moon-boots')]);
});

it('putting on a top replaces the previous top', async () => {
  const { got } = open({ pieces: [{ id: 'body-plain', colours: { skin: '#d9a27c' } }, { id: 'top-tee', colours: { main: '#e8e4dc', trim: '#3a6ea5' } }], painting: false });
  await waitFor(() => expect(got.ready).toBe(true));
  const before = got.doc!.rev;
  const bomber = await catalogPiece('top-bomber');
  act(() => got.editor!.putOn(bomber));
  await waitFor(() => expect(ids(got.editor!.look)).toEqual(['body-plain', 'top-bomber']));
  expect(got.editor!.look!.pieces[1]!.colours).toEqual({ main: '#4a5d3a', trim: '#2b2b2b', zip: '#c9c9c9' });
  expect(got.doc!.rev).toBeGreaterThan(before);
  expect(got.doc!.canUndo).toBe(true);
});

it('changes autosave once after 600 ms', async () => {
  const { got, calls } = open({ pieces: [{ id: 'body-plain', colours: { skin: '#d9a27c' } }], painting: false });
  await waitFor(() => expect(got.ready).toBe(true));
  vi.useFakeTimers({ shouldAdvanceTime: true });
  act(() => got.editor!.setColour('body-plain', 'skin', '#8d5524'));
  await waitFor(() => expect(got.editor!.look!.pieces[0]!.colours.skin).toBe('#8d5524'));
  act(() => got.editor!.setColour('body-plain', 'skin', '#f1c27d'));
  await waitFor(() => expect(got.editor!.look!.pieces[0]!.colours.skin).toBe('#f1c27d'));
  await act(() => vi.advanceTimersByTimeAsync(300));
  expect(sent(calls, 'skin_look_save')).toEqual([]);
  await act(() => vi.advanceTimersByTimeAsync(400));
  const saves = sent(calls, 'skin_look_save') as { id: string; look: SkinLook; painting: string | null }[];
  expect(saves).toHaveLength(1);
  expect(saves[0]!.id).toBe(ID);
  expect(saves[0]!.look).toEqual({ pieces: [{ id: 'body-plain', colours: { skin: '#f1c27d' } }], painting: false });
  expect(saves[0]!.painting).toBeNull();
  expect(sent(calls, 'skin_save')).toEqual([]);
});

it('make part of the skin leaves no pieces and one undo brings them back', async () => {
  const worn: WornPiece[] = [{ id: 'body-plain', colours: { skin: '#d9a27c' } }, { id: 'hat-cap', colours: { main: '#1f3b73', peak: '#d23c2f' } }];
  const { got } = open({ pieces: worn, painting: false });
  await waitFor(() => expect(got.ready).toBe(true));
  const texels = new Uint8ClampedArray(got.doc!.pixels!.data);
  act(() => got.editor!.bakeAll());
  await waitFor(() => expect(got.editor!.look!.pieces).toEqual([]));
  expect(got.editor!.look!.painting).toBe(true);
  // The finished skin is unchanged: it is now the painting.
  expect(got.editor!.painting).toEqual(texels);
  expect(got.doc!.pixels!.data).toEqual(texels);
  act(() => got.doc!.undo());
  await waitFor(() => expect(got.editor!.look!.pieces).toEqual(worn));
  expect(got.editor!.painting).toBeNull();
});

it('a plain skin keeps its texels as the painting under its first piece', async () => {
  const { got, calls } = open();
  await waitFor(() => expect(got.ready).toBe(true));
  expect(got.editor!.look).toBeNull();
  const texels = new Uint8ClampedArray(got.doc!.pixels!.data);
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const cap = await catalogPiece('hat-cap');
  act(() => got.editor!.putOn(cap));
  await waitFor(() => expect(ids(got.editor!.look)).toEqual(['hat-cap']));
  expect(got.editor!.painting).toEqual(texels);
  expect(got.editor!.look!.painting).toBe(true);
  await act(() => vi.advanceTimersByTimeAsync(700));
  expect((sent(calls, 'skin_look_save') as { painting: string | null }[])[0]!.painting).toMatch(/^data:image\/png;base64,SUM/);
});

it('make part of the skin right after a colour change still loading keeps that colour', async () => {
  const files = gate();
  const cape = { id: 'back-cape', colours: { main: '#7a1f2b', trim: '#d4af37' } };
  const { got } = open({ pieces: [{ id: 'body-plain', colours: { skin: '#d9a27c' } }, cape], painting: false }, files);
  await waitFor(() => expect(got.ready).toBe(true));
  const opened = new Uint8ClampedArray(got.doc!.pixels!.data);
  files.close();
  act(() => got.editor!.setColour('back-cape', 'main', '#123456'));
  act(() => got.editor!.bakeAll());
  files.open();
  await waitFor(() => expect(got.editor!.look!.pieces).toEqual([]));
  // The bake took the recoloured cape into the painting, not the skin as it was before the change landed.
  expect(got.editor!.painting).toEqual(got.doc!.pixels!.data);
  expect(got.editor!.painting).not.toEqual(opened);
  act(() => got.doc!.undo());
  await waitFor(() => expect(got.editor!.look!.pieces[1]!.colours.main).toBe('#123456'));
});

it('undo while a change is loading holds, and the change lands on what is left', async () => {
  const files = gate();
  const { got, calls } = open({ pieces: [{ id: 'body-plain', colours: { skin: '#d9a27c' } }], painting: false }, files);
  await waitFor(() => expect(got.ready).toBe(true));
  act(() => got.editor!.setColour('body-plain', 'skin', '#8d5524'));
  await waitFor(() => expect(got.editor!.look!.pieces[0]!.colours.skin).toBe('#8d5524'));
  files.close();
  const scarf = await catalogPiece('neck-scarf');
  const asked = sent(calls, 'skin_catalog_file').length;
  act(() => got.editor!.putOn(scarf));
  // The change is under way: it waits on the scarf's files.
  await waitFor(() => expect(sent(calls, 'skin_catalog_file').length).toBeGreaterThan(asked));
  act(() => got.doc!.undo());
  // The undo holds while the scarf loads.
  expect(got.editor!.look!.pieces[0]!.colours.skin).toBe('#d9a27c');
  files.open();
  await waitFor(() => expect(ids(got.editor!.look)).toEqual(['body-plain', 'neck-scarf']));
  expect(got.editor!.look!.pieces[0]!.colours.skin).toBe('#d9a27c');
  // The scarf is a new step after the undo: nothing to redo, and one undo takes the scarf off.
  expect(got.doc!.canRedo).toBe(false);
  act(() => got.doc!.undo());
  await waitFor(() => expect(ids(got.editor!.look)).toEqual(['body-plain']));
  expect(got.editor!.look!.pieces[0]!.colours.skin).toBe('#d9a27c');
});

const RED = [255, 0, 0, 255];
const pencil: StrokeOptions = { tool: 'pencil', color: [255, 0, 0, 255] as [number, number, number, number], mirror: false, darken: false };
const at = (data: Uint8ClampedArray | null | undefined, x: number, y: number) => (data ? Array.from(data.slice((y * 64 + x) * 4, (y * 64 + x) * 4 + 4)) : null);
const TEE: WornPiece = { id: 'top-tee', colours: { main: '#e8e4dc', trim: '#3a6ea5' } };
const BODY: WornPiece = { id: 'body-plain', colours: { skin: '#d9a27c' } };

function stroke(doc: SkinDoc, x: number, y: number) {
  act(() => {
    doc.begin();
    doc.paint(x, y, pencil, false);
    doc.end();
  });
}

it('painting a skin with pieces edits the painting under them', async () => {
  const { got } = open({ pieces: [BODY, TEE], painting: false });
  await waitFor(() => expect(got.ready && got.doc!.piecesReady).toBe(true));
  // Under the T-shirt (the fake top covers rows 32–35) and where nothing is worn (row 40).
  stroke(got.doc!, 10, 33);
  stroke(got.doc!, 10, 40);
  expect(at(got.editor!.painting, 10, 33)).toEqual(RED);
  expect(at(got.editor!.painting, 10, 40)).toEqual(RED);
  expect(got.editor!.look!.painting).toBe(true);
  expect(ids(got.editor!.look)).toEqual(['body-plain', 'top-tee']);
  // The finished skin keeps the shirt on top, and shows the stroke where nothing covers it.
  expect(at(got.doc!.pixels!.data, 10, 33)).toEqual([0xe8, 0xe4, 0xdc, 255]);
  expect(at(got.doc!.pixels!.data, 10, 40)).toEqual(RED);
  // A later change in Create recomposes from the painting, so the strokes stay.
  act(() => got.editor!.setColour('top-tee', 'main', '#112233'));
  await waitFor(() => expect(got.editor!.look!.pieces[1]!.colours.main).toBe('#112233'));
  expect(at(got.doc!.pixels!.data, 10, 40)).toEqual(RED);
  expect(at(got.doc!.pixels!.data, 10, 33)).toEqual([0x11, 0x22, 0x33, 255]);
  // Each stroke is one undo step.
  act(() => got.doc!.undo());
  act(() => got.doc!.undo());
  expect(at(got.editor!.painting, 10, 40)).not.toEqual(RED);
  expect(at(got.editor!.painting, 10, 33)).toEqual(RED);
  act(() => got.doc!.undo());
  expect(at(got.editor!.painting, 10, 33)).not.toEqual(RED);
});

it('a stroke hidden by a piece marks the pieces as on top', async () => {
  const { got } = open({ pieces: [BODY, TEE], painting: false });
  await waitFor(() => expect(got.ready && got.doc!.piecesReady).toBe(true));
  stroke(got.doc!, 10, 33);
  expect(got.doc!.covered).toBe(true);
  stroke(got.doc!, 10, 40);
  expect(got.doc!.covered).toBe(false);
  // The body is under the painting, so painting over it shows.
  stroke(got.doc!, 10, 1);
  expect(got.doc!.covered).toBe(false);
});

it('strokes on a skin with pieces save the painting through skin_look_save', async () => {
  const { got, calls } = open({ pieces: [BODY, TEE], painting: false });
  await waitFor(() => expect(got.ready && got.doc!.piecesReady).toBe(true));
  vi.useFakeTimers({ shouldAdvanceTime: true });
  stroke(got.doc!, 10, 40);
  await act(() => vi.advanceTimersByTimeAsync(700));
  const saves = sent(calls, 'skin_look_save') as { look: SkinLook; painting: string | null }[];
  expect(saves).toHaveLength(1);
  expect(saves[0]!.look.pieces.map((p) => p.id)).toEqual(['body-plain', 'top-tee']);
  expect(saves[0]!.painting).toMatch(/^data:image\/png;base64,SUM/);
  expect(sent(calls, 'skin_save')).toEqual([]);
});

it('skins without a look paint exactly as before', async () => {
  const { got, calls } = open();
  await waitFor(() => expect(got.ready).toBe(true));
  vi.useFakeTimers({ shouldAdvanceTime: true });
  stroke(got.doc!, 10, 33);
  expect(at(got.doc!.pixels!.data, 10, 33)).toEqual(RED);
  expect(got.doc!.look).toBeNull();
  expect(got.doc!.covered).toBe(false);
  await act(() => vi.advanceTimersByTimeAsync(700));
  expect(sent(calls, 'skin_save')).toHaveLength(1);
  expect(sent(calls, 'skin_look_save')).toEqual([]);
});

const EYES: WornPiece = { id: 'eyes-round', colours: { iris: '#4a3324' } };
function strokeWith(doc: SkinDoc, x: number, y: number, opts: Partial<StrokeOptions> & { opacity?: number }) {
  act(() => {
    doc.begin();
    doc.paint(x, y, { ...pencil, ...opts }, false);
    doc.end();
  });
}

it('fill over the body piece fills only the visible same-colour region', async () => {
  // The head's top face (x 8–15, y 0–7): the fake body covers rows 0–3, the fake eyes rows 4–7.
  const { got } = open({ pieces: [BODY, EYES], painting: false });
  await waitFor(() => expect(got.ready && got.doc!.piecesReady).toBe(true));
  strokeWith(got.doc!, 10, 1, { tool: 'fill' });
  expect(at(got.editor!.painting, 10, 1)).toEqual(RED);
  expect(at(got.editor!.painting, 14, 3)).toEqual(RED);
  // The eyes are another colour on screen, so the fill stops at them.
  expect(at(got.editor!.painting, 10, 6)![3]).toBe(0);
  expect(at(got.doc!.pixels!.data, 10, 6)).toEqual([0x4a, 0x33, 0x24, 255]);
});

it('shade over the body piece darkens the visible colour', async () => {
  const { got } = open({ pieces: [BODY], painting: false });
  await waitFor(() => expect(got.ready && got.doc!.piecesReady).toBe(true));
  strokeWith(got.doc!, 10, 1, { tool: 'shade', darken: true });
  // #d9a27c moved 12% towards black.
  expect(at(got.editor!.painting, 10, 1)).toEqual([191, 143, 109, 255]);
  expect(at(got.doc!.pixels!.data, 10, 1)).toEqual([191, 143, 109, 255]);
});

it('without the pieces, an eraser leaves the finished skin and a light stroke lays over it', async () => {
  const { got } = open({ pieces: [BODY, TEE], painting: false }, gate(), true);
  await waitFor(() => expect(got.doc!.pixels).not.toBeNull());
  expect(got.doc!.piecesReady).toBe(false);
  const before = at(got.doc!.pixels!.data, 5, 22)!;
  expect(before[3]).toBe(255);
  strokeWith(got.doc!, 5, 22, { tool: 'eraser' });
  expect(at(got.doc!.pixels!.data, 5, 22)).toEqual(before);
  strokeWith(got.doc!, 5, 22, { opacity: 0.5 });
  const after = at(got.doc!.pixels!.data, 5, 22)!;
  // Laid over what shows, not punched through as a half-transparent hole.
  expect(after[3]).toBe(255);
  expect(after).not.toEqual(before);
  expect(after).not.toEqual(RED);
  // Both went into the painting under the pieces.
  expect(at(got.editor!.painting, 5, 22)![3]).toBeGreaterThan(0);
  expect(got.doc!.canUndo).toBe(true);
});

it('switching arms clears the pieces-on-top hint', async () => {
  const { got } = open({ pieces: [BODY, TEE], painting: false });
  await waitFor(() => expect(got.ready && got.doc!.piecesReady).toBe(true));
  stroke(got.doc!, 10, 33);
  expect(got.doc!.covered).toBe(true);
  act(() => got.doc!.switchModel('slim'));
  expect(got.doc!.covered).toBe(false);
});

it('without the pieces, a shade reads the painting, never the pieces on top', async () => {
  const { got } = open({ pieces: [BODY, TEE], painting: false }, gate(), true);
  await waitFor(() => expect(got.doc!.pixels).not.toBeNull());
  // Opaque on the finished skin, empty in the painting.
  expect(at(got.doc!.pixels!.data, 5, 22)![3]).toBe(255);
  strokeWith(got.doc!, 5, 22, { tool: 'shade', darken: true });
  expect(at(got.editor!.painting, 5, 22)?.[3] ?? 0).toBe(0);
});

const opaque = (rgba: number[]) => {
  const out = new Uint8ClampedArray(64 * 64 * 4);
  for (let i = 0; i < out.length; i += 4) out.set(rgba, i);
  return out;
};

it('a face piece the painting covers raises the hint, and the next change that shows clears it', async () => {
  const { got } = open();
  await waitFor(() => expect(got.ready).toBe(true));
  // A painting that covers the whole skin.
  const red = opaque(RED);
  act(() => got.doc!.changeLook({ look: { pieces: [], painting: true }, painting: red }, red));
  expect(got.editor!.paintingCovers).toBe(false);
  act(() => got.editor!.putOn(EYES_PIECE!));
  await waitFor(() => expect(ids(got.editor!.look)).toEqual(['eyes-round']));
  expect(got.editor!.paintingCovers).toBe(true);
  // A recolour under the painting changes nothing either.
  act(() => got.editor!.setColour('eyes-round', 'iris', '#00ff00'));
  await waitFor(() => expect(got.editor!.look!.pieces[0]!.colours.iris).toBe('#00ff00'));
  expect(got.editor!.paintingCovers).toBe(true);
  // A top is drawn over the painting, so it shows.
  act(() => got.editor!.putOn(TEE_PIECE!));
  await waitFor(() => expect(ids(got.editor!.look)).toEqual(['eyes-round', 'top-tee']));
  expect(got.editor!.paintingCovers).toBe(false);
});

it('a face piece that shows raises no hint', async () => {
  const { got } = open({ pieces: [BODY], painting: false });
  await waitFor(() => expect(got.ready).toBe(true));
  act(() => got.editor!.putOn(EYES_PIECE!));
  await waitFor(() => expect(ids(got.editor!.look)).toEqual(['body-plain', 'eyes-round']));
  expect(got.editor!.paintingCovers).toBe(false);
});

it('after a change the doc keeps the pieces, so strokes go under them at once', async () => {
  const { got } = open({ pieces: [BODY], painting: false });
  await waitFor(() => expect(got.ready && got.doc!.piecesReady).toBe(true));
  act(() => got.editor!.putOn(TEE_PIECE!));
  await waitFor(() => expect(ids(got.editor!.look)).toEqual(['body-plain', 'top-tee']));
  // No render showed the new look without its pieces.
  expect(got.renders.filter((r) => r.ids?.includes('top-tee')).every((r) => r.piecesReady)).toBe(true);
});

it('settled waits for a change still loading', async () => {
  const files = gate();
  const { got } = open({ pieces: [BODY], painting: false }, files);
  await waitFor(() => expect(got.ready && got.doc!.piecesReady).toBe(true));
  files.close();
  // No other test here loads the moon boots' files, so the change waits on them.
  act(() => got.editor!.putOn(BOOTS_PIECE!));
  let done = false;
  const settled = got.editor!.settled().then(() => (done = true));
  await act(() => new Promise((r) => setTimeout(r, 30)));
  expect(done).toBe(false);
  files.open();
  await act(() => settled);
  expect(ids(got.doc!.look?.look)).toEqual(['body-plain', 'shoes-moon-boots']);
});
