// When a skin version was kept, as the history list and the preview say it.
import type { TFunction } from 'i18next';

/** "Today, 3:42 PM", "Yesterday, 9:05 AM" or "Nov 5, 3:42 PM" for Unix seconds `taken`. */
export function whenLabel(taken: number, lang: string, t: TFunction, now = new Date()): string {
  const at = new Date(taken * 1000);
  const time = new Intl.DateTimeFormat(lang, { hour: 'numeric', minute: '2-digit' }).format(at);
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(now) - day(at)) / 86_400_000);
  if (days === 0) return t('skins.history.today', { time });
  if (days === 1) return t('skins.history.yesterday', { time });
  const date = new Intl.DateTimeFormat(lang, { month: 'short', day: 'numeric', ...(at.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }) }).format(at);
  return t('skins.history.onDay', { date, time });
}
