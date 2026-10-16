import { open } from '@tauri-apps/plugin-dialog';
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import { isLauncherError } from '../../ipc/errors';
import type { ImportPreview } from '../../ipc/schema';
import { clearImportProgress, useImportProgress } from '../../state/importProgress';
import { useImportInspect, useImportInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { formatBytes } from '../../ui/format';
import { Meter } from '../../ui/Meter';
import { useVersionChoice } from './useVersionChoice';
import { VersionFields } from './VersionFields';

const folderParts = (path: string) => path.split(/[/\\]/).filter(Boolean);
const folderName = (path: string) => folderParts(path).pop() ?? path;

/** The name to fall back on: the folder's own, or its parent's when it is a bare `.minecraft`. */
function fallbackNameFor(path: string) {
  const parts = folderParts(path);
  const last = parts.pop();
  if (last === undefined) return path;
  if (/^\.?minecraft$/i.test(last)) return parts.pop() ?? last;
  return last;
}

/** The player's cancel isn't a failure: the form just comes back. */
const isCancel = (error: unknown) => isLauncherError(error) && error.code === 'import_cancelled';

/** "Choose folder…", then the picked folder's name (mono) with a sentence under it. */
function PickRow({ path, note, busy, onPick }: { path: string | null; note: string; busy: boolean; onPick: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="grid gap-2">
      <div className="flex min-w-0 items-center gap-4">
        <Button onClick={onPick} disabled={busy} className="flex-none">
          {t('import.choose')}
        </Button>
        {path && <p className="selectable truncate font-mono text-sm text-ink">{folderName(path)}</p>}
      </div>
      <p className="text-ink-dim">{note}</p>
    </div>
  );
}

/** The sticky action row: Cancel and the one lit Import, with the copy's meter while it runs. */
function ActionRow({ meter, onCancel, cancelPending, disabled, pending }: {
  meter: ReactNode;
  onCancel: () => void;
  cancelPending?: boolean;
  disabled: boolean;
  pending: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div className="sticky bottom-0 flex items-end justify-end gap-3 bg-ground py-4 shadow-[0_-1px_0_var(--color-line)]">
      {meter && <div className="grid min-w-0 flex-1 gap-2 self-center">{meter}</div>}
      <Button onClick={onCancel} aria-disabled={cancelPending || undefined}>
        {t('common.cancel')}
      </Button>
      <Button type="submit" variant="primary" size="lg" disabled={disabled} aria-disabled={pending || undefined}>
        {pending ? t('import.importing') : t('import.submit')}
      </Button>
    </div>
  );
}

/** The New instance mode "From another launcher": pick a folder, check what was found, copy it. */
export function ImportForm() {
  const { t } = useTranslation();
  const inspect = useImportInspect();
  const [pickError, setPickError] = useState<unknown>(null);
  const pick = async () => {
    setPickError(null);
    try {
      const chosen = await open({ directory: true, multiple: false, title: t('import.pickTitle') });
      if (typeof chosen === 'string') inspect.mutate(chosen);
    } catch (error: unknown) {
      setPickError(error);
    }
  };
  const errors = (
    <>
      {pickError !== null && <ErrorCard error={pickError} />}
      {inspect.isError && <ErrorCard error={inspect.error} />}
    </>
  );

  if (inspect.data) {
    return <ImportDetails key={inspect.data.path} preview={inspect.data} onPick={() => void pick()} errors={errors} />;
  }
  return (
    <form onSubmit={(e) => e.preventDefault()} className="grid max-w-2xl gap-8">
      <PickRow path={null} note={t('import.hint')} busy={inspect.isPending} onPick={() => void pick()} />
      {errors}
      <ActionRow meter={null} onCancel={() => navigate({ name: 'home', id: null })} disabled pending={false} />
    </form>
  );
}

/** The form prefilled from what the folder holds. Parts that weren't detected get the defaults. */
function ImportDetails({ preview, onPick, errors }: { preview: ImportPreview; onPick: () => void; errors: ReactNode }) {
  const { t, i18n } = useTranslation();
  const choice = useVersionChoice({ minecraft: preview.minecraft, loader: preview.loader });
  const [name, setName] = useState(preview.name ?? '');
  const [cancelling, setCancelling] = useState(false);
  const run = useImportInstance({ inline: true });
  const progress = useImportProgress();
  const busy = run.isPending;
  const fallbackName = fallbackNameFor(preview.path);
  // Leaving the form (Back, another mode) mid-copy cancels it: nobody would see it finish.
  const busyRef = useRef(false);
  useEffect(() => {
    busyRef.current = busy;
  }, [busy]);
  useEffect(
    () => () => {
      if (busyRef.current) void commands.importCancel();
    },
    [],
  );

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const { minecraft, loader, ready } = choice.resolved;
    if (!ready || !minecraft || busy) return;
    clearImportProgress();
    setCancelling(false);
    run.mutate(
      { path: preview.path, name: name.trim() || fallbackName, minecraft, loader },
      {
        onSuccess: (instance) => navigate({ name: 'home', id: instance.id }),
        onSettled: () => clearImportProgress(),
      },
    );
  };
  const cancel = () => {
    if (!busy) {
      navigate({ name: 'home', id: null });
      return;
    }
    if (cancelling) return;
    setCancelling(true);
    void commands.importCancel();
  };

  const done = progress?.done ?? 0;
  const total = progress?.total ?? preview.sizeBytes;
  const copying = t('import.copying', { done: formatBytes(done, i18n.language), total: formatBytes(total, i18n.language) });
  const meter = busy && (
    <>
      <p className="tabular truncate text-sm text-ink-dim">{copying}</p>
      <Meter value={total > 0 ? done / total : 0} label={copying} />
    </>
  );

  return (
    <form onSubmit={submit} className="grid max-w-2xl gap-8">
      <PickRow
        path={preview.path}
        note={t('import.size', { size: formatBytes(preview.sizeBytes, i18n.language) })}
        busy={busy}
        onPick={onPick}
      />
      {errors}
      <fieldset disabled={busy} className="grid min-w-0 gap-8">
        <Field label={t('newInstance.name')}>
          <input className="field-input" value={name} placeholder={fallbackName} maxLength={64} onChange={(e) => setName(e.target.value)} />
        </Field>
        <VersionFields choice={choice} />
      </fieldset>
      {run.isError && !isCancel(run.error) && <ErrorCard error={run.error} />}
      <ActionRow meter={meter} onCancel={cancel} cancelPending={busy && cancelling} disabled={!choice.resolved.ready} pending={busy} />
    </form>
  );
}
