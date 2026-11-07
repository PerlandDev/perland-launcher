import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { CreatableTeam, FreeServerPage } from '../../ipc/schema';
import { qk } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { CreateServerDialog } from './CreateServerDialog';
import { EmailCodeStep } from './EmailCodeStep';
import { reasonLine } from './freeServer';

/** The slot in Your servers where a new server starts. With teams the player may create on, "Create a server", whose dialog
 *  asks Where (the free server while they can make it, or a team). Without, the free server's slot: "Create a server for
 *  friends", or one line saying why the player can't now, and nothing once they have one (its card shows). The dialog stays
 *  open after the server appears, until closed. Missing an email, the slot also offers "Add an email", which opens the email
 *  step in the launcher; once verified, the free server's page is read again. `page` is null while free servers are off. */
export function FreeServerEntry({ page, teams = [] }: { page: FreeServerPage | null; teams?: CreatableTeam[] }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const hasServer = page?.server != null;
  const reason = page ? reasonLine(t, page.create, hasServer) : null;
  const qc = useQueryClient();
  const [addingEmail, setAddingEmail] = useState(false);
  // Kept from when the dialog opened, so it outlives the free server's entry once the server exists.
  const freeOpen = page && !hasServer && page.create.allowed ? page.limits : null;
  const [openLimits, setOpenLimits] = useState(freeOpen);
  let slot = null;
  if (teams.length > 0) {
    slot = (
      <div className="empty-slot free-entry">
        <p>{t(freeOpen ? 'teamServer.entry.lead' : 'teamServer.entry.leadTeams')}</p>
        <Button
          onClick={() => {
            setOpenLimits(freeOpen);
            setOpen(true);
          }}
        >
          {t('teamServer.entry.create')}
        </Button>
      </div>
    );
  } else if (page && !hasServer && page.create.allowed) {
    slot = (
      <div className="empty-slot free-entry">
        <p>{t('freeServer.entry.lead', { count: page.limits.maxPlayers })}</p>
        <Button
          onClick={() => {
            setOpenLimits(page.limits);
            setOpen(true);
          }}
        >{t('freeServer.entry.create')}</Button>
      </div>
    );
  } else if (reason && page?.create.reason === 'email_required') {
    slot = (
      <div className="empty-slot free-entry">
        <p>{reason}</p>
        <Button onClick={() => setAddingEmail(true)}>{t('freeServer.entry.addEmail')}</Button>
      </div>
    );
  } else if (reason) {
    slot = <p className="empty-slot">{reason}</p>;
  }
  return (
    <>
      {slot}
      {open && <CreateServerDialog limits={openLimits} teams={teams} onClose={() => setOpen(false)} />}
      {addingEmail && (
        <Dialog open onClose={() => setAddingEmail(false)} title={t('freeServer.email.title')}>
          <EmailCodeStep
            onVerified={() => {
              void qc.invalidateQueries({ queryKey: qk.freeServer });
              void qc.invalidateQueries({ queryKey: qk.perland });
              setAddingEmail(false);
            }}
            onCancel={() => setAddingEmail(false)}
          />
        </Dialog>
      )}
    </>
  );
}
