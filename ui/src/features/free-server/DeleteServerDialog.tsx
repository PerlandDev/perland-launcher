import { useTranslation } from 'react-i18next';
import { useFreeServerDelete } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';

/** The confirmation before the free server goes: its consequence first, then the coral Delete server. A failure stays in the
 *  dialog. Mount it only while open. */
export function DeleteServerDialog({ name, onClose, onDeleted }: { name: string; onClose: () => void; onDeleted?: () => void }) {
  const { t } = useTranslation();
  const remove = useFreeServerDelete({ inline: true });
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
              remove.mutate(undefined, {
                onSuccess: () => {
                  onClose();
                  onDeleted?.();
                },
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
        {remove.isError && <ErrorCard error={remove.error} />}
      </div>
    </Dialog>
  );
}
