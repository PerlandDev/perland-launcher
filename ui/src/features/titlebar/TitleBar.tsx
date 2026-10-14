import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Avatar } from '../../art/Avatar';
import mark from '../../assets/mark.svg';
import { commands } from '../../ipc/commands';
import { useAccounts, useUpdate } from '../../state/queries';
import { navigate } from '../../state/route';
import { useActiveIds } from '../../state/runStore';
import { pushError } from '../../state/toasts';
import { Close, Gear, Minus, Square, WindowFrame } from '../../ui/Icon';

/** The custom title bar: drag anywhere that isn't a button. `minimal` keeps only the window
 *  controls, for screens (first run, settings failed) where account and settings can't be reached. */
export function TitleBar({ minimal = false }: { minimal?: boolean }) {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const update = useUpdate();
  const busy = useActiveIds().length > 0;
  const active = accounts.data?.accounts.find((a) => a.uuid === accounts.data?.active) ?? null;
  const [maximized, setMaximized] = useState(false);

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
    <header className="flex h-9 items-center justify-between bg-ground" data-tauri-drag-region>
      <div className="flex items-center gap-2 pl-3" data-tauri-drag-region>
        <img src={mark} alt="" width={21} height={18} data-tauri-drag-region />
        <span className="text-sm font-bold" data-tauri-drag-region>
          {t('app.name')}
        </span>
      </div>
      <div className="flex items-center">
        {!minimal && (
          <>
            {update.data?.state === 'ready' && (
              <span className="flex items-center gap-2 pr-2 text-sm text-ink-dim">
                {t('titlebar.updateReady')}
                <button
                  type="button"
                  className="tb-btn font-semibold text-ink"
                  disabled={busy}
                  title={busy ? t('settings.updates.busy') : undefined}
                  onClick={() => void commands.updateRestart().catch(pushError)}
                >
                  {t('titlebar.restart')}
                </button>
              </span>
            )}
            <button type="button" className="tb-btn flex" onClick={() => navigate({ name: 'accounts' })}>
              {active ? (
                <>
                  <Avatar uuid={active.uuid} size={20} />
                  <span className="font-semibold text-ink">{active.username}</span>
                </>
              ) : (
                t('titlebar.noAccount')
              )}
            </button>
            <button type="button" className="tb-btn" aria-label={t('titlebar.settings')} onClick={() => navigate({ name: 'settings' })}>
              <Gear aria-hidden />
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
