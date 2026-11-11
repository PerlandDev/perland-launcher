import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CatalogPiece, SkinLook } from '../../ipc/schema';
import { compose, type ComposeLayer } from './compose';
import { putOn } from './look';
import { hoverPreviews, sameTexels, TRY_DELAY_MS, TryOn, tryState, type Candidate } from './tryOn';

const texels = (fill: number[]) => {
  const out = new Uint8ClampedArray(64 * 64 * 4);
  for (let i = 0; i < out.length; i += 4) out.set(fill, i);
  return out;
};

const TEE: CatalogPiece = {
  id: 'top-tee',
  slot: 'top',
  name: { en: 'T-shirt', es: 'Camiseta' },
  tags: [],
  areas: [{ id: 'main', name: { en: 'Main', es: 'Principal' }, default: '#e8e4dc' }],
  files: { classic: { art: 'a', mask: 'm' } },
  thumb: 't',
  retired: false,
};

describe('tryState', () => {
  it('a plain skin tries a piece over its own texels, as the first real piece would go on', () => {
    const pixels = texels([10, 20, 30, 255]);
    const tried = tryState({ look: null, pixels }, (look) => putOn(look, TEE, [TEE]));
    expect(tried).not.toBeNull();
    expect(tried!.look.pieces.map((p) => p.id)).toEqual(['top-tee']);
    expect(tried!.painting).toBe(pixels);
  });

  it('a skin with a look keeps its painting and its other pieces', () => {
    const look: SkinLook = { pieces: [{ id: 'body-plain', colours: {} }], painting: true };
    const painting = texels([1, 2, 3, 255]);
    const before = JSON.stringify(look);
    const tried = tryState({ look: { look, painting }, pixels: texels([9, 9, 9, 255]) }, (l) => putOn(l, TEE, [TEE]));
    expect(tried!.look.pieces.map((p) => p.id)).toEqual(['body-plain', 'top-tee']);
    expect(tried!.painting).toBe(painting);
    // The look it started from is left as it was.
    expect(JSON.stringify(look)).toBe(before);
  });

  it('is nothing when the change hands the same look back', () => {
    const look: SkinLook = { pieces: [], painting: false };
    expect(tryState({ look: { look, painting: null }, pixels: texels([0, 0, 0, 0]) }, (l) => l)).toBeNull();
  });

  it('composes to what wearing the piece would: the painting under the top', () => {
    const painting = texels([10, 20, 30, 255]);
    const art = texels([128, 128, 128, 255]);
    const mask = texels([0, 0, 0, 0]);
    const tried = tryState({ look: null, pixels: painting }, (look) => putOn(look, TEE, [TEE]))!;
    const layers: ComposeLayer[] = [{ slot: 'top', id: 'top-tee', art, mask, colours: [] }];
    const shown = compose(tried.painting, layers);
    expect([...shown.slice(0, 4)]).toEqual([128, 128, 128, 255]);
    // Nothing it was given changed.
    expect([...painting.slice(0, 4)]).toEqual([10, 20, 30, 255]);
  });
});

it('sameTexels tells equal buffers from different ones', () => {
  expect(sameTexels(texels([1, 2, 3, 4]), texels([1, 2, 3, 4]))).toBe(true);
  const other = texels([1, 2, 3, 4]);
  other[4000] = 9;
  expect(sameTexels(texels([1, 2, 3, 4]), other)).toBe(false);
});

it('only a mouse or a pen hovers: a finger gets no preview', () => {
  expect(hoverPreviews('mouse')).toBe(true);
  expect(hoverPreviews('pen')).toBe(true);
  expect(hoverPreviews('touch')).toBe(false);
});

