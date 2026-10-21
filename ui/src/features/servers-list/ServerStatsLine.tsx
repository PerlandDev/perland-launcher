import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import type { ServerStats } from '../../ipc/schema';
import { useServerStats } from '../../state/queries';
import { formatPlaytime } from '../../ui/playtime';
import { useAchievementsDialog } from '../medals/useAchievementsDialog';

/** "12 achievements · 30 h played": the achievements only when there are some, the time only from a minute up, '' with neither. */
// A pure helper kept beside the component that uses it; losing fast refresh for this file is harmless.
// eslint-disable-next-line react-refresh/only-export-components
export function statsText(t: TFunction, stats: ServerStats): string {
  const played = formatPlaytime(stats.playtimeSeconds, t);
  return [
    stats.advancements > 0 ? t('servers.stats.achievements', { count: stats.advancements }) : '',
    played ? t('servers.stats.played', { time: played }) : '',
  ]
    .filter(Boolean)
    .join(' · ');
}

/** The player's numbers on a server card, in the game line's quiet ink. With achievements to list, the line is a flat key
 *  that opens them. Nothing renders without stats (signed out, not linked, the add-on off) or with nothing to say. */
export function ServerStatsLine({ token }: { token: string }) {
  const { t } = useTranslation();
  const stats = useServerStats(token);
  const { show, dialog } = useAchievementsDialog(stats);
  const text = stats ? statsText(t, stats) : '';
  if (!stats || !text) return null;
  if (stats.advancementIds.length === 0) {
    return (
      <p className="tabular text-sm text-ink-dim">
        <span className="sr-only">{t('servers.stats.label')} </span>
        <span>{text}</span>
      </p>
    );
  }
  return (
    <>
      <button
        type="button"
        className="stats-key tabular text-sm text-ink-dim"
        aria-label={`${t('servers.stats.label')} ${text}. ${t('achievements.open', { server: stats.name })}`}
        onClick={show}
      >
        {text}
      </button>
      {dialog}
    </>
  );
}
