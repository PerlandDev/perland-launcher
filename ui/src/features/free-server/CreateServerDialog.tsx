import { useQueryClient } from '@tanstack/react-query';
import type { i18n as I18n, TFunction } from 'i18next';
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { describeError, isLauncherError } from '../../ipc/errors';
import type { FreeLimits, FreeServerCard, FreeServerKind } from '../../ipc/schema';
import { qk, useFreeServerCreate, useInstances, usePerlandStatus } from '../../state/queries';
import { navigate } from '../../state/route';
import { pushNotice } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { copyText } from '../../ui/copyText';
import { Dialog } from '../../ui/Dialog';
import { Field } from '../../ui/Field';
import { Check, Copy } from '../../ui/Icon';
import { engineLanguage, isServerName, limitsText, maybeCreated, prefillName, retryText } from './freeServer';
import { usePlayFreeServer } from './usePlayFreeServer';

/** After a lost create answer: how often the page is read again, and for how long. */
export const LOST_POLL_MS = 5_000;
export const LOST_GIVE_UP_MS = 5 * 60_000;

type Step = { kind: 'pick' } | { kind: 'creating'; lost: boolean } | { kind: 'ready'; server: FreeServerCard };

const FORM_ID = 'free-server-pick';

/** Create a server for friends, in three steps: pick (vanilla or a modpack, and the name, under the limits), creating, and
 *  share (the invite link with Copy, and Play now). A refusal brings the player back to the first step with the reason inline
 *  and what they typed kept. Mount it only while it is open: closing unmounts it, so it starts over. */
