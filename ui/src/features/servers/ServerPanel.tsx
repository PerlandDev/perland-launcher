import { useTranslation } from 'react-i18next';
import type { Instance } from '../../ipc/schema';
import { useActiveAccountName, useServerStatus } from '../../state/queries';
import { SELECTED_POLL } from '../../state/serverPoll';
import { AddressLine } from './AddressLine';
import { DeadLinkPlate } from './DeadLinkPlate';
import { StatusPill } from './StatusPill';

/** A server instance's live state, in the slot the desktop spec reserved for it. */
export function ServerPanel({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const status = useServerStatus(instance.id, true, SELECTED_POLL);
  const account = useActiveAccountName();
  const saved = instance.server;
  const live = status.data;
  const gone = live?.gone ?? saved?.gone ?? false;
  const whitelisted = live?.whitelisted ?? saved?.whitelisted ?? false;
  return (
    <div className="mt-3 grid gap-2">
      <StatusPill status={live ? live.status : (saved?.lastStatus ?? null)} players={live?.players ?? null} gone={gone} />
      <AddressLine address={live?.address ?? saved?.address ?? null} />
      {whitelisted && (
        <p className="invite-only">{account ? t('join.inviteOnly', { name: account }) : t('join.inviteOnlyNoAccount')}</p>
      )}
      {gone && <DeadLinkPlate id={instance.id} />}
    </div>
  );
}
