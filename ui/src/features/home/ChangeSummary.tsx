import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { TFunction } from 'i18next';
import type { PackUpdate, UpdateFile } from '../../ipc/schema';
import { InfoBox, Upload } from '../../ui/Icon';
import { changeLine } from './labels';

const KINDS = ['added', 'updated', 'removed'] as const;

/** Where one file comes from, in the player's words. `server` names the server's owner and pack file. */
function originLabel(t: TFunction, file: UpdateFile, server: boolean): string {
  switch (file.origin) {
    case 'modrinth':
      return t('update.originModrinth');
    case 'curseForge':
      return t('update.originCurseForge');
    case 'uploaded':
      return t('update.originUploaded');
    case 'web':
      return file.host ? t('update.originWeb', { host: file.host }) : t('update.originWebUnknown');
    case 'pack':
      return server ? t('update.originPackServer') : t('update.originPack');
  }
}

/** The change line, the files from outside Modrinth and CurseForge (always in view, each with where it comes from), the
 *  names behind the line on request, and the edits the update keeps. With `held`, a line says why an update that installs on
 *  its own is waiting. Seam: once the Engine's mod-safety scan is live, each called-out row gains its verdict line and the
 *  summary a "Perland removed N files" line; nothing reads or invents them today. */
export function ChangeSummary({ update, server = false }: { update: PackUpdate; server?: boolean }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const listId = useId();
  const lookId = useId();
  const counts = { added: update.added.length, updated: update.updated.length, removed: update.removed.length };
  const hasNames = counts.added + counts.updated + counts.removed > 0;
  const look = update.files.filter((f) => f.needsLook);
  const catalogue = update.files.filter((f) => !f.needsLook && (f.origin === 'modrinth' || f.origin === 'curseForge')).length;
  return (
    <div className="grid gap-3">
      {update.held && (
        <p className="flex items-start gap-2 text-blue-ink">
          <InfoBox className="mt-0.5 h-5 w-5 flex-none" aria-hidden />
          <span>{t('update.held')}</span>
        </p>
      )}
      <div className="grid gap-1">
        <p className="tabular text-ink">{changeLine(t, counts)}</p>
        {catalogue > 0 && <p className="tabular text-sm">{t('update.catalogue', { count: catalogue })}</p>}
      </div>
      {look.length > 0 && (
        <section className="grid gap-2" aria-labelledby={lookId}>
          <h3 id={lookId} className="text-sm font-semibold text-ink">
            {t('update.lookHeading', { count: look.length })}
          </h3>
          <ul className="well grid gap-2 p-3 text-sm">
            {look.map((file) => (
              <li key={`${file.change}:${file.name}`} className="grid gap-0.5">
                <span className="flex items-center gap-1.5 font-semibold text-ink">
                  {file.origin === 'uploaded' && <Upload className="h-4 w-4 flex-none" aria-hidden />}
                  <span className="min-w-0 break-words">{file.name}</span>
                </span>
                <span>
                  {originLabel(t, file, server)} · {file.change === 'added' ? t('update.changeAdded') : t('update.changeUpdated')}
                </span>
              </li>
            ))}
          </ul>
          <p className="text-sm">{t('update.lookNote')}</p>
        </section>
      )}
      {hasNames && (
        <button
          type="button"
          className="justify-self-start text-sm text-blue-ink underline underline-offset-4"
          aria-expanded={open}
          aria-controls={listId}
          onClick={() => setOpen((v) => !v)}
        >
          {open ? t('update.hideNames') : t('update.showNames')}
        </button>
      )}
      {open && (
        <dl id={listId} className="well grid gap-2 p-3 text-sm">
          {KINDS.filter((kind) => update[kind].length > 0).map((kind) => (
            <div key={kind}>
              <dt className="font-semibold text-ink">{t(`update.${kind}Heading`)}</dt>
              <dd>{update[kind].join(', ')}</dd>
            </div>
          ))}
        </dl>
      )}
      {update.kept.length > 0 && <p>{t('update.kept', { count: update.kept.length })}</p>}
    </div>
  );
}
