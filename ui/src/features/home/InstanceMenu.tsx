import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { useDeleteInstance, useRenameInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Folder, Gear, Pencil, Trash } from '../../ui/Icon';
import { Menu } from '../../ui/Menu';

function RenameDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
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

function DeleteDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
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

export function InstanceMenu({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const [dialog, setDialog] = useState<'rename' | 'delete' | null>(null);
  const close = () => setDialog(null);
  return (
    <>
      <Menu
        label={t('home.more')}
        items={[
          { label: t('home.instanceSettings'), icon: <Gear />, onSelect: () => navigate({ name: 'instanceSettings', id: instance.id }) },
          { label: t('common.rename'), icon: <Pencil />, onSelect: () => setDialog('rename') },
          {
            label: t('common.openFolder'),
            icon: <Folder />,
            onSelect: () => void commands.openFolder({ kind: 'instance', id: instance.id }).catch(pushError),
          },
          { label: t('common.delete'), icon: <Trash />, danger: true, onSelect: () => setDialog('delete') },
        ]}
      />
      {dialog === 'rename' && <RenameDialog instance={instance} onClose={close} />}
      {dialog === 'delete' && <DeleteDialog instance={instance} onClose={close} />}
    </>
  );
}
