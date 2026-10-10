import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { LocalMod } from '../../ipc/schema';
import { useInstances, useModMutations, useMods } from '../../state/queries';
import { navigate } from '../../state/route';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { formatBytes } from '../../ui/format';
import { Folder, InfoBox, Package, Plus, Trash } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { Toggle } from '../../ui/Toggle';

const displayName = (fileName: string) => fileName.replace(/\.disabled$/, '').replace(/\.jar$/i, '');

/** Lets one run of an action be in flight at a time; the ref closes the gap before React re-renders. */
function useSingleFlight() {
  const busy = useRef(false);
  return useCallback((start: (done: () => void) => void) => {
    if (busy.current) return;
    busy.current = true;
    start(() => {
      busy.current = false;
    });
  }, []);
}

export function Mods({ id }: { id: string }) {
  const { t, i18n } = useTranslation();
  const listing = useInstances();
  const instance = listing.data?.instances.find((i) => i.id === id);
  const mods = useMods(id);
  const { toggle, add, remove } = useModMutations(id, { inline: true });
  const [removing, setRemoving] = useState<LocalMod | null>(null);
  const [dragging, setDragging] = useState(false);
  const [picking, setPicking] = useState(false);
  const [opening, setOpening] = useState(false);
  // Failures from the file dialog, drag and drop, opening the folder, toggling and adding.
  const [failure, setFailure] = useState<{ error: unknown } | null>(null);
  const fail = useCallback((error: unknown) => setFailure({ error }), []);
  const addOnce = useSingleFlight();
  const toggleOnce = useSingleFlight();
  const removeOnce = useSingleFlight();
  const addMods = add.mutate;

  const submit = useCallback(
    (paths: string[]) => {
      if (paths.length === 0) return;
      addOnce((done) => {
        setFailure(null);
        addMods(paths, { onError: fail, onSettled: done });
      });
    },
    [addOnce, addMods, fail],
  );

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    const listen = async () => {
      try {
        const stop = await getCurrentWebview().onDragDropEvent((event) => {
          const { type } = event.payload;
          if (type === 'enter' || type === 'over') setDragging(true);
          else if (type === 'leave') setDragging(false);
          else if (type === 'drop') {
            setDragging(false);
            submit(event.payload.paths);
          }
        });
        if (alive) unlisten = stop;
        else stop();
      } catch (error: unknown) {
        if (alive) fail(error);
      }
    };
    void listen();
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [submit, fail]);

  const pick = async () => {
    setPicking(true);
    setFailure(null);
    try {
      const chosen = await open({ multiple: true, title: t('mods.pickTitle'), filters: [{ name: t('mods.jarFiles'), extensions: ['jar'] }] });
      if (Array.isArray(chosen)) submit(chosen);
    } catch (error: unknown) {
      fail(error);
    } finally {
      setPicking(false);
    }
  };

  const openFolder = () => {
    setOpening(true);
    setFailure(null);
    void commands
      .openFolder({ kind: 'mods', id })
      .catch(fail)
      .finally(() => setOpening(false));
  };

  const closeRemove = () => {
    remove.reset();
    setRemoving(null);
  };
  const confirmRemove = (mod: LocalMod) =>
    removeOnce((done) => remove.mutate(mod.fileName, { onSuccess: closeRemove, onSettled: done }));

  const list = mods.data ?? [];
  return (
    <ScreenFrame
      title={t('mods.title')}
      subtitle={instance ? `${instance.name} · ${t('mods.count', { count: list.length })}` : undefined}
      onBack={() => navigate({ name: 'home', id })}
      actions={
        <>
          <Button icon={<Folder />} disabled={opening} onClick={openFolder}>
            {t('common.openFolder')}
          </Button>
          <Button variant="primary" icon={<Plus />} disabled={picking || add.isPending} onClick={() => void pick()}>
            {t('mods.add')}
          </Button>
        </>
      }
    >
      <div className={`grid max-w-3xl gap-4 ${dragging ? 'outline-2 outline-offset-8 outline-amber outline-dashed' : ''}`}>
        {instance && !instance.game.loader && (
          <p className="panel flex items-start gap-3 p-4 text-ink-dim">
            <InfoBox className="mt-0.5 h-5 w-5 flex-none text-blue-ink" aria-hidden />
            {t('mods.vanillaNote')}
          </p>
        )}
        {dragging && <p className="font-semibold text-amber">{t('mods.dropHere')}</p>}
        {failure && <ErrorCard error={failure.error} />}
        {mods.isPending && <p className="text-ink-dim">{t('common.loading')}</p>}
        {mods.isError && (
          <ErrorCard
            error={mods.error}
            actions={
              <Button disabled={mods.isFetching} onClick={() => void mods.refetch()}>
                {t('common.retry')}
              </Button>
            }
          />
        )}
        {mods.isSuccess && list.length === 0 && <p className="text-ink-dim">{t('mods.empty')}</p>}
        <ul className="grid gap-2">
          {list.map((mod) => {
            const name = displayName(mod.fileName);
            return (
              // Keyed by name, not file: toggling renames the file, and the row must keep keyboard focus.
              <li key={name} className="panel flex items-center gap-4 px-4 py-3">
                <Package className="h-6 w-6 flex-none text-ink-dim" aria-hidden />
                <div className="min-w-0 flex-1">
                  <p className={`truncate font-semibold ${mod.enabled ? '' : 'text-ink-dim'}`}>{name}</p>
                  <p className="tabular text-sm text-ink-dim">{formatBytes(mod.size, i18n.language)}</p>
                </div>
                <Toggle
                  checked={mod.enabled}
                  disabled={toggle.isPending}
                  label={t('mods.toggle', { name })}
                  onChange={(on) =>
                    toggleOnce((done) => {
                      setFailure(null);
                      toggle.mutate({ file: mod.fileName, enabled: on }, { onError: fail, onSettled: done });
                    })
                  }
                />
                <Button variant="ghost" size="icon" aria-label={t('mods.remove', { name })} icon={<Trash />} onClick={() => setRemoving(mod)} />
              </li>
            );
          })}
        </ul>
      </div>
      {removing && (
        <Dialog
          open
          onClose={closeRemove}
          title={t('mods.removeTitle', { name: displayName(removing.fileName) })}
          actions={
            <>
              <Button onClick={closeRemove}>{t('common.cancel')}</Button>
              <Button variant="danger" disabled={remove.isPending} onClick={() => confirmRemove(removing)}>
                {t('mods.removeConfirm')}
              </Button>
            </>
          }
        >
          {t('mods.removeBody')}
          {remove.isError && (
            <div className="mt-4">
              <ErrorCard error={remove.error} />
            </div>
          )}
        </Dialog>
      )}
    </ScreenFrame>
  );
}
