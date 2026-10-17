import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { graphemes, initials } from '../../art/monogram';
import { detectLang } from '../../i18n';
import { commands } from '../../ipc/commands';
import { describeError, isLauncherError } from '../../ipc/errors';
import { onAppEvent } from '../../ipc/events';
import type { PerlandAccount, PerlandStatus } from '../../ipc/schema';
import { qk, usePerlandStatus } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Field } from '../../ui/Field';
import { WarningDiamond } from '../../ui/Icon';
import { MinecraftLink } from './MinecraftLink';

/** Where the card is: signed out (`idle`), waiting on the browser, or the two email steps. */
type Step = 'idle' | 'discord' | 'email' | 'code';
const METHODS = ['discord', 'email'] as const;

/** The Perland account on Accounts: sign in with Discord or an email code, or who is signed in and Sign out.
 *  It renders nothing while the Engine has player accounts closed, so a player who never signs in sees no change. */
export function PerlandAccountCard() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const status = usePerlandStatus();
  const [step, setStep] = useState<Step>('idle');
  const [phase, setPhase] = useState<'waiting' | 'exchanging'>('waiting');
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [email, setEmail] = useState('');
  const [code, setCode] = useState('');
  const [resendAt, setResendAt] = useState(0);
  const [now, setNow] = useState(() => Date.now());
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

  // A one-second clock only while Send a new code is still waiting.
  useEffect(() => {
    if (step !== 'code' || resendAt <= Date.now()) return;
    const timer = setInterval(() => {
      const at = Date.now();
      setNow(at);
      if (at >= resendAt) clearInterval(timer);
    }, 1000);
    return () => clearInterval(timer);
  }, [step, resendAt]);

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

  // An error from the email or code step puts the player back on the field it is about, so they can fix it.
  useEffect(() => {
    if (error !== null && (step === 'email' || step === 'code')) firstControl.current?.focus();
  }, [error, step]);

  // Exchanging drops "Open the page again"; if it had focus, Cancel takes it rather than the page.
  useEffect(() => {
    if (step !== 'discord' || phase !== 'exchanging') return;
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
    setCode('');
    setAnnounce(account ? t('perland.signedIn', { name: shownName(account) }) : t('perland.signedOut'));
  };

  const startDiscord = () => {
    const mine = ++attempt.current;
    setError(null);
    setPhase('waiting');
    setStep('discord');
    commands.perlandSignInDiscord(language).then(
      (account) => {
        if (mine !== attempt.current) return;
        if (account) switched(account);
        else setStep('idle'); // canceled in the browser or here: back to signed out, quietly
      },
      (failure: unknown) => {
        if (mine !== attempt.current) return;
        setError(failure);
        setStep('idle');
      },
    );
  };

  const cancelDiscord = () => {
    attempt.current++;
    setStep('idle');
    // A sign-in that finished just as the player pressed Cancel still counts: read the status again.
    void commands
      .perlandSignInCancel()
      .catch(() => {})
      .then(() => qc.invalidateQueries({ queryKey: qk.perland }));
  };

  const sendCode = async () => {
    setBusy(true);
    setError(null);
    try {
      const sent = await commands.perlandEmailStart(email.trim(), language);
      const at = Date.now();
      setNow(at);
      setResendAt(at + sent.resendIn * 1000);
      setEmail(email.trim());
      setCode('');
      setStep('code');
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  };

  const submitEmail = (event: FormEvent) => {
    event.preventDefault();
    if (!busy) void sendCode();
  };

  const submitCode = async (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      switched(await commands.perlandEmailVerify(email, code.trim()));
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
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

  const go = (next: Step) => {
    setError(null);
    setStep(next);
  };

  /** A wrong code says how many tries are left; with none left, the code is spent. */
  const errorText = (failure: unknown): string => {
    if (isLauncherError(failure) && failure.code === 'invalid_code') {
      const left = Number(failure.params.attemptsLeft);
      if (left === 0) return t('perland.codeUsedUp');
      if (Number.isFinite(left)) return t('perland.triesLeft', { message: t('errors.invalid_code.message'), count: left });
    }
    return describeError(i18n, failure).message;
  };
  const errorMessage = error === null ? null : errorText(error);
  const errorLine = errorMessage && (
    <p role="alert" className="flex items-start gap-2 text-sm text-coral">
      <WarningDiamond className="mt-0.5 h-4 w-4 flex-none" aria-hidden />
      {errorMessage}
    </p>
  );
  const pending = busy || undefined;
  const methods = METHODS.filter((m) => data.methods.includes(m));
  const account = data.account;
  const waitLeft = Math.max(0, Math.ceil((resendAt - now) / 1000));

  let body;
  if (account) {
    const name = shownName(account);
    const second = account.email && account.email !== name ? account.email : account.discord ? t('perland.viaDiscord') : null;
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
  } else if (step === 'discord') {
    body = (
      <div className="panel grid gap-4 p-5">
        <p aria-live="polite" className="font-semibold">
          {phase === 'exchanging' ? t('perland.exchanging') : t('perland.waiting')}
        </p>
        <div className="flex flex-wrap gap-3">
          {phase === 'waiting' && (
            <Button ref={(el) => void (firstControl.current = el)} onClick={() => void commands.perlandSignInReopen().catch(() => {})}>
              {t('perland.reopen')}
            </Button>
          )}
          <Button ref={cancelButton} variant="ghost" onClick={cancelDiscord}>
            {t('common.cancel')}
          </Button>
        </div>
      </div>
    );
  } else if (step === 'email') {
    body = (
      <form onSubmit={submitEmail} className="panel grid gap-4 p-5" noValidate>
        <Field label={t('perland.emailLabel')} error={errorMessage} alert>
          <input
            ref={(el) => void (firstControl.current = el)}
            className="field-input"
            type="email"
            value={email}
            autoComplete="email"
            spellCheck={false}
            onChange={(e) => setEmail(e.target.value)}
          />
        </Field>
        <div className="flex flex-wrap gap-3">
          <Button type="submit" aria-disabled={pending}>
            {t('perland.sendCode')}
          </Button>
          <Button variant="ghost" onClick={() => go('idle')}>
            {t('common.cancel')}
          </Button>
        </div>
      </form>
    );
  } else if (step === 'code') {
    body = (
      <form onSubmit={(e) => void submitCode(e)} className="panel grid gap-4 p-5">
        <p className="text-ink-dim">{t('perland.codeSent', { email })}</p>
        <Field label={t('perland.codeLabel')} error={errorMessage} alert>
          <input
            ref={(el) => void (firstControl.current = el)}
            className="field-input tabular max-w-48 tracking-widest"
            value={code}
            inputMode="numeric"
            autoComplete="one-time-code"
            maxLength={12}
            spellCheck={false}
            onChange={(e) => setCode(e.target.value)}
          />
        </Field>
        <div className="flex flex-wrap gap-3">
          <Button type="submit" aria-disabled={pending}>
            {t('perland.verify')}
          </Button>
          <Button variant="ghost" disabled={waitLeft > 0} aria-disabled={pending} onClick={() => !busy && void sendCode()}>
            {waitLeft > 0 ? t('perland.resendIn', { seconds: waitLeft }) : t('perland.resend')}
          </Button>
          <Button variant="ghost" onClick={() => go('email')}>
            {t('perland.otherEmail')}
          </Button>
        </div>
      </form>
    );
  } else {
    body = (
      <div className="panel grid gap-4 p-5">
        <p className="text-ink-dim">{t('perland.pitch')}</p>
        <div className="flex flex-wrap gap-3">
          {methods.map((method, i) => (
            <Button
              key={method}
              ref={i === 0 ? (el) => void (firstControl.current = el) : undefined}
              onClick={method === 'discord' ? startDiscord : () => go('email')}
            >
              {method === 'discord' ? t('perland.discord') : t('perland.email')}
            </Button>
          ))}
        </div>
        {errorLine}
      </div>
    );
  }

  return (
    <section className="grid gap-3" aria-labelledby="perland-heading">
      <h2 id="perland-heading" className="text-lg font-bold">
        {t('perland.title')}
      </h2>
      <p aria-live="polite" className="sr-only">
        {announce}
      </p>
      {body}
    </section>
  );
}
