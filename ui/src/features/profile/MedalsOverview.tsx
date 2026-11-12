import { useTranslation } from 'react-i18next';
import type { MedalView } from '../../ipc/schema';
import { useMedals, usePerlandStats } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Trophy } from '../../ui/Icon';
import { Meter } from '../../ui/Meter';
import { MedalBadge } from '../medals/MedalBadge';
import { amount, day, stateLine, tierName } from '../medals/words';

/** How many of each the overview shows: enough to say what is happening, never the whole list. */
const LATEST = 3;
const NEXT = 2;

/** The medals earned most recently, newest first; medals earned on the same day keep the catalogue's order. */
function latest(earned: MedalView[]): MedalView[] {
  return [...earned].sort((a, b) => (b.earnedAt ?? '').localeCompare(a.earnedAt ?? '')).slice(0, LATEST);
}

/** The ladders closest to their next tier, among those the player has started on. */
function nextUp(list: MedalView[]): MedalView[] {
  const share = (m: MedalView) => m.value / (m.goal ?? 1);
  return list
    .filter((m) => m.kind === 'ladder' && m.goal !== null && m.value > 0)
    .sort((a, b) => share(b) - share(a))
    .slice(0, NEXT);
}

/** The Profile's look at the medals: how many are earned, the latest ones with their date, and the ones closest to their
 *  next tier with how far along they are. It is a glance, not the list: what every medal is and what earns it is on Medals
 *  and achievements, which its key opens. With the Minecraft account linked it also says how many of the game's own
 *  achievements the player has on Perland servers. */
export function MedalsOverview() {
  const { t, i18n } = useTranslation();
  const view = useMedals();
  // Reading the stats also hands them to Rust, which counts the Perland medals from them (as Medals and achievements does).
  const stats = usePerlandStats();
  const list = view.data?.medals ?? [];
  const earned = list.filter((m) => m.tier > 0);
  const recent = latest(earned);
  const next = nextUp(list);
  const servers = (stats.data?.servers ?? []).filter((s) => s.advancementIds.length > 0);
  const achievements = servers.reduce((sum, s) => sum + s.advancements, 0);
  const name = (medal: MedalView) => t(`medals.${medal.id}.name`);
  return (
    <section className="panel profile-card profile-medals-card" aria-labelledby="profile-medals">
      <div className="flex items-baseline justify-between gap-4">
        <h2 id="profile-medals" className="profile-title">
          {t('medals.medalsTitle')}
        </h2>
        {view.isSuccess && <p className="tabular text-sm text-ink-dim">{t('medals.earnedOf', { earned: earned.length, total: list.length })}</p>}
      </div>
      {view.isSuccess && earned.length === 0 && <p className="text-ink-dim">{t('profile.medals.none')}</p>}
      {recent.length > 0 && (
        <div className="grid gap-1">
          <h3 id="profile-medals-latest" className="profile-subtitle">
            {t('profile.medals.latest')}
          </h3>
          <ul className="profile-medal-rows" aria-labelledby="profile-medals-latest">
            {recent.map((medal) => {
              const on = medal.earnedAt ? day(medal.earnedAt, i18n.language) : '';
              return (
                <li key={medal.id} className="profile-medal-row">
                  <MedalBadge id={medal.id} tier={medal.tier} size={40} />
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-bold">{name(medal)}</p>
                    <p className="truncate text-sm text-ink-dim">{stateLine(medal, t)}</p>
                  </div>
                  {on && (
                    <p className="tabular text-sm text-ink-dim">
                      <span className="sr-only">{t('medals.earnedOn', { date: on })}</span>
                      <span aria-hidden>{on}</span>
                    </p>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      )}
      {next.length > 0 && (
        <div className="grid gap-1">
          <h3 id="profile-medals-next" className="profile-subtitle">
            {t('profile.medals.next')}
          </h3>
          <ul className="profile-medal-rows" aria-labelledby="profile-medals-next">
            {next.map((medal) => {
              const goal = amount(medal, medal.goal ?? 0, t);
              return (
                <li key={medal.id} className="profile-medal-row">
                  <MedalBadge id={medal.id} tier={medal.tier} size={40} />
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-bold">{name(medal)}</p>
                    {/* It wraps rather than truncates: what the next tier takes is the point of the row. */}
                    <p className="text-sm text-ink-dim">{t('medals.next', { tier: tierName(medal.tier + 1, t), goal })}</p>
                  </div>
                  <div className="profile-medal-meter">
                    <Meter value={medal.value / (medal.goal ?? 1)} label={t('medals.progress', { value: amount(medal, medal.value, t), goal })} segments={10} />
                  </div>
                </li>
              );
            })}
          </ul>
        </div>
      )}
      {achievements > 0 && (
        <p className="text-sm text-ink-dim">
          {t('profile.medals.achievements', { count: achievements })} {t('profile.medals.onServers', { count: servers.length })}
        </p>
      )}
      <div>
        <Button icon={<Trophy aria-hidden />} onClick={() => navigate({ name: 'achievements' })}>
          {t('titlebar.achievements')}
        </Button>
      </div>
    </section>
  );
}
