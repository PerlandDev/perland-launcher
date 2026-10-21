import { useQuery } from '@tanstack/react-query';
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { FreeLimits, Instance } from '../../ipc/schema';
import { useFriendsCreate, usePerlandStatus } from '../../state/queries';
import { useRun } from '../../state/runStore';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { refusalOf, serverStep, useLostPoll, type Step } from './steps';
import { engineLanguage, isServerName, limitsText, maybeCreated, prefillName } from './freeServer';
import { usePlayFreeServer } from './usePlayFreeServer';

const FORM_ID = 'friends-server';

/** Memory in GB with at most one decimal, as the player reads it ("2.5"). */
const gb = (mb: number, language: string) => new Intl.NumberFormat(language, { maximumFractionDigits: 1 }).format(mb / 1024);

/** Play this with friends (spec 2026-10-21): the player's free server made from this instance's mods and settings. The check
 *  says what the server gets and refuses, before anything is sent, mods that aren't on Modrinth and mods that need more memory
 *  than a free server has; then the create dialog's creating and ready steps. Mount it only while it is open. */
export function FriendsServerDialog({
  instance,
  limits,
  onClose,
}: {
  instance: Instance;
  limits: FreeLimits;
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const status = usePerlandStatus();
  // Asked fresh each time the dialog opens: the mods may have changed since.
  const check = useQuery({
    queryKey: ['friends', 'check', instance.id],
    queryFn: () => commands.friendsCheck(instance.id),
    gcTime: 0,
    retry: false,
  });
  const [step, setStep] = useState<Step>({ kind: 'pick' });
  const [name, setName] = useState(() => prefillName(status.data?.account?.displayName ?? ''));
  const [nameError, setNameError] = useState<string | null>(null);
  const [refusal, setRefusal] = useState<string | null>(null);
  const create = useFriendsCreate({ inline: true });
  const running = useRun(instance.id).state !== 'idle';
  const data = check.data;
  // Why Create is off, said under the form; none while it's on.
  const blocked = !data
    ? null
    : data.notOnModrinth.length > 0
      ? t('friends.notOnModrinth', {
          count: data.notOnModrinth.length,
          names: data.notOnModrinth.join(', '),
        })
      : data.requiredMb > data.limitMb
        ? t('friends.tooHeavy', {
            need: gb(data.requiredMb, i18n.language),
            have: gb(data.limitMb, i18n.language),
          })
        : running
          ? t('friends.running')
          : null;
  const canCreate = check.isSuccess && blocked === null && !create.isPending;
  const refused = blocked !== null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!canCreate) return;
    if (!isServerName(name)) {
      setNameError(t('freeServer.pick.nameInvalid'));
      return;
    }
    setNameError(null);
    setRefusal(null);
    setStep({ kind: 'creating', lost: false });
    create.mutate(
      { id: instance.id, name, language: engineLanguage(i18n.language) },
      {
        onSuccess: (server) => setStep({ kind: 'ready', server }),
        onError: (error) => {
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
  if (step.kind !== 'pick') {
    ({ title, actions, content } = serverStep(t, step, play, onClose));
  } else {
    title = t('friends.title', { name: instance.name });
    actions = (
      <>
        <Button variant="ghost" onClick={onClose}>
          {t('common.cancel')}
        </Button>
        {check.isSuccess && (
          <Button
            type="submit"
            form={FORM_ID}
            variant="primary"
            disabled={refused}
            aria-describedby={blocked !== null ? 'friends-blocked' : undefined}
          >
            {t('freeServer.pick.create')}
          </Button>
        )}
      </>
    );
    content = (
      <form id={FORM_ID} className="grid gap-4" onSubmit={submit} noValidate>
        <p>{t('friends.intro')}</p>
        {blocked !== null && (
          <p id="friends-blocked" className="text-sm font-semibold text-coral" aria-live="polite">
            {blocked}
          </p>
        )}
        {check.isPending && <p role="status">{t('friends.loading')}</p>}
        {check.isError && (
          <ErrorCard error={check.error} actions={<Button onClick={() => void check.refetch()}>{t('common.retry')}</Button>} />
        )}
        {data && (
          <section aria-labelledby="friends-what" className="grid gap-1">
            <h3 id="friends-what" className="text-sm font-semibold text-ink">
              {t('friends.whatGoes')}
            </h3>
            <ul>
              <li className="content-row">
                <div className="min-w-0 flex-1">
                  <p className="font-semibold text-ink">{t('friends.mods')}</p>
                  <p className="tabular text-sm text-ink-dim">{t('friends.modsCount', { count: data.mods })}</p>
                  <p className="tabular text-sm text-ink">
                    {t('friends.memory', {
                      need: gb(data.requiredMb, i18n.language),
                      have: gb(data.limitMb, i18n.language),
                    })}
                  </p>
                </div>
              </li>
              <li className="content-row">
                <div className="min-w-0 flex-1">
                  <p className="font-semibold text-ink">{t('friends.settings')}</p>
                  <p className="tabular text-sm text-ink-dim">{t('friends.settingsCount', { count: data.settings })}</p>
                </div>
              </li>
            </ul>
            <p className="text-sm text-ink-dim">{t('friends.stays')}</p>
          </section>
        )}
        {data && <p className="text-sm">{limitsText(t, limits)}</p>}
        {data && (
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
        )}
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
      <div ref={body} tabIndex={-1} className={`grid gap-3 pt-1 outline-none${step.kind === 'pick' ? ' share-sheet' : ''}`}>
        {content}
      </div>
    </Dialog>
  );
}
