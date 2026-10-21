import { useQuery } from '@tanstack/react-query';
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { FreeLimits, Instance } from '../../ipc/schema';
import { useFriendsCreate, useMcVersions, usePerlandStatus } from '../../state/queries';
import { useRun } from '../../state/runStore';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { ServerOptions } from './ServerOptions';
import { useServerOptions } from './serverOptions';
import { refusalOf, serverStep, useLostPoll, type Step } from './steps';
import { engineLanguage, friendsPath, isOldVersion, isServerName, limitsText, maybeCreated, prefillName } from './freeServer';
import { usePlayFreeServer } from './usePlayFreeServer';

const FORM_ID = 'friends-server';

/** Memory in GB with at most one decimal, as the player reads it ("2.5"). */
const gb = (mb: number, language: string) => new Intl.NumberFormat(language, { maximumFractionDigits: 1 }).format(mb / 1024);

/** Play this with friends (specs 2026-10-21): the player's free server made from this instance, the way friends_create makes
 *  it. Vanilla runs the instance's Minecraft version (said, not picked; an old alpha or beta can't be a server). A Modrinth
 *  modpack runs as published. Any other modded instance is checked first: what the server gets, and, before anything is sent,
 *  mods that aren't on Modrinth and mods that need more memory than a free server has. Then the name, the Options when the
 *  Engine takes them, and the create dialog's creating and ready steps. Mount it only while it is open. */
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
  const path = friendsPath(instance);
  const minecraft = instance.game.minecraft;
  // Asked fresh each time the dialog opens: the mods may have changed since. Only the pack upload has mods to check.
  const check = useQuery({
    queryKey: ['friends', 'check', instance.id],
    queryFn: () => commands.friendsCheck(instance.id),
    gcTime: 0,
    retry: false,
    enabled: path === 'pack',
  });
  const versions = useMcVersions(path === 'vanilla');
  const old = path === 'vanilla' && isOldVersion(minecraft, versions.data);
  const options = useServerOptions(limits);
  const [step, setStep] = useState<Step>({ kind: 'pick' });
  const [name, setName] = useState(() => prefillName(status.data?.account?.displayName ?? ''));
  const [nameError, setNameError] = useState<string | null>(null);
  const [refusal, setRefusal] = useState<string | null>(null);
  const create = useFriendsCreate({ inline: true });
  // The pack is read from the instance's folder, so it waits for the game to stop.
  const runState = useRun(instance.id).state;
  const running = path === 'pack' && runState !== 'idle';
  const data = path === 'pack' ? check.data : undefined;
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
  // Whether the form shows: the pack's check answered, or (vanilla, Modrinth) at once unless the version can't run.
  const ready = path === 'pack' ? check.isSuccess : !old;
  const canCreate = ready && blocked === null && !create.isPending;
  const refused = blocked !== null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!canCreate) return;
    const nameOk = isServerName(name);
    setNameError(nameOk ? null : t('freeServer.pick.nameInvalid'));
    if (!options.check() || !nameOk) return;
    setRefusal(null);
    setStep({ kind: 'creating', lost: false });
    create.mutate(
      { id: instance.id, name, settings: options.settings(), language: engineLanguage(i18n.language) },
      {
        onSuccess: (server) => setStep({ kind: 'ready', server }),
        onError: (error) => {
          if (maybeCreated(error)) return setStep({ kind: 'creating', lost: true });
          const { field, text } = refusalOf(t, i18n, error, name, limits.maxPlayers);
          setNameError(field === 'name' ? text : null);
          options.clearErrors();
          const optionField = field !== null && field.startsWith('settings.') && options.show;
          if (optionField) options.refuse(field, text);
          // The version isn't picked here, so its refusal is a line.
          setRefusal(field === 'name' || optionField ? null : text);
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
        {ready && (
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
        {path === 'pack' && <p>{t('friends.intro')}</p>}
        {path === 'vanilla' &&
          (old ? (
            <p className="font-semibold text-coral">{t('friends.oldVersion', { version: minecraft })}</p>
          ) : (
            <p>{t('friends.vanillaIntro', { version: minecraft })}</p>
          ))}
        {path === 'modrinth' && (
          <div className="grid gap-1">
            <p>{t('friends.modrinthIntro', { pack: instance.name })}</p>
            <p className="tabular text-sm text-ink-dim">{t('friends.version', { version: minecraft })}</p>
          </div>
        )}
        {blocked !== null && (
          <p id="friends-blocked" className="text-sm font-semibold text-coral" aria-live="polite">
            {blocked}
          </p>
        )}
        {path === 'pack' && check.isPending && <p role="status">{t('friends.loading')}</p>}
        {path === 'pack' && check.isError && (
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
        {ready && <p className="text-sm">{limitsText(t, limits)}</p>}
        {ready && (
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
        {ready && <ServerOptions options={options} />}
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
