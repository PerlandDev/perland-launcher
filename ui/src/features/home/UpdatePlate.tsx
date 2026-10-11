import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance } from '../../ipc/schema';
import { usePackStatus } from '../../state/queries';
import { useRun } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { changeLine } from './labels';

/** The marquee plate above the name. After a sync it says what changed; between plays it
 *  offers the update (never lit: Play is the one lit button). */
export function UpdatePlate({ instance }: { instance: Instance }) {
  const { t } = useTranslation();
  const run = useRun(instance.id);
  const status = usePackStatus(instance.id, true);
  const updateOnce = useSingleFlight();
  if (run.state !== 'idle') return null;
  const synced = run.lastExit?.synced ?? null;
  if (synced) {
    const kept = synced.keptModified.length;
    return (
      <p className="marquee-plate update-plate mb-3" data-lamp="ok" role="status">
        {t('update.updatedTo', { version: synced.toVersion })}
        {kept > 0 && ` · ${t('update.kept', { count: kept })}`}
      </p>
    );
  }
  const update = status.data?.update ?? null;
  if (update) {
    const counts = { added: update.added.length, updated: update.updated.length, removed: update.removed.length };
    return (
      <div className="marquee-plate update-plate mb-3" data-lamp="info" role="status">
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