describe('TryOn', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  const candidate = (key: string): Candidate => ({ key, name: key, change: (look) => look });
  /** A controller whose loads are answered by hand. */
  function setup() {
    const pending = new Map<string, (result: string | null) => void>();
    const loads: string[] = [];
    const shown: (string | null)[] = [];
    const tryOn = new TryOn<string>(
      (c) => {
        loads.push(c.key);
        return new Promise((resolve) => pending.set(c.key, resolve));
      },
      (s) => shown.push(s ? `${s.candidate.key}:${s.result}` : null),
    );
    const answer = async (key: string, result: string | null) => {
      pending.get(key)!(result);
      await vi.advanceTimersByTimeAsync(0);
    };
    return { tryOn, loads, shown, answer };
  }

  it('waits before it loads, then shows what loaded', async () => {
    const { tryOn, loads, shown, answer } = setup();
    tryOn.point(candidate('a'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS - 1);
    expect(loads).toEqual([]);
    await vi.advanceTimersByTimeAsync(1);
    expect(loads).toEqual(['a']);
    expect(shown).toEqual([]);
    await answer('a', 'A');
    expect(shown).toEqual(['a:A']);
  });

  it('a pointer swept across tiles loads none of the ones it merely crossed', async () => {
    const { tryOn, loads, shown, answer } = setup();
    for (const key of ['a', 'b', 'c', 'd']) {
      tryOn.point(candidate(key));
      await vi.advanceTimersByTimeAsync(40);
      tryOn.leave(key);
    }
    tryOn.point(candidate('e'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    expect(loads).toEqual(['e']);
    await answer('e', 'E');
    expect(shown).toEqual(['e:E']);
  });

  it('a newer hover cancels an older one that is still loading', async () => {
    const { tryOn, loads, shown, answer } = setup();
    tryOn.point(candidate('a'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    tryOn.point(candidate('b'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    expect(loads).toEqual(['a', 'b']);
    // The older one lands last: it is dropped.
    await answer('b', 'B');
    await answer('a', 'A');
    expect(shown).toEqual(['b:B']);
  });

  it('leaving returns to the real skin after a short grace, at once on clear', async () => {
    const { tryOn, shown, answer } = setup();
    tryOn.point(candidate('a'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    await answer('a', 'A');
    tryOn.leave('a');
    expect(shown).toEqual(['a:A']);
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    expect(shown).toEqual(['a:A', null]);
    tryOn.point(candidate('b'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    await answer('b', 'B');
    tryOn.clear();
    expect(shown).toEqual(['a:A', null, 'b:B', null]);
  });

  it('moving straight to the next tile swaps the preview without a flash of the real skin', async () => {
    const { tryOn, shown, answer } = setup();
    tryOn.point(candidate('a'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    await answer('a', 'A');
    tryOn.leave('a');
    await vi.advanceTimersByTimeAsync(20);
    tryOn.point(candidate('b'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    await answer('b', 'B');
    expect(shown).toEqual(['a:A', 'b:B']);
  });

  it('leaving while it loads shows nothing, and a late leave of another tile is ignored', async () => {
    const { tryOn, shown, answer } = setup();
    tryOn.point(candidate('a'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    tryOn.leave('a');
    await answer('a', 'A');
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    expect(shown).toEqual([null]);
    tryOn.point(candidate('b'));
    tryOn.leave('a');
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    await answer('b', 'B');
    expect(shown).toEqual([null, 'b:B']);
  });

  it('a change with nothing to show clears what was shown', async () => {
    const { tryOn, shown, answer } = setup();
    tryOn.point(candidate('a'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    await answer('a', 'A');
    tryOn.point(candidate('worn'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    await answer('worn', null);
    expect(shown).toEqual(['a:A', null]);
  });

  it('a click stops a hover that has not started, and keeps the tile pointed for later', async () => {
    const { tryOn, loads, shown, answer } = setup();
    tryOn.point(candidate('a'));
    await vi.advanceTimersByTimeAsync(50);
    tryOn.commit();
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS * 2);
    expect(loads).toEqual([]);
    // The skin changed (the click landed): the pointed tile is tried again on what is there now.
    tryOn.refresh();
    expect(loads).toEqual(['a']);
    await answer('a', null);
    expect(shown).toEqual([null]);
    tryOn.leave('a');
    tryOn.refresh();
    expect(loads).toEqual(['a']);
  });

  it('pointing the tile already pointed starts nothing new', async () => {
    const { tryOn, loads } = setup();
    tryOn.point(candidate('a'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    tryOn.point(candidate('a'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    expect(loads).toEqual(['a']);
  });

  it('a load that fails shows the real skin', async () => {
    const shown: unknown[] = [];
    const tryOn = new TryOn<string>(() => Promise.reject(new Error('offline')), (s) => shown.push(s));
    tryOn.point(candidate('a'));
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS);
    expect(shown).toEqual([null]);
  });

  it('dispose stops everything', async () => {
    const { tryOn, loads, shown } = setup();
    tryOn.point(candidate('a'));
    tryOn.dispose();
    await vi.advanceTimersByTimeAsync(TRY_DELAY_MS * 2);
    expect(loads).toEqual([]);
    expect(shown).toEqual([]);
  });
});
