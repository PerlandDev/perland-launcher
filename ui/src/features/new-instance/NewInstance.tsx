import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useCreateInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ComingSoon } from '../../ui/ComingSoon';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { Download, Package } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { useVersionChoice } from './useVersionChoice';
import { VersionFields } from './VersionFields';

export function NewInstance() {
  const { t } = useTranslation();
  const choice = useVersionChoice();
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
    <ScreenFrame title={t('newInstance.title')} onBack={() => navigate({ name: 'home', id: null })}>
      <form onSubmit={submit} className="grid max-w-2xl gap-8">
        <Field label={t('newInstance.name')}>
          <input className="field-input" value={name} placeholder={fallbackName} maxLength={64} onChange={(e) => setName(e.target.value)} />
        </Field>
        <VersionFields choice={choice} />
        <div className="grid gap-3 sm:grid-cols-2">
          <ComingSoon label={t('newInstance.packLink')} icon={<Download className="h-5 w-5" />} />
          <ComingSoon label={t('newInstance.mrpack')} icon={<Package className="h-5 w-5" />} />
        </div>
        {create.isError && <ErrorCard error={create.error} />}
        <div className="sticky bottom-0 flex justify-end gap-3 bg-ground py-4 shadow-[0_-1px_0_var(--color-line)]">
          <Button onClick={() => navigate({ name: 'home', id: null })}>{t('common.cancel')}</Button>
          <Button type="submit" variant="primary" size="lg" disabled={!choice.resolved.ready || create.isPending}>
            {create.isPending ? t('newInstance.creating') : t('newInstance.create')}
          </Button>
        </div>
      </form>
    </ScreenFrame>
  );
}
