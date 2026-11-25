import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AccountAvatar } from '../accounts/AccountAvatar';
import mark from '../../assets/mark.svg';
import { usePersonalizeSeen } from '../../state/personalizeSeen';
import { useAccounts, useSettings } from '../../state/queries';
import { navigate, useRoute } from '../../state/route';
import { Brush, Close, Minus, SettingsCog, Square, WindowFrame } from '../../ui/Icon';
import { Places } from './Places';

/** The custom title bar: drag anywhere that isn't a button. After the mark come the launcher's places as tabs; the account
 *  at the right opens the player's Profile, and sits pressed while it is open. `minimal` keeps only the window controls, for
 *  screens (first run, settings failed) where nothing else can be reached. */
export function TitleBar({ minimal = false }: { minimal?: boolean }) {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const route = useRoute().name;
  const onProfile = route === 'profile';
  const onPersonalize = route === 'personalize';
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
            <button
              type="button"
              className="tb-btn tb-account flex shrink-0"
              aria-label={active ? t('titlebar.profile', { name: active.username }) : undefined}
              aria-current={onProfile ? 'page' : undefined}
              onClick={() => navigate({ name: 'profile' })}
            >
              {active ? (
                <>
                  <AccountAvatar account={active} size={20} />
                  <span className="font-semibold text-ink">{active.username}</span>
                </>
              ) : (
                t('titlebar.noAccount')
              )}
            </button>
            <BrushKey current={onPersonalize} />
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


/** The brush, which opens Personalize. Until the player has opened it once it carries the blue "new" lamp; not during first
 *  run, and not for a player who has already changed the look from the default (they found it, perhaps when it was still
 *  in Settings). Its own component, so the minimal bar (shown while the settings fail) doesn't ask for the settings. */
function BrushKey({ current }: { current: boolean }) {
  const { t } = useTranslation();
  const settings = useSettings().data;
  const seen = usePersonalizeSeen();
  const newId = useId();
  const fresh = !seen && settings !== undefined && settings.firstRunDone && settings.theme === 'laminate' && settings.accent === 'room';
  return (
    <button
      type="button"
      className="tb-btn"
      aria-label={t('titlebar.personalize')}
      aria-current={current ? 'page' : undefined}
      aria-describedby={fresh ? newId : undefined}
      onClick={() => navigate({ name: 'personalize' })}
    >
      <Brush aria-hidden />
      {fresh && (
        <span className="tb-new" aria-hidden>
          <span id={newId} className="sr-only">
            {t('titlebar.personalizeNew')}
          </span>
        </span>
      )}
    </button>
  );
}
