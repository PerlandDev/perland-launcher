import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AccountAvatar } from '../accounts/AccountAvatar';
import mark from '../../assets/mark.svg';
import { commands } from '../../ipc/commands';
import { useAccounts, useUpdate } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushError } from '../../state/toasts';
import { useUpdateBusy } from '../../state/updateBusy';
import { Close, Minus, SettingsCog, Square, WindowFrame } from '../../ui/Icon';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { Places } from './Places';

/** The custom title bar: drag anywhere that isn't a button. After the mark come the launcher's places as tabs. `minimal`
 *  keeps only the window controls, for screens (first run, settings failed) where nothing else can be reached. */
export function TitleBar({ minimal = false }: { minimal?: boolean }) {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const update = useUpdate();
  const busy = useUpdateBusy();
  const active = accounts.data?.accounts.find((a) => a.uuid === accounts.data?.active) ?? null;
  const [maximized, setMaximized] = useState(false);
  const restartOnce = useSingleFlight();
  const noteId = useId();
  const note = t(busy ? 'titlebar.updateReadyBusy' : 'titlebar.updateReady');

  useEffect(() => {
    const win = getCurrentWindow();
    let alive = true;
    let unlisten: (() => void) | undefined;
    const sync = () =>
      void win.isMaximized().then((value) => {
        if (alive) setMaximized(Boolean(value));
      });
    sync();
    void win.onResized(sync).then((stop) => {
      if (alive) unlisten = stop;
      else stop();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  const win = getCurrentWindow();
  return (
    // With the places in it the bar is 2.75rem tall, so a tab is a comfortable target; minimal, it keeps its 2.25rem.
    <header className={`flex min-w-0 items-center justify-between gap-3 bg-ground ${minimal ? 'h-9' : 'h-11'}`} data-tauri-drag-region>
      <div className="flex min-w-0 shrink-0 items-center gap-2 pl-3" data-tauri-drag-region>
        <img src={mark} alt="" width={21} height={18} data-tauri-drag-region />
        {/* The name gives its room to the places when the window is narrow (app.css). */}
        <span className={`text-sm font-bold ${minimal ? '' : 'tb-name'}`} data-tauri-drag-region>
          {t('app.name')}
        </span>
        {!minimal && <Places />}
      </div>
      <div className="flex min-w-0 items-center">
        {!minimal && (
          <>
            {update.data?.state === 'ready' && (
              // Quiet by design: dim text and a flat title-bar button, never lit, on one line. When the
              // bar runs short the sentence truncates and Restart now and the account keep their room.
              <span className="tb-note" data-tauri-drag-region>
                {/* While a game or an install runs the note itself says why Restart now is off, in visible text:
                    a disabled button can't take focus, so a hover title would reach no one. */}
                <span id={noteId} className="truncate" title={note} data-tauri-drag-region>
                  {note}
                </span>
                <button
                  type="button"
                  className="tb-btn tb-restart"
                  disabled={busy}
                  aria-describedby={busy ? noteId : undefined}
                  onClick={() => restartOnce((done) => void commands.updateRestart().catch(pushError).finally(done))}
                >
                  {t('titlebar.restart')}
                </button>
              </span>
            )}
            <button type="button" className="tb-btn flex shrink-0" onClick={() => navigate({ name: 'accounts' })}>
              {active ? (
                <>
                  <AccountAvatar account={active} size={20} />
                  <span className="font-semibold text-ink">{active.username}</span>
                </>
              ) : (
                t('titlebar.noAccount')
              )}
            </button>
            <button type="button" className="tb-btn" aria-label={t('titlebar.settings')} onClick={() => navigate({ name: 'settings' })}>
              <SettingsCog data-icon="settings-cog" aria-hidden />
            </button>
            <span className="mx-1 h-4 w-px bg-line" aria-hidden />
          </>
        )}
        <button type="button" className="tb-btn" aria-label={t('titlebar.minimize')} onClick={() => void win.minimize()}>
          <Minus aria-hidden />
        </button>
        <button
          type="button"
          className="tb-btn"
          aria-label={maximized ? t('titlebar.restore') : t('titlebar.maximize')}
          onClick={() => void win.toggleMaximize()}
        >
          {maximized ? <WindowFrame aria-hidden /> : <Square aria-hidden />}
        </button>
        <button type="button" className="tb-btn tb-close" aria-label={t('titlebar.close')} onClick={() => void win.close()}>
          <Close aria-hidden />
        </button>
      </div>
    </header>
  );
}