export function CreateServerDialog({ limits, onClose }: { limits: FreeLimits; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const status = usePerlandStatus();
  const instances = useInstances();
  const packs = (instances.data?.instances ?? []).flatMap((i) =>
    i.source.kind === 'modrinth' ? [{ id: i.id, name: i.name, versionId: i.source.versionId }] : [],
  );
  const [step, setStep] = useState<Step>({ kind: 'pick' });
  const [kind, setKind] = useState<FreeServerKind>('vanilla');
  const [versionId, setVersionId] = useState<string | null>(null);
  const [name, setName] = useState(() => prefillName(status.data?.account?.displayName ?? ''));
  const [nameError, setNameError] = useState<string | null>(null);
  const [refusal, setRefusal] = useState<string | null>(null);
  const create = useFreeServerCreate({ inline: true });
  // The pack picked, else the first one listed.
  const pack = packs.find((p) => p.versionId === versionId) ?? packs[0] ?? null;
  const noPacks = kind === 'modrinth' && instances.isSuccess && packs.length === 0;
  const canCreate = kind === 'vanilla' || pack !== null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (create.isPending || !canCreate) return;
    if (!isServerName(name)) {
      setNameError(t('freeServer.pick.nameInvalid'));
      return;
    }
    setNameError(null);
    setRefusal(null);
    setStep({ kind: 'creating', lost: false });
    create.mutate(
      {
        name,
        kind,
        modrinthVersionId: kind === 'modrinth' ? (pack?.versionId ?? null) : null,
        language: engineLanguage(i18n.language),
      },
      {
        onSuccess: (server) => setStep({ kind: 'ready', server }),
        onError: (error) => {
          // The Engine may have made it anyway: wait for it to show rather than say it failed.
          if (maybeCreated(error)) return setStep({ kind: 'creating', lost: true });
          const shown = refusalOf(t, i18n, error, name);
          setNameError(shown.name);
          setRefusal(shown.line);
          setStep({ kind: 'pick' });
        },
      },
    );
  };

  const play = usePlayFreeServer(onClose);
  useLostPoll(step.kind === 'creating' && step.lost, (server) => setStep({ kind: 'ready', server }), onClose);
  // A new step replaces the control that had focus (Create): focus goes to the step's own content.
  const body = useRef<HTMLDivElement>(null);
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    body.current?.focus();
  }, [step.kind]);

  let title: string;
  let actions: ReactNode;
  let content: ReactNode;
  if (step.kind === 'creating') {
    title = t(step.lost ? 'freeServer.creating.lost' : 'freeServer.creating.title');
    actions = <Button onClick={onClose}>{t('common.close')}</Button>;
    content = (
      <p role="status" className="text-sm">
        {t(step.lost ? 'freeServer.creating.lostHint' : 'freeServer.creating.hint')}
      </p>
    );
  } else if (step.kind === 'ready') {
    const { server } = step;
    title = t('freeServer.ready.title');
    actions = (
      <>
        <Button onClick={onClose}>{t('common.close')}</Button>
        {server.invite !== null && (
          <Button variant="primary" aria-disabled={play.busy || undefined} onClick={play.run}>
            {play.busy ? t('freeServer.ready.starting') : t('freeServer.ready.playNow')}
          </Button>
        )}
      </>
    );
    content = server.inviteUrl ? (
      <>
        <p className="text-sm">{t('freeServer.ready.share')}</p>
        <CopyRow
          text={server.inviteUrl}
          label={t('freeServer.ready.link')}
          copy={t('freeServer.ready.copyLink')}
          copied={t('freeServer.ready.linkCopied')}
        />
      </>
    ) : (
      server.address && (
        <>
          <p className="text-sm">{t('freeServer.ready.addressHint')}</p>
          <CopyRow
            text={server.address}
            label={t('freeServer.ready.address')}
            copy={t('freeServer.ready.copyAddress')}
            copied={t('freeServer.ready.addressCopied')}
          />
        </>
      )
    );
  } else {
    title = t('freeServer.pick.title');
    actions = (
      <>
        <Button onClick={onClose}>{t('common.cancel')}</Button>
        <Button type="submit" form={FORM_ID} variant="primary" disabled={!canCreate}>
          {t('freeServer.pick.create')}
        </Button>
      </>
    );
    content = (
      <form id={FORM_ID} className="grid gap-5" onSubmit={submit} noValidate>
        <p className="text-sm">{limitsText(t, limits)}</p>
        <fieldset className="grid gap-1.5">
          <legend className="mb-1.5 text-sm font-semibold">{t('freeServer.pick.what')}</legend>
          <div className="flex flex-wrap gap-2">
            {(['vanilla', 'modrinth'] as const).map((value) => (
              <label key={value} className="seg text-ink" data-checked={kind === value ? '' : undefined}>
                <input type="radio" name="free-server-kind" className="sr-only" checked={kind === value} onChange={() => setKind(value)} />
                {t(value === 'vanilla' ? 'freeServer.pick.vanilla' : 'freeServer.pick.modpack')}
              </label>
            ))}
          </div>
          {/* 0.25rem below the segments' 4px travel, so the hint never touches a pressed segment. */}
          {kind === 'vanilla' && <p className="mt-1 text-sm">{t('freeServer.pick.vanillaHint')}</p>}
        </fieldset>
        {kind === 'modrinth' && noPacks && (
          <p className="text-sm">
            {t('freeServer.pick.noPacks')}{' '}
            <button
              type="button"
              className="text-blue-ink underline underline-offset-4"
              onClick={() => {
                onClose();
                navigate({ name: 'browse', kind: 'modpack' });
              }}
            >
              {t('freeServer.pick.browse')}
            </button>
          </p>
        )}
        {kind === 'modrinth' && pack && (
          <Field label={t('freeServer.pick.pack')} hint={t('freeServer.pick.packHint')}>
            <select className="field-input text-ink" value={pack.versionId} onChange={(e) => setVersionId(e.target.value)}>
              {packs.map((p) => (
                <option key={p.id} value={p.versionId}>
                  {p.name}
                </option>
              ))}
            </select>
          </Field>
        )}
        <Field label={t('freeServer.pick.name')} hint={t('freeServer.pick.nameHint')} error={nameError} alert>
          <input
            className="field-input text-ink"
            value={name}
            maxLength={32}
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => {
              setName(e.target.value);
              setNameError(null);
            }}
          />
        </Field>
        {refusal && (
          <p role="alert" className="text-sm text-coral">
            {refusal}
          </p>
        )}
      </form>
    );
  }
  return (
    <Dialog open onClose={onClose} title={title} actions={actions}>
      <div ref={body} tabIndex={-1} className="grid gap-3 pt-1 outline-none">
        {content}
      </div>
    </Dialog>
  );
}

