import { createInstance } from 'i18next';
import { describe, expect, it } from 'vitest';
import errorCodes from '../ipc/contract/error_codes.json';
import en from './en.json';
import es from './es.json';
import { detectLang } from './index';

type Tree = { [key: string]: string | Tree };

function flatten(tree: Tree, prefix = ''): Record<string, string> {
  return Object.entries(tree).reduce<Record<string, string>>((out, [key, value]) => {
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof value === 'string') out[path] = value;
    else Object.assign(out, flatten(value, path));
    return out;
  }, {});
}

const placeholders = (text: string) => [...text.matchAll(/{{(\w+)}}/g)].map((m) => m[1]).sort();

describe('translations', () => {
  const flatEn = flatten(en as Tree);
  const flatEs = flatten(es as Tree);

  it('have the same keys in English and Spanish', () => {
    expect(Object.keys(flatEs).sort()).toEqual(Object.keys(flatEn).sort());
  });

  it('use the same placeholders in both languages', () => {
    for (const [key, text] of Object.entries(flatEn)) {
      expect(placeholders(flatEs[key] ?? ''), key).toEqual(placeholders(text));
    }
  });

  it('translate every error code the core can produce', () => {
    for (const code of errorCodes) {
      expect(flatEn[`errors.${code}.message`], code).toBeTruthy();
      expect(flatEs[`errors.${code}.message`], code).toBeTruthy();
    }
  });

  it('give every error context a plain fallback in both languages', () => {
    // describeError only looks a context up when the plain key exists, so a lone
    // `message_<context>` or `fix_<context>` would never show.
    for (const flat of [flatEn, flatEs]) {
      const contexts = Object.keys(flat).filter((k) => /^errors\.\w+\.(message|fix)_\w+$/.test(k));
      expect(contexts.length).toBeGreaterThan(0);
      for (const key of contexts) {
        expect(flat[key.replace(/_\w+$/, '')], key).toBeTruthy();
      }
    }
  });

  it('never leave a Spanish string identical to a long English one by accident', () => {
    const same = Object.keys(flatEn).filter(
      (k) => flatEn[k] === flatEs[k] && (flatEn[k]?.length ?? 0) > 24 && !k.startsWith('app.'),
    );
    expect(same).toEqual([]);
  });
});

describe('Spanish counts', () => {
  // Spanish has a "many" plural for exact multiples of a million; without it i18next falls back to English.
  it('stay Spanish at a million', async () => {
    const i18n = createInstance();
    await i18n.init({ resources: { en: { translation: en }, es: { translation: es } }, lng: 'es', fallbackLng: 'en' });
    expect(i18n.t('browse.downloads', { count: 1000000, shown: '1 M' })).toBe('1 M descargas');
    expect(i18n.t('browse.follows', { count: 2000000, shown: '2 M' })).toBe('2 M seguidores');
    expect(i18n.t('browse.results', { count: 1000000, shown: '1.000.000' })).toBe('1.000.000 resultados');
    expect(i18n.t('install.addedMore', { count: 1000000, name: 'Sodium', instance: 'Asalith' })).toBe('Se agregó Sodium y 1000000 más a Asalith');
  });
});

describe('detectLang', () => {
  it('picks Spanish for any Spanish locale and English otherwise', () => {
    expect(detectLang('es-AR')).toBe('es');
    expect(detectLang('ES')).toBe('es');
    expect(detectLang('en-US')).toBe('en');
    expect(detectLang('pt-BR')).toBe('en');
    expect(detectLang(undefined)).toBe('en');
  });
});
