import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { CatalogManifest, SkinLook } from '../../ipc/schema';
import { BLINK_MS, blinkGap, blinkSeed } from './blink';
import { compose, type ComposeLayer } from './compose';
import { Pixels } from './paint';
import { BLINK_SETTLE_MS, useBlinkFrame, useBlinking } from './useBlink';
import type { Catalog } from './useCatalog';
import type { SkinDoc } from './useSkinDoc';

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const SKIN = 'skin-0a1b2c3d4e5f';
const first = blinkGap(blinkSeed(SKIN), 0);
const second = blinkGap(blinkSeed(SKIN), 1);

it('the eyes shut for a moment at the skin’s own rhythm', () => {
  const { result } = renderHook(() => useBlinking(true, SKIN));
  expect(result.current).toBe(false);
  act(() => void vi.advanceTimersByTime(first - 1));
  expect(result.current).toBe(false);
  act(() => void vi.advanceTimersByTime(1));
  expect(result.current).toBe(true);
  act(() => void vi.advanceTimersByTime(BLINK_MS - 1));
  expect(result.current).toBe(true);
  act(() => void vi.advanceTimersByTime(1));
  expect(result.current).toBe(false);
  // The next one comes its own gap later.
  act(() => void vi.advanceTimersByTime(second - 1));
  expect(result.current).toBe(false);
  act(() => void vi.advanceTimersByTime(1));
  expect(result.current).toBe(true);
});

it('nothing blinks while it is off, and turning it off mid-blink opens the eyes', () => {
  const { result, rerender } = renderHook(({ active }) => useBlinking(active, SKIN), { initialProps: { active: false } });
  act(() => void vi.advanceTimersByTime(20_000));
  expect(result.current).toBe(false);
  rerender({ active: true });
  act(() => void vi.advanceTimersByTime(first));
  expect(result.current).toBe(true);
  rerender({ active: false });
  expect(result.current).toBe(false);
  // Back on, it starts over with the eyes open.
  rerender({ active: true });
  expect(result.current).toBe(false);
  act(() => void vi.advanceTimersByTime(first - 1));
  expect(result.current).toBe(false);
});

it('never blinks under reduced motion', () => {
  vi.stubGlobal('matchMedia', (query: string) => ({ matches: query.includes('reduce'), addEventListener() {}, removeEventListener() {} }));
  const { result } = renderHook(() => useBlinking(true, SKIN));
  act(() => void vi.advanceTimersByTime(20_000));
  expect(result.current).toBe(false);
});

// The closed-eyes frame of the open skin.
const blank = () => new Uint8ClampedArray(64 * 64 * 4);
const filled = (rgba: number[]) => {
  const out = blank();
  for (let i = 0; i < out.length; i += 4) out.set(rgba, i);
  return out;
};
const eyesArt = blank();
eyesArt.set([20, 30, 90, 255], (12 * 64 + 9) * 4);
const LAYERS: ComposeLayer[] = [
  { slot: 'body', id: 'body-plain', art: filled([200, 150, 120, 255]), mask: blank(), colours: [] },
  { slot: 'eyes', id: 'eyes-round', art: eyesArt, mask: blank(), colours: [] },
];
const MANIFEST = { pieces: [{ id: 'body-plain', slot: 'body' }, { id: 'eyes-round', slot: 'eyes' }] } as unknown as CatalogManifest;

function world(pieces = ['body-plain', 'eyes-round']) {
  const look: SkinLook = { pieces: pieces.map((id) => ({ id, colours: {} })), painting: false };
  const layers = LAYERS.filter((l) => pieces.includes(l.id!));
  const state = { version: 1, pixels: new Pixels(compose(null, layers)), look: { look, painting: null } };
  const layersAsked = vi.fn(async () => layers);
  const catalog = { manifest: MANIFEST, layers: layersAsked } as unknown as Catalog;
  const doc = (rev: number) => ({ rev, look: state.look, current: (id: string) => (id === SKIN ? { look: state.look, pixels: state.pixels, version: state.version } : null) }) as unknown as SkinDoc;
  return { state, catalog, doc, layersAsked };
}
const settle = () => act(() => vi.advanceTimersByTimeAsync(BLINK_SETTLE_MS));

it('the closed-eyes frame is made once the skin has rested, and only the eyes differ', async () => {
  const { state, catalog, doc } = world();
  const { result } = renderHook(() => useBlinkFrame(SKIN, doc(1), catalog, 'classic', true));
  expect(result.current).toBeNull();
  await settle();
  const frame = result.current;
  expect(frame).not.toBeNull();
  const eye = (12 * 64 + 9) * 4;
  expect([...frame!.data.slice(eye, eye + 4)]).toEqual([140, 105, 84, 255]);
  // The skin itself is untouched.
  expect([...state.pixels.data.slice(eye, eye + 4)]).toEqual([20, 30, 90, 255]);
});

it('a frame made for an older skin is never shown: it is made again after the change', async () => {
  const { state, catalog, doc } = world();
  const { result, rerender } = renderHook(({ rev }) => useBlinkFrame(SKIN, doc(rev), catalog, 'classic', true), { initialProps: { rev: 1 } });
  await settle();
  expect(result.current).not.toBeNull();
  // A real change: new texels, a new version, a redraw.
  state.pixels.data.set([1, 2, 3, 255], 0);
  state.version = 2;
  rerender({ rev: 2 });
  expect(result.current).toBeNull();
  await settle();
  expect(result.current).not.toBeNull();
  expect([...result.current!.data.slice(0, 4)]).toEqual([1, 2, 3, 255]);
});

it('there is no frame without eyes, while it is off, or for a plain painted skin, and no files are asked for', async () => {
  const noEyes = world(['body-plain']);
  const a = renderHook(() => useBlinkFrame(SKIN, noEyes.doc(1), noEyes.catalog, 'classic', true));
  await settle();
  expect(a.result.current).toBeNull();
  expect(noEyes.layersAsked).not.toHaveBeenCalled();
  const off = world();
  const b = renderHook(() => useBlinkFrame(SKIN, off.doc(1), off.catalog, 'classic', false));
  await settle();
  expect(b.result.current).toBeNull();
  expect(off.layersAsked).not.toHaveBeenCalled();
  const plain = world();
  const plainDoc = { rev: 1, look: null, current: () => ({ look: null, pixels: plain.state.pixels, version: 1 }) } as unknown as SkinDoc;
  const c = renderHook(() => useBlinkFrame(SKIN, plainDoc, plain.catalog, 'classic', true));
  await settle();
  expect(c.result.current).toBeNull();
  expect(plain.layersAsked).not.toHaveBeenCalled();
});
