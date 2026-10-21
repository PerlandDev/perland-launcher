import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import type { ServerStats } from '../../ipc/schema';
import { usePerlandStats, usePerlandStatus, usePlaytime } from '../../state/queries';
import { ChevronRight } from '../../ui/Icon';
import { Meter } from '../../ui/Meter';
import { formatPlaytime } from '../../ui/playtime';
import { useAchievementsDialog } from './useAchievementsDialog';
import { MedalBadge } from './MedalBadge';
import { medals, type Medal } from './rules';

const tierName = (tier: number, t: TFunction) => t(`medals.tier${tier}`);

/** A ladder's reading in its unit: time in its two biggest units ("0 min" under a minute), a count as it is. */
function amount(medal: Medal, value: number, t: TFunction): string {
  if (medal.unit === 'count') return String(value);
  return formatPlaytime(value, t) || t('time.minutes', { count: 0 });
}

/** What earns the medal's next step, as a sentence: "Play 10 h on this computer", "Mine a diamond". */
function goalLine(medal: Medal, t: TFunction): string {
  if (medal.kind === 'single') return t(`medals.${medal.id}.goal`);
  const goal = medal.goal ?? 0;
  return medal.unit === 'time' ? t(`medals.${medal.id}.goal`, { goal: amount(medal, goal, t) }) : t(`medals.${medal.id}.goal`, { count: goal });
}

/** The medal's state under its name: what earns it while locked, else its tier and reading. */
function stateLine(medal: Medal, t: TFunction): string {
  if (medal.tier === 0) return goalLine(medal, t);
  if (medal.kind === 'single') return `${tierName(medal.tier, t)} · ${t('medals.earned')}`;
  return `${tierName(medal.tier, t)} · ${amount(medal, medal.value, t)}`;
}

function MedalItem({ medal }: { medal: Medal }) {
  const { t } = useTranslation();
  const ladderNext = medal.kind === 'ladder' && medal.goal !== null;
  const progress = ladderNext ? t('medals.progress', { value: amount(medal, medal.value, t), goal: amount(medal, medal.goal ?? 0, t) }) : '';
  return (
    <li className="medal" data-locked={medal.tier === 0 ? '' : undefined}>
      <MedalBadge id={medal.id} tier={medal.tier} />
      <div className="min-w-0 flex-1">
        <p className="font-bold">{t(`medals.${medal.id}.name`)}</p>
        <p className="text-sm text-ink-dim">
          {medal.tier === 0 && <span className="sr-only">{t('medals.locked')}: </span>}
          {stateLine(medal, t)}
        </p>
        {medal.kind === 'ladder' && medal.tier > 0 && (
          <p className="text-sm text-ink-dim">
            {medal.goal === null ? t('medals.top') : t('medals.next', { tier: tierName(medal.tier + 1, t), goal: amount(medal, medal.goal, t) })}
          </p>
        )}
        {ladderNext && (
          <div className="medal-meter">
            <Meter value={medal.value / (medal.goal ?? 1)} label={progress} segments={10} />
          </div>
        )}
      </div>
    </li>
  );
}

/** One server with achievements: its name and count, opening the list. */
function ServerRow({ server }: { server: ServerStats }) {
  const { t } = useTranslation();
  const { show, dialog } = useAchievementsDialog(server);
  return (
    <li>
      <button type="button" className="medal-server" aria-label={t('achievements.open', { server: server.name })} onClick={show}>
        <span className="min-w-0 flex-1 truncate font-bold">{server.name}</span>
        <span className="tabular text-sm text-ink-dim">{t('servers.stats.achievements', { count: server.advancements })}</span>
        <ChevronRight aria-hidden className="h-4 w-4 text-ink-dim" />
      </button>
      {dialog}
    </li>
  );
}

/** Accounts' medal case: the local time medal always, the server medals while Perland's stats are on (locked, with a line on
 *  linking, until a Minecraft account is linked), then the servers whose achievements can be opened. */
export function Medals() {
  const { t } = useTranslation();
  const playtime = usePlaytime();
  const status = usePerlandStatus();
  const stats = usePerlandStats();
  const statsOn = Boolean(stats.data);
  const linked = Boolean(status.data?.account?.minecraft);
  const localSeconds = playtime.data?.reduce((sum, p) => sum + p.seconds, 0) ?? 0;
  const list = medals({ localSeconds, stats: stats.data ?? null }).filter((m) => statsOn || !m.needsStats);
  const earned = list.filter((m) => m.tier > 0).length;
  const servers = (stats.data?.servers ?? []).filter((s) => s.advancementIds.length > 0);
  return (
    <section className="grid gap-4" aria-labelledby="medals-heading">
      <div className="flex items-baseline justify-between gap-4">
        <h2 id="medals-heading" className="text-lg font-bold">
          {t('medals.title')}
        </h2>
        <p className="tabular text-sm text-ink-dim">{t('medals.earnedCount', { earned, total: list.length })}</p>
      </div>
      {statsOn && !linked && <p className="text-sm text-ink-dim">{t('medals.linkToEarn')}</p>}
      <ul className="medal-case panel">
        {list.map((medal) => (
          <MedalItem key={medal.id} medal={medal} />
        ))}
      </ul>
      {servers.length > 0 && (
        <section className="grid gap-2" aria-labelledby="medal-servers-heading">
          <h3 id="medal-servers-heading" className="font-bold">
            {t('medals.servers')}
          </h3>
          <ul className="grid gap-1">
            {servers.map((server) => (
              <ServerRow key={server.token} server={server} />
            ))}
          </ul>
        </section>
      )}
    </section>
  );
}
