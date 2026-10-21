import { useMutation, useQueryClient } from '@tanstack/react-query';
import type { TFunction } from 'i18next';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance, RestoreChange, RestorePoint } from '../../ipc/schema';
import { qk } from '../../state/queries';
import { runStore } from '../../state/runStore';
import { pushError, pushNotice } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { formatBytes } from '../../ui/format';
import { WarningDiamond } from '../../ui/Icon';

const SHOWN = 3;
const MIN_SIZE = 1_000_000;

function changeText(t: TFunction, change: RestoreChange): string {
  switch (change.kind) {
    case 'packUpdate':
      return t('restore.change.packUpdate', { to: change.to });
    case 'installed':
      return t('restore.change.installed', { name: change.name });
    case 'removed':
      return t('restore.change.removed', { name: change.name });
  }
}

/** "updated to 1.4.2, installed Sodium, installed Lithium and 2 more": at most three, then a count. */
function changesList(t: TFunction, lang: string, changes: RestoreChange[]): string | null {
  if (changes.length === 0) return null;
  const shown = changes.slice(0, SHOWN).map((change) => changeText(t, change));
  const rest = changes.length - shown.length;
  if (rest > 0) return `${shown.join(', ')} ${t('restore.more', { count: rest })}`;
  return new Intl.ListFormat(lang, { type: 'conjunction' }).format(shown);
}

/** The confirm every "Go back" opens: what it undoes, what it keeps, and what a server will say. */
export function GoBackDialog({
  instance,
  point,
  onClose,
}: {
  instance: Instance;
  point: RestorePoint;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const id = instance.id;
  const goBack = useMutation({
    mutationFn: () => commands.restoreGoBack(id),
    onSuccess: () => {
      // Going back puts back files, the pack's bookkeeping and the game version.
      const keys = [qk.restorePoint(id), qk.instances, qk.mods(id), qk.packStatus(id), qk.packContents(id), qk.contentFiles(id, 'resource_pack'), qk.contentFiles(id, 'shader')];
      for (const queryKey of keys) void qc.invalidateQueries({ queryKey });
      pushNotice({ title: t('restore.done', { name: instance.name }) });
      // The last run's news (Updated to X, or the crash) no longer describes the files.
      runStore.dismissExit(id);
      onClose();
    },
    onError: pushError,
  });
  const list = changesList(t, i18n.language, point.changes);
  return (
    <Dialog
      open
      onClose={onClose}
      title={t('restore.title', { name: instance.name })}
      actions={
        <>
          <Button onClick={onClose}>{t('common.cancel')}</Button>
          <Button
            variant="primary"
            aria-disabled={goBack.isPending || undefined}
            onClick={() => {
              if (!goBack.isPending) goBack.mutate();
            }}
          >
            {t('restore.confirm')}
          </Button>
        </>
      }
    >
      <div className="grid max-w-prose gap-3 pt-1">
        {list && <p className="text-ink">{t('restore.undoes', { list })}</p>}
        <p>{t('restore.safe')}</p>
        {point.sizeBytes >= MIN_SIZE && (
          <p className="tabular text-sm">{t('restore.size', { size: formatBytes(point.sizeBytes, i18n.language) })}</p>
        )}
        {point.server && (
          <p className="flex items-start gap-2 font-semibold text-ink">
            <WarningDiamond className="mt-0.5 h-4 w-4 flex-none text-coral" aria-hidden />
            <span>{t('restore.serverWarning', { server: point.server })}</span>
          </p>
        )}
      </div>
    </Dialog>
  );
}
