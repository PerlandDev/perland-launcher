import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCreateInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Search } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { PackFileForm, PackLinkForm } from './PackSource';
import { useVersionChoice } from './useVersionChoice';
import { VersionFields } from './VersionFields';

type Mode = 'version' | 'link' | 'file';

export function NewInstance({ link }: { link?: string }) {
  const { t } = useTranslation();
  const choice = useVersionChoice();
  const [mode, setMode] = useState<Mode>(link ? 'link' : 'version');
  const [name, setName] = useState('');
  const create = useCreateInstance({ inline: true });
  const { minecraft: resolvedVersion } = choice.resolved;
  const fallbackName = resolvedVersion ? t('newInstance.namePlaceholder', { version: resolvedVersion }) : t('newInstance.namePlaceholderFallback');

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const { minecraft, loader, ready } = choice.resolved;
    if (!ready || !minecraft) return;
    create.mutate(
      { name: name.trim() || fallbackName, minecraft, loader },
      { onSuccess: (instance) => navigate({ name: 'home', id: instance.id }) },
    );
  };

  return (
    <ScreenFrame
      title={t('newInstance.title')}
      onBack={() => navigate({ name: 'home', id: null })}
      actions={
        <Button icon={<Search />} onClick={() => navigate({ name: 'browse', kind: 'modpack' })}>
          {t('browse.browseModpacks')}
        </Button>
      }
    >
      <fieldset className="mb-8 grid gap-1.5">
        <legend className="sr-only">{t('newInstance.title')}</legend>
        <div className="flex flex-wrap gap-2">
          {(['version', 'link', 'file'] as const).map((m) => (
            <label key={m} className="seg" data-checked={mode === m ? '' : undefined}>
              <input type="radio" name="new-instance-mode" className="sr-only" checked={mode === m} onChange={() => setMode(m)} />
              {m === 'version' ? t('newInstance.modeVersion') : m === 'link' ? t('newInstance.packLink') : t('newInstance.mrpack')}
            </label>
          ))}
        </div>
      </fieldset>
      {mode === 'version' && (
        <form onSubmit={submit} className="grid max-w-2xl gap-8">
          <Field label={t('newInstance.name')}>
            <input className="field-input" value={name} placeholder={fallbackName} maxLength={64} onChange={(e) => setName(e.target.value)} />
          </Field>
          <VersionFields choice={choice} />
          {create.isError && <ErrorCard error={create.error} />}
          <div className="sticky bottom-0 flex justify-end gap-3 bg-ground py-4 shadow-[0_-1px_0_var(--color-line)]">
            <Button onClick={() => navigate({ name: 'home', id: null })}>{t('common.cancel')}</Button>
            <Button type="submit" variant="primary" size="lg" disabled={!choice.resolved.ready || create.isPending}>
              {create.isPending ? t('newInstance.creating') : t('newInstance.create')}
            </Button>
          </div>
        </form>
      )}
      {mode === 'link' && <PackLinkForm initial={link} />}
      {mode === 'file' && <PackFileForm />}
    </ScreenFrame>
  );
}
