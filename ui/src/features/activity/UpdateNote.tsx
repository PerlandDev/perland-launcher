import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { useUpdate } from '../../state/queries';
import { pushError } from '../../state/toasts';
import { useUpdateBusy } from '../../state/updateBusy';
import { useSingleFlight } from '../../ui/useSingleFlight';

/** A launcher update is ready: a quiet line at the right of the Activity bar, out of the title bar's way. The lime lamp
 *  marks it, the words say when it installs, and Restart now is a flat key, never lit. One line; the words truncate
 *  first, so Restart now keeps its room. */
export function UpdateNote() {
  const { t } = useTranslation();
  const update = useUpdate();
  const busy = useUpdateBusy();
  const restartOnce = useSingleFlight();
  const noteId = useId();
  if (update.data?.state !== 'ready') return null;
  const note = t(busy ? 'activity.updateReadyBusy' : 'activity.updateReady');
  return (
    <div className="update-note">
      <span className="h-2 w-2 flex-none bg-lime" aria-hidden />
      {/* While a game or an install runs the note itself says why Restart now is off, in visible text: a disabled
          button can't take focus, so a hover title would reach no one. */}
      <span id={noteId} className="truncate" title={note}>
        {note}
      </span>
      <button
        type="button"
        className="update-restart"
        disabled={busy}
        aria-describedby={busy ? noteId : undefined}
        onClick={() => restartOnce((done) => void commands.updateRestart().catch(pushError).finally(done))}
      >
        {t('activity.restart')}
      </button>
    </div>
  );
}
