import { useQueryClient } from '@tanstack/react-query';
import { getCurrentWebview, type DragDropEvent } from '@tauri-apps/api/webview';
import { useEffect, useEffectEvent, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { DroppedFile, DropResult, InstanceListing } from '../../ipc/schema';
import { qk, useInstances } from '../../state/queries';
import { lastInstance, navigate, useRoute, type Route } from '../../state/route';
import { pushError, pushNotice } from '../../state/toasts';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { Upload } from '../../ui/Icon';
import { Select } from '../../ui/Select';
import { useSingleFlight } from '../../ui/useSingleFlight';
import {
  ADDED_KINDS,
  addsAnything,
  contextInstance,
  countAdded,
  dropsAccepted,
  kindList,
  planDrop,
  skippedUpFront,
  tabOf,
  type AddedKind,
} from './dropPlan';

/** A drag over the window: its files (what each is, once the core has looked) and a rail instance held over. */
interface Hover {
  paths: string[];
  files: DroppedFile[] | null;
  over: string | null;
}

/** A drop with no instance on screen: the player picks one. */
interface Asking {
  paths: string[];
  files: DroppedFile[];
  choice: string;
}

const RAIL_MARK = 'data-drop-target';
const sameFiles = (a: string[], b: string[]) => a.length === b.length && a.every((p, i) => p === b[i]);

/** The rail cabinet under a drag, by the point Tauri gives (physical pixels). */
function railInstanceAt(position: { x: number; y: number }): HTMLElement | null {
  if (typeof document.elementFromPoint !== 'function') return null;
  const scale = window.devicePixelRatio || 1;
  const hit = document.elementFromPoint(position.x / scale, position.y / scale);
  return hit?.closest<HTMLElement>('[data-instance]') ?? null;
}

function markRail(element: HTMLElement | null) {
  for (const marked of document.querySelectorAll(`[${RAIL_MARK}]`)) if (marked !== element) marked.removeAttribute(RAIL_MARK);
  element?.setAttribute(RAIL_MARK, '');
}

/** A modal dialog is up: a drop under it would act behind the player's back. */
const dialogOpen = () => document.querySelector('dialog[open]') !== null;

/**
 * The whole window takes dropped files. Mods (.jar) and resource or shader packs (.zip) go into the instance on
 * screen (or the rail instance the files are held over, or one the player picks after the drop); a .mrpack dropped
 * on its own opens New instance with it. One listener for the window: no screen listens for drops itself.
 */
export function DropZone() {
  const { t, i18n } = useTranslation();
  const route = useRoute();
  const listing = useInstances().data;
  const queryClient = useQueryClient();
  const [hover, setHover] = useState<Hover | null>(null);
  const [asking, setAsking] = useState<Asking | null>(null);
  const addOnce = useSingleFlight();
  const nameOf = (id: string) => listing?.instances.find((i) => i.id === id)?.name ?? id;
  const what = (counts: Record<AddedKind, number>) => kindList(t, i18n.language, counts);

  const skipList = (skipped: DropResult['skipped']) =>
    t('drop.skipped', {
      list: skipped
        .map((s) => {
          const reason = s.reason === 'needs_loader' ? (s.kind === 'mod' ? 'needs_loader_mod' : 'needs_loader_shader') : s.reason;
          return t('drop.skipItem', { name: s.name, reason: t(`drop.reasons.${reason}`) });
        })
        .join(', '),
    });

  const report = (id: string, result: DropResult) => {
    const name = nameOf(id);
    const first = ADDED_KINDS.find((kind) => result.added.some((file) => file.kind === kind));
    const body = result.skipped.length > 0 ? skipList(result.skipped) : undefined;
    if (!first) {
      pushNotice({ title: t('drop.addedNothing', { name }), body, tone: 'info' });
      return;
    }
    pushNotice({
      title: t('drop.added', { count: result.added.length, what: what(countAdded(result.added)), name }),
      body,
      action: { label: t('drop.show'), run: () => navigate({ name: 'mods', id, tab: tabOf(first) }) },
    });
  };

  // Whatever the add did, the open lists (Mods & packs, Browse's installed marks) show the folder as it is now.
  const refresh = (id: string) => {
    for (const key of [qk.mods(id), qk.packContents(id), qk.contentFiles(id, 'resource_pack'), qk.contentFiles(id, 'shader'), qk.browseInstalled(id)])
      void queryClient.invalidateQueries({ queryKey: key });
  };

  const add = (id: string, paths: string[]) => {
    const started = addOnce((done) => {
      commands
        .dropAdd(id, paths)
        .then(
          (result) => {
            refresh(id);
            report(id, result);
          },
          (error: unknown) => {
            refresh(id);
            pushError(error);
          },
        )
        .finally(done);
    });
    if (!started) pushNotice({ title: t('mods.busy'), tone: 'info' });
  };

  const land = async (paths: string[], over: string | null, known: DroppedFile[] | null) => {
    let files = known;
    if (!files) {
      try {
        files = await commands.dropInspect(paths);
      } catch (error: unknown) {
        pushError(error);
        return;
      }
    }
    const plan = planDrop(files, null);
    if (plan.modpack) {
      navigate({ name: 'newInstance', file: plan.modpack.path });
      return;
    }
    const target = over ?? contextInstance(route, listing);
    if (target) {
      add(target, paths);
      return;
    }
    if (!addsAnything(plan.adds)) {
      pushNotice({ title: t('drop.nothingAdded'), body: skipList(skippedUpFront(files)), tone: 'info' });
      return;
    }
    const ids = listing?.instances.map((i) => i.id) ?? [];
    if (ids.length === 0) {
      pushNotice({ title: t('drop.noInstance'), body: t('drop.noInstanceNote'), tone: 'info' });
      return;
    }
    const last = lastInstance();
    setAsking({ paths, files, choice: last && ids.includes(last) ? last : (ids[0] as string) });
  };

  // The listener lives as long as the window; this reads the screen as it is when each event comes.
  const onDrag = useEffectEvent((event: DragDropEvent) => {
    if (event.type === 'leave') {
      markRail(null);
      setHover(null);
      return;
    }
    if (!dropsAccepted(route) || dialogOpen()) {
      markRail(null);
      setHover(null);
      return;
    }
    if (event.type === 'enter') {
      const paths = event.paths;
      setHover({ paths, files: null, over: null });
      commands.dropInspect(paths).then(
        (files) => setHover((h) => (h && sameFiles(h.paths, paths) ? { ...h, files } : h)),
        // The count waits for the drop, which asks again and reports a failure then.
        () => {},
      );
      return;
    }
    // A damaged instance's cabinet takes nothing.
    const found = railInstanceAt(event.position);
    const cabinet = found && listing?.instances.some((i) => i.id === found.dataset.instance) ? found : null;
    const over = cabinet?.dataset.instance ?? null;
    if (event.type === 'over') {
      markRail(cabinet);
      setHover((h) => (h && h.over !== over ? { ...h, over } : h));
      return;
    }
    markRail(null);
    setHover(null);
    if (event.paths.length === 0) return;
    const known = hover?.files && sameFiles(hover.paths, event.paths) ? hover.files : null;
    void land(event.paths, over ?? hover?.over ?? null, known);
  });

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => onDrag(event.payload))
      .then(
        (stop) => {
          if (alive) unlisten = stop;
          else stop();
        },
        (error: unknown) => {
          if (alive) pushError(error);
        },
      );
    return () => {
      alive = false;
      unlisten?.();
      markRail(null);
    };
  }, []);

  return (
    <>
      {hover && <DropOverlay hover={hover} route={route} listing={listing} what={what} />}
      {asking && (
        <ChooseInstance
          asking={asking}
          listing={listing}
          what={what}
          onChange={(choice) => setAsking({ ...asking, choice })}
          onCancel={() => setAsking(null)}
          onAdd={() => {
            setAsking(null);
            add(asking.choice, asking.paths);
          }}
        />
      )}
    </>
  );
}

