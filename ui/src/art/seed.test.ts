import { describe, expect, it } from 'vitest';
import { artSeed, presetSeeds } from './seed';

describe('artSeed', () => {
  it('keeps a pinned seed and falls back to the id', () => {
    expect(artSeed({ id: 'asalith-fields', artSeed: null })).toBe('asalith-fields');
    expect(artSeed({ id: 'asalith-2', artSeed: 'asalith-fields' })).toBe('asalith-fields');
  });
});

describe('presetSeeds', () => {
  it('gives six unique seeds from the base', () => {
    const seeds = presetSeeds({ id: 'asalith-fields', artSeed: null });
    expect(seeds).toEqual(['asalith-fields', 'asalith-fields~2', 'asalith-fields~3', 'asalith-fields~4', 'asalith-fields~5', 'asalith-fields~6']);
    expect(new Set(seeds).size).toBe(6);
  });

  it('strips a picked preset suffix so the six stay the same', () => {
    const picked = presetSeeds({ id: 'renamed', artSeed: 'asalith-fields~3' });
    expect(picked).toEqual(presetSeeds({ id: 'asalith-fields', artSeed: null }));
    expect(picked).toContain('asalith-fields~3');
  });
});
