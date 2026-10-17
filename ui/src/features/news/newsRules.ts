import type { TFunction } from 'i18next';
import type { NewsChangesView, NewsPostView } from '../../ipc/schema';

/** An event with no `endsAt` is over this long after it starts. */
export const EVENT_DEFAULT_MS = 3 * 60 * 60 * 1000;
const DAY_MS = 24 * 60 * 60 * 1000;
const FIRST_VISIT_MS = 7 * DAY_MS;

const time = (iso: string | null): number | null => {
  if (iso === null) return null;
  const ms = Date.parse(iso);
  return Number.isNaN(ms) ? null : ms;
};

/** Has this event ended? It ends at `endsAt`, else three hours after it starts. A post that is not an event never ends. */
export function isOver(post: NewsPostView, now: Date): boolean {
  const start = time(post.startsAt);
  if (post.type !== 'event' || start === null) return false;
  const end = time(post.endsAt) ?? start + EVENT_DEFAULT_MS;
  return now.getTime() >= end;
}

const newestFirst = (a: NewsPostView, b: NewsPostView) => (time(b.createdAt) ?? 0) - (time(a.createdAt) ?? 0);

/** The post the line shows: the newest event that isn't over (what a player can still act on), else the newest post. */
export function latestLine(posts: NewsPostView[], now: Date): NewsPostView | null {
  const byNewest = [...posts].sort(newestFirst);
  return byNewest.find((p) => p.type === 'event' && p.startsAt !== null && !isOver(p, now)) ?? byNewest[0] ?? null;
}

/** Does the new dot show? The newest post is after the marker (Engine time, so a wrong clock changes nothing).
 *  With no marker yet, only for a post from the last 7 days by this computer's clock. */
export function isNew(posts: NewsPostView[], seen: string | null, now: Date): boolean {
  const newest = Math.max(...posts.map((p) => time(p.createdAt) ?? -Infinity));
  if (!Number.isFinite(newest)) return false;
  const marker = time(seen);
  if (marker !== null) return newest > marker;
  return now.getTime() - newest <= FIRST_VISIT_MS;
}

/** The number of calendar days from `now` to `at` in the computer's time zone. */
function daysAhead(at: Date, now: Date): number {
  const day = (d: Date) => Date.UTC(d.getFullYear(), d.getMonth(), d.getDate());
  return Math.round((day(at) - day(now)) / DAY_MS);
}

/** An event's time in words: "Now", "Today 8 PM", "Tomorrow 8 PM", "Sat 8 PM", "Oct 30, 8 PM". Null for a post that is not an event.
 *  Runs on the computer's time zone, through `Intl.DateTimeFormat` in the UI language. */
export function eventLabel(post: NewsPostView, now: Date, t: TFunction, locale: string): string | null {
  const start = time(post.startsAt);
  if (post.type !== 'event' || start === null) return null;
  if (start <= now.getTime() && !isOver(post, now)) return t('news.now');
  const at = new Date(start);
  const clock = new Intl.DateTimeFormat(locale, at.getMinutes() === 0 ? { hour: 'numeric' } : { hour: 'numeric', minute: '2-digit' }).format(at);
  const days = daysAhead(at, now);
  if (days === 0) return t('news.today', { time: clock });
  if (days === 1) return t('news.tomorrow', { time: clock });
  if (days >= 2 && days <= 6) return `${new Intl.DateTimeFormat(locale, { weekday: 'short' }).format(at)} ${clock}`;
  return `${new Intl.DateTimeFormat(locale, { month: 'short', day: 'numeric' }).format(at)}, ${clock}`;
}

/** "3 mods added, 1 removed", with the parts that are 0 left out; "The modpack changed." when there is nothing to count. */
export function modpackSummary(changes: NewsChangesView | null, t: TFunction): string {
  const parts = changes
    ? ([
        ['news.modpack.added', changes.added.count],
        ['news.modpack.removed', changes.removed.count],
        ['news.modpack.updated', changes.updated.count],
      ] as const)
        .filter(([, count]) => count > 0)
        .map(([key, count]) => t(key, { count }))
    : [];
  return parts.length > 0 ? parts.join(', ') : t('news.modpack.changed');
}
