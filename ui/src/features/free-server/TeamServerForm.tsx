import { useQuery } from '@tanstack/react-query';
import { useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { isLauncherError } from '../../ipc/errors';
import type { Instance, TeamServerCreated, TeamServerType } from '../../ipc/schema';
import { useMcVersions, usePerlandStatus, useTeamCreate, useTeamCreateFromInstance, useTeamRoom, useTeamVersions } from '../../state/queries';
import { useRun } from '../../state/runStore';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Select } from '../../ui/Select';
import { loaderName } from '../home/labels';
import { LockedNote } from '../manage/ManageSettings';
import { MinecraftVersionField } from '../new-instance/VersionFields';
import { friendsPath, gbText, isOldVersion, isServerName, prefillName } from './freeServer';
import { isOptionField, TEAM_OPTION_LIMITS, useServerOptions } from './optionRules';
import { ServerOptions } from './ServerOptions';
import { ServerPackRows } from './ServerPackRows';
import { teamRefusalOf } from './teamSteps';

const TYPES: TeamServerType[] = ['vanilla', 'paper', 'fabric', 'forge', 'neoforge', 'quilt'];
const MODDED = new Set<TeamServerType>(['fabric', 'forge', 'neoforge', 'quilt']);

/** The form's fields a refusal or a plan lock can name. */
type FormField = 'name' | 'type' | 'version' | 'loaderVersion' | 'ram';
const FORM_FIELDS: readonly string[] = ['name', 'type', 'version', 'loaderVersion', 'ram'];

/** A server on one of the player's teams (spec 2026-10-21 §6.3), the pick step of the create dialogs once Where names a team:
 *  name, server type, the Minecraft version from the team's Engine and, for a modded type, the loader version, the memory
 *  (whole GB up to what the team has), and the Options with a team's numbers. With `instance`, the instance decides what
 *  runs (said, not picked) and a modded one's mods are checked first. Never says "free". The dialog owns the steps: this
 *  stays mounted (hidden after Create) so a refusal comes back to what the player typed. Key it by team. */
