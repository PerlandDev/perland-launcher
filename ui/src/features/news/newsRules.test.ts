import { createInstance, type TFunction } from 'i18next';
import { beforeAll, describe, expect, it } from 'vitest';
import en from '../../i18n/en.json';
import es from '../../i18n/es.json';
import type { NewsChangesView, NewsPostView } from '../../ipc/schema';
import { EVENT_DEFAULT_MS, eventLabel, isNew, isOver, latestLine, modpackSummary, newestCreatedAt } from './newsRules';

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

const now = new Date('2026-10-17T12:00:00Z'); // a Saturday; the tests run in UTC

const post = (over: Partial<NewsPostView> = {}): NewsPostView => ({
  id: '3f2b8c1e-5d4a-4e6b-9c7d-1a2b3c4d5e6f',
  type: 'update',
  title: 'A post',
  body: '',
  startsAt: null,
  endsAt: null,
  imageVersion: null,
  modpack: null,
  createdAt: '2026-10-10T10:00:00Z',
  ...over,
});
const event = (startsAt: string, over: Partial<NewsPostView> = {}) => post({ type: 'event', title: 'Build contest', startsAt, ...over });
/** Collapse the narrow no-break space newer ICU puts before "PM". */
const plain = (text: string | null) => text?.replace(/\s/g, ' ') ?? null;

describe('isOver', () => {
  it('is over at endsAt', () => {
    const e = event('2026-10-17T09:00:00Z', { endsAt: '2026-10-17T11:00:00Z' });
    expect(isOver(e, now)).toBe(true);
    expect(isOver(e, new Date('2026-10-17T10:59:59Z'))).toBe(false);
  });

  it('is over three hours after the start without endsAt', () => {
    expect(EVENT_DEFAULT_MS).toBe(3 * 60 * 60 * 1000);
    const e = event('2026-10-17T09:00:00Z');
    expect(isOver(e, now)).toBe(true);
    expect(isOver(e, new Date('2026-10-17T11:59:59Z'))).toBe(false);
  });

  it('is never over for a post that is not an event', () => {
    expect(isOver(post(), now)).toBe(false);
  });
});

describe('latestLine', () => {
  it('prefers an event that is not over to a newer update', () => {
    const upcoming = event('2026-10-18T18:00:00Z', { id: 'e', createdAt: '2026-10-01T10:00:00Z' });
    const newer = post({ id: 'u', createdAt: '2026-10-16T10:00:00Z' });
    expect(latestLine([newer, upcoming], now)?.id).toBe('e');
  });

  it('falls back to the newest post when every event is over', () => {
    const past = event('2026-10-10T18:00:00Z', { id: 'e', createdAt: '2026-10-01T10:00:00Z' });
    const newer = post({ id: 'u', createdAt: '2026-10-16T10:00:00Z' });
    const older = post({ id: 'o', createdAt: '2026-10-05T10:00:00Z' });
    expect(latestLine([older, past, newer], now)?.id).toBe('u');
  });

  it('is null without posts', () => {
    expect(latestLine([], now)).toBeNull();
  });
});

describe('newestCreatedAt', () => {
  it('compares times, not strings', () => {
    // As strings "…:00Z" sorts after "…:00.5Z", though it is half a second earlier.
    const posts = [post({ createdAt: '2026-10-16T10:00:00Z' }), post({ createdAt: '2026-10-16T10:00:00.5Z' })];
    expect(newestCreatedAt(posts)).toBe('2026-10-16T10:00:00.5Z');
    expect(newestCreatedAt([...posts].reverse())).toBe('2026-10-16T10:00:00.5Z');
  });
  it('is null without posts', () => {
    expect(newestCreatedAt([])).toBeNull();
  });
});

