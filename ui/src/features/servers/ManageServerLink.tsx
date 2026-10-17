import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { inviteCanonical } from '../../ipc/inviteLink';
import type { Instance } from '../../ipc/schema';
import { useJoinPreview, useServerUnlink } from '../../state/queries';
import { navigate } from '../../state/route';
import { useRun } from '../../state/runStore';
import { pushNotice } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { CopyRow } from '../../ui/CopyRow';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { addressText, inviteOf } from '../home/labels';
import { ServerCard } from '../join/ServerCard';
import { InviteOnly } from './InviteOnly';

/** The confirmation before unlinking: what stays (everything) and what stops, in plain words. */
function UnlinkDialog({ instance, onBack, onDone }: { instance: Instance; onBack: () => void; onDone: () => void }) {
  const { t } = useTranslation();
  const unlink = useServerUnlink(instance.id, { inline: true });
  const server = instance.server?.name || instance.name;
  return (
    <Dialog
      open
      onClose={onBack}
      title={t('server.manage.unlinkTitle', { name: instance.name })}
      actions={
        <>
          <Button onClick={onBack}>{t('common.cancel')}</Button>
          <Button
            variant="danger"
            aria-disabled={unlink.isPending || undefined}
            onClick={() => {
              if (unlink.isPending) return;
              unlink.mutate(undefined, {
                onSuccess: () => {
                  pushNotice({ title: t('server.manage.unlinked', { name: instance.name }) });
                  onDone();
                },
              });
            }}
          >
            {unlink.isPending ? t('server.manage.unlinking') : t('server.manage.unlink')}
          </Button>
        </>
      }
    >
      <div className="grid gap-4">
        <p>{t('server.manage.unlinkBody', { name: instance.name, server })}</p>
        {unlink.isError && <ErrorCard error={unlink.error} />}
      </div>
    </Dialog>
  );
}

/** "Manage server link": the linked server as Join server shows it, the saved invite and address with Copy, then Change
 *  link (the relink screen) and Unlink (after a confirmation). Nothing is lit: Play keeps the room's one amber. A link that
 *  stopped working still shows what was saved, so the player can share it or move on. Mount it only while open. */
export function ManageServerLink({ instance, onClose }: { instance: Instance; onClose: () => void }) {
  const { t } = useTranslation();
  const [confirming, setConfirming] = useState(false);
  // A run saves the whole instance when it syncs, so the link changes only while it is idle.
  const running = useRun(instance.id).state !== 'idle';
  const invite = inviteOf(instance) ?? '';
  const preview = useJoinPreview(inviteCanonical(invite) ?? invite);
  const address = instance.server?.address ?? null;
  if (confirming) return <UnlinkDialog instance={instance} onBack={() => setConfirming(false)} onDone={onClose} />;
  const hint = running ? 'manage-link-running' : undefined;
  return (
    <Dialog
      open
      onClose={onClose}
      title={t('server.manage.title')}
      actions={
        <>
          <Button variant="ghost" className="mr-auto" disabled={running} aria-describedby={hint} onClick={() => setConfirming(true)}>
            {t('server.manage.unlink')}
          </Button>
          <Button
            disabled={running}
            aria-describedby={hint}
            onClick={() => {
              onClose();
              navigate({ name: 'joinServer', invite: '', relinkFor: instance.id });
            }}
          >
            {t('server.manage.change')}
          </Button>
          <Button onClick={onClose}>{t('common.close')}</Button>
        </>
      }
    >
      <div className="manage-link grid gap-5 text-ink">
        {preview.isLoading && (
          <p role="status" className="text-sm text-ink-dim">
            {t('server.manage.checking')}
          </p>
        )}
        {preview.isError && <ErrorCard error={preview.error} />}
        {preview.data && <ServerCard preview={preview.data} bare />}
        {preview.data?.whitelisted && <InviteOnly />}
        {invite && (
          <div className="grid gap-1.5">
            {/* The row names itself for screen readers; this is the same word for the eye. */}
            <p aria-hidden className="text-sm font-semibold">
              {t('server.manage.invite')}
            </p>
            <CopyRow text={invite} label={t('server.manage.invite')} copy={t('server.copyLink')} copied={t('server.manage.inviteCopied')} />
          </div>
        )}
        <div className="grid gap-1.5">
          <p aria-hidden className="text-sm font-semibold">
            {t('server.address')}
          </p>
          {address ? (
            <CopyRow text={addressText(address)} label={t('server.address')} copy={t('server.copyAddress')} copied={t('server.addressCopied')} />
          ) : (
            <p className="text-sm text-ink-dim">{t('server.noAddress')}</p>
          )}
        </div>
        {running && (
          <p id="manage-link-running" className="text-sm text-ink-dim">
            {t('server.manage.running')}
          </p>
        )}
      </div>
    </Dialog>
  );
}