export function TeamServerForm({
  formId,
  tenantId,
  instance,
  hidden,
  onCreating,
  onCreated,
  onRefused,
  onCanCreate,
  lead,
}: {
  /** What comes first in the form (the dialog's Where). */
  lead?: ReactNode;
  formId: string;
  tenantId: string;
  instance: Instance | null;
  hidden: boolean;
  onCreating: () => void;
  onCreated: (server: TeamServerCreated) => void;
  onRefused: () => void;
  onCanCreate: (can: boolean) => void;
}) {
  const { t, i18n } = useTranslation();
  const status = usePerlandStatus();
  const room = useTeamRoom(tenantId);
  const [type, setType] = useState<TeamServerType>('vanilla');
  const [snapshots, setSnapshots] = useState(false);
  const [minecraft, setMinecraft] = useState<string | null>(null);
  const [loaderVersion, setLoaderVersion] = useState<string | null>(null);
  const [memory, setMemory] = useState<number | null>(null);
  const [name, setName] = useState(() => prefillName(instance?.name ?? status.data?.account?.displayName ?? ''));
  const [errors, setErrors] = useState<Partial<Record<FormField, string>>>({});
  const [locked, setLocked] = useState<ReadonlySet<string>>(new Set());
  const [refusal, setRefusal] = useState<{ text: string; locked: boolean } | null>(null);
  const options = useServerOptions(TEAM_OPTION_LIMITS);

  // Picked here only without an instance; an instance decides what runs.
  const picking = instance === null;
  const modded = MODDED.has(type);
  const versions = useTeamVersions(tenantId, type, null, picking);
  const all = versions.data ?? [];
  const list = all.filter((v) => v.stable || snapshots || v.id === minecraft);
  const mc = minecraft ?? list.find((v) => v.stable)?.id ?? list[0]?.id ?? null;
  const loaders = useTeamVersions(tenantId, type, mc, picking && modded && mc !== null);
  const loaderList = loaders.data ?? [];
  const loader = loaderVersion ?? (loaderList.find((v) => v.recommended) ?? loaderList.find((v) => v.stable) ?? loaderList[0])?.id ?? null;
  const noLoader = picking && modded && loaders.isSuccess && loaderList.length === 0;

  // From an instance: what runs follows it, as the app decides it.
  const path = instance ? friendsPath(instance) : null;
  const check = useQuery({
    queryKey: ['teams', tenantId, 'check', instance?.id ?? ''],
    queryFn: () => commands.teamCheck(tenantId, instance?.id ?? ''),
    gcTime: 0,
    retry: false,
    enabled: path === 'pack',
  });
  const mcVersions = useMcVersions(path === 'vanilla');
  const old = path === 'vanilla' && instance !== null && isOldVersion(instance.game.minecraft, mcVersions.data);
  const runState = useRun(instance?.id ?? '').state;
  const data = path === 'pack' ? check.data : undefined;
  // A loader instance with no mods runs that loader as it is: nothing to upload or check.
  const plain = data !== undefined && data.mods === 0;
  const needGb = data && !plain ? Math.ceil(data.requiredMb / 1024) : 0;

  const choices = room.data?.memoryGb ?? [];
  // The first pick: the team's default, or the least that holds the mods.
  const firstGb = room.data ? (choices.find((g) => g >= Math.max(needGb, room.data.defaultGb)) ?? choices.at(-1) ?? null) : null;
  const memoryGb = memory ?? firstGb;

  // The team can make no more servers: said plainly, before anything is sent.
  const noServers = room.data?.serversLeft === 0;
  const blocked = noServers
    ? t('teamServer.noServers')
    : data && !plain
      ? data.notOnModrinth.length > 0
        ? t('teamServer.notOnModrinth', { count: data.notOnModrinth.length, names: data.notOnModrinth.join(', ') })
        : memoryGb !== null && needGb > memoryGb
          ? t('teamServer.needsMemory', { need: gbText(data.requiredMb, i18n.language) })
          : runState !== 'idle'
            ? t('friends.running')
            : null
      : null;
  const ready = path === null ? true : path === 'pack' ? check.isSuccess : !old;
  const create = useTeamCreate({ inline: true });
  const fromInstance = useTeamCreateFromInstance({ inline: true });
  const pending = create.isPending || fromInstance.isPending;
  const versionReady = !picking || (mc !== null && (!modded || loader !== null));
  const canCreate = room.isSuccess && memoryGb !== null && ready && versionReady && blocked === null && !noLoader && !pending;
  useEffect(() => onCanCreate(canCreate), [canCreate, onCanCreate]);

  // A change to a field takes its line away, and its plan lock: another choice may be in the plan.
  const clear = (field: FormField) => {
    setErrors((e) => ({ ...e, [field]: undefined }));
    setLocked((l) => {
      if (!l.has(field)) return l;
      const next = new Set(l);
      next.delete(field);
      return next;
    });
  };
  const isLocked = (field: string) => locked.has(field);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!canCreate || memoryGb === null) return;
    const nameOk = isServerName(name);
    setErrors(nameOk ? {} : { name: t('freeServer.pick.nameInvalid') });
    if (!options.check() || !nameOk) return;
    setRefusal(null);
    // An option's lock is asked again with this create.
    setLocked((l) => new Set([...l].filter((f) => !isOptionField(f))));
    onCreating();
    const settings = options.settings();
    const handlers = {
      onSuccess: onCreated,
      onError: async (error: unknown) => {
        const refused = teamRefusalOf(t, i18n, error);
        // The Engine's 402 is the same for the size and the server count: the room read again tells them apart.
        if (isLauncherError(error) && error.code === 'team_server_too_big') {
          // At its count, the form's own line says so (and turns Create off): nothing under Memory.
          const fresh = await room.refetch();
          if (fresh.data?.serversLeft === 0) {
            options.clearErrors();
            setErrors({});
            setRefusal(null);
            return onRefused();
          }
        }
        options.clearErrors();
        const field = refused.field;
        const optionField = field !== null && isOptionField(field);
        const formField = field !== null && FORM_FIELDS.includes(field) ? (field as FormField) : null;
        if (refused.locked && field !== null && (formField || optionField)) setLocked((l) => new Set([...l, field]));
        if (optionField) options.refuse(field, refused.text);
        setErrors(formField && !refused.locked ? { [formField]: refused.text } : {});
        setRefusal(formField || optionField ? null : { text: refused.text, locked: refused.locked });
        onRefused();
      },
    };
    if (instance) {
      fromInstance.mutate({ tenantId, id: instance.id, name, memoryGb, settings }, handlers);
    } else {
      create.mutate(
        { tenantId, name, type, version: mc ?? '', loaderVersion: modded ? loader : null, memoryGb, settings },
        handlers,
      );
    }
  };

  const lockedNote = (field: string) => (isLocked(field) ? <LockedNote tenantId={tenantId} text={t('teamServer.locked')} /> : null);
  const optionLocked = [...locked].some(isOptionField);

  return (
    <form id={formId} className="grid gap-5" onSubmit={submit} noValidate hidden={hidden}>
      {lead}
      {room.isError && <ErrorCard error={room.error} actions={<Button onClick={() => void room.refetch()}>{t('common.retry')}</Button>} />}
      {instance && path === 'vanilla' &&
        (old ? (
          <p className="font-semibold text-coral">{t('teamServer.oldVersion', { version: instance.game.minecraft })}</p>
        ) : (
          <p>{t('teamServer.vanillaIntro', { version: instance.game.minecraft })}</p>
        ))}
      {instance && path === 'modrinth' && (
        <div className="grid gap-1">
          <p>{t('teamServer.modrinthIntro', { pack: instance.name })}</p>
          <p className="tabular text-sm text-ink-dim">{t('friends.version', { version: instance.game.minecraft })}</p>
        </div>
      )}
      {instance && path === 'pack' && plain && instance.game.loader && (
        <p>
          {t('teamServer.loaderIntro', {
            version: instance.game.minecraft,
            loader: loaderName(instance.game.loader.kind),
            loaderVersion: instance.game.loader.version,
          })}
        </p>
      )}
      {instance && path === 'pack' && data && !plain && <p>{t('teamServer.packIntro')}</p>}
      {blocked !== null && (
        <p id={`${formId}-blocked`} className="text-sm font-semibold text-coral" aria-live="polite">
          {blocked}
        </p>
      )}
      {path === 'pack' && check.isPending && <p role="status">{t('friends.loading')}</p>}
      {path === 'pack' && check.isError && (
        <ErrorCard error={check.error} actions={<Button onClick={() => void check.refetch()}>{t('common.retry')}</Button>} />
      )}
      {data && !plain && <ServerPackRows data={data} memoryLine={t('teamServer.memoryLine', { need: gbText(data.requiredMb, i18n.language) })} />}
      {picking && (
        <fieldset className="grid gap-1.5">
          <legend className="mb-1.5 text-sm font-semibold">{t('teamServer.type')}</legend>
          <div className="flex flex-wrap gap-2">
            {TYPES.map((value) => (
              <label key={value} className="seg text-ink" data-checked={type === value ? '' : undefined}>
                <input
                  type="radio"
                  name={`${formId}-type`}
                  className="sr-only"
                  checked={type === value}
                  onChange={() => {
                    setType(value);
                    setMinecraft(null);
                    setLoaderVersion(null);
                    clear('type');
                    clear('version');
                  }}
                />
                {t(`teamServer.types.${value}`)}
              </label>
            ))}
          </div>
          {errors.type && (
            <p role="alert" className="mt-1 text-sm text-coral">
              {errors.type}
            </p>
          )}
          {isLocked('type') && <div className="mt-1">{lockedNote('type')}</div>}
        </fieldset>
      )}
      {picking && (
        <div className="grid gap-3">
          <MinecraftVersionField
            choice={{
              list,
              mc,
              setMinecraft: (next) => {
                setMinecraft(next);
                setLoaderVersion(null);
                clear('version');
              },
              snapshots,
              setSnapshots,
              versions,
            }}
            error={errors.version}
          />
          {versions.isSuccess && all.length === 0 && <p className="text-sm text-ink-dim">{t('teamServer.noVersions')}</p>}
          {lockedNote('version')}
        </div>
      )}
      {picking &&
        modded &&
        mc !== null &&
        (noLoader ? (
          <p className="text-ink-dim">{t('teamServer.noLoader', { version: mc })}</p>
        ) : (
          <div className="grid gap-3">
            <Select
              label={t('teamServer.loaderVersion')}
              value={loader ?? ''}
              onChange={(next) => {
                setLoaderVersion(next);
                clear('loaderVersion');
              }}
              disabled={loaderList.length === 0}
              error={errors.loaderVersion}
              options={loaderList.map((v) => ({
                value: v.id,
                label: t(v.recommended ? 'teamServer.recommended' : v.stable ? 'teamServer.stable' : 'teamServer.beta', { version: v.id }),
              }))}
            />
            {loaders.isError && <ErrorCard error={loaders.error} actions={<Button onClick={() => void loaders.refetch()}>{t('common.retry')}</Button>} />}
            {lockedNote('loaderVersion')}
          </div>
        ))}
      {ready && (
        <div className="grid gap-3">
          <Field label={t('teamServer.memory')} hint={isLocked('ram') ? null : t('teamServer.memoryHint')} error={errors.ram} alert>
            <select
              className="field-input w-40 tabular text-ink"
              value={memoryGb ?? ''}
              disabled={choices.length === 0}
              onChange={(e) => {
                setMemory(Number(e.target.value));
                clear('ram');
              }}
            >
              {choices.map((g) => (
                <option key={g} value={g}>
                  {t('teamServer.memoryGb', { count: g })}
                </option>
              ))}
            </select>
          </Field>
          {lockedNote('ram')}
        </div>
      )}
      {ready && (
        <Field label={t('freeServer.pick.name')} hint={t('teamServer.nameHint')} error={errors.name} alert>
          <input
            className="field-input text-ink"
            value={name}
            maxLength={32}
            spellCheck={false}
            autoComplete="off"
            onChange={(e) => {
              setName(e.target.value);
              clear('name');
            }}
          />
        </Field>
      )}
      {ready && <ServerOptions options={options} />}
      {optionLocked && <LockedNote tenantId={tenantId} text={t('teamServer.locked')} />}
      {refusal &&
        (refusal.locked ? (
          <LockedNote tenantId={tenantId} text={refusal.text} />
        ) : (
          <p role="alert" className="text-sm text-coral">
            {refusal.text}
          </p>
        ))}
    </form>
  );
}
