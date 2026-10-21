import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { InstanceIcon } from '../../art/InstanceIcon';
import type { ManagedServer } from '../../ipc/schema';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { cardIconSize, useServerIconSize } from '../join/iconScale';
import { manageStateView } from '../manage/stateView';
import { StatusPill } from '../servers/StatusPill';
import { JoinFromMinecraft } from './PublicServerCard';

/** A server the player runs that no Your servers card shows (no invite the launcher knows, so no Play): the server tile with
 *  its name, its team, the state in Manage's words, the address with Copy, and Manage as the key that lights with the card.
 *  The art is Manage's own, seeded the same way, so the card and the screen it opens look like one server. */
export function ManagedServerCard({ server }: { server: ManagedServer }) {
  const { t } = useTranslation();
  const nameId = useId();
  const size = useServerIconSize(cardIconSize);
  const view = manageStateView(t, server.state, null);
  return (
    <article className="server-tile" aria-labelledby={nameId}>
      <div className="server-tile-head">
        <span className="server-icon" aria-hidden>
          <InstanceIcon name={server.stack} seed={`${server.tenantId}/${server.stack}`} size={size} />
        </span>
        <div className="grid min-w-0 content-start gap-1">
          <h3 id={nameId} className="server-tile-name">
            {server.name}
          </h3>
          <p className="text-sm text-ink-dim">{server.teamName}</p>
          <span className="server-tile-status">
            <StatusPill status={view.led} players={null} gone={false} word={view.word} live={false} />
          </span>
        </div>
      </div>
      {server.address && (
        <div className="server-tile-facts">
          <JoinFromMinecraft address={server.address} />
        </div>
      )}
      <Button
        size="lg"
        className="server-tile-play"
        aria-describedby={nameId}
        onClick={() => navigate({ name: 'manage', tenantId: server.tenantId, stack: server.stack })}
      >
        {t('servers.yours.manageHere')}
      </Button>
    </article>
  );
}
