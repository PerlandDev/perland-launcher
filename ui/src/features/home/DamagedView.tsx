import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { DamagedInstance } from '../../ipc/schema';
import { useDeleteInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Folder, Trash } from '../../ui/Icon';

export function DamagedView({ damaged }: { damaged: DamagedInstance }) {
  const { t } = useTranslation();
  const [confirming, setConfirming] = useState(false);
  const remove = useDeleteInstance();
  return (
    <div className="grid h-full place-items-center p-10">
      <div className="grid w-full max-w-xl gap-6">
        <h1 className="font-display text-3xl">{t('damaged.title', { id: damaged.id })}</h1>
        <ErrorCard
          error={damaged.error}
          actions={
            <>
              <Button icon={<Folder />} onClick={() => commands.openFolder({ kind: 'instance', id: damaged.id }).catch(pushError)}>
                {t('common.openFolder')}
              </Button>
              <Button variant="danger" icon={<Trash />} onClick={() => setConfirming(true)}>
                {t('common.delete')}
              </Button>
            </>
          }
        />
      </div>
      {confirming && (
        <Dialog
          open
          onClose={() => setConfirming(false)}
          title={t('damaged.deleteTitle', { id: damaged.id })}
          actions={
            <>
              <Button onClick={() => setConfirming(false)}>{t('common.cancel')}</Button>
              <Button
                variant="danger"
                onClick={() =>
                  remove.mutate(damaged.id, {
                    onSuccess: () => {
                      setConfirming(false);
                      navigate({ name: 'home', id: null });
                    },
                  })
                }
              >
                {t('common.delete')}
              </Button>
            </>
          }
        >
          {t('damaged.deleteBody')}
        </Dialog>
      )}
    </div>
  );
}
