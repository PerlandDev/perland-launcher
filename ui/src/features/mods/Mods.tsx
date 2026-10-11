import { useQueryClient } from '@tanstack/react-query';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { LocalMod } from '../../ipc/schema';
import { useInstances, useModMutations, useMods, usePackContents, usePackOptional } from '../../state/queries';
import { navigate } from '../../state/route';
import { useRun } from '../../state/runStore';
import { isPack } from '../home/labels';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { formatBytes } from '../../ui/format';
import { Folder, InfoBox, Package, Plus, Trash } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { Toggle } from '../../ui/Toggle';
import { useSingleFlight } from '../../ui/useSingleFlight';

const displayName = (fileName: string) => fileName.replace(/\.disabled$/, '').replace(/\.jar$/i, '');
const isJar = (path: string) => /\.jar$/i.test(path);
const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;

type Notice = { kind: 'busy' } | { kind: 'skipped'; names: string[] };

export function Mods({ id }: { id: string }) {
  const { t, i18n } = useTranslation();
  const listing = useInstances();
  const instance = listing.data?.instances.find((i) => i.id === id);
  const mods = useMods(id);
  const pack = instance ? isPack(instance) : false;
  const contents = usePackContents(id, pack);
  const optional = usePackOptional(id, { inline: true });
  // A pack instance lists its own mods apart; the jars below are only the player's.
  const source = pack ? contents : mods;
  // The game holds its jars open, so toggling or removing waits for it to stop. Adding is fine.
  const running = useRun(id).state !== 'idle';
  const queryClient = useQueryClient();
  const refetchMods = source.refetch;
  const { toggle, add, remove } = useModMutations(id, { inline: true });
  const [removing, setRemoving] = useState<LocalMod | null>(null);
  const [dragging, setDragging] = useState(false);
  const [picking, setPicking] = useState(false);
  const [opening, setOpening] = useState(false);
  // Failures from the file dialog, drag and drop, opening the folder, toggling and adding.
  const [failure, setFailure] = useState<{ error: unknown } | null>(null);
  const fail = useCallback((error: unknown) => setFailure({ error }), []);
  // What happened to the files the player just gave us, when it is not an error.
  const [notice, setNotice] = useState<Notice | null>(null);
  const addOnce = useSingleFlight();
  const toggleOnce = useSingleFlight();
  const removeOnce = useSingleFlight();
  const addMods = add.mutate;

  const submit = useCallback(
    (paths: string[]) => {
      if (paths.length === 0) return;
      const jars = paths.filter(isJar);
      const skipped: Notice | null = jars.length < paths.length ? { kind: 'skipped', names: paths.filter((p) => !isJar(p)).map(baseName) } : null;
      if (jars.length === 0) {
        setNotice(skipped);
        return;
      }
      const started = addOnce((done) => {
        setFailure(null);
        setNotice(skipped);
        addMods(jars, {
          onError: fail,
          onSettled: () => {
            setNotice((current) => (current?.kind === 'busy' ? null : current));
            done();
          },
        });
      });
      if (!started) setNotice({ kind: 'busy' });
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

  // Players add files through Open folder and come back: show what is there now.
  useEffect(() => {
    const onFocus = () => {
      if (queryClient.isMutating() === 0) void refetchMods();
    };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [queryClient, refetchMods]);

  const pick = async () => {
    setPicking(true);
    setFailure(null);
    setNotice(null);
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

  const list = (pack ? contents.data?.additions : mods.data) ?? [];
  // A name normally keys its row, so a toggle (which renames the file) keeps focus. A .jar and its
  // .jar.disabled twin share a name, so those fall back to the file name.
  const nameCount = new Map<string, number>();
  for (const mod of list) nameCount.set(displayName(mod.fileName), (nameCount.get(displayName(mod.fileName)) ?? 0) + 1);
  return (
    <ScreenFrame
      title={t('mods.title')}
      subtitle={instance ? `${instance.name} · ${t('mods.count', { count: list.length + (contents.data?.managed.length ?? 0) })}` : undefined}
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
        {instance && !pack && !instance.game.loader && (
          <p className="panel flex items-start gap-3 p-4 text-ink-dim">
            <InfoBox className="mt-0.5 h-5 w-5 flex-none text-blue-ink" aria-hidden />
            {t('mods.vanillaNote')}
          </p>
        )}
        {running && (
          <p className="panel flex items-start gap-3 p-4 text-ink-dim">
            <InfoBox className="mt-0.5 h-5 w-5 flex-none text-blue-ink" aria-hidden />
            {t('mods.running')}
          </p>
        )}
        {dragging && <p className="font-semibold text-amber">{t('mods.dropHere')}</p>}
        {failure && <ErrorCard error={failure.error} />}
        {notice && (
          <p role="status" className="panel p-4 text-ink-dim">
            {notice.kind === 'busy' ? t('mods.busy') : t('mods.skipped', { names: notice.names.join(', ') })}
          </p>
        )}
        {source.isPending && <p className="text-ink-dim">{t('common.loading')}</p>}
        {source.isError && (
          <ErrorCard
            error={source.error}
            actions={
              <Button disabled={source.isFetching} onClick={() => void source.refetch()}>
                {t('common.retry')}
              </Button>
            }
          />
        )}
        {pack && contents.isSuccess && (
          <section className="grid gap-3" aria-labelledby="pack-mods">
            <h2 id="pack-mods" className="text-lg font-bold">{t('mods.fromPackTitle')}</h2>
            {contents.data.managed.length === 0 ? (
              <p className="text-ink-dim">{t('mods.packEmpty')}</p>
            ) : (
              <ul className="grid gap-2">
                {contents.data.managed.map((mod) => (
                  <li key={mod.path} className="panel flex items-center gap-4 px-4 py-3">
                    <Package className="h-6 w-6 flex-none text-ink-dim" aria-hidden />
                    <div className="min-w-0 flex-1">
                      <p className="truncate font-semibold">{mod.name}</p>
                      {mod.optional?.description && <p className="text-sm text-ink-dim">{mod.optional.description}</p>}
                    </div>
                    {mod.optional ? (
                      <Toggle
                        checked={mod.optional.enabled}
                        disabled={optional.isPending}
                        label={t('mods.toggle', { name: mod.name })}
                        onChange={(on) => optional.mutate({ path: mod.path, enabled: on }, { onError: fail })}
                      />
                    ) : (
                      <span className="badge">{t('mods.fromPack')}</span>
                    )}
                  </li>
                ))}
              </ul>
            )}
            {contents.data.managed.some((m) => m.optional) && <p className="text-sm text-ink-dim">{t('mods.optionalNote')}</p>}
            <h2 className="mt-4 text-lg font-bold">{t('mods.additionsTitle')}</h2>
          </section>
        )}
        {source.isSuccess && list.length === 0 && <p className="text-ink-dim">{pack ? t('mods.noAdditions') : t('mods.empty')}</p>}
        <ul className="grid gap-2">
          {list.map((mod) => {
            const name = displayName(mod.fileName);
            return (
              <li key={(nameCount.get(name) ?? 0) > 1 ? mod.fileName : name} className="panel flex items-center gap-4 px-4 py-3">
                <Package className="h-6 w-6 flex-none text-ink-dim" aria-hidden />
                <div className="min-w-0 flex-1">
                  <p className={`truncate font-semibold ${mod.enabled ? '' : 'text-ink-dim'}`}>{name}</p>
                  <p className="tabular text-sm text-ink-dim">{formatBytes(mod.size, i18n.language)}</p>
                </div>
                <Toggle
                  checked={mod.enabled}
                  disabled={toggle.isPending || running}
                  label={t('mods.toggle', { name })}
                  onChange={(on) =>
                    toggleOnce((done) => {
                      setFailure(null);
                      toggle.mutate({ file: mod.fileName, enabled: on }, { onError: fail, onSettled: done });
                    })
                  }
                />
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t('mods.remove', { name })}
                  aria-disabled={running || undefined}
                  icon={<Trash />}
                  onClick={() => {
                    if (!running) setRemoving(mod);
                  }}
                />
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
