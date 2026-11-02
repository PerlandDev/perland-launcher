import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { inviteCanonical } from '../../ipc/inviteLink';
import type { Instance } from '../../ipc/schema';
import { useDeleteInstance, useForget, useRenameInstance } from '../../state/queries';
import { pushError } from '../../state/toasts';
import { SwitchRow } from '../../ui/SwitchRow';
import { useYours } from '../servers-list/yours';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';

export function RenameDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
  const { t } = useTranslation();
  const [name, setName] = useState(instance.name);
  const rename = useRenameInstance({ inline: true });
  return (
    <Dialog
      open
      onClose={onClose}
      title={t('home.renameTitle')}
      actions={
        <>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button
            variant="primary"
            disabled={!name.trim() || rename.isPending}
            onClick={() => rename.mutate({ id: instance.id, name }, { onSuccess: onClose })}
          >
            {t('common.save')}
          </Button>
        </>
      }
    >
      <div className="grid gap-4 pt-2">
        <Field label={t('home.nameLabel')}>
          <input className="field-input" value={name} maxLength={64} onChange={(e) => setName(e.target.value)} />
        </Field>
        {rename.isError && <ErrorCard error={rename.error} />}
      </div>
    </Dialog>
  );
}

export function DeleteDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
  const { t } = useTranslation();
  const remove = useDeleteInstance();
  const forget = useForget();
  // The invited server this instance joins (by its invite link, so a list read before the instance existed still matches),
  // while it is on Your servers: deleting the instance can take it off too.
  const { servers } = useYours();
  const joins = instance.source.kind === 'perland' ? inviteCanonical(instance.source.invite) : null;
  const invited =
    (joins !== null && servers?.find((s) => s.relation === 'invited' && s.invite !== null && inviteCanonical(s.invite) === joins)) || null;
  const [alsoForget, setAlsoForget] = useState(true);
  return (
    <Dialog
      open
      onClose={onClose}
      title={t('home.deleteTitle', { name: instance.name })}
      actions={
        <>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button
            variant="danger"
            disabled={remove.isPending}
            onClick={() =>
              remove.mutate(instance.id, {
                onSuccess: () => {
                  if (invited && alsoForget) forget.mutate(invited.invite as string, { onError: pushError });
                  onClose();
                  navigate({ name: 'home', id: null });
                },
              })
            }
          >
            {t('home.deleteConfirm')}
          </Button>
        </>
      }
    >
      <div className="grid gap-5">
        <p>{t('home.deleteBody')}</p>
        {invited && (
          <SwitchRow
            label={t('home.deleteForget')}
            hint={t('home.deleteForgetHint', { name: invited.displayName || invited.name })}
            checked={alsoForget}
            onChange={setAlsoForget}
          />
        )}
      </div>
    </Dialog>
  );
}
