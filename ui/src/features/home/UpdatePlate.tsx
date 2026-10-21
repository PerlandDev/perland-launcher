import { useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { qk, usePackStatus } from '../../state/queries';
import { useRun } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Undo } from '../../ui/Icon';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { GoBackDialog } from './GoBackDialog';
import { changeLine, isServer } from './labels';
import { useRestorePoint } from './useRestorePoint';

/** The marquee plate above the name. After a sync it says what changed; between plays it
 *  offers the update (never lit: Play is the one lit button). */
export function UpdatePlate({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const run = useRun(instance.id);
  const status = usePackStatus(instance.id, true);
  const updateOnce = useSingleFlight();
  const qc = useQueryClient();
  const point = useRestorePoint(instance.id).data ?? null;
  const [confirming, setConfirming] = useState(false);
  if (run.state !== 'idle') return null;
  if (instance.updateHold) {
    // The player went back: updates wait for them, and the plate keeps saying so (it has no dismiss).
    const again = () =>
      updateOnce(
        (done) =>
          void commands
            .packUpdateAgain(instance.id)
            .then(() => qc.invalidateQueries({ queryKey: qk.instances }))
            .catch(pushError)
            .finally(done),
      );
    return (
      <div className="marquee-plate plate-row mb-3" data-lamp="info">
        <span role="status">
          {isServer(instance)
            ? t('restore.heldServer', { server: instance.server?.name || instance.name })
            : t('restore.held')}
        </span>
        <Button onClick={again}>{t('restore.updateAgain')}</Button>
      </div>
    );
  }
  const synced = run.lastExit?.synced ?? null;
  if (synced) {
    const kept = synced.keptModified.length;
    const setAside = synced.setAside.length;
    return (
      <div className="marquee-plate plate-row mb-3" data-lamp="ok">
        <span role="status">
          {t('update.updatedTo', { version: synced.toVersion })}
          {kept > 0 && ` · ${t('update.kept', { count: kept })}`}
          {setAside > 0 && ` · ${t('update.setAside', { count: setAside })}`}
        </span>
        {point?.kind === 'beforeChange' && (
          // Quiet on purpose: a ghost key beside the news, so Play stays the one thing that lights.
          <Button variant="ghost" icon={<Undo />} onClick={() => setConfirming(true)}>
            {t('restore.plateAction')}
          </Button>
        )}
        {confirming && point && <GoBackDialog instance={instance} point={point} onClose={() => setConfirming(false)} />}
      </div>
    );
  }
  const update = status.data?.update ?? null;
  if (isServer(instance)) {
    // Server instances always update when played: the plate only says so, with no Update button.
    return update ? (
      <p className="marquee-plate plate-row mb-3" data-lamp="info" role="status">
        {t('server.updated')}
      </p>
    ) : null;
  }
  if (update) {
    const counts = { added: update.added.length, updated: update.updated.length, removed: update.removed.length };
    return (
      <div className="marquee-plate plate-row mb-3" data-lamp="info" role="status">
        <span className="font-semibold">
          {update.fromVersion
            ? t('update.title', { from: update.fromVersion, to: update.toVersion })
            : t('update.titleNew', { to: update.toVersion })}
        </span>
        <span className="tabular text-ink-dim">{changeLine(t, counts)}</span>
        <Button onClick={() => updateOnce((done) => void commands.packUpdate(instance.id).catch(pushError).finally(done))}>
          {t('update.update')}
        </Button>
      </div>
    );
  }
  if (status.data?.checkError) {
    return <p className="marquee-plate mb-3 text-ink-dim">{t('update.checkFailed')}</p>;
  }
  return null;
}
