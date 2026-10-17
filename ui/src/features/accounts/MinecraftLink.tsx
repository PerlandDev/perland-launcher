import { useQueryClient, type QueryClient } from '@tanstack/react-query';
import { useEffect, useId, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { describeError, isLauncherError } from '../../ipc/errors';
import type { PerlandStatus } from '../../ipc/schema';
import { qk, usePerlandStats, usePerlandStatus } from '../../state/queries';
import { Button } from '../../ui/Button';
import { copyText } from '../../ui/copyText';
import { formatCountdown } from '../../ui/format';
import { Check, Copy, Link, WarningDiamond } from '../../ui/Icon';

/** Not linked: the pitch (`idle`), a code waiting to be typed in game, or a code that ran out. */
type Step = 'idle' | 'code' | 'expired';
/** A link code, and when it arrived here: it runs out `expiresIn` seconds later by this computer's clock. */
interface Code {
  code: string;
  arrivedAt: number;
  expiresIn: number;
}

const POLL_MS = 3000;
/** Stands in for an element inside a translated sentence, so the sentence keeps its word order in each language. */
const SLOT = '\u0000';

/** `sentence` (translated with SLOT for its one value) with `piece` where the value goes. */
function fill(sentence: string, piece: ReactNode) {
  const [before, after = ''] = sentence.split(SLOT);
  return (
    <>
      {before}
      {piece}
      {after}
    </>
  );
}

/** The status says who is linked: set it now, so the row changes before the status reads again. The new name is a new stats key,
 *  which does the read; the stats already cached are only marked stale, so none is read again under the old name and filled with
 *  the new answer. */
function setLinkedName(qc: QueryClient, name: string | null) {
  qc.setQueryData<PerlandStatus>(qk.perland, (old) => (old?.account ? { ...old, account: { ...old.account, minecraft: name } } : old));
  void qc.invalidateQueries({ queryKey: qk.perland });
  void qc.invalidateQueries({ queryKey: qk.perlandStats, refetchType: 'none' });
}

/** Whether `failure` says the session is gone (signed out elsewhere, or a 401). */
const isSignedOut = (failure: unknown) => isLauncherError(failure) && failure.code === 'perland_signed_out';

/** The Minecraft account row inside the signed-in Perland card: link it with a code typed on a Perland server, or
 *  who is linked and Unlink. It renders nothing unless signed in with the Engine's stats add-on on. */
export function MinecraftLink() {
  const { t, i18n } = useTranslation();
  const qc = useQueryClient();
  const status = usePerlandStatus();
  const stats = usePerlandStats();
  const [step, setStep] = useState<Step>('idle');
  const [code, setCode] = useState<Code | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [announce, setAnnounce] = useState('');
  const headingId = useId();
  const firstControl = useRef<HTMLButtonElement | null>(null);
  const copiedTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const mounted = useRef(true);

  const account = status.data?.enabled ? status.data.account : null;
  const linkedName = account?.minecraft ?? null;
  // A link or unlink puts the new name in the stats' key, so the stats read again; the row stays while they do.
  const [available, setAvailable] = useState(false);
  const knownAvailable = !account ? false : stats.data === undefined ? available : stats.data !== null;
  if (knownAvailable !== available) setAvailable(knownAvailable);
  // Linked, however the news arrived (the poll, or the status reading it first): the code is spent, so a later Unlink
  // starts again from the pitch. The poll announces its own link; this one is the status's.
  if (linkedName !== null && (step !== 'idle' || code !== null)) {
    setStep('idle');
    setCode(null);
    setAnnounce(t('perland.minecraft.linkedAnnounce', { name: linkedName }));
  }
  const view = !available || !account ? null : linkedName !== null ? 'linked' : step;

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      clearTimeout(copiedTimer.current);
    };
  }, []);

  // While the code shows (never while the row is hidden), ask every 3 s whether it was typed in game. An answer that lands after it stopped counts for nothing.
  useEffect(() => {
    if (view !== 'code') return;
    let alive = true;
    let asking = false;
    const stop = () => {
      alive = false;
      clearInterval(timer);
    };
    const timer = setInterval(() => {
      if (asking) return;
      asking = true;
      commands.perlandLinkPoll().then(
        (name) => {
          asking = false;
          if (!alive || name === null) return;
          stop();
          setLinkedName(qc, name);
          setStep('idle');
          setCode(null);
          setError(null);
          setAnnounce(t('perland.minecraft.linkedAnnounce', { name }));
        },
        (failure: unknown) => {
          asking = false;
          if (!alive) return;
          // Signed out elsewhere: the code is gone with the session, and the card reads the status again.
          // Anything else (offline, a slow Engine) waits for the next tick.
          if (isSignedOut(failure)) {
            stop();
            setStep('idle');
            setCode(null);
            void qc.invalidateQueries({ queryKey: qk.perland });
          }
        },
      );
    }, POLL_MS);
    return stop;
  }, [view, qc, t]);

  // The countdown: one tick a second while the code shows, until it runs out.
  useEffect(() => {
    if (view !== 'code' || code === null) return;
    const endsAt = code.arrivedAt + code.expiresIn * 1000;
    const timer = setInterval(() => {
      const at = Date.now();
      setNow(at);
      if (at >= endsAt) setStep('expired');
    }, 1000);
    return () => clearInterval(timer);
  }, [view, code]);

  const shownView = useRef(view);
  // Moving to another state puts the player on its first control; opening Accounts focuses nothing.
  useEffect(() => {
    const before = shownView.current;
    shownView.current = view;
    if (before !== null && view !== null && before !== view) firstControl.current?.focus();
  }, [view]);

  if (view === null) return null;

  const getCode = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const fresh = await commands.perlandLinkCode();
      const at = Date.now();
      setNow(at);
      setCode({ code: fresh.code, arrivedAt: at, expiresIn: fresh.expiresIn });
      setCopied(false);
      setStep('code');
    } catch (failure) {
      setError(failure);
      // Signed out elsewhere: the card reads the status again, as the poll does.
      if (isSignedOut(failure)) void qc.invalidateQueries({ queryKey: qk.perland });
    } finally {
      setBusy(false);
    }
  };

  const cancel = () => {
    setError(null);
    setCode(null);
    setStep('idle');
  };

  const unlink = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await commands.perlandUnlink();
      setLinkedName(qc, null);
    } catch (failure) {
      setError(failure);
      if (isSignedOut(failure)) void qc.invalidateQueries({ queryKey: qk.perland });
    } finally {
      setBusy(false);
    }
  };

  const command = code ? `/perland link ${code.code}` : '';
  const copy = () =>
    void copyText(command).then((ok) => {
      if (!ok || !mounted.current) return;
      setCopied(true);
      setAnnounce(t('perland.minecraft.copied'));
      clearTimeout(copiedTimer.current);
      copiedTimer.current = setTimeout(() => setCopied(false), 2000);
    });

  const pending = busy || undefined;
  const setFirst = (el: HTMLButtonElement | null) => void (firstControl.current = el);
  const errorMessage = error === null ? null : describeError(i18n, error).message;

  let line;
  let action = null;
  if (view === 'linked') {
    line = fill(t('perland.minecraft.linked', { name: SLOT }), <span className="font-bold text-ink">{linkedName}</span>);
    action = (
      <Button ref={setFirst} variant="ghost" aria-disabled={pending} onClick={() => void unlink()}>
        {t('perland.minecraft.unlink')}
      </Button>
    );
  } else if (view === 'expired') {
    line = t('perland.minecraft.expired');
    action = (
      <Button ref={setFirst} aria-disabled={pending} onClick={() => void getCode()}>
        {t('perland.minecraft.newCode')}
      </Button>
    );
  } else if (view === 'code') {
    line = fill(t('perland.minecraft.instructions', { command: SLOT }), <code className="inline-code selectable text-ink">{command}</code>);
  } else {
    line = t('perland.minecraft.pitch');
    action = (
      <Button ref={setFirst} aria-disabled={pending} onClick={() => void getCode()}>
        {t('perland.minecraft.link')}
      </Button>
    );
  }

  const left = code ? Math.max(0, Math.ceil((code.arrivedAt + code.expiresIn * 1000 - now) / 1000)) : 0;

  return (
    <section aria-labelledby={headingId} className="panel grid gap-3 px-4 py-3">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
        <span aria-hidden className="stepped flex h-10 w-10 flex-none items-center justify-center bg-raised text-ink-dim">
          <Link className="h-5 w-5" />
        </span>
        <div className="min-w-0 flex-1 basis-56">
          <h3 id={headingId} className="font-bold">
            {t('perland.minecraft.title')}
          </h3>
          <p className="text-sm text-ink-dim">{line}</p>
        </div>
        {action}
      </div>
      {view === 'code' && code && (
        <div className="grid justify-items-start gap-4 pb-1 pl-14">
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <p className="well selectable px-4 py-2 font-display text-2xl tracking-[0.12em] tabular">{code.code}</p>
            <p className="text-sm text-ink-dim tabular">{t('perland.minecraft.expiresIn', { time: formatCountdown(left) })}</p>
          </div>
          <div className="flex flex-wrap gap-3">
            <Button ref={setFirst} icon={copied ? <Check /> : <Copy />} aria-label={copied ? t('common.copied') : undefined} onClick={copy}>
              {copied ? t('common.copied') : t('perland.minecraft.copy')}
            </Button>
            <Button variant="ghost" onClick={cancel}>
              {t('common.cancel')}
            </Button>
          </div>
        </div>
      )}
      {errorMessage && (
        <p role="alert" className="flex items-start gap-2 pl-14 text-sm text-coral">
          <WarningDiamond className="mt-0.5 h-4 w-4 flex-none" aria-hidden />
          {errorMessage}
        </p>
      )}
      <p aria-live="polite" className="sr-only">
        {announce}
      </p>
    </section>
  );
}
