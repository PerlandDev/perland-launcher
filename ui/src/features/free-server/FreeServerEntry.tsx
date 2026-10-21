import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { FreeServerPage } from '../../ipc/schema';
import { useAppInfo } from '../../state/queries';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { CreateServerDialog } from './CreateServerDialog';
import { reasonLine } from './freeServer';

/** The slot in Your servers where a free server would go: "Create a server for friends", or one line saying why the player
 *  can't now. Nothing once they have one (its card shows). The dialog stays open after the server appears, until closed.
 *  Missing an email, the slot also offers "Add an email", which opens the panel's account page, while a panel is configured. */
export function FreeServerEntry({ page }: { page: FreeServerPage }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const hasServer = page.server !== null;
  const reason = reasonLine(t, page.create, hasServer);
  const panel = useAppInfo().data?.panel === true;
  let slot = null;
  if (!hasServer && page.create.allowed) {
    slot = (
      <div className="empty-slot free-entry">
        <p>{t('freeServer.entry.lead', { count: page.limits.maxPlayers })}</p>
        <Button onClick={() => setOpen(true)}>{t('freeServer.entry.create')}</Button>
      </div>
    );
  } else if (reason && panel && page.create.reason === 'email_required') {
    slot = (
      <div className="empty-slot free-entry">
        <p>{reason}</p>
        <Button onClick={() => void commands.perlandOpenAccount().catch(pushError)}>{t('freeServer.entry.addEmail')}</Button>
      </div>
    );
  } else if (reason) {
    slot = <p className="empty-slot">{reason}</p>;
  }
  return (
    <>
      {slot}
      {open && <CreateServerDialog limits={page.limits} onClose={() => setOpen(false)} />}
    </>
  );
}
