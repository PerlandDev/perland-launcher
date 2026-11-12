import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import type { MedalView, ServerStats } from '../../ipc/schema';
import { useAccounts, useMedals, usePerlandStats, usePerlandStatus } from '../../state/queries';
import { ChevronRight } from '../../ui/Icon';
import { Meter } from '../../ui/Meter';
import { formatPlaytime } from '../../ui/playtime';
import { useAchievementsDialog } from './useAchievementsDialog';
import { MedalBadge } from './MedalBadge';
import { navigate } from '../../state/route';

const tierName = (tier: number, t: TFunction) => t(`medals.tier${tier}`);

/** A ladder's reading in its unit: time in its two biggest units ("0 min" under a minute), a count as it is. */
function amount(medal: MedalView, value: number, t: TFunction): string {
  if (medal.unit === 'count') return String(value);
  return formatPlaytime(value, t) || t('time.minutes', { count: 0 });
}

/** A step's threshold, short enough for its column: whole hours for time ("100 h"), as the ladder defines them. */
function stepLabel(medal: MedalView, goal: number, t: TFunction): string {
  return medal.unit === 'time' ? t('time.hours', { count: goal / 3600 }) : String(goal);
}

/** What earns the medal's next step, as a sentence: "Play 10 h on this computer", "Mine a diamond". */
function goalLine(medal: MedalView, t: TFunction): string {
  if (medal.kind === 'single') return t(`medals.${medal.id}.goal`);
  const goal = medal.goal ?? 0;
  return medal.unit === 'time' ? t(`medals.${medal.id}.goal`, { goal: amount(medal, goal, t) }) : t(`medals.${medal.id}.goal`, { count: goal });
}

/** The medal's state under its name: what earns it while locked, else its tier and reading. */
function stateLine(medal: MedalView, t: TFunction): string {
  if (medal.tier === 0) return goalLine(medal, t);
  if (medal.kind === 'single') return `${tierName(medal.tier, t)} · ${t('medals.earned')}`;
  return `${tierName(medal.tier, t)} · ${amount(medal, medal.value, t)}`;
}

/** The day a tier was reached, in the reader's language ("7 nov 2026"); empty for a date that doesn't read. */
function day(at: string, lang: string): string {
  const date = new Date(at);
  return Number.isNaN(date.getTime()) ? '' : new Intl.DateTimeFormat(lang, { day: 'numeric', month: 'short', year: 'numeric' }).format(date);
}

