import { useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { commands } from '../../ipc/commands';
import type { ContentFile, OpenTarget } from '../../ipc/schema';
import { useContentFiles, useContentRemove, useInstances } from '../../state/queries';
import { navigate, type ModsTab } from '../../state/route';
import { useRun } from '../../state/runStore';
import { Button } from '../../ui/Button';
import { Dialog } from '../../ui/Dialog';
import { ErrorCard } from '../../ui/ErrorCard';
import { formatBytes } from '../../ui/format';
import { Folder, InfoBox, Package, Search, Trash } from '../../ui/Icon';
import { ScreenFrame } from '../../ui/ScreenFrame';
import { useSingleFlight } from '../../ui/useSingleFlight';
import { ContentTabs } from './ContentTabs';

export type PackKind = Exclude<ModsTab, 'mod'>;

const displayName = (file: ContentFile) => (file.folder ? file.fileName : file.fileName.replace(/\.zip$/i, ''));
const folderTarget = (kind: PackKind, id: string): OpenTarget =>
  kind === 'resource_pack' ? { kind: 'resource_packs', id } : { kind: 'shader_packs', id };

/** The Resource packs and Shaders tabs of the Mods & packs screen: what is in the instance's folder. */
export function PackList({ id, kind }: { id: string; kind: PackKind }) {
  const { t, i18n } = useTranslation();
  const listing = useInstances();
  const instance = listing.data?.instances.find((i) => i.id === id);
  const files = useContentFiles(id, kind);
  const remove = useContentRemove(id, kind);
  // The game holds its packs open, so removing waits for it to stop.
  const running = useRun(id).state !== 'idle';
  const [removing, setRemoving] = useState<ContentFile | null>(null);
  const [opening, setOpening] = useState(false);
  const [failure, setFailure] = useState<{ error: unknown } | null>(null);
  const removeOnce = useSingleFlight();
  const queryClient = useQueryClient();
  const refetch = files.refetch;

  // Players add files through Open folder and come back: show what is there now.
  useEffect(() => {
    const onFocus = () => {
      if (queryClient.isMutating() === 0) void refetch();
    };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [queryClient, refetch]);

  const openFolder = () => {
    setOpening(true);
    setFailure(null);
    void commands
      .openFolder(folderTarget(kind, id))
      .catch((error: unknown) => setFailure({ error }))
      .finally(() => setOpening(false));
  };

  const closeRemove = () => setRemoving(null);
  const confirmRemove = (file: ContentFile) =>
    removeOnce((done) =>
      remove.mutate(file.fileName, {
        onSettled: () => {
          closeRemove();
          done();
        },
      }),
    );

  // Shaders run through a loader's shader mod; a vanilla instance has nothing to run them, so no
  // Find button there (the list still shows what is in the folder).
  const needsLoader = kind === 'shader' && instance !== undefined && !instance.game.loader;
  const canFind = instance !== undefined && !needsLoader;
  const list = files.data ?? [];
  const managed = list.filter((f) => f.managed);
  const yours = list.filter((f) => !f.managed);
  const loading = listing.isPending || files.isPending;

  const row = (file: ContentFile) => (
    <li key={file.fileName} className="panel flex items-center gap-4 px-4 py-3">
      <Package className="h-6 w-6 flex-none text-ink-dim" aria-hidden />
      <div className="min-w-0 flex-1">
        <p className="truncate font-semibold">{displayName(file)}</p>
        {!file.folder && <p className="tabular text-sm text-ink-dim">{formatBytes(file.size, i18n.language)}</p>}
      </div>
      {file.folder && <span className="badge">{t('packs.folder')}</span>}
      {!file.managed && !file.folder && (
        <Button
          variant="ghost"
          size="icon"
          aria-label={t('packs.remove', { name: displayName(file) })}
          aria-disabled={running || undefined}
          icon={<Trash />}
          onClick={() => {
            if (!running) setRemoving(file);
          }}
        />
      )}
    </li>
  );

  const yoursHeading = yours.length > 0 || managed.length > 0;
  return (
    <ScreenFrame
      title={t('mods.screenTitle')}
      subtitle={instance?.name}
      onBack={() => navigate({ name: 'home', id })}
      actions={
        <>
          {canFind && (
            <Button icon={<Search />} onClick={() => navigate({ name: 'browse', kind, instance: id })}>
              {kind === 'resource_pack' ? t('packs.findResourcePacks') : t('packs.findShaders')}
            </Button>
          )}
          <Button icon={<Folder />} disabled={opening} onClick={openFolder}>
            {t('common.openFolder')}
          </Button>
        </>
      }
    >
      <div className="grid max-w-3xl gap-4">
        <ContentTabs id={id} active={kind} />
        {needsLoader && (
          <p className="panel flex items-start gap-3 p-4 text-ink-dim">
            <InfoBox className="mt-0.5 h-5 w-5 flex-none text-blue-ink" aria-hidden />
            {t('packs.shadersNeedLoader')}
          </p>
        )}
        {running && (
          <p className="panel flex items-start gap-3 p-4 text-ink-dim">
            <InfoBox className="mt-0.5 h-5 w-5 flex-none text-blue-ink" aria-hidden />
            {t('packs.running')}
          </p>
        )}
        <p className="text-ink-dim">{kind === 'resource_pack' ? t('install.turnOnResourcePack') : t('install.turnOnShader')}</p>
        {failure && <ErrorCard error={failure.error} />}
        {loading && <p className="text-ink-dim">{t('common.loading')}</p>}
        {files.isError && (
          <ErrorCard
            error={files.error}
            actions={
              <Button disabled={files.isFetching} onClick={() => void files.refetch()}>
                {t('common.retry')}
              </Button>
            }
          />
        )}
        {!loading && files.isSuccess && (
          <>
            {managed.length > 0 && (
              <section className="grid gap-3" aria-labelledby="pack-files">
                <h2 id="pack-files" className="text-lg font-bold">
                  {t('mods.fromPackTitle')}
                </h2>
                <ul className="grid gap-2">{managed.map(row)}</ul>
              </section>
            )}
            <section className="grid gap-3" aria-labelledby={yoursHeading ? 'your-packs' : undefined}>
              {yoursHeading && (
                <h2 id="your-packs" className={`text-lg font-bold ${managed.length > 0 ? 'mt-4' : ''}`}>
                  {t('packs.yours')}
                </h2>
              )}
              {yours.length === 0 && <p className="text-ink-dim">{t(`packs.empty.${kind}`)}</p>}
              {yours.length > 0 && <ul className="grid gap-2">{yours.map(row)}</ul>}
            </section>
          </>
        )}
      </div>
      {removing && (
        <Dialog
          open
          onClose={closeRemove}
          title={t('packs.removeTitle', { name: displayName(removing) })}
          actions={
            <>
              <Button onClick={closeRemove}>{t('common.cancel')}</Button>
              <Button variant="danger" disabled={remove.isPending} onClick={() => confirmRemove(removing)}>
                {t('mods.removeConfirm')}
              </Button>
            </>
          }
        >
          {t('packs.removeBody')}
        </Dialog>
      )}
    </ScreenFrame>
  );
}
