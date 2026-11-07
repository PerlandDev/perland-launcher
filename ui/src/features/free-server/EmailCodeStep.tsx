import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { detectLang } from '../../i18n';
import { commands } from '../../ipc/commands';
import { describeError, isLauncherError } from '../../ipc/errors';
import type { PerlandStatus } from '../../ipc/schema';
import { qk } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Field } from '../../ui/Field';

/** Put a verified email on the signed-in Perland account, which a free server needs: the email, Send code, then the code
 *  with Verify, Send a new code (after a countdown) and Use another email. Errors go under the field they are about. On a
 *  verified email the Perland status is read again (so the account shows it) and `onVerified` gets the email. Mount it only
 *  while shown: it always starts at the email field. */
export function EmailCodeStep({ onVerified, onCancel }: { onVerified: (email: string) => void; onCancel: () => void }) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const [step, setStep] = useState<'email' | 'code'>('email');
  const [email, setEmail] = useState('');
  const [code, setCode] = useState('');
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [resendAt, setResendAt] = useState(0);
  const [now, setNow] = useState(() => Date.now());
  const field = useRef<HTMLInputElement | null>(null);
  const language = detectLang(i18n.language);

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

  // Each step starts on its field, so the keyboard never falls back to the page.
  useEffect(() => field.current?.focus(), [step]);

  // An error puts the player back on the field it is about, so they can fix it.
  useEffect(() => {
    if (error !== null) field.current?.focus();
  }, [error]);

  const sendCode = async () => {
    setBusy(true);
    setError(null);
    try {
      const sent = await commands.perlandAccountEmailStart(email.trim(), language);
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
      const account = await commands.perlandAccountEmailVerify(email, code.trim());
      qc.setQueryData<PerlandStatus>(qk.perland, (old) => (old ? { ...old, account } : old));
      void qc.invalidateQueries({ queryKey: qk.perland });
      onVerified(email);
    } catch (failure) {
      setError(failure);
    } finally {
      setBusy(false);
    }
  };

  const otherEmail = () => {
    setError(null);
    setStep('email');
  };

  /** A wrong code says how many tries are left; with none left, the code is spent. */
  const errorText = (failure: unknown): string => {
    if (isLauncherError(failure) && failure.code === 'invalid_code') {
      const left = Number(failure.params.attemptsLeft);
      if (left === 0) return t('freeServer.email.codeUsedUp');
      if (Number.isFinite(left)) return t('freeServer.email.triesLeft', { message: t('errors.invalid_code.message'), count: left });
    }
    return describeError(i18n, failure).message;
  };
  const errorMessage = error === null ? null : errorText(error);
  const errorFix = error === null ? null : describeError(i18n, error).fix;
  const pending = busy || undefined;
  const waitLeft = Math.max(0, Math.ceil((resendAt - now) / 1000));
  const cancel = (
    <Button variant="ghost" onClick={onCancel}>
      {t('common.cancel')}
    </Button>
  );

  if (step === 'email') {
    return (
      <form onSubmit={submitEmail} className="grid gap-4" noValidate>
        <p>{t('freeServer.email.lead')}</p>
        <Field label={t('freeServer.email.emailLabel')} error={errorMessage} fix={errorFix} alert>
          <input
            ref={field}
            className="field-input text-ink"
            type="email"
            value={email}
            autoComplete="email"
            spellCheck={false}
            onChange={(e) => setEmail(e.target.value)}
          />
        </Field>
        <div className="flex flex-wrap gap-3">
          <Button type="submit" aria-disabled={pending}>
            {t('freeServer.email.sendCode')}
          </Button>
          {cancel}
        </div>
      </form>
    );
  }
  return (
    <form onSubmit={(e) => void submitCode(e)} className="grid gap-4">
      <p>{t('freeServer.email.codeSent', { email })}</p>
      <Field label={t('freeServer.email.codeLabel')} error={errorMessage} fix={errorFix} alert>
        <input
          ref={field}
          className="field-input tabular max-w-48 tracking-widest text-ink"
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
          {t('freeServer.email.verify')}
        </Button>
        <Button variant="ghost" disabled={waitLeft > 0} aria-disabled={pending} onClick={() => !busy && void sendCode()}>
          {waitLeft > 0 ? t('freeServer.email.resendIn', { seconds: waitLeft }) : t('freeServer.email.resend')}
        </Button>
        <Button variant="ghost" onClick={otherEmail}>
          {t('freeServer.email.otherEmail')}
        </Button>
        {cancel}
      </div>
    </form>
  );
}