function MedalItem({ medal, tag = false }: { medal: MedalView; tag?: boolean }) {
  const { t, i18n } = useTranslation();
  const earnedOn = medal.tier > 0 && medal.earnedAt ? day(medal.earnedAt, i18n.language) : '';
  const ladderNext = medal.kind === 'ladder' && medal.goal !== null;
  const progress = ladderNext ? t('medals.progress', { value: amount(medal, medal.value, t), goal: amount(medal, medal.goal ?? 0, t) }) : '';
  return (
    <li className="medal" data-locked={medal.tier === 0 ? '' : undefined}>
      <MedalBadge id={medal.id} tier={medal.tier} />
      <div className="min-w-0 flex-1">
        <p className="flex flex-wrap items-center gap-2 font-bold">
          {t(`medals.${medal.id}.name`)}
          {tag && <span className="medal-tag">{t('medals.perlandTag')}</span>}
        </p>
        <p className="text-sm text-ink-dim">
          {medal.tier === 0 && <span className="sr-only">{t('medals.locked')}: </span>}
          {stateLine(medal, t)}
        </p>
        {medal.kind === 'ladder' && medal.tier > 0 && (
          <ol className="medal-steps" aria-label={t('medals.steps')}>
            {medal.steps.map((goal, i) => {
              const step = i + 1;
              const got = medal.tier >= step;
              return (
                <li key={step} className="medal-step" data-locked={got ? undefined : ''}>
                  <MedalBadge id={medal.id} tier={got ? step : 0} size={24} />
                  <span className="tabular text-xs">{stepLabel(medal, goal, t)}</span>
                  <span className="sr-only">
                    {tierName(step, t)}, {got ? t('medals.earned') : t('medals.locked')}
                  </span>
                </li>
              );
            })}
          </ol>
        )}
        {medal.kind === 'ladder' && medal.tier > 0 && (
          <p className="text-sm text-ink-dim">
            {medal.goal === null ? t('medals.top') : t('medals.next', { tier: tierName(medal.tier + 1, t), goal: amount(medal, medal.goal, t) })}
          </p>
        )}
        {earnedOn && <p className="text-xs text-ink-dim">{t('medals.earnedOn', { date: earnedOn })}</p>}
        {ladderNext && medal.tier > 0 && (
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

/** What the reader still lacks for the Perland medals and the Minecraft achievements: signing in (to Perland or to a Microsoft
 *  account) or, with both, linking them. Both open Accounts, where the link is made; nothing once the accounts are linked. */
function AccountsLink() {
  const { t } = useTranslation();
  const status = usePerlandStatus();
  const accounts = useAccounts();
  if (status.data?.account?.minecraft) return null;
  const microsoft = accounts.data?.accounts.some((a) => a.provider === 'microsoft') ?? false;
  const label = status.data?.account && microsoft ? t('medals.linkAccounts') : t('medals.signIn');
  return (
    <>
      {' '}
      <button type="button" className="text-blue-ink underline underline-offset-4" onClick={() => navigate({ name: 'accounts' })}>
        {label}
      </button>
    </>
  );
}

/** The Medals and achievements screen: the medals Rust keeps (earned ones with their date, then the ones still to earn, with
 *  what the Perland ones need), then the Perland servers whose Minecraft achievements can be opened. Reading the stats here
 *  also hands them to Rust, which counts the Perland medals from them. */
export function Medals() {
  const { t } = useTranslation();
  const view = useMedals();
  const status = usePerlandStatus();
  const stats = usePerlandStats();
  const list = view.data?.medals ?? [];
  const earnedList = list.filter((m) => m.tier > 0);
  const lockedLocal = list.filter((m) => m.tier === 0 && m.group === 'local');
  const lockedPerland = list.filter((m) => m.tier === 0 && m.group === 'perland');
  const signedIn = Boolean(status.data?.account);
  const saved = view.data?.savedToAccount ?? signedIn;
  const servers = (stats.data?.servers ?? []).filter((s) => s.advancementIds.length > 0);
  return (
    <div className="grid gap-12">
      <section className="grid gap-6" aria-labelledby="medals-heading">
        <div className="grid gap-1">
          <div className="flex items-baseline justify-between gap-4">
            <h2 id="medals-heading" className="text-xl font-bold">{t('medals.medalsTitle')}</h2>
            <p className="tabular text-sm text-ink-dim">{t('medals.earnedOf', { earned: earnedList.length, total: list.length })}</p>
          </div>
          <p className="text-sm text-ink-dim">{t('medals.medalsHint')}</p>
          <p className="medal-saved text-sm">{saved ? t('medals.savedAccount') : t('medals.savedLocal')}</p>
        </div>
        <section className="grid gap-3" aria-labelledby="medals-earned">
          <div className="flex items-baseline justify-between gap-4">
            <h3 id="medals-earned" className="font-bold">{t('medals.earnedTitle')}</h3>
            <p className="tabular text-sm text-ink-dim">{earnedList.length}</p>
          </div>
          <ul className="medal-case panel">
            {earnedList.map((medal) => (
              <MedalItem key={medal.id} medal={medal} tag={medal.group === 'perland'} />
            ))}
          </ul>
        </section>
        <section className="grid gap-3" aria-labelledby="medals-locked">
          <div className="flex items-baseline justify-between gap-4">
            <h3 id="medals-locked" className="font-bold">{t('medals.lockedTitle')}</h3>
            <p className="tabular text-sm text-ink-dim">{lockedLocal.length + lockedPerland.length}</p>
          </div>
          {lockedPerland.length > 0 && (
            <p className="text-sm text-ink-dim">
              {t('medals.perlandHint')}
              <AccountsLink />
            </p>
          )}
          <ul className="medal-case panel" data-unearned="">
            {[...lockedLocal, ...lockedPerland].map((medal) => (
              <MedalItem key={medal.id} medal={medal} tag={medal.group === 'perland'} />
            ))}
          </ul>
        </section>
      </section>
      <section className="grid gap-3" aria-labelledby="medal-servers-heading">
        <div className="flex flex-wrap items-center gap-2">
          <h2 id="medal-servers-heading" className="text-xl font-bold">
            {t('medals.mcTitle')}
          </h2>
          <span className="medal-tag">{t('medals.perlandTag')}</span>
        </div>
        <p className="text-sm text-ink-dim">
          {t('medals.mcHint')}
          <AccountsLink />
        </p>
        {signedIn && servers.length > 0 && (
          <ul className="grid gap-1">
            {servers.map((server) => (
              <ServerRow key={server.token} server={server} />
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
