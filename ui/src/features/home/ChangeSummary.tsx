import { useId, useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { PackUpdate } from '../../ipc/schema';
import { changeLine } from './labels';

const KINDS = ['added', 'updated', 'removed'] as const;

/** The change line, the names behind it on request, and the edits the update keeps. */
export function ChangeSummary({ update }: { update: PackUpdate }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const listId = useId();
  const counts = { added: update.added.length, updated: update.updated.length, removed: update.removed.length };
  const hasNames = counts.added + counts.updated + counts.removed > 0;
  return (
    <div className="grid gap-2">
      <p className="tabular text-ink">{changeLine(t, counts)}</p>
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
