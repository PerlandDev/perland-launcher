import { useQueryClient } from '@tanstack/react-query';
import type { i18n as I18n, TFunction } from 'i18next';
import { useEffect, useRef, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { describeError, isLauncherError } from '../../ipc/errors';
import type { FreeServerCard } from '../../ipc/schema';
import { qk } from '../../state/queries';
import { pushNotice } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { CopyRow } from '../../ui/CopyRow';
import { isServerName, retryText } from './freeServer';

// The steps of making the free server that every dialog shares: creating, ready, waiting out a lost answer, and a refusal
// in words.

/** After a lost create answer: how often the page is read again, and for how long. */
export const LOST_POLL_MS = 5_000;
export const LOST_GIVE_UP_MS = 5 * 60_000;

export type Step = { kind: 'pick' } | { kind: 'creating'; lost: boolean } | { kind: 'ready'; server: FreeServerCard };

/** The steps after the pick, shared by every way of making the free server: creating (a status sentence and Close), and
 *  ready (the invite link with Copy, and Play now). */
export function serverStep(
  t: TFunction,
  step: Exclude<Step, { kind: 'pick' }>,
  play: { busy: boolean; run: () => void },
  onClose: () => void,
): { title: string; actions: ReactNode; content: ReactNode } {
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
  } else {
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
  }
  return { title, actions, content };
}

/** After a lost create answer (`active`): the page is read every 5 s until the server shows, done being created; after
 *  5 minutes the dialog closes with a notice that the server will show in Your servers. */
export function useLostPoll(active: boolean, onReady: (server: FreeServerCard) => void, onClose: () => void) {
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

/** A refusal in words: a bad name goes on the name field, everything else is one line under the form. `invalid_free_server`
 *  names no field, so it goes on the name only when the name itself breaks the rule. */
export function refusalOf(t: TFunction, i18n: I18n, error: unknown, name: string): { name: string | null; line: string | null } {
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
