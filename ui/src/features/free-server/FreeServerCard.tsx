import type { TFunction } from 'i18next';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { FreeServerCard as FreeServer, PublicServerCard as Card } from '../../ipc/schema';
import { PublicServerCard } from '../servers-list/PublicServerCard';
import { useManageItem, useManageKey } from '../servers-list/yours';
import { DeleteServerDialog } from './DeleteServerDialog';
import { usePlayFreeServer } from './usePlayFreeServer';

const LIST_STATUSES: readonly string[] = ['online', 'starting', 'sleeping', 'offline'];
/** Statuses where Play can't do anything: the server isn't there yet, is going, or is out of space. */
const UNPLAYABLE: readonly string[] = ['creating', 'deleting', 'stopped'];

/** The player's own free server, first in Your servers: the public card tagged Owner, with its warning lines, its own status
 *  words while it is created or deleted, Play through `free_server_play`, Manage beside Play when the launcher can run it (else
 *  Manage in Perland in its menu, with a panel), and Delete server as its ×. */
export function FreeServerCard({ server, onDeleting }: { server: FreeServer; onDeleting?: (going: boolean) => void }) {
  const { t, i18n } = useTranslation();
  const [deleting, setDeleting] = useState(false);
  const play = usePlayFreeServer();
  const manage = useManageItem();
  const manageKey = useManageKey();
  const busy = server.freeStatus === 'creating' || server.freeStatus === 'deleting';
  const unplayable = UNPLAYABLE.includes(server.freeStatus) || server.disk.over;
  // The card takes its pill, Play and address from the Engine's own word. A word the app doesn't know keeps the card's status.
  const card: Card = {
    ...server,
    status: pillStatus(server),
    joinable: server.invite !== null && !unplayable,
    // Out of space or not there yet, the address would only invite a join that can't happen.
    address: unplayable ? null : server.address,
  };
  return (
    <>
      <PublicServerCard
        card={card}
        tag="owner"
        follow={false}
        notes={notes(t, i18n.language, server)}
        statusWord={statusWord(t, server.freeStatus)}
        onPlay={play.run}
        primary={manageKey(server.manage)}
        // With Manage on the card, More in Perland is on the Manage screen.
        menuItems={server.manage ? undefined : manage(server.name)}
        remove={{ label: t('freeServer.delete.key', { name: server.displayName || server.name }), disabled: busy, onSelect: () => setDeleting(true) }}
      />
      {deleting && <DeleteServerDialog name={server.name} onClose={() => setDeleting(false)} onDeleting={onDeleting} />}
    </>
  );
}

function pillStatus(server: FreeServer): Card['status'] {
  if (server.freeStatus === 'creating' || server.freeStatus === 'deleting') return 'starting';
  if (server.freeStatus === 'stopped') return 'offline';
  return LIST_STATUSES.includes(server.freeStatus) ? (server.freeStatus as Card['status']) : server.status;
}

function statusWord(t: TFunction, freeStatus: string): string | undefined {
  if (freeStatus === 'creating') return t('freeServer.card.creating');
  if (freeStatus === 'deleting') return t('freeServer.card.deleting');
  return undefined;
}

/** The warning lines: deletion soon (once the player was told), and out of space. The date is the day only, in the UI's language. */
function notes(t: TFunction, lang: string, server: FreeServer): string[] {
  const lines: string[] = [];
  const at = server.deletion?.warned ? new Date(server.deletion.deletesAt) : null;
  if (at && !Number.isNaN(at.getTime())) {
    lines.push(t('freeServer.card.deletes', { date: new Intl.DateTimeFormat(lang, { day: 'numeric', month: 'short' }).format(at) }));
  }
  if (server.disk.over || server.freeStatus === 'stopped') lines.push(t('freeServer.card.outOfSpace'));
  return lines;
}
