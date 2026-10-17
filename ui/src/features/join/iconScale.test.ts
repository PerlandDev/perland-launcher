import { describe, expect, it } from 'vitest';
import { cardIconSize, serverIconSize } from './iconScale';

describe('serverIconSize', () => {
  it.each([
    [1, 128], // 2 device px per icon px
    [1.25, 102.4], // 2 device px per icon px: 128 device px
    [1.5, 128], // 3 device px per icon px: 192 device px
    [1.75, 192 / 1.75], // 3 device px per icon px: 192 device px
    [2, 128], // 4 device px per icon px: 256 device px
  ])('at %s dppx is %s CSS px, a whole number of device pixels per icon pixel', (dpr, css) => {
    expect(serverIconSize(dpr)).toBeCloseTo(css, 6);
    expect(Number.isInteger(Math.round(serverIconSize(dpr) * dpr * 1e6) / 1e6 / 64)).toBe(true);
  });

  it('never shrinks below the icon itself', () => {
    expect(serverIconSize(0.4)).toBeCloseTo(64 / 0.4, 6);
  });
});

describe('cardIconSize', () => {
  it.each([
    [1, 64], // 1 device px per icon px
    [1.25, 51.2], // 1 device px per icon px: 64 device px
    [1.5, 128 / 1.5], // 2 device px per icon px: 128 device px
    [2, 64], // 2 device px per icon px
    [3, 64], // 3 device px per icon px
  ])('at %s dppx is %s CSS px, the whole number of device pixels per icon pixel nearest 64 CSS px', (dpr, css) => {
    expect(cardIconSize(dpr)).toBeCloseTo(css, 6);
    expect(Number.isInteger(Math.round(cardIconSize(dpr) * dpr * 1e6) / 1e6 / 64)).toBe(true);
  });
});
