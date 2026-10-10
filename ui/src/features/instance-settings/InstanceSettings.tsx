import { open } from '@tauri-apps/plugin-dialog';
import { useState } from 'react';
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
  const [javaResult, setJavaResult] = useState<{ ok: number } | { error: unknown } | null>(null);
  const update = useUpdateInstance({ inline: true });

  const pickJava = async () => {
    const chosen = await open({ multiple: false, directory: false, title: t('instanceSettings.javaFiles') });
    if (typeof chosen === 'string') {
      setJavaPath(chosen);
      setJavaResult(null);
    }
  };
  const testJava = () => {
    if (!javaPath) return;
    commands.javaTest(javaPath).then(
      (r) => setJavaResult({ ok: r.major }),
      (error: unknown) => setJavaResult({ error }),
    );
  };
  const ready = fixedLoader !== null || choice.resolved.ready;
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
        <input className="field-input" value={name} maxLength={64} onChange={(e) => setName(e.target.value)} />
      </Field>
      <section className="grid gap-4" aria-labelledby="game-heading">
        <h2 id="game-heading" className="text-lg font-bold">{t('instanceSettings.game')}</h2>
        {fixedLoader ? <p className="text-ink-dim">{t('instanceSettings.loaderFixed', { loader: loaderName(fixedLoader) })}</p> : <VersionFields choice={choice} />}
      </section>
      <MemorySlider label={t('instanceSettings.memory')} valueMb={memoryMb} onChange={setMemoryMb} systemMb={systemMb} />
      <fieldset className="grid gap-3">
        <legend className="mb-1.5 text-lg font-bold">{t('instanceSettings.java')}</legend>
        {(['auto', 'custom'] as const).map((mode) => (
          <label key={mode} className="flex items-center gap-3">
            <input type="radio" name="java" checked={javaMode === mode} onChange={() => setJavaMode(mode)} className="accent-[var(--color-amber)]" />
            {mode === 'auto' ? t('instanceSettings.javaAuto') : t('instanceSettings.javaCustom')}
          </label>
        ))}
        {javaMode === 'custom' && (
          <div className="grid gap-3 pl-7">
            <p className="selectable truncate font-mono text-sm text-ink-dim">{javaPath ?? t('instanceSettings.javaNone')}</p>
            <div className="flex gap-3">
              <Button onClick={() => void pickJava()}>{t('instanceSettings.javaPick')}</Button>
              <Button disabled={!javaPath} onClick={testJava}>{t('instanceSettings.javaTest')}</Button>
            </div>
            {javaResult && 'ok' in javaResult && (
              <p className="flex items-center gap-2 text-lime">
                <Check className="h-5 w-5" aria-hidden />
                {t('instanceSettings.javaOk', { major: javaResult.ok })}
              </p>
            )}
            {javaResult && 'error' in javaResult && <ErrorCard error={javaResult.error} />}
          </div>
        )}
      </fieldset>
      {update.isError && <ErrorCard error={update.error} />}
      <div className="flex items-center justify-end gap-4">
        <p className="text-sm text-ink-dim">{update.isSuccess ? t('common.saved') : t('instanceSettings.applyNote')}</p>
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