function DropOverlay({
  hover,
  route,
  listing,
  what,
}: {
  hover: Hover;
  route: Route;
  listing: InstanceListing | undefined;
  what: (counts: Record<AddedKind, number>) => string;
}) {
  const { t } = useTranslation();
  const targetId = hover.over ?? contextInstance(route, listing);
  const target = listing?.instances.find((i) => i.id === targetId) ?? null;
  const hasInstances = (listing?.instances.length ?? 0) > 0;
  const plan = hover.files ? planDrop(hover.files, target ? target.game.loader !== null : null) : null;

  let title: string;
  const notes: string[] = [];
  if (!plan) title = t('drop.reading');
  else if (plan.modpack) {
    title = t('drop.toImport');
    notes.push(t('drop.importNote'));
  } else if (!hasInstances) {
    title = t('drop.noInstance');
    notes.push(t('drop.noInstanceNote'));
  } else {
    title = addsAnything(plan.adds) ? t('drop.toAdd', { what: what(plan.adds) }) : t('drop.nothing');
    if (target && plan.needsLoader > 0) notes.push(t('drop.needsLoader', { name: target.name }));
    if (plan.other > 0) notes.push(t('drop.other', { count: plan.other }));
    if (!addsAnything(plan.adds) && plan.needsLoader === 0) notes.push(t('drop.nothingNote'));
  }
  const showTarget = plan !== null && !plan.modpack && hasInstances;

  return (
    <div className="drop-veil" data-testid="drop-overlay">
      <div className="drop-frame">
        <div className="drop-panel" role="status" aria-live="polite">
          <Upload className="drop-glyph" aria-hidden />
          <p className="drop-title">{title}</p>
          {plan?.modpack && <p className="drop-file">{plan.modpack.name}</p>}
          {showTarget &&
            (target ? (
              <p className="drop-target">
                <span className="text-ink-dim">{t('drop.addsTo')}</span> <span className="drop-target-name">{target.name}</span>
              </p>
            ) : (
              <p className="drop-target text-ink-dim">{t('drop.chooseAfter')}</p>
            ))}
          {notes.map((note) => (
            <p key={note} className="drop-note">
              {note}
            </p>
          ))}
          {showTarget && (listing?.instances.length ?? 0) > 1 && <p className="drop-hint">{t('drop.retarget')}</p>}
        </div>
      </div>
    </div>
  );
}

