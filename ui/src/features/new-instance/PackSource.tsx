import { open } from '@tauri-apps/plugin-dialog';
import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { useAddPack } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';

/** The host of an https link, for "From perlanddev.github.io"; null for anything else. */
function hostOf(link: string): string | null {
  try {
    const url = new URL(link.trim());
    return url.protocol === 'https:' ? url.host : null;
  } catch {
    return null;
  }
}

function AddBar({ disabled, pending }: { disabled: boolean; pending: boolean }) {
  const { t } = useTranslation();
  return (
    <div className="sticky bottom-0 flex items-center justify-end gap-4 bg-ground py-4 shadow-[0_-1px_0_var(--color-line)]">
      <p className="text-sm text-ink-dim">{t('newInstance.packNote')}</p>
      <Button onClick={() => navigate({ name: 'home', id: null })}>{t('common.cancel')}</Button>
      <Button type="submit" variant="primary" size="lg" disabled={disabled || pending}>
        {pending ? t('newInstance.adding') : t('newInstance.addPack')}
      </Button>
    </div>
  );
}

/** Paste a pack link (or arrive from perland://). Nothing is added before Add pack. */
export function PackLinkForm({ initial }: { initial?: string }) {
  const { t } = useTranslation();
  const [url, setUrl] = useState(initial ?? '');
  const add = useAddPack({ inline: true });
  const host = initial ? hostOf(initial) : null;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!url.trim()) return;
    add.mutate({ kind: 'link', url: url.trim() }, { onSuccess: (instance) => navigate({ name: 'home', id: instance.id }) });
  };
  return (
    <form onSubmit={submit} className="grid max-w-2xl gap-6">
      <Field label={t('newInstance.linkLabel')}>
        <input
          className="field-input"
          type="url"
          inputMode="url"
          spellCheck={false}
          value={url}
          placeholder={t('newInstance.linkPlaceholder')}
          onChange={(e) => {
            add.reset();
            setUrl(e.target.value);
          }}
        />
      </Field>
      <p className="text-sm text-ink-dim">{host ? t('newInstance.linkFrom', { host }) : t('newInstance.linkHint')}</p>
      {add.isError && <ErrorCard error={add.error} />}
      <AddBar disabled={!url.trim()} pending={add.isPending} />
    </form>
  );
}

/** Pick a .mrpack file from disk. */
export function PackFileForm() {
  const { t } = useTranslation();
  const [path, setPath] = useState<string | null>(null);
  const [pickError, setPickError] = useState<unknown>(null);
  const add = useAddPack({ inline: true });
  const pick = async () => {
    setPickError(null);
    try {
      const chosen = await open({
        multiple: false,
        directory: false,
        title: t('newInstance.mrpack'),
        filters: [{ name: t('newInstance.fileFilter'), extensions: ['mrpack'] }],
      });
      if (typeof chosen === 'string') {
        add.reset();
        setPath(chosen);
      }
    } catch (error: unknown) {
      setPickError(error);
    }
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!path) return;
    add.mutate({ kind: 'file', path }, { onSuccess: (instance) => navigate({ name: 'home', id: instance.id }) });
  };
  return (
    <form onSubmit={submit} className="grid max-w-2xl gap-6">
      <div className="flex items-center gap-4">
        <Button onClick={() => void pick()}>{t('newInstance.filePick')}</Button>
        <p className="selectable truncate font-mono text-sm text-ink-dim">
          {path ? (path.split(/[/\\]/).pop() ?? path) : t('newInstance.fileNone')}
        </p>
      </div>
      {pickError !== null && <ErrorCard error={pickError} />}
      {add.isError && <ErrorCard error={add.error} />}
      <AddBar disabled={!path} pending={add.isPending} />
    </form>
  );
}
