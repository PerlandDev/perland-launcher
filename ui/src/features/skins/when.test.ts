import { expect, it } from 'vitest';
import i18n from '../../i18n';
import { whenLabel } from './when';

const t = i18n.getFixedT('en');
const now = new Date(2026, 10, 7, 18, 0);
const at = (d: Date) => Math.floor(d.getTime() / 1000);

it('says today, yesterday, a date this year, and the year for an older one', () => {
  expect(whenLabel(at(new Date(2026, 10, 7, 15, 42)), 'en', t, now)).toBe('Today, 3:42 PM');
  expect(whenLabel(at(new Date(2026, 10, 6, 9, 5)), 'en', t, now)).toBe('Yesterday, 9:05 AM');
  expect(whenLabel(at(new Date(2026, 10, 4, 15, 42)), 'en', t, now)).toBe('Nov 4, 3:42 PM');
  expect(whenLabel(at(new Date(2025, 10, 4, 15, 42)), 'en', t, now)).toBe('Nov 4, 2025, 3:42 PM');
});
