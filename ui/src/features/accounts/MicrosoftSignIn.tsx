import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { detectLang } from '../../i18n';
import { commands } from '../../ipc/commands';
import { describeError, isLauncherError } from '../../ipc/errors';
import { onAppEvent } from '../../ipc/events';
import type { AccountsFile, MsaStep } from '../../ipc/schema';
import { qk } from '../../state/queries';
import { Button } from '../../ui/Button';
import { Clock, WarningDiamond } from '../../ui/Icon';
import { SignInSteps } from './SignInSteps';

export interface MicrosoftSignInProps {
  /** Called once the account is signed in and in use. */
  onSignedIn?: () => void;
  /** `primary`: the lit large key of a step that is about signing in (First run, the sign-in prompt). `inline`: Accounts' add
   *  key, reading "Add Microsoft account". */
  variant: 'primary' | 'inline';
  /** Off where the place around it already says why (the sign-in prompt, the offline-accounts notice). A refusal is still said. */
  showWhy?: boolean;
}

/** Microsoft sign-in: the why line and the key, then the browser step and each call of the chain (Microsoft, Xbox Live, XSTS,
 *  Minecraft, the profile) as it runs, with Cancel. Every press runs the whole chain; nothing is skipped because an earlier one was
 *  refused. After a refusal the steps stay, showing what passed and where it stopped. A failure is said inline under the key in
 *  the player's language; Minecraft not having approved the app yet is said as info, not as a fault: there is nothing for the
 *  player to fix. */
export function MicrosoftSignIn({ onSignedIn, variant, showWhy = true }: MicrosoftSignInProps) {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [phase, setPhase] = useState<'waiting' | 'exchanging'>('waiting');
  /** The call of the chain this sign-in reached; `null` before the browser opened (or after a cancel). */
  const [reached, setReached] = useState<MsaStep | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [announce, setAnnounce] = useState('');
  /** Bumped by every start and by Cancel, so a sign-in left behind can't land later. */
  const attempt = useRef(0);
  const signInKey = useRef<HTMLButtonElement | null>(null);
  const cancelKey = useRef<HTMLButtonElement | null>(null);
  const shownBusy = useRef(busy);

  // The browser step reports its phase: waiting on the player, then the launcher finishing the sign-in.
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    void onAppEvent((event) => {
      if (event.type === 'microsoft_sign_in') {
        setPhase(event.phase);
        // The browser step is the Microsoft account step.
        if (event.phase === 'waiting') setReached('microsoft');
      }
      if (event.type === 'microsoft_sign_in_step') setReached(event.step);
    }).then((stop) => {
      if (alive) unlisten = stop;
      else stop();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  // Moving between the key and the browser step keeps the keyboard on the control that took its place.
  useEffect(() => {
    if (shownBusy.current === busy) return;
    shownBusy.current = busy;
    (busy ? cancelKey : signInKey).current?.focus();
  }, [busy]);

  const refresh = () =>
    Promise.all([qc.invalidateQueries({ queryKey: qk.accountReady }), qc.invalidateQueries({ queryKey: qk.accounts })]);

  const start = () => {
    if (busy) return;
    const mine = ++attempt.current;
    setError(null);
    setAnnounce('');
    setPhase('waiting');
    setReached(null);
    setBusy(true);
    commands.accountSignInMicrosoft(detectLang(i18n.language)).then(
      (file: AccountsFile | null) => {
        if (mine !== attempt.current) return;
        setBusy(false);
        setReached(null);
        if (!file) return; // canceled here or in the browser: back to the key, quietly
        qc.setQueryData(qk.accounts, file);
        void qc.invalidateQueries({ queryKey: qk.accountReady });
        const name = file.accounts.find((a) => a.uuid === file.active)?.username;
        if (name) setAnnounce(t('accounts.microsoftSignedIn', { name }));
        onSignedIn?.();
      },
      (failure: unknown) => {
        if (mine !== attempt.current) return;
        setBusy(false);
        setError(failure);
        void refresh();
      },
    );
  };

  const cancel = () => {
    attempt.current++;
    setBusy(false);
    setReached(null);
    // A sign-in that finished just as the player pressed Cancel still counts: read the accounts again.
    void commands
      .accountSignInCancel()
      .catch(() => {})
      .then(refresh);
  };

  const awaiting = isLauncherError(error) && error.code === 'microsoft_awaiting_approval';
  const failure = error !== null && !awaiting ? describeError(i18n, error) : null;
  const primary = variant === 'primary';
  const label = primary ? t('accounts.microsoft') : t('accounts.microsoftAdd');

  const live = (
    <p aria-live="polite" className="sr-only">
      {announce}
    </p>
  );

  if (busy) {
    return (
      <div className={primary ? 'grid gap-4' : 'panel grid gap-4 p-5'}>
        <p aria-live="polite" className="font-semibold">
          {phase === 'exchanging' ? t('accounts.microsoftExchanging') : t('accounts.microsoftWaiting')}
        </p>
        {reached && <SignInSteps reached={reached} end="running" />}
        <Button ref={cancelKey} variant="ghost" className="justify-self-start" onClick={cancel}>
          {t('common.cancel')}
        </Button>
        {live}
      </div>
    );
  }

  return (
    <div className="grid gap-4">
      {error !== null && reached && <SignInSteps reached={reached} end={awaiting ? 'waiting' : 'fault'} />}
      {awaiting ? (
        <p className="flex items-start gap-2 text-blue-ink">
          <Clock className="mt-0.5 h-[1.125rem] w-[1.125rem] flex-none" aria-hidden />
          {t('errors.microsoft_awaiting_approval.message')}
        </p>
      ) : (
        showWhy && <p className="text-ink-dim">{t('accounts.microsoftWhy')}</p>
      )}
      <Button
        ref={signInKey}
        variant="primary"
        size={primary ? 'lg' : 'md'}
        className={primary ? undefined : 'justify-self-start'}
        onClick={start}
      >
        {label}
      </Button>
      {failure && (
        <div role="alert" className="flex items-start gap-2 text-sm">
          <WarningDiamond className="mt-0.5 h-4 w-4 flex-none text-coral" aria-hidden />
          <div className="grid gap-1">
            <p className="font-semibold text-coral">{failure.message}</p>
            {failure.fix && <p className="text-ink-dim">{failure.fix}</p>}
          </div>
        </div>
      )}
      {live}
    </div>
  );
}
