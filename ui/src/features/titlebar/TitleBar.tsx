import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AccountAvatar } from '../accounts/AccountAvatar';
import mark from '../../assets/mark.svg';
import { commands } from '../../ipc/commands';
import { useAccounts, useUpdate } from '../../state/queries';
import { navigate, useRoute } from '../../state/route';
import { pushError } from '../../state/toasts';
import { useUpdateBusy } from '../../state/updateBusy';
import { Brush, Close, Download, Minus, SettingsCog, Square, WindowFrame } from '../../ui/Icon';
import { Button } from '../../ui/Button';
import { useSingleFlight } from '../../ui/useSingleFlight';
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
  const update = useUpdate();
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
            {update.data?.state === 'ready' && <UpdateKey />}
            <button
              type="button"
              className="tb-btn"
              aria-label={t('titlebar.personalize')}
              aria-current={onPersonalize ? 'page' : undefined}
              onClick={() => navigate({ name: 'personalize' })}
            >
              <Brush aria-hidden />
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

/** An update is ready: one glyph-sized key with the lime lamp, so a narrow bar keeps its room. It opens a card under it
 *  that says when the update installs and holds Restart now. Escape or a click outside closes it. */
function UpdateKey() {
  const { t } = useTranslation();
  const busy = useUpdateBusy();
  const restartOnce = useSingleFlight();
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const key = useRef<HTMLButtonElement>(null);
  const cardId = useId();
  const titleId = useId();
  const busyId = useId();

  useEffect(() => {
    if (!open) return;
    const outside = (event: PointerEvent) => {
      if (!wrap.current?.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      setOpen(false);
      key.current?.focus();
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('keydown', escape);
    return () => {
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('keydown', escape);
    };
  }, [open]);

  return (
    <div ref={wrap} className="relative flex shrink-0">
      <button
        ref={key}
        type="button"
        className="tb-btn tb-update"
        aria-label={t('titlebar.update')}
        title={t('titlebar.update')}
        aria-expanded={open}
        aria-controls={open ? cardId : undefined}
        onClick={() => setOpen((value) => !value)}
      >
        <Download aria-hidden />
        <span className="tb-lamp" aria-hidden />
      </button>
      {open && (
        <section id={cardId} className="tb-card" aria-labelledby={titleId}>
          <h2 id={titleId} className="font-semibold text-ink">
            {t('titlebar.update')}
          </h2>
          {/* While a game or an install runs the card says why Restart now is off, in visible text: a disabled button
              can't take focus, so a hover title would reach no one. */}
          <p id={busyId} className="text-sm text-ink-dim">
            {t(busy ? 'settings.updates.busy' : 'titlebar.updateWhen')}
          </p>
          <div className="flex justify-end">
            <Button
              variant="primary"
              disabled={busy}
              aria-describedby={busy ? busyId : undefined}
              onClick={() => restartOnce((done) => void commands.updateRestart().catch(pushError).finally(done))}
            >
              {t('titlebar.restart')}
            </Button>
          </div>
        </section>
      )}
    </div>
  );
}
