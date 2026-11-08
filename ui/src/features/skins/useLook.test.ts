import { act, waitFor } from '@testing-library/react';
import { createElement } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import { createMockBackend } from '../../ipc/mock';
import type { SkinLook, WornPiece } from '../../ipc/schema';
import { sent } from '../../test/backend';
import { renderWithProviders } from '../../test/render';
import { useCatalog } from './useCatalog';
import { useLook, type LookEditor } from './useLook';
import { useSkinDoc, type SkinDoc } from './useSkinDoc';

vi.mock('./codec', async () => (await import('../../test/skinFakes')).fakeCodec());

afterEach(() => {
  vi.useRealTimers();
});

const ID = 'skin-0a1b2c3d4e5f';
const noop = () => {};

function open(look?: SkinLook) {
  const backend = createMockBackend();
  if (look) backend.state.skins.entries.find((e) => e.id === ID)!.look = look;
  const calls: [string, unknown][] = [];
  const got: { doc?: SkinDoc; editor?: LookEditor; ready?: boolean } = {};
  function Harness() {
    const doc = useSkinDoc(ID, 'classic', noop, noop, look);
    const catalog = useCatalog();
    got.doc = doc;
    got.editor = useLook(ID, 'classic', doc, catalog);
    got.ready = doc.pixels !== null && catalog.manifest !== null;
    return null;
  }
  renderWithProviders(createElement(Harness), {
    ipc: (cmd, args) => {
      calls.push([cmd, args]);
      return backend.handle(cmd, args);
    },
  });
  return { got, calls };
}

const ids = (look: SkinLook | null | undefined) => look?.pieces.map((p) => p.id);
const catalogPiece = async (id: string) => (await import('../../ipc/mockCatalog')).mockCatalog().manifest.pieces.find((p) => p.id === id)!;

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
