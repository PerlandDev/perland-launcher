import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useId, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import mark from '../../assets/mark.svg';
import { detectLang } from '../../i18n';
import { commands } from '../../ipc/commands';
import { describeError, isLauncherError } from '../../ipc/errors';
import { onAppEvent } from '../../ipc/events';
import { qk, usePerlandStatus } from '../../state/queries';
import { Button } from '../../ui/Button';
import { WarningDiamond } from '../../ui/Icon';
import { perlandName, setPerlandAccount } from '../accounts/perlandSession';

/** Where the step is: its key (`idle`) or waiting on the browser. */
type Step = 'idle' | 'browser';

/** The Perland account step of Finish setting up: one key, Continue with Perland, which signs in on the Perland page (OAuth,
 *  where the player picks GitHub, Discord or an email code), then the wait on the browser with Cancel. It renders nothing
 *  while the Engine has player accounts closed, and nothing once someone is signed in: the account then has its row in
 *  Accounts. `onSignedIn` hears who signed in, since the step is gone by then. */
export function PerlandStep({ onSignedIn }: { onSignedIn?: (name: string) => void }) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const status = usePerlandStatus();
  const [step, setStep] = useState<Step>('idle');
  const [phase, setPhase] = useState<'waiting' | 'exchanging'>('waiting');
  const [error, setError] = useState<unknown>(null);
  /** The player canceled on the Perland page: a quiet note, not an error. */
  const [canceled, setCanceled] = useState(false);
  /** Bumped by every sign-in start and by Cancel, so a sign-in that was left behind can't land later. */
  const attempt = useRef(0);
  const firstControl = useRef<HTMLElement | null>(null);
  const cancelButton = useRef<HTMLButtonElement | null>(null);
  const titleId = useId();
  const language = detectLang(i18n.language);

  // The browser step reports its phase: waiting on the player, then the launcher finishing the sign-in.
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void onAppEvent((event) => {
      if (event.type === 'perland_sign_in') setPhase(event.phase);
    }).then((stop) => {
      if (alive) unlisten = stop;
      else stop();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  const data = status.data;
  const view = data?.enabled && !data.account ? step : null;
  const shownView = useRef(view);
  // Moving between the key and the wait puts the player on the control that took its place, so the keyboard never falls back
  // to the page. Opening the Profile focuses nothing: only a change the player made does.
  useEffect(() => {
    const before = shownView.current;
    shownView.current = view;
    if (before !== null && view !== null && before !== view) firstControl.current?.focus();
  }, [view]);

  // Exchanging drops "Open the page again"; if it had focus, Cancel takes it rather than the page.
  useEffect(() => {
    if (step !== 'browser' || phase !== 'exchanging') return;
    const active = document.activeElement;
    if (active === null || active === document.body) cancelButton.current?.focus();
  }, [step, phase]);

  if (view === null) return null;

  const startPerland = () => {
    const mine = ++attempt.current;
    setError(null);
    setCanceled(false);
    setPhase('waiting');
    setStep('browser');
    commands.perlandSignIn(language).then(
      (account) => {
        if (mine !== attempt.current) return;
        setStep('idle');
        if (!account) return; // canceled in the browser or here: back to the key, quietly
        setPerlandAccount(qc, account);
        onSignedIn?.(perlandName(account, t));
      },
      (failure: unknown) => {
        if (mine !== attempt.current) return;
        if (isLauncherError(failure) && failure.code === 'sign_in_canceled') setCanceled(true);
        else setError(failure);
        setStep('idle');
      },
    );
  };

  const cancelBrowser = () => {
    attempt.current++;
    setStep('idle');
    // A sign-in that finished just as the player pressed Cancel still counts: read the status again.
    void commands
      .perlandSignInCancel()
      .catch(() => {})
      .then(() => qc.invalidateQueries({ queryKey: qk.perland }));
  };

  const failure = error === null ? null : describeError(i18n, error);
  // After a canceled, failed or timed-out try the same key says Try again.
  const retry = canceled || error !== null;
  return (
    <li className="setup-step" data-step="perland" aria-labelledby={titleId}>
      <span className="setup-todo" aria-hidden />
      <div className="setup-what">
        <h3 id={titleId} className="font-bold">
          {t('perland.title')}
          <span className="sr-only">: {t('profile.setup.todo')}</span>
        </h3>
        <p className="text-sm text-ink-dim">
          {t('profile.setup.perlandWhy')} {t('perland.pagePitch')}
        </p>
      </div>
      {step === 'idle' ? (
        <div className="setup-act">
          <Button ref={(el) => void (firstControl.current = el)} icon={<img src={mark} alt="" width={21} height={18} />} onClick={startPerland}>
            {retry ? t('perland.tryAgain') : t('perland.withPerland')}
          </Button>
        </div>
      ) : (
        <div className="setup-more grid gap-3">
          <div className="grid gap-1">
            <p aria-live="polite" className="font-semibold">
              {phase === 'exchanging' ? t('perland.exchanging') : t('perland.waiting')}
            </p>
            {phase === 'waiting' && <p className="text-sm text-ink-dim">{t('perland.waitingHint')}</p>}
          </div>
          <div className="flex flex-wrap gap-3">
            {phase === 'waiting' && (
              <Button ref={(el) => void (firstControl.current = el)} onClick={() => void commands.perlandSignInReopen().catch(() => {})}>
                {t('perland.reopen')}
              </Button>
            )}
            <Button ref={cancelButton} variant="ghost" onClick={cancelBrowser}>
              {t('common.cancel')}
            </Button>
          </div>
        </div>
      )}
      {step === 'idle' && canceled && (
        <div role="status" className="setup-more grid gap-1 text-sm">
          <p className="font-semibold">{t('errors.sign_in_canceled.message')}</p>
          <p className="text-ink-dim">{t('perland.canceledHint')}</p>
        </div>
      )}
      {step === 'idle' && failure && (
        <div role="alert" className="setup-more grid gap-1 text-sm">
          <p className="flex items-start gap-2 font-semibold text-coral">
            <WarningDiamond className="mt-0.5 h-4 w-4 flex-none" aria-hidden />
            {failure.message}
          </p>
          {failure.fix && <p className="pl-6 text-ink-dim">{failure.fix}</p>}
        </div>
      )}
    </li>
  );
}