function ChooseInstance({
  asking,
  listing,
  what,
  onChange,
  onCancel,
  onAdd,
}: {
  asking: Asking;
  listing: InstanceListing | undefined;
  what: (counts: Record<AddedKind, number>) => string;
  onChange: (choice: string) => void;
  onCancel: () => void;
  onAdd: () => void;
}) {
  const { t } = useTranslation();
  const instances = listing?.instances ?? [];
  const chosen = instances.find((i) => i.id === asking.choice);
  const plan = planDrop(asking.files, chosen ? chosen.game.loader !== null : null);
  const everything = planDrop(asking.files, null);
  return (
    <Dialog
      open
      onClose={onCancel}
      title={t('drop.chooseTitle')}
      description={t('drop.chooseBody', { what: what(everything.adds) })}
      actions={
        <>
          <Button onClick={onCancel}>{t('common.cancel')}</Button>
          <Button variant="primary" onClick={onAdd}>
            {t('drop.add')}
          </Button>
        </>
      }
    >
      <div className="grid gap-3">
        <Select label={t('drop.instance')} value={asking.choice} onChange={onChange} options={instances.map((i) => ({ value: i.id, label: i.name }))} />
        {chosen && plan.needsLoader > 0 && <p className="note-plate">{t('drop.needsLoader', { name: chosen.name })}</p>}
      </div>
    </Dialog>
  );
}