/** After a lost create answer (`active`): the page is read every 5 s until the server shows, done being created; after
 *  5 minutes the dialog closes with a notice that the server will show in Your servers. */
function useLostPoll(active: boolean, onReady: (server: FreeServerCard) => void, onClose: () => void) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  // The latest callbacks and `t`, so the poll runs once per lost answer: a language change keeps its window.
  const done = useRef({ onClose, onReady, t });
  useEffect(() => {
    done.current = { onClose, onReady, t };
  });
  useEffect(() => {
    if (!active) return;
    const started = Date.now();
    let stopped = false;
    const stop = () => {
      stopped = true;
      clearInterval(timer);
    };
    const timer = setInterval(() => {
      if (Date.now() - started >= LOST_GIVE_UP_MS) {
        stop();
        const { t } = done.current;
        pushNotice({
          title: t('freeServer.creating.gaveUp'),
          body: t('freeServer.creating.gaveUpBody'),
        });
        done.current.onClose();
        return;
      }
      void qc
        .fetchQuery({
          queryKey: qk.freeServer,
          queryFn: commands.freeServer,
          staleTime: 0,
        })
        .then((page) => {
          const server = page?.server;
          if (stopped || !server || server.freeStatus === 'creating') return;
          stop();
          done.current.onReady(server);
        })
        .catch(() => {});
    }, LOST_POLL_MS);
    return stop;
  }, [active, qc]);
}

/** A recessed row holding `text` (selectable, in mono) and a Copy key that swaps to a check and `copied` for 2 s. */
function CopyRow({ text, label, copy, copied }: { text: string; label: string; copy: string; copied: string }) {
  const { t } = useTranslation();
  const [done, setDone] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const run = () =>
    void copyText(text).then((ok) => {
      if (!ok) return;
      setDone(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setDone(false), 2000);
    });
  return (
    <div className="well copy-row">
      <span className="sr-only">{label}</span>
      <span className="copy-row-text">{text}</span>
      <Button variant="ghost" icon={done ? <Check /> : <Copy />} aria-label={copy} onClick={run}>
        {done ? t('common.copied') : t('common.copy')}
      </Button>
      <span className="sr-only" aria-live="polite">
        {done ? copied : ''}
      </span>
    </div>
  );
}

/** A refusal in words: a bad name goes on the name field, everything else is one line under the form. `invalid_free_server`
 *  names no field, so it goes on the name only when the name itself breaks the rule. */
function refusalOf(t: TFunction, i18n: I18n, error: unknown, name: string): { name: string | null; line: string | null } {
  if (!isLauncherError(error)) return { name: null, line: describeError(i18n, error).message };
  switch (error.code) {
    case 'invalid_free_server':
      return isServerName(name) ? { name: null, line: t('freeServer.pick.refused') } : { name: t('freeServer.pick.nameInvalid'), line: null };
    case 'modpack_unsupported': {
      const key = `freeServer.unsupported.${error.params.reason ?? ''}`;
      return {
        name: null,
        line: i18n.exists(key) ? t(key) : describeError(i18n, error).message,
      };
    }
    case 'email_required':
      return { name: null, line: t('freeServer.reason.emailRequired') };
    case 'too_many_free_servers': {
      const retry = Number(error.params.retryAfter);
      return {
        name: null,
        line: Number.isFinite(retry) ? retryText(t, retry) : describeError(i18n, error).message,
      };
    }
    default:
      return { name: null, line: describeError(i18n, error).message };
  }
}
