import { open } from '@tauri-apps/plugin-dialog';
import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { Instance, LoaderKind } from '../../ipc/schema';
import { useAppInfo, useInstances, useUpdateInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Check } from '../../ui/Icon';
import { MemorySlider } from '../../ui/MemorySlider';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { loaderName } from '../home/labels';
import { useVersionChoice } from '../new-instance/useVersionChoice';
import { VersionFields } from '../new-instance/VersionFields';

/** The picker cannot install or choose these yet, so an instance that has one keeps its game as it is. */
const FIXED_LOADERS: LoaderKind[] = ['forge', 'neoforge'];

function Form({ instance, systemMb }: { instance: Instance; systemMb: number }) {
  const { t } = useTranslation();
  const choice = useVersionChoice(instance.game);
  const fixedLoader = instance.game.loader && FIXED_LOADERS.includes(instance.game.loader.kind) ? instance.game.loader.kind : null;
  const [name, setName] = useState(instance.name);
  const [memoryMb, setMemoryMb] = useState(instance.java.memoryMb);
  const [javaMode, setJavaMode] = useState<'auto' | 'custom'>(instance.java.path ? 'custom' : 'auto');
  const [javaPath, setJavaPath] = useState<string | null>(instance.java.path);
  // A result belongs to the path it was produced for and only shows while that path is still chosen.
  const [javaResult, setJavaResult] = useState<({ path: string | null } & ({ ok: number } | { error: unknown })) | null>(null);
  const [testing, setTesting] = useState(false);
  const inFlight = useRef(false);
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

  const pickJava = async () => {
    try {
      const chosen = await open({ multiple: false, directory: false, title: t('instanceSettings.javaFiles') });
      if (typeof chosen === 'string') {
        touched();
        setJavaPath(chosen);
        setJavaResult(null);
      }
    } catch (error: unknown) {
      setJavaResult({ path: javaPath, error });
    }
  };
  const testJava = () => {
    if (!javaPath || inFlight.current) return;
    const path = javaPath;
    inFlight.current = true;
    setTesting(true);
    void commands
      .javaTest(path)
      .then(
        (r) => setJavaResult({ path, ok: r.major }),
        (error: unknown) => setJavaResult({ path, error }),
      )
      .finally(() => {
        inFlight.current = false;
        setTesting(false);
      });
  };
  const shownResult = javaResult && javaResult.path === javaPath ? javaResult : null;
  const ready = (fixedLoader !== null || choice.resolved.ready) && (javaMode === 'auto' || javaPath !== null);
  const save = () =>
    update.mutate({
      ...instance,
      name,
      game: fixedLoader ? instance.game : { minecraft: choice.resolved.minecraft ?? instance.game.minecraft, loader: choice.resolved.loader },
      java: { memoryMb, path: javaMode === 'custom' ? javaPath : null },
    });

  return (
    <div className="grid max-w-2xl gap-10">
      <Field label={t('home.nameLabel')}>
        <input className="field-input" value={name} maxLength={64} onChange={(e) => edit(setName)(e.target.value)} />
      </Field>
      <section className="grid gap-4" aria-labelledby="game-heading">
        <h2 id="game-heading" className="text-lg font-bold">{t('instanceSettings.game')}</h2>
        {fixedLoader ? <p className="text-ink-dim">{t('instanceSettings.loaderFixed', { loader: loaderName(fixedLoader) })}</p> : <VersionFields choice={versions} />}
      </section>
      <MemorySlider label={t('instanceSettings.memory')} valueMb={memoryMb} onChange={edit(setMemoryMb)} systemMb={systemMb} />
      <fieldset className="grid gap-3">
        <legend className="mb-1.5 text-lg font-bold">{t('instanceSettings.java')}</legend>
        <div className="flex flex-wrap gap-2">
          {(['auto', 'custom'] as const).map((mode) => (
            <label key={mode} className="seg" data-checked={javaMode === mode ? '' : undefined}>
              <input type="radio" name="java" className="sr-only" checked={javaMode === mode} onChange={() => edit(setJavaMode)(mode)} />
              {mode === 'auto' ? t('instanceSettings.javaAuto') : t('instanceSettings.javaCustom')}
            </label>
          ))}
        </div>
        {javaMode === 'custom' && (
          <div className="grid gap-3">
            <p className="selectable truncate font-mono text-sm text-ink-dim">{javaPath ?? t('instanceSettings.javaNone')}</p>
            <div className="flex gap-3">
              <Button onClick={() => void pickJava()}>{t('instanceSettings.javaPick')}</Button>
              <Button disabled={!javaPath || testing} onClick={testJava}>
                {testing ? t('instanceSettings.javaTesting') : t('instanceSettings.javaTest')}
              </Button>
            </div>
            {shownResult && 'ok' in shownResult && (
              <p role="status" className="flex items-center gap-2 text-lime">
                <Check className="h-5 w-5" aria-hidden />
                {t('instanceSettings.javaOk', { major: shownResult.ok })}
              </p>
            )}
            {shownResult && 'error' in shownResult && <ErrorCard error={shownResult.error} />}
          </div>
        )}
      </fieldset>
      {update.isError && <ErrorCard error={update.error} />}
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
