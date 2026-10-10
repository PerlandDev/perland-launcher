import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { onCloseRequested } from '../../ipc/events';
import { pushError } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { useSingleFlight } from '../../ui/useSingleFlight';

/** Closing the window while a game runs asks first; minimizing is the safe default. */
export function CloseGuard() {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const quitOnce = useSingleFlight();

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void onCloseRequested(() => setOpen(true)).then((stop) => {
      if (alive) unlisten = stop;
      else stop();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  // A quit that succeeds ends the app, so it stays "in flight"; only a failure frees the button.
  const quit = () =>
    quitOnce(
      (done) =>
        void commands.quit(true).catch((error: unknown) => {
          pushError(error);
          done();
        }),
    );

  return (
    <Dialog
      open={open}
      onClose={() => setOpen(false)}
      title={t('closeGuard.title')}
      actions={
        <>
          <Button
            variant="primary"
            onClick={() => {
              setOpen(false);
              void getCurrentWindow().minimize().catch(pushError);
            }}
          >
            {t('closeGuard.minimize')}
          </Button>
          <Button variant="danger" onClick={quit}>
            {t('closeGuard.quit')}
          </Button>
        </>
      }
    >
      {t('closeGuard.body')}
    </Dialog>
  );
}
