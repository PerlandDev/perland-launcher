import type { TFunction } from 'i18next';
import type { MedalView } from '../../ipc/schema';
import { formatPlaytime } from '../../ui/playtime';

// A medal in words: its tier, its reading, what earns it and when it was earned. Shared by Medals and achievements and the
// Profile's look at the medals, so both say the same thing about the same medal.

export const tierName = (tier: number, t: TFunction) => t(`medals.tier${tier}`);

/** A ladder's reading in its unit: time in its two biggest units ("0 min" under a minute), a count as it is. */
export function amount(medal: MedalView, value: number, t: TFunction): string {
  if (medal.unit === 'count') return String(value);
  return formatPlaytime(value, t) || t('time.minutes', { count: 0 });
}

/** A step's threshold, short enough for its column: whole hours for time ("100 h"), as the ladder defines them. */
export function stepLabel(medal: MedalView, goal: number, t: TFunction): string {
  return medal.unit === 'time' ? t('time.hours', { count: goal / 3600 }) : String(goal);
}

/** What earns the medal's next step, as a sentence: "Play 10 h on this computer", "Mine a diamond". */
export function goalLine(medal: MedalView, t: TFunction): string {
  if (medal.kind === 'single') return t(`medals.${medal.id}.goal`);
  const goal = medal.goal ?? 0;
  return medal.unit === 'time' ? t(`medals.${medal.id}.goal`, { goal: amount(medal, goal, t) }) : t(`medals.${medal.id}.goal`, { count: goal });
}

/** The medal's state under its name: what earns it while locked, else its tier and reading. */
export function stateLine(medal: MedalView, t: TFunction): string {
  if (medal.tier === 0) return goalLine(medal, t);
  if (medal.kind === 'single') return `${tierName(medal.tier, t)} · ${t('medals.earned')}`;
  return `${tierName(medal.tier, t)} · ${amount(medal, medal.value, t)}`;
}

/** The day a tier was reached, in the reader's language ("7 nov 2026"); empty for a date that doesn't read. */
export function day(at: string, lang: string): string {
  const date = new Date(at);
  return Number.isNaN(date.getTime()) ? '' : new Intl.DateTimeFormat(lang, { day: 'numeric', month: 'short', year: 'numeric' }).format(date);
}
