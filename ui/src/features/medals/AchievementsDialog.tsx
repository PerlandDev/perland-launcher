import { useTranslation } from 'react-i18next';
import type { ServerStats } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { Check, Lock } from '../../ui/Icon';
import { advancementName, groupAchievements, groupName, isKnown } from './advancements';

/** The player's achievements on one server, grouped like the game's tabs: earned first, then the vanilla ones not earned yet,
 *  dimmed behind a lock, so a tab reads as a checklist. Mods list what was earned only. Nothing in it is lit. */
export function AchievementsDialog({ server, open, onClose }: { server: ServerStats; open: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const groups = groupAchievements(server.advancementIds);
  const modded = server.advancementIds.some((id) => !isKnown(id));
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t('achievements.title', { server: server.name })}
      actions={
        <Button variant="ghost" onClick={onClose}>
          {t('common.close')}
        </Button>
      }
    >
      <p className="tabular">
        {t('servers.stats.achievements', { count: server.advancements })}
        <span className="text-ink-dim"> · {t('achievements.serverKeeps')}</span>
      </p>
      {/* Only the list scrolls, so Close stays in view; it takes focus first, at the top. */}
      <section className="achievements-list" tabIndex={0} aria-label={t('achievements.listLabel')}>
        {groups.map((group) => {
          const heading = groupName(group.group, t);
          return (
            <section key={group.group} className="achievements-group" aria-label={heading}>
              <h3 className="achievements-head">
                <span>{heading}</span>
                <span className="tabular text-ink-dim">
                  {group.total === null
                    ? group.earned.length
                    : t('achievements.count', { earned: group.earned.length, total: Math.max(group.total, group.earned.length) })}
                </span>
              </h3>
              <ul className="achievements-items">
                {group.earned.map((id) => (
                  <li key={id} data-earned="">
                    <Check aria-hidden />
                    <span className="sr-only">{t('achievements.gotIt')}: </span>
                    <span>{advancementName(id, t)}</span>
                  </li>
                ))}
                {group.locked.map((id) => (
                  <li key={id}>
                    <Lock aria-hidden />
                    <span className="sr-only">{t('achievements.notYet')}: </span>
                    <span>{advancementName(id, t)}</span>
                  </li>
                ))}
              </ul>
            </section>
          );
        })}
        {modded && <p className="mt-4 text-sm text-ink-dim">{t('achievements.modNote')}</p>}
      </section>
    </Dialog>
  );
}
