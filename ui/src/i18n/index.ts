import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import type { Lang } from '../ipc/schema';
import en from './en.json';
import es from './es.json';

/** The OS language decides until the player picks one: any Spanish locale gets Spanish. */
export function detectLang(language: string | undefined): Lang {
  return language?.toLowerCase().startsWith('es') ? 'es' : 'en';
}

void i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, es: { translation: es } },
  lng: detectLang(typeof navigator === 'undefined' ? undefined : navigator.language),
  fallbackLng: 'en',
  interpolation: { escapeValue: false },
});

export default i18n;
