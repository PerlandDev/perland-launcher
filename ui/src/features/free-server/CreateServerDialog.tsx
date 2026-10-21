import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import type { FreeLimits, FreeServerKind } from '../../ipc/schema';
import { useFreeServerCreate, useInstances, usePerlandStatus } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { Field } from '../../ui/Field';
import { useVersionChoice } from '../new-instance/useVersionChoice';
import { VersionFields } from '../new-instance/VersionFields';
import { engineLanguage, isServerName, limitsText, maybeCreated, prefillName } from './freeServer';
import { ServerOptions } from './ServerOptions';
import { isOptionField, useServerOptions } from './serverOptions';
import { refusalOf, serverStep, useLostPoll, type Step } from './steps';
import { usePlayFreeServer } from './usePlayFreeServer';

const FORM_ID = 'free-server-pick';

/** Create a server for friends, in three steps: pick (vanilla on a Minecraft version or a modpack, the name and, closed by
 *  default, the Options, under the limits), creating, and share (the invite link with Copy, and Play now). A refusal brings the player back to the first step with the reason inline
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
  const [versionError, setVersionError] = useState<string | null>(null);
  const create = useFreeServerCreate({ inline: true });
  const options = useServerOptions(limits);
  // The version picker shows only on an Engine that checks the version (the same `limits.settings` as the Options).
  const choice = useVersionChoice();
  const picksVersion = kind === 'vanilla' && options.show;
  // The pack picked, else the first one listed.
  const pack = packs.find((p) => p.versionId === versionId) ?? packs[0] ?? null;
  const noPacks = kind === 'modrinth' && instances.isSuccess && packs.length === 0;
  const canCreate = kind === 'vanilla' || pack !== null;

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (create.isPending || !canCreate) return;
    const nameOk = isServerName(name);
    setNameError(nameOk ? null : t('freeServer.pick.nameInvalid'));
    if (!options.check() || !nameOk) return;
    setVersionError(null);
    setRefusal(null);
    setStep({ kind: 'creating', lost: false });
    create.mutate(
      {
        name,
        kind,
        // The release the list resolved (or the one picked); none before the list loads, so the Engine runs LATEST.
        version: picksVersion ? choice.mc : null,
        modrinthVersionId: kind === 'modrinth' ? (pack?.versionId ?? null) : null,
        settings: options.settings(),
        language: engineLanguage(i18n.language),
      },
      {
        onSuccess: (server) => setStep({ kind: 'ready', server }),
        onError: (error) => {
          // The Engine may have made it anyway: wait for it to show rather than say it failed.
          if (maybeCreated(error)) return setStep({ kind: 'creating', lost: true });
          const { field, text } = refusalOf(t, i18n, error, name, limits.maxPlayers);
          setNameError(field === 'name' ? text : null);
          setVersionError(field === 'version' && picksVersion ? text : null);
          options.clearErrors();
          const optionField = field !== null && isOptionField(field) && options.show;
          if (optionField) options.refuse(field, text);
          const placed = field === 'name' || (field === 'version' && picksVersion) || optionField;
          setRefusal(placed ? null : text);
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
          {kind === 'vanilla' && !picksVersion && <p className="mt-1 text-sm">{t('freeServer.pick.vanillaHint')}</p>}
        </fieldset>
        {picksVersion && (
          <VersionFields
            choice={{
              ...choice,
              setMinecraft: (next) => {
                setVersionError(null);
                choice.setMinecraft(next);
              },
            }}
            withLoader={false}
            error={versionError}
          />
        )}
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
        <ServerOptions options={options} />
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
      {/* While picking, only the body scrolls (Options open runs past 600 px), so Create stays in view. */}
      <div ref={body} tabIndex={-1} className={`grid gap-3 pt-1 outline-none${step.kind === 'pick' ? ' share-sheet' : ''}`}>
        {content}
      </div>
    </Dialog>
  );
}

