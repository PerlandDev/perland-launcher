import { useEffect, useEffectEvent, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import type { Instance } from '../../ipc/schema';
import { useInstances, useSettings } from '../../state/queries';
import { instanceOf, navigate, useRoute } from '../../state/route';
import { useActiveIds } from '../../state/runStore';
import { ArrowDown, ArrowUp, ChevronRight, Search } from '../../ui/Icon';
import { useInstanceActions } from '../home/InstanceMenu';
import { pickId } from '../home/pick';
import { rank } from './match';
import { buildEntries, suggested, type Entry, type InstanceAction } from './registry';
import { land, reveal } from './reveal';
import { isShortcut, onOpenQuickJump } from './store';
import './quickJump.css';

/** Enough to choose from without scrolling a wall: a query that matches more is a query to type further. */
const MAX_RESULTS = 40;

/** An instance action chosen in Go to: the same list the ⋯ menu shows runs it, and holds the dialog it opens. */
function RunAction({ instance, action }: { instance: Instance; action: InstanceAction }) {
  const actions = useInstanceActions(instance);
  const start = useEffectEvent(() => {
    const item = actions.items.find((i) => i.id === action);
    if (item && !item.disabled) item.onSelect();
  });
  useEffect(() => start(), []);
  return actions.dialogs;
}

/** The results in groups, each where its best result ranks, the results inside it in their own rank. */
function grouped(entries: Entry[]): { id: string; label: string; entries: Entry[] }[] {
  const groups = new Map<string, { id: string; label: string; entries: Entry[] }>();
  for (const entry of entries) {
    const group = groups.get(entry.group.id) ?? { ...entry.group, entries: [] };
    group.entries.push(entry);
    groups.set(entry.group.id, group);
  }
  return [...groups.values()];
}

/** Go to (#quick-jump): type to reach any place, instance screen, instance action or setting. Ctrl+K (⌘K on macOS)
 *  or the title bar's search key opens it, never over another dialog. A combobox over a grouped listbox: the arrow
 *  keys move, Enter opens, Escape closes and gives the keyboard back. */
export function QuickJump() {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [at, setAt] = useState(0);
  const [run, setRun] = useState<{ instance: string; action: InstanceAction; nonce: number } | null>(null);
  const ref = useRef<HTMLDialogElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const opener = useRef<HTMLElement | null>(null);
  const listId = useId();
  const route = useRoute();
  const listing = useInstances().data;
  const settings = useSettings().data;
  const running = useActiveIds();

  const instances = listing?.instances ?? [];
  const entries = buildEntries({
    t,
    instances,
    current: pickId(listing, instanceOf(route)),
    running,
    ownTheme: settings?.theme === 'custom',
  });
  const results = query.trim() === '' ? suggested(entries) : rank(entries, query).slice(0, MAX_RESULTS);
  const groups = grouped(results);
  // The rows in the order they show, which is the order the arrow keys walk.
  const rows = groups.flatMap((group) => group.entries);
  const active = rows.length > 0 ? Math.min(at, rows.length - 1) : -1;
  const optionId = (entry: Entry) => `${listId}-${entry.id}`;
  const activeId = active >= 0 ? optionId(rows[active]!) : undefined;

  const show = useEffectEvent(() => {
    if (open || document.querySelector('dialog[open]')) return;
    opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setQuery('');
    setAt(0);
    setOpen(true);
  });

  const close = (restore: boolean) => {
    ref.current?.close();
    setOpen(false);
    const back = opener.current;
    opener.current = null;
    if (restore && back?.isConnected) back.focus();
  };
  const closeOnShortcut = useEffectEvent(() => close(true));

  useEffect(() => onOpenQuickJump(() => show()), []);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!isShortcut(event) || event.repeat) return;
      if (ref.current?.open) {
        event.preventDefault();
        closeOnShortcut();
        return;
      }
      // A dialog over the screen owns the keyboard; Go to would strand its task.
      if (document.querySelector('dialog[open]')) return;
      event.preventDefault();
      show();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) {
      dialog.showModal();
      inputRef.current?.focus();
    }
    if (!open && dialog.open) dialog.close();
  }, [open]);

  // The row the keyboard is on stays in view as it walks a long list.
  useEffect(() => {
    if (activeId) document.getElementById(activeId)?.scrollIntoView?.({ block: 'nearest' });
  }, [activeId]);

  const choose = (entry: Entry) => {
    const target = entry.target;
    if (target.kind === 'action') {
      // The keyboard goes back first, so the action's own dialog returns it there when it closes.
      close(true);
      // A new run each time, even of the same action: its dialog starts afresh.
      setRun((last) => ({ instance: target.instance, action: target.action, nonce: (last?.nonce ?? 0) + 1 }));
      return;
    }
    close(false);
    navigate(target.route);
    void (target.kind === 'field' ? reveal(target.find) : land());
  };

  const onKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (rows.length === 0) return;
      const step = event.key === 'ArrowDown' ? 1 : -1;
      setAt((active + step + rows.length) % rows.length);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      if (active >= 0) choose(rows[active]!);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      close(true);
    }
  };

  const ranInstance = run && instances.find((i) => i.id === run.instance);
  return (
    <>
      <dialog
        ref={ref}
        className="dialog qj"
        aria-label={t('quickJump.title')}
        onCancel={(event) => {
          event.preventDefault();
          close(true);
        }}
        onClose={() => {
          if (open) setOpen(false);
        }}
        // A press on the backdrop lands on the dialog itself, outside the panel: it closes, as Escape does.
        onClick={(event) => {
          if (event.target === event.currentTarget) close(true);
        }}
      >
        {open && (
          <div className="qj-panel">
            <div className="qj-field">
              <Search aria-hidden />
              <input
                ref={inputRef}
                type="text"
                role="combobox"
                aria-expanded="true"
                aria-autocomplete="list"
                aria-controls={listId}
                aria-activedescendant={activeId}
                aria-label={t('quickJump.title')}
                placeholder={t('quickJump.placeholder')}
                value={query}
                spellCheck={false}
                autoComplete="off"
                onChange={(event) => {
                  setQuery(event.target.value);
                  setAt(0);
                }}
                onKeyDown={onKeyDown}
              />
            </div>
            <div id={listId} role="listbox" aria-label={t('quickJump.results')} className="qj-list">
              {groups.map((group) => (
                <div key={group.id} role="group" aria-labelledby={`${listId}-${group.id}`} className="qj-group">
                  <div id={`${listId}-${group.id}`} role="presentation" className="qj-group-label">
                    {group.label}
                  </div>
                  {group.entries.map((entry) => {
                    const Icon = entry.icon;
                    const selected = activeId === optionId(entry);
                    return (
                      <div
                        key={entry.id}
                        id={optionId(entry)}
                        role="option"
                        aria-selected={selected}
                        className="qj-option"
                        // The keyboard stays in the search; a press only chooses.
                        onMouseDown={(event) => event.preventDefault()}
                        onMouseMove={() => {
                          if (!selected) setAt(rows.indexOf(entry));
                        }}
                        onClick={() => choose(entry)}
                      >
                        <Icon aria-hidden />
                        <span className="qj-label">{entry.label}</span>
                        {selected && <ChevronRight className="qj-go" aria-hidden />}
                      </div>
                    );
                  })}
                </div>
              ))}
            </div>
            {rows.length === 0 && (
              <p role="status" className="qj-empty">
                {t('quickJump.none', { query: query.trim() })}
              </p>
            )}
            <p role="status" className="sr-only">
              {query.trim() !== '' && rows.length > 0 ? t('quickJump.count', { count: rows.length }) : ''}
            </p>
            <div className="qj-hint">
              <span className="qj-hint-item">
                <kbd>
                  <ArrowUp aria-hidden />
                  <span className="sr-only">{t('quickJump.keyUp')}</span>
                </kbd>
                <kbd>
                  <ArrowDown aria-hidden />
                  <span className="sr-only">{t('quickJump.keyDown')}</span>
                </kbd>
                {t('quickJump.hintMove')}
              </span>
              <span className="qj-hint-item">
                <kbd>{t('quickJump.keyEnter')}</kbd>
                {t('quickJump.hintOpen')}
              </span>
              <span className="qj-hint-item">
                <kbd>{t('quickJump.keyEsc')}</kbd>
                {t('quickJump.hintClose')}
              </span>
            </div>
          </div>
        )}
      </dialog>
      {ranInstance && <RunAction key={run.nonce} instance={ranInstance} action={run.action} />}
    </>
  );
}
