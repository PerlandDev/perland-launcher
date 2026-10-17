import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useFreeServerDelete } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';

/** The confirmation before the free server goes: its consequence first, then the coral Delete server. A failure, or a delete
 *  the Engine did not do (`deleted: false`: the server is still being created), stays in the dialog. `onDeleting(true)` is
 *  called when Confirm is pressed and `onDeleting(false)` when nothing was deleted after all, so the caller can move focus
 *  once the card is gone, whenever that is. Mount it only while open. */
export function DeleteServerDialog({
  name,
  onClose,
  onDeleting,
}: {
  name: string;
  onClose: () => void;
  onDeleting?: (going: boolean) => void;
}) {
  const { t } = useTranslation();
  const remove = useFreeServerDelete({ inline: true });
  const [notDeleted, setNotDeleted] = useState(false);
  return (
    <Dialog
      open
      onClose={onClose}
      title={t('freeServer.delete.title', { name })}
      actions={
        <>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button
            variant="danger"
            aria-disabled={remove.isPending || undefined}
            onClick={() => {
              if (remove.isPending) return;
              setNotDeleted(false);
              onDeleting?.(true);
              remove.mutate(undefined, {
                onSuccess: (deleted) => {
                  if (deleted) return onClose();
                  onDeleting?.(false);
                  setNotDeleted(true);
                },
                onError: () => onDeleting?.(false),
              });
            }}
          >
            {remove.isPending ? t('freeServer.delete.deleting') : t('freeServer.delete.confirm')}
          </Button>
        </>
      }
    >
      <div className="grid gap-4">
        <p>{t('freeServer.delete.body')}</p>
        {notDeleted && (
          <p role="alert" className="text-sm">
            {t('freeServer.delete.notDeleted')}
          </p>
        )}
        {remove.isError && <ErrorCard error={remove.error} />}
      </div>
    </Dialog>
  );
}
