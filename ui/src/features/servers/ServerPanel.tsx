import { useTranslation } from 'react-i18next';
import type { Instance } from '../../ipc/schema';
import { useInstanceStats, useServerStatus } from '../../state/queries';
import { Trophy } from '../../ui/Icon';
import { SELECTED_POLL } from '../../state/serverPoll';
import { NewsLine } from '../news/NewsLine';
import { AddressLine } from './AddressLine';
import { DeadLinkPlate } from './DeadLinkPlate';
import { InviteOnly } from './InviteOnly';
import { StatusPill } from './StatusPill';

/** A server instance's live state, in the slot the desktop spec reserved for it: small plates under
 *  the version plate, so the art never decides their contrast. */
export function ServerPanel({ instance }: { instance: Instance }) {
  const status = useServerStatus(instance.id, true, SELECTED_POLL);
  const saved = instance.server;
  const live = status.data;
  const gone = live?.gone ?? saved?.gone ?? false;
  const whitelisted = live?.whitelisted ?? saved?.whitelisted ?? false;
  return (
    <div className="mt-2 grid justify-items-start gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <StatusPill status={live ? live.status : (saved?.lastStatus ?? null)} players={live?.players ?? null} gone={gone} />
        <AddressLine address={live?.address ?? saved?.address ?? null} />
      </div>
      {/* Home is open, so its news reads now; the line is absent without news. */}
      <NewsLine invite={instance.source.kind === 'perland' ? instance.source.invite : null} name={instance.name} visible onArt />
      <AchievementsPlate id={instance.id} />
      {whitelisted && <InviteOnly onArt />}
      {gone && <DeadLinkPlate id={instance.id} />}
    </div>
  );
}

/** The player's achievements on this server, on a laminate plate. Only the count: Home's version line already holds the
 *  time played on this computer, so the server's own playtime stays on the card. Absent without achievements. */
function AchievementsPlate({ id }: { id: string }) {
  const { t } = useTranslation();
  const stats = useInstanceStats(id);
  if (!stats || stats.advancements === 0) return null;
  return (
    <p className="stats-plate marquee-plate">
      <Trophy aria-hidden />
      <span className="sr-only">{t('servers.stats.label')} </span>
      <span>{t('servers.stats.achievements', { count: stats.advancements })}</span>
    </p>
  );
}
