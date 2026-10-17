import { open } from '@tauri-apps/plugin-dialog';
import type { TFunction } from 'i18next';
import { useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { ImportPreview } from '../../ipc/schema';
import {
  beginImport,
  clearImportFailure,
  markImportCancelling,
  useImportFailure,
  useImportProgress,
  useRunningImport,
  watchImport,
  type RunningImport,
} from '../../state/importProgress';
import { useImportInspect, useImportInstance } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { ErrorCard } from '../../ui/ErrorCard';
import { Field } from '../../ui/Field';
import { formatBytes, formatGb } from '../../ui/format';
import { Meter } from '../../ui/Meter';
import { formatPlaytime } from '../../ui/playtime';
import { useVersionChoice } from './useVersionChoice';
import { VersionFields } from './VersionFields';

const folderParts = (path: string) => path.split(/[/\\]/).filter(Boolean);
const folderName = (path: string) => folderParts(path).pop() ?? path;

/** The longest name an instance can have, in characters. */
const MAX_NAME = 64;
/** A prefilled name is cut to what an instance name can be (counted in characters, as Rust does). */
const capName = (name: string) => [...name].slice(0, MAX_NAME).join('');

/** The name to fall back on: the folder's own, or its parent's when it is a bare `.minecraft`. */
function fallbackNameFor(path: string) {
  const parts = folderParts(path);
  const last = parts.pop();
  if (last === undefined) return capName(path);
  if (/^\.?minecraft$/i.test(last)) return capName(parts.pop() ?? last);
  return capName(last);
}

/** "Choose folder…", then the picked folder's name (mono) with a sentence under it. While the folder is
 *  being read, a short status line stands where its name will be. */
function PickRow({ path, note, busy, reading = false, onPick }: {
  path: string | null;
  note: string;
  busy: boolean;
  reading?: boolean;
  onPick: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="grid gap-2">
      <div className="flex min-w-0 items-center gap-4">
        <Button onClick={onPick} disabled={busy} className="flex-none">
          {t('import.choose')}
        </Button>
        {reading && (
          <p role="status" className="truncate text-sm text-ink-dim">
            {t('import.reading')}
          </p>
        )}
        {path && !reading && <p className="selectable truncate font-mono text-sm text-ink">{folderName(path)}</p>}
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

/** The copy's line and meter, from the import store. */
function CopyMeter({ sizeBytes }: { sizeBytes: number }) {
  const { t, i18n } = useTranslation();
  const progress = useImportProgress();
  const done = progress?.done ?? 0;
  const total = progress?.total ?? sizeBytes;
  const copying = t('import.copying', { done: formatBytes(done, i18n.language), total: formatBytes(total, i18n.language) });
  return (
    <>
      <p className="tabular truncate text-sm text-ink-dim">{copying}</p>
      <Meter value={total > 0 ? done / total : 0} label={copying} />
    </>
  );
}

/** Ask the running import to stop, once. */
function cancelImport() {
  if (markImportCancelling()) void commands.importCancel();
}

/** The New instance mode "From another launcher": pick a folder, check what was found, copy it. The copy
 *  keeps going when the player leaves; coming back while it runs shows it, with its meter and Cancel. */
export function ImportForm() {
  const { t } = useTranslation();
  const inspect = useImportInspect();
  const running = useRunningImport();
  const failure = useImportFailure();
  const [pickError, setPickError] = useState<unknown>(null);
  useEffect(() => watchImport(), []);
  const pick = async () => {
    setPickError(null);
    try {
      const chosen = await open({ directory: true, multiple: false, title: t('import.pickTitle') });
      if (typeof chosen === 'string') {
        clearImportFailure();
        inspect.mutate(chosen);
      }
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
  if (running) return <ImportRunning running={running} />;
  return (
    <form onSubmit={(e) => e.preventDefault()} className="grid max-w-2xl gap-8">
      <PickRow path={null} note={t('import.hint')} busy={inspect.isPending} reading={inspect.isPending} onPick={() => void pick()} />
      {errors}
      {failure !== null && <ErrorCard error={failure} />}
      <ActionRow meter={null} onCancel={() => navigate({ name: 'home', id: null })} disabled pending={false} />
    </form>
  );
}

/** An import the player started, left, and came back to: the folder, its name, and the copy. */
function ImportRunning({ running }: { running: RunningImport }) {
  const { t, i18n } = useTranslation();
  return (
    <form onSubmit={(e) => e.preventDefault()} className="grid max-w-2xl gap-8">
      <PickRow
        path={running.path}
        note={t('import.size', { size: formatBytes(running.sizeBytes, i18n.language) })}
        busy
        onPick={() => {}}
      />
      <fieldset disabled className="grid min-w-0 gap-8">
        <Field label={t('newInstance.name')}>
          <input className="field-input" value={running.name} readOnly />
        </Field>
      </fieldset>
      <ActionRow
        meter={<CopyMeter sizeBytes={running.sizeBytes} />}
        onCancel={cancelImport}
        cancelPending={running.cancelling}
        disabled
        pending
      />
    </form>
  );
}

/** What the instance keeps of its own settings in the other launcher (#57, #58), one sentence each. */
function keptNotes(preview: ImportPreview, t: TFunction, lang: string): string[] {
  const notes: string[] = [];
  const { memoryMb, memoryWantedMb, javaArgs, playtimeSeconds } = preview;
  const gb = memoryMb !== null ? formatGb(memoryMb, lang) : '';
  if (memoryMb !== null && memoryWantedMb !== null) {
    const wanted = formatGb(memoryWantedMb, lang);
    notes.push(t(memoryWantedMb > memoryMb ? 'import.memoryTooHigh' : 'import.memoryTooLow', { wanted, gb }));
    if (javaArgs !== null) notes.push(t('import.keepsArgs'));
  } else if (memoryMb !== null) {
    notes.push(t(javaArgs !== null ? 'import.keepsMemoryAndArgs' : 'import.keepsMemory', { gb }));
  } else if (javaArgs !== null) {
    notes.push(t('import.keepsArgs'));
  }
  const played = playtimeSeconds !== null ? formatPlaytime(playtimeSeconds, t) : '';
  if (played) notes.push(t('import.keepsPlaytime', { time: played }));
  return notes;
}

/** The form prefilled from what the folder holds. Parts that weren't detected get the defaults. */
function ImportDetails({ preview, onPick, errors }: { preview: ImportPreview; onPick: () => void; errors: ReactNode }) {
  const { t, i18n } = useTranslation();
  const choice = useVersionChoice({ minecraft: preview.minecraft, loader: preview.loader });
  const [name, setName] = useState(() => capName(preview.name ?? ''));
  const run = useImportInstance();
  const running = useRunningImport();
  const failure = useImportFailure();
  const busy = running !== null;
  const fallbackName = fallbackNameFor(preview.path);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const { minecraft, loader, ready } = choice.resolved;
    if (!ready || !minecraft || busy) return;
    const request = { path: preview.path, name: name.trim() || fallbackName, minecraft, loader };
    // The store, not this form, holds the running import: the player may leave and come back.
    beginImport({ path: request.path, name: request.name, sizeBytes: preview.sizeBytes });
    run.mutate(request);
  };
  const cancel = () => {
    if (busy) cancelImport();
    else navigate({ name: 'home', id: null });
  };

  return (
    <form onSubmit={submit} className="grid max-w-2xl gap-8">
      <PickRow
        path={preview.path}
        note={[t('import.size', { size: formatBytes(preview.sizeBytes, i18n.language) }), ...keptNotes(preview, t, i18n.language)].join(' ')}
        busy={busy}
        onPick={onPick}
      />
      {errors}
      <fieldset disabled={busy} className="grid min-w-0 gap-8">
        <Field label={t('newInstance.name')}>
          <input className="field-input" value={name} placeholder={fallbackName} maxLength={MAX_NAME} onChange={(e) => setName(e.target.value)} />
        </Field>
        <VersionFields choice={choice} />
      </fieldset>
      {failure !== null && <ErrorCard error={failure} />}
      <ActionRow
        meter={busy && <CopyMeter sizeBytes={preview.sizeBytes} />}
        onCancel={cancel}
        cancelPending={running?.cancelling}
        disabled={!choice.resolved.ready}
        pending={busy}
      />
    </form>
  );
}
