import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { describeError, isLauncherError } from '../../ipc/errors';
import { inviteHost } from '../../ipc/inviteLink';
import type { Instance } from '../../ipc/schema';
import { useAppInfo, useInstances, useUpdateInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { useRun } from '../../state/runStore';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Copy } from '../../ui/Icon';
import { copyText } from '../../ui/copyText';
import { MemorySlider } from '../../ui/MemorySlider';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { inviteOf, isPack, isServer, versionLine } from '../home/labels';
import { useVersionChoice } from '../new-instance/useVersionChoice';
import { VersionFields } from '../new-instance/VersionFields';
import { JavaPicker } from './JavaPicker';
import { useJavaChoice } from './useJavaChoice';

function Form({ instance, systemMb }: { instance: Instance; systemMb: number }) {
  const { t, i18n } = useTranslation();
  const choice = useVersionChoice(instance.game);
  const pack = isPack(instance);
  const server = isServer(instance);
  // A run saves the whole instance when it syncs, so the server link changes only while idle.
  const running = useRun(instance.id).state !== 'idle';
  const [policy, setPolicy] = useState(instance.updatePolicy);
  const [name, setName] = useState(instance.name);
  const [memoryMb, setMemoryMb] = useState(instance.java.memoryMb);
  const [javaArgs, setJavaArgs] = useState(instance.java.args);
  const argsField = useRef<HTMLTextAreaElement>(null);
  const update = useUpdateInstance({ inline: true });

  // Any edit after a save (or a failed save) makes "Saved" or the old error stale.
  const touched = () => {
    if (update.isSuccess || update.isError) update.reset();
  };
  const edit =
    <T,>(set: (value: T) => void) =>
    (value: T) => {
      touched();
      set(value);
    };
  const versions: typeof choice = {
    ...choice,
    setMinecraft: edit(choice.setMinecraft),
    setLoader: edit(choice.setLoader),
    setLoaderVersion: edit(choice.setLoaderVersion),
  };
  const minecraft = pack ? instance.game.minecraft : (choice.resolved.minecraft ?? instance.game.minecraft);
  const java = useJavaChoice(instance, minecraft, touched);

  // A refused argument belongs under its field; every other failure stays on the general card.
  const argsRefused = update.isError && isLauncherError(update.error) && update.error.code.startsWith('jvm_');
  const ready = (pack || choice.resolved.ready) && java.ready;
  const save = () =>
    update.mutate(
      {
        ...instance,
        name,
        game: pack ? instance.game : { minecraft, loader: choice.resolved.loader },
        updatePolicy: policy,
        java: { memoryMb, path: java.value, args: javaArgs },
      },
      {
        // The Save bar is sticky, so a refusal under the field can sit off-screen; focusing it scrolls it into view.
        onError: (error) => {
          if (isLauncherError(error) && error.code.startsWith('jvm_')) argsField.current?.focus();
        },
      },
    );

  return (
    <div className="grid max-w-2xl gap-10">
      <Field label={t('home.nameLabel')}>
        <input className="field-input" value={name} maxLength={64} onChange={(e) => edit(setName)(e.target.value)} />
      </Field>
      <section className="grid gap-4" aria-labelledby="game-heading">
        <h2 id="game-heading" className="text-lg font-bold">{t('instanceSettings.game')}</h2>
        {pack ? (
          <div className="grid gap-1">
            <p>{versionLine(t, instance)}</p>
            <p className="text-sm text-ink-dim">{t('instanceSettings.gameFromPack')}</p>
          </div>
        ) : (
          <VersionFields choice={versions} />
        )}
      </section>
      {pack && !server && (
        <fieldset className="grid gap-3">
          <legend className="mb-1.5 text-lg font-bold">{t('instanceSettings.updates')}</legend>
          <div className="flex flex-wrap gap-2">
            {(['auto', 'ask', 'pinned'] as const).map((value) => (
              <label key={value} className="seg" data-checked={policy === value ? '' : undefined}>
                <input type="radio" name="updates" className="sr-only" checked={policy === value} onChange={() => edit(setPolicy)(value)} />
                {t(value === 'auto' ? 'instanceSettings.updatesAuto' : value === 'ask' ? 'instanceSettings.updatesAsk' : 'instanceSettings.updatesPinned')}
              </label>
            ))}
          </div>
        </fieldset>
      )}
      {server && (
        <section className="grid gap-3" aria-labelledby="server-settings">
          <h2 id="server-settings" className="text-lg font-bold">
            {t('server.settingsTitle')}
          </h2>
          <p>{t('server.settingsHost', { host: inviteHost(inviteOf(instance) ?? '') ?? '' })}</p>
          <div className="flex flex-wrap gap-3">
            <Button icon={<Copy />} onClick={() => void copyText(inviteOf(instance) ?? '')}>
              {t('server.settingsCopyLink')}
            </Button>
            <Button
              disabled={running}
              aria-describedby={running ? 'relink-running' : undefined}
              onClick={() => navigate({ name: 'joinServer', invite: '', relinkFor: instance.id })}
            >
              {t('server.useNewLink')}
            </Button>
          </div>
          {running && (
            <p id="relink-running" className="text-sm text-ink-dim">
              {t('server.relinkRunning')}
            </p>
          )}
        </section>
      )}
      {!pack && (
        <section className="grid gap-3" aria-labelledby="server-settings">
          <h2 id="server-settings" className="text-lg font-bold">
            {t('server.settingsTitle')}
          </h2>
          <p>{t('server.linkSettingsBody')}</p>
          <div className="flex flex-wrap gap-3">
            <Button
              disabled={running}
              aria-describedby={running ? 'link-running' : undefined}
              onClick={() => navigate({ name: 'joinServer', invite: '', linkFor: instance.id })}
            >
              {t('server.linkToServer')}
            </Button>
          </div>
          {running && (
            <p id="link-running" className="text-sm text-ink-dim">
              {t('server.linkRunning')}
            </p>
          )}
        </section>
      )}
      <MemorySlider label={t('instanceSettings.memory')} valueMb={memoryMb} onChange={edit(setMemoryMb)} systemMb={systemMb} />
      <JavaPicker choice={java} />
      <Field
        label={t('instanceSettings.javaArgs')}
        hint={t('instanceSettings.javaArgsHint')}
        error={argsRefused ? describeError(i18n, update.error).message : null}
      >
        <textarea
          ref={argsField}
          className="field-input resize-y font-mono text-sm"
          rows={2}
          spellCheck={false}
          autoComplete="off"
          value={javaArgs}
          onChange={(e) => edit(setJavaArgs)(e.target.value)}
        />
      </Field>
      {update.isError && !argsRefused && <ErrorCard error={update.error} />}
      <div className="sticky bottom-0 flex items-center justify-end gap-4 bg-ground py-4 shadow-[0_-1px_0_var(--color-line)]">
        <p role="status" className="text-sm text-ink-dim">{update.isSuccess ? t('common.saved') : t('instanceSettings.applyNote')}</p>
        <Button variant="primary" size="lg" disabled={!ready || update.isPending} onClick={save}>
          {update.isPending ? t('common.saving') : t('common.save')}
        </Button>
      </div>
    </div>
  );
}

export function InstanceSettings({ id }: { id: string }) {
  const { t } = useTranslation();
  const listing = useInstances();
  const appInfo = useAppInfo();
  const instance = listing.data?.instances.find((i) => i.id === id);
  return (
    <ScreenFrame title={t('instanceSettings.title')} subtitle={instance?.name} onBack={() => navigate({ name: 'home', id })}>
      {instance && appInfo.data ? <Form key={instance.id} instance={instance} systemMb={appInfo.data.systemMemoryMb} /> : null}
    </ScreenFrame>
  );
}
