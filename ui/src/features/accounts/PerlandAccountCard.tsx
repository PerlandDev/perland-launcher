import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { graphemes, initials } from '../../art/monogram';
import { detectLang } from '../../i18n';
import { commands } from '../../ipc/commands';
import { describeError, isLauncherError } from '../../ipc/errors';
import { onAppEvent } from '../../ipc/events';
import type { PerlandAccount, PerlandStatus } from '../../ipc/schema';
import { qk, usePerlandStatus } from '../../state/queries';
import { Button } from '../../ui/Button';
import mark from '../../assets/mark.svg';
import { WarningDiamond } from '../../ui/Icon';
import { MinecraftLink } from './MinecraftLink';

/** Where the card is: signed out (`idle`) or waiting on the browser. */
type Step = 'idle' | 'browser';

/** The Perland account on Accounts: sign in on the Perland page (OAuth, where the player picks GitHub, Discord or an email code),
 *  or who is signed in and Sign out. It renders nothing while the Engine has player accounts closed (or no Perland page),
 *  so a player who never signs in sees no change. */
export function PerlandAccountCard() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const status = usePerlandStatus();
  const [step, setStep] = useState<Step>('idle');
  const [phase, setPhase] = useState<'waiting' | 'exchanging'>('waiting');
  const [error, setError] = useState<unknown>(null);
  /** The player canceled on the Perland page: a quiet note, not an error. */
  const [canceled, setCanceled] = useState(false);
  const [busy, setBusy] = useState(false);
  const [announce, setAnnounce] = useState('');
  /** Bumped by every sign-in start and by Cancel, so a sign-in that was left behind can't land later. */
  const attempt = useRef(0);
  const firstControl = useRef<HTMLElement | null>(null);
  const cancelButton = useRef<HTMLButtonElement | null>(null);
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
  const view = data?.enabled ? (data.account ? 'signedIn' : step) : null;
  const shownView = useRef(view);
  // Moving to another step puts the player on its first control, so the keyboard never falls back to the page.
  // Opening Accounts focuses nothing: only a change the player made does.
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

  if (!data?.enabled) return null;

  const shownName = (account: PerlandAccount) => account.name || account.email || t('perland.player');

  /** Someone new is (or no one is) signed in: the old player's servers must never show for the next one. */
  const switched = (account: PerlandAccount | null) => {
    qc.setQueryData<PerlandStatus>(qk.perland, (old) => (old ? { ...old, account } : old));
    void qc.invalidateQueries({ queryKey: qk.perland });
    qc.removeQueries({ queryKey: qk.yourServers });
    qc.removeQueries({ queryKey: qk.perlandStats });
    setStep('idle');
    setError(null);
    setCanceled(false);
    setAnnounce(account ? t('perland.signedIn', { name: shownName(account) }) : t('perland.signedOut'));
  };

  const startPerland = () => {
    const mine = ++attempt.current;
    setError(null);
    setCanceled(false);
    setPhase('waiting');
    setStep('browser');
    commands.perlandSignIn(language).then(
      (account) => {
        if (mine !== attempt.current) return;
        if (account) switched(account);
        else setStep('idle'); // canceled in the browser or here: back to signed out, quietly
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

  const signOut = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await commands.perlandSignOut();
      switched(null);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  };

  const errorMessage = error === null ? null : describeError(i18n, error).message;
  const errorFix = error === null ? null : describeError(i18n, error).fix;
  const errorLine = errorMessage && (
    <div role="alert" className="grid gap-1 text-sm">
      <p className="flex items-start gap-2 font-semibold text-coral">
        <WarningDiamond className="mt-0.5 h-4 w-4 flex-none" aria-hidden />
        {errorMessage}
      </p>
      {errorFix && <p className="pl-6 text-ink-dim">{errorFix}</p>}
    </div>
  );
  const pending = busy || undefined;
  const account = data.account;

  let body;
  if (account) {
    const name = shownName(account);
    // The email when it isn't already the name, else how the account signs in. The Engine names the account by Discord first,
    // then GitHub, so the method follows the same order.
    const method = account.discord ? t('perland.viaDiscord') : account.github ? t('perland.viaGithub') : null;
    const second = account.email && account.email !== name ? account.email : method;
    body = (
      <div className="grid gap-3">
        <div className="panel flex items-center gap-4 px-4 py-3">
          <span
            aria-hidden
            className="stepped flex h-10 w-10 flex-none items-center justify-center bg-raised font-display text-lg text-ink"
          >
            {graphemes(initials(name))[0] ?? 'P'}
          </span>
          <div className="min-w-0 flex-1">
            <p className="truncate font-bold" title={name}>
              {name}
            </p>
            {second && <p className="truncate text-sm text-ink-dim">{second}</p>}
          </div>
          <Button ref={(el) => void (firstControl.current = el)} aria-disabled={pending} onClick={() => void signOut()}>
            {t('perland.signOut')}
          </Button>
        </div>
        {errorLine}
        <MinecraftLink />
      </div>
    );
  } else if (step === 'browser') {
    body = (
      <div className="panel grid gap-4 p-5">
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
    );
  } else {
    // One way in: the Perland page in the browser. After a canceled, failed or timed-out try the same key says Try again.
    const retry = canceled || error !== null;
    body = (
      <div className="panel grid gap-4 p-5">
        <p className="text-ink-dim">{t('perland.pitch')}</p>
        <div className="grid justify-items-start gap-2">
          <Button
            ref={(el) => void (firstControl.current = el)}
            size="lg"
            icon={<img src={mark} alt="" width={28} height={24} />}
            onClick={startPerland}
          >
            {retry ? t('perland.tryAgain') : t('perland.withPerland')}
          </Button>
          {!retry && <p className="text-sm text-ink-dim">{t('perland.pagePitch')}</p>}
        </div>
        {canceled && (
          <div role="status" className="grid gap-1 text-sm">
            <p className="font-semibold">{t('errors.sign_in_canceled.message')}</p>
            <p className="text-ink-dim">{t('perland.canceledHint')}</p>
          </div>
        )}
        {errorLine}
      </div>
    );
  }

  return (
    <section className="grid gap-3" aria-labelledby="perland-heading">
      <h2 id="perland-heading" className="flex items-center gap-2 text-lg font-bold">
        <img src={mark} alt="" width={21} height={18} className="flex-none" />
        {t('perland.title')}
      </h2>
      <p aria-live="polite" className="sr-only">
        {announce}
      </p>
      {body}
    </section>
  );
}
