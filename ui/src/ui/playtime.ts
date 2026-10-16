import type { TFunction } from 'i18next';

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const YEAR = 365 * DAY;

/** Time played in its two biggest units ("3 h 5 min", "1 year 35 days"), or '' under a minute,
 *  so a game that never really ran shows nothing rather than "0 min". */
export function formatPlaytime(seconds: number, t: TFunction): string {
  if (seconds < MINUTE) return '';
  const minutes = Math.floor(seconds / MINUTE);
  const hours = Math.floor(seconds / HOUR);
  const days = Math.floor(seconds / DAY);
  const years = Math.floor(seconds / YEAR);
  const unit = (key: string, count: number) => (count > 0 ? t(`time.${key}`, { count }) : '');
  const pair = (big: string, bigCount: number, small: string, smallCount: number) =>
    [unit(big, bigCount), unit(small, smallCount)].filter(Boolean).join(' ');
  if (seconds < HOUR) return unit('minutes', minutes);
  if (seconds < DAY) return pair('hours', hours, 'minutes', minutes % 60);
  if (seconds < YEAR) return pair('days', days, 'hours', hours % 24);
  return pair('years', years, 'days', days % 365);
}
