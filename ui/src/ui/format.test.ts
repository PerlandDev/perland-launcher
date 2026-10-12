import { expect, it } from 'vitest';
import { formatBytes, formatCompact, formatCount, formatGb } from './format';

it('formats sizes for players', () => {
  expect(formatBytes(312_000_000, 'en')).toBe('312 MB');
  expect(formatBytes(1_500_000_000, 'en')).toBe('1.5 GB');
  expect(formatBytes(900, 'en')).toBe('1 KB');
  expect(formatCount(3575, 'en')).toBe('3,575');
  expect(formatCount(35750, 'es')).toMatch(/35.750/); // es only groups from five digits
  expect(formatGb(6144, 'en')).toBe('6');
  expect(formatGb(2560, 'en')).toBe('2.5');
});

it('abbreviates big counts', () => {
  expect(formatCompact(81_000_000, 'en')).toBe('81M');
  expect(formatCompact(1_250, 'en')).toBe('1.3K');
});
