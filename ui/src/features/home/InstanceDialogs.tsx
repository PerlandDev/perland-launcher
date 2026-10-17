import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { Instance } from '../../ipc/schema';
import { useDeleteInstance, useRenameInstance } from '../../state/queries';
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
            onClick={() =>
              remove.mutate(instance.id, {
                onSuccess: () => {
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
      {t('home.deleteBody')}
    </Dialog>
  );
}
