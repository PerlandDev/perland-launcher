import { createInstance, type TFunction } from 'i18next';
import { beforeAll, describe, expect, it } from 'vitest';
import en from '../i18n/en.json';
import es from '../i18n/es.json';
import { formatPlaytime } from './playtime';

let tEn: TFunction;
let tEs: TFunction;

beforeAll(async () => {
  const make = async (lng: string) => {
    const i18n = createInstance();
    await i18n.init({ resources: { en: { translation: en }, es: { translation: es } }, lng, fallbackLng: 'en' });
    return i18n.t;
  };
  tEn = await make('en');
  tEs = await make('es');
});

describe('formatPlaytime', () => {
  it('hides anything under a minute', () => {
    expect(formatPlaytime(0, tEn)).toBe('');
    expect(formatPlaytime(30, tEn)).toBe('');
  });

  it('reads in the two biggest units', () => {
    expect(formatPlaytime(59 * 60, tEn)).toBe('59 min');
    expect(formatPlaytime(3600, tEn)).toBe('1 h');
    expect(formatPlaytime(3 * 3600 + 5 * 60, tEn)).toBe('3 h 5 min');
    expect(formatPlaytime(2 * 86400 + 4 * 3600, tEn)).toBe('2 days 4 h');
    expect(formatPlaytime(86400, tEn)).toBe('1 day');
    expect(formatPlaytime(400 * 86400, tEn)).toBe('1 year 35 days');
  });

  it('speaks Spanish', () => {
    expect(formatPlaytime(2 * 86400, tEs)).toBe('2 días');
    expect(formatPlaytime(86400 + 3600, tEs)).toBe('1 día 1 h');
    expect(formatPlaytime(2 * 365 * 86400, tEs)).toBe('2 años');
  });

  it('fits the plate and the Accounts line in both languages', () => {
    expect(tEn('home.played', { time: '12 h' })).toBe('12 h played');
    expect(tEs('home.played', { time: '12 h' })).toBe('12 h de juego');
    expect(tEs('accounts.totalPlayed', { time: '2 días' })).toBe('Tiempo jugado en esta computadora: 2 días');
  });
});
