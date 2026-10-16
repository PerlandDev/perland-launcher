import { expect, it } from 'vitest';
import { initials, terraceHeights } from './monogram';

it.each([
  ['Asalith Fields', 'AF'],
  ['creative', 'C'],
  ['my-cool_pack', 'MC'],
  ['  ñandú  ', 'Ñ'],
  ['1.21 Survival', '1S'],
  ['!!!', ''],
  ['👍 pack', 'P'],
])('takes the initials of %j as %j', (name, want) => {
  expect(initials(name)).toBe(want);
});

it('keeps a letter with a combining mark whole', () => {
  expect(initials('ñandu éxito')).toBe('ÑÉ');
});

it("flattens the seed's plot terraces into a quiet base under the letters", () => {
  const heights = terraceHeights('asalith-fields');
  expect(heights).toHaveLength(8);
  for (const h of heights) {
    expect(h).toBeGreaterThanOrEqual(8);
    expect(h).toBeLessThanOrEqual(14);
  }
  expect(terraceHeights('asalith-fields')).toEqual(heights);
});
