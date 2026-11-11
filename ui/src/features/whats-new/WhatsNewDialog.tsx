import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { NoteGroup, ReleaseNote } from '../../ipc/schema';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';

const KINDS: ReleaseNote['kind'][] = ['new', 'improved', 'fixed'];
const count = (groups: NoteGroup[]) => groups.reduce((sum, group) => sum + group.notes.length, 0);

/** The launcher's release notes: `groups`, newest first, each version's notes under New, Improved and Fixed in the
 *  player's language. When `all` holds more than `groups`, All updates swaps the list for it. Nothing in it is lit:
 *  Got it is ghost and All updates is raised, like server news. */
export function WhatsNewDialog({ open, groups, all, onClose }: { open: boolean; groups: NoteGroup[]; all: NoteGroup[] | null; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const [everything, setEverything] = useState(false);
  // Each opening starts from what it was asked to show.
  const close = () => {
    setEverything(false);
    onClose();
  };
  const more = all !== null && count(all) > count(groups);
  const shown = everything && all !== null ? all : groups;
  const spanish = i18n.language.startsWith('es');
  const day = new Intl.DateTimeFormat(i18n.language, { month: 'short', day: 'numeric', timeZone: 'UTC' });
  return (
    <Dialog
      open={open}
      onClose={close}
      title={t('whatsNew.title')}
      actions={
        <>
          {more && !everything && <Button onClick={() => setEverything(true)}>{t('whatsNew.all')}</Button>}
          <Button variant="ghost" onClick={close}>
            {t('whatsNew.close')}
          </Button>
        </>
      }
    >
      {/* The list scrolls on its own, so the buttons stay in view; it takes focus first, at the top. */}
      <section className="whats-new-list" tabIndex={0} aria-label={t('whatsNew.title')}>
        {shown.length === 0 && <p>{t('whatsNew.empty')}</p>}
        {shown.map((group) => {
          const date = new Date(`${group.date}T00:00:00Z`);
          return (
            <article key={group.base ?? 'next'} className="whats-new-version">
              <h3 className="whats-new-version-title">
                {group.base === null || Number.isNaN(date.getTime()) ? (group.base ?? t('whatsNew.next')) : t('whatsNew.version', { version: group.base, date: day.format(date) })}
              </h3>
              {KINDS.filter((kind) => group.notes.some((note) => note.kind === kind)).map((kind) => (
                <div key={kind} className="whats-new-kind" data-kind={kind}>
                  <h4 className="whats-new-kind-title">{t(`whatsNew.kind.${kind}`)}</h4>
                  <ul className="whats-new-notes">
                    {group.notes
                      .filter((note) => note.kind === kind)
                      .map((note) => (
                        // Plain text: never parsed as HTML or Markdown.
                        <li key={note.id}>{spanish ? note.es : note.en}</li>
                      ))}
                  </ul>
                </div>
              ))}
            </article>
          );
        })}
      </section>
    </Dialog>
  );
}
