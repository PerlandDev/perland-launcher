import type { Instance } from '../../ipc/schema';
import { useServerStatus } from '../../state/queries';
import { SELECTED_POLL } from '../../state/serverPoll';
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
      {whitelisted && <InviteOnly onArt />}
      {gone && <DeadLinkPlate id={instance.id} />}
    </div>
  );
}