describe('isNew', () => {
  const posts = [post({ createdAt: '2026-10-16T10:00:00Z' }), post({ createdAt: '2026-10-12T10:00:00Z' })];

  it('uses the marker, not the clock', () => {
    expect(isNew(posts, '2026-10-15T00:00:00Z', now)).toBe(true);
    expect(isNew(posts, '2026-10-16T10:00:00Z', now)).toBe(false);
    expect(isNew(posts, '2026-10-20T00:00:00Z', now)).toBe(false);
    // A clock years ahead of the Engine changes nothing while there is a marker.
    expect(isNew(posts, '2026-10-15T00:00:00Z', new Date('2030-01-01T00:00:00Z'))).toBe(true);
  });

  it('on a first visit shows the dot only for a post from the last 7 days', () => {
    expect(isNew(posts, null, now)).toBe(true);
    expect(isNew(posts, null, new Date('2026-10-23T10:00:00Z'))).toBe(true);
    expect(isNew(posts, null, new Date('2026-10-23T10:00:01Z'))).toBe(false);
  });

  it('is false without posts', () => {
    expect(isNew([], null, now)).toBe(false);
    expect(isNew([], '2026-10-15T00:00:00Z', now)).toBe(false);
  });
});

describe('eventLabel', () => {
  it('is null for a post that is not an event', () => {
    expect(eventLabel(post(), now, tEn, 'en')).toBeNull();
  });

  it('says Now once the event started and is not over', () => {
    expect(eventLabel(event('2026-10-17T11:00:00Z'), now, tEn, 'en')).toBe('Now');
    expect(eventLabel(event('2026-10-17T11:00:00Z'), now, tEs, 'es')).toBe('Ahora');
  });

  it('says Today and Tomorrow with the time', () => {
    expect(plain(eventLabel(event('2026-10-17T20:00:00Z'), now, tEn, 'en'))).toBe('Today 8 PM');
    expect(plain(eventLabel(event('2026-10-18T20:30:00Z'), now, tEn, 'en'))).toBe('Tomorrow 8:30 PM');
    expect(eventLabel(event('2026-10-17T20:00:00Z'), now, tEs, 'es')).toBe('Hoy 20:00');
    expect(eventLabel(event('2026-10-18T20:00:00Z'), now, tEs, 'es')).toBe('Mañana 20:00');
  });

  it('names the weekday within 6 days', () => {
    expect(plain(eventLabel(event('2026-10-19T20:00:00Z'), now, tEn, 'en'))).toBe('Mon 8 PM');
    expect(plain(eventLabel(event('2026-10-23T20:00:00Z'), now, tEn, 'en'))).toBe('Fri 8 PM');
    expect(eventLabel(event('2026-10-19T20:00:00Z'), now, tEs, 'es')).toMatch(/^lun 20:00$/);
  });

  it('gives the date from the seventh day on', () => {
    expect(plain(eventLabel(event('2026-10-24T20:00:00Z'), now, tEn, 'en'))).toBe('Oct 24, 8 PM');
    expect(plain(eventLabel(event('2026-10-30T20:00:00Z'), now, tEn, 'en'))).toBe('Oct 30, 8 PM');
    expect(eventLabel(event('2026-10-30T20:00:00Z'), now, tEs, 'es')).toMatch(/^30 oct,? 20:00$/);
  });

  it('gives the date of an event that is over', () => {
    expect(plain(eventLabel(event('2026-10-10T20:00:00Z'), now, tEn, 'en'))).toBe('Oct 10, 8 PM');
  });
});

describe('modpackSummary', () => {
  const names = (count: number) => ({ count, names: [] as string[] });
  const changes = (added: number, removed: number, updated: number): NewsChangesView => ({
    added: names(added),
    removed: names(removed),
    updated: names(updated),
  });

  it('joins the parts and pluralizes in English', () => {
    expect(modpackSummary(changes(3, 1, 0), tEn)).toBe('3 mods added, 1 removed');
    expect(modpackSummary(changes(1, 2, 5), tEn)).toBe('1 mod added, 2 removed, 5 updated');
  });

  it('pluralizes in Spanish', () => {
    expect(modpackSummary(changes(3, 1, 0), tEs)).toBe('3 mods agregados, 1 quitado');
    expect(modpackSummary(changes(1, 0, 2), tEs)).toBe('1 mod agregado, 2 actualizados');
  });

  it('leaves out the parts with 0', () => {
    expect(modpackSummary(changes(0, 0, 4), tEn)).toBe('4 updated');
  });

  it('says the modpack changed without changes, or when every part is 0', () => {
    expect(modpackSummary(null, tEn)).toBe('The modpack changed.');
    expect(modpackSummary(null, tEs)).toBe('El modpack cambió.');
    expect(modpackSummary(changes(0, 0, 0), tEn)).toBe('The modpack changed.');
  });
});
