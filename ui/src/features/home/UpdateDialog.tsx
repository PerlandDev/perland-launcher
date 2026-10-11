import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance, PackUpdate } from '../../ipc/schema';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { ChangeSummary } from './ChangeSummary';

/** Play paused on an `ask` pack's update. Update and play is the one lit button; closing cancels the run. */
export function UpdateDialog({ instance, update }: { instance: Instance; update: PackUpdate }) {
  const { t } = useTranslation();
  const answerOnce = useSingleFlight();
  const answer = (take: boolean) =>
    answerOnce((done) => void commands.playAnswer(instance.id, take).catch(pushError).finally(done));
  const title = update.fromVersion
    ? t('update.title', { from: update.fromVersion, to: update.toVersion })
    : t('update.titleNew', { to: update.toVersion });
  return (
    <Dialog
      open
      onClose={() => void commands.stop(instance.id).catch(pushError)}
      title={title}
      actions={
        <>
          <Button onClick={() => answer(false)}>
            {update.fromVersion ? t('update.playOld', { version: update.fromVersion }) : t('update.playInstalled')}
          </Button>
          <Button variant="primary" onClick={() => answer(true)}>
            {t('update.updateAndPlay')}
          </Button>
        </>
      }
    >
      <ChangeSummary update={update} />
    </Dialog>
  );
}
